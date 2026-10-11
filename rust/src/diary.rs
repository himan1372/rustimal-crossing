//! Player diary (m_diary.c, m_diary_ovl.c) pure kernels.
//!
//! Verified against GAFE01_00 Rev 0 decomp (`src/game/m_diary.c`,
//! `src/game/m_diary_ovl.c`, `include/m_diary.h`, `include/m_card.h`).
//! Only deterministic, engine-free helpers are ported here: the
//! fixed-size entry representation, the backward text-length scan,
//! space-filled initialization, entry indexing, the font-width line
//! layout scan, the line-splitting state machine, scroll limits, the
//! read-mode input-repeat timing, the month frame offsets, and the
//! edit-confirmation branch.
//!
//! The following stay in C until Rust/C equivalence is proven:
//! ARAM load/save (`mCD_save_data_aram_to_main` /
//! `mCD_save_data_main_to_aram`), memory-card serialization and
//! checksums (`mFRm_GetFlatCheckSum`), `mFont_GetCodeWidth`, the
//! shared editor and edit-end-check overlays, `chase_f()` scroll
//! easing, transitions, sounds, graphics/display lists, the static
//! overlay object and its allocation, the calendar's open call, and
//! controller polling.

#![allow(non_snake_case)]

use std::os::raw::c_int;

/// Diary entry text size (`mDI_ENTRY_SIZE`).
pub const DIARY_ENTRY_SIZE: i32 = 992;
/// Player slots (`mCD_KEEP_DIARY_COUNT` = PLAYER_NUM).
pub const DIARY_PLAYERS: i32 = 4;
/// Monthly slots per player (`mCD_KEEP_DIARY_ENTRY_COUNT` =
/// `lbRTC_MONTHS_MAX`).
pub const DIARY_MONTHS: i32 = 12;
/// Total entries (4 players x 12 months).
pub const DIARY_ENTRY_COUNT: i32 = 48;
/// Text storage total (48 x 992).
pub const DIARY_TEXT_BYTES: i32 = 47616;
/// Checksum field size at the start of `mCD_keep_diary_c`.
pub const DIARY_CHECKSUM_SIZE: i32 = 2;
/// Aligned diary structure size (`mCD_KEEP_DIARY_SIZE` =
/// `ALIGN_NEXT(47618, 32)`).
pub const DIARY_STRUCT_SIZE: i32 = 47648;
/// Game space character (`CHAR_SPACE`).
pub const DIARY_CHAR_SPACE: u8 = 0x20;
/// Explicit newline character (`CHAR_NEW_LINE`).
pub const DIARY_CHAR_NEW_LINE: u8 = 205;
/// Line-wrap width (`mDI_get_col_line_width` / `mDI_strLineCheck`).
pub const DIARY_WRAP_WIDTH: i32 = 192;
/// Visible reading window in lines (six-line window).
pub const DIARY_VISIBLE_LINES: i32 = 6;
/// Rendered line height.
pub const DIARY_LINE_HEIGHT: f32 = 16.0;

/// Month frame horizontal adjustments (`month_tex_adjust`,
/// `m_diary_ovl.c`), indexed by 0-based month (0 = January).
/// The diary opens with `data3 = calendar month % 12`, so the
/// table is 0-based, not 1-based `lbRTC_JANUARY`.
pub const DIARY_MONTH_ADJUST: [f32; DIARY_MONTHS as usize] = [
    -26.0, -16.0, -40.0, -52.0, -57.0, -52.0, -57.0, -32.0, 0.0, -23.0, -6.0, -6.0,
];

/// Input-repeat initial delay (`mDI_Set_command_read`).
pub const DIARY_REPEAT_DELAY: i32 = 30;
/// Input-repeat count (`mDI_Set_command_read`).
pub const DIARY_REPEAT_COUNT: i32 = 26;
/// Fast-repeat speed marker (`_30 = 3` when `_11 == 0`).
pub const DIARY_REPEAT_FAST: i32 = 3;
/// Slow-repeat speed marker.
pub const DIARY_REPEAT_SLOW: i32 = 2;

/// Read-mode command codes from `mDI_Set_command_read`.
pub const DIARY_CMD_UP: i32 = 2;
pub const DIARY_CMD_DOWN: i32 = 3;
pub const DIARY_CMD_UP10: i32 = 8;
pub const DIARY_CMD_DOWN10: i32 = 9;

