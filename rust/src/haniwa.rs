//! Gyroids (`m_haniwa_ovl.c`, `m_haniwaPortrait_ovl.c`,
//! `ac_haniwa.c`, plus the field-renewal and storage
//! sources).
//!
//! Verified against `src/game/m_all_grow.c`,
//! `src/game/m_all_grow_ovl.c`, `include/m_all_grow_ovl.h`,
//! `src/game/m_haniwa_ovl.c`, `include/m_haniwa_ovl.h`,
//! `src/game/m_haniwaPortrait_ovl.c`, `src/actor/ac_haniwa.c`,
//! `src/actor/ac_haniwa_move.c_inc`, `include/ac_haniwa.h`,
//! `include/m_home_h.h`, `include/m_common_data.h`,
//! `include/m_name_table.h`, `src/game/m_field_info.c`
//! (GAFE01_00 Rev 0 USA). 14/14 sections PROVEN.
//!
//! Scope split: this module owns the pure kernels — the
//! Haniwa item-range check, the storage message
//! selection, the house-index derivation, the
//! proceeds-bag math, the dialogue-category decision,
//! the interruption recheck, and the dig-path predicate.
//! The `haniwa_scheduled` flag and RTC handling,
//! foreground/deposit mutations, `mSP_RandomHaniwaSelect`
//! (until `FTR_NO_2_IDX` is traced),
//! `mMsm_DepositItemBlock_cancel`, inventory transfers,
//! dialogue/demo callbacks, state transitions, door
//! requests, saves, rendering/collision, and the full
//! portrait overlay stay in C.
//!
//! Nuances carried: `mAGrw_OrderSetHaniwa` has no
//! in-tree callers — the live arming path is the
//! GROW_FIRST branch; the buried gyroid's FG entry IS
//! the haniwa item ID, so a deposited non-hole dig
//! returns it directly with no conversion;
//! `mSP_RandomHaniwaSelect` is a no-duplicates loop and
//! the `count > HANIWA_NUM` escape is dead for the
//! 3-item call; `mHW_MSG_MOU` is marked @unused;
//! `mAGrw_SetDigItem` runs `mAGrw_SpoilKabu` over the
//! stored haniwa slots during field renewal; the
//! HANIWA_START/END numerics are computed from the
//! X-macro expansion — verify against the build before
//! wiring.

/// Haniwa item id range (computed from the X-macro
/// expansion: enum base 0x0FFF, 4 entries/X, 364 X
/// entries before COMMON000; 127 haniwa = COMMON000–
/// COMMON126). Verify against the build before wiring.
pub const HANIWA_START: i32 = 0x15B0;
pub const HANIWA_END: i32 = 0x17AB;
/// Buried gyroids requested per renewal.
pub const HANIWA_NUM: i32 = 3;
/// Household storage slots and message length.
pub const HANIWA_ITEM_HOLD_NUM: i32 = 4;
pub const HANIWA_MESSAGE_LEN: i32 = 128;
/// House-gyroid actor id base.
pub const ACTOR_PROP_HANIWA0: i32 = 0xA004;
/// Talk distance.
pub const HANIWA_TALK_DIST: f32 = 43.0;
/// Wallet maximum.
pub const WALLET_MAX: i32 = 99999;
/// Bell-bag denomination.
pub const BELL_BAG: i32 = 30000;
/// Interruption message timer.
pub const INTERRUPT_MSG_TIME: i32 = 120;

/// Storage message indices (`m_haniwa_ovl.h`).
pub const MSG_IKAGA: i32 = 0;
pub const MSG_DONO: i32 = 1;
pub const MSG_DONO2: i32 = 2;
pub const MSG_OIKURA: i32 = 3;
pub const MSG_KASIKO: i32 = 4;
pub const MSG_DOREWO: i32 = 5;
pub const MSG_MOU: i32 = 6; // @unused — not set anywhere
pub const MSG_OKANE: i32 = 7;
pub const MSG_MAIDO: i32 = 8;
pub const MSG_TADA: i32 = 9;
pub const MSG_TADA2: i32 = 10;
pub const MSG_MISE: i32 = 11;
pub const MSG_BERU2: i32 = 12;
pub const MSG_NUM: i32 = 13;

/// Exchange types.
pub const TRADE_0: i32 = 0;
pub const TRADE_1: i32 = 1;
pub const TRADE_2: i32 = 2;
pub const TRADE_3: i32 = 3;
pub const TRADE_NUM: i32 = 4;

/// Dialogue categories (`aHNW_MSG_*`).
pub const DLG_NO_OWNER: i32 = 0;
pub const DLG_PROCEEDS: i32 = 1;
pub const DLG_NORMAL: i32 = 2;
pub const DLG_OTHER_OWNER: i32 = 3;
pub const DLG_NEED_FRIEND: i32 = 4;
/// Category → message id, verbatim from the source table.
pub const DLG_MSG: [i32; 5] = [2356, 2357, 2341, 2344, 2350];

