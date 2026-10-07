//! Item-preference structures for the Rust rewrite.
//!
//! Source-verified (upstream `include/m_npc.h`, `src/game/m_npc.c`,
//! `src/game/m_quest.c`):
//!
//! The GameCube does NOT give normal villagers a static
//! favorite-item list. `Animal_c` carries identity, memories,
//! friendship, house info, clothing, mood, relations, and quest state —
//! but no `favorite_items[]` field. Instead, NPC-associated furniture
//! is *derived* from the villager's house:
//!
//! 1. `npc_def_list[]` gives defaults: cloth, umbrella,
//!    catchphrase string index (`mNpc_SetDefAnimalInfo`,
//!    `m_npc.c:2266`).
//! 2. `npc_house_list[npc_id & 0xFFF]` gives the house template: type,
//!    palette, wall_id, floor_id, main_layer_id, secondary_layer_id
//!    (`m_npc.c:2825`).
//! 3. `mNpc_DecideNpcFurniture` scans a 10x10 region of the main
//!    furniture layer, filters eligible furniture, counts, and picks
//!    `RANDOM(num)`.
//! 4. The pick is stored as `reward_furniture` and retrieved with
//!    `mNpc_GetNpcFurniture`.
//! 5. `mQst_GetGoods_common` (`m_quest.c:934`, with the source
//!    comment): `RANDOM(10)` gives a 1/10 chance to use the villager's
//!    house furniture for furniture "goods" instead of the general
//!    random-item generator.
//!
//! The Islander (GBA) system is much more explicit:
//!
//! * `Anm_bestFtr_c { u32 check; u16 have_bitfield; }` sits inside
//!   `memuni_u` (`m_npc.h:166`), the per-player memory union.
//! * `mNpc_Island_Ftr_c { u16 set_ftr_bitfield; trade_list[4];
//!   item_list[16]; }` (`m_npc.c:5204`); 16 furniture slots, 4 saved
//!   trade entries.
//! * `mNpc_SetIslandRoomFtr` ORs every player memory's
//!   `have_bitfield` into `set_ftr_bitfield`.
//! * `mNpc_GetIslandFtrIdx` normalizes variants with
//!   `aMR_CorrespondFurniture` / `aMR_GetFurnitureUnit`, so the same
//!   furniture type in different orientations maps to one slot.
//!
//! The apparent per-villager "favorite furniture" lists observed by
//! players are therefore best reconstructed as *emergent* from house
//! contents, not as a static preference table. Anything beyond this
//! (fixed 8-item lists, style/color favorites) is modern-game
//! terminology and is NOT confirmed for the GameCube.

use crate::inventory::{selectable_furniture, ExcludedFurniture};

/// Default presentation data for one NPC (`mNpc_Default_Data_c`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NpcDefData {
    pub cloth: u16,
    pub umbrella: u16,
    pub catchphrase_str_idx: u16,
}

/// House template for one NPC (`mNpc_NpcHouseData_c`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NpcHouseData {
    pub house_type: u8,
    pub palette: u8,
    pub wall_id: u16,
    pub floor_id: u16,
    pub main_layer_id: u32,
    pub secondary_layer_id: u32,
}

/// 10x10 furniture-layer region scanned by `mNpc_DecideNpcFurniture`.
pub const HOUSE_SCAN_W: usize = 10;
pub const HOUSE_SCAN_H: usize = 10;

/// Select the NPC-associated furniture: count eligible furniture in
/// the 10x10 region, then pick the one at `rng_index % eligible_count`
/// (`num = RANDOM(num)` in `mNpc_DecideNpcFurniture`).
///
/// `is_furniture` / `excluded` classify each grid cell; cells with
/// `EMPTY_NO` are skipped.
pub fn select_reward_furniture(
    items: &[[u16; HOUSE_SCAN_W]; HOUSE_SCAN_H],
    classify: &dyn Fn(u16) -> (bool, Option<ExcludedFurniture>),
    rng_index: u32,
) -> u16 {
    let mut eligible = [0u16; HOUSE_SCAN_W * HOUSE_SCAN_H];
    let mut n = 0usize;
    for row in items.iter() {
        for &it in row.iter() {
            if it == 0 {
                continue;
            }
            let (is_ftr, excl) = classify(it);
            if selectable_furniture(is_ftr, excl) {
                eligible[n] = it;
                n += 1;
            }
        }
    }
    if n == 0 {
        return 0;
    }
    eligible[(rng_index as usize) % n]
}

/// Where a furniture "good" comes from (`mQst_GetGoods_common`).
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GoodsSource {
    /// General random-item generator (`mSP_SelectRandomItem_New`).
    RandomItem = 0,
    /// The villager's house furniture (`mNpc_GetNpcFurniture`).
    VillagerHouse = 1,
}

/// The 1/10 branch: `generate_random_item = RANDOM(10)`; roll 0 picks
/// the villager's house furniture, any other roll uses the general
/// random-item generator. Applies to the FURNITURE goods category.
pub fn goods_source_for_furniture(roll_0_9: u32) -> GoodsSource {
    if roll_0_9 == 0 {
        GoodsSource::VillagerHouse
    } else {
        GoodsSource::RandomItem
    }
}

