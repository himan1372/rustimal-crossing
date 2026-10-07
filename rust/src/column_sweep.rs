//! Swept movement vs column collision (`m_collision_bg_column.c_inc`).
//!
//! Verified against the USA Rev. 0 decomp.
//!
//! Two complementary sweep primitives:
//!
//! - `line_wall_check_column`: the XZ circle-line intersection finds the
//!   collision time `t`; Y is interpolated at `t`; accepted iff the
//!   interpolated Y is at or below the column top.
//! - `line_ground_check_column`: the Y-plane crossing finds `t`; X/Z are
//!   interpolated at `t`.
//!
//! Source-structure note: both functions compute `tmp_end = end + reverse`
//! per column, but `reverse` is reset to zero at the top of every iteration
//! and the functions return on the first accept — so `tmp_end` is always
//! exactly `end_pos` and the accumulation is dead code. The kernels below
//! implement the observable behavior directly and document this.

use crate::endpoint_circle::{cross_circle_and_line_2dvector, judge_point_in_circle};

/// `F32_IS_ZERO` (types.h) verbatim: NOT exact zero — `|v| < 0.008`.
pub fn f32_is_zero(v: f32) -> bool {
    v.abs() < 0.008
}

/// Column record: the fields of `mCoBG_column_c` used by the sweeps.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Column {
    pub pos: [f32; 3],
    pub height: f32,
    pub radius: f32,
}

/// Object classes that produce columns (`mCoBG_MakeOneColumnCollisionData`).
/// Item-ID classification lives in the `IS_ITEM_*` macros
/// (m_name_table.h); this table carries the portable geometry.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ColumnKind {
    /// Dug/shining hole: flat (height = terrain Y), `atr_wall = TRUE`,
    /// only when the actor was on the ground.
    Hole,
    SmallTree,
    MediumTree,
    LargeTree,
    /// Full-grown tree (incl. RSV_TREE).
    FullTree,
    /// Stump: radius 10 for the *001 variants, 18 otherwise.
    StumpNarrow,
    StumpWide,
    Rock,
    Mailbox,
    Sign,
    /// Reserve signboard: narrower than a normal sign.
    SignboardReserve,
    /// Koinobori / flag: very tall.
    Flag,
}

/// (radius, height above the terrain-center Y, atr_wall), verbatim from
/// `mCoBG_MakeOneColumnCollisionData`.
pub fn column_spec(kind: ColumnKind) -> (f32, f32, bool) {
    match kind {
        ColumnKind::Hole => (19.0, 0.0, true),
        ColumnKind::SmallTree => (19.0, 30.0, false),
        ColumnKind::MediumTree => (19.0, 40.0, false),
        ColumnKind::LargeTree => (19.0, 60.0, false),
        ColumnKind::FullTree => (19.0, 80.0, false),
        ColumnKind::StumpNarrow => (10.0, 30.0, false),
        ColumnKind::StumpWide => (18.0, 30.0, false),
        ColumnKind::Rock => (19.0, 31.5, false),
        ColumnKind::Mailbox => (15.0, 50.0, false),
        ColumnKind::Sign => (19.0, 45.0, false),
        ColumnKind::SignboardReserve => (10.0, 45.0, false),
        ColumnKind::Flag => (19.0, 160.0, false),
    }
}

/// Column top height: `terrain_center_y + spec height`
/// (`col->height = col->pos.y + <object height>`; holes: `height = pos.y`).
/// `pos.y` itself is the terrain-center Y at the column's own unit
/// (`mCoBG_GetBgY_OnlyCenter_FromWpos2`).
pub fn column_top_height(terrain_center_y: f32, kind: ColumnKind) -> f32 {
    terrain_center_y + column_spec(kind).1
}

/// Maximum columns per check (`mCoBG_MakeColumnCollisionData` cap).
pub const COLUMN_MAX: usize = 16;

