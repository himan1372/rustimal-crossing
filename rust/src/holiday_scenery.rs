//! Holiday / seasonal scenery structure actors: carp streamer
//! (`ac_koinobori`), festival stall (`ac_yatai`), wishing well
//! (`ac_shrine`), pond lotus (`ac_lotus`) and moon-viewing stand
//! (`ac_tukimi`).
//!
//! Source-verified against ac-decomp `src/actor/ac_{koinobori,yatai,
//! shrine,lotus,tukimi}*.c{,_inc}` and their headers (primary), ACGC-PC-Port
//! (secondary; diffs listed per actor) and `src/game/m_lib.c`
//! (`add_calc`, `get_percent_forAccelBrake`), `src/system/sys_math3d.c`
//! (`Math3DInDivPos2`). All five are `STRUCTURE_ACTOR`s spawned from FG
//! structure ids by `structure_clip->setup_actor_proc`; each writes a
//! `DUMMY_*` FG id over its own unit in `init` (`mFI_SetFG_common`).
//! Display lists, skeletons and palettes stay in C; this module ports the
//! decisions, tables and per-frame math.
//!
//! ## Koinobori (carp streamer, `KOINOBORI_WINDSOCK` 0x583A)
//! Single wait action; keyframe animation looping at speed 0.5, ambient
//! sound `0x35` at the actor position. Cull box 480 x 830.
//! PC: the cull override is skipped in borderless mode (wide default).
//!
//! ## Yatai (fireworks festival stall, `FIREWORKS_STALL0/1` 0x582D/E)
//! `arg0 = npc_id - FIREWORKS_STALL0` selects the left/right model. The
//! 3x3 unit footprint is raised with `mCoBG_SetPluss5PointOffset` from
//! the tables below (stall L skips cells 0 and 8, R skips 2 and 6). It
//! deletes itself as soon as the player stands in another block (unless a
//! scroll demo is running). PC: *unchanged*, unlike tukimi (see below).
//!
//! ## Shrine (wishing well, `WISHING_WELL` 0x5825)
//! - Season/leaf: `season` stored at ct; leaf texture `arg1`:
//!   Christmas leaves for terms 15-16 (Dec 10-25), cherry for term 4
//!   (Apr 1-8), else winter/summer by season.
//! - `arg2` = New Year's Day active: disables talking.
//! - Lights on (`arg0_f = 1`) when `now_sec < 6:00` or `>= 18:00` at ct.
//! - Talk: player within +-22.5 deg (4096 s16 units, compared without
//!   wrap-around) of the well's facing, no other talk actor, no HEM.
//!   Menu: "How are things?" -> town rating (golden axe if rank 6 held
//!   long enough, see `town_rating_reply`); "Apologize" -> discard an
//!   inventory item (refused for quest items: 0x1129 recipient, 0x112A
//!   sender / part-time job, else 0x112B and the item is deleted).
//! - Present: hand-over item glides from (2,40,10) to (2,40,33) with an
//!   accel/brake curve, +0.012 per frame. PC scales by
//!   `dt_num_60fps_frames` and clamps at 1; the water/bubble texture
//!   scroll uses a PC-side `texture_frame` (wraps at 8192) instead of
//!   `play->game_frame`.
//!
//! ## Lotus (`LOTUS` 0x5841)
//! Palette by calendar term (`aLOT_getPalNo`); flower joint 0x12 shown
//! May 26 - Aug 25 (`aLOT_actor_draw_before`). Collides as a pipe
//! (r 70, h 6); a fishing bobber (`mAc_PROFILE_UKI`) hitting it speeds
//! the sway to 2.0 then eases back to 0.5 (`add_calc(.., 0.1, 0.03, 0)`).
//!
//! ## Tukimi (moon-viewing stand, ids `NEWYEAR_SHRINE0/1` 0x582F/0x5830)
//! The profile reuses the `NEWYEAR_SHRINE*` FG ids; `arg0` picks the L/R
//! model, palette `aSTR_PAL_TUKIMI + arg0`, type `aSTR_TYPE_TUKIMI + arg0`.
//! Ground raised by 6 with attribute 100; anim frames 1..271 at 0.5.
//! Deletes itself when the player leaves its block; **PC changes this to
//! "more than one block away"** (borderless camera) but leaves the
//! identical yatai rule alone - an inconsistency worth knowing.

