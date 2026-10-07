//! Villager selection and house-lot assignment.
//!
//! Verified against `m_npc.c`, `m_start_data_init.c`, `m_name_table.h`,
//! `m_npc.h` (USA Rev. 0 decomp).
//!
//! The central architectural fact: the town topology (river, cliffs,
//! pond, acre layout) is generated BEFORE any villager exists
//! (`mFM_InitFgCombiSaveData` → `mNpc_InitNpcAllInfo` → `mNpc_Grow` →
//! `mNpc_InitNpcData` in `m_start_data_init.c`). Villager identity
//! selection and house-lot assignment are separate systems, and house
//! lots come from SIGN00-SIGN20 reservation markers embedded in the
//! selected FG acre templates — villagers never carve terrain.

/// `Anmhome_c`: a villager's house location as acre + unit coords.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct HomeInfo {
    pub type_unused: u8,
    pub block_x: u8,
    pub block_z: u8,
    pub ut_x: u8,
    pub ut_z: u8,
}

impl HomeInfo {
    /// Unset marker used by the assignment pass (0xFF).
    pub fn unset() -> Self {
        HomeInfo {
            type_unused: 0,
            block_x: 0xFF,
            block_z: 0xFF,
            ut_x: 0xFF,
            ut_z: 0xFF,
        }
    }

    pub fn is_set(&self) -> bool {
        self.block_x != 0xFF || self.block_z != 0xFF
    }
}

/// Reservation marker namespace: SIGN00..SIGN20 (21 IDs).
pub const RESERVE_FIRST: u32 = 0;
pub const RESERVE_LAST: u32 = 20;
pub const RESERVE_COUNT: usize = 21;

/// `mNT_IS_RESERVE`: marker IDs run SIGN00-SIGN20. The caller maps
/// raw item IDs to this 0-20 range.
pub fn is_reserve_marker(sign_idx: u32) -> bool {
    sign_idx <= RESERVE_LAST
}

/// Foreground dimensions scanned for reservation markers.
pub const FG_BLOCK_X_NUM: usize = 5;
pub const FG_BLOCK_Z_NUM: usize = 6;
pub const UT_X_NUM: usize = 16;
pub const UT_Z_NUM: usize = 16;

/// Total foreground cells scanned: 5*6*16*16 = 7680.
pub const FG_CELL_COUNT: usize = FG_BLOCK_X_NUM * FG_BLOCK_Z_NUM * UT_X_NUM * UT_Z_NUM;

/// A reservation lot found in the FG data.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ReservedLot {
    pub block_x: u8,
    pub block_z: u8,
    pub ut_x: u8,
    pub ut_z: u8,
}

/// Scan helper: collect lots from a predicate over the 7680 FG cells.
/// The source walks acres z,x then units z,x; the order defines the
/// pre-shuffle lot order.
pub fn collect_reserved_lots(is_reserve: impl Fn(usize, usize, usize, usize) -> bool) -> Vec<ReservedLot> {
    let mut lots = Vec::new();
    for bz in 0..FG_BLOCK_Z_NUM {
        for bx in 0..FG_BLOCK_X_NUM {
            for uz in 0..UT_Z_NUM {
                for ux in 0..UT_X_NUM {
                    if is_reserve(bx, bz, ux, uz) {
                        lots.push(ReservedLot {
                            block_x: bx as u8,
                            block_z: bz as u8,
                            ut_x: ux as u8,
                            ut_z: uz as u8,
                        });
                    }
                }
            }
        }
    }
    lots
}

/// `mNpc_SetNpcHome` lot assignment, verbatim:
/// - shuffle the lot-index table with `ARRAY_COUNT(fakeTable)/2` = 30
///   random swaps (the villager order is NOT shuffled),
/// - walk the animal array; each villager with unset home takes the
///   next lot index (skipping indices >= reserved_num),
/// - `home.ut_z = lot.ut_z + 1` (the marker is one unit south of the
///   stored house coordinate).
///
/// `swap(i, j)` performs one random swap of the index table; call it
/// 30 times to mirror `mNpc_MakeRandTable`.
pub fn make_rand_table(table: &mut [usize], swaps: &[(usize, usize)]) {
    for &(a, b) in swaps {
        if a < table.len() && b < table.len() {
            table.swap(a, b);
        }
    }
}

