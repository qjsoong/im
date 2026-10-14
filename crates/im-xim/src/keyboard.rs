//! Turning an X11 key event into the front-end neutral [`KeyEvent`].
//!
//! XIM hands us a keycode and a modifier state; every other front end speaks in
//! characters. This is the part of `XLookupString` that an input method needs,
//! written out rather than linked, because the only thing we want from a
//! keystroke is *which character the user meant*.
//!
//! Multi-group layouts (`Mode_switch`) are handled; dead keys, Compose and
//! input-method-specific XKB state are not. A pinyin scheme is typed on a
//! Latin layout, which is what this covers.

use anyhow::{Context as _, Result};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::ConnectionExt;

use im_core::{Key, KeyEvent, Mods, Named};

/// Modifier state bits, named the way `X.h` names them.
mod state {
    /// `ShiftMask`.
    pub const SHIFT: u16 = 1 << 0;
    /// `LockMask` (caps lock).
    pub const LOCK: u16 = 1 << 1;
    /// `ControlMask`.
    pub const CONTROL: u16 = 1 << 2;
    /// `Mod1Mask`, which is `Alt` in every common layout.
    pub const MOD1: u16 = 1 << 3;
    /// `Mod4Mask`, which is `Super`.
    pub const MOD4: u16 = 1 << 6;
    /// `Mod5Mask`, which is `Mode_switch` / `AltGr`.
    pub const MOD5: u16 = 1 << 7;
}

/// Keysyms we care about by name. Values from `X11/keysymdef.h`.
mod keysym {
    pub const TAB: u32 = 0xff09;
    pub const ISO_LEFT_TAB: u32 = 0xfe20;
    pub const RETURN: u32 = 0xff0d;
    pub const KP_ENTER: u32 = 0xff8d;
    pub const BACKSPACE: u32 = 0xff08;
    pub const ESCAPE: u32 = 0xff1b;
    pub const INSERT: u32 = 0xff63;
    pub const DELETE: u32 = 0xffff;
    pub const HOME: u32 = 0xff50;
    pub const LEFT: u32 = 0xff51;
    pub const UP: u32 = 0xff52;
    pub const RIGHT: u32 = 0xff53;
    pub const DOWN: u32 = 0xff54;
    pub const PAGE_UP: u32 = 0xff55;
    pub const PAGE_DOWN: u32 = 0xff56;
    pub const END: u32 = 0xff57;
    pub const F1: u32 = 0xffbe;
    pub const F35: u32 = 0xffe0;
    /// Keysyms for Unicode characters start here.
    pub const UNICODE_BASE: u32 = 0x0100_0000;
}

/// The keyboard mapping, cached until the X server says it changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Keyboard {
    min_keycode: u8,
    keysyms_per_keycode: usize,
    keysyms: Vec<u32>,
}

impl Keyboard {
    /// Read the current keyboard mapping.
    pub fn load<C>(connection: &C) -> Result<Self>
    where
        C: Connection + ConnectionExt,
    {
        let setup = connection.setup();
        // The X server always reports 8..=255, so this fits in the byte the
        // request asks for; saturate rather than trust that on a strange setup.
        let count = setup
            .max_keycode
            .saturating_sub(setup.min_keycode)
            .saturating_add(1);
        let reply = connection
            .get_keyboard_mapping(setup.min_keycode, count)
            .context("asking the X server for the keyboard mapping")?
            .reply()
            .context("reading the keyboard mapping")?;
        Ok(Self {
            min_keycode: setup.min_keycode,
            keysyms_per_keycode: usize::from(reply.keysyms_per_keycode),
            keysyms: reply.keysyms,
        })
    }

    /// Build a mapping directly, for tests.
    #[cfg(test)]
    pub fn from_keysyms(
        min_keycode: u8,
        keysyms_per_keycode: usize,
        keysyms: Vec<u32>,
    ) -> Self {
        Self {
            min_keycode,
            keysyms_per_keycode,
            keysyms,
        }
    }

