//! Player wall-priority sort for the Rust rewrite.
//!
//! Source-verified (upstream `src/game/m_collision_bg.c`):
//!
//! The player wall solver is three passes, not one sort+resolve:
//!
//! ```text
//! actor_start = last XZ, shifted by mCoBG_MakeTab2MoveTail
//! actor_end   = current XZ
//! PASS 1: check_type = NORMAL;  Distance2Reverse over all walls
//!         (original order), actor_end += rev each wall
//! PRIORITY: midpoint dist² from actor_start → merge sort →
//!         u64 used-mask index reconstruction → prio_tbl[]
//! PASS 2: check_type = PLAYER;  player-special Distance2Reverse
//!         in priority order, actor_end += rev each wall
//! PASS 3: Cross2Reverse over all walls (original order)
//! rev_pos = actor_end - original_end
//! ```
//!
//! Verified details:
//!
//! * `mCoBG_MakeTab2MoveTail` (`m_collision_bg.c:476`):
//!   `x_bias = |dx| / (|dx| + |dz|)`, `z_bias = 1 - x_bias`;
//!   shifts start backward 0.2 units proportionally.
//! * Priority metric: `((start+end)/2 - actor_start)²` — wall
//!   *midpoint*, squared, from the (adjusted) *old* position. No
//!   normals, types, or heights participate.
//! * `mCoBG_MergeSortFloat` (`m_collision_bg.c:1033`): recursive,
//!   `middle = (first+last)>>1`, halves staged in `pre_work[65]` /
//!   `bk_work[65]`, merge comparison `<=` (left-first).
//! * Reconstruction: for each sorted distance, scan wall indices
//!   0..count and take the first unused index whose
//!   `dist_table[unit] == sorted[i]`, tracked with a `u64` used
//!   mask — so ties resolve in original wall order. NOTE: the mask
//!   is 64 bits while the wall array holds 128; whether ≥64 walls
//!   can reach this function in practice is unresolved.
//! * Dispatch tables (`m_collision_bg.c:1010`):
//!   NORMAL → `{ Normal, Attribute, Normal }`;
//!   PLAYER → `{ NormalSpecial, AttributeSpecial, NormalSpecial }`.
//!   The NORMAL path additionally requires
//!   `mCoBG_RangeCheckLinePoint`; the player-special path instead
//!   requires *both* actor_start and actor_end in front, then does
//!   endpoint-circle handling.
//! * `mCoBG_CheckDistSPCheck`: suppresses the endpoint-circle
//!   correction when another wall shares an endpoint within 0.1 and
//!   the normal-angle difference (u16) is `< 90° − 0x100`.
//! * `mCoBG_Distance2Reverse_NormalWall_Special`
//!   (`m_collision_bg.c:894`): front(end) && front(start), then
//!   `mCoBG_JudgePointInCircle` endpoint tests.

/// Backward movement-tail adjustment (`mCoBG_MakeTab2MoveTail`).
pub fn make_tab_2_move_tail(dst: &mut [f32; 2], src: [f32; 2]) {
    let ax = src[0].abs();
    let az = src[1].abs();
    let denom = (ax + az).max(1e-9);
    let x_bias = ax / denom;
    let z_bias = 1.0 - x_bias;
    if src[0] > 0.0 {
        dst[0] -= x_bias * 0.2;
    } else if src[0] < 0.0 {
        dst[0] += x_bias * 0.2;
    }
    if src[1] > 0.0 {
        dst[1] -= z_bias * 0.2;
    } else if src[1] < 0.0 {
        dst[1] += z_bias * 0.2;
    }
}

/// Recursive merge sort with left-first `<=` merge
/// (`mCoBG_MergeSortFloat`).
pub fn merge_sort_float(data: &mut [f32]) {
    fn sort(data: &mut [f32], first: usize, last: usize) {
        if first < last {
            let middle = (first + last) >> 1;
            sort(data, first, middle);
            sort(data, middle + 1, last);
            let pre: Vec<f32> = data[first..=middle].to_vec();
            let bk: Vec<f32> = data[middle + 1..=last].to_vec();
            let (mut p, mut b, mut s) = (0usize, 0usize, first);
            while p < pre.len() && b < bk.len() {
                if pre[p] <= bk[b] {
                    data[s] = pre[p];
                    p += 1;
                } else {
                    data[s] = bk[b];
                    b += 1;
                }
                s += 1;
            }
            while p < pre.len() {
                data[s] = pre[p];
                p += 1;
                s += 1;
            }
            while b < bk.len() {
                data[s] = bk[b];
                b += 1;
                s += 1;
            }
        }
    }
    if data.len() > 1 {
        let last = data.len() - 1;
        sort(data, 0, last);
    }
}

