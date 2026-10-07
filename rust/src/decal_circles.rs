//! Decal-circle machinery for the Rust rewrite.
//!
//! Source-verified (upstream `src/game/m_collision_bg_wall.c_inc`,
//! `src/game/m_collision_bg_column.c_inc`, `src/game/m_collision_bg.c`,
//! `src/game/m_player_main_dig_scoop.c_inc`,
//! `src/game/m_player_main_get_scoop.c_inc`, `src/game/m_play.c`):
//!
//! There are two distinct "circle" mechanisms, and the research
//! brief conflated them — the decomp source separates them cleanly:
//!
//! ## 1. Circle-defence walls (`mCoBG_MakeCircleDefenceWall`)
//!
//! This does NOT populate `mCoBG_decal_circle`. It appends *wall
//! vectors* to the wall list: for each ordered pair of distinct
//! columns whose unit offset (dx,dz) matches one of eight
//! `defence_wall_info` entries, it emits TWO walls from col0's
//! position to col1's position — one per normal/angle/wall_name
//! pair in the table entry (opposite-facing normals), both with
//! `atr_wall = TRUE` and `regist_p = NULL`. Geometrically these
//! bridge the gaps between adjacent object columns. Gated on
//! `attr_wall && old_on_ground`; capped at `mCoBG_UNIT_VEC_INFO_MAX`
//! (128) wall vectors.
//!
//! The eight (dx,dz) entries (verbatim):
//! * (±1,0): normals (0,+1)/0° and (0,−1)/180°, both WALL_UP
//! * (0,±1): normals (+1,0)/90° and (−1,0)/−90°, both WALL_RIGHT
//! * (±1,±1): (±√½,∓√½)/135° and (∓√½,±√½)/−45°, both WALL_SLATE_DOWN
//! * (±1,∓1): (∓√½,∓√½)/−135° and (±√½,±√½)/45°, both WALL_SLATE_UP
//!
//! ## 2. Decal circles (`mCoBG_decal_circle[3]`)
//!
//! Temporary timer-driven collision columns. Registration records
//! (`mCoBG_regist_circle_info[3]`: position, in-use flag,
//! start/end/now radius, start/now timer) drive live
//! `mCoBG_column_c` records fed as the *second*
//! `mCoBG_ColumnWallCheck` pass. The radius interpolates linearly
//! from start_radius to end_radius over the timer, then the slot
//! deactivates. Columns get `height = pos.y`,
//! `atr_wall = TRUE`, and unit coords from the position.
//!
//! Gameplay meaning (the "why decal" answer): the registrars are
//! the player dig/scoop actions —
//! `mCoBG_RegistDecalCircle(pos, 0.0f, 19.0f, 12)` — i.e. the
//! freshly dug hole's visible decal gets a matching temporary
//! collision circle that grows 0→19 over 12 frames. Initialized at
//! scene start (`m_play.c:435`), ticked per frame
//! (`m_play.c:539`).
//!
//! ## Original-game bugs (decomp-annotated, not reproduced)
//!
//! Without the decomp's BUGFIXES option:
//! * `mCoBG_RegistDecalCircle` clears `sizeof(whole array)`
//!   instead of one record — clobbering into
//!   `mCoBG_decal_circle`'s data — and doesn't stop after the
//!   first free slot, so all free slots get used.
//! * `mCoBG_InitDecalCircle` clears 3× too much memory.
//! The Rust port implements the intended (fixed) behavior; the
//! bugs are documented here, not reproduced.
//! `mCoBG_CrossOffDecalCircle` is marked @unused/@fabricated in
//! the decomp and is not ported.

use crate::columns::Column;
use crate::segment_map::WallName;

/// Maximum decal circles.
pub const DECAL_CIRCLE_MAX: usize = 3;

/// Wall-vector cap shared with the wall list.
pub const UNIT_VEC_INFO_MAX: usize = 128;

/// One circle-defence wall entry: two opposite-facing wall
/// definitions for a column-pair offset.
#[derive(Clone, Copy, Debug)]
pub struct DefenceWallInfo {
    pub dx: i32,
    pub dz: i32,
    pub normal_angle0_deg: f32,
    pub normal0: [f32; 2],
    pub wall_name0: WallName,
    pub normal_angle1_deg: f32,
    pub normal1: [f32; 2],
    pub wall_name1: WallName,
}

