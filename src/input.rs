//! Input thread: reads the controller via gilrs and drives `Output`.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use core_graphics::geometry::CGPoint;
use gilrs::{Axis, Button, EventType, GamepadId, Gilrs};

use crate::config::Config;
use crate::mapping::{Action, KeyCombo};
use crate::motion::{Accumulator, shape};
use crate::output::Output;

const TICK: Duration = Duration::from_millis(8);
const IDLE_WAIT: Duration = Duration::from_millis(500);

/// Messages from the input thread to the UI.
#[derive(Debug, Clone)]
pub enum Status {
    Connected(String),
    Disconnected,
    Enabled(bool),
}

/// State shared between the tray (main thread) and the input thread.
pub struct Shared {
    pub enabled: AtomicBool,
    config: RwLock<Arc<Config>>,
    speed_bits: AtomicU32,
}

impl Shared {
    pub fn new(config: Config) -> Arc<Shared> {
        Arc::new(Shared {
            enabled: AtomicBool::new(true),
            config: RwLock::new(Arc::new(config)),
            speed_bits: AtomicU32::new(1.0f32.to_bits()),
        })
    }

    pub fn config(&self) -> Arc<Config> {
        self.config.read().unwrap().clone()
    }

    pub fn set_config(&self, config: Config) {
        *self.config.write().unwrap() = Arc::new(config);
    }

    pub fn speed(&self) -> f32 {
        f32::from_bits(self.speed_bits.load(Ordering::Relaxed))
    }

    pub fn set_speed(&self, s: f32) {
        self.speed_bits.store(s.to_bits(), Ordering::Relaxed);
    }
}

struct HeldKey {
    combo: KeyCombo,
    next_repeat: Instant,
}

struct Engine {
    shared: Arc<Shared>,
    notify: Box<dyn Fn(Status) + Send>,
    out: Output,
    active: Option<GamepadId>,
    /// Buttons currently held, with the action they were pressed with
    /// (so a config reload mid-press still releases the right thing).
    pressed: HashMap<Button, Action>,
    keys: HashMap<Button, HeldKey>,
    cursor: Option<CGPoint>,
    scroll_acc: Accumulator,
    last_tick: Instant,
    was_enabled: bool,
}

pub fn run(shared: Arc<Shared>, notify: impl Fn(Status) + Send + 'static) -> anyhow::Result<()> {
    let mut gilrs = Gilrs::new().map_err(|e| anyhow::anyhow!("gilrs init failed: {e}"))?;
    let mut engine = Engine {
        was_enabled: shared.enabled.load(Ordering::Relaxed),
        shared,
        notify: Box::new(notify),
        out: Output::new()?,
        active: None,
        pressed: HashMap::new(),
        keys: HashMap::new(),
        cursor: None,
        scroll_acc: Accumulator::default(),
        last_tick: Instant::now(),
    };

    if let Some((id, gp)) = gilrs.gamepads().next() {
        engine.active = Some(id);
        (engine.notify)(Status::Connected(gp.name().to_string()));
    }

    loop {
        // Block while idle; tick at a fixed rate while sticks or key repeats need it.
        let busy = engine.is_busy(&gilrs);
        let first = if busy {
            std::thread::sleep(TICK.saturating_sub(engine.last_tick.elapsed()));
            gilrs.next_event()
        } else {
            gilrs.next_event_blocking(Some(IDLE_WAIT))
        };
        if let Some(ev) = first {
            engine.handle(&gilrs, ev.id, ev.event);
            while let Some(ev) = gilrs.next_event() {
                engine.handle(&gilrs, ev.id, ev.event);
            }
        }
        engine.tick(&gilrs);
    }
}

impl Engine {
    fn enabled(&self) -> bool {
        self.shared.enabled.load(Ordering::Relaxed)
    }

    fn sticks(&self, gilrs: &Gilrs) -> [f32; 4] {
        let Some(gp) = self.active.and_then(|id| gilrs.connected_gamepad(id)) else {
            return [0.0; 4];
        };
        [Axis::LeftStickX, Axis::LeftStickY, Axis::RightStickX, Axis::RightStickY].map(|a| gp.value(a))
    }

    fn is_busy(&self, gilrs: &Gilrs) -> bool {
        if !self.enabled() {
            return false;
        }
        let cfg = self.shared.config();
        let [lx, ly, rx, ry] = self.sticks(gilrs);
        !self.keys.is_empty()
            || shape(lx, ly, cfg.cursor.stick.deadzone, 1.0) != (0.0, 0.0)
            || shape(rx, ry, cfg.scroll.stick.deadzone, 1.0) != (0.0, 0.0)
    }

    fn handle(&mut self, gilrs: &Gilrs, id: GamepadId, ev: EventType) {
        match ev {
            EventType::Connected => {
                self.active = Some(id);
                (self.notify)(Status::Connected(gilrs.gamepad(id).name().to_string()));
            }
            EventType::Disconnected => {
                if self.active == Some(id) {
                    self.release_all();
                    self.active = gilrs.gamepads().next().map(|(id, _)| id);
                    match self.active {
                        Some(next) => (self.notify)(Status::Connected(gilrs.gamepad(next).name().to_string())),
                        None => (self.notify)(Status::Disconnected),
                    }
                }
            }
            EventType::ButtonPressed(b, _) => {
                self.active = Some(id);
                self.press(b, gilrs);
            }
            EventType::ButtonReleased(b, _) => self.release(b),
            EventType::AxisChanged(..) => self.active = Some(id),
            _ => {}
        }
    }

