//! The Sports Fair ball-toss NPCs (`ac_tamaire_npc0.c`,
//! `ac_tamaire_npc1.c`).
//!
//! Verified against `src/actor/npc/ac_tamaire_npc0.c`,
//! `ac_tamaire_npc0_schedule.c_inc`, `ac_tamaire_npc0_talk.c_inc`,
//! `include/ac_tamaire_npc0.h`, `src/actor/npc/ac_tamaire_npc1.c`,
//! `ac_tamaire_npc1_schedule.c_inc`, `ac_tamaire_npc1_talk.c_inc`,
//! `include/ac_tamaire_npc1.h`, `src/game/m_event_map_npc_data.c_inc`,
//! `src/game/m_event_schedule.c_inc`, `include/ac_npc.h`,
//! `include/ac_npc_anim_def.h` (GAFE01_00 Rev 0 USA). All 13
//! items PROVEN with two DIFFERS-level corrections (see below).
//!
//! Scope split: the two schedule windows (Mar 19–21 13:00–
//! 14:00 and Sep 21–23 13:00–14:00) live in the sibling
//! `event_day` module. This module owns the pure kernels:
//! NPC0's route destinations and transition logic, NPC1's
//! team-relative origin and random-move math, the team flag
//! bits, the ball tool table, the throw argument construction,
//! and the dialogue arithmetic. NPC clip/action requests, think
//! scheduling, the tool-actor lifecycle (including the
//! asymmetric destructor), animation frame control and
//! passCheck, all RNG call order, the eEC throw effect, the
//! shared static team flag with its per-construction reset,
//! talk/demo, and the map placement stay in C.
//!
//! Nuances carried: NPC0's destination switch has a TRUE
//! `default:` branch (commented as the Team 1 turn-2 case) —
//! preserved, not normalized into an explicit case; the walk
//! destination-mismatch path rolls think_idx BACKWARD; NPC1's
//! origin uses a bit test on the ID difference, not a team
//! enum; the team flag bits are raw 1/2, not normalized;
//! `angle = r − Tamakago_angle` is f32 arithmetic truncated
//! into an s16 (reproduced with Rust's truncating `as`);
//! sin_s/cos_s take short angles (kept in C); the pickup
//! frames {15, 27, 39} are edge-crossing tests via
//! passCheck; the throw effect's RANDOM(3000) is consumed
//! only on the effect-creation path; the destructor deletes
//! the LEFT hand actor only. DIFFERS corrections: throw-end
//! expiry goes to KYORO, not KYORO_MAE; the ball prop's
//! block-deletion test is strict `!=` (adjacent blocks DO
//! delete it) — the OPPOSITE of the turi stand's `ABS > 1`.

/// First ball-toss NPC ID (`m_name_table.h`).
pub const SP_NPC_EV_TAMAIRE_0: u16 = 0xD053;
/// Base ID for NPC1's relative-ID expressions.
pub const SP_NPC_EV_TAMAIRE_1: u16 = 0xD054;

/// NPC0 think states (`ac_tamaire_npc0.h`).
pub const TMN0_THINK_BIRTH: i32 = 0;
pub const TMN0_THINK_TEAM0_TURN: i32 = 1;
pub const TMN0_THINK_TEAM0_MOVE: i32 = 2;
pub const TMN0_THINK_TEAM0_TURN2: i32 = 3;
pub const TMN0_THINK_TEAM0_KANSEN: i32 = 4;
pub const TMN0_THINK_TEAM1_TURN: i32 = 5;
pub const TMN0_THINK_TEAM1_MOVE: i32 = 6;
pub const TMN0_THINK_TEAM1_TURN2: i32 = 7;
pub const TMN0_THINK_TEAM1_KANSEN: i32 = 8;

/// NPC1 think states (`ac_tamaire_npc1.c:10`).
pub const TMN1_THINK_BIRTH: i32 = 0;
pub const TMN1_THINK_KYORO_MAE: i32 = 1;
pub const TMN1_THINK_KYORO: i32 = 2;
pub const TMN1_THINK_WALK_TURN: i32 = 3;
pub const TMN1_THINK_WALK: i32 = 4;
pub const TMN1_THINK_HIROU_MAE: i32 = 5;
pub const TMN1_THINK_HIROU: i32 = 6;
pub const TMN1_THINK_HIROU_SP: i32 = 7;
pub const TMN1_THINK_HIROU_END: i32 = 8;
pub const TMN1_THINK_NAGERU: i32 = 9;
pub const TMN1_THINK_NAGERU_END: i32 = 10;

