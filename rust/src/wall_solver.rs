//! Inner wall solver's geometric core for the Rust rewrite.
//!
//! Source-verified (upstream `src/game/m_collision_bg.c`):
//!
//! The wall solver is a staged 2D (X/Z) geometric solver with Y as a
//! separate wall-height validity test — not one generic push-out.
//!
//! * Wall record (`m_collision_bg.c:27`): start/end XZ, vertical
//!   bounds (start_top/btm, end_top/btm), normal, normal_angle,
//!   wall_name, regist_p (moving), atr_wall.
//! * Wall kinds: `regist_p != NULL` → MOVE; else `atr_wall` →
//!   ATTRIBUTE; else NORMAL. Crossing dispatch table
//!   (`m_collision_bg.c:816`): `{ Normal, Attribute, Normal }` —
//!   moving walls reuse the normal crossing routine.
//! * `mCoBG_JudgeWallFromVector`: `|angle| > 89.5°` under the
//!   engine's `mCoBG_Get2VectorAngleF` convention (movement
//!   sufficiently toward the wall; do not reinterpret the constant
//!   without the angle convention).
//! * `mCoBG_RoughCheckWallHeight`: `bot_y + 3.0f <= start_top ||
//!   <= end_top` before exact interpolation; `mCoBG_GetWallHeight`
//!   interpolates top/bottom along the wall.
//! * Normal crossing (`mCoBG_Cross2Reverse_NormalWall`):
//!   `rev_dist = range + dist + 0.00001f`,
//!   `reverse = normal * rev_dist` — radius-aware separation, not a
//!   teleport to the intersection.
//! * Attribute crossing (`mCoBG_Cross2Reverse_AttributeWall`):
//!   `reverse = cross - actor_end` (line-line intersection).
//! * Distance (`mCoBG_Distance2Reverse_NormalWall`): `dist < range`
//!   → push `(range - dist) + 0.00001f`; else
//!   `|dist - range| < 2.7f` → register contact, no push.
//! * Normal actors (`m_collision_bg.c:1181`): `spd > range * 0.5` →
//!   crossing pass over all walls; then distance pass for static
//!   walls (`regist_p == NULL`); then distance pass for moving
//!   walls. After every wall: `actor_end += rev`.
//! * Player (`m_collision_bg.c:1140`): distance pass over all walls
//!   (temporarily as NORMAL type) → `mCoBG_GetWallPriority`
//!   (merge-sort by squared midpoint distance from actor_start) →
//!   distance pass in priority order (as PLAYER) → crossing pass.
//! * Final: `rev_pos = actor_end - original_end` — the working
//!   endpoint is corrected iteratively; the start is preserved.
//! * Wall geometry padding: `mCoBG_tab_data = { {5.0, 10.0},
//!   {0.000001, 0.000002} }` expands segments beyond tile edges.
//!
//! The exact 2D primitives (segment intersection, point-line
//! distance, endpoint circles, the 0.1f/90°−0x100 neighbor
//! suppression) are modeled with standard equivalents; the
//! dispatch order and formulas above are the verified port.

/// Wall kind discriminator.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WallKind2 {
    Normal = 0,
    Attribute = 1,
    Move = 2,
}

/// One wall candidate.
#[derive(Clone, Debug)]
pub struct WallSeg2 {
    pub start: [f32; 2],
    pub end: [f32; 2],
    pub start_top: f32,
    pub start_btm: f32,
    pub end_top: f32,
    pub end_btm: f32,
    pub normal: [f32; 2],
    pub normal_angle: i16,
    /// True when `regist_p != NULL` (moving platform wall).
    pub moving: bool,
    /// True for attribute/forbidden walls.
    pub atr_wall: bool,
}

impl WallSeg2 {
    pub fn kind(&self) -> WallKind2 {
        if self.moving {
            WallKind2::Move
        } else if self.atr_wall {
            WallKind2::Attribute
        } else {
            WallKind2::Normal
        }
    }

    /// Wall midpoint (used by the player priority sort).
    pub fn midpoint(&self) -> [f32; 2] {
        [
            (self.start[0] + self.end[0]) * 0.5,
            (self.start[1] + self.end[1]) * 0.5,
        ]
    }
}

