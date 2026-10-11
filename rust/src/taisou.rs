//! The Morning Aerobics NPC (`ac_taisou_npc0.c`).
//!
//! Verified against `src/actor/npc/ac_taisou_npc0.c`,
//! `ac_taisou_npc0_talk.c_inc`, `ac_taisou_npc0_anime.c_inc`,
//! `include/ac_taisou_npc0.h`, `include/audio.h`,
//! `src/game/m_event_map_npc_data.c_inc`,
//! `src/game/m_event_schedule.c_inc`, `src/actor/ac_event_manager.c`,
//! `include/ac_npc_anim_def.h`, `include/m_name_table.h`
//! (GAFE01_00 Rev 0 USA). All 10 brief claims PROVEN.
//!
//! Scope split: the MORNING_AEROBICS Jul 25–Aug 31 06:00 schedule
//! row already lives in the sibling `event_day` module; this
//! module adds the rumor/talk rows there. This module owns the
//! pure kernels: the failure-rate table and eligibility, the
//! action→animation table, all ten ratio tables, the delay math,
//! the dialogue formulas, the init-angle rule, the leader
//! selection, and the shrine layout. The radio counter,
//! RNG call order, NPC clip lifecycle, the leader/group globals,
//! animation init, demo/talk, Tortimer's radio-card inventory
//! mutations, and the event-manager handlers stay in C.
//!
//! Nuances carried: the failure `type` falls back to 0 when the
//! NPC id is more than 4 above TAISOU_0; the mistake is eligible
//! only for actions other than 0 and 9; the failure action uses
//! `1 + RANDOM(8)` with a do/while rejection of `next_action`
//! (the loop stays in C); the taisou6 ratio array is genuinely
//! unsorted (`{0.0, 0.5, 1.0, 1.5, 0.96875}`) — preserved as
//! written; the block-part comparison is against
//! `startRatio[idx][seq + 1]` (the NEXT segment boundary);
//! NPC 0 never gets the RANDOM(48) delay; the dialogue row is 0
//! while Morning Aerobics is active, 1 for the Sports Fair
//! variant; action == 10 fixes the villager message offset at +2
//! instead of calling RANDOM(2).

/// First aerobics NPC ID (`m_name_table.h`).
pub const SP_NPC_EV_TAISOU_0: u16 = 0xD04E;
/// Last aerobics NPC ID.
pub const SP_NPC_EV_TAISOU_4: u16 = 0xD052;
/// Tortimer's aerobics ID.
pub const SP_NPC_SONCHO_D078: u16 = 0xD078;
/// The aerobics radio prop actor ID.
pub const AEROBICS_RADIO: u16 = 0x582C;
/// Leader sentinel: `SP_NPC_EV_TAISOU_4 + 1`; registration keeps
/// the smallest NPC ID seen.
pub const LEADER_SENTINEL: u16 = 0xD053;

/// Exercise-failure probabilities per NPC type
/// (`chk_failure_rate`).
pub const FAILURE_RATE: [f32; 5] = [0.0, 0.05, 0.1, 0.15, 0.3];

/// Returns the failure rate for an NPC type index, with retail's
/// `> 4 → 0` fallback (out-of-range indices also yield 0 rather
/// than an out-of-bounds read).
#[no_mangle]
pub extern "C" fn pc_taisou_failure_rate(npc_type: i32) -> f32 {
    if npc_type < 0 || npc_type > 4 {
        0.0
    } else {
        FAILURE_RATE[npc_type as usize]
    }
}

/// The next exercise action: `(radio.measure + 1) % 10`
/// (`aTS0_ctrl_gymnastic_sub0`).
#[no_mangle]
pub extern "C" fn pc_taisou_next_action(measure: i32) -> i32 {
    (measure + 1) % 10
}

/// Returns true when a mistake may be selected: no failure actor
/// is currently designated, the next action is neither 0 nor 9,
/// and the sampled random float is below the type's rate. The
/// float sample stays in C.
#[no_mangle]
pub extern "C" fn pc_taisou_failure_eligible(
    next_action: i32,
    rand_float: f32,
    rate: f32,
    failure_active: bool,
) -> bool {
    !failure_active && next_action != 0 && next_action != 9 && rand_float < rate
}

