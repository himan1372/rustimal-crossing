//! Segment-orientation mapping for the Rust rewrite.
//!
//! Source-verified (upstream `src/game/m_collision_bg_wall.c_inc`,
//! `src/game/m_collision_bg_math.c_inc`, `src/game/m_collision_bg.c`,
//! `include/m_collision_bg.h`, `include/m_field_info.h`):
//!
//! The central architectural point: a wall's *segment geometry*
//! (start→end) and its *collision orientation* (normal +
//! normal_angle) are independent, separately stored quantities.
//! `wall_name` picks the segment placement and interpolation
//! behavior; the normal is chosen from terrain height ordering.
//!
//! * `mCoBG_UnitNoName2StartEnd` (`m_collision_bg_wall.c_inc:1`):
//!   maps (unit, wall_name, check_type) to start/end. UP runs +X
//!   along the north boundary; DOWN is the same physical
//!   orientation with reversed endpoint order; LEFT/RIGHT run +Z;
//!   SLATE_UP descends (−Z with +X), SLATE_DOWN ascends (+Z).
//!   Segments extend past the unit by `tab.t0`/`tab.t1`.
//! * `mCoBG_tab_data` (`m_collision_bg.c:58`):
//!   `{{5.0, 10.0}, {0.000001, 0.000002}}` — NORMAL walls get
//!   ~5/10-unit extensions, PLAYER walls get essentially exact
//!   boundaries. Deliberate per-path tuning.
//! * Unit world size: `mFI_UNIT_BASE_SIZE = 40`
//!   (`include/m_field_info.h:16`).
//! * `mCoBG_SearchWallFlag`: axis walls choose normals from
//!   neighboring corner-height comparisons — the normal points
//!   toward the higher side:
//!   UP: neighbor down-offsets higher → (0,+1)/0° else (0,−1)/180°
//!   DOWN: neighbor up-offsets higher → (0,−1)/180° else (0,+1)/0°
//!   LEFT: neighbor right offsets higher → (+1,0)/90° else (−1,0)/−90°
//!   RIGHT: neighbor left offsets higher → (−1,0)/−90° else (+1,0)/90°
//! * Slate walls pick one of two diagonal normals from corner
//!   relationships: SLATE_UP uses leftUp vs rightDown → ±45°/±135°;
//!   SLATE_DOWN uses leftDown vs rightUp → ±135°/±45°.
//! * `mCoBG_GetPointInfoFrontLine` (`m_collision_bg_math.c_inc:251`):
//!   front ⇔ `n·point − n·start ≥ 0`. Front/back is derived from
//!   the stored normal, never from segment direction.
//! * `wall_name` drives height interpolation (X for UP/DOWN, Z for
//!   LEFT/RIGHT, projected segment for slate) — already modeled
//!   in `wall_solver.rs`'s `wall_height_at`.
//!
//! Unknown: Nintendo's original terminology for the
//! wall_name/normal/normal_angle distinction is not in the decomp.

/// Wall-name constants (`include/m_collision_bg.h`).
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WallName {
    Up = 0,
    Left = 1,
    Down = 2,
    Right = 3,
    SlateUp = 4,
    SlateDown = 5,
}

/// Check types indexing `mCoBG_tab_data`.
#[repr(usize)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CheckType {
    Normal = 0,
    Player = 1,
}

/// `mCoBG_tab_data` — segment extension padding per check type.
pub const TAB_DATA: [[f32; 2]; 2] = [[5.0, 10.0], [0.000001, 0.000002]];

/// Unit world size (`mFI_UNIT_BASE_SIZE`).
pub const UNIT_SIZE: f32 = 40.0;

/// Segment placement (`mCoBG_UnitNoName2StartEnd`), verbatim.
/// `ux`/`uz` are unit coordinates; returns (start, end) in X/Z.
pub fn unit_no_name_2_start_end(
    ux: f32,
    uz: f32,
    wall_name: WallName,
    check_type: CheckType,
) -> ([f32; 2], [f32; 2]) {
    let tab = TAB_DATA[check_type as usize];
    let (t0, t1) = (tab[0], tab[1]);
    let u = UNIT_SIZE;
    match wall_name {
        WallName::Up => {
            let s = [ux * u - t0, uz * u];
            (s, [s[0] + u + t1, s[1]])
        }
        WallName::Left => {
            let s = [ux * u, uz * u - t0];
            (s, [s[0], s[1] + u + t1])
        }
        WallName::Down => {
            let e = [ux * u - t0, (uz + 1.0) * u];
            ([e[0] + u + t1, e[1]], e)
        }
        WallName::Right => {
            let s = [(ux + 1.0) * u, uz * u - t0];
            (s, [s[0], s[1] + u + t1])
        }
        WallName::SlateUp => {
            let s = [ux * u - t0, (uz + 1.0) * u + t0];
            (s, [s[0] + u + t1, s[1] - u - t1])
        }
        WallName::SlateDown => {
            let s = [ux * u - t0, uz * u - t0];
            (s, [s[0] + u + t1, s[1] + u + t1])
        }
    }
}

