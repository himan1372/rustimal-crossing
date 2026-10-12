//! Snowman building (`m_snowman.c`, `ac_snowman.c`,
//! `ac_psnowman.c`).
//!
//! Verified against `src/game/m_snowman.c`, `include/m_snowman.h`,
//! `src/actor/ac_snowman.c`, `include/ac_snowman.h`,
//! `src/actor/ac_psnowman.c`, `src/game/m_event_schedule.c_inc`,
//! `src/actor/ac_event_manager.c` (GAFE01_00 Rev 0 USA). All
//! 12 sections PROVEN with DIFFERS-level nuances (see below).
//!
//! Scope split: the snowman-season schedule row (Dec 25 –
//! Feb 17) lives in the sibling `event_day` module. This
//! module owns the pure kernels: the growth step, the
//! proportion score, the message arithmetic, the reward-index
//! selection, the record packing, the slot/ID mapping, the
//! melt step, the lifespan check, and the push-gate
//! predicate. Actor lifecycle, collision ownership, the
//! player-push demo calls, foreground placement, save-pointer
//! access, date-growth integration, quest callbacks,
//! dialogue/demo state, effects/audio, mail submission, RNG
//! call order, and the event-manager birth orchestration
//! stay in C.
//!
//! Nuances carried: quality is scored from the ACTUAL actor
//! scales (`actorx->scale.x` = 0.01 + 0.02·normalized), not
//! the normalized values — the ratio differs; the stored
//! `body_scale` is the OTHER ball's normalized scale;
//! combining rejects only when BOTH balls are stationary
//! (&&, not either); the push timer saturates at 17, not 16
//! (gate `prev >= 16 || timer > 16`); the reward has exactly
//! ONE re-roll (no loop); `head_size = trunc(norm·255)`;
//! melting applies `trunc(0.8·size)` per elapsed day and
//! ROTATES the actor ID by the day count; `days < 0`
//! becomes days = 1 AND clears the saved date/hour;
//! registration requires PART0 + COMBINED + HEAD_JUMP +
//! nonzero fg_pos + !DELETE; the no-return function is
//! `mSN_ClearSnowmanData`, not `mSN_ClearSnowman` (the brief
//! mixed them up); `move_dist` persists across sessions via
//! the event common area (the accessor stays in C, only the
//! math is here).

/// Snowball actor names.
pub const ETC_SNOWMAN_BALL_A: u16 = 0x800D;
pub const ETC_SNOWMAN_BALL_B: u16 = 0x800E;
/// First completed-snowman actor ID.
pub const SNOWMAN0: u16 = 0xA008;
/// Last completed-snowman actor ID.
pub const SNOWMAN8: u16 = 0xA010;
/// Saved snowman slots.
pub const SNOWMAN_SAVE_COUNT: i32 = 3;

/// Snowball state flags (`ac_snowman.h`).
pub const FLAG_HEAD_JUMP: u8 = 1 << 0;
pub const FLAG_ON_GROUND: u8 = 1 << 1;
pub const FLAG_MOVED: u8 = 1 << 2;
pub const FLAG_IN_HOLE: u8 = 1 << 3;
pub const FLAG_4: u8 = 1 << 4; // commented "unused?" in source — do not assign meaning
pub const FLAG_COMBINED: u8 = 1 << 5;
pub const FLAG_NO_SPEAK: u8 = 1 << 6;
pub const FLAG_DELETE: u8 = 1 << 7;

/// Growth constants.
pub const MOVE_DIST_MAX: f32 = 6400.0;
pub const GRASS_ATTR_MAX: i32 = 3; // mCoBG_ATTRIBUTE_GRASS3
pub const SCALE_BASE: f32 = 0.01;
pub const SCALE_SPAN: f32 = 0.02;
pub const SCALE_CAP: f32 = 0.03;