/// `mCoBG_GetBGHeight_Column` as a pure kernel
/// (m_collision_bg_column.c_inc:444): the column comes from the unit's
/// foreground object (`column_spec`); if the query XZ is inside the
/// column's circular footprint (`mCoBG_JudgePointInCircle_Xyz`: XZ only,
/// `dx²+dz² <= r²`), the column top is the candidate ground, else 0.0 —
/// the "no column" sentinel. `make_column_ok` models
/// `mCoBG_MakeOneColumnCollisionData` succeeding; it is FALSE here for
/// holes because this path passes `old_on_ground = FALSE`.
/// The query Y is irrelevant.
pub fn get_bg_height_column(
    pos_x: f32,
    pos_z: f32,
    col_cx: f32,
    col_cz: f32,
    col_radius: f32,
    col_height: f32,
    make_column_ok: bool,
) -> f32 {
    if make_column_ok {
        let dx = pos_x - col_cx;
        let dz = pos_z - col_cz;
        if dx * dx + dz * dz <= col_radius * col_radius {
            return col_height;
        }
    }
    0.0
}

/// `mCoBG_GetBGHeight_NormalColumn` (m_collision_bg.c:1689):
/// ground = max(normal terrain, column). Ties go to the COLUMN
/// (the comparison is strict `normal > column`), and when the column
/// wins the ground angle is the zero-initialized `ground_angle0`
/// (flat cap, no slope). `directed_excluded` is the
/// `ut == (l_ActorInf._68, _6C)` case, forcing the column to 0.0.
/// Returns (ground_y, column_won).
pub fn get_bg_height_normal_column(
    normal_ground_y: f32,
    column_ground_y: f32,
    directed_excluded: bool,
) -> (f32, bool) {
    let cy = if directed_excluded { 0.0 } else { column_ground_y };
    if normal_ground_y > cy {
        (normal_ground_y, false)
    } else {
        (cy, true)
    }
}

/// `mCoBG_AdjustActorY` solid-ground branch (m_collision_bg.c:403):
/// when `ground_y >= actor_foot_y`, the feet are placed exactly on the
/// ground: returns the Y revision `(ground_y - ground_dist) - actor_y`
/// (with `on_ground = TRUE`, vertical speed zeroed by the caller).
/// Column-derived ground flows through this same branch — there is no
/// separate "on object" state.
pub fn adjust_actor_y_ground(ground_y: f32, actor_y: f32, ground_dist: f32) -> Option<f32> {
    let foot_y = actor_y + ground_dist;
    if ground_y >= foot_y {
        Some((ground_y - ground_dist) - actor_y)
    } else {
        None
    }
}

/// Single-column kernel of `mCoBG_LineWallCheck_Column` as a pure function.
/// `start`/`end` are the movement segment endpoints. Returns the rewind
/// vector on an accepted collision.
///
/// Verbatim details:
/// - `vec_end_start = start - end`; XZ length gate `!F32_IS_ZERO`.
/// - Start inside the column XZ circle -> no sweep collision.
/// - Both circle intersections computed; the nearer (by squared XZ
///   distance from start) is tested against the per-axis segment bounds.
/// - `mult = (len_xz - sqrt(d_sq)) / len_xz`; rewind scales the FULL XYZ
///   `vec_end_start` (Y included) — the trajectory is truncated, preserving
///   its direction.
/// - Height gate on the interpolated Y: `end.y + rev.y <= col.height`.
/// - No actor radius is added anywhere (unlike `ColumnCheck_NormalWall`).
pub fn line_wall_check_column_one(
    start: [f32; 3],
    end: [f32; 3],
    col_pos: [f32; 3],
    col_radius: f32,
    col_height: f32,
) -> Option<[f32; 3]> {
    let dx = start[0] - end[0];
    let dy = start[1] - end[1];
    let dz = start[2] - end[2];
    let len_xz = (dx * dx + dz * dz).sqrt();
    if f32_is_zero(len_xz) {
        return None;
    }
    if judge_point_in_circle([col_pos[0], col_pos[2]], [start[0], start[2]], col_radius) {
        return None;
    }
    let (cross0, cross1) = cross_circle_and_line_2dvector(
        [start[0], start[2]],
        [dx, dz],
        [col_pos[0], col_pos[2]],
        col_radius,
    )?;
    let d0 = (cross0[0] - start[0]).powi(2) + (cross0[1] - start[2]).powi(2);
    let d1 = (cross1[0] - start[0]).powi(2) + (cross1[1] - start[2]).powi(2);
    // Nearer intersection wins; strict `<` picks cross1 on a tie, verbatim.
    let (cx, cz, d_sq) = if d0 < d1 {
        (cross0[0], cross0[1], d0)
    } else {
        (cross1[0], cross1[1], d1)
    };
    let in_x = (cx >= start[0] && cx <= end[0]) || (cx >= end[0] && cx <= start[0]);
    let in_z = (cz >= start[2] && cz <= end[2]) || (cz >= end[2] && cz <= start[2]);
    if !(in_x && in_z) {
        return None;
    }
    let mult = (len_xz - d_sq.sqrt()) / len_xz;
    let rev = [dx * mult, dy * mult, dz * mult];
    if end[1] + rev[1] <= col_height {
        Some(rev)
    } else {
        None
    }
}

