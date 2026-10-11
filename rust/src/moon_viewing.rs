//! The Moon Viewing actors (`ac_tukimi.c`, `ac_tukimi_npc0.c`, `ac_tukimi_npc1.c`).
//!
//! Verified against `src/actor/ac_tukimi.c`,
//! `src/actor/ac_tukimi_move.c_inc`, `src/actor/ac_tukimi_draw.c_inc`,
//! `src/actor/npc/ac_tukimi_npc0_talk.c_inc`,
//! `src/actor/npc/ac_tukimi_npc1_talk.c_inc`,
//! `src/game/m_event_map_npc_data.c_inc`,
//! `src/game/m_event_schedule.c_inc`, `src/actor/ac_event_manager.c`,
//! `src/game/m_bgm.c`, `include/m_name_table.h`, `include/ac_tukimi.h`,
//! `include/ac_tukimi_npc0.h`, `include/ac_tukimi_npc1.h`
//! (GAFE01_00 Rev 0 USA). 11 of 12 brief claims PROVEN; the 12th
//! (the structure-actor deletion test) DIFFERS in the brief's favor
//! and is documented below.
//!
//! Scope split: the METEOR_SHOWER Aug 12 18:00–20:00 schedule row
//! and the harvest-moon lunar-date row live in the sibling
//! `event_day` module. This module owns the Tukimi structure actor's
//! pure kernels (variant index, the ABS-diff > 1 deletion rule), the
//! two NPCs' angle/offset tables, their head-target offsets, their
//! dialogue formulas, and the seven Meteor Shower pool layouts.
//! The Harvest Moon Festival event manager (which never spawns
//! these actors) and the Meteor Shower manager handlers, the
//! skeleton animation, NPC lifecycle, RNG calls, and message
//! resources stay in C.
//!
//! The key retail fact: the `ac_tukimi_npc0/npc1` actors are spawned
//! by the METEOR SHOWER event manager, not the Harvest Moon
//! Festival manager. The two events share the pool block but have
//! distinct event IDs, effects (eEC_EFFECT_NIGHT13_MOON vs.
//! eEC_EFFECT_SHOOTING_SET), and music tracks (30 vs. 250), so they
//! must not be merged.
//!
//! Nuances carried: the structure actor deletes itself only when
//! ABS(block_diff) > 1 on either axis (an adjacent block does NOT
//! delete it) — the brief said a plain block mismatch, which would
//! delete too aggressively; `mFI_GetPuleIdx()` returns `type - 69`
//! for the seven pool layouts (0–6) and -1 otherwise, so NPC0's
//! 8th angle entry (index 7 = 0) is unreachable through the pool
//! path and the 8-entry/7-entry table-length mismatch is safe in
//! practice; NPC1's fifth-NPC dialogue dispatch tests
//! `npc_id == SP_NPC_EV_TUKIMI_4`, not `event_idx == 4`; NPC1's
//! angle-table values are raw s16 short-angle units (stored as hex
//! in the source), not degrees; NPC0's head target sits 30 units
//! BELOW the eye while NPC1's sits 30 units ABOVE (and is skipped
//! for index 4); Meteor Shower layout 2 has slots 3 and 4 both at
//! (13, 2) — preserved verbatim, not "fixed"; layouts 2/4/7 pair
//! with the first roster and 1/3/5/6 with the second (identical IDs,
//! table-selection difference), the same pairing as fireworks; the
//! meteor-shower stop handler deletes eEC_EFFECT_SHOOTING, not
//! _SHOOTING_SET.

/// The structure actor's variant index from its actor ID:
/// `tukimi->arg0 = actor->npc_id - NEWYEAR_SHRINE0` (`ac_tukimi.c`).
/// The valid IDs are NEWYEAR_SHRINE0 and NEWYEAR_SHRINE0 + 1
/// (left and right variants); the source indexes two-entry arrays
/// directly, so retail has no clamp.
pub const NEWYEAR_SHRINE0: u16 = 0x582F; // (STRUCTURE_START + 47) = 22575
/// Field-ground marker written at init: DUMMY_TUKIMI = 0xF10A.
pub const DUMMY_TUKIMI: u16 = 0xF10A;
/// The background-offset helper always passes offset ID 6:
/// `id = id == 0 ? 6 : 6` (`ac_tukimi_move.c_inc`).
pub const TUKIMI_BG_OFFSET_ID: i32 = 6;
/// The value passed to `mCoBG_SetPlussOffset` alongside the ID.
pub const TUKIMI_BG_OFFSET_VALUE: i32 = 100;
/// Animation parameters passed to `cKF_SkeletonInfo_R_init`
/// (`ac_tukimi_move.c_inc`): speed 1.0, frame 271.0, 1.0, 0.5, 0.0, 1.
pub const TUKIMI_ANIM_FRAME: f32 = 271.0;
/// Number of left/right structure variants.
pub const TUKIMI_VARIANT_COUNT: usize = 2;

