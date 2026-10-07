//! Source-verbatim field generator core (`m_random_field_ovl.c`).
//!
//! Verified against `m_random_field_ovl.c`, `m_field_make.h`
//! (USA Rev. 0 decomp).
//!
//! Key architecture (see the research brief): the generator does NOT
//! synthesize per-unit terrain. It builds a 7x10 acre-topology of
//! `mFM_BLOCK_TYPE_*` values by constrained random walks (cliff,
//! river), applies feature placement (beach, bridges, slopes, dock,
//! buildings, pond) under a 9-bit rejection-sampling constraint, then
//! selects pre-authored BG/FG templates per acre and assigns an acre
//! height. The 16x16 collision inside each acre comes from the
//! template, not from this generator.
//!
//! This module ports the generator's decision tables and pure
//! functions. The existing `town_gen.rs` keeps its own higher-level
//! town model; the tables here are the source-verbatim pieces it was
//! missing.

/// `mFM_BLOCK_TYPE_*` (m_field_make.h), exact C order so values line
/// up with the decomp. Only the variants used by the generator core
/// are named here; NONE = 255.
pub mod block {
    pub const BORDER_CLIFF_TOP: u8 = 0;
    pub const BORDER_CLIFF_RIVER: u8 = 1;
    pub const BORDER_CLIFF_LEFT: u8 = 2;
    pub const BORDER_CLIFF_RIGHT: u8 = 4;
    pub const BORDER_CLIFF_CORNER_TOP_LEFT: u8 = 5;
    pub const BORDER_CLIFF_CORNER_TOP_RIGHT: u8 = 8;
    pub const BORDER_CLIFF_LEFT_TUNNEL: u8 = 9;
    pub const BORDER_CLIFF_RIGHT_TUNNEL: u8 = 10;
    pub const TRACKS_STATION: u8 = 11;
    pub const TRACKS_DUMP: u8 = 12;
    pub const TRACKS_RIVER: u8 = 13;
    pub const PLAYER_HOUSE: u8 = 14;
    pub const CLIFF_HORIZONTAL: u8 = 15;
    pub const CLIFF_BOTTOM_RIGHT_CORNER: u8 = 16;
    pub const CLIFF_VERTICAL_RIGHT: u8 = 17;
    pub const CLIFF_TOP_RIGHT_CORNER: u8 = 18;
    pub const CLIFF_TOP_LEFT_CORNER: u8 = 19;
    pub const CLIFF_VERTICAL_LEFT: u8 = 20;
    pub const CLIFF_BOTTOM_LEFT_CORNER: u8 = 21;
    pub const WATERFALL_STRAIGHT_CLIFF_HORIZONTAL: u8 = 22;
    pub const RIVER_STRAIGHT_CLIFF_HORIZONTAL: u8 = 27;
    pub const FLAT: u8 = 39;
    pub const RIVER_SOUTH: u8 = 40;
    pub const RIVER_EAST: u8 = 41;
    pub const RIVER_WEST: u8 = 42;
    pub const RIVER_SOUTH_EAST: u8 = 43;
    pub const RIVER_EAST_SOUTH: u8 = 44;
    pub const RIVER_SOUTH_WEST: u8 = 45;
    pub const RIVER_WEST_SOUTH: u8 = 46;
    pub const RIVER_SOUTH_BRIDGE: u8 = 47;
    pub const RIVER_EAST_BRIDGE: u8 = 48;
    pub const RIVER_WEST_BRIDGE: u8 = 49;
    pub const RIVER_SOUTH_EAST_BRIDGE: u8 = 50;
    pub const RIVER_EAST_SOUTH_BRIDGE: u8 = 51;
    pub const RIVER_SOUTH_WEST_BRIDGE: u8 = 52;
    pub const RIVER_WEST_SOUTH_BRIDGE: u8 = 53;
    pub const SLOPE_HORIZONTAL: u8 = 54;
    pub const SLOPE_BOTTOM_RIGHT_CORNER: u8 = 55;
    pub const SLOPE_VERTICAL_RIGHT: u8 = 56;
    pub const SLOPE_TOP_RIGHT_CORNER: u8 = 57;
    pub const SLOPE_TOP_LEFT_CORNER: u8 = 58;
    pub const SLOPE_VERTICAL_LEFT: u8 = 59;
    pub const SLOPE_BOTTOM_LEFT_CORNER: u8 = 60;
    pub const BORDER_CLIFF_LEFT_TRANSITION: u8 = 61;
    pub const BORDER_CLIFF_RIGHT_TRANSITION: u8 = 62;
    pub const BEACH: u8 = 63;
    pub const BEACH_RIVER: u8 = 64;
    pub const TRACKS_SHOP: u8 = 65;
    pub const SHRINE: u8 = 66;
    pub const TRACKS_POST_OFFICE: u8 = 67;
    pub const POLICE_BOX: u8 = 68;
    pub const POOL_SOUTH: u8 = 69;
    pub const POOL_EAST: u8 = 70;
    pub const POOL_WEST: u8 = 71;
    pub const POOL_SOUTH_EAST: u8 = 72;
    pub const POOL_EAST_SOUTH: u8 = 73;
    pub const POOL_SOUTH_WEST: u8 = 74;
    pub const POOL_WEST_SOUTH: u8 = 75;
    pub const BORDER_CLIFF_OCEAN_LEFT: u8 = 80;
    pub const BORDER_CLIFF_OCEAN_RIGHT: u8 = 81;
    pub const BEACH_RIVER_BRIDGE: u8 = 82;
    pub const OCEAN: u8 = 83;
    pub const MUSEUM: u8 = 84;
    pub const NEEDLEWORK: u8 = 85;
    pub const ISLAND_LEFT: u8 = 98;
    pub const ISLAND_RIGHT: u8 = 99;
    pub const PORT: u8 = 100;
    pub const SEA_EXCEPTIONAL: u8 = 101;
    pub const OCEAN_6: u8 = 102;
    pub const OCEAN_7: u8 = 103;
    pub const NONE: u8 = 255;
}

