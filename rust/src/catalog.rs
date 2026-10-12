//! Catalog collection UI data model and the Nook order transaction.
//!
//! Verified against `src/game/m_catalog_ovl.c`,
//! `include/m_catalog_ovl.h`, `src/game/m_tag_ovl.c`,
//! `src/actor/npc/ac_npc_shop_common.c`,
//! `include/ac_npc_shop_common.h`, `src/game/m_shop.c`,
//! `include/m_private.h` (GAFE01_00 Rev 0 USA decomp). All 12 brief
//! claims checked; no behavioral corrections. The catalog-to-order
//! chain turned out fully traceable, stronger than the brief's
//! "strong reconstruction".
//!
//! Nuances carried: only FIVE `*_init` functions exist (furniture,
//! paper, wall, carpet, music) — umbrellas/haniwa/fossils/clothing
//! go through `mCL_furniture_init` via the `mCL_item_data_set`
//! dispatch; exactly five collected bitfields exist (others reuse the
//! furniture bitfield); `aNSC_set_ftr_order` has NO NULL check (the
//! caller guarantees a free slot); `mSP_get_sell_price` folds
//! consumed bag value into the wallet and the remainder may exceed
//! `mPr_WALLET_MAX` (no change is made into bags); the
//! `no_convert_tools` parameter of the FTR→ITEM1 conversion is dead
//! (never read); price == 0 ("Not for Sale") becomes
//! ORDER_UNAVAILABLE via `slot_no = 0`.
//!
//! Scope split: the sibling `special_delivery` module owns the
//! five-slot order queue, the placement, the ticket-first delivery,
//! and the mail template. This module owns the catalog collection
//! model, the page/selection math, the price rules, and the
//! order-transaction boundary. Rendering, animation, input, the
//! item-name decoding, the FTR index tables, and the save stay in C.

/// Catalog menu constants (`include/m_catalog_ovl.h`).
pub mod menu {
    /// Max collected entries per category menu.
    pub const ITEM_MAX: usize = 742;
    /// Rows per page.
    pub const PAGE_SIZE: usize = 7;
    /// Preview buffers (page-transition animation).
    pub const ITEM_DATA_NUM: usize = 2;
}

/// Catalog categories (`mCL_MENU_*`, 0-based enum, 9 entries).
pub mod category {
    pub const FTR: u8 = 0;
    pub const WALL: u8 = 1;
    pub const CARPET: u8 = 2;
    pub const CLOTH: u8 = 3;
    pub const UMBRELLA: u8 = 4;
    pub const PAPER: u8 = 5;
    pub const HANIWA: u8 = 6;
    pub const FOSSIL: u8 = 7;
    pub const MUSIC: u8 = 8;
    pub const NUM: u8 = 9;
}

/// Collected-bitfield word counts (`include/m_private.h`). Only five
/// exist; umbrella/haniwa/fossil/clothing reuse the furniture field.
pub mod bitfields {
    pub const FURNITURE_WORDS: usize = 43;
    pub const WALL_WORDS: usize = 3;
    pub const CARPET_WORDS: usize = 3;
    pub const PAPER_WORDS: usize = 2;
    pub const MUSIC_WORDS: usize = 2;
}

/// Collection-bit test (`mCL_check_item_collect_bit`), verbatim:
/// word index `idx >> 5`, bit `idx & 0x1F`, tests
/// `(1 << subidx) & bit_data[entry]`.
pub fn check_collect_bit(bit_data: &[u32], idx: u32) -> bool {
    let entry = (idx >> 5) as usize;
    let subidx = idx & 0x1F;
    match bit_data.get(entry) {
        Some(&word) => ((1u32 << subidx) & word) != 0,
        None => false,
    }
}

/// Category completion (`mCL_catalog_ovl_init`), verbatim:
/// `completed_flag` is set when the collected count equals that
/// category's catalog list length — not the pocket/house contents.
pub fn completed(item_count: i16, list_size: i16) -> bool {
    item_count == list_size
}

