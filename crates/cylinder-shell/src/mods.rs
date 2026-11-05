//! Seat-local modifier state for the Wayland panel path.
//!
//! Manifold's `KeyEvent.modifiers` is a **global** xkb mask. Flatland's host-mirror path
//! (key_sink → uinput) lets the host compositor derive Shift from real key down/up, so a
//! stale Shift bit in that mask does not uppercase letters. Cylinder-shell forwards keys
//! over `PanelItem::key` → `wl_keyboard.modifiers`; clients (ghostty) trust the mask. If
//! Shift/Caps is set in manifold's mask for keys this seat never saw, every character
//! looks shifted.
//!
//! Fix: derive `depressed` / `locked` from modifier keycodes this seat has actually
//! received (same bit map as host-mirror `modifier_bit`), and only take `layout_group`
//! from the remote event.

use stardust_xr_panels::panel_item::ModifierState;

/// Linux/evdev keycodes (Wayland wl_keyboard / Stardust KeyEvent).
const KEY_LEFTSHIFT: u32 = 42;
const KEY_RIGHTSHIFT: u32 = 54;
const KEY_LEFTCTRL: u32 = 29;
const KEY_RIGHTCTRL: u32 = 97;
const KEY_LEFTALT: u32 = 56;
const KEY_LEFTMETA: u32 = 125;
const KEY_RIGHTMETA: u32 = 126;
const KEY_CAPSLOCK: u32 = 58;

const MOD_SHIFT: u32 = 1 << 0;
const MOD_LOCK: u32 = 1 << 1;
const MOD_CTRL: u32 = 1 << 2;
const MOD_ALT: u32 = 1 << 3;
const MOD_LOGO: u32 = 1 << 6;

/// Tracks modifier keys observed on this panel's KeyboardHandler.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ModTracker {
    shift: bool,
    ctrl: bool,
    alt: bool,
    logo: bool,
    /// Caps Lock toggle (locked mask bit 1).
    caps: bool,
}

impl ModTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Apply one key event and return the seat-local `ModifierState`.
    /// `layout_group` is taken from the remote event (layout is not key-local).
    pub fn apply(&mut self, keycode: u32, pressed: bool, layout_group: u32) -> ModifierState {
        match keycode {
            KEY_LEFTSHIFT | KEY_RIGHTSHIFT => self.shift = pressed,
            KEY_LEFTCTRL | KEY_RIGHTCTRL => self.ctrl = pressed,
            KEY_LEFTALT => self.alt = pressed,
            KEY_LEFTMETA | KEY_RIGHTMETA => self.logo = pressed,
            KEY_CAPSLOCK if pressed => self.caps = !self.caps,
            _ => {}
        }
        self.to_modifier_state(layout_group)
    }

    pub fn to_modifier_state(&self, layout_group: u32) -> ModifierState {
        let mut depressed = 0;
        if self.shift {
            depressed |= MOD_SHIFT;
        }
        if self.ctrl {
            depressed |= MOD_CTRL;
        }
        if self.alt {
            depressed |= MOD_ALT;
        }
        if self.logo {
            depressed |= MOD_LOGO;
        }
        let locked = if self.caps { MOD_LOCK } else { 0 };
        ModifierState {
            depressed,
            latched: 0,
            locked,
            layout_group,
        }
    }
}

/// Map an evdev modifier keycode to its standard xkb mod bit (host-mirror-compatible).
pub fn modifier_bit(keycode: u32) -> Option<u32> {
    match keycode {
        KEY_LEFTSHIFT | KEY_RIGHTSHIFT => Some(MOD_SHIFT),
        KEY_LEFTCTRL | KEY_RIGHTCTRL => Some(MOD_CTRL),
        KEY_LEFTALT => Some(MOD_ALT),
        KEY_LEFTMETA | KEY_RIGHTMETA => Some(MOD_LOGO),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letter_alone_has_no_shift() {
        let mut t = ModTracker::new();
        let m = t.apply(30, true, 0); // KEY_A
        assert_eq!(m.depressed, 0);
        assert_eq!(m.locked, 0);
        let m = t.apply(30, false, 0);
        assert_eq!(m.depressed, 0);
    }

    #[test]
    fn shift_down_then_letter_then_shift_up() {
        let mut t = ModTracker::new();
        let m = t.apply(KEY_LEFTSHIFT, true, 0);
        assert_eq!(m.depressed, MOD_SHIFT);
        let m = t.apply(30, true, 0);
        assert_eq!(m.depressed, MOD_SHIFT, "letter must not clear Shift");
        let m = t.apply(30, false, 0);
        assert_eq!(m.depressed, MOD_SHIFT);
        let m = t.apply(KEY_LEFTSHIFT, false, 0);
        assert_eq!(m.depressed, 0, "Shift up must clear depressed");
    }

    #[test]
    fn remote_shift_mask_ignored_until_shift_key_seen() {
        // Simulates the stuck-Shift bug: remote depressed has Shift, but this seat
        // never received KEY_LEFTSHIFT. Local tracker must stay clear.
        let mut t = ModTracker::new();
        let remote_depressed = MOD_SHIFT;
        let local = t.apply(30, true, 0);
        assert_eq!(local.depressed, 0);
        assert_ne!(local.depressed, remote_depressed);
    }

    #[test]
    fn caps_toggles_locked_not_depressed() {
        let mut t = ModTracker::new();
        let m = t.apply(KEY_CAPSLOCK, true, 0);
        assert_eq!(m.depressed, 0);
        assert_eq!(m.locked, MOD_LOCK);
        let m = t.apply(KEY_CAPSLOCK, false, 0); // release does not toggle again
        assert_eq!(m.locked, MOD_LOCK);
        let m = t.apply(KEY_CAPSLOCK, true, 0);
        assert_eq!(m.locked, 0);
    }

    #[test]
    fn layout_group_passthrough() {
        let mut t = ModTracker::new();
        let m = t.apply(30, true, 2);
        assert_eq!(m.layout_group, 2);
    }

    #[test]
    fn modifier_bit_matches_host_mirror() {
        assert_eq!(modifier_bit(42), Some(MOD_SHIFT));
        assert_eq!(modifier_bit(54), Some(MOD_SHIFT));
        assert_eq!(modifier_bit(29), Some(MOD_CTRL));
        assert_eq!(modifier_bit(125), Some(MOD_LOGO));
        assert_eq!(modifier_bit(30), None);
    }
}
