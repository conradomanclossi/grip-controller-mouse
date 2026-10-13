mod config;
mod input;
mod mapping;
mod motion;
mod output;
mod permissions;
mod settings;
mod tray;

use std::sync::Mutex;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use serde_json::json;
use tao::event::{Event, StartCause, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopWindowTarget};
use tao::platform::macos::{ActivationPolicy, EventLoopExtMacOS, EventLoopWindowTargetExtMacOS};
use tray_icon::menu::MenuEvent;

use input::{Shared, Status};
use settings::{Request, Settings};

enum UserEvent {
    Menu(MenuEvent),
    Input(Status),
    Ipc(String),
}

const TRUST_POLL: Duration = Duration::from_secs(2);

/// Main-thread state.
struct App {
    shared: std::sync::Arc<Shared>,
    proxy: tao::event_loop::EventLoopProxy<UserEvent>,
    tray: Option<tray::Tray>,
    settings: Option<Settings>,
    trusted: bool,
    controller: Option<String>,
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let (config, first_run) = config::load().unwrap_or_else(|e| {
        log::error!("config error, using defaults: {e:#}");
        (config::Config::default(), false)
    });
    let shared = Shared::new(config);

    let mut event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    event_loop.set_activation_policy(ActivationPolicy::Accessory);

    let proxy = Mutex::new(event_loop.create_proxy());
    MenuEvent::set_event_handler(Some(move |e| {
        let _ = proxy.lock().unwrap().send_event(UserEvent::Menu(e));
    }));

    let proxy = event_loop.create_proxy();
    let input_shared = shared.clone();
    std::thread::Builder::new()
        .name("input".into())
        .spawn(move || {
            if let Err(e) = input::run(input_shared, move |s| {
                let _ = proxy.send_event(UserEvent::Input(s));
            }) {
                log::error!("input thread stopped: {e:#}");
            }
        })
        .expect("spawn input thread");

    let mut app = App {
        shared,
        proxy: event_loop.create_proxy(),
        tray: None,
        settings: None,
        trusted: false,
        controller: None,
    };

    event_loop.run(move |event, target, control_flow| {
        *control_flow = ControlFlow::WaitUntil(Instant::now() + TRUST_POLL);

        match event {
            // The tray icon must be created once the app's run loop is up.
            Event::NewEvents(StartCause::Init) => {
                let t = tray::Tray::new().expect("create tray icon");
                app.trusted = permissions::is_trusted(true);
                t.set_trusted(app.trusted);
                app.tray = Some(t);
                if first_run {
                    app.open_settings(target);
                }
            }
            Event::NewEvents(StartCause::ResumeTimeReached { .. }) => {
                let now = permissions::is_trusted(false);
                if now != app.trusted {
                    app.trusted = now;
                    if let Some(t) = &app.tray {
                        t.set_trusted(now);
                    }
                    app.send(json!({ "type": "trusted", "value": now }));
                }
            }
            // Launching the app again (Finder, Spotlight) opens the settings.
            Event::Reopen { .. } => app.open_settings(target),
            Event::WindowEvent { window_id, event: WindowEvent::CloseRequested, .. }
                if app.settings.as_ref().is_some_and(|s| s.id() == window_id) =>
            {
                app.close_settings(target);
            }
            Event::UserEvent(UserEvent::Input(status)) => app.on_status(status),
            Event::UserEvent(UserEvent::Ipc(body)) => {
                if let Some(req) = settings::parse(&body) {
                    app.on_request(req, target);
                }
            }
            Event::UserEvent(UserEvent::Menu(e)) => {
                if app.on_menu(e, target) {
                    *control_flow = ControlFlow::Exit;
                }
            }
            _ => {}
        }
    });
}

impl App {
    fn send(&self, msg: serde_json::Value) {
        if let Some(s) = &self.settings {
            s.send(msg);
        }
    }

    fn open_settings(&mut self, target: &EventLoopWindowTarget<UserEvent>) {
        if let Some(s) = &self.settings {
            s.focus();
            return;
        }
        // Show in the Dock / app switcher while the window is open.
        target.set_activation_policy_at_runtime(ActivationPolicy::Regular);
        let proxy = self.proxy.clone();
        match Settings::open(target, move |body| {
            let _ = proxy.send_event(UserEvent::Ipc(body));
        }) {
            Ok(s) => {
                self.settings = Some(s);
                self.shared.ui_open.store(true, Ordering::Relaxed);
            }
            Err(e) => {
                log::error!("could not open settings: {e:#}");
                target.set_activation_policy_at_runtime(ActivationPolicy::Accessory);
            }
        }
    }

