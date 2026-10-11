//! Notice-board overlay and auto-post scheduler
//! (m_notice_ovl.c, m_notice.c) pure kernels.
//!
//! Verified against GAFE01_00 Rev 0 decomp. The record layer
//! (first-empty scan, FIFO shift, page clear, init handbills,
//! auto-post time, handbill-id formulas) is already ported in
//! `town_board.rs` and is NOT duplicated here. The handbill
//! composer primitives live in `npc_reply.rs`.
//!
//! The following stay in C until Rust/C equivalence is proven:
//! RTC compares and save/common reads-writes, the shared editor
//! and edit-end overlays, `mFont_GetCodeWidth`, handbill ROM
//! loading, `mHandbill_Set_free_str` side effects, fishing-result
//! mail, furniture selection, treasure placement, animal-memory
//! queries, first-job quest mutation, sounds/music,
//! graphics/display lists, the `add_calc`/`chase_f` easing,
//! transitions, and overlay allocation.

#![allow(non_snake_case)]

use std::os::raw::{c_int, c_uchar};

/// Number of scheduled auto-post dates (`mNtc_DATA_LIST_NUM`).
pub const NOTICE_DATA_LIST_NUM: i32 = 43;
/// Max auto-posts per run (`mNtc_NWRITE_NUM`).
pub const NOTICE_NWRITE_NUM: i32 = 5;
/// Max text width (`mNT_MAX_WIDTH`).
pub const NOTICE_MAX_WIDTH: i32 = 192;
/// Max drawn lines (`mNT_MAX_LINES`).
pub const NOTICE_MAX_LINES: i32 = 6;
/// Working-post page index (the editor buffer).
pub const NOTICE_WORKING_PAGE: i32 = 15;
/// Line-check codes (`m_notice_ovl.h`).
pub const NOTICE_LINE_OK: i32 = 0;
pub const NOTICE_LINE_NEWLINE: i32 = 1;
pub const NOTICE_LINE_OVERLINE: i32 = 2;
pub const NOTICE_LINE_OVERSTRING: i32 = 3;
/// Stick-area codes (`m_editor_ovl.h` order).
pub const NOTICE_STICK_LEFT: i32 = 0;
pub const NOTICE_STICK_TOP_LEFT: i32 = 1;
pub const NOTICE_STICK_TOP: i32 = 2;
pub const NOTICE_STICK_TOP_RIGHT: i32 = 3;
pub const NOTICE_STICK_RIGHT: i32 = 4;
pub const NOTICE_STICK_BOTTOM_RIGHT: i32 = 5;
pub const NOTICE_STICK_BOTTOM: i32 = 6;
pub const NOTICE_STICK_BOTTOM_LEFT: i32 = 7;
pub const NOTICE_STICK_CENTER: i32 = 8;
/// Stick magnitude threshold for CENTER.
pub const NOTICE_STICK_THRESHOLD: f32 = 0.2;
/// Fishing-day markers.
pub const NOTICE_FISHING_DAY_NUM: i32 = 0xFF;
pub const NOTICE_FISHING_DAY_YEAR: i32 = 0xFFFF;
/// Treasure chance (`mNtc_TREASURE_CHANCE`).
pub const NOTICE_TREASURE_CHANCE: f32 = 0.4;
/// Minimum days between treasures.
pub const NOTICE_MIN_DAYS_BETWEEN_TREASURE: i32 = 3;
/// Game explicit newline.
pub const NOTICE_CHAR_NEW_LINE: u8 = 205;

/// `kei_win_st_tex_tbl` (`m_notice_ovl.c`) as texture-slot indices
/// (st5=5, st4=4, st2=2, st6=6, st3=3, st1=1), indexed by stick area.
/// The actual textures are ROM assets; the repeats are preserved.
pub const NOTICE_KEI_TEX: [u8; 9] = [5, 4, 2, 4, 5, 6, 3, 6, 1];

/// The 43-entry auto-post date table (`mNtc_copy_data_list`,
/// verbatim): (template id, packed month<<8|day). Entries 0x27/0x28
/// are 0xFFFF (unused).
pub const NOTICE_DATE_TABLE: [(u8, u16); NOTICE_DATA_LIST_NUM as usize] = [
    (0x00, 0x0101), // Nook's New Year's post
    (0x01, 0x010F), // Snow weather report
    (0x02, 0x0119), // Groundhog day notice
    (0x03, 0x0201), // Valentine's Day notice
    (0x04, 0x020F), // Pete's Valentine's Day complaint
    (0x05, 0x030F), // Cherry Blossom Festival notice
    (0x06, 0x030B), // Spring Sports Fair notice
    (0x07, 0x0310), // Spring Sports Fair vernal equinox notice
    (0x08, 0x0314), // Spring Sports Fair schedule
    (0x09, 0x0403), // Cherry Blossom Festival reminder notice
    (0x0A, 0x0415), // Spring weather notice
    (0x0B, 0x0506), // June weather notice
    (0x0C, 0x0514), // Summer fishing tourney notice
    (0x0D, 0x0608), // Chip's 1st summer fishing tourney post
    (0x0E, 0x0617), // Chip's final summer fishing tourney post
    (0x0F, 0x0619), // Fireworks Festival notice
    (0x10, 0x0701), // July weather notice
    (0x11, 0x0705), // Fireworks Festival complaint
    (0x12, 0x070F), // Morning aerobics notice
    (0x13, 0x0719), // Copper morning aerobics post
    (0x14, 0x0801), // Meteor Shower notice
    (0x15, 0x081E), // Copper morning aerobics final day notice
    (0x16, 0x0901), // Moon Viewing notice
    (0x17, 0x090F), // September weather report
    (0x18, 0x090D), // Pete's Fall Sports Fair post
    (0x19, 0x0912), // Fall Sports Fair notice
    (0x1A, 0x0916), // Fall Sports Fair schedule
    (0x1B, 0x0A0A), // Mushrooming season notice
    (0x1C, 0x0A10), // Nook's Halloween candy post
    (0x1D, 0x0A14), // Halloween notice
    (0x1E, 0x0A19), // Fall fishing tourney notice
    (0x1F, 0x0B08), // Chip's 1st fall fishing tourney post
    (0x20, 0x0B0A), // Harvest Festival notice
    (0x21, 0x0B0C), // November weather report
    (0x22, 0x0B17), // Chip's final fall fishing tourney post
    (0x23, 0x0C09), // Winter storm warning
    (0x24, 0x0C14), // Jingle's holiday notice
    (0x25, 0x0C19), // New Year's Eve countdown notice
    (0x26, 0x0C1C), // New Year's Day event notice
    (0x27, 0xFFFF), // Pete check mailbox info?
    (0x28, 0xFFFF), // Anti-litter notice?
    (0x29, 0x031F), // Day light savings begin notice
    (0x2A, 0x0A1F), // Day light savings end notice
];

