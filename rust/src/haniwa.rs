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
//! 3-item call (name-table HANIWA_NUM is 127);
//! `mHW_MSG_MOU` is set (literal 6) by
//! `mTG_present_open_proc` when pockets are full;
//! `mAGrw_SpoilAllPossession` (not SetDigItem) spoils
//! turnips in the stored haniwa slots during field
//! renewal; HANIWA_START/END verified with gcc
//! (claude 2026-10-12, see the follow-up section).

/// Haniwa item id range (COMMON000–COMMON126 south..west;
/// verified by compiling m_name_table.h with gcc).
pub const HANIWA_START: i32 = 0x15B0;
pub const HANIWA_END: i32 = 0x17AB;
/// Buried gyroids requested per renewal (`mAGrw_HANIWA_NUM`;
/// the name-table `HANIWA_NUM` is `HANIWA_KIND_NUM` = 127).
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
pub const MSG_MOU: i32 = 6; // set as literal 6 by mTG_present_open_proc (pockets full)
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

// ---------------------------------------------------------------------------
// Follow-ups resolved (claude, 2026-10-12)
// Sources: ac-decomp m_shop.c (mSP_RandomHaniwaSelect), m_haniwa_ovl.c
// (interrupt messages), m_tag_ovl.c (gyroid storage trade/buy),
// m_all_grow_ovl.c (mAGrw_SetHaniwa, mAGrw_SpoilKabu,
// mAGrw_SpoilAllPossession). IDs compiled from the real m_name_table.h with
// mingw32 gcc. The PC port is unchanged for all of these except
// ac_haniwa_move.c_inc (dt only).
//
// Corrections to the notes above:
// - `HANIWA_NUM` above (3) is `mAGrw_HANIWA_NUM` (gyroids buried per
//   renewal). The name-table `HANIWA_NUM` is 127 = `HANIWA_KIND_NUM`.
// - HANIWA_START/END are verified: gcc gives 0x15B0 / 0x17AB.
// - `mHW_MSG_MOU` (6) is not unused: `mTG_present_open_proc` sets
//   `msg_interrupt_idx = 6` (literal) when the pockets are full.
// - `mAGrw_SetDigItem` does not spoil anything. The sweep over the stored
//   gyroid slots is `mAGrw_SpoilAllPossession`, run right after
//   SetDigItem during field renewal.
// ---------------------------------------------------------------------------

/// Name-table `HANIWA_NUM`: distinct gyroid kinds (COMMON000..COMMON126).
pub const HANIWA_KIND_NUM: i32 = 127;
/// Last south-facing gyroid id (`HANIWA_START + FTR_NO_2_IDX(126)`).
pub const HANIWA_LAST_SOUTH: i32 = 0x17A8;
/// Interrupt-message duration in frames.
pub const INTERRUPT_FRAMES: i32 = 120;

/// `FTR_NO_2_IDX`: furniture items come in 4 rotations.
pub const fn ftr_no_2_idx(f: i32) -> i32 {
    f << 2
}

/// `mSP_RandomHaniwaSelect`: `count` south-facing gyroid ids with no
/// duplicates; `random(n)` is `RANDOM(n)`. Duplicates are only accepted
/// when `count > 127`, so the escape is dead for the 3-gyroid renewal.
pub fn random_haniwa_select(count: usize, mut random: impl FnMut(i32) -> i32) -> Vec<i32> {
    let mut list: Vec<i32> = Vec::with_capacity(count);
    for _ in 0..count {
        loop {
            let haniwa = HANIWA_START + ftr_no_2_idx(random(HANIWA_KIND_NUM));
            if !list.contains(&haniwa) || count > HANIWA_KIND_NUM as usize {
                list.push(haniwa);
                break;
            }
        }
    }
    list
}

/// Interrupt message state (`msg_interrupt_idx`, `msg_time`, table/sub).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InterruptMsg {
    pub idx: i32,
    pub time: i32,
    pub table: i32,
    pub sub: i32,
}

/// Tag cursor state seen by the interrupt logic. `tag_table`/`tag_col`
/// describe `tags[sel_tag_idx]` and are only read when it is valid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TagCursor {
    pub sel_tag_idx: i32,
    pub ret_tag_idx: i32,
    pub tag_table: i32,
    pub tag_col: i32,
}