/// Returns true when the Tukimi structure actor deletes itself:
/// every scroll-demo check is false AND `ABS(tukimi_b - player_b) > 1`
/// on either block axis (`aTUK_actor_move`). Note the strict
/// inequality — an actor in an adjacent block (diff == 1) is kept.
#[no_mangle]
pub extern "C" fn pc_tukimi_should_delete(
    scroll0: bool,
    scroll1: bool,
    scroll2: bool,
    tukimi_bx: i32,
    tukimi_bz: i32,
    player_bx: i32,
    player_bz: i32,
) -> bool {
    !scroll0
        && !scroll1
        && !scroll2
        && ((tukimi_bx - player_bx).abs() > 1 || (tukimi_bz - player_bz).abs() > 1)
}

/// NPC0 facing angles per pool layout (puleIdx 0–6), s16
/// short-angle units (`ac_tukimi_npc0_talk.c_inc`). An eighth entry
/// (index 7 = 0) exists in the source but is unreachable through
/// `mFI_GetPuleIdx()`, which returns 0–6 for pool blocks.
pub const NPC0_DEF_ANGLE: [i16; 8] = [
    0xA000u16 as i16,
    0x2000,
    0x6000,
    0xE000u16 as i16,
    0xE000u16 as i16,
    0x2000,
    0xA000u16 as i16,
    0,
];
/// NPC0 X position offsets added to the actor's world X, per
/// puleIdx (`ac_tukimi_npc0_talk.c_inc`).
pub const NPC0_DEF_POS_X: [f32; 7] = [1.0, 39.0, 39.0, 1.0, 1.0, 39.0, 1.0];
/// NPC0 Z position offsets added to the actor's world Z, per
/// puleIdx (`ac_tukimi_npc0_talk.c_inc`).
pub const NPC0_DEF_POS_Z: [f32; 7] = [1.0, 39.0, 1.0, 39.0, 39.0, 39.0, 1.0];
/// NPC0 dialogue bases per looks index (0x1EB0..), plain event.
/// (`aTM0_set_talk_info`).
pub const NPC0_MSG_BASE: [i32; 6] = [7856, 7871, 7841, 7886, 7901, 7916];
/// NPC0 dialogue bases per looks index (0x3F46..), used while the
/// Meteor Shower event is ACTIVE.
pub const NPC0_MSG_BASE_METEOR: [i32; 6] = [16198, 16213, 16183, 16228, 16243, 16258];
/// NPC0 head-target offsets (`aTM0_schedule_main_proc`): 40 units
/// forward, 30 units BELOW the eye, priority 3. Set only while the
/// action is aNPC_ACT_SPECIAL.
pub const NPC0_HEAD_FORWARD: f32 = 40.0;
pub const NPC0_HEAD_Y_OFFSET: f32 = -30.0;

/// Returns NPC0's facing angle (s16 short-angle units) for a pool
/// layout index, or -1 when `pule_idx` is outside 0..7. The same
/// angle is assigned to shape rotation, world angle, and movement
/// angle (`aTM0_schedule_init_proc`).
#[no_mangle]
pub extern "C" fn pc_tukimi_npc0_facing(pule_idx: i32) -> i32 {
    if pule_idx < 0 || pule_idx >= NPC0_DEF_ANGLE.len() as i32 {
        return -1;
    }
    NPC0_DEF_ANGLE[pule_idx as usize] as i32
}

/// Writes NPC0's position offsets for a pool layout index to
/// `out[0..2]` (x, z); returns 1 on success. The offsets are
/// ADDED to the actor's existing world X/Z. Valid range 0..6;
/// `mFI_GetPuleIdx()` guarantees this for pool blocks.
#[no_mangle]
pub extern "C" fn pc_tukimi_npc0_pos_offset(pule_idx: u8, out: *mut f32) -> u8 {
    if out.is_null() {
        return 0;
    }
    let idx = pule_idx as usize;
    if idx >= NPC0_DEF_POS_X.len() {
        return 0;
    }
    let out = unsafe { std::slice::from_raw_parts_mut(out, 2) };
    out[0] = NPC0_DEF_POS_X[idx];
    out[1] = NPC0_DEF_POS_Z[idx];
    1
}

