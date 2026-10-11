//! Former-villager visit ("go home" NPC, `ac_go_home_npc.c` + the
//! `mGH_*` helpers in `m_event.c`).
//!
//! When a villager moves out, `mNpc_SetReturnAnimal` (m_npc.c) records it
//! in `Save_Get(return_animal)` (`Anmret_c`: npc id, per-player talk
//! bits, `exist`, `renew_time` = the move-out date). The event manager's
//! `gohome_mask_start` later spawns `SP_NPC_GO_HONE_NPC` (0xD077, masked
//! as the old villager via `mNpc_RegistMaskNpc`) in a free acre if
//! `mGH_check_birth()` passes; the Mask Cat shares the same event slot
//! and wins on its own birthday (`mMC_check_birth_day`).
//!
//! Source-verified against ac-decomp `src/actor/npc/ac_go_home_npc.c`
//! (primary; PC port identical) and ACGC-PC-Port `src/game/m_event.c`
//! `mGH_check_birth{,2}` / `mGH_check_delete`, `src/lb_rtc.c`
//! (`lbRTC_Add_DD/MM/YY`, `lbRTC_IsEqualDate`), `src/game/m_font.c`
//! (`mFont_UnintToString`).
//!
//! ## Timing (retail semantics)
//! - **Visit window opens** once `renew_time + 7 days < today`
//!   (`mGH_check_birth2`). The decomp comment says "less than 1 week"
//!   but the code requires *more than* one week; the code is authoritative.
//! - **Per player**: `mGH_check_birth` is FALSE once this player's talk bit
//!   is set, i.e. each player meets the visitor at most once.
//! - **Record cleared** (`mGH_check_delete`) when `npc_force_go_home` is
//!   set, when `renew_time + 60 days >= today` stops being in the future
//!   (i.e. today >= move-out + 60; the +60 is done as 3x `lbRTC_Add_DD(20)`
//!   because `Add_DD` only rolls over one month), when the villager lives
//!   in town again, or when every existing player has talked to them.
//! - `lbRTC_Add_DD` rolls over at most one month (`days -= month_len`
//!   once), so callers keep each step <= 28 days.
//!
//! ## Talk (`ac_go_home_npc.c`)
//! - Message `MSG_12827 + looks*4` the first time; afterwards
//!   `+ 1 + RANDOM(3)` (one of three repeat lines). "First time" is the
//!   per-session `spnpc_first_talk_flags` bit 1 (`aNPC_SPNPC_BIT_GOHOME_NPC`),
//!   which also makes the constructor delete the actor if it respawns.
//! - `aGHN_talk_init` sets that bit and this player's `talk_bit`, and puts
//!   the day count since move-out into free string 0
//!   (`mFont_UnintToString(str, 2, n, n < 10 ? 1 : 2, left_cut)`), so
//!   counts >= 100 would show only the last two digits (unreachable, the
//!   record is cleared at 60 days).
//!
//! ## PC-port bug: `lbRTC_IsEqualDate` on little-endian
//! `lbRTC_IsEqualDate` compares dates through `union { int raw;
//! lbRTC_ymd_c ymd; }` (`{u16 year; u8 month; u8 day;}`). On big-endian
//! GameCube `raw = year<<16 | month<<8 | day`, a correct ordering. On the
//! MSYS2 mingw32 (i686, little-endian) build `raw = day<<24 | month<<16 |
//! year`, so dates compare **day first**. Reproduced with mingw32 gcc:
//! 2002-01-20 vs 2002-02-05 returns OVER (retail LESS). This skews the
//! visit window/deletion here and every other caller (19 call sites:
//! turnip prices `m_kabu_manager.c`, fish records, bridge, Tortimer's
//! `m_soncho.c`, `lbRTC_GetIntervalDays2`). `date_cmp_retail` is the
//! correct ordering; `date_cmp_pc_le` reproduces the shipped PC behaviour
//! for comparison. Not fixed on the C side by this module.

use std::cmp::Ordering;
use std::ffi::c_int;

/// `MSG_12827`: first go-home greeting for looks 0.
pub const MSG_GOHOME_BASE: c_int = 12827;
/// `aNPC_SPNPC_BIT_GOHOME_NPC`.
pub const SPNPC_BIT_GOHOME_NPC: u32 = 1;
/// `SP_NPC_GO_HONE_NPC` (sic).
pub const SP_NPC_GO_HONE_NPC: u16 = 0xD077;
/// `EMPTY_NO`.
pub const EMPTY_NO: u16 = 0;
/// `lbRTC_WEEK`.
pub const VISIT_DELAY_DAYS: i32 = 7;
/// Total record lifetime (3 x 20 days).
pub const RECORD_LIFETIME_STEPS: [i32; 3] = [20, 20, 20];

