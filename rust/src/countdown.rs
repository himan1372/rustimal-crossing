//! New Year countdown event actors.
//!
//! Verified against `src/actor/ac_count02.c`,
//! `src/actor/ac_count02_move.c_inc`,
//! `src/actor/ac_countdown.c`,
//! `src/actor/ac_countdown_move.c_inc`,
//! `include/ac_countdown_clip.h`,
//! `src/actor/npc/ac_countdown_npc0.c`,
//! `src/actor/npc/ac_countdown_npc0_talk.c_inc`,
//! `src/actor/npc/ac_countdown_npc1.c`,
//! `src/actor/npc/ac_countdown_npc1_talk.c_inc`,
//! `src/game/m_event_map_npc_data.c_inc`, `include/m_time.h`
//! (GAFE01_00 Rev 0 USA). All 10 brief claims PROVEN.
//!
//! Scope split: the countdown NPC message-number selection is
//! already ported in the sibling `talk_request` module. This
//! module owns the countdown timer arithmetic, the phase
//! thresholds, the digit extraction and change mask, the seven
//! layout tables, the prop/display clip handshake, the prop
//! action states, and the NPC1 movement and firing sequence.
//! The NPC talk/demo logic, the digit-transition animation (it
//! runs in the DRAW callback, not the movement callback), the
//! cracker tool actor, and the save stay in C.
//!
//! Nuances carried: `aCOU_setNowTime` does NOT check the day —
//! don't "fix" it to require Dec 31; the phase comparisons are
//! strict `>`/`<`; NPC1's term function is simpler than NPC0's
//! (no AFTER_10_SEC branch); the recalculation gate is
//! `arg1_f <= 0.0f`; the range check runs BEFORE the block check
//! in the NPC1 movement revision; layout 1 really does put
//! villager 4 and Tortimer both at (2, 11).

pub const DECEMBER: u8 = 12;
pub const SECONDS_IN_MINUTE: i32 = 60;
pub const MINUTES_IN_HOUR: i32 = 60;
pub const SECONDS_IN_HOUR: i32 = 3600;
pub const SECONDS_IN_DAY: i32 = 86400;

/// `mTM_TIME2SEC(h, m, s)`.
pub const fn time2sec(h: i32, m: i32, s: i32) -> i32 {
    h * SECONDS_IN_HOUR + m * SECONDS_IN_MINUTE + s
}

/// `aCOU_setNowTime`, verbatim: December && now_sec >= 23:00 →
/// seconds until midnight; non-December → 0; December before
/// 23:00 → 3599. Note: the day is NOT checked.
pub fn set_now_time(month: u8, now_sec: i32) -> i32 {
    if month == DECEMBER && now_sec >= time2sec(23, 0, 0) {
        time2sec(24, 0, 0) - now_sec
    } else if month != DECEMBER {
        0
    } else {
        time2sec(1, 0, 0) - 1
    }
}

/// `aCOU_getTime`, verbatim: digit 0 = seconds ones, 1 =
/// seconds tens, 2 = minutes ones, 3 = minutes tens. The display
/// renders MM:SS.
pub fn get_time(nowsec: i32, digit: u8) -> i32 {
    let min = (nowsec / SECONDS_IN_MINUTE) % MINUTES_IN_HOUR;
    let sec = nowsec % SECONDS_IN_MINUTE;
    match digit {
        0 => sec % 10,
        1 => sec / 10,
        2 => min % 10,
        3 => min / 10,
        _ => 0,
    }
}

/// NPC0 countdown phases (`ac_countdown_npc0.c:17-26`).
pub mod term0 {
    pub const HOUR_1: u8 = 0;
    pub const MIN_30: u8 = 1;
    pub const MIN_10: u8 = 2;
    pub const MIN_5: u8 = 3;
    pub const MIN_1: u8 = 4;
    pub const NEW_YEAR: u8 = 5;
    pub const AFTER_10_SEC: u8 = 6;
    pub const NUM: u8 = 7;

    /// `aCD0_get_now_term`, verbatim. The comparisons are strict
    /// `>`/`<`: the exact threshold seconds fall into the next
    /// branch (e.g. exactly 23:55:00 is still the 10-minute term).
    pub fn now_term(now_sec: i32) -> u8 {
        if now_sec > super::time2sec(23, 59, 0) {
            MIN_1
        } else if now_sec > super::time2sec(23, 55, 0) {
            MIN_5
        } else if now_sec > super::time2sec(23, 50, 0) {
            MIN_10
        } else if now_sec > super::time2sec(23, 30, 0) {
            MIN_30
        } else if now_sec > super::time2sec(23, 0, 0) {
            HOUR_1
        } else if now_sec < super::time2sec(0, 0, 10) {
            NEW_YEAR
        } else {
            AFTER_10_SEC
        }
    }
}

