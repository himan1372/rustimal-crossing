//! Player house progression for the Rust rewrite.
//!
//! Source-verified architecture (upstream `include/m_player.h`,
//! `include/m_home_h.h`, `include/m_private.h`,
//! `include/ac_npc_shop_common.h`,
//! `src/actor/npc/ac_npc_shop_common.c`,
//! `src/actor/ac_intro_demo_move.c_inc`):
//!
//! * Mortgage values (`mPlayer_DEBT0`..`DEBT4`): 17400 (buy house), 148000
//!   (medium), 398000 (large), 49800 (basement), 798000 (upper). The Nook
//!   dialogue side mirrors them as `aNSC_LOAN_MEDIUM/LARGE/UPPER/STATUE(0)/
//!   BASEMENT`.
//! * The mortgage lives in `Private_c.inventory.loan` (`m_private.h:204`);
//!   the intro sets `loan = mPlayer_DEBT0` (17400) directly.
//! * House sizes (`mHm_HOMESIZE_*`): SMALL (initial), MEDIUM (paid off first
//!   debt), LARGE (paid off second debt, excluding basement), UPPER (paid
//!   off third debt & basement), STATUE (paid off final debt). There is no
//!   separate basement size; the basement is a flag.
//! * `home_size_info_s`: `size:3` / `next_size:3` / `statue_rank:2`
//!   (0=gold, 1=silver, 2=bronze, 3=jade), `renew:1`, `statue_ordered:1`,
//!   `basement_ordered:1`, plus the upgrade order date.
//! * Expansion rule (`aNSC_set_talk_info_start_wait`): when construction
//!   finishes (`renew`), the new loan is assigned for the house just built:
//!   basement orders get 49800, otherwise
//!   `rehouse_loan[size-1]` = {148000, 398000, 798000, 0}[size-1].
//!   Accepting the next mortgage does not enlarge the house; the house only
//!   changes after the existing debt reaches zero.
//! * Statue: when `loan == 0` and size is UPPER, Nook offers the statue;
//!   `statue_rank` is the town's statue count capped at 3
//!   (`Save_Get(num_statues)`, "number of statues built for players who
//!   have paid off their debts").
//!
//! Rewrite-owned: the pay/order/complete state-transition API. Storage is
//! per-furniture (3 items per storage unit, per contemporary guides), not a
//! house-wide inventory; that container model is not in this module.

// Public shop/house API for the rewrite and future adapters. The crate
// builds as a staticlib, so unused public items would warn as dead code.
#![allow(dead_code)]

/// Mortgage for the starter house (`mPlayer_DEBT0`).
pub const DEBT_BUY_HOUSE: u32 = 17400;
/// Mortgage for the medium main-floor expansion (`mPlayer_DEBT1`).
pub const DEBT_MEDIUM: u32 = 148000;
/// Mortgage for the large main-floor expansion (`mPlayer_DEBT2`).
pub const DEBT_LARGE: u32 = 398000;
/// Mortgage for the basement (`mPlayer_DEBT3`).
pub const DEBT_BASEMENT: u32 = 49800;
/// Mortgage for the upper floor (`mPlayer_DEBT4`).
pub const DEBT_UPPER: u32 = 798000;

/// Nook-dialogue-side loan aliases (`aNSC_LOAN_*`; same values).
pub const LOAN_MEDIUM: u32 = DEBT_MEDIUM;
pub const LOAN_LARGE: u32 = DEBT_LARGE;
pub const LOAN_UPPER: u32 = DEBT_UPPER;
pub const LOAN_STATUE: u32 = 0;
pub const LOAN_BASEMENT: u32 = DEBT_BASEMENT;

/// House sizes (`mHm_HOMESIZE_*`), in decomp order.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum HouseSize {
    /// Initial size.
    #[default]
    Small = 0,
    /// Paid off first debt.
    Medium = 1,
    /// Paid off second debt (excluding basement).
    Large = 2,
    /// Paid off third debt & basement.
    Upper = 3,
    /// Paid off final debt.
    Statue = 4,
}

