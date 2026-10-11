//! The Tug of War actors (`ac_tunahiki_control.c`,
//! `ac_tunahiki_npc0.c`, `ac_tunahiki_npc1.c`).
//!
//! Verified against `src/actor/ac_tunahiki_control.c`,
//! `ac_tunahiki_npc0.c`, `ac_tunahiki_npc0_schedule.c_inc`,
//! `ac_tunahiki_npc0_talk.c_inc`, `ac_tunahiki_npc1.c`,
//! `ac_tunahiki_npc1_schedule.c_inc`,
//! `ac_tunahiki_npc1_talk.c_inc`,
//! `include/ac_tunahiki_control.h`,
//! `include/ac_tunahiki_npc0.h`, `include/ac_tunahiki_npc1.h`,
//! `src/game/m_event_map_npc_data.c_inc`,
//! `src/game/m_event_schedule.c_inc`, `src/actor/ac_event_manager.c`,
//! `include/m_name_table.h` (GAFE01_00 Rev 0 USA). All 8 brief
//! claims PROVEN.
//!
//! Scope split: the SPORTS_FAIR_TUG_OF_WAR schedule rows (Mar
//! 19–21 and Sep 21–23) are added to the sibling `event_day`
//! module. This module owns the shared-simulation pure kernels:
//! the controller's rope update, the NPC ID decomposition and
//! flag macros, the pull-speed selection, the footstep and
//! end-state decisions, the team direction/home-offset math, the
//! dialogue arithmetic, and the placement table. The event save
//! area, actor lifecycle, rope actor, NPC clip, animation
//! frame-control, RNG calls, effects, audio, and demo/talk stay
//! in C.
//!
//! The design: one controller owns the shared rope simulation
//! (`aEv_tunahiki_c`); NPC1 participants animate from it and
//! feed pull speeds back. NPC1's X follows `rope_base`, NOT
//! `rope` (which carries the visual ±0.3 shake).
//!
//! Nuances carried: the team macro returns the raw masked value
//! (0 or 2), not a normalized 0/1; the counter resets to 2 (not
//! 0) at 65; the rope clamp uses strict comparisons and zeroes
//! speed; the constructor never initializes speed/next_speed/
//! counter (their values come from the save-area allocator — do
//! not add zero-init); `aTNN1_aiko` calls `fqrand()`
//! unconditionally at entry even when the sample is unused, so
//! RNG stays in C; NPC1's destructor and several init paths
//! dereference the save area with NO null check (only NPC0's
//! destructor guards) — preserve the call-order assumptions;
//! NPC0's destructor clears the whole flag field, so delete bits
//! do not survive the STATE2 transition; the home-offset index
//! test uses `aTNC_NPCID2IDX` (relative to TUNAHIKI_0) while the
//! direction test uses `npc_id - SP_NPC_EV_TUNAHIKI_1`.

/// First tug NPC ID of the contiguous range
/// (`m_name_table.h`).
pub const SP_NPC_EV_TUNAHIKI_0: u16 = 0xD05F;
/// Base ID used by NPC1's direction test.
pub const SP_NPC_EV_TUNAHIKI_1: u16 = 0xD060;
/// Number of tug NPCs (0–4) plus Tortimer in the roster.
pub const TUNAHIKI_NPC_COUNT: usize = 5;

/// Shared flag bits (`ac_tunahiki_control.h`).
pub const FLAG_DELETE0: u16 = 1 << 0;
pub const FLAG_TALK0: u16 = 1 << 5;
/// OR of the five talk bits (5–9).
pub const FLAG_TALK_ALL: u16 = 0x03E0;
/// Rope-shake bit (15).
pub const FLAG_SHAKE: u16 = 1 << 15;
/// Rope pull increment per controller update: `speed * 0.025f`.
pub const ROPE_SPEED_STEP: f32 = 0.025;
/// Rope displacement clamp bounds (strict comparisons).
pub const ROPE_MIN: f32 = -10.0;
pub const ROPE_MAX: f32 = 10.0;
/// Visual shake amplitude, sign from `game_frame & 1`.
pub const SHAKE_AMPLITUDE: f32 = 0.3;
/// Counter reset value (resets to 2, not 0, at >= 65).
pub const COUNTER_RESET: u16 = 2;
pub const COUNTER_LIMIT: u16 = 65;
/// NPC1 animation frame tracks the shared counter:
/// `current_frame = counter * 0.5f`.
pub const COUNTER_TO_FRAME: f32 = 0.5;

