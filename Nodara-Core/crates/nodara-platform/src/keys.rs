//! Keyboard name -> virtual-key mapping.
//!
//! Keeping the mapping pure and table-driven means it is fully unit tested even
//! though the calls that consume it are Win32-specific.

/// A virtual key with the modifier set needed to produce it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyStroke {
    /// Virtual-key code.
    pub virtual_key: u8,
    /// Whether the `Shift` modifier must be held.
    pub shift: bool,
}

/// Windows virtual-key codes referenced by name.
pub mod vk {
    /// Left shift.
    pub const SHIFT: u8 = 0x10;
    /// Either control key.
    pub const CONTROL: u8 = 0x11;
    /// Either alt key.
    pub const MENU: u8 = 0x12;
    /// Left Windows key.
    pub const LWIN: u8 = 0x5B;
    /// Caps lock.
    pub const CAPITAL: u8 = 0x14;
    /// `Backspace`.
    pub const BACK: u8 = 0x08;
    /// `Tab`.
    pub const TAB: u8 = 0x09;
    /// `Enter`.
    pub const RETURN: u8 = 0x0D;
    /// `Escape`.
    pub const ESCAPE: u8 = 0x1B;
    /// `Space`.
    pub const SPACE: u8 = 0x20;
    /// `Page Up`.
    pub const PRIOR: u8 = 0x21;
    /// `Page Down`.
    pub const NEXT: u8 = 0x22;
    /// `End`.
    pub const END: u8 = 0x23;
    /// `Home`.
    pub const HOME: u8 = 0x24;
    /// `Left Arrow`.
    pub const LEFT: u8 = 0x25;
    /// `Up Arrow`.
    pub const UP: u8 = 0x26;
    /// `Right Arrow`.
    pub const RIGHT: u8 = 0x27;
    /// `Down Arrow`.
    pub const DOWN: u8 = 0x28;
    /// `Insert`.
    pub const INSERT: u8 = 0x2D;
    /// `Delete`.
    pub const DELETE: u8 = 0x2E;
    /// `;:` on a US layout.
    pub const OEM_1: u8 = 0xBA;
    /// `=+`.
    pub const OEM_PLUS: u8 = 0xBB;
    /// `,<`.
    pub const OEM_COMMA: u8 = 0xBC;
    /// `-_`.
    pub const OEM_MINUS: u8 = 0xBD;
    /// `.>`.
    pub const OEM_PERIOD: u8 = 0xBE;
    /// `/?`.
    pub const OEM_2: u8 = 0xBF;
    /// `` `~ ``.
    pub const OEM_3: u8 = 0xC0;
    /// `[{`.
    pub const OEM_4: u8 = 0xDB;
    /// `\|`.
    pub const OEM_5: u8 = 0xDC;
    /// `]}`.
    pub const OEM_6: u8 = 0xDD;
    /// `'"`.
    pub const OEM_7: u8 = 0xDE;
    /// First function key (`F1`).
    pub const F1: u8 = 0x70;
}

/// Whether a modifier name maps to a modifier key.
pub fn modifier_vk(name: &str) -> Option<u8> {
    match normalise(name).as_str() {
        "ctrl" | "control" => Some(vk::CONTROL),
        "alt" | "menu" => Some(vk::MENU),
        "shift" => Some(vk::SHIFT),
        "win" | "windows" | "meta" | "super" | "cmd" | "command" => Some(vk::LWIN),
        _ => None,
    }
}

/// Resolve a key name to a virtual key plus required shift state.
pub fn resolve(name: &str) -> Option<KeyStroke> {
    let normalised = normalise(name);
    if let Some(virtual_key) = named_key(&normalised) {
        return Some(KeyStroke {
            virtual_key,
            shift: false,
        });
    }
    resolve_shifted(&normalised)
}

fn named_key(name: &str) -> Option<u8> {
    Some(match name {
        "enter" | "return" => vk::RETURN,
        "tab" => vk::TAB,
        "esc" | "escape" => vk::ESCAPE,
        "space" | "spacebar" => vk::SPACE,
        "backspace" | "back" => vk::BACK,
        "delete" | "del" => vk::DELETE,
        "insert" | "ins" => vk::INSERT,
        "home" => vk::HOME,
        "end" => vk::END,
        "pageup" | "pgup" => vk::PRIOR,
        "pagedown" | "pgdn" => vk::NEXT,
        "left" | "arrowleft" => vk::LEFT,
        "up" | "arrowup" => vk::UP,
        "right" | "arrowright" => vk::RIGHT,
        "down" | "arrowdown" => vk::DOWN,
        "capslock" | "caps" => vk::CAPITAL,
        ";" => vk::OEM_1,
        "=" => vk::OEM_PLUS,
        "," => vk::OEM_COMMA,
        "-" => vk::OEM_MINUS,
        "." => vk::OEM_PERIOD,
        "/" => vk::OEM_2,
        "`" => vk::OEM_3,
        "[" => vk::OEM_4,
        "\\" => vk::OEM_5,
        "]" => vk::OEM_6,
        "'" => vk::OEM_7,
        _ => return None,
    })
}