/// Multi-column driver: first accepted column wins (source returns on the
/// first accept; column order = array order).
pub fn line_wall_check_column(start: [f32; 3], end: [f32; 3], cols: &[Column]) -> Option<[f32; 3]> {
    for col in cols {
        if let Some(rev) = line_wall_check_column_one(start, end, col.pos, col.radius, col.height) {
            return Some(rev);
        }
    }
    None
}

/// Single-column kernel of `mCoBG_LineGroundCheck_Column`: accepted when
/// the movement crosses the column-top plane downward
/// (`start.y > height && end.y < height`); rewinds the trajectory to the
/// Y = height crossing. Returns FALSE (None) immediately — without trying
/// later columns — when the Y gate passes but `start.y - end.y` is
/// ~zero (verbatim). The static `reverse0` the source worries about is
/// zero-initialized, so deterministic.
pub fn line_ground_check_column_one(
    start: [f32; 3],
    end: [f32; 3],
    col_height: f32,
) -> Option<[f32; 3]> {
    if !(start[1] > col_height && end[1] < col_height) {
        return None;
    }
    let dx = start[0] - end[0];
    let dy = start[1] - end[1];
    let dz = start[2] - end[2];
    if f32_is_zero(dy) {
        return None;
    }
    let mult = (col_height - end[1]) / dy;
    Some([dx * mult, dy * mult, dz * mult])
}

/// Multi-column driver for the ground sweep. NOTE the verbatim early-out:
/// if the Y gate passes for a column but its rewind is degenerate, the
/// source returns FALSE immediately instead of continuing.
pub fn line_ground_check_column(
    start: [f32; 3],
    end: [f32; 3],
    cols: &[Column],
) -> Option<[f32; 3]> {
    for col in cols {
        if start[1] > col.height && end[1] < col.height {
            return line_ground_check_column_one(start, end, col.height);
        }
    }
    None
}

// ---- C ABI ----

/// C ABI: ground selection; returns the winning ground Y.
/// Writes 1 to out_column_won when the column wins (flat cap).
#[no_mangle]
pub extern "C" fn pc_column_ground_select(
    normal_y: f32,
    column_y: f32,
    directed_excluded: u8,
    out_column_won: *mut u8,
) -> f32 {
    let (y, won) = get_bg_height_normal_column(normal_y, column_y, directed_excluded != 0);
    if !out_column_won.is_null() {
        unsafe {
            *out_column_won = won as u8;
        }
    }
    y
}

/// C ABI: single-column wall sweep; writes rev[3]; returns 1 on accept.
#[no_mangle]
pub extern "C" fn pc_line_wall_check_column_one(
    sx: f32,
    sy: f32,
    sz: f32,
    ex: f32,
    ey: f32,
    ez: f32,
    cx: f32,
    cz: f32,
    radius: f32,
    height: f32,
    out_rev: *mut f32,
) -> u8 {
    match line_wall_check_column_one(
        [sx, sy, sz],
        [ex, ey, ez],
        [cx, 0.0, cz],
        radius,
        height,
    ) {
        Some(rev) => {
            if !out_rev.is_null() {
                unsafe {
                    *out_rev.add(0) = rev[0];
                    *out_rev.add(1) = rev[1];
                    *out_rev.add(2) = rev[2];
                }
            }
            1
        }
        None => 0,
    }
}