/// Selected entry (`mCL_item_data_set`), verbatim:
/// `item_no = page->item_list[page->top_idx + page->y_idx]`.
pub fn selected_index(top_idx: i16, y_idx: i16) -> u16 {
    (top_idx + y_idx) as u16
}

/// Special furniture index ranges that are always priced
/// (`mCL_furniture_init`): balloons, fans, pinwheels, gold tools,
/// tools. Diaries/notebooks use the `FTR_NOG_COLLEGENOTE ..
/// FTR_IKE_NIKKI_WAFU1` symbol range (C-side).
pub mod special_range {
    pub const BALLOON: (u16, u16) = (0x3FC, 0x403);
    pub const FAN: (u16, u16) = (0x453, 0x45A);
    pub const PINWHEEL: (u16, u16) = (0x45B, 0x462);
    pub const GOLD_TOOL: (u16, u16) = (0x44F, 0x452);
    pub const TOOL: (u16, u16) = (0x463, 0x466);
}

/// Price rule for a furniture-catalog entry, from `mCL_furniture_init`.
/// The list-gated and symbol-range cases need the C item tables, so
/// the caller resolves them; the unconditional cases are exact here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PriceRule {
    /// Haniwa, fossils, music, and the non-FTR/non-ITEM1 fallback:
    /// price is unconditionally 0 → "Not for Sale".
    AlwaysZero,
    /// Cloth (only when in the COMMON/UNCOMMON/RARE cloth lists),
    /// other furniture (only when in the furniture priority lists),
    /// paper/wall/carpet (only when in their lists).
    ListGated,
    /// Umbrellas, the special index ranges, diaries/notebooks:
    /// always priced via `mSP_ItemNo2ItemPrice`.
    AlwaysPriced,
}

/// Order-selection result carried back to the shop dialogue
/// (`mTG_order_proc`, verbatim): the FTR-space selection is converted
/// to ITEM1-space (C-side), and `price == 0` ("Not for Sale") is
/// carried as `slot_no = 0`.
pub fn order_slot(price: u32) -> u8 {
    if price == 0 { 0 } else { 1 }
}

/// What the shop dialogue does with the returned selection
/// (`aNSC_msg_win_open_wait2`, verbatim): EMPTY_NO → cancel;
/// `slot_no != 0` → ORDER_OFFER; otherwise ORDER_UNAVAILABLE.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OrderAction {
    Cancel = 0,
    Unavailable = 1,
    Offer = 2,
}

pub fn order_action(item: u16, slot_no: u8) -> OrderAction {
    if item == 0x0000 {
        OrderAction::Cancel
    } else if slot_no != 0 {
        OrderAction::Offer
    } else {
        OrderAction::Unavailable
    }
}

/// Order confirmation outcome (`aNSC_order_check`, verbatim): on the
/// affirmative choice the price comes from `mSP_ItemNo2ItemPrice`
/// (C-side); insufficient funds → ORDER_INSUFFICIENT_FUNDS, otherwise
/// the order is stored, the price is taken, and sales are updated →
/// ORDER_CONFIRM. The decline choice → ORDER_CANCEL.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OrderMsg {
    Confirm = 0,
    InsufficientFunds = 1,
    Cancel = 2,
}

pub fn order_check_msg(chose_yes: bool, funds_ok: bool) -> OrderMsg {
    if !chose_yes {
        OrderMsg::Cancel
    } else if funds_ok {
        OrderMsg::Confirm
    } else {
        OrderMsg::InsufficientFunds
    }
}

/// Bell-bag values used by the shop money routines (`mSP_sack_amount`).
pub const SACK_AMOUNTS: [u32; 4] = [100, 1_000, 10_000, 30_000];

