//! Police station lost-and-found storage and building — pure data
//! and kernels.
//!
//! Verified against `src/game/m_police_box.c`,
//! `src/actor/ac_police_box.c`, `src/actor/ac_police_box_move.c_inc`,
//! `src/bg_item/bg_police_item_move.c_inc`, `src/game/m_npc.c`,
//! `src/game/m_start_data_init.c`, `src/game/m_all_grow_ovl.c`,
//! `src/actor/ac_event_manager.c`, `include/m_police_box.h`,
//! `include/m_name_table.h` (GAFE01_00 Rev 0 USA decomp).
//! All 12 brief claims checked verbatim; no corrections.
//!
//! Nuances carried: the brief's `mPB_keep_item()` caller list was
//! incomplete (8 more call sites found: br_shop, douzou,
//! event_manager, house, shop_level, m_field_info, m_start_data_init,
//! and m_police_box itself); bulk collection has FOUR cases (POOL,
//! SHRINE, STATION, PLAYER), not two; the double `mPB_police_box_init`
//! call is version-gated — exactly one compiles for GAFE01_00; the
//! display Y function takes `(pos, 1.0f)`.
//!
//! Scope split: the sibling `police` module covers Booker's
//! interaction math (slot index, dialogue, movement, pickup
//! constants) and the bulk-merge index math. This module covers the
//! storage buffer operations, the floor display mapping, and the
//! station building. The save array, the field state, the item-name
//! decoding, the item selectors, and the RNG stay in C.
//!
//! Do not replace the 20-entry array with a dynamically sized
//! collection: the save layout (`sizeof(PoliceBox_c) == 0x28`) and the
//! hole-tolerant behavior are retail behavior.

/// Storage constants (`include/m_police_box.h`).
pub mod store {
    /// 20 slots of `mActor_name_t` (u16).
    pub const COUNT: usize = 20;
    /// Grow cap for forced generation.
    pub const MAX_GROW_SIZE: u8 = 5;
    /// Empty-slot marker.
    pub const EMPTY: u16 = 0x0000;
}

/// Reserved police-item IDs (`include/m_name_table.h`): 32 IDs,
/// 0xF128–0xF147, vs 20 storage slots. The decomp's own comment flags
/// that 0xF128 is shared with DUMMY_BOAT — the constant is
/// duplicated, not a typo.
pub mod rsv {
    pub const ITEM_0: u16 = 0xF128;
    pub const ITEM_END: u16 = 0xF148;
    pub const COUNT: u8 = 32;
}

/// Reserved-range test (`ITEM_IS_RSVPOLICE`).
pub fn is_rsvpolice(n: u16) -> bool {
    n >= rsv::ITEM_0 && n < rsv::ITEM_END
}

/// Station light (`aPBOX_ctrl_light`), verbatim:
/// `!(now_sec < 64800 && now_sec >= 18000)` — the light is ON outside
/// [05:00, 18:00), OFF from 05:00 up to but not including 18:00.
pub fn station_light_on(now_sec: u32) -> bool {
    !(now_sec < 64800 && now_sec >= 18000)
}

/// Light-angle targets: ON = `DEG2SHORT_ANGLE(90.0) − 1` = 0x3FFF,
/// OFF = 0x0000, chased via `chase_s` at rate 320/frame.
pub mod light {
    pub const ON_TARGET: u16 = 0x3FFF;
    pub const OFF_TARGET: u16 = 0x0000;
    pub const CHASE_RATE: u16 = 320;
}

/// Floor-display coordinate table (`bPI_outPutData`).
pub const DISPLAY_POS_TABLE: [f32; 16] = [
    20.0, 60.0, 100.0, 140.0, 180.0, 220.0, 260.0, 300.0,
    340.0, 380.0, 420.0, 460.0, 500.0, 540.0, 580.0, 620.0,
];

/// Floor-display X offset: the LOW nibble of the foreground unit index
/// (`pos.x = block_x + pos_table[idx & 15]`). The block offset is added
/// by the caller (in C).
pub fn display_x(idx: u8) -> f32 {
    DISPLAY_POS_TABLE[(idx & 15) as usize]
}

/// Floor-display Z offset: the HIGH nibble (`pos.z = block_z +
/// pos_table[(idx >> 4) & 15]`).
pub fn display_z(idx: u8) -> f32 {
    DISPLAY_POS_TABLE[((idx >> 4) & 15) as usize]
}