/// C ABI: single-column ground sweep; writes rev[3]; returns 1 on accept.
#[no_mangle]
pub extern "C" fn pc_line_ground_check_column_one(
    sx: f32,
    sy: f32,
    sz: f32,
    ex: f32,
    ey: f32,
    ez: f32,
    height: f32,
    out_rev: *mut f32,
) -> u8 {
    match line_ground_check_column_one([sx, sy, sz], [ex, ey, ez], height) {
        Some(rev) => {
            if !out_rev.is_null() {
                unsafe {
                    *out_rev.add(0) = rev[0];
                    *out_rev.add(1) = rev[1];
                    *out_rev.add(2) = rev[2];
                }
            }
            1
        }
        None => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn f32_zero_tolerance() {
        assert!(f32_is_zero(0.0079));
        assert!(!f32_is_zero(0.008));
        assert!(f32_is_zero(0.0));
    }

    #[test]
    fn wall_sweep_basic() {
        // Move +X from (0,0,0) to (10,0,0); column at (5,0) radius 1.
        // XZ intersections at x=4 and x=6; nearer is x=4, d=4, L=10,
        // mult=(10-4)/10=0.6, rev = (0-10,0,0)*0.6 = (-6,0,0).
        // end + rev = (4,0,0): exactly the first intersection.
        let rev = line_wall_check_column_one(
            [0.0, 0.0, 0.0],
            [10.0, 0.0, 0.0],
            [5.0, 0.0, 0.0],
            1.0,
            10.0,
        )
        .unwrap();
        assert!((rev[0] - -6.0).abs() < 1e-4);
        assert!(rev[1].abs() < 1e-4 && rev[2].abs() < 1e-4);
    }

    #[test]
    fn wall_sweep_rewinds_y_too() {
        // Diagonal upward movement: Y is truncated proportionally.
        let rev = line_wall_check_column_one(
            [0.0, 0.0, 0.0],
            [10.0, 10.0, 0.0],
            [5.0, 0.0, 0.0],
            1.0,
            10.0,
        )
        .unwrap();
        assert!((rev[0] - -6.0).abs() < 1e-4);
        assert!((rev[1] - -6.0).abs() < 1e-4);
        // Interpolated Y at impact: 10 + (-6) = 4 <= 10 -> accepted.
    }

    #[test]
    fn wall_sweep_height_gate() {
        // Same geometry, but the interpolated Y (4) is above the column.
        assert!(line_wall_check_column_one(
            [0.0, 0.0, 0.0],
            [10.0, 10.0, 0.0],
            [5.0, 0.0, 0.0],
            1.0,
            3.0,
        )
        .is_none());
    }

    #[test]
    fn wall_sweep_edge_cases() {
        // Start inside the column -> no sweep.
        assert!(line_wall_check_column_one(
            [5.0, 0.0, 0.0],
            [10.0, 0.0, 0.0],
            [5.0, 0.0, 0.0],
            1.0,
            10.0
        )
        .is_none());
        // Zero XZ movement -> None.
        assert!(line_wall_check_column_one(
            [0.0, 0.0, 0.0],
            [0.0, 5.0, 0.0],
            [5.0, 0.0, 0.0],
            1.0,
            10.0
        )
        .is_none());
        // Column beside the path (no intersection) -> None.
        assert!(line_wall_check_column_one(
            [0.0, 0.0, 0.0],
            [10.0, 0.0, 0.0],
            [5.0, 0.0, 5.0],
            1.0,
            10.0
        )
        .is_none());
        // Intersection beyond the segment -> None.
        assert!(line_wall_check_column_one(
            [0.0, 0.0, 0.0],
            [2.0, 0.0, 0.0],
            [5.0, 0.0, 0.0],
            1.0,
            10.0
        )
        .is_none());
    }

    #[test]
    fn ground_sweep_basic() {
        // Falling from y=10 to y=0 through a column top at y=4:
        // mult = (4-0)/(10-0) = 0.4; rev = (0-0, 10-0, 0)*0.4 = (0,4,0).
        // end + rev = (x, 4, z): exactly the plane crossing.
        let rev = line_ground_check_column_one([3.0, 10.0, 2.0], [7.0, 0.0, 6.0], 4.0).unwrap();
        assert!((rev[0] - -1.6).abs() < 1e-4);
        assert!((rev[1] - 4.0).abs() < 1e-4);
        assert!((rev[2] - -1.6).abs() < 1e-4);
        // No downward crossing -> None.
        assert!(line_ground_check_column_one([0.0, 2.0, 0.0], [0.0, 0.0, 0.0], 4.0).is_none());
        assert!(line_ground_check_column_one([0.0, 10.0, 0.0], [0.0, 8.0, 0.0], 4.0).is_none());
        // Upward crossing -> None (gate requires start above, end below).
        assert!(line_ground_check_column_one([0.0, 0.0, 0.0], [0.0, 10.0, 0.0], 4.0).is_none());
    }

    #[test]
    fn column_data_table() {
        // Verbatim specs from mCoBG_MakeOneColumnCollisionData.
        assert_eq!(column_spec(ColumnKind::SmallTree), (19.0, 30.0, false));
        assert_eq!(column_spec(ColumnKind::FullTree), (19.0, 80.0, false));
        assert_eq!(column_spec(ColumnKind::StumpNarrow), (10.0, 30.0, false));
        assert_eq!(column_spec(ColumnKind::StumpWide), (18.0, 30.0, false));
        assert_eq!(column_spec(ColumnKind::Rock), (19.0, 31.5, false));
        assert_eq!(column_spec(ColumnKind::Mailbox), (15.0, 50.0, false));
        assert_eq!(column_spec(ColumnKind::SignboardReserve), (10.0, 45.0, false));
        assert_eq!(column_spec(ColumnKind::Flag), (19.0, 160.0, false));
        // Holes are flat and attribute-walled.
        assert_eq!(column_spec(ColumnKind::Hole), (19.0, 0.0, true));
        // Top height = terrain-center Y + object height.
        assert_eq!(column_top_height(100.0, ColumnKind::LargeTree), 160.0);
        assert_eq!(column_top_height(100.0, ColumnKind::Hole), 100.0);
        assert_eq!(COLUMN_MAX, 16);
    }

    #[test]
    fn column_ground_height() {
        // Inside the footprint -> column top; Y of the query is irrelevant.
        assert_eq!(
            get_bg_height_column(5.0, 0.0, 5.0, 0.0, 19.0, 130.0, true),
            130.0
        );
        // On the boundary (<=) -> inside.
        assert_eq!(
            get_bg_height_column(24.0, 0.0, 5.0, 0.0, 19.0, 130.0, true),
            130.0
        );
        // Outside -> 0.0 sentinel.
        assert_eq!(
            get_bg_height_column(25.0, 0.0, 5.0, 0.0, 19.0, 130.0, true),
            0.0
        );
        // Column construction failed (e.g. hole via this path) -> 0.0.
        assert_eq!(
            get_bg_height_column(5.0, 0.0, 5.0, 0.0, 19.0, 130.0, false),
            0.0
        );
        // Selection: column only ever raises the ground.
        assert_eq!(get_bg_height_normal_column(100.0, 130.0, false), (130.0, true));
        assert_eq!(get_bg_height_normal_column(100.0, 80.0, false), (100.0, false));
        // Tie goes to the column (strict > for normal).
        assert_eq!(get_bg_height_normal_column(100.0, 100.0, false), (100.0, true));
        // Directed-unit exclusion forces the column to 0.0.
        assert_eq!(get_bg_height_normal_column(100.0, 130.0, true), (100.0, false));
        assert_eq!(get_bg_height_normal_column(0.0, 0.0, true), (0.0, true));
        // AdjustActorY ground branch: feet placed exactly on the ground.
        assert_eq!(adjust_actor_y_ground(130.0, 100.0, 5.0), Some(25.0));
        assert_eq!(adjust_actor_y_ground(100.0, 100.0, 5.0), None);
        // C ABI.
        let mut won = 0u8;
        let y = pc_column_ground_select(100.0, 130.0, 0, &mut won);
        assert_eq!((y, won), (130.0, 1));
    }

    #[test]
    fn multi_column_first_accept_wins() {
        let cols = [
            Column { pos: [50.0, 0.0, 0.0], height: 10.0, radius: 1.0 }, // no hit
            Column { pos: [5.0, 0.0, 0.0], height: 10.0, radius: 1.0 },  // hit
        ];
        let rev = line_wall_check_column([0.0, 0.0, 0.0], [10.0, 0.0, 0.0], &cols).unwrap();
        assert!((rev[0] - -6.0).abs() < 1e-4);
    }
}
