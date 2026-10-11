//! Shared choice window (m_choice.c) pure kernels.
//!
//! Verified against the ACGC-PC-Port tree (GAFE01_00 Rev 0 USA
//! base). This is the reusable dialogue/prompt selection widget
//! (up to six options, cursor + confirmed result, open/close
//! animation). Yes/no prompts are just two-option windows; the
//! caller interprets the resulting index.
//!
//! m_select.c ("FOREST MAP SELECT") is a separate debug scene
//! selector, not the generic widget; only its index wraparound is
//! ported here.
//!
//! Tree-vs-brief notes: the constructor sets `data.choice_num =
//! 6`, not 0 (the zero reset lives in the `Set_choice_data`
//! path); D-pad Up/Down also drive the cursor (in addition to
//! the C-stick); the repeat machine accumulates float dt
//! (`dt_num_60fps_frames`) with 18.0/9.0 thresholds, not integer
//! frames; appear/disappear animate via dt to 10.2f with easing.
//!
//! The following stay in C: message-window embedding, font width
//! measurement, ROM/ARAM string loading, control-code
//! substitution, the `mMsg` control-code dispatch, sounds,
//! graphics/display lists, matrix setup, and easing.

#![allow(non_snake_case)]

use std::os::raw::{c_int, c_uchar};

/// Max option text bytes (`mChoice_CHOICE_STRING_LEN`).
pub const CHOICE_STRING_LEN: i32 = 16;
/// Max options (`mChoice_CHOICE_NUM`).
pub const CHOICE_NUM: i32 = 6;
/// Choice resource count (`mChoice_SELECT_STR_NUM`).
pub const CHOICE_SELECT_STR_NUM: i32 = 607;
/// Cursor repeat states (`m_choice.h`).
pub const CHOICE_AUTOMOVE_STOPPED: i32 = 0;
pub const CHOICE_AUTOMOVE_INCREMENT_WAIT: i32 = 1;
pub const CHOICE_AUTOMOVE_INCREMENT: i32 = 2;
pub const CHOICE_AUTOMOVE_DECREMENT_WAIT: i32 = 3;
pub const CHOICE_AUTOMOVE_DECREMENT: i32 = 4;
/// Main window states.
pub const CHOICE_MAIN_HIDE: i32 = 0;
pub const CHOICE_MAIN_APPEAR: i32 = 1;
pub const CHOICE_MAIN_NORMAL: i32 = 2;
pub const CHOICE_MAIN_DISAPPEAR: i32 = 3;
/// Repeat thresholds (float dt).
pub const CHOICE_REPEAT_WAIT: f32 = 18.0;
pub const CHOICE_REPEAT_RATE: f32 = 9.0;
/// Game space character.
pub const CHOICE_CHAR_SPACE: u8 = 0x20;

/// Trailing-space trim (`mMsg_Get_Length_String` semantics).
fn trim_len(s: &[u8]) -> usize {
    let mut i = s.len();
    while i > 0 && s[i - 1] == CHOICE_CHAR_SPACE {
        i -= 1;
    }
    i
}

/// `mChoice_Set_choice_data` reset: the choice count starts at 0
/// before options are admitted one by one.
#[no_mangle]
pub extern "C" fn pc_choice_begin(count: *mut c_int) {
    if !count.is_null() {
        unsafe {
            *count = 0;
        }
    }
}

/// `mChoice_Add_choice_data` (`m_choice_main.c_inc`): admits one
/// option. Returns the slot index, or -1 when rejected (null
/// pointer, `max_len <= 0`, `max_len > 16`, or no free slot).
/// The effective length is the trailing-space trim; exactly
/// `len` bytes are copied with NO row clearing beforehand.
#[no_mangle]
pub extern "C" fn pc_choice_add_option(
    strings: *mut c_uchar,
    lens: *mut c_int,
    count: *mut c_int,
    text: *const c_uchar,
    max_len: c_int,
) -> c_int {
    if strings.is_null() || lens.is_null() || count.is_null() || text.is_null() {
        return -1;
    }
    if max_len <= 0 || max_len > CHOICE_STRING_LEN {
        return -1;
    }
    let n = unsafe { *count };
    if n < 0 || n >= CHOICE_NUM {
        return -1;
    }
    let src = unsafe { std::slice::from_raw_parts(text, max_len as usize) };
    let len = trim_len(src);
    let dst = unsafe { std::slice::from_raw_parts_mut(strings.add((n * CHOICE_STRING_LEN) as usize), CHOICE_STRING_LEN as usize) };
    dst[..len].copy_from_slice(&src[..len]);
    unsafe {
        *lens.add(n as usize) = len as c_int;
        *count = n + 1;
    }
    n
}

