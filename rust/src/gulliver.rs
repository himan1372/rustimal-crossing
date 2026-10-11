//! Gulliver the seagull (internal name Dozaemon) — pure kernels.
//!
//! Verified against `src/game/m_event.c`, `src/game/m_event_schedule.c_inc`,
//! `src/actor/ac_event_manager.c`, `src/actor/npc/event/ac_ev_dozaemon_move.c_inc`,
//! `include/m_name_table.h`, `include/m_event.h`, `include/ac_ev_dozaemon.h`,
//! `include/lb_rtc.h` (GAFE01_00 Rev 0 USA decomp). 9 of 10 brief claims
//! checked verbatim; one correction: the WAKEUP save flag routes the
//! schedule proc to `aEDZ_THINK_WANDER`, it does NOT delete the actor — the
//! only deletion in `aEDZ_actor_ct` is `mv_proc != NULL && dozaemon_completed`.
//!
//! Architecture: the weekly scheduler (`init_weekly_event`), event
//! placement (`make_actor_in_seaside_block`), the actor lifecycle and
//! thought/talk state machine, dialogue, the furniture selector, the
//! inventory grant, and all save mutations stay in C. This module holds
//! only the source-proven pure kernels with primitive arguments.
//!
//! Note: the decomp's `after_n_day` is pure packed month/day math, but its
//! `last_day_of_month` helper reads the RTC year for the leap-year check —
//! so the year is passed in as an argument to keep these kernels pure.

/// Identity (`include/m_name_table.h`).
pub mod id {
    /// Gulliver's event identity (`SP_NPC_START + 100`).
    pub const EV_DOZAEMON: u16 = 0xD064;
    /// Boat-travel castaway identity (`SP_NPC_START + 123`) — a different
    /// actor gated by Gulliver's event state. Do not merge with EV_DOZAEMON.
    pub const CASTAWAY: u16 = 0xD07B;
    /// Bit in `spnpc_first_talk_flags` (`aNPC_SPNPC_BIT_DOZAEMON`).
    pub const FIRST_TALK_BIT: u8 = 4;
}

/// Save flags (`include/ac_ev_dozaemon.h`). The event struct itself
/// (`mEv_dozaemon_c`) is just `{ u16 flags; }` and stays in C.
pub mod save_flag {
    pub const WAKEUP: u16 = 1 << 0;
    pub const GIVE: u16 = 1 << 1;
    pub const GAVE_FIRST: u16 = 1 << 2;
}

/// Event active window (`m_event_schedule.c_inc`): 06:00–22:00 on the
/// saved weekly date. Not a complete spawn predicate — placement and
/// actor-init checks are separate stages in C.
pub mod window {
    pub const START_HOUR: u8 = 6;
    pub const END_HOUR: u8 = 22;
}

/// Packed month/day layout: `raw = (month << 8) | day`
/// (little-endian `mEv_MonthDay_u` on the PC build).
pub fn month_day_split(raw: u16) -> (u8, u8) {
    ((raw >> 8) as u8, raw as u8)
}

pub fn month_day_pack(month: u8, day: u8) -> u16 {
    ((month as u16) << 8) | day as u16
}

/// `lbRTC_IS_LEAPYEAR`, verbatim.
pub fn is_leapyear(year: u16) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