/// The do/while rejection predicate for the failure action:
/// `1 + RANDOM(8)` is re-rolled while it equals `next_action`.
/// The loop and its RNG calls stay in C.
#[no_mangle]
pub extern "C" fn pc_taisou_failure_reject(failure_action: i32, next_action: i32) -> bool {
    failure_action == next_action
}

/// Action → base animation ID (`aTS0_set_animation`,
/// `ac_npc_anim_def.h` values). The C caller adds
/// `(anime_dir & 1)` for the variant and passes FALSE to the
/// animation init.
pub const ANIM_SEQ_NO: [i32; 13] = [
    81, // INTRO1_A
    83, // TAISOU1
    84, // TAISOU2
    87, // TAISOU4_A
    89, // TAISOU5_A
    91, // TAISOU6_A
    93, // TAISOU7
    85, // TAISOU3_A
    83, // TAISOU1
    5,  // WAIT1
    94, // KIDUKU1
    5,  // WAIT1
    105, // TIRED1
];

/// Returns the base animation ID for an exercise action (0–12),
/// or -1 outside the table. The variant bit is separate.
#[no_mangle]
pub extern "C" fn pc_taisou_anim_id(action: i32) -> i32 {
    if action < 0 || action >= ANIM_SEQ_NO.len() as i32 {
        return -1;
    }
    ANIM_SEQ_NO[action as usize]
}

/// Returns the animation variant bit: `anime_dir & 1`.
#[no_mangle]
pub extern "C" fn pc_taisou_anim_variant(anime_dir: i32) -> i32 {
    anime_dir & 1
}

/// Segment start-ratio tables, in segment order 0–9
/// (`aTS0_startRatio`). Lengths vary: 3,5,3,5,3,5,5,5,5,2.
/// Segment 6 (taisou6) is unsorted as written — do not sort it.
pub const START_RATIO_0: [f32; 3] = [0.0, 0.5, 0.93581255];
pub const START_RATIO_1: [f32; 5] = [0.0, 0.25, 0.5, 0.75, 0.96875];
pub const START_RATIO_2: [f32; 3] = [0.0, 0.5, 0.96875];
pub const START_RATIO_3: [f32; 5] = [0.0, 0.25, 0.5, 0.75, 0.96875];
pub const START_RATIO_4: [f32; 3] = [0.0, 0.5, 0.96875];
pub const START_RATIO_5: [f32; 5] = [0.0, 0.25, 0.5, 0.75, 0.96875];
pub const START_RATIO_6: [f32; 5] = [0.0, 0.5, 1.0, 1.5, 0.96875];
pub const START_RATIO_7: [f32; 5] = [0.0, 0.25, 0.5, 0.75, 0.96875];
pub const START_RATIO_8: [f32; 5] = [0.0, 0.25, 0.5, 0.75, 0.96875];
pub const START_RATIO_9: [f32; 2] = [0.0, 0.96875];
/// Measure-end ratios per segment (`aTS0_endRatio`).
pub const END_RATIO: [f32; 10] = [
    0.93581255, 0.96875, 0.96875, 0.96875, 0.96875, 0.96875, 0.96875, 0.96875, 0.96875,
    0.96875,
];
/// Animation morph ratios per segment (`aTS0_morphRatio`).
pub const MORPH_RATIO: [f32; 10] = [
    0.030187488, 0.03125, 0.03125, 0.03125, 0.03125, 0.03125, 0.03125, 0.03125, 0.03125,
    0.03125,
];
/// Ratio spans per segment (`aTS0_ratioLength`).
pub const RATIO_LENGTH: [[f32; 2]; 10] = [
    [0.5, 0.5],
    [0.25, 0.25],
    [0.5, 0.5],
    [0.25, 0.25],
    [0.5, 0.5],
    [0.25, 0.25],
    [0.5, 0.5],
    [0.25, 0.25],
    [0.25, 0.25],
    [1.0, 1.0],
];

fn start_ratio(seg: usize, idx: usize) -> Option<f32> {
    match seg {
        0 => START_RATIO_0.get(idx),
        1 => START_RATIO_1.get(idx),
        2 => START_RATIO_2.get(idx),
        3 => START_RATIO_3.get(idx),
        4 => START_RATIO_4.get(idx),
        5 => START_RATIO_5.get(idx),
        6 => START_RATIO_6.get(idx),
        7 => START_RATIO_7.get(idx),
        8 => START_RATIO_8.get(idx),
        9 => START_RATIO_9.get(idx),
        _ => None,
    }
    .copied()
}

