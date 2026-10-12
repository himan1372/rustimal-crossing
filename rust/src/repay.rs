//! Mortgage repayment overlay (`m_repay_ovl.c`, `m_repay_ovl.h`).
//!
//! Verified against GAFE01_00 Rev 0 USA (`src/game/m_repay_ovl.c`,
//! `include/m_repay_ovl.h`, `include/m_name_table.h`, `include/m_private.h`).
//!
//! Pure kernels: the displayed-funds init, the repayment-amount digit
//! adjustment with its clamps, the preview fields, and the
//! `mRP_money_repay` wallet/pocket consumption routine. The submenu flow,
//! sounds, drawing, and the `mPlib_request_main_give_from_submenu` call stay
//! in C.
//!
//! Retail quirk preserved: the payment is NOT made by consolidating pocket
//! money into the wallet first. If the wallet alone covers the amount, only
//! the wallet is touched; otherwise pocket money items are consumed in the
//! declared denomination order (100, 1000, 10000, 30000), scanning every
//! pocket per denomination, and the running total minus the amount becomes
//! the new wallet.

/// Money item ids (`m_name_table.h`).
pub const ITM_MONEY_1000: u16 = 0x2100;
pub const ITM_MONEY_10000: u16 = 0x2101;
pub const ITM_MONEY_30000: u16 = 0x2102;
pub const ITM_MONEY_100: u16 = 0x2103;

/// Denomination scan order and values (`mRP_money_name`, `mRP_money_amount`).
pub const MONEY_ORDER: [(u16, u32); 4] = [
    (ITM_MONEY_100, 100),
    (ITM_MONEY_1000, 1000),
    (ITM_MONEY_10000, 10000),
    (ITM_MONEY_30000, 30000),
];

/// Cursor positions (`mRP_CURSOR_*`).
pub const CURSOR_100000: i32 = 0;
pub const CURSOR_10000: i32 = 1;
pub const CURSOR_1000: i32 = 2;
pub const CURSOR_100: i32 = 3;
pub const CURSOR_10: i32 = 4;
pub const CURSOR_1: i32 = 5;
pub const CURSOR_OK: i32 = 6;

/// Per-cursor digit step (`step_money`).
pub const CURSOR_STEPS: [u32; 7] = [100000, 10000, 1000, 100, 10, 1, 0];

/// Normal item condition (`mPr_ITEM_COND_NORMAL`).
pub const ITEM_COND_NORMAL: u8 = 0;

/// Pocket slot count.
pub const POCKETS_SLOT_COUNT: usize = 15;

/// `mRP_Ovl_c` layout (24 bytes retail): money, loan, repay_amount,
/// max_money, max_repay, cursor_idx.
#[repr(C)]
pub struct RepayOvl {
    pub money: u32,
    pub loan: u32,
    pub repay_amount: u32,
    pub max_money: u32,
    pub max_repay: u32,
    pub cursor_idx: i32,
}

/// Displayed funds at overlay open: wallet plus eligible pocket money items
/// (`mRP_repay_ovl_init`). Pockets are parallel `items`/`conds` arrays of `n`
/// slots.
#[no_mangle]
pub extern "C" fn pc_repay_init_money(
    wallet: u32,
    items: *const u16,
    conds: *const u8,
    n: usize,
) -> u32 {
    let mut money = wallet;
    if items.is_null() || conds.is_null() {
        return money;
    }
    for i in 0..n {
        let (item, cond) = unsafe { (*items.add(i), *conds.add(i)) };
        if cond == ITEM_COND_NORMAL {
            for &(id, amount) in &MONEY_ORDER {
                if item == id {
                    money += amount;
                    break;
                }
            }
        }
    }
    money
}

/// `max_repay = min(max_money, loan)` (`mRP_repay_ovl_init`).
#[no_mangle]
pub extern "C" fn pc_repay_max_repay(max_money: u32, loan: u32) -> u32 {
    max_money.min(loan)
}