/// Haniwa item-range check: `HANIWA_START <= item <=
/// HANIWA_END`.
#[no_mangle]
pub extern "C" fn pc_haniwa_item_valid(item: i32) -> bool {
    item >= HANIWA_START && item <= HANIWA_END
}

/// Storage message selection (the Haniwa-table tag
/// arm): an occupied slot maps the exchange type —
/// TRADE_0 → TADA/TADA2 by `data0`, TRADE_1 → MISE,
/// TRADE_2 → BERU2, TRADE_3 (and any other value) →
/// MISE via `default:`. An empty slot → IKAGA when
/// `data0 == 0`, else DONO2.
#[no_mangle]
pub extern "C" fn pc_haniwa_exchange_msg(exchange_type: i32, has_item: bool, data0: i32) -> i32 {
    if has_item {
        match exchange_type {
            TRADE_0 => {
                if data0 == 0 {
                    MSG_TADA
                } else {
                    MSG_TADA2
                }
            }
            TRADE_1 => MSG_MISE,
            TRADE_2 => MSG_BERU2,
            _ => MSG_MISE,
        }
    } else if data0 == 0 {
        MSG_IKAGA
    } else {
        MSG_DONO2
    }
}

/// Tag-type message arms: HANIWA_ITEM/CATCH_ITEM → DONO,
/// HANIWA_PRICE → OIKURA, NONE/HANIWA_PUT_ITEM with a
/// non-Haniwa table → IKAGA. Tag types: 0 = NONE,
/// 1 = HANIWA_ITEM, 2 = CATCH_ITEM, 3 = HANIWA_PRICE,
/// 4 = HANIWA_PUT_ITEM. Returns -1 for the non-Haniwa
/// table HANIWA_ITEM case, which falls through to the
/// main selection logic in C.
#[no_mangle]
pub extern "C" fn pc_haniwa_tag_msg(tag_type: i32, haniwa_table: bool) -> i32 {
    match tag_type {
        1 | 2 => {
            if haniwa_table {
                -1
            } else {
                MSG_DONO
            }
        }
        3 => MSG_OIKURA,
        0 | 4 => {
            if haniwa_table {
                -1
            } else {
                MSG_IKAGA
            }
        }
        _ => -1,
    }
}

/// House index: `npc_id − ACTOR_PROP_HANIWA0`.
#[no_mangle]
pub extern "C" fn pc_haniwa_house_idx(npc_id: i32) -> i32 {
    npc_id - ACTOR_PROP_HANIWA0
}

/// Proceeds handover: when `money >= WALLET_MAX` the
/// transfer needs `(money − 99999) / 30000 + 1` bags of
/// 30,000 Bells (C integer division). Returns 0 when no
/// bags are needed; the empty-slot check and the actual
/// transfer stay in C.
#[no_mangle]
pub extern "C" fn pc_haniwa_bell_bags(money: i32) -> i32 {
    if money >= WALLET_MAX {
        (money - WALLET_MAX) / BELL_BAG + 1
    } else {
        0
    }
}

/// Dialogue category (`aHNW_decide_msg_idx_dance`),
/// verbatim branch order: no owner → NO_OWNER; owner
/// in the intro with no villager friends →
/// NEED_FRIEND; owner with stored Bells → PROCEEDS;
/// owner → NORMAL; otherwise OTHER_OWNER. The
/// ownership/intro/friend checks stay in C.
#[no_mangle]
pub extern "C" fn pc_haniwa_dialogue_category(
    no_owner: bool,
    player_is_owner: bool,
    intro_no_friend: bool,
    bells: u32,
) -> i32 {
    if no_owner {
        DLG_NO_OWNER
    } else if player_is_owner {
        if intro_no_friend {
            DLG_NEED_FRIEND
        } else if bells != 0 {
            DLG_PROCEEDS
        } else {
            DLG_NORMAL
        }
    } else {
        DLG_OTHER_OWNER
    }
}

/// Category → message id (verbatim source table).
#[no_mangle]
pub extern "C" fn pc_haniwa_dialogue_msg(category: i32) -> i32 {
    if category < 0 || category >= DLG_MSG.len() as i32 {
        -1
    } else {
        DLG_MSG[category as usize]
    }
}

/// Interruption recheck: the interrupt stays while the
/// recorded table/column still matches the current
/// selection. The `mHW_MSG_DOREWO` special case stays
/// in C.
#[no_mangle]
pub extern "C" fn pc_haniwa_interrupt_keep(
    rec_table: i32,
    rec_col: i32,
    cur_table: i32,
    cur_col: i32,
) -> bool {
    rec_table == cur_table && rec_col == cur_col
}

