//! Fireworks festival stall vendor (ac_ev_yomise.c) pure kernels.
//!
//! Verified against the ACGC-PC-Port tree (GAFE01_00 Rev 0 USA
//! base). The vendor (SP_NPC_EV_YOMISE / SP_NPC_EV_YOMISE2) runs a
//! function-pointer talk state machine over a persistent 8-entry
//! stock list kept in the fireworks event save area. The stall
//! prop actor itself (ac_yatai.c) is already ported in
//! `holiday_scenery.rs`; only the vendor transaction logic is
//! ported here.
//!
//! Tree-vs-brief notes: the event map has SEVEN pule variants
//! (not three); the map counts are 4 normal NPCs / 7 variants /
//! 7 total (not 5/1/8); the block kind is mRF_BLOCKKIND_POOL
//! (not shrine). The 8 stock IDs per family are contiguous
//! (proven: 0x2244/0x224C/0x2254 base + index).
//!
//! The following stay in C: save-area reservation/persistence,
//! the RANDOM() call, inventory/Bells mutation, the mDemo_*
//! protocol, aNPC_DEMO_GIVE_ITEM, NPC lifecycle, placement and
//! deletion, background offsets, message resources, and the
//! choice-window operations.

#![allow(non_snake_case)]

use std::os::raw::{c_int, c_ushort};

/// Stock entries (`aEYMS_GOODS_COUNT`).
pub const YOMISE_GOODS_COUNT: i32 = 8;
/// Empty stock slot (`EMPTY_NO`).
pub const YOMISE_EMPTY_NO: u16 = 0x0000;
/// Stock family base item IDs (`setUp_yomise_goods` sell_table).
pub const YOMISE_BASE_FAN: u16 = 0x2244; // ITM_BLUEBELL_FAN
pub const YOMISE_BASE_PINWHEEL: u16 = 0x224C; // ITM_YELLOW_PINWHEEL
pub const YOMISE_BASE_BALLOON: u16 = 0x2254; // ITM_RED_BALLOON
/// Prices per stock kind.
pub const YOMISE_PRICE: [i32; 3] = [780, 680, 480];
/// Opening messages per stock kind.
pub const YOMISE_MSG_OPEN: [i32; 3] = [0x1758, 0x1759, 0x175A];
/// No goods remain.
pub const YOMISE_MSG_EMPTY: i32 = 0x1757;
/// Talk actions (`ac_ev_yomise.c`, in order).
pub const YOMISE_TALK_ADD_ACTION: i32 = 0;
pub const YOMISE_TALK_SELECT: i32 = 1;
pub const YOMISE_TALK_SELECT2: i32 = 2;
pub const YOMISE_TALK_FRUIT: i32 = 3;
pub const YOMISE_TALK_END: i32 = 4;
pub const YOMISE_TALK_GIVE: i32 = 5;
/// Continuation messages.
pub const YOMISE_MSG_FULL: i32 = 0x175D; // inventory full
pub const YOMISE_MSG_BROKE: i32 = 0x175E; // insufficient Bells
pub const YOMISE_MSG_BOUGHT: i32 = 0x1764; // purchase done
pub const YOMISE_MSG_DONE: i32 = 0x1765; // handoff done, vendor done
pub const YOMISE_MSG_MORE: i32 = 0x1766; // handoff done, more stock
pub const YOMISE_MSG_PAGE_FEW: i32 = 0x1761; // <4 items in range
pub const YOMISE_MSG_PAGE_MANY: i32 = 0x1762; // >=4 items in range
pub const YOMISE_MSG_SENTINEL_FEW: i32 = 0x1763;
pub const YOMISE_MSG_SENTINEL_MANY: i32 = 0x1767;
/// Sentinel: choice index 3 means "next page / decline".
pub const YOMISE_SENTINEL_CHOICE: i32 = 3;

/// `setUp_yomise_goods` price table: price for a stock kind.
/// Returns -1 for an invalid kind.
#[no_mangle]
pub extern "C" fn pc_yomise_stock_price(kind: c_int) -> c_int {
    if kind < 0 || kind > 2 {
        return -1;
    }
    YOMISE_PRICE[kind as usize]
}

/// Opening message: 0x1757 when no stock remains, otherwise the
/// kind-specific message (0x1758 + kind).
#[no_mangle]
pub extern "C" fn pc_yomise_opening_msg(nonempty_total: c_int, kind: c_int) -> c_int {
    if nonempty_total <= 0 {
        return YOMISE_MSG_EMPTY;
    }
    if kind < 0 || kind > 2 {
        return -1;
    }
    YOMISE_MSG_OPEN[kind as usize]
}

