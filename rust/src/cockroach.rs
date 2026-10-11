//! Cockroaches (`m_cockroach.c`, `ac_house_goki.c`).
//!
//! Verified against `src/game/m_cockroach.c`,
//! `include/m_cockroach.h`, `src/actor/ac_house_goki.c`,
//! `include/ac_house_goki.h` (GAFE01_00 Rev 0 USA). 19/19
//! sections PROVEN, 0 DIFFERS (see nuances below).
//!
//! Scope split: this module owns the pure kernels — the
//! infestation growth and clamp, the visible-count math,
//! the house-index mask, the stomp predicate, the
//! proximity predicate, the wait/move decision, the move
//! target offsets, the retail jump-angle bug, the wall
//! cardinal constraint, and the timer/alpha/shadow
//! steps. RTC/`lbRTC_*` calls, house/cottage save writes,
//! actor allocation/deletion/callback registration,
//! collision queries, `aMR_CLIP` calls, sound/effect
//! creation, shadow/rendering, RNG call order, and the
//! `mCkRh_DecideNowGokiFamilyCount` orchestration stay in
//! C.
//!
//! Nuances carried: the growth formula branches — when
//! the saved count is already positive the ENTIRE day
//! gap is added, not just the days past six; the day
//! calculation pins the saved time to 01:01:01 and uses
//! `lbRTC_GetIntervalDays`; `mCkRh_CalcCanLookGokiCount`
//! returns FALSE when it clamps; the cottage add/remove
//! branches write the count but fall through to return
//! FALSE; the USA target uses the pre-VER_GAFU01_00
//! cottage branch; the retail `RANDOM(1)` always yields
//! 0, so the jump angle is always player angle + 22.5°
//! (= 4096); the WAIT/MOVE `||` short-circuit skips the
//! RNG call when `game_frame % 100 > 20`; the move
//! transition draws `RANDOM_F(5.0f)` twice (decide +
//! `aHG_move_init`); timer/alpha steps scale by
//! `dt_num_60fps_frames`; `aHG_check_threshold` always
//! returns FALSE (dead doorway branch); the wall-angle
//! proc does nothing unless the player is more than
//! 20.0f away; actor death decrements only the
//! transient visible count — the saved infestation
//! count's lifecycle is untraced, so this port stays
//! arithmetic-only.

/// Maximum saved infestation count.
pub const MAX_NUM: i32 = 10;
/// Days before the infestation count grows.
pub const INTERVAL_DAYS: i32 = 6;
/// Maximum visible cockroaches in the room.
pub const CAN_LOOK_NUM: i32 = 3;

/// Stomp horizontal distance (strict).
pub const STOMP_DIST: f32 = 9.0;
/// Proximity distance (strict).
pub const NEAR_DIST: f32 = 60.0;
/// Wall-angle sentinel for "no usable angle".
pub const WALL_SENTINEL: i32 = 777;
/// Cardinal direction mask.
pub const CARDINAL_MASK: i32 = 0xC000;
/// Retail jump angle: player angle + DEG2SHORT_ANGLE2(22.5°) = 4096.
pub const JUMP_ANGLE_OFFSET: i32 = 4096;

/// Actor speeds and physics.
pub const AWAY_SPEED: f32 = 8.0;
pub const MOVE_SPEED: f32 = 4.0;
pub const JUMP_VY: f32 = 17.0;
pub const JUMP_SPEED: f32 = 5.0;
pub const GRAVITY: f32 = -2.0;
pub const FALL_GRAVITY: f32 = -7.0;
pub const WALL_RADIUS: f32 = 15.0;
/// Timers and fades.
pub const TIMER_STEP: f32 = 0.5;
pub const ALPHA_STEP: f32 = 3.5;
pub const ALPHA_MAX: f32 = 255.0;
pub const ALPHA_FADE_INIT: f32 = 30.0;
pub const DEATH_TIMER: f32 = 40.0;
pub const SHADOW_FADE_STEP: f32 = 2.5;
/// Wait/move actions.
pub const ACT_WAIT: i32 = 0;
pub const ACT_MOVE: i32 = 1;

/// Count clamp (`mCkRh_GokiFamilyCount2Good`):
/// negative → 0, 0..10 unchanged, > 10 → 10.
#[no_mangle]
pub extern "C" fn pc_cockroach_clamp_count(count: i32) -> i32 {
    if count < 0 {
        0
    } else if count > MAX_NUM {
        MAX_NUM
    } else {
        count
    }
}

