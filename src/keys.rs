//! Key combos such as `ctrl+shift+a`, `space` or `f5`.
//!
//! The text form is a `+`-separated list of key names, pressed in order and
//! released in reverse. It's what the CLI accepts, what the desktop app sends,
//! and how combos are serialized.

use crate::Error;
use enigo::Key;
use std::fmt;
use std::str::FromStr;

/// Most keys a combo may contain. Enough for any real shortcut.
const MAX_KEYS: usize = 5;

/// Single-character keys typed as themselves.
const PUNCTUATION: &str = "-=[]\\;',./`";

/// One or more keys pressed together.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyCombo {
    /// Canonical names, in press order.
    names: Vec<String>,
    keys: Vec<Key>,
}

impl KeyCombo {
    /// Keys in the order they're pressed.
    pub fn keys(&self) -> &[Key] {
        &self.keys
    }
}

impl fmt::Display for KeyCombo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.names.join("+"))
    }
}

impl FromStr for KeyCombo {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Error> {
        let mut names = Vec::new();
        let mut keys = Vec::new();
        for part in s.split('+') {
            let (name, key) = lookup(part.trim())
                .ok_or_else(|| Error::new(format!("unknown key {part:?} in {s:?}")))?;
            if names.contains(&name) {
                return Err(Error::new(format!("{name} appears twice in {s:?}")));
            }
            names.push(name);
            keys.push(key);
        }
        if keys.len() > MAX_KEYS {
            return Err(Error::new(format!("{s:?} has more than {MAX_KEYS} keys")));
        }
        Ok(Self { names, keys })
    }
}

/// Maps a key name (case-insensitive, with common aliases) to its canonical
/// name and enigo key. Only keys available on Windows, macOS and Linux.
fn lookup(raw: &str) -> Option<(String, Key)> {
    let lower = raw.to_ascii_lowercase();

    let mut chars = lower.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        if c.is_ascii_alphanumeric() || PUNCTUATION.contains(c) {
            return Some((c.to_string(), Key::Unicode(c)));
        }
    }

    if let Some(n) = lower.strip_prefix('f').and_then(|n| n.parse::<u8>().ok()) {
        const F: [Key; 20] = [
            Key::F1, Key::F2, Key::F3, Key::F4, Key::F5, Key::F6, Key::F7,
            Key::F8, Key::F9, Key::F10, Key::F11, Key::F12, Key::F13, Key::F14,
            Key::F15, Key::F16, Key::F17, Key::F18, Key::F19, Key::F20,
        ];
        let key = *F.get(usize::from(n).checked_sub(1)?)?;
        return Some((format!("f{n}"), key));
    }

    let (name, key) = match lower.as_str() {
        "ctrl" | "control" => ("ctrl", Key::Control),
        "shift" => ("shift", Key::Shift),
        "alt" | "option" => ("alt", Key::Alt),
        "meta" | "cmd" | "command" | "super" | "win" | "windows" => ("meta", Key::Meta),
        "space" => ("space", Key::Space),
        "enter" | "return" => ("enter", Key::Return),
        "tab" => ("tab", Key::Tab),
        "escape" | "esc" => ("escape", Key::Escape),
        "backspace" => ("backspace", Key::Backspace),
        "delete" | "del" => ("delete", Key::Delete),
        "capslock" => ("capslock", Key::CapsLock),
        "home" => ("home", Key::Home),
        "end" => ("end", Key::End),
        "pageup" | "pgup" => ("pageup", Key::PageUp),
        "pagedown" | "pgdn" => ("pagedown", Key::PageDown),
        "up" | "arrowup" => ("up", Key::UpArrow),
        "down" | "arrowdown" => ("down", Key::DownArrow),
        "left" | "arrowleft" => ("left", Key::LeftArrow),
        "right" | "arrowright" => ("right", Key::RightArrow),
        "plus" => ("plus", Key::Unicode('+')),
        _ => return None,
    };
    Some((name.to_string(), key))
}

#[cfg(feature = "serde")]
impl serde::Serialize for KeyCombo {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for KeyCombo {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = <std::borrow::Cow<'de, str>>::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_canonicalizes() {
        let combo: KeyCombo = "Control + Shift + A".parse().unwrap();
        assert_eq!(combo.to_string(), "ctrl+shift+a");
        assert_eq!(combo.keys(), &[Key::Control, Key::Shift, Key::Unicode('a')]);

        assert_eq!("F12".parse::<KeyCombo>().unwrap().keys(), &[Key::F12]);
        assert_eq!("cmd+.".parse::<KeyCombo>().unwrap().to_string(), "meta+.");
        assert_eq!("ArrowUp".parse::<KeyCombo>().unwrap().to_string(), "up");
        assert_eq!("ctrl+plus".parse::<KeyCombo>().unwrap().keys()[1], Key::Unicode('+'));
    }

    #[test]
    fn rejects_bad_input() {
        for bad in ["", "ctrl+", "f0", "f21", "hyper", "a+a", "é", "ctrl+alt+shift+meta+a+b"] {
            assert!(bad.parse::<KeyCombo>().is_err(), "{bad:?} should be rejected");
        }
    }
}
