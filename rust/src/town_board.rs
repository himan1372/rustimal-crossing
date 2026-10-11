//! Town bulletin board: the persistent 15-slot dated
//! board (`m_notice.c`, `m_notice.h`,
//! `m_notice_ovl.c`).
//!
//! Verified against GAFE01_00 Rev 0 USA (see
//! `board-verification-report.md`; 8/8 sections
//! PROVEN).
//!
//! Key resolution: the real town-board writer is
//! `m_notice_ovl.c`, not `m_board_ovl.c` — the
//! latter is the *general letter-editing overlay*
//! (also opened by quest talk, mailbox, and the tag
//! overlay). Button A in `mNT_Play_page_read`
//! stamps the current RTC time into the post at
//! editor *open*; on confirmation `mNT_move_Obey`
//! calls `mNtc_notice_write` and then
//! `mNT_finish_notice_first_job`.
//!
//! This module ports the pure record kernels: the
//! first-empty-slot scan, the full-board shift,
//! page clearing, the initial handbill ids, the
//! automatic-post time, and the handbill-id
//! formulas. RTC comparison and save ownership,
//! the editor and submenu flows, handbill ROM
//! loading, font rendering, the mail/letter
//! overlay, and the automatic-schedule date
//! selection stay in C.
//!
//! Insertion is FIFO by array position, not a
//! timestamp sort: the oldest entry is defined
//! operationally by being at the start of the
//! array.

/// Record count.
pub const BOARD_POST_COUNT: i32 = 15;
/// Message buffer length (`MAIL_BODY_LEN`).
pub const BOARD_MSG_LEN: usize = 192;
/// Record size: 192 message bytes + 8-byte
/// `lbRTC_time_c` at 0xC0.
pub const BOARD_RECORD_SIZE: usize = 200;
/// Timestamp offset within a record.
pub const BOARD_TIME_OFS: usize = 192;
/// Timestamp size.
pub const BOARD_TIME_SIZE: usize = 8;
/// Space fill for cleared messages.
pub const CHAR_SPACE: u8 = 32;
/// Initial handbill ids.
pub const INIT_HANDBILLS: [i32; 4] = [0x1E, 0x1F, 0x20, 0x21];
/// Standard announcement handbill base.
pub const AUTO_HANDBILL_BASE: i32 = 0x1A4;
/// Fishing-day handbill base.
pub const FISHING_HANDBILL_BASE: i32 = 0x242;

/// First-empty-slot scan (`mNtc_notice_write_num`):
/// `is_empty` points at 15 bytes (1 = the slot's
/// `post_time` equals the RTC clear code — the
/// compare itself stays in C). Returns the lowest
/// empty index, or 15 when full.
#[no_mangle]
pub extern "C" fn pc_board_first_empty(is_empty: *const u8) -> i32 {
    if is_empty.is_null() {
        return BOARD_POST_COUNT;
    }
    let flags = unsafe { core::slice::from_raw_parts(is_empty, BOARD_POST_COUNT as usize) };
    for (i, &f) in flags.iter().enumerate() {
        if f != 0 {
            return i as i32;
        }
    }
    BOARD_POST_COUNT
}

/// Full-board eviction (`mNtc_notice_write` when
/// full): records 1..14 shift down to 0..13; the
/// caller then copies the new record into slot 14.
/// `board` points at 15 × 200 bytes; the move is
/// overlap-safe. Whole records move — timestamp
/// and message travel together.
#[no_mangle]
pub extern "C" fn pc_board_shift_full(board: *mut u8) {
    if board.is_null() {
        return;
    }
    let total = (BOARD_POST_COUNT as usize) * BOARD_RECORD_SIZE;
    let b = unsafe { core::slice::from_raw_parts_mut(board, total) };
    b.copy_within(BOARD_RECORD_SIZE..total, 0);
}