/// Infestation growth
/// (`mCkRh_DecideNowGokiFamilyCount`): when the day gap
/// is strictly greater than 6, the new count is the
/// clamped sum of the saved count and — if the saved
/// count is already positive — the entire day gap,
/// otherwise the gap minus 6. Otherwise the saved count
/// is unchanged. The day-gap computation stays in C.
#[no_mangle]
pub extern "C" fn pc_cockroach_growth_count(day_gap: i32, goki_num: i32) -> i32 {
    if day_gap > INTERVAL_DAYS {
        let count = if goki_num > 0 {
            day_gap
        } else {
            day_gap - INTERVAL_DAYS
        };
        pc_cockroach_clamp_count(count + goki_num)
    } else {
        goki_num
    }
}

/// Visible-count update (`mCkRh_CalcCanLookGokiCount`):
/// `current + delta` clamped to 0..3. Writes the new
/// count to `out`; returns 1 (TRUE) when no clamping
/// occurred, 0 (FALSE) when it clamped.
#[no_mangle]
pub extern "C" fn pc_cockroach_visible_count(current: i32, delta: i32, out: *mut i32) -> u8 {
    if out.is_null() {
        return 0;
    }
    let new = current + delta;
    let (clamped, ok) = if new < 0 {
        (0, 0u8)
    } else if new > CAN_LOOK_NUM {
        (CAN_LOOK_NUM, 0u8)
    } else {
        (new, 1u8)
    };
    unsafe {
        *out = clamped;
    }
    ok
}

/// House index: `mHS_get_arrange_idx(player_no) & 3`.
#[no_mangle]
pub extern "C" fn pc_cockroach_house_index(arrange_idx: i32) -> i32 {
    arrange_idx & 3
}

/// Stomp predicate (`aHG_check_dead`, third condition):
/// the player exists, is moving, is within a strict
/// 9.0f horizontal distance, and the cockroach is
/// exactly at its home Y. The `aMR_CLIP` callback and
/// the two background-collision flags stay in C.
#[no_mangle]
pub extern "C" fn pc_goki_stomp_check(
    player_exists: bool,
    player_moving: bool,
    dist_xz: f32,
    y: f32,
    home_y: f32,
) -> bool {
    player_exists && player_moving && dist_xz < STOMP_DIST && y == home_y
}

/// Proximity predicate (`aHG_player_check`, distance
/// part): strict 60.0f. The player-exists and speed
/// checks stay in C.
#[no_mangle]
pub extern "C" fn pc_goki_player_near(dist_xz: f32) -> bool {
    dist_xz < NEAR_DIST
}

/// Wait/move decision: WAIT when
/// `frame_mod100 > 20 || rng01 < 0.5`, else WAIT when
/// `dist_xz < 60`, else MOVE. The RNG sample stays in C
/// (and is skipped entirely when `frame_mod100 > 20`
/// per the source short-circuit). Returns ACT_WAIT or
/// ACT_MOVE.
#[no_mangle]
pub extern "C" fn pc_goki_decide_wait_move(frame_mod100: i32, dist_xz: f32, rng01: f32) -> i32 {
    if frame_mod100 > 20 || rng01 < 0.5 {
        ACT_WAIT
    } else if dist_xz < NEAR_DIST {
        ACT_WAIT
    } else {
        ACT_MOVE
    }
}

/// Move-target offsets: `(80 − rng1·160, 80 − rng2·160)`.
/// The two `RANDOM_F(160.0f)` samples stay in C.
/// Writes to `out[0..2]`; returns 1 on success.
#[no_mangle]
pub extern "C" fn pc_goki_move_target(rng1: f32, rng2: f32, out: *mut f32) -> u8 {
    if out.is_null() {
        return 0;
    }
    let out = unsafe { std::slice::from_raw_parts_mut(out, 2) };
    out[0] = 80.0 - rng1 * 160.0;
    out[1] = 80.0 - rng2 * 160.0;
    1
}

/// Retail jump-angle offset: `RANDOM(1)` always yields 0,
/// so the angle is always the player angle + 22.5°
/// (4096). Preserve the bug; do not use RANDOM(2).
#[no_mangle]
pub extern "C" fn pc_goki_jump_angle_offset() -> i32 {
    JUMP_ANGLE_OFFSET
}