/// NPC1 countdown phases — SIMPLER than NPC0's: no
/// AFTER_10_SEC branch and no `< 00:00:10` branch; the `else`
/// goes straight to NEW_YEAR.
pub mod term1 {
    pub const HOUR_1: u8 = 0;
    pub const MIN_30: u8 = 1;
    pub const MIN_10: u8 = 2;
    pub const MIN_5: u8 = 3;
    pub const MIN_1: u8 = 4;
    pub const NEW_YEAR: u8 = 5;
    pub const NUM: u8 = 6;

    pub fn now_term(now_sec: i32) -> u8 {
        if now_sec > super::time2sec(23, 59, 0) {
            MIN_1
        } else if now_sec > super::time2sec(23, 55, 0) {
            MIN_5
        } else if now_sec > super::time2sec(23, 50, 0) {
            MIN_10
        } else if now_sec > super::time2sec(23, 30, 0) {
            MIN_30
        } else if now_sec > super::time2sec(23, 0, 0) {
            HOUR_1
        } else {
            NEW_YEAR
        }
    }
}

/// Countdown event layouts (`m_event_map_npc_data.c_inc:427-541`).
/// Eight actor slots: five countdown villagers, the countdown
/// prop (NEWYEAR_COUNTDOWN0), the numeric display
/// (NEWYEAR_COUNTDOWN1), and Tortimer. Seven candidate layouts
/// (`l_cdn_pule1_map` … `l_cdn_pule7_map`), placed on a pool
/// block, with the five-NPC-slot mask 0b0011111.
pub mod layout {
    pub const SLOT_NUM: usize = 8;
    pub const LAYOUT_NUM: usize = 7;
    pub const NPC_MASK: u8 = 0b0011111;

    pub const LAYOUTS: [[(u8, u8); 8]; 7] = [
        // pule1 — note slot 4 AND slot 7 are both at (2, 11); preserved verbatim
        [(10, 13), (13, 9), (12, 2), (2, 3), (2, 11), (12, 12), (12, 11), (2, 11)],
        // pule2
        [(5, 13), (13, 13), (13, 2), (4, 3), (13, 12), (3, 12), (3, 11), (5, 2)],
        // pule3
        [(10, 13), (12, 8), (2, 12), (4, 3), (12, 3), (12, 12), (12, 11), (3, 13)],
        // pule4
        [(5, 13), (8, 13), (13, 10), (12, 3), (2, 3), (3, 12), (3, 11), (12, 4)],
        // pule5
        [(11, 4), (13, 6), (2, 2), (10, 2), (13, 13), (13, 4), (13, 3), (2, 13)],
        // pule6
        [(10, 13), (13, 8), (12, 2), (4, 4), (2, 13), (12, 12), (12, 11), (13, 3)],
        // pule7
        [(5, 2), (2, 7), (13, 3), (12, 13), (3, 13), (3, 3), (3, 2), (13, 11)],
    ];

    pub fn slot_pos(layout: usize, slot: usize) -> Option<(u8, u8)> {
        LAYOUTS.get(layout)?.get(slot).copied()
    }
}

/// The countdown prop (`ac_countdown.c`).
pub mod prop {
    pub const ACT_WAIT: u8 = 0;
    pub const ACT_MOVE: u8 = 1;
    pub const ACT_HAPPY_NEW_YEAR: u8 = 2;
    pub const ACT_NUM: u8 = 3;

    /// MOVE animation: start frame 1.0f, terminal frame 61.0f,
    /// speed 0.5f.
    pub const MOVE_START_FRAME: f32 = 1.0;
    pub const MOVE_END_FRAME: f32 = 61.0;
    pub const MOVE_SPEED: f32 = 0.5;

    /// The happy-state counter wraps 59 → 0.
    pub const HAPPY_MAX: u8 = 59;

