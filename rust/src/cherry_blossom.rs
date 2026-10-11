//! The Cherry Blossom Festival NPCs (`ac_hanami_npc0.c`, `ac_hanami_npc1.c`).
//!
//! Verified against `src/actor/npc/ac_hanami_npc0.c`,
//! `src/actor/npc/ac_hanami_npc0_talk.c_inc`,
//! `src/actor/npc/ac_hanami_npc0_anime.c_inc`,
//! `include/ac_hanami_npc0.h`, `src/actor/npc/ac_hanami_npc1.c`,
//! `src/actor/npc/ac_hanami_npc1_talk.c_inc`,
//! `src/actor/npc/ac_hanami_npc1_anime.c_inc`,
//! `include/ac_hanami_npc1.h`, `src/game/m_event_map_npc_data.c_inc`,
//! `src/game/m_event_schedule.c_inc`
//! (GAFE01_00 Rev 0 USA). All 13 brief claims PROVEN.
//!
//! Scope split: the CHERRY_BLOSSOM_FESTIVAL event ID and its
//! festival-placement schedule row are already ported in the
//! sibling `event_day` module. This module owns the two NPC
//! actors' pure kernels: the action selections, the angle
//! tables, the movement bounds, the message arithmetic, and the
//! shrine-block layout. The talk state machine, the tumbler and
//! fan tool actors, the positional sound, and RNG calls stay
//! in C.
//!
//! Nuances carried: NPC0's parity rule (even event_idx →
//! WAIT/MERRY, odd → WAIT/DRINK) is a shift, not a free
//! choice; both hanami angle tables use DEG2SHORT_ANGLE2 (int
//! cast), unlike the fireworks NPC1 table which uses
//! DEG2SHORT_ANGLE (s16); NPC1's walk-end tests ANY collision
//! flag while its think's turn/walk choice tests only
//! wall/wall-front flags — both kept distinct; the tumbler's
//! local variable is oddly named `utiwa_p` in the source (a
//! copy-paste artifact, behaviorally irrelevant).

/// `DEG2SHORT_ANGLE2(deg) = (int)(deg * 65536/360)` (`m_lib.h:47`).
pub fn deg2short2(deg: f32) -> i32 {
    (deg * (65536.0 / 360.0)) as i32
}

/// NPC0: the seated festival participant.
pub mod npc0 {
    pub const ACT_WAIT: u8 = 0;
    pub const ACT_MERRY: u8 = 1;
    pub const ACT_DRINK: u8 = 2;
    pub const ACT_NUM: u8 = 3;

    /// The action array indexed by the shifted random value.
    pub const ACTIONS: [u8; 3] = [ACT_WAIT, ACT_MERRY, ACT_DRINK];

    /// The pure action selection: `idx = RANDOM(2); idx <<= (event_idx & 1)`.
    /// The RNG draw stays in C — this preserves the shift-and-index.
    pub fn select_action(random2: u8, event_idx: u8) -> u8 {
        let idx = ((random2 as u8) << (event_idx & 1)) as usize;
        ACTIONS[idx % ACTIONS.len()]
    }

    /// Animation loop tables (`aHM0_setupAction`).
    pub const LOOP_BASE: [i32; 3] = [1, 2, 1];
    pub const LOOP_RND: [f32; 3] = [2.0, 3.0, 1.0];

    /// Clap sounds: NA_SE_2F, NA_SE_31, NA_SE_32, NA_SE_33,
    /// rolled with `RANDOM(ARRAY_COUNT)` only for MERRY, after
    /// the animation setup.
    pub const CLAP_SOUNDS: [&str; 4] = ["NA_SE_2F", "NA_SE_31", "NA_SE_32", "NA_SE_33"];

    /// `SP_NPC_EV_HANAMI_0` = `SP_NPC_START + 50`.
    pub const EV_HANAMI_0: u16 = 0xD032;

    /// The four initial angles, in int short-angle units
    /// (`DEG2SHORT_ANGLE2`), indexed by `event_idx = npc_id − SP_NPC_EV_HANAMI_0`.
    pub fn def_angle(event_idx: usize) -> Option<i32> {
        [45.0f32, -90.0, -45.0, 90.0]
            .get(event_idx)
            .map(|d| super::deg2short2(*d))
    }

    /// Talk message bases, indexed by `mNpc_GetNpcLooks`.
    pub const MSG_BASES: [u16; 6] = [6445, 6460, 6430, 6475, 6490, 6505];