/// Funds check (`mSP_money_check`), verbatim: the wallet first; if it
/// falls short, normal-condition bag values are added
/// (100 → 30000 order) until the total covers the amount.
/// `bags` holds the per-denomination bag counts.
pub fn money_check(amount: u32, wallet: u32, bags: [u32; 4]) -> bool {
    if wallet >= amount {
        return true;
    }
    let mut money = wallet;
    for i in 0..4 {
        money += bags[i] * SACK_AMOUNTS[i];
        if money >= amount {
            return true;
        }
    }
    false
}

/// Payment wallet result (`mSP_get_sell_price` +
/// `mSP_get_sell_price_sub`), verbatim arithmetic: if the wallet
/// covers the price it is simply debited; otherwise bags are consumed
/// smallest-denomination-first, each bag's value folded into the
/// money, stopping as soon as the total covers the price — and the
/// remainder is written back to the wallet WITHOUT making change
/// into bags, so the wallet may exceed `mPr_WALLET_MAX`. The `return
/// 0` fallthrough is unreachable on the order path (the funds check
/// passed). The inventory mutations stay in C; this is the wallet
/// number only.
pub fn sell_price_wallet(amount: u32, wallet: u32, bags: [u32; 4]) -> u32 {
    if wallet >= amount {
        return wallet - amount;
    }
    let mut money = wallet;
    for i in 0..4 {
        for _ in 0..bags[i] {
            money += SACK_AMOUNTS[i];
            if money >= amount {
                return money - amount;
            }
        }
    }
    0
}

/// Shop-sales cap (`mSP_PlusSales`): the full price is added to
/// `sales_sum`, clamped to the per-shop-level cap (C-side values).
pub fn cap_sales(sales_sum: u32, cap: u32) -> u32 {
    sales_sum.min(cap)
}

// ---- C ABI ----
// All pc_-prefixed: the C originals stay compiled; these are staged kernels.

#[no_mangle]
pub extern "C" fn pc_cl_check_collect_bit(
    bit_data: *const u32,
    words: usize,
    idx: u32,
) -> u8 {
    if bit_data.is_null() {
        return 0;
    }
    let data = unsafe { std::slice::from_raw_parts(bit_data, words) };
    check_collect_bit(data, idx) as u8
}

#[no_mangle]
pub extern "C" fn pc_cl_completed(item_count: i16, list_size: i16) -> u8 {
    completed(item_count, list_size) as u8
}

#[no_mangle]
pub extern "C" fn pc_cl_selected_index(top_idx: i16, y_idx: i16) -> u16 {
    selected_index(top_idx, y_idx)
}

#[no_mangle]
pub extern "C" fn pc_cl_order_slot(price: u32) -> u8 {
    order_slot(price)
}

#[no_mangle]
pub extern "C" fn pc_cl_order_action(item: u16, slot_no: u8) -> u8 {
    order_action(item, slot_no) as u8
}

#[no_mangle]
pub extern "C" fn pc_cl_order_check_msg(chose_yes: u8, funds_ok: u8) -> u8 {
    order_check_msg(chose_yes != 0, funds_ok != 0) as u8
}

#[no_mangle]
pub extern "C" fn pc_cl_money_check(
    amount: u32,
    wallet: u32,
    b0: u32,
    b1: u32,
    b2: u32,
    b3: u32,
) -> u8 {
    money_check(amount, wallet, [b0, b1, b2, b3]) as u8
}

