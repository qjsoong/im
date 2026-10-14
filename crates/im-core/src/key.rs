//! A front-end neutral description of a keystroke.
//!
//! Terminals, X11 and configuration files all describe keys differently. They
//! all lower into [`KeyEvent`] so that a single [`Keymap`](crate::Keymap) can
//! drive every front end.

use std::fmt;

/// A keyboard modifier set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, PartialOrd, Ord)]
pub struct Mods(u8);

impl Mods {
    /// No modifiers.
    pub const NONE: Self = Self(0);
    /// `Shift`.
    pub const SHIFT: Self = Self(1 << 0);
    /// `Control`.
    pub const CTRL: Self = Self(1 << 1);
    /// `Alt` / `Meta`.
    pub const ALT: Self = Self(1 << 2);
    /// `Super` / `Command`.
    pub const SUPER: Self = Self(1 << 3);

    /// Raw bit set, for front ends that need to store it.
    pub const fn bits(self) -> u8 {
        self.0
    }

    /// Rebuild from [`Mods::bits`].
    pub const fn from_bits(bits: u8) -> Self {
        Self(bits)
    }

    /// True when the set is empty.
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// True when every modifier in `other` is present.
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// Add `other` to the set.
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// True when `shift` is held.
    pub const fn shift(self) -> bool {
        self.contains(Self::SHIFT)
    }

    /// True when `control` is held.
    pub const fn ctrl(self) -> bool {
        self.contains(Self::CTRL)
    }

    /// True when `alt` is held.
    pub const fn alt(self) -> bool {
        self.contains(Self::ALT)
    }

    /// True when `super` is held.
    pub const fn super_key(self) -> bool {
        self.contains(Self::SUPER)
    }
}

/// A key that is not a printable character.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Named {
    /// The main `Enter` / `Return` key.
    Enter,
    /// `Escape`.
    Escape,
    /// `Tab`.
    Tab,
    /// `Backspace`.
    Backspace,
    /// `Delete`.
    Delete,
    /// `Insert`.
    Insert,
    /// Cursor up.
    Up,
    /// Cursor down.
    Down,
    /// Cursor left.
    Left,
    /// Cursor right.
    Right,
    /// `Home`.
    Home,
    /// `End`.
    End,
    /// `Page Up`.
    PageUp,
    /// `Page Down`.
    PageDown,
    /// The space bar.
    Space,
    /// A function key, 1-based.
    F(u8),
}

impl Named {
    /// The canonical name used in configuration files.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Enter => "enter",
            Self::Escape => "escape",
            Self::Tab => "tab",
            Self::Backspace => "backspace",
            Self::Delete => "delete",
            Self::Insert => "insert",
            Self::Up => "up",
            Self::Down => "down",
            Self::Left => "left",
            Self::Right => "right",
            Self::Home => "home",
            Self::End => "end",
            Self::PageUp => "page_up",
            Self::PageDown => "page_down",
            Self::Space => "space",
            Self::F(_) => "f",
        }
    }

    fn parse(name: &str) -> Option<Self> {
        // `Page-Up` and `page_up` name the same key.
        let lower = name.to_ascii_lowercase().replace('-', "_");
        let named = match lower.as_str() {
            "enter" | "return" | "ret" | "cr" => Self::Enter,
            "esc" | "escape" => Self::Escape,
            "tab" => Self::Tab,
            "backspace" | "bs" => Self::Backspace,
            "delete" | "del" => Self::Delete,
            "insert" | "ins" => Self::Insert,
            "up" => Self::Up,
            "down" => Self::Down,
            "left" => Self::Left,
            "right" => Self::Right,
            "home" => Self::Home,
            "end" => Self::End,
            "pageup" | "page_up" | "pgup" | "prior" => Self::PageUp,
            "pagedown" | "page_down" | "pgdn" | "next" => Self::PageDown,
            "space" | "spc" => Self::Space,
            _ => {
                let number = lower.strip_prefix('f')?;
                let number: u8 = number.parse().ok()?;
                if number == 0 {
                    return None;
                }
                Self::F(number)
            }
        };
        Some(named)
    }
}