fn resolve_shifted(name: &str) -> Option<KeyStroke> {
    if let Some(digit) = name.chars().next().filter(|_| name.len() == 1) {
        if digit.is_ascii_digit() {
            return Some(KeyStroke {
                virtual_key: digit as u8,
                shift: false,
            });
        }
        if digit.is_ascii_lowercase() {
            return Some(KeyStroke {
                virtual_key: digit.to_ascii_uppercase() as u8,
                shift: false,
            });
        }
    }
    let shift_pairs: &[(&str, u8)] = &[
        ("!", b'1'),
        ("@", b'2'),
        ("#", b'3'),
        ("$", b'4'),
        ("%", b'5'),
        ("^", b'6'),
        ("&", b'7'),
        ("*", b'8'),
        ("(", b'9'),
        (")", b'0'),
        ("_", vk::OEM_MINUS),
        ("+", vk::OEM_PLUS),
        ("{", vk::OEM_4),
        ("}", vk::OEM_6),
        ("|", vk::OEM_5),
        (":", vk::OEM_1),
        ("\"", vk::OEM_7),
        ("<", vk::OEM_COMMA),
        (">", vk::OEM_PERIOD),
        ("?", vk::OEM_2),
        ("~", vk::OEM_3),
    ];
    for (symbol, virtual_key) in shift_pairs {
        if *symbol == name {
            return Some(KeyStroke {
                virtual_key: *virtual_key,
                shift: true,
            });
        }
    }
    if let Some(rest) = name.strip_prefix('f') {
        if let Ok(index) = rest.parse::<u8>() {
            if (1..=24).contains(&index) {
                return Some(KeyStroke {
                    virtual_key: vk::F1 + index - 1,
                    shift: false,
                });
            }
        }
    }
    None
}

/// Parse a chord such as `ctrl+shift+s` into an ordered key sequence.
pub fn parse_chord(chord: &str) -> Option<(Vec<u8>, KeyStroke)> {
    let parts: Vec<&str> = chord
        .split('+')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect();
    if parts.is_empty() {
        return None;
    }
    let (last, modifiers) = parts.split_last()?;
    let mut modifier_keys = Vec::with_capacity(modifiers.len());
    for modifier in modifiers {
        modifier_keys.push(modifier_vk(modifier)?);
    }
    let stroke = resolve(last)?;
    Some((modifier_keys, stroke))
}

fn normalise(name: &str) -> String {
    name.trim().to_ascii_lowercase().replace([' ', '_'], "")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_named_keys() {
        assert_eq!(resolve("Enter").unwrap().virtual_key, vk::RETURN);
        assert_eq!(resolve("esc").unwrap().virtual_key, vk::ESCAPE);
        assert_eq!(resolve("F12").unwrap().virtual_key, vk::F1 + 11);
        assert_eq!(resolve("arrowleft").unwrap().virtual_key, vk::LEFT);
    }

    #[test]
    fn resolves_letters_and_digits_case_insensitively() {
        assert_eq!(resolve("a").unwrap().virtual_key, b'A');
        assert_eq!(resolve("Z").unwrap().virtual_key, b'Z');
        assert_eq!(resolve("7").unwrap().virtual_key, b'7');
    }

    #[test]
    fn shifted_symbols_set_the_shift_flag() {
        let bang = resolve("!").unwrap();
        assert_eq!(bang.virtual_key, b'1');
        assert!(bang.shift);
        assert!(!resolve("1").unwrap().shift);
    }

    #[test]
    fn rejects_unknown_keys() {
        assert!(resolve("definitely-not-a-key").is_none());
    }

    #[test]
    fn parses_chords() {
        let (modifiers, stroke) = parse_chord("Ctrl+Shift+S").unwrap();
        assert_eq!(modifiers, vec![vk::CONTROL, vk::SHIFT]);
        assert_eq!(stroke.virtual_key, b'S');
    }

    #[test]
    fn rejects_chords_with_unknown_modifiers() {
        assert!(parse_chord("hyper+x").is_none());
    }
}
