//! Albumin outputs traced to BG geometry, collision, and height.
//!
//! Verified against `src/data/combi/data_combi.c` (17 albumin block
//! types -> BG/FG resources, primary entries + variant counts,
//! extracted programmatically), `m_field_make.c` (mFM_BlockDataSet,
//! mFM_SetBG, mFM_BgUtDataSet 16x16 copy), `m_collision_bg.c`
//! (mCoBG_GetUnitArea, corner*10+base height math), `m_field_make.h`
//! (mFM_bg_data_c, mFM_bg_info_c, mFM_combination_c) and
//! `m_random_field_ovl.c` (albumin tables, trace legality)
//! (USA Rev. 0 decomp / PC port).
//!
//! Chain: albumin block type -> data_combi_table -> bg_id -> data_bgd
//! -> collision[16][16] + acre base height -> world Y.
//! The numeric 16x16 collision values live in the disc resource
//! data_bgd, NOT in the decomp source; this module models the full
//! architecture but marks those values as resource-side (unknown).

/// Albumin block type id -> (primary BG asset, primary FG asset, variant count).
/// BG/FG names as in data_combi.c; variants = number of entries of that type.
pub struct AlbuminAsset {
    pub block_type: u8,
    pub bg: &'static str,
    pub fg: &'static str,
    pub variants: u8,
}

/// The 17 albumin outputs with their authored assets, in table order:
/// south row (7), east row (5), west row (5).
pub const ALBUMIN_ASSETS: [AlbuminAsset; 17] = [
    AlbuminAsset { block_type: 22, bg: "GRD_S_C1_R1_1", fg: "GRD_S_C1_R1_1_29", variants: 3 },
    AlbuminAsset { block_type: 23, bg: "GRD_S_C2_R1_1", fg: "GRD_S_C2_R1_1", variants: 2 },
    AlbuminAsset { block_type: 24, bg: "GRD_S_C3_R1_1", fg: "GRD_S_C3_R1_1", variants: 2 },
    AlbuminAsset { block_type: 25, bg: "GRD_S_C4_R1_1", fg: "GRD_S_C4_R1_1", variants: 2 },
    AlbuminAsset { block_type: 26, bg: "GRD_S_C5_R1_1", fg: "GRD_S_C5_R1_1", variants: 2 },
    AlbuminAsset { block_type: 27, bg: "GRD_S_C6_R1_1", fg: "GRD_S_C6_R1_1", variants: 2 },
    AlbuminAsset { block_type: 28, bg: "GRD_S_C7_R1_1", fg: "GRD_S_C7_R1_1", variants: 2 },
    AlbuminAsset { block_type: 29, bg: "GRD_S_C1_R2_1", fg: "GRD_S_C1_R2_1", variants: 3 },
    AlbuminAsset { block_type: 30, bg: "GRD_S_C2_R2_1", fg: "GRD_S_C2_R2_1", variants: 2 },
    AlbuminAsset { block_type: 31, bg: "GRD_S_C3_R2_1", fg: "GRD_S_C3_R2_1", variants: 2 },
    AlbuminAsset { block_type: 32, bg: "GRD_S_C4_R2_1", fg: "GRD_S_C4_R2_1", variants: 2 },
    AlbuminAsset { block_type: 33, bg: "GRD_S_C5_R2_1", fg: "GRD_S_C5_R2_1", variants: 2 },
    AlbuminAsset { block_type: 34, bg: "GRD_S_C1_R3_1", fg: "GRD_S_C1_R3_1", variants: 3 },
    AlbuminAsset { block_type: 35, bg: "GRD_S_C4_R3_1", fg: "GRD_S_C4_R3_1", variants: 2 },
    AlbuminAsset { block_type: 36, bg: "GRD_S_C5_R3_1", fg: "GRD_S_C5_R3_1", variants: 2 },
    AlbuminAsset { block_type: 37, bg: "GRD_S_C6_R3_1", fg: "GRD_S_C6_R3_1", variants: 1 },
    AlbuminAsset { block_type: 38, bg: "GRD_S_C7_R3_1", fg: "GRD_S_C7_R3_1", variants: 2 },
];

