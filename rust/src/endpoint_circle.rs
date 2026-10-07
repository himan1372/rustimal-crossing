//! Endpoint-circle intersection math for the Rust rewrite.
//!
//! Source-verified (upstream `src/game/m_collision_bg.c`,
//! `src/game/m_collision_bg_math.c_inc`):
//!
//! CORRECTION (confirmed at `m_collision_bg.c:908`): the player-special
//! endpoint solver does NOT intersect the wall segment with the
//! actor's circle. It intersects a line through the wall ENDPOINT
//! parallel to the wall NORMAL with the actor's X/Z circle:
//!
//! ```text
//! mCoBG_GetCrossCircleAndLine2Dvector(
//!     cross0, cross1,
//!     unit_vec->start,   // endpoint, not segment start
//!     unit_vec->normal,  // NORMAL, not (end - start)
//!     actor_end,         // circle center
//!     actor_info->range  // circle radius
//! );
//! ```
//!
//! The segment-based sibling `mCoBG_GetCrossCircleAndLine2D` is
//! decomp-marked @unused/@fabricated — the engine deliberately does
//! not use it here.
//!
//! Verified details:
//!
//! * `mCoBG_GetCrossCircleAndLine2Dvector`
//!   (`m_collision_bg_math.c_inc:295`): general parametric
//!   line/circle solver. `A = vx²+vz²` (no normalization assumed);
//!   `B = 2(v·point − v·center)`;
//!   `C = |point−center|² − r²`; `R = B²−4AC` accepted when
//!   `R >= 0` (tangent counts); `root = ABS(sqrtf(R))`
//!   (redundant but verbatim); `A != 0` guard;
//!   `t = (−B ± root) / 2A`; `cross = point + t·vec`.
//! * `mCoBG_GetSpecialDistanceReverse` (`m_collision_bg.c:858`):
//!   picks the first intersection on the wall's NON-front side
//!   (`!GetPointInfoFrontLine(start, cross, normal)`);
//!   `reverse = edge − cross`. Since `cross = edge + t·N`, the
//!   correction is exactly parallel to the wall normal (`−tN`).
//! * Gate sequence (`mCoBG_Distance2Reverse_NormalWall_Special`,
//!   `m_collision_bg.c:894`): front(end) && front(start) → normal
//!   distance `dist < range` → start endpoint in circle?
//!   (else end endpoint) → `CheckDistSPCheck` suppression →
//!   circle/normal-line intersection → height gate
//!   `(old_ground_y − 5) + 3 ≤ height.top` → special reverse +
//!   wall-info registration.
//! * `mCoBG_GetDistPointAndLine2D_Norm`: `dist = |n·p − n·start|`
//!   with no division by |n| (terrain normals are unit length).
//! * `mCoBG_JudgePointInCircle`: squared comparison, no sqrt.
//! * The XYZ wrapper `...PlaneXZ_Xyz` extracts X/Z, runs the 2-D
//!   math, writes X/Z back — Y never participates.
//!
//! Interpretation (not a source claim): this is an
//! endpoint-normal-line/actor-circle construction, not a capsule
//! intersection. Do not describe it as "capsule collision" in
//! faithful documentation.

use crate::segment_map::point_info_front_line;
use crate::wall_priority::check_dist_sp_suppress;

/// Squared point-in-circle test (`mCoBG_JudgePointInCircle`).
pub fn judge_point_in_circle(center: [f32; 2], p: [f32; 2], radius: f32) -> bool {
    let dx = p[0] - center[0];
    let dz = p[1] - center[1];
    dx * dx + dz * dz <= radius * radius
}

/// Normal-plane distance (`mCoBG_GetDistPointAndLine2D_Norm`):
/// `|n·point − n·line0|`, no normalization.
pub fn dist_point_and_line_2d_norm(line0: [f32; 2], point: [f32; 2], normal: [f32; 2]) -> f32 {
    (normal[0] * point[0] + normal[1] * point[1]
        - (normal[0] * line0[0] + normal[1] * line0[1]))
    .abs()
}

/// Parametric line/circle intersection
/// (`mCoBG_GetCrossCircleAndLine2Dvector`), verbatim.
/// Returns the two intersections; `None` when `R < 0` or `A == 0`.
pub fn cross_circle_and_line_2dvector(
    point: [f32; 2],
    vec: [f32; 2],
    center: [f32; 2],
    radius: f32,
) -> Option<([f32; 2], [f32; 2])> {
    let a = vec[0] * vec[0] + vec[1] * vec[1];
    let b = 2.0
        * (vec[0] * point[0] - vec[0] * center[0] + vec[1] * point[1] - vec[1] * center[1]);
    let c = (point[0] - center[0]).powi(2) + (point[1] - center[1]).powi(2) - radius * radius;
    let r = b * b - 4.0 * a * c;
    if r >= 0.0 {
        let root = r.sqrt().abs();
        if a != 0.0 {
            let t0 = (-b + root) / (2.0 * a);
            let t1 = (-b - root) / (2.0 * a);
            let cross0 = [point[0] + t0 * vec[0], point[1] + t0 * vec[1]];
            let cross1 = [point[0] + t1 * vec[0], point[1] + t1 * vec[1]];
            return Some((cross0, cross1));
        }
    }
    None
}