/// Computes the final NPC0 dialogue message from an explicit looks
/// index, an explicit `random(3)` result supplied by the C caller
/// (so RNG call order stays in C), and whether the Meteor Shower
/// event is active. (`aTM0_set_talk_info`: `msg[looks] + RANDOM(3)`.

#[no_mangle]
pub extern "C" fn pc_tukimi_npc0_message(looks: i32, rand3: i32, meteor_active: bool) -> i32 {
    let base = if meteor_active {
        &NPC0_MSG_BASE_METEOR
    } else {
        &NPC0_MSG_BASE
    };
    let l = looks.rem_euclid(base.len() as i32) as usize;
    base[l] + rand3
}

/// NPC1 facing angles: rows are event_idx (0–4), columns are
/// puleIdx (0–6), s16 short-angle units
/// (`ac_tukimi_npc1_talk.c_inc`).
pub const NPC1_DEF_ANGLE: [[i16; 7]; 5] = [
    [
        0xC000u16 as i16,
        0x4000,
        0xA000u16 as i16,
        0x8000u16 as i16,
        0xE000u16 as i16,
        0xC000u16 as i16,
        0x2000,
    ],
    [
        0x2000,
        0xA000u16 as i16,
        0xE000u16 as i16,
        0x2000,
        0xE000u16 as i16,
        0xE000u16 as i16,
        0xC000u16 as i16,
    ],
    [
        0x2000,
        0xA000u16 as i16,
        0xE000u16 as i16,
        0x2000,
        0xE000u16 as i16,
        0xE000u16 as i16,
        0xC000u16 as i16,
    ],
    [
        0xE000u16 as i16,
        0xE000u16 as i16,
        0x0000,
        0x0000,
        0x2000,
        0x6000,
        0xE000u16 as i16,
    ],
    [0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000],
];
/// First special-NPC ID of the contiguous range; event_idx is
/// `npc_id - SP_NPC_EV_TUKIMI_0` (`m_name_table.h`).
pub const SP_NPC_EV_TUKIMI_0: u16 = 0xD03F;
pub const SP_NPC_EV_TUKIMI_4: u16 = 0xD043;
/// Number of Tukimi NPC1 event indices (0–4); the fifth NPC is the
/// special "eater" variant.
pub const NPC1_EVENT_COUNT: usize = 5;
/// NPC1 dialogue bases per looks index, ordinary indices:
/// 0x1EB0 family (plain) and 0x3F46 family (Meteor Shower active).
pub const NPC1_MSG_BASE: [i32; 6] = [7856, 7871, 7841, 7886, 7901, 7916];
pub const NPC1_MSG_BASE_METEOR: [i32; 6] = [16198, 16213, 16183, 16228, 16243, 16258];
/// NPC1 dialogue bases per looks index, fifth NPC (`aTM1_set_talk_info2`):
/// 0x1EBC family (plain) and 0x3F52 family (Meteor Shower active).
pub const NPC1_MSG_BASE_IDX4: [i32; 6] = [7868, 7883, 7853, 7898, 7913, 7928];
pub const NPC1_MSG_BASE_METEOR_IDX4: [i32; 6] = [16210, 16225, 16195, 16240, 16255, 16270];
/// NPC1 head-target offsets: 40 units forward, 30 units ABOVE the
/// eye (opposite sign from NPC0), priority 3; the update is
/// skipped entirely for event_idx == 4.
pub const NPC1_HEAD_FORWARD: f32 = 40.0;
pub const NPC1_HEAD_Y_OFFSET: f32 = 30.0;

/// Returns NPC1's facing angle (s16 short-angle units) for an
/// event index and pool layout index, or -1 outside the valid
/// 5x7 range.
#[no_mangle]
pub extern "C" fn pc_tukimi_npc1_facing(event_idx: i32, pule_idx: i32) -> i32 {
    if event_idx < 0
        || event_idx >= NPC1_DEF_ANGLE.len() as i32
        || pule_idx < 0
        || pule_idx >= NPC1_DEF_ANGLE[0].len() as i32
    {
        return -1;
    }
    NPC1_DEF_ANGLE[event_idx as usize][pule_idx as usize] as i32
}

