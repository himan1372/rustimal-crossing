//! The Field Day Footrace (`ac_tokyoso_control.c`,
//! `ac_tokyoso_npc0.c`, `ac_tokyoso_npc1.c`).
//!
//! Verified against `src/actor/ac_tokyoso_control.c`,
//! `ac_tokyoso_clip.c_inc`, `include/ac_tokyoso_control.h`,
//! `src/actor/npc/ac_tokyoso_npc0.c`,
//! `ac_tokyoso_npc0_schedule.c_inc`, `ac_tokyoso_npc0_talk.c_inc`,
//! `include/ac_tokyoso_npc0.h`, `src/actor/npc/ac_tokyoso_npc1.c`,
//! `ac_tokyoso_npc1_schedule.c_inc`, `ac_tokyoso_npc1_talk.c_inc`,
//! `include/ac_tokyoso_npc1.h`, `src/game/m_event_map_npc_data.c_inc`,
//! `src/game/m_event_schedule.c_inc`, `include/ac_npc_anim_def.h`
//! (GAFE01_00 Rev 0 USA). All 11 sections PROVEN with
//! DIFFERS-level nuances (see below).
//!
//! Naming note: despite the "summer festival stalls" request
//! title, these files implement the Field Day Footrace (the
//! save area is keyed to mEv_EVENT_SPORTS_FAIR_FOOT_RACE and
//! the map setup calls it fdd_tokyoso). They are ported as the
//! footrace. Real stall behavior lives under the Fireworks
//! Show's YOMISE/FIREWORKS_STALL actors — a separate future
//! commission.
//!
//! Scope split: the two schedule windows (Mar 19–21 11:00–
//! 12:00 and Sep 21–23 11:00–12:00) live in the sibling
//! `event_day` module. This module owns the pure kernels:
//! the NPC index/team/role decomposition, the flag-bit
//! shifts, the lap-crossing test, the goal decision, the run
//! target/speed math, the run/top checks, and the dialogue
//! arithmetic. The controller state machine, the shared save
//! area (slot 8), NPC clip/action requests, animation frame
//! control, all RNG call order, the eEC effects, sounds,
//! talk/demo, and the map placement stay in C.
//!
//! Nuances carried: team is the raw low bit (0/1), role is the
//! raw second bit (0/2, never normalized); NPC1's birth role
//! test subtracts 1 from the NPC ID BEFORE the mask
//! (TOKYOSO_0/3/4 = spectators, TOKYOSO_1/2 = runners) — the
//! brief missed the −1; `aTKC_set` fires on `(flags &
//! READY_ALL) == 0` (mask empty), not "incomplete"; the
//! `@BUG` double `setupAction(GO)` is preserved as a
//! documented call-order note (the C keeps it); the run
//! destination is `+cos_s` for X but `−sin_s` for Z; the
//! finish-branch draws are `(RANDOM_F(10) − 5)` centered;
//! `angle += (s16)(4000 + rand)` truncates toward zero;
//! NPC0's destructor ASSIGNS `flags = DELETE_NPC0` (wipes
//! other bits — do not "fix" to OR); NPC1's `_00 == 3`
//! self-delete lives in the MOVE callback, not the
//! destructor; the retail `run_check` is `angle > −1000 &&
//! angle < 4096` (the BUGFIXES variant differs — retail
//! keeps the −1000 form); `aTKC_FLAG_15` and the full `_01`
//! semantics are unresolved and stay in C.

/// First footrace NPC ID (`m_name_table.h`).
pub const SP_NPC_EV_TOKYOSO_0: u16 = 0xD02D;

/// Lap-completion angle threshold (~22°).
pub const ANGLE_FOR_LAP_COMPLETION: i16 = 4000;
/// Laps per race × 2 half-circuit segments.
pub const SUBLAP_NUM: i32 = 8;
/// Goal results.
pub const GOAL_NONE: i32 = 0;
pub const GOAL_WON: i32 = 1;
pub const GOAL_LOSE: i32 = 2;