/// Special reverse selection (`mCoBG_GetSpecialDistanceReverse`):
/// first intersection on the non-front side wins;
/// `reverse = edge − cross`. `None` when both are in front.
pub fn get_special_distance_reverse(
    edge: [f32; 2],
    cross0: [f32; 2],
    cross1: [f32; 2],
    wall_start: [f32; 2],
    normal: [f32; 2],
) -> Option<[f32; 2]> {
    if !point_info_front_line(wall_start, cross0, normal) {
        Some([edge[0] - cross0[0], edge[1] - cross0[1]])
    } else if !point_info_front_line(wall_start, cross1, normal) {
        Some([edge[0] - cross1[0], edge[1] - cross1[1]])
    } else {
        None
    }
}

/// Which endpoint (if any) participates.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EndpointChoice {
    Start,
    End,
}

/// Full player-special endpoint-circle sequence
/// (`mCoBG_Distance2Reverse_NormalWall_Special` core geometry).
///
/// Returns the chosen endpoint and the reverse vector, or `None`
/// when any gate fails. `sp_suppress` = the `CheckDistSPCheck`
/// result (true = another wall shares the endpoint → suppress).
/// `height_top` = the endpoint's wall-bounds top.
pub fn endpoint_circle_collision(
    wall_start: [f32; 2],
    wall_end: [f32; 2],
    normal: [f32; 2],
    actor_start: [f32; 2],
    actor_end: [f32; 2],
    range: f32,
    old_ground_y: f32,
    height_top: f32,
    sp_suppress: bool,
) -> Option<(EndpointChoice, [f32; 2])> {
    if !(point_info_front_line(wall_start, actor_end, normal)
        && point_info_front_line(wall_start, actor_start, normal))
    {
        return None;
    }
    if dist_point_and_line_2d_norm(wall_start, actor_end, normal) >= range {
        return None;
    }
    let edge = if judge_point_in_circle(wall_start, actor_end, range) {
        EndpointChoice::Start
    } else if judge_point_in_circle(wall_end, actor_end, range) {
        EndpointChoice::End
    } else {
        return None;
    };
    if sp_suppress {
        return None;
    }
    let edge_pt = match edge {
        EndpointChoice::Start => wall_start,
        EndpointChoice::End => wall_end,
    };
    let (cross0, cross1) = cross_circle_and_line_2dvector(edge_pt, normal, actor_end, range)?;
    if (old_ground_y - 5.0) + 3.0 > height_top {
        return None;
    }
    let reverse = get_special_distance_reverse(edge_pt, cross0, cross1, wall_start, normal)?;
    Some((edge, reverse))
}

/// Wall-kind dispatch values (`mCoBG_GetWallKind` enum verbatim):
/// `regist_p != NULL` -> MOVE; else `atr_wall` -> ATTRIBUTE; else NORMAL.
/// The player table is `{ NormalWall_Special, AttributeWall_Special,
/// NormalWall_Special }`, so MOVE walls reuse the normal-special path.
pub mod wall_kind {
    pub const NORMAL: u8 = 0;
    pub const ATTRIBUTE: u8 = 1;
    pub const MOVE: u8 = 2;
}

/// `mCoBG_GetWallKind` verbatim.
pub fn get_wall_kind(has_regist: bool, atr_wall: bool) -> u8 {
    if has_regist {
        wall_kind::MOVE
    } else if atr_wall {
        wall_kind::ATTRIBUTE
    } else {
        wall_kind::NORMAL
    }
}

/// The normal player-special height gate, factored out so the exact
/// difference is explicit: `(old_ground_y - 5.0) + 3.0 <= height_top`
/// (i.e. `old_ground_y - 2.0 <= top`). The attribute path never calls
/// this — it has no height gate at all.
pub fn normal_special_height_gate(old_ground_y: f32, height_top: f32) -> bool {
    (old_ground_y - 5.0) + 3.0 <= height_top
}