/// `mNT_Play_page_read` navigation (`m_notice_ovl.c`): returns
/// `(move_page, action)` with the source else-if priority.
/// `action`: 0 = none, 1 = begin write (A), 2 = exit (B/Start).
/// The C caller combines stick areas and C-button triggers before
/// calling; this kernel takes the resolved direction flags.
#[no_mangle]
pub extern "C" fn pc_notice_nav_step(
    now_page: c_int,
    page_count: c_int,
    left: c_int,
    right: c_int,
    down: c_int,
    up: c_int,
    a: c_int,
    b_start: c_int,
    out_move_page: *mut c_int,
    out_action: *mut c_int,
) {
    let (mut move_page, mut action) = (0, 0);
    if now_page != 0 && left != 0 {
        move_page = -1;
    } else if now_page < page_count - 1 && right != 0 {
        move_page = 1;
    } else if now_page != 0 && down != 0 {
        move_page = -now_page;
    } else if now_page < page_count - 1 && up != 0 {
        move_page = (page_count - now_page) - 1;
    } else if a != 0 {
        action = 1;
    } else if b_start != 0 {
        action = 2;
    }
    if !out_move_page.is_null() {
        unsafe {
            *out_move_page = move_page;
        }
    }
    if !out_action.is_null() {
        unsafe {
            *out_action = action;
        }
    }
}

/// Page-turn time (`mNT_Play_page_read`): `min(|move_page|, 3)`.
#[no_mangle]
pub extern "C" fn pc_notice_move_time(move_page: c_int) -> c_int {
    move_page.abs().min(3)
}

/// First page advance of a turn (`mNT_Play_page_read`):
/// `now_page += move_page / move_time` (C integer division).
#[no_mangle]
pub extern "C" fn pc_notice_first_step(now_page: c_int, move_page: c_int, move_time: c_int) -> c_int {
    now_page + move_page / move_time
}

/// Later page advances (`mNT_Play_page_move`): forward
/// `now_page += ((page_count - now_page) - 1) / move_time`,
/// backward `now_page -= now_page / move_time`.
#[no_mangle]
pub extern "C" fn pc_notice_later_step(
    now_page: c_int,
    page_count: c_int,
    forward: c_int,
    move_time: c_int,
) -> c_int {
    if forward != 0 {
        now_page + ((page_count - now_page) - 1) / move_time
    } else {
        now_page - now_page / move_time
    }
}

/// `mNT_stick_area_check` (`m_notice_ovl.c`): classifies the stick
/// into the 9 areas. `move_angle` is the signed s16 angle.
#[no_mangle]
pub extern "C" fn pc_notice_stick_area(adjusted_pR: f32, move_angle: i16) -> c_int {
    if adjusted_pR < NOTICE_STICK_THRESHOLD {
        return NOTICE_STICK_CENTER;
    }
    let a = move_angle as i32;
    if a < -0x7000 {
        NOTICE_STICK_LEFT
    } else if a < -0x5000 {
        NOTICE_STICK_BOTTOM_LEFT
    } else if a < -0x3000 {
        NOTICE_STICK_BOTTOM
    } else if a < -0x1000 {
        NOTICE_STICK_BOTTOM_RIGHT
    } else if a < 0x1000 {
        NOTICE_STICK_RIGHT
    } else if a < 0x3000 {
        NOTICE_STICK_TOP_RIGHT
    } else if a < 0x5000 {
        NOTICE_STICK_TOP
    } else if a < 0x7000 {
        NOTICE_STICK_TOP_LEFT
    } else {
        NOTICE_STICK_LEFT
    }
}

/// `kei_win_st_tex_tbl` lookup: texture-slot index for a stick area.
#[no_mangle]
pub extern "C" fn pc_notice_kei_tex(stick_area: c_int) -> c_int {
    if stick_area < 0 || stick_area > 8 {
        return -1;
    }
    NOTICE_KEI_TEX[stick_area as usize] as c_int
}