/// NPC index: `aTKC_NPCID2IDX(id) = id − SP_NPC_EV_TOKYOSO_0`.
#[no_mangle]
pub extern "C" fn pc_tokyoso_npc_index(npc_id: u16) -> i32 {
    npc_id.wrapping_sub(SP_NPC_EV_TOKYOSO_0) as i32
}

/// Team: `aTKC_NPCID2TEAM(id) = idx & 1` — raw 0 or 1.
#[no_mangle]
pub extern "C" fn pc_tokyoso_team(npc_id: u16) -> i32 {
    pc_tokyoso_npc_index(npc_id) & 1
}

/// Role: `aTKC_NPCID2ROLE(id) = idx & 2` — raw 0 (runner) or
/// 2 (spectator), never normalized.
#[no_mangle]
pub extern "C" fn pc_tokyoso_role(npc_id: u16) -> i32 {
    pc_tokyoso_npc_index(npc_id) & 2
}

/// NPC1's birth role test: `aTKC_NPCID2ROLE(npc_id − 1)` — the
/// brief missed the −1 shift. Result: TOKYOSO_0/3/4 are
/// spectators (2), TOKYOSO_1/2 are runners (0).
#[no_mangle]
pub extern "C" fn pc_tokyoso_birth_role(npc_id: u16) -> i32 {
    npc_id.wrapping_sub(1).wrapping_sub(SP_NPC_EV_TOKYOSO_0) as i32 & 2
}

/// Flag-bit bases (controller `flags` field).
pub const FLAG_DELETE_NPC0: u16 = 1 << 0;
pub const FLAG_STRETCH_TEAM0: u16 = 1 << 5;
pub const FLAG_READY_TEAM0: u16 = 1 << 7;
pub const FLAG_READY_SHOOTER: u16 = 1 << 9;
pub const FLAG_RACE_ACTIVE: u16 = 1 << 10;
pub const FLAG_KANSEN_TEAM0: u16 = 1 << 11;
pub const FLAG_GOAL_TEAM0: u16 = 1 << 13;

/// `aTKC_NPCIDX2DELETEFLG(n) = DELETE_NPC0 << idx`.
#[no_mangle]
pub extern "C" fn pc_tokyoso_delete_flag(npc_id: u16) -> u16 {
    FLAG_DELETE_NPC0 << ((pc_tokyoso_npc_index(npc_id) as u32) & 15)
}

/// `aTKC_NPCIDX2STRETCHFLG(n) = STRETCH_TEAM0 << team`.
#[no_mangle]
pub extern "C" fn pc_tokyoso_stretch_flag(npc_id: u16) -> u16 {
    FLAG_STRETCH_TEAM0 << (pc_tokyoso_team(npc_id) as u32)
}

/// `aTKC_NPCIDX2READYLFG(n) = READY_TEAM0 << team` (macro name
/// misspelled in source).
#[no_mangle]
pub extern "C" fn pc_tokyoso_ready_flag(npc_id: u16) -> u16 {
    FLAG_READY_TEAM0 << (pc_tokyoso_team(npc_id) as u32)
}

/// `aTKC_NPCIDX2KANSENFLG(n) = KANSEN_TEAM0 << team`.
#[no_mangle]
pub extern "C" fn pc_tokyoso_kansen_flag(npc_id: u16) -> u16 {
    FLAG_KANSEN_TEAM0 << (pc_tokyoso_team(npc_id) as u32)
}

/// `aTKC_NPCIDX2GOALFLG(n) = GOAL_TEAM0 << team`.
#[no_mangle]
pub extern "C" fn pc_tokyoso_goal_flag(npc_id: u16) -> u16 {
    FLAG_GOAL_TEAM0 << (pc_tokyoso_team(npc_id) as u32)
}

/// Returns true when the runner's angle crosses the lap
/// threshold for its current lap (`aTKC_go`): odd lap →
/// `angle < −4000`; even lap → `angle > 4000`.
#[no_mangle]
pub extern "C" fn pc_tokyoso_lap_advance(lap: i32, angle: i16) -> bool {
    if lap & 1 != 0 {
        angle < -ANGLE_FOR_LAP_COMPLETION
    } else {
        angle > ANGLE_FOR_LAP_COMPLETION
    }
}

