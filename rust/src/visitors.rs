//! Special-visitor NPC classification kernels.
//!
//! Verified against `include/m_npc.h`, `include/m_name_table.h`,
//! `src/actor/npc/ac_npc_init.c_inc`, and
//! `src/actor/npc/ac_npc_curator_move.c_inc` (GAFE01_00 Rev 0 USA decomp).
//!
//! Architecture: NPC identity tables, the set-NPC manager
//! (`ac_set_npc_manager.c`), actor init branches, event scheduling, RNG,
//! and all saved records stay in C. There is NO single retail predicate
//! deciding "visitor X appears today" — appearance emerges from the
//! manager's regular/guest procedures plus event state — so no such
//! kernel is provided. This module holds only the source-proven pure
//! classifications with primitive arguments.
//!
//! NPC ID encoding (`m_npc.h`): the upper nibble is the type.
//! `0xD000 | idx` = special NPC, `0xE000 | idx` = villager.
//! Do not confuse with `m_npc.h`'s separate `mNpc_NAME_TYPE_*` enum.

/// NPC ID encoding (`include/m_npc.h`, verbatim).
pub mod npc_id {
    /// Index bits of an NPC id.
    pub const IDX_MASK: u16 = 0x0FFF;
    /// Type bits of an NPC id.
    pub const TYPE_MASK: u16 = 0xF000;
    /// Type value marking a special NPC.
    pub const SPECIAL_TYPE: u16 = 0xD000;
}

/// Name types (`include/m_name_table.h`).
pub mod name_type {
    /// Special NPCs (`ITEM_NAME_GET_TYPE(npc_id)`).
    pub const SPNPC: u8 = 13;
    /// Villager NPCs.
    pub const NPC: u8 = 14;
}

/// NPC capacity constants (`include/m_npc.h`).
pub mod capacity {
    /// Event-NPC records.
    pub const EVENT_NPC_NUM: u8 = 5;
    /// Mask-NPC records.
    pub const MASK_NPC_NUM: u8 = 3;
}

/// Curator item ranges (`ac_npc_curator_move.c_inc:612-618`, verbatim).
/// Note the asymmetry: insect end is EXCLUSIVE, fish end is INCLUSIVE.
pub mod curator_range {
    /// `FTR_START(FTR_SUM_ART01)..=FTR_END(FTR_SUM_ART15)`.
    pub const ART_LO: u16 = 0x12AC;
    pub const ART_HI: u16 = 0x12E7;
    /// `ITM_INSECT_START..ITM_INSECT_END` (end exclusive).
    pub const INSECT_LO: u16 = 0x2D00;
    pub const INSECT_HI_EXCL: u16 = 0x2D28;
    /// `ITM_FISH_START..=ITM_FISH_END` (end inclusive).
    pub const FISH_LO: u16 = 0x2300;
    pub const FISH_HI: u16 = 0x2340;
    /// Generic unidentified fossil.
    pub const ITM_FOSSIL: u16 = 0x2511;
    /// Empty item slot.
    pub const EMPTY_NO: u16 = 0x0000;
}

/// Curator offer classification, matching the priority order in
/// `aCR_msg_win_open_wait`: empty, fossil, art, insect, fish,
/// generic fossil, other.
pub mod offer_class {
    pub const EMPTY: u8 = 0;
    pub const FOSSIL: u8 = 1;
    pub const ART: u8 = 2;
    pub const INSECT: u8 = 3;
    pub const FISH: u8 = 4;
    pub const GENERIC_FOSSIL: u8 = 5;
    pub const OTHER: u8 = 6;
}

/// `mNpc_GET_IDX`: index bits of an NPC id.
pub fn npc_get_idx(npc_id: u16) -> u16 {
    npc_id & npc_id::IDX_MASK
}

/// `mNpc_GET_TYPE`: type bits of an NPC id.
pub fn npc_get_type(npc_id: u16) -> u16 {
    npc_id & npc_id::TYPE_MASK
}

/// `mNpc_IS_SPECIAL`: true when the id's type bits are 0xD000.
pub fn npc_is_special(npc_id: u16) -> bool {
    npc_get_type(npc_id) == npc_id::SPECIAL_TYPE
}

/// `ITEM_NAME_GET_TYPE`: upper nibble of a name id
/// (13 = special NPC, 14 = villager).
pub fn name_type_of(name_id: u16) -> u8 {
    ((name_id & npc_id::TYPE_MASK) >> 12) as u8
}

/// `aCR_IS_ART`: identified art furniture range (inclusive).
pub fn curator_is_art(item: u16) -> bool {
    (curator_range::ART_LO..=curator_range::ART_HI).contains(&item)
}