/// `mNT_strLineCheck` (`m_notice_ovl.c`): the notice line-check
/// state machine. Font widths are injected (`get_width(code,
/// flag)`), mirroring the diary's port. Returns one of the
/// `NOTICE_LINE_*` codes; `pos`, `width`, `line` are in/out.
#[no_mangle]
pub extern "C" fn pc_notice_line_check(
    text: *const c_uchar,
    text_len: c_int,
    pos: *mut c_int,
    width: *mut c_int,
    line: *mut c_int,
    get_width: extern "C" fn(code: c_uchar, flag: c_int) -> c_int,
) -> c_int {
    if text.is_null() || pos.is_null() || width.is_null() || line.is_null() || text_len <= 0 {
        return NOTICE_LINE_OVERSTRING;
    }
    let p = unsafe { *pos };
    if p < 0 || p >= text_len {
        return NOTICE_LINE_OVERSTRING;
    }
    let ch = unsafe { *text.add(p as usize) };
    if ch == NOTICE_CHAR_NEW_LINE {
        unsafe {
            *pos = p + 1;
            *line += 1;
        }
        NOTICE_LINE_NEWLINE
    } else {
        let w = unsafe { *width } + get_width(ch, 1);
        if w > NOTICE_MAX_WIDTH {
            NOTICE_LINE_OVERLINE
        } else {
            unsafe {
                *pos = p + 1;
                *width = w;
                *line += 1;
            }
            NOTICE_LINE_OK
        }
    }
}

/// Page-number display (`m_notice_ovl.c`): `page_no + 1`, except the
/// working post (`page_no == 15`) shows `min(page_count + 1, 15)`.
#[no_mangle]
pub extern "C" fn pc_notice_page_no(page_no: c_int, page_count: c_int) -> c_int {
    if page_no == NOTICE_WORKING_PAGE {
        (page_count + 1).min(15)
    } else {
        page_no + 1
    }
}

/// Date-header month rule (`m_notice_ovl.c`): the displayed month
/// index is the month when valid, otherwise the stored timestamp
/// is mutated to month 13 (the "???" header). Returns the month to
/// display.
#[no_mangle]
pub extern "C" fn pc_notice_month_idx(month: c_int) -> c_int {
    if (1..=12).contains(&month) {
        month
    } else {
        13
    }
}

/// Confirmation branches (`mNT_move_Wait`): 0 = commit the post,
/// 1 = return to the editor, anything else = discard.
#[no_mangle]
pub extern "C" fn pc_notice_confirm(data1: c_int) -> c_int {
    match data1 {
        0 => 0,
        1 => 1,
        _ => 2,
    }
}

/// `mNtc_operate_data_list` (`m_notice.c`): applies the computed
/// equinox/DST dates to the table. The equinox days and DST Sundays
/// come from C (`lbRk_VernalEquinoxDay`,
/// `lbRk_AutumnalEquinoxDay`, `mEv_weekday2day`); this kernel does
/// only the packed-date arithmetic, verbatim (the additions carry
/// from the day byte into the month byte).
#[no_mangle]
pub extern "C" fn pc_notice_operate_dates(
    dates: *mut u16,
    vernal_equinox_day: c_int,
    autumnal_equinox_day: c_int,
    dst_march_sunday: c_int,
    dst_october_sunday: c_int,
) {
    if dates.is_null() {
        return;
    }
    let d = unsafe { std::slice::from_raw_parts_mut(dates, NOTICE_DATA_LIST_NUM as usize) };
    d[6] = (vernal_equinox_day + 0x2F6) as u16;
    d[7] = (vernal_equinox_day + 0x2FB) as u16;
    d[8] = (vernal_equinox_day + 0x2FF) as u16;
    d[0x18] = (autumnal_equinox_day + 0x8F6) as u16;
    d[0x19] = (autumnal_equinox_day + 0x8FB) as u16;
    d[0x1A] = (autumnal_equinox_day + 0x8FF) as u16;
    d[0x29] = (dst_march_sunday + 0x300) as u16;
    d[0x2A] = (dst_october_sunday + 0x9F9) as u16;
}

/// `mNtc_sort_data_list` (`m_notice.c`): in-place selection sort
/// of the 43 (id, date) pairs by packed date, earliest first.
/// `0xFFFF` unused entries sink to the end by construction.
#[no_mangle]
pub extern "C" fn pc_notice_sort_pairs(ids: *mut c_uchar, dates: *mut u16) {
    if ids.is_null() || dates.is_null() {
        return;
    }
    let (ids, dates) = unsafe {
        (
            std::slice::from_raw_parts_mut(ids, NOTICE_DATA_LIST_NUM as usize),
            std::slice::from_raw_parts_mut(dates, NOTICE_DATA_LIST_NUM as usize),
        )
    };
    for i in 0..NOTICE_DATA_LIST_NUM as usize {
        let mut replace_idx = i;
        for j in (i + 1)..NOTICE_DATA_LIST_NUM as usize {
            if dates[j] < dates[replace_idx] {
                replace_idx = j;
            }
        }
        dates.swap(i, replace_idx);
        ids.swap(i, replace_idx);
    }
}

