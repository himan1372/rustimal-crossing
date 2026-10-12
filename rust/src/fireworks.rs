//! The Fireworks Show NPCs (`ac_hanabi_npc0.c`, `ac_hanabi_npc1.c`).
//!
//! Verified against `src/actor/npc/ac_hanabi_npc0.c`,
//! `src/actor/npc/ac_hanabi_npc0_talk.c_inc`,
//! `src/actor/npc/ac_hanabi_npc0_anime.c_inc`,
//! `include/ac_hanabi_npc0.h`, `src/actor/npc/ac_hanabi_npc1.c`,
//! `src/actor/npc/ac_hanabi_npc1_talk.c_inc`,
//! `src/actor/npc/ac_hanabi_npc1_anime.c_inc`,
//! `include/ac_hanabi_npc1.h`, `src/game/m_event_map_npc_data.c_inc`,
//! `src/game/m_event_schedule.c_inc`, `src/actor/ac_event_manager.c`,
//! `include/m_name_table.h`, `include/m_lib.h`
//! (GAFE01_00 Rev 0 USA). All 13 brief claims PROVEN.
//!
//! Scope split: the FIREWORKS_SHOW July 4 19:00–20:00 schedule
//! row is already ported in the sibling `event_day` module.
//! This module owns the two NPC actors' pure kernels: the NPC0
//! radius projection and think selection, the NPC1 angle table
//! and action selection, the message arithmetic, and the seven
//! pool layouts with their two rosters. The fan tool actor, the
//! event-manager handlers (pool-block validation, field-ground
//! cleanup, the HANABI_SWITCH effect), the talk state machine,
//! and RNG calls stay in C.
//!
//! Nuances carried: NPC0's walk-end checks ANY collision flag,
//! not just wall flags; the NPC1 angle table must be converted
//! via DEG2SHORT_ANGLE (s16 units), not stored as raw degrees;
//! the seven layouts pair with TWO different rosters (layouts
//! 2/4/7 use YOMISE + STALL0, the rest use YOMISE2 + STALL1);
//! NPC1's TURN tests rotation equality directly, unlike
//! NPC0's chase_angle; the NPC1 loop-flag and clap-sound RNG
//! calls happen in action setup and must stay in C.

/// `DEG2SHORT_ANGLE(deg) = (s16)(deg * 65536/360)`
/// (`m_lib.h:46`).
pub fn deg2short(deg: f32) -> i16 {
    (deg * (65536.0 / 360.0)) as i16
}

/// `DEG2SHORT_ANGLE2(deg) = (int)(deg * 65536/360)`
/// (`m_lib.h:47`).
pub fn deg2short2(deg: f32) -> i32 {
    (deg * (65536.0 / 360.0)) as i32
}

/// `sin_s`/`cos_s` equivalent: short-angle units where 65536 = 2π.
pub fn sin_s(angle: i16) -> f32 {
    (angle as f32 / 65536.0 * std::f32::consts::TAU).sin()
}
pub fn cos_s(angle: i16) -> f32 {
    (angle as f32 / 65536.0 * std::f32::consts::TAU).cos()
}

/// NPC0: the wandering, cheering NPC.
pub mod npc0 {
    pub const ACT_TURN: u8 = 0;
    pub const ACT_WALK: u8 = 1;
    pub const ACT_HURRAHS: u8 = 2;
    pub const ACT_NUM: u8 = 3;

    /// The 100.0f world-unit radius from home.
    pub const MAX_DIST: f32 = 100.0;
    /// Block-edge clamp offsets (`{ 0.0f, 319.0f }`).
    pub const EDGE_OFFSETS: [f32; 2] = [0.0, 319.0];
    /// The banzai probability thresholds: no attention / attention target.
    pub const CHK_VAL: [f32; 2] = [0.05, 0.3];
    /// The turn threshold as a short-angle comparison:
    /// `ABS(diff_angle) > DEG2SHORT_ANGLE2(135.0f)`.
    pub const TURN_THRESHOLD: i32 = 24576; // (int)(135 * 65536/360)
    /// The walk action ends when `move_timer > 60`.
    pub const MOVE_TIMER_LIMIT: u8 = 60;
    /// Walk speed triple; turn and banzai zero all speed params.
    pub const WALK_SPEED: (f32, f32, f32) = (1.0, 0.1, 0.1);

