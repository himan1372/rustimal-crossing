//! Moving-background walls (`m_collision_bg_move.c_inc`).
//!
//! Verified against `m_collision_bg_move.c_inc`, `m_collision_bg.c`, and
//! `m_collision_bg.h` (USA Rev. 0 decomp).
//!
//! Architecture: moving objects register into a 64-slot global manager.
//! Each registered object generates FOUR vertical wall segments (not an
//! arbitrary polygon) that enter the SAME `unit_vec[128]` buffer as terrain
//! walls, after terrain but before columns. The walls carry
//! `regist_p != NULL` (with `atr_wall = FALSE`) — that pointer IS the wall
//! kind: `regist_p != NULL → MOVE`, `atr_wall → ATTRIBUTE`, else NORMAL.
//! Moving walls reuse the ordinary normal-wall solver (distance + crossing),
//! and a moving-wall hit reports a side contact (actor + wall angle) to the
//! object's contact record.
//!
//! Standing on a moving object is a SEPARATE ground test
//! (`mCoBG_GetMoveBgHeight`): broad square phase (`|dx|<dist && |dz|<dist`),
//! then an exact rotated-footprint test via two `RangeCheckLinePoint` slab
//! tests. An actor on the platform is carried next frame by
//! `wpos - last_wpos` — translation only; rotation and scale affect the
//! collision geometry but NOT the actor carry.
//!
//! Faithfulness details preserved:
//! - Wall dimensions use `mCoBG_tab_data[check_type]` (NORMAL: t0=5/t1=10,
//!   PLAYER: t0=1e-6/t1=2e-6) — not one universal epsilon.
//! - Rotation applies only when `|RAD2DEG(rad)| >= 0.05`, but
//!   `normal_angle += angleY` ALWAYS runs.
//! - `mCoBG_RotateY` sign convention: x' = x·cos + z·sin,
//!   z' = −x·sin + z·cos.
//! - The footprint ground test rotates when `rad != 0` (no 0.05 threshold)
//!   and the LAST matching registration wins the height lookup.
//! - Side/on contacts are capped at 5 with no deduplication.
//! - Registration slots are not compacted on removal — indices are slot IDs.
//!
//! The 64-entry registry and actor/contact state stay in C initially; this
//! module ports the stateless geometry kernels (Wave 1C).

/// Four-sided collision dimensions (`mCoBG_bg_size_c`).
/// Fields are asymmetric extents: right/left on X, up/down on Z.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MoveBgSize {
    pub right: f32,
    pub left: f32,
    pub up: f32,
    pub down: f32,
}

/// The six predefined size records (`mCoBG_mBg_data`, verbatim).
pub mod size_preset {
    use super::MoveBgSize;
    pub const A: MoveBgSize = MoveBgSize { right: 20.0, left: 20.0, up: 20.0, down: 20.0 };
    pub const B_0: MoveBgSize = MoveBgSize { right: 60.0, left: 20.0, up: 20.0, down: 20.0 };
    pub const B_180: MoveBgSize = MoveBgSize { right: 20.0, left: 60.0, up: 20.0, down: 20.0 };
    pub const B_270: MoveBgSize = MoveBgSize { right: 20.0, left: 20.0, up: 20.0, down: 60.0 };
    pub const B_90: MoveBgSize = MoveBgSize { right: 20.0, left: 20.0, up: 60.0, down: 20.0 };
    pub const C: MoveBgSize = MoveBgSize { right: 40.0, left: 40.0, up: 40.0, down: 40.0 };
    /// Boat collision footprint (`l_mCoBG_boat_size`): {20, 20, 40, 40}.
    pub const BOAT: MoveBgSize = MoveBgSize { right: 20.0, left: 20.0, up: 40.0, down: 40.0 };
}

