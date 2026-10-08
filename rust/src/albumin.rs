//! River-cliff "albumin" combination tables.
//!
//! Verified against `m_random_field_ovl.c` (mRF_RiverAlbuminCliff,
//! mRF_DecideRiverAlbuminCliff, blockGroup, river trace legality),
//! `m_field_make.h` (block type enum values)
//! (USA Rev. 0 decomp / PC port).
//!
//! Albumin is the river x cliff compatibility/merge table: it converts
//! a (cliff, river) pair of procedural block types into a combined
//! RIVER_CLIFF_*/WATERFALL_* block type, or NONE. The merge writes into
//! cliff_blocks (which becomes the canonical landform map); a river
//! with no valid albumin cell is copied as an ordinary river block.
//! It also gates river tracing: an incompatible river/cliff encounter
//! fails the whole river-generation attempt. Step-3 towns bypass it
//! (pre-authored templates).

/// Block type ids (mFM_BLOCK_TYPE_*).
pub mod bt {
    pub const CLIFF_HORIZONTAL: u8 = 15;
    pub const RIVER_SOUTH: u8 = 40;
    pub const NONE: u8 = 255;
}

/// Block group ranges (mRF_BLOCK_GROUP_*, blockGroup table):
/// (min, max) inclusive.
pub mod group {
    pub const CLIFF: (u8, u8) = (15, 21);
    pub const RIVER: (u8, u8) = (40, 46);
    pub const RIVER_CLIFF_ANY: (u8, u8) = (22, 38);
    pub const RIVER_CLIFF_1: (u8, u8) = (22, 28);
    pub const RIVER_CLIFF_2: (u8, u8) = (29, 33);
    pub const RIVER_CLIFF_3: (u8, u8) = (34, 38);
}

fn in_group(t: u8, g: (u8, u8)) -> bool {
    t >= g.0 && t <= g.1
}

/// Albumin rows: river index 0..2 (SOUTH/EAST/WEST) x cliff index 0..6.
/// Rows 3..6 (corner rivers) are all NONE.
pub const ALBUMIN: [[u8; 7]; 3] = [
    [22, 23, 24, 25, 26, 27, 28], // south river: all 7 valid
    [29, 30, 31, 32, 33, bt::NONE, bt::NONE], // east river: 5 valid
    [34, bt::NONE, bt::NONE, 35, 36, 37, 38], // west river: 5 valid
];

/// Number of valid albumin cells: 7 + 5 + 5 = 17.
pub const ALBUMIN_VALID_COUNT: usize = 17;

/// Albumin lookup: mRF_RiverAlbuminCliff(cliff_type, river_type).
/// Returns NONE unless cliff is in CLIFF group and river in RIVER group.
pub fn river_albumin_cliff(cliff_type: u8, river_type: u8) -> u8 {
    if !in_group(cliff_type, group::CLIFF) || !in_group(river_type, group::RIVER) {
        return bt::NONE;
    }
    let river = river_type - bt::RIVER_SOUTH; // 0..6
    let cliff = cliff_type - bt::CLIFF_HORIZONTAL; // 0..6
    if river < 3 {
        ALBUMIN[river as usize][cliff as usize]
    } else {
        bt::NONE // corner rivers: river_no_album_data
    }
}

/// Merge step (mRF_DecideRiverAlbuminCliff): for one block, given the
/// current cliff block and river block, returns the new cliff_blocks
/// value, or None if the cell is left unchanged.
/// Border-cliff-river (1) and tracks-river (13) also copy through as rivers.
pub fn decide_albumin_cell(cliff_block: u8, river_block: u8) -> Option<u8> {
    let album = river_albumin_cliff(cliff_block, river_block);
    if album != bt::NONE {
        Some(album)
    } else if in_group(river_block, group::RIVER) || river_block == 1 || river_block == 13 {
        Some(river_block) // ordinary river copied into cliff_blocks
    } else {
        None // unchanged
    }
}

/// Count valid albumin cells (for verification).
pub fn albumin_valid_count() -> usize {
    ALBUMIN.iter().flatten().filter(|&&t| t != bt::NONE).count()
}

// ---- C ABI ----

/// C ABI: albumin lookup. Returns the combined block type or 255 (NONE).
#[no_mangle]
pub extern "C" fn pc_river_albumin_cliff(cliff_type: u8, river_type: u8) -> u8 {
    river_albumin_cliff(cliff_type, river_type)
}

/// C ABI: merge step for one cell. Returns 1 and fills out when the
/// cell changes, else 0.
#[no_mangle]
pub extern "C" fn pc_decide_albumin_cell(cliff_block: u8, river_block: u8, out: *mut u8) -> u8 {
    if out.is_null() {
        return 0;
    }
    match decide_albumin_cell(cliff_block, river_block) {
        Some(t) => unsafe {
            *out = t;
            1
        },
        None => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn albumin_table() {
        assert_eq!(albumin_valid_count(), ALBUMIN_VALID_COUNT);
        assert_eq!(ALBUMIN_VALID_COUNT, 17);
        // South river x all cliffs: 22..28.
        for c in 0..7u8 {
            assert_eq!(river_albumin_cliff(15 + c, 40), 22 + c);
        }
        // East river holes.
        assert_eq!(river_albumin_cliff(20, 41), bt::NONE); // VL
        assert_eq!(river_albumin_cliff(21, 41), bt::NONE); // BL
        assert_eq!(river_albumin_cliff(15, 41), 29);
        // West river holes.
        assert_eq!(river_albumin_cliff(16, 42), bt::NONE); // BR
        assert_eq!(river_albumin_cliff(17, 42), bt::NONE); // VR
        assert_eq!(river_albumin_cliff(15, 42), 34); // H -> RIVER_WEST_CLIFF_HORIZONTAL
        // Corner rivers: all NONE.
        for r in 43..47u8 {
            for c in 0..7u8 {
                assert_eq!(river_albumin_cliff(15 + c, r), bt::NONE);
            }
        }
        // Out-of-group inputs -> NONE.
        assert_eq!(river_albumin_cliff(39, 40), bt::NONE); // FLAT cliff
        assert_eq!(river_albumin_cliff(15, 39), bt::NONE); // FLAT river
        // Group ranges.
        assert_eq!(group::RIVER_CLIFF_ANY, (22, 38));
        assert_eq!(group::RIVER_CLIFF_1, (22, 28));
        assert_eq!(group::RIVER_CLIFF_2, (29, 33));
        assert_eq!(group::RIVER_CLIFF_3, (34, 38));
        // Merge semantics.
        assert_eq!(decide_albumin_cell(15, 40), Some(22));
        assert_eq!(decide_albumin_cell(20, 41), Some(41)); // river copied
        assert_eq!(decide_albumin_cell(15, 1), Some(1)); // BORDER_CLIFF_RIVER copied
        assert_eq!(decide_albumin_cell(15, 13), Some(13)); // TRACKS_RIVER copied
        assert_eq!(decide_albumin_cell(15, 39), None); // unchanged
        // C ABI.
        assert_eq!(pc_river_albumin_cliff(15, 40), 22);
        let mut out = 0u8;
        assert_eq!(pc_decide_albumin_cell(15, 40, &mut out as *mut u8), 1);
        assert_eq!(out, 22);
    }
}
