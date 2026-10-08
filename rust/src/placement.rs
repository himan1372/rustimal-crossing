//! Town placement passes: beach, bridges, slopes, buildings, pond.
//!
//! Verified against `m_random_field_ovl.c` (mRF_MakeRandomField_ovl,
//! SetMarinBlock, SetBridgeBlock, GetRiverCrossCliffInfo,
//! SetSlopeBlock, SetNeedleworkAndWharfBlock, SetUniqueFlatBlock,
//! SetUniqueRailBlock, CountPureRiver, SetPoolBlock,
//! SetSeaBlockWithBridgeRiver) and `m_field_make.h` (block ids)
//! (USA Rev. 0 decomp / PC port).
//!
//! Model: placement is monotonic acre-type replacement on the 7x10
//! block grid, inside a 9-bit acceptance loop. Order:
//! beach -> bridges+slopes -> needlework/wharf -> shrine/police/museum
//! -> shop/post (railroad) -> pond -> beach-river bridge fallback ->
//! heights -> BG/FG combination selection.

/// Grid dimensions.
pub const BLOCK_X: usize = 7;
pub const BLOCK_Z: usize = 10;

/// Block type ids used by the placement passes.
pub mod bt {
    pub const FLAT: u8 = 39;
    pub const RIVER_SOUTH: u8 = 40;
    pub const RIVER_SOUTH_BRIDGE: u8 = 47;
    pub const SLOPE_HORIZONTAL: u8 = 54;
    pub const CLIFF_HORIZONTAL: u8 = 15;
    pub const BEACH: u8 = 63;
    pub const BEACH_RIVER: u8 = 64;
    pub const TRACKS_SHOP: u8 = 65;
    pub const SHRINE: u8 = 66;
    pub const TRACKS_POST_OFFICE: u8 = 67;
    pub const POLICE_BOX: u8 = 68;
    pub const POOL_SOUTH: u8 = 69;
    pub const BORDER_CLIFF_OCEAN_LEFT: u8 = 80;
    pub const BORDER_CLIFF_OCEAN_RIGHT: u8 = 81;
    pub const BEACH_RIVER_BRIDGE: u8 = 82;
    pub const MUSEUM: u8 = 84;
    pub const NEEDLEWORK: u8 = 85;
    pub const PORT: u8 = 100;
    pub const TRACKS_DUMP: u8 = 12;
    pub const NONE: u8 = 109;
}

/// Bridge type offset: RIVER_SOUTH_BRIDGE - RIVER_SOUTH = 7.
pub const BRIDGE_OFFSET: u8 = 7;
/// Pond type offset: POOL_SOUTH - RIVER_SOUTH = 29.
pub const POOL_OFFSET: u8 = 29;

/// Acceptance bits (mRF_BIT_*).
pub mod bit {
    pub const SLOPE_LEFT: u8 = 0;
    pub const SLOPE_RIGHT: u8 = 1;
    pub const BRIDGE_UPPER: u8 = 2;
    pub const BRIDGE_LOWER: u8 = 3;
    pub const SHRINE: u8 = 4;
    pub const POLICE: u8 = 5;
    pub const MUSEUM: u8 = 6;
    pub const POOL: u8 = 7;
    pub const NEEDLEWORK: u8 = 8;
    pub const NUM: u8 = 9;
}
pub const PERFECT_BITS: u16 = 0x1FF;

/// Beach row.
pub const BEACH_Z: usize = 6;

/// Beach pass (mRF_SetMarinBlock): z=6, x=1..5: FLAT->BEACH,
/// RIVER_SOUTH->BEACH_RIVER; (0,6)/(6,6) become ocean border cliffs.
/// Returns the new type for a cell, or None if unchanged.
pub fn marin_cell(current: u8, x: usize) -> Option<u8> {
    if x == 0 {
        Some(bt::BORDER_CLIFF_OCEAN_LEFT)
    } else if x == BLOCK_X - 1 {
        Some(bt::BORDER_CLIFF_OCEAN_RIGHT)
    } else if current == bt::FLAT {
        Some(bt::BEACH)
    } else if current == bt::RIVER_SOUTH {
        Some(bt::BEACH_RIVER)
    } else {
        None
    }
}