    /// `msg_base[looks] + RANDOM(3) + event_idx * 3`.
    pub fn talk_msg(looks: usize, random3: u16, event_idx: u8) -> u16 {
        MSG_BASES[looks % MSG_BASES.len()] + random3 + event_idx as u16 * 3
    }

    /// The tumbler is attempted only for odd `event_idx` with a
    /// NULL right-hand item actor (the C then creates
    /// TOOL_TUMBLER through the tools clip and assigns the
    /// pointer only if non-NULL).
    pub fn should_make_tumbler(event_idx: u8, right_hand_null: bool) -> bool {
        (event_idx & 1) == 1 && right_hand_null
    }
}

/// NPC1: the standing, dancing, wandering participant.
pub mod npc1 {
    pub const ACT_TURN: u8 = 0;
    pub const ACT_WALK: u8 = 1;
    pub const ACT_NUM: u8 = 2;

    /// The 100.0f world-unit radius from home (squared, strict `>`).
    pub const MAX_DIST: f32 = 100.0;
    /// Block-edge clamp offsets (`{ 0.0f, 319.0f }`).
    pub const EDGE_OFFSETS: [f32; 2] = [0.0, 319.0];
    /// The turn threshold as a short-angle comparison:
    /// `ABS(diff) > DEG2SHORT_ANGLE2(135.0f)`.
    pub const TURN_THRESHOLD: i32 = 24576;
    /// Walk speeds; turn zeroes all speed params.
    pub const WALK_SPEED: (f32, f32, f32) = (1.0, 0.1, 0.1);

    /// The seven initial angles in int short-angle units
    /// (`DEG2SHORT_ANGLE2`), indexed by `mFI_GetPuleIdx()`.
    pub fn def_angle(pool_idx: usize) -> Option<i32> {
        [45.0f32, 45.0, 315.0, 270.0, 0.0, 315.0, 315.0]
            .get(pool_idx)
            .map(|d| super::deg2short2(*d))
    }

    /// The pure action selection: `next_action != −1` wins;
    /// otherwise `ABS(diff) > 135°` → TURN, else WALK when no
    /// wall/wall-front flags, else TURN. RNG stays in C.
    pub fn select_action(
        next_action: i8,
        diff_angle: i16,
        collision_flags: u32,
        wall_flags: u32,
    ) -> u8 {
        if next_action >= 0 {
            next_action as u8
        } else if (diff_angle as i32).abs() > TURN_THRESHOLD {
            ACT_TURN
        } else if collision_flags & wall_flags == 0 {
            ACT_WALK
        } else {
            ACT_TURN
        }
    }

    /// Squared-radius check: TRUE when strictly outside the 100-unit radius.
    pub fn out_of_range(home: (f32, f32), pos: (f32, f32)) -> bool {
        let dx = home.0 - pos.0;
        let dz = home.1 - pos.1;
        dx * dx + dz * dz > MAX_DIST * MAX_DIST
    }

    /// Project back to the 100-unit boundary along the
    /// home→current angle (the C uses search_position_angleY +
    /// sin_s/cos_s; atan2 is the equivalent).
    pub fn project_to_radius(home: (f32, f32), pos: (f32, f32)) -> (f32, f32) {
        let angle = (pos.1 - home.1).atan2(pos.0 - home.0);
        (home.0 + angle.sin() * MAX_DIST, home.1 + angle.cos() * MAX_DIST)
    }

    /// Talk message bases, indexed by `mNpc_GetNpcLooks`.
    pub const MSG_BASES: [u16; 6] = [6457, 6472, 6442, 6487, 6502, 6517];

    /// `msg_base[looks] + RANDOM(3)` — confirmed to have NO
    /// event_idx offset, unlike NPC0.
    pub fn talk_msg(looks: usize, random3: u16) -> u16 {
        MSG_BASES[looks % MSG_BASES.len()] + random3
    }
}

/// The festival layout (`m_event_map_npc_data.c_inc`).
/// Eight slots: the five hanami NPCs, SAKURA_TABLE1,
/// SAKURA_TABLE0, and SP_NPC_EV_SONCHO2. A single map variant
/// on a shrine block (unlike the fireworks pool layouts).
pub mod layout {
    pub const SLOT_NUM: usize = 8;
    pub const NPC_MASK: u8 = 0b0011111;