/// Boat consumer constants (`mCoBG_MakeBoatCollision`).
pub mod boat {
    pub const HEIGHT: f32 = 30.0;
    pub const ATTRIBUTE_SAND: u8 = 18;
    pub const ACTIVE_DIST: f32 = 120.0;
    pub const MAX_SLOTS: usize = 2;
}

/// Moving-object transform for collision geometry.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MoveBgTransform {
    /// Current world position (walls are built from the CURRENT position,
    /// not swept between last/current).
    pub pos: [f32; 3],
    /// Y rotation as a GameCube short angle.
    pub angle_y: i16,
    /// Optional local collision offset (added before rotation).
    pub base_ofs: Option<[f32; 3]>,
    /// Collision-size scalar (1.0 when the C `scale_percent` is NULL).
    pub scale: f32,
    /// Collision top above the object position.
    pub height: f32,
}

/// GameCube short-angle → radians (0x10000 = 2π).
pub fn short_to_rad(angle_y: i16) -> f32 {
    angle_y as f32 * (core::f32::consts::TAU / 65536.0)
}

/// Degrees → GameCube short angle.
pub fn deg_to_short(deg: f32) -> i16 {
    (deg * (65536.0 / 360.0)).round() as i16
}

/// `mCoBG_RotateY` verbatim: x' = x·cos + z·sin, z' = −x·sin + z·cos.
pub fn rotate_y(p: [f32; 2], rad: f32) -> [f32; 2] {
    let (s, c) = rad.sin_cos();
    [p[0] * c + p[1] * s, -p[0] * s + p[1] * c]
}

/// One generated moving-background wall.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MoveBgWall {
    pub start: [f32; 2],
    pub end: [f32; 2],
    pub normal: [f32; 2],
    pub normal_angle: i16,
    pub wall_name: u8,
    /// Flat vertical extent: bottom = pos.y, top = pos.y + height.
    pub bottom: f32,
    pub top: f32,
}

/// Wall-name numbers (match `segment_map::WallName` discriminants).
pub mod wall_name {
    pub const UP: u8 = 0;
    pub const LEFT: u8 = 1;
    pub const DOWN: u8 = 2;
    pub const RIGHT: u8 = 3;
}

/// The check-type epsilon table (`mCoBG_tab_data`).
fn tab_t(check_type: u8) -> (f32, f32) {
    if check_type == 1 {
        (0.000001, 0.000002)
    } else {
        (5.0, 10.0)
    }
}