/// `mNtc_get_auto_nwrite_data_last_idx` (`m_notice.c`): last index
/// whose date is not `0xFFFF`.
#[no_mangle]
pub extern "C" fn pc_notice_last_valid_idx(dates: *const u16) -> c_int {
    if dates.is_null() {
        return NOTICE_DATA_LIST_NUM - 1;
    }
    let d = unsafe { std::slice::from_raw_parts(dates, NOTICE_DATA_LIST_NUM as usize) };
    let mut res = NOTICE_DATA_LIST_NUM - 1;
    for i in (0..NOTICE_DATA_LIST_NUM as usize).rev() {
        if d[i] != 0xFFFF {
            res = i as c_int;
            break;
        }
    }
    res
}

/// `mNtc_set_near_old_nwrite_data` (`m_notice.c`): walks the sorted
/// table from the end while the cursor date is at or before each
/// entry (with the `hour >= renew_hour` tiebreak). Returns
/// `(year_offset, index)`: `year_offset` 0 = cursor year,
/// -1 = cursor year - 1 (cursor ran off the front).
#[no_mangle]
pub extern "C" fn pc_notice_near_old_cursor(
    dates: *const u16,
    month: c_int,
    day: c_int,
    hour: c_int,
    renew_hour: c_int,
    out_year_offset: *mut c_int,
    out_index: *mut c_int,
) {
    if dates.is_null() {
        return;
    }
    let d = unsafe { std::slice::from_raw_parts(dates, NOTICE_DATA_LIST_NUM as usize) };
    let date = ((month << 8) + day) as u16;
    // mNtc_set_near_old_nwrite_data: walk from the end while the
    // cursor date is at or before each entry; last_idx tracks the
    // pointer, ending at -1 when the cursor runs off the front.
    let mut li = NOTICE_DATA_LIST_NUM - 1;
    let mut pp = NOTICE_DATA_LIST_NUM as usize - 1;
    loop {
        if date > d[pp] || (date == d[pp] && hour >= renew_hour) {
            break;
        }
        if pp == 0 {
            li = -1;
            break;
        }
        pp -= 1;
        li -= 1;
    }
    if !out_year_offset.is_null() {
        unsafe {
            *out_year_offset = if li != -1 { 0 } else { -1 };
        }
    }
    if !out_index.is_null() {
        unsafe {
            *out_index = if li != -1 {
                li
            } else {
                pc_notice_last_valid_idx(dates)
            };
        }
    }
}

/// `mNtc_decide_nwrite_data` (`m_notice.c`): selects up to 5 posts.
/// Fishing-day candidates are taken first (marked
/// `0xFFFF/0xFF` in the out arrays), then standard entries walk
/// the cursor backward, wrapping the year at index 0. Out arrays
/// are filled at the *decremented* index (reverse-indexed).
/// `fishing_ymds`/`fishing_monthdays` hold the upcoming fishing
/// days (year, packed monthday); `fishing_day` is the in/out
/// remaining count. Returns the remaining `write_count`.
#[no_mangle]
pub extern "C" fn pc_notice_decide(
    write_count: *mut c_int,
    nwrite_year: *mut c_int,
    nwrite_num: *mut c_int,
    now_nwrite_year: c_int,
    now_nwrite_num: c_int,
    out_years: *mut c_int,
    out_nums: *mut c_uchar,
    fishing_day: *mut c_int,
    fishing_term_date: u16,
    fishing_term_year: c_int,
    fishing_years: *const c_int,
    fishing_monthdays: *const u16,
    dates: *const u16,
) -> c_int {
    if write_count.is_null()
        || nwrite_year.is_null()
        || nwrite_num.is_null()
        || out_years.is_null()
        || out_nums.is_null()
        || fishing_day.is_null()
        || dates.is_null()
    {
        return 0;
    }
    let d = unsafe { std::slice::from_raw_parts(dates, NOTICE_DATA_LIST_NUM as usize) };
    let (fy, fm) = unsafe {
        (
            std::slice::from_raw_parts(fishing_years, *fishing_day as usize),
            std::slice::from_raw_parts(fishing_monthdays, *fishing_day as usize),
        )
    };
    let (wc, ny, nn, fd) = (write_count, nwrite_year, nwrite_num, fishing_day);
    while unsafe { *wc } != 0 {
        if unsafe { *fd } > 0 {
            let fi = unsafe { *fd } as usize - 1;
            if fi < fy.len()
                && (fy[fi] > fishing_term_year
                    || (fishing_term_year == fy[fi] && fm[fi] >= fishing_term_date))
            {
                if fy[fi] > unsafe { *ny }
                    || (unsafe { *ny } == fy[fi] && fm[fi] >= d[unsafe { *nn } as usize])
                {
                    unsafe {
                        *wc -= 1;
                        *fd -= 1;
                        *out_years.add(*wc as usize) = NOTICE_FISHING_DAY_YEAR;
                        *out_nums.add(*wc as usize) = NOTICE_FISHING_DAY_NUM as c_uchar;
                    }
                    continue;
                }
            }
        }
        let (nyv, nnv) = unsafe { (*ny, *nn) };
        if nyv > now_nwrite_year || (nyv == now_nwrite_year && nnv > now_nwrite_num) {
            unsafe {
                *wc -= 1;
                *out_years.add(*wc as usize) = nyv;
                *out_nums.add(*wc as usize) = nnv as c_uchar;
            }
            if nnv == 0 {
                unsafe {
                    *nn = pc_notice_last_valid_idx(dates);
                    *ny -= 1;
                }
            } else {
                unsafe {
                    *nn -= 1;
                }
            }
        } else {
            break;
        }
    }
    unsafe { *wc }
}