/// Looks up `startRatio[seg][idx]`; returns -1.0 when out of
/// range.
#[no_mangle]
pub extern "C" fn pc_taisou_start_ratio(seg: i32, idx: i32) -> f32 {
    if seg < 0 || idx < 0 {
        return -1.0;
    }
    start_ratio(seg as usize, idx as usize).unwrap_or(-1.0)
}

/// The `aTS0_set_current` frame-sync math:
/// `start + ((measure_progress - now) - startRatio[seg][anime_dir])
/// * (end - start) / ratioLength[seg][anime_dir & 1]`.
/// Returns -1.0 when the table lookup is out of range.
#[no_mangle]
pub extern "C" fn pc_taisou_sync_frame(
    start: f32,
    end: f32,
    measure_progress: f32,
    now: f32,
    seg: i32,
    anime_dir: i32,
) -> f32 {
    let sr = if seg < 0 || anime_dir < 0 {
        None
    } else {
        start_ratio(seg as usize, anime_dir as usize)
    };
    let rl = if seg < 0 || seg >= RATIO_LENGTH.len() as i32 {
        None
    } else {
        Some(RATIO_LENGTH[seg as usize][(anime_dir & 1) as usize])
    };
    match (sr, rl) {
        (Some(sr), Some(rl)) if rl != 0.0 => {
            start + ((measure_progress - now) - sr) * (end - start) / rl
        }
        _ => -1.0,
    }
}

/// Returns true when the NPC gets the random timing offset:
/// every participant except SP_NPC_EV_TAISOU_0
/// (`aTS0_setup_delay_cnt`; the RANDOM(48) call stays in C).
#[no_mangle]
pub extern "C" fn pc_taisou_has_delay(npc_id: u16) -> bool {
    npc_id != SP_NPC_EV_TAISOU_0
}

/// One `aTS0_check_over_bgm_block_part` delay step, purely: when
/// the NPC has a delay and `delay_cnt != 0`, decrements the
/// counter, saves `ratio * 0.2` as the new current ratio, and
/// scales the ratio by 0.8; otherwise subtracts the saved
/// current ratio. Writes (new_current_ratio, adjusted_ratio) to
/// `out[0..2]` and returns the new delay count.
#[no_mangle]
pub extern "C" fn pc_taisou_delay_step(
    ratio: f32,
    delay_cnt: i32,
    current_ratio: f32,
    has_delay: bool,
    out: *mut f32,
) -> i32 {
    let (new_delay, new_cur, adjusted) = if has_delay && delay_cnt != 0 {
        (delay_cnt - 1, ratio * 0.2, ratio * 0.8)
    } else {
        (delay_cnt, current_ratio, ratio - current_ratio)
    };
    if !out.is_null() {
        let out = unsafe { std::slice::from_raw_parts_mut(out, 2) };
        out[0] = new_cur;
        out[1] = adjusted;
    }
    new_delay
}

/// Tests whether the adjusted ratio passed the next segment
/// boundary: `ratio > startRatio[seg][seq + 1]`
/// (`aTS0_check_over_bgm_block_part` returns FALSE while
/// `ratio <=` that entry). Returns false on out-of-range
/// lookups.
#[no_mangle]
pub extern "C" fn pc_taisou_over_block(adjusted: f32, seg: i32, seq: i32) -> bool {
    if seg < 0 || seq < 0 {
        return false;
    }
    match start_ratio(seg as usize, (seq + 1) as usize) {
        Some(boundary) => adjusted > boundary,
        None => false,
    }
}

/// Dialogue bases per looks index: row 0 while Morning Aerobics
/// is active, row 1 for the Sports Fair variant.
pub const MSG_BASE: [[i32; 6]; 2] = [
    [6716, 6725, 6701, 6746, 6761, 6776],
    [6806, 6815, 6791, 6836, 6851, 6862],
];
/// Copper's message bases per event row (0x2665 / 0x2668).
pub const COPPER_BASE: [i32; 2] = [9829, 9832];

/// Selects the dialogue row: 0 while Morning Aerobics is active,
/// 1 otherwise.
#[no_mangle]
pub extern "C" fn pc_taisou_msg_row(aerobics_active: bool) -> i32 {
    if aerobics_active { 0 } else { 1 }
}

