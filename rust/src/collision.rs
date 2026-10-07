//! Collision for the Rust rewrite.
//!
//! Source-verified (upstream `include/m_collision_bg.h`,
//! `src/game/m_collision_bg.c`, `include/m_collision_obj.h`):
//!
//! There are two separate systems, not one physics engine:
//!
//! **Background collision** (`m_collision_bg.c`): terrain, ground height,
//! slopes, cliffs/walls, water, moving background objects. Each map unit
//! carries a 4-byte record — 1 slate bit, five 5-bit height samples
//! (center + four corners), and a 6-bit terrain attribute. The engine
//! reconstructs local triangles/wall segments around the actor at
//! runtime; there is no global collision mesh.
//!
//! **Object collision** (`m_collision_obj.c`): actor↔actor/actor↔object
//! overlap using joint spheres, pipes, and triangle colliders, with
//! collision groups, mass categories, and positional separation.
//!
//! Key verified facts:
//! * `mCoBG_CollisionData_c` bit layout (slate:1, center:5, top_left:5,
//!   bot_left:5, bot_right:5, top_right:5, attribute:6).
//! * Units subdivide into four triangle areas (N/W/S/E); ground height
//!   comes from the plane equation, not a lookup.
//! * Walls are built from neighboring-unit edge height differences, as
//!   2D segments with top/bottom heights, normals, and angles.
//! * Wall kinds: normal, attribute, move (moving background).
//! * Slate walls: diagonal normals at ±45°-ish from diagonal height
//!   comparison (`mCoBG_WALL_SLATE_UP/DOWN`).
//! * Local query: 3×3/5×5/7×7 unit neighborhood by range (≤40 → 3,
//!   ≤80 → 5, else 7); at most 128 wall vectors
//!   (`mCoBG_UNIT_VEC_INFO_MAX`); at most 2 wall contacts
//!   (`mCoBG_WALL_COL_NUM`).
//! * Swept test: previous→current position segment vs geometry;
//!   correction is `rev_pos` along the wall normal
//!   (`rev_dist = range + dist + epsilon`).
//! * Result flags: on_ground, hit_wall_count, directional wall flags
//!   (front/right/left/back), is_in_water, is_on_move_bg_obj.
//! * Moving background objects: 64 registrations max; actors standing
//!   on one are carried by its position delta.
//! * Object colliders: joint sphere / pipe / triangle; 50-collider
//!   table; groups PLAYER/GROUP_2/GROUP_3; masses
//!   immovable/heavy/normal; overlap produces `collision_vec`
//!   separation split by mass.
//!
//! Rewrite-owned: geometry math is `f32` here; fixed-point game angles
//! are modeled as degrees.

/// The 4-byte per-unit collision record. Bit layout mirrors
/// `mCoBG_CollisionData_c` exactly.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CollisionData {
    pub raw: u32,
}

impl CollisionData {
    pub fn slate_flag(self) -> bool {
        self.raw & 1 != 0
    }
    pub fn center(self) -> u8 {
        ((self.raw >> 1) & 0x1F) as u8
    }
    pub fn top_left(self) -> u8 {
        ((self.raw >> 6) & 0x1F) as u8
    }
    pub fn bot_left(self) -> u8 {
        ((self.raw >> 11) & 0x1F) as u8
    }
    pub fn bot_right(self) -> u8 {
        ((self.raw >> 16) & 0x1F) as u8
    }
    pub fn top_right(self) -> u8 {
        ((self.raw >> 21) & 0x1F) as u8
    }
    pub fn attribute(self) -> u8 {
        ((self.raw >> 26) & 0x3F) as u8
    }

    pub fn pack(
        slate: bool,
        center: u8,
        top_left: u8,
        bot_left: u8,
        bot_right: u8,
        top_right: u8,
        attribute: u8,
    ) -> Self {
        let raw = (slate as u32)
            | ((center as u32 & 0x1F) << 1)
            | ((top_left as u32 & 0x1F) << 6)
            | ((bot_left as u32 & 0x1F) << 11)
            | ((bot_right as u32 & 0x1F) << 16)
            | ((top_right as u32 & 0x1F) << 21)
            | ((attribute as u32 & 0x3F) << 26);
        Self { raw }
    }

    /// Flat-unit test: all five height samples equal.
    pub fn is_flat(self) -> bool {
        let c = self.center();
        c == self.top_left() && c == self.bot_left() && c == self.bot_right() && c == self.top_right()
    }
}

/// Unit triangle areas, from `mCoBG_GetUnitArea`.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UnitArea {
    North = 0,
    West = 1,
    South = 2,
    East = 3,
}

/// Wall kinds (`mCoBG_WALL_KIND_*`).
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WallKind {
    Normal = 0,
    Attribute = 1,
    Move = 2,
}

/// Slate (diagonal wall) direction.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SlateDir {
    Up = 0,
    Down = 1,
}

/// Determine slate direction from diagonal height comparison.
pub fn slate_dir(data: CollisionData) -> Option<SlateDir> {
    if data.bot_right() != data.top_left() {
        Some(SlateDir::Up)
    } else if data.top_right() != data.bot_left() {
        Some(SlateDir::Down)
    } else {
        None
    }
}

