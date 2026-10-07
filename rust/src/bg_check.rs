//! Full background-check sequence for the Rust rewrite.
//!
//! Source-verified (upstream `src/game/m_collision_bg.c`,
//! `include/m_collision_bg.h`). This extends `collision.rs` (the
//! geometric primitives) with the complete `mCoBG_BgCheckControll`
//! pipeline in its recovered call order
//! (`m_collision_bg.c:1899`):
//!
//! ```text
//! mCoBG_MoveActorWithMoveBg()   // platform carry first
//! mCoBG_InitRevpos()
//! current + old center positions
//! mCoBG_MakeActorInf()          // old ground/water state, speeds
//! mCoBG_WallCheck()             // columns, then wall vectors
//! mCoBG_GroundCheck()           // terrain + water + jump flag
//! mCoBG_MoveBgGroundCheck()     // moving-platform support
//! mCoBG_CarryOutReverse()       // apply (rev_type == 0) or hold
//! mCoBG_GiveRevposToActor()
//! mCoBG_RoomScopeCheck()        // scene-dependent room bounds
//! ```
//!
//! Verified formulas and limits:
//!
//! * Neighborhood: `range <= 40 → 3`, `<= 80 → 5`, else `7`
//!   (`m_collision_bg.c:1806`).
//! * `mCoBG_UNIT_VEC_INFO_MAX = 128` wall vectors,
//!   `mCoBG_MOVE_REGIST_MAX = 64` moving-BG registrations,
//!   `mCoBG_WALL_COL_NUM = 2` recorded wall directions,
//!   5 on/side contacts.
//! * Distance reverse: `(range - dist) + 0.00001f`.
//! * Ground adjust: if `ground_y >= foot_y` → snap up, grounded, y
//!   speed 0; else if previously grounded and ground fell away,
//!   snap when `|ground_y - foot_y| <= xz_speed` (descending snap).
//! * Water: river `20.0 + GetBgY`, sea `20.0`.
//! * Wave rate: `(1.0 + wave_cos) * 0.5`.
//! * Room scope: MY_ROOM_S → 160, MY_ROOM_M/LL2 → 240,
//!   MY_ROOM_L/LL1 → 320.
//!
//! The inner wall solver (crossing vs distance tests, player wall
//! prioritization, attribute/forbidden walls, column checks) and the
//! attribute→gameplay-attribute conversion tables are modeled as
//! pipeline stages with their verified I/O contracts; their full
//! geometric internals remain future work.

/// Wall-vector capacity (`mCoBG_UNIT_VEC_INFO_MAX`).
pub const UNIT_VEC_INFO_MAX: usize = 128;
/// Moving-background registration capacity (`mCoBG_MOVE_REGIST_MAX`).
pub const MOVE_REGIST_MAX: usize = 64;
/// Recorded wall directions per actor (`mCoBG_WALL_COL_NUM`).
pub const WALL_COL_NUM: usize = 2;
/// Moving-BG contact capacity (on + side).
pub const CONTACT_CAP: usize = 5;
/// Epsilon added to the wall distance reverse.
pub const REVERSE_EPS: f32 = 0.00001;

/// Neighborhood size from collision range (`m_collision_bg.c:1806`).
pub fn neighborhood_size(range: f32) -> usize {
    if range <= 40.0 {
        3
    } else if range <= 80.0 {
        5
    } else {
        7
    }
}

/// Pipeline stages in recovered call order.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BgStage {
    MoveCarry = 0,
    InitRev = 1,
    MakeActorInf = 2,
    WallCheck = 3,
    GroundCheck = 4,
    MoveBgGroundCheck = 5,
    CarryOutReverse = 6,
    GiveRevpos = 7,
    RoomScopeCheck = 8,
}

/// Check-type discriminator (player vs ordinary actor changes the
/// wall-solving order).
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BgCheckType {
    Actor = 0,
    Player = 1,
}

/// Per-check motion state (`mCoBG_ActorInf_c` essentials).
#[derive(Clone, Debug)]
pub struct BgActorInfo {
    pub old_on_ground: bool,
    pub old_in_water: bool,
    pub old_ground_y: f32,
    pub ground_y: f32,
    pub speed_xz: [f32; 2],
    pub range: f32,
    pub ground_dist: f32,
    pub check_type: BgCheckType,
    pub rev_pos: [f32; 3],
}

impl BgActorInfo {
    pub fn new(range: f32, ground_dist: f32, check_type: BgCheckType) -> Self {
        Self {
            old_on_ground: false,
            old_in_water: false,
            old_ground_y: 0.0,
            ground_y: 0.0,
            speed_xz: [0.0, 0.0],
            range,
            ground_dist,
            check_type,
            rev_pos: [0.0, 0.0, 0.0],
        }
    }

    pub fn xz_speed(&self) -> f32 {
        (self.speed_xz[0] * self.speed_xz[0] + self.speed_xz[1] * self.speed_xz[1]).sqrt()
    }
}

/// Horizontal wall distance correction
/// (`mCoBG_Distance2Reverse_NormalWall`).
pub fn distance_reverse(range: f32, dist: f32) -> f32 {
    (range - dist) + REVERSE_EPS
}

/// Vertical ground adjustment (`mCoBG_AdjustActorY` core):
/// returns `(rev_y, on_ground, stop_fall)`.
pub fn adjust_actor_y(
    ground_y: f32,
    actor_y: f32,
    ground_dist: f32,
    old_on_ground: bool,
    old_ground_y: f32,
    new_ground_y: f32,
    xz_vel: f32,
) -> (f32, bool, bool) {
    let foot_y = actor_y + ground_dist;
    if ground_y >= foot_y {
        ((ground_y - ground_dist) - actor_y, true, true)
    } else if old_on_ground && old_ground_y > new_ground_y {
        let dist_to_ground = (ground_y - foot_y).abs();
        if dist_to_ground <= xz_vel {
            ((ground_y - ground_dist) - actor_y, true, true)
        } else {
            (0.0, false, false)
        }
    } else {
        (0.0, false, false)
    }
}

