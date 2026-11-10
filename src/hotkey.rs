//! Parses hotkey descriptions such as `Ctrl+Alt+S` from the settings file.

pub const MOD_ALT: u32 = 0x1;
pub const MOD_CONTROL: u32 = 0x2;
pub const MOD_SHIFT: u32 = 0x4;
pub const MOD_WIN: u32 = 0x8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hotkey {
    /// `MOD_*` flags, as taken by `RegisterHotKey`.
    pub modifiers: u32,
    /// Virtual-key code.
    pub key: u32,
}

/// Keys that have names rather than a single character.
const NAMED_KEYS: &[(&str, u32)] = &[
    ("space", 0x20),
    ("enter", 0x0D),
    ("tab", 0x09),
    ("esc", 0x1B),
    ("escape", 0x1B),
    ("backspace", 0x08),
    ("insert", 0x2D),
    ("delete", 0x2E),
    ("home", 0x24),
    ("end", 0x23),
    ("pageup", 0x21),
    ("pagedown", 0x22),
    ("left", 0x25),
    ("up", 0x26),
    ("right", 0x27),
    ("down", 0x28),
    ("pause", 0x13),
    ("printscreen", 0x2C),
    ("scrolllock", 0x91),
];

pub fn parse(text: &str) -> Result<Hotkey, String> {
    let mut modifiers = 0;
    let mut key = None;
    for part in text.split('+').map(str::trim) {
        let lower = part.to_ascii_lowercase();
        match lower.as_str() {
            "ctrl" | "control" => modifiers |= MOD_CONTROL,
            "alt" => modifiers |= MOD_ALT,
            "shift" => modifiers |= MOD_SHIFT,
            "win" | "windows" => modifiers |= MOD_WIN,
            _ if key.is_some() => return Err(format!("\"{text}\" has more than one key")),
            name => key = Some(key_code(name).ok_or_else(|| format!("\"{part}\" is not a key I know"))?),
        }
    }
    let key = key.ok_or_else(|| format!("\"{text}\" has no key, only modifiers"))?;
    // A bare letter or digit as a global hotkey would make it untypeable.
    let standalone = (0x70..=0x87).contains(&key) || matches!(key, 0x13 | 0x2C | 0x91);
    if modifiers == 0 && !standalone {
        return Err(format!("\"{text}\" needs Ctrl, Alt, Shift or Win"));
    }
    Ok(Hotkey { modifiers, key })
}

fn key_code(name: &str) -> Option<u32> {
    if let [c] = name.as_bytes() {
        if c.is_ascii_alphanumeric() {
            return Some(u32::from(c.to_ascii_uppercase()));
        }
    }
    if let Some(n) = name.strip_prefix('f').and_then(|n| n.parse::<u32>().ok()) {
        return (1..=24).contains(&n).then(|| 0x70 + n - 1);
    }
    NAMED_KEYS
        .iter()
        .find(|(known, _)| *known == name)
        .map(|&(_, code)| code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_common_combinations() {
        assert_eq!(
            parse("Ctrl+Alt+S"),
            Ok(Hotkey {
                modifiers: MOD_CONTROL | MOD_ALT,
                key: 0x53
            })
        );
        assert_eq!(
            parse(" win + shift + f12 "),
            Ok(Hotkey {
                modifiers: MOD_WIN | MOD_SHIFT,
                key: 0x7B
            })
        );
        assert_eq!(
            parse("Control+0"),
            Ok(Hotkey {
                modifiers: MOD_CONTROL,
                key: 0x30
            })
        );
        assert_eq!(
            parse("Alt+PageDown"),
            Ok(Hotkey {
                modifiers: MOD_ALT,
                key: 0x22
            })
        );
    }

    #[test]
    fn some_keys_work_without_modifiers() {
        assert_eq!(
            parse("F24"),
            Ok(Hotkey {
                modifiers: 0,
                key: 0x87
            })
        );
        assert_eq!(
            parse("Pause"),
            Ok(Hotkey {
                modifiers: 0,
                key: 0x13
            })
        );
        assert!(parse("S").is_err());
        assert!(parse("Space").is_err());
    }

    #[test]
    fn rejects_nonsense() {
        assert!(parse("").is_err());
        assert!(parse("Ctrl+Alt").is_err());
        assert!(parse("Ctrl+A+B").is_err());
        assert!(parse("Ctrl+F25").is_err());
        assert!(parse("Ctrl+Banana").is_err());
    }
}