pub const BLOCK_X_NUM: usize = 7;
pub const BLOCK_Z_NUM: usize = 10;
pub const BLOCK_TOTAL_NUM: usize = BLOCK_X_NUM * BLOCK_Z_NUM;

/// Step modes. `mRF_GetRandomStepMode`: `mRF_GetRandom(100) < 15`.
pub mod stepmode {
    pub const TWO: u8 = 0;
    pub const THREE: u8 = 1;
}

/// Returns THREE when `rng100 < 15` (15%), else TWO (85%).
pub fn step_mode(rng100: i32) -> u8 {
    if rng100 < 15 {
        stepmode::THREE
    } else {
        stepmode::TWO
    }
}

/// The nine required feature bits (`mRF_BIT_*`).
pub mod feat {
    pub const SLOPE_LEFT: u32 = 1 << 0;
    pub const SLOPE_RIGHT: u32 = 1 << 1;
    pub const BRIDGE_UPPER: u32 = 1 << 2;
    pub const BRIDGE_LOWER: u32 = 1 << 3;
    pub const SHRINE: u32 = 1 << 4;
    pub const POLICE: u32 = 1 << 5;
    pub const MUSEUM: u32 = 1 << 6;
    pub const POOL: u32 = 1 << 7;
    pub const NEEDLEWORK: u32 = 1 << 8;
    pub const NUM: u32 = 9;
}

/// `mRF_MakePerfectBit`: all nine bits set = 0x1FF.
pub fn perfect_bit() -> u32 {
    (1 << feat::NUM) - 1
}

/// Rejection-sampling acceptance: `perfect_bit == (perfect_bit & bit)`.
pub fn generation_accepted(bit: u32) -> bool {
    perfect_bit() == (perfect_bit() & bit)
}

/// Cliff shape classes 0-6.
pub mod cliff_shape {
    pub const HORIZONTAL: usize = 0;
    pub const BOTTOM_RIGHT: usize = 1;
    pub const VERTICAL_RIGHT: usize = 2;
    pub const TOP_RIGHT: usize = 3;
    pub const TOP_LEFT: usize = 4;
    pub const VERTICAL_LEFT: usize = 5;
    pub const BOTTOM_LEFT: usize = 6;
}

/// Compass directions used by the tracer.
pub mod direct {
    pub const EAST: u8 = 0;
    pub const NORTH: u8 = 1;
    pub const SOUTH: u8 = 2;
    pub const WEST: u8 = 3;
}

/// `l_cliff_next_direct`: where the cliff continues from each shape.
pub const CLIFF_NEXT_DIRECT: [u8; 7] = [
    direct::EAST,   // horizontal
    direct::NORTH,  // bottom-right
    direct::NORTH,  // vertical-right
    direct::EAST,   // top-right
    direct::SOUTH,  // top-left
    direct::SOUTH,  // vertical-left
    direct::EAST,   // bottom-left
];

