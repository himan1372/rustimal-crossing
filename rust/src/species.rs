//! Seasonal species tables for fish and insects.
//!
//! Ports the retail spawn-data layer from `src/actor/ac_set_ovl_gyoei.c`
//! (fish) and `src/actor/ac_set_ovl_insect.c` (insects), GAFE01_00 Rev. 0.
//!
//! Architecture (retail):
//! - Fish: 24 half-month terms x 4 daily time periods, separate
//!   river/ocean/pond tables plus tournament and island tables.
//! - Insects: 12 monthly terms x 6 daily time periods, town tables plus a
//!   separate 6-term island table.
//! - Tables are ordered entry lists (species, spawn area, weight), NOT
//!   species-keyed maps: seasonal transitions concatenate the current and
//!   next term's tables, so a species may appear twice.
//! - Selection uses the unusual retail algorithms in `ecology.rs`
//!   (`fish_select`, `insect_select`) with field-rank rates; do NOT use a
//!   normalized `WeightedIndex`.
//!
//! The verbatim table data lives in the generated `fish_tables` and
//! `insect_tables` modules. This module holds the enums, term/time lookup,
//! transition blending, and the dynamic runtime insertions (Coelacanth,
//! Whale, Ant/Cockroach, Spirit).

use crate::ecology;

// ---- Fish ----

/// Fish types in exact `aGYO_TYPE_*` source order (`include/ac_gyoei.h`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum FishType {
    CrucianCarp = 0,
    BrookTrout = 1,
    Carp = 2,
    Koi = 3,
    Catfish = 4,
    SmallBass = 5,
    Bass = 6,
    LargeBass = 7,
    Bluegill = 8,
    GiantCatfish = 9,
    GiantSnakehead = 10,
    BarbelSteed = 11,
    Dace = 12,
    PaleChub = 13,
    Bitterling = 14,
    Loach = 15,
    PondSmelt = 16,
    Sweetfish = 17,
    CherrySalmon = 18,
    LargeChar = 19,
    RainbowTrout = 20,
    Stringfish = 21,
    Salmon = 22,
    Goldfish = 23,
    Piranha = 24,
    Arowana = 25,
    Eel = 26,
    FreshwaterGoby = 27,
    Angelfish = 28,
    Guppy = 29,
    PopeyedGoldfish = 30,
    Coelacanth = 31,
    Crawfish = 32,
    Frog = 33,
    Killifish = 34,
    Jellyfish = 35,
    SeaBass = 36,
    RedSnapper = 37,
    BarredKnifejaw = 38,
    Arapaima = 39,
    Whale = 40,
    EmptyCan = 41,
    Boot = 42,
    OldTire = 43,
    Salmon2 = 44,
}

impl FishType {
    pub fn from_u8(v: u8) -> Option<FishType> {
        if v <= 44 {
            // SAFETY: repr(u8) enum with contiguous discriminants 0..=44.
            Some(unsafe { core::mem::transmute::<u8, FishType>(v) })
        } else {
            None
        }
    }

    /// Trash fish: Empty Can ..= Old Tire (`aGYO_IS_FISH_TRASH`).
    pub fn is_trash(self) -> bool {
        matches!(self, FishType::EmptyCan | FishType::Boot | FishType::OldTire)
    }
}

/// Fish spawn areas in `aSOG_SPAWN_AREA_*` order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum FishArea {
    Pool = 0,
    Waterfall = 1,
    /// River mouth: in GAFE01_00 (USA Rev. 0) the acre must be RIVER;
    /// the Australian build (`VERSION >= VER_GAFU01_00`) additionally
    /// requires MARINE (`ac_set_ovl_gyoei.c:1311-1320`).
    RiverMouth = 2,
    Offing = 3,
    Sea = 4,
    River = 5,
    Pond = 6,
}

/// One static fish spawn-table entry: species, desired area, relative weight
/// (source `u8`; weights are relative, not percentages).
#[derive(Clone, Copy, Debug)]
pub struct FishSpawnEntry {
    pub fish: FishType,
    pub area: FishArea,
    pub weight: u8,
}

/// Fish environments: river / ocean / pond.
pub mod fish_env {
    pub const RIVER: u8 = 0;
    pub const SEA: u8 = 1;
    pub const POND: u8 = 2;
}