    /// The wait state's RTC gate: Dec 31, 23:59 (checked only
    /// when the clip's anime_play_flag is set). Note the prop
    /// dereferences the shared clip with NO NULL check — the
    /// numeric display checks first.
    pub fn wait_move_gate(month: u8, day: u8, hour: u8, min: u8, clip_flag: bool) -> bool {
        clip_flag && month == 12 && day == 31 && hour == 23 && min == 59
    }

    /// One happy-state tick: `action >= 59 ? 0 : action + 1`.
    pub fn happy_tick(action: u8) -> u8 {
        if action >= HAPPY_MAX { 0 } else { action + 1 }
    }
}

/// NPC1 movement (`ac_countdown_npc1_talk.c_inc`).
pub mod npc1_move {
    pub const MAX_DIST: f32 = 60.0;
    /// Block-edge clamp offsets (`offset[] = { 0.0f, 319.0f }`).
    pub const EDGE_OFFSETS: [f32; 2] = [0.0, 319.0];

    /// Before-midnight run: 3.0 / 0.3 / 0.3.
    pub const BEFORE_SPEED: (f32, f32, f32) = (3.0, 0.3, 0.3);
    /// After-midnight run: 1.0 / 0.1 / 0.1.
    pub const AFTER_SPEED: (f32, f32, f32) = (1.0, 0.1, 0.1);

    /// `aCD1_check_moveRange`: TRUE when outside the 60-unit radius.
    pub fn out_of_range(home: (f32, f32), pos: (f32, f32)) -> bool {
        let dx = home.0 - pos.0;
        let dz = home.1 - pos.1;
        dx * dx + dz * dz > MAX_DIST * MAX_DIST
    }

    /// Project back to the 60-unit boundary along the
    /// home→current angle (the C uses search_position_angleY +
    /// sin_s/cos_s).
    pub fn project_to_radius(home: (f32, f32), pos: (f32, f32)) -> (f32, f32) {
        // Verbatim: search_position_angleY = atans_table(dz, dx), i.e. the
        // angle from +Z toward +X, then x = sin_s, z = cos_s. (The earlier
        // `atan2(dz, dx)` swapped the axes and mirrored the snap point.)
        use crate::game_math::{cos_s, search_position_angle_y, sin_s, Xyz};
        let angle = search_position_angle_y(Xyz::new(home.0, 0.0, home.1), Xyz::new(pos.0, 0.0, pos.1));
        (home.0 + sin_s(angle) * MAX_DIST, home.1 + cos_s(angle) * MAX_DIST)
    }
}

/// NPC1 countdown/firing sequence.
pub mod npc1_seq {
    /// `aCD1_front_wait`: now_sec > 23:59:50 → countdown-wait.
    pub const COUNT_WAIT_SEC: i32 = super::time2sec(23, 59, 50);
    /// Animation loop count set on entering countdown-wait.
    pub const COUNT_WAIT_LOOPS: u8 = 2;
    /// The cracker effect fires when the animation frame passes 3.0f
    /// AND the actor is drawn.
    pub const FIRE_FRAME: f32 = 3.0;
    /// Clap loop counter.
    pub const CLAP_LOOPS: u8 = 43;
    /// Clap sounds: NA_SE_2F, NA_SE_31, NA_SE_32, NA_SE_33,
    /// indexed by `npc_id - SP_NPC_EV_COUNTDOWN_1`.
    pub const CLAP_SOUNDS: [&str; 4] = ["NA_SE_2F", "NA_SE_31", "NA_SE_32", "NA_SE_33"];

    pub fn should_count_wait(now_sec: i32) -> bool {
        now_sec > COUNT_WAIT_SEC
    }
}

/// The numeric display's digit-change logic
/// (`ac_count02_move.c_inc`, `aCOU_wait`).
pub mod display {
    /// The recalculation gate: `arg1_f <= 0.0f` (the draw
    /// callback advances arg1_f toward 8.0f during a digit
    /// transition and resets it to 0.0f when done).
    pub fn should_recalc(transition_progress: f32) -> bool {
        transition_progress <= 0.0
    }

    /// The four-bit changed-digit mask, verbatim: compare digits
    /// from most significant (3) downward; shift the mask right
    /// for each matching digit; stop at the first difference.
    /// Covers the first changed digit and all less-significant ones.
    pub fn change_mask(old: i32, new: i32) -> u8 {
        let mut bits: u8 = 0b1111;
        for i in 0..4 {
            if super::get_time(new, 3 - i) != super::get_time(old, 3 - i) {
                break;
            }
            bits >>= 1;
        }
        bits
    }