/// NPC action/step constants (`ac_npc.h`).
pub const NPC_ACT_TURN: i32 = 3;
pub const NPC_ACT_WALK: i32 = 1;
pub const NPC_ACTION_END_STEP: i32 = 0xFF;

/// Animation IDs (`ac_npc_anim_def.h`).
pub const ANIM_CLAP1: i32 = 67;
pub const ANIM_BANZAI1: i32 = 66;
pub const ANIM_KYORO1: i32 = 50;
pub const ANIM_TAMAHIROI1: i32 = 113;
pub const ANIM_TAMANAGE1: i32 = 114;
pub const ANIM_WAIT1: i32 = 5;

/// Computes NPC0's movement destination (`aTMN0_set_move_pos`):
/// Team 0 turn → origin + (80, 160); Team 0 turn 2 → origin +
/// (200, 40); Team 1 turn → origin + (−40, 160); every other
/// state → origin + (−160, 40) via the TRUE default branch.
/// Writes (x, z) to `out[0..2]` as s16 with wrapping; returns
/// 1 on success.
#[no_mangle]
pub extern "C" fn pc_tamaire_npc0_destination(
    origin_x: i32,
    origin_z: i32,
    think_idx: i32,
    out: *mut i16,
) -> u8 {
    if out.is_null() {
        return 0;
    }
    let (dx, dz): (i16, i16) = match think_idx {
        TMN0_THINK_TEAM0_TURN => (80, 160),
        TMN0_THINK_TEAM0_TURN2 => (200, 40),
        TMN0_THINK_TEAM1_TURN => (-40, 160),
        _ => (-160, 40),
    };
    let out = unsafe { std::slice::from_raw_parts_mut(out, 2) };
    out[0] = (origin_x as i16).wrapping_add(dx);
    out[1] = (origin_z as i16).wrapping_add(dz);
    1
}

/// Returns true when the turn state may advance: the action is
/// aNPC_ACT_TURN and its step is aNPC_ACTION_END_STEP.
#[no_mangle]
pub extern "C" fn pc_tamaire_npc0_turn_advance(action: i32, step: i32) -> bool {
    action == NPC_ACT_TURN && step == NPC_ACTION_END_STEP
}

/// NPC0 walk-end resolution: returns +1 when the walk ended and
/// the engine destination matches move_pos (advance), −1 when
/// the walk ended but the destination drifted (roll think_idx
/// backward), 0 when the walk has not ended.
#[no_mangle]
pub extern "C" fn pc_tamaire_npc0_move_step(
    walk_ended: bool,
    dst_x: i16,
    dst_z: i16,
    move_x: i16,
    move_z: i16,
) -> i8 {
    if !walk_ended {
        return 0;
    }
    if dst_x == move_x && dst_z == move_z {
        1
    } else {
        -1
    }
}

/// Selects the cheer animation from the C-side sample:
/// `RANDOM_F(1.0f) < 0.5f` → CLAP1, else BANZAI1.
#[no_mangle]
pub extern "C" fn pc_tamaire_cheer_anim(rand_float: f32) -> i32 {
    if rand_float < 0.5 {
        ANIM_CLAP1
    } else {
        ANIM_BANZAI1
    }
}

/// Computes NPC1's team-relative origin (`aTMN1_birth`): z is
/// always origin z + 40; x is origin x + 200 when
/// `((npc_id − TAMAIRE_1) & 2) != 0`, else origin x − 160.
/// Writes (x, z) to `out[0..2]` as s16 with wrapping; returns
/// 1 on success.
#[no_mangle]
pub extern "C" fn pc_tamaire_npc1_origin(
    origin_x: i32,
    origin_z: i32,
    npc_id: u16,
    out: *mut i16,
) -> u8 {
    if out.is_null() {
        return 0;
    }
    let dx: i16 = if npc_id.wrapping_sub(SP_NPC_EV_TAMAIRE_1) & 2 != 0 {
        200
    } else {
        -160
    };
    let out = unsafe { std::slice::from_raw_parts_mut(out, 2) };
    out[0] = (origin_x as i16).wrapping_add(dx);
    out[1] = (origin_z as i16).wrapping_add(40);
    1
}