/// Entry door data (`aPBOX_police_box_enter_data`): interior spawn at
/// {200, 0, 380}, facing north.
pub mod door {
    pub const ENTER_POS: (i16, i16, i16) = (200, 0, 380);
    /// Exit-door rewrite: wipe type and value.
    pub const EXIT_WIPE: u16 = 0x2168;
    /// Exit position is the station home +60/+60 in x/z.
    pub const EXIT_OFFSET: f32 = 60.0;
    /// Proximity door prompt offset.
    pub const PROMPT_OFFSET: f32 = 50.0;
}

/// Stored-item count (`mPB_get_keep_item_sum`), verbatim: scans all 20
/// slots, counts non-EMPTY_NO, never stops at the first empty entry.
/// Holes from pickups are counted correctly.
pub fn keep_item_sum(items: &[u16; store::COUNT]) -> u8 {
    let mut sum = 0u8;
    for i in 0..store::COUNT {
        if items[i] != store::EMPTY {
            sum += 1;
        }
    }
    sum
}

/// Order-preserving compaction (`mPB_copy_itemBuf`), verbatim effect.
/// Safe for in-place use (the exit path passes the save array itself):
/// the write cursor only advances on non-empty copies while the read
/// cursor advances every iteration, so writes never clobber unread
/// entries. The tail is filled with EMPTY_NO.
pub fn compact(items: &mut [u16; store::COUNT]) {
    let mut keep = 0usize;
    for i in 0..store::COUNT {
        if items[i] != store::EMPTY {
            items[keep] = items[i];
            keep += 1;
        }
    }
    for i in keep..store::COUNT {
        items[i] = store::EMPTY;
    }
}

/// Single-item insertion (`mPB_keep_item`), verbatim effect. The
/// ITEM1/FTR type filter (`ITEM_IS_ITEM1 || ITEM_IS_FTR`) needs the
/// item-name decoder and stays in C — pass its result as `accepted`.
/// When the store is full the FIRST entry is dropped and the rest
/// shift left (FIFO eviction), and the new item lands at index 19;
/// otherwise it is appended at the current count (not the first
/// hole — holes are preserved until compaction).
pub fn keep_item(items: &mut [u16; store::COUNT], item: u16, accepted: bool) -> bool {
    if !accepted {
        return false;
    }
    let sum = keep_item_sum(items) as usize;
    if sum >= store::COUNT {
        for i in 0..store::COUNT - 1 {
            items[i] = items[i + 1];
        }
        items[store::COUNT - 1] = item;
    } else {
        items[sum] = item;
    }
    true
}

/// Initial stock (`mPB_police_box_init`): one furniture and two
/// clothing items via `mSP_LISTTYPE_ABC`, the rest EMPTY_NO. The item
/// selection stays in C; this documents the slot layout.
pub mod init {
    pub const FURNITURE_SLOTS: usize = 1;
    pub const CLOTH_SLOTS: usize = 2;
    pub const FIRST_EMPTY: usize = 3;
}

// ---- C ABI ----
// All pc_-prefixed: the C originals stay compiled; these are staged kernels.
// The buffer functions take raw pointers to 20-u16 arrays.

#[no_mangle]
pub extern "C" fn pc_lf_count(items: *const u16) -> u8 {
    if items.is_null() {
        return 0;
    }
    let items = unsafe { &*(items as *const [u16; store::COUNT]) };
    keep_item_sum(items)
}

#[no_mangle]
pub extern "C" fn pc_lf_compact(items: *mut u16) {
    if !items.is_null() {
        let items = unsafe { &mut *(items as *mut [u16; store::COUNT]) };
        compact(items);
    }
}

#[no_mangle]
pub extern "C" fn pc_lf_insert(items: *mut u16, item: u16, accepted: u8) -> u8 {
    if items.is_null() {
        return 0;
    }
    let items = unsafe { &mut *(items as *mut [u16; store::COUNT]) };
    keep_item(items, item, accepted != 0) as u8
}

#[no_mangle]
pub extern "C" fn pc_lf_display_x(idx: u8) -> f32 {
    display_x(idx)
}

#[no_mangle]
pub extern "C" fn pc_lf_display_z(idx: u8) -> f32 {
    display_z(idx)
}

