//! Inventory scan system for the Rust rewrite.
//!
//! Source-verified (upstream `src/game/m_private.c`,
//! `include/m_private.h`, `src/game/m_npc.c`):
//!
//! The GameCube inventory is a tiny flat array: 15 pocket slots
//! (`mPr_POCKETS_SLOT_COUNT`) of item IDs, plus a packed bitfield of
//! 2-bit per-slot item conditions (`mPr_GET_ITEM_COND` shifts by
//! `slot_no << 1`). Possession queries are linear scans:
//!
//! * `mPr_GetPossessionItemIdx`: first slot whose ID equals the item,
//!   or -1. Deterministic, no RNG, stops at the first match.
//! * `mPr_GetPossessionItemIdxWithCond`: same, but the 2-bit condition
//!   must also match.
//! * `mPr_GetPossessionItemSum` / `...WithCond`: count matches instead
//!   of finding the first.
//! * The same primitive doubles as free-slot search:
//!   `mPr_GetPossessionItemIdx(priv, EMPTY_NO)`.
//! * A family of category/type/range variants exists
//!   (`...FGTypeWithCond_cancel`, `...Item1Category...`,
//!   `...KindWithCond`), forming a small inventory-query API used by
//!   quests and NPC systems alike.
//!
//! "Impulse buying" is community terminology, not a decomp function
//! name. The defensible model is two-stage: NPC logic picks a candidate
//! item (mechanism untraced — it may be a favorite item or a random
//! carried item), then the possession primitive checks the pockets.
//! What IS confirmed: villagers can request items the player carries,
//! and the inventory API above is the mechanism available for it.
//!
//! NPC furniture rewards (`mNpc_DecideNpcFurniture`) are a separate
//! subsystem: it scans the NPC's house foreground, filters OUT clothing,
//! umbrellas, insects, fish, gyroids, identified fossils, and NES games
//! (`mNpc_CheckSelectFurniture`), counts the rest, and picks randomly.

/// Pocket slot count (`mPr_POCKETS_SLOT_COUNT`).
pub const POCKETS_SLOT_COUNT: usize = 15;

/// Empty-slot sentinel.
pub const EMPTY_NO: u16 = 0x0000;

/// Bits per item-condition field.
pub const ITEM_COND_BITS: u32 = 2;

/// Player inventory: pocket item IDs + packed 2-bit conditions.
#[derive(Clone, Debug)]
pub struct Inventory {
    pub pockets: [u16; POCKETS_SLOT_COUNT],
    pub item_conditions: u32,
}

impl Default for Inventory {
    fn default() -> Self {
        Self { pockets: [EMPTY_NO; POCKETS_SLOT_COUNT], item_conditions: 0 }
    }
}

impl Inventory {
    /// Condition for a slot (`mPr_GET_ITEM_COND`).
    pub fn item_cond(&self, slot: usize) -> u32 {
        (self.item_conditions >> ((slot as u32) << 1)) & 0x3
    }

    /// Set a slot's condition (`mPr_SET_ITEM_COND`).
    pub fn set_item_cond(&mut self, slot: usize, cond: u32) {
        let shift = (slot as u32) << 1;
        self.item_conditions =
            (self.item_conditions & !(0x3 << shift)) | ((cond & 0x3) << shift);
    }

    /// First pocket containing `item`, or `None`
    /// (`mPr_GetPossessionItemIdx`; -1 becomes `None`).
    pub fn find_item(&self, item: u16) -> Option<usize> {
        self.pockets.iter().position(|&it| it == item)
    }

    /// First pocket containing `item` with condition `cond`
    /// (`mPr_GetPossessionItemIdxWithCond`).
    pub fn find_item_with_cond(&self, item: u16, cond: u32) -> Option<usize> {
        self.pockets
            .iter()
            .enumerate()
            .find(|(i, &it)| it == item && self.item_cond(*i) == cond)
            .map(|(i, _)| i)
    }

    /// Count of pockets containing `item`
    /// (`mPr_GetPossessionItemSum`).
    pub fn count_item(&self, item: u16) -> u32 {
        self.pockets.iter().filter(|&&it| it == item).count() as u32
    }

    /// Count of pockets containing `item` with condition `cond`.
    pub fn count_item_with_cond(&self, item: u16, cond: u32) -> u32 {
        self.pockets
            .iter()
            .enumerate()
            .filter(|(i, &it)| it == item && self.item_cond(*i) == cond)
            .count() as u32
    }

    /// First free pocket (`mPr_GetPossessionItemIdx(priv, EMPTY_NO)`).
    pub fn find_free_slot(&self) -> Option<usize> {
        self.find_item(EMPTY_NO)
    }

    /// Place an item with a condition, returning the slot.
    pub fn put(&mut self, item: u16, cond: u32) -> Option<usize> {
        let slot = self.find_free_slot()?;
        self.pockets[slot] = item;
        self.set_item_cond(slot, cond);
        Some(slot)
    }
}