use std::ffi::c_int;

// ---------------------------------------------------------------------------
// Shared helpers (retail m_lib.c helpers live in game_math)
// ---------------------------------------------------------------------------

pub use crate::game_math::{add_calc, percent_accel_brake};

/// `mCoBG_OffsetTable_c`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OffsetTable {
    pub unit_attribute: u8,
    pub center_right: i8,
    pub left_up: i8,
    pub left_down: i8,
    pub right_down: i8,
    pub right_up: i8,
    pub shape: i8,
}

const fn ot(a: u8, c: i8, lu: i8, ld: i8, rd: i8, ru: i8, s: i8) -> OffsetTable {
    OffsetTable { unit_attribute: a, center_right: c, left_up: lu, left_down: ld, right_down: rd, right_up: ru, shape: s }
}

/// One `mCoBG_SetPluss5PointOffset` call: world (x, z) + table entry.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OffsetPlacement {
    pub x: f32,
    pub z: f32,
    pub offset: OffsetTable,
}

/// Block-change despawn rule shared by yatai/tukimi (`aYAT/aTUK_actor_move`).
/// `in_scroll_demo`: any of `mDemo_TYPE_SCROLL{,2,3}` active.
pub fn should_despawn_on_block_change(
    actor_block: (i32, i32),
    player_block: (i32, i32),
    in_scroll_demo: bool,
    pc_adjacent_rule: bool,
) -> bool {
    if in_scroll_demo {
        return false;
    }
    let (dx, dz) = (actor_block.0 - player_block.0, actor_block.1 - player_block.1);
    if pc_adjacent_rule {
        dx.abs() > 1 || dz.abs() > 1
    } else {
        dx != 0 || dz != 0
    }
}

// ---------------------------------------------------------------------------
// Koinobori
// ---------------------------------------------------------------------------

pub mod koinobori {
    pub const FG_ID: u16 = 0x583A; // KOINOBORI_WINDSOCK
    pub const DUMMY_FG: u16 = 0xF114; // DUMMY_KOINOBORI
    pub const CULL_RADIUS: f32 = 480.0;
    pub const CULL_HEIGHT: f32 = 830.0;
    pub const ANIME_SPEED: f32 = 0.5;
    pub const ONGEN_SE: u16 = 0x35;
    /// `aKOI_shadow_vtx_fix_flg_table`.
    pub const SHADOW_VTX_FIX: [u8; 8] = [1, 0, 0, 1, 1, 0, 0, 1];
    pub const SHADOW_SIZE: f32 = 60.0;
}

// ---------------------------------------------------------------------------
// Yatai
// ---------------------------------------------------------------------------

pub mod yatai {
    use super::{ot, OffsetPlacement, OffsetTable};

    pub const FIREWORKS_STALL0: u16 = 0x582D;
    pub const DUMMY_FG: u16 = 0xF10A; // DUMMY_YATAI (== DUMMY_TUKIMI)

