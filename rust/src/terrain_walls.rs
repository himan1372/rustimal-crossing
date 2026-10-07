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

/// Terrain attribute numbers (from `m_collision_bg.h`, enum
/// `background_attribute`; GRASS0 = 0).
pub mod attribute {
    pub const GRASS0: u8 = 0;
    pub const STONE: u8 = 7;
    pub const WATER: u8 = 12;
    pub const WATERFALL: u8 = 13;
    pub const RIVER_N: u8 = 14;
    pub const RIVER_NW: u8 = 15;
    pub const RIVER_W: u8 = 16;
    pub const RIVER_SW: u8 = 17;
    pub const RIVER_S: u8 = 18;
    pub const RIVER_SE: u8 = 19;
    pub const RIVER_E: u8 = 20;
    pub const RIVER_NE: u8 = 21;
    pub const SAND: u8 = 22;
    pub const WOOD: u8 = 23;
    pub const SEA: u8 = 24;
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
    [15, 15, 23, 23], // 27 wood bridge NW: RIVER_NW, RIVER_NW, WOOD, WOOD
    [23, 17, 17, 23], // 28 wood bridge SW: WOOD, RIVER_SW, RIVER_SW, WOOD
    [23, 23, 19, 19], // 29 wood bridge SE: WOOD, WOOD, RIVER_SE, RIVER_SE
    [21, 23, 23, 21], // 30 wood bridge NE: RIVER_NE, WOOD, WOOD, RIVER_NE
    [23, 23, 23, 23], // 31 wood bridge center: all WOOD
    [23, 23, 23, 23], // 32 stone bridge N: all WOOD
];

/// Attribute numbers used above: 15=RIVER_NW, 21=RIVER_NE, 17=RIVER_SW,
/// 19=RIVER_SE, 23=WOOD.
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

/// The nine bridge attributes as a named enum.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum BridgeAttribute {
    WoodNW = 27,
    WoodSW = 28,
    WoodSE = 29,
    WoodNE = 30,
    WoodCenter = 31,
    StoneN = 32,
    StoneE = 33,
    StoneW = 34,
    StoneS = 35,
}

/// Water/river classification range used by the bridge water search.
pub fn is_water_attribute(attr: u8) -> bool {
    attr >= attribute::WATER && attr <= attribute::RIVER_NE
}

/// `mCoBG_unit_attribute_water_info` verbatim (64 entries): water/river
/// attributes map to themselves; wood-bridge corners 27–30 and river
/// banks 39–42 map to their river corners; everything else → GRASS0.
pub const UNIT_ATTRIBUTE_WATER_INFO: [u8; 64] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // 0-11
    12, 13, 14, 15, 16, 17, 18, 19, 20, 21, // 12-21: WATER..RIVER_NE
    0, 0, 0, 0, 0, // 22-26
    15, 17, 19, 21, // 27-30: wood bridge corners -> river corners
    0, 0, 0, 0, 0, 0, 0, 0, // 31-38
    15, 17, 19, 21, // 39-42: river banks -> river corners
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // 43-63
];

/// `mCoBG_SearchWaterAttributeFrom4Area` core: map a raw neighbor
/// attribute through the water table.
pub fn search_water_attribute(raw_attr: u8) -> u8 {
    UNIT_ATTRIBUTE_WATER_INFO[(raw_attr as usize).min(63)]
}

/// Unit-area classification (`mCoBG_GetUnitArea`, verbatim): the
/// triangle of the unit the local position falls in. Matches the
/// header enum order AREA_N=0, AREA_W=1, AREA_S=2, AREA_E=3.
pub fn get_unit_area(x: f32, z: f32) -> u8 {
    if x < z {
        if z > -x {
            2 // AREA_S
        } else {
            1 // AREA_W
        }
    } else if z > -x {
        3 // AREA_E
    } else {
        0 // AREA_N
    }
}