/// Returns NPC1's animation slot: 1 (KUISINBO1) only for
/// event_idx == 4, else 0 (WAIT1). The same selection runs in
/// both the action init and the action-end step.
#[no_mangle]
pub extern "C" fn pc_tukimi_npc1_animation_index(event_idx: i32) -> i32 {
    if event_idx == 4 { 1 } else { 0 }
}

/// Returns true for the fifth event NPC (the "eater"): no head
/// turning and KUISINBO1 as the default animation. The source
/// tests `npc_id == SP_NPC_EV_TUKIMI_4` for the talk config and
/// `event_idx == 4` for the head-target skip; both are equivalent
/// over the contiguous ID range.
#[no_mangle]
pub extern "C" fn pc_tukimi_npc1_is_fifth(npc_id: u16) -> bool {
    npc_id == SP_NPC_EV_TUKIMI_4
}

/// Computes the final NPC1 dialogue message. Ordinary indices use
/// `msg[looks] + RANDOM(3) + event_idx * 3`; the fifth NPC uses
/// its own base family without the event_idx term. The meteor
/// flag selects the active-shower family; `rand3` is supplied by
/// the C caller so RNG order stays in C.
#[no_mangle]
pub extern "C" fn pc_tukimi_npc1_message(
    looks: i32,
    rand3: i32,
    event_idx: i32,
    meteor_active: bool,
) -> i32 {
    let (base, indexed) = if event_idx == 4 {
        (
            if meteor_active {
                &NPC1_MSG_BASE_METEOR_IDX4
            } else {
                &NPC1_MSG_BASE_IDX4
            },
            0,
        )
    } else {
        (
            if meteor_active {
                &NPC1_MSG_BASE_METEOR
            } else {
                &NPC1_MSG_BASE
            },
            event_idx * 3,
        )
    };
    let l = looks.rem_euclid(base.len() as i32) as usize;
    base[l] + rand3 + indexed
}

/// Returns true when the NPC1 head-target request should be made:
/// event_idx != 4 AND the action is aNPC_ACT_SPECIAL. The C side
/// performs the actual eye-relative math.
#[no_mangle]
pub extern "C" fn pc_tukimi_npc1_wants_head_target(event_idx: i32, action_is_special: bool) -> bool {
    event_idx != 4 && action_is_special
}

/// Meteor Shower pool layouts: seven layouts x five slots of
/// (field-unit X, Z) (`m_event_map_npc_data.c_inc`).
pub const METEOR_SHOWER_LAYOUTS: [[(i32, i32); 5]; 7] = [
    [(13, 7), (3, 3), (2, 4), (12, 2), (7, 12)],
    [(3, 8), (12, 13), (13, 12), (13, 2), (13, 2)],
    [(9, 13), (12, 3), (13, 3), (4, 4), (11, 8)],
    [(9, 12), (3, 2), (2, 3), (8, 2), (11, 11)],
    [(9, 2), (12, 6), (13, 7), (2, 2), (2, 13)],
    [(12, 8), (12, 2), (13, 3), (2, 13), (11, 9)],
    [(3, 7), (13, 8), (13, 9), (12, 2), (2, 8)],
];
/// The five actor slots per layout: SP_NPC_EV_TUKIMI_0..3, then
/// Tortimer. Two roster arrays exist in the source with identical
/// IDs; layouts 2/4/7 pair with roster 0, layouts 1/3/5/6 with
/// roster 1 (same pairing as the fireworks layouts).
pub const METEOR_SHOWER_ROSTER0_USES_FIRST: [bool; 7] = [false, true, false, true, false, false, true];
/// NPC slot mask `0b0011111` in the data record; five slots in the
/// pool block (`l_ms_set_data = { 0, 4, 7, 5, ..., mRF_BLOCKKIND_POOL }`).
pub const METEOR_SHOWER_SLOT_MASK: u32 = 0b0011111;
pub const METEOR_SHOWER_SLOT_COUNT: usize = 5;