#[no_mangle]
pub extern "C" fn pc_cl_sell_price_wallet(
    amount: u32,
    wallet: u32,
    b0: u32,
    b1: u32,
    b2: u32,
    b3: u32,
) -> u32 {
    sell_price_wallet(amount, wallet, [b0, b1, b2, b3])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constants() {
        assert_eq!(menu::ITEM_MAX, 742);
        assert_eq!(menu::PAGE_SIZE, 7);
        assert_eq!(menu::ITEM_DATA_NUM, 2);
        assert_eq!(category::NUM, 9);
        assert_eq!(category::FTR, 0);
        assert_eq!(category::MUSIC, 8);
        assert_eq!(bitfields::FURNITURE_WORDS, 43);
        assert_eq!(bitfields::MUSIC_WORDS, 2);
    }

    #[test]
    fn collect_bit_boundaries() {
        // word index = idx >> 5, bit = idx & 0x1F
        let mut words = [0u32; 43];
        words[0] = 1 << 0; // idx 0
        words[0] |= 1 << 31; // idx 31
        words[1] = 1 << 0; // idx 32
        words[42] = 1 << 7; // a late word
        assert!(check_collect_bit(&words, 0));
        assert!(check_collect_bit(&words, 31));
        assert!(check_collect_bit(&words, 32));
        assert!(check_collect_bit(&words, 42 * 32 + 7));
        assert!(!check_collect_bit(&words, 1));
        assert!(!check_collect_bit(&words, 33));
        assert!(!check_collect_bit(&words, 42 * 32 + 8));
        // out of range reads as not collected
        assert!(!check_collect_bit(&words[..2], 64));
    }

    #[test]
    fn completion_and_selection() {
        assert!(completed(742, 742));
        assert!(!completed(741, 742));
        assert!(completed(0, 0));
        assert_eq!(selected_index(7, 3), 10);
        assert_eq!(selected_index(0, 0), 0);
    }

    #[test]
    fn order_slot_and_action() {
        // price == 0 ("Not for Sale") -> slot 0 -> unavailable
        assert_eq!(order_slot(0), 0);
        assert_eq!(order_action(0x1234, 0), OrderAction::Unavailable);
        // priced -> slot 1 -> offer
        assert_eq!(order_slot(480), 1);
        assert_eq!(order_action(0x1234, 1), OrderAction::Offer);
        // empty selection -> cancel regardless of slot
        assert_eq!(order_action(0x0000, 1), OrderAction::Cancel);
    }

    #[test]
    fn order_check() {
        assert_eq!(order_check_msg(true, true), OrderMsg::Confirm);
        assert_eq!(order_check_msg(true, false), OrderMsg::InsufficientFunds);
        assert_eq!(order_check_msg(false, true), OrderMsg::Cancel);
    }

    #[test]
    fn money() {
        // wallet covers it
        assert!(money_check(500, 600, [0, 0, 0, 0]));
        // wallet short, bags cover it
        assert!(money_check(500, 100, [4, 0, 0, 0]));
        assert!(!money_check(500, 100, [3, 0, 0, 0]));
        // bags accumulate across denominations
        // (100 wallet + 4 x 100 + 1 x 1000 = 1500; mSP_money_check adds
        // each denomination's total before testing)
        assert!(money_check(1_500, 100, [4, 1, 0, 0]));
        assert!(!money_check(1_500, 100, [0, 0, 0, 0]));
        assert!(money_check(1_500, 100, [0, 2, 0, 0]));
    }

    #[test]
    fn sell_price() {
        // wallet covers: simple debit
        assert_eq!(sell_price_wallet(300, 500, [9, 9, 9, 9]), 200);
        // bags consumed smallest-first; remainder stays in the wallet
        // (no change made into bags — may exceed WALLET_MAX)
        assert_eq!(sell_price_wallet(250, 100, [2, 0, 0, 0]), 50);
        assert_eq!(sell_price_wallet(150, 100, [0, 1, 0, 0]), 950);
        // exact cover
        assert_eq!(sell_price_wallet(200, 100, [1, 0, 0, 0]), 0);
        // unreachable-on-order-path fallthrough
        assert_eq!(sell_price_wallet(10_000, 0, [0, 0, 0, 0]), 0);
    }

    #[test]
    fn sales_cap() {
        assert_eq!(cap_sales(1_000, 500), 500);
        assert_eq!(cap_sales(400, 500), 400);
    }
}