/// Quality results (`mSN_RESULT_*`).
pub const RESULT_PERFECT: i32 = 0;
pub const RESULT_GOOD: i32 = 1;
pub const RESULT_OK: i32 = 2;
pub const RESULT_BAD: i32 = 3;
/// Target head/body ratio and inclusive thresholds.
pub const TARGET_RATIO: f32 = 0.85;
pub const THRESH_PERFECT: f32 = 0.05;
pub const THRESH_GOOD: f32 = 0.15;
pub const THRESH_OK: f32 = 0.25;

/// One growth step (`aSMAN_calc_scale`): on attributes ≤ 3
/// the distance grows by speed, otherwise it shrinks at 75%
/// of speed; clamped to [0, 6400]. Writes (new_move_dist,
/// normalized_scale, actor_scale) to `out[0..3]`; returns 1
/// on success. The same function drives the collision pipe
/// and shadow size from the normalized scale (kept in C).
#[no_mangle]
pub extern "C" fn pc_snowman_scale_step(
    attr: i32,
    speed: f32,
    move_dist: f32,
    out: *mut f32,
) -> u8 {
    if out.is_null() {
        return 0;
    }
    let mut dist = if attr <= GRASS_ATTR_MAX {
        move_dist + speed
    } else {
        move_dist - speed * 0.75
    };
    if dist > MOVE_DIST_MAX {
        dist = MOVE_DIST_MAX;
    } else if dist < 0.0 {
        dist = 0.0;
    }
    let normalized = dist / MOVE_DIST_MAX;
    let mut scale = normalized * SCALE_SPAN + SCALE_BASE;
    if scale > SCALE_CAP {
        scale = SCALE_CAP;
    }
    let out = unsafe { std::slice::from_raw_parts_mut(out, 3) };
    out[0] = dist;
    out[1] = normalized;
    out[2] = scale;
    1
}

/// Scores a build (`aSMAN_decide_scale_result`): the inputs
/// are the ACTUAL actor scales (0.01 + 0.02·normalized), not
/// the normalized values. `|head/body − 0.85| ≤ 0.05` →
/// PERFECT; `≤ 0.15` → GOOD; `≤ 0.25` → OK; else BAD. All
/// boundaries inclusive. Returns -1 if body_scale is zero.
#[no_mangle]
pub extern "C" fn pc_snowman_score(head_scale: f32, body_scale: f32) -> i32 {
    if body_scale == 0.0 {
        return -1;
    }
    let adjusted = (head_scale / body_scale - TARGET_RATIO).abs();
    if adjusted <= THRESH_PERFECT {
        RESULT_PERFECT
    } else if adjusted <= THRESH_GOOD {
        RESULT_GOOD
    } else if adjusted <= THRESH_OK {
        RESULT_OK
    } else {
        RESULT_BAD
    }
}

/// Completed-snowman message:
/// `2209 + ((snowman_msg_id + slot) % 3) + result·3`.
#[no_mangle]
pub extern "C" fn pc_snowman_msg_no(msg_id: i32, slot: i32, result: i32) -> i32 {
    2209 + (msg_id + slot).rem_euclid(3) + result * 3
}

/// Assembly dialogue bases per quality result (literal).
pub const ASSEMBLY_MSG_BASE: [i32; 4] = [2197, 2200, 2203, 2206];

/// Assembly message: `base[result] + rand3`. The RANDOM(3)
/// sample stays in C.
#[no_mangle]
pub extern "C" fn pc_snowman_assembly_msg(result: i32, rand3: i32) -> i32 {
    if result < 0 || result >= ASSEMBLY_MSG_BASE.len() as i32 {
        return -1;
    }
    ASSEMBLY_MSG_BASE[result as usize] + rand3
}

