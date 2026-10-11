//! Camera parameters and pure camera math (`m_camera2.c`).
//!
//! Source-verified against ac-decomp `src/game/m_camera2.c` (primary) and
//! ACGC-PC-Port (secondary). The camera's per-mode state machines (normal,
//! talk, wade, door, lock, demo, item, simple, cust_talk, inter,
//! staff_roll) drive `GAME_PLAY::camera` and stay in C; this module ports
//! the parameter tables and the geometric helpers they share. Angle
//! constants were evaluated with mingw32 gcc (`DEG2SHORT_ANGLE` truncates).
//!
//! ## PC-port changes (m_camera2.c is the most-modified file in this set)
//! - Every morph/cull counter advances by `dt_num_60fps_frames`
//!   (`Camera2_AddCullTimer`, `morph_counter`, `now_delta`, staff roll).
//! - `PC_ENHANCEMENTS`: C-stick camera control in every scene (retail:
//!   indoors only) and outdoor C-stick changes reset instead of
//!   double-transitioning; `Camera2_InDoorCheck` made non-static so the HUD
//!   hint can be restricted to indoors.
//! - Borderless mode (`mPlib_IsWadeDisabled`) skips the block-edge swing.
//! - Extra Z distance while riding Kapp'n's boat below the dock.
//!
//! ## Facts
//! - Goal distance 620 for every mode except talk and cust_talk (290);
//!   goal direction X = -135 deg (-164.114 deg talk, -165.2125 deg
//!   cust_talk), Y = -180 deg.
//! - Indoors in normal mode, the C-stick picks one of three zoom steps:
//!   distance -280 / 0 / +300 with pitch -23.24 / 0 / +19.12 deg (pitch
//!   follows the *distance* index), and yaw -16.1 / 0 / +16.1 deg.
//! - Outdoors (normal/wade, not "long border" mode) the camera tilts down
//!   when the player is in the southern part of acre row 1:
//!   `dir.x += z_ratio * 250` for negative ratios.
//! - Eye = center + polar(distance, inverse direction): `eye.y += d sin(ax)`,
//!   `eye.xz += d cos(ax) * (sin(ay), cos(ay))`.
//! - Culling of foreground objects (`Camera2_CheckEnterCullingArea`) is
//!   only active in talk/cust_talk after 15 frames, item/simple(mode 1)
//!   after 6 frames, or a door with `morph_counter < 70`; an object is
//!   culled when it is south of the look-at centre and within the X span
//!   of the talkers (or player) widened by `width`, but never narrower than
//!   the eye X +- width.
//! - Border box: 110 units in from the acre edges (1.5x in "long border"
//!   mode), with a -15 under-border adjustment outdoors and one extra unit
//!   indoors.
//! - Mic position is fixed (not following the camera) in the scenes marked
//!   in `MIC_POS_FIX`.

use crate::game_math::Xyz;

/// `CAMERA2_PROCESS_*`.
pub const PROCESS_STOP: usize = 0;
pub const PROCESS_NORMAL: usize = 1;
pub const PROCESS_WADE: usize = 2;
pub const PROCESS_TALK: usize = 3;
pub const PROCESS_DEMO: usize = 4;
pub const PROCESS_ITEM: usize = 5;
pub const PROCESS_LOCK: usize = 6;
pub const PROCESS_DOOR: usize = 7;
pub const PROCESS_SIMPLE: usize = 8;
pub const PROCESS_CUST_TALK: usize = 9;
pub const PROCESS_INTER: usize = 10;
pub const PROCESS_STAFF_ROLL: usize = 11;
pub const PROCESS_INTER2: usize = 12;
pub const PROCESS_NUM: usize = 13;

/// `distance_array`.
pub const GOAL_DISTANCE: [f32; PROCESS_NUM] =
    [620.0, 620.0, 620.0, 290.0, 620.0, 620.0, 620.0, 620.0, 620.0, 290.0, 620.0, 620.0, 620.0];

/// `direction_array` X (Y is -180 deg = -32768, Z 0 for all modes).
pub const GOAL_DIRECTION_X: [i16; PROCESS_NUM] =
    [-24576, -24576, -24576, -29876, -24576, -24576, -24576, -24576, -24576, -30076, -24576, -24576, -24576];
pub const GOAL_DIRECTION_Y: i16 = -32768;

