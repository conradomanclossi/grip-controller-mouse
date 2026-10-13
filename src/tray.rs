//! Menu bar icon and menu.

use tray_icon::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

pub const SPEEDS: [(&str, f32); 3] = [("Slow", 0.6), ("Normal", 1.0), ("Fast", 1.6)];

pub struct Tray {
    handle: TrayIcon,
    pub status: MenuItem,
    pub access: MenuItem,
    pub enabled: CheckMenuItem,
    pub speeds: Vec<CheckMenuItem>,
    pub open_config: MenuItem,
    pub reload_config: MenuItem,
    pub quit: MenuItem,
}

impl Tray {
    pub fn new() -> anyhow::Result<Tray> {
        let status = MenuItem::new("No controller", false, None);
        let access = MenuItem::new("Accessibility: OK", false, None);
        let enabled = CheckMenuItem::new("Enabled", true, true, None);
        let speeds: Vec<CheckMenuItem> = SPEEDS
            .iter()
            .map(|(name, s)| CheckMenuItem::new(*name, true, *s == 1.0, None))
            .collect();
        let speed_menu = Submenu::new("Cursor Speed", true);
        for item in &speeds {
            speed_menu.append(item)?;
        }
        let open_config = MenuItem::new("Open Config…", true, None);
        let reload_config = MenuItem::new("Reload Config", true, None);
        let quit = MenuItem::new("Quit Controller Mouse", true, None);

        let menu = Menu::new();
        menu.append_items(&[
            &status,
            &access,
            &PredefinedMenuItem::separator(),
            &enabled,
            &speed_menu,
            &PredefinedMenuItem::separator(),
            &open_config,
            &reload_config,
            &PredefinedMenuItem::separator(),
            &quit,
        ])?;

        let icon = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("Controller Mouse")
            .with_icon_templated(icon(true)?)
            .build()?;

        Ok(Tray { handle: icon, status, access, enabled, speeds, open_config, reload_config, quit })
    }

    pub fn set_enabled(&self, on: bool) {
        self.enabled.set_checked(on);
        let _ = self.handle.set_icon_templated(icon(on).ok());
    }

    pub fn set_trusted(&self, trusted: bool) {
        if trusted {
            self.access.set_text("Accessibility: OK");
            self.access.set_enabled(false);
        } else {
            self.access.set_text("⚠ Grant Accessibility Permission…");
            self.access.set_enabled(true);
        }
    }
}

/// A small gamepad glyph drawn procedurally (template image: black + alpha).
/// When disabled, a diagonal slash is drawn through it.
fn icon(enabled: bool) -> anyhow::Result<Icon> {
    const S: usize = 36;
    let mut px = vec![0u8; S * S * 4];
    let inside = |x: f32, y: f32| -> bool {
        // Body: a stadium (rounded bar) plus two grips.
        let body = {
            let (cx, cy, hw, r) = (18.0, 15.0, 9.0, 7.0);
            let dx = ((x - cx).abs() - hw).max(0.0);
            dx * dx + (y - cy) * (y - cy) <= r * r
        };
        let grip = |gx: f32| {
            let (dx, dy) = (x - gx, y - 22.0);
            dx * dx + dy * dy <= 5.0 * 5.0
        };
        let solid = body || grip(9.5) || grip(26.5);
        // Cut-outs: d-pad cross (left) and two buttons (right).
        let dpad = ((x - 10.5).abs() <= 3.5 && (y - 15.0).abs() <= 1.0)
            || ((x - 10.5).abs() <= 1.0 && (y - 15.0).abs() <= 3.5);
        let btn = |bx: f32, by: f32| (x - bx).powi(2) + (y - by).powi(2) <= 1.8 * 1.8;
        let holes = dpad || btn(25.0, 12.5) || btn(28.0, 16.5);
        let slash = !enabled && ((x - y) - 0.0).abs() <= 1.6 && (3.0..33.0).contains(&x);
        let slash_gap = !enabled && ((x - y) - 0.0).abs() <= 3.4 && !slash;
        (solid && !holes && !slash_gap) || slash
    };
    for y in 0..S {
        for x in 0..S {
            // 4x supersampling for smooth edges.
            let mut hits = 0;
            for sy in 0..2 {
                for sx in 0..2 {
                    if inside(x as f32 + 0.25 + sx as f32 * 0.5, y as f32 + 0.25 + sy as f32 * 0.5) {
                        hits += 1;
                    }
                }
            }
            px[(y * S + x) * 4 + 3] = (hits * 255 / 4) as u8;
        }
    }
    Ok(Icon::from_rgba(px, S as u32, S as u32)?)
}