    /// Talk message bases, indexed by `mNpc_GetNpcLooks`.
    pub const MSG_BASES: [u16; 6] = [5711, 5726, 5696, 5741, 5756, 5771];

    /// `aHN0_check_moveRange` equivalent: TRUE when outside the radius.
    pub fn out_of_range(home: (f32, f32), pos: (f32, f32)) -> bool {
        let dx = home.0 - pos.0;
        let dz = home.1 - pos.1;
        dx * dx + dz * dz > MAX_DIST * MAX_DIST
    }

    /// Project back to the 100-unit boundary along the
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

    /// The pure think selection, given the pre-rolled values.
    /// `banzai_roll` is the `RANDOM_F(1.0f)` draw compared
    /// against the attention-gated threshold; `diff_angle` is
    /// `DIFF_SHORT_ANGLE(move_angle, shape_rotation_y)`;
    /// `collision_flags` is the actor's collision flags.
    /// RNG stays in C — this only preserves the branching.
    pub fn select_action(
        banzai_roll: f32,
        has_attention: bool,
        diff_angle: i16,
        collision_flags: u32,
        wall_flags: u32,
    ) -> u8 {
        let threshold = if has_attention { CHK_VAL[1] } else { CHK_VAL[0] };
        if banzai_roll < threshold {
            ACT_HURRAHS
        } else if (diff_angle as i32).abs() > TURN_THRESHOLD {
            ACT_TURN
        } else if collision_flags & wall_flags == 0 {
            ACT_WALK
        } else {
            ACT_TURN
        }
    }

    /// `msg_base[looks] + RANDOM(3)` — the random draw is a C-side parameter here.
    pub fn talk_msg(looks: usize, random3: u16) -> u16 {
        MSG_BASES[looks % MSG_BASES.len()] + random3
    }
}

/// NPC1: the positioned fanning/clapping/cheering NPC.
pub mod npc1 {
    pub const ACT_TURN: u8 = 0;
    pub const ACT_FAN: u8 = 1;
    pub const ACT_CLAPPING: u8 = 2;
    pub const ACT_HURRAHS: u8 = 3;
    pub const ACT_NUM: u8 = 4;

    /// `npc_idx = npc_id - SP_NPC_EV_HANABI_1`.
    pub const EV_HANABI_1: u16 = 0xD028;

    /// `mFI_UNIT_BASE_SIZE_F` = 40.0f.
    pub const UNIT_BASE_SIZE: f32 = 40.0;

    /// The default angle table in degrees (`def_angle[4][7]`,
    /// columns indexed by `mFI_GetPuleIdx()`); the source
    /// stores them via `DEG2SHORT_ANGLE`, so a port must
    /// convert the same way (see [`def_angle`]).
    pub const ANGLE_DEG: [[f32; 7]; 4] = [
        [0.0, 45.0, 45.0, 45.0, 90.0, 90.0, 315.0],
        [135.0, 315.0, 0.0, 0.0, 225.0, 0.0, 135.0],
        [270.0, 90.0, 270.0, 90.0, 45.0, 180.0, 90.0],
        [315.0, 270.0, 180.0, 180.0, 90.0, 180.0, 0.0],
    ];

    /// The table as the C sees it: s16 short-angle units.
    pub fn def_angle(npc_idx: usize, pool_idx: usize) -> Option<i16> {
        ANGLE_DEG.get(npc_idx)?.get(pool_idx).map(|d| super::deg2short(*d))
    }

    /// The destination one field-unit base size from home:
    /// `home + sin_s/cos_s(angle) * mFI_UNIT_BASE_SIZE_F`.
    pub fn dst_pos(home: (f32, f32), angle: i16) -> (f32, f32) {
        (
            home.0 + super::sin_s(angle) * UNIT_BASE_SIZE,
            home.1 + super::cos_s(angle) * UNIT_BASE_SIZE,
        )
    }

    /// The action-selection array `{ FAN, CLAPPING, HURRAHS }`.
    pub const SELECT: [u8; 3] = [ACT_FAN, ACT_CLAPPING, ACT_HURRAHS];

