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
    }
}
