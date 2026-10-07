//! Bridge-acres and the bridge-counterpart mapping.
//!
//! Verified against `m_random_field_ovl.c`, `m_map_ovl.c`, and
//! `m_field_make.h` (USA Rev. 0 decomp).
//!
//! The brief hypothesized a "bridge-water mask" as an acre-level concept.
//! The decomp confirms the mechanism, and it is block-type arithmetic,
//! not a bitmap:
//!
//! 1. **Bridge variants are parallel enum entries.** Seven river block
//!    types (40-46: SOUTH, EAST, WEST, SOUTH_EAST, EAST_SOUTH,
//!    SOUTH_WEST, WEST_SOUTH) are immediately followed by seven bridge
//!    variants (47-53) in the same order. Converting a river block to
//!    its bridge version is `type + 7`
//!    (`mFM_BLOCK_TYPE_RIVER_SOUTH_BRIDGE - mFM_BLOCK_TYPE_RIVER_SOUTH`).
//! 2. **Placement is template-driven, not free placement.**
//!    `mRF_SetBridgeBlock` finds where the river crosses the cliff,
//!    counts river blocks before/after the crossing, picks ONE random
//!    river block before the crossing (and, for two-step towns with a
//!    coin flip, one after) and converts it. A bridge is never placed
//!    on arbitrary water — the brief's "bridge counterpart" model is
//!    exactly right.
//! 3. **The map overlay** uses `pluss_bridge[type]` (verbatim below) to
//!    swap in the bridge variant for rendering when the town-fund
//!    bridge exists in that block.
//!
//! The collision-level bridge masks (`bridge_search_water`, woodb
//! table) live in `terrain_walls.rs`; this module is the town-gen side.

/// Block-type numbers (`m_field_make.h`).
pub mod block_type {
    pub const RIVER_FIRST: u8 = 40;
    pub const RIVER_LAST: u8 = 46;
    pub const RIVER_BRIDGE_FIRST: u8 = 47;
    pub const RIVER_BRIDGE_LAST: u8 = 53;
    /// Enum-arithmetic bridge offset (`RIVER_SOUTH_BRIDGE - RIVER_SOUTH`).
    pub const BRIDGE_OFFSET: u8 = 7;
    pub const TRACKS_RIVER: u8 = 13;
    pub const TRACKS_RIVER_BRIDGE: u8 = 86;
    pub const NONE: u8 = 255;
    pub const TYPE_NUM: usize = 108;
}

/// Is this a plain river block type (40-46)?
pub fn is_river_block(t: u8) -> bool {
    t >= block_type::RIVER_FIRST && t <= block_type::RIVER_LAST
}

/// Is this a bridge-variant block type (47-53)?
pub fn is_bridge_block(t: u8) -> bool {
    t >= block_type::RIVER_BRIDGE_FIRST && t <= block_type::RIVER_BRIDGE_LAST
}

/// Convert a river block type to its bridge counterpart via the
/// enum-arithmetic used by `mRF_SetBridgeBlock`
/// (`type + (RIVER_SOUTH_BRIDGE - RIVER_SOUTH)`).
pub fn river_to_bridge_block(t: u8) -> Option<u8> {
    if is_river_block(t) {
        Some(t + block_type::BRIDGE_OFFSET)
    } else {
        None
    }
}

/// `pluss_bridge` (m_map_ovl.c, verbatim): block type -> bridge counterpart,
/// 255 (NONE) when the type has no bridge variant.
pub const PLUSS_BRIDGE: [u8; 108] = [
    255, 255, 255, 255, 255, 255, 255, 255,
    255, 255, 255, 255, 255,  86, 255, 255,
    255, 255, 255, 255, 255, 255, 255, 255,
     88,  89, 255,  92,  93,  87, 255, 255,
     90,  91, 105, 106, 107, 255, 255, 255,
     47,  48,  49,  50,  51,  52,  53, 255,
    255, 255, 255, 255, 255, 255, 255, 255,
    255, 255, 255, 255, 255, 255, 255, 255,
     82, 255, 255, 255, 255, 255, 255, 255,
    255, 255, 255, 255, 255, 255, 255, 255,
    255, 255, 255, 255, 255, 255, 255, 255,
    255, 255, 255, 255, 255, 255, 255, 255,
    255, 255, 255, 255, 255, 255, 255, 255,
    255, 255, 255, 255,
];