/// The seven waterfall crossing types that anchor bridge placement
/// (mRF_GetRiverCrossCliffInfo cross_data).
pub const CROSS_DATA: [u8; 7] = [22, 23, 26, 30, 31, 37, 38];

/// Bridge variant for a river type via the constant offset.
pub fn bridge_variant(river_type: u8) -> u8 {
    river_type + BRIDGE_OFFSET
}

/// Lower-bridge condition: after_cross != 0 && stepmode == TWO &&
/// (RANDOM(10) & 1) != 0.
pub fn lower_bridge_ok(after_cross: usize, stepmode_two: bool, rng_bit: bool) -> bool {
    after_cross != 0 && stepmode_two && rng_bit
}

/// Slope replacement: SLOPE_HORIZONTAL + cliff index (0..6).
pub fn slope_variant(cliff_idx: u8) -> u8 {
    bt::SLOPE_HORIZONTAL + cliff_idx
}

/// Pond variant for a pure river type via the constant offset.
pub fn pool_variant(river_type: u8) -> u8 {
    river_type + POOL_OFFSET
}

/// Pure river types eligible for pond conversion (mRF_CountPureRiver).
pub fn is_pure_river(t: u8) -> bool {
    (40..=46).contains(&t)
}

/// Wharf/dock: (5,6) must be BEACH -> PORT; needlework picks the
/// needlework_bx-th BEACH cell scanning x=1..5.
pub const WHARF_X: usize = 5;
pub const WHARF_Z: usize = 6;

/// Shop/post railroad placement: bx = 1+RANDOM(2) and bx = 4+RANDOM(2)
/// on z=1, requiring TRACKS_DUMP cells; which special goes left/right
/// is randomized.
pub const RAIL_Z: usize = 1;

/// Step-mode selection: mRF_GetRandom(100) < 15 -> three-step.
pub fn is_step_three(r100: u8) -> bool {
    r100 < 15
}

/// Block index: mRF_D2ToD1(bx, bz) = bz * BLOCK_X_NUM + bx.
pub fn d2_to_d1(bx: usize, bz: usize) -> usize {
    bz * BLOCK_X + bx
}

/// Placement scans cover z = 0..7 (56 cells), not the full 70.
pub const PLACEMENT_CELL_COUNT: usize = (BLOCK_Z - 2) * BLOCK_X;

/// River-side classification (mRF_RIVER_SIDE_*).
pub mod side {
    pub const LEFT: i8 = 0;
    pub const RIGHT: i8 = 1;
    pub const BOTH: i8 = 2;
}
/// Cliff-height classification (mRF_CLIFF_HEIGHT_*).
pub mod cheight {
    pub const ABOVE: i8 = 0;
    pub const BELOW: i8 = 1;
    pub const BOTH: i8 = 2;
}
/// Base height step 0 (mRF_FIELD_STEP1 = first level).
pub const FIELD_STEP1: u8 = 0;

/// Flat-place river-side classification for one row (mRF_MakeFlatPlaceInfomation).
/// `is_river[i]` = block is in RIVER or RIVER_CLIFF_ANY group.
/// Scans left to right: LEFT until a river block, then RIGHT.
pub fn classify_river_row(is_river: &[bool; 5]) -> [i8; 5] {
    let mut out = [side::LEFT; 5];
    let mut s = side::LEFT;
    for (i, &r) in is_river.iter().enumerate() {
        if s == side::LEFT && r {
            s = side::RIGHT;
        }
        out[i] = s;
    }
    out
}