/// Movement-toward-wall test (`mCoBG_JudgeWallFromVector`).
/// `angle_deg` must use the engine's 2-vector angle convention.
pub fn judge_wall_from_vector(angle_deg: f32) -> bool {
    angle_deg.abs() > 89.5
}

/// Cheap wall-height rejection (`mCoBG_RoughCheckWallHeight`).
pub fn rough_check_wall_height(bot_y: f32, start_top: f32, end_top: f32) -> bool {
    let y = bot_y + 3.0;
    y <= start_top || y <= end_top
}

/// Interpolated wall top/bottom at parameter `p` along the wall
/// (`mCoBG_GetWallHeight`).
pub fn wall_height_at(w: &WallSeg2, p: f32) -> (f32, f32) {
    let top = w.start_top + p * (w.end_top - w.start_top);
    let btm = w.start_btm + p * (w.end_btm - w.start_btm);
    (top, btm)
}

/// Signed distance from a point to the wall's line, using the stored
/// unit normal (equivalent to `mCoBG_GetDistPointAndLine2D_Norm` for
/// unit normals).
pub fn dist_point_line_n(w: &WallSeg2, p: [f32; 2]) -> f32 {
    (p[0] - w.start[0]) * w.normal[0] + (p[1] - w.start[1]) * w.normal[1]
}

/// 2D segment-segment intersection test (crossing predicate).
pub fn segments_cross(a0: [f32; 2], a1: [f32; 2], b0: [f32; 2], b1: [f32; 2]) -> bool {
    let d = |p: [f32; 2], q: [f32; 2], r: [f32; 2]| {
        (q[0] - p[0]) * (r[1] - p[1]) - (q[1] - p[1]) * (r[0] - p[0])
    };
    let d1 = d(a0, a1, b0);
    let d2 = d(a0, a1, b1);
    let d3 = d(b0, b1, a0);
    let d4 = d(b0, b1, a1);
    ((d1 > 0.0) != (d2 > 0.0)) && ((d3 > 0.0) != (d4 > 0.0))
}

/// Segment-segment intersection point.
pub fn segment_intersection(a0: [f32; 2], a1: [f32; 2], b0: [f32; 2], b1: [f32; 2]) -> Option<[f32; 2]> {
    let r = [a1[0] - a0[0], a1[1] - a0[1]];
    let s = [b1[0] - b0[0], b1[1] - b0[1]];
    let denom = r[0] * s[1] - r[1] * s[0];
    if denom.abs() < 1e-9 {
        return None;
    }
    let t = ((b0[0] - a0[0]) * s[1] - (b0[1] - a0[1]) * s[0]) / denom;
    Some([a0[0] + t * r[0], a0[1] + t * r[1]])
}

/// Normal-wall crossing correction:
/// `rev_dist = range + dist + 0.00001`, `reverse = normal * rev_dist`.
pub fn cross_reverse_normal(range: f32, dist: f32, normal: [f32; 2]) -> [f32; 2] {
    let rev_dist = range + dist + 0.00001;
    [normal[0] * rev_dist, normal[1] * rev_dist]
}

/// Attribute-wall crossing correction: `reverse = cross - actor_end`.
pub fn cross_reverse_attribute(cross: [f32; 2], actor_end: [f32; 2]) -> [f32; 2] {
    [cross[0] - actor_end[0], cross[1] - actor_end[1]]
}

/// Contact tolerance for the distance solver.
pub const CONTACT_TOL: f32 = 2.7;

/// Distance-solver outcome.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DistOutcome {
    /// Push out along the normal.
    Push,
    /// Register contact only.
    Contact,
    /// Ignore.
    Ignore,
}

/// Normal-wall distance dispatch:
/// `dist < range` → push; `|dist - range| < 2.7` → contact; else ignore.
pub fn distance_dispatch(dist: f32, range: f32) -> DistOutcome {
    if dist < range {
        DistOutcome::Push
    } else if (dist - range).abs() < CONTACT_TOL {
        DistOutcome::Contact
    } else {
        DistOutcome::Ignore
    }
}