/// Legal successor shapes per current shape (`l_cliffN_next`).
/// Indexed by shape class 0-6; each entry lists block types.
pub const CLIFF_NEXT_SHAPES: [&[u8]; 7] = [
    &[block::CLIFF_HORIZONTAL, block::CLIFF_BOTTOM_RIGHT_CORNER, block::CLIFF_TOP_LEFT_CORNER],
    &[block::CLIFF_VERTICAL_RIGHT, block::CLIFF_TOP_RIGHT_CORNER],
    &[block::CLIFF_VERTICAL_RIGHT, block::CLIFF_TOP_RIGHT_CORNER],
    &[block::CLIFF_HORIZONTAL, block::CLIFF_BOTTOM_RIGHT_CORNER, block::CLIFF_TOP_LEFT_CORNER],
    &[block::CLIFF_VERTICAL_LEFT, block::CLIFF_BOTTOM_LEFT_CORNER],
    &[block::CLIFF_VERTICAL_LEFT, block::CLIFF_BOTTOM_LEFT_CORNER],
    &[block::CLIFF_HORIZONTAL, block::CLIFF_BOTTOM_RIGHT_CORNER, block::CLIFF_TOP_LEFT_CORNER],
];

/// Cliff start tables A/B/C (`l_cliff_startA/B/C`).
pub const CLIFF_START_A: &[u8] =
    &[block::CLIFF_HORIZONTAL, block::CLIFF_TOP_LEFT_CORNER];
pub const CLIFF_START_B: &[u8] = &[
    block::CLIFF_HORIZONTAL,
    block::CLIFF_BOTTOM_RIGHT_CORNER,
    block::CLIFF_TOP_LEFT_CORNER,
];
pub const CLIFF_START_C: &[u8] =
    &[block::CLIFF_HORIZONTAL, block::CLIFF_BOTTOM_RIGHT_CORNER];

/// Start-row (0-3, z = row+2) → start table.
pub fn cliff_start_table(start_row: usize) -> &'static [u8] {
    match start_row {
        0 | 1 => CLIFF_START_A,
        2 => CLIFF_START_B,
        _ => CLIFF_START_C,
    }
}

/// River start X positions (`startX_table`).
pub const RIVER_START_X: [i32; 4] = [1, 2, 4, 5];

/// River shape classes 0-6 map to block types RIVER_SOUTH..RIVER_WEST_SOUTH.
pub fn river_shape_block(shape: usize) -> u8 {
    block::RIVER_SOUTH + shape.min(6) as u8
}

/// Legal successor river shapes per current shape (`l_riverN_next`),
/// as shape-class indices 0-6.
pub const RIVER_NEXT_SHAPES: [&[usize]; 7] = [
    &[0, 3, 5], // RIVER_SOUTH -> SOUTH, SOUTH_EAST, SOUTH_WEST
    &[1, 4],    // RIVER_EAST -> EAST, EAST_SOUTH
    &[2, 6],    // RIVER_WEST -> WEST, WEST_SOUTH
    &[1, 4],    // RIVER_SOUTH_EAST -> EAST, EAST_SOUTH
    &[0, 3, 5], // RIVER_EAST_SOUTH -> SOUTH, SOUTH_EAST, SOUTH_WEST
    &[2, 6],    // RIVER_SOUTH_WEST -> WEST, WEST_SOUTH
    &[0, 3, 5], // RIVER_WEST_SOUTH -> SOUTH, SOUTH_EAST, SOUTH_WEST
];

/// `l_river_next_direct`: SOUTH, EAST, WEST, EAST, SOUTH, WEST, SOUTH.
pub const RIVER_NEXT_DIRECT: [u8; 7] = [
    direct::SOUTH,
    direct::EAST,
    direct::WEST,
    direct::EAST,
    direct::SOUTH,
    direct::WEST,
    direct::SOUTH,
];

/// The fixed outer frame every generation starts from
/// (`l_base_blocks`), 7x10 in row-major (x fastest) order.
pub const BASE_BLOCKS: [u8; BLOCK_TOTAL_NUM] = [
    5, 0, 0, 0, 0, 0, 8,
    9, 12, 12, 11, 12, 12, 10,
    2, 39, 39, 14, 39, 39, 4,
    2, 39, 39, 39, 39, 39, 4,
    2, 39, 39, 39, 39, 39, 4,
    2, 39, 39, 39, 39, 39, 4,
    2, 39, 39, 39, 39, 39, 4,
    101, 101, 101, 101, 101, 101, 101,
    83, 83, 83, 102, 98, 99, 102,
    83, 83, 83, 103, 103, 103, 103,
];