    fn set_enabled(&mut self, on: bool) {
        self.shared.enabled.store(on, Ordering::Relaxed);
        (self.notify)(Status::Enabled(on));
        log::info!("enabled = {on}");
    }

    fn press(&mut self, b: Button, gilrs: &Gilrs) {
        let cfg = self.shared.config();
        let action = cfg.bindings.get(&b).cloned().unwrap_or(Action::None);

        // Clicking both sticks toggles, even when the binding says otherwise.
        let both_thumbs = matches!(b, Button::LeftThumb | Button::RightThumb)
            && self.active.and_then(|id| gilrs.connected_gamepad(id)).is_some_and(|gp| {
                gp.is_pressed(Button::LeftThumb) && gp.is_pressed(Button::RightThumb)
            });
        if action == Action::Toggle || both_thumbs {
            let on = !self.enabled();
            self.set_enabled(on);
            return;
        }
        if !self.enabled() {
            return;
        }
        match &action {
            Action::Mouse(m) => {
                let at = self.cursor.unwrap_or_else(|| self.out.cursor());
                self.out.mouse_button(*m, true, at);
            },
            Action::Key(k) => {
                self.out.key(k, true, false);
                // Only bare keys repeat; repeating shortcuts would strobe Spotlight, Mission Control, etc.
                if k.modifiers.is_empty() {
                    let next_repeat = Instant::now() + Duration::from_millis(cfg.repeat.delay_ms);
                    self.keys.insert(b, HeldKey { combo: k.clone(), next_repeat });
                }
            }
            Action::None | Action::Toggle | Action::Precision | Action::Turbo => {}
        }
        self.pressed.insert(b, action);
    }

    fn release(&mut self, b: Button) {
        match self.pressed.remove(&b) {
            Some(Action::Mouse(m)) => {
                let at = self.cursor.unwrap_or_else(|| self.out.cursor());
                self.out.mouse_button(m, false, at);
            },
            Some(Action::Key(k)) => {
                self.keys.remove(&b);
                self.out.key(&k, false, false);
            }
            _ => {}
        }
    }

    fn release_all(&mut self) {
        let buttons: Vec<Button> = self.pressed.keys().copied().collect();
        for b in buttons {
            self.release(b);
        }
        self.out.release_mouse();
        self.cursor = None;
        self.scroll_acc.reset();
    }

    fn holding(&self, action: &Action) -> bool {
        self.pressed.values().any(|a| a == action)
    }

    fn tick(&mut self, gilrs: &Gilrs) {
        let now = Instant::now();
        let dt = (now - self.last_tick).as_secs_f64().min(0.05);
        self.last_tick = now;

        let enabled = self.enabled();
        if self.was_enabled && !enabled {
            self.release_all();
        }
        self.was_enabled = enabled;
        if !enabled {
            return;
        }

        let cfg = self.shared.config();
        let [lx, ly, rx, ry] = self.sticks(gilrs);

        // Cursor. Track our own position while moving; resync when motion starts so
        // the physical mouse is respected.
        let (sx, sy) = shape(lx, ly, cfg.cursor.stick.deadzone, cfg.cursor.stick.exponent);
        if (sx, sy) != (0.0, 0.0) {
            let mut mult = self.shared.speed() as f64;
            if self.holding(&Action::Precision) {
                mult *= cfg.cursor.precision_multiplier;
            }
            if self.holding(&Action::Turbo) {
                mult *= cfg.cursor.turbo_multiplier;
            }
            let from = self.cursor.unwrap_or_else(|| self.out.cursor());
            let step = cfg.cursor.stick.max_speed * mult * dt;
            // gilrs Y is up-positive; screen Y is down-positive.
            let to = CGPoint::new(from.x + sx as f64 * step, from.y - sy as f64 * step);
            let to = self.out.clamp(from, to);
            self.out.move_to(to);
            self.cursor = Some(to);
        } else {
            self.cursor = None;
        }

        // Scroll.
        let (vx, vy) = shape(rx, ry, cfg.scroll.stick.deadzone, cfg.scroll.stick.exponent);
        if (vx, vy) != (0.0, 0.0) {
            let step = cfg.scroll.stick.max_speed * dt;
            let fx = if cfg.scroll.invert_x { 1.0 } else { -1.0 };
            let fy = if cfg.scroll.invert_y { -1.0 } else { 1.0 };
            let (dx, dy) = self.scroll_acc.add(vx as f64 * step * fx, vy as f64 * step * fy);
            self.out.scroll(dx, dy);
        } else {
            self.scroll_acc.reset();
        }

        // Key repeat.
        let interval = Duration::from_millis(cfg.repeat.interval_ms.max(10));
        for held in self.keys.values_mut() {
            if now >= held.next_repeat {
                self.out.key(&held.combo, true, true);
                held.next_repeat = now + interval;
            }
        }
    }
}