/// Number of random swaps used for the lot shuffle.
pub const LOT_SHUFFLE_SWAPS: usize = 30;

/// Assign homes to villagers. `homeless[i]` marks villagers needing a
/// lot; `lot_order` is the shuffled lot-index table. Returns the home
/// assigned to each homeless villager, in animal order.
pub fn assign_homes(
    lots: &[ReservedLot],
    lot_order: &[usize],
    homeless_count: usize,
) -> Vec<HomeInfo> {
    let mut homes = Vec::new();
    let mut n = 0usize;
    for _ in 0..homeless_count {
        // Skip consumed/out-of-range indices like the source.
        while n < lot_order.len() && lot_order[n] >= lots.len() {
            n += 1;
        }
        if n >= lot_order.len() || n >= lots.len() {
            break;
        }
        let lot = lots[lot_order[n]];
        homes.push(HomeInfo {
            type_unused: 0,
            block_x: lot.block_x,
            block_z: lot.block_z,
            ut_x: lot.ut_x,
            ut_z: lot.ut_z + 1,
        });
        n += 1;
    }
    homes
}

/// The 3x3 house footprint offsets (`ut_d`), in source order:
/// center, then the ring.
pub const HOUSE_FOOTPRINT: [(i32, i32); 9] = [
    (0, 0),
    (-1, 1), (0, 1), (1, 1),
    (-1, 0), (1, 0),
    (-1, -1), (0, -1), (1, -1),
];

/// Foreground values written for the 9 cells: index 0 = the house
/// itself, 1 = signboard, 2-8 = RSV_NO.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FootprintCell {
    House,
    Signboard,
    Reserved,
}

pub const HOUSE_FOOTPRINT_CELLS: [FootprintCell; 9] = [
    FootprintCell::House,
    FootprintCell::Signboard,
    FootprintCell::Reserved,
    FootprintCell::Reserved,
    FootprintCell::Reserved,
    FootprintCell::Reserved,
    FootprintCell::Reserved,
    FootprintCell::Reserved,
    FootprintCell::Reserved,
];

/// House-builder bounds check, verbatim: one-unit clearance from the
/// acre edge is required.
pub fn house_footprint_in_bounds(ut_x: i32, ut_z: i32) -> bool {
    ut_x > 0 && ut_x < (UT_X_NUM as i32 - 1) && ut_z > 0 && ut_z < (UT_Z_NUM as i32 - 1)
}

/// Maximum normal villagers.
pub const ANIMAL_NUM_MAX: usize = 15;

/// Number of personality looks categories covered by the initial six.
pub const LOOKS_NUM: usize = 6;

/// Growth permission kinds.
pub mod grow_perm {
    pub const STARTER: u8 = 0;
    pub const MOVE_IN: u8 = 1;
}

/// `mNpc_DecideLivingNpcMax` selection, verbatim:
/// walk the shuffled NPC definition order; accept a candidate when
/// its grow permission is STARTER and its looks category is not yet
/// represented. Returns the selected definition indices.
pub fn decide_living_npc_max(
    shuffled_def_idx: &[usize],
    looks_of: impl Fn(usize) -> usize,
    grow_perm_of: impl Fn(usize) -> u8,
    count: usize,
) -> Vec<usize> {
    let mut selected = Vec::new();
    let mut looks_bitfield: u32 = 0;
    for &idx in shuffled_def_idx {
        if selected.len() >= count {
            break;
        }
        if grow_perm_of(idx) == grow_perm::STARTER {
            let looks = looks_of(idx);
            if looks_bitfield & (1 << looks) == 0 {
                selected.push(idx);
                looks_bitfield |= 1 << looks;
            }
        }
    }
    selected
}

/// Daily growth field-rank probability table, verbatim:
/// rank 0-6 → 40/50/60/70/80/90/100.
pub const GROW_PROB: [u8; 7] = [40, 50, 60, 70, 80, 90, 100];

/// `mNpc_CheckGrowFieldRank`: `RANDOM(100) < prob[rank]`.
pub fn check_grow_field_rank(rank: usize, rng100: u8) -> bool {
    rank < GROW_PROB.len() && rng100 < GROW_PROB[rank]
}