/// Flat-place cliff-height classification for one column.
/// `is_cliff[i]` = block is in CLIFF_ANY group (cliff, slope, or river/cliff).
/// Scans top to bottom (bz=1..8): ABOVE until a cliff block, then BELOW.
pub fn classify_cliff_col(is_cliff: &[bool; 8]) -> [i8; 8] {
    let mut out = [cheight::ABOVE; 8];
    let mut s = cheight::ABOVE;
    for (i, &c) in is_cliff.iter().enumerate() {
        if s == cheight::ABOVE && c {
            s = cheight::BELOW;
        }
        out[i] = s;
    }
    out
}

/// Building-acre predicate (mRF_JudgeFlatBlock), source-faithful.
/// NOTE: BOTH is not a pure wildcard. When the request cliff_height is
/// BOTH, the source requires cliff_info[bnum] == BOTH (true only on
/// unclassified border cells); likewise a BOTH river_side with specific
/// cliff_height is a real wildcard on the side. In practice the game
/// only calls with cliff_height = BELOW.
pub fn judge_flat_block(
    is_flat: bool,
    river_side: i8,
    cliff_height: i8,
    block_side: i8,
    block_height: i8,
) -> bool {
    if !(0..3).contains(&river_side) || !(0..3).contains(&cliff_height) {
        return false;
    }
    if !is_flat {
        return false;
    }
    if cliff_height != cheight::BOTH {
        if river_side != side::BOTH {
            river_side == block_side && cliff_height == block_height
        } else {
            cliff_height == block_height
        }
    } else if river_side != side::BOTH {
        river_side == block_side && cliff_height == block_height
    } else {
        cliff_height == block_height
    }
}

/// Select the n-th qualifying flat block (mRF_RewriteFlatType scan order).
/// Returns the index within `candidates` (a pre-filtered ordered list).
pub fn rewrite_flat_idx(selected: usize, count: usize) -> Option<usize> {
    if selected < count {
        Some(selected)
    } else {
        None
    }
}

/// Shrine side pick: side0 = RANDOM(100) & 1, side1 = side0 ^ 1.
pub fn shrine_sides(r100: u8) -> (i8, i8) {
    let s0 = (r100 & 1) as i8;
    (s0, s0 ^ 1)
}

/// Shop/post left-right assignment: RANDOM(1000) & 1.
/// Returns true if SHOP goes to the x=1..2 slots (else POST_OFFICE).
pub fn shop_first(r1000: u16) -> bool {
    r1000 & 1 == 1
}

/// Needlework selection: pick the r3-th BEACH cell in x=1..5 scan order
/// (NOT x = r3 + 1). `beach_xs` = sorted x coords of beach cells on row 6.
/// Returns the chosen x, or None.
pub fn needlework_pick(beach_xs: &[usize], r3: usize) -> Option<usize> {
    beach_xs.get(r3).copied()
}

/// Base height table step (mRF_GetBlockBase): for one X column scanned
/// from z=9 down to z=0, starting at FIELD_STEP1, incrementing after any
/// block whose cliff bits include HORIZONTAL/TOP_RIGHT/TOP_LEFT or which
/// is a border cliff transition. `cliff_bits[i]` = whether block i (in
/// scan order, bottom-up) increments the height. Returns the height per
/// block in scan order.
pub fn base_height_column(cliff_bits: &[bool; 10]) -> [u8; 10] {
    let mut out = [0u8; 10];
    let mut h = FIELD_STEP1;
    for (i, &inc) in cliff_bits.iter().enumerate() {
        out[i] = h;
        if inc {
            h += 1;
        }
    }
    out
}

/// Pass order of mRF_MakeRandomField_ovl (source-proven). flat_info is
/// computed BEFORE beach/bridge/slope mutation and never recomputed.
pub const PASS_ORDER: [&str; 12] = [
    "make_base_landform",
    "make_flat_place_infomation",
    "set_marin_block",
    "set_bridge_and_slope_block",
    "set_needlework_and_wharf_block",
    "set_unique_flat_block",
    "set_unique_rail_block",
    "set_pool_block",
    "set_sea_block_with_bridge_river",
    "make_base_height_table",
    "select_block",
    "copy_block_base_height_data",
];