/// NPC index: `aTNC_NPCID2IDX(id) = id - SP_NPC_EV_TUNAHIKI_0`.
#[no_mangle]
pub extern "C" fn pc_tunahiki_npc_index(npc_id: u16) -> i32 {
    npc_id.wrapping_sub(SP_NPC_EV_TUNAHIKI_0) as i32
}

/// Member bit: `aTNC_NPCID2MEMBER(id) = idx & 1`.
#[no_mangle]
pub extern "C" fn pc_tunahiki_member(npc_id: u16) -> i32 {
    pc_tunahiki_npc_index(npc_id) & 1
}

/// Team mask: `aTNC_NPCID2TEAM(id) = idx & 2` — the RAW masked
/// value (0 or 2), not a normalized 0/1.
#[no_mangle]
pub extern "C" fn pc_tunahiki_team_mask(npc_id: u16) -> i32 {
    pc_tunahiki_npc_index(npc_id) & 2
}

/// `aTNC_NPCIDX2DELETEFLG(n) = DELETE0 << idx`.
#[no_mangle]
pub extern "C" fn pc_tunahiki_delete_flag(npc_id: u16) -> u16 {
    FLAG_DELETE0 << ((pc_tunahiki_npc_index(npc_id) as u32) & 15)
}

/// `aTNC_NPCIDX2TALKFLG(n) = TALK0 << idx`.
#[no_mangle]
pub extern "C" fn pc_tunahiki_talk_flag(npc_id: u16) -> u16 {
    FLAG_TALK0 << ((pc_tunahiki_npc_index(npc_id) as u32) & 15)
}

/// NPC1 pull direction: +1 when `((npc_id - SP_NPC_EV_TUNAHIKI_1)
/// & 2) == 0`, else −1. The facing angles are
/// `DEG2SHORT_ANGLE2(±90.0f)` = ±16384.
#[no_mangle]
pub extern "C" fn pc_tunahiki_dir(npc_id: u16) -> i32 {
    if npc_id.wrapping_sub(SP_NPC_EV_TUNAHIKI_1) & 2 == 0 {
        1
    } else {
        -1
    }
}

/// Writes NPC1's home-position offsets to `out[0..2]` as (x, z):
/// x = dir * 95 for ID indices 2–3 (relative to TUNAHIKI_0),
/// else dir * 85; z = dir * 5. Returns 1 on success.
#[no_mangle]
pub extern "C" fn pc_tunahiki_home_offset(npc_id: u16, dir: i32, out: *mut f32) -> u8 {
    if out.is_null() {
        return 0;
    }
    let idx = pc_tunahiki_npc_index(npc_id);
    if idx < 0 || idx > 4 {
        return 0;
    }
    let d = dir as f32;
    let x = if idx == 2 || idx == 3 { d * 95.0 } else { d * 85.0 };
    let out = unsafe { std::slice::from_raw_parts_mut(out, 2) };
    out[0] = x;
    out[1] = d * 5.0;
    1
}

/// NPC1's per-frame X: `home.x + rope_base` — the shared base,
/// NOT the shake-carrying `rope` (`aTNN1_actor_move`).
#[no_mangle]
pub extern "C" fn pc_tunahiki_follow_x(home_x: f32, rope_base: f32) -> f32 {
    home_x + rope_base
}