/// USA treasure gate (`mNtc_check_treasure`, `#else` branch,
/// `m_notice.c`): returns 1 when a buried treasure should be
/// generated. All save/RTC reads are done by the caller and
/// injected; `rng_draw` is the pre-drawn `fqrand()` value.
#[no_mangle]
pub extern "C" fn pc_notice_treasure_ok(
    hour: c_int,
    renew_hour: c_int,
    buried_clear: c_int,
    interval_days: c_int,
    checked_clear: c_int,
    checked_is_today: c_int,
    valid_animal_num: c_int,
    rng_draw: f32,
) -> c_int {
    if hour < renew_hour {
        return 0;
    }
    // interval via lbRTC_TIME_TO_U32 comparison + lbRTC_GetIntervalDays
    // (0 when equal); passes when buried time is clear or >= 3 days
    if buried_clear == 0 && interval_days < NOTICE_MIN_DAYS_BETWEEN_TREASURE {
        return 0;
    }
    if checked_clear == 0 && checked_is_today != 0 {
        return 0;
    }
    if valid_animal_num == 0 || rng_draw >= NOTICE_TREASURE_CHANCE {
        return 0;
    }
    1
}

/// Treasure item pick (`mNtc_check_treasure`): the same `fqrand()`
/// draw selects pitfall vs the list type. Returns 0 = pitfall,
/// 1 = lottery list, 2 = event list.
#[no_mangle]
pub extern "C" fn pc_notice_treasure_pick(rng: f32) -> c_int {
    if rng < 1.0 / 3.0 {
        0
    } else if rng < 2.0 / 3.0 {
        1
    } else {
        2
    }
}

#[cfg(test)]
/// Bulletin-board batch-trigger, board-write, and save-check kernels
/// (`m_notice.c`, `save_check_take.c_inc`).
///
/// Ported pure pieces: `mNtc_TO_MONTHDAY` date packing, the two handbill
/// number formulas, the used-prefix scan (`mNtc_notice_write_num`), the
/// overflow shift (`mNtc_notice_write`), `mNtc_ClearPage` semantics, and
/// the save-check byte predicate. Stays in C: the batch-trigger gating
/// (`mEv_CheckTitleDemo() == mEv_TITLEDEMO_NONE` and foreground field),
/// RTC reads, all `Common_Set`/`Save_Set` checkpoint writes (which run
/// AFTER the write decision even when nothing was posted), ROM handbill
/// loading, `mFR_fishmail()` mail delivery, treasure state, and the
/// editor's ownership of the house-message buffer.
///
/// House-message validation: NOTHING validates or normalizes
/// `homes[].haniwa.message` at save time. `save_check_take.c_inc` has only
/// four validators (board, weather, snowman, save-take) and no
/// `*save_data_check*` touches the field anywhere in the tree. The editor
/// constrains newly entered text (`mED_TYPE_HBOARD`), and `m_home.c`
/// rebuilds the default from four ROM strings, but no universal save-time
/// validator exists — do not add a Rust-side sanitizer for it.

/// Standard announcement handbill base (`mNtc_set_auto_nwrite`): the real
/// handbill is `0x1A4 + announcement_id`.
pub const NOTICE_HANDBILL_BASE: i32 = 0x1A4;
/// Fishing-day handbill base: the real handbill is
/// `0x242 + ((fishing_day_candidate.day / 7) & 3)`. The day is the
/// day-of-month of the fishing-day candidate ymd, not a post date.
pub const NOTICE_FISHING_HANDBILL_BASE: i32 = 0x242;
/// Auto-post timestamp hour (`mNtc_set_auto_nwrite`): sec/min are 0,
/// hour is 6; year/month/day/weekday are populated per post.
pub const NOTICE_POST_HOUR: i32 = 6;
/// Board capacity (`mNtc_BOARD_POST_COUNT`).
pub const NOTICE_BOARD_POST_COUNT: i32 = 15;
/// Saved post stride in bytes (`mNtc_board_post_c` is 0xC8: 192-byte
/// message at 0x00, 8-byte RTC time at 0xC0).
pub const NOTICE_BOARD_POST_STRIDE: usize = 0xC8;
/// Board message body length (`MAIL_BODY_LEN`).
pub const NOTICE_MAIL_BODY_LEN: usize = 192;
/// Rejected message bytes (`mFont_char_save_data_check`): control code
/// and message tag.
pub const NOTICE_FONT_CHAR_CONTROL_CODE: u8 = 127;
pub const NOTICE_FONT_CHAR_MESSAGE_TAG: u8 = 128;
/// House gyroid default message is rebuilt from this many ROM strings
/// (`HANIWA_MSG_LINES` in `m_home.c`); no save-time validator exists for
/// the 128-byte house message (see module docs).
pub const HANIWA_MSG_LINE_COUNT: i32 = 4;

/// `mNtc_TO_MONTHDAY(mo, day)`: packs month/day as `(month << 8) | day`.
#[no_mangle]
pub extern "C" fn pc_notice_monthday(month: c_int, day: c_int) -> c_int {
    ((((month & 0xFF) << 8) | (day & 0xFF)) as u16) as c_int
}

/// Standard announcement handbill number: `0x1A4 + announcement_id`.
#[no_mangle]
pub extern "C" fn pc_notice_handbill_no(announcement_id: c_int) -> c_int {
    NOTICE_HANDBILL_BASE + announcement_id
}