/// NPC type for dialogue: `npc_id - SP_NPC_EV_TAISOU_0`.
#[no_mangle]
pub extern "C" fn pc_taisou_npc_type(npc_id: u16) -> i32 {
    npc_id.wrapping_sub(SP_NPC_EV_TAISOU_0) as i32
}

/// Villager exercise message: `base + type*3 + rand2`, except
/// action == 10 uses the fixed offset +2 instead of RANDOM(2).
/// The random values are supplied by the C caller.
#[no_mangle]
pub extern "C" fn pc_taisou_villager_msg(
    row: i32,
    looks: i32,
    npc_type: i32,
    rand2: i32,
    action: i32,
) -> i32 {
    let r = (row & 1) as usize;
    let l = looks.rem_euclid(6) as usize;
    let base = MSG_BASE[r][l] + npc_type * 3;
    if action == 10 {
        base + 2
    } else {
        base + rand2
    }
}

/// Copper's message: `COPPER_BASE[row] + rand3`.
#[no_mangle]
pub extern "C" fn pc_taisou_copper_msg(row: i32, rand3: i32) -> i32 {
    COPPER_BASE[(row & 1) as usize] + rand3
}

/// Schedule-init facing: NPC 0 faces 180° (32768 short-angle
/// units); everyone else faces 0°.
#[no_mangle]
pub extern "C" fn pc_taisou_init_angle(npc_id: u16) -> i32 {
    if npc_id == SP_NPC_EV_TAISOU_0 {
        32768
    } else {
        0
    }
}

/// Leader selection: the smallest NPC ID wins; the sentinel
/// starts above the range (`aTS0_leader_no`).
#[no_mangle]
pub extern "C" fn pc_taisou_leader_update(leader: u16, npc_id: u16) -> u16 {
    leader.min(npc_id)
}

/// Aerobics shrine layout: seven slots of (field-unit X, Z):
/// TAISOU_1..4, TAISOU_0, the radio, Tortimer.
pub const SPAWN_LAYOUT: [(i32, i32); 7] = [
    (4, 10),
    (6, 10),
    (8, 10),
    (10, 10),
    (7, 12),
    (5, 13),
    (9, 8),
];
/// NPC slot mask `0b0011111` in the data record (4 NPC bits).
pub const SPAWN_NPC_MASK: u32 = 0b0011111;
pub const SPAWN_SLOT_COUNT: usize = 7;