/// Reward item IDs, computed from the X-macro expansion
/// (each entry wrapped in FTR_START() = _SOUTH orientation):
/// 0x1F54, 0x1F58, 0x1F5C, 0x1F60, 0x1F64, 0x1F68, 0x1F6C,
/// 0x1F70, 0x1F74, 0x1F78, 0x2619 (CARPET25), 0x2719
/// (WALL25). Computed values — verify against the build
/// before wiring.
pub const REWARD_ITEMS: [i32; 12] = [
    0x1F54, 0x1F58, 0x1F5C, 0x1F60, 0x1F64, 0x1F68, 0x1F6C, 0x1F70, 0x1F74, 0x1F78, 0x2619,
    0x2719,
];
pub const REWARD_COUNT: i32 = 12;
/// Mail resource base: `mail_idx = 0x202 + rnd`.
pub const REWARD_MAIL_BASE: i32 = 0x202;

/// Selects the reward table index: the first RANDOM(12)
/// result stands unless the item is already collected, in
/// which case exactly ONE re-roll is used (no loop). Both
/// RNG samples and the ownership check stay in C.
#[no_mangle]
pub extern "C" fn pc_snowman_reward_index(rnd1: i32, owned1: bool, rnd2: i32) -> i32 {
    if owned1 {
        rnd2
    } else {
        rnd1
    }
}

/// Packs the 4-byte save record (`aSNOWMAN_Set_PSnowman_info`):
/// exists = 1, head/body = trunc(normalized·255), score =
/// result. Writes to `out[0..4]`; returns 1 on success.
#[no_mangle]
pub extern "C" fn pc_snowman_record_pack(
    norm_head: f32,
    norm_body: f32,
    result: i32,
    out: *mut u8,
) -> u8 {
    if out.is_null() {
        return 0;
    }
    let out = unsafe { std::slice::from_raw_parts_mut(out, 4) };
    out[0] = 1;
    out[1] = (norm_head * 255.0) as u8;
    out[2] = (norm_body * 255.0) as u8;
    out[3] = result as u8;
    1
}

/// Maps a completed-snowman actor ID to its save slot:
/// `(npc_id − SNOWMAN0) / 3`. The sub-ID within the slot is
/// `(npc_id − SNOWMAN0) % 3`.
#[no_mangle]
pub extern "C" fn pc_snowman_slot_from_id(npc_id: u16) -> i32 {
    npc_id.wrapping_sub(SNOWMAN0) as i32 / SNOWMAN_SAVE_COUNT
}

/// The sub-ID within a save slot: `(npc_id − SNOWMAN0) % 3`.
#[no_mangle]
pub extern "C" fn pc_snowman_sub_id(npc_id: u16) -> i32 {
    npc_id.wrapping_sub(SNOWMAN0) as i32 % SNOWMAN_SAVE_COUNT
}

/// One melt step (`mSN_MeltSnowman`): applies
/// `trunc(0.8·size)` per elapsed day, `days` times.
#[no_mangle]
pub extern "C" fn pc_snowman_melt_step(size_byte: u8, days: i32) -> u8 {
    let mut size = size_byte;
    let mut d = days;
    while d > 0 {
        size = (size as f32 * 0.8) as u8;
        d -= 1;
    }
    size
}

/// Lifespan check (`mSN_check_life`): the snowman survives
/// when the season is winter AND `((sub_id) + days) < 3`.
#[no_mangle]
pub extern "C" fn pc_snowman_check_life(winter: bool, npc_id: u16, days: i32) -> bool {
    winter && (pc_snowman_sub_id(npc_id) + days) < SNOWMAN_SAVE_COUNT
}

/// Normalizes the melt day count: `days < 0` becomes 1 (the
/// C code also clears the saved date/hour in that case —
/// kept in C).
#[no_mangle]
pub extern "C" fn pc_snowman_melt_days(days: i32) -> i32 {
    if days < 0 {
        1
    } else {
        days
    }
}

/// Push-gate predicate: normalized > 0.2, distance <
/// normalized·20 + 10 + 15, the request timer (`prev >= 16
/// || timer > 16` — saturates at 17, not 16), height gap in
/// (−10, 25), player moving, snowball speed < 3.0, and
/// `|angle_diff| < DEG2SHORT_ANGLE2(55°)`. The C caller
/// supplies the measured values; the angle threshold is
/// passed as a short-angle constant.
pub const PUSH_ANGLE_MAX: i32 = 10012; // DEG2SHORT_ANGLE2(55°)