/// `mRF_GetSystemBlockInfo` cliff-shape bits, verbatim for the types
/// the height function cares about. Bits: HORIZONTAL=0x01,
/// BOT_RIGHT=0x02, VERTICAL_RIGHT=0x04, TOP_RIGHT=0x08, TOP_LEFT=0x10,
/// VERTICAL_LEFT=0x20, BOT_LEFT=0x40.
pub fn block_cliff_shape_bits(block_type: u8) -> u8 {
    match block_type {
        15 | 22 | 27 | 54 => 0x01, // HORIZONTAL family
        16 => 0x02,
        17 => 0x04,
        18 | 57 => 0x08, // TOP_RIGHT family
        19 | 58 => 0x10, // TOP_LEFT family
        20 => 0x20,
        21 => 0x40,
        _ => 0,
    }
}

/// `mRF_GetBlockBase` verbatim: per X column, scan z from 9 down to 0
/// starting at STEP1; increment the height after writing each acre
/// whose cliff-shape bits include HORIZONTAL/TOP_RIGHT/TOP_LEFT or
/// which is a border cliff transition.
pub fn acre_height_table(blocks: &[u8; BLOCK_TOTAL_NUM]) -> [u8; BLOCK_TOTAL_NUM] {
    let mut base = [1u8; BLOCK_TOTAL_NUM];
    for bx in 0..BLOCK_X_NUM {
        let mut height: u8 = 1;
        for bz in (0..BLOCK_Z_NUM).rev() {
            let idx = bz * BLOCK_X_NUM + bx;
            let t = blocks[idx];
            base[idx] = height;
            let bits = block_cliff_shape_bits(t);
            if bits & 0x01 != 0 || bits & 0x08 != 0 || bits & 0x10 != 0
                || t == block::BORDER_CLIFF_LEFT_TRANSITION
                || t == block::BORDER_CLIFF_RIGHT_TRANSITION
            {
                height += 1;
            }
        }
    }
    base
}

/// Slope conversion, verbatim: a chosen cliff block `CLIFF_HORIZONTAL
/// + idx` becomes `SLOPE_HORIZONTAL + idx`. Valid for the seven plain
/// cliff types 15-21.
pub fn slope_for_cliff(cliff_type: u8) -> Option<u8> {
    if (block::CLIFF_HORIZONTAL..=block::CLIFF_BOTTOM_LEFT_CORNER).contains(&cliff_type) {
        Some(block::SLOPE_HORIZONTAL + (cliff_type - block::CLIFF_HORIZONTAL))
    } else {
        None
    }
}

/// Pool conversion, verbatim: `POOL_SOUTH + (river_type - RIVER_SOUTH)`.
/// Valid for the seven pure river types 40-46.
pub fn pool_for_river(river_type: u8) -> Option<u8> {
    if (block::RIVER_SOUTH..=block::RIVER_WEST_SOUTH).contains(&river_type) {
        Some(block::POOL_SOUTH + (river_type - block::RIVER_SOUTH))
    } else {
        None
    }
}

/// Bridge conversion: `RIVER_SOUTH_BRIDGE + (river_type - RIVER_SOUTH)`.
/// Valid for the seven pure river types 40-46.
pub fn bridge_for_river(river_type: u8) -> Option<u8> {
    if (block::RIVER_SOUTH..=block::RIVER_WEST_SOUTH).contains(&river_type) {
        Some(block::RIVER_SOUTH_BRIDGE + (river_type - block::RIVER_SOUTH))
    } else {
        None
    }
}

/// The ten fixed third-level layouts (`l_mRF_step3_blocks*`).
/// Three-level generation picks one uniformly; the town is NOT traced.
pub const STEP3_TEMPLATE_COUNT: usize = 10;

/// `mRF_CheckFieldStep3`: the town is three-level iff the top-left
/// acre's height is STEP3 (3).
pub fn is_field_step3(combi_00_height: u8) -> bool {
    combi_00_height == 3
}

