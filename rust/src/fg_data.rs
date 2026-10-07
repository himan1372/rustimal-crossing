//! FG template data: format, parser, and combination statistics.
//!
//! Verified against `m_field_make.c`, `m_field_make.h`,
//! `m_name_table.h`, `src/data/combi/data_combi.c`,
//! `src/static/jsyswrap.cpp` (USA Rev. 0 decomp / PC port).
//!
//! KEY FINDING: the per-template SIGN00-SIGN20 house-lot locations
//! live in `fgdata.bin`, a binary asset inside `forest_1st.arc` on the
//! game disc. It is NOT in the decomp source tree — the decomp only
//! carries the C structs and the combination table. This module
//! therefore provides:
//!   1. the exact binary record format (from `mFM_fg_data_c`),
//!   2. a parser that extracts SIGN locations per FG template,
//!   3. the source-derived combination statistics (which block types
//!      have how many FG variants).
//!
//! To finish the house-lot distribution: extract `fgdata.bin` from
//! `forest_1st.arc` (game disc), run `parse_fg_records` over it, and
//! join the SIGN locations with the combination table below.

/// `mFM_fg_data_c` binary layout (big-endian on disc; the PC port
/// byte-swaps u16s via `mFM_ByteSwapFGData` after loading).
pub const FG_RECORD_SIZE: usize = 518; // 2 + 256*2 + 4
pub const FG_ITEMS: usize = 256; // 16x16, row-major (z, x)

/// SIGN00-SIGN20 item IDs (`STRUCTURE_START + 16 + idx`).
pub const SIGN_FIRST: u16 = 0x5810;
pub const SIGN_LAST: u16 = 0x5824;

/// A parsed FG template record.
#[derive(Clone, Debug)]
pub struct FgRecord {
    pub fg_id: u16,
    pub items: [u16; FG_ITEMS], // items[z * 16 + x]
    pub haniwa_step: [u8; 4],
}

/// Parse big-endian `fgdata.bin` bytes into records.
/// Returns an error string on truncated input.
pub fn parse_fg_records(data: &[u8]) -> Result<Vec<FgRecord>, &'static str> {
    if data.len() % FG_RECORD_SIZE != 0 {
        return Err("fgdata.bin size is not a multiple of 518");
    }
    let mut out = Vec::with_capacity(data.len() / FG_RECORD_SIZE);
    for chunk in data.chunks_exact(FG_RECORD_SIZE) {
        let fg_id = u16::from_be_bytes([chunk[0], chunk[1]]);
        let mut items = [0u16; FG_ITEMS];
        for (i, slot) in items.iter_mut().enumerate() {
            let o = 2 + i * 2;
            *slot = u16::from_be_bytes([chunk[o], chunk[o + 1]]);
        }
        let haniwa_step = [chunk[514], chunk[515], chunk[516], chunk[517]];
        out.push(FgRecord { fg_id, items, haniwa_step });
    }
    Ok(out)
}

/// SIGN marker locations in a record: (sign_index 0-20, unit_x, unit_z).
pub fn sign_locations(rec: &FgRecord) -> Vec<(u8, u8, u8)> {
    let mut out = Vec::new();
    for (i, &item) in rec.items.iter().enumerate() {
        if (SIGN_FIRST..=SIGN_LAST).contains(&item) {
            out.push(((item - SIGN_FIRST) as u8, (i % 16) as u8, (i / 16) as u8));
        }
    }
    out
}

/// Combination-table statistics derived from `data_combi.c`:
/// (block type id, combination count). Block type ids are the
/// `mFM_BLOCK_TYPE_*` enum values. 368 combinations total.
pub const COMBI_COUNTS: &[(u8, u16)] = &[
    (255, 92), // NONE (unused/special)
    (104, 26), // OCEAN_8
    (83, 19),  // OCEAN
    (39, 10),  // FLAT
    (63, 10),  // BEACH
    (13, 5),   // TRACKS_RIVER
    (15, 5),   // CLIFF_HORIZONTAL
    (64, 5),   // BEACH_RIVER
    (97, 5),   // OCEAN_5 (enum value verified: 97)
    (54, 4),   // SLOPE_HORIZONTAL
    (40, 4),   // RIVER_SOUTH
    (41, 4),   // RIVER_EAST
    (42, 4),   // RIVER_WEST
    (99, 4),   // ISLAND_RIGHT
    (98, 4),   // ISLAND_LEFT
];