/// Reconstruct wall indices from sorted distances
/// (`mCoBG_GetWallPriority` tail): for each sorted distance, take
/// the first not-yet-used wall index with an equal distance.
///
/// DEVIATION FROM SOURCE (deliberate, analyzed 2026-10-07): the
/// original uses a `u64` used mask, which is undefined behavior for
/// wall index ≥ 64 (`1 << unit` with `unit >= 64`) while the wall
/// array holds 128 entries. Analysis of the player path
/// (`mCoBG_BgCheckControll` range 18 → 3×3 neighborhood):
/// max terrain walls = 9 slate + 12 normal (edge de-duplication
/// bitmask) + 18 forbid = 39, plus up to 48 circle-defence walls
/// (8 surrounding columns × ordered adjacent pairs × 2) — a
/// theoretical max of ~87, so ≥ 64 is reachable in pathological
/// arrangements (columns in all 8 surrounding units + cliffs on
/// every checked edge + forbid attributes everywhere). On x86-64
/// the shift wraps mod 64, aliasing wall 64 to bit 0 and silently
/// duplicating/skipping a wall in the priority table. Normal
/// gameplay sees < 20 walls and never triggers it, but the Rust
/// port uses `u128` so behavior is bit-identical below 64 walls
/// and correct above — the original's UB is not reproduced.
pub fn reconstruct_priority(dist_table: &[f32], sorted: &[f32]) -> Vec<u8> {
    let count = dist_table.len().min(sorted.len()).min(128);
    let mut flag: u128 = 0;
    let mut prio = Vec::with_capacity(count);
    for i in 0..count {
        let mut pick: u8 = 0;
        for unit in 0..count {
            if dist_table[unit] == sorted[i] && ((flag >> unit) & 1) == 0 {
                flag |= 1 << unit;
                pick = unit as u8;
                break;
            }
        }
        prio.push(pick);
    }
    prio
}

/// Squared midpoint distances from the actor start.
pub fn midpoint_dist2(
    walls: &[([f32; 2], [f32; 2])],
    actor_start: [f32; 2],
) -> Vec<f32> {
    walls
        .iter()
        .map(|(s, e)| {
            let cx = (s[0] + e[0]) * 0.5 - actor_start[0];
            let cz = (s[1] + e[1]) * 0.5 - actor_start[1];
            cx * cx + cz * cz
        })
        .collect()
}

/// Full faithful priority construction: midpoint dist² → merge sort
/// → u64-mask reconstruction → wall indices in priority order.
pub fn priority_order(
    walls: &[([f32; 2], [f32; 2])],
    actor_start: [f32; 2],
) -> Vec<usize> {
    let dist_table = midpoint_dist2(walls, actor_start);
    let mut sorted = dist_table.clone();
    merge_sort_float(&mut sorted);
    reconstruct_priority(&dist_table, &sorted)
        .into_iter()
        .map(|i| i as usize)
        .collect()
}

/// Distance-dispatch mode.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DistRevMode {
    Normal = 0,
    Player = 1,
}

/// Which distance routine a wall kind uses in each mode.
/// NORMAL → { Normal, Attribute, Normal };
/// PLAYER → { NormalSpecial, AttributeSpecial, NormalSpecial }.
pub fn dist_routine(mode: DistRevMode, kind: u8) -> &'static str {
    match (mode, kind) {
        (DistRevMode::Normal, 1) => "Attribute",
        (DistRevMode::Normal, _) => "Normal",
        (DistRevMode::Player, 1) => "AttributeSpecial",
        (DistRevMode::Player, _) => "NormalSpecial",
    }
}

/// Endpoint-sharing suppression test (`mCoBG_CheckDistSPCheck`):
/// true = another wall shares the endpoint within 0.1 AND the
/// u16 normal-angle difference is < 90°−0x100 (0x3F00) → suppress.
pub fn check_dist_sp_suppress(
    point: [f32; 2],
    other_start: [f32; 2],
    other_end: [f32; 2],
    angle_diff_u16: u16,
) -> bool {
    let near = |p: [f32; 2]| {
        (p[0] - point[0]).abs() < 0.1 && (p[1] - point[1]).abs() < 0.1
    };
    (near(other_start) || near(other_end)) && (angle_diff_u16 as u32) < 0x3F00
}