    /// `height_table_ct_l`.
    pub const HEIGHT_L: [OffsetTable; 9] = [
        ot(0x64, 0, 0, 0, 0, 0, 0),
        ot(0x64, 8, 0, 8, 8, 8, 1),
        ot(0x64, 8, 8, 8, 8, 0, 1),
        ot(0x64, 8, 0, 8, 8, 8, 1),
        ot(0x64, 8, 8, 8, 8, 8, 0),
        ot(0x64, 8, 8, 8, 0, 8, 1),
        ot(0x64, 8, 8, 0, 8, 8, 1),
        ot(0x64, 8, 8, 8, 0, 8, 1),
        ot(0x64, 0, 0, 0, 0, 0, 0),
    ];
    /// `height_table_ct_r`.
    pub const HEIGHT_R: [OffsetTable; 9] = [
        ot(0x64, 8, 0, 8, 8, 8, 1),
        ot(0x64, 8, 8, 8, 8, 0, 1),
        ot(0x64, 0, 0, 0, 0, 0, 0),
        ot(0x64, 8, 8, 0, 8, 8, 1),
        ot(0x64, 8, 8, 8, 8, 8, 0),
        ot(0x64, 8, 8, 8, 8, 0, 1),
        ot(0x64, 0, 0, 0, 0, 0, 0),
        ot(0x64, 8, 8, 0, 8, 8, 1),
        ot(0x64, 8, 8, 8, 0, 8, 1),
    ];
    const ADD: [f32; 3] = [-40.0, 0.0, 40.0];

    /// Stall variant (0 = L, 1 = R) from its FG id.
    pub fn variant(npc_id: u16) -> u16 {
        npc_id.wrapping_sub(FIREWORKS_STALL0)
    }

    /// `aYAT_set_bgOffset(yatai, arg0 + 2)`: the 7 raised cells.
    pub fn bg_offsets(home_x: f32, home_z: f32, variant: u16) -> Vec<OffsetPlacement> {
        let idx = variant as usize + 2;
        let table = if idx & 1 == 0 { &HEIGHT_L } else { &HEIGHT_R };
        let skip: [usize; 2] = if idx & 1 == 0 { [0, 8] } else { [2, 6] };
        let mut out = Vec::with_capacity(7);
        for i in 0..3 {
            let z = 40.0 + (home_z + ADD[i]);
            for j in 0..3 {
                let cell = j + i * 3;
                if !skip.contains(&cell) {
                    out.push(OffsetPlacement { x: home_x + ADD[j], z, offset: table[cell] });
                }
            }
        }
        out
    }
}

// ---------------------------------------------------------------------------
// Shrine (wishing well)
// ---------------------------------------------------------------------------

pub mod shrine {
    use super::{ot, percent_accel_brake, OffsetPlacement, OffsetTable};
    use std::ffi::c_int;

    pub const FG_ID: u16 = 0x5825; // WISHING_WELL
    pub const DUMMY_FG: u16 = 0xF103; // DUMMY_SHRINE
    pub const CULL_RADIUS: f32 = 900.0;
    pub const CULL_HEIGHT: f32 = 1400.0;
    pub const TALK_DISTANCE: f32 = 100.0;
    /// World offset applied at ct (`world.x += 20`, `world.z -= 19`).
    pub const WORLD_OFFSET: (f32, f32) = (20.0, -19.0);
    pub const ONGEN_SE: u16 = 0x5A;
    /// `DEG2SHORT_ANGLE(22.5f)`.
    pub const TALK_HALF_ANGLE: i32 = 0x1000;
    pub const MSG_TALK: c_int = 4388; // MSG_4388
    pub const WINDOW_COLOR: [u8; 4] = [205, 80, 40, 255];
    /// `SP_NPC_HEM` spawned for the golden axe, at unit (7, 8).
    pub const SP_NPC_HEM: u16 = 0xD08E;
    pub const HEM_UNIT: (c_int, c_int) = (7, 8);

    /// `aSHR_LEAF_TYPE_*`.
    pub const LEAF_SUMMER: c_int = 0;
    pub const LEAF_WINTER: c_int = 1;
    pub const LEAF_CHRISTMAS: c_int = 2;
    pub const LEAF_CHERRY: c_int = 3;