/// The raw team flag bit for an NPC1 ID: 1 when
/// `((npc_id − TAMAIRE_1) & 2)` is clear, 2 when set. NOT
/// normalized to 0/1.
#[no_mangle]
pub extern "C" fn pc_tamaire_team_bit(npc_id: u16) -> u16 {
    if npc_id.wrapping_sub(SP_NPC_EV_TAMAIRE_1) & 2 != 0 {
        2
    } else {
        1
    }
}

/// The team-like bit passed to the throw effect:
/// `((npc_id − TAMAIRE_1) >> 1) & 1`.
#[no_mangle]
pub extern "C" fn pc_tamaire_throw_team_bit(npc_id: u16) -> i32 {
    ((npc_id.wrapping_sub(SP_NPC_EV_TAMAIRE_1) >> 1) & 1) as i32
}

/// Ball tool IDs (`{TOOL_TAMA2, TOOL_TAMA1, TOOL_TAMA4,
/// TOOL_TAMA3}` = {49, 48, 51, 50}).
pub const TOOL_TABLE: [u8; 4] = [49, 48, 51, 50];

/// Selects the hand ball tool ID (`aTMN1_tama_process`):
/// index = `((((npc_id − TAMAIRE_1) >> 1) & 1) ^ 1)` plus 2
/// for the left hand. The table order is preserved verbatim.
#[no_mangle]
pub extern "C" fn pc_tamaire_tool_id(npc_id: u16, left_hand: bool) -> u8 {
    let mut idx = ((((npc_id.wrapping_sub(SP_NPC_EV_TAMAIRE_1) >> 1) & 1) ^ 1) & 1) as usize;
    if left_hand {
        idx += 2;
    }
    TOOL_TABLE[idx]
}

/// The angular addend for the throw effect:
/// `RANDOM(3000) + DEG2SHORT_ANGLE2(67.5f)`; the RANDOM(3000)
/// sample stays in C and is consumed only when the effect is
/// created (frame 15 with right == 1).
pub const THROW_ANGLE_BASE: i32 = 12288;

/// Adds the retail angle base to the C-side random sample.
#[no_mangle]
pub extern "C" fn pc_tamaire_throw_angle(rand3000: i32) -> i32 {
    rand3000 + THROW_ANGLE_BASE
}

/// Computes the random run destination (`aTMN1_Next_move`):
/// `angle = trunc(r_mag − tamakago_angle)` as s16 (C's f32→s16
/// truncation, not rounding), then the destination is
/// origin + (sin_s(angle)·r_rad, cos_s(angle)·r_rad). All
/// RANDOM_F samples stay in C in the original order
/// (1500/2000 magnitude, sign, 20/70 radius); sin_s/cos_s
/// take the short angle and stay in C. This helper performs
/// the angle subtraction; the C caller does the trig.
/// Returns the truncated s16 angle.
#[no_mangle]
pub extern "C" fn pc_tamaire_next_move_angle(r_mag: f32, tamakago_angle: i16) -> i16 {
    (r_mag - tamakago_angle as f32) as i16
}

/// Pickup frame targets: {15, 27, 39}, edge-crossed via
/// cKF_FrameControl_passCheck_now (never `==`).
pub const PICKUP_FRAMES: [u8; 3] = [15, 27, 39];

/// Returns the pickup-frame index (0–2) for a crossed frame, or
/// -1 when the frame is not a target.
#[no_mangle]
pub extern "C" fn pc_tamaire_pickup_frame(frame: i32) -> i32 {
    match frame as u8 {
        15 => 0,
        27 => 1,
        39 => 2,
        _ => -1,
    }
}

/// Returns true while another throw should follow: `left != 0`
/// after the throw animation stops.
#[no_mangle]
pub extern "C" fn pc_tamaire_throw_again(left: u8) -> bool {
    left != 0
}

