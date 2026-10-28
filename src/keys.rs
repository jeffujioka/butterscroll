//! Key combinations: parsing `"cmd+shift+4"` into a US-ANSI virtual keycode plus
//! CoreGraphics modifier flags.

use std::fmt;

pub const FLAG_SHIFT: u64 = 0x0002_0000;
pub const FLAG_CONTROL: u64 = 0x0004_0000;
pub const FLAG_OPTION: u64 = 0x0008_0000;
pub const FLAG_COMMAND: u64 = 0x0010_0000;
pub const FLAG_NUMPAD: u64 = 0x0020_0000;
pub const FLAG_FN: u64 = 0x0080_0000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shortcut {
    pub keycode: u16,
    /// Flags to set on the synthesized key events, including the implicit
    /// fn/numeric-pad bits real keyboards add to arrows and function keys.
    pub flags: u64,
}

impl Shortcut {
    pub fn parse(text: &str) -> Result<Shortcut, String> {
        let mut flags = 0;
        let mut key = None;
        for part in text.split('+').map(str::trim) {
            let lower = part.to_ascii_lowercase();
            let modifier = match lower.as_str() {
                "cmd" | "command" => Some(FLAG_COMMAND),
                "shift" => Some(FLAG_SHIFT),
                "ctrl" | "control" => Some(FLAG_CONTROL),
                "alt" | "opt" | "option" => Some(FLAG_OPTION),
                "fn" => Some(FLAG_FN),
                _ => None,
            };
            match (modifier, key) {
                (Some(m), _) => flags |= m,
                (None, None) => {
                    let (code, implicit) =
                        keycode(&lower).ok_or_else(|| format!("unknown key {part:?} in shortcut {text:?}"))?;
                    flags |= implicit;
                    key = Some(code);
                }
                (None, Some(_)) => return Err(format!("shortcut {text:?} has more than one non-modifier key")),
            }
        }
        let keycode = key.ok_or_else(|| format!("shortcut {text:?} has no key"))?;
        Ok(Shortcut { keycode, flags })
    }
}

impl fmt::Display for Shortcut {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "keycode {} flags {:#x}", self.keycode, self.flags)
    }
}

/// Returns (virtual keycode, implicit flags). Codes from HIToolbox Events.h (ANSI layout).
fn keycode(name: &str) -> Option<(u16, u64)> {
    const ARROW: u64 = FLAG_FN | FLAG_NUMPAD;
    let plain = |c| Some((c, 0));
    let fnkey = |c| Some((c, FLAG_FN));
    match name {
        "a" => plain(0), "s" => plain(1), "d" => plain(2), "f" => plain(3), "h" => plain(4),
        "g" => plain(5), "z" => plain(6), "x" => plain(7), "c" => plain(8), "v" => plain(9),
        "b" => plain(11), "q" => plain(12), "w" => plain(13), "e" => plain(14), "r" => plain(15),
        "y" => plain(16), "t" => plain(17), "1" => plain(18), "2" => plain(19), "3" => plain(20),
        "4" => plain(21), "6" => plain(22), "5" => plain(23), "=" => plain(24), "9" => plain(25),
        "7" => plain(26), "-" => plain(27), "8" => plain(28), "0" => plain(29), "]" => plain(30),
        "o" => plain(31), "u" => plain(32), "[" => plain(33), "i" => plain(34), "p" => plain(35),
        "return" | "enter" => plain(36), "l" => plain(37), "j" => plain(38), "'" => plain(39),
        "k" => plain(40), ";" => plain(41), "\\" => plain(42), "," => plain(43), "/" => plain(44),
        "n" => plain(45), "m" => plain(46), "." => plain(47), "tab" => plain(48),
        "space" => plain(49), "`" => plain(50), "delete" | "backspace" => plain(51),
        "escape" | "esc" => plain(53),
        "f1" => fnkey(122), "f2" => fnkey(120), "f3" => fnkey(99), "f4" => fnkey(118),
        "f5" => fnkey(96), "f6" => fnkey(97), "f7" => fnkey(98), "f8" => fnkey(100),
        "f9" => fnkey(101), "f10" => fnkey(109), "f11" => fnkey(103), "f12" => fnkey(111),
        "home" => fnkey(115), "pageup" => fnkey(116), "forwarddelete" => fnkey(117),
        "end" => fnkey(119), "pagedown" => fnkey(121),
        "left" => Some((123, ARROW)), "right" => Some((124, ARROW)),
        "down" => Some((125, ARROW)), "up" => Some((126, ARROW)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_modifiers_in_any_order_and_case() {
        let a = Shortcut::parse("Cmd+Shift+4").unwrap();
        let b = Shortcut::parse("shift + command + 4").unwrap();
        assert_eq!(a, b);
        assert_eq!(a, Shortcut { keycode: 21, flags: FLAG_COMMAND | FLAG_SHIFT });
    }

    #[test]
    fn arrows_carry_fn_and_numpad_like_real_keyboards() {
        let s = Shortcut::parse("ctrl+up").unwrap();
        assert_eq!(s, Shortcut { keycode: 126, flags: FLAG_CONTROL | FLAG_FN | FLAG_NUMPAD });
    }

    #[test]
    fn rejects_malformed() {
        assert!(Shortcut::parse("cmd+shift").unwrap_err().contains("no key"));
        assert!(Shortcut::parse("a+b").unwrap_err().contains("more than one"));
        assert!(Shortcut::parse("cmd+ä").unwrap_err().contains("unknown key"));
    }
}
