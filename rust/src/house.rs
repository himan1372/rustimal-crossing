//! Player house progression for the Rust rewrite.
//!
//! Source-verified architecture (`include/m_player.h`,
//! `include/m_home_h.h`, `src/game/m_home.c` (mHm_CheckRehouseOrder),
//! `src/actor/npc/ac_npc_shop_common.c` (Nook order/dialogue/renewal),
//! `src/game/m_repay_ovl.c`):
//!
//! * Mortgage values (`mPlayer_DEBT0`..`DEBT4`): 17400 (buy house), 148000
//!   (medium), 398000 (large), 49800 (basement), 798000 (upper).
//! * The mortgage lives in `Private_c.inventory.loan`; the intro sets
//!   `loan = mPlayer_DEBT0` (17400) directly.
//! * House sizes (`mHm_HOMESIZE_*`): SMALL/MEDIUM/LARGE/UPPER/STATUE.
//!   Basement is a flag (`flags.has_basement`), not a size.
//! * Three distinct phases (do NOT conflate):
//!   Phase A (order): Nook accepts -> next_size += 1 (or
//!     basement_ordered = TRUE), ordered palette, upgrade_order_date.
//!     size and renew are UNTOUCHED.
//!   Phase B (construction): mHm_CheckRehouseOrder on a later calendar
//!     date -> size = next_size (or has_basement = TRUE, or
//!     next_size = STATUE), renew = TRUE, physical room rebuilt.
//!   Phase C (Nook renewal): aNSC_set_talk_info_start_wait sees renew ->
//!     assigns next loan (basement: 49800 + pad_1 = 1; else
//!     rehouse_loan[size-1]), clears renew.
//! * Basement can be ordered from MEDIUM or LARGE (not just LARGE).
//! * pad_1 is a basement-completion progression flag (set when the
//!   basement loan is assigned; gates the UPPER offer).
//! * Statue: ordered when loan==0 && size==UPPER && next_size==UPPER;
//!   next_size becomes STATUE on a later date; Nook then clears
//!   statue_ordered. statue_rank = town statue count capped at 3.

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

/// Ordinary upgrade loans indexed by completed size-1:
/// MEDIUM->148k, LARGE->398k, UPPER->798k, STATUE->0.
pub const REHOUSE_LOAN: [u32; 4] = [LOAN_MEDIUM, LOAN_LARGE, LOAN_UPPER, LOAN_STATUE];

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

/// Per-player home size state (`home_size_info_s` + related flags).
#[derive(Clone, Copy, Debug, Default)]
pub struct HomeSizeInfo {
    /// Current house size.
    pub size: HouseSize,
    /// Next house size when an upgrade is ordered.
    pub next_size: HouseSize,
    /// Statue ranking (gold/silver/bronze/jade).
    pub statue_rank: u8,
    /// Construction finished; Nook must assign the next loan.
    pub renew: bool,
    /// Statue ordered from Nook.
    pub statue_ordered: bool,
    /// Basement ordered.
    pub basement_ordered: bool,
    /// Retail `pad_1`: basement-completion progression flag.
    pub basement_completion_marker: bool,
    /// Basement physically exists (`flags.has_basement`).
    pub has_basement: bool,
    /// Date the upgrade was ordered (year, month, day).
    pub upgrade_order_ymd: (u16, u8, u8),
    /// Chosen roof palette (ordered_outlook_pal).
    pub ordered_outlook_pal: u8,
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

/// CHECK_ORDER_DATE: true when the calendar date differs from the order
/// date (any component).
pub fn order_date_passed(order: (u16, u8, u8), today: (u16, u8, u8)) -> bool {
    order.0 != today.0 || order.1 != today.1 || order.2 != today.2
}

impl House {
    /// Pay `bells` toward the mortgage. Returns the amount actually applied.
    pub fn pay(&mut self, bells: u32) -> u32 {
        let applied = bells.min(self.loan);
        self.loan -= applied;
        applied
    }

    /// True when the current debt is fully paid.
    pub fn debt_cleared(&self) -> bool {
        self.loan == 0
    }