/// `setUp_yomise_goods` (`ac_ev_yomise_move.c_inc`): initialize
/// the 8 stock slots from an injected RNG kind (the C calls
/// `RANDOM(3)`; the RNG itself stays C-side). `goods[j] =
/// base[kind] + j`; returns the kind, or -1 when invalid.
#[no_mangle]
pub extern "C" fn pc_yomise_init_stock(goods: *mut c_ushort, rng_kind: c_int) -> c_int {
    if goods.is_null() || rng_kind < 0 || rng_kind > 2 {
        return -1;
    }
    let base = [YOMISE_BASE_FAN, YOMISE_BASE_PINWHEEL, YOMISE_BASE_BALLOON][rng_kind as usize];
    unsafe {
        for j in 0..YOMISE_GOODS_COUNT as usize {
            *goods.add(j) = base.wrapping_add(j as u16);
        }
    }
    rng_kind
}

/// `aYMS_check_goods_cnt`: number of nonempty slots from
/// `start_idx` to the end of the 8-entry list.
#[no_mangle]
pub extern "C" fn pc_yomise_nonempty_count(goods: *const c_ushort, start_idx: c_int) -> c_int {
    if goods.is_null() {
        return 0;
    }
    let mut cnt = 0;
    let mut i = start_idx.max(0);
    unsafe {
        while i < YOMISE_GOODS_COUNT {
            if *goods.add(i as usize) != YOMISE_EMPTY_NO {
                cnt += 1;
            }
            i += 1;
        }
    }
    cnt
}

/// `get_now_select_goods_idx`: resolve a menu choice to a stock
/// index. Choice 3 is the sentinel and yields -1; otherwise the
/// `buy_idx`-th nonempty slot at/after `cur_choice_start` is
/// returned (-1 when there is none).
#[no_mangle]
pub extern "C" fn pc_yomise_choice_to_stock(
    goods: *const c_ushort,
    cur_choice_start: c_int,
    buy_idx: c_int,
) -> c_int {
    if goods.is_null() {
        return -1;
    }
    if buy_idx == YOMISE_SENTINEL_CHOICE {
        return -1;
    }
    let mut b = buy_idx;
    let mut i = cur_choice_start.max(0);
    unsafe {
        while i < YOMISE_GOODS_COUNT {
            if *goods.add(i as usize) != YOMISE_EMPTY_NO {
                if b == 0 {
                    return i;
                }
                b -= 1;
            }
            i += 1;
        }
    }
    -1
}

/// Next talk action after a purchase: TALK_END when nothing
/// remains, otherwise TALK_ADD_ACTION (return to selection).
#[no_mangle]
pub extern "C" fn pc_yomise_next_after_purchase(remaining: c_int) -> c_int {
    if remaining <= 0 {
        YOMISE_TALK_END
    } else {
        YOMISE_TALK_ADD_ACTION
    }
}

/// `aEYMS_set_choise_data` scan
/// (`ac_ev_yomise_move.c_inc`): walk `goods[start_idx..8]`,
/// filling up to `max` item choices (skipping EMPTY_NO). When
/// `max == 3`, the first unfilled slot takes a fallback literal:
/// `*fallback` = 1 ("I'm not buying! ") if the scan reached the
/// end, 2 ("I don't want it!") otherwise, 0 when every slot got
/// an item. Returns the next scan index `j`; `*filled` receives
/// the item-choice count. (Item-name copying via
/// `mIN_copy_name_str` stays in C.)
#[no_mangle]
pub extern "C" fn pc_yomise_choice_scan(
    goods: *const c_ushort,
    start_idx: c_int,
    max: c_int,
    filled: *mut c_int,
    fallback: *mut c_int,
) -> c_int {
    if goods.is_null() {
        return start_idx;
    }
    let mut i = 0;
    let mut j = start_idx.max(0);
    unsafe {
        while i < max && j < YOMISE_GOODS_COUNT {
            if *goods.add(j as usize) != YOMISE_EMPTY_NO {
                i += 1;
            }
            j += 1;
        }
    }
    let mut fb = 0;
    if max == 3 && i < max {
        // First unfilled slot: the C fills it with a fallback
        // literal and stops.
        fb = if j >= YOMISE_GOODS_COUNT { 1 } else { 2 };
        i += 1;
    }
    unsafe {
        if !filled.is_null() {
            *filled = i;
        }
        if !fallback.is_null() {
            *fallback = fb;
        }
    }
    j
}

/// Sentinel follow-up in `aEYMS_talk_fruit`: when choice 3 was
/// picked, `remaining` (nonempty count from the current range)
/// selects 0x1763 (<4 remain) or 0x1767 (>=4 remain, page
/// advance). Returns the continuation message.
#[no_mangle]
pub extern "C" fn pc_yomise_sentinel_msg(remaining: c_int) -> c_int {
    if remaining < 4 {
        YOMISE_MSG_SENTINEL_FEW
    } else {
        YOMISE_MSG_SENTINEL_MANY
    }
}