#[no_mangle]
pub extern "C" fn pc_lf_station_light_on(now_sec: u32) -> u8 {
    station_light_on(now_sec) as u8
}

#[no_mangle]
pub extern "C" fn pc_lf_is_rsvpolice(n: u16) -> u8 {
    is_rsvpolice(n) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constants() {
        assert_eq!(store::COUNT, 20);
        assert_eq!(store::EMPTY, 0x0000);
        assert_eq!(rsv::ITEM_0, 0xF128);
        assert_eq!(rsv::ITEM_END, 0xF148);
        assert_eq!(rsv::COUNT, 32);
        assert!(is_rsvpolice(0xF128));
        assert!(is_rsvpolice(0xF147));
        assert!(!is_rsvpolice(0xF148));
        assert!(!is_rsvpolice(0xF127));
        assert_eq!(light::ON_TARGET, 0x3FFF);
        assert_eq!(light::OFF_TARGET, 0x0000);
    }

    #[test]
    fn light() {
        assert!(station_light_on(4 * 3600 + 59 * 60)); // before 05:00
        assert!(!station_light_on(5 * 3600)); // 05:00 off
        assert!(!station_light_on(12 * 3600));
        assert!(!station_light_on(17 * 3600 + 59 * 60 + 59)); // before 18:00
        assert!(station_light_on(18 * 3600)); // 18:00 on
        assert!(station_light_on(23 * 3600));
    }

    #[test]
    fn count_and_compact() {
        let items: [u16; 20] = [
            0x2F00, 0x0000, 0x2F01, 0x0000, 0x2F02, 0x0000, 0x0000, 0x0000,
            0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000,
            0x0000, 0x0000, 0x0000, 0x0000,
        ];
        // holes don't stop the count
        assert_eq!(keep_item_sum(&items), 3);
        let mut c = items;
        compact(&mut c);
        assert_eq!(&c[0..3], &[0x2F00, 0x2F01, 0x2F02]);
        assert!(c[3..].iter().all(|&x| x == 0x0000));
        // already-packed is a no-op
        let mut p = [0x1234u16; 20];
        compact(&mut p);
        assert!(p.iter().all(|&x| x == 0x1234));
        // all empty stays empty
        let mut e = [0x0000u16; 20];
        compact(&mut e);
        assert!(e.iter().all(|&x| x == 0x0000));
    }

    #[test]
    fn insert() {
        // rejected type: untouched
        let mut items = [0x0000u16; 20];
        assert!(!keep_item(&mut items, 0x2F00, false));
        assert_eq!(keep_item_sum(&items), 0);
        // append at count
        assert!(keep_item(&mut items, 0x2F00, true));
        assert!(keep_item(&mut items, 0x2F01, true));
        assert_eq!(&items[0..2], &[0x2F00, 0x2F01]);
        // append goes to the COUNT index, not the first hole (mPB_keep_item
        // writes keep_items[sum], sum = non-empty slots). With a hole at 0
        // the count is 1, so retail OVERWRITES the item at index 1.
        items[0] = 0x0000;
        assert!(keep_item(&mut items, 0x2F02, true));
        assert_eq!(items[0], 0x0000);
        assert_eq!(items[1], 0x2F02);
        assert_eq!(items[2], 0x0000);
        // full: drop first, shift left, append at 19
        let mut full = [0x1000u16; 20];
        for (i, v) in full.iter_mut().enumerate() {
            *v = 0x1000 + i as u16;
        }
        assert!(keep_item(&mut full, 0x2F00, true));
        assert_eq!(full[0], 0x1001);
        assert_eq!(full[18], 0x1013);
        assert_eq!(full[19], 0x2F00);
        assert_eq!(keep_item_sum(&full), 20);
    }

    #[test]
    fn display() {
        assert_eq!(display_x(0x00), 20.0);
        assert_eq!(display_z(0x00), 20.0);
        assert_eq!(display_x(0x0F), 620.0);
        assert_eq!(display_z(0x0F), 20.0);
        assert_eq!(display_x(0x10), 20.0);
        assert_eq!(display_z(0x10), 60.0);
        assert_eq!(display_x(0xFF), 620.0);
        assert_eq!(display_z(0xFF), 620.0);
        assert_eq!(door::ENTER_POS, (200, 0, 380));
        assert_eq!(door::EXIT_WIPE, 0x2168);
    }
}
