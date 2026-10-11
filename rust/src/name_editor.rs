//! Name editor, character entry, and NG-word filter — Layer A: pure data and helpers.
//!
//! Verified against `src/game/m_editor_ovl.c`, `src/game/m_ledit_ovl.c`,
//! `include/m_editor_ovl.h`, `include/m_ledit_ovl.h`, `include/m_font.h`
//! (GAFE01_00 Rev 0 USA, via public decomp).
//!
//! Scope: the NG-word matching kernel, the buffer-clear helper, and the
//! all-space check as pure functions, plus the entry-type and keyboard
//! constants. The keyboard tables, the editor state machine, overlay
//! lifecycle, and resource-string loading stay in C.
//!
//! Pattern *contents* are game assets (resource IDs 0x74A..0x757) — they are
//! NOT hardcoded here. C loads them via `mString_Load_StringFromRom` and
//! passes the bytes in. Only the pattern *lengths* are in the C source.

/// NG-word filter constants (`m_editor_ovl.h:15-16`, `m_editor_ovl.c:1089`).
pub mod ng {
    /// First resource ID of the 14 patterns.
    pub const WORD_START: u16 = 0x74A;
    /// Number of patterns.
    pub const WORD_NUM: usize = 14;
    /// Hard-coded pattern lengths (`ng_word_length` table). The C source
    /// provides these separately from the pattern contents.
    pub const WORD_LENGTHS: [usize; WORD_NUM] =
        [4, 4, 4, 4, 4, 5, 5, 7, 7, 7, 7, 7, 9, 10];
    /// C's `ng_word[10]` buffer size; longest pattern is 10.
    pub const MAX_PATTERN_LEN: usize = 10;

    /// Pure NG-word substring scan (`mED_ng_word_check` core).
    ///
    /// Checks each of the 14 patterns against every start offset in
    /// `input[0..input_length]`. This is a substring scan — no word
    /// boundaries required — over the full configured capacity, NOT the
    /// logical text length. Comparison is raw bytes; no case folding or
    /// normalization (the C code does none).
    ///
    /// Returns `Some(i)` for the first matching pattern (lowest index),
    /// `None` if nothing matches. The C version iterates offsets high→low
    /// within a pattern; that order does not affect which pattern is
    /// reported, only which offset — so the boolean/index result matches.
    pub fn check(
        input: &[u8],
        input_length: usize,
        patterns: [&[u8]; WORD_NUM],
    ) -> Option<usize> {
        let input_length = input_length.min(input.len());
        for (i, pattern) in patterns.iter().enumerate() {
            let len = WORD_LENGTHS[i];
            if len == 0 || len > input_length {
                continue;
            }
            let pat = &pattern[..len.min(pattern.len())];
            // Valid start offsets: 0..=(input_length - len).
            for start in 0..=(input_length - len) {
                if input[start..start + len] == *pat {
                    return Some(i);
                }
            }
        }
        None
    }
}

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

