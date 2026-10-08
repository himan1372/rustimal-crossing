//! Template anti-reuse: concrete BG/FG combination selection.
//!
//! Verified against `m_random_field_ovl.c` (mRF_SelectBlock,
//! mRF_TypeCombCount, mRF_IndexInType2BlockNo, mRF_SearchAlreadyUse,
//! mRF_BgName2RandomConbiNo), `src/data/combi/data_combi.c`
//! (368 entries, 92 NONE, per-type counts extracted programmatically),
//! `m_combi_type.h` (BLOCK_COMBI_GRD_S_F_7 = 161)
//! (USA Rev. 0 decomp / PC port).
//!
//! Anti-reuse operates on concrete data_combi_table indices, not
//! semantic block types. l_use_data[70] is reset to -1 at the start of
//! each SelectBlock; duplicate detection is exact index equality over
//! all 70 slots. Selection is uniform among unused candidates of the
//! requested type; when exhausted, reuse is allowed. SEA_EXCEPTIONAL
//! bypasses anti-reuse (BG-name path, no use_data update) and carries
//! the retail mRF_GetRandom(0) bug (PC port BUGFIXES uses count).

/// Anti-reuse table length (BLOCK_TOTAL_NUM).
pub const USE_DATA_LEN: usize = 70;
/// Unused marker.
pub const USE_DATA_EMPTY: i16 = -1;
/// Defensive fallback (BLOCK_COMBI_GRD_S_F_7).
pub const FALLBACK_COMBI: u16 = 161;
/// Combi count (data_combi_table entries).
pub const COMBI_COUNT: usize = 368;

/// Fresh anti-reuse table (mRF_SetShortData(l_use_data, -1, 70)).
pub fn use_data_reset() -> [i16; USE_DATA_LEN] {
    [USE_DATA_EMPTY; USE_DATA_LEN]
}

/// Duplicate test (mRF_SearchAlreadyUse): exact index equality over all slots.
pub fn search_already_use(use_data: &[i16; USE_DATA_LEN], value: i16) -> bool {
    use_data.iter().any(|&v| v == value)
}

/// Candidate count (mRF_TypeCombCount). `types[i]` = semantic type of
/// combi entry i. reuse=false excludes already-used indices.
pub fn type_comb_count(types: &[u8], ty: u8, use_data: &[i16; USE_DATA_LEN], reuse: bool) -> usize {
    types
        .iter()
        .enumerate()
        .filter(|(i, t)| {
            **t == ty && (reuse || !search_already_use(use_data, *i as i16))
        })
        .count()
}

/// Index lookup (mRF_IndexInType2BlockNo): the idx-th candidate in table
/// order. Returns None when idx is out of range (source returns -1).
pub fn index_in_type_2_block_no(
    types: &[u8],
    ty: u8,
    idx: usize,
    use_data: &[i16; USE_DATA_LEN],
    reuse: bool,
) -> Option<usize> {
    let mut count = 0usize;
    for (i, t) in types.iter().enumerate() {
        if *t == ty && (reuse || !search_already_use(use_data, i as i16)) {
            if count == idx {
                return Some(i);
            }
            count += 1;
        }
    }
    None
}

/// One block's selection (mRF_SelectBlock inner logic, minus the
/// SEA_EXCEPTIONAL branch). `rand_n(n)` must emulate mRF_GetRandom(n).
/// Returns (combination_index, mark_used).
pub fn select_block_type(
    types: &[u8],
    ty: u8,
    use_data: &[i16; USE_DATA_LEN],
    mut rand_n: impl FnMut(usize) -> usize,
) -> (u16, bool) {
    let type_count = type_comb_count(types, ty, use_data, false);
    if type_count != 0 {
        let selected = rand_n(type_count);
        if let Some(block_no) = index_in_type_2_block_no(types, ty, selected, use_data, false) {
            return (block_no as u16, true);
        }
        // Defensive: fall through to reuse pool (source duplicates this).
    }
    let all = type_comb_count(types, ty, use_data, true);
    let selected = rand_n(all);
    match index_in_type_2_block_no(types, ty, selected, use_data, true) {
        Some(block_no) => (block_no as u16, true),
        None => (FALLBACK_COMBI, false),
    }
}

/// SEA_EXCEPTIONAL BG-name selection (mRF_BgName2RandomConbiNo).
/// `bg_ids[i]` = bg asset of combi entry i; `types[i]` = its semantic
/// type (NONE entries excluded). `retail_bug`: true = mRF_GetRandom(0)
/// (always first match); false = BUGFIXES mRF_GetRandom(count).
/// Returns combi_count when no match (source returns combi_count).
pub fn bg_name_2_random_combi_no(
    bg_ids: &[u32],
    types: &[u8],
    none_type: u8,
    bg_name: u32,
    combi_count: usize,
    retail_bug: bool,
    mut rand_n: impl FnMut(usize) -> usize,
) -> usize {
    let matches: Vec<usize> = bg_ids
        .iter()
        .zip(types.iter())
        .enumerate()
        .filter(|(_, (b, t))| **b == bg_name && **t != none_type)
        .map(|(i, _)| i)
        .collect();
    if matches.is_empty() {
        return combi_count;
    }
    let selected = if retail_bug {
        rand_n(0)
    } else {
        rand_n(matches.len())
    };
    // NOTE: with retail_bug, selected is rand_n(0)'s result; the source
    // then walks to the selected-th match. Clamp defensively.
    matches[selected.min(matches.len() - 1)]
}