/// Writes the (x, z) field-unit position of a spawn slot to
/// `out[0..2]`; returns 1 on success. Slots are 0-based.
#[no_mangle]
pub extern "C" fn pc_taisou_spawn_pos(slot: u8, out: *mut i32) -> u8 {
    if out.is_null() {
        return 0;
    }
    let s = slot as usize;
    if s >= SPAWN_LAYOUT.len() {
        return 0;
    }
    let (px, pz) = SPAWN_LAYOUT[s];
    let out = unsafe { std::slice::from_raw_parts_mut(out, 2) };
    out[0] = px;
    out[1] = pz;
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failure_arithmetic() {
        assert_eq!(pc_taisou_failure_rate(0), 0.0);
        assert_eq!(pc_taisou_failure_rate(4), 0.3);
        assert_eq!(pc_taisou_failure_rate(5), 0.0);
        assert_eq!(pc_taisou_failure_rate(-1), 0.0);
        assert_eq!(pc_taisou_next_action(3), 4);
        assert_eq!(pc_taisou_next_action(9), 0);
        // Eligible: no failure active, action not 0/9, float below rate.
        assert!(pc_taisou_failure_eligible(5, 0.04, 0.05, false));
        assert!(!pc_taisou_failure_eligible(5, 0.06, 0.05, false));
        assert!(!pc_taisou_failure_eligible(0, 0.0, 0.3, false));
        assert!(!pc_taisou_failure_eligible(9, 0.0, 0.3, false));
        assert!(!pc_taisou_failure_eligible(5, 0.0, 0.3, true));
        assert!(pc_taisou_failure_reject(5, 5));
        assert!(!pc_taisou_failure_reject(4, 5));
    }

    #[test]
    fn animation_table() {
        assert_eq!(pc_taisou_anim_id(0), 81);
        assert_eq!(pc_taisou_anim_id(7), 85);
        assert_eq!(pc_taisou_anim_id(12), 105);
        assert_eq!(pc_taisou_anim_id(13), -1);
        assert_eq!(pc_taisou_anim_variant(3), 1);
        assert_eq!(pc_taisou_anim_variant(4), 0);
    }

    #[test]
    fn ratio_tables() {
        assert_eq!(pc_taisou_start_ratio(0, 2), 0.93581255);
        // Segment 6 keeps its unsorted order.
        assert_eq!(pc_taisou_start_ratio(6, 2), 1.0);
        assert_eq!(pc_taisou_start_ratio(6, 3), 1.5);
        assert_eq!(pc_taisou_start_ratio(6, 4), 0.96875);
        assert_eq!(pc_taisou_start_ratio(9, 1), 0.96875);
        assert_eq!(pc_taisou_start_ratio(10, 0), -1.0);
        assert_eq!(pc_taisou_start_ratio(0, 3), -1.0);
        assert_eq!(END_RATIO[0], 0.93581255);
        assert_eq!(MORPH_RATIO[0], 0.030187488);
        assert_eq!(RATIO_LENGTH[9], [1.0, 1.0]);
        // Sync frame: start + ((prog - now) - sr) * (end - start) / rl.
        let f = pc_taisou_sync_frame(0.0, 100.0, 0.5, 0.0, 1, 1);
        assert!((f - (0.5 - 0.25) * 100.0 / 0.25).abs() < 1e-4);
        assert_eq!(pc_taisou_sync_frame(0.0, 100.0, 0.5, 0.0, 10, 0), -1.0);
    }

    #[test]
    fn delay_math() {
        assert!(pc_taisou_has_delay(0xD04F));
        assert!(!pc_taisou_has_delay(0xD04E));
        let mut out = [0.0f32; 2];
        // With delay: decrement, save ratio*0.2, scale by 0.8.
        let d = pc_taisou_delay_step(1.0, 10, 0.0, true, out.as_mut_ptr());
        assert_eq!(d, 9);
        assert_eq!(out, [0.2, 0.8]);
        // Without delay: subtract saved ratio.
        let d = pc_taisou_delay_step(1.0, 0, 0.3, true, out.as_mut_ptr());
        assert_eq!(d, 0);
        assert_eq!(out, [0.3, 0.7]);
        // Over-block: adjusted > startRatio[seg][seq+1].
        assert!(pc_taisou_over_block(0.6, 1, 1)); // > 0.5
        assert!(!pc_taisou_over_block(0.5, 1, 1));
        assert!(!pc_taisou_over_block(0.6, 1, 4)); // seq+1 OOB
    }

    #[test]
    fn dialogue() {
        assert_eq!(pc_taisou_msg_row(true), 0);
        assert_eq!(pc_taisou_msg_row(false), 1);
        assert_eq!(pc_taisou_npc_type(0xD050), 2);
        // Villager: base + type*3 + rand2; action 10 fixes +2.
        assert_eq!(pc_taisou_villager_msg(0, 0, 2, 1, 5), 6716 + 6 + 1);
        assert_eq!(pc_taisou_villager_msg(0, 0, 2, 1, 10), 6716 + 6 + 2);
        assert_eq!(pc_taisou_villager_msg(1, 0, 2, 1, 5), 6806 + 6 + 1);
        assert_eq!(pc_taisou_copper_msg(0, 2), 9829 + 2);
        assert_eq!(pc_taisou_copper_msg(1, 2), 9832 + 2);
        assert_eq!(pc_taisou_init_angle(0xD04E), 32768);
        assert_eq!(pc_taisou_init_angle(0xD04F), 0);
        assert_eq!(pc_taisou_leader_update(0xD053, 0xD050), 0xD050);
        assert_eq!(pc_taisou_leader_update(0xD04E, 0xD050), 0xD04E);
    }

    #[test]
    fn spawn_layout() {
        let mut out = [0i32; 2];
        assert_eq!(pc_taisou_spawn_pos(0, out.as_mut_ptr()), 1);
        assert_eq!(out, [4, 10]);
        assert_eq!(pc_taisou_spawn_pos(6, out.as_mut_ptr()), 1);
        assert_eq!(out, [9, 8]);
        assert_eq!(pc_taisou_spawn_pos(7, out.as_mut_ptr()), 0);
    }
}