/// Confirmation result codes from `mDI_move_Obey`.
pub const DIARY_CONFIRM_WRITE: i32 = 0;
pub const DIARY_CONFIRM_REEDIT: i32 = 1;

/// `mDi_strlen` (`m_diary.c`): scans backward from `max_len - 1`
/// and returns the one-based position of the first byte that
/// differs from `end_char`; returns 0 when every byte equals
/// `end_char`. Does not stop at zero bytes and does not search
/// for a terminator. For diary entries the caller passes
/// `CHAR_SPACE` and `mDI_ENTRY_SIZE`.
#[no_mangle]
pub extern "C" fn pc_di_strlen(str_: *const u8, max_len: c_int, end_char: u8) -> c_int {
    if str_.is_null() || max_len <= 0 {
        return 0;
    }
    let bytes = unsafe { std::slice::from_raw_parts(str_, max_len as usize) };
    let mut i = max_len;
    while i != 0 {
        if bytes[(i - 1) as usize] != end_char {
            return i;
        }
        i -= 1;
    }
    0
}

/// `mDi_init_diary` / `mDi_clear_diary` (`m_diary.c`): fills the
/// full 992-byte entry with `CHAR_SPACE`. The source zeroes the
/// buffer first and then overwrites it with spaces, so the final
/// initialized entry is space-filled, not zero-filled.
#[no_mangle]
pub extern "C" fn pc_di_init_entry(entry: *mut u8) {
    if !entry.is_null() {
        unsafe {
            std::ptr::write_bytes(entry, DIARY_CHAR_SPACE, DIARY_ENTRY_SIZE as usize);
        }
    }
}

/// Byte offset of `entries[player][month]` within the diary
/// structure, including the leading 2-byte checksum:
/// `2 + (player * 12 + month) * 992`. The overlay selects
/// `data->entries[menu_info->data1] + menu_info->data3`, i.e.
/// `data1` is the player slot and `data3` the month slot.
/// Returns -1 for out-of-range indices.
#[no_mangle]
pub extern "C" fn pc_di_entry_offset(player: c_int, month: c_int) -> c_int {
    if player < 0 || player >= DIARY_PLAYERS || month < 0 || month >= DIARY_MONTHS {
        return -1;
    }
    DIARY_CHECKSUM_SIZE + (player * DIARY_MONTHS + month) * DIARY_ENTRY_SIZE
}

/// `mDI_get_col_line_width` (`m_diary_ovl.c:27`): scans the first
/// `len` bytes, accumulating per-byte font widths supplied in
/// `widths` (the injected `mFont_GetCodeWidth(*str_p, TRUE)`
/// results). `text` is still consulted for `CHAR_NEW_LINE`
/// (205). A line wraps when the accumulated width is strictly
/// greater than 192 (`width > 192`, not `>=`); a newline is
/// consumed as a line break and tracked separately. `out`
/// receives `[last_line_width, line_count, last_newline_width,
/// newline_count]`. When 31 lines/newlines are reached the
/// counts clamp to 30 with the source quirks
/// (`cur_line_count = line_width + 1`, `cur_newline_width =
/// `newline_width + 1`).
#[no_mangle]
pub extern "C" fn pc_di_wrap_scan(
    text: *const u8,
    widths: *const c_int,
    len: c_int,
    out: *mut c_int,
) {
    if text.is_null() || widths.is_null() || len <= 0 {
        return;
    }
    let text = unsafe { std::slice::from_raw_parts(text, len as usize) };
    let widths = unsafe { std::slice::from_raw_parts(widths, len as usize) };

    let mut width: i32 = 0;
    let mut lines: i32 = 0;
    let mut line_width: i32 = 0;
    let mut cur_line_count: i32 = 0;
    let mut newlines: i32 = 0;
    let mut newline_width: i32 = 0;
    let mut cur_newline_width: i32 = 0;

    for i in 0..len as usize {
        width += widths[i];
        if width > DIARY_WRAP_WIDTH || text[i] == DIARY_CHAR_NEW_LINE {
            lines += 1;
            line_width = cur_line_count;
            cur_line_count = 0;
            width = 0;
            if text[i] == DIARY_CHAR_NEW_LINE {
                newlines += 1;
                newline_width = cur_newline_width;
                cur_newline_width = 0;
            }
        } else {
            cur_newline_width += 1;
            cur_line_count += 1;
        }
    }

    if lines >= 31 {
        cur_line_count = line_width + 1;
        lines = 30;
    }
    if newlines >= 31 {
        cur_newline_width = newline_width + 1;
        newlines = 30;
    }

    if !out.is_null() {
        unsafe {
            *out.add(0) = cur_line_count;
            *out.add(1) = lines;
            *out.add(2) = cur_newline_width;
            *out.add(3) = newlines;
        }
    }
}