/// Pond tables exist only Apr-Aug (both halves) and Sep 1-15;
/// the second half of September and all other months are NULL.
pub fn pond_table_exists(month: u8, day: u8) -> bool {
    (4..=8).contains(&month) || (month == 9 && day <= 15)
}

/// Dynamic Coelacanth insertion (`aSOG_add_kaseki_range_data`):
/// rain + ocean + NOT 09:00-15:59 injects Coelacanth (SEA, weight 2.0)
/// into the *current* term's table only (not the next term during a
/// transition). Still applies on the island during rain (the island skips
/// seasonal transition blending but not this injection).
pub fn coelacanth_active(hour: u8, raining: bool) -> bool {
    raining && !(9..16).contains(&hour)
}

/// Offing: copy the ocean table, multiply all weights by 10, add Whale = 1
/// (this artificially suppresses the whale). The whale is added for the
/// current term only, not the next term during a transition
/// (`aSOG_gyoei_make_offing_range_data`).
pub fn offing_entry(ocean: &[FishSpawnEntry]) -> Vec<(FishType, FishArea, f32)> {
    let mut out: Vec<(FishType, FishArea, f32)> = ocean
        .iter()
        .map(|e| (e.fish, e.area, e.weight as f32 * 10.0))
        .collect();
    out.push((FishType::Whale, FishArea::Offing, 1.0));
    out
}

/// Blend current and next term tables for a seasonal transition:
/// ordered concatenation of current x rate + next x (1-rate).
/// Species may appear twice; do NOT merge by species.
pub fn blend_fish_tables(
    current: &[FishSpawnEntry],
    next: &[FishSpawnEntry],
    rate: f32,
) -> Vec<(FishType, FishArea, f32)> {
    let mut out = Vec::with_capacity(current.len() + next.len());
    out.extend(current.iter().map(|e| (e.fish, e.area, e.weight as f32 * rate)));
    out.extend(
        next.iter()
            .map(|e| (e.fish, e.area, e.weight as f32 * (1.0 - rate))),
    );
    out
}

// ---- Insects ----

/// Insect types in exact `aINS_INSECT_TYPE_*` source order
/// (`include/ac_insect_h.h`); 40 normal + SPIRIT + NONE.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum InsectType {
    CommonButterfly = 0,
    YellowButterfly = 1,
    TigerButterfly = 2,
    PurpleButterfly = 3,
    RobustCicada = 4,
    WalkerCicada = 5,
    EveningCicada = 6,
    BrownCicada = 7,
    Bee = 8,
    CommonDragonfly = 9,
    RedDragonfly = 10,
    DarnerDragonfly = 11,
    BandedDragonfly = 12,
    LongLocust = 13,
    MigratoryLocust = 14,
    Cricket = 15,
    Grasshopper = 16,
    BellCricket = 17,
    PineCricket = 18,
    DroneBeetle = 19,
    DynastidBeetle = 20,
    FlatStagBeetle = 21,
    JewelBeetle = 22,
    LonghornBeetle = 23,
    Ladybug = 24,
    SpottedLadybug = 25,
    Mantis = 26,
    Firefly = 27,
    Cockroach = 28,
    SawStagBeetle = 29,
    MountainBeetle = 30,
    GiantBeetle = 31,
    Snail = 32,
    MoleCricket = 33,
    PondSkater = 34,
    Bagworm = 35,
    PillBug = 36,
    Spider = 37,
    Ant = 38,
    Mosquito = 39,
    Spirit = 40,
    None = 41,
}

impl InsectType {
    pub fn from_u8(v: u8) -> Option<InsectType> {
        if v <= 41 {
            // SAFETY: repr(u8) enum with contiguous discriminants 0..=41.
            Some(unsafe { core::mem::transmute::<u8, InsectType>(v) })
        } else {
            None
        }
    }
}

/// Insect spawn areas in `aSOI_SPAWN_AREA_*` order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum InsectArea {
    OnTree = 0,
    OnFlower = 1,
    RainingOnFlower = 2,
    Flying = 3,
    OnGround = 4,
    InBush = 5,
    FlyingNearWater = 6,
    OnWater = 7,
    OnCandy = 8,
    OnTrash = 9,
    UnderRock = 10,
    Underground = 11,
    FlyingNearFlowersOrAround = 12,
    Nothing = 13,
}