    /// Phase A: order a main-floor expansion. Only records next_size
    /// (size+1), the palette, and the order date. size and renew are
    /// untouched -- construction happens in check_rehouse_order.
    /// Requires the current loan to be cleared.
    pub fn order_expansion(&mut self, palette: u8, ymd: (u16, u8, u8)) -> bool {
        if !self.debt_cleared() {
            return false;
        }
        let next = match self.size_info.size {
            HouseSize::Small => HouseSize::Medium,
            HouseSize::Medium => HouseSize::Large,
            HouseSize::Large => HouseSize::Upper,
            _ => return false,
        };
        // Retail does next_size += 1; the enum order makes this identical.
        self.size_info.next_size = next;
        self.size_info.ordered_outlook_pal = palette;
        self.size_info.upgrade_order_ymd = ymd;
        true
    }

    /// Phase A: order the basement. Allowed from MEDIUM or LARGE when no
    /// basement exists or is ordered. Does not change size.
    pub fn order_basement(&mut self, ymd: (u16, u8, u8)) -> bool {
        match self.size_info.size {
            HouseSize::Medium | HouseSize::Large => {}
            _ => return false,
        }
        if self.size_info.has_basement || self.size_info.basement_ordered {
            return false;
        }
        self.size_info.basement_ordered = true;
        self.size_info.upgrade_order_ymd = ymd;
        true
    }

    /// Phase B: mHm_CheckRehouseOrder. On a later calendar date than the
    /// order, completes construction: size = next_size (main), or
    /// has_basement = TRUE (basement), or next_size = STATUE (statue).
    /// Sets renew for the main/basement paths.
    pub fn check_rehouse_order(&mut self, today: (u16, u8, u8)) -> bool {
        if !order_date_passed(self.size_info.upgrade_order_ymd, today) {
            return false;
        }
        let si = &mut self.size_info;
        if si.size != si.next_size && (si.next_size as u8) < HouseSize::Statue as u8 {
            si.size = si.next_size;
            si.renew = true;
            true
        } else if si.basement_ordered {
            si.has_basement = true;
            si.renew = true;
            true
        } else if si.statue_ordered {
            si.next_size = HouseSize::Statue;
            true
        } else {
            false
        }
    }

    /// Phase C: Nook's renewal processing (aNSC_set_talk_info_start_wait).
    /// Assigns the next loan, clears basement_ordered/renew, sets pad_1
    /// for the basement path. Returns the assigned loan, or None.
    pub fn nook_process_renewal(&mut self) -> Option<u32> {
        let si = &mut self.size_info;
        if !si.renew {
            return None;
        }
        si.basement_completion_marker = false; // pad_1 = 0 first
        let loan = if si.basement_ordered {
            si.basement_ordered = false;
            si.basement_completion_marker = true; // pad_1 = 1
            LOAN_BASEMENT
        } else {
            let idx = si.size as usize;
            if idx == 0 || idx > REHOUSE_LOAN.len() {
                return None;
            }
            REHOUSE_LOAN[idx - 1]
        };
        self.loan = loan;
        si.renew = false;
        Some(loan)
    }

    /// Phase A (statue): order the statue. Requires loan==0, size==UPPER,
    /// next_size==UPPER, not already ordered. Rank = town count capped at 3.
    pub fn order_statue(&mut self, town_statues: u8, ymd: (u16, u8, u8)) -> bool {
        let si = &self.size_info;
        if self.loan != 0
            || si.size != HouseSize::Upper
            || si.next_size != HouseSize::Upper
            || si.statue_ordered
        {
            return false;
        }
        self.size_info.statue_ordered = true;
        self.size_info.statue_rank = town_statues.min(STATUE_RANK_MAX);
        self.size_info.upgrade_order_ymd = ymd;
        true
    }

    /// Phase C (statue): Nook sees statue_ordered && next_size == STATUE
    /// (set by check_rehouse_order on a later date) and marks it built.
    pub fn nook_process_statue_built(&mut self) -> bool {
        let si = &mut self.size_info;
        if si.statue_ordered && si.next_size == HouseSize::Statue {
            si.statue_ordered = false;
            true
        } else {
            false
        }
    }