/// Writes the (x, z) field-unit position of a Meteor Shower actor
/// slot to `out[0..2]`; returns 1 on success. Layouts and slots
/// are 0-based in Rust (1-based in the source).
#[no_mangle]
pub extern "C" fn pc_tukimi_ms_layout(layout: u8, slot: u8, out: *mut i32) -> u8 {
    if out.is_null() {
        return 0;
    }
    let (l, s) = (layout as usize, slot as usize);
    if l >= METEOR_SHOWER_LAYOUTS.len() || s >= METEOR_SHOWER_SLOT_COUNT {
        return 0;
    }
    let (px, pz) = METEOR_SHOWER_LAYOUTS[l][s];
    let out = unsafe { std::slice::from_raw_parts_mut(out, 2) };
    out[0] = px;
    out[1] = pz;
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delete_rule_needs_abs_diff_gt_1() {
        // Same block: kept.
        assert!(!pc_tukimi_should_delete(false, false, false, 5, 5, 5, 5));
        // Adjacent block: KEPT (this is the brief correction).
        assert!(!pc_tukimi_should_delete(false, false, false, 6, 5, 5, 5));
        // Two blocks away: deleted.
        assert!(pc_tukimi_should_delete(false, false, false, 7, 5, 5, 5));
        // Any scroll demo active: kept even far away.
        assert!(!pc_tukimi_should_delete(true, false, false, 7, 5, 5, 5));
    }

    #[test]
    fn npc0_tables() {
        assert_eq!(pc_tukimi_npc0_facing(0), 0xA000u16 as i16 as i32);
        assert_eq!(pc_tukimi_npc0_facing(6), 0xA000u16 as i16 as i32);
        assert_eq!(pc_tukimi_npc0_facing(7), 0); // unreachable-but-present entry
        assert_eq!(pc_tukimi_npc0_facing(8), -1);
        let mut out = [0.0f32; 2];
        assert_eq!(pc_tukimi_npc0_pos_offset(0, out.as_mut_ptr()), 1);
        assert_eq!(out, [1.0, 1.0]);
        assert_eq!(pc_tukimi_npc0_pos_offset(7, out.as_mut_ptr()), 0);
        assert_eq!(pc_tukimi_npc0_pos_offset(7, std::ptr::null_mut()), 0);
        assert_eq!(pc_tukimi_npc0_message(0, 2, false), 7856 + 2);
        assert_eq!(pc_tukimi_npc0_message(0, 2, true), 16198 + 2);
    }

    #[test]
    fn npc1_facing_and_animation() {
        assert_eq!(pc_tukimi_npc1_facing(0, 0), 0xC000u16 as i16 as i32);
        assert_eq!(pc_tukimi_npc1_facing(4, 6), 0);
        assert_eq!(pc_tukimi_npc1_facing(5, 0), -1);
        assert_eq!(pc_tukimi_npc1_animation_index(0), 0);
        assert_eq!(pc_tukimi_npc1_animation_index(3), 0);
        assert_eq!(pc_tukimi_npc1_animation_index(4), 1);
        assert!(pc_tukimi_npc1_is_fifth(0xD043));
        assert!(!pc_tukimi_npc1_is_fifth(0xD03F));
        assert!(pc_tukimi_npc1_wants_head_target(2, true));
        assert!(!pc_tukimi_npc1_wants_head_target(4, true));
        assert!(!pc_tukimi_npc1_wants_head_target(2, false));
    }

    #[test]
    fn npc1_dialogue_formulas() {
        // Ordinary: base + rand3 + event_idx * 3.
        assert_eq!(pc_tukimi_npc1_message(0, 1, 2, false), 7856 + 1 + 6);
        assert_eq!(pc_tukimi_npc1_message(0, 1, 2, true), 16198 + 1 + 6);
        // Fifth: separate base family, no event_idx term.
        assert_eq!(pc_tukimi_npc1_message(0, 1, 4, false), 7868 + 1);
        assert_eq!(pc_tukimi_npc1_message(0, 1, 4, true), 16210 + 1);
    }

    fn ms_layout(layout: u8, slot: u8) -> Option<(i32, i32)> {
        let mut out = [0i32; 2];
        if pc_tukimi_ms_layout(layout, slot, out.as_mut_ptr()) == 1 {
            Some((out[0], out[1]))
        } else {
            None
        }
    }

    #[test]
    fn ms_layout_lookup() {
        assert_eq!(ms_layout(0, 0), Some((13, 7)));
        // Layout 2 (index 1) slots 3 and 4 both at (13, 2).
        assert_eq!(ms_layout(1, 3), Some((13, 2)));
        assert_eq!(ms_layout(1, 4), Some((13, 2)));
        assert_eq!(ms_layout(7, 0), None);
        assert_eq!(ms_layout(0, 5), None);
    }
}