/// Returns true when a push request may mature.
#[no_mangle]
pub extern "C" fn pc_snowman_push_ok(
    normalized: f32,
    dist: f32,
    prev_req: i32,
    req_timer: i32,
    height_gap: f32,
    player_moving: bool,
    ball_speed: f32,
    angle_diff: i32,
) -> bool {
    normalized > 0.2
        && dist < normalized * 20.0 + 10.0 + 15.0
        && (prev_req >= 16 || req_timer > 16)
        && height_gap > -10.0
        && height_gap < 25.0
        && player_moving
        && ball_speed < 3.0
        && angle_diff.abs() < PUSH_ANGLE_MAX
}

/// Returns true when the two balls may combine: both parts
/// present and different, both above 0.2 normalized, and at
/// least one ball moving (the C rejects when BOTH are
/// stationary). Ground/attribute and placement gates stay
/// in C.
#[no_mangle]
pub extern "C" fn pc_snowman_combine_ok(
    part_a: i32,
    part_b: i32,
    norm_a: f32,
    norm_b: f32,
    speed_a: f32,
    speed_b: f32,
) -> bool {
    part_a != part_b && norm_a > 0.2 && norm_b > 0.2 && (speed_a != 0.0 || speed_b != 0.0)
}

/// Selects the jumping head: the faster ball becomes the
/// head (1 when ball A's speed >= ball B's, else 0). Ties go
/// to ball A per the source comparison.
#[no_mangle]
pub extern "C" fn pc_snowman_head_is_a(speed_a: f32, speed_b: f32) -> bool {
    speed_a >= speed_b
}

// ---------------------------------------------------------------------------
// Follow-ups resolved (claude, 2026-10-12)
// Sources: ac-decomp ac_snowman.c, ac_event_manager.c (snowman_start),
// m_event.c (mEv_snowman_born_check, check_ymdh_range, after_n_day).
// `REWARD_ITEMS` above is verified: the 12 ids match FTR_START(...) /
// ITM_CARPET25 / ITM_WALL25 compiled from m_name_table.h with gcc.
// ---------------------------------------------------------------------------

/// Event common-area slot each ball keeps its rolled distance in
/// (`mEv_get_common_area(SNOWMAN_SEASON, part)`): ball A -> part 0,
/// everything else -> part 1. A missing area is reserved and the distance
/// starts at 0; a combined, moved, in-hole or off-map ball clears it on
/// destruction, otherwise the distance is written back.
pub fn move_dist_part(npc_id: u16) -> usize {
    if npc_id == ETC_SNOWMAN_BALL_A {
        0
    } else {
        1
    }
}

/// Whether the destructor keeps the ball's progress for next time.
pub fn keeps_move_dist(combined: bool, in_hole: bool, moved: bool, position_ok: bool) -> bool {
    !combined && !in_hole && !moved && position_ok
}

/// `aSMAN_FG_Position_Get` probe offsets (unit size 40): centre, then the
/// 4 sides, then the 4 diagonals.
pub const FG_PROBE: [(f32, f32); 9] = [
    (0.0, 0.0),
    (40.0, 0.0),
    (-40.0, 0.0),
    (0.0, 40.0),
    (0.0, -40.0),
    (40.0, 40.0),
    (-40.0, 40.0),
    (40.0, -40.0),
    (-40.0, -40.0),
];
pub const RSV_NO: u16 = 0xFFFF;
pub const RSV_WALL_NO: u16 = 0xFFFE;
/// `mCoBG_ATTRIBUTE_STONE` / `_WOOD` values passed in by the caller.
pub fn fg_spot_ok(fg: Option<u16>, height_gap: bool, attr_is_stone_or_wood: bool, ground_dy: f32) -> bool {
    // @BUG: the C dereferences mFI_GetUnitFG(pos) without a NULL check; a
    // probe outside the field (None here) would crash on hardware. The
    // model rejects it instead.
    match fg {
        None => false,
        Some(item) => {
            !height_gap && item != RSV_NO && item != RSV_WALL_NO && !attr_is_stone_or_wood && ground_dy < 60.0
        }
    }
}