/// Bridge counterpart via the map-overlay table (`pluss_bridge`):
/// 255 (NONE) when the type has no bridge variant.
pub fn bridge_counterpart(t: u8) -> u8 {
    if (t as usize) < PLUSS_BRIDGE.len() {
        PLUSS_BRIDGE[t as usize]
    } else {
        block_type::NONE
    }
}

/// Bridge-block selection (`mRF_SetBridgeBlock` core as a pure
/// kernel): `before`/`after` are the river-block indices on each side
/// of the river/cliff crossing; `pick_before`/`pick_after` are the
/// random selections; `second_bridge_allowed` is
/// `stepmode == TWO && rng != 0`. Returns the chosen indices.
/// The source keeps scanning after a match ("no break?") — the
/// selection is by ordinal among river blocks, so this is exact.
pub fn select_bridge_blocks(
    before: &[usize],
    after: &[usize],
    pick_before: usize,
    pick_after: usize,
    second_bridge_allowed: bool,
) -> (Option<usize>, Option<usize>) {
    let first = if before.is_empty() {
        None
    } else {
        Some(before[pick_before.min(before.len() - 1)])
    };
    let second = if after.is_empty() || !second_bridge_allowed {
        None
    } else {
        Some(after[pick_after.min(after.len() - 1)])
    };
    (first, second)
}

// ---- C ABI ----

/// C ABI: river block -> bridge block, or 255 when not a river type.
#[no_mangle]
pub extern "C" fn pc_river_to_bridge_block(t: u8) -> u8 {
    river_to_bridge_block(t).unwrap_or(block_type::NONE)
}

/// C ABI: map-overlay bridge counterpart (255 = none).
#[no_mangle]
pub extern "C" fn pc_bridge_counterpart(t: u8) -> u8 {
    bridge_counterpart(t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counterpart_arithmetic() {
        // The 7 river types map to the 7 bridge types in order.
        for (river, bridge) in (40u8..=46).zip(47u8..=53) {
            assert_eq!(river_to_bridge_block(river), Some(bridge));
            assert!(is_river_block(river));
            assert!(is_bridge_block(bridge));
        }
        assert_eq!(river_to_bridge_block(39), None);
        assert_eq!(river_to_bridge_block(47), None); // bridge types don't chain
        assert_eq!(river_to_bridge_block(13), None); // tracks river uses the table
        assert_eq!(pc_river_to_bridge_block(40), 47);
        assert_eq!(pc_river_to_bridge_block(99), 255);
    }

    #[test]
    fn pluss_bridge_table_spots() {
        // Verbatim spot checks against m_map_ovl.c.
        assert_eq!(bridge_counterpart(13), 86); // TRACKS_RIVER -> TRACKS_RIVER_BRIDGE
        assert_eq!(bridge_counterpart(40), 47); // RIVER_SOUTH -> RIVER_SOUTH_BRIDGE
        assert_eq!(bridge_counterpart(46), 53);
        assert_eq!(bridge_counterpart(47), 255); // bridge variants map to NONE
        assert_eq!(bridge_counterpart(0), 255);
        assert_eq!(bridge_counterpart(200), 255); // out of range
        assert_eq!(pc_bridge_counterpart(40), 47);
        // The river->bridge arithmetic and the table agree on 40-46.
        for t in 40u8..=46 {
            assert_eq!(bridge_counterpart(t), river_to_bridge_block(t).unwrap());
        }
    }

    #[test]
    fn bridge_selection() {
        let before = vec![3, 7, 11];
        let after = vec![20, 25];
        // Picks are ordinals among river blocks.
        assert_eq!(select_bridge_blocks(&before, &after, 1, 0, true), (Some(7), Some(20)));
        assert_eq!(select_bridge_blocks(&before, &after, 0, 1, false), (Some(3), None));
        assert_eq!(select_bridge_blocks(&[], &after, 0, 0, true), (None, Some(20)));
        assert_eq!(select_bridge_blocks(&before, &[], 2, 0, true), (Some(11), None));
    }
}