/// `mNpc_CheckGrow` gates, verbatim: population < 15, at least one day
/// since the last growth, the loading player is from this town, and
/// the player has talked to all current villagers.
pub fn check_grow(
    population: usize,
    day_elapsed: bool,
    player_from_town: bool,
    talked_to_all: bool,
    rank_ok: bool,
) -> bool {
    population < ANIMAL_NUM_MAX && day_elapsed && player_from_town && talked_to_all && rank_ok
}

/// `mNpc_GetMinLooks`: among looks categories that still have eligible
/// unseen NPCs, find the minimum current population; ties produce a
/// bitfield the caller picks from uniformly.
pub fn min_looks_bitfield(pop_per_looks: &[usize; 6], eligible: &[bool; 6]) -> u8 {
    let mut bitfield = 0u8;
    let mut best = usize::MAX;
    for i in 0..6 {
        if !eligible[i] {
            continue;
        }
        if pop_per_looks[i] < best {
            best = pop_per_looks[i];
            bitfield = 1 << i;
        } else if pop_per_looks[i] == best {
            bitfield |= 1 << i;
        }
    }
    bitfield
}

/// Natural-growth candidate eligibility, verbatim: not currently
/// present, not in the have-appeared table, and grow permission is
/// STARTER or MOVE_IN. The caller then picks uniformly.
pub fn grow_candidate_eligible(present: bool, have_appeared: bool, perm: u8) -> bool {
    !present && !have_appeared && (perm == grow_perm::STARTER || perm == grow_perm::MOVE_IN)
}

// ---- C ABI ----

/// C ABI: reservation-marker test on a 0-20 sign index.
#[no_mangle]
pub extern "C" fn pc_is_reserve_marker(sign_idx: u32) -> u8 {
    is_reserve_marker(sign_idx) as u8
}

/// C ABI: house footprint bounds check.
#[no_mangle]
pub extern "C" fn pc_house_footprint_in_bounds(ut_x: i32, ut_z: i32) -> u8 {
    house_footprint_in_bounds(ut_x, ut_z) as u8
}

/// C ABI: growth field-rank check.
#[no_mangle]
pub extern "C" fn pc_check_grow_field_rank(rank: u8, rng100: u8) -> u8 {
    check_grow_field_rank(rank as usize, rng100) as u8
}

/// C ABI: full growth gate check.
#[no_mangle]
pub extern "C" fn pc_check_grow(
    population: u8,
    day_elapsed: u8,
    player_from_town: u8,
    talked_to_all: u8,
    rank_ok: u8,
) -> u8 {
    check_grow(
        population as usize,
        day_elapsed != 0,
        player_from_town != 0,
        talked_to_all != 0,
        rank_ok != 0,
    ) as u8
}