/// Reward mail handbill index for table index `rnd` (0x202 + rnd); the
/// mail uses paper `ITM_PAPER12` (0x200C) and is only sent for a PERFECT
/// snowman when the post office has room and the player is a resident.
pub fn reward_mail_idx(rnd: i32) -> i32 {
    REWARD_MAIL_BASE + rnd
}

/// Head-jump height above the body (`aSMAN_process_combine_head_jump_init`):
/// 0.6 x (head radius + body radius), radius = scale x 20 + 10.
pub fn head_jump_height(head_norm: f32, body_norm: f32) -> f32 {
    ((head_norm * 20.0 + 10.0) + (body_norm * 20.0 + 10.0)) * 0.6
}

/// Parabolic arc for frame t (0..60): `(8t - t^2 / 7.5) * 25` model units,
/// peaking at t = 30 (3000) and landing at t = 60. Position meanwhile moves
/// linearly by 1/60 of the offset per frame.
pub fn head_jump_ofs_y(t: i32) -> f32 {
    let t = t as f32;
    (t * 8.0 + t * -(1.0 / 7.5) * t) * 25.0
}

/// Landing dust (frame 60): 5 `eEC_EFFECT_DUST` puffs at -90, -45, 0, 45,
/// 90 degrees (short angles), 0.7 x head radius below the head centre.
pub const DUST_ANGLES: [i16; 5] = [-16384, -8192, 0, 8192, 16384];
pub fn dust_drop(head_norm: f32) -> f32 {
    (head_norm * 20.0 + 10.0) * 0.7
}

/// Snowman placeholders that block a new pair of balls (`snowman_start`).
pub const SNOWMAN_START_BLOCKERS: [u16; 3] = [0xA008, 0xA00B, 0xA00E];

/// Packed `mEv_ymdh_u` (big-endian layout; the PC port swaps the struct
/// fields so `raw` keeps this order on x86).
pub fn ymdh(year: u8, month: u8, day: u8, hour: u8) -> u32 {
    (year as u32) << 24 | (month as u32) << 16 | (day as u32) << 8 | hour as u32
}

/// `mEv_snowman_born_check`: a new pair of balls may appear unless "now"
/// lies in [last snowman, end], where end = 05:00 on the same day if it was
/// finished before 06:00, otherwise 05:00 the next day (year rolls over
/// on Jan 1). The range is inclusive of hour 5, so new balls appear from
/// 06:00. `days_in_month` = `last_day_of_month`.
pub fn snowman_born_allowed(now: (u8, u8, u8, u8), last: (u8, u8, u8, u8), days_in_month: impl Fn(u8) -> u8) -> bool {
    let (ly, lm, ld, lh) = last;
    let (mut ey, mut em, mut ed) = (ly, lm, ld);
    if lh >= 6 {
        let mut d = ld + 1;
        let mut m = lm;
        if d > days_in_month(lm) {
            d -= days_in_month(lm);
            m = if lm == 12 { 1 } else { lm + 1 };
        }
        if (m, d) == (1, 1) {
            ey = (ly + 1) % 100;
        }
        em = m;
        ed = d;
    }
    let now_raw = ymdh(now.0, now.1, now.2, now.3);
    !(ymdh(ly, lm, ld, lh) <= now_raw && now_raw <= ymdh(ey, em, ed, 5))
}