    /// `aSHR_ACTION_*`.
    #[repr(C)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Action {
        Wait = 0,
        Talk = 1,
        TalkGomen = 2,
        MakeHem = 3,
        TalkEnd = 4,
    }

    /// `aSHR_getLeaftype(winter)`.
    pub fn leaf_type(term_idx: c_int, winter: bool) -> c_int {
        match term_idx {
            15 | 16 => LEAF_CHRISTMAS,
            4 => LEAF_CHERRY,
            _ => winter as c_int,
        }
    }

    /// `aSHR_ctrl_light`: lamps lit before 6:00 or from 18:00.
    pub fn lights_on(now_sec: u32) -> bool {
        !(6 * 3600..18 * 3600).contains(&now_sec)
    }

    /// `aSHR_talk_check` angle/state part. Angles are s16; the
    /// difference is taken in int, so there is no wrap-around.
    pub fn can_talk(new_year: bool, other_talk_actor: bool, hem_active: bool, player_angle_y: i16, rot_y: i16) -> bool {
        if hem_active || new_year || other_talk_actor {
            return false;
        }
        (player_angle_y as i32 - rot_y as i32).abs() < TALK_HALF_ANGLE
    }

    /// `height_table_ct` rows (x-20 entry, x+20 entry) for z offsets
    /// 20, -20, -60, -100. Both `height_table` slots alias this.
    pub const HEIGHT: [[OffsetTable; 2]; 4] = [
        [ot(100, 1, 1, 1, 1, 1, 0), ot(100, 1, 1, 1, 1, 1, 0)],
        [ot(100, 2, 2, 1, 1, 4, 0), ot(100, 2, 4, 1, 1, 2, 0)],
        [ot(100, 11, 11, 11, 11, 11, 0), ot(100, 11, 11, 11, 11, 11, 0)],
        [ot(100, 11, 11, 11, 11, 11, 0), ot(100, 11, 11, 11, 11, 11, 0)],
    ];
    const ADD_X: [f32; 2] = [-20.0, 20.0];
    const ADD_Z: [f32; 4] = [20.0, -20.0, -60.0, -100.0];

    /// `aSHR_set_bgOffset`: the 8 raised points.
    pub fn bg_offsets(home_x: f32, home_z: f32) -> [OffsetPlacement; 8] {
        let mut out = [OffsetPlacement { x: 0.0, z: 0.0, offset: HEIGHT[0][0] }; 8];
        for i in 0..4 {
            for k in 0..2 {
                out[i * 2 + k] = OffsetPlacement { x: home_x + ADD_X[k], z: home_z + ADD_Z[i], offset: HEIGHT[i][k] };
            }
        }
        out
    }

    /// `mFAs_CONDITION_*` as used by `aSHR_talk`.
    pub const COND_NO_CASE: c_int = 0;
    pub const COND_DUST_OVER: c_int = 1;
    pub const COND_TREE_LESS: c_int = 2;
    pub const COND_TREE_OVER: c_int = 3;
    pub const COND_GRASS_OVER: c_int = 4;
    /// `mFAs_FIELDRANK_SIX`.
    pub const FIELDRANK_SIX: c_int = 6;

    /// Inputs to the "How are things?" branch of `aSHR_talk`.
    #[derive(Clone, Copy, Debug)]
    pub struct RatingInputs {
        pub condition: c_int,
        pub field_rank: c_int,
        pub good_field: bool,
        pub has_golden_axe: bool,
        pub player_is_foreign: bool,
        pub has_free_slot: bool,
        pub doing_first_job: bool,
    }

    /// `(msg_no, next action)` for "How are things?". The acre-naming
    /// replies also set free strings: column digit `bx` and row letter
    /// `"QABCDEF"[bz]`.
    pub fn town_rating_reply(r: &RatingInputs) -> (c_int, Action) {
        if r.field_rank == FIELDRANK_SIX
            && r.good_field
            && !r.has_golden_axe
            && !r.player_is_foreign
            && r.has_free_slot
            && !r.doing_first_job
            && r.condition != COND_DUST_OVER
        {
            return (0x2C50, Action::MakeHem);
        }
        if r.condition != COND_NO_CASE {
            let msg = match r.condition {
                COND_DUST_OVER => 0x2C4F,
                COND_TREE_LESS => 0x2C45,
                COND_TREE_OVER => 0x2C46,
                _ => 0x2C47,
            };
            return (msg, Action::TalkEnd);
        }
        (0x2C4E - r.field_rank, Action::TalkEnd)
    }

    /// Row letters for the acre free string (`choume_str`).
    pub const CHOUME_STR: [u8; 7] = *b"QABCDEF";

    /// "Apologize" item check (`aSHR_talk_gomen` case 2): `None` = item
    /// is consumed with message 0x112B; `Some(msg)` = refused.
    pub fn gomen_refusal(quest_recipient: bool, quest_sender: bool, first_job_item: bool) -> Option<c_int> {
        if quest_recipient {
            Some(0x1129)
        } else if quest_sender || first_job_item {
            Some(0x112A)
        } else {
            None
        }
    }

    pub const PRESENT_INIT: [f32; 3] = [2.0, 40.0, 10.0];
    pub const PRESENT_AIM: [f32; 3] = [2.0, 40.0, 33.0];
    pub const PRESENT_STEP: f32 = 0.012;

    /// `aSHR_Present_c` walk state.
    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    pub struct Present {
        pub trans: [f32; 3],
        pub percent: f32,
        pub finish: bool,
    }

    impl Present {
        /// `aSHR_Present_move_walk`; `dt_frames` = 1 for retail,
        /// `dt_num_60fps_frames` (+ clamp) for the PC port.
        pub fn walk(&mut self, dt_frames: f32, pc_clamp: bool) {
            let t = percent_accel_brake(self.percent, 0.0, 1.0, 0.15, 0.15);
            self.percent += PRESENT_STEP * dt_frames;
            if pc_clamp && self.percent > 1.0 {
                self.percent = 1.0;
            }
            for i in 0..3 {
                self.trans[i] = PRESENT_INIT[i] + (PRESENT_AIM[i] - PRESENT_INIT[i]) * t;
            }
            if t >= 1.0 {
                self.finish = true;
            }
        }
    }

    /// Texture scroll offsets for the three water tiles
    /// (`aSHR_GetTwoTileGfx{,2}` args), from the frame counter.
    pub fn water_scroll(frame: i32) -> [[i32; 4]; 3] {
        [
            [-(frame.wrapping_mul(50)), 0, 0, -(frame.wrapping_mul(13))],
            [0, frame.wrapping_mul(5), 0, frame.wrapping_mul(3)],
            [0, -(frame.wrapping_mul(12)), 0, 0],
        ]
    }
}