/// Cursor step with clamping (`m_choice_main_normal.c_inc`): no
/// wraparound. `dir` is +1 (down) or -1 (up). When `count == 0`
/// the result is -1.
#[no_mangle]
pub extern "C" fn pc_choice_cursor_step(cursor: c_int, count: c_int, dir: c_int) -> c_int {
    let mut idx = cursor + dir;
    if idx < 0 {
        idx = 0;
    } else if idx >= count {
        idx = count - 1;
    }
    idx
}

/// Cursor auto-repeat state machine
/// (`m_choice_main_normal.c_inc`). `up_held`/`down_held` are the
/// held-button states, `up_trig`/`down_trig` the newly-pressed
/// states, `stick_y` the adjusted vertical stick value
/// (`percent`), `dt` the frame's `dt_num_60fps_frames`. Updates
/// `*state`, `*timer`, `*cursor` (with the verbatim no-wrap
/// clamp), and sets `*play_sound` to 1 when the cursor moved
/// without clamping.
#[no_mangle]
pub extern "C" fn pc_choice_repeat_step(
    state: *mut c_int,
    timer: *mut f32,
    up_held: c_int,
    down_held: c_int,
    up_trig: c_int,
    down_trig: c_int,
    stick_y: f32,
    dt: f32,
    cursor: *mut c_int,
    count: c_int,
    play_sound: *mut c_int,
) {
    if state.is_null() || timer.is_null() || cursor.is_null() {
        return;
    }
    let (st, tm, mut cur) = unsafe { (*state, *timer, *cursor) };
    let mut sfx = 0;
    let mut nst = st;
    let mut ntm = tm;
    match st {
        CHOICE_AUTOMOVE_INCREMENT_WAIT => {
            if stick_y < 0.0 || down_held != 0 {
                ntm += dt;
                if ntm >= CHOICE_REPEAT_WAIT {
                    ntm = 0.0;
                    nst = CHOICE_AUTOMOVE_INCREMENT;
                    cur += 1;
                    sfx = 1;
                }
            } else {
                nst = CHOICE_AUTOMOVE_STOPPED;
                ntm = 0.0;
            }
        }
        CHOICE_AUTOMOVE_INCREMENT => {
            if stick_y < 0.0 || down_held != 0 {
                ntm += dt;
                if ntm >= CHOICE_REPEAT_RATE {
                    ntm = 0.0;
                    cur += 1;
                    sfx = 1;
                }
            } else {
                nst = CHOICE_AUTOMOVE_STOPPED;
                ntm = 0.0;
            }
        }
        CHOICE_AUTOMOVE_DECREMENT_WAIT => {
            if stick_y > 0.0 || up_held != 0 {
                ntm += dt;
                if ntm >= CHOICE_REPEAT_WAIT {
                    ntm = 0.0;
                    nst = CHOICE_AUTOMOVE_DECREMENT;
                    cur -= 1;
                    sfx = 1;
                }
            } else {
                nst = CHOICE_AUTOMOVE_STOPPED;
                ntm = 0.0;
            }
        }
        CHOICE_AUTOMOVE_DECREMENT => {
            if stick_y > 0.0 || up_held != 0 {
                ntm += dt;
                if ntm >= CHOICE_REPEAT_RATE {
                    ntm = 0.0;
                    cur -= 1;
                    sfx = 1;
                }
            } else {
                nst = CHOICE_AUTOMOVE_STOPPED;
                ntm = 0.0;
            }
        }
        _ => {
            if stick_y > 0.0 || up_trig != 0 {
                nst = CHOICE_AUTOMOVE_DECREMENT_WAIT;
                ntm = 0.0;
                cur -= 1;
                sfx = 1;
            } else if stick_y < 0.0 || down_trig != 0 {
                nst = CHOICE_AUTOMOVE_INCREMENT_WAIT;
                ntm = 0.0;
                cur += 1;
                sfx = 1;
            }
        }
    }
    // Verbatim clamp (no wrap); the cursor sound is suppressed
    // when the move was clamped.
    let clamped = if cur < 0 {
        cur = 0;
        true
    } else if cur >= count {
        cur = count - 1;
        true
    } else {
        false
    };
    if clamped {
        sfx = 0;
    }
    unsafe {
        *state = nst;
        *timer = ntm;
        *cursor = cur;
        if !play_sound.is_null() {
            *play_sound = sfx;
        }
    }
}