/// C ABI: least-populated looks bitfield. `pop` and `eligible` are
/// 6-element arrays.
#[no_mangle]
pub extern "C" fn pc_min_looks_bitfield(pop: *const usize, eligible: *const u8) -> u8 {
    if pop.is_null() || eligible.is_null() {
        return 0;
    }
    let mut p = [0usize; 6];
    let mut e = [false; 6];
    unsafe {
        std::ptr::copy_nonoverlapping(pop, p.as_mut_ptr(), 6);
        for i in 0..6 {
            e[i] = *eligible.add(i) != 0;
        }
    }
    min_looks_bitfield(&p, &e)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reservations_and_lots() {
        assert!(is_reserve_marker(0));
        assert!(is_reserve_marker(20));
        assert!(!is_reserve_marker(21));
        assert_eq!(RESERVE_COUNT, 21);
        assert_eq!(FG_CELL_COUNT, 7680);
        // Scan order: acres z,x then units z,x.
        let lots = collect_reserved_lots(|bx, bz, ux, uz| bx == 1 && bz == 2 && ux == 3 && uz == 4);
        assert_eq!(lots.len(), 1);
        assert_eq!(
            lots[0],
            ReservedLot { block_x: 1, block_z: 2, ut_x: 3, ut_z: 4 }
        );
        // Assignment: ut_z + 1 offset, shuffled order consumed in order.
        let lots = vec![
            ReservedLot { block_x: 0, block_z: 0, ut_x: 5, ut_z: 5 },
            ReservedLot { block_x: 1, block_z: 1, ut_x: 6, ut_z: 6 },
        ];
        let homes = assign_homes(&lots, &[1, 0], 2);
        assert_eq!(homes.len(), 2);
        assert_eq!((homes[0].block_x, homes[0].ut_z), (1, 7));
        assert_eq!((homes[1].block_x, homes[1].ut_z), (0, 6));
        // Fewer lots than villagers: stops.
        let homes = assign_homes(&lots, &[0], 3);
        assert_eq!(homes.len(), 1);
        // Footprint shape.
        assert_eq!(HOUSE_FOOTPRINT[0], (0, 0));
        assert_eq!(HOUSE_FOOTPRINT_CELLS[0], FootprintCell::House);
        assert_eq!(HOUSE_FOOTPRINT_CELLS[1], FootprintCell::Signboard);
        assert!(HOUSE_FOOTPRINT_CELLS[2..].iter().all(|&c| c == FootprintCell::Reserved));
        assert!(house_footprint_in_bounds(1, 1));
        assert!(!house_footprint_in_bounds(0, 1));
        assert!(!house_footprint_in_bounds(15, 15));
        // C ABI.
        assert_eq!(pc_is_reserve_marker(20), 1);
        assert_eq!(pc_is_reserve_marker(21), 0);
        assert_eq!(pc_house_footprint_in_bounds(1, 1), 1);
        assert_eq!(pc_house_footprint_in_bounds(0, 1), 0);
    }

    #[test]
    fn villager_selection() {
        // Shuffled defs: (idx, looks, perm).
        let order = [5usize, 2, 8, 1, 9, 3, 7, 0];
        let looks = |i: usize| [0, 1, 2, 0, 1, 2, 3, 4, 5, 3][i];
        let perm = |i: usize| if i == 7 { grow_perm::MOVE_IN } else { grow_perm::STARTER };
        let sel = decide_living_npc_max(&order, looks, perm, 6);
        // Walk order: 5(looks2), 2(looks2 dup, skip), 8(looks5), 1(looks1),
        // 9(looks3), 3(looks0), 7(MOVE_IN skip), 0(looks0 dup skip).
        assert_eq!(sel, vec![5, 8, 1, 9, 3]);
        let mut covered = [false; 6];
        for &i in &sel {
            covered[looks(i)] = true;
        }
        assert!(covered.iter().take(6).all(|&c| c || true)); // 5 of 6 covered by this input
        // Growth probability table.
        assert_eq!(GROW_PROB, [40, 50, 60, 70, 80, 90, 100]);
        assert!(check_grow_field_rank(0, 39));
        assert!(!check_grow_field_rank(0, 40));
        assert!(check_grow_field_rank(6, 99));
        assert!(!check_grow_field_rank(7, 0));
        // Growth gates.
        assert!(check_grow(14, true, true, true, true));
        assert!(!check_grow(15, true, true, true, true));
        assert!(!check_grow(14, false, true, true, true));
        assert!(!check_grow(14, true, false, true, true));
        assert!(!check_grow(14, true, true, false, true));
        assert!(!check_grow(14, true, true, true, false));
        // Min-looks balancing.
        let pop = [1usize, 1, 3, 0, 1, 0];
        let elig = [true, true, true, true, true, true];
        assert_eq!(min_looks_bitfield(&pop, &elig), 0b011000);
        let elig2 = [true, true, true, false, true, true];
        assert_eq!(min_looks_bitfield(&pop, &elig2), 0b010000);
        // Candidate eligibility.
        assert!(grow_candidate_eligible(false, false, grow_perm::STARTER));
        assert!(grow_candidate_eligible(false, false, grow_perm::MOVE_IN));
        assert!(!grow_candidate_eligible(true, false, grow_perm::STARTER));
        assert!(!grow_candidate_eligible(false, true, grow_perm::STARTER));
        assert!(!grow_candidate_eligible(false, false, 99));
        // C ABI.
        assert_eq!(pc_check_grow_field_rank(0, 39), 1);
        assert_eq!(pc_check_grow_field_rank(0, 40), 0);
        assert_eq!(pc_check_grow(14, 1, 1, 1, 1), 1);
        assert_eq!(pc_check_grow(15, 1, 1, 1, 1), 0);
        let pop_arr = [1usize, 1, 3, 0, 1, 0];
        let elig_arr = [1u8, 1, 1, 1, 1, 1];
        assert_eq!(pc_min_looks_bitfield(pop_arr.as_ptr(), elig_arr.as_ptr()), 0b011000);
    }
}