/// Runs one `aTNC_wait` controller update purely:
/// - shake = ±0.3 from `game_frame & 1` when SHAKE is set, else 0
/// - while SHAKE is set and no TALK_ALL bit is set:
///   `rope_base += speed * 0.025`, clamped strictly to
///   [−10, 10] with speed zeroed on clamp
/// - `rope = rope_base + shake`
/// Writes (new_rope_base, new_speed, rope) to `out[0..3]`;
/// returns 1 on success.
#[no_mangle]
pub extern "C" fn pc_tunahiki_rope_step(
    rope_base: f32,
    speed: f32,
    flags: u16,
    game_frame: u32,
    out: *mut f32,
) -> u8 {
    if out.is_null() {
        return 0;
    }
    let mut base = rope_base;
    let mut spd = speed;
    let mut shake = 0.0f32;
    if flags & FLAG_SHAKE != 0 {
        shake = if game_frame & 1 != 0 {
            SHAKE_AMPLITUDE
        } else {
            -SHAKE_AMPLITUDE
        };
        if flags & FLAG_TALK_ALL == 0 {
            base += spd * ROPE_SPEED_STEP;
            if base < ROPE_MIN {
                base = ROPE_MIN;
                spd = 0.0;
            } else if base > ROPE_MAX {
                base = ROPE_MAX;
                spd = 0.0;
            }
        }
    }
    let out = unsafe { std::slice::from_raw_parts_mut(out, 3) };
    out[0] = base;
    out[1] = spd;
    out[2] = base + shake;
    1
}

/// Advances the shared counter: increments, resetting to 2 (not
/// 0) when it reaches 65 (`aTNC_wait` tail).
#[no_mangle]
pub extern "C" fn pc_tunahiki_counter_step(counter: u16) -> u16 {
    let c = counter.wrapping_add(1);
    if c >= COUNTER_LIMIT {
        COUNTER_RESET
    } else {
        c
    }
}

/// Selects the next pull speed from the values the C caller
/// sampled (`aTNN1_aiko`): `rng < 0.45` → `rand_mag + 3.0`;
/// `rng < 0.9` → `-(rand_mag + 3.0)`; otherwise 0. The `fqrand()`
/// sample and the conditional `RANDOM_F(20.0f)` call stay in C so
/// the global random sequence is unchanged.
#[no_mangle]
pub extern "C" fn pc_tunahiki_next_speed(rng: f32, rand_mag: f32) -> f32 {
    if rng < 0.45 {
        rand_mag + 3.0
    } else if rng < 0.9 {
        -(rand_mag + 3.0)
    } else {
        0.0
    }
}

/// Footstep-effect condition: `speed * dir > 8.0f`, tested when
/// the animation crosses frames 12/18 (`aTNN1_aiko`).
#[no_mangle]
pub extern "C" fn pc_tunahiki_footstep(speed: f32, dir: i32) -> bool {
    speed * dir as f32 > 8.0
}

/// End-of-animation think selection (`aTNN1_aiko`):
/// `-(rope_base * dir) > 5` → 2; `< -5` with
/// `next_speed * dir >= 0` → 3; otherwise 1. The `>=` keeps the
/// zero-speed equality case on the state-3 branch.
#[no_mangle]
pub extern "C" fn pc_tunahiki_next_think(dir: i32, rope_base: f32, next_speed: f32) -> i32 {
    let d = dir as f32;
    let pull = -(rope_base * d);
    if pull > 5.0 {
        2
    } else if pull < -5.0 && next_speed * d >= 0.0 {
        3
    } else {
        1
    }
}

/// Tug NPC dialogue bases per looks index
/// (`ac_tunahiki_npc0.c`, shared with NPC1).
pub const NPC_MSG_BASE: [i32; 6] = [6532, 6544, 6520, 6556, 6568, 6580];
/// NPC0's talk-table `msg_no` (0); NPC1 uses 3/6/9 per talk row.
pub const NPC0_MSG_NO: i32 = 0;
pub const NPC1_MSG_NOS: [i32; 3] = [3, 6, 9];