/// Traversal order of mRF_SelectBlock: Z-major, X-minor over 7x10.
pub fn select_traversal() -> impl Iterator<Item = (usize, usize)> {
    (0..10).flat_map(|bz| (0..7).map(move |bx| (bx, bz)))
}

// ---- C ABI ----

/// C ABI: candidate count. types_ptr = semantic type per combi entry.
#[no_mangle]
pub extern "C" fn pc_type_comb_count(
    types_ptr: *const u8,
    combi_count: usize,
    ty: u8,
    use_data_ptr: *const i16,
    reuse: u8,
) -> usize {
    if types_ptr.is_null() || use_data_ptr.is_null() {
        return 0;
    }
    let types = unsafe { std::slice::from_raw_parts(types_ptr, combi_count) };
    let mut use_data = [USE_DATA_EMPTY; USE_DATA_LEN];
    let src = unsafe { std::slice::from_raw_parts(use_data_ptr, USE_DATA_LEN) };
    use_data.copy_from_slice(src);
    type_comb_count(types, ty, &use_data, reuse != 0)
}

/// C ABI: duplicate test.
#[no_mangle]
pub extern "C" fn pc_search_already_use(use_data_ptr: *const i16, value: i16) -> u8 {
    if use_data_ptr.is_null() {
        return 0;
    }
    let src = unsafe { std::slice::from_raw_parts(use_data_ptr, USE_DATA_LEN) };
    let mut use_data = [USE_DATA_EMPTY; USE_DATA_LEN];
    use_data.copy_from_slice(src);
    search_already_use(&use_data, value) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tiny fake combi table: types per index.
    fn fake_types() -> Vec<u8> {
        // idx: 0 FLAT, 1 FLAT, 2 RIVER_SOUTH, 3 FLAT, 4 NONE
        vec![39, 39, 40, 39, 109]
    }

    #[test]
    fn anti_reuse() {
        let types = fake_types();
        let mut used = use_data_reset();
        assert!(used.iter().all(|&v| v == -1));
        assert!(!search_already_use(&used, 0));
        // Count unused FLAT: 3.
        assert_eq!(type_comb_count(&types, 39, &used, false), 3);
        assert_eq!(type_comb_count(&types, 39, &used, true), 3);
        // Select first FLAT with deterministic rand.
        let (c0, mark) = select_block_type(&types, 39, &used, |_| 0);
        assert_eq!((c0, mark), (0, true));
        used[0] = c0 as i16;
        assert!(search_already_use(&used, 0));
        assert_eq!(type_comb_count(&types, 39, &used, false), 2);
        // Index lookup skips used.
        assert_eq!(index_in_type_2_block_no(&types, 39, 0, &used, false), Some(1));
        assert_eq!(index_in_type_2_block_no(&types, 39, 5, &used, false), None);
        // Exhaust FLAT, then reuse allowed.
        used[1] = 1;
        used[2] = 3;
        assert_eq!(type_comb_count(&types, 39, &used, false), 0);
        let (c1, _) = select_block_type(&types, 39, &used, |_| 0);
        assert_eq!(c1, 0); // reuse pool
        // NONE type never selected as FLAT.
        assert_eq!(type_comb_count(&types, 109, &used, false), 1);
        // Fallback when no candidates at all.
        let (c2, mark2) = select_block_type(&types, 41, &used, |_| 0);
        assert_eq!((c2, mark2), (FALLBACK_COMBI, false));
        assert_eq!(FALLBACK_COMBI, 161);
        // Traversal order: Z-major, X-minor.
        let trav: Vec<_> = select_traversal().take(8).collect();
        assert_eq!(trav[0], (0, 0));
        assert_eq!(trav[6], (6, 0));
        assert_eq!(trav[7], (0, 1));
        // SEA_EXCEPTIONAL BG-name path.
        let bg_ids = vec![100u32, 100, 200, 100];
        let btypes = vec![39u8, 39, 39, 109];
        // retail bug: rand_n(0) -> 0 -> first match.
        let r = bg_name_2_random_combi_no(&bg_ids, &btypes, 109, 100, 4, true, |_| 0);
        assert_eq!(r, 0);
        // bugfixes: uniform among matches (indices 0,1).
        let r2 = bg_name_2_random_combi_no(&bg_ids, &btypes, 109, 100, 4, false, |_| 1);
        assert_eq!(r2, 1);
        // no match -> combi_count.
        let r3 = bg_name_2_random_combi_no(&bg_ids, &btypes, 109, 999, 4, false, |_| 0);
        assert_eq!(r3, 4);
        // C ABI.
        let mut u = use_data_reset();
        u[5] = 3;
        assert_eq!(pc_search_already_use(u.as_ptr(), 3), 1);
        assert_eq!(pc_search_already_use(u.as_ptr(), 4), 0);
        assert_eq!(pc_type_comb_count(types.as_ptr(), 5, 39, u.as_ptr(), 0), 2);
    }
}
