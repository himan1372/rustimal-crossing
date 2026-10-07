//! Terrain-wall generation and the bridge/water special case.
//!
//! Verified against `m_collision_bg.c`, `m_collision_bg_wall.c_inc`,
//! `m_collision_bg_water.c_inc`, and `m_collision_bg.h`.
//!
//! Pipeline (every background-collision check):
//!   raw terrain grid → 3×3/5×5/7×7 neighborhood → `mCoBG_MakeUnitVector`
//!   (slate walls, cardinal terrain walls with bridge/water modifications,
//!   attribute/forbid walls) → unit_vec[128] → moving-BG walls → columns →
//!   circle-defence walls → penetration/crossing tests → actor reverse.
//!
//! Key architecture: terrain walls are generated DYNAMICALLY from pairs of
//! neighboring `TerrainUnit`s, not from a prebuilt segment list. Each wall
//! carries geometry (start/end) SEPARATELY from its collision normal, plus
//! interpolated height bounds — a wall is "top height / bottom" with
//! potentially different heights at each endpoint.
//!
//! The bridge/water special case (the heart of this module): the generator
//! remembers `old_in_water` from the previous frame. When true, WOOD↔BRIDGE
//! boundaries get the bridge side's heights flattened to its minimum corner
//! height (slate disabled) before the ordinary wall generator runs; slope
//! walls on bridge attributes (27–35) are suppressed entirely; and the
//! ground check searches neighboring water cells through per-piece
//! direction masks. Dock/island blocks forcibly clear `old_in_water`.
//!
//! Confirmed-vs-inferred: every table and branch below is verbatim from
//! source. The *motivation* (letting the actor step from water onto a
//! bridge without hitting a phantom cliff wall) is strong inference — the
//! source has no developer comment saying so.
//!
//! Decomp-flagged bug reproduced: in `mCoBG_UtInf2NormalWallVector` the
//! `mCoBG_WALL_UP` branch never assigns `unit_vec->wall_name` (the other
//! three branches do). This port keeps that behavior — the wall_name slot
//! is left untouched for UP walls.

/// Terrain attribute numbers (from `m_collision_bg.h`).
pub mod attribute {
    pub const WATER: u8 = 8;
    pub const RIVER_NE: u8 = 17;
    pub const WOOD: u8 = 19;
    pub const SEA: u8 = 20;
    /// Bridge attributes: 27=wood NW, 28=wood SW, 29=wood SE, 30=wood NE,
    /// 31=wood center, 32=stone N, 33=stone E, 34=stone W, 35=stone S.
    pub const BRIDGE_FIRST: u8 = 27;
    pub const BRIDGE_LAST: u8 = 35;
    pub const SLATE_ATTR: u8 = 63;
}

/// Wall-generation direction bits used by `make_info` in
/// `mCoBG_MakeUnitVector`: 1=UP, 2=LEFT, 4=DOWN, 8=RIGHT.
pub mod direction_bit {
    pub const UP: u8 = 1;
    pub const LEFT: u8 = 2;
    pub const DOWN: u8 = 4;
    pub const RIGHT: u8 = 8;
}

/// Neighbor direction index (`mCoBG_DIRECT_*`): 0=N, 1=W, 2=S, 3=E,
/// 4=NW, 5=NE, 6=SE, 7=SW.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Direct {
    N = 0,
    W = 1,
    S = 2,
    E = 3,
    NW = 4,
    NE = 5,
    SE = 6,
    SW = 7,
}

/// Unit offset for a neighbor direction (`mCoBG_unit_offset` verbatim).
pub fn direct_offset(dir: Direct) -> (i32, i32) {
    match dir {
        Direct::N => (0, -1),
        Direct::W => (-1, 0),
        Direct::S => (0, 1),
        Direct::E => (1, 0),
        Direct::NW => (-1, -1),
        Direct::NE => (-1, 1),
        Direct::SE => (1, 1),
        Direct::SW => (1, -1),
    }
}

/// Wall-name numbers, matching `segment_map::WallName` discriminants.
pub mod wall_name {
    pub const UP: u8 = 0;
    pub const LEFT: u8 = 1;
    pub const DOWN: u8 = 2;
    pub const RIGHT: u8 = 3;
    pub const SLATE_UP: u8 = 4;
    pub const SLATE_DOWN: u8 = 5;
}