/// `lbRTC_IsEqualDate` result (`lbRTC_LESS/EQUAL/OVER`).
pub const RTC_LESS: c_int = -1;
pub const RTC_EQUAL: c_int = 0;
pub const RTC_OVER: c_int = 1;

/// A calendar date (`lbRTC_ymd_c` fields).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Ymd {
    pub year: u16,
    pub month: u8,
    pub day: u8,
}

impl Ymd {
    pub const fn new(year: u16, month: u8, day: u8) -> Self {
        Self { year, month, day }
    }
}

const fn is_leap_year(year: u16) -> bool {
    (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400)
}

/// `lbRTC_GetDaysByMonth` (0 for an invalid month, like the table).
pub fn days_by_month(year: u16, month: u8) -> u8 {
    const DAYS: [[u8; 13]; 2] = [
        [0, 31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31],
        [0, 31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31],
    ];
    DAYS[is_leap_year(year) as usize].get(month as usize).copied().unwrap_or(0)
}

/// `lbRTC_Add_MM`: month carry into years via `/12`, `%12`.
pub fn add_months(d: &mut Ymd, months: i32) {
    let mut m = d.month as i32 + months;
    if m > 12 {
        d.year = d.year.wrapping_add((m / 12) as u16);
        m %= 12;
    }
    d.month = m as u8;
}

/// `lbRTC_Add_DD`: single-rollover day add (faithful, see module docs).
pub fn add_days_once(d: &mut Ymd, days: i32) {
    let month_days = days_by_month(d.year, d.month) as i32;
    let mut n = d.day as i32 + days;
    if n > month_days {
        n -= month_days;
        add_months(d, 1);
    }
    d.day = n as u8;
}

/// Retail (big-endian) `lbRTC_IsEqualDate`: year, month, day order.
pub fn date_cmp_retail(a: Ymd, b: Ymd) -> Ordering {
    (a.year, a.month, a.day).cmp(&(b.year, b.month, b.day))
}

/// What the PC build (little-endian union) actually computes.
pub fn date_cmp_pc_le(a: Ymd, b: Ymd) -> Ordering {
    let raw = |d: Ymd| -> i32 { (d.year as u32 | (d.month as u32) << 16 | (d.day as u32) << 24) as i32 };
    // C computes raw0 - raw1 in int (wrapping) and tests the sign.
    raw(a).wrapping_sub(raw(b)).cmp(&0)
}

fn ord_to_rtc(o: Ordering) -> c_int {
    match o {
        Ordering::Less => RTC_LESS,
        Ordering::Equal => RTC_EQUAL,
        Ordering::Greater => RTC_OVER,
    }
}

/// `mGH_check_birth2`: the visit window is open.
pub fn check_birth2(npc_id: u16, renew: Ymd, today: Ymd) -> bool {
    if npc_id == EMPTY_NO {
        return false;
    }
    let mut t = renew;
    add_days_once(&mut t, VISIT_DELAY_DAYS);
    date_cmp_retail(t, today) == Ordering::Less
}

/// `mGH_check_birth`: window open and this player has not talked yet.
pub fn check_birth(npc_id: u16, talk_bit: u8, player_no: u32, renew: Ymd, today: Ymd) -> bool {
    if player_no < 8 && talk_bit & (1 << player_no) != 0 {
        return false;
    }
    check_birth2(npc_id, renew, today)
}

/// `mGH_check_delete` decision: `true` means `mGH_animal_return_init()`.
/// `players_present` bit i = player slot i exists
/// (`mPr_NullCheckPersonalID == FALSE`); 4 players.
pub fn should_clear_record(
    npc_id: u16,
    force_go_home: bool,
    renew: Ymd,
    today: Ymd,
    villager_back_in_town: bool,
    talk_bit: u8,
    players_present: u8,
) -> bool {
    if npc_id == EMPTY_NO {
        return false;
    }
    if force_go_home {
        return true;
    }
    let mut t = renew;
    for step in RECORD_LIFETIME_STEPS {
        add_days_once(&mut t, step);
    }
    if date_cmp_retail(t, today) != Ordering::Greater {
        return true;
    }
    if villager_back_in_town {
        // C clears here and then keeps looping; the outcome is cleared
        // either way.
        return true;
    }
    for i in 0..4 {
        if players_present & (1 << i) != 0 && talk_bit & (1 << i) == 0 {
            return false;
        }
    }
    true
}

/// `aGHN_set_talk_info`: message number. `repeat_roll` is `RANDOM(3)`
/// and only used once this NPC has been talked to this session.
pub fn talk_msg_no(looks: c_int, talked_before: bool, repeat_roll: c_int) -> c_int {
    let mut msg = MSG_GOHOME_BASE + looks * 4;
    if talked_before {
        msg += 1 + repeat_roll;
    }
    msg
}