/// Page message in `aEYMS_to_talk_buy`: 0x1761 when fewer than 4
/// items remain in range, 0x1762 otherwise.
#[no_mangle]
pub extern "C" fn pc_yomise_page_msg(remaining: c_int) -> c_int {
    if remaining < 4 {
        YOMISE_MSG_PAGE_FEW
    } else {
        YOMISE_MSG_PAGE_MANY
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn full_stock(kind: i32) -> [u16; 8] {
        let mut g = [0u16; 8];
        assert_eq!(pc_yomise_init_stock(g.as_mut_ptr(), kind), kind);
        g
    }

    #[test]
    fn price_and_opening_table() {
        assert_eq!((pc_yomise_stock_price(0), pc_yomise_opening_msg(8, 0)), (780, 0x1758));
        assert_eq!((pc_yomise_stock_price(1), pc_yomise_opening_msg(8, 1)), (680, 0x1759));
        assert_eq!((pc_yomise_stock_price(2), pc_yomise_opening_msg(8, 2)), (480, 0x175A));
        assert_eq!(pc_yomise_opening_msg(0, 1), 0x1757);
        assert_eq!(pc_yomise_stock_price(3), -1);
    }

    #[test]
    fn stock_init_families() {
        // 8 contiguous IDs per family, proven in the decomp.
        assert_eq!(full_stock(0), [0x2244, 0x2245, 0x2246, 0x2247, 0x2248, 0x2249, 0x224A, 0x224B]);
        assert_eq!(full_stock(1)[0], 0x224C);
        assert_eq!(full_stock(1)[7], 0x2253);
        assert_eq!(full_stock(2)[0], 0x2254);
        assert_eq!(full_stock(2)[7], 0x225B);
        assert_eq!(pc_yomise_init_stock(std::ptr::null_mut(), 0), -1);
        assert_eq!(pc_yomise_init_stock([0u16; 8].as_mut_ptr(), 5), -1);
    }

    #[test]
    fn nonempty_count_and_resolution() {
        let mut g = full_stock(0);
        assert_eq!(pc_yomise_nonempty_count(g.as_ptr(), 0), 8);
        assert_eq!(pc_yomise_nonempty_count(g.as_ptr(), 4), 4);
        // sparse: sell slots 1 and 3
        g[1] = YOMISE_EMPTY_NO;
        g[3] = YOMISE_EMPTY_NO;
        assert_eq!(pc_yomise_nonempty_count(g.as_ptr(), 0), 6);
        // choice 0 -> stock 0, choice 1 -> stock 2 (skips sold 1)
        assert_eq!(pc_yomise_choice_to_stock(g.as_ptr(), 0, 0), 0);
        assert_eq!(pc_yomise_choice_to_stock(g.as_ptr(), 0, 1), 2);
        // sentinel
        assert_eq!(pc_yomise_choice_to_stock(g.as_ptr(), 0, 3), -1);
        // start at 4: choice 0 -> stock 4
        assert_eq!(pc_yomise_choice_to_stock(g.as_ptr(), 4, 0), 4);
        // exhausted range
        let e = [YOMISE_EMPTY_NO; 8];
        assert_eq!(pc_yomise_choice_to_stock(e.as_ptr(), 0, 0), -1);
    }

    #[test]
    fn next_action_and_messages() {
        assert_eq!(pc_yomise_next_after_purchase(0), YOMISE_TALK_END);
        assert_eq!(pc_yomise_next_after_purchase(3), YOMISE_TALK_ADD_ACTION);
        assert_eq!(pc_yomise_sentinel_msg(2), 0x1763);
        assert_eq!(pc_yomise_sentinel_msg(5), 0x1767);
        assert_eq!(pc_yomise_page_msg(2), 0x1761);
        assert_eq!(pc_yomise_page_msg(4), 0x1762);
    }

    #[test]
    fn choice_scan_pages() {
        let g = full_stock(2);
        let (mut filled, mut fb) = (0, 0);
        // full page of 3 from 0: next start 3, no fallback
        assert_eq!(pc_yomise_choice_scan(g.as_ptr(), 0, 3, &mut filled, &mut fb), 3);
        assert_eq!((filled, fb), (3, 0));
        // last page: start 6, two items left -> fallback "I'm not buying! " (end reached)
        assert_eq!(pc_yomise_choice_scan(g.as_ptr(), 6, 3, &mut filled, &mut fb), 8);
        assert_eq!((filled, fb), (3, 1));
        // sparse: only slot 7 left from 4 -> "I don't want it!" (j < 8 impossible here since scan ends at 8;
        // use a gap: sell 4..6, one item at 7)
        let mut s = [YOMISE_EMPTY_NO; 8];
        s[7] = 0x225B;
        assert_eq!(pc_yomise_choice_scan(s.as_ptr(), 4, 3, &mut filled, &mut fb), 8);
        assert_eq!((filled, fb), (2, 1));
        // max 4 path (select2): no fallback logic
        assert_eq!(pc_yomise_choice_scan(g.as_ptr(), 0, 4, &mut filled, &mut fb), 4);
        assert_eq!((filled, fb), (4, 0));
    }
}