    /// True once the statue pipeline has fully completed.
    pub fn statue_built(&self) -> bool {
        !self.size_info.statue_ordered && self.size_info.next_size == HouseSize::Statue
    }
}

/// C ABI: mortgage assigned when construction of `size` completes.
/// `basement_ordered` nonzero selects the basement loan. Returns
/// `u32::MAX` for an invalid size.
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

/// C ABI: 1 if the order date has passed (any calendar component differs).
#[no_mangle]
pub extern "C" fn pc_order_date_passed(
    oy: u16,
    om: u8,
    od: u8,
    ty: u16,
    tm: u8,
    td: u8,
) -> u8 {
    order_date_passed((oy, om, od), (ty, tm, td)) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starter_debt_and_payoff() {
        let mut house = House::default();
        assert_eq!(house.loan, 17400);
        assert_eq!(house.pay(20000), 17400);
        assert_eq!(house.loan, 0);
        assert!(house.debt_cleared());
    }

    #[test]
    fn three_phase_expansion() {
        let mut house = House::default();
        house.pay(17400);
        // Phase A: order records next_size/date only.
        assert!(house.order_expansion(3, (2026, 10, 7)));
        assert_eq!(house.size_info.next_size, HouseSize::Medium);
        assert_eq!(house.size_info.size, HouseSize::Small); // NOT changed
        assert!(!house.size_info.renew); // NOT set
        // Same date: no construction.
        assert!(!house.check_rehouse_order((2026, 10, 7)));
        // Next date: Phase B builds.
        assert!(house.check_rehouse_order((2026, 10, 8)));
        assert_eq!(house.size_info.size, HouseSize::Medium);
        assert!(house.size_info.renew);
        // Phase C: Nook assigns the loan.
        assert_eq!(house.nook_process_renewal(), Some(DEBT_MEDIUM));
        assert_eq!(house.loan, DEBT_MEDIUM);
        assert!(!house.size_info.renew);
    }

    #[test]
    fn basement_from_medium() {
        let mut house = House::default();
        house.size_info.size = HouseSize::Medium;
        house.size_info.next_size = HouseSize::Medium; // no expansion pending
        // Basement allowed from MEDIUM (retail correction).
        assert!(house.order_basement((2026, 10, 7)));
        assert!(!house.size_info.has_basement); // not yet built
        assert!(house.check_rehouse_order((2026, 10, 8)));
        assert!(house.size_info.has_basement);
        assert!(house.size_info.renew);
        assert_eq!(house.nook_process_renewal(), Some(DEBT_BASEMENT));
        assert_eq!(house.loan, DEBT_BASEMENT);
        assert!(house.size_info.basement_completion_marker); // pad_1
        assert_eq!(house.size_info.size, HouseSize::Medium); // size unchanged
    }

    #[test]
    fn basement_from_large() {
        let mut house = House::default();
        house.size_info.size = HouseSize::Large;
        assert!(house.order_basement((2026, 10, 7)));
        assert!(house.check_rehouse_order((2026, 10, 8)));
        assert_eq!(house.nook_process_renewal(), Some(DEBT_BASEMENT));
    }

    #[test]
    fn statue_pipeline() {
        let mut house = House::default();
        house.size_info.size = HouseSize::Upper;
        house.size_info.next_size = HouseSize::Upper;
        house.pay(DEBT_BUY_HOUSE);
        assert!(house.order_statue(7, (2026, 10, 7)));
        assert_eq!(house.size_info.statue_rank, STATUE_RANK_JADE);
        // Later date: next_size becomes STATUE.
        assert!(house.check_rehouse_order((2026, 10, 8)));
        assert_eq!(house.size_info.next_size, HouseSize::Statue);
        // Nook marks it built.
        assert!(house.nook_process_statue_built());
        assert!(house.statue_built());
        assert_eq!(house.loan, 0);
    }

    #[test]
    fn ordering_is_gated() {
        let mut house = House::default();
        // Cannot order with debt outstanding.
        assert!(!house.order_expansion(0, (2026, 10, 7)));
        house.pay(17400);
        assert!(house.order_expansion(0, (2026, 10, 7)));
        // Small -> Medium only; no skipping.
        let mut h2 = House::default();
        h2.pay(17400);
        h2.size_info.size = HouseSize::Medium;
        h2.size_info.next_size = HouseSize::Medium;
        assert!(h2.order_expansion(0, (2026, 10, 7)));
        assert_eq!(h2.size_info.next_size, HouseSize::Large);
        // Basement not from Small.
        let mut h3 = House::default();
        assert!(!h3.order_basement((2026, 10, 7)));
        // No double basement.
        let mut h4 = House::default();
        h4.size_info.size = HouseSize::Medium;
        assert!(h4.order_basement((2026, 10, 7)));
        assert!(!h4.order_basement((2026, 10, 7)));
    }
}