/// Generate the four moving-background wall segments
/// (`mCoBG_SizeData2CollisionData` core, verbatim).
///
/// Order and geometry: UP (normal (0,−1), 180°), LEFT (normal (−1,0),
/// −90°), DOWN (normal (0,1), 0°), RIGHT (normal (1,0), 90°). Normals
/// point OUTWARD from the rectangle. Rotation applies at ≥ 0.05°, but
/// `normal_angle += angleY` always runs. Base offset (when present) is
/// added before rotation; world translation after.
pub fn make_move_bg_walls(size: &MoveBgSize, t: &MoveBgTransform, check_type: u8) -> [MoveBgWall; 4] {
    let (t0, _t1) = tab_t(check_type);
    let rate = t.scale;
    let rad = short_to_rad(t.angle_y);
    let rotate = rad.to_degrees().abs() >= 0.05;

    // (start, end, normal, base_angle_short, wall_name)
    let defs: [([f32; 2], [f32; 2], [f32; 2], i16, u8); 4] = [
        (
            [-size.left * rate - t0, -size.up * rate],
            [size.right * rate + t0, -size.up * rate],
            [0.0, -1.0],
            deg_to_short(180.0),
            wall_name::UP,
        ),
        (
            [-size.left * rate, size.down * rate + t0],
            [-size.left * rate, -size.up * rate - t0],
            [-1.0, 0.0],
            deg_to_short(-90.0),
            wall_name::LEFT,
        ),
        (
            [size.right * rate + t0, size.down * rate],
            [-size.left * rate - t0, size.down * rate],
            [0.0, 1.0],
            deg_to_short(0.0),
            wall_name::DOWN,
        ),
        (
            [size.right * rate, -size.up * rate - t0],
            [size.right * rate, size.down * rate + t0],
            [1.0, 0.0],
            deg_to_short(90.0),
            wall_name::RIGHT,
        ),
    ];

    let mut out = [MoveBgWall {
        start: [0.0, 0.0],
        end: [0.0, 0.0],
        normal: [0.0, 0.0],
        normal_angle: 0,
        wall_name: 0,
        bottom: t.pos[1],
        top: t.pos[1] + t.height,
    }; 4];

    for (i, (s, e, n, base_angle, name)) in defs.into_iter().enumerate() {
        let mut start = s;
        let mut end = e;
        let mut normal = n;
        // Base offset before rotation.
        if let Some(b) = t.base_ofs {
            start[0] += b[0];
            start[1] += b[2];
            end[0] += b[0];
            end[1] += b[2];
        }
        // Rotation (thresholded).
        if rotate {
            normal = rotate_y(normal, rad);
            start = rotate_y(start, rad);
            end = rotate_y(end, rad);
        }
        // World translation after rotation.
        start[0] += t.pos[0];
        start[1] += t.pos[2];
        end[0] += t.pos[0];
        end[1] += t.pos[2];
        out[i] = MoveBgWall {
            start,
            end,
            normal,
            normal_angle: base_angle.wrapping_add(t.angle_y),
            wall_name: name,
            bottom: t.pos[1],
            top: t.pos[1] + t.height,
        };
    }
    out
}

/// `mCoBG_RangeCheckLinePoint` verbatim: the point's projection must fall
/// within the segment's slab (front-line test at both endpoints).
pub fn range_check_line_point(start: [f32; 2], end: [f32; 2], point: [f32; 2]) -> bool {
    crate::segment_map::point_info_front_line(start, point, [end[0] - start[0], end[1] - start[1]])
        && crate::segment_map::point_info_front_line(end, point, [start[0] - end[0], start[1] - end[1]])
}

/// Broad phase for the ground test (`mCoBG_JudgeMoveBgGroundCheck`):
/// `|dx| < dist && |dz| < dist`.
pub fn judge_move_bg_ground_check(base: [f32; 2], pos: [f32; 2], dist: f32) -> bool {
    (base[0] - pos[0]).abs() < dist && (base[1] - pos[1]).abs() < dist
}

/// Footprint test for one registration (`mCoBG_GetMoveBgHeight` core).
/// Returns the platform top height when the XZ point is inside the
/// (scaled, offset, rotated) footprint. Note: the ground test rotates
/// whenever `rad != 0` (no 0.05° threshold, unlike wall generation).
/// `active_dist` is the registration's activation distance.
pub fn move_bg_footprint_height(
    size: &MoveBgSize,
    t: &MoveBgTransform,
    active_dist: f32,
    pos: [f32; 2],
) -> Option<f32> {
    if !judge_move_bg_ground_check([t.pos[0], t.pos[2]], pos, active_dist) {
        return None;
    }
    let rate = t.scale;
    let rad = short_to_rad(t.angle_y);
    let mut left_up = [-size.left * rate, -size.up * rate];
    let mut left_down = [-size.left * rate, size.down * rate];
    let mut right_down = [size.right * rate, size.down * rate];
    if let Some(b) = t.base_ofs {
        for p in [&mut left_up, &mut left_down, &mut right_down] {
            p[0] += b[0];
            p[1] += b[2];
        }
    }
    if rad != 0.0 {
        left_up = rotate_y(left_up, rad);
        left_down = rotate_y(left_down, rad);
        right_down = rotate_y(right_down, rad);
    }
    for p in [&mut left_up, &mut left_down, &mut right_down] {
        p[0] += t.pos[0];
        p[1] += t.pos[2];
    }
    if range_check_line_point(left_up, left_down, pos)
        && range_check_line_point(left_down, right_down, pos)
    {
        Some(t.pos[1] + t.height)
    } else {
        None
    }
}