/// Player-special front gate: both actor_end and actor_start must be
/// in front of the wall.
pub fn player_special_front_gate(end_front: bool, start_front: bool) -> bool {
    end_front && start_front
}

/// Point-in-circle test for endpoint handling
/// (`mCoBG_JudgePointInCircle` equivalent).
pub fn point_in_circle(center: [f32; 2], p: [f32; 2], radius: f32) -> bool {
    let dx = p[0] - center[0];
    let dz = p[1] - center[1];
    dx * dx + dz * dz <= radius * radius
}

/// C ABI: apply the movement-tail adjustment in place.
#[no_mangle]
pub extern "C" fn pc_make_tab_2_move_tail(dst_xz: *mut f32, src_x: f32, src_z: f32) {
    if dst_xz.is_null() {
        return;
    }
    let dst = unsafe { core::slice::from_raw_parts_mut(dst_xz, 2) };
    let mut d = [dst[0], dst[1]];
    make_tab_2_move_tail(&mut d, [src_x, src_z]);
    dst[0] = d[0];
    dst[1] = d[1];
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn move_tail_adjustment() {
        // Pure +x movement: shift back 0.2 in x only.
        let mut d = [10.0f32, 5.0];
        make_tab_2_move_tail(&mut d, [4.0, 0.0]);
        assert!((d[0] - 9.8).abs() < 1e-6 && (d[1] - 5.0).abs() < 1e-6);
        // Diagonal: proportional split.
        let mut d = [0.0f32, 0.0];
        make_tab_2_move_tail(&mut d, [3.0, 3.0]);
        assert!((d[0] + 0.1).abs() < 1e-6 && (d[1] + 0.1).abs() < 1e-6);
        // Negative direction shifts forward-positive.
        let mut d = [0.0f32, 0.0];
        make_tab_2_move_tail(&mut d, [-2.0, 0.0]);
        assert!((d[0] - 0.2).abs() < 1e-6);
    }

    #[test]
    fn merge_sort_orders() {
        let mut v = [5.0f32, 1.0, 4.0, 1.0, 3.0];
        merge_sort_float(&mut v);
        assert_eq!(v, [1.0, 1.0, 3.0, 4.0, 5.0]);
        let mut empty: [f32; 0] = [];
        merge_sort_float(&mut empty);
    }

    #[test]
    fn reconstruction_tie_breaks_by_index() {
        // Walls 1 and 3 tie at 100; wall 1 wins the first slot.
        let dist = [500.0f32, 100.0, 300.0, 100.0];
        let mut sorted = dist.to_vec();
        merge_sort_float(&mut sorted);
        assert_eq!(reconstruct_priority(&dist, &sorted), vec![1, 3, 2, 0]);
    }

    #[test]
    fn priority_is_midpoint_based() {
        let walls = [([100.0f32, 0.0], [110.0, 0.0]), ([10.0, 0.0], [20.0, 0.0])];
        assert_eq!(priority_order(&walls, [0.0, 0.0]), vec![1, 0]);
    }

    #[test]
    fn dispatch_tables() {
        assert_eq!(dist_routine(DistRevMode::Normal, 0), "Normal");
        assert_eq!(dist_routine(DistRevMode::Normal, 1), "Attribute");
        assert_eq!(dist_routine(DistRevMode::Normal, 2), "Normal");
        assert_eq!(dist_routine(DistRevMode::Player, 0), "NormalSpecial");
        assert_eq!(dist_routine(DistRevMode::Player, 1), "AttributeSpecial");
        assert_eq!(dist_routine(DistRevMode::Player, 2), "NormalSpecial");
    }

    #[test]
    fn sp_suppression() {
        assert!(check_dist_sp_suppress([1.0, 1.0], [1.05, 1.02], [9.0, 9.0], 0x1000));
        assert!(!check_dist_sp_suppress([1.0, 1.0], [5.0, 5.0], [9.0, 9.0], 0x1000));
        assert!(!check_dist_sp_suppress([1.0, 1.0], [1.05, 1.02], [9.0, 9.0], 0x4000));
        assert!(player_special_front_gate(true, true));
        assert!(!player_special_front_gate(true, false));
        assert!(point_in_circle([0.0, 0.0], [3.0, 4.0], 5.0));
        assert!(!point_in_circle([0.0, 0.0], [3.0, 4.0], 4.9));
    }
}