    /// Prop-animation request: shared clip non-NULL, flag not
    /// already set, and arg0 <= 2 → call anime_play_proc().
    pub fn should_request_anime(clip_present: bool, flag_set: bool, arg0: i32) -> bool {
        clip_present && !flag_set && arg0 <= 2
    }
}

// ---- C ABI ----
// All pc_-prefixed: the C originals stay compiled; these are staged kernels.

#[no_mangle]
pub extern "C" fn pc_cd_set_now_time(month: u8, now_sec: i32) -> i32 {
    set_now_time(month, now_sec)
}

#[no_mangle]
pub extern "C" fn pc_cd_get_time(nowsec: i32, digit: u8) -> i32 {
    get_time(nowsec, digit)
}

#[no_mangle]
pub extern "C" fn pc_cd_term0(now_sec: i32) -> u8 {
    term0::now_term(now_sec)
}

#[no_mangle]
pub extern "C" fn pc_cd_term1(now_sec: i32) -> u8 {
    term1::now_term(now_sec)
}

#[no_mangle]
pub extern "C" fn pc_cd_digit_change_mask(old: i32, new: i32) -> u8 {
    display::change_mask(old, new)
}

#[no_mangle]
pub extern "C" fn pc_cd_request_anime(clip_present: u8, flag_set: u8, arg0: i32) -> u8 {
    display::should_request_anime(clip_present != 0, flag_set != 0, arg0) as u8
}

#[no_mangle]
pub extern "C" fn pc_cd_in_move_range(home_x: f32, home_z: f32, x: f32, z: f32) -> u8 {
    (!npc1_move::out_of_range((home_x, home_z), (x, z))) as u8
}

#[no_mangle]
pub extern "C" fn pc_cd_project_radius(home_x: f32, home_z: f32, x: f32, z: f32, out: *mut f32) {
    if !out.is_null() {
        let (px, pz) = npc1_move::project_to_radius((home_x, home_z), (x, z));
        let out = unsafe { std::slice::from_raw_parts_mut(out, 2) };
        out[0] = px;
        out[1] = pz;
    }
}