/// Symbols that have names, so a binding copied from another input method
/// (`C-bracketleft`, `minus`) keeps working.
fn symbol_key(name: &str) -> Option<char> {
    let lower = name.to_ascii_lowercase();
    let ch = match lower.as_str() {
        "minus" => '-',
        "equal" | "equals" => '=',
        "comma" => ',',
        "period" | "dot" => '.',
        "slash" => '/',
        "semicolon" => ';',
        "apostrophe" | "quote" => '\'',
        "bracketleft" | "leftbracket" => '[',
        "bracketright" | "rightbracket" => ']',
        "backslash" => '\\',
        "grave" | "backtick" => '`',
        _ => return None,
    };
    Some(ch)
}

/// The key half of a keystroke.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Key {
    /// A printable character, always lowercase for ASCII letters.
    Char(char),
    /// A named key.
    Named(Named),
}

/// A key plus its modifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct KeyEvent {
    /// Which key was pressed.
    pub key: Key,
    /// Which modifiers were held.
    pub mods: Mods,
}

/// Why a key binding string could not be parsed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct KeyParseError {
    /// Human-readable reason.
    pub message: String,
}

impl KeyEvent {
    /// Build an unmodified key event.
    pub const fn key(key: Key) -> Self {
        Self {
            key,
            mods: Mods::NONE,
        }
    }

    /// Build a plain character keystroke.
    ///
    /// A space is normalised to [`Named::Space`], which is how configuration
    /// files spell it.
    pub fn ch(ch: char) -> Self {
        Self::key(match ch {
            ' ' => Key::Named(Named::Space),
            ch => Key::Char(ch.to_ascii_lowercase()),
        })
    }

    /// Parse a binding such as `Enter`, `C-n`, `Ctrl+Alt+Delete` or
    /// `Page_Down`.
    ///
    /// Modifiers are written before the key, separated by `-` or `+`. Accepted
    /// modifier names are `c`/`ctrl`/`control`, `s`/`shift`, `a`/`alt`/`m`/
    /// `meta` and `super`/`win`/`cmd`. A single character is itself, a few
    /// symbols have names (`minus`, `bracketleft`, …) and everything else is
    /// looked up in [`Named`].
    pub fn parse(text: &str) -> Result<Self, KeyParseError> {
        let text = text.trim();
        if text.is_empty() {
            return Err(KeyParseError {
                message: "empty binding".to_owned(),
            });
        }
        if let Some(event) = split_modifiers(text)? {
            return Ok(event);
        }
        // Not `modifier-modifier-key`, so the whole string must be a key name.
        let key = parse_key(text).ok_or_else(|| KeyParseError {
            message: format!("unknown key {text:?}"),
        })?;
        Ok(Self {
            key,
            mods: Mods::NONE,
        })
    }
}

/// Interpret `text` as `modifier… key`.
///
/// Returns `Ok(None)` when the leading parts are not modifiers, which lets the
/// caller fall back to treating the whole string as a key name. That is what
/// makes `Page-Up` work while `C-n` is still two tokens wide.
fn split_modifiers(text: &str) -> Result<Option<KeyEvent>, KeyParseError> {
    let separator = if text.contains('+') { '+' } else { '-' };
    let parts: Vec<&str> = text.split(separator).map(str::trim).collect();
    if parts.len() < 2 {
        return Ok(None);
    }
    let Some((key_part, modifier_parts)) = parts.split_last() else {
        return Ok(None);
    };
    if key_part.is_empty() {
        return Ok(None);
    }
    let mut mods = Mods::NONE;
    for part in modifier_parts {
        let lower = part.to_ascii_lowercase();
        let modifier = match lower.as_str() {
            "c" | "ctrl" | "control" => Mods::CTRL,
            "s" | "shift" => Mods::SHIFT,
            "a" | "alt" | "m" | "meta" => Mods::ALT,
            "super" | "win" | "cmd" | "os" => Mods::SUPER,
            // Not a modifier after all; let the caller retry as a key name.
            _ => return Ok(None),
        };
        mods = mods.union(modifier);
    }
    let Some(key) = parse_key(key_part) else {
        return Err(KeyParseError {
            message: format!("unknown key {key_part:?}"),
        });
    };
    Ok(Some(KeyEvent { key, mods }))
}