/// Goal decision (`aTKC_go`): when `lap > 8` and the runner
/// has no goal yet — WON if the other runner has no goal,
/// otherwise LOSE. Returns GOAL_NONE otherwise.
#[no_mangle]
pub extern "C" fn pc_tokyoso_goal_decision(lap: i32, own_goal: i32, other_goal: i32) -> i32 {
    if lap > SUBLAP_NUM && own_goal == GOAL_NONE {
        if other_goal == GOAL_NONE {
            GOAL_WON
        } else {
            GOAL_LOSE
        }
    } else {
        GOAL_NONE
    }
}

/// The next angular target (`aTKC_clip_next_run`):
/// `angle + (s16)(4000 + rand4000)`, with C's truncating
/// f32→s16 conversion. The RANDOM_F(4000) sample stays in C.
#[no_mangle]
pub extern "C" fn pc_tokyoso_next_angle(angle: i16, rand4000: f32) -> i16 {
    angle.wrapping_add((ANGLE_FOR_LAP_COMPLETION as f32 + rand4000) as i16)
}

/// Run speed (`aTKC_clip_next_run`):
/// `(4.0 + dist·(1/90)) − idx·0.5`, minus 1.0 when the
/// runner is currently ahead. `f` is the distance; all
/// RANDOM_F samples stay in C.
#[no_mangle]
pub extern "C" fn pc_tokyoso_run_speed(dist: f32, idx: i32, ahead: bool) -> f32 {
    let speed = (4.0 + dist * (1.0 / 90.0)) - idx as f32 * 0.5;
    if ahead {
        speed - 1.0
    } else {
        speed
    }
}

/// Run destination (`aTKC_clip_next_run`): X = origin_x +
/// dist·cos_s(angle), Z = origin_z − dist·sin_s(angle) —
/// note the MINUS on Z. The C caller supplies the short-angle
/// trig values; writes (x, z) to `out[0..2]`.
#[no_mangle]
pub extern "C" fn pc_tokyoso_run_target(
    origin_x: f32,
    origin_z: f32,
    dist: f32,
    cos_s: f32,
    sin_s: f32,
    out: *mut f32,
) -> u8 {
    if out.is_null() {
        return 0;
    }
    let out = unsafe { std::slice::from_raw_parts_mut(out, 2) };
    out[0] = origin_x + dist * cos_s;
    out[1] = origin_z - dist * sin_s;
    1
}

/// Finish-line target (`aTKC_clip_next_run` special branch,
/// lap == 8 and next angle positive): X = origin_x + 220 −
/// idx·60 + (sample_x − 5), Z = origin_z − 130 + idx·60 +
/// (sample_z − 5). The two RANDOM_F(10.0f) samples stay in C.
/// Writes (x, z) to `out[0..2]`.
#[no_mangle]
pub extern "C" fn pc_tokyoso_finish_target(
    origin_x: f32,
    origin_z: f32,
    idx: i32,
    sample_x: f32,
    sample_z: f32,
    out: *mut f32,
) -> u8 {
    if out.is_null() {
        return 0;
    }
    let i = idx as f32;
    let out = unsafe { std::slice::from_raw_parts_mut(out, 2) };
    out[0] = origin_x + 220.0 - i * 60.0 + (sample_x - 5.0);
    out[1] = origin_z - 130.0 + i * 60.0 + (sample_z - 5.0);
    1
}

/// The retail run-check (`aTKC_clip_run_check`, non-BUGFIXES
/// branch): `angle > −1000 && angle < 4096`
/// (DEG2SHORT_ANGLE2(22.5°) = 4096). The BUGFIXES variant
/// uses `> −4096`; retail keeps the −1000 form.
#[no_mangle]
pub extern "C" fn pc_tokyoso_run_check(angle_diff: i16) -> bool {
    angle_diff > -1000 && angle_diff < 4096
}

/// Returns true when the runner is ahead
/// (`aTKC_clip_top_check` without the race-active gate, which
/// stays in C): higher lap wins; tied laps fall back to a
/// positive angle difference.
#[no_mangle]
pub extern "C" fn pc_tokyoso_is_ahead(my_lap: i32, other_lap: i32, angle_diff: i16) -> bool {
    my_lap > other_lap || (my_lap == other_lap && angle_diff > 0)
}