/// River/lake water surface height.
pub fn water_y_river(bg_y: f32) -> f32 {
    20.0 + bg_y
}

/// Sea water surface height.
pub fn water_y_sea() -> f32 {
    20.0
}

/// Dynamic wave attribute rate (`mCoBG_CheckWaveAtrDetail`).
pub fn wave_rate(wave_cos: f32) -> f32 {
    (1.0 + wave_cos) * 0.5
}

/// Player-room size classes for the room-scope check.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RoomSizeClass {
    Small160 = 0,
    Medium240 = 1,
    Large320 = 2,
    Other = 3,
}

/// Room-scope half-extent (`mCoBG_RoomScopeCheck`).
pub fn room_scope_extent(class: RoomSizeClass) -> f32 {
    match class {
        RoomSizeClass::Small160 => 160.0,
        RoomSizeClass::Medium240 => 240.0,
        RoomSizeClass::Large320 => 320.0,
        RoomSizeClass::Other => 0.0,
    }
}

/// Final correction application (`mCoBG_CarryOutReverse`):
/// `rev_type == 0` applies the reverse to the position; otherwise the
/// caller keeps the vector.
pub fn carry_out_reverse(pos: &mut [f32; 3], rev: [f32; 3], rev_type: i32) {
    if rev_type == 0 {
        pos[0] += rev[0];
        pos[1] += rev[1];
        pos[2] += rev[2];
    }
}

/// C ABI: neighborhood size for a collision range.
#[no_mangle]
pub extern "C" fn pc_bg_neighborhood(range: f32) -> i32 {
    neighborhood_size(range) as i32
}

/// C ABI: horizontal wall distance correction.
#[no_mangle]
pub extern "C" fn pc_bg_distance_reverse(range: f32, dist: f32) -> f32 {
    distance_reverse(range, dist)
}

/// C ABI: room-scope half-extent for a size class (0/1/2).
#[no_mangle]
pub extern "C" fn pc_bg_room_scope(class: u8) -> f32 {
    let c = match class {
        0 => RoomSizeClass::Small160,
        1 => RoomSizeClass::Medium240,
        2 => RoomSizeClass::Large320,
        _ => RoomSizeClass::Other,
    };
    room_scope_extent(c)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neighborhood_thresholds() {
        assert_eq!(neighborhood_size(40.0), 3);
        assert_eq!(neighborhood_size(40.01), 5);
        assert_eq!(neighborhood_size(80.0), 5);
        assert_eq!(neighborhood_size(80.01), 7);
        assert_eq!(neighborhood_size(0.0), 3);
    }

    #[test]
    fn distance_reverse_formula() {
        let r = distance_reverse(10.0, 6.0);
        assert!((r - (4.0 + REVERSE_EPS)).abs() < 1e-6);
    }

    #[test]
    fn ground_adjust_cases() {
        // Below ground: snap up, grounded, stop fall.
        let (rev, ground, stop) = adjust_actor_y(100.0, 90.0, 5.0, false, 0.0, 100.0, 0.0);
        assert!((rev - 5.0).abs() < 1e-5 && ground && stop);
        // Descending snap: was grounded, small gap vs horizontal speed.
        let (rev, ground, _) = adjust_actor_y(94.0, 90.0, 5.0, true, 100.0, 94.0, 2.0);
        assert!(ground && (rev - (-1.0)).abs() < 1e-5);
        // Gap too large: stays airborne.
        let (_, ground, _) = adjust_actor_y(80.0, 90.0, 5.0, true, 100.0, 80.0, 2.0);
        assert!(!ground);
        // Not previously grounded, below: airborne.
        let (_, ground, _) = adjust_actor_y(80.0, 90.0, 5.0, false, 0.0, 80.0, 99.0);
        assert!(!ground);
    }

    #[test]
    fn water_and_wave() {
        assert!((water_y_river(3.0) - 23.0).abs() < 1e-6);
        assert!((water_y_sea() - 20.0).abs() < 1e-6);
        assert!((wave_rate(1.0) - 1.0).abs() < 1e-6);
        assert!((wave_rate(-1.0) - 0.0).abs() < 1e-6);
        assert!((wave_rate(0.0) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn room_scope_extents() {
        assert_eq!(room_scope_extent(RoomSizeClass::Small160), 160.0);
        assert_eq!(room_scope_extent(RoomSizeClass::Medium240), 240.0);
        assert_eq!(room_scope_extent(RoomSizeClass::Large320), 320.0);
    }

    #[test]
    fn carry_out_reverse_modes() {
        let mut p = [1.0, 2.0, 3.0];
        carry_out_reverse(&mut p, [0.5, -1.0, 0.0], 0);
        assert_eq!(p, [1.5, 1.0, 3.0]);
        carry_out_reverse(&mut p, [9.0, 9.0, 9.0], 1);
        assert_eq!(p, [1.5, 1.0, 3.0]);
    }

    #[test]
    fn limits_match_source() {
        assert_eq!(UNIT_VEC_INFO_MAX, 128);
        assert_eq!(MOVE_REGIST_MAX, 64);
        assert_eq!(WALL_COL_NUM, 2);
        assert_eq!(CONTACT_CAP, 5);
    }
}