// ---- C ABI ----

/// C ABI: beach cell replacement. Returns new type, or 255 if unchanged.
#[no_mangle]
pub extern "C" fn pc_marin_cell(current: u8, x: usize) -> u16 {
    match marin_cell(current, x) {
        Some(t) => t as u16,
        None => 255,
    }
}

/// C ABI: bridge variant for a river type.
#[no_mangle]
pub extern "C" fn pc_bridge_variant(river_type: u8) -> u8 {
    bridge_variant(river_type)
}

/// C ABI: lower-bridge condition.
#[no_mangle]
pub extern "C" fn pc_lower_bridge_ok(after_cross: usize, stepmode_two: u8, rng_bit: u8) -> u8 {
    lower_bridge_ok(after_cross, stepmode_two != 0, rng_bit != 0) as u8
}

/// C ABI: slope variant for a cliff index.
#[no_mangle]
pub extern "C" fn pc_slope_variant(cliff_idx: u8) -> u8 {
    slope_variant(cliff_idx)
}

/// C ABI: pond variant for a pure river type.
#[no_mangle]
pub extern "C" fn pc_pool_variant(river_type: u8) -> u8 {
    pool_variant(river_type)
}

/// C ABI: block index from 2D coords.
#[no_mangle]
pub extern "C" fn pc_d2_to_d1(bx: usize, bz: usize) -> usize {
    d2_to_d1(bx, bz)
}

/// C ABI: shrine side pick. Packs (side0, side1) into one byte.
#[no_mangle]
pub extern "C" fn pc_shrine_sides(r100: u8) -> u8 {
    let (s0, s1) = shrine_sides(r100);
    ((s0 as u8) << 4) | (s1 as u8)
}