/// Decoded terrain unit: the five height samples as world-space offsets
/// (`collision_value * 10.0 + base_height`), the slope flag, and the
/// 6-bit terrain attribute.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TerrainUnit {
    pub left_up: f32,
    pub left_down: f32,
    pub right_down: f32,
    pub right_up: f32,
    pub center: f32,
    pub slate: bool,
    pub attribute: u8,
}

/// Decode a raw 5-bit collision height into a world offset
/// (`value * 10.0 + base_height`).
pub fn decode_height(raw5: u32, base_height: f32) -> f32 {
    (raw5 & 0x1F) as f32 * 10.0 + base_height
}

/// Wall kind: the original distinguishes normal terrain walls,
/// attribute/forbid walls, and moving-background walls through the
/// whole collision pipeline.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WallKind {
    NormalTerrain,
    Attribute,
    MovingBackground,
}

/// Interpolated height bounds at each wall endpoint
/// (`mCoBG_WallBounds_c`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WallBounds {
    pub start_top: f32,
    pub start_btm: f32,
    pub end_top: f32,
    pub end_btm: f32,
}

/// A generated collision wall vector (`mCoBG_unit_vec_info_c`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TerrainWall {
    pub start: [f32; 2],
    pub end: [f32; 2],
    pub bounds: WallBounds,
    pub normal: [f32; 2],
    /// Collision angle in degrees (C packs this to a short at its boundary).
    pub normal_angle_deg: f32,
    pub wall_name: u8,
    pub kind: WallKind,
    pub atr_wall: bool,
}

/// `mCoBG_SearchSlateDetail` verbatim: slope direction from the opposing
/// corner height values (raw 5-bit samples, not world offsets — the
/// comparison is on the packed values).
pub fn search_slate_detail(bot_right: u32, top_left: u32, top_right: u32, bot_left: u32) -> u8 {
    if bot_right != top_left {
        wall_name::SLATE_UP
    } else if top_right != bot_left {
        wall_name::SLATE_DOWN
    } else {
        wall_name::SLATE_UP
    }
}

/// `mCoBG_JudgeTopAndSet` verbatim: top gets the larger, bottom the smaller.
pub fn judge_top_and_set(y0: f32, y1: f32) -> (f32, f32) {
    if y0 >= y1 {
        (y0, y1)
    } else {
        (y1, y0)
    }
}