/// Furniture categories excluded from NPC reward selection
/// (`mNpc_CheckSelectFurniture`).
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExcludedFurniture {
    Clothing = 0,
    Umbrella = 1,
    Insect = 2,
    Fish = 3,
    Gyroid = 4,
    IdentifiedFossil = 5,
    NesGame = 6,
}

/// Whether a furniture item is eligible for NPC reward selection: it
/// must be furniture and none of the excluded categories.
pub fn selectable_furniture(is_furniture: bool, excluded: Option<ExcludedFurniture>) -> bool {
    is_furniture && excluded.is_none()
}

/// Candidate-selection strategies for the impulse-buying interaction.
/// The possession check itself is source-verified; which strategy the
/// NPC uses to pick the candidate is NOT yet traced and is modeled as
/// rewrite-owned options.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CandidateStrategy {
    /// Check a favorite/preferred item first (observed behavior).
    FavoriteFirst = 0,
    /// Pick a random carried item (observed "impulse" behavior).
    RandomCarried = 1,
}

/// Resolve an impulse-buy candidate: given a candidate item ID, find
/// the pocket holding it. Returns the pocket index, or `None` when the
/// player does not carry it.
pub fn resolve_candidate(inv: &Inventory, candidate: u16) -> Option<usize> {
    if candidate == EMPTY_NO {
        return None;
    }
    inv.find_item(candidate)
}

/// C ABI: first pocket containing `item`, or -1.
/// Retail boundary: `mPr_GetPossessionItemIdx` (m_private.c); first match
/// wins, slots scanned 0..15.
#[no_mangle]
pub extern "C" fn pc_inventory_find(pockets: *const u16, item: u16) -> i32 {
    if pockets.is_null() {
        return -1;
    }
    let slots = unsafe { core::slice::from_raw_parts(pockets, POCKETS_SLOT_COUNT) };
    slots.iter().position(|&it| it == item).map(|i| i as i32).unwrap_or(-1)
}

/// C ABI: count of pockets containing `item`.
/// Retail boundary: `mPr_GetPossessionItemSum` (m_private.c), the 15-slot
/// count used by shop/quest code.
#[no_mangle]
pub extern "C" fn pc_inventory_count(pockets: *const u16, item: u16) -> u32 {
    if pockets.is_null() {
        return 0;
    }
    let slots = unsafe { core::slice::from_raw_parts(pockets, POCKETS_SLOT_COUNT) };
    slots.iter().filter(|&&it| it == item).count() as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_returns_first_match() {
        let mut inv = Inventory::default();
        inv.pockets[0] = 100;
        inv.pockets[2] = 100;
        assert_eq!(inv.find_item(100), Some(0));
        assert_eq!(inv.find_item(999), None);
    }

    #[test]
    fn cond_packing_matches_source() {
        let mut inv = Inventory::default();
        inv.set_item_cond(0, 2);
        inv.set_item_cond(7, 3);
        inv.set_item_cond(14, 1);
        assert_eq!(inv.item_cond(0), 2);
        assert_eq!(inv.item_cond(7), 3);
        assert_eq!(inv.item_cond(14), 1);
        assert_eq!(inv.item_cond(1), 0);
        // Setting one slot does not disturb neighbors.
        inv.set_item_cond(7, 0);
        assert_eq!(inv.item_cond(6), 0);
        assert_eq!(inv.item_cond(8), 0);
    }

    #[test]
    fn find_with_cond() {
        let mut inv = Inventory::default();
        inv.put(50, 1);
        inv.put(50, 2);
        assert_eq!(inv.find_item_with_cond(50, 2), Some(1));
        assert_eq!(inv.find_item_with_cond(50, 3), None);
        assert_eq!(inv.count_item(50), 2);
        assert_eq!(inv.count_item_with_cond(50, 1), 1);
    }

    #[test]
    fn free_slot_search() {
        let inv = Inventory::default();
        assert_eq!(inv.find_free_slot(), Some(0));
        let mut full = Inventory::default();
        for i in 0..POCKETS_SLOT_COUNT {
            full.pockets[i] = 7;
        }
        assert_eq!(full.find_free_slot(), None);
        assert_eq!(full.put(9, 0), None);
    }

    #[test]
    fn furniture_filter() {
        assert!(selectable_furniture(true, None));
        assert!(!selectable_furniture(true, Some(ExcludedFurniture::Fish)));
        assert!(!selectable_furniture(true, Some(ExcludedFurniture::NesGame)));
        assert!(!selectable_furniture(false, None));
    }

    #[test]
    fn candidate_resolution() {
        let mut inv = Inventory::default();
        inv.put(1234, 0);
        assert_eq!(resolve_candidate(&inv, 1234), Some(0));
        assert_eq!(resolve_candidate(&inv, 9999), None);
        assert_eq!(resolve_candidate(&inv, EMPTY_NO), None);
    }
}