/// Page clear (`mNtc_ClearPage`): the timestamp
/// becomes the RTC clear code (`clear_time` points
/// at 8 bytes) and the message is filled with
/// `CHAR_SPACE` — not zeroed.
#[no_mangle]
pub extern "C" fn pc_board_clear_page(record: *mut u8, clear_time: *const u8) {
    if record.is_null() || clear_time.is_null() {
        return;
    }
    let r = unsafe { core::slice::from_raw_parts_mut(record, BOARD_RECORD_SIZE) };
    let t = unsafe { core::slice::from_raw_parts(clear_time, BOARD_TIME_SIZE) };
    r[..BOARD_MSG_LEN].fill(CHAR_SPACE);
    r[BOARD_TIME_OFS..BOARD_TIME_OFS + BOARD_TIME_SIZE].copy_from_slice(t);
}

/// Initial handbill id for slots 0–3; −1 for other
/// slots (which are cleared instead).
#[no_mangle]
pub extern "C" fn pc_board_init_handbill(slot: i32) -> i32 {
    if (0..4).contains(&slot) {
        INIT_HANDBILLS[slot as usize]
    } else {
        -1
    }
}

/// Automatic-post time: sec = 0, min = 0, hour = 6.
/// The date fields are filled by the caller. Writes
/// (sec, min, hour) into `out[3]`.
#[no_mangle]
pub extern "C" fn pc_board_auto_post_time(out: *mut u8) {
    if !out.is_null() {
        unsafe {
            *out = 0;
            *out.add(1) = 0;
            *out.add(2) = 6;
        }
    }
}

/// Standard announcement handbill: `0x1A4 + id`.
#[no_mangle]
pub extern "C" fn pc_board_standard_handbill_id(id: i32) -> i32 {
    AUTO_HANDBILL_BASE + id
}

/// Fishing-day handbill: `0x242 + ((day / 7) & 3)`.
#[no_mangle]
pub extern "C" fn pc_board_fishing_handbill_id(day: i32) -> i32 {
    FISHING_HANDBILL_BASE + ((day / 7) & 3)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insertion_and_eviction() {
        let mut empty = [1u8; 15];
        assert_eq!(pc_board_first_empty(empty.as_ptr()), 0);
        empty[0] = 0;
        empty[1] = 0;
        assert_eq!(pc_board_first_empty(empty.as_ptr()), 2);
        let full = [0u8; 15];
        assert_eq!(pc_board_first_empty(full.as_ptr()), 15);
        assert_eq!(pc_board_first_empty(core::ptr::null()), 15);
        // Shift: records 1..14 → 0..13.
        let mut board = [0u8; 15 * 200];
        for i in 0..15 {
            board[i * 200] = i as u8;
        }
        pc_board_shift_full(board.as_mut_ptr());
        for i in 0..14 {
            assert_eq!(board[i * 200], (i + 1) as u8);
        }
        // Whole-record copy: a timestamp byte travels too.
        board[199] = 0xAA;
        pc_board_shift_full(board.as_mut_ptr());
        assert_eq!(board[14 * 200 + 199 - 200], 0xAA);
    }

    #[test]
    fn clearing_and_init() {
        let mut rec = [0xFFu8; 200];
        let clear = [0x11u8; 8];
        pc_board_clear_page(rec.as_mut_ptr(), clear.as_ptr());
        assert!(rec[..192].iter().all(|&b| b == 32)); // CHAR_SPACE, not zeroed
        assert_eq!(&rec[192..200], &clear);
        assert_eq!(pc_board_init_handbill(0), 0x1E);
        assert_eq!(pc_board_init_handbill(3), 0x21);
        assert_eq!(pc_board_init_handbill(4), -1);
        assert_eq!(pc_board_init_handbill(-1), -1);
        // Auto-post time.
        let mut t = [0u8; 3];
        pc_board_auto_post_time(t.as_mut_ptr());
        assert_eq!(t, [0, 0, 6]);
        assert_eq!(pc_board_standard_handbill_id(3), 0x1A7);
        assert_eq!(pc_board_fishing_handbill_id(0), 0x242);
        assert_eq!(pc_board_fishing_handbill_id(7), 0x243);
        assert_eq!(pc_board_fishing_handbill_id(28), 0x242); // (28/7)&3 = 0
    }
}