/// `mCoBG_SearchWallFlag` core verbatim, per wall direction.
///
/// `h0`/`h1` are the two height pairs on either side of the boundary
/// (start-endpoint pair first, then end-endpoint pair), matching the
/// source's per-direction offset comparisons. Returns the normal and
/// its angle in degrees, or `None` when no wall is generated (all
/// compared heights equal).
pub fn search_wall_flag(wall_name: u8, h0: [f32; 2], h1: [f32; 2]) -> Option<([f32; 2], f32)> {
    use crate::terrain_walls::wall_name as wn;
    match wall_name {
        wn::UP => {
            if h0[0] != h1[0] || h0[1] != h1[1] {
                if h1[0] > h0[0] || h1[1] > h0[1] {
                    Some(([0.0, 1.0], 0.0))
                } else {
                    Some(([0.0, -1.0], 180.0))
                }
            } else {
                None
            }
        }
        wn::LEFT => {
            if h0[0] != h1[0] || h0[1] != h1[1] {
                if h1[0] > h0[0] || h1[1] > h0[1] {
                    Some(([1.0, 0.0], 90.0))
                } else {
                    Some(([-1.0, 0.0], -90.0))
                }
            } else {
                None
            }
        }
        wn::DOWN => {
            if h0[0] != h1[0] || h0[1] != h1[1] {
                if h1[0] > h0[0] || h1[1] > h0[1] {
                    Some(([0.0, -1.0], 180.0))
                } else {
                    Some(([0.0, 1.0], 0.0))
                }
            } else {
                None
            }
        }
        wn::RIGHT => {
            if h0[0] != h1[0] || h0[1] != h1[1] {
                if h1[0] > h0[0] || h1[1] > h0[1] {
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

/// Is this a bridge attribute (27–35)?
pub fn is_bridge_attribute(attr: u8) -> bool {
    attr >= attribute::BRIDGE_FIRST && attr <= attribute::BRIDGE_LAST
}

/// Bridge/water policy for one boundary between two terrain units —
/// the `mCoBG_RegistNormalWallVector_AttributeOff` special case as a
/// pure kernel. `old_in_water` is the previous frame's
/// `result.is_in_water`, already passed through the dock/island rule.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WallPolicy {
    /// Ordinary terrain-wall generation.
    Normal,
    /// Flatten the FIRST unit's heights to its minimum and disable its
    /// slate flag before generating (bridge on the ut_info0 side).
    FlattenFirst,
    /// Flatten the SECOND unit's heights (bridge on the ut_info1 side).
    FlattenSecond,
    /// Do not generate a normal wall for this boundary.
    Suppress,
}

pub fn terrain_wall_policy(attr0: u8, attr1: u8, old_in_water: bool) -> WallPolicy {
    if !old_in_water {
        return WallPolicy::Normal;
    }
    let b0 = is_bridge_attribute(attr0);
    let b1 = is_bridge_attribute(attr1);
    if attr0 == attribute::WOOD && b1 {
        WallPolicy::FlattenSecond
    } else if attr1 == attribute::WOOD && b0 {
        WallPolicy::FlattenFirst
    } else if !b0 && !b1 {
        WallPolicy::Normal
    } else {
        WallPolicy::Suppress
    }
}

/// The bridge-height flattening: copy the unit, take the minimum of
/// the four corner offsets, disable slate, set all corners to the
/// minimum — then the ordinary wall generator sees this unit.
pub fn flatten_bridge_unit(unit: &TerrainUnit) -> TerrainUnit {
    let min_h = unit
        .left_down
        .min(unit.left_up)
        .min(unit.right_down)
        .min(unit.right_up);
    TerrainUnit {
        left_up: min_h,
        left_down: min_h,
        right_down: min_h,
        right_up: min_h,
        center: unit.center,
        slate: false,
        attribute: unit.attribute,
    }
}

/// Slate-wall suppression: when the actor was in water, no slope wall
/// is generated for bridge attributes
/// (`mCoBG_RegistSlatingWallVector_AttributeOff_Slate_OldInWater`).
pub fn slate_wall_suppressed(attribute: u8, old_in_water: bool) -> bool {
    old_in_water && is_bridge_attribute(attribute)
}

/// Dock/island block rule (`mCoBG_MakeUnitVector`): blocks classified
/// DOCK or ISLAND forcibly clear `old_in_water` before wall generation.
pub fn apply_block_water_rule(is_dock_or_island: bool, old_in_water: bool) -> bool {
    if is_dock_or_island {
        false
    } else {
        old_in_water
    }
}

/// Per-bridge-piece water-search masks (`mCoBG_bridge_search_water`,
/// verbatim). Bits index `Direct` (0=N … 7=SW); index is
/// `attribute - 27`, valid for 27–35.
const BRIDGE_SEARCH_WATER: [u8; 9] = [3, 6, 12, 9, 240, 1, 8, 2, 4];

pub fn bridge_search_water_mask(attribute: u8) -> Option<u8> {
    if is_bridge_attribute(attribute) {
        Some(BRIDGE_SEARCH_WATER[(attribute - attribute::BRIDGE_FIRST) as usize])
    } else {
        None
    }
}

/// Quarter-based wood/water interpretation of bridge attributes
/// (`mCoBG_woodb_water_info`, verbatim). Only attributes 27–31 have
/// non-trivial rows (32–35 map to the all-WOOD row); `quarter` is the
/// terrain-unit quarter the actor is in (0–3).
const WOODB_WATER_INFO: [[u8; 4]; 6] = [
    [12, 12, 19, 19], // 27 wood bridge NW: RIVER_NW, RIVER_NW, WOOD, WOOD
    [19, 16, 16, 19], // 28 wood bridge SW: WOOD, RIVER_SW, RIVER_SW, WOOD
    [19, 19, 17, 17], // 29 wood bridge SE: WOOD, WOOD, RIVER_SE, RIVER_SE
    [13, 19, 19, 13], // 30 wood bridge NE: RIVER_NE, WOOD, WOOD, RIVER_NE
    [19, 19, 19, 19], // 31 wood bridge center: all WOOD
    [19, 19, 19, 19], // 32 stone bridge N: all WOOD
];

/// Attribute numbers used above: 12=RIVER_NW, 13=RIVER_NE, 16=RIVER_SW,
/// 17=RIVER_SE, 19=WOOD.
pub fn bridge_quarter_attribute(bridge_attr: u8, quarter: usize) -> Option<u8> {
    if (attribute::BRIDGE_FIRST..=32).contains(&bridge_attr) {
        Some(WOODB_WATER_INFO[(bridge_attr - attribute::BRIDGE_FIRST) as usize][quarter.min(3)])
    } else {
        None
    }
}

/// Slate-wall normal/bounds core (`mCoBG_GetUnitVecInf_SlatingWall`).
/// Returns (normal, angle_deg, bounds). The segment placement
/// (`UnitNoName2StartEnd`) is the caller's job.
pub fn slate_wall_geometry(unit: &TerrainUnit, slate_detail: u8) -> ([f32; 2], f32, WallBounds) {
    use crate::terrain_walls::wall_name as wn;
    const S: f32 = core::f32::consts::FRAC_1_SQRT_2;
    if slate_detail == wn::SLATE_UP {
        if unit.left_up > unit.right_down {
            let b = WallBounds {
                start_top: unit.left_up,
                start_btm: unit.right_down,
                end_top: unit.left_up,
                end_btm: unit.right_down,
            };
            ([S, S], 45.0, b)
        } else {
            let b = WallBounds {
                start_top: unit.right_down,
                start_btm: unit.left_up,
                end_top: unit.right_down,
                end_btm: unit.left_up,
            };
            ([-S, -S], -135.0, b)
        }
    } else {
        if unit.left_down > unit.right_up {
            let b = WallBounds {
                start_top: unit.left_down,
                start_btm: unit.right_up,
                end_top: unit.left_down,
                end_btm: unit.right_up,
            };
            ([S, -S], 135.0, b)
        } else {
            let b = WallBounds {
                start_top: unit.right_up,
                start_btm: unit.left_down,
                end_top: unit.right_up,
                end_btm: unit.left_down,
            };
            ([-S, S], -45.0, b)
        }
    }
}

/// Build a cardinal wall from two neighboring units
/// (`mCoBG_UtInf2NormalWallVector` core, without the capacity check).
///
/// `unit0` is the current unit, `unit1` the neighbor; the per-direction
/// height pairs follow the source exactly. `check_type` selects the
/// normal/player segment placement. `existing_wall_name` is the
/// current slot's wall_name — the source only overwrites it for
/// LEFT/DOWN/RIGHT (the UP branch's missing assignment is a
/// decomp-flagged @BUG, reproduced here).
pub fn cardinal_wall_from_units(
    unit0: &TerrainUnit,
    unit1: &TerrainUnit,
    ux: f32,
    uz: f32,
    wall_name: u8,
    check_type: crate::segment_map::CheckType,
    existing_wall_name: u8,
) -> Option<TerrainWall> {
    use crate::segment_map::WallName;
    use crate::terrain_walls::wall_name as wn;
    let (h0, h1): ([f32; 2], [f32; 2]) = match wall_name {
        wn::UP => ([unit0.left_up, unit0.right_up], [unit1.left_down, unit1.right_down]),
        wn::LEFT => ([unit0.left_up, unit0.left_down], [unit1.right_up, unit1.right_down]),
        wn::DOWN => ([unit0.left_down, unit0.right_down], [unit1.left_up, unit1.right_up]),
        wn::RIGHT => ([unit0.right_up, unit0.right_down], [unit1.left_up, unit1.left_down]),
        _ => return None,
    };
    let (normal, angle) = search_wall_flag(wall_name, h0, h1)?;
    let (s0t, s0b) = judge_top_and_set(h0[0], h1[0]);
    let (e0t, e0b) = judge_top_and_set(h0[1], h1[1]);
    let name = match wall_name {
        wn::UP => WallName::Up,
        wn::LEFT => WallName::Left,
        wn::DOWN => WallName::Down,
        _ => WallName::Right,
    };
    let (start, end) = crate::segment_map::unit_no_name_2_start_end(ux, uz, name, check_type);
    // Faithful @BUG: UP does not overwrite wall_name.
    let final_name = if wall_name == wn::UP { existing_wall_name } else { wall_name };
    Some(TerrainWall {
        start,
        end,
        bounds: WallBounds { start_top: s0t, start_btm: s0b, end_top: e0t, end_btm: e0b },
        normal,
        normal_angle_deg: angle,
        wall_name: final_name,
        kind: WallKind::NormalTerrain,
        atr_wall: false,
    })
}

// ---- C ABI ----

/// C ABI: wall policy for a unit boundary.
/// 0=Normal, 1=FlattenFirst, 2=FlattenSecond, 3=Suppress.
#[no_mangle]
pub extern "C" fn pc_terrain_wall_policy(attr0: u8, attr1: u8, old_in_water: u8) -> u8 {
    match terrain_wall_policy(attr0, attr1, old_in_water != 0) {
        WallPolicy::Normal => 0,
        WallPolicy::FlattenFirst => 1,
        WallPolicy::FlattenSecond => 2,
        WallPolicy::Suppress => 3,
    }
}

/// C ABI: bridge water-search mask, or 0 when not a bridge attribute.
#[no_mangle]
pub extern "C" fn pc_bridge_search_water_mask(attribute: u8) -> u8 {
    bridge_search_water_mask(attribute).unwrap_or(0)
}

/// C ABI: bridge quarter attribute, or 0xFF when not applicable.
#[no_mangle]
pub extern "C" fn pc_bridge_quarter_attribute(bridge_attr: u8, quarter: u8) -> u8 {
    bridge_quarter_attribute(bridge_attr, quarter as usize).unwrap_or(0xFF)
}

/// C ABI: slate detail for raw corner samples. Returns 4 (SLATE_UP) or 5 (SLATE_DOWN).
#[no_mangle]
pub extern "C" fn pc_search_slate_detail(bot_right: u32, top_left: u32, top_right: u32, bot_left: u32) -> u8 {
    search_slate_detail(bot_right, top_left, top_right, bot_left)
}

/// C ABI: wall-flag normal selection. Writes normal[2] and angle_deg;
/// returns 1 when a wall is generated, 0 otherwise.
#[no_mangle]
pub unsafe extern "C" fn pc_search_wall_flag(
    wall_name: u8,
    h0s: f32,
    h0e: f32,
    h1s: f32,
    h1e: f32,
    out_normal: *mut f32,
    out_angle_deg: *mut f32,
) -> u8 {
    match search_wall_flag(wall_name, [h0s, h0e], [h1s, h1e]) {
        Some((n, a)) => {
            if !out_normal.is_null() {
                let d = unsafe { core::slice::from_raw_parts_mut(out_normal, 2) };
                d[0] = n[0];
                d[1] = n[1];
            }
            if !out_angle_deg.is_null() {
                unsafe { *out_angle_deg = a };
            }
            1
        }
        None => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terrain_walls::wall_name as wn;

    fn unit(attr: u8, lu: f32, ld: f32, rd: f32, ru: f32) -> TerrainUnit {
        TerrainUnit { left_up: lu, left_down: ld, right_down: rd, right_up: ru, center: 0.0, slate: false, attribute: attr }
    }

    #[test]
    fn wall_flag_normals_per_direction() {
        // UP: neighbor higher -> +Z/0deg.
        assert_eq!(search_wall_flag(wn::UP, [10.0, 10.0], [30.0, 30.0]), Some(([0.0, 1.0], 0.0)));
        // UP: current higher -> -Z/180deg.
        assert_eq!(search_wall_flag(wn::UP, [30.0, 30.0], [10.0, 10.0]), Some(([0.0, -1.0], 180.0)));
        // UP: equal heights -> no wall.
        assert_eq!(search_wall_flag(wn::UP, [10.0, 10.0], [10.0, 10.0]), None);
        // One endpoint differing is enough.
        assert!(search_wall_flag(wn::UP, [10.0, 10.0], [10.0, 30.0]).is_some());
        // LEFT: neighbor higher -> +X/90deg.
        assert_eq!(search_wall_flag(wn::LEFT, [5.0, 5.0], [9.0, 9.0]), Some(([1.0, 0.0], 90.0)));
        assert_eq!(search_wall_flag(wn::LEFT, [9.0, 9.0], [5.0, 5.0]), Some(([-1.0, 0.0], -90.0)));
        // DOWN: neighbor higher -> -Z/180deg.
        assert_eq!(search_wall_flag(wn::DOWN, [5.0, 5.0], [9.0, 9.0]), Some(([0.0, -1.0], 180.0)));
        // RIGHT: neighbor higher -> -X/-90deg.
        assert_eq!(search_wall_flag(wn::RIGHT, [5.0, 5.0], [9.0, 9.0]), Some(([-1.0, 0.0], -90.0)));
        assert_eq!(search_wall_flag(wn::RIGHT, [9.0, 9.0], [5.0, 5.0]), Some(([1.0, 0.0], 90.0)));
    }

    #[test]
    fn slate_detail_rules() {
        assert_eq!(search_slate_detail(1, 2, 0, 0), wn::SLATE_UP); // bot_right != top_left
        assert_eq!(search_slate_detail(1, 1, 2, 0), wn::SLATE_DOWN); // top_right != bot_left
        assert_eq!(search_slate_detail(1, 1, 1, 1), wn::SLATE_UP); // all equal -> UP
    }

    #[test]
    fn judge_top_and_set_orders() {
        assert_eq!(judge_top_and_set(30.0, 10.0), (30.0, 10.0));
        assert_eq!(judge_top_and_set(10.0, 30.0), (30.0, 10.0));
        assert_eq!(judge_top_and_set(10.0, 10.0), (10.0, 10.0)); // y0 >= y1 -> (y0, y1)
    }

    #[test]
    fn bridge_policy_matrix() {
        use attribute::*;
        // No water history: always normal.
        assert_eq!(terrain_wall_policy(WOOD, BRIDGE_FIRST, false), WallPolicy::Normal);
        assert_eq!(terrain_wall_policy(0, 0, false), WallPolicy::Normal);
        // WOOD <-> bridge: flatten the bridge side.
        assert_eq!(terrain_wall_policy(WOOD, 27, true), WallPolicy::FlattenSecond);
        assert_eq!(terrain_wall_policy(35, WOOD, true), WallPolicy::FlattenFirst);
        assert_eq!(terrain_wall_policy(WOOD, 35, true), WallPolicy::FlattenSecond);
        // Bridge <-> bridge: suppress.
        assert_eq!(terrain_wall_policy(27, 31, true), WallPolicy::Suppress);
        // Bridge <-> ordinary non-wood: suppress.
        assert_eq!(terrain_wall_policy(27, 0, true), WallPolicy::Suppress);
        // Ordinary <-> ordinary: normal.
        assert_eq!(terrain_wall_policy(0, 1, true), WallPolicy::Normal);
        // WOOD <-> WOOD: normal (neither is a bridge).
        assert_eq!(terrain_wall_policy(WOOD, WOOD, true), WallPolicy::Normal);
        // C ABI codes.
        assert_eq!(pc_terrain_wall_policy(WOOD, 27, 1), 2);
        assert_eq!(pc_terrain_wall_policy(27, WOOD, 1), 1);
        assert_eq!(pc_terrain_wall_policy(27, 31, 1), 3);
        assert_eq!(pc_terrain_wall_policy(0, 0, 1), 0);
    }

    #[test]
    fn bridge_flattening() {
        let u = unit(27, 40.0, 30.0, 50.0, 20.0);
        let f = flatten_bridge_unit(&u);
        assert_eq!((f.left_up, f.left_down, f.right_down, f.right_up), (20.0, 20.0, 20.0, 20.0));
        assert!(!f.slate);
        assert_eq!(f.attribute, 27); // attribute preserved
    }

    #[test]
    fn slate_suppression_and_block_rule() {
        assert!(slate_wall_suppressed(27, true));
        assert!(slate_wall_suppressed(35, true));
        assert!(!slate_wall_suppressed(27, false));
        assert!(!slate_wall_suppressed(26, true));
        assert!(!slate_wall_suppressed(36, true));
        assert!(!apply_block_water_rule(true, true)); // dock/island clears
        assert!(apply_block_water_rule(false, true));
        assert!(!apply_block_water_rule(false, false));
    }

    #[test]
    fn bridge_search_masks() {
        // Bits: 0=N,1=W,2=S,3=E,4=NW,5=NE,6=SE,7=SW.
        assert_eq!(bridge_search_water_mask(27), Some(3));   // NW -> N+W
        assert_eq!(bridge_search_water_mask(28), Some(6));   // SW -> W+S
        assert_eq!(bridge_search_water_mask(29), Some(12));  // SE -> S+E
        assert_eq!(bridge_search_water_mask(30), Some(9));   // NE -> N+E
        assert_eq!(bridge_search_water_mask(31), Some(240)); // center -> diagonals
        assert_eq!(bridge_search_water_mask(32), Some(1));   // N
        assert_eq!(bridge_search_water_mask(33), Some(8));   // E
        assert_eq!(bridge_search_water_mask(34), Some(2));   // W
        assert_eq!(bridge_search_water_mask(35), Some(4));   // S
        assert_eq!(bridge_search_water_mask(26), None);
        assert_eq!(pc_bridge_search_water_mask(31), 240);
    }

    #[test]
    fn bridge_quarter_table() {
        use attribute::*;
        assert_eq!(bridge_quarter_attribute(27, 0), Some(12)); // RIVER_NW
        assert_eq!(bridge_quarter_attribute(27, 2), Some(WOOD));
        assert_eq!(bridge_quarter_attribute(28, 1), Some(16)); // RIVER_SW
        assert_eq!(bridge_quarter_attribute(29, 3), Some(17)); // RIVER_SE
        assert_eq!(bridge_quarter_attribute(30, 0), Some(13)); // RIVER_NE
        assert_eq!(bridge_quarter_attribute(31, 2), Some(WOOD));
        assert_eq!(bridge_quarter_attribute(32, 0), Some(WOOD));
        assert_eq!(bridge_quarter_attribute(33, 0), None); // beyond row 5
        assert_eq!(pc_bridge_quarter_attribute(27, 0), 12);
        assert_eq!(pc_bridge_quarter_attribute(99, 0), 0xFF);
    }

    #[test]
    fn cardinal_wall_construction() {
        use crate::segment_map::CheckType;
        // UP wall: current unit high side at 10, neighbor at 30 -> normal +Z.
        let u0 = unit(0, 10.0, 10.0, 10.0, 10.0);
        let u1 = unit(0, 30.0, 30.0, 30.0, 30.0);
        let w = cardinal_wall_from_units(&u0, &u1, 1.0, 1.0, wn::UP, CheckType::Normal, 0xAA).unwrap();
        assert_eq!(w.normal, [0.0, 1.0]);
        assert_eq!(w.normal_angle_deg, 0.0);
        assert_eq!((w.bounds.start_top, w.bounds.start_btm), (30.0, 10.0));
        assert!(!w.atr_wall);
        assert_eq!(w.kind, WallKind::NormalTerrain);
        // @BUG reproduced: UP keeps the pre-existing wall_name.
        assert_eq!(w.wall_name, 0xAA);
        // LEFT overwrites wall_name.
        let w = cardinal_wall_from_units(&u0, &u1, 1.0, 1.0, wn::LEFT, CheckType::Normal, 0xAA).unwrap();
        assert_eq!(w.wall_name, wn::LEFT);
        // Equal heights -> no wall.
        assert!(cardinal_wall_from_units(&u0, &u0, 1.0, 1.0, wn::UP, CheckType::Normal, 0).is_none());
    }

    #[test]
    fn slate_wall_normals() {
        let u = unit(0, 40.0, 10.0, 10.0, 10.0);
        let (n, a, b) = slate_wall_geometry(&u, wn::SLATE_UP);
        let s = core::f32::consts::FRAC_1_SQRT_2;
        assert!((n[0] - s).abs() < 1e-6 && (n[1] - s).abs() < 1e-6);
        assert_eq!(a, 45.0);
        assert_eq!((b.start_top, b.start_btm), (40.0, 10.0));
        let u = unit(0, 10.0, 10.0, 10.0, 40.0);
        let (n, a, _) = slate_wall_geometry(&u, wn::SLATE_DOWN);
        assert!((n[0] + s).abs() < 1e-6 && (n[1] - s).abs() < 1e-6);
        assert_eq!(a, -45.0);
    }

    #[test]
    fn decode_height_and_direct_offsets() {
        assert_eq!(decode_height(2, 100.0), 120.0);
        assert_eq!(direct_offset(Direct::N), (0, -1));
        assert_eq!(direct_offset(Direct::E), (1, 0));
        assert_eq!(direct_offset(Direct::SW), (1, -1));
    }
}