/// Position-to-attribute bridge branch (`mCoBG_Wpos2Attribute`):
/// wood bridges (27–31) use the area-dependent water/wood table;
/// stone bridges (32–35) resolve to STONE. This is why the bridge
/// family is NOT uniform in attribute lookup even though the wall
/// policy treats 27–35 identically.
pub fn bridge_wpos_attribute(bridge_attr: u8, area: u8) -> Option<u8> {
    if (27..=31).contains(&bridge_attr) {
        bridge_quarter_attribute(bridge_attr, area as usize)
    } else if (32..=35).contains(&bridge_attr) {
        Some(attribute::STONE)
    } else {
        None
    }
}

/// Bridge ground/water search kernel — the `mCoBG_GroundCheck` bridge
/// branch (m_collision_bg.c:1730) as a pure function.
///
/// Runs only when `old_in_water && !attribute_wall && is_bridge(attr)`.
/// Scans directions 0..8 in order, masked by `bridge_search_water`;
/// each neighbor's RAW attribute is mapped through the water table,
/// and the FIRST water/river result wins (direction order is
/// behaviorally significant — do NOT reorder or use `any()`).
/// Returns the selected water attribute, or `None` when no water is
/// found (caller then falls back to `Wpos2Attribute`).
pub fn bridge_water_search(
    bridge_attr: u8,
    old_in_water: bool,
    attribute_wall: bool,
    neighbor_attrs: [u8; 8],
) -> Option<u8> {
    if attribute_wall || !old_in_water || !is_bridge_attribute(bridge_attr) {
        return None;
    }
    let mask = bridge_search_water_mask(bridge_attr).unwrap_or(0);
    for dir in 0..8u8 {
        if mask & (1 << dir) != 0 {
            let water_attr = search_water_attribute(neighbor_attrs[dir as usize]);
            if is_water_attribute(water_attr) {
                return Some(water_attr);
            }
        }
    }
    None
}

/// Positive form of the slate rule (the C code early-returns; this
/// reads as "should a slate wall be made").
pub fn bridge_should_make_slate(attribute: u8, old_in_water: bool) -> bool {
    !slate_wall_suppressed(attribute, old_in_water)
}

//
// ---- Cardinal edge construction ----
//

// Canonical edge-ownership tables (`l_make33/55/77_coldata`, verbatim).
// Bit 0=UP, 1=LEFT, 2=DOWN, 3=RIGHT. Only UP/LEFT bits are ever set:
// each shared edge is constructed once, from the canonical side.
const MAKE_33_COLDATA: [u8; 9] = [
    0x00, 0x02, 0x02,
    0x01, 0x03, 0x03,
    0x01, 0x03, 0x03,
];
const MAKE_55_COLDATA: [u8; 25] = [
    0x00, 0x02, 0x02, 0x02, 0x02,
    0x01, 0x03, 0x03, 0x03, 0x03,
    0x01, 0x03, 0x03, 0x03, 0x03,
    0x01, 0x03, 0x03, 0x03, 0x03,
    0x01, 0x03, 0x03, 0x03, 0x03,
];
const MAKE_77_COLDATA: [u8; 49] = [
    0x00, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02,
    0x01, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03,
    0x01, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03,
    0x01, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03,
    0x01, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03,
    0x01, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03,
    0x01, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03,
];

/// Edge-ownership mask for a neighborhood cell
/// (`mCoBG_GetUnitInfSearchData` + table lookup). Unknown sizes fall
/// back to the 3×3 table, exactly like the source's `default` case.
pub fn cardinal_edge_mask(size: usize, index: usize) -> u8 {
    let (table, len) = match size {
        5 => (&MAKE_55_COLDATA[..], 25),
        7 => (&MAKE_77_COLDATA[..], 49),
        _ => (&MAKE_33_COLDATA[..], 9),
    };
    if index < len {
        table[index]
    } else {
        0
    }
}

