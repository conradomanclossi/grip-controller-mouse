mod config;
mod input;
mod mapping;
mod motion;
mod output;
mod permissions;
mod tray;

use std::sync::Mutex;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use tao::event::{Event, StartCause};
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tao::platform::macos::{ActivationPolicy, EventLoopExtMacOS};
use tray_icon::menu::MenuEvent;

use input::{Shared, Status};

enum UserEvent {
    Menu(MenuEvent),
    Input(Status),
}

const TRUST_POLL: Duration = Duration::from_secs(2);

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let config = config::load().unwrap_or_else(|e| {
        log::error!("config error, using defaults: {e:#}");
        config::Config::default()
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

    let mut tray: Option<tray::Tray> = None;
    let mut trusted = false;

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::WaitUntil(Instant::now() + TRUST_POLL);

        match event {
            // The tray icon must be created once the app's run loop is up.
            Event::NewEvents(StartCause::Init) => {
                let t = tray::Tray::new().expect("create tray icon");
                trusted = permissions::is_trusted(true);
                t.set_trusted(trusted);
                tray = Some(t);
            }
            Event::NewEvents(StartCause::ResumeTimeReached { .. }) => {
                if let Some(t) = &tray {
                    let now = permissions::is_trusted(false);
                    if now != trusted {
                        trusted = now;
                        t.set_trusted(trusted);
                    }
                }
            }
            Event::UserEvent(UserEvent::Input(status)) => {
                let Some(t) = &tray else { return };
                match status {
                    Status::Connected(name) => t.status.set_text(format!("🎮 {name}")),
                    Status::Disconnected => t.status.set_text("No controller"),
                    Status::Enabled(on) => t.set_enabled(on),
                }
            }
            Event::UserEvent(UserEvent::Menu(e)) => {
                let Some(t) = &tray else { return };
                let id = e.id();
                if id == t.quit.id() {
                    *control_flow = ControlFlow::Exit;
                } else if id == t.enabled.id() {
                    let on = t.enabled.is_checked();
                    shared.enabled.store(on, Ordering::Relaxed);
                    t.set_enabled(on);
                } else if id == t.access.id() {
                    permissions::open_settings();
                } else if id == t.open_config.id() {
                    let _ = std::process::Command::new("open").arg("-t").arg(config::path()).spawn();
                } else if id == t.reload_config.id() {
                    match config::load() {
                        Ok(c) => {
                            shared.set_config(c);
                            log::info!("config reloaded");
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
                    shared.set_speed(tray::SPEEDS[i].1);
                }
            }
            _ => {}
        }
    });
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