/// Islander "best furniture" progress (`Anm_bestFtr_c`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AnmBestFtr {
    pub check: u32,
    pub have_bitfield: u16,
}

/// Number of Islander furniture slots (`mNpc_ISLAND_FTR_NUM`).
pub const ISLAND_FTR_NUM: usize = 16;
/// Number of saved trade entries (`mNpc_ISLAND_FTR_SAVE_NUM`).
pub const ISLAND_FTR_SAVE_NUM: usize = 4;

/// Islander furniture/trade state (`mNpc_Island_Ftr_c`).
#[derive(Clone, Debug)]
pub struct IslandFtr {
    pub set_ftr_bitfield: u16,
    pub trade_list: [u16; ISLAND_FTR_SAVE_NUM],
    pub item_list: [u16; ISLAND_FTR_NUM],
}

impl IslandFtr {
    pub fn new() -> Self {
        Self {
            set_ftr_bitfield: 0,
            trade_list: [0; ISLAND_FTR_SAVE_NUM],
            item_list: [0; ISLAND_FTR_NUM],
        }
    }

    /// Merge one memory's `have_bitfield` into the combined set
    /// (`mNpc_SetIslandRoomFtr`: `set_ftr_bitfield |= ...`).
    pub fn merge_memory(&mut self, have_bitfield: u16) {
        self.set_ftr_bitfield |= have_bitfield;
    }

    /// Whether furniture slot `i` has been obtained.
    pub fn slot_obtained(&self, i: usize) -> bool {
        i < ISLAND_FTR_NUM && (self.set_ftr_bitfield & (1 << i)) != 0
    }

    /// Find which conceptual slot `item` belongs to, using the
    /// caller's variant normalizer (models
    /// `aMR_CorrespondFurniture` / `aMR_GetFurnitureUnit`): two items
    /// mapping to the same unit share a slot.
    pub fn find_slot(
        &self,
        item: u16,
        normalize: &dyn Fn(u16) -> u32,
    ) -> Option<usize> {
        let unit = normalize(item);
        (0..ISLAND_FTR_NUM)
            .find(|&i| self.item_list[i] != 0 && normalize(self.item_list[i]) == unit)
    }
}

/// C ABI: 1 when the `RANDOM(10)` roll selects the villager's house
/// furniture for furniture goods, 0 for the general generator.
#[no_mangle]
pub extern "C" fn pc_npc_house_goods(roll_0_9: u32) -> u8 {
    (goods_source_for_furniture(roll_0_9) == GoodsSource::VillagerHouse) as u8
}

/// C ABI: count of eligible furniture in a 10x10 house grid.
/// `flags[i]` packs: bit 0 = is furniture, bits 1..3 = excluded kind
/// (7 = none). Matches `mNpc_DecideNpcFurniture`'s count pass.
#[no_mangle]
pub extern "C" fn pc_eligible_furniture_count(flags: *const u8) -> u32 {
    if flags.is_null() {
        return 0;
    }
    let f = unsafe { core::slice::from_raw_parts(flags, HOUSE_SCAN_W * HOUSE_SCAN_H) };
    f.iter().filter(|&&b| (b & 1) != 0 && ((b >> 1) & 7) == 7).count() as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn classify(item: u16) -> (bool, Option<ExcludedFurniture>) {
        match item {
            1 => (true, None),
            2 => (true, None),
            3 => (true, Some(ExcludedFurniture::Fish)),
            _ => (false, None),
        }
    }

    #[test]
    fn reward_selection_filters_and_picks() {
        let mut items = [[0u16; HOUSE_SCAN_W]; HOUSE_SCAN_H];
        items[0][0] = 1;
        items[0][1] = 3; // excluded (fish)
        items[1][0] = 2;
        // 2 eligible; rng picks index 1 -> item 2.
        assert_eq!(select_reward_furniture(&items, &classify, 1), 2);
        assert_eq!(select_reward_furniture(&items, &classify, 3), 2);
        assert_eq!(select_reward_furniture(&items, &classify, 0), 1);
        let empty = [[0u16; HOUSE_SCAN_W]; HOUSE_SCAN_H];
        assert_eq!(select_reward_furniture(&empty, &classify, 0), 0);
    }

    #[test]
    fn house_goods_roll() {
        assert_eq!(goods_source_for_furniture(0), GoodsSource::VillagerHouse);
        for r in 1..10u32 {
            assert_eq!(goods_source_for_furniture(r), GoodsSource::RandomItem);
        }
    }

    #[test]
    fn island_bitfield_merge() {
        let mut f = IslandFtr::new();
        f.merge_memory(0b0101);
        f.merge_memory(0b1010);
        assert_eq!(f.set_ftr_bitfield, 0b1111);
        assert!(f.slot_obtained(0));
        assert!(f.slot_obtained(3));
        assert!(!f.slot_obtained(4));
        assert!(!f.slot_obtained(16));
    }

    #[test]
    fn island_slot_normalization() {
        let mut f = IslandFtr::new();
        f.item_list[5] = 100;
        // Normalizer maps variants to one unit.
        let norm = |it: u16| (it / 10) as u32;
        assert_eq!(f.find_slot(109, &norm), Some(5));
        assert_eq!(f.find_slot(200, &norm), None);
    }
}