/// Neighbor anchor for a cardinal edge in a row-major neighborhood
/// (`mCoBG_MakeUnitVector` call pattern): UP = index − size,
/// LEFT = index − 1, DOWN = index + size, RIGHT = index + 1.
/// `None` on underflow (the source relies on the padded neighborhood).
pub fn cardinal_neighbor_index(index: usize, size: usize, wall_name: u8) -> Option<usize> {
    match wall_name {
        wall_name::UP => index.checked_sub(size),
        wall_name::LEFT => index.checked_sub(1),
        wall_name::DOWN => index.checked_add(size),
        wall_name::RIGHT => index.checked_add(1),
        _ => None,
    }
}

/// Explicit edge-existence test: the two units' corresponding edge
/// corners, per direction (the `mCoBG_SearchWallFlag` difference test
/// without the normal selection).
pub fn cardinal_edge_exists(unit0: &TerrainUnit, unit1: &TerrainUnit, wall_name: u8) -> bool {
    match wall_name {
        wall_name::UP => {
            unit0.left_up != unit1.left_down || unit0.right_up != unit1.right_down
        }
        wall_name::LEFT => {
            unit0.left_up != unit1.right_up || unit0.left_down != unit1.right_down
        }
        wall_name::DOWN => {
            unit0.left_down != unit1.left_up || unit0.right_down != unit1.right_up
        }
        wall_name::RIGHT => {
            unit0.right_up != unit1.left_up || unit0.right_down != unit1.left_down
        }
        _ => false,
    }
}

/// Slate-unit corner adjustment (`mCoBG_UtInf2NormalSlateWallVector`
/// verbatim): when exactly one side of the boundary is sloped, the
/// slate unit is copied and one corner is overwritten from its diagonal
/// partner so the cardinal wall meets the slope consistently.
/// `slate_is_unit1` selects which side is the slate unit.
pub fn adjust_slate_unit_for_cardinal(
    unit: &TerrainUnit,
    slate_detail: u8,
    wall_name: u8,
    slate_is_unit1: bool,
) -> TerrainUnit {
    let mut tmp = *unit;
    let up = slate_detail == wall_name::SLATE_UP;
    if slate_is_unit1 {
        match wall_name {
            wall_name::UP => {
                if up {
                    tmp.left_down = tmp.right_down;
                } else {
                    tmp.right_down = tmp.left_down;
                }
            }
            wall_name::LEFT => {
                if up {
                    tmp.right_up = tmp.right_down;
                } else {
                    tmp.right_down = tmp.right_up;
                }
            }
            wall_name::DOWN => {
                if up {
                    tmp.right_up = tmp.left_up;
                } else {
                    tmp.left_up = tmp.right_up;
                }
            }
            wall_name::RIGHT => {
                if up {
                    tmp.left_down = tmp.left_up;
                } else {
                    tmp.left_up = tmp.left_down;
                }
            }
            _ => {}
        }
    } else {
        match wall_name {
            wall_name::UP => {
                if up {
                    tmp.right_up = tmp.left_up;
                } else {
                    tmp.left_up = tmp.right_up;
                }
            }
            wall_name::LEFT => {
                if up {
                    tmp.left_down = tmp.left_up;
                } else {
                    tmp.left_up = tmp.left_down;
                }
            }
            wall_name::DOWN => {
                if up {
                    tmp.left_down = tmp.right_down;
                } else {
                    tmp.right_down = tmp.left_down;
                }
            }
            wall_name::RIGHT => {
                if up {
                    tmp.right_down = tmp.right_up;
                } else {
                    tmp.right_up = tmp.right_down;
                }
            }
            _ => {}
        }
    }
    tmp
}