/// Attribute-wall player special path
/// (`mCoBG_Distance2Reverse_AttributeWall_Special` verbatim):
/// IDENTICAL horizontal test to `endpoint_circle_collision` — same
/// front-line gates, same `dist < range`, same endpoint-circle tests,
/// same SP suppression, same `GetSpecialDistanceReverse` — but with
/// NO height gate (the wall always blocks regardless of elevation)
/// and NULL registered height. It also does not set the
/// moving-background contact side.
pub fn attribute_wall_special_collision(
    wall_start: [f32; 2],
    wall_end: [f32; 2],
    normal: [f32; 2],
    actor_start: [f32; 2],
    actor_end: [f32; 2],
    range: f32,
    sp_suppress: bool,
) -> Option<(EndpointChoice, [f32; 2])> {
    if !(point_info_front_line(wall_start, actor_end, normal)
        && point_info_front_line(wall_start, actor_start, normal))
    {
        return None;
    }
    if dist_point_and_line_2d_norm(wall_start, actor_end, normal) >= range {
        return None;
    }
    let edge = if judge_point_in_circle(wall_start, actor_end, range) {
        EndpointChoice::Start
    } else if judge_point_in_circle(wall_end, actor_end, range) {
        EndpointChoice::End
    } else {
        return None;
    };
    if sp_suppress {
        return None;
    }
    let edge_pt = match edge {
        EndpointChoice::Start => wall_start,
        EndpointChoice::End => wall_end,
    };
    let (cross0, cross1) = cross_circle_and_line_2dvector(edge_pt, normal, actor_end, range)?;
    // NOTE: no height gate here — this is THE difference from the
    // normal-special path (which rejects when
    // `(old_ground_y - 5.0) + 3.0 > height_top`).
    let reverse = get_special_distance_reverse(edge_pt, cross0, cross1, wall_start, normal)?;
    Some((edge, reverse))
}

/// Re-export of the corner-suppression test for the endpoint path
/// (`mCoBG_CheckDistSPCheck`; see `wall_priority.rs`).
pub use crate::wall_priority::check_dist_sp_suppress as check_dist_sp_check;

/// C ABI: attribute-wall special path; writes reverse[2]; returns 1 on hit.
#[no_mangle]
pub extern "C" fn pc_attribute_wall_special(
    ws_x: f32,
    ws_z: f32,
    we_x: f32,
    we_z: f32,
    n_x: f32,
    n_z: f32,
    as_x: f32,
    as_z: f32,
    ae_x: f32,
    ae_z: f32,
    range: f32,
    sp_suppress: u8,
    out_reverse: *mut f32,
) -> u8 {
    match attribute_wall_special_collision(
        [ws_x, ws_z],
        [we_x, we_z],
        [n_x, n_z],
        [as_x, as_z],
        [ae_x, ae_z],
        range,
        sp_suppress != 0,
    ) {
        Some((_, rev)) => {
            if !out_reverse.is_null() {
                unsafe {
                    *out_reverse.add(0) = rev[0];
                    *out_reverse.add(1) = rev[1];
                }
            }
            1
        }
        None => 0,
    }
}