/// One static insect spawn-table entry: species, desired area, weight.
/// `None`/`Nothing` is a real entry: an explicit no-insect outcome.
#[derive(Clone, Copy, Debug)]
pub struct InsectSpawnEntry {
    pub insect: InsectType,
    pub area: InsectArea,
    pub weight: u8,
}

/// Six insect daily time terms (`ac_set_ovl_insect.h`):
/// 0 = 23:00-03:59, 1 = 04:00-07:59, 2 = 08:00-15:59,
/// 3 = 16:00-16:59, 4 = 17:00-18:59, 5 = 19:00-22:59.
pub fn insect_time_no(hour: u8) -> u8 {
    match hour {
        23 | 0..=3 => 0,
        4..=7 => 1,
        8..=15 => 2,
        16 => 3,
        17..=18 => 4,
        _ => 5,
    }
}

/// Insect seasonal term is the zero-based month index.
pub fn insect_term(month: u8) -> u8 {
    month.saturating_sub(1) % 12
}

/// Blend current and next month insect tables for a seasonal transition
/// (ordered concatenation; species may appear twice).
pub fn blend_insect_tables(
    current: &[InsectSpawnEntry],
    next: &[InsectSpawnEntry],
    rate: f32,
) -> Vec<(InsectType, InsectArea, f32)> {
    let mut out = Vec::with_capacity(current.len() + next.len());
    out.extend(
        current
            .iter()
            .map(|e| (e.insect, e.area, e.weight as f32 * rate)),
    );
    out.extend(
        next.iter()
            .map(|e| (e.insect, e.area, e.weight as f32 * (1.0 - rate))),
    );
    out
}

/// Dynamic town-insect entries appended at runtime: Ant on candy, Ant on
/// trash, Cockroach on trash (each weight 1).
pub fn candy_trash_entries() -> [(InsectType, InsectArea, f32); 3] {
    [
        (InsectType::Ant, InsectArea::OnCandy, 1.0),
        (InsectType::Ant, InsectArea::OnTrash, 1.0),
        (InsectType::Cockroach, InsectArea::OnTrash, 1.0),
    ]
}

/// When candy or spoiled turnips are present, all ordinary insect weights
/// are removed and selection is limited to the candy/trash entries.
pub fn candy_trash_override_active(candy_out: bool, spoiled_turnips: bool) -> bool {
    candy_out || spoiled_turnips
}

/// Spirit (Wisp) spawn: the normal seasonal table is discarded and replaced
/// with the single entry SPIRIT / FLYING / 100.
pub fn spirit_table() -> [(InsectType, InsectArea, f32); 1] {
    [(InsectType::Spirit, InsectArea::Flying, 100.0)]
}

/// Multi-birth lookup: `l_insect_birth_sum` (min, additional_range) by insect.
/// Only red dragonfly and firefly have (6, 3); NONE is implicitly (0, 0).
pub fn insect_birth_sum(insect: InsectType) -> (u8, u8) {
    match insect {
        InsectType::RedDragonfly | InsectType::Firefly => (6, 3),
        InsectType::None => (0, 0),
        _ => (1, 0),
    }
}

/// Convenience: full fish table lookup for (month, day, hour, env).
/// Returns None for pond months without tables.
pub fn fish_seasonal_table(
    month: u8,
    day: u8,
    hour: u8,
    env: u8,
) -> Option<&'static [FishSpawnEntry]> {
    let term = ecology::fish_term(month, day);
    let time = ecology::fish_time_no(hour);
    crate::fish_tables::fish_table(env, term, time)
}

/// Convenience: full insect table lookup for (month, hour).
pub fn insect_seasonal_table(month: u8, hour: u8) -> &'static [InsectSpawnEntry] {
    let m = insect_term(month) as usize;
    let t = insect_time_no(hour) as usize;
    crate::insect_tables::INSECT_TOWN[m][t]
}

// ---- C ABI ----