/// Actor-carry delta (`mCoBG_MoveActorWithMoveBg_OnMoveBg` core):
/// translation only — rotation and scale do NOT carry the actor.
pub fn move_bg_delta(current_wpos: [f32; 3], last_wpos: [f32; 3]) -> [f32; 3] {
    [
        current_wpos[0] - last_wpos[0],
        current_wpos[1] - last_wpos[1],
        current_wpos[2] - last_wpos[2],
    ]
}

/// Side contact record (`mCoBG_side_contact_c`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SideContact {
    pub name: i16,
    pub angle: i16,
}

/// Moving-object contact state: up to 5 side contacts and 5 on-contacts,
/// no deduplication (`mCoBG_bg_contact_c`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MoveBgContact {
    pub side: [Option<SideContact>; 5],
    pub side_count: usize,
    pub on: [Option<i16>; 5],
    pub on_count: usize,
}

/// `mCoBG_SetMoveBgContactSide` verbatim: capped at 5, no dedup.
pub fn set_side_contact(c: &mut MoveBgContact, actor_id: i16, angle: i16) {
    if c.side_count < 5 {
        c.side[c.side_count] = Some(SideContact { name: actor_id, angle });
        c.side_count += 1;
    }
}

/// `mCoBG_SetMoveBgContactOn` verbatim: records the actor ID only.
pub fn set_on_contact(c: &mut MoveBgContact, actor_id: i16) {
    if c.on_count < 5 {
        c.on[c.on_count] = Some(actor_id);
        c.on_count += 1;
    }
}

/// Registration slot selection (`mCoBG_RegistMoveBg` core): first free
/// slot of the 64, or `None` when full. Indices are slot IDs — the array
/// is never compacted.
pub fn register_slot(occupied: &[bool; 64]) -> Option<usize> {
    occupied.iter().position(|&o| !o)
}

// ---- C ABI ----

/// C-compatible moving-background wall.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct PcMoveBgWall {
    pub start_x: f32,
    pub start_z: f32,
    pub end_x: f32,
    pub end_z: f32,
    pub normal_x: f32,
    pub normal_z: f32,
    pub normal_angle: i16,
    pub wall_name: u8,
    pub bottom: f32,
    pub top: f32,
}

/// C ABI: generate the four moving-background walls into the caller's
/// `out[4]` buffer. `base_ofs` may be NULL. `scale` is 1.0 when the
/// registration's `scale_percent` is NULL. Returns the wall count (4).
#[no_mangle]
pub unsafe extern "C" fn pc_make_move_bg_walls(
    right: f32,
    left: f32,
    up: f32,
    down: f32,
    pos_x: f32,
    pos_y: f32,
    pos_z: f32,
    angle_y: i16,
    base_ofs: *const f32,
    scale: f32,
    height: f32,
    check_type: u8,
    out: *mut PcMoveBgWall,
) -> u8 {
    if out.is_null() {
        return 0;
    }
    let bo = if base_ofs.is_null() {
        None
    } else {
        let b = unsafe { core::slice::from_raw_parts(base_ofs, 3) };
        Some([b[0], b[1], b[2]])
    };
    let walls = make_move_bg_walls(
        &MoveBgSize { right, left, up, down },
        &MoveBgTransform { pos: [pos_x, pos_y, pos_z], angle_y, base_ofs: bo, scale, height },
        check_type,
    );
    let dst = unsafe { core::slice::from_raw_parts_mut(out, 4) };
    for (i, w) in walls.into_iter().enumerate() {
        dst[i] = PcMoveBgWall {
            start_x: w.start[0],
            start_z: w.start[1],
            end_x: w.end[0],
            end_z: w.end[1],
            normal_x: w.normal[0],
            normal_z: w.normal[1],
            normal_angle: w.normal_angle,
            wall_name: w.wall_name,
            bottom: w.bottom,
            top: w.top,
        };
    }
    4
}