/// B-button shortcut rule (`m_choice_main_normal.c_inc`):
/// returns 1 when `no_b_flag` makes B select and confirm the
/// final option.
#[no_mangle]
pub extern "C" fn pc_choice_b_shortcut(no_b_flag: c_int) -> c_int {
    (no_b_flag != 0) as c_int
}

/// `mChoice_determimation_set` (`m_choice_main.c_inc`): commits
/// the highlighted option. Copies the 16-byte option row, records
/// its length, and returns the confirmed index. No bounds check
/// in the C; this kernel guards the cursor range.
#[no_mangle]
pub extern "C" fn pc_choice_determination(
    strings: *const c_uchar,
    lens: *const c_int,
    cursor: c_int,
    out16: *mut c_uchar,
    out_len: *mut c_int,
) -> c_int {
    if strings.is_null() || lens.is_null() || out16.is_null() {
        return -1;
    }
    if cursor < 0 || cursor >= CHOICE_NUM {
        return -1;
    }
    unsafe {
        std::ptr::copy_nonoverlapping(
            strings.add((cursor * CHOICE_STRING_LEN) as usize),
            out16,
            CHOICE_STRING_LEN as usize,
        );
        if !out_len.is_null() {
            *out_len = *lens.add(cursor as usize);
        }
    }
    cursor
}

/// Close-path sound/flag selection
/// (`m_choice_main_disappear.c_inc`): 0 = ordinary confirm sound
/// (`NA_SE_SENTAKU_KETTEI`), 1 = short zoom-down, 2 = long
/// zoom-down + `mMsg_STATUS_FLAG_NO_ZOOMDOWN`.
#[no_mangle]
pub extern "C" fn pc_choice_close_kind(
    no_b_flag: c_int,
    no_close_flag: c_int,
    via_b_shortcut: c_int,
    cursor_is_last: c_int,
) -> c_int {
    if via_b_shortcut != 0 && no_b_flag != 0 && cursor_is_last != 0 {
        if no_close_flag != 0 {
            2
        } else {
            1
        }
    } else {
        0
    }
}