/// `mHW_make_message_interrupt`: returns the message to show this frame.
/// It returns the *previous* index even on the frame it is cleared.
/// DOREWO ("which one?") survives while no tag is open and is dropped
/// once the selected tag points elsewhere; other messages are also
/// dropped when the tag stack returns.
pub fn interrupt_message_step(m: &mut InterruptMsg, t: &TagCursor) -> i32 {
    let shown = m.idx;
    if m.time - 1 < 0 {
        m.idx = -1;
    }
    m.time -= 1;
    let moved = t.tag_table != m.table || t.tag_col != m.sub;
    if shown != MSG_DOREWO {
        if t.sel_tag_idx == 0 {
            if moved {
                m.idx = -1;
            }
        } else if t.ret_tag_idx == t.sel_tag_idx {
            m.idx = -1;
        }
    } else if t.sel_tag_idx != -1 && moved {
        m.idx = -1;
    }
    shown
}

/// `mHW_haniwa_ovl_init`: `data0 == 1` opens with DOREWO for 120 frames.
pub fn interrupt_on_open(data0: i32, haniwa_table: i32) -> InterruptMsg {
    if data0 == 1 {
        InterruptMsg { idx: MSG_DOREWO, time: INTERRUPT_FRAMES, table: haniwa_table, sub: 0 }
    } else {
        InterruptMsg { idx: -1, time: 0, table: 0, sub: 0 }
    }
}

/// Bell-bag items in the order `mTG_present_open_proc` spends them.
pub const MONEY_ITEMS: [(u16, u32); 4] = [(0x2103, 100), (0x2100, 1000), (0x2101, 10000), (0x2102, 30000)];

/// `mTG_itemNo_to_amount`.
pub fn money_item_amount(item: u16) -> u32 {
    MONEY_ITEMS.iter().find(|m| m.0 == item).map_or(0, |m| m.1)
}

/// Result of buying a gyroid-stored item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HaniwaBuy {
    /// No empty normal pocket: message 6 (`mHW_MSG_MOU`).
    PocketsFull,
    /// Wallet + bell bags < price: `mHW_MSG_OKANE`.
    NotEnough,
    /// Bought (`mHW_MSG_MAIDO`): new wallet, pocket slots of the bags
    /// spent, and the slot that receives the item.
    Bought { wallet: u32, spent_slots: Vec<usize>, item_slot: usize },
}

/// `mTG_present_open_proc`. `pockets` = (item, condition is NORMAL).
/// Bags are spent smallest denomination first, scanning the pockets for
/// each denomination; any change goes back to the wallet. The item goes to
/// the first empty slot found *before* the bags were spent.
pub fn haniwa_buy(wallet: u32, pockets: &[(u16, bool)], price: u32) -> HaniwaBuy {
    let Some(item_slot) = pockets.iter().position(|&(it, normal)| it == 0 && normal) else {
        return HaniwaBuy::PocketsFull;
    };
    let total: i64 = wallet as i64
        + pockets.iter().filter(|p| p.1).map(|p| money_item_amount(p.0) as i64).sum::<i64>();
    if total - (price as i64) < 0 {
        return HaniwaBuy::NotEnough;
    }
    let mut keep = wallet as i64 - price as i64;
    let mut spent = Vec::new();
    for &(name, amount) in MONEY_ITEMS.iter() {
        if keep >= 0 {
            break;
        }
        for (k, &(it, normal)) in pockets.iter().enumerate() {
            if normal && it == name && !spent.contains(&k) {
                keep += amount as i64;
                spent.push(k);
                if keep >= 0 {
                    break;
                }
            }
        }
    }
    HaniwaBuy::Bought { wallet: keep as u32, spent_slots: spent, item_slot }
}

/// Wallet roll-up animation after a paid trade: `None` = instant
/// (|change| < 42), else the per-frame step over 42 frames, nudged toward
/// zero when it is a multiple of 5 so the last digits visibly roll.
pub fn money_roll_step(change: i32) -> Option<i32> {
    if change.abs() < 42 {
        return None;
    }
    let mut step = change / 42;
    if step.abs() % 5 == 0 {
        step += if step > 0 { -1 } else { 1 };
    }
    Some(step)
}

/// Turnip ids (`ITM_KABU_*`, gcc-evaluated).
pub const ITM_KABU_START: u16 = 0x2F00;
pub const ITM_KABU_END: u16 = 0x2F04;
pub const ITM_KABU_SPOILED: u16 = 0x2F03;