/// `aGHN_set_free_str` digits: (ASCII digits, length). Mirrors
/// `mFont_UnintToString(str, 2, num, fig, TRUE, FALSE, FALSE)`.
pub fn days_away_str(num: u32) -> ([u8; 2], usize) {
    if num < 10 {
        ([b'0' + num as u8, b' '], 1)
    } else {
        ([b'0' + ((num / 10) % 10) as u8, b'0' + (num % 10) as u8], 2)
    }
}

// ---------------------------------------------------------------------------
// C ABI
// ---------------------------------------------------------------------------

/// Correct (big-endian semantics) `lbRTC_IsEqualDate`, a drop-in for the
/// little-endian bug described above.
#[no_mangle]
pub extern "C" fn pc_lbrtc_is_equal_date(y0: u16, m0: u8, d0: u8, y1: u16, m1: u8, d1: u8) -> c_int {
    ord_to_rtc(date_cmp_retail(Ymd::new(y0, m0, d0), Ymd::new(y1, m1, d1)))
}

#[no_mangle]
pub extern "C" fn pc_gohome_check_birth(
    npc_id: u16,
    talk_bit: u8,
    player_no: c_int,
    renew_y: u16,
    renew_m: u8,
    renew_d: u8,
    now_y: u16,
    now_m: u8,
    now_d: u8,
) -> c_int {
    check_birth(
        npc_id,
        talk_bit,
        player_no as u32,
        Ymd::new(renew_y, renew_m, renew_d),
        Ymd::new(now_y, now_m, now_d),
    ) as c_int
}

#[no_mangle]
pub extern "C" fn pc_gohome_talk_msg_no(looks: c_int, talked_before: c_int, repeat_roll: c_int) -> c_int {
    talk_msg_no(looks, talked_before != 0, repeat_roll)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pc_le_compare_is_day_major() {
        let a = Ymd::new(2002, 1, 20);
        let b = Ymd::new(2002, 2, 5);
        assert_eq!(date_cmp_retail(a, b), Ordering::Less);
        assert_eq!(date_cmp_pc_le(a, b), Ordering::Greater);
        assert_eq!(date_cmp_pc_le(Ymd::new(2001, 12, 31), Ymd::new(2002, 1, 1)), Ordering::Greater);
    }

    #[test]
    fn add_days_single_rollover() {
        let mut d = Ymd::new(2001, 12, 28);
        add_days_once(&mut d, 7);
        assert_eq!(d, Ymd::new(2002, 1, 4));
        // Feb 2004 (leap): 20 + 20 -> rolls once.
        let mut d = Ymd::new(2004, 2, 15);
        add_days_once(&mut d, 20);
        assert_eq!(d, Ymd::new(2004, 3, 6));
    }

    #[test]
    fn visit_window_needs_more_than_a_week() {
        let left = Ymd::new(2002, 3, 1);
        assert!(!check_birth2(0x1234, left, Ymd::new(2002, 3, 8)));
        assert!(check_birth2(0x1234, left, Ymd::new(2002, 3, 9)));
        assert!(!check_birth(0x1234, 0b0010, 1, left, Ymd::new(2002, 3, 9)));
        assert!(!check_birth2(EMPTY_NO, left, Ymd::new(2003, 1, 1)));
    }

    #[test]
    fn record_clears_after_60_days_or_all_talked() {
        let left = Ymd::new(2002, 3, 1);
        // 2002-03-01 + 60 = 2002-04-30.
        assert!(!should_clear_record(1, false, left, Ymd::new(2002, 4, 29), false, 0, 0b0001));
        assert!(should_clear_record(1, false, left, Ymd::new(2002, 4, 30), false, 0, 0b0001));
        assert!(should_clear_record(1, false, left, Ymd::new(2002, 3, 10), false, 0b0001, 0b0001));
        assert!(!should_clear_record(1, false, left, Ymd::new(2002, 3, 10), false, 0b0001, 0b0011));
        assert!(should_clear_record(1, false, left, Ymd::new(2002, 3, 10), true, 0, 0b0001));
        assert!(should_clear_record(1, true, left, left, false, 0, 0b0001));
    }

    #[test]
    fn talk_and_digits() {
        assert_eq!(talk_msg_no(2, false, 2), 12835);
        assert_eq!(talk_msg_no(2, true, 2), 12838);
        assert_eq!(days_away_str(8), ([b'8', b' '], 1));
        assert_eq!(days_away_str(42), ([b'4', b'2'], 2));
    }
}