/// FG variant counts for the block types most likely to contain house
/// lots (from `data_combi.c`).
pub mod house_lot_candidates {
    /// 10 FG variants: GRD_S_F_1_2F, GRD_S_F_2 .. GRD_S_F_10.
    pub const FLAT_FG_COUNT: usize = 10;
    /// 10 FG variants: 0061, 0062, GRD_S_M_3 .. GRD_S_M_10.
    pub const BEACH_FG_COUNT: usize = 10;
    /// 4 FG variants: GRD_S_R1_1 .. GRD_S_R1_4.
    pub const RIVER_SOUTH_FG_COUNT: usize = 4;
}

/// Total combinations and distinct FG types in `data_combi.c`.
pub const COMBI_TOTAL: usize = 368;
pub const DISTINCT_FG_TYPES: usize = 267;

/// Join helper: for a block type, the combination table gives the FG
/// ids; this maps an FG id to its SIGN locations once `fgdata.bin` is
/// parsed. `fg_index` maps fg_id -> record index in the parsed vec.
pub fn build_fg_index(records: &[FgRecord]) -> std::collections::HashMap<u16, usize> {
    records.iter().enumerate().map(|(i, r)| (r.fg_id, i)).collect()
}

// ---- C ABI ----

/// C ABI: parse result code for a buffer. Returns record count, or
/// u32::MAX on format error.
#[no_mangle]
pub extern "C" fn pc_fg_parse_count(data: *const u8, len: usize) -> u32 {
    if data.is_null() {
        return u32::MAX;
    }
    let bytes = unsafe { std::slice::from_raw_parts(data, len) };
    match parse_fg_records(bytes) {
        Ok(recs) => recs.len() as u32,
        Err(_) => u32::MAX,
    }
}

/// C ABI: count SIGN markers in one 518-byte record.
/// Returns 255 on format error.
#[no_mangle]
pub extern "C" fn pc_fg_sign_count(record: *const u8) -> u8 {
    if record.is_null() {
        return 255;
    }
    let bytes = unsafe { std::slice::from_raw_parts(record, FG_RECORD_SIZE) };
    match parse_fg_records(bytes) {
        Ok(recs) if recs.len() == 1 => sign_locations(&recs[0]).len() as u8,
        _ => 255,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_record() -> Vec<u8> {
        let mut buf = vec![0u8; FG_RECORD_SIZE];
        // fg_id = 0x0004 big-endian
        buf[0] = 0x00;
        buf[1] = 0x04;
        // SIGN05 at (x=3, z=7) -> item index 7*16+3 = 115, offset 2+230
        let o = 2 + 115 * 2;
        buf[o] = 0x58;
        buf[o + 1] = 0x15; // 0x5815 = SIGN05
        // SIGN20 at (x=0, z=0)
        buf[2] = 0x58;
        buf[3] = 0x24;
        buf[514] = 7; // haniwa_step untouched by swap
        buf
    }

    #[test]
    fn parse_and_signs() {
        let buf = test_record();
        let recs = parse_fg_records(&buf).unwrap();
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].fg_id, 0x0004);
        assert_eq!(recs[0].haniwa_step[0], 7);
        let signs = sign_locations(&recs[0]);
        assert_eq!(signs.len(), 2);
        assert!(signs.contains(&(5, 3, 7)));
        assert!(signs.contains(&(20, 0, 0)));
        // Truncated input errors.
        assert!(parse_fg_records(&buf[..100]).is_err());
        // Sign range constants.
        assert_eq!(SIGN_FIRST, 0x5810);
        assert_eq!(SIGN_LAST, 0x5824);
        assert_eq!(FG_RECORD_SIZE, 518);
        // C ABI.
        assert_eq!(pc_fg_parse_count(buf.as_ptr(), buf.len()), 1);
        assert_eq!(pc_fg_parse_count(buf.as_ptr(), 100), u32::MAX);
        assert_eq!(pc_fg_sign_count(buf.as_ptr()), 2);
    }

    #[test]
    fn combi_stats() {
        assert_eq!(COMBI_TOTAL, 368);
        assert_eq!(DISTINCT_FG_TYPES, 267);
        // FLAT -> 10 combos is the headline house-lot number.
        let flat = COMBI_COUNTS.iter().find(|&&(t, _)| t == 39).unwrap();
        assert_eq!(flat.1, 10);
        assert_eq!(house_lot_candidates::FLAT_FG_COUNT, 10);
        assert_eq!(house_lot_candidates::BEACH_FG_COUNT, 10);
    }
}