/// NPC0 dialogue bases per looks index (literal values).
pub const NPC0_MSG_BASE: [i32; 6] = [7781, 7793, 7769, 7805, 7817, 7829];
/// NPC1 talk offsets per talk row (`dt_tbl` msg_idx).
pub const NPC1_MSG_OFFSET: [i32; 6] = [3, 3, 3, 6, 6, 9];

/// Computes a dialogue message: `base + offset + rand3`. The
/// RANDOM(3) sample stays in C.
#[no_mangle]
pub extern "C" fn pc_tamaire_talk_msg(base: i32, offset: i32, rand3: i32) -> i32 {
    base + offset + rand3
}

/// NPC0's message base for a looks index.
#[no_mangle]
pub extern "C" fn pc_tamaire_npc0_msg_base(looks: i32) -> i32 {
    NPC0_MSG_BASE[looks.rem_euclid(6) as usize]
}

/// NPC1's talk offset for a talk row.
#[no_mangle]
pub extern "C" fn pc_tamaire_npc1_msg_offset(talk_idx: i32) -> i32 {
    if talk_idx < 0 || talk_idx >= NPC1_MSG_OFFSET.len() as i32 {
        -1
    } else {
        NPC1_MSG_OFFSET[talk_idx as usize]
    }
}

/// Ball-prop lifetime (`ac_tama_move.c_inc`): the prop
/// self-deletes when the player is outside every scroll-demo
/// state AND the block differs in either axis — a STRICT `!=`
/// test, so adjacent blocks DO delete it (opposite of the
/// turi stand's `ABS > 1`).
#[no_mangle]
pub extern "C" fn pc_tama_should_delete(scroll_demo: bool, bx_diff: i32, bz_diff: i32) -> bool {
    !scroll_demo && (bx_diff != 0 || bz_diff != 0)
}

/// Sports Fair ball-toss placement: ten slots of (field-unit
/// X, Z): TAMAIRE_0..4, red ball, red basket, white ball,
/// white basket, Tortimer.
pub const SPAWN_LAYOUT: [(i32, i32); 10] = [
    (7, 12),
    (5, 6),
    (2, 9),
    (13, 6),
    (10, 9),
    (3, 8),
    (3, 8),
    (12, 8),
    (12, 8),
    (9, 4),
];
/// NPC slot mask `0b0011111` in the data record.
pub const SPAWN_NPC_MASK: u32 = 0b0011111;