#[no_mangle]
pub extern "C" fn pc_fish_seasonal_count(month: u8, day: u8, hour: u8, env: u8) -> i32 {
    match fish_seasonal_table(month, day, hour, env) {
        Some(t) => t.len() as i32,
        None => -1,
    }
}

#[no_mangle]
pub extern "C" fn pc_fish_table_entry(
    month: u8,
    day: u8,
    hour: u8,
    env: u8,
    idx: u32,
    out_fish: *mut u8,
    out_area: *mut u8,
    out_weight: *mut u8,
) -> u8 {
    let Some(t) = fish_seasonal_table(month, day, hour, env) else {
        return 0;
    };
    let Some(e) = t.get(idx as usize) else {
        return 0;
    };
    if !out_fish.is_null() {
        unsafe { *out_fish = e.fish as u8 };
    }
    if !out_area.is_null() {
        unsafe { *out_area = e.area as u8 };
    }
    if !out_weight.is_null() {
        unsafe { *out_weight = e.weight };
    }
    1
}

#[no_mangle]
pub extern "C" fn pc_insect_seasonal_count(month: u8, hour: u8, island: u8) -> i32 {
    if island != 0 {
        crate::insect_tables::INSECT_ISLAND[insect_time_no(hour) as usize].len() as i32
    } else {
        insect_seasonal_table(month, hour).len() as i32
    }
}

#[no_mangle]
pub extern "C" fn pc_insect_table_entry(
    month: u8,
    hour: u8,
    island: u8,
    idx: u32,
    out_insect: *mut u8,
    out_area: *mut u8,
    out_weight: *mut u8,
) -> u8 {
    let t: &[InsectSpawnEntry] = if island != 0 {
        crate::insect_tables::INSECT_ISLAND[insect_time_no(hour) as usize]
    } else {
        insect_seasonal_table(month, hour)
    };
    let Some(e) = t.get(idx as usize) else {
        return 0;
    };
    if !out_insect.is_null() {
        unsafe { *out_insect = e.insect as u8 };
    }
    if !out_area.is_null() {
        unsafe { *out_area = e.area as u8 };
    }
    if !out_weight.is_null() {
        unsafe { *out_weight = e.weight };
    }
    1
}

#[no_mangle]
pub extern "C" fn pc_insect_time_no(hour: u8) -> u8 {
    insect_time_no(hour)
}