/// `mDI_strLineCheck` (`m_diary_ovl.c`): the line-splitting state
/// machine used when rendering. `pos` is the in/out scan index
/// into `text` (length `len`), `width` the in/out accumulated
/// line width, `line_len` the in/out rendered-line length, and
/// `widths` the injected per-byte font widths.
/// Returns 3 when the scan reaches the end, 1 on an explicit
/// newline (pointer advanced), 2 when adding the character would
/// push the width over 192 (pointer NOT advanced, so the next
/// line begins with the overflowing character), and 0 when the
/// character is consumed.
#[no_mangle]
pub extern "C" fn pc_di_line_check(
    text: *const u8,
    widths: *const c_int,
    len: c_int,
    pos: *mut c_int,
    width: *mut c_int,
    line_len: *mut c_int,
) -> c_int {
    if text.is_null() || widths.is_null() || pos.is_null() || width.is_null() || line_len.is_null() {
        return 3;
    }
    let (p, w, ll) = unsafe { (*pos, *width, *line_len) };
    if p >= len {
        return 3;
    }
    let text = unsafe { std::slice::from_raw_parts(text, len as usize) };
    let widths = unsafe { std::slice::from_raw_parts(widths, len as usize) };
    let ch = text[p as usize];
    if ch == DIARY_CHAR_NEW_LINE {
        unsafe {
            *pos = p + 1;
            *line_len = ll + 1;
        }
        1
    } else {
        let new_w = w + widths[p as usize];
        if new_w > DIARY_WRAP_WIDTH {
            unsafe {
                *width = new_w;
            }
            2
        } else {
            unsafe {
                *width = new_w;
                *pos = p + 1;
                *line_len = ll + 1;
            }
            0
        }
    }
}

/// One-line scroll step (`mDI_move_cursol_upper` /
/// `mDI_move_cursol_lower`, `m_diary_ovl.c`): `dir` is -1 (up)
/// or +1 (down). Returns the new `_18` scroll position. Up
/// decrements only when above 0; down increments only while
/// below `line_count - 6` (the six-line window).
#[no_mangle]
pub extern "C" fn pc_di_scroll_one(cur: c_int, line_count: c_int, dir: c_int) -> c_int {
    if dir < 0 {
        if cur > 0 {
            cur - 1
        } else {
            cur
        }
    } else if dir > 0 {
        if cur < line_count - DIARY_VISIBLE_LINES {
            cur + 1
        } else {
            cur
        }
    } else {
        cur
    }
}

/// Ten-line scroll step (`mDI_move_cursol_upper10` /
/// `mDI_move_cursol_lower10`, `m_diary_ovl.c`): `dir` is -1 (up)
/// or +1 (down). Returns the new `_18` scroll position, clamped
/// to `[0, line_count - 6]`. Up uses the strict test
/// `_18 - 10 > 0`, else 0.
#[no_mangle]
pub extern "C" fn pc_di_scroll_ten(cur: c_int, line_count: c_int, dir: c_int) -> c_int {
    let limit = line_count - DIARY_VISIBLE_LINES;
    if dir < 0 {
        if cur > 0 {
            if cur - 10 > 0 {
                cur - 10
            } else {
                0
            }
        } else {
            cur
        }
    } else if dir > 0 {
        if cur < limit {
            if cur + 10 < limit {
                cur + 10
            } else {
                limit
            }
        } else {
            cur
        }
    } else {
        cur
    }
}

/// Input-repeat state for [`pc_di_repeat_step`], mirroring the
/// overlay fields `_0C` (as `prev_dir`), `_10`, `_11`, `_12`,
/// `_30`. Zero-initialize before use.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct DiRepeatState {
    /// Previously held direction: -1 (up), +1 (down), 0 (none).
    pub prev_dir: c_int,
    pub f10: c_int,
    pub f11: c_int,
    pub f12: c_int,
    pub f30: c_int,
}