    fn close_settings(&mut self, target: &EventLoopWindowTarget<UserEvent>) {
        self.settings = None;
        self.shared.ui_open.store(false, Ordering::Relaxed);
        target.set_activation_policy_at_runtime(ActivationPolicy::Accessory);
    }

    fn set_enabled(&self, on: bool) {
        self.shared.enabled.store(on, Ordering::Relaxed);
        if let Some(t) = &self.tray {
            t.set_enabled(on);
        }
        self.send(json!({ "type": "enabled", "value": on }));
    }

    fn on_status(&mut self, status: Status) {
        match status {
            Status::Connected(name) => {
                if let Some(t) = &self.tray {
                    t.status.set_text(format!("🎮 {name}"));
                }
                self.send(json!({ "type": "controller", "name": name }));
                self.controller = Some(name);
            }
            Status::Disconnected => {
                if let Some(t) = &self.tray {
                    t.status.set_text("No controller");
                }
                self.send(json!({ "type": "controller", "name": null }));
                self.controller = None;
            }
            Status::Enabled(on) => {
                if let Some(t) = &self.tray {
                    t.set_enabled(on);
                }
                self.send(json!({ "type": "enabled", "value": on }));
            }
            Status::Live(live) => {
                self.send(json!({ "type": "live", "sticks": live.sticks, "pressed": live.pressed }))
            }
        }
    }

    fn on_request(&mut self, req: Request, target: &EventLoopWindowTarget<UserEvent>) {
        match req {
            Request::Ready => {
                let raw = config::load_raw().unwrap_or_else(|e| {
                    log::error!("{e:#}");
                    self.send(json!({ "type": "error", "message": format!("{e:#}") }));
                    config::RawConfig::with_default_buttons()
                });
                let enabled = self.shared.enabled.load(Ordering::Relaxed);
                self.send(settings::init_message(&raw, enabled, self.controller.as_deref(), self.trusted));
            }
            Request::Save { config } => match config::save(&config) {
                Ok(c) => {
                    self.shared.set_config(c);
                    log::info!("config saved from settings window");
                    self.send(json!({ "type": "saved" }));
                }
                Err(e) => self.send(json!({ "type": "error", "message": format!("{e:#}") })),
            },
            Request::SetEnabled { value } => self.set_enabled(value),
            Request::OpenAccessibility => permissions::open_settings(),
            Request::OpenConfigFile => open_config_file(),
            Request::Close => self.close_settings(target),
        }
    }

    /// Returns true when the app should quit.
    fn on_menu(&mut self, e: MenuEvent, target: &EventLoopWindowTarget<UserEvent>) -> bool {
        let Some(t) = &self.tray else { return false };
        let id = e.id();
        if id == t.quit.id() {
            return true;
        } else if id == t.settings.id() {
            self.open_settings(target);
        } else if id == t.enabled.id() {
            let on = t.enabled.is_checked();
            self.set_enabled(on);
        } else if id == t.access.id() {
            permissions::open_settings();
        } else if id == t.open_config.id() {
            open_config_file();
        } else if id == t.reload_config.id() {
            match config::load() {
                Ok((c, _)) => {
                    self.shared.set_config(c);
                    log::info!("config reloaded");
                    if self.settings.is_some() {
                        self.on_request(Request::Ready, target);
                    }
                }
                Err(e) => {
                    log::error!("{e:#}");
                    alert("Config error", &format!("{e:#}\n\nThe previous config is still active."));
                }
            }
        } else if let Some(i) = t.speeds.iter().position(|s| s.id() == id) {
            for (j, s) in t.speeds.iter().enumerate() {
                s.set_checked(i == j);
            }
            self.shared.set_speed(tray::SPEEDS[i].1);
        }
        false
    }
}

fn open_config_file() {
    let _ = std::process::Command::new("open").arg("-t").arg(config::path()).spawn();
}

/// Shows a simple modal alert via AppleScript (keeps us free of extra UI deps).
fn alert(title: &str, message: &str) {
    let esc = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
    let script = format!(
        "display alert \"{}\" message \"{}\" as critical",
        esc(title),
        esc(message)
    );
    let _ = std::process::Command::new("osascript").arg("-e").arg(script).spawn();
}