impl HouseSize {
    pub fn from_u8(v: u8) -> Option<HouseSize> {
        match v {
            0 => Some(HouseSize::Small),
            1 => Some(HouseSize::Medium),
            2 => Some(HouseSize::Large),
            3 => Some(HouseSize::Upper),
            4 => Some(HouseSize::Statue),
            _ => None,
        }
    }
}

/// Statue ranks: 0=gold, 1=silver, 2=bronze, 3=jade.
pub const STATUE_RANK_GOLD: u8 = 0;
pub const STATUE_RANK_SILVER: u8 = 1;
pub const STATUE_RANK_BRONZE: u8 = 2;
pub const STATUE_RANK_JADE: u8 = 3;
/// Maximum statue rank index (town statue count is capped at 3).
pub const STATUE_RANK_MAX: u8 = 3;

/// Per-player home size state (`home_size_info_s`).
#[derive(Clone, Copy, Debug, Default)]
pub struct HomeSizeInfo {
    /// Current house size.
    pub size: HouseSize,
    /// Next house size when an upgrade is ordered.
    pub next_size: HouseSize,
    /// Statue ranking (gold/silver/bronze/jade).
    pub statue_rank: u8,
    /// Construction finished; refresh size on next Nook visit.
    pub renew: bool,
    /// Statue ordered from Nook.
    pub statue_ordered: bool,
    /// Basement ordered.
    pub basement_ordered: bool,
    /// Date the upgrade was ordered (year, month, day).
    pub upgrade_order_ymd: (u16, u8, u8),
}

/// One player's house: mortgage plus size state.
#[derive(Clone, Copy, Debug)]
pub struct House {
    /// Outstanding mortgage (`Private_c.inventory.loan`).
    pub loan: u32,
    pub size_info: HomeSizeInfo,
}

impl Default for House {
    fn default() -> Self {
        Self {
            loan: DEBT_BUY_HOUSE,
            size_info: HomeSizeInfo::default(),
        }
    }
}

impl House {
    /// Pay `bells` toward the mortgage. Returns the amount actually applied.
    /// The loan saturates at zero; overpayment is not banked.
    pub fn pay(&mut self, bells: u32) -> u32 {
        let applied = bells.min(self.loan);
        self.loan -= applied;
        applied
    }

    /// True when the current debt is fully paid.
    pub fn debt_cleared(&self) -> bool {
        self.loan == 0
    }

    /// Order a main-floor expansion. Records the order date and marks the
    /// house for renewal; the physical house changes only after
    /// construction completes.
    pub fn order_expansion(&mut self, next: HouseSize, ymd: (u16, u8, u8)) -> bool {
        match (self.size_info.size, next) {
            (HouseSize::Small, HouseSize::Medium)
            | (HouseSize::Medium, HouseSize::Large)
            | (HouseSize::Large, HouseSize::Upper) => {
                self.size_info.next_size = next;
                self.size_info.size = next;
                self.size_info.renew = true;
                self.size_info.upgrade_order_ymd = ymd;
                true
            }
            _ => false,
        }
    }

    /// Order the basement. The basement does not change `size`; it is
    /// tracked by `basement_ordered`.
    pub fn order_basement(&mut self, ymd: (u16, u8, u8)) -> bool {
        if self.size_info.size == HouseSize::Large && !self.size_info.basement_ordered {
            self.size_info.basement_ordered = true;
            self.size_info.renew = true;
            self.size_info.upgrade_order_ymd = ymd;
            true
        } else {
            false
        }
    }

    /// Complete construction: assign the mortgage for the expansion just
    /// built. Mirrors the `renew` branch of `aNSC_set_talk_info_start_wait`:
    /// basement orders get `LOAN_BASEMENT`, otherwise
    /// `rehouse_loan[size - 1]`.
    pub fn complete_construction(&mut self) -> bool {
        if !self.size_info.renew {
            return false;
        }
        if self.size_info.basement_ordered {
            self.size_info.basement_ordered = false;
            self.loan = LOAN_BASEMENT;
        } else {
            let idx = self.size_info.size as usize;
            const REHOUSE_LOAN: [u32; 4] = [LOAN_MEDIUM, LOAN_LARGE, LOAN_UPPER, LOAN_STATUE];
            if idx == 0 || idx > REHOUSE_LOAN.len() {
                return false;
            }
            self.loan = REHOUSE_LOAN[idx - 1];
        }
        self.size_info.renew = false;
        true
    }