    /// The pure action selection: no attention → always FAN;
    /// otherwise `action[(cur == FAN ? 1 : 0) + (npc_idx & 1)]`.
    /// Gives: even+fan→clap, odd+fan→banzai, even+other→fan, odd+other→clap.
    pub fn select_action(has_attention: bool, cur_action: u8, npc_idx: u8) -> u8 {
        if !has_attention {
            ACT_FAN
        } else {
            let idx = (if cur_action == ACT_FAN { 1 } else { 0 }) + (npc_idx & 1) as usize;
            SELECT[idx]
        }
    }

    /// Animation loop-count tables (`aHN1_setupAction`).
    pub const LOOP_BASE: [i32; 4] = [0, 1, 3, 3];
    pub const LOOP_RND: [f32; 4] = [0.0, 2.0, 4.0, 4.0];

    /// Clap sounds: NA_SE_2F, NA_SE_31, NA_SE_32, NA_SE_33,
    /// selected with `RANDOM(4)` in C.
    pub const CLAP_SOUNDS: [&str; 4] = ["NA_SE_2F", "NA_SE_31", "NA_SE_32", "NA_SE_33"];

    /// Talk message bases, indexed by `mNpc_GetNpcLooks`.
    pub const MSG_BASES: [u16; 6] = [5714, 5729, 5699, 5744, 5759, 5774];

    /// `msg_base[looks] + RANDOM(3) + npc_idx * 3`.
    pub fn talk_msg(looks: usize, random3: u16, npc_idx: u8) -> u16 {
        MSG_BASES[looks % MSG_BASES.len()] + random3 + npc_idx as u16 * 3
    }
}

/// The fireworks pool layouts
/// (`m_event_map_npc_data.c_inc:175-291`).
/// Seven slots: the five fireworks NPCs, the vendor, and the
/// stall. The vendor and stall share the same coordinates in
/// every layout — preserved verbatim.
pub mod layout {
    pub const SLOT_NUM: usize = 7;
    pub const LAYOUT_NUM: usize = 7;
    pub const NPC_MASK: u8 = 0b0011111;

    /// `l_fws_pule1_map` … `l_fws_pule7_map`.
    pub const LAYOUTS: [[(u8, u8); 7]; 7] = [
        [(2, 3), (8, 1), (2, 11), (13, 7), (3, 3), (12, 12), (12, 12)],
        [(13, 13), (1, 3), (13, 2), (3, 8), (14, 7), (3, 12), (3, 12)],
        [(12, 2), (2, 3), (7, 2), (12, 7), (13, 7), (12, 11), (12, 11)],
        [(13, 11), (2, 3), (7, 2), (2, 8), (9, 12), (3, 12), (3, 12)],
        [(13, 6), (1, 7), (13, 13), (2, 2), (1, 11), (12, 3), (12, 3)],
        [(13, 2), (1, 8), (8, 1), (9, 13), (6, 14), (12, 11), (12, 11)],
        [(13, 12), (12, 2), (2, 11), (3, 7), (8, 1), (3, 3), (3, 3)],
    ];

    /// The two actor rosters: 0 = `l_fws_set_actor0`
    /// (YOMISE + FIREWORKS_STALL0), 1 = `l_fws_set_actor1`
    /// (YOMISE2 + FIREWORKS_STALL1). Layouts 2/4/7 use roster 0,
    /// layouts 1/3/5/6 use roster 1.
    pub const ROSTER: [u8; 7] = [1, 0, 1, 0, 1, 1, 0];

    pub fn slot_pos(layout: usize, slot: usize) -> Option<(u8, u8)> {
        LAYOUTS.get(layout)?.get(slot).copied()
    }
}

/// Actor IDs and event-manager constants.
pub mod ids {
    /// `SP_NPC_EV_HANABI_0..4` = `SP_NPC_START + 39..+43`.
    pub const EV_HANABI_0: u16 = 0xD027;
    pub const EV_HANABI_4: u16 = 0xD02B;
    /// `SP_NPC_EV_YOMISE` = `SP_NPC_START + 44`.
    pub const EV_YOMISE: u16 = 0xD02C;
    /// `SP_NPC_EV_YOMISE2` = `SP_NPC_START + 57`.
    pub const EV_YOMISE2: u16 = 0xD039;

    /// `firework_start`: six field-ground objects placed in the pool block.
    pub const FG_COUNT: u8 = 6;
    /// `firework_start`: five actors placed in the pool block.
    pub const ACTOR_COUNT: u8 = 5;
}