/// Wall cardinal constraint: `angle.y &= 0xC000`.
#[no_mangle]
pub extern "C" fn pc_goki_wall_cardinal(angle_y: i32) -> i32 {
    angle_y & CARDINAL_MASK
}

/// Jump roll: 0.5 when no usable wall angle was found,
/// 0.2 otherwise. The `RANDOM_F(1.0f)` sample stays in C.
#[no_mangle]
pub extern "C" fn pc_goki_should_jump(rand01: f32, no_usable_angle: bool) -> bool {
    let chance = if no_usable_angle { 0.5 } else { 0.2 };
    rand01 < chance
}

/// Timer step (`aHG_calc_timer`): subtracts 0.5 × dt,
/// clamped at zero.
#[no_mangle]
pub extern "C" fn pc_goki_timer_step(timer: f32, dt_frames: f32) -> f32 {
    (timer - TIMER_STEP * dt_frames).max(0.0)
}

/// Fade-in alpha step: +3.5 × dt, capped at 255.
#[no_mangle]
pub extern "C" fn pc_goki_alpha_step(alpha: f32, dt_frames: f32) -> f32 {
    (alpha + ALPHA_STEP * dt_frames).min(ALPHA_MAX)
}

/// Shadow fade step: −2.5 × dt, floored at zero.
#[no_mangle]
pub extern "C" fn pc_goki_shadow_step(shadow_alpha: f32, dt_frames: f32) -> f32 {
    (shadow_alpha - SHADOW_FADE_STEP * dt_frames).max(0.0)
}

/// Death flicker visibility: `((int)timer & 2) == 0`.
#[no_mangle]
pub extern "C" fn pc_goki_death_visible(timer: f32) -> bool {
    (timer as i32 & 2) == 0
}

/// AWAY timer init: `20.0 + RANDOM_F(20.0)`. The sample
/// stays in C.
#[no_mangle]
pub extern "C" fn pc_goki_away_timer(rng: f32) -> f32 {
    20.0 + rng * 20.0
}

/// WAIT/MOVE timer init: `5.0 + RANDOM_F(5.0)`. The
/// sample stays in C.
#[no_mangle]
pub extern "C" fn pc_goki_wait_timer(rng: f32) -> f32 {
    5.0 + rng * 5.0
}

// ---------------------------------------------------------------------------
// Follow-ups resolved (claude, 2026-10-12)
// Sources: ac-decomp m_cockroach.c, ac_my_room_goki.c_inc,
// ac_my_room.c (aMR_SetClip / aMR_CheckDannaKill), ac_house_goki.c,
// m_start_data_init.c, m_card.c / m_shop.c (mCkRh_SavePlayTime callers).
// m_cockroach.c and the room code are identical on PC; ac_house_goki.c
// only gains dt scaling.
//
// Call chain:
// 1. Player data load (`m_start_data_init.c`, both the normal path and
//    the "loaded while travelling" path) calls
//    `mCkRh_DecideNowGokiFamilyCount(player_no)`. This is not on house
//    entry: the saved house count grows once per load.
// 2. Room entry: `aMR_GokiInfoCt` (first my_room move) resets the visible
//    count and spawns min(saved, 3) actors. Each spawn moves one roach
//    from "saved" to "visible" (`MinusGokiN` + `CalcCanLookGokiCount(1)`).
//    The first one appears next to the player in the player's own rooms
//    and triggers the one-time scare.
// 3. Stomping (`aHG_dead_init`) only does `CalcCanLookGokiCount(-1)`.
// 4. Leaving (`aMR_GokiInfoDt`) adds the still-visible ones back
//    (`PlussGokiN_NowRoom(visible)`).
//    So yes: a stomped cockroach is gone for good, and the saved count
//    drops by one.
// 5. The play time is only saved by `mCkRh_SavePlayTime` (card saves,
//    shop renewal). Loading twice without saving re-applies the growth.
// ---------------------------------------------------------------------------

/// Saved + visible cockroach bookkeeping for one room visit.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GokiVisit {
    /// `homes[].goki.num` (or the cottage's).
    pub saved: i32,
    /// `Common can_look_goki_count`.
    pub visible: i32,
}