const R: f32 = 0.7071067811865476;

/// `defence_wall_info[8]` — verbatim.
pub const DEFENCE_WALL_INFO: [DefenceWallInfo; 8] = [
    DefenceWallInfo { dx: 1, dz: 0, normal_angle0_deg: 0.0, normal0: [0.0, 1.0], wall_name0: WallName::Up, normal_angle1_deg: 180.0, normal1: [0.0, -1.0], wall_name1: WallName::Up },
    DefenceWallInfo { dx: -1, dz: 0, normal_angle0_deg: 0.0, normal0: [0.0, 1.0], wall_name0: WallName::Up, normal_angle1_deg: 180.0, normal1: [0.0, -1.0], wall_name1: WallName::Up },
    DefenceWallInfo { dx: 0, dz: 1, normal_angle0_deg: 90.0, normal0: [1.0, 0.0], wall_name0: WallName::Right, normal_angle1_deg: -90.0, normal1: [-1.0, 0.0], wall_name1: WallName::Right },
    DefenceWallInfo { dx: 0, dz: -1, normal_angle0_deg: 90.0, normal0: [1.0, 0.0], wall_name0: WallName::Right, normal_angle1_deg: -90.0, normal1: [-1.0, 0.0], wall_name1: WallName::Right },
    DefenceWallInfo { dx: 1, dz: 1, normal_angle0_deg: 135.0, normal0: [R, -R], wall_name0: WallName::SlateDown, normal_angle1_deg: -45.0, normal1: [-R, R], wall_name1: WallName::SlateDown },
    DefenceWallInfo { dx: -1, dz: -1, normal_angle0_deg: 135.0, normal0: [R, -R], wall_name0: WallName::SlateDown, normal_angle1_deg: -45.0, normal1: [-R, R], wall_name1: WallName::SlateDown },
    DefenceWallInfo { dx: 1, dz: -1, normal_angle0_deg: -135.0, normal0: [-R, -R], wall_name0: WallName::SlateUp, normal_angle1_deg: 45.0, normal1: [R, R], wall_name1: WallName::SlateUp },
    DefenceWallInfo { dx: -1, dz: 1, normal_angle0_deg: -135.0, normal0: [-R, -R], wall_name0: WallName::SlateUp, normal_angle1_deg: 45.0, normal1: [R, R], wall_name1: WallName::SlateUp },
];

/// Table lookup (`mCoBG_CircleDefenceWallIdx`).
pub fn circle_defence_wall_idx(dx: i32, dz: i32) -> Option<&'static DefenceWallInfo> {
    DEFENCE_WALL_INFO.iter().find(|e| e.dx == dx && e.dz == dz)
}

/// A generated circle-defence wall vector.
#[derive(Clone, Copy, Debug)]
pub struct DefenceWall {
    pub start: [f32; 2],
    pub end: [f32; 2],
    pub normal: [f32; 2],
    pub normal_angle_deg: f32,
    pub wall_name: WallName,
    pub atr_wall: bool,
}

/// `mCoBG_MakeCircleDefenceWall`: for each ordered pair of distinct
/// columns whose (ux,uz) offset matches the table, emit the two
/// opposite-facing walls. `columns` holds (ux, uz, x, z) per column.
pub fn make_circle_defence_walls(
    columns: &[(i32, i32, f32, f32)],
    attr_wall: bool,
    old_on_ground: bool,
) -> Vec<DefenceWall> {
    let mut out = Vec::new();
    if !(attr_wall && old_on_ground) {
        return out;
    }
    for (i0, &(ux0, uz0, x0, z0)) in columns.iter().enumerate() {
        for (i1, &(ux1, uz1, x1, z1)) in columns.iter().enumerate() {
            if i0 == i1 {
                continue;
            }
            if let Some(info) = circle_defence_wall_idx(ux0 - ux1, uz0 - uz1) {
                if out.len() < UNIT_VEC_INFO_MAX {
                    out.push(DefenceWall {
                        start: [x0, z0],
                        end: [x1, z1],
                        normal: info.normal0,
                        normal_angle_deg: info.normal_angle0_deg,
                        wall_name: info.wall_name0,
                        atr_wall: true,
                    });
                }
                if out.len() < UNIT_VEC_INFO_MAX {
                    out.push(DefenceWall {
                        start: [x0, z0],
                        end: [x1, z1],
                        normal: info.normal1,
                        normal_angle_deg: info.normal_angle1_deg,
                        wall_name: info.wall_name1,
                        atr_wall: true,
                    });
                }
            }
        }
    }
    out
}