/// Look up the authored assets for an albumin block type.
pub fn albumin_asset(block_type: u8) -> Option<&'static AlbuminAsset> {
    ALBUMIN_ASSETS.iter().find(|a| a.block_type == block_type)
}

/// Whether a block type is a waterfall-family albumin output
/// (types 22,23,26,30,31,37,38).
pub fn is_waterfall_output(block_type: u8) -> bool {
    matches!(block_type, 22 | 23 | 26 | 30 | 31 | 37 | 38)
}

/// BG collision grid dimensions: 16x16 units per acre.
pub const UT_X_NUM: usize = 16;
pub const UT_Z_NUM: usize = 16;

/// Corner-height scale: collision height units -> world units (x10),
/// plus the acre base height: world_y = corner * 10.0 + base_height.
pub const COLLISION_HEIGHT_SCALE: f32 = 10.0;

/// Acre base height bit layout in mFM_combination_c:
/// combination_type : 14 bits, height : 2 bits.
pub const COMBI_TYPE_BITS: u8 = 14;
pub const COMBI_HEIGHT_BITS: u8 = 2;

/// World Y from a collision corner height value and the acre base height:
/// world_y = corner * 10.0 + base_height (mCoBG math).
pub fn world_y(corner: i8, base_height: f32) -> f32 {
    corner as f32 * COLLISION_HEIGHT_SCALE + base_height
}

// ---- C ABI ----

/// C ABI: returns the primary BG asset index (0..16) for an albumin
/// block type, or 255 if not an albumin output.
#[no_mangle]
pub extern "C" fn pc_albumin_asset_idx(block_type: u8) -> u8 {
    ALBUMIN_ASSETS
        .iter()
        .position(|a| a.block_type == block_type)
        .map(|i| i as u8)
        .unwrap_or(255)
}

/// C ABI: 1 if the albumin output is waterfall-family.
#[no_mangle]
pub extern "C" fn pc_is_waterfall_output(block_type: u8) -> u8 {
    is_waterfall_output(block_type) as u8
}

/// C ABI: variant count for an albumin block type (0 if unknown).
#[no_mangle]
pub extern "C" fn pc_albumin_variants(block_type: u8) -> u8 {
    albumin_asset(block_type).map(|a| a.variants).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn albumin_geometry_trace() {
        assert_eq!(ALBUMIN_ASSETS.len(), 17);
        // Primary entries match data_combi.c extraction.
        let a = albumin_asset(22).unwrap();
        assert_eq!(a.bg, "GRD_S_C1_R1_1");
        assert_eq!(a.fg, "GRD_S_C1_R1_1_29");
        assert_eq!(a.variants, 3);
        assert_eq!(albumin_asset(37).unwrap().variants, 1);
        assert!(albumin_asset(40).is_none());
        // Waterfall family: 7 of 17.
        let wf: Vec<u8> = (22..=38).filter(|&t| is_waterfall_output(t)).collect();
        assert_eq!(wf.len(), 7);
        assert!(is_waterfall_output(22));
        assert!(!is_waterfall_output(24));
        // Grid + height math.
        assert_eq!((UT_X_NUM, UT_Z_NUM), (16, 16));
        assert_eq!((COMBI_TYPE_BITS, COMBI_HEIGHT_BITS), (14, 2));
        assert_eq!(world_y(5, 100.0), 150.0);
        // C ABI.
        assert_eq!(pc_albumin_asset_idx(22), 0);
        assert_eq!(pc_albumin_asset_idx(38), 16);
        assert_eq!(pc_albumin_asset_idx(40), 255);
        assert_eq!(pc_is_waterfall_output(30), 1);
        assert_eq!(pc_albumin_variants(34), 3);
    }
}