/// `last_day_of_month` from `m_event.c`, with the RTC year passed in
/// (the decomp reads `Common_Get(time.rtc_time.year)` here).
pub fn last_day_of_month(month: u8, year: u16) -> u8 {
    const LAST_DAY: [u8; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let day = LAST_DAY[(month - 1) as usize];
    if month != 2 {
        day
    } else if is_leapyear(year) {
        29
    } else {
        28
    }
}

/// `after_n_day` from `m_event.c`, verbatim semantics: pure packed
/// month/day arithmetic. The u8 assignments wrap exactly like C
/// (negative `n_day` lands via two's-complement, then the
/// `(day & 0x80) != 0 || day == 0` check rolls the month back).
pub fn after_n_day(month_day: u16, n_day: i32, year: u16) -> u16 {
    let (mut month, mut day) = month_day_split(month_day);

    if n_day >= 0 {
        let last = last_day_of_month(month, year);
        day = (day as i32 + n_day) as u8;
        if day > last {
            month = if month == 12 { 1 } else { month + 1 };
            day = day.wrapping_sub(last);
        }
    } else {
        day = (day as i32 + n_day) as u8;
        if (day & 0x80) != 0 || day == 0 {
            month = if month == 1 { 12 } else { month - 1 };
            let last = last_day_of_month(month, year);
            day = day.wrapping_add(last);
        }
    }

    month_day_pack(month, day)
}

/// Weekly date offset (`init_weekly_event`): `1 + (today_date.raw + hour) % 5`.
/// No RNG — deterministic in packed month/day + hour. Range 1–5.
pub fn weekly_ofs(today_raw: u16, hour: u8) -> u8 {
    (1 + (today_raw as u32 + hour as u32) % 5) as u8
}

/// Normal-talk message: `RANDOM_F(1.0f) < 0.35f` (strict) → 0x23F8 and the
/// wake-up continuation; otherwise `0x23F9 + RANDOM(5)`. Rolls stay in C
/// and are passed in.
pub fn norm_talk_msg(wakeup_roll: f32, rand5: u8) -> u16 {
    if wakeup_roll < 0.35f {
        0x23F8
    } else {
        0x23F9 + (rand5 % 5) as u16
    }
}

/// Forced-talk message: `0x23FE + RANDOM(6)`. The WAKEUP flag is set by the
/// C callback, not here.
pub fn force_talk_msg(rand6: u8) -> u16 {
    0x23FE + (rand6 % 6) as u16
}

/// First wake-up continuation (`aEDZ_to_ageru`): 0x2404 with an eligible
/// item (sets GAVE_FIRST in C), 0x2408 without. Uses
/// `mMsg_SET_CONTINUE_MSG_NUM` in the decomp.
pub fn ageru_msg(has_items: bool) -> u16 {
    if has_items {
        0x2404
    } else {
        0x2408
    }
}

/// Later wake-up continuation (`aEDZ_to_ageru2`): 0x2405 with an eligible
/// item (clears GAVE_FIRST in C), 0x2409 without. Uses `mDemo_Set_msg_num`
/// — a different macro than `aEDZ_to_ageru`; preserve the distinction in C.
pub fn ageru2_msg(has_items: bool) -> u16 {
    if has_items {
        0x2405
    } else {
        0x2409
    }
}

/// Post-give message (`aEDZ_talk_give`): 0x2406 if GAVE_FIRST is set,
/// 0x2407 otherwise. `dozaemon_completed` is written by the C callback.
pub fn give_msg(gave_first: bool) -> u16 {
    if gave_first {
        0x2406
    } else {
        0x2407
    }
}

/// Wandering talk once the reward was given: `0x240A + RANDOM(6)`.
pub fn wander_msg(rand6: u8) -> u16 {
    0x240A + (rand6 % 6) as u16
}

// ---- C ABI ----
// All pc_-prefixed: the C originals stay compiled; these are staged kernels.

#[no_mangle]
pub extern "C" fn pc_gulliver_after_n_day(month_day: u16, n_day: i32, year: u16) -> u16 {
    after_n_day(month_day, n_day, year)
}

#[no_mangle]
pub extern "C" fn pc_gulliver_weekly_ofs(today_raw: u16, hour: u8) -> u8 {
    weekly_ofs(today_raw, hour)
}

#[no_mangle]
pub extern "C" fn pc_gulliver_norm_talk_msg(wakeup_roll: f32, rand5: u8) -> u16 {
    norm_talk_msg(wakeup_roll, rand5)
}

#[no_mangle]
pub extern "C" fn pc_gulliver_force_talk_msg(rand6: u8) -> u16 {
    force_talk_msg(rand6)
}

#[no_mangle]
pub extern "C" fn pc_gulliver_ageru_msg(has_items: u8) -> u16 {
    ageru_msg(has_items != 0)
}

#[no_mangle]
pub extern "C" fn pc_gulliver_ageru2_msg(has_items: u8) -> u16 {
    ageru2_msg(has_items != 0)
}

#[no_mangle]
pub extern "C" fn pc_gulliver_give_msg(gave_first: u8) -> u16 {
    give_msg(gave_first != 0)
}

#[no_mangle]
pub extern "C" fn pc_gulliver_wander_msg(rand6: u8) -> u16 {
    wander_msg(rand6)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity() {
        assert_eq!(id::EV_DOZAEMON, 0xD064);
        assert_eq!(id::CASTAWAY, 0xD07B);
        assert_eq!(id::FIRST_TALK_BIT, 4);
    }

    #[test]
    fn packed_layout() {
        // March 5 -> raw (3<<8)|5
        assert_eq!(month_day_pack(3, 5), 0x0305);
        assert_eq!(month_day_split(0x0305), (3, 5));
    }

    #[test]
    fn after_n_day_forward() {
        // March 5 + 3 = March 8
        assert_eq!(after_n_day(0x0305, 3, 2002), 0x0308);
        // Jan 30 + 5 = Feb 4 (non-leap)
        assert_eq!(after_n_day(0x011E, 5, 2002), 0x0204);
        // Dec 30 + 5 = Jan 4 (year wrap, month only)
        assert_eq!(after_n_day(0x0C1E, 5, 2002), 0x0104);
    }

    #[test]
    fn after_n_day_backward() {
        // March 5 - 10 = Feb 23
        assert_eq!(after_n_day(0x0305, -10, 2002), 0x0217);
        // Jan 3 - 5 = Dec 29
        assert_eq!(after_n_day(0x0103, -5, 2002), 0x0C1D);
    }

    #[test]
    fn after_n_day_leap() {
        // Feb 28 + 1 in leap year = Feb 29; non-leap = Mar 1
        assert_eq!(after_n_day(0x021C, 1, 2004), 0x021D);
        assert_eq!(after_n_day(0x021C, 1, 2002), 0x0301);
    }

    #[test]
    fn weekly_ofs_range() {
        for raw in [0x0101u16, 0x0305, 0x0C1F] {
            for hour in 0..24u8 {
                let o = weekly_ofs(raw, hour);
                assert!((1..=5).contains(&o));
            }
        }
    }

    #[test]
    fn talk_messages() {
        assert_eq!(norm_talk_msg(0.1, 0), 0x23F8);
        assert_eq!(norm_talk_msg(0.34999, 3), 0x23F8);
        assert_eq!(norm_talk_msg(0.35, 3), 0x23F9 + 3);
        assert_eq!(norm_talk_msg(0.9, 4), 0x23FD);
        assert_eq!(force_talk_msg(0), 0x23FE);
        assert_eq!(force_talk_msg(5), 0x2403);
        assert_eq!(ageru_msg(true), 0x2404);
        assert_eq!(ageru_msg(false), 0x2408);
        assert_eq!(ageru2_msg(true), 0x2405);
        assert_eq!(ageru2_msg(false), 0x2409);
        assert_eq!(give_msg(true), 0x2406);
        assert_eq!(give_msg(false), 0x2407);
        assert_eq!(wander_msg(2), 0x240C);
    }
}