/// Decal-circle registration record
/// (`mCoBG_regist_circle_info_c`).
#[derive(Clone, Copy, Debug, Default)]
pub struct RegistCircleInfo {
    pub pos: [f32; 3],
    pub in_use: bool,
    pub start_radius: f32,
    pub end_radius: f32,
    pub now_radius: f32,
    pub start_timer: i16,
    pub now_timer: i16,
}

/// Linear timer interpolation (`mCoBG_CalcAdjust`).
pub fn calc_adjust(now_a: i16, start_a: i16, end_a: i16, start_val: f32, end_val: f32) -> f32 {
    if start_a == end_a {
        return start_val;
    }
    if now_a <= start_a {
        return start_val;
    }
    if now_a >= end_a {
        return end_val;
    }
    let d_a = (now_a - start_a) as f32;
    let n_a = (end_a - start_a) as f32;
    start_val + d_a * ((end_val - start_val) / n_a)
}

/// The decal-circle system: registration records plus the live
/// collision columns consumed by the second `ColumnWallCheck` pass.
#[derive(Clone, Debug, Default)]
pub struct DecalCircleSystem {
    pub regist: [RegistCircleInfo; DECAL_CIRCLE_MAX],
    pub circles: [Column; DECAL_CIRCLE_MAX],
    pub count: usize,
}

impl DecalCircleSystem {
    pub fn new() -> Self {
        Self::default()
    }

    /// Clear everything (`mCoBG_InitDecalCircle`, fixed behavior).
    pub fn init(&mut self) {
        *self = Self::default();
    }

    /// Advance one timer tick for a slot
    /// (`mCoBG_CalcTimerDecalCircleOne`).
    fn tick_one(&mut self, i: usize) {
        let reg = &mut self.regist[i];
        if !reg.in_use {
            return;
        }
        if reg.start_timer != -100 {
            let now_a = reg.start_timer - reg.now_timer;
            reg.now_radius = calc_adjust(now_a, 0, reg.start_timer, reg.start_radius, reg.end_radius);
            let col = &mut self.circles[i];
            col.pos = reg.pos;
            col.height = reg.pos[1];
            col.radius = reg.now_radius;
            col.atr_wall = true;
            col.ux = (reg.pos[0] / crate::segment_map::UNIT_SIZE) as i32;
            col.uz = (reg.pos[2] / crate::segment_map::UNIT_SIZE) as i32;
            reg.now_timer -= 1;
            if reg.now_timer < 0 {
                reg.in_use = false;
                self.count = self.count.saturating_sub(1);
            }
        }
    }

    /// Per-frame update (`mCoBG_CalcTimerDecalCircle`).
    pub fn calc_timer(&mut self) {
        if self.count > 0 {
            for i in 0..DECAL_CIRCLE_MAX {
                self.tick_one(i);
            }
        }
    }

    /// Register a decal circle (`mCoBG_RegistDecalCircle`, intended
    /// behavior: first free slot only). Returns the slot or -1.
    pub fn regist(&mut self, pos: [f32; 3], start_radius: f32, end_radius: f32, timer: i16) -> i32 {
        if self.count >= DECAL_CIRCLE_MAX {
            return -1;
        }
        for i in 0..DECAL_CIRCLE_MAX {
            if !self.regist[i].in_use {
                self.regist[i] = RegistCircleInfo {
                    pos,
                    in_use: true,
                    start_radius,
                    end_radius,
                    now_radius: start_radius,
                    start_timer: timer,
                    now_timer: timer,
                };
                self.circles[i] = Column::default();
                self.count += 1;
                self.tick_one(i);
                return i as i32;
            }
        }
        -1
    }