/// Height interpolation along a cardinal wall (`mCoBG_CheckHeightExactly`
/// verbatim, cardinal branches): LEFT/RIGHT interpolate along Z,
/// UP/DOWN along X, using the formula
/// `start + (point − start) * ((end − start) / (end − start))` with a
/// zero-division guard. Moving walls (`is_move_bg`) use the end
/// bounds directly. Returns `Some((top, bot))` when
/// `pos_y + 3.0 <= top`, else `None`.
pub fn check_height_exactly(
    bounds: &WallBounds,
    start: [f32; 2],
    end: [f32; 2],
    wall_name: u8,
    pos_y: f32,
    point: [f32; 2],
    is_move_bg: bool,
) -> Option<(f32, f32)> {
    if is_move_bg {
        let (top, bot) = (bounds.end_top, bounds.end_btm);
        return if pos_y + 3.0 <= top { Some((top, bot)) } else { None };
    }
    let (axis, p, s) = match wall_name {
        wall_name::LEFT | wall_name::RIGHT => (end[1] - start[1], point[1], start[1]),
        wall_name::UP | wall_name::DOWN => (end[0] - start[0], point[0], start[0]),
        _ => return None,
    };
    if axis == 0.0 {
        return None;
    }
    let top = bounds.start_top + (p - s) * ((bounds.end_top - bounds.start_top) / axis);
    let bot = bounds.start_btm + (p - s) * ((bounds.end_btm - bounds.start_btm) / axis);
    if pos_y + 3.0 <= top {
        Some((top, bot))
    } else {
        None
    }
}

/// C ABI: edge-ownership mask for a neighborhood cell.
#[no_mangle]
pub extern "C" fn pc_cardinal_edge_mask(size: u8, index: u8) -> u8 {
    cardinal_edge_mask(size as usize, index as usize)
}

/// C ABI: cardinal height test; writes top/bot, returns 1 when the
/// height gate passes.
#[no_mangle]
pub unsafe extern "C" fn pc_check_height_exactly(
    start_top: f32,
    start_btm: f32,
    end_top: f32,
    end_btm: f32,
    start_x: f32,
    start_z: f32,
    end_x: f32,
    end_z: f32,
    wall_name: u8,
    pos_y: f32,
    point_x: f32,
    point_z: f32,
    is_move_bg: u8,
    out_top: *mut f32,
    out_bot: *mut f32,
) -> u8 {
    let bounds = WallBounds { start_top, start_btm, end_top, end_btm };
    match check_height_exactly(
        &bounds,
        [start_x, start_z],
        [end_x, end_z],
        wall_name,
        pos_y,
        [point_x, point_z],
        is_move_bg != 0,
    ) {
        Some((top, bot)) => {
            if !out_top.is_null() {
                unsafe { *out_top = top };
            }
            if !out_bot.is_null() {
                unsafe { *out_bot = bot };
            }
            1
        }
        None => 0,
    }
}