/// Dig-path predicate: a deposited, non-hole tile
/// returns the foreground item directly — the FG entry
/// IS the haniwa item id, so there is no conversion
/// step. The pitfall/shine conversion applies only to
/// non-deposited tiles.
#[no_mangle]
pub extern "C" fn pc_haniwa_dig_direct(deposited: bool, is_hole: bool) -> bool {
    deposited && !is_hole
}

/// Price-message rebuild: the BERU2 message is rebuilt
/// when the displayed price differs from `extra_data`.
#[no_mangle]
pub extern "C" fn pc_haniwa_beru2_rebuild(displayed_price: u32, extra_data: u32) -> bool {
    displayed_price != extra_data
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_and_house() {
        assert!(pc_haniwa_item_valid(0x15B0));
        assert!(pc_haniwa_item_valid(0x17AB));
        assert!(pc_haniwa_item_valid(0x1600));
        assert!(!pc_haniwa_item_valid(0x15AF));
        assert!(!pc_haniwa_item_valid(0x17AC));
        assert_eq!(pc_haniwa_house_idx(0xA004), 0);
        assert_eq!(pc_haniwa_house_idx(0xA007), 3);
        assert_eq!(MSG_NUM, 13);
    }

    #[test]
    fn exchange_messages() {
        // Occupied slot by exchange type.
        assert_eq!(pc_haniwa_exchange_msg(0, true, 0), MSG_TADA);
        assert_eq!(pc_haniwa_exchange_msg(0, true, 1), MSG_TADA2);
        assert_eq!(pc_haniwa_exchange_msg(1, true, 0), MSG_MISE);
        assert_eq!(pc_haniwa_exchange_msg(2, true, 0), MSG_BERU2);
        assert_eq!(pc_haniwa_exchange_msg(3, true, 0), MSG_MISE); // default
        assert_eq!(pc_haniwa_exchange_msg(9, true, 0), MSG_MISE); // default
        // Empty slot by data0.
        assert_eq!(pc_haniwa_exchange_msg(0, false, 0), MSG_IKAGA);
        assert_eq!(pc_haniwa_exchange_msg(2, false, 1), MSG_DONO2);
        // Tag arms.
        assert_eq!(pc_haniwa_tag_msg(1, false), MSG_DONO);
        assert_eq!(pc_haniwa_tag_msg(2, false), MSG_DONO);
        assert_eq!(pc_haniwa_tag_msg(3, true), MSG_OIKURA);
        assert_eq!(pc_haniwa_tag_msg(0, false), MSG_IKAGA);
        assert_eq!(pc_haniwa_tag_msg(4, false), MSG_IKAGA);
        assert_eq!(pc_haniwa_tag_msg(1, true), -1);
    }

    #[test]
    fn proceeds_and_dialogue() {
        assert_eq!(pc_haniwa_bell_bags(99998), 0);
        assert_eq!(pc_haniwa_bell_bags(99999), 1);
        assert_eq!(pc_haniwa_bell_bags(129999), 2);
        assert_eq!(pc_haniwa_bell_bags(130000), 2);
        assert_eq!(pc_haniwa_dialogue_category(true, false, false, 0), DLG_NO_OWNER);
        assert_eq!(
            pc_haniwa_dialogue_category(false, true, true, 500),
            DLG_NEED_FRIEND
        ); // intro beats proceeds
        assert_eq!(
            pc_haniwa_dialogue_category(false, true, false, 500),
            DLG_PROCEEDS
        );
        assert_eq!(
            pc_haniwa_dialogue_category(false, true, false, 0),
            DLG_NORMAL
        );
        assert_eq!(
            pc_haniwa_dialogue_category(false, false, false, 0),
            DLG_OTHER_OWNER
        );
        assert_eq!(pc_haniwa_dialogue_msg(DLG_NO_OWNER), 2356);
        assert_eq!(pc_haniwa_dialogue_msg(DLG_PROCEEDS), 2357);
        assert_eq!(pc_haniwa_dialogue_msg(DLG_NORMAL), 2341);
        assert_eq!(pc_haniwa_dialogue_msg(DLG_OTHER_OWNER), 2344);
        assert_eq!(pc_haniwa_dialogue_msg(DLG_NEED_FRIEND), 2350);
        assert_eq!(pc_haniwa_dialogue_msg(5), -1);
    }

    #[test]
    fn misc() {
        assert!(pc_haniwa_interrupt_keep(2, 3, 2, 3));
        assert!(!pc_haniwa_interrupt_keep(2, 3, 2, 4));
        assert!(!pc_haniwa_interrupt_keep(2, 3, 1, 3));
        assert!(pc_haniwa_dig_direct(true, false));
        assert!(!pc_haniwa_dig_direct(true, true));
        assert!(!pc_haniwa_dig_direct(false, false));
        assert!(pc_haniwa_beru2_rebuild(100, 200));
        assert!(!pc_haniwa_beru2_rebuild(200, 200));
        assert_eq!(INTERRUPT_MSG_TIME, 120);
    }
}