// ---------------------------------------------------------------------------
// Lotus
// ---------------------------------------------------------------------------

pub mod lotus {
    use super::add_calc;

    pub const FG_ID: u16 = 0x5841; // LOTUS
    pub const DUMMY_FG: u16 = 0xF11B; // DUMMY_LOTUS
    pub const PIPE_RADIUS: i16 = 70;
    pub const PIPE_HEIGHT: i16 = 6;
    pub const ANIME_END_FRAME: f32 = 129.0;
    pub const SPEED_IDLE: f32 = 0.5;
    pub const SPEED_HIT: f32 = 2.0;
    /// Skeleton joint carrying the flower.
    pub const FLOWER_JOINT: i32 = 0x12;

    /// `flower_pal_idx_table` indexed by `term_idx`.
    pub const PAL_BY_TERM: [u8; 18] = [8, 8, 8, 0, 1, 1, 1, 2, 2, 3, 4, 5, 6, 7, 8, 8, 8, 8];

    /// `aLOT_getPalNo`.
    pub fn pal_no(term_idx: usize) -> Option<u8> {
        PAL_BY_TERM.get(term_idx).copied()
    }

    /// `aLOT_actor_draw_before` for the flower joint.
    pub fn flower_visible(month: u8, day: u8) -> bool {
        const ON_OFF: [bool; 12] = [false, false, false, false, true, true, true, false, false, false, false, false];
        let Some(&base) = ON_OFF.get((month as usize).wrapping_sub(1)) else { return false };
        match month {
            5 if day < 26 => false,
            8 if day < 26 => true,
            _ => base,
        }
    }