    /// `l_fv_set_actor0` positions in field-unit X/Z.
    pub const SLOTS: [(u8, u8); 8] = [
        (10, 6), // SP_NPC_EV_HANAMI_0
        (12, 8), // SP_NPC_EV_HANAMI_1
        (6, 10), // SP_NPC_EV_HANAMI_2
        (4, 12), // SP_NPC_EV_HANAMI_3
        (4, 6),  // SP_NPC_EV_HANAMI_4
        (5, 11), // SAKURA_TABLE1
        (11, 7), // SAKURA_TABLE0
        (9, 11), // SP_NPC_EV_SONCHO2
    ];

    /// The record fields: `{ 0, 5, 1, 8, ..., mRF_BLOCKKIND_SHRINE }`.
    pub const MAP_MAX: u8 = 1;
    pub const NPC_SLOT_COUNT: u8 = 5;
    pub const SLOT_TOTAL: u8 = 8;

    pub fn slot_pos(slot: usize) -> Option<(u8, u8)> {
        SLOTS.get(slot).copied()
    }
}

/// Actor and object IDs.
pub mod ids {
    /// `SP_NPC_EV_HANAMI_0..4` = `SP_NPC_START + 50..+54`.
    pub const EV_HANAMI_0: u16 = 0xD032;
    pub const EV_HANAMI_4: u16 = 0xD036;
    /// `SP_NPC_EV_SONCHO2`.
    pub const EV_SONCHO2: u16 = 0xD074;
    /// The Sakura tables.
    pub const SAKURA_TABLE0: u16 = 0x582A;
    pub const SAKURA_TABLE1: u16 = 0x582B;
}

// ---- C ABI ----
// All pc_-prefixed: the C originals stay compiled; these are staged kernels.

#[no_mangle]
pub extern "C" fn pc_cb_npc0_next(random2: u8, event_idx: u8) -> u8 {
    npc0::select_action(random2, event_idx)
}

#[no_mangle]
pub extern "C" fn pc_cb_npc0_msg(looks: u8, random3: u16, event_idx: u8) -> u16 {
    npc0::talk_msg(looks as usize, random3, event_idx)
}

#[no_mangle]
pub extern "C" fn pc_cb_npc0_def_angle(event_idx: u8) -> i32 {
    npc0::def_angle(event_idx as usize).unwrap_or(0)
}

#[no_mangle]
pub extern "C" fn pc_cb_npc0_make_tumbler(event_idx: u8, right_hand_null: u8) -> u8 {
    npc0::should_make_tumbler(event_idx, right_hand_null != 0) as u8
}

#[no_mangle]
pub extern "C" fn pc_cb_npc1_next(next_action: i8, diff_angle: i16, collision_flags: u32, wall_flags: u32) -> u8 {
    npc1::select_action(next_action, diff_angle, collision_flags, wall_flags)
}

#[no_mangle]
pub extern "C" fn pc_cb_npc1_msg(looks: u8, random3: u16) -> u16 {
    npc1::talk_msg(looks as usize, random3)
}

#[no_mangle]
pub extern "C" fn pc_cb_npc1_def_angle(pool_idx: u8) -> i32 {
    npc1::def_angle(pool_idx as usize).unwrap_or(0)
}

#[no_mangle]
pub extern "C" fn pc_cb_npc1_out_of_range(home_x: f32, home_z: f32, x: f32, z: f32) -> u8 {
    npc1::out_of_range((home_x, home_z), (x, z)) as u8
}

#[no_mangle]
pub extern "C" fn pc_cb_npc1_project_radius(home_x: f32, home_z: f32, x: f32, z: f32, out: *mut f32) {
    if !out.is_null() {
        let (px, pz) = npc1::project_to_radius((home_x, home_z), (x, z));
        let out = unsafe { std::slice::from_raw_parts_mut(out, 2) };
        out[0] = px;
        out[1] = pz;
    }
}