    /// Active live columns for the second `ColumnWallCheck` pass.
    pub fn active_circles(&self) -> Vec<&Column> {
        self.regist
            .iter()
            .zip(self.circles.iter())
            .filter(|(r, _)| r.in_use)
            .map(|(_, c)| c)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defence_table_lookup() {
        assert!(circle_defence_wall_idx(1, 0).is_some());
        assert!(circle_defence_wall_idx(1, 1).is_some());
        assert!(circle_defence_wall_idx(1, -1).is_some());
        assert!(circle_defence_wall_idx(2, 0).is_none());
        let e = circle_defence_wall_idx(0, 1).unwrap();
        assert_eq!((e.normal_angle0_deg, e.normal_angle1_deg), (90.0, -90.0));
        assert_eq!(e.wall_name0, WallName::Right);
    }

    #[test]
    fn defence_walls_bridge_adjacent_columns() {
        // Two x-adjacent columns -> ordered pairs (0,1) and (1,0)
        // each emit two walls = 4 walls.
        let cols = [(5, 5, 200.0, 200.0), (6, 5, 240.0, 200.0)];
        let walls = make_circle_defence_walls(&cols, true, true);
        assert_eq!(walls.len(), 4);
        assert!(walls.iter().all(|w| w.atr_wall));
        assert!(walls.iter().all(|w| w.wall_name == WallName::Up));
        let angles: Vec<f32> = walls.iter().map(|w| w.normal_angle_deg).collect();
        assert!(angles.contains(&0.0) && angles.contains(&180.0));
        // Gated off.
        assert!(make_circle_defence_walls(&cols, false, true).is_empty());
        assert!(make_circle_defence_walls(&cols, true, false).is_empty());
        // Non-adjacent columns -> nothing.
        let cols = [(0, 0, 0.0, 0.0), (5, 5, 200.0, 200.0)];
        assert!(make_circle_defence_walls(&cols, true, true).is_empty());
    }

    #[test]
    fn decal_registration_and_growth() {
        let mut sys = DecalCircleSystem::new();
        // The dig/scoop recipe: 0 -> 19 over 12 frames.
        let slot = sys.regist([100.0, 5.0, 100.0], 0.0, 19.0, 12);
        assert_eq!(slot, 0);
        assert_eq!(sys.count, 1);
        // First tick ran inside regist: radius interpolated at now_a=0.
        assert!((sys.circles[0].radius - 0.0).abs() < 1e-4);
        assert!(sys.circles[0].atr_wall);
        assert!((sys.circles[0].height - 5.0).abs() < 1e-4);
        // Advance 6 more frames: radius ~ half.
        for _ in 0..6 {
            sys.calc_timer();
        }
        let r = sys.circles[0].radius;
        assert!(r > 8.0 && r < 12.0, "r={r}");
        // Run out the timer: slot deactivates.
        for _ in 0..8 {
            sys.calc_timer();
        }
        assert_eq!(sys.count, 0);
        assert!(sys.active_circles().is_empty());
    }

    #[test]
    fn decal_slots_and_limits() {
        let mut sys = DecalCircleSystem::new();
        assert_eq!(sys.regist([0.0, 0.0, 0.0], 0.0, 5.0, 3), 0);
        assert_eq!(sys.regist([40.0, 0.0, 0.0], 0.0, 5.0, 3), 1);
        assert_eq!(sys.regist([80.0, 0.0, 0.0], 0.0, 5.0, 3), 2);
        assert_eq!(sys.regist([120.0, 0.0, 0.0], 0.0, 5.0, 3), -1);
        sys.init();
        assert_eq!(sys.count, 0);
    }

    #[test]
    fn calc_adjust_edges() {
        assert_eq!(calc_adjust(0, 0, 12, 0.0, 19.0), 0.0);
        assert_eq!(calc_adjust(12, 0, 12, 0.0, 19.0), 19.0);
        assert_eq!(calc_adjust(5, 5, 5, 3.0, 9.0), 3.0);
    }
}
