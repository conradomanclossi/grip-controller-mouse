//! TOML config at `~/Library/Application Support/grip/config.toml`.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use gilrs::Button;
use serde::{Deserialize, Serialize};

use crate::mapping::{Action, button_by_name};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Stick {
    pub deadzone: f32,
    pub exponent: f32,
    /// Pixels per second at full deflection.
    pub max_speed: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Cursor {
    #[serde(flatten)]
    pub stick: Stick,
    pub precision_multiplier: f64,
    pub turbo_multiplier: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Scroll {
    #[serde(flatten)]
    pub stick: Stick,
    pub invert_x: bool,
    pub invert_y: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Repeat {
    pub delay_ms: u64,
    pub interval_ms: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct RawConfig {
    pub cursor: Cursor,
    pub scroll: Scroll,
    pub repeat: Repeat,
    /// Button name → action string. Merged over the defaults.
    pub buttons: BTreeMap<String, String>,
}

impl Default for Stick {
    fn default() -> Self {
        Stick { deadzone: 0.12, exponent: 2.0, max_speed: 1400.0 }
    }
}

impl Default for Cursor {
    fn default() -> Self {
        Cursor { stick: Stick::default(), precision_multiplier: 0.3, turbo_multiplier: 2.0 }
    }
}

impl Default for Scroll {
    fn default() -> Self {
        Scroll {
            stick: Stick { deadzone: 0.15, exponent: 2.0, max_speed: 1500.0 },
            invert_x: false,
            invert_y: false,
        }
    }
}

impl Default for Repeat {
    fn default() -> Self {
        Repeat { delay_ms: 400, interval_ms: 60 }
    }
}

pub fn default_buttons() -> BTreeMap<String, String> {
    [
        ("south", "left_click"),
        ("east", "right_click"),
        ("west", "middle_click"),
        ("north", "cmd+space"),
        ("left_bumper", "cmd+["),
        ("right_bumper", "cmd+]"),
        ("left_trigger", "precision"),
        ("right_trigger", "turbo"),
        ("dpad_up", "up"),
        ("dpad_down", "down"),
        ("dpad_left", "left"),
        ("dpad_right", "right"),
        ("start", "ctrl+up"),
        ("select", "cmd+tab"),
        ("mode", "toggle"),
        ("left_thumb", "none"),
        ("right_thumb", "none"),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v.to_string()))
    .collect()
}

/// Validated config ready for the input loop.
#[derive(Clone, Debug)]
pub struct Config {
    pub cursor: Cursor,
    pub scroll: Scroll,
    pub repeat: Repeat,
    pub bindings: HashMap<Button, Action>,
}

impl Default for Config {
    fn default() -> Self {
        Config::from_raw(RawConfig::default()).expect("default config is valid")
    }
}

impl Config {
    pub fn from_raw(raw: RawConfig) -> Result<Config> {
        let mut merged = default_buttons();
        merged.extend(raw.buttons);
        let mut bindings = HashMap::new();
        for (name, action) in &merged {
            let Some(button) = button_by_name(name) else {
                bail!("unknown button `{name}` in [buttons]");
            };
            let action = Action::parse(action).with_context(|| format!("button `{name}`"))?;
            bindings.insert(button, action);
        }
        for s in [&raw.cursor.stick, &raw.scroll.stick] {
            if !(0.0..0.95).contains(&s.deadzone) {
                bail!("deadzone must be in 0.0..0.95, got {}", s.deadzone);
            }
            if s.exponent <= 0.0 {
                bail!("exponent must be positive, got {}", s.exponent);
            }
        }
        Ok(Config { cursor: raw.cursor, scroll: raw.scroll, repeat: raw.repeat, bindings })
    }

    #[cfg(test)]
    pub fn parse(text: &str) -> Result<Config> {
        Config::from_raw(toml::from_str(text).context("invalid TOML")?)
    }
}

pub fn path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("grip")
        .join("config.toml")
}

/// Loads the config, writing the defaults first if the file doesn't exist.
/// Returns whether the file was just created (first run).
pub fn load() -> Result<(Config, bool)> {
    let path = path();
    let created = !path.exists();
    if created {
        std::fs::create_dir_all(path.parent().unwrap())?;
        std::fs::write(&path, to_toml(&RawConfig::with_default_buttons()))?;
        log::info!("wrote default config to {}", path.display());
    }
    Ok((Config::from_raw(load_raw()?).with_context(|| format!("in {}", path.display()))?, created))
}

/// Reads the file as-is (buttons merged over the defaults), for the settings window.
pub fn load_raw() -> Result<RawConfig> {
    let path = path();
    let text = std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let mut raw: RawConfig = toml::from_str(&text).with_context(|| format!("invalid TOML in {}", path.display()))?;
    let mut buttons = default_buttons();
    buttons.extend(std::mem::take(&mut raw.buttons));
    raw.buttons = buttons;
    Ok(raw)
}

/// Validates and writes the config, returning the parsed result.
pub fn save(raw: &RawConfig) -> Result<Config> {
    let config = Config::from_raw(raw.clone())?;
    let path = path();
    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(&path, to_toml(raw)).with_context(|| format!("writing {}", path.display()))?;
    Ok(config)
}

impl RawConfig {
    pub fn with_default_buttons() -> RawConfig {
        RawConfig { buttons: default_buttons(), ..RawConfig::default() }
    }
}

fn to_toml(raw: &RawConfig) -> String {
    let body = toml::to_string_pretty(raw).expect("serializable");
    format!(
        "# GRIP. config. Edit here or in the Settings window (menu bar → Settings…).\n\
         # After editing by hand, use \"Reload Config\" in the menu bar.\n\
         # Button actions: left_click, right_click, middle_click, precision, turbo, toggle, none,\n\
         # or a key combo like \"cmd+shift+tab\", \"ctrl+up\", \"space\", \"f5\".\n\
         # Buttons use the Xbox-style layout: south=A/✕, east=B/○, west=X/□, north=Y/△.\n\n{body}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapping::MouseButton;

    #[test]
    fn default_toml_round_trips() {
        let cfg = Config::parse(&to_toml(&RawConfig::with_default_buttons())).unwrap();
        assert_eq!(cfg.bindings[&Button::South], Action::Mouse(MouseButton::Left));
        assert_eq!(cfg.cursor.stick.max_speed, 1400.0);
    }

    #[test]
    fn partial_config_merges_over_defaults() {
        let cfg = Config::parse("[cursor]\nmax_speed = 900\n[buttons]\nsouth = \"space\"\n").unwrap();
        assert_eq!(cfg.cursor.stick.max_speed, 900.0);
        assert_eq!(cfg.cursor.precision_multiplier, 0.3);
        assert!(matches!(cfg.bindings[&Button::South], Action::Key(_)));
        assert_eq!(cfg.bindings[&Button::East], Action::Mouse(MouseButton::Right));
    }

    #[test]
    fn settings_window_json_round_trips() {
        // Shape the settings page sends back on save.
        let json = serde_json::to_string(&RawConfig::with_default_buttons()).unwrap();
        assert!(json.contains("\"max_speed\":1400.0") && json.contains("\"deadzone\":0.12"));
        let raw: RawConfig = serde_json::from_str(&json).unwrap();
        let cfg = Config::from_raw(raw).unwrap();
        assert_eq!(cfg.bindings.len(), default_buttons().len());
    }

    #[test]
    fn empty_config_is_default() {
        let cfg = Config::parse("").unwrap();
        assert_eq!(cfg.bindings.len(), default_buttons().len());
    }

    #[test]
    fn rejects_bad_values() {
        assert!(Config::parse("[buttons]\nfoo = \"space\"").is_err());
        assert!(Config::parse("[buttons]\nsouth = \"cmd+wat\"").is_err());
        assert!(Config::parse("[cursor]\ndeadzone = 1.5").is_err());
    }
}