/// Fishing-day handbill number: `0x242 + ((post_day / 7) & 3)`, where
/// `post_day` is the day-of-month of the fishing-day candidate.
#[no_mangle]
pub extern "C" fn pc_notice_fishing_handbill_no(post_day: c_int) -> c_int {
    NOTICE_FISHING_HANDBILL_BASE + ((post_day / 7) & 3)
}

/// `mNtc_notice_write_num`: counts the used prefix of the board.
/// `is_clear` points at 15 bytes, one per post, nonzero when that post's
/// timestamp equals the RTC clear code (the C side computes each flag
/// with `lbRTC_IsEqualTime(..., lbRTC_CHECK_ALL)`, not a raw byte
/// compare). Returns the number of leading non-clear posts.
#[no_mangle]
pub extern "C" fn pc_notice_write_num(is_clear: *const u8) -> c_int {
    if is_clear.is_null() {
        return 0;
    }
    let flags = unsafe { std::slice::from_raw_parts(is_clear, NOTICE_BOARD_POST_COUNT as usize) };
    let mut n = 0;
    for &f in flags {
        if f != 0 {
            break;
        }
        n += 1;
    }
    n
}

/// `mNtc_notice_write` destination slot: the lowest unused slot, or the
/// last slot (14) when the board is full — the C side then shifts posts
/// down one (dropping the oldest) before writing there.
#[no_mangle]
pub extern "C" fn pc_notice_write_slot(write_num: c_int) -> c_int {
    if write_num >= NOTICE_BOARD_POST_COUNT {
        NOTICE_BOARD_POST_COUNT - 1
    } else {
        write_num
    }
}

/// `mNtc_notice_write` overflow shift: moves posts 1..15 down to 0..14
/// over a flat 15-entry board with stride 0xC8 (memmove semantics; the
/// caller then writes the new post into slot 14).
#[no_mangle]
pub extern "C" fn pc_notice_shift_posts(board: *mut u8) {
    if board.is_null() {
        return;
    }
    unsafe {
        let dst = board;
        let src = board.add(NOTICE_BOARD_POST_STRIDE);
        let n = NOTICE_BOARD_POST_STRIDE * (NOTICE_BOARD_POST_COUNT as usize - 1);
        std::ptr::copy(src, dst, n);
    }
}

/// `mNtc_ClearPage`: fills `message[..192]` with `CHAR_SPACE` and copies
/// the 8-byte RTC clear code into `time_out`. (The C side assigns the
/// clear-code struct; it does not zero the timestamp.)
#[no_mangle]
pub extern "C" fn pc_notice_clear_page(message: *mut u8, clear_time: *const u8, time_out: *mut u8) {
    if !message.is_null() {
        unsafe {
            std::slice::from_raw_parts_mut(message, NOTICE_MAIL_BODY_LEN).fill(32);
        }
    }
    if !clear_time.is_null() && !time_out.is_null() {
        unsafe {
            std::ptr::copy_nonoverlapping(clear_time, time_out, 8);
        }
    }
}

/// `mFont_char_save_data_check`: nonzero when a message byte is rejected
/// (`CHAR_CONTROL_CODE` = 127 or `CHAR_MESSAGE_TAG` = 128). Used by
/// `sChk_board_save_check` on each used board post's message bytes —
/// bulletin-board posts only, not the house message.
#[no_mangle]
pub extern "C" fn pc_notice_font_char_bad(c: u8) -> c_int {
    (c == NOTICE_FONT_CHAR_CONTROL_CODE || c == NOTICE_FONT_CHAR_MESSAGE_TAG) as c_int
}

mod tests {
    use super::*;

    extern "C" fn fake_width(_code: c_uchar, _flag: c_int) -> c_int {
        10
    }

    #[test]
    fn nav_priority() {
        let (mut mp, mut act) = (0, 0);
        // left has priority over right
        pc_notice_nav_step(3, 10, 1, 1, 0, 0, 0, 0, &mut mp, &mut act);
        assert_eq!((mp, act), (-1, 0));
        // at page 0, left is guarded -> right wins
        pc_notice_nav_step(0, 10, 1, 1, 0, 0, 0, 0, &mut mp, &mut act);
        assert_eq!((mp, act), (1, 0));
        // down jumps to slot 0
        pc_notice_nav_step(5, 10, 0, 0, 1, 0, 0, 0, &mut mp, &mut act);
        assert_eq!((mp, act), (-5, 0));
        // up jumps to newest
        pc_notice_nav_step(2, 10, 0, 0, 0, 1, 0, 0, &mut mp, &mut act);
        assert_eq!((mp, act), (7, 0));
        // A -> write
        pc_notice_nav_step(2, 10, 0, 0, 0, 0, 1, 0, &mut mp, &mut act);
        assert_eq!((mp, act), (0, 1));
        // B -> exit
        pc_notice_nav_step(2, 10, 0, 0, 0, 0, 0, 1, &mut mp, &mut act);
        assert_eq!((mp, act), (0, 2));
    }