/// `snowman_start` (event manager, SNOWMAN_SEASON): if no SNOWMAN0/3/6
/// placeholder exists and the born check passes, spawn ball A (place id
/// 0x64) and ball B (0x65) in free blocks.
pub fn snowman_start_spawns(blocker_present: bool, born_allowed: bool) -> Option<[(u16, u8); 2]> {
    (!blocker_present && born_allowed).then_some([(ETC_SNOWMAN_BALL_A, 0x64), (ETC_SNOWMAN_BALL_B, 0x65)])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scale_step(attr: i32, speed: f32, dist: f32) -> (f32, f32, f32) {
        let mut out = [0.0f32; 3];
        assert_eq!(pc_snowman_scale_step(attr, speed, dist, out.as_mut_ptr()), 1);
        (out[0], out[1], out[2])
    }

    #[test]
    fn growth() {
        // Grass: grows by speed.
        assert_eq!(scale_step(3, 100.0, 0.0), (100.0, 100.0 / 6400.0, 100.0 / 6400.0 * 0.02 + 0.01));
        // Other: shrinks at 75%.
        assert_eq!(scale_step(4, 100.0, 1000.0), (925.0, 925.0 / 6400.0, 925.0 / 6400.0 * 0.02 + 0.01));
        // Clamp at max.
        let (d, n, s) = scale_step(3, 100.0, 6390.0);
        assert_eq!((d, n, s), (6400.0, 1.0, 0.03));
        // Clamp at zero.
        assert_eq!(scale_step(4, 100.0, 50.0), (0.0, 0.0, 0.01));
    }

    #[test]
    fn scoring() {
        // Exact 0.85 ratio -> perfect; inclusive boundaries.
        assert_eq!(pc_snowman_score(0.85, 1.0), RESULT_PERFECT);
        assert_eq!(pc_snowman_score(0.90, 1.0), RESULT_PERFECT); // 0.05 boundary
        assert_eq!(pc_snowman_score(0.9001, 1.0), RESULT_GOOD);
        assert_eq!(pc_snowman_score(1.00, 1.0), RESULT_GOOD); // 0.15 boundary
        assert_eq!(pc_snowman_score(1.10, 1.0), RESULT_OK); // 0.25 boundary
        assert_eq!(pc_snowman_score(1.11, 1.0), RESULT_BAD);
        assert_eq!(pc_snowman_score(0.82, 1.0), RESULT_PERFECT); // |0.82-0.85| < 0.05
        assert_eq!(pc_snowman_score(1.0, 0.0), -1);
    }

    #[test]
    fn messages() {
        assert_eq!(pc_snowman_msg_no(2, 1, 0), 2209 + 0 + 0);
        assert_eq!(pc_snowman_msg_no(2, 1, 2), 2209 + 0 + 6);
        assert_eq!(pc_snowman_msg_no(0, 0, 3), 2209 + 0 + 9);
        assert_eq!(pc_snowman_assembly_msg(0, 2), 2199);
        assert_eq!(pc_snowman_assembly_msg(3, 2), 2208);
        assert_eq!(pc_snowman_assembly_msg(4, 0), -1);
    }

    #[test]
    fn reward_and_record() {
        // Single re-roll, no loop.
        assert_eq!(pc_snowman_reward_index(5, false, 7), 5);
        assert_eq!(pc_snowman_reward_index(5, true, 7), 7);
        assert_eq!(REWARD_MAIL_BASE + 5, 0x207);
        let mut out = [0u8; 4];
        assert_eq!(pc_snowman_record_pack(1.0, 0.5, 2, out.as_mut_ptr()), 1);
        assert_eq!(out, [1, 255, 127, 2]); // trunc(0.5*255) = 127
        assert_eq!(pc_snowman_slot_from_id(0xA008), 0);
        assert_eq!(pc_snowman_slot_from_id(0xA00A), 0);
        assert_eq!(pc_snowman_slot_from_id(0xA00B), 1);
        assert_eq!(pc_snowman_slot_from_id(0xA010), 2);
        assert_eq!(pc_snowman_sub_id(0xA00A), 2);
    }

    #[test]
    fn melting() {
        assert_eq!(pc_snowman_melt_step(255, 1), 204); // trunc(0.8*255)
        assert_eq!(pc_snowman_melt_step(255, 0), 255);
        assert_eq!(pc_snowman_melt_step(204, 1), 163);
        assert!(pc_snowman_check_life(true, 0xA008, 1)); // (0+1) < 3
        assert!(!pc_snowman_check_life(true, 0xA00A, 1)); // (2+1) = 3, not < 3
        assert!(pc_snowman_check_life(true, 0xA00A, 0)); // (2+0) < 3
        assert!(pc_snowman_check_life(true, 0xA009, 1)); // (1+1) < 3
        assert!(!pc_snowman_check_life(true, 0xA00A, 2)); // (2+2) = 4
        assert!(!pc_snowman_check_life(false, 0xA008, 1));
        assert_eq!(pc_snowman_melt_days(-5), 1);
        assert_eq!(pc_snowman_melt_days(3), 3);
    }

    #[test]
    fn push_and_combine() {
        assert!(pc_snowman_push_ok(0.5, 20.0, 16, 0, 0.0, true, 1.0, 5000));
        assert!(!pc_snowman_push_ok(0.2, 20.0, 16, 0, 0.0, true, 1.0, 5000)); // not > 0.2
        assert!(!pc_snowman_push_ok(0.5, 40.0, 16, 0, 0.0, true, 1.0, 5000)); // too far
        assert!(!pc_snowman_push_ok(0.5, 20.0, 15, 16, 0.0, true, 1.0, 5000)); // timer gate
        assert!(pc_snowman_push_ok(0.5, 20.0, 15, 17, 0.0, true, 1.0, 5000)); // saturates at 17
        assert!(!pc_snowman_push_ok(0.5, 20.0, 16, 0, 0.0, true, 1.0, 10028)); // angle
        assert!(pc_snowman_combine_ok(0, 1, 0.5, 0.6, 2.0, 0.0));
        assert!(!pc_snowman_combine_ok(0, 1, 0.5, 0.6, 0.0, 0.0)); // both stationary
        assert!(!pc_snowman_combine_ok(0, 0, 0.5, 0.6, 2.0, 1.0)); // same part
        assert!(!pc_snowman_combine_ok(0, 1, 0.2, 0.6, 2.0, 1.0)); // at 0.2
        assert!(pc_snowman_head_is_a(3.0, 2.0));
        assert!(pc_snowman_head_is_a(2.0, 2.0)); // ties go to A
        assert!(!pc_snowman_head_is_a(1.0, 2.0));
        assert_eq!(PUSH_ANGLE_MAX, (55.0f32 * 65536.0 / 360.0) as i32);
    }
}