/// Indoor zoom steps.
pub const INDOOR_ADD_DISTANCE: [f32; 3] = [-280.0, 0.0, 300.0];
pub const INDOOR_ADD_DIR_Y: [i16; 3] = [-2930, 0, 2930];
pub const INDOOR_ADD_DIR_X: [i16; 3] = [-4230, 0, 3480];

/// `Camera2_InDoorCheck` on a field id (room, NPC room, player room).
pub fn is_indoor_field(field_id: u16) -> bool {
    matches!(field_id & 0xF000, 0x3000 | 0x4000 | 0x6000)
}

/// Swing input for `Camera2_Normal_Swing` (outdoor normal/wade only).
#[derive(Clone, Copy, Debug)]
pub struct Swing {
    pub block_z: i32,
    pub z_ratio: f32,
}

/// `Camera2_Get_GoalDistanceAndDirection` (before
/// `Camera2_main_Normal_AdjustDistanceAndDirection`, which stays in C).
pub fn goal_distance_direction(
    main_index: usize,
    indoor: bool,
    add_dist_idx: i32,
    add_dir_idx: i32,
    long_border: bool,
    swing: Option<Swing>,
) -> (f32, [i16; 3]) {
    let mi = if main_index < PROCESS_NUM { main_index } else { 0 };
    let mut dist = GOAL_DISTANCE[mi];
    let mut dir = [GOAL_DIRECTION_X[mi], GOAL_DIRECTION_Y, 0];
    if mi == PROCESS_NORMAL && indoor {
        let di = if (0..3).contains(&add_dist_idx) { add_dist_idx as usize } else { 1 };
        let yi = if (0..3).contains(&add_dir_idx) { add_dir_idx as usize } else { 1 };
        dist += INDOOR_ADD_DISTANCE[di];
        dir[1] = dir[1].wrapping_add(INDOOR_ADD_DIR_Y[yi]);
        dir[0] = dir[0].wrapping_add(INDOOR_ADD_DIR_X[di]);
    }
    if (mi == PROCESS_NORMAL || mi == PROCESS_WADE) && !indoor && !long_border {
        if let Some(s) = swing {
            if s.block_z == 1 && s.z_ratio < 0.0 {
                dir[0] = dir[0].wrapping_add((s.z_ratio * 250.0) as i16);
            }
        }
    }
    (dist, dir)
}

/// `Camera2_PolaPosCalc`: eye from centre, inverse direction and distance.
pub fn pola_pos(center: Xyz, inv_dir_x: i16, inv_dir_y: i16, dist: f32) -> Xyz {
    let ax = inv_dir_x as f32 * (std::f32::consts::PI / 32768.0);
    let ay = inv_dir_y as f32 * (std::f32::consts::PI / 32768.0);
    let dxz = dist * ax.cos();
    Xyz::new(center.x + dxz * ay.sin(), center.y + dist * ax.sin(), center.z + dxz * ay.cos())
}

/// Inverse of the stored direction (`dir + 0x8000`), as used by
/// `Camera2_SetEyePos_fromCenterPosCalc`.
pub fn inverse_dir(d: i16) -> i16 {
    d.wrapping_add(i16::MIN)
}

/// `Camera2_BorderSize` (110 everywhere).
pub const BORDER_SIZE: f32 = 110.0;

/// `Camera2_GetBorderScale`: (x0, x1, z0, z1) border box for an acre whose
/// entrance/base is at (`base_x`, `base_z`). Names follow the C (x0 > x1).
pub fn border_box(base_x: f32, base_z: f32, block_w: f32, block_h: f32, long_border: bool, indoor: bool) -> (f32, f32, f32, f32) {
    let scale = if long_border { 1.5 } else { 1.0 };
    let under = if indoor { 0.0 } else { -15.0 };
    let x0 = base_x + block_w - scale * BORDER_SIZE;
    let x1 = base_x + scale * BORDER_SIZE;
    let z1 = block_h + (base_z - scale * (BORDER_SIZE + under));
    let mut z0 = base_z + scale * (BORDER_SIZE + 40.0);
    if indoor {
        z0 -= 40.0;
    }
    (x0, x1, z0, z1)
}

/// `Camera2_Get_PosBlockRatio`: position within the border box, -1..1.
pub fn pos_block_ratio(pos: Xyz, border: (f32, f32, f32, f32)) -> (f32, f32) {
    let (x0, x1, z0, z1) = border;
    let xc = (x0 + x1) * 0.5;
    let xr = xc - x1;
    let zc = (z0 + z1) * 0.5;
    let zl = zc - z0;
    let x = if crate::game_math::f32_is_zero(xr) { 0.0 } else { ((pos.x - xc) / xr).clamp(-1.0, 1.0) };
    let z = if crate::game_math::f32_is_zero(zl) { 0.0 } else { ((pos.z - zc) / zl).clamp(-1.0, 1.0) };
    (x, z)
}