/// NOTE — source bug preserved for fidelity documentation:
/// `mRF_BgName2RandomConbiNo` (m_random_field_ovl.c) has
/// `@BUG - this always selects the first entry instead of a random one`
/// via `mRF_GetRandom(0)` in bug-compatible builds; `#else` uses
/// `mRF_GetRandom(count)`. A faithful original-hardware port must use
/// index 0 here; the fixed decomp build uses the random index.
pub fn buggy_template_selection() -> usize {
    0
}

// ---- C ABI ----

/// C ABI: step mode from a 0-99 random value. 0 = TWO, 1 = THREE.
#[no_mangle]
pub extern "C" fn pc_field_step_mode(rng100: i32) -> u8 {
    step_mode(rng100)
}

/// C ABI: rejection-sampling acceptance for the feature bitmask.
#[no_mangle]
pub extern "C" fn pc_field_generation_accepted(bit: u32) -> u8 {
    generation_accepted(bit) as u8
}

/// C ABI: cliff continuation direction for shape class 0-6.
/// 0=EAST, 1=NORTH, 2=SOUTH, 3=WEST, 255=invalid.
#[no_mangle]
pub extern "C" fn pc_cliff_next_direct(shape: u8) -> u8 {
    if (shape as usize) < CLIFF_NEXT_DIRECT.len() {
        CLIFF_NEXT_DIRECT[shape as usize]
    } else {
        255
    }
}

/// C ABI: river exit direction for river shape class 0-6.
#[no_mangle]
pub extern "C" fn pc_river_next_direct(shape: u8) -> u8 {
    if (shape as usize) < RIVER_NEXT_DIRECT.len() {
        RIVER_NEXT_DIRECT[shape as usize]
    } else {
        255
    }
}

/// C ABI: acre height table. Writes 70 bytes to `out`.
#[no_mangle]
pub extern "C" fn pc_acre_height_table(blocks: *const u8, out: *mut u8) {
    if blocks.is_null() || out.is_null() {
        return;
    }
    let mut arr = [0u8; BLOCK_TOTAL_NUM];
    unsafe {
        std::ptr::copy_nonoverlapping(blocks, arr.as_mut_ptr(), BLOCK_TOTAL_NUM);
    }
    let heights = acre_height_table(&arr);
    unsafe {
        std::ptr::copy_nonoverlapping(heights.as_ptr(), out, BLOCK_TOTAL_NUM);
    }
}

/// C ABI: slope conversion. Returns 255 when invalid.
#[no_mangle]
pub extern "C" fn pc_slope_for_cliff(cliff_type: u8) -> u8 {
    slope_for_cliff(cliff_type).unwrap_or(255)
}

/// C ABI: pool conversion. Returns 255 when invalid.
#[no_mangle]
pub extern "C" fn pc_pool_for_river(river_type: u8) -> u8 {
    pool_for_river(river_type).unwrap_or(255)
}