    #[test]
    fn move_time_and_steps() {
        assert_eq!(pc_notice_move_time(7), 3);
        assert_eq!(pc_notice_move_time(-2), 2);
        assert_eq!(pc_notice_move_time(1), 1);
        // first step: now_page += move_page / move_time (C truncation)
        assert_eq!(pc_notice_first_step(2, 7, 3), 4); // 7/3 = 2
        assert_eq!(pc_notice_first_step(5, -5, 3), 4); // -5/3 = -1
        // later steps
        assert_eq!(pc_notice_later_step(4, 10, 1, 2), 4 + ((10 - 4) - 1) / 2);
        assert_eq!(pc_notice_later_step(4, 10, 0, 2), 4 - 4 / 2);
    }

    #[test]
    fn stick_classification() {
        assert_eq!(pc_notice_stick_area(0.1, 0), NOTICE_STICK_CENTER);
        assert_eq!(pc_notice_stick_area(0.2, 0), NOTICE_STICK_RIGHT); // threshold is strict <
        assert_eq!(pc_notice_stick_area(0.5, -0x7001), NOTICE_STICK_LEFT);
        assert_eq!(pc_notice_stick_area(0.5, -0x7000), NOTICE_STICK_BOTTOM_LEFT);
        assert_eq!(pc_notice_stick_area(0.5, 0x6FFF), NOTICE_STICK_TOP_LEFT);
        assert_eq!(pc_notice_stick_area(0.5, 0x7000), NOTICE_STICK_LEFT);
        assert_eq!(pc_notice_kei_tex(0), 5);
        assert_eq!(pc_notice_kei_tex(8), 1);
        assert_eq!(pc_notice_kei_tex(9), -1);
    }

    #[test]
    fn line_check_machine() {
        let text = [b'A', b'B', NOTICE_CHAR_NEW_LINE, b'C'];
        let (mut pos, mut width, mut line) = (0, 0, 0);
        assert_eq!(
            pc_notice_line_check(text.as_ptr(), 4, &mut pos, &mut width, &mut line, fake_width),
            NOTICE_LINE_OK
        );
        assert_eq!((pos, width, line), (1, 10, 1));
        // force overflow: width 190 + 10 > 192 -> OVERLINE, no consume
        width = 190;
        assert_eq!(
            pc_notice_line_check(text.as_ptr(), 4, &mut pos, &mut width, &mut line, fake_width),
            NOTICE_LINE_OVERLINE
        );
        assert_eq!((pos, width), (1, 190));
        // newline consumes
        pos = 2;
        assert_eq!(
            pc_notice_line_check(text.as_ptr(), 4, &mut pos, &mut width, &mut line, fake_width),
            NOTICE_LINE_NEWLINE
        );
        assert_eq!((pos, line), (3, 2));
        // past end -> OVERSTRING
        pos = 4;
        assert_eq!(
            pc_notice_line_check(text.as_ptr(), 4, &mut pos, &mut width, &mut line, fake_width),
            NOTICE_LINE_OVERSTRING
        );
    }

    #[test]
    fn page_no_and_month() {
        assert_eq!(pc_notice_page_no(3, 10), 4);
        assert_eq!(pc_notice_page_no(15, 10), 11);
        assert_eq!(pc_notice_page_no(15, 20), 15); // capped
        assert_eq!(pc_notice_month_idx(5), 5);
        assert_eq!(pc_notice_month_idx(0), 13);
        assert_eq!(pc_notice_month_idx(13), 13);
        assert_eq!(pc_notice_confirm(0), 0);
        assert_eq!(pc_notice_confirm(1), 1);
        assert_eq!(pc_notice_confirm(7), 2);
    }

    #[test]
    fn date_table_and_sort() {
        assert_eq!(NOTICE_DATE_TABLE.len(), 43);
        assert_eq!(NOTICE_DATE_TABLE[0], (0x00, 0x0101));
        assert_eq!(NOTICE_DATE_TABLE[42], (0x2A, 0x0A1F));
        let mut ids = [0u8; 43];
        let mut dates = [0u16; 43];
        for i in 0..43 {
            ids[i] = NOTICE_DATE_TABLE[i].0;
            dates[i] = NOTICE_DATE_TABLE[i].1;
        }
        // apply fake equinox/DST then sort
        let mut d = dates;
        pc_notice_operate_dates(
            d.as_mut_ptr(),
            20,   // vernal equinox March 20
            23,   // autumnal equinox Sept 23
            25,   // last Sunday of March (25th)
            28,   // last Sunday of October (28th)
        );
        // March entries: 20-10=10 -> 0x030A via 20+0x2F6 = 0x30A
        assert_eq!(d[6], 0x030A);
        assert_eq!(d[7], 0x030F);
        assert_eq!(d[8], 0x0313);
        // DST: 25+0x300 = 0x0319; 28+0x9F9: 0x9F9+28 = 0xA15 -> Oct 21
        assert_eq!(d[0x29], 0x0319);
        assert_eq!(d[0x2A], 0x0A15);
        pc_notice_sort_pairs(ids.as_mut_ptr(), d.as_mut_ptr());
        for w in d.windows(2) {
            assert!(w[0] <= w[1]);
        }
        // 0xFFFF entries sink to the end
        assert_eq!(d[41], 0xFFFF);
        assert_eq!(d[42], 0xFFFF);
        // earliest is still Jan 1
        assert_eq!((ids[0], d[0]), (0x00, 0x0101));
    }