/// Cull timers / counters that gate `Camera2_CheckCullingMode`.
#[derive(Clone, Copy, Debug, Default)]
pub struct CullState {
    pub talk_cull_timer: f32,
    pub item_cull_timer: f32,
    pub door_morph_counter: i32,
    pub cust_talk_cull_timer: f32,
    pub simple_mode: i32,
    pub simple_cull_timer: f32,
}

/// `Camera2_CheckCullingMode`.
pub fn culling_mode(main_index: usize, s: &CullState) -> bool {
    match main_index {
        PROCESS_TALK => s.talk_cull_timer >= 15.0,
        PROCESS_ITEM => s.item_cull_timer >= 6.0,
        PROCESS_DOOR => s.door_morph_counter < 70,
        PROCESS_CUST_TALK => s.cust_talk_cull_timer >= 15.0,
        PROCESS_SIMPLE => s.simple_mode == 1 && s.simple_cull_timer >= 6.0,
        _ => false,
    }
}

/// `Camera2_AddCullTimer` (retail: +1 per frame, PC: +dt).
pub fn add_cull_timer(t: f32, max: f32, dt: f32) -> f32 {
    (t + dt).min(max)
}

/// `Camera2_CheckEnterCullingArea` core: is (pos_x, pos_z) hidden?
/// `xs` are the X positions of the two subjects (talkers or player).
pub fn in_culling_area(center_z: f32, eye_x: f32, xs: (f32, f32), pos_x: f32, pos_z: f32, width: f32) -> bool {
    if center_z > pos_z {
        return false;
    }
    let (a, b) = xs;
    let (lo, hi) = if a > b { (b, a) } else { (a, b) };
    let lo = (lo - width).min(eye_x - width);
    let hi = (hi + width).max(eye_x + width);
    lo <= pos_x && pos_x <= hi
}

/// `Camera2_Check_MicPosFix` per scene (52 scenes; out of range -> TRUE).
pub const MIC_POS_FIX: [bool; 52] = [
    true, true, true, true, true, true, false, true, true, false, true, true, false, false, false, true, true, false,
    false, true, false, false, false, false, false, false, true, true, true, false, true, false, true, true, true,
    false, false, false, false, false, false, false, false, false, false, false, false, false, false, true, false,
    false,
];

pub fn mic_pos_fixed(scene: i32) -> bool {
    usize::try_from(scene).ok().and_then(|s| MIC_POS_FIX.get(s).copied()).unwrap_or(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn goal_tables() {
        let (d, dir) = goal_distance_direction(PROCESS_TALK, false, 1, 1, false, None);
        assert_eq!((d, dir), (290.0, [-29876, -32768, 0]));
        let (d, dir) = goal_distance_direction(PROCESS_NORMAL, true, 0, 2, false, None);
        assert_eq!(d, 340.0);
        assert_eq!(dir, [-24576 - 4230, (-32768i16).wrapping_add(2930), 0]);
        let (_, dir) = goal_distance_direction(PROCESS_NORMAL, false, 1, 1, false, Some(Swing { block_z: 1, z_ratio: -0.5 }));
        assert_eq!(dir[0], -24576 - 125);
        assert_eq!(goal_distance_direction(99, false, 1, 1, false, None).0, 620.0);
    }

    #[test]
    fn geometry() {
        let eye = pola_pos(Xyz::ZERO, 0, 0, 100.0);
        assert!((eye.z - 100.0).abs() < 1e-4 && eye.x.abs() < 1e-4);
        assert_eq!(inverse_dir(-24576), 8192);
        assert!(is_indoor_field(0x6001) && !is_indoor_field(0x0001));
        assert!(in_culling_area(0.0, 0.0, (10.0, -10.0), 50.0, 5.0, 60.0));
        assert!(!in_culling_area(10.0, 0.0, (10.0, -10.0), 50.0, 5.0, 60.0));
        assert!(culling_mode(PROCESS_DOOR, &CullState::default()));
        assert!(!mic_pos_fixed(6));
        assert!(mic_pos_fixed(-1));
        assert_eq!(add_cull_timer(14.5, 15.0, 1.0), 15.0);
    }
}
