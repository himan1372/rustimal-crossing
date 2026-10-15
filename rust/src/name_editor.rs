//! Name editor, character entry — Layer A: pure data and helpers.
//!
//! Verified against `src/game/m_editor_ovl.c`, `src/game/m_ledit_ovl.c`,
//! `include/m_editor_ovl.h`, `include/m_ledit_ovl.h`, `include/m_font.h`
//! (GAFE01_00 Rev 0 USA, via public decomp).
//!
//! Scope: the buffer-clear helper and the all-space check as pure
//! functions, plus the entry-type and keyboard constants. The keyboard
//! tables, the editor state machine, overlay lifecycle, and
//! resource-string loading stay in C.
//!
//! NOTE (2026-10-15, Philip's call): the NG-word filter is intentionally
//! absent. The `ng` module and `pc_ed_ng_check` were removed — no name
//! is rejected. The C-side `mED_ng_word_check` in `m_editor_ovl.c` must
//! also be disabled/removed on the Windows side for the behavior to
//! take effect in-game.

/// Text buffer model (`m_editor_ovl.c`).
///
/// The editor uses a fixed-capacity, space-filled byte buffer — NOT a
/// NUL-terminated string. Three values matter: buffer capacity
/// (`input_length`), logical text length (`now_str_len`), and cursor
/// position (`cursor_idx`). Do not collapse them into a Rust `String`.
pub mod buffer {
    /// `CHAR_SPACE` (`include/m_font.h`).
    pub const SPACE: u8 = 32;

    /// `mED_clear_input_data` core: fill `input_length` bytes with spaces.
    /// (The C version also zeroes `now_str_len`, `cursor_idx`, `_22`, `_26`
    /// on the editor struct; the caller owns those fields.)
    pub fn clear(buf: &mut [u8]) {
        buf.fill(SPACE);
    }

    /// `mED_all_space_check` core: true if the first `len` bytes are all
    /// spaces. The C caller clears the buffer and rejects the entry when
    /// this is true (with a catchphrase special-case on the data2 buffer
    /// that stays in C).
    pub fn is_all_spaces(input: &[u8], len: usize) -> bool {
        let len = len.min(input.len());
        input[..len].iter().all(|&b| b == SPACE)
    }
}

/// Single-line entry types (`m_ledit_ovl.h:12-18`, exact order).
pub mod entry_type {
    pub const PLAYER_NAME: u8 = 0;
    pub const COUNTRY_NAME: u8 = 1; // town name
    pub const EPHRASE: u8 = 2; // catchphrase
    pub const RESET: u8 = 3;
    pub const REQUEST: u8 = 4; // song request
    pub const MYORIGINAL_NAME: u8 = 5;
    pub const ISLAND_NAME: u8 = 6;
    pub const NUM: u8 = 7;
}

/// Keyboard geometry and modes (`m_editor_ovl.h`).
pub mod keyboard {
    pub const COLUMNS: usize = 10;
    pub const ROWS: usize = 4;
    /// Positions per character table.
    pub const POSITIONS: usize = COLUMNS * ROWS; // 40

    pub const SHIFT_LOWER: u8 = 0;
    pub const SHIFT_UPPER: u8 = 1;

    pub const ARRANGE_QWERTY: u8 = 0;
    pub const ARRANGE_ALPHA: u8 = 1;

    pub const INPUT_MODE_LETTER: u8 = 0;
    pub const INPUT_MODE_SIGN: u8 = 1;
    pub const INPUT_MODE_MARK: u8 = 2;
}

// ---- C ABI ----
// All pc_-prefixed: the C originals stay compiled; these are staged kernels.


/// Fill `len` bytes with `CHAR_SPACE` (`mED_clear_input_data` buffer part).
#[no_mangle]
pub extern "C" fn pc_ed_clear_buffer(buf: *mut u8, len: usize) {
    if !buf.is_null() {
        let b = unsafe { std::slice::from_raw_parts_mut(buf, len) };
        buffer::clear(b);
    }
}

/// 1 if the first `len` bytes are all spaces, else 0 (`mED_all_space_check` core).
#[no_mangle]
pub extern "C" fn pc_ed_is_all_spaces(input: *const u8, len: usize) -> i32 {
    if input.is_null() {
        return 1;
    }
    let b = unsafe { std::slice::from_raw_parts(input, len) };
    buffer::is_all_spaces(b, len) as i32
}

#[cfg(test)]
mod tests {
    use super::*;


    #[test]
    fn buffer_clear_and_all_spaces() {
        let mut buf = *b"hello world!";
        buffer::clear(&mut buf);
        assert!(buf.iter().all(|&b| b == buffer::SPACE));
        assert!(buffer::is_all_spaces(&buf, buf.len()));
        assert!(buffer::is_all_spaces(b"   ", 3));
        assert!(!buffer::is_all_spaces(b"  x", 3));
        assert!(buffer::is_all_spaces(b"", 0));
    }

    #[test]
    fn entry_and_keyboard_consts() {
        assert_eq!(entry_type::PLAYER_NAME, 0);
        assert_eq!(entry_type::ISLAND_NAME, 6);
        assert_eq!(keyboard::POSITIONS, 40);
        assert_eq!(buffer::SPACE, 32);
    }


    #[test]
    fn c_abi_buffer_helpers() {
        let mut buf = *b"abcdef";
        pc_ed_clear_buffer(buf.as_mut_ptr(), buf.len());
        assert!(buf.iter().all(|&b| b == 32));
        assert_eq!(pc_ed_is_all_spaces(buf.as_ptr(), buf.len()), 1);
        assert_eq!(pc_ed_is_all_spaces(b"ab".as_ptr(), 2), 0);
    }
}