/// C ABI: line/circle intersection; writes cross0[2], cross1[2];
/// returns 1 on intersection.
#[no_mangle]
pub extern "C" fn pc_cross_circle_line(
    point_x: f32,
    point_z: f32,
    vec_x: f32,
    vec_z: f32,
    center_x: f32,
    center_z: f32,
    radius: f32,
    out0: *mut f32,
    out1: *mut f32,
) -> u8 {
    match cross_circle_and_line_2dvector(
        [point_x, point_z],
        [vec_x, vec_z],
        [center_x, center_z],
        radius,
    ) {
        Some((c0, c1)) => {
            if !out0.is_null() {
                unsafe { core::slice::from_raw_parts_mut(out0, 2).copy_from_slice(&c0) };
            }
            if !out1.is_null() {
                unsafe { core::slice::from_raw_parts_mut(out1, 2).copy_from_slice(&c1) };
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
    fn quadratic_basics() {
        // Line x=0 (point (0,0), vec (0,1)) vs circle center (3,0) r=5:
        // intersections at z = ±4.
        let (c0, c1) =
            cross_circle_and_line_2dvector([0.0, 0.0], [0.0, 1.0], [3.0, 0.0], 5.0).unwrap();
        assert!((c0[1] - 4.0).abs() < 1e-4 && (c1[1] + 4.0).abs() < 1e-4);
        // Tangent: R = 0 counts.
        assert!(cross_circle_and_line_2dvector([5.0, 0.0], [0.0, 1.0], [0.0, 0.0], 5.0).is_some());
        // Miss.
        assert!(cross_circle_and_line_2dvector([6.0, 0.0], [0.0, 1.0], [0.0, 0.0], 5.0).is_none());
        // Zero vector.
        assert!(cross_circle_and_line_2dvector([0.0, 0.0], [0.0, 0.0], [0.0, 0.0], 5.0).is_none());
    }

    #[test]
    fn special_reverse_picks_non_front() {
        // Wall start (0,0), normal (0,1): front is z >= 0.
        // cross0 in front, cross1 behind -> picks cross1.
        let r = get_special_distance_reverse(
            [0.0, 0.0],
            [0.0, 2.0],
            [0.0, -3.0],
            [0.0, 0.0],
            [0.0, 1.0],
        )
        .unwrap();
        assert_eq!(r, [0.0, 3.0]); // edge - cross1, parallel to normal
        // Both in front -> None.
        assert!(get_special_distance_reverse(
            [0.0, 0.0],
            [0.0, 2.0],
            [0.0, 1.0],
            [0.0, 0.0],
            [0.0, 1.0]
        )
        .is_none());
    }

    #[test]
    fn full_sequence_gates() {
        let ws = [0.0f32, 0.0];
        let we = [40.0, 0.0];
        let n = [0.0, 1.0];
        // Actor in front, near the start endpoint, inside circle.
        let ok = endpoint_circle_collision(ws, we, n, [2.0, 6.0], [2.0, 4.0], 18.0, 0.0, 100.0, false);
        let (choice, rev) = ok.unwrap();
        assert_eq!(choice, EndpointChoice::Start);
        // Reverse is parallel to the normal (x-component ~ 0).
        assert!(rev[0].abs() < 1e-3 && rev[1] > 0.0);
        // Behind the wall -> None.
        assert!(endpoint_circle_collision(ws, we, n, [2.0, -6.0], [2.0, -4.0], 18.0, 0.0, 100.0, false).is_none());
        // Suppressed -> None.
        assert!(endpoint_circle_collision(ws, we, n, [2.0, 6.0], [2.0, 4.0], 18.0, 0.0, 100.0, true).is_none());
        // Too low a wall -> None.
        assert!(endpoint_circle_collision(ws, we, n, [2.0, 6.0], [2.0, 4.0], 18.0, 0.0, -10.0, false).is_none());
        // Endpoint outside the circle -> None.
        assert!(endpoint_circle_collision(ws, we, n, [60.0, 6.0], [60.0, 4.0], 18.0, 0.0, 100.0, false).is_none());
    }

    #[test]
    fn attribute_wall_special_exact_differences() {
        let ws = [0.0, 0.0];
        let we = [10.0, 0.0];
        let n = [0.0, 1.0];
        // Same horizontal setup the normal path accepts.
        let args = (ws, we, n, [2.0, 6.0], [2.0, 4.0], 18.0);
        // Normal path: too low a wall -> None (height gate).
        assert!(endpoint_circle_collision(args.0, args.1, args.2, args.3, args.4, args.5, 0.0, -10.0, false).is_none());
        // Attribute path: NO height gate -> hit even with a very low wall top.
        let (choice, rev) = attribute_wall_special_collision(ws, we, n, [2.0, 6.0], [2.0, 4.0], 18.0, false).unwrap();
        assert_eq!(choice, EndpointChoice::Start);
        // Same reverse math as the normal path: parallel to the normal.
        assert!(rev[0].abs() < 1e-3 && rev[1] > 0.0);
        // The normal gate, factored: (0 - 5) + 3 = -2 <= -10 is false.
        assert!(!normal_special_height_gate(0.0, -10.0));
        assert!(normal_special_height_gate(0.0, 100.0));
        // Boundary: (10 - 5) + 3 = 8 <= 8 -> hit.
        assert!(normal_special_height_gate(10.0, 8.0));
        assert!(!normal_special_height_gate(10.0, 7.9));
        // Attribute path still respects the horizontal gates.
        assert!(attribute_wall_special_collision(ws, we, n, [2.0, -6.0], [2.0, -4.0], 18.0, false).is_none());
        assert!(attribute_wall_special_collision(ws, we, n, [2.0, 6.0], [2.0, 4.0], 18.0, true).is_none());
        // Wall-kind dispatch.
        assert_eq!(get_wall_kind(false, false), wall_kind::NORMAL);
        assert_eq!(get_wall_kind(false, true), wall_kind::ATTRIBUTE);
        assert_eq!(get_wall_kind(true, true), wall_kind::MOVE);
        assert_eq!(get_wall_kind(true, false), wall_kind::MOVE);
    }

    #[test]
    fn dist_norm_and_circle() {
        assert!((dist_point_and_line_2d_norm([0.0, 0.0], [3.0, 4.0], [0.0, 1.0]) - 4.0).abs() < 1e-6);
        assert!(judge_point_in_circle([0.0, 0.0], [3.0, 4.0], 5.0));
        assert!(!judge_point_in_circle([0.0, 0.0], [3.0, 4.0], 4.9));
        // check_dist_sp_check re-export works.
        assert!(check_dist_sp_check([1.0, 1.0], [1.05, 1.02], [9.0, 9.0], 0x1000));
    }
}