/// Writes the (x, z) unit coordinates of a layout slot; returns 1 on success.
#[no_mangle]
pub extern "C" fn pc_cd_layout_pos(layout_idx: u8, slot: u8, out: *mut u8) -> u8 {
    match layout::slot_pos(layout_idx as usize, slot as usize) {
        Some((x, z)) if !out.is_null() => {
            let out = unsafe { std::slice::from_raw_parts_mut(out, 2) };
            out[0] = x;
            out[1] = z;
            1
        }
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn now_time() {
        // December, from 23:00 onward: seconds until midnight
        assert_eq!(set_now_time(12, time2sec(23, 0, 0)), 3600);
        assert_eq!(set_now_time(12, time2sec(23, 59, 59)), 1);
        assert_eq!(set_now_time(12, time2sec(0, 0, 0)), 3599);
        // December before 23:00: 3599 (the day is NOT checked)
        assert_eq!(set_now_time(12, time2sec(22, 59, 59)), 3599);
        // any other month: 0
        assert_eq!(set_now_time(1, time2sec(23, 59, 59)), 0);
        assert_eq!(set_now_time(11, 0), 0);
    }

    #[test]
    fn digits() {
        // 59:59 -> all nines
        assert_eq!(get_time(3599, 0), 9);
        assert_eq!(get_time(3599, 1), 5);
        assert_eq!(get_time(3599, 2), 9);
        assert_eq!(get_time(3599, 3), 5);
        // 01:00 -> min ones = 1
        assert_eq!(get_time(60, 2), 1);
        assert_eq!(get_time(60, 3), 0);
        // 00:09
        assert_eq!(get_time(9, 0), 9);
        assert_eq!(get_time(9, 1), 0);
        // out-of-range digit
        assert_eq!(get_time(60, 4), 0);
    }

    #[test]
    fn phases() {
        // strict comparisons: exact thresholds fall into the NEXT branch
        assert_eq!(term0::now_term(time2sec(23, 59, 1)), term0::MIN_1);
        assert_eq!(term0::now_term(time2sec(23, 59, 0)), term0::MIN_5);
        assert_eq!(term0::now_term(time2sec(23, 55, 0)), term0::MIN_10);
        assert_eq!(term0::now_term(time2sec(23, 50, 0)), term0::MIN_30);
        assert_eq!(term0::now_term(time2sec(23, 30, 0)), term0::HOUR_1);
        assert_eq!(term0::now_term(time2sec(23, 0, 0)), term0::AFTER_10_SEC);
        assert_eq!(term0::now_term(time2sec(0, 0, 9)), term0::NEW_YEAR);
        assert_eq!(term0::now_term(time2sec(0, 0, 10)), term0::AFTER_10_SEC);
        assert_eq!(term0::now_term(time2sec(12, 0, 0)), term0::AFTER_10_SEC);
        assert_eq!(term0::NUM, 7);
        // NPC1: simpler — no AFTER_10_SEC, else goes straight to NEW_YEAR
        assert_eq!(term1::now_term(time2sec(23, 0, 0)), term1::NEW_YEAR);
        assert_eq!(term1::now_term(time2sec(12, 0, 0)), term1::NEW_YEAR);
        assert_eq!(term1::now_term(time2sec(0, 0, 5)), term1::NEW_YEAR);
        assert_eq!(term1::NUM, 6);
    }

    #[test]
    fn layouts() {
        assert_eq!(layout::LAYOUT_NUM, 7);
        assert_eq!(layout::NPC_MASK, 0b0011111);
        assert_eq!(layout::slot_pos(0, 0), Some((10, 13)));
        // the (2, 11) overlap is preserved, not "fixed"
        assert_eq!(layout::slot_pos(0, 4), Some((2, 11)));
        assert_eq!(layout::slot_pos(0, 7), Some((2, 11)));
        assert_eq!(layout::slot_pos(1, 7), Some((5, 2)));
        assert_eq!(layout::slot_pos(6, 7), Some((13, 11)));
        assert_eq!(layout::slot_pos(7, 0), None);
        assert_eq!(layout::slot_pos(0, 8), None);
    }

    #[test]
    fn prop_states() {
        assert_eq!(prop::ACT_NUM, 3);
        assert_eq!((prop::MOVE_START_FRAME, prop::MOVE_END_FRAME, prop::MOVE_SPEED), (1.0, 61.0, 0.5));
        assert!(prop::wait_move_gate(12, 31, 23, 59, true));
        assert!(!prop::wait_move_gate(12, 31, 23, 59, false));
        assert!(!prop::wait_move_gate(12, 30, 23, 59, true));
        assert_eq!(prop::happy_tick(58), 59);
        assert_eq!(prop::happy_tick(59), 0);
    }

    #[test]
    fn movement() {
        assert!(!npc1_move::out_of_range((0.0, 0.0), (30.0, 40.0))); // 50 < 60
        assert!(npc1_move::out_of_range((0.0, 0.0), (60.0, 1.0)));
        let (px, pz) = npc1_move::project_to_radius((0.0, 0.0), (100.0, 0.0));
        assert!((px - 60.0).abs() < 0.001 && pz.abs() < 0.001);
        assert_eq!(npc1_move::BEFORE_SPEED, (3.0, 0.3, 0.3));
        assert_eq!(npc1_move::AFTER_SPEED, (1.0, 0.1, 0.1));
        assert_eq!(npc1_move::EDGE_OFFSETS, [0.0, 319.0]);
    }

    #[test]
    fn sequence() {
        assert!(npc1_seq::should_count_wait(time2sec(23, 59, 51)));
        assert!(!npc1_seq::should_count_wait(time2sec(23, 59, 50)));
        assert_eq!(npc1_seq::COUNT_WAIT_LOOPS, 2);
        assert_eq!(npc1_seq::CLAP_LOOPS, 43);
    }

    #[test]
    fn digit_mask() {
        // only seconds-ones changed -> 0b0001
        assert_eq!(display::change_mask(60, 61), 0b0001);
        // minutes-tens changed -> 0b1111
        assert_eq!(display::change_mask(599, 600), 0b1111);
        // no change -> 0b0000
        assert_eq!(display::change_mask(61, 61), 0b0000);
        assert!(display::should_recalc(0.0));
        assert!(!display::should_recalc(4.0));
        assert!(display::should_request_anime(true, false, 2));
        // `arg0 <= 2` (ac_count02_move.c_inc:56): 3 seconds left is too early.
        assert!(!display::should_request_anime(true, false, 3));
        assert!(!display::should_request_anime(true, true, 2));
        assert!(!display::should_request_anime(false, false, 2));
    }
}