// ---- C ABI ----
// All pc_-prefixed: the C originals stay compiled; these are staged kernels.

/// Wall-flag mask used by the C caller (mCoBG_HIT_WALL | mCoBG_HIT_WALL_FRONT).
#[no_mangle]
pub extern "C" fn pc_fw_npc0_next(
    banzai_roll: f32,
    has_attention: u8,
    diff_angle: i16,
    collision_flags: u32,
    wall_flags: u32,
) -> u8 {
    npc0::select_action(banzai_roll, has_attention != 0, diff_angle, collision_flags, wall_flags)
}

#[no_mangle]
pub extern "C" fn pc_fw_npc0_out_of_range(home_x: f32, home_z: f32, x: f32, z: f32) -> u8 {
    npc0::out_of_range((home_x, home_z), (x, z)) as u8
}

#[no_mangle]
pub extern "C" fn pc_fw_npc0_project_radius(home_x: f32, home_z: f32, x: f32, z: f32, out: *mut f32) {
    if !out.is_null() {
        let (px, pz) = npc0::project_to_radius((home_x, home_z), (x, z));
        let out = unsafe { std::slice::from_raw_parts_mut(out, 2) };
        out[0] = px;
        out[1] = pz;
    }
}

#[no_mangle]
pub extern "C" fn pc_fw_npc1_next(has_attention: u8, cur_action: u8, npc_idx: u8) -> u8 {
    npc1::select_action(has_attention != 0, cur_action, npc_idx)
}

/// Returns the default angle in s16 short-angle units (or -1 if out of range).
#[no_mangle]
pub extern "C" fn pc_fw_npc1_default_angle(npc_idx: u8, pool_idx: u8) -> i16 {
    npc1::def_angle(npc_idx as usize, pool_idx as usize).unwrap_or(-1)
}

#[no_mangle]
pub extern "C" fn pc_fw_npc1_dst_pos(home_x: f32, home_z: f32, angle: i16, out: *mut f32) {
    if !out.is_null() {
        let (px, pz) = npc1::dst_pos((home_x, home_z), angle);
        let out = unsafe { std::slice::from_raw_parts_mut(out, 2) };
        out[0] = px;
        out[1] = pz;
    }
}