/// `aCR_IS_INSECT`: insect range, end EXCLUSIVE (retail asymmetry).
pub fn curator_is_insect(item: u16) -> bool {
    (curator_range::INSECT_LO..curator_range::INSECT_HI_EXCL).contains(&item)
}

/// `aCR_IS_FISH`: fish range, end INCLUSIVE.
pub fn curator_is_fish(item: u16) -> bool {
    (curator_range::FISH_LO..=curator_range::FISH_HI).contains(&item)
}

/// Curator offer classification in `aCR_msg_win_open_wait` priority order.
/// `is_fossil` is the caller's `aCR_IS_FOSSIL` result (lives in museum.rs
/// to avoid duplicating the range); everything else is computed here.
pub fn curator_offer_class(item: u16, is_fossil: bool) -> u8 {
    if item == curator_range::EMPTY_NO {
        offer_class::EMPTY
    } else if is_fossil {
        offer_class::FOSSIL
    } else if curator_is_art(item) {
        offer_class::ART
    } else if curator_is_insect(item) {
        offer_class::INSECT
    } else if curator_is_fish(item) {
        offer_class::FISH
    } else if item == curator_range::ITM_FOSSIL {
        offer_class::GENERIC_FOSSIL
    } else {
        offer_class::OTHER
    }
}

// ---- C ABI ----
// All pc_-prefixed: the C originals stay compiled; these are staged kernels.

#[no_mangle]
pub extern "C" fn pc_visitor_is_special(npc_id: u16) -> u8 {
    npc_is_special(npc_id) as u8
}

#[no_mangle]
pub extern "C" fn pc_visitor_name_type(npc_id: u16) -> u8 {
    name_type_of(npc_id)
}

#[no_mangle]
pub extern "C" fn pc_visitor_npc_idx(npc_id: u16) -> u16 {
    npc_get_idx(npc_id)
}

#[no_mangle]
pub extern "C" fn pc_curator_is_art(item: u16) -> u8 {
    curator_is_art(item) as u8
}

#[no_mangle]
pub extern "C" fn pc_curator_is_insect(item: u16) -> u8 {
    curator_is_insect(item) as u8
}

#[no_mangle]
pub extern "C" fn pc_curator_is_fish(item: u16) -> u8 {
    curator_is_fish(item) as u8
}

#[no_mangle]
pub extern "C" fn pc_curator_offer_class(item: u16, is_fossil: u8) -> u8 {
    curator_offer_class(item, is_fossil != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_encoding() {
        assert!(npc_is_special(0xD008)); // Tom Nook
        assert!(npc_is_special(0xD06D)); // Blathers
        assert!(!npc_is_special(0xE000)); // villager
        assert!(!npc_is_special(0x1234));
        assert_eq!(npc_get_idx(0xD08F), 0x008F);
        assert_eq!(npc_get_type(0xD008), 0xD000);
        assert_eq!(name_type_of(0xD008), name_type::SPNPC);
        assert_eq!(name_type_of(0xE005), name_type::NPC);
    }

    #[test]
    fn curator_ranges() {
        assert!(curator_is_art(0x12AC));
        assert!(curator_is_art(0x12E7));
        assert!(!curator_is_art(0x12E8));
        assert!(!curator_is_art(0x12AB));
        // insect: exclusive end
        assert!(curator_is_insect(0x2D00));
        assert!(curator_is_insect(0x2D27));
        assert!(!curator_is_insect(0x2D28));
        // fish: inclusive end
        assert!(curator_is_fish(0x2300));
        assert!(curator_is_fish(0x2340));
        assert!(!curator_is_fish(0x2341));
    }

    #[test]
    fn offer_priority() {
        assert_eq!(curator_offer_class(0x0000, false), offer_class::EMPTY);
        assert_eq!(curator_offer_class(0x1EEC, true), offer_class::FOSSIL);
        assert_eq!(curator_offer_class(0x12AC, false), offer_class::ART);
        assert_eq!(curator_offer_class(0x2D00, false), offer_class::INSECT);
        assert_eq!(curator_offer_class(0x2300, false), offer_class::FISH);
        assert_eq!(curator_offer_class(0x2511, false), offer_class::GENERIC_FOSSIL);
        assert_eq!(curator_offer_class(0x9999, false), offer_class::OTHER);
        // fossil check wins over art range overlap is impossible; priority holds
        assert_eq!(curator_offer_class(0x1EEC, false), offer_class::OTHER);
    }
}