/// Distance push correction: `(range - dist) + 0.00001` along normal.
pub fn distance_push(range: f32, dist: f32, normal: [f32; 2]) -> [f32; 2] {
    let rev_dist = (range - dist) + 0.00001;
    [normal[0] * rev_dist, normal[1] * rev_dist]
}

/// Player wall priority: indices sorted by squared midpoint distance
/// from the actor start (`mCoBG_GetWallPriority`).
pub fn wall_priority(walls: &[WallSeg2], actor_start: [f32; 2]) -> Vec<usize> {
    let mut idx: Vec<usize> = (0..walls.len()).collect();
    idx.sort_by(|&a, &b| {
        let da = {
            let m = walls[a].midpoint();
            (m[0] - actor_start[0]).powi(2) + (m[1] - actor_start[1]).powi(2)
        };
        let db = {
            let m = walls[b].midpoint();
            (m[0] - actor_start[0]).powi(2) + (m[1] - actor_start[1]).powi(2)
        };
        da.partial_cmp(&db).unwrap()
    });
    idx
}

/// The crossing-vs-distance dispatch (`mCoBG_GetWallReverse` core):
///
/// * Player: distance pass (all) → priority-ordered distance pass →
///   crossing pass.
/// * Normal actor: `speed > range * 0.5` → crossing pass; then static
///   distance pass; then moving distance pass.
/// * After every wall: `actor_end += rev`.
/// * Returns `rev_pos = corrected_end - original_end`.
///
/// Height validity and front-side tests are caller-supplied via
/// `wall_ok`; crossing/distance use the verified formulas.
pub fn solve_walls(
    walls: &[WallSeg2],
    actor_start: [f32; 2],
    actor_end: [f32; 2],
    range: f32,
    speed: f32,
    is_player: bool,
    wall_ok: &dyn Fn(&WallSeg2) -> bool,
) -> [f32; 2] {
    let mut end = actor_end;
    let original_end = actor_end;

    // Distance pass over a wall ordering.
    fn distance_pass(
        walls: &[WallSeg2],
        end: &mut [f32; 2],
        order: &[usize],
        range: f32,
        only_moving: Option<bool>,
        wall_ok: &dyn Fn(&WallSeg2) -> bool,
    ) {
        for &i in order {
            let w = &walls[i];
            if !wall_ok(w) {
                continue;
            }
            if let Some(moving) = only_moving {
                if w.moving != moving {
                    continue;
                }
            }
            let dist = dist_point_line_n(w, *end);
            if distance_dispatch(dist, range) == DistOutcome::Push {
                let r = distance_push(range, dist, w.normal);
                end[0] += r[0];
                end[1] += r[1];
            }
        }
    }

    // Crossing pass over all walls.
    fn crossing_pass(
        walls: &[WallSeg2],
        actor_start: [f32; 2],
        end: &mut [f32; 2],
        range: f32,
        wall_ok: &dyn Fn(&WallSeg2) -> bool,
    ) {
        for w in walls.iter() {
            if !wall_ok(w) {
                continue;
            }
            if !segments_cross(actor_start, *end, w.start, w.end) {
                continue;
            }
            let rev = match w.kind() {
                WallKind2::Attribute => {
                    let cross = segment_intersection(actor_start, *end, w.start, w.end)
                        .unwrap_or(*end);
                    cross_reverse_attribute(cross, *end)
                }
                _ => {
                    let dist = dist_point_line_n(w, *end);
                    cross_reverse_normal(range, dist, w.normal)
                }
            };
            end[0] += rev[0];
            end[1] += rev[1];
        }
    }

    if is_player {
        let all: Vec<usize> = (0..walls.len()).collect();
        distance_pass(walls, &mut end, &all, range, None, wall_ok);
        let prio = wall_priority(walls, actor_start);
        distance_pass(walls, &mut end, &prio, range, None, wall_ok);
        crossing_pass(walls, actor_start, &mut end, range, wall_ok);
    } else {
        if speed > range * 0.5 {
            crossing_pass(walls, actor_start, &mut end, range, wall_ok);
        }
        let all: Vec<usize> = (0..walls.len()).collect();
        distance_pass(walls, &mut end, &all, range, Some(false), wall_ok);
        distance_pass(walls, &mut end, &all, range, Some(true), wall_ok);
    }

    [end[0] - original_end[0], end[1] - original_end[1]]
}