impl GokiVisit {
    /// `mCkRh_CalcCanLookGokiCount`: clamps to 0..=3; false when clamped.
    pub fn calc_visible(&mut self, delta: i32) -> bool {
        let c = self.visible + delta;
        if c < 0 {
            self.visible = 0;
            false
        } else if c > CAN_LOOK_NUM {
            self.visible = CAN_LOOK_NUM;
            false
        } else {
            self.visible = c;
            true
        }
    }

    /// `aMR_GokiInfoCt`: number of spawns attempted on entry
    /// (`spawned_ok` = whether each attempt actually made an actor).
    pub fn enter_room(&mut self, mut spawned_ok: impl FnMut(usize) -> bool) {
        self.visible = 0;
        let max = self.saved.min(3);
        for i in 0..max as usize {
            if spawned_ok(i) {
                self.calc_visible(1);
                self.saved = (self.saved - 1).clamp(0, MAX_NUM);
            }
        }
    }

    /// `aHG_dead_init`.
    pub fn stomp(&mut self) {
        self.calc_visible(-1);
    }

    /// `aMR_GokiInfoDt`.
    pub fn leave_room(&mut self) {
        self.saved = (self.saved + self.visible).clamp(0, MAX_NUM);
    }
}

/// `mCkRh_DecideNowGokiFamilyCount` growth for one house. QUIRK: once a
/// house has any roach, the whole day gap is added, not `gap - 6`.
pub fn decide_count(saved: i32, day_gap: i32) -> i32 {
    if day_gap > INTERVAL_DAYS {
        let add = if saved > 0 { day_gap } else { day_gap - INTERVAL_DAYS };
        (saved + add).clamp(0, MAX_NUM)
    } else {
        saved
    }
}

/// Island cottage: the Australian build skips the update when the saved
/// year is 0; US/JP compute a gap from year 0, which fills the cottage
/// to the maximum.
pub fn cottage_update_applies(aus: bool, saved_year: u16) -> bool {
    !(aus && saved_year == 0)
}

pub const FTR0_START: u16 = 0x1000;
pub const ITEM1_NO_START: u16 = 0x2000;
pub const RSV_FE1F: u16 = 0xFE1F;

/// `aMR_CheckDannaKill` id gate: a roach dies when it is inside a solid
/// furniture's edge collision. BUG: only FTR0 ids are tested (inclusive of
/// 0x2000), so FTR1 furniture (0x3000+) never kills.
pub fn danna_kill_candidate(fg: u16) -> bool {
    (FTR0_START..=ITEM1_NO_START).contains(&fg) || fg == RSV_FE1F
}

/// `aHG_check_dead`: killed by furniture, a BG flag, or the moving
/// player within 9 units while it is on its home height.
pub fn goki_dead(furniture_kill: bool, bg_flag: bool, player_moving: bool, dist_xz: f32, on_home_y: bool) -> bool {
    furniture_kill || bg_flag || (player_moving && dist_xz < STOMP_DIST && on_home_y)
}