/// Computes the tug dialogue message:
/// `base_msg + msg_no + rand3`. The `rand3` value is supplied by
/// the C caller so RNG order stays in C.
#[no_mangle]
pub extern "C" fn pc_tunahiki_msg(base: i32, msg_no: i32, rand3: i32) -> i32 {
    base + msg_no + rand3
}

/// Rope-creation offset from NPC0 (`aTNN0_birth`): the rope actor
/// spawns at (x + 20, y, z + 45) via `mAc_PROFILE_ROPE`.
pub const ROPE_SPAWN_DX: f32 = 20.0;
pub const ROPE_SPAWN_DZ: f32 = 45.0;
/// NPC0's post-birth shift: +20 X, −30 Z; pipe radius 60.
pub const NPC0_SHIFT_DX: f32 = 20.0;
pub const NPC0_SHIFT_DZ: f32 = -30.0;

/// Tug-of-war shrine placement: six slots of (field-unit X, Z)
/// (`m_event_map_npc_data.c_inc`): TUNAHIKI_0..4 then Tortimer.
pub const SPAWN_LAYOUT: [(i32, i32); 6] = [
    (7, 10),
    (3, 11),
    (4, 11),
    (11, 11),
    (12, 11),
    (10, 8),
];
/// NPC slot mask `0b0011111` in the data record (5 NPC bits).
pub const SPAWN_NPC_MASK: u32 = 0b0011111;
pub const SPAWN_SLOT_COUNT: usize = 6;

/// Writes the (x, z) field-unit position of a spawn slot to
/// `out[0..2]`; returns 1 on success. Slots are 0-based.
#[no_mangle]
pub extern "C" fn pc_tunahiki_spawn_pos(slot: u8, out: *mut i32) -> u8 {
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


/// Event-manager identity of the tug-of-war event
/// (`mEv_EVENT_SPORTS_FAIR_TUG_OF_WAR`; the rope's draw path uses the
/// literal 0xE).
pub const TUG_EVENT_TYPE: i32 = 0xE;
/// Shared event save-area ID used by the controller, NPCs, and rope.
pub const TUG_SAVE_AREA_ID: i32 = 9;
/// Common placement ID used by `field_day_ct` / `field_day_delete`.
pub const TUG_COMMON_PLACE_ID: i32 = 5;
/// Controller/NPC lifecycle states (`ac_tunahiki_control.h`).
pub const TUG_NPC_STATE0: i32 = 0;
pub const TUG_NPC_STATE1: i32 = 1;
pub const TUG_NPC_STATE2: i32 = 2;
/// Controller constructor explicitly initializes rope, rope_base, flag,
/// npc_state, and _14 to these; speed, next_speed, and counter are NOT
/// explicitly initialized (they start at zero only via the generic
/// reserve zeroing).
pub const TUG_CTOR_ROPE: f32 = 0.0;
pub const TUG_CTOR_ROPE_BASE: f32 = 0.0;
pub const TUG_CTOR_FLAG: u16 = 0;

/// Rope-actor collision lifecycle (`aRP_actor_ct` / `aRP_actor_dt`,
/// `ac_rope.c`): three `mCoBG_SetPlussOffset` calls at unit-center
/// X offsets (i-1)*40 with the constructor's mode and the destructor's.
pub const ROPE_OFFSET_STEP: f32 = 40.0;
pub const ROPE_OFFSET_COUNT: i32 = 3;
pub const ROPE_OFFSET_MODE_CT: i32 = 2;
pub const ROPE_OFFSET_MODE_DT: i32 = 0;
/// Attribute argument passed to the helper: this equals
/// `mCoBG_ATTRIBUTE_NONE`, so the rope never writes the unit attribute.
pub const ROPE_OFFSET_ATTR: i32 = 100;

/// World X of the i-th (0-based) collision-offset call, given the
/// field-unit-center X: x0-40, x0, x0+40 with fixed Z.
#[no_mangle]
pub extern "C" fn pc_rope_offset_x(unit_center_x: f32, i: i32) -> f32 {
    unit_center_x + (i - 1) as f32 * ROPE_OFFSET_STEP
}

/// Vertex-deformation fact (`aRP_make_vtx`): the event save area's
/// `rope` float times 100, converted to int (C truncation toward zero).
#[no_mangle]
pub extern "C" fn pc_rope_fact(rope: f32) -> i32 {
    (rope * 100.0f32) as i32
}

/// The 60-entry (0x3C) vertex flag table (`aRP_make_vtx`): vertices
/// whose flag is 1 receive the displacement. Counted: 22 set.
pub const ROPE_VTX_FLAGS: [u8; 60] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 1,
    1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 1, 1, 0, 1, 1, 1, 1, 1, 1,
];