/// Writes the (x, z) unit coordinates of a layout slot; returns 1 on success.
#[no_mangle]
pub extern "C" fn pc_cb_layout_pos(slot: u8, out: *mut u8) -> u8 {
    match layout::slot_pos(slot as usize) {
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
    fn npc0_parity() {
        // even event_idx: RANDOM(2) selects WAIT or MERRY
        assert_eq!(npc0::select_action(0, 0), npc0::ACT_WAIT);
        assert_eq!(npc0::select_action(1, 0), npc0::ACT_MERRY);
        assert_eq!(npc0::select_action(1, 2), npc0::ACT_MERRY);
        // odd event_idx: RANDOM(2) selects WAIT or DRINK
        assert_eq!(npc0::select_action(0, 1), npc0::ACT_WAIT);
        assert_eq!(npc0::select_action(1, 1), npc0::ACT_DRINK);
        assert_eq!(npc0::select_action(1, 3), npc0::ACT_DRINK);
    }

    #[test]
    fn npc0_angles() {
        assert_eq!(npc0::def_angle(0), Some(8192));    // 45°
        assert_eq!(npc0::def_angle(1), Some(-16384));  // -90°
        assert_eq!(npc0::def_angle(2), Some(-8192));   // -45°
        assert_eq!(npc0::def_angle(3), Some(16384));    // 90°
        assert_eq!(npc0::def_angle(4), None);
        assert_eq!(npc0::EV_HANAMI_0, 0xD032);
    }

    #[test]
    fn npc0_msgs() {
        assert_eq!(npc0::talk_msg(0, 2, 0), 6447);
        assert_eq!(npc0::talk_msg(0, 2, 1), 6450); // + event_idx * 3
        assert_eq!(npc0::talk_msg(5, 0, 2), 6511);
        assert!(npc0::should_make_tumbler(1, true));
        assert!(!npc0::should_make_tumbler(0, true));
        assert!(!npc0::should_make_tumbler(1, false));
        assert_eq!(npc0::LOOP_BASE, [1, 2, 1]);
        assert_eq!(npc0::LOOP_RND, [2.0, 3.0, 1.0]);
    }

    #[test]
    fn npc1_angles() {
        let expected = [8192, 8192, 57344, 49152, 0, 57344, 57344];
        for (i, e) in expected.iter().enumerate() {
            assert_eq!(npc1::def_angle(i), Some(*e), "pool_idx {i}");
        }
        assert_eq!(npc1::def_angle(7), None);
        assert_eq!(npc1::TURN_THRESHOLD, 24576);
    }

    #[test]
    fn npc1_think() {
        let wf = 0b11u32;
        // next_action takes priority
        assert_eq!(npc1::select_action(1, 0, 0, wf), npc1::ACT_WALK);
        // angle difference strictly > 135° -> turn
        assert_eq!(npc1::select_action(-1, 24577, 0, wf), npc1::ACT_TURN);
        assert_eq!(npc1::select_action(-1, 24576, 0, wf), npc1::ACT_WALK);
        // collision flags -> turn
        assert_eq!(npc1::select_action(-1, 0, wf, wf), npc1::ACT_TURN);
        assert_eq!(npc1::select_action(-1, 0, 0, wf), npc1::ACT_WALK);
    }

    #[test]
    fn npc1_range() {
        assert!(!npc1::out_of_range((0.0, 0.0), (60.0, 80.0))); // exactly 100, not over
        assert!(npc1::out_of_range((0.0, 0.0), (60.0, 80.1)));
        assert_eq!(npc1::EDGE_OFFSETS, [0.0, 319.0]);
        assert_eq!(npc1::WALK_SPEED, (1.0, 0.1, 0.1));
    }

    #[test]
    fn npc1_msgs() {
        assert_eq!(npc1::talk_msg(0, 2), 6459);
        assert_eq!(npc1::talk_msg(5, 0), 6517);
    }

    #[test]
    fn festival_layout() {
        assert_eq!(layout::SLOT_NUM, 8);
        assert_eq!(layout::NPC_MASK, 0b0011111);
        assert_eq!(layout::MAP_MAX, 1); // single map variant
        assert_eq!(layout::slot_pos(0), Some((10, 6)));
        assert_eq!(layout::slot_pos(5), Some((5, 11)));  // SAKURA_TABLE1
        assert_eq!(layout::slot_pos(6), Some((11, 7)));  // SAKURA_TABLE0
        assert_eq!(layout::slot_pos(7), Some((9, 11)));  // SONCHO2
        assert_eq!(layout::slot_pos(8), None);
        assert_eq!(ids::EV_HANAMI_0, 0xD032);
        assert_eq!(ids::EV_HANAMI_4, 0xD036);
        assert_eq!(ids::EV_SONCHO2, 0xD074);
        assert_eq!(ids::SAKURA_TABLE0, 0x582A);
        assert_eq!(ids::SAKURA_TABLE1, 0x582B);
    }
}