#[cfg(test)]
mod followup_tests {
    use super::*;

    #[test]
    fn snowman_followups() {
        assert_eq!(move_dist_part(ETC_SNOWMAN_BALL_A), 0);
        assert_eq!(move_dist_part(ETC_SNOWMAN_BALL_B), 1);
        assert!(!fg_spot_ok(None, false, false, 0.0));
        assert!(fg_spot_ok(Some(0), false, false, 10.0));
        assert!((head_jump_ofs_y(30) - 3000.0).abs() < 0.01);
        assert!(head_jump_ofs_y(60).abs() < 1e-3);
        assert_eq!(reward_mail_idx(11), 0x20D);
        let dim = |m: u8| if m == 2 { 28 } else { 31 };
        // Built Dec 31 at 20:00: blocked until Jan 1 05:59 of the next year.
        assert!(!snowman_born_allowed((26, 1, 1, 5), (25, 12, 31, 20), dim));
        assert!(snowman_born_allowed((26, 1, 1, 6), (25, 12, 31, 20), dim));
        // Built at 03:00: allowed again from 06:00 the same day.
        assert!(!snowman_born_allowed((26, 1, 10, 5), (26, 1, 10, 3), dim));
        assert!(snowman_born_allowed((26, 1, 10, 6), (26, 1, 10, 3), dim));
        assert!(snowman_start_spawns(true, true).is_none());
    }
}