/// Deformed vertex X: `vtx->v.ob[0] = make->v.ob[0] + fact` when the
/// flag entry is 1 (`flag[0] == 1` in C), else the original X.
#[no_mangle]
pub extern "C" fn pc_rope_vtx_x(orig_x: i16, flag: u8, fact: i32) -> i16 {
    if flag == 1 {
        (orig_x as i32 + fact) as i16
    } else {
        orig_x
    }
}

/// `fdtug_start` return tree (`ac_event_manager.c`): 0 when the shrine
/// block is missing (event gets STATUS_ERROR); 2 when already kept;
/// 1 when the title fade succeeds (field_day_delete runs before the
/// new placement); 0 when the fade is still pending.
#[no_mangle]
pub extern "C" fn pc_fdtug_start(shrine_exists: i32, kept: i32, fade_ok: i32) -> i32 {
    if shrine_exists == 0 {
        0
    } else if kept != 0 {
        2
    } else if fade_ok != 0 {
        1
    } else {
        0
    }
}

/// `fdtug_stop` return tree: 2 when not kept; 1 when kept and the
/// field-day is already over (clears keep) or the closing title fade
/// completes (field_day_delete runs); 0 when the fade is pending.
#[no_mangle]
pub extern "C" fn pc_fdtug_stop(kept: i32, over_status: i32, fade_done: i32) -> i32 {
    if kept == 0 {
        2
    } else if over_status != -1 {
        1
    } else if fade_done != 0 {
        1
    } else {
        0
    }
}