/// C ABI: actor-carry translation delta; writes `out_delta[3]`.
#[no_mangle]
pub unsafe extern "C" fn pc_move_bg_delta(
    cur_x: f32,
    cur_y: f32,
    cur_z: f32,
    last_x: f32,
    last_y: f32,
    last_z: f32,
    out_delta: *mut f32,
) {
    if out_delta.is_null() {
        return;
    }
    let d = move_bg_delta([cur_x, cur_y, cur_z], [last_x, last_y, last_z]);
    let dst = unsafe { core::slice::from_raw_parts_mut(out_delta, 3) };
    dst.copy_from_slice(&d);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat_transform() -> MoveBgTransform {
        MoveBgTransform { pos: [100.0, 5.0, 200.0], angle_y: 0, base_ofs: None, scale: 1.0, height: 30.0 }
    }

    #[test]
    fn wall_geometry_unrotated() {
        // Normal check: t0 = 5.
        let walls = make_move_bg_walls(&size_preset::A, &flat_transform(), 0);
        // UP wall: (-25, -20) -> (25, -20) + world (100, 200).
        assert_eq!(walls[0].start, [75.0, 180.0]);
        assert_eq!(walls[0].end, [125.0, 180.0]);
        assert_eq!(walls[0].normal, [0.0, -1.0]);
        assert_eq!(walls[0].normal_angle, deg_to_short(180.0));
        assert_eq!(walls[0].wall_name, wall_name::UP);
        assert_eq!((walls[0].bottom, walls[0].top), (5.0, 35.0));
        // LEFT wall: (80, 225) -> (80, 175).
        assert_eq!(walls[1].start, [80.0, 225.0]);
        assert_eq!(walls[1].end, [80.0, 175.0]);
        assert_eq!(walls[1].normal, [-1.0, 0.0]);
        // DOWN wall: (125, 220) -> (75, 220).
        assert_eq!(walls[2].start, [125.0, 220.0]);
        assert_eq!(walls[2].end, [75.0, 220.0]);
        assert_eq!(walls[2].normal, [0.0, 1.0]);
        assert_eq!(walls[2].normal_angle, 0);
        // RIGHT wall: (120, 175) -> (120, 225).
        assert_eq!(walls[3].start, [120.0, 175.0]);
        assert_eq!(walls[3].end, [120.0, 225.0]);
        assert_eq!(walls[3].normal, [1.0, 0.0]);
        assert_eq!(walls[3].normal_angle, deg_to_short(90.0));
    }

    #[test]
    fn player_check_uses_tiny_epsilon() {
        let walls = make_move_bg_walls(&size_preset::A, &flat_transform(), 1);
        // t0 = 1e-6: UP start x = 100 - 20 - 1e-6.
        assert!((walls[0].start[0] - (80.0 - 0.000001)).abs() < 1e-9);
    }

    #[test]
    fn rotation_threshold_and_angle_accumulation() {
        // 90-degree rotation: geometry rotates, angle accumulates.
        let mut t = flat_transform();
        t.angle_y = deg_to_short(90.0);
        let walls = make_move_bg_walls(&size_preset::A, &t, 0);
        // UP wall normal (0,-1) rotated 90° by RotateY: x' = z*sin = -1*1 = -1... check sign convention.
        let n = walls[0].normal;
        assert!((n[0] - -1.0).abs() < 1e-5 && n[1].abs() < 1e-5, "got {n:?}");
        assert_eq!(walls[0].normal_angle, deg_to_short(180.0).wrapping_add(deg_to_short(90.0)));
        // Below 0.05°: no geometry rotation, but the angle still accumulates.
        let mut t2 = flat_transform();
        t2.angle_y = 1; // 1 short-angle step ≈ 0.0055°, below threshold
        let walls2 = make_move_bg_walls(&size_preset::A, &t2, 0);
        assert_eq!(walls2[0].normal, [0.0, -1.0]);
        assert_eq!(walls2[0].normal_angle, deg_to_short(180.0).wrapping_add(1));
        assert_eq!(walls2[0].start, [75.0, 180.0]);
    }

    #[test]
    fn scale_and_base_offset() {
        let mut t = flat_transform();
        t.scale = 2.0;
        t.base_ofs = Some([10.0, 0.0, -5.0]);
        let walls = make_move_bg_walls(&size_preset::A, &t, 0);
        // left*2 = 40, t0 = 5, base x +10: start x = 100 - 40 - 5 + 10 = 65.
        assert!((walls[0].start[0] - 65.0).abs() < 1e-5);
        // up*2 = 40, base z -5: start z = 200 - 40 - 5 = 155.
        assert!((walls[0].start[1] - 155.0).abs() < 1e-5);
    }

    #[test]
    fn carry_delta_is_translation_only() {
        assert_eq!(move_bg_delta([10.0, 5.0, 0.0], [7.0, 5.0, -2.0]), [3.0, 0.0, 2.0]);
        let mut d = [0.0f32; 3];
        unsafe { pc_move_bg_delta(10.0, 5.0, 0.0, 7.0, 5.0, -2.0, d.as_mut_ptr()) };
        assert_eq!(d, [3.0, 0.0, 2.0]);
    }

    #[test]
    fn contacts_capped_no_dedup() {
        let mut c = MoveBgContact::default();
        for i in 0..7 {
            set_side_contact(&mut c, 3, i * 10);
        }
        assert_eq!(c.side_count, 5);
        assert_eq!(c.side[4], Some(SideContact { name: 3, angle: 40 })); // same actor recorded 5x
        for _ in 0..7 {
            set_on_contact(&mut c, 9);
        }
        assert_eq!(c.on_count, 5);
    }

    #[test]
    fn registration_first_free_slot() {
        let mut occ = [false; 64];
        assert_eq!(register_slot(&occ), Some(0));
        occ[0] = true;
        occ[1] = true;
        assert_eq!(register_slot(&occ), Some(2));
        occ = [true; 64];
        assert_eq!(register_slot(&occ), None);
    }

    #[test]
    fn footprint_and_broad_phase() {
        // Broad phase: |dx| < dist && |dz| < dist.
        assert!(judge_move_bg_ground_check([0.0, 0.0], [5.0, 5.0], 10.0));
        assert!(!judge_move_bg_ground_check([0.0, 0.0], [15.0, 0.0], 10.0));
        // Slab test: point inside segment band.
        assert!(range_check_line_point([0.0, 0.0], [10.0, 0.0], [5.0, 2.0]));
        assert!(!range_check_line_point([0.0, 0.0], [10.0, 0.0], [15.0, 0.0]));
    }

    #[test]
    fn abi_writes_four_walls() {
        let mut out = [PcMoveBgWall::default(); 4];
        let n = unsafe {
            pc_make_move_bg_walls(
                20.0, 20.0, 20.0, 20.0,
                100.0, 5.0, 200.0,
                0,
                core::ptr::null(),
                1.0, 30.0,
                0,
                out.as_mut_ptr(),
            )
        };
        assert_eq!(n, 4);
        assert_eq!((out[0].start_x, out[0].start_z), (75.0, 180.0));
        assert_eq!((out[0].bottom, out[0].top), (5.0, 35.0));
        assert_eq!(unsafe { pc_make_move_bg_walls(20.0, 20.0, 20.0, 20.0, 0.0, 0.0, 0.0, 0, core::ptr::null(), 1.0, 0.0, 0, core::ptr::null_mut()) }, 0);
    }
}