/// Warmup destination X bases per runner index
/// (`aTKC_clip_next_warmup`).
pub const WARMUP_TABLE_X: [i32; 2] = [200, 160];

/// Warmup start position: X = origin_x + tableX[idx] +
/// trunc(sample_x) (the C-side RANDOM_F(±40.0f) sample, cast
/// with (s16) truncation), Z = origin_z + trunc(sample_z −
/// 40.0) (the C-side RANDOM_F(80.0f) sample). Writes (x, z)
/// to `out[0..2]` as s16 with wrapping; returns 1 on
/// success, 0 for an invalid runner index.
#[no_mangle]
pub extern "C" fn pc_tokyoso_warmup_target(
    origin_x: i32,
    origin_z: i32,
    idx: i32,
    sample_x: f32,
    sample_z: f32,
    out: *mut i16,
) -> u8 {
    if out.is_null() || idx < 0 || idx >= WARMUP_TABLE_X.len() as i32 {
        return 0;
    }
    let out = unsafe { std::slice::from_raw_parts_mut(out, 2) };
    out[0] = (origin_x as i16)
        .wrapping_add(WARMUP_TABLE_X[idx as usize] as i16)
        .wrapping_add(sample_x as i16);
    out[1] = (origin_z as i16).wrapping_add((sample_z - 40.0) as i16);
    1
}

/// NPC0 dialogue bases per looks index (literal values).
pub const NPC0_MSG_BASE: [i32; 6] = [6621, 6637, 6605, 6653, 6669, 6685];
/// NPC1 talk offsets (13-entry `dt_tbl` msg_no values).
pub const NPC1_MSG_OFFSET: [i32; 13] = [14, 2, 14, 4, 6, 8, 8, 10, 10, 12, 12, 12, 12];

/// Computes a dialogue message: `base + offset + rand2`. The
/// RANDOM(2) sample stays in C.
#[no_mangle]
pub extern "C" fn pc_tokyoso_talk_msg(base: i32, offset: i32, rand2: i32) -> i32 {
    base + offset + rand2
}

/// NPC0's message base for a looks index.
#[no_mangle]
pub extern "C" fn pc_tokyoso_npc0_msg_base(looks: i32) -> i32 {
    NPC0_MSG_BASE[looks.rem_euclid(6) as usize]
}

/// NPC1's talk offset for a talk row.
#[no_mangle]
pub extern "C" fn pc_tokyoso_npc1_msg_offset(talk_idx: i32) -> i32 {
    if talk_idx < 0 || talk_idx >= NPC1_MSG_OFFSET.len() as i32 {
        -1
    } else {
        NPC1_MSG_OFFSET[talk_idx as usize]
    }
}

/// Footrace shrine placement: six slots of (field-unit X, Z):
/// TOKYOSO_0..4, Tortimer.
pub const SPAWN_LAYOUT: [(i32, i32); 6] = [
    (13, 3),
    (11, 7),
    (12, 7),
    (14, 7),
    (14, 8),
    (5, 8),
];
/// NPC slot mask `0b0011111` in the data record.
pub const SPAWN_NPC_MASK: u32 = 0b0011111;