#[no_mangle]
pub extern "C" fn pc_coelacanth_active(hour: u8, raining: u8) -> u8 {
    coelacanth_active(hour, raining != 0) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fish_term_and_time() {
        // ecology.rs already covers these; spot-check the glue.
        assert_eq!(ecology::fish_term(1, 1), 0);
        assert_eq!(ecology::fish_term(1, 16), 1);
        assert_eq!(ecology::fish_term(9, 15), 16);
        assert_eq!(ecology::fish_term(9, 16), 17);
        assert_eq!(ecology::fish_time_no(22), 0);
        assert_eq!(ecology::fish_time_no(10), 2);
    }

    #[test]
    fn insect_time_terms() {
        assert_eq!(insect_time_no(23), 0);
        assert_eq!(insect_time_no(3), 0);
        assert_eq!(insect_time_no(4), 1);
        assert_eq!(insect_time_no(7), 1);
        assert_eq!(insect_time_no(8), 2);
        assert_eq!(insect_time_no(15), 2);
        assert_eq!(insect_time_no(16), 3);
        assert_eq!(insect_time_no(17), 4);
        assert_eq!(insect_time_no(18), 4);
        assert_eq!(insect_time_no(19), 5);
        assert_eq!(insect_time_no(22), 5);
    }

    #[test]
    fn fish_table_lookup() {
        // Jan 1, 10:00, river -> term 0, time 2.
        let t = fish_seasonal_table(1, 1, 10, fish_env::RIVER).unwrap();
        assert_eq!(t.len(), 14);
        assert_eq!(t[0].fish, FishType::CrucianCarp);
        assert_eq!(t[0].area, FishArea::River);
        // time period 2 maps to r_m1_t1 (weight 7), not r_m1_t2.
        assert_eq!(t[0].weight, 7);
        // Pond in January: no table.
        assert!(fish_seasonal_table(1, 1, 10, fish_env::POND).is_none());
        // Pond in May: table exists.
        assert!(fish_seasonal_table(5, 1, 10, fish_env::POND).is_some());
        // September uses distinct latter-half river tables (term 17).
        let sep1 = fish_seasonal_table(9, 15, 10, fish_env::RIVER).unwrap();
        let sep2 = fish_seasonal_table(9, 16, 10, fish_env::RIVER).unwrap();
        assert_eq!(sep1.len(), 15);
        assert_eq!(sep2.len(), 14);
    }

    #[test]
    fn insect_table_lookup() {
        // January: the sparse fallback table for every term.
        for h in [0, 6, 12, 16, 18, 21] {
            let t = insect_seasonal_table(1, h);
            assert_eq!(t.len(), 3);
            assert_eq!(t[0].insect, InsectType::PillBug);
            assert_eq!(t[0].area, InsectArea::UnderRock);
            assert_eq!(t[0].weight, 5);
            assert_eq!(t[1].insect, InsectType::MoleCricket);
            assert_eq!(t[2].insect, InsectType::Bagworm);
        }
        // July daytime (term 2) is the richest table: 21 entries.
        let jul = insect_seasonal_table(7, 12);
        assert_eq!(jul.len(), 21);
        // Island term tables exist for all 6 terms.
        for h in [0, 6, 12, 16, 18, 21] {
            let t = crate::insect_tables::INSECT_ISLAND[insect_time_no(h) as usize];
            assert!(!t.is_empty());
        }
        // Island term 1 has the explicit NONE entry.
        let isl1 = crate::insect_tables::INSECT_ISLAND[1];
        assert!(isl1.iter().any(|e| e.insect == InsectType::None && e.weight == 78));
    }

    #[test]
    fn transition_blend_keeps_duplicates() {
        let a = fish_seasonal_table(1, 1, 10, fish_env::RIVER).unwrap();
        let b = fish_seasonal_table(1, 16, 10, fish_env::RIVER).unwrap();
        let blended = blend_fish_tables(a, b, 5.0 / 6.0);
        assert_eq!(blended.len(), a.len() + b.len());
        // Same species can appear twice (ordered concatenation, no merge).
        let crucian = blended.iter().filter(|(f, _, _)| *f == FishType::CrucianCarp).count();
        assert_eq!(crucian, 2);
    }

    #[test]
    fn dynamic_insertions() {
        assert!(coelacanth_active(22, true));
        assert!(!coelacanth_active(12, true)); // 09:00-15:59 excluded
        assert!(!coelacanth_active(22, false));
        let ocean = fish_seasonal_table(1, 1, 10, fish_env::SEA).unwrap();
        let off = offing_entry(ocean);
        assert_eq!(off.len(), ocean.len() + 1);
        assert_eq!(off[off.len() - 1].0, FishType::Whale);
        assert_eq!(off[0].2, ocean[0].weight as f32 * 10.0);
        assert!(candy_trash_override_active(true, false));
        assert!(!candy_trash_override_active(false, false));
        let spirit = spirit_table();
        assert_eq!(spirit[0], (InsectType::Spirit, InsectArea::Flying, 100.0));
    }

    #[test]
    fn birth_table() {
        assert_eq!(insect_birth_sum(InsectType::RedDragonfly), (6, 3));
        assert_eq!(insect_birth_sum(InsectType::Firefly), (6, 3));
        assert_eq!(insect_birth_sum(InsectType::CommonButterfly), (1, 0));
        assert_eq!(insect_birth_sum(InsectType::None), (0, 0));
        assert_eq!(crate::insect_tables::INSECT_BIRTH_SUM.len(), 42);
    }

    #[test]
    fn c_abi_tables() {
        assert_eq!(pc_fish_seasonal_count(1, 1, 10, fish_env::RIVER), 14);
        assert_eq!(pc_fish_seasonal_count(1, 1, 10, fish_env::POND), -1);
        assert_eq!(pc_insect_seasonal_count(1, 12, 0), 3);
        assert_eq!(pc_insect_time_no(16), 3);
        assert_eq!(pc_coelacanth_active(22, 1), 1);
        assert_eq!(pc_coelacanth_active(12, 1), 0);
    }
}
