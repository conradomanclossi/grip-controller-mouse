//! Settings window: a webview (ui/settings.html) talking to Rust over JSON IPC.

use serde::Deserialize;
use serde_json::{Value, json};
use tao::dpi::LogicalSize;
use tao::event_loop::EventLoopWindowTarget;
use tao::window::{Window, WindowBuilder, WindowId};
use wry::{WebView, WebViewBuilder};

use crate::config::RawConfig;

const HTML: &str = include_str!("../ui/settings.html");

/// Messages from the page.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Request {
    Ready,
    Save { config: RawConfig },
    SetEnabled { value: bool },
    OpenAccessibility,
    OpenConfigFile,
    Close,
}

pub struct Settings {
    window: Window,
    webview: WebView,
}

impl Settings {
    pub fn open<T: 'static>(
        target: &EventLoopWindowTarget<T>,
        on_message: impl Fn(String) + 'static,
    ) -> anyhow::Result<Settings> {
        let window = WindowBuilder::new()
            .with_title("GRIP.")
            .with_inner_size(LogicalSize::new(780.0, 860.0))
            .with_min_inner_size(LogicalSize::new(620.0, 560.0))
            .build(target)?;
        let webview = WebViewBuilder::new()
            .with_html(HTML)
            .with_devtools(cfg!(debug_assertions))
            .with_ipc_handler(move |req| on_message(req.into_body()))
            .build(&window)?;
        window.set_focus();
        Ok(Settings { window, webview })
    }

    pub fn id(&self) -> WindowId {
        self.window.id()
    }

    pub fn focus(&self) {
        self.window.set_focus();
    }

    /// Sends a message to the page (`window.grip.receive(msg)`).
    pub fn send(&self, msg: Value) {
        let script = format!("window.grip && window.grip.receive({msg})");
        if let Err(e) = self.webview.evaluate_script(&script) {
            log::warn!("settings: evaluate_script failed: {e}");
        }
    }
}

pub fn parse(body: &str) -> Option<Request> {
    match serde_json::from_str(body) {
        Ok(r) => Some(r),
        Err(e) => {
            log::warn!("settings: bad message {body:?}: {e}");
            None
        }
    }
}

pub fn init_message(config: &RawConfig, enabled: bool, controller: Option<&str>, trusted: bool) -> Value {
    json!({
        "type": "init",
        "config": config,
        "defaults": RawConfig::with_default_buttons(),
        "enabled": enabled,
        "controller": controller,
        "trusted": trusted,
        "version": env!("CARGO_PKG_VERSION"),
    })
}