/// Debug scene-selector wraparound (`m_select.c`
/// `select_course_step_sub`): unlike the choice widget, the scene
/// index wraps around the table.
#[no_mangle]
pub extern "C" fn pc_select_wrap(course: c_int, scene_num: c_int, dir: c_int) -> c_int {
    if scene_num <= 0 {
        return 0;
    }
    (course + dir).rem_euclid(scene_num)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts() -> ([u8; 96], [c_int; 6], c_int) {
        ([0xAA; 96], [0; 6], 0)
    }

    #[test]
    fn option_admission() {
        let (mut s, mut lens, mut count) = opts();
        // valid
        let t = b"Hello           "; // 16 bytes, trims to 5
        assert_eq!(
            pc_choice_add_option(
                s.as_mut_ptr(),
                lens.as_mut_ptr(),
                &mut count,
                t.as_ptr(),
                16
            ),
            0
        );
        assert_eq!((count, lens[0]), (1, 5));
        assert_eq!(&s[..5], b"Hello");
        // row NOT cleared: tail keeps the 0xAA fill
        assert_eq!(s[5], 0xAA);
        // null / bad lengths rejected
        assert_eq!(
            pc_choice_add_option(s.as_mut_ptr(), lens.as_mut_ptr(), &mut count, std::ptr::null(), 16),
            -1
        );
        assert_eq!(pc_choice_add_option(s.as_mut_ptr(), lens.as_mut_ptr(), &mut count, t.as_ptr(), 0), -1);
        assert_eq!(pc_choice_add_option(s.as_mut_ptr(), lens.as_mut_ptr(), &mut count, t.as_ptr(), 17), -1);
        assert_eq!(count, 1);
        // fill to six, seventh rejected
        for _ in 1..6 {
            assert!(pc_choice_add_option(s.as_mut_ptr(), lens.as_mut_ptr(), &mut count, t.as_ptr(), 16) >= 0);
        }
        assert_eq!(count, 6);
        assert_eq!(pc_choice_add_option(s.as_mut_ptr(), lens.as_mut_ptr(), &mut count, t.as_ptr(), 16), -1);
    }

    #[test]
    fn cursor_clamp_no_wrap() {
        assert_eq!(pc_choice_cursor_step(0, 6, -1), 0);
        assert_eq!(pc_choice_cursor_step(5, 6, 1), 5);
        assert_eq!(pc_choice_cursor_step(2, 6, 1), 3);
        assert_eq!(pc_choice_cursor_step(0, 0, 1), -1); // empty -> -1
    }

    #[test]
    fn repeat_machine() {
        let (mut st, mut tm, mut cur, mut sfx) = (CHOICE_AUTOMOVE_STOPPED, 0.0f32, 2, 0);
        // fresh down trigger: immediate move + WAIT state
        pc_choice_repeat_step(&mut st, &mut tm, 0, 0, 0, 1, 0.0, 1.0, &mut cur, 6, &mut sfx);
        assert_eq!((st, cur, sfx), (CHOICE_AUTOMOVE_INCREMENT_WAIT, 3, 1));
        // held but under 18.0: no move
        pc_choice_repeat_step(&mut st, &mut tm, 0, 1, 0, 0, 0.0, 10.0, &mut cur, 6, &mut sfx);
        assert_eq!((cur, sfx), (3, 0));
        // reaching 18.0: move + INCREMENT state
        pc_choice_repeat_step(&mut st, &mut tm, 0, 1, 0, 0, 0.0, 8.0, &mut cur, 6, &mut sfx);
        assert_eq!((st, cur, sfx), (CHOICE_AUTOMOVE_INCREMENT, 4, 1));
        // repeat at 9.0
        pc_choice_repeat_step(&mut st, &mut tm, 0, 1, 0, 0, 0.0, 9.0, &mut cur, 6, &mut sfx);
        assert_eq!((cur, sfx), (5, 1));
        // clamped at last option: no sound
        pc_choice_repeat_step(&mut st, &mut tm, 0, 1, 0, 0, 0.0, 9.0, &mut cur, 6, &mut sfx);
        assert_eq!((cur, sfx), (5, 0));
        // release: back to STOPPED
        pc_choice_repeat_step(&mut st, &mut tm, 0, 0, 0, 0, 0.0, 1.0, &mut cur, 6, &mut sfx);
        assert_eq!((st, tm), (CHOICE_AUTOMOVE_STOPPED, 0.0));
        // stick drives it too (negative Y = down)
        pc_choice_repeat_step(&mut st, &mut tm, 0, 0, 0, 0, -0.5, 1.0, &mut cur, 6, &mut sfx);
        assert_eq!((st, cur), (CHOICE_AUTOMOVE_INCREMENT_WAIT, 5));
    }

    #[test]
    fn b_and_close_rules() {
        assert_eq!(pc_choice_b_shortcut(1), 1);
        assert_eq!(pc_choice_b_shortcut(0), 0);
        assert_eq!(pc_choice_close_kind(1, 0, 1, 1), 1); // short zoom-down
        assert_eq!(pc_choice_close_kind(1, 1, 1, 1), 2); // long + flag
        assert_eq!(pc_choice_close_kind(1, 1, 1, 0), 0); // not last -> confirm
        assert_eq!(pc_choice_close_kind(0, 0, 0, 0), 0); // ordinary confirm
    }

    #[test]
    fn determination_copy() {
        let (mut s, mut lens, mut count) = opts();
        let t = b"Yes             ";
        pc_choice_add_option(s.as_mut_ptr(), lens.as_mut_ptr(), &mut count, t.as_ptr(), 16);
        let t2 = b"No              ";
        pc_choice_add_option(s.as_mut_ptr(), lens.as_mut_ptr(), &mut count, t2.as_ptr(), 16);
        let (mut out, mut olen) = ([0u8; 16], 0);
        assert_eq!(
            pc_choice_determination(s.as_ptr(), lens.as_ptr(), 1, out.as_mut_ptr(), &mut olen),
            1
        );
        assert_eq!(&out[..2], b"No");
        assert_eq!(olen, 2);
        assert_eq!(pc_choice_determination(s.as_ptr(), lens.as_ptr(), 9, out.as_mut_ptr(), &mut olen), -1);
    }

    #[test]
    fn select_wrap() {
        assert_eq!(pc_select_wrap(0, 10, -1), 9);
        assert_eq!(pc_select_wrap(9, 10, 1), 0);
        assert_eq!(pc_select_wrap(3, 10, 1), 4);
    }
}