/// Digit adjustment (`mRP_move_Play`). Decreasing below the step resets the
/// amount to zero (unless it is already zero, which is rejected); increasing
/// clamps at `max_repay`. Returns `(new_amount, rejected)`.
#[no_mangle]
pub extern "C" fn pc_repay_adjust(
    repay_amount: u32,
    max_repay: u32,
    cursor_idx: i32,
    increase: bool,
    out_rejected: *mut bool,
) -> u32 {
    let step = CURSOR_STEPS[(cursor_idx.clamp(0, 6)) as usize];
    let (new_amount, rejected) = if increase {
        if repay_amount == max_repay {
            (repay_amount, true)
        } else {
            (repay_amount.saturating_add(step).min(max_repay), false)
        }
    } else if repay_amount < step {
        if repay_amount == 0 {
            (0, true)
        } else {
            (0, false)
        }
    } else {
        (repay_amount - step, false)
    };
    if !out_rejected.is_null() {
        unsafe {
            *out_rejected = rejected;
        }
    }
    new_amount
}

/// Preview fields after an amount change: `money = max_money - repay_amount`,
/// `loan = saved_loan - repay_amount`.
#[no_mangle]
pub extern "C" fn pc_repay_preview(
    max_money: u32,
    saved_loan: u32,
    repay_amount: u32,
    out_money: *mut u32,
    out_loan: *mut u32,
) {
    if !out_money.is_null() {
        unsafe {
            *out_money = max_money - repay_amount;
        }
    }
    if !out_loan.is_null() {
        unsafe {
            *out_loan = saved_loan - repay_amount;
        }
    }
}

