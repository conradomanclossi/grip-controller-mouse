//! Button actions and parsing of key-combo strings like `"cmd+shift+tab"`.

use anyhow::{Result, bail};
use core_graphics::event::CGEventFlags;
use gilrs::Button;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Modifier {
    Cmd,
    Shift,
    Option,
    Ctrl,
}

impl Modifier {
    pub fn keycode(self) -> u16 {
        match self {
            Modifier::Cmd => 0x37,
            Modifier::Shift => 0x38,
            Modifier::Option => 0x3A,
            Modifier::Ctrl => 0x3B,
        }
    }

    pub fn flag(self) -> CGEventFlags {
        match self {
            Modifier::Cmd => CGEventFlags::CGEventFlagCommand,
            Modifier::Shift => CGEventFlags::CGEventFlagShift,
            Modifier::Option => CGEventFlags::CGEventFlagAlternate,
            Modifier::Ctrl => CGEventFlags::CGEventFlagControl,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyCombo {
    pub modifiers: Vec<Modifier>,
    pub keycode: u16,
    /// Extra flags real keyboards set for this key (arrows, F-keys, nav keys).
    pub key_flags: CGEventFlags,
}

impl KeyCombo {
    pub fn flags(&self) -> CGEventFlags {
        self.modifiers
            .iter()
            .fold(self.key_flags, |acc, m| acc | m.flag())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    None,
    Mouse(MouseButton),
    Key(KeyCombo),
    Toggle,
    Precision,
    Turbo,
}

impl Action {
    pub fn parse(s: &str) -> Result<Action> {
        let s = s.trim().to_ascii_lowercase();
        Ok(match s.as_str() {
            "" | "none" => Action::None,
            "left_click" => Action::Mouse(MouseButton::Left),
            "right_click" => Action::Mouse(MouseButton::Right),
            "middle_click" => Action::Mouse(MouseButton::Middle),
            "toggle" => Action::Toggle,
            "precision" => Action::Precision,
            "turbo" => Action::Turbo,
            _ => Action::Key(parse_combo(&s)?),
        })
    }
}

fn parse_combo(s: &str) -> Result<KeyCombo> {
    let parts: Vec<&str> = s.split('+').map(str::trim).collect();
    let (key, mods) = parts.split_last().expect("split yields at least one part");
    let mut modifiers = Vec::new();
    for m in mods {
        modifiers.push(match *m {
            "cmd" | "command" => Modifier::Cmd,
            "shift" => Modifier::Shift,
            "opt" | "option" | "alt" => Modifier::Option,
            "ctrl" | "control" => Modifier::Ctrl,
            other => bail!("unknown modifier `{other}` in `{s}`"),
        });
    }
    let (keycode, key_flags) = keycode(key).ok_or_else(|| anyhow::anyhow!("unknown key `{key}` in `{s}`"))?;
    Ok(KeyCombo { modifiers, keycode, key_flags })
}

/// macOS virtual keycodes (ANSI layout).
fn keycode(key: &str) -> Option<(u16, CGEventFlags)> {
    let none = CGEventFlags::CGEventFlagNull;
    let fn_ = CGEventFlags::CGEventFlagSecondaryFn;
    let arrow = fn_ | CGEventFlags::CGEventFlagNumericPad;
    let code = match key {
        "a" => 0, "s" => 1, "d" => 2, "f" => 3, "h" => 4, "g" => 5, "z" => 6, "x" => 7,
        "c" => 8, "v" => 9, "b" => 11, "q" => 12, "w" => 13, "e" => 14, "r" => 15,
        "y" => 16, "t" => 17, "1" => 18, "2" => 19, "3" => 20, "4" => 21, "6" => 22,
        "5" => 23, "=" => 24, "9" => 25, "7" => 26, "-" => 27, "8" => 28, "0" => 29,
        "]" => 30, "o" => 31, "u" => 32, "[" => 33, "i" => 34, "p" => 35,
        "return" | "enter" => 36, "l" => 37, "j" => 38, "'" => 39, "k" => 40, ";" => 41,
        "\\" => 42, "," => 43, "/" => 44, "n" => 45, "m" => 46, "." => 47,
        "tab" => 48, "space" => 49, "`" => 50, "backspace" | "delete" => 51,
        "escape" | "esc" => 53,
        "left" => return Some((123, arrow)),
        "right" => return Some((124, arrow)),
        "down" => return Some((125, arrow)),
        "up" => return Some((126, arrow)),
        "home" => return Some((115, fn_)),
        "end" => return Some((119, fn_)),
        "pageup" => return Some((116, fn_)),
        "pagedown" => return Some((121, fn_)),
        "forwarddelete" => return Some((117, fn_)),
        "f1" => return Some((122, fn_)), "f2" => return Some((120, fn_)),
        "f3" => return Some((99, fn_)), "f4" => return Some((118, fn_)),
        "f5" => return Some((96, fn_)), "f6" => return Some((97, fn_)),
        "f7" => return Some((98, fn_)), "f8" => return Some((100, fn_)),
        "f9" => return Some((101, fn_)), "f10" => return Some((109, fn_)),
        "f11" => return Some((103, fn_)), "f12" => return Some((111, fn_)),
        _ => return None,
    };
    Some((code, none))
}

/// Config names for controller buttons (gilrs' unified layout).
pub const BUTTON_NAMES: &[(&str, Button)] = &[
    ("south", Button::South),
    ("east", Button::East),
    ("west", Button::West),
    ("north", Button::North),
    ("left_bumper", Button::LeftTrigger),
    ("right_bumper", Button::RightTrigger),
    ("left_trigger", Button::LeftTrigger2),
    ("right_trigger", Button::RightTrigger2),
    ("select", Button::Select),
    ("start", Button::Start),
    ("mode", Button::Mode),
    ("left_thumb", Button::LeftThumb),
    ("right_thumb", Button::RightThumb),
    ("dpad_up", Button::DPadUp),
    ("dpad_down", Button::DPadDown),
    ("dpad_left", Button::DPadLeft),
    ("dpad_right", Button::DPadRight),
];

pub fn button_by_name(name: &str) -> Option<Button> {
    BUTTON_NAMES
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, b)| *b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_actions() {
        assert_eq!(Action::parse("left_click").unwrap(), Action::Mouse(MouseButton::Left));
        assert_eq!(Action::parse(" None ").unwrap(), Action::None);
        assert_eq!(Action::parse("precision").unwrap(), Action::Precision);
    }

    #[test]
    fn parses_combos() {
        let Action::Key(k) = Action::parse("cmd+shift+tab").unwrap() else { panic!() };
        assert_eq!(k.modifiers, vec![Modifier::Cmd, Modifier::Shift]);
        assert_eq!(k.keycode, 48);
        assert!(k.flags().contains(CGEventFlags::CGEventFlagCommand | CGEventFlags::CGEventFlagShift));

        let Action::Key(k) = Action::parse("cmd+[").unwrap() else { panic!() };
        assert_eq!(k.keycode, 33);
    }

    #[test]
    fn arrows_carry_fn_and_numpad_flags() {
        let Action::Key(k) = Action::parse("ctrl+up").unwrap() else { panic!() };
        assert_eq!(k.keycode, 126);
        let f = k.flags();
        assert!(f.contains(CGEventFlags::CGEventFlagControl));
        assert!(f.contains(CGEventFlags::CGEventFlagSecondaryFn | CGEventFlags::CGEventFlagNumericPad));
    }

    #[test]
    fn rejects_garbage() {
        assert!(Action::parse("hyper+q").is_err());
        assert!(Action::parse("cmd+nope").is_err());
    }

    #[test]
    fn button_names_resolve() {
        assert_eq!(button_by_name("south"), Some(Button::South));
        assert_eq!(button_by_name("bogus"), None);
    }
}