    /// Offer/order the statue once the upper-floor debt is cleared. Mirrors
    /// the statue branch: `statue_rank` is the town statue count capped at 3.
    pub fn order_statue(&mut self, town_statues: u8, ymd: (u16, u8, u8)) -> bool {
        if self.loan == 0
            && self.size_info.size == HouseSize::Upper
            && !self.size_info.statue_ordered
        {
            self.size_info.statue_ordered = true;
            self.size_info.statue_rank = town_statues.min(STATUE_RANK_MAX);
            self.size_info.upgrade_order_ymd = ymd;
            true
        } else {
            false
        }
    }

    /// Finish the statue build: the house reaches its final state.
    pub fn complete_statue(&mut self) -> bool {
        if self.size_info.statue_ordered {
            self.size_info.statue_ordered = false;
            self.size_info.size = HouseSize::Statue;
            self.size_info.next_size = HouseSize::Statue;
            true
        } else {
            false
        }
    }
}

/// C ABI: mortgage assigned when construction of `size` completes.
/// `basement_ordered` nonzero selects the basement loan. Mirrors the
/// `renew` branch loan assignment. Returns `u32::MAX` for an invalid size.
#[no_mangle]
pub extern "C" fn pc_house_next_loan(size: u8, basement_ordered: i32) -> u32 {
    if basement_ordered != 0 {
        return LOAN_BASEMENT;
    }
    match HouseSize::from_u8(size) {
        Some(HouseSize::Medium) => LOAN_MEDIUM,
        Some(HouseSize::Large) => LOAN_LARGE,
        Some(HouseSize::Upper) => LOAN_UPPER,
        Some(HouseSize::Statue) => LOAN_STATUE,
        _ => u32::MAX,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starter_debt_and_payoff() {
        let mut house = House::default();
        assert_eq!(house.loan, 17400);
        assert_eq!(house.pay(20000), 17400); // overpayment not banked
        assert_eq!(house.loan, 0);
        assert!(house.debt_cleared());
    }

    #[test]
    fn expansion_assigns_next_mortgage_on_completion() {
        let mut house = House::default();
        house.pay(17400);
        assert!(house.order_expansion(HouseSize::Medium, (2026, 10, 7)));
        assert!(house.complete_construction());
        assert_eq!(house.loan, DEBT_MEDIUM);
        assert!(house.order_expansion(HouseSize::Large, (2026, 10, 8)));
        assert!(house.complete_construction());
        assert_eq!(house.loan, DEBT_LARGE);
    }

    #[test]
    fn basement_uses_flag_not_size() {
        let mut house = House::default();
        house.size_info.size = HouseSize::Large;
        assert!(house.order_basement((2026, 10, 7)));
        assert!(house.complete_construction());
        assert_eq!(house.loan, DEBT_BASEMENT);
        assert_eq!(house.size_info.size, HouseSize::Large); // size unchanged
    }

    #[test]
    fn statue_rank_caps_at_jade() {
        let mut house = House::default();
        house.size_info.size = HouseSize::Upper;
        house.pay(DEBT_BUY_HOUSE); // statue requires a paid-off loan
        assert!(house.order_statue(7, (2026, 10, 7)));
        assert_eq!(house.size_info.statue_rank, STATUE_RANK_JADE);
        assert!(house.complete_statue());
        assert_eq!(house.size_info.size, HouseSize::Statue);
        assert_eq!(house.loan, 0);
    }

    #[test]
    fn expansion_order_is_gated() {
        let mut house = House::default();
        // Cannot skip straight to the upper floor.
        assert!(!house.order_expansion(HouseSize::Upper, (2026, 10, 7)));
        // Basement only after the large main floor.
        assert!(!house.order_basement((2026, 10, 7)));
    }
}