/// Front-side test (`mCoBG_GetPointInfoFrontLine`):
/// `n·point − n·start ≥ 0`.
pub fn point_info_front_line(start: [f32; 2], point: [f32; 2], normal: [f32; 2]) -> bool {
    normal[0] * point[0] + normal[1] * point[1] - (normal[0] * start[0] + normal[1] * start[1]) >= 0.0
}

/// The four corner height offsets used by the normal selectors.
#[derive(Clone, Copy, Debug, Default)]
pub struct CornerOffsets {
    pub left_up: f32,
    pub right_up: f32,
    pub left_down: f32,
    pub right_down: f32,
}

/// Normal selection for axis walls (`mCoBG_SearchWallFlag`).
/// `own` = current unit's corners, `nbr` = neighboring unit's.
/// Returns (normal, angle_deg) or None when heights are equal
/// (no wall registered).
pub fn search_wall_flag(
    wall_name: WallName,
    own: CornerOffsets,
    nbr: CornerOffsets,
) -> Option<([f32; 2], f32)> {
    match wall_name {
        WallName::Up => {
            if own.left_up != nbr.left_down || own.right_up != nbr.right_down {
                if nbr.left_down > own.left_up || nbr.right_down > own.right_up {
                    Some(([0.0, 1.0], 0.0))
                } else {
                    Some(([0.0, -1.0], 180.0))
                }
            } else {
                None
            }
        }
        WallName::Down => {
            if own.left_down != nbr.left_up || own.right_down != nbr.right_up {
                if nbr.left_up > own.left_down || nbr.right_up > own.right_down {
                    Some(([0.0, -1.0], 180.0))
                } else {
                    Some(([0.0, 1.0], 0.0))
                }
            } else {
                None
            }
        }
        WallName::Left => {
            if own.left_up != nbr.right_up || own.left_down != nbr.right_down {
                if nbr.right_up > own.left_up || nbr.right_down > own.left_down {
                    Some(([1.0, 0.0], 90.0))
                } else {
                    Some(([-1.0, 0.0], -90.0))
                }
            } else {
                None
            }
        }
        WallName::Right => {
            if own.right_up != nbr.left_up || own.right_down != nbr.left_down {
                if nbr.left_up > own.left_down || nbr.right_up > own.right_down {
                    Some(([-1.0, 0.0], -90.0))
                } else {
                    Some(([1.0, 0.0], 90.0))
                }
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Slate normal selection: SLATE_UP compares leftUp vs rightDown,
/// SLATE_DOWN compares leftDown vs rightUp.
pub fn slate_normal(wall_name: WallName, o: CornerOffsets) -> ([f32; 2], f32) {
    const R: f32 = 0.7071067811865476;
    match wall_name {
        WallName::SlateUp => {
            if o.left_up > o.right_down {
                ([R, R], 45.0)
            } else {
                ([-R, -R], -135.0)
            }
        }
        _ => {
            if o.left_down > o.right_up {
                ([R, -R], 135.0)
            } else {
                ([-R, R], -45.0)
            }
        }
    }
}

/// Interpolation axis implied by the wall name (used for wall
/// height): X for UP/DOWN, Z for LEFT/RIGHT, projected for slate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InterpAxis {
    X,
    Z,
    Projected,
}

pub fn interp_axis(wall_name: WallName) -> InterpAxis {
    match wall_name {
        WallName::Up | WallName::Down => InterpAxis::X,
        WallName::Left | WallName::Right => InterpAxis::Z,
        WallName::SlateUp | WallName::SlateDown => InterpAxis::Projected,
    }
}

/// C ABI: segment placement; writes start[2], end[2].
#[no_mangle]
pub extern "C" fn pc_unit_no_name_2_start_end(
    ux: f32,
    uz: f32,
    wall_name: u8,
    check_type: u8,
    out_start: *mut f32,
    out_end: *mut f32,
) {
    let name = match wall_name {
        0 => WallName::Up,
        1 => WallName::Left,
        2 => WallName::Down,
        3 => WallName::Right,
        4 => WallName::SlateUp,
        _ => WallName::SlateDown,
    };
    let ct = if check_type == 1 { CheckType::Player } else { CheckType::Normal };
    let (s, e) = unit_no_name_2_start_end(ux, uz, name, ct);
    if !out_start.is_null() {
        let d = unsafe { core::slice::from_raw_parts_mut(out_start, 2) };
        d.copy_from_slice(&s);
    }
    if !out_end.is_null() {
        let d = unsafe { core::slice::from_raw_parts_mut(out_end, 2) };
        d.copy_from_slice(&e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: [f32; 2], b: [f32; 2]) {
        assert!((a[0] - b[0]).abs() < 1e-4 && (a[1] - b[1]).abs() < 1e-4, "{a:?} vs {b:?}");
    }

    #[test]
    fn segment_placement_normal() {
        // UP: +X along north boundary, extended by 5/10.
        let (s, e) = unit_no_name_2_start_end(2.0, 3.0, WallName::Up, CheckType::Normal);
        approx(s, [75.0, 120.0]);
        approx(e, [125.0, 120.0]);
        // DOWN: same line, reversed endpoint order.
        let (s, e) = unit_no_name_2_start_end(2.0, 3.0, WallName::Down, CheckType::Normal);
        approx(e, [75.0, 160.0]);
        approx(s, [125.0, 160.0]);
        // LEFT/RIGHT: +Z verticals.
        let (s, e) = unit_no_name_2_start_end(2.0, 3.0, WallName::Left, CheckType::Normal);
        approx(s, [80.0, 115.0]);
        approx(e, [80.0, 165.0]);
        let (s, e) = unit_no_name_2_start_end(2.0, 3.0, WallName::Right, CheckType::Normal);
        approx(s, [120.0, 115.0]);
        approx(e, [120.0, 165.0]);
        // Slates: diagonals.
        let (s, e) = unit_no_name_2_start_end(2.0, 3.0, WallName::SlateUp, CheckType::Normal);
        approx(s, [75.0, 165.0]);
        approx(e, [125.0, 115.0]);
        let (s, e) = unit_no_name_2_start_end(2.0, 3.0, WallName::SlateDown, CheckType::Normal);
        approx(s, [75.0, 115.0]);
        approx(e, [125.0, 165.0]);
    }

    #[test]
    fn player_padding_is_near_zero() {
        let (s, e) = unit_no_name_2_start_end(0.0, 0.0, WallName::Up, CheckType::Player);
        assert!((e[0] - s[0] - 40.0).abs() < 1e-3);
        let (s, e) = unit_no_name_2_start_end(0.0, 0.0, WallName::Up, CheckType::Normal);
        assert!((e[0] - s[0] - 50.0).abs() < 1e-6); // U + t1
    }

    #[test]
    fn front_line_test() {
        assert!(point_info_front_line([0.0, 0.0], [0.0, 5.0], [0.0, 1.0]));
        assert!(!point_info_front_line([0.0, 0.0], [0.0, -5.0], [0.0, 1.0]));
        assert!(point_info_front_line([0.0, 0.0], [0.0, 0.0], [0.0, 1.0])); // >= 0
    }

    #[test]
    fn normal_selection() {
        let low = CornerOffsets { left_up: 0.0, right_up: 0.0, left_down: 0.0, right_down: 0.0 };
        let high = CornerOffsets { left_up: 10.0, right_up: 10.0, left_down: 10.0, right_down: 10.0 };
        // UP: higher neighbor to the north -> (0,+1)/0°.
        assert_eq!(search_wall_flag(WallName::Up, low, high), Some(([0.0, 1.0], 0.0)));
        assert_eq!(search_wall_flag(WallName::Up, high, low), Some(([0.0, -1.0], 180.0)));
        assert_eq!(search_wall_flag(WallName::Up, low, low), None);
        // LEFT: higher neighbor to the west -> (+1,0)/90°.
        assert_eq!(search_wall_flag(WallName::Left, low, high), Some(([1.0, 0.0], 90.0)));
        // DOWN/RIGHT mirror.
        assert_eq!(search_wall_flag(WallName::Down, low, high), Some(([0.0, -1.0], 180.0)));
        assert_eq!(search_wall_flag(WallName::Right, low, high), Some(([-1.0, 0.0], -90.0)));
        // Slate: height relationship picks the diagonal.
        let o = CornerOffsets { left_up: 5.0, right_down: 1.0, ..Default::default() };
        assert_eq!(slate_normal(WallName::SlateUp, o).1, 45.0);
        let o = CornerOffsets { left_down: 5.0, right_up: 1.0, ..Default::default() };
        assert_eq!(slate_normal(WallName::SlateDown, o).1, 135.0);
    }

    #[test]
    fn axes() {
        assert_eq!(interp_axis(WallName::Up), InterpAxis::X);
        assert_eq!(interp_axis(WallName::Down), InterpAxis::X);
        assert_eq!(interp_axis(WallName::Left), InterpAxis::Z);
        assert_eq!(interp_axis(WallName::Right), InterpAxis::Z);
        assert_eq!(interp_axis(WallName::SlateUp), InterpAxis::Projected);
    }
}