    /// Translate a keycode plus modifier state.
    ///
    /// `None` for a key that produces no character and has no meaning here —
    /// `Shift`, a dead key, a media key.
    pub fn translate(&self, keycode: u8, state: u16) -> Option<KeyEvent> {
        if self.keysyms_per_keycode == 0 {
            return None;
        }
        let offset = usize::from(keycode.checked_sub(self.min_keycode)?)
            .checked_mul(self.keysyms_per_keycode)?;
        let mut column = 0usize;
        if state & state::SHIFT != 0 {
            column += 1;
        }
        if state & state::MOD5 != 0 {
            column += 2;
        }
        if column >= self.keysyms_per_keycode {
            // A mapping too narrow for the requested column: fall back to the
            // unshifted one rather than reporting no key at all.
            column = 0;
        }
        let keysym = *self.keysyms.get(offset.checked_add(column)?)?;
        if keysym == 0 {
            return None;
        }
        let keysym = if state & state::LOCK != 0 {
            swap_case(keysym)
        } else {
            keysym
        };
        let key = keysym_to_key(keysym)?;

        let mut mods = Mods::NONE;
        if state & state::SHIFT != 0 {
            mods = mods.union(Mods::SHIFT);
        }
        if state & state::CONTROL != 0 {
            mods = mods.union(Mods::CTRL);
        }
        if state & state::MOD1 != 0 {
            mods = mods.union(Mods::ALT);
        }
        if state & state::MOD4 != 0 {
            mods = mods.union(Mods::SUPER);
        }
        Some(KeyEvent { key, mods })
    }
}

/// Caps Lock turns letters into their other case and leaves everything else
/// alone; this is what `XLookupString` does.
fn swap_case(keysym: u32) -> u32 {
    const CASE_SHIFT: u32 = 0x20;
    let base = if keysym >= keysym::UNICODE_BASE {
        keysym::UNICODE_BASE
    } else {
        0
    };
    match keysym - base {
        0x61..=0x7a => keysym - CASE_SHIFT, // a-z to A-Z
        0x41..=0x5a => keysym + CASE_SHIFT, // A-Z to a-z
        _ => keysym,
    }
}