/// C ABI: movement-toward-wall test on a precomputed angle.
#[no_mangle]
pub extern "C" fn pc_judge_wall_from_vector(angle_deg: f32) -> i32 {
    judge_wall_from_vector(angle_deg) as i32
}

/// C ABI: distance-solver outcome (0 push, 1 contact, 2 ignore).
#[no_mangle]
pub extern "C" fn pc_distance_dispatch(dist: f32, range: f32) -> i32 {
    match distance_dispatch(dist, range) {
        DistOutcome::Push => 0,
        DistOutcome::Contact => 1,
        DistOutcome::Ignore => 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wall(sx: f32, sz: f32, ex: f32, ez: f32, nx: f32, nz: f32) -> WallSeg2 {
        WallSeg2 {
            start: [sx, sz], end: [ex, ez],
            start_top: 100.0, start_btm: 0.0, end_top: 100.0, end_btm: 0.0,
            normal: [nx, nz], normal_angle: 0,
            moving: false, atr_wall: false,
        }
    }

    #[test]
    fn vector_angle_gate() {
        assert!(judge_wall_from_vector(90.0));
        assert!(judge_wall_from_vector(-95.0));
        assert!(!judge_wall_from_vector(89.5));
        assert!(!judge_wall_from_vector(0.0));
    }

    #[test]
    fn rough_height() {
        assert!(rough_check_wall_height(97.0, 100.0, 100.0));
        assert!(!rough_check_wall_height(100.0, 100.0, 100.0));
    }

    #[test]
    fn crossing_corrections() {
        let r = cross_reverse_normal(10.0, 3.0, [0.0, 1.0]);
        assert!((r[1] - (13.0 + 0.00001)).abs() < 1e-4);
        let a = cross_reverse_attribute([5.0, 5.0], [8.0, 9.0]);
        assert_eq!(a, [-3.0, -4.0]);
    }

    #[test]
    fn distance_outcomes() {
        assert_eq!(distance_dispatch(5.0, 10.0), DistOutcome::Push);
        assert_eq!(distance_dispatch(11.0, 10.0), DistOutcome::Contact);
        assert_eq!(distance_dispatch(12.7, 10.0), DistOutcome::Contact);
        assert_eq!(distance_dispatch(12.71, 10.0), DistOutcome::Ignore);
        assert_eq!(distance_dispatch(50.0, 10.0), DistOutcome::Ignore);
    }

    #[test]
    fn priority_orders_by_midpoint() {
        let w0 = wall(100.0, 0.0, 110.0, 0.0, 0.0, 1.0);
        let w1 = wall(10.0, 0.0, 20.0, 0.0, 0.0, 1.0);
        let prio = wall_priority(&[w0, w1], [0.0, 0.0]);
        assert_eq!(prio, vec![1, 0]);
    }

    #[test]
    fn solver_pushes_out_of_wall() {
        // Vertical wall at x=10, normal facing -x (actor approaches from -x side... use +x normal).
        let w = wall(10.0, -50.0, 10.0, 50.0, -1.0, 0.0);
        let ok = |_: &WallSeg2| true;
        // Actor ends at x=8 (2 units inside range 10 from wall at x=10, dist measured along -x normal = (8-10)*-1 = 2).
        let rev = solve_walls(&[w], [0.0, 0.0], [8.0, 0.0], 10.0, 1.0, false, &ok);
        // dist = 2 < 10 -> push (10-2+eps) along (-1,0) -> rev_x negative.
        assert!(rev[0] < -7.9 && rev[0] > -8.1);
        assert!(rev[1].abs() < 1e-5);
    }

    #[test]
    fn solver_ignores_distant_wall() {
        let w = wall(100.0, -50.0, 100.0, 50.0, -1.0, 0.0);
        let ok = |_: &WallSeg2| true;
        let rev = solve_walls(&[w], [0.0, 0.0], [8.0, 0.0], 10.0, 1.0, false, &ok);
        assert!(rev[0].abs() < 1e-5 && rev[1].abs() < 1e-5);
    }
}