/// Pure step kernel of `mDI_Set_command_read` (`m_diary_ovl.c:167`).
/// `dir` is -1 (up held), +1 (down held), or 0 (released);
/// `busy` is nonzero while a scroll movement is in progress
/// (`_32 != 0`). Updates `state` in place and returns
/// [`DIARY_CMD_UP`] (2), [`DIARY_CMD_DOWN`] (3), or 0.
///
/// A fresh direction sets the initial delay (30) and repeat
/// count (26); while held, the counters decrement and commands
/// fire once the delay expires (`_11 == 0` selects the fast
/// speed marker `_30 = 3`, otherwise `_30 = 2`).
#[no_mangle]
pub extern "C" fn pc_di_repeat_step(state: *mut DiRepeatState, dir: c_int, busy: c_int) -> c_int {
    if state.is_null() {
        return 0;
    }
    let st = unsafe { &mut *state };
    let cmd = if dir < 0 {
        DIARY_CMD_UP
    } else {
        DIARY_CMD_DOWN
    };

    if dir != 0 {
        if dir == st.prev_dir {
            if st.f10 == 1 && st.f11 != 0 {
                st.f11 -= 1;
            }
            if st.f12 != 0 {
                st.f12 -= 1;
            } else if busy == 0 {
                if st.f11 == 0 {
                    st.f30 = DIARY_REPEAT_FAST;
                    return cmd;
                } else {
                    st.f10 = 1;
                    st.f30 = DIARY_REPEAT_SLOW;
                    return cmd;
                }
            }
        } else if busy == 0 {
            st.prev_dir = dir;
            st.f12 = DIARY_REPEAT_DELAY;
            st.f11 = DIARY_REPEAT_COUNT;
            st.f10 = 0;
            st.f30 = 1;
            return cmd;
        }
    } else {
        st.f12 = 0;
        st.f30 = 1;
        st.prev_dir = 0;
    }
    0
}

/// C-up / C-down edge trigger (`mDI_Set_command_read`):
/// returns [`DIARY_CMD_UP10`] (8) when `trig_up` is set,
/// [`DIARY_CMD_DOWN10`] (9) when `trig_down` is set, else 0.
/// No command fires while `busy` (`_32 != 0`).
#[no_mangle]
pub extern "C" fn pc_di_ten_line_cmd(trig_up: c_int, trig_down: c_int, busy: c_int) -> c_int {
    if busy != 0 {
        return 0;
    }
    if trig_up != 0 {
        DIARY_CMD_UP10
    } else if trig_down != 0 {
        DIARY_CMD_DOWN10
    } else {
        0
    }
}

/// Month frame horizontal adjustment (`month_tex_adjust`,
/// `m_diary_ovl.c`): `month` is 0-based (0 = January, matching
/// the calendar's `data3`). Returns 0.0 for out-of-range months.
#[no_mangle]
pub extern "C" fn pc_di_month_adjust(month: c_int) -> f32 {
    if month < 0 || month >= DIARY_MONTHS {
        0.0
    } else {
        DIARY_MONTH_ADJUST[month as usize]
    }
}