/// `fdtug_in` return mapping: `shown` is -1 for
/// `aEvMgr_SHOW_ACTOR_RESULT_NOT_SHOWN` (a real pointer value, NOT
/// NULL), 0 for NULL, nonzero for a valid placement. Only 1 marks
/// the event STATUS_SHOW; 2 is not interchangeable with success.
#[no_mangle]
pub extern "C" fn pc_fdtug_in(shown: i32) -> i32 {
    if shown == -1 {
        2
    } else if shown != 0 {
        1
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_decomposition() {
        assert_eq!(pc_tunahiki_npc_index(0xD05F), 0);
        assert_eq!(pc_tunahiki_npc_index(0xD063), 4);
        assert_eq!(pc_tunahiki_member(0xD060), 1);
        assert_eq!(pc_tunahiki_member(0xD061), 0);
        // Team mask is raw: 0 or 2, not normalized.
        assert_eq!(pc_tunahiki_team_mask(0xD05F), 0);
        assert_eq!(pc_tunahiki_team_mask(0xD061), 2);
        assert_eq!(pc_tunahiki_delete_flag(0xD05F), 0x0001);
        assert_eq!(pc_tunahiki_delete_flag(0xD063), 0x0010);
        assert_eq!(pc_tunahiki_talk_flag(0xD05F), 0x0020);
        assert_eq!(pc_tunahiki_talk_flag(0xD063), 0x0200);
        // Direction: (id - TUNAHIKI_1) & 2 == 0 -> +1.
        assert_eq!(pc_tunahiki_dir(0xD060), 1);
        assert_eq!(pc_tunahiki_dir(0xD061), 1);
        assert_eq!(pc_tunahiki_dir(0xD062), -1);
        assert_eq!(pc_tunahiki_dir(0xD063), -1);
    }

    #[test]
    fn home_offsets() {
        let mut out = [0.0f32; 2];
        // Index 1 (TUNAHIKI_1): 85, not 95.
        assert_eq!(pc_tunahiki_home_offset(0xD060, 1, out.as_mut_ptr()), 1);
        assert_eq!(out, [85.0, 5.0]);
        // Indices 2,3: 95.
        assert_eq!(pc_tunahiki_home_offset(0xD061, -1, out.as_mut_ptr()), 1);
        assert_eq!(out, [-95.0, -5.0]);
        assert_eq!(pc_tunahiki_home_offset(0xD05F, 1, out.as_mut_ptr()), 1);
        assert_eq!(out, [85.0, 5.0]);
        assert_eq!(pc_tunahiki_home_offset(0xD064, 1, out.as_mut_ptr()), 0);
        assert_eq!(pc_tunahiki_follow_x(100.0, 3.5), 103.5);
    }

    fn rope_step(base: f32, speed: f32, flags: u16, frame: u32) -> (f32, f32, f32) {
        let mut out = [0.0f32; 3];
        assert_eq!(pc_tunahiki_rope_step(base, speed, flags, frame, out.as_mut_ptr()), 1);
        (out[0], out[1], out[2])
    }

    #[test]
    fn controller_rope_update() {
        // No shake: rope == base, nothing accumulates.
        assert_eq!(rope_step(3.0, 5.0, 0, 0), (3.0, 5.0, 3.0));
        // Shake on, even frame: -0.3; base advances by speed*0.025.
        assert_eq!(rope_step(0.0, 40.0, FLAG_SHAKE, 0), (1.0, 40.0, 0.7));
        // Shake on, odd frame: +0.3.
        assert_eq!(rope_step(0.0, 40.0, FLAG_SHAKE, 1), (1.0, 40.0, 1.3));
        // Talk freeze: shake still applies, base does not advance.
        let (b, s, r) = rope_step(0.0, 40.0, FLAG_SHAKE | 0x0020, 1);
        assert_eq!((b, s, r), (0.0, 40.0, 0.3));
        // Strict clamp: crossing -10 zeroes speed.
        assert_eq!(rope_step(-9.9, -40.0, FLAG_SHAKE, 0), (-10.0, 0.0, -10.3));
        // Exactly at the bound: no clamp (strict <).
        assert_eq!(rope_step(-10.0, 0.0, FLAG_SHAKE, 0), (-10.0, 0.0, -10.3));
        // Counter: 64 -> 2, 63 -> 64.
        assert_eq!(pc_tunahiki_counter_step(64), 2);
        assert_eq!(pc_tunahiki_counter_step(63), 64);
        assert_eq!(pc_tunahiki_counter_step(0), 1);
    }

    #[test]
    fn speed_selection() {
        assert_eq!(pc_tunahiki_next_speed(0.1, 10.0), 13.0);
        assert_eq!(pc_tunahiki_next_speed(0.449, 10.0), 13.0);
        assert_eq!(pc_tunahiki_next_speed(0.45, 10.0), -13.0);
        assert_eq!(pc_tunahiki_next_speed(0.899, 10.0), -13.0);
        assert_eq!(pc_tunahiki_next_speed(0.9, 10.0), 0.0);
        assert!(pc_tunahiki_footstep(9.0, 1));
        assert!(!pc_tunahiki_footstep(8.0, 1));
        assert!(pc_tunahiki_footstep(-9.0, -1));
    }

    #[test]
    fn end_state_decision() {
        // -(base*dir) > 5 -> 2.
        assert_eq!(pc_tunahiki_next_think(1, -6.0, 0.0), 2);
        // < -5 with next_speed*dir >= 0 -> 3 (zero speed keeps >=).
        assert_eq!(pc_tunahiki_next_think(1, 6.0, 0.0), 3);
        assert_eq!(pc_tunahiki_next_think(1, 6.0, 5.0), 3);
        // < -5 but next_speed*dir < 0 -> 1.
        assert_eq!(pc_tunahiki_next_think(1, 6.0, -5.0), 1);
        // Within bounds -> 1.
        assert_eq!(pc_tunahiki_next_think(1, 0.0, 0.0), 1);
        // Exactly 5 is not > 5.
        assert_eq!(pc_tunahiki_next_think(1, -5.0, 0.0), 1);
    }

    #[test]
    fn dialogue_and_spawn() {
        assert_eq!(pc_tunahiki_msg(6532, 0, 2), 6534);
        assert_eq!(pc_tunahiki_msg(6532, 6, 2), 6540);
        let mut out = [0i32; 2];
        assert_eq!(pc_tunahiki_spawn_pos(0, out.as_mut_ptr()), 1);
        assert_eq!(out, [7, 10]);
        assert_eq!(pc_tunahiki_spawn_pos(5, out.as_mut_ptr()), 1);
        assert_eq!(out, [10, 8]);
        assert_eq!(pc_tunahiki_spawn_pos(6, out.as_mut_ptr()), 0);
    }

    #[test]
    fn rope_actor_kernels() {
        // Collision offsets: x0-40, x0, x0+40.
        assert_eq!(pc_rope_offset_x(100.0, 0), 60.0);
        assert_eq!(pc_rope_offset_x(100.0, 1), 100.0);
        assert_eq!(pc_rope_offset_x(100.0, 2), 140.0);
        // Attribute 100 is ATTRIBUTE_NONE — the helper never writes it.
        assert_eq!(ROPE_OFFSET_ATTR, 100);
        // Vertex fact: f32*100 truncated toward zero.
        assert_eq!(pc_rope_fact(2.5), 250);
        assert_eq!(pc_rope_fact(-1.234), -123);
        // Flag table: 60 entries, 22 set, at the decomp positions.
        assert_eq!(ROPE_VTX_FLAGS.len(), 60);
        assert_eq!(ROPE_VTX_FLAGS.iter().filter(|&&f| f == 1).count(), 22);
        assert_eq!(ROPE_VTX_FLAGS[15], 0);
        assert_eq!(ROPE_VTX_FLAGS[16], 1);
        assert_eq!(ROPE_VTX_FLAGS[28], 0);
        assert_eq!(ROPE_VTX_FLAGS[29], 1);
        assert_eq!(ROPE_VTX_FLAGS[30], 1);
        assert_eq!(ROPE_VTX_FLAGS[31], 0);
        assert_eq!(ROPE_VTX_FLAGS[51], 1);
        assert_eq!(ROPE_VTX_FLAGS[53], 0);
        assert_eq!(ROPE_VTX_FLAGS[54], 1);
        // Vertex X: flag==1 adds fact, else original.
        assert_eq!(pc_rope_vtx_x(1000, 1, 250), 1250);
        assert_eq!(pc_rope_vtx_x(1000, 0, 250), 1000);
        // fdtug_start: shrine missing -> 0; kept -> 2; fade ok -> 1; pending -> 0.
        assert_eq!(pc_fdtug_start(0, 0, 1), 0);
        assert_eq!(pc_fdtug_start(1, 1, 1), 2);
        assert_eq!(pc_fdtug_start(1, 0, 1), 1);
        assert_eq!(pc_fdtug_start(1, 0, 0), 0);
        // fdtug_stop: not kept -> 2; over -> 1; fade done -> 1; pending -> 0.
        assert_eq!(pc_fdtug_stop(0, -1, 0), 2);
        assert_eq!(pc_fdtug_stop(1, 0, 0), 1);
        assert_eq!(pc_fdtug_stop(1, -1, 1), 1);
        assert_eq!(pc_fdtug_stop(1, -1, 0), 0);
        // fdtug_in: -1 sentinel -> 2; valid -> 1; NULL -> 0.
        assert_eq!(pc_fdtug_in(-1), 2);
        assert_eq!(pc_fdtug_in(1), 1);
        assert_eq!(pc_fdtug_in(0), 0);
    }
}