/// `mAGrw_SpoilKabu`: any ITEM1 item in category KABU (0x2Fxx) becomes
/// spoiled, unconditionally when `grow_spoil` is None (possessions) or
/// when the grow info says so. Returns whether the item was a turnip.
pub fn spoil_kabu(item: &mut u16, grow_spoil: Option<bool>) -> bool {
    let is_kabu = (*item >> 12) == 2 && ((*item & 0x0F00) >> 8) == 0xF;
    if is_kabu && grow_spoil.unwrap_or(true) {
        *item = ITM_KABU_SPOILED;
    }
    is_kabu
}

/// `mAGrw_CheckSpoilKabuTime`: days added to the purchase time to reach
/// the next Sunday (Sunday itself adds 0); turnips spoil once the clock
/// passes that point (at the purchase hour).
pub fn days_until_spoil(weekday: u8) -> u8 {
    if weekday != 0 && weekday < 7 {
        7 - weekday
    } else {
        0
    }
}

/// Places `mAGrw_SpoilAllPossession` touches when turnips spoil.
pub const SPOIL_SCOPE: [&str; 4] = [
    "every player's pockets (plus the foreigner's)",
    "every room and layer of every house",
    "the 4 gyroid storage slots of every house",
    "not the island: mAGrw_SpoilKabuIsland is called from Kapp'n's boat",
];

/// `mAGrw_SetHaniwa` block choice for each gyroid: `candidates[b]` = number
/// of free units in block b (0 = not usable). QUIRK: when only one block
/// remains it is not consumed, so later gyroids can land in the same block.
pub fn haniwa_blocks(candidates: &mut [u8], gyroids: usize, mut random: impl FnMut(i32) -> i32) -> Vec<usize> {
    let mut usable = candidates.iter().filter(|&&c| c != 0).count() as i32;
    let mut out = Vec::new();
    for _ in 0..gyroids {
        if usable == 0 {
            break;
        }
        let mut selected = random(usable);
        for (b, c) in candidates.iter_mut().enumerate() {
            if *c != 0 {
                if selected <= 0 {
                    out.push(b);
                    if usable > 1 {
                        usable -= 1;
                        *c = 0;
                    }
                    break;
                }
                selected -= 1;
            }
        }
    }
    out
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

#[cfg(test)]
mod followup_tests {
    use super::*;

    #[test]
    fn ids_and_select() {
        assert_eq!(HANIWA_START + ftr_no_2_idx(126), HANIWA_LAST_SOUTH);
        let mut seq = [5, 5, 9, 0].into_iter();
        let l = random_haniwa_select(3, |_| seq.next().unwrap());
        assert_eq!(l, vec![0x15B0 + 20, 0x15B0 + 36, 0x15B0]);
    }

    #[test]
    fn interrupt_and_buy() {
        let mut m = interrupt_on_open(1, 7);
        let t = TagCursor { sel_tag_idx: -1, ret_tag_idx: -1, tag_table: 0, tag_col: 0 };
        assert_eq!(interrupt_message_step(&mut m, &t), MSG_DOREWO);
        assert_eq!(m.idx, MSG_DOREWO);
        let moved = TagCursor { sel_tag_idx: 0, ret_tag_idx: -1, tag_table: 1, tag_col: 0 };
        assert_eq!(interrupt_message_step(&mut m, &moved), MSG_DOREWO);
        assert_eq!(m.idx, -1);
        // 500 wallet + a 1000 bag + a 100 bag, price 1200: 100 bag first.
        let pockets = [(0x2100u16, true), (0x2103, true), (0, true)];
        assert_eq!(haniwa_buy(500, &pockets, 1200), HaniwaBuy::Bought { wallet: 400, spent_slots: vec![1, 0], item_slot: 2 });
        assert_eq!(haniwa_buy(0, &pockets, 5000), HaniwaBuy::NotEnough);
        assert_eq!(haniwa_buy(0, &[(1, true)], 0), HaniwaBuy::PocketsFull);
        assert_eq!(money_roll_step(30), None);
        assert_eq!(money_roll_step(420), Some(9));
    }

    #[test]
    fn spoil_and_blocks() {
        let mut k = 0x2F00;
        assert!(spoil_kabu(&mut k, None));
        assert_eq!(k, ITM_KABU_SPOILED);
        let mut k2 = 0x2F01;
        assert!(spoil_kabu(&mut k2, Some(false)));
        assert_eq!(k2, 0x2F01);
        assert_eq!(days_until_spoil(0), 0);
        assert_eq!(days_until_spoil(3), 4);
        let mut c = [0, 2, 0];
        assert_eq!(haniwa_blocks(&mut c, 3, |_| 0), vec![1, 1, 1]);
    }
}