/// Parse a single key token.
fn parse_key(text: &str) -> Option<Key> {
    if let Some(ch) = symbol_key(text) {
        return Some(Key::Char(ch));
    }
    if let Some(named) = Named::parse(text) {
        return Some(Key::Named(named));
    }
    let mut chars = text.chars();
    let first = chars.next()?;
    if chars.next().is_some() {
        return None;
    }
    // A space is one key, and `space` is how it is spelled. Normalising here
    // means `commit = [" "]` and `commit = ["space"]` are the same binding.
    Some(match first {
        ' ' => Key::Named(Named::Space),
        ch => Key::Char(ch.to_ascii_lowercase()),
    })
}

impl fmt::Display for KeyEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.mods.ctrl() {
            f.write_str("C-")?;
        }
        if self.mods.alt() {
            f.write_str("M-")?;
        }
        if self.mods.shift() {
            f.write_str("S-")?;
        }
        if self.mods.super_key() {
            f.write_str("Super-")?;
        }
        match self.key {
            Key::Char(ch) => write!(f, "{ch}"),
            Key::Named(Named::F(n)) => write!(f, "f{n}"),
            Key::Named(named) => f.write_str(named.as_str()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> KeyEvent {
        KeyEvent::parse(text).unwrap_or_else(|e| panic!("{text}: {e}"))
    }

    #[test]
    fn parses_plain_characters() {
        assert_eq!(parse("n"), KeyEvent::ch('n'));
        assert_eq!(parse("N"), KeyEvent::ch('n'));
        assert_eq!(parse("1"), KeyEvent::ch('1'));
        assert_eq!(parse("="), KeyEvent::ch('='));
    }

    #[test]
    fn parses_named_keys_and_aliases() {
        assert_eq!(parse("Enter").key, Key::Named(Named::Enter));
        assert_eq!(parse("return").key, Key::Named(Named::Enter));
        assert_eq!(parse("Esc").key, Key::Named(Named::Escape));
        assert_eq!(parse("PAGE_UP").key, Key::Named(Named::PageUp));
        assert_eq!(parse("Page-Up").key, Key::Named(Named::PageUp));
        assert_eq!(parse("pgdn").key, Key::Named(Named::PageDown));
        assert_eq!(parse("f5").key, Key::Named(Named::F(5)));
    }

    #[test]
    fn parses_symbol_names() {
        assert_eq!(parse("minus").key, Key::Char('-'));
        assert_eq!(parse("C-bracketleft").key, Key::Char('['));
        assert_eq!(parse("equal").key, Key::Char('='));
    }

    #[test]
    fn parses_modifiers_in_both_notations() {
        for text in ["C-M-S-x", "C+M+S+x", "ctrl+alt+shift+x"] {
            let event = parse(text);
            assert_eq!(event.key, Key::Char('x'), "{text}");
            assert!(
                event.mods.ctrl() && event.mods.alt() && event.mods.shift(),
                "{text}"
            );
            assert!(!event.mods.super_key(), "{text}");
        }
    }

    #[test]
    fn a_space_is_the_named_space_key() {
        assert_eq!(parse("space").key, Key::Named(Named::Space));
        assert_eq!(parse("space"), KeyEvent::ch(' '));
        assert_eq!(parse("C-space").mods, Mods::CTRL);
        // Bindings are trimmed, so the space bar has to be spelled `space`.
        assert!(KeyEvent::parse(" ").is_err());
    }

    #[test]
    fn rejects_nonsense() {
        for bad in ["", "C+", "ctrl+nope", "f0", "f999", "ab", "zz+z"] {
            assert!(KeyEvent::parse(bad).is_err(), "{bad:?} should not parse");
        }
    }

    #[test]
    fn lone_symbols_are_plain_keys() {
        assert_eq!(parse("+").key, Key::Char('+'));
        assert_eq!(parse("-").key, Key::Char('-'));
    }

    #[test]
    fn round_trips_through_display() {
        for text in ["C-n", "S-Tab", "M-x", "Page_Up", "f5", "1", "Super-q"] {
            let event = parse(text);
            let printed = event.to_string();
            assert_eq!(parse(&printed), event, "{text} printed as {printed}");
        }
    }
}