/// A runtime wall segment: 2D line plus vertical bounds, normal, angle.
#[derive(Clone, Copy, Debug)]
pub struct WallSeg {
    pub start_x: f32,
    pub start_z: f32,
    pub end_x: f32,
    pub end_z: f32,
    pub start_top: f32,
    pub start_bottom: f32,
    pub end_top: f32,
    pub end_bottom: f32,
    pub normal_x: f32,
    pub normal_z: f32,
    pub normal_angle_deg: f32,
    pub kind: WallKind,
}

impl WallSeg {
    /// Distance from a point to the wall line (2D), signed by side.
    pub fn signed_distance(&self, x: f32, z: f32) -> f32 {
        (x - self.start_x) * self.normal_x + (z - self.start_z) * self.normal_z
    }

    /// Correction push for an actor of `range` penetrating to `dist`:
    /// `rev = normal * (range + dist + epsilon)`.
    pub fn correction(&self, dist: f32, range: f32) -> (f32, f32) {
        let rev_dist = range + dist + 0.001;
        (self.normal_x * rev_dist, self.normal_z * rev_dist)
    }
}

/// Directional wall-hit flags (`mCoBG_HIT_WALL*`).
pub mod hit {
    pub const WALL: u32 = 1 << 0;
    pub const WALL_FRONT: u32 = 1 << 1;
    pub const WALL_RIGHT: u32 = 1 << 2;
    pub const WALL_LEFT: u32 = 1 << 3;
    pub const WALL_BACK: u32 = 1 << 4;
}

/// Background collision result for one actor.
#[derive(Clone, Debug, Default)]
pub struct BgResult {
    pub on_ground: bool,
    pub hit_wall_count: u8,
    pub hit_flags: u32,
    pub is_in_water: bool,
    pub is_on_move_bg_obj: bool,
    pub rev_x: f32,
    pub rev_y: f32,
    pub rev_z: f32,
    pub ground_y: f32,
    pub water_y: f32,
}

/// Maximum wall contacts stored (`mCoBG_WALL_COL_NUM`).
pub const WALL_COL_NUM: usize = 2;
/// Maximum generated wall vectors per query
/// (`mCoBG_UNIT_VEC_INFO_MAX`).
pub const UNIT_VEC_INFO_MAX: usize = 128;
/// Maximum moving-background registrations
/// (`mCoBG_MOVE_REGIST_MAX`).
pub const MOVE_REGIST_MAX: usize = 64;

/// Neighborhood size from actor range: ≤40 → 3, ≤80 → 5, else 7.
pub fn neighborhood_size(range: f32) -> usize {
    if range <= 40.0 {
        3
    } else if range <= 80.0 {
        5
    } else {
        7
    }
}

/// Ground height from the terrain plane:
/// `height = (-(n.y * center_h) + (n.x * x + n.z * z)) / -n.y`.
pub fn ground_height_from_plane(
    normal: (f32, f32, f32),
    center_height: f32,
    x: f32,
    z: f32,
) -> Option<f32> {
    let (nx, ny, nz) = normal;
    if ny.abs() < 1e-6 {
        return None;
    }
    Some((-(ny * center_height) + (nx * x + nz * z)) / -ny)
}

/// Vertical correction to rest feet on the ground:
/// `rev_y = (ground_y - ground_dist) - pos_y`.
pub fn ground_correction(ground_y: f32, ground_dist: f32, pos_y: f32) -> f32 {
    (ground_y - ground_dist) - pos_y
}

/// Object collider types (`ClObj_TYPE_*`).
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ColliderType {
    JointSphere = 0,
    Pipe = 1,
    Triangle = 2,
}

/// Collision groups (`ClObj_GROUP_*`).
pub mod group {
    pub const PLAYER: u32 = 1 << 3;
    pub const GROUP_2: u32 = 1 << 4;
    pub const GROUP_3: u32 = 1 << 5;
}

/// Mass categories for overlap separation.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mass {
    Immovable = 0,
    Heavy = 1,
    Normal = 2,
}

/// Maximum registered object colliders (`Cl_COLLIDER_NUM`).
pub const COLLIDER_NUM: usize = 50;

/// Split overlap displacement between two actors by mass: immovable
/// takes none, heavy/heavy splits 50/50, otherwise the lighter actor
/// takes proportionally more. Returns (share_a, share_b).
pub fn mass_split(a: Mass, b: Mass) -> (f32, f32) {
    match (a, b) {
        (Mass::Immovable, _) => (0.0, 1.0),
        (_, Mass::Immovable) => (1.0, 0.0),
        (Mass::Heavy, Mass::Heavy) => (0.5, 0.5),
        (Mass::Heavy, Mass::Normal) => (0.25, 0.75),
        (Mass::Normal, Mass::Heavy) => (0.75, 0.25),
        (Mass::Normal, Mass::Normal) => (0.5, 0.5),
    }
}