/// NG-word check. `patterns` points to `14 * 10` bytes (pattern `i` at
/// offset `i*10`, first `WORD_LENGTHS[i]` bytes valid — as loaded by
/// `mString_Load_StringFromRom`). Returns the matching pattern index,
/// or -1 if no match (or on null input).
#[no_mangle]
pub extern "C" fn pc_ed_ng_check(
    input: *const u8,
    input_length: usize,
    patterns: *const u8,
) -> i32 {
    if input.is_null() || patterns.is_null() {
        return -1;
    }
    let input = unsafe { std::slice::from_raw_parts(input, input_length) };
    let pat_buf = unsafe { std::slice::from_raw_parts(patterns, ng::WORD_NUM * ng::MAX_PATTERN_LEN) };
    let mut pats: [&[u8]; ng::WORD_NUM] = [&[]; ng::WORD_NUM];
    for i in 0..ng::WORD_NUM {
        let len = ng::WORD_LENGTHS[i];
        pats[i] = &pat_buf[i * ng::MAX_PATTERN_LEN..i * ng::MAX_PATTERN_LEN + len];
    }
    ng::check(input, input_length, pats)
        .map(|i| i as i32)
        .unwrap_or(-1)
}

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

    fn test_patterns() -> [&'static [u8]; ng::WORD_NUM] {
        // Stand-in patterns with the exact C lengths; contents are arbitrary
        // (real ones come from game assets).
        [
            b"abcd", b"efgh", b"ijkl", b"mnop", b"qrst",
            b"uvwxy", b"z0123",
            b"4567890", b"ABCDEFG", b"HIJKLMN", b"OPQRSTU", b"VWXYZab",
            b"cdefghijk", b"lmnopqrstu",
        ]
    }

    #[test]
    fn ng_word_lengths_match_source() {
        assert_eq!(
            ng::WORD_LENGTHS,
            [4, 4, 4, 4, 4, 5, 5, 7, 7, 7, 7, 7, 9, 10]
        );
        assert_eq!(ng::WORD_START, 0x74A);
        assert_eq!(ng::WORD_NUM, 14);
    }

    #[test]
    fn ng_no_match_leaves_buffer() {
        let input = b"hello world         ";
        assert_eq!(ng::check(input, input.len(), test_patterns()), None);
    }

    #[test]
    fn ng_match_at_offset_zero() {
        let input = b"abcd rest of buffer ";
        assert_eq!(ng::check(input, input.len(), test_patterns()), Some(0));
    }

    #[test]
    fn ng_match_at_final_offset() {
        // Pattern at the last valid start: input_length - len.
        let mut input = [b' '; 20];
        input[16..20].copy_from_slice(b"abcd"); // len 4 at offset 16 = 20-4
        assert_eq!(ng::check(&input, 20, test_patterns()), Some(0));
    }

    #[test]
    fn ng_match_inside_longer_string() {
        let input = b"xxabcdefg0123       ";
        // "abcd" is a substring starting at offset 2.
        assert_eq!(ng::check(input, input.len(), test_patterns()), Some(0));
    }

    #[test]
    fn ng_scans_capacity_not_logical_len() {
        // Pattern sits past the "logical" text end but inside capacity.
        // check() takes input_length explicitly; trailing spaces don't hide it.
        let mut input = [b' '; 16];
        input[0..2].copy_from_slice(b"hi");
        input[12..16].copy_from_slice(b"abcd");
        assert_eq!(ng::check(&input, 16, test_patterns()), Some(0));
        // A shorter capacity scan would miss it.
        assert_eq!(ng::check(&input, 12, test_patterns()), None);
    }

    #[test]
    fn ng_first_pattern_wins() {
        // "abcd" (pattern 0) contains nothing overlapping here; craft input
        // matching both pattern 0 and pattern 5: "abcd" at 0, "uvwxy" at 4.
        let input = b"abcduvwxy           ";
        assert_eq!(ng::check(input, input.len(), test_patterns()), Some(0));
    }

    #[test]
    fn ng_each_length() {
        // One test per pattern length class.
        let pats = test_patterns();
        for (i, len) in ng::WORD_LENGTHS.iter().enumerate() {
            let mut input = [b' '; 24];
            input[..*len].copy_from_slice(&pats[i][..*len]);
            assert_eq!(ng::check(&input, 24, pats), Some(i), "pattern {}", i);
        }
    }

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
    fn c_abi_ng_check() {
        let pats = test_patterns();
        // Flatten to 14x10 buffer as C would lay it out.
        let mut flat = [0u8; 14 * 10];
        for (i, p) in pats.iter().enumerate() {
            flat[i * 10..i * 10 + p.len()].copy_from_slice(p);
        }
        let input = b"xxabcd              ";
        assert_eq!(
            pc_ed_ng_check(input.as_ptr(), input.len(), flat.as_ptr()),
            0
        );
        let clean = b"hello               ";
        assert_eq!(
            pc_ed_ng_check(clean.as_ptr(), clean.len(), flat.as_ptr()),
            -1
        );
        assert_eq!(pc_ed_ng_check(std::ptr::null(), 10, flat.as_ptr()), -1);
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