/// The clip hook is registered by `aMR_SetClip` as
/// `my_room->clip.check_danna_kill_proc = &aMR_CheckDannaKill`.
pub const DANNA_KILL_REGISTRATION: &str = "aMR_SetClip -> clip.check_danna_kill_proc";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_and_growth() {
        assert_eq!(pc_cockroach_clamp_count(-3), 0);
        assert_eq!(pc_cockroach_clamp_count(0), 0);
        assert_eq!(pc_cockroach_clamp_count(10), 10);
        assert_eq!(pc_cockroach_clamp_count(11), 10);
        // Strict > 6 gate.
        assert_eq!(pc_cockroach_growth_count(6, 0), 0);
        assert_eq!(pc_cockroach_growth_count(6, 2), 2);
        // Zero-count branch: day_gap - 6.
        assert_eq!(pc_cockroach_growth_count(7, 0), 1);
        assert_eq!(pc_cockroach_growth_count(12, 0), 6);
        assert_eq!(pc_cockroach_growth_count(16, 0), 10);
        // Positive-count branch: entire gap added.
        assert_eq!(pc_cockroach_growth_count(7, 2), 9);
        assert_eq!(pc_cockroach_growth_count(8, 2), 10); // clamped
        assert_eq!(pc_cockroach_growth_count(7, 1), 8);
        assert_eq!(pc_cockroach_growth_count(100, 5), 10);
    }

    #[test]
    fn visible_and_index() {
        let mut out = 0i32;
        assert_eq!(pc_cockroach_visible_count(2, -1, &mut out), 1);
        assert_eq!(out, 1);
        assert_eq!(pc_cockroach_visible_count(0, -1, &mut out), 0); // clamped
        assert_eq!(out, 0);
        assert_eq!(pc_cockroach_visible_count(3, 1, &mut out), 0); // clamped
        assert_eq!(out, 3);
        assert_eq!(pc_cockroach_visible_count(3, -3, &mut out), 1);
        assert_eq!(out, 0);
        assert_eq!(pc_cockroach_house_index(7), 3);
        assert_eq!(pc_cockroach_house_index(4), 0);
    }

    #[test]
    fn stomp_and_proximity() {
        assert!(pc_goki_stomp_check(true, true, 8.9, 10.0, 10.0));
        assert!(!pc_goki_stomp_check(true, true, 9.0, 10.0, 10.0)); // strict
        assert!(!pc_goki_stomp_check(true, false, 8.9, 10.0, 10.0)); // not moving
        assert!(!pc_goki_stomp_check(true, true, 8.9, 10.1, 10.0)); // not home Y
        assert!(!pc_goki_stomp_check(false, true, 8.9, 10.0, 10.0));
        assert!(pc_goki_player_near(59.9));
        assert!(!pc_goki_player_near(60.0));
    }

    #[test]
    fn decisions_and_targets() {
        // game_frame % 100 > 20 short-circuits to WAIT.
        assert_eq!(pc_goki_decide_wait_move(21, 100.0, 0.9), ACT_WAIT);
        assert_eq!(pc_goki_decide_wait_move(20, 100.0, 0.4), ACT_WAIT);
        assert_eq!(pc_goki_decide_wait_move(20, 59.0, 0.6), ACT_WAIT);
        assert_eq!(pc_goki_decide_wait_move(20, 60.0, 0.6), ACT_MOVE);
        let mut out = [0.0f32; 2];
        assert_eq!(pc_goki_move_target(0.0, 1.0, out.as_mut_ptr()), 1);
        assert_eq!(out, [80.0, -80.0]);
        assert_eq!(pc_goki_jump_angle_offset(), 4096);
        assert_eq!(pc_goki_wall_cardinal(0x1234), 0x1000);
        assert!(pc_goki_should_jump(0.4, true));
        assert!(!pc_goki_should_jump(0.5, true));
        assert!(pc_goki_should_jump(0.1, false));
        assert!(!pc_goki_should_jump(0.2, false));
    }

    #[test]
    fn timers_and_fades() {
        assert_eq!(pc_goki_timer_step(1.0, 1.0), 0.5);
        assert_eq!(pc_goki_timer_step(0.25, 1.0), 0.0);
        assert_eq!(pc_goki_timer_step(2.0, 2.0), 1.0);
        assert_eq!(pc_goki_alpha_step(30.0, 1.0), 33.5);
        assert_eq!(pc_goki_alpha_step(254.0, 1.0), 255.0);
        assert_eq!(pc_goki_shadow_step(2.0, 1.0), 0.0);
        assert_eq!(pc_goki_shadow_step(10.0, 1.0), 7.5);
        assert!(pc_goki_death_visible(40.0)); // 40 & 2 == 0
        assert!(!pc_goki_death_visible(39.0)); // 39 & 2 == 2
        assert_eq!(pc_goki_away_timer(0.5), 30.0);
        assert_eq!(pc_goki_wait_timer(0.5), 7.5);
    }
}

#[cfg(test)]
mod followup_tests {
    use super::*;

    #[test]
    fn visit_cycle() {
        let mut v = GokiVisit { saved: 5, visible: 0 };
        v.enter_room(|_| true);
        assert_eq!((v.saved, v.visible), (2, 3));
        v.stomp();
        v.leave_room();
        assert_eq!(v.saved, 4);
        assert_eq!(decide_count(0, 7), 1);
        assert_eq!(decide_count(1, 7), 8);
        assert_eq!(decide_count(9, 20), 10);
        assert!(danna_kill_candidate(0x1000) && danna_kill_candidate(0x2000) && !danna_kill_candidate(0x3000));
        assert!(!cottage_update_applies(true, 0) && cottage_update_applies(false, 0));
    }
}