/// Sphere-sphere overlap test returning penetration depth.
pub fn sphere_overlap(
    ax: f32,
    ay: f32,
    az: f32,
    ar: f32,
    bx: f32,
    by: f32,
    bz: f32,
    br: f32,
) -> Option<f32> {
    let dx = ax - bx;
    let dy = ay - by;
    let dz = az - bz;
    let dist = (dx * dx + dy * dy + dz * dz).sqrt();
    let depth = (ar + br) - dist;
    if depth > 0.0 {
        Some(depth)
    } else {
        None
    }
}

/// C ABI: neighborhood size for an actor range.
#[no_mangle]
pub extern "C" fn pc_collision_neighborhood(range: f32) -> u32 {
    neighborhood_size(range) as u32
}

/// C ABI: pack a unit collision record.
#[no_mangle]
pub extern "C" fn pc_collision_pack(
    slate: u8,
    center: u8,
    top_left: u8,
    bot_left: u8,
    bot_right: u8,
    top_right: u8,
    attribute: u8,
) -> u32 {
    CollisionData::pack(
        slate != 0,
        center,
        top_left,
        bot_left,
        bot_right,
        top_right,
        attribute,
    )
    .raw
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collision_record_bit_layout() {
        let d = CollisionData::pack(true, 17, 3, 29, 8, 31, 42);
        assert!(d.slate_flag());
        assert_eq!(d.center(), 17);
        assert_eq!(d.top_left(), 3);
        assert_eq!(d.bot_left(), 29);
        assert_eq!(d.bot_right(), 8);
        assert_eq!(d.top_right(), 31);
        assert_eq!(d.attribute(), 42);
        // 1 + 5*5 + 6 = 32 bits total.
        assert_eq!(d.raw >> 26, 42);
    }

    #[test]
    fn flat_detection() {
        let flat = CollisionData::pack(false, 10, 10, 10, 10, 10, 0);
        assert!(flat.is_flat());
        let sloped = CollisionData::pack(false, 10, 10, 12, 10, 10, 0);
        assert!(!sloped.is_flat());
    }

    #[test]
    fn slate_direction() {
        // bot_right != top_left -> UP.
        let up = CollisionData::pack(false, 5, 5, 5, 9, 5, 0);
        assert_eq!(slate_dir(up), Some(SlateDir::Up));
        // diagonals equal on one axis, differ on the other -> DOWN.
        let down = CollisionData::pack(false, 5, 5, 5, 5, 9, 0);
        assert_eq!(slate_dir(down), Some(SlateDir::Down));
        let none = CollisionData::pack(false, 5, 5, 5, 5, 5, 0);
        assert_eq!(slate_dir(none), None);
    }

    #[test]
    fn wall_correction_pushes_along_normal() {
        let wall = WallSeg {
            start_x: 0.0, start_z: 0.0, end_x: 10.0, end_z: 0.0,
            start_top: 50.0, start_bottom: 0.0, end_top: 50.0, end_bottom: 0.0,
            normal_x: 0.0, normal_z: 1.0, normal_angle_deg: 0.0,
            kind: WallKind::Normal,
        };
        assert!((wall.signed_distance(3.0, 4.0) - 4.0).abs() < 1e-5);
        let (rx, rz) = wall.correction(2.0, 5.0);
        assert!(rx.abs() < 1e-5);
        assert!((rz - 7.001).abs() < 1e-3);
    }

    #[test]
    fn neighborhood_sizes() {
        assert_eq!(neighborhood_size(40.0), 3);
        assert_eq!(neighborhood_size(80.0), 5);
        assert_eq!(neighborhood_size(81.0), 7);
        assert_eq!(WALL_COL_NUM, 2);
        assert_eq!(UNIT_VEC_INFO_MAX, 128);
        assert_eq!(MOVE_REGIST_MAX, 64);
        assert_eq!(COLLIDER_NUM, 50);
    }

    #[test]
    fn ground_height_plane_math() {
        // Flat ground: normal straight up, height = center height.
        let h = ground_height_from_plane((0.0, 1.0, 0.0), 20.0, 5.0, 7.0).unwrap();
        assert!((h - 20.0).abs() < 1e-4);
        // Vertical normal: no solution.
        assert!(ground_height_from_plane((1.0, 0.0, 0.0), 20.0, 5.0, 7.0).is_none());
    }

    #[test]
    fn mass_split_rules() {
        assert_eq!(mass_split(Mass::Immovable, Mass::Normal), (0.0, 1.0));
        assert_eq!(mass_split(Mass::Normal, Mass::Immovable), (1.0, 0.0));
        assert_eq!(mass_split(Mass::Heavy, Mass::Heavy), (0.5, 0.5));
        assert_eq!(mass_split(Mass::Heavy, Mass::Normal), (0.25, 0.75));
    }

    #[test]
    fn sphere_overlap_depth() {
        assert_eq!(sphere_overlap(0.0, 0.0, 0.0, 2.0, 3.0, 0.0, 0.0, 2.0), Some(1.0));
        assert_eq!(sphere_overlap(0.0, 0.0, 0.0, 1.0, 5.0, 0.0, 0.0, 1.0), None);
    }
}
