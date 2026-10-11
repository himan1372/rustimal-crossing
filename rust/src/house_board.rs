//! House board: the visitor-message editor for the
//! house's Haniwa data (`m_hboard_ovl.c`,
//! `m_hboard_ovl.h`, `m_home_h.h`).
//!
//! Verified against GAFE01_00 Rev 0 USA (see
//! `board-verification-report.md`; 8/8 sections
//! PROVEN).
//!
//! This is NOT another 15-post board: it is a
//! single 128-byte message at offset 0x20 in the
//! 0xA4-byte `Haniwa_c` save structure
//! (`homes[house_idx].haniwa.message`). The overlay
//! opens the shared editor directly against that
//! buffer (`mED_TYPE_HBOARD`).
//!
//! This module ports the pure line-layout check
//! (`mHB_strLineCheck`) with the font-width
//! function injected. The saved message bytes and
//! the shared editor lifecycle (`hboard_exists`,
//! submenu transitions), font rendering, and
//! `mFont_GetCodeWidth` itself stay in C.

/// Maximum line width.
pub const HBOARD_LINE_WIDTH_MAX: i32 = 192;
/// Displayed lines.
pub const HBOARD_LINE_NUM: i32 = 4;
/// Message buffer length.
pub const HBOARD_MESSAGE_LEN: usize = 128;
/// Vertical advance per rendered line.
pub const HBOARD_LINE_Y_STEP: f32 = 16.0;
/// Encoded newline byte.
pub const CHAR_NEW_LINE: u8 = 205;

/// Line-check results.
pub const LINE_CHECK_OK: i32 = 0;
pub const LINE_CHECK_NEWLINE: i32 = 1;
pub const LINE_CHECK_OVER_WIDTH: i32 = 2;
pub const LINE_CHECK_OVER_STR_LEN: i32 = 3;

/// Line check (`mHB_strLineCheck`): with `pos` the
/// current byte index into a `str_len`-byte string,
/// `line_width` the accumulated line width, and
/// `get_width` the injected encoded-font width
/// function:
/// - `pos >= str_len` → OVER_STR_LEN (end reached)
/// - byte == CHAR_NEW_LINE → NEWLINE (pos and
///   line_len advance)
/// - `line_width + get_width(ch) > 192` →
///   OVER_WIDTH (nothing advances)
/// - else → OK (pos and line_len advance)
/// Writes (new_pos, new_line_width, new_line_len)
/// into `out[3]`; returns the result.
#[no_mangle]
pub extern "C" fn pc_hboard_line_check(
    str: *const u8,
    pos: usize,
    str_len: usize,
    line_width: i32,
    line_len: i32,
    get_width: extern "C" fn(u8) -> i32,
    out: *mut i32,
) -> i32 {
    let (result, new_pos, new_width, new_len) = if pos >= str_len {
        (LINE_CHECK_OVER_STR_LEN, pos, line_width, line_len)
    } else {
        let ch = unsafe { *str.add(pos) };
        if ch == CHAR_NEW_LINE {
            (LINE_CHECK_NEWLINE, pos + 1, line_width, line_len + 1)
        } else {
            let width = line_width + get_width(ch);
            if width > HBOARD_LINE_WIDTH_MAX {
                (LINE_CHECK_OVER_WIDTH, pos, line_width, line_len)
            } else {
                (LINE_CHECK_OK, pos + 1, width, line_len + 1)
            }
        }
    };
    if !out.is_null() {
        unsafe {
            *out = new_pos as i32;
            *out.add(1) = new_width;
            *out.add(2) = new_len;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    extern "C" fn width10(_ch: u8) -> i32 {
        10
    }
    extern "C" fn width200(_ch: u8) -> i32 {
        200
    }

    #[test]
    fn line_check() {
        let s = b"ab\xCDc"; // \xCD = 205 = newline
        let mut out = [0i32; 3];
        // Ordinary char: advance.
        assert_eq!(
            pc_hboard_line_check(s.as_ptr(), 0, 4, 0, 0, width10, out.as_mut_ptr()),
            LINE_CHECK_OK
        );
        assert_eq!(out, [1, 10, 1]);
        // Newline: advance pos and len, width unchanged.
        assert_eq!(
            pc_hboard_line_check(s.as_ptr(), 2, 4, 10, 1, width10, out.as_mut_ptr()),
            LINE_CHECK_NEWLINE
        );
        assert_eq!(out, [3, 10, 2]);
        // Over width: nothing advances.
        assert_eq!(
            pc_hboard_line_check(s.as_ptr(), 0, 4, 190, 0, width10, out.as_mut_ptr()),
            LINE_CHECK_OVER_WIDTH
        );
        assert_eq!(out, [0, 190, 0]);
        // Exactly at the limit is fine.
        assert_eq!(
            pc_hboard_line_check(s.as_ptr(), 0, 4, 182, 0, width10, out.as_mut_ptr()),
            LINE_CHECK_OK
        );
        assert_eq!(out, [1, 192, 1]);
        // A single char wider than the line overflows immediately.
        assert_eq!(
            pc_hboard_line_check(s.as_ptr(), 0, 4, 0, 0, width200, out.as_mut_ptr()),
            LINE_CHECK_OVER_WIDTH
        );
        // End of string.
        assert_eq!(
            pc_hboard_line_check(s.as_ptr(), 4, 4, 0, 0, width10, out.as_mut_ptr()),
            LINE_CHECK_OVER_STR_LEN
        );
        assert_eq!(
            pc_hboard_line_check(s.as_ptr(), 9, 4, 0, 0, width10, out.as_mut_ptr()),
            LINE_CHECK_OVER_STR_LEN
        );
    }
}