/// Writes the (x, z) field-unit position of a spawn slot to
/// `out[0..2]`; returns 1 on success. Slots are 0-based.
#[no_mangle]
pub extern "C" fn pc_tokyoso_spawn_pos(slot: u8, out: *mut i32) -> u8 {
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


/// Controller action enum (`ac_tokyoso_control.c`), separate from the
/// shared save field `_00`.
pub const ACT_WAIT: i32 = 0;
pub const ACT_WAIT2: i32 = 1;
pub const ACT_STRECH: i32 = 2;
pub const ACT_READY: i32 = 3;
pub const ACT_SET: i32 = 4;
pub const ACT_GO: i32 = 5;
pub const ACT_GOAL: i32 = 6;
pub const ACT_IREKAE_1: i32 = 7;
pub const ACT_IREKAE_2: i32 = 8;
/// Shared save-area identity: event 15 (0xF), slot 8.
pub const FOOTRACE_EVENT: i32 = 15;
pub const FOOTRACE_SLOT: i32 = 8;
/// Controller frame timers.
pub const TIMER_WAIT: i32 = 600;
pub const TIMER_READY: i32 = 120;
pub const TIMER_WARMUP: i32 = 240;
pub const TIMER_RUN: i32 = 300;
/// Combined flag masks (`ac_tokyoso_control.h`).
pub const READY_ALL: u16 = 0x380;
pub const STRETCH_ALL: u16 = 0x60;
pub const KANSEN_ALL: u16 = 0x1800;
pub const GOAL_ALL: u16 = 0x6000;
/// Bit 15 is declared (`aTKC_FLAG_15`) but has zero uses and zero
/// `0x8000` literals anywhere in the tokyoso sources — reserved, not
/// repurposed. Preserve it through read-modify-write.
pub const FLAG_15: u16 = 0x8000;

/// WAIT2 override (`aTKC_actor_move`): when `_00 == 3` and the current
/// action is not WAIT2, the controller is forced into WAIT2 (the
/// NPC0-deletion recovery path).
#[no_mangle]
pub extern "C" fn pc_tokyoso_wait2_override(phase: u8, action: i32) -> i32 {
    if phase == 3 && action != ACT_WAIT2 {
        ACT_WAIT2
    } else {
        action
    }
}

/// `aTKC_set` start predicate: the race starts when the full ready
/// mask is NOT present — `(flags & READY_ALL) == 0`. NPCs clearing
/// their own ready bits is the start trigger. The C body then calls
/// `aTKC_setupAction(actor, GO)` TWICE (decomp `// @BUG`), with the
/// flags/lap/goal initialization between the two calls — the call
/// order is part of the retail behavior, not cleaned up here.
#[no_mangle]
pub extern "C" fn pc_tokyoso_set_starts(flags: u16) -> i32 {
    (flags & READY_ALL == 0) as i32
}

/// `_01` spectator-transition bit operations. NOTE the asymmetry: the
/// goal-init SET uses `(2 + idx) & 3` while the spectator CLEAR uses
/// `idx & 3`, where `idx = npc_id - SP_NPC_EV_TOKYOSO_0`.
#[no_mangle]
pub extern "C" fn pc_tokyoso_01_set(u01: u8, npc_id: u16) -> u8 {
    let idx = npc_id.wrapping_sub(SP_NPC_EV_TOKYOSO_0);
    u01 | (1u8 << ((2 + idx) & 3))
}

#[no_mangle]
pub extern "C" fn pc_tokyoso_01_clear(u01: u8, npc_id: u16) -> u8 {
    let idx = npc_id.wrapping_sub(SP_NPC_EV_TOKYOSO_0);
    u01 & !(1u8 << (idx & 3))
}

/// `aTKN1_kansen_bf` leaves audience setup when RACE_ACTIVE is set or
/// its `_01` bit is set.
#[no_mangle]
pub extern "C" fn pc_tokyoso_kansen_bf_leave(race_active: i32, u01_bit: i32) -> i32 {
    (race_active != 0 || u01_bit != 0) as i32
}

/// `aTKN1_kansen` leaves ordinary audience behavior when either
/// runner has a nonzero goal result or its `_01` bit is set.
#[no_mangle]
pub extern "C" fn pc_tokyoso_kansen_leave(goal0: i32, goal1: i32, u01_bit: i32) -> i32 {
    (goal0 != 0 || goal1 != 0 || u01_bit != 0) as i32
}

/// Dash-effect roll (`think_idx == 11`): `RANDOM_F(1.0f)` is drawn on
/// EVERY running-state update (even when no effect spawns); the effect
/// fires when the sample is below 0.1.
#[no_mangle]
pub extern "C" fn pc_tokyoso_dash_roll(rng: f32) -> i32 {
    (rng < 0.1f32) as i32
}

/// Stumble selection: only in the lead-check branch — the runner must
/// be leading AND the sample below 0.1, otherwise it stays in the
/// normal running state (think 11 vs 12).
#[no_mangle]
pub extern "C" fn pc_tokyoso_stumble_roll(leading: i32, rng: f32) -> i32 {
    (leading != 0 && rng < 0.1f32) as i32
}

/// Stumble exit (`aTKN1_kokeru`): the stumble animation's stop
/// procedure must return TRUE AND `speed == 0.0f` before the runner
/// transitions (to think 13, the get-up init).
#[no_mangle]
pub extern "C" fn pc_tokyoso_kokeru_done(anim_stopped: i32, speed: f32) -> i32 {
    (anim_stopped != 0 && speed == 0.0f32) as i32
}

/// Late-race target override (`aTKC_clip_next_run`): when the lap
/// count equals SUBLAP_NUM (8) and the next angle is positive, the
/// target is overridden with two extra `RANDOM_F(10.0f)` perturbations.
#[no_mangle]
pub extern "C" fn pc_tokyoso_late_race(lap: i32, next_angle: i16) -> i32 {
    (lap == SUBLAP_NUM && next_angle > 0) as i32
}

/// Goal-result think mapping (`aTKN1_local_to_goal`): think `13 + goal`.
#[no_mangle]
pub extern "C" fn pc_tokyoso_goal_think(goal: i32) -> i32 {
    13 + goal
}

/// NPC0 birth recovery (`ac_tokyoso_npc0.c`): a newly created NPC0
/// seeing `_00 == 3` resets it to 0 and clears flags (sets
/// `*did_reset`); otherwise the phase passes through.
#[no_mangle]
pub extern "C" fn pc_tokyoso_npc0_birth(phase: u8, did_reset: *mut i32) -> u8 {
    let reset = (phase == 3) as i32;
    unsafe {
        if !did_reset.is_null() {
            *did_reset = reset;
        }
    }
    if reset != 0 {
        0
    } else {
        phase
    }
}

/// NPC1 birth requires `_00 == 2` (set by NPC0's schedule birth from
/// `_00 == 1` — the phase the brief's lifecycle table missed).
#[no_mangle]
pub extern "C" fn pc_tokyoso_npc1_birth_ok(phase: u8) -> i32 {
    (phase == 2) as i32
}

/// NPC1's `_00 == 3` self-delete lives in its MOVE callback (not its
/// destructor, unlike NPC0's flag assignment).
#[no_mangle]
pub extern "C" fn pc_tokyoso_npc1_self_delete(phase: u8) -> i32 {
    (phase == 3) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_decomposition() {
        assert_eq!(pc_tokyoso_npc_index(0xD02D), 0);
        assert_eq!(pc_tokyoso_npc_index(0xD031), 4);
        // Raw team 0/1, raw role 0/2.
        assert_eq!(pc_tokyoso_team(0xD02D), 0);
        assert_eq!(pc_tokyoso_team(0xD02E), 1);
        assert_eq!(pc_tokyoso_role(0xD02D), 0);
        assert_eq!(pc_tokyoso_role(0xD02F), 2);
        // Birth role: (id - 1 - 0xD02D) & 2 — the -1 shift.
        assert_eq!(pc_tokyoso_birth_role(0xD02D), 2); // spectator
        assert_eq!(pc_tokyoso_birth_role(0xD02E), 0); // runner
        assert_eq!(pc_tokyoso_birth_role(0xD02F), 0); // runner
        assert_eq!(pc_tokyoso_birth_role(0xD030), 2); // spectator
        assert_eq!(pc_tokyoso_birth_role(0xD031), 2); // spectator
        assert_eq!(pc_tokyoso_delete_flag(0xD02D), 0x0001);
        assert_eq!(pc_tokyoso_delete_flag(0xD031), 0x0010);
        assert_eq!(pc_tokyoso_stretch_flag(0xD02D), 1 << 5);
        assert_eq!(pc_tokyoso_stretch_flag(0xD02E), 1 << 6);
        assert_eq!(pc_tokyoso_ready_flag(0xD02E), 1 << 8);
        assert_eq!(pc_tokyoso_kansen_flag(0xD02D), 1 << 11);
        assert_eq!(pc_tokyoso_goal_flag(0xD02E), 1 << 14);
    }

    #[test]
    fn lap_and_goal() {
        // Odd lap: angle < -4000; even lap: angle > 4000.
        assert!(pc_tokyoso_lap_advance(1, -4001));
        assert!(!pc_tokyoso_lap_advance(1, -4000));
        assert!(pc_tokyoso_lap_advance(2, 4001));
        assert!(!pc_tokyoso_lap_advance(2, 4000));
        assert!(pc_tokyoso_lap_advance(0, 4001));
        // Goal: lap > 8 with no goal yet.
        assert_eq!(pc_tokyoso_goal_decision(9, 0, 0), GOAL_WON);
        assert_eq!(pc_tokyoso_goal_decision(9, 0, 1), GOAL_LOSE);
        assert_eq!(pc_tokyoso_goal_decision(8, 0, 0), GOAL_NONE);
        assert_eq!(pc_tokyoso_goal_decision(9, 1, 0), GOAL_NONE);
        assert_eq!(pc_tokyoso_next_angle(1000, 2500.0), 1000 + 6500);
        assert_eq!(pc_tokyoso_next_angle(-30000, 3999.9), (-30000i16).wrapping_add(7999));
    }

    #[test]
    fn run_math() {
        // (4 + dist/90) - idx*0.5, -1 when ahead.
        assert!((pc_tokyoso_run_speed(90.0, 0, false) - 5.0).abs() < 1e-5);
        assert!((pc_tokyoso_run_speed(90.0, 1, false) - 4.5).abs() < 1e-5);
        assert!((pc_tokyoso_run_speed(90.0, 1, true) - 3.5).abs() < 1e-5);
        let mut out = [0.0f32; 2];
        assert_eq!(pc_tokyoso_run_target(100.0, 200.0, 50.0, 1.0, 0.0, out.as_mut_ptr()), 1);
        assert_eq!(out, [150.0, 200.0]); // Z uses minus sin_s.
        assert_eq!(pc_tokyoso_run_target(100.0, 200.0, 50.0, 0.0, 1.0, out.as_mut_ptr()), 1);
        assert_eq!(out, [100.0, 150.0]);
        assert_eq!(pc_tokyoso_finish_target(100.0, 200.0, 1, 7.0, 3.0, out.as_mut_ptr()), 1);
        assert_eq!(out, [100.0 + 220.0 - 60.0 + 2.0, 200.0 - 130.0 + 60.0 - 2.0]);
        // Retail run check: > -1000 && < 4096.
        assert!(pc_tokyoso_run_check(0));
        assert!(pc_tokyoso_run_check(4095));
        assert!(!pc_tokyoso_run_check(4096));
        assert!(!pc_tokyoso_run_check(-1000));
        assert!(pc_tokyoso_run_check(-999));
        // Ahead: higher lap, or tied lap with positive angle diff.
        assert!(pc_tokyoso_is_ahead(5, 4, -100));
        assert!(pc_tokyoso_is_ahead(5, 5, 1));
        assert!(!pc_tokyoso_is_ahead(5, 5, 0));
        assert!(!pc_tokyoso_is_ahead(4, 5, 100));
    }

    #[test]
    fn warmup_and_dialogue() {
        let mut out = [0i16; 2];
        assert_eq!(pc_tokyoso_warmup_target(100, 200, 0, 39.9, 79.9, out.as_mut_ptr()), 1);
        assert_eq!(out, [100 + 200 + 39, 200 + 39]);
        assert_eq!(pc_tokyoso_warmup_target(100, 200, 1, -39.9, 0.5, out.as_mut_ptr()), 1);
        assert_eq!(out, [100 + 160 - 39, 200 - 39]);
        assert_eq!(pc_tokyoso_warmup_target(100, 200, 2, 0.0, 0.0, out.as_mut_ptr()), 0);
        assert_eq!(pc_tokyoso_npc0_msg_base(0), 6621);
        assert_eq!(pc_tokyoso_npc0_msg_base(5), 6685);
        assert_eq!(pc_tokyoso_npc1_msg_offset(0), 14);
        assert_eq!(pc_tokyoso_npc1_msg_offset(12), 12);
        assert_eq!(pc_tokyoso_npc1_msg_offset(13), -1);
        assert_eq!(pc_tokyoso_talk_msg(6621, 0, 1), 6622);
        let mut p = [0i32; 2];
        assert_eq!(pc_tokyoso_spawn_pos(0, p.as_mut_ptr()), 1);
        assert_eq!(p, [13, 3]);
        assert_eq!(pc_tokyoso_spawn_pos(5, p.as_mut_ptr()), 1);
        assert_eq!(p, [5, 8]);
        assert_eq!(pc_tokyoso_spawn_pos(6, p.as_mut_ptr()), 0);
    }

    #[test]
    fn controller_kernels() {
        // WAIT2 override: _00==3 forces WAIT2 unless already there.
        assert_eq!(pc_tokyoso_wait2_override(3, ACT_GO), ACT_WAIT2);
        assert_eq!(pc_tokyoso_wait2_override(3, ACT_WAIT2), ACT_WAIT2);
        assert_eq!(pc_tokyoso_wait2_override(1, ACT_GO), ACT_GO);
        // Set starts when the ready-all mask is NOT fully present.
        assert_eq!(pc_tokyoso_set_starts(0x000), 1);
        assert_eq!(pc_tokyoso_set_starts(0x380), 0);
        assert_eq!(pc_tokyoso_set_starts(0x060), 1); // stretch bits only, no ready bits
        // _01 set/clear asymmetry: set uses (2+idx)&3, clear uses idx&3.
        let npc = SP_NPC_EV_TOKYOSO_0 + 1; // idx 1
        assert_eq!(pc_tokyoso_01_set(0, npc), 0b1000); // (2+1)&3 = 3
        assert_eq!(pc_tokyoso_01_clear(0b1010, npc), 0b1000); // 1&3 = 1
        // Kansen transitions.
        assert_eq!(pc_tokyoso_kansen_bf_leave(1, 0), 1);
        assert_eq!(pc_tokyoso_kansen_bf_leave(0, 1), 1);
        assert_eq!(pc_tokyoso_kansen_bf_leave(0, 0), 0);
        assert_eq!(pc_tokyoso_kansen_leave(0, 2, 0), 1);
        assert_eq!(pc_tokyoso_kansen_leave(0, 0, 0), 0);
        // Dash/stumble rolls.
        assert_eq!(pc_tokyoso_dash_roll(0.05), 1);
        assert_eq!(pc_tokyoso_dash_roll(0.1), 0);
        assert_eq!(pc_tokyoso_stumble_roll(1, 0.05), 1);
        assert_eq!(pc_tokyoso_stumble_roll(0, 0.05), 0); // must be leading
        // Kokeru exit needs anim stop AND zero speed.
        assert_eq!(pc_tokyoso_kokeru_done(1, 0.0), 1);
        assert_eq!(pc_tokyoso_kokeru_done(1, 0.5), 0);
        assert_eq!(pc_tokyoso_kokeru_done(0, 0.0), 0);
        // Late-race predicate.
        assert_eq!(pc_tokyoso_late_race(8, 100), 1);
        assert_eq!(pc_tokyoso_late_race(8, -100), 0);
        assert_eq!(pc_tokyoso_late_race(7, 100), 0);
        // Goal think mapping.
        assert_eq!(pc_tokyoso_goal_think(1), 14);
        // NPC0 birth reset; NPC1 birth/self-delete phases.
        let mut r = 0;
        assert_eq!(pc_tokyoso_npc0_birth(3, &mut r), 0);
        assert_eq!(r, 1);
        assert_eq!(pc_tokyoso_npc0_birth(1, &mut r), 1);
        assert_eq!(r, 0);
        assert_eq!(pc_tokyoso_npc1_birth_ok(2), 1);
        assert_eq!(pc_tokyoso_npc1_birth_ok(1), 0);
        assert_eq!(pc_tokyoso_npc1_self_delete(3), 1);
        assert_eq!(pc_tokyoso_npc1_self_delete(2), 0);
        // FLAG_15 is reserved-but-unused.
        assert_eq!(FLAG_15, 0x8000);
    }
}