    #[test]
    fn near_old_cursor_rules() {
        let mut ids = [0u8; 43];
        let mut dates = [0u16; 43];
        for i in 0..43 {
            ids[i] = NOTICE_DATE_TABLE[i].0;
            dates[i] = NOTICE_DATE_TABLE[i].1;
        }
        pc_notice_sort_pairs(ids.as_mut_ptr(), dates.as_mut_ptr());
        let (mut yo, mut idx) = (0, 0);
        // cursor Jan 10, 06:00, renew hour 6 -> latest entry <= Jan 10 is Jan 1 (idx 0)
        pc_notice_near_old_cursor(dates.as_ptr(), 1, 10, 6, 6, &mut yo, &mut idx);
        assert_eq!((yo, idx), (0, 0));
        // cursor exactly Jan 1 before renew hour -> runs off front -> year -1, last valid idx
        pc_notice_near_old_cursor(dates.as_ptr(), 1, 1, 5, 6, &mut yo, &mut idx);
        assert_eq!(yo, -1);
        assert_eq!(idx, 40); // last non-0xFFFF
        // cursor Dec 30 -> latest entry Dec 28 (idx 40 in sorted order)
        pc_notice_near_old_cursor(dates.as_ptr(), 12, 30, 12, 6, &mut yo, &mut idx);
        assert_eq!((yo, idx), (0, 40));
    }

    #[test]
    fn batch_trigger_kernels() {
        // mNtc_TO_MONTHDAY: (month << 8) | day, masked to bytes.
        assert_eq!(pc_notice_monthday(1, 1), 0x0101);
        assert_eq!(pc_notice_monthday(12, 25), 0x0C19);
        assert_eq!(pc_notice_monthday(0x1FF, 0x1FF), 0xFFFF);
        // Standard handbill: 0x1A4 + id.
        assert_eq!(pc_notice_handbill_no(0), 0x1A4);
        assert_eq!(pc_notice_handbill_no(0x27), 0x1CB);
        // Fishing handbill: 0x242 + ((day / 7) & 3).
        assert_eq!(pc_notice_fishing_handbill_no(1), 0x242);
        assert_eq!(pc_notice_fishing_handbill_no(7), 0x243);
        assert_eq!(pc_notice_fishing_handbill_no(25), 0x245);
        assert_eq!(pc_notice_fishing_handbill_no(30), 0x242);
        // write_num: leading non-clear count.
        let clear = [1u8; 15];
        assert_eq!(pc_notice_write_num(clear.as_ptr()), 0);
        let mut flags = [1u8; 15];
        flags[0] = 0;
        flags[1] = 0;
        flags[2] = 0;
        assert_eq!(pc_notice_write_num(flags.as_ptr()), 3);
        let used = [0u8; 15];
        assert_eq!(pc_notice_write_num(used.as_ptr()), 15);
        // write_slot: lowest unused, or last slot when full.
        assert_eq!(pc_notice_write_slot(0), 0);
        assert_eq!(pc_notice_write_slot(14), 14);
        assert_eq!(pc_notice_write_slot(15), 14);
        // shift: posts 1..15 move down to 0..14.
        let mut board = vec![0u8; 15 * NOTICE_BOARD_POST_STRIDE];
        for i in 0..15 {
            board[i * NOTICE_BOARD_POST_STRIDE] = i as u8;
        }
        pc_notice_shift_posts(board.as_mut_ptr());
        for i in 0..14 {
            assert_eq!(board[i * NOTICE_BOARD_POST_STRIDE], (i + 1) as u8);
        }
        // clear_page: message -> CHAR_SPACE, time <- clear code.
        let mut msg = [0xFFu8; 192];
        let clear_time = [9u8; 8];
        let mut time_out = [0u8; 8];
        pc_notice_clear_page(msg.as_mut_ptr(), clear_time.as_ptr(), time_out.as_mut_ptr());
        assert!(msg.iter().all(|&b| b == 32));
        assert_eq!(time_out, clear_time);
        // font_char_bad: only 127 and 128 rejected.
        assert_eq!(pc_notice_font_char_bad(127), 1);
        assert_eq!(pc_notice_font_char_bad(128), 1);
        assert_eq!(pc_notice_font_char_bad(32), 0);
        assert_eq!(pc_notice_font_char_bad(0), 0);
        assert_eq!(pc_notice_font_char_bad(205), 0);
    }

    #[test]
    fn treasure_gates() {
        // all pass
        assert_eq!(pc_notice_treasure_ok(6, 6, 1, 0, 1, 0, 3, 0.2), 1);
        // hour too early
        assert_eq!(pc_notice_treasure_ok(5, 6, 1, 0, 1, 0, 3, 0.2), 0);
        // buried 2 days ago
        assert_eq!(pc_notice_treasure_ok(6, 6, 0, 2, 1, 0, 3, 0.2), 0);
        // buried 3 days ago -> ok
        assert_eq!(pc_notice_treasure_ok(6, 6, 0, 3, 1, 0, 3, 0.2), 1);
        // checked today
        assert_eq!(pc_notice_treasure_ok(6, 6, 1, 0, 0, 1, 3, 0.2), 0);
        // no animals
        assert_eq!(pc_notice_treasure_ok(6, 6, 1, 0, 1, 0, 0, 0.2), 0);
        // rng too high
        assert_eq!(pc_notice_treasure_ok(6, 6, 1, 0, 1, 0, 3, 0.5), 0);
        // picks
        assert_eq!(pc_notice_treasure_pick(0.1), 0);
        assert_eq!(pc_notice_treasure_pick(0.5), 1);
        assert_eq!(pc_notice_treasure_pick(0.9), 2);
    }
}