/// Edit-confirmation branch (`mDI_move_Obey`, `m_diary_ovl.c`):
/// `data1 == 0` writes the whole `mCD_KEEP_DIARY_SIZE` structure
/// to ARAM, opens the copy-warning overlay, and waits
/// ([`DIARY_CONFIRM_WRITE`]); `data1 == 1` sets `_42`, restores
/// the scroll state, and returns to editing with no ARAM write
/// ([`DIARY_CONFIRM_REEDIT`]).
#[no_mangle]
pub extern "C" fn pc_di_confirm_branch(data1: c_int) -> c_int {
    if data1 == 0 {
        DIARY_CONFIRM_WRITE
    } else {
        DIARY_CONFIRM_REEDIT
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strlen_backward_scan() {
        let mut buf = [DIARY_CHAR_SPACE; DIARY_ENTRY_SIZE as usize];
        assert_eq!(pc_di_strlen(buf.as_ptr(), DIARY_ENTRY_SIZE, DIARY_CHAR_SPACE), 0);
        buf[0] = b'A';
        assert_eq!(pc_di_strlen(buf.as_ptr(), DIARY_ENTRY_SIZE, DIARY_CHAR_SPACE), 1);
        buf[1] = b'B';
        buf[2] = b' ';
        assert_eq!(pc_di_strlen(buf.as_ptr(), DIARY_ENTRY_SIZE, DIARY_CHAR_SPACE), 2);
        // embedded zero does not terminate the scan
        buf[10] = 0;
        buf[11] = b'Z';
        assert_eq!(pc_di_strlen(buf.as_ptr(), DIARY_ENTRY_SIZE, DIARY_CHAR_SPACE), 12);
        // non-space at the last byte
        buf[991] = b'!';
        assert_eq!(pc_di_strlen(buf.as_ptr(), DIARY_ENTRY_SIZE, DIARY_CHAR_SPACE), 992);
    }

    #[test]
    fn init_fills_spaces() {
        let mut buf = [0u8; DIARY_ENTRY_SIZE as usize];
        pc_di_init_entry(buf.as_mut_ptr());
        assert!(buf.iter().all(|&b| b == DIARY_CHAR_SPACE));
    }

    #[test]
    fn entry_offsets() {
        assert_eq!(pc_di_entry_offset(0, 0), 2);
        assert_eq!(pc_di_entry_offset(0, 11), 2 + 11 * 992);
        assert_eq!(pc_di_entry_offset(3, 11), 2 + 47 * 992);
        assert_eq!(2 + 47 * 992, 47618 - 992);
        assert_eq!(pc_di_entry_offset(4, 0), -1);
        assert_eq!(pc_di_entry_offset(0, 12), -1);
    }

    #[test]
    fn wrap_scan_strict_192() {
        // widths: each char 64 wide -> 3 chars = 192 (no wrap), 4th wraps
        let text = [b'a', b'a', b'a', b'a'];
        let widths = [64, 64, 64, 64];
        let mut out = [0; 4];
        pc_di_wrap_scan(text.as_ptr(), widths.as_ptr(), 4, out.as_mut_ptr());
        assert_eq!(out[1], 1); // one wrap
        assert_eq!(out[0], 0); // wrapping char starts the next line uncounted
        // exactly 192 does not wrap
        let widths3 = [64, 64, 64];
        pc_di_wrap_scan(text.as_ptr(), widths3.as_ptr(), 3, out.as_mut_ptr());
        assert_eq!(out[1], 0);
        assert_eq!(out[0], 3);
    }

    #[test]
    fn wrap_scan_newline() {
        let text = [b'a', DIARY_CHAR_NEW_LINE, b'b'];
        let widths = [10, 10, 10];
        let mut out = [0; 4];
        pc_di_wrap_scan(text.as_ptr(), widths.as_ptr(), 3, out.as_mut_ptr());
        assert_eq!(out[1], 1);
        assert_eq!(out[3], 1);
        assert_eq!(out[0], 1); // 'b' on the new line
        assert_eq!(out[2], 1); // one char before the newline
    }

    #[test]
    fn wrap_scan_31_clamp_quirk() {
        // 32 newlines -> newlines clamps to 30, cur_newline_width = newline_width + 1
        let text = [DIARY_CHAR_NEW_LINE; 32];
        let widths = [1; 32];
        let mut out = [0; 4];
        pc_di_wrap_scan(text.as_ptr(), widths.as_ptr(), 32, out.as_mut_ptr());
        assert_eq!(out[1], 30);
        assert_eq!(out[3], 30);
        assert_eq!(out[2], 1); // newline_width (0) + 1
    }

    #[test]
    fn line_check_codes() {
        let text = [b'a', DIARY_CHAR_NEW_LINE, b'b'];
        let widths = [10, 10, 10];
        let (mut pos, mut w, mut ll) = (0, 0, 0);
        // 'a' consumed
        assert_eq!(
            pc_di_line_check(text.as_ptr(), widths.as_ptr(), 3, &mut pos, &mut w, &mut ll),
            0
        );
        assert_eq!((pos, w, ll), (1, 10, 1));
        // newline consumed
        assert_eq!(
            pc_di_line_check(text.as_ptr(), widths.as_ptr(), 3, &mut pos, &mut w, &mut ll),
            1
        );
        assert_eq!((pos, ll), (2, 2));
        // end of text
        pos = 3;
        assert_eq!(
            pc_di_line_check(text.as_ptr(), widths.as_ptr(), 3, &mut pos, &mut w, &mut ll),
            3
        );
        // overflow does not advance
        let text2 = [b'a', b'b'];
        let widths2 = [190, 10];
        let (mut p2, mut w2, mut l2) = (0, 0, 0);
        assert_eq!(
            pc_di_line_check(text2.as_ptr(), widths2.as_ptr(), 2, &mut p2, &mut w2, &mut l2),
            0
        );
        assert_eq!(
            pc_di_line_check(text2.as_ptr(), widths2.as_ptr(), 2, &mut p2, &mut w2, &mut l2),
            2
        );
        assert_eq!((p2, w2, l2), (1, 200, 1));
    }

    #[test]
    fn scroll_limits() {
        // one-line
        assert_eq!(pc_di_scroll_one(0, 20, -1), 0);
        assert_eq!(pc_di_scroll_one(5, 20, -1), 4);
        assert_eq!(pc_di_scroll_one(14, 20, 1), 14); // 14 == 20-6, at limit
        assert_eq!(pc_di_scroll_one(13, 20, 1), 14);
        // ten-line
        assert_eq!(pc_di_scroll_ten(0, 20, -1), 0);
        assert_eq!(pc_di_scroll_ten(15, 20, -1), 5);
        assert_eq!(pc_di_scroll_ten(10, 20, -1), 0); // 10-10>0 false -> 0
        assert_eq!(pc_di_scroll_ten(11, 20, -1), 1);
        assert_eq!(pc_di_scroll_ten(0, 20, 1), 10);
        assert_eq!(pc_di_scroll_ten(8, 20, 1), 14); // 8+10<14 false -> limit
        assert_eq!(pc_di_scroll_ten(14, 20, 1), 14);
    }

    #[test]
    fn repeat_timing() {
        let mut st = DiRepeatState::default();
        // fresh up trigger
        assert_eq!(pc_di_repeat_step(&mut st, -1, 0), DIARY_CMD_UP);
        assert_eq!((st.f12, st.f11, st.f10, st.f30), (30, 26, 0, 1));
        // held: counters decrement, no command yet
        assert_eq!(pc_di_repeat_step(&mut st, -1, 0), 0);
        assert_eq!((st.f12, st.f11), (29, 26));
        // after delay expires with _11 != 0 -> slow repeat fires
        st.f12 = 1;
        assert_eq!(pc_di_repeat_step(&mut st, -1, 0), 0); // f12 1->0
        assert_eq!(pc_di_repeat_step(&mut st, -1, 0), DIARY_CMD_UP); // f12==0, _11!=0
        assert_eq!(st.f30, DIARY_REPEAT_SLOW);
        // _11 == 0 -> fast repeat
        st.f11 = 0;
        st.f12 = 0;
        assert_eq!(pc_di_repeat_step(&mut st, -1, 0), DIARY_CMD_UP);
        assert_eq!(st.f30, DIARY_REPEAT_FAST);
        // busy suppresses commands but still decrements
        st.f12 = 5;
        assert_eq!(pc_di_repeat_step(&mut st, -1, 1), 0);
        assert_eq!(st.f12, 4);
        // release resets
        assert_eq!(pc_di_repeat_step(&mut st, 0, 0), 0);
        assert_eq!((st.prev_dir, st.f12, st.f30), (0, 0, 1));
        // direction change is a fresh trigger
        assert_eq!(pc_di_repeat_step(&mut st, 1, 0), DIARY_CMD_DOWN);
    }

    #[test]
    fn ten_line_cmd() {
        assert_eq!(pc_di_ten_line_cmd(1, 0, 0), DIARY_CMD_UP10);
        assert_eq!(pc_di_ten_line_cmd(0, 1, 0), DIARY_CMD_DOWN10);
        assert_eq!(pc_di_ten_line_cmd(0, 0, 0), 0);
        assert_eq!(pc_di_ten_line_cmd(1, 0, 1), 0);
    }

    #[test]
    fn month_adjust() {
        assert_eq!(pc_di_month_adjust(0), -26.0);
        assert_eq!(pc_di_month_adjust(8), 0.0);
        assert_eq!(pc_di_month_adjust(11), -6.0);
        assert_eq!(pc_di_month_adjust(12), 0.0);
        assert_eq!(pc_di_month_adjust(-1), 0.0);
    }

    #[test]
    fn confirm_branch() {
        assert_eq!(pc_di_confirm_branch(0), DIARY_CONFIRM_WRITE);
        assert_eq!(pc_di_confirm_branch(1), DIARY_CONFIRM_REEDIT);
    }
}