/// Writes the (x, z) field-unit position of a spawn slot to
/// `out[0..2]`; returns 1 on success. Slots are 0-based.
#[no_mangle]
pub extern "C" fn pc_tamaire_spawn_pos(slot: u8, out: *mut i32) -> u8 {
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

    fn dest(ox: i32, oz: i32, think: i32) -> (i16, i16) {
        let mut out = [0i16; 2];
        assert_eq!(pc_tamaire_npc0_destination(ox, oz, think, out.as_mut_ptr()), 1);
        (out[0], out[1])
    }

    #[test]
    fn npc0_destinations() {
        // True default branch preserved for TEAM1_TURN2 and unknown states.
        assert_eq!(dest(100, 200, TMN0_THINK_TEAM0_TURN), (180, 360));
        assert_eq!(dest(100, 200, TMN0_THINK_TEAM0_TURN2), (300, 240));
        assert_eq!(dest(100, 200, TMN0_THINK_TEAM1_TURN), (60, 360));
        assert_eq!(dest(100, 200, TMN0_THINK_TEAM1_TURN2), (-60, 240));
        assert_eq!(dest(100, 200, TMN0_THINK_BIRTH), (-60, 240));
        assert_eq!(dest(100, 200, 99), (-60, 240));
        // Wrapping s16 arithmetic.
        assert_eq!(dest(32760, 0, TMN0_THINK_TEAM0_TURN), (32760i16.wrapping_add(80), 160));
    }

    #[test]
    fn npc0_transitions() {
        assert!(pc_tamaire_npc0_turn_advance(NPC_ACT_TURN, NPC_ACTION_END_STEP));
        assert!(!pc_tamaire_npc0_turn_advance(NPC_ACT_TURN, 0));
        assert!(!pc_tamaire_npc0_turn_advance(NPC_ACT_WALK, NPC_ACTION_END_STEP));
        // Walk-end: +1 advance, -1 rollback, 0 not ended.
        assert_eq!(pc_tamaire_npc0_move_step(true, 10, 20, 10, 20), 1);
        assert_eq!(pc_tamaire_npc0_move_step(true, 10, 21, 10, 20), -1);
        assert_eq!(pc_tamaire_npc0_move_step(false, 10, 20, 10, 20), 0);
        assert_eq!(pc_tamaire_cheer_anim(0.4), ANIM_CLAP1);
        assert_eq!(pc_tamaire_cheer_anim(0.5), ANIM_BANZAI1);
    }

    #[test]
    fn npc1_origin_and_bits() {
        let mut out = [0i16; 2];
        // (id - 0xD054) & 2: 0xD054 -> clear -> -160; 0xD056 -> set -> +200.
        assert_eq!(pc_tamaire_npc1_origin(100, 200, 0xD054, out.as_mut_ptr()), 1);
        assert_eq!((out[0], out[1]), (-60, 240));
        assert_eq!(pc_tamaire_npc1_origin(100, 200, 0xD056, out.as_mut_ptr()), 1);
        assert_eq!((out[0], out[1]), (300, 240));
        // Raw bits 1/2, not normalized.
        assert_eq!(pc_tamaire_team_bit(0xD054), 1);
        assert_eq!(pc_tamaire_team_bit(0xD055), 1);
        assert_eq!(pc_tamaire_team_bit(0xD056), 2);
        assert_eq!(pc_tamaire_team_bit(0xD057), 2);
        assert_eq!(pc_tamaire_throw_team_bit(0xD054), 0);
        assert_eq!(pc_tamaire_throw_team_bit(0xD056), 1);
    }

    #[test]
    fn tool_table() {
        // ((((id - 0xD054) >> 1) & 1) ^ 1): 0xD054 -> 1 -> TOOL_TAMA1 (48).
        assert_eq!(pc_tamaire_tool_id(0xD054, false), 48);
        assert_eq!(pc_tamaire_tool_id(0xD054, true), 50);
        assert_eq!(pc_tamaire_tool_id(0xD056, false), 49);
        assert_eq!(pc_tamaire_tool_id(0xD056, true), 51);
        assert_eq!(pc_tamaire_throw_angle(1500), 12288 + 1500);
    }

    #[test]
    fn move_angle_and_frames() {
        // Truncation toward zero, not rounding.
        assert_eq!(pc_tamaire_next_move_angle(100.9, 10), 90);
        assert_eq!(pc_tamaire_next_move_angle(-100.9, 10), -110);
        assert_eq!(pc_tamaire_pickup_frame(15), 0);
        assert_eq!(pc_tamaire_pickup_frame(27), 1);
        assert_eq!(pc_tamaire_pickup_frame(39), 2);
        assert_eq!(pc_tamaire_pickup_frame(16), -1);
        assert!(pc_tamaire_throw_again(2));
        assert!(!pc_tamaire_throw_again(0));
    }

    #[test]
    fn dialogue_and_spawn() {
        assert_eq!(pc_tamaire_npc0_msg_base(0), 7781);
        assert_eq!(pc_tamaire_npc0_msg_base(5), 7829);
        assert_eq!(pc_tamaire_npc1_msg_offset(3), 6);
        assert_eq!(pc_tamaire_npc1_msg_offset(5), 9);
        assert_eq!(pc_tamaire_npc1_msg_offset(6), -1);
        assert_eq!(pc_tamaire_talk_msg(7781, 0, 2), 7783);
        // Ball prop: strict != — adjacent blocks DO delete.
        assert!(pc_tama_should_delete(false, 1, 0));
        assert!(pc_tama_should_delete(false, 0, 1));
        assert!(!pc_tama_should_delete(false, 0, 0));
        assert!(!pc_tama_should_delete(true, 5, 5));
        let mut out = [0i32; 2];
        assert_eq!(pc_tamaire_spawn_pos(0, out.as_mut_ptr()), 1);
        assert_eq!(out, [7, 12]);
        assert_eq!(pc_tamaire_spawn_pos(9, out.as_mut_ptr()), 1);
        assert_eq!(out, [9, 4]);
        assert_eq!(pc_tamaire_spawn_pos(10, out.as_mut_ptr()), 0);
    }
}