/// `mRP_money_repay`: commit the payment and return the new wallet balance.
/// `pockets` holds `(item_id, condition)` pairs and is mutated in place:
/// consumed money items are cleared (item 0, condition normal). If the wallet
/// alone covers `repay_amount`, pockets are untouched.
#[no_mangle]
pub extern "C" fn pc_repay_commit(
    wallet: u32,
    repay_amount: u32,
    items: *mut u16,
    conds: *mut u8,
    n: usize,
) -> u32 {
    if wallet >= repay_amount {
        return wallet - repay_amount;
    }
    if items.is_null() || conds.is_null() {
        return 0;
    }
    let mut total = wallet;
    for &(id, amount) in &MONEY_ORDER {
        for i in 0..n {
            let (item, cond) = unsafe { (*items.add(i), *conds.add(i)) };
            if cond == ITEM_COND_NORMAL && item == id {
                total += amount;
                unsafe {
                    *items.add(i) = 0;
                    *conds.add(i) = ITEM_COND_NORMAL;
                }
                if total >= repay_amount {
                    return total - repay_amount;
                }
            }
        }
    }
    // Unreachable when repay_amount <= max_repay <= max_money, but the C
    // falls through to `money_left` (0) in that case; mirror it.
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Pockets {
        items: [u16; POCKETS_SLOT_COUNT],
        conds: [u8; POCKETS_SLOT_COUNT],
    }
    impl Pockets {
        fn new(pairs: &[(u16, u8)]) -> Self {
            let mut p = Pockets {
                items: [0; POCKETS_SLOT_COUNT],
                conds: [0; POCKETS_SLOT_COUNT],
            };
            for (i, &(item, cond)) in pairs.iter().enumerate() {
                p.items[i] = item;
                p.conds[i] = cond;
            }
            p
        }
        fn ptrs(&mut self) -> (*mut u16, *mut u8) {
            (self.items.as_mut_ptr(), self.conds.as_mut_ptr())
        }
    }

    #[test]
    fn init_money() {
        // Wallet plus normal-cond money items only.
        let mut p = Pockets::new(&[
            (ITM_MONEY_1000, ITEM_COND_NORMAL),
            (ITM_MONEY_100, ITEM_COND_NORMAL),
            (ITM_MONEY_10000, 1), // present-cond: ignored
            (0x2000, ITEM_COND_NORMAL), // not money: ignored
        ]);
        let (items, conds) = p.ptrs();
        assert_eq!(
            pc_repay_init_money(500, items, conds, POCKETS_SLOT_COUNT),
            500 + 1000 + 100
        );
        assert_eq!(pc_repay_max_repay(1600, 5000), 1600);
        assert_eq!(pc_repay_max_repay(6000, 5000), 5000);
        assert_eq!(CURSOR_STEPS[CURSOR_100000 as usize], 100000);
        assert_eq!(CURSOR_STEPS[CURSOR_OK as usize], 0);
    }

    #[test]
    fn adjust() {
        let mut rej = false;
        // Increase clamps at max_repay; at max_repay it is rejected.
        assert_eq!(pc_repay_adjust(900, 1000, CURSOR_100, true, &mut rej), 1000);
        assert!(!rej);
        assert_eq!(pc_repay_adjust(1000, 1000, CURSOR_100, true, &mut rej), 1000);
        assert!(rej);
        // Decrease below the step resets to zero; zero decrease is rejected.
        assert_eq!(pc_repay_adjust(50, 1000, CURSOR_100, false, &mut rej), 0);
        assert!(!rej);
        assert_eq!(pc_repay_adjust(0, 1000, CURSOR_100, false, &mut rej), 0);
        assert!(rej);
        assert_eq!(pc_repay_adjust(350, 1000, CURSOR_100, false, &mut rej), 250);
        // Null out-pointer is safe.
        assert_eq!(
            pc_repay_adjust(350, 1000, CURSOR_100, false, std::ptr::null_mut()),
            250
        );
    }

    #[test]
    fn preview() {
        let mut money = 0;
        let mut loan = 0;
        pc_repay_preview(1600, 5000, 600, &mut money, &mut loan);
        assert_eq!((money, loan), (1000, 4400));
    }

    #[test]
    fn commit() {
        // Wallet covers it: pockets untouched.
        let mut p = Pockets::new(&[(ITM_MONEY_1000, ITEM_COND_NORMAL)]);
        let (items, conds) = p.ptrs();
        assert_eq!(pc_repay_commit(800, 500, items, conds, POCKETS_SLOT_COUNT), 300);
        assert_eq!((p.items[0], p.conds[0]), (ITM_MONEY_1000, ITEM_COND_NORMAL));
        // Wallet short: consumes 100s first (denomination order), then 1000s.
        let mut p = Pockets::new(&[
            (ITM_MONEY_1000, ITEM_COND_NORMAL),
            (ITM_MONEY_100, ITEM_COND_NORMAL),
            (ITM_MONEY_100, ITEM_COND_NORMAL),
        ]);
        let (items, conds) = p.ptrs();
        assert_eq!(pc_repay_commit(50, 250, items, conds, POCKETS_SLOT_COUNT), 0);
        assert_eq!((p.items[0], p.conds[0]), (ITM_MONEY_1000, ITEM_COND_NORMAL)); // untouched
        assert_eq!((p.items[1], p.conds[1]), (0, ITEM_COND_NORMAL)); // consumed
        assert_eq!((p.items[2], p.conds[2]), (0, ITEM_COND_NORMAL)); // consumed
        // Excess becomes the wallet balance.
        let mut p = Pockets::new(&[(ITM_MONEY_1000, ITEM_COND_NORMAL)]);
        let (items, conds) = p.ptrs();
        assert_eq!(pc_repay_commit(50, 900, items, conds, POCKETS_SLOT_COUNT), 150);
        assert_eq!((p.items[0], p.conds[0]), (0, ITEM_COND_NORMAL));
        // Present-condition money is not spendable.
        let mut p = Pockets::new(&[(ITM_MONEY_1000, 1)]);
        let (items, conds) = p.ptrs();
        assert_eq!(pc_repay_commit(50, 900, items, conds, POCKETS_SLOT_COUNT), 0);
        assert_eq!((p.items[0], p.conds[0]), (ITM_MONEY_1000, 1));
    }
}