    /// `aLOT_ACTION_*`.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Action {
        Wait1,
        Wait2,
        Shake,
    }

    /// One `action_proc` frame. `bobber_hit`: pipe collided with an
    /// `mAc_PROFILE_UKI` actor; `comeback`: `mPlib_Check_UKI_COMEBACK_STATUS`.
    pub fn step(action: Action, speed: &mut f32, bobber_hit: bool, comeback: bool) -> Action {
        match action {
            Action::Wait1 | Action::Wait2 => {
                if !comeback && bobber_hit {
                    *speed = SPEED_HIT;
                    Action::Shake
                } else {
                    add_calc(speed, SPEED_IDLE, 0.1, 0.03, 0.0);
                    action
                }
            }
            Action::Shake => {
                if !comeback {
                    add_calc(speed, SPEED_IDLE, 0.1, 0.03, 0.0);
                    Action::Shake
                } else {
                    Action::Wait2
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tukimi
// ---------------------------------------------------------------------------

pub mod tukimi {
    pub const NEWYEAR_SHRINE0: u16 = 0x582F;
    pub const DUMMY_FG: u16 = 0xF10A; // DUMMY_TUKIMI (== DUMMY_YATAI)
    pub const BG_OFFSET_HEIGHT: i32 = 6;
    pub const BG_OFFSET_ATTR: i32 = 100;
    pub const ANIME_END_FRAME: f32 = 271.0;
    pub const ANIME_SPEED: f32 = 0.5;
    /// Joint replaced by the stand ("endai") model.
    pub const ENDAI_JOINT: i32 = 7;
    pub const SHADOW_VTX_FIX: [u8; 8] = [1, 1, 0, 0, 1, 0, 1, 0];

    /// `arg0` (0 = L, 1 = R).
    pub fn variant(npc_id: u16) -> u16 {
        npc_id.wrapping_sub(NEWYEAR_SHRINE0)
    }
}

// ---------------------------------------------------------------------------
// C ABI
// ---------------------------------------------------------------------------

#[no_mangle]
pub extern "C" fn pc_shrine_leaf_type(term_idx: c_int, winter: c_int) -> c_int {
    shrine::leaf_type(term_idx, winter != 0)
}

#[no_mangle]
pub extern "C" fn pc_shrine_lights_on(now_sec: u32) -> c_int {
    shrine::lights_on(now_sec) as c_int
}

#[no_mangle]
pub extern "C" fn pc_lotus_pal_no(term_idx: c_int) -> c_int {
    usize::try_from(term_idx).ok().and_then(lotus::pal_no).map_or(-1, |p| p as c_int)
}

#[no_mangle]
pub extern "C" fn pc_lotus_flower_visible(month: u8, day: u8) -> c_int {
    lotus::flower_visible(month, day) as c_int
}

/// Yatai/tukimi despawn rule; `pc_adjacent_rule` selects the PC tukimi
/// behaviour.
#[no_mangle]
pub extern "C" fn pc_scenery_should_despawn(
    actor_bx: c_int,
    actor_bz: c_int,
    player_bx: c_int,
    player_bz: c_int,
    in_scroll_demo: c_int,
    pc_adjacent_rule: c_int,
) -> c_int {
    should_despawn_on_block_change(
        (actor_bx, actor_bz),
        (player_bx, player_bz),
        in_scroll_demo != 0,
        pc_adjacent_rule != 0,
    ) as c_int
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_calc_eases_and_clamps() {
        let mut v = 2.0;
        add_calc(&mut v, 0.5, 0.1, 0.03, 0.0);
        assert!((v - 1.97).abs() < 1e-6);
        let mut v = 0.51;
        add_calc(&mut v, 0.5, 0.1, 0.03, 0.0);
        assert!((v - 0.509).abs() < 1e-6);
    }

    #[test]
    fn accel_brake_endpoints() {
        assert_eq!(percent_accel_brake(0.0, 0.0, 1.0, 0.15, 0.15), 0.0);
        assert_eq!(percent_accel_brake(1.0, 0.0, 1.0, 0.15, 0.15), 1.0);
        let mid = percent_accel_brake(0.5, 0.0, 1.0, 0.15, 0.15);
        assert!((mid - 0.5).abs() < 1e-6);
    }

    #[test]
    fn yatai_offsets_skip_corners() {
        let l = yatai::bg_offsets(100.0, 200.0, 0);
        assert_eq!(l.len(), 7);
        assert_eq!((l[0].x, l[0].z), (100.0, 200.0)); // cell 1: x+0, z-40+40
        let r = yatai::bg_offsets(100.0, 200.0, 1);
        assert_eq!(r.len(), 7);
        assert_eq!(r[0].offset, yatai::HEIGHT_R[0]);
    }

    #[test]
    fn shrine_rules() {
        assert_eq!(shrine::leaf_type(15, false), shrine::LEAF_CHRISTMAS);
        assert_eq!(shrine::leaf_type(4, true), shrine::LEAF_CHERRY);
        assert_eq!(shrine::leaf_type(9, true), shrine::LEAF_WINTER);
        assert!(shrine::lights_on(5 * 3600));
        assert!(!shrine::lights_on(12 * 3600));
        assert!(shrine::can_talk(false, false, false, 0x0FFF, 0));
        assert!(!shrine::can_talk(false, false, false, 0x1000, 0));
        // No wrap-around: -32000 vs 32000 is "far" even though close on the circle.
        assert!(!shrine::can_talk(false, false, false, -32000, 32000));
        assert!(!shrine::can_talk(true, false, false, 0, 0));
    }

    #[test]
    fn shrine_rating() {
        let mut r = shrine::RatingInputs {
            condition: shrine::COND_NO_CASE,
            field_rank: 6,
            good_field: true,
            has_golden_axe: false,
            player_is_foreign: false,
            has_free_slot: true,
            doing_first_job: false,
        };
        assert_eq!(shrine::town_rating_reply(&r), (0x2C50, shrine::Action::MakeHem));
        r.has_golden_axe = true;
        assert_eq!(shrine::town_rating_reply(&r), (0x2C48, shrine::Action::TalkEnd));
        r.condition = shrine::COND_TREE_LESS;
        assert_eq!(shrine::town_rating_reply(&r).0, 0x2C45);
    }

    #[test]
    fn shrine_present_reaches_goal() {
        let mut p = shrine::Present::default();
        let mut frames = 0;
        while !p.finish && frames < 1000 {
            p.walk(1.0, false);
            frames += 1;
        }
        assert!(p.finish);
        assert_eq!(p.trans, shrine::PRESENT_AIM);
        assert_eq!(frames, 85); // ceil(1 / 0.012) + 1
    }

    #[test]
    fn lotus_rules() {
        assert_eq!(lotus::pal_no(3), Some(0));
        assert_eq!(lotus::pal_no(18), None);
        assert!(!lotus::flower_visible(5, 25));
        assert!(lotus::flower_visible(5, 26));
        assert!(lotus::flower_visible(8, 25));
        assert!(!lotus::flower_visible(8, 26));
        assert!(!lotus::flower_visible(0, 1));
        let mut s = 0.5;
        assert_eq!(lotus::step(lotus::Action::Wait1, &mut s, true, false), lotus::Action::Shake);
        assert_eq!(s, 2.0);
        assert_eq!(lotus::step(lotus::Action::Shake, &mut s, false, true), lotus::Action::Wait2);
    }

    #[test]
    fn despawn_rules() {
        assert!(should_despawn_on_block_change((2, 2), (3, 2), false, false));
        assert!(!should_despawn_on_block_change((2, 2), (3, 2), false, true));
        assert!(should_despawn_on_block_change((2, 2), (4, 2), false, true));
        assert!(!should_despawn_on_block_change((2, 2), (4, 2), true, false));
    }
}