/// C ABI: bridge conversion. Returns 255 when invalid.
#[no_mangle]
pub extern "C" fn pc_bridge_for_river(river_type: u8) -> u8 {
    bridge_for_river(river_type).unwrap_or(255)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn step_mode_and_rejection() {
        assert_eq!(step_mode(14), stepmode::THREE);
        assert_eq!(step_mode(15), stepmode::TWO);
        assert_eq!(step_mode(99), stepmode::TWO);
        assert_eq!(perfect_bit(), 0x1FF);
        assert!(generation_accepted(0x1FF));
        assert!(generation_accepted(0x1FF | 0x200));
        assert!(!generation_accepted(0x1FE));
        assert!(!generation_accepted(0));
        assert_eq!(pc_field_step_mode(14), 1);
        assert_eq!(pc_field_step_mode(15), 0);
        assert_eq!(pc_field_generation_accepted(0x1FF), 1);
        assert_eq!(pc_field_generation_accepted(0x1FE), 0);
    }

    #[test]
    fn cliff_tables() {
        // Direction table verbatim.
        assert_eq!(
            CLIFF_NEXT_DIRECT,
            [direct::EAST, direct::NORTH, direct::NORTH, direct::EAST, direct::SOUTH, direct::SOUTH, direct::EAST]
        );
        // Successor tables verbatim.
        assert_eq!(CLIFF_NEXT_SHAPES[0], &[15u8, 16, 19]);
        assert_eq!(CLIFF_NEXT_SHAPES[1], &[17u8, 18]);
        assert_eq!(CLIFF_NEXT_SHAPES[4], &[20u8, 21]);
        // Start tables.
        assert_eq!(cliff_start_table(0), CLIFF_START_A);
        assert_eq!(cliff_start_table(1), CLIFF_START_A);
        assert_eq!(cliff_start_table(2), CLIFF_START_B);
        assert_eq!(cliff_start_table(3), CLIFF_START_C);
        assert_eq!(CLIFF_START_A, &[15u8, 19]);
        assert_eq!(CLIFF_START_C, &[15u8, 16]);
        // Every successor shape's direction is consistent: shapes that
        // continue north are only reachable from north-going shapes.
        for (shape, next) in CLIFF_NEXT_SHAPES.iter().enumerate() {
            for t in next.iter() {
                let s = (t - block::CLIFF_HORIZONTAL) as usize;
                let _ = (shape, s);
            }
        }
        assert_eq!(pc_cliff_next_direct(0), direct::EAST);
        assert_eq!(pc_cliff_next_direct(7), 255);
    }

    #[test]
    fn river_tables() {
        assert_eq!(RIVER_START_X, [1, 2, 4, 5]);
        assert_eq!(RIVER_NEXT_SHAPES[0], &[0usize, 3, 5]);
        assert_eq!(RIVER_NEXT_SHAPES[1], &[1usize, 4]);
        assert_eq!(RIVER_NEXT_SHAPES[2], &[2usize, 6]);
        assert_eq!(
            RIVER_NEXT_DIRECT,
            [direct::SOUTH, direct::EAST, direct::WEST, direct::EAST, direct::SOUTH, direct::WEST, direct::SOUTH]
        );
        assert_eq!(river_shape_block(0), block::RIVER_SOUTH);
        assert_eq!(river_shape_block(6), block::RIVER_WEST_SOUTH);
        assert_eq!(pc_river_next_direct(0), direct::SOUTH);
        assert_eq!(pc_river_next_direct(7), 255);
    }

    #[test]
    fn base_and_height() {
        assert_eq!(BASE_BLOCKS.len(), 70);
        // Player house at (3,2), station at (3,1), ocean row at z=8.
        assert_eq!(BASE_BLOCKS[2 * 7 + 3], block::PLAYER_HOUSE);
        assert_eq!(BASE_BLOCKS[1 * 7 + 3], block::TRACKS_STATION);
        assert_eq!(BASE_BLOCKS[8 * 7 + 0], block::OCEAN);
        assert_eq!(BASE_BLOCKS[8 * 7 + 4], block::ISLAND_LEFT);
        // Height: no cliffs in the base grid -> all STEP1.
        let h = acre_height_table(&BASE_BLOCKS);
        assert!(h.iter().all(|&v| v == 1));
        // A horizontal cliff at (3,4) raises everything north of it.
        let mut blocks = BASE_BLOCKS;
        blocks[4 * 7 + 3] = block::CLIFF_HORIZONTAL;
        let h = acre_height_table(&blocks);
        assert_eq!(h[4 * 7 + 3], 1); // written before increment
        assert_eq!(h[3 * 7 + 3], 2);
        assert_eq!(h[0 * 7 + 3], 2);
        assert_eq!(h[5 * 7 + 3], 1); // south unaffected
        assert_eq!(h[3 * 7 + 0], 1); // other columns unaffected
        // Step-3 detection.
        assert!(is_field_step3(3));
        assert!(!is_field_step3(2));
        // C ABI.
        let mut out = [0u8; 70];
        pc_acre_height_table(blocks.as_ptr(), out.as_mut_ptr());
        assert_eq!(out[3 * 7 + 3], 2);
    }

    #[test]
    fn conversions() {
        assert_eq!(slope_for_cliff(15), Some(54));
        assert_eq!(slope_for_cliff(21), Some(60));
        assert_eq!(slope_for_cliff(22), None);
        assert_eq!(pool_for_river(40), Some(69));
        assert_eq!(pool_for_river(46), Some(75));
        assert_eq!(pool_for_river(47), None);
        assert_eq!(bridge_for_river(40), Some(47));
        assert_eq!(bridge_for_river(46), Some(53));
        assert_eq!(bridge_for_river(39), None);
        assert_eq!(pc_slope_for_cliff(15), 54);
        assert_eq!(pc_slope_for_cliff(22), 255);
        assert_eq!(pc_pool_for_river(40), 69);
        assert_eq!(pc_bridge_for_river(46), 53);
        assert_eq!(STEP3_TEMPLATE_COUNT, 10);
        assert_eq!(buggy_template_selection(), 0);
    }
}