/// Map a keysym to the key the engine understands.
fn keysym_to_key(keysym: u32) -> Option<Key> {
    use keysym::*;
    Some(match keysym {
        BACKSPACE => Key::Named(Named::Backspace),
        TAB | ISO_LEFT_TAB => Key::Named(Named::Tab),
        RETURN | KP_ENTER => Key::Named(Named::Enter),
        ESCAPE => Key::Named(Named::Escape),
        INSERT => Key::Named(Named::Insert),
        DELETE => Key::Named(Named::Delete),
        HOME => Key::Named(Named::Home),
        LEFT => Key::Named(Named::Left),
        UP => Key::Named(Named::Up),
        RIGHT => Key::Named(Named::Right),
        DOWN => Key::Named(Named::Down),
        PAGE_UP => Key::Named(Named::PageUp),
        PAGE_DOWN => Key::Named(Named::PageDown),
        END => Key::Named(Named::End),
        F1..=F35 => Key::Named(Named::F((keysym - F1 + 1) as u8)),
        // The space bar is a key with a name, not a character: `commit =
        // ["space"]` is how a configuration file spells it.
        0x20 => Key::Named(Named::Space),
        _ if keysym >= UNICODE_BASE => {
            char::from_u32(keysym - UNICODE_BASE).map(Key::Char)?
        }
        // Printable Latin-1, which is where a Latin layout's punctuation lives.
        0x21..=0x7e => Key::Char(char::from_u32(keysym)?),
        0xa0..=0xff => char::from_u32(keysym).map(Key::Char)?,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN: u8 = 8;

    /// A three-key board: `a`, `1`, and the Tab key.
    fn keyboard() -> Keyboard {
        let mut keysyms = vec![0u32; 3 * 4];
        keysyms[0] = 0x61; // a
        keysyms[1] = 0x41; // A
        keysyms[4] = 0x31; // 1
        keysyms[5] = 0x21; // !
        keysyms[8] = keysym::TAB;
        Keyboard::from_keysyms(MIN, 4, keysyms)
    }

    #[test]
    fn reads_the_unshifted_key() {
        let event = keyboard()
            .translate(MIN, 0)
            .unwrap_or_else(|| panic!("no event"));
        assert_eq!(event.key, Key::Char('a'));
        assert_eq!(event.mods, Mods::NONE);
    }

    #[test]
    fn reads_the_shifted_key_and_reports_shift() {
        let event = keyboard()
            .translate(MIN, state::SHIFT)
            .unwrap_or_else(|| panic!("no event"));
        assert_eq!(event.key, Key::Char('A'));
        assert!(event.mods.shift());
    }

    #[test]
    fn caps_lock_swaps_the_case() {
        let event = keyboard()
            .translate(MIN, state::LOCK)
            .unwrap_or_else(|| panic!("no event"));
        assert_eq!(event.key, Key::Char('A'));
        // Caps Lock is not a modifier the keymap cares about.
        assert_eq!(event.mods, Mods::NONE);
    }

    #[test]
    fn control_is_reported_but_does_not_change_the_key() {
        let event = keyboard()
            .translate(MIN, state::CONTROL)
            .unwrap_or_else(|| panic!("no event"));
        assert_eq!(event.key, Key::Char('a'));
        assert!(event.mods.ctrl());
    }

    #[test]
    fn named_keys_come_through() {
        let event = keyboard()
            .translate(MIN + 2, 0)
            .unwrap_or_else(|| panic!("no event"));
        assert_eq!(event.key, Key::Named(Named::Tab));
    }

    #[test]
    fn shift_tab_from_a_real_layout_is_still_tab() {
        let mut keysyms = vec![0u32; 4];
        keysyms[0] = keysym::TAB;
        keysyms[1] = keysym::ISO_LEFT_TAB;
        let keyboard = Keyboard::from_keysyms(MIN, 4, keysyms);
        let event = keyboard
            .translate(MIN, state::SHIFT)
            .unwrap_or_else(|| panic!("no event"));
        assert_eq!(event.key, Key::Named(Named::Tab));
        assert!(event.mods.shift());
    }

    #[test]
    fn unknown_keysyms_and_out_of_range_keycodes_are_ignored() {
        let keyboard = keyboard();
        assert!(keyboard.translate(MIN + 9, 0).is_none());
        assert!(keyboard.translate(MIN - 1, 0).is_none());
        let empty = Keyboard::from_keysyms(0, 0, Vec::new());
        assert!(empty.translate(10, 0).is_none());
    }

    #[test]
    fn unicode_keysyms_map_to_characters() {
        assert_eq!(
            keysym_to_key(keysym::UNICODE_BASE + 0x4e2d),
            Some(Key::Char('中'))
        );
        // A code point that is not a character is not a key.
        assert_eq!(keysym_to_key(keysym::UNICODE_BASE + 0xd800), None);
    }

    #[test]
    fn arrow_keys_and_function_keys() {
        assert_eq!(keysym_to_key(keysym::DOWN), Some(Key::Named(Named::Down)));
        assert_eq!(keysym_to_key(keysym::F1), Some(Key::Named(Named::F(1))));
        assert_eq!(keysym_to_key(keysym::F35), Some(Key::Named(Named::F(35))));
        assert_eq!(keysym_to_key(keysym::F35 + 1), None);
    }

    #[test]
    fn a_short_mapping_does_not_read_past_the_end() {
        // One key with a single keysym, asked for the shifted column.
        let keyboard = Keyboard::from_keysyms(MIN, 1, vec![0x61]);
        let event = keyboard
            .translate(MIN, state::SHIFT)
            .unwrap_or_else(|| panic!("no event"));
        assert_eq!(event.key, Key::Char('a'), "falls back to column 0");
    }
}
