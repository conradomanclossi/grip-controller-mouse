//! Synthetic mouse / scroll / keyboard events via CoreGraphics.

use std::time::{Duration, Instant};

use core_graphics::display::CGDisplay;
use core_graphics::event::{
    CGEvent, CGEventFlags, CGEventTapLocation, CGEventType, CGMouseButton, EventField,
    ScrollEventUnit,
};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use core_graphics::geometry::{CGPoint, CGRect};

use crate::mapping::{KeyCombo, MouseButton};

const DOUBLE_CLICK_TIME: Duration = Duration::from_millis(500);
const DOUBLE_CLICK_DISTANCE: f64 = 4.0;

pub struct Output {
    source: CGEventSource,
    held: Option<MouseButton>,
    last_click: Option<(MouseButton, Instant, CGPoint)>,
    click_count: i64,
}

impl Output {
    pub fn new() -> anyhow::Result<Output> {
        let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
            .map_err(|_| anyhow::anyhow!("CGEventSourceCreate failed"))?;
        Ok(Output { source, held: None, last_click: None, click_count: 0 })
    }

    pub fn cursor(&self) -> CGPoint {
        CGEvent::new(self.source.clone())
            .map(|e| e.location())
            .unwrap_or(CGPoint::new(0.0, 0.0))
    }

    /// Clamps `to` onto a display: keeps it if it is on any display, otherwise
    /// clamps it to the display that `from` is on (so the cursor stops at screen edges
    /// but can still cross between adjacent displays).
    pub fn clamp(&self, from: CGPoint, to: CGPoint) -> CGPoint {
        let bounds: Vec<CGRect> = CGDisplay::active_displays()
            .unwrap_or_default()
            .into_iter()
            .map(|id| CGDisplay::new(id).bounds())
            .collect();
        if bounds.iter().any(|r| contains(r, to)) {
            return to;
        }
        let Some(r) = bounds
            .iter()
            .find(|r| contains(r, from))
            .or(bounds.first())
        else {
            return to;
        };
        CGPoint::new(
            to.x.clamp(r.origin.x, r.origin.x + r.size.width - 1.0),
            to.y.clamp(r.origin.y, r.origin.y + r.size.height - 1.0),
        )
    }

    pub fn move_to(&self, p: CGPoint) {
        let (ty, button) = match self.held {
            None => (CGEventType::MouseMoved, CGMouseButton::Left),
            Some(MouseButton::Left) => (CGEventType::LeftMouseDragged, CGMouseButton::Left),
            Some(MouseButton::Right) => (CGEventType::RightMouseDragged, CGMouseButton::Right),
            Some(MouseButton::Middle) => (CGEventType::OtherMouseDragged, CGMouseButton::Center),
        };
        self.post_mouse(ty, p, button, None);
    }

    pub fn mouse_button(&mut self, button: MouseButton, down: bool, p: CGPoint) {
        let (cg_button, down_ty, up_ty) = match button {
            MouseButton::Left => (CGMouseButton::Left, CGEventType::LeftMouseDown, CGEventType::LeftMouseUp),
            MouseButton::Right => (CGMouseButton::Right, CGEventType::RightMouseDown, CGEventType::RightMouseUp),
            MouseButton::Middle => (CGMouseButton::Center, CGEventType::OtherMouseDown, CGEventType::OtherMouseUp),
        };
        if down {
            let now = Instant::now();
            self.click_count = match self.last_click {
                Some((b, t, q))
                    if b == button
                        && now - t < DOUBLE_CLICK_TIME
                        && (p.x - q.x).hypot(p.y - q.y) < DOUBLE_CLICK_DISTANCE =>
                {
                    (self.click_count % 3) + 1
                }
                _ => 1,
            };
            self.last_click = Some((button, now, p));
            self.held = Some(button);
            self.post_mouse(down_ty, p, cg_button, Some(self.click_count));
        } else {
            if self.held == Some(button) {
                self.held = None;
            }
            self.post_mouse(up_ty, p, cg_button, Some(self.click_count.max(1)));
        }
    }

    pub fn release_mouse(&mut self) {
        if let Some(b) = self.held {
            self.mouse_button(b, false, self.cursor());
        }
    }

    /// Pixel-unit scroll. Positive `dy` scrolls content up (toward the top), positive `dx` left.
    pub fn scroll(&self, dx: i32, dy: i32) {
        if dx == 0 && dy == 0 {
            return;
        }
        if let Ok(e) = CGEvent::new_scroll_event(self.source.clone(), ScrollEventUnit::PIXEL, 2, dy, dx, 0) {
            e.post(CGEventTapLocation::HID);
        }
    }

    /// Presses or releases a combo; modifiers get their own key events so things like the
    /// cmd+tab switcher behave like a real keyboard.
    pub fn key(&self, combo: &KeyCombo, down: bool, repeat: bool) {
        let mod_flags = |n: usize| {
            combo.modifiers[..n]
                .iter()
                .fold(CGEventFlags::CGEventFlagNull, |acc, m| acc | m.flag())
        };
        if down {
            if !repeat {
                for (i, m) in combo.modifiers.iter().enumerate() {
                    self.post_key(m.keycode(), true, mod_flags(i + 1), false);
                }
            }
            self.post_key(combo.keycode, true, combo.flags(), repeat);
        } else {
            self.post_key(combo.keycode, false, combo.flags(), false);
            for (i, m) in combo.modifiers.iter().enumerate().rev() {
                self.post_key(m.keycode(), false, mod_flags(i), false);
            }
        }
    }

    fn post_key(&self, code: u16, down: bool, flags: CGEventFlags, repeat: bool) {
        if let Ok(e) = CGEvent::new_keyboard_event(self.source.clone(), code, down) {
            e.set_flags(flags);
            if repeat {
                e.set_integer_value_field(EventField::KEYBOARD_EVENT_AUTOREPEAT, 1);
            }
            e.post(CGEventTapLocation::HID);
        }
    }

    fn post_mouse(&self, ty: CGEventType, p: CGPoint, button: CGMouseButton, clicks: Option<i64>) {
        if let Ok(e) = CGEvent::new_mouse_event(self.source.clone(), ty, p, button) {
            if let Some(n) = clicks {
                e.set_integer_value_field(EventField::MOUSE_EVENT_CLICK_STATE, n);
            }
            e.post(CGEventTapLocation::HID);
        }
    }
}

fn contains(r: &CGRect, p: CGPoint) -> bool {
    p.x >= r.origin.x
        && p.y >= r.origin.y
        && p.x < r.origin.x + r.size.width
        && p.y < r.origin.y + r.size.height
}