/// C ABI: needlework pick. beach_xs_ptr = sorted x coords of beach cells
/// on row 6, len = count. Returns chosen x, or 255.
#[no_mangle]
pub extern "C" fn pc_needlework_pick(beach_xs_ptr: *const usize, len: usize, r3: usize) -> usize {
    if beach_xs_ptr.is_null() {
        return 255;
    }
    let xs = unsafe { std::slice::from_raw_parts(beach_xs_ptr, len) };
    needlework_pick(xs, r3).unwrap_or(255)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placement_passes() {
        // Beach.
        assert_eq!(marin_cell(bt::FLAT, 3), Some(bt::BEACH));
        assert_eq!(marin_cell(bt::RIVER_SOUTH, 3), Some(bt::BEACH_RIVER));
        assert_eq!(marin_cell(bt::FLAT, 0), Some(bt::BORDER_CLIFF_OCEAN_LEFT));
        assert_eq!(marin_cell(bt::FLAT, 6), Some(bt::BORDER_CLIFF_OCEAN_RIGHT));
        assert_eq!(marin_cell(22, 3), None);
        assert_eq!(BEACH_Z, 6);
        assert_eq!(pc_marin_cell(bt::FLAT, 3), bt::BEACH as u16);
        assert_eq!(pc_marin_cell(22, 3), 255);
        // Bridges.
        assert_eq!(BRIDGE_OFFSET, 7);
        assert_eq!(bridge_variant(40), 47);
        assert_eq!(bridge_variant(42), 49);
        assert_eq!(pc_bridge_variant(40), 47);
        assert!(lower_bridge_ok(2, true, true));
        assert!(!lower_bridge_ok(0, true, true));
        assert!(!lower_bridge_ok(2, false, true));
        assert!(!lower_bridge_ok(2, true, false));
        assert_eq!(pc_lower_bridge_ok(2, 1, 1), 1);
        assert_eq!(CROSS_DATA.len(), 7);
        // Slopes.
        assert_eq!(slope_variant(0), 54);
        assert_eq!(slope_variant(6), 60);
        assert_eq!(pc_slope_variant(3), 57);
        // Pond.
        assert_eq!(POOL_OFFSET, 29);
        assert_eq!(pool_variant(40), 69);
        assert!(is_pure_river(46));
        assert!(!is_pure_river(22));
        assert_eq!(pc_pool_variant(41), 70);
        // Bits.
        assert_eq!(PERFECT_BITS, 0x1FF);
        assert_eq!(bit::NUM, 9);
        // Wharf/shop.
        assert_eq!((WHARF_X, WHARF_Z), (5, 6));
        assert_eq!(RAIL_Z, 1);
        assert!(!is_step_three(15));
        assert!(is_step_three(14));
        // Index math.
        assert_eq!(d2_to_d1(3, 2), 17);
        assert_eq!(d2_to_d1(5, 6), 47);
        assert_eq!(PLACEMENT_CELL_COUNT, 56);
        assert_eq!(pc_d2_to_d1(5, 6), 47);
        // Flat classification.
        assert_eq!(
            classify_river_row(&[false, false, true, false, false]),
            [side::LEFT, side::LEFT, side::RIGHT, side::RIGHT, side::RIGHT]
        );
        assert_eq!(
            classify_cliff_col(&[false, false, true, false, false, false, false, false]),
            [
                cheight::ABOVE, cheight::ABOVE, cheight::BELOW, cheight::BELOW,
                cheight::BELOW, cheight::BELOW, cheight::BELOW, cheight::BELOW
            ]
        );
        // Judge predicate.
        assert!(judge_flat_block(true, side::LEFT, cheight::BELOW, side::LEFT, cheight::BELOW));
        assert!(!judge_flat_block(true, side::LEFT, cheight::BELOW, side::RIGHT, cheight::BELOW));
        assert!(judge_flat_block(true, side::BOTH, cheight::BELOW, side::RIGHT, cheight::BELOW));
        assert!(!judge_flat_block(false, side::LEFT, cheight::BELOW, side::LEFT, cheight::BELOW));
        // BOTH request only matches BOTH info (border cells).
        assert!(!judge_flat_block(true, side::LEFT, cheight::BOTH, side::LEFT, cheight::BELOW));
        assert!(judge_flat_block(true, side::BOTH, cheight::BOTH, side::BOTH, cheight::BOTH));
        // Rewrite selection.
        assert_eq!(rewrite_flat_idx(2, 5), Some(2));
        assert_eq!(rewrite_flat_idx(5, 5), None);
        // Shrine sides / shop order.
        assert_eq!(shrine_sides(0), (0, 1));
        assert_eq!(shrine_sides(1), (1, 0));
        assert_eq!(pc_shrine_sides(1), 0x10);
        assert!(shop_first(1));
        assert!(!shop_first(0));
        // Needlework ordinal pick (not x = r3 + 1).
        assert_eq!(needlework_pick(&[1, 2, 4], 2), Some(4));
        assert_eq!(needlework_pick(&[1, 2, 4], 3), None);
        let xs = [1usize, 2, 4];
        assert_eq!(pc_needlework_pick(xs.as_ptr(), 3, 1), 2);
        // Base height column: bottom-up, increments after cliff bits.
        let col = base_height_column(&[false, false, true, false, false, false, false, false, false, false]);
        assert_eq!(col[0], 0);
        assert_eq!(col[2], 0);
        assert_eq!(col[3], 1);
        assert_eq!(col[9], 1);
        assert_eq!(FIELD_STEP1, 0);
        // Pass order: flat info before beach/bridge/slope.
        assert_eq!(PASS_ORDER[1], "make_flat_place_infomation");
        assert_eq!(PASS_ORDER[2], "set_marin_block");
        assert_eq!(PASS_ORDER.len(), 12);
    }
}