/// Writes the (x, z) unit coordinates of a layout slot; returns 1 on success.
#[no_mangle]
pub extern "C" fn pc_fw_layout_pos(layout_idx: u8, slot: u8, out: *mut u8) -> u8 {
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

/// Returns the roster index (0 = YOMISE+STALL0, 1 = YOMISE2+STALL1).
#[no_mangle]
pub extern "C" fn pc_fw_layout_roster(layout_idx: u8) -> u8 {
    layout::ROSTER.get(layout_idx as usize).copied().unwrap_or(0xFF)
}

#[no_mangle]
pub extern "C" fn pc_fw_npc0_msg(looks: u8, random3: u16) -> u16 {
    npc0::talk_msg(looks as usize, random3)
}

#[no_mangle]
pub extern "C" fn pc_fw_npc1_msg(looks: u8, random3: u16, npc_idx: u8) -> u16 {
    npc1::talk_msg(looks as usize, random3, npc_idx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn angles() {
        assert_eq!(deg2short(0.0), 0);
        assert_eq!(deg2short(45.0), 8192);
        assert_eq!(deg2short(135.0), 24576);
        assert_eq!(deg2short2(135.0), 24576);
        assert_eq!(npc0::TURN_THRESHOLD, 24576);
        // short-angle sin/cos at 90°
        assert!((sin_s(deg2short(90.0)) - 1.0).abs() < 1e-4);
        assert!(cos_s(deg2short(90.0)).abs() < 1e-4);
    }

    #[test]
    fn npc0_think() {
        let wf = 0b11u32;
        // no attention: roll below 0.05 -> banzai
        assert_eq!(npc0::select_action(0.04, false, 0, 0, wf), npc0::ACT_HURRAHS);
        assert_eq!(npc0::select_action(0.06, false, 0, 0, wf), npc0::ACT_WALK);
        // attention: roll below 0.3 -> banzai
        assert_eq!(npc0::select_action(0.29, true, 0, 0, wf), npc0::ACT_HURRAHS);
        assert_eq!(npc0::select_action(0.31, true, 0, 0, wf), npc0::ACT_WALK);
        // angular difference above 135° -> turn
        assert_eq!(npc0::select_action(0.9, false, 24577, 0, wf), npc0::ACT_TURN);
        assert_eq!(npc0::select_action(0.9, false, 24576, 0, wf), npc0::ACT_WALK);
        // collision flags -> turn
        assert_eq!(npc0::select_action(0.9, false, 0, wf, wf), npc0::ACT_TURN);
        assert_eq!(npc0::select_action(0.9, false, 0, 0, wf), npc0::ACT_WALK);
    }

    #[test]
    fn npc0_range() {
        assert!(!npc0::out_of_range((0.0, 0.0), (60.0, 80.0))); // 100 == 100, not over
        assert!(npc0::out_of_range((0.0, 0.0), (60.0, 80.1)));
        let (px, pz) = npc0::project_to_radius((0.0, 0.0), (200.0, 0.0));
        assert!((px - 100.0).abs() < 0.001 && pz.abs() < 0.001);
        assert_eq!(npc0::EDGE_OFFSETS, [0.0, 319.0]);
        assert_eq!(npc0::MOVE_TIMER_LIMIT, 60);
    }

    #[test]
    fn npc1_angle_table() {
        assert_eq!(npc1::def_angle(0, 0), Some(0));
        assert_eq!(npc1::def_angle(0, 1), Some(8192));
        assert_eq!(npc1::def_angle(1, 0), Some(24576));
        assert_eq!(npc1::def_angle(3, 6), Some(0));
        assert_eq!(npc1::def_angle(4, 0), None);
        assert_eq!(npc1::def_angle(0, 7), None);
        // dst one base size (40.0) from home at 0°
        let (px, pz) = npc1::dst_pos((10.0, 10.0), 0);
        assert!((px - 10.0).abs() < 0.01 && (pz - 50.0).abs() < 0.01);
    }

    #[test]
    fn npc1_action() {
        // no attention -> always fan
        assert_eq!(npc1::select_action(false, npc1::ACT_CLAPPING, 0), npc1::ACT_FAN);
        // even + fan -> clap, odd + fan -> banzai
        assert_eq!(npc1::select_action(true, npc1::ACT_FAN, 0), npc1::ACT_CLAPPING);
        assert_eq!(npc1::select_action(true, npc1::ACT_FAN, 1), npc1::ACT_HURRAHS);
        // even + other -> fan, odd + other -> clap
        assert_eq!(npc1::select_action(true, npc1::ACT_CLAPPING, 0), npc1::ACT_FAN);
        assert_eq!(npc1::select_action(true, npc1::ACT_CLAPPING, 1), npc1::ACT_CLAPPING);
    }

    #[test]
    fn messages() {
        assert_eq!(npc0::talk_msg(0, 2), 5713);
        assert_eq!(npc0::talk_msg(5, 0), 5771);
        assert_eq!(npc1::talk_msg(0, 2, 0), 5716);
        assert_eq!(npc1::talk_msg(0, 2, 1), 5719); // + npc_idx * 3
        assert_eq!(npc1::talk_msg(5, 0, 2), 5780);
    }

    #[test]
    fn layouts() {
        assert_eq!(layout::LAYOUT_NUM, 7);
        assert_eq!(layout::NPC_MASK, 0b0011111);
        assert_eq!(layout::slot_pos(0, 0), Some((2, 3)));
        assert_eq!(layout::slot_pos(0, 5), Some((12, 12)));
        assert_eq!(layout::slot_pos(0, 6), Some((12, 12))); // vendor+stall share coords
        assert_eq!(layout::slot_pos(1, 0), Some((13, 13)));
        assert_eq!(layout::slot_pos(6, 4), Some((8, 1)));
        // roster pairing: 2/4/7 -> 0, 1/3/5/6 -> 1
        assert_eq!(layout::ROSTER, [1, 0, 1, 0, 1, 1, 0]);
        assert_eq!(ids::EV_HANABI_0, 0xD027);
        assert_eq!(ids::EV_HANABI_4, 0xD02B);
        assert_eq!(ids::EV_YOMISE, 0xD02C);
        assert_eq!(ids::EV_YOMISE2, 0xD039);
    }
}