/// C ABI: bridge water search; takes 8 raw neighbor attributes in
/// Direct order (0=N..7=SW); returns the selected water attribute,
/// or 0xFF when the search does not run or finds no water.
#[no_mangle]
pub unsafe extern "C" fn pc_bridge_water_search(
    bridge_attr: u8,
    old_in_water: u8,
    attribute_wall: u8,
    neighbors: *const u8,
) -> u8 {
    if neighbors.is_null() {
        return 0xFF;
    }
    let n = unsafe { core::slice::from_raw_parts(neighbors, 8) };
    let mut arr = [0u8; 8];
    arr.copy_from_slice(n);
    bridge_water_search(bridge_attr, old_in_water != 0, attribute_wall != 0, arr).unwrap_or(0xFF)
}

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
        assert_eq!(bridge_quarter_attribute(27, 0), Some(15)); // RIVER_NW
        assert_eq!(bridge_quarter_attribute(27, 2), Some(WOOD));
        assert_eq!(bridge_quarter_attribute(28, 1), Some(17)); // RIVER_SW
        assert_eq!(bridge_quarter_attribute(29, 3), Some(19)); // RIVER_SE
        assert_eq!(bridge_quarter_attribute(30, 0), Some(21)); // RIVER_NE
        assert_eq!(bridge_quarter_attribute(31, 2), Some(WOOD));
        assert_eq!(bridge_quarter_attribute(32, 0), Some(WOOD));
        assert_eq!(bridge_quarter_attribute(33, 0), None); // beyond row 5
        assert_eq!(pc_bridge_quarter_attribute(27, 0), 15);
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
    fn bridge_wpos_attribute_branches() {
        use attribute::*;
        // Wood bridges: area-dependent.
        assert_eq!(bridge_wpos_attribute(27, 0), Some(RIVER_NW));
        assert_eq!(bridge_wpos_attribute(31, 3), Some(WOOD));
        // Stone bridges: always STONE, regardless of area.
        assert_eq!(bridge_wpos_attribute(32, 0), Some(STONE));
        assert_eq!(bridge_wpos_attribute(35, 3), Some(STONE));
        assert_eq!(bridge_wpos_attribute(26, 0), None);
    }

    #[test]
    fn water_table_and_search() {
        use attribute::*;
        // Water attrs map to themselves; ordinary attrs to GRASS0.
        assert_eq!(search_water_attribute(WATER), WATER);
        assert_eq!(search_water_attribute(RIVER_NE), RIVER_NE);
        assert_eq!(search_water_attribute(WOOD), GRASS0);
        assert_eq!(search_water_attribute(0), GRASS0);
        // Wood bridge corners map to their river corners.
        assert_eq!(search_water_attribute(27), RIVER_NW);
        assert_eq!(search_water_attribute(30), RIVER_NE);
        // River banks 39-42 map to river corners.
        assert_eq!(search_water_attribute(39), RIVER_NW);
        assert_eq!(search_water_attribute(42), RIVER_NE);
        assert!(is_water_attribute(WATER));
        assert!(is_water_attribute(RIVER_NE));
        assert!(!is_water_attribute(WOOD));
        assert!(!is_water_attribute(GRASS0));
    }

    #[test]
    fn unit_area_classification() {
        // x < z, z > -x -> S(2); x < z, z <= -x -> W(1).
        assert_eq!(get_unit_area(-1.0, 5.0), 2);
        assert_eq!(get_unit_area(-5.0, 1.0), 1);
        // x >= z, z > -x -> E(3); else N(0).
        assert_eq!(get_unit_area(5.0, 1.0), 3);
        assert_eq!(get_unit_area(5.0, -6.0), 0);
    }

    #[test]
    fn bridge_water_search_kernel() {
        use attribute::*;
        // Attr 27 (wood NW): mask 3 = N(0) + W(1).
        let mut n = [GRASS0; 8];
        n[0] = WATER; // north neighbor is water
        assert_eq!(bridge_water_search(27, true, false, n), Some(WATER));
        // Direction order matters: N checked before W.
        let mut n = [GRASS0; 8];
        n[0] = RIVER_N;
        n[1] = RIVER_W;
        assert_eq!(bridge_water_search(27, true, false, n), Some(RIVER_N));
        // Stone bridge S (35): mask 4 = S(2) only.
        let mut n = [GRASS0; 8];
        n[0] = WATER; // north water is NOT searched
        assert_eq!(bridge_water_search(35, true, false, n), None);
        n[2] = RIVER_S;
        assert_eq!(bridge_water_search(35, true, false, n), Some(RIVER_S));
        // Gates: needs old_in_water and !attribute_wall and a bridge attr.
        let n = [WATER; 8];
        assert_eq!(bridge_water_search(27, false, false, n), None);
        assert_eq!(bridge_water_search(27, true, true, n), None);
        assert_eq!(bridge_water_search(26, true, false, n), None);
        // No water anywhere -> None (caller falls back to Wpos2Attribute).
        assert_eq!(bridge_water_search(27, true, false, [GRASS0; 8]), None);
        // Slate positive form.
        assert!(bridge_should_make_slate(27, false));
        assert!(!bridge_should_make_slate(27, true));
        assert!(bridge_should_make_slate(26, true));
        // C ABI.
        let n = [WATER; 8];
        assert_eq!(unsafe { pc_bridge_water_search(27, 1, 0, n.as_ptr()) }, WATER);
        assert_eq!(unsafe { pc_bridge_water_search(27, 0, 0, n.as_ptr()) }, 0xFF);
        assert_eq!(unsafe { pc_bridge_water_search(27, 1, 0, core::ptr::null()) }, 0xFF);
    }

    #[test]
    fn decode_height_and_direct_offsets() {
        assert_eq!(decode_height(2, 100.0), 120.0);
        assert_eq!(direct_offset(Direct::N), (0, -1));
        assert_eq!(direct_offset(Direct::E), (1, 0));
        assert_eq!(direct_offset(Direct::SW), (1, -1));
    }

    #[test]
    fn edge_mask_tables() {
        use direction_bit::*;
        // 3x3 verbatim.
        assert_eq!(cardinal_edge_mask(3, 0), 0x00);
        assert_eq!(cardinal_edge_mask(3, 1), 0x02);
        assert_eq!(cardinal_edge_mask(3, 3), 0x01);
        assert_eq!(cardinal_edge_mask(3, 4), 0x03);
        assert_eq!(cardinal_edge_mask(3, 8), 0x03);
        // 5x5: first row LEFT-only except corner, rest UP or UP+LEFT.
        assert_eq!(cardinal_edge_mask(5, 0), 0x00);
        assert_eq!(cardinal_edge_mask(5, 4), 0x02);
        assert_eq!(cardinal_edge_mask(5, 5), 0x01);
        assert_eq!(cardinal_edge_mask(5, 24), 0x03);
        // 7x7 spot checks.
        assert_eq!(cardinal_edge_mask(7, 0), 0x00);
        assert_eq!(cardinal_edge_mask(7, 7), 0x01);
        assert_eq!(cardinal_edge_mask(7, 48), 0x03);
        // Unknown size falls back to the 3x3 table.
        assert_eq!(cardinal_edge_mask(9, 4), 0x03);
        assert_eq!(cardinal_edge_mask(3, 99), 0x00);
        // Only UP/LEFT bits are ever set in any table.
        for size in [3, 5, 7] {
            let n = size * size;
            for i in 0..n {
                let m = cardinal_edge_mask(size, i);
                assert_eq!(m & !(UP | LEFT), 0, "size {size} idx {i}: {m:#x}");
            }
        }
        assert_eq!(pc_cardinal_edge_mask(3, 4), 0x03);
    }

    #[test]
    fn neighbor_anchors() {
        assert_eq!(cardinal_neighbor_index(12, 5, wall_name::UP), Some(7));
        assert_eq!(cardinal_neighbor_index(12, 5, wall_name::LEFT), Some(11));
        assert_eq!(cardinal_neighbor_index(12, 5, wall_name::DOWN), Some(17));
        assert_eq!(cardinal_neighbor_index(12, 5, wall_name::RIGHT), Some(13));
        assert_eq!(cardinal_neighbor_index(2, 5, wall_name::UP), None); // underflow
        assert_eq!(cardinal_neighbor_index(0, 3, wall_name::LEFT), None);
    }

    #[test]
    fn edge_existence() {
        let flat = unit(0, 10.0, 10.0, 10.0, 10.0);
        let high = unit(0, 30.0, 30.0, 30.0, 30.0);
        assert!(!cardinal_edge_exists(&flat, &flat, wall_name::UP));
        assert!(cardinal_edge_exists(&flat, &high, wall_name::UP));
        assert!(cardinal_edge_exists(&flat, &high, wall_name::LEFT));
        assert!(cardinal_edge_exists(&flat, &high, wall_name::DOWN));
        assert!(cardinal_edge_exists(&flat, &high, wall_name::RIGHT));
        // Partial difference on one endpoint is enough.
        let mut part = flat;
        part.right_up = 11.0;
        assert!(cardinal_edge_exists(&flat, &part, wall_name::UP));
        assert!(!cardinal_edge_exists(&flat, &part, wall_name::LEFT));
    }

    #[test]
    fn slate_corner_adjustment() {
        // unit1 slate, UP edge, SLATE_UP: leftDown = rightDown.
        let u = unit(0, 10.0, 20.0, 40.0, 30.0); // lu, ld, rd, ru
        let a = adjust_slate_unit_for_cardinal(&u, wall_name::SLATE_UP, wall_name::UP, true);
        assert_eq!((a.left_down, a.right_down), (40.0, 40.0));
        // unit1 slate, UP edge, SLATE_DOWN: rightDown = leftDown.
        let a = adjust_slate_unit_for_cardinal(&u, wall_name::SLATE_DOWN, wall_name::UP, true);
        assert_eq!((a.left_down, a.right_down), (20.0, 20.0));
        // unit0 slate, LEFT edge, SLATE_UP: leftDown = leftUp.
        let a = adjust_slate_unit_for_cardinal(&u, wall_name::SLATE_UP, wall_name::LEFT, false);
        assert_eq!((a.left_down, a.left_up), (10.0, 10.0));
        // unit0 slate, RIGHT edge, SLATE_DOWN: rightUp = rightDown.
        let a = adjust_slate_unit_for_cardinal(&u, wall_name::SLATE_DOWN, wall_name::RIGHT, false);
        assert_eq!((a.right_up, a.right_down), (40.0, 40.0));
        // Other fields untouched.
        assert_eq!(a.attribute, 0);
    }

    #[test]
    fn height_interpolation() {
        let b = WallBounds { start_top: 20.0, start_btm: 10.0, end_top: 40.0, end_btm: 30.0 };
        // UP wall along X: midpoint interpolates to 30/20.
        let r = check_height_exactly(&b, [0.0, 0.0], [100.0, 0.0], wall_name::UP, 0.0, [50.0, 0.0], false);
        assert_eq!(r, Some((30.0, 20.0)));
        // LEFT wall along Z.
        let r = check_height_exactly(&b, [0.0, 0.0], [0.0, 100.0], wall_name::LEFT, 0.0, [0.0, 25.0], false);
        assert_eq!(r, Some((25.0, 15.0)));
        // Gate: pos_y + 3 > top -> None.
        let r = check_height_exactly(&b, [0.0, 0.0], [100.0, 0.0], wall_name::UP, 38.0, [50.0, 0.0], false);
        assert_eq!(r, None);
        // Zero-length axis -> None.
        let r = check_height_exactly(&b, [5.0, 0.0], [5.0, 0.0], wall_name::UP, 0.0, [5.0, 0.0], false);
        assert_eq!(r, None);
        // Moving wall: end bounds directly.
        let r = check_height_exactly(&b, [0.0, 0.0], [100.0, 0.0], wall_name::UP, 0.0, [50.0, 0.0], true);
        assert_eq!(r, Some((40.0, 30.0)));
        // Slate wall names are not handled here (GetWallHeight path).
        let r = check_height_exactly(&b, [0.0, 0.0], [100.0, 0.0], wall_name::SLATE_UP, 0.0, [50.0, 0.0], false);
        assert_eq!(r, None);
        // C ABI.
        let (mut top, mut bot) = (0.0f32, 0.0f32);
        let ok = unsafe {
            pc_check_height_exactly(
                20.0, 10.0, 40.0, 30.0,
                0.0, 0.0, 100.0, 0.0,
                wall_name::UP, 0.0, 50.0, 0.0, 0,
                &mut top as *mut f32, &mut bot as *mut f32,
            )
        };
        assert_eq!(ok, 1);
        assert_eq!((top, bot), (30.0, 20.0));
    }
}
