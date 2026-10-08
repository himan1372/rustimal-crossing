//! Tom Nook's shop for the Rust rewrite.
//!
//! Source-verified architecture (upstream `include/m_shop.h`,
//! `src/game/m_shop.c`, `src/actor/npc/ac_npc_shop_common.c`,
//! `src/actor/ac_shop_design.c`):
//!
//! * Shop tiers (`mSP_SHOP_TYPE_*`): ZAKKA (Nook's Cranny), COMBINI
//!   (Nook 'n' Go), SUPER (Nookway), DSUPER (Nookington's).
//! * Upgrade thresholds are cumulative sales sums (`m_shop.h`):
//!   `mSP_COMBINI_SUM` 25000, `mSP_SUPER_SUM` 90000,
//!   `mSP_DSUPER_SUM` 240000.
//! * The town owns one counter: `Shop_c.sales_sum` ("current money towards
//!   upgrading shop", u32 at save offset 0x128). `mSP_PlusSales` adds to it
//!   and clamps it to the current tier's threshold (excess before a remodel
//!   is discarded).
//! * Transaction accounting (verified call sites): selling calls
//!   `mSP_PlusSales(money / 2)` (`ac_npc_shop_common.c:2220`) — sales add
//!   half of Nook's payout; catalog orders call `mSP_PlusSales(price)`
//!   (`ac_npc_shop_common.c:2358`) — full price. Catalog/furniture orders
//!   (`ac_shop_design.c:368`) also add the full price.
//! * `mSP_GetRealShopLevel` derives the deserved tier from the counter;
//!   Nookington's additionally needs `visitor_flag` ("set when a foreign
//!   player enters Nook's shop"). The PC port has a
//!   `disable_shop_visitor_req` toggle for it. `mSP_RenewShopLevel` syncs
//!   the saved (displayed) level to the real one; `shop_info.upgrading_today`
//!   marks the remodeling day.
//! * Tool eligibility (`mSP_SelectTool`): the shovel is always available;
//!   net at `mSP_NET_SALES_SUM` 3000, rod at `mSP_ROD_SALES_SUM` 8000, axe at
//!   `mSP_AXE_SALES_SUM` 12000 — but the lockout applies only in Nook's
//!   Cranny; higher tiers unlock all four tools.
//! * Persistent stock: `Shop_c.items[mSP_GOODS_COUNT]` (39 slots),
//!   `rare_item` (spotlight), `lottery_items[3]`, plus `shop_info`
//!   bitfields (shop_level:2, upgrading_today, send_upgrade_notice,
//!   not_loaded_before, paint_color:4), `exchange_time`, `renewal_time`.
//!
//! Guide-derived (not decomp-traced here): the per-tier category slot
//! counts (furniture/clothing/tool/etc. capacities). They are labeled as
//! such below.

// Public shop/house API for the rewrite and future adapters. The crate
// builds as a staticlib, so unused public items would warn as dead code.
#![allow(dead_code)]

/// Standard shop inventory slots (`mSP_GOODS_COUNT`).
pub const GOODS_COUNT: usize = 39;
/// Lottery item slots.
pub const LOTTERY_ITEM_COUNT: usize = 3;

/// Sales sum before the net becomes eligible, Nook's Cranny only.
pub const NET_SALES_SUM: u32 = 3000;
/// Sales sum before the rod becomes eligible, Nook's Cranny only.
pub const ROD_SALES_SUM: u32 = 8000;
/// Sales sum before the axe becomes eligible, Nook's Cranny only.
pub const AXE_SALES_SUM: u32 = 12000;
/// Cumulative sales sum for Nook 'n' Go.
pub const COMBINI_SUM: u32 = 25000;
/// Cumulative sales sum for Nookway.
pub const SUPER_SUM: u32 = 90000;
/// Cumulative sales sum for Nookington's.
pub const DSUPER_SUM: u32 = 240000;

/// Shop tiers (`mSP_SHOP_TYPE_*`), in decomp order.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, PartialOrd, Ord)]
pub enum ShopTier {
    /// Nook's Cranny.
    #[default]
    Zakka = 0,
    /// Nook 'n' Go.
    Combini = 1,
    /// Nookway.
    Super = 2,
    /// Nookington's.
    Dsuper = 3,
}

impl ShopTier {
    pub fn from_u8(v: u8) -> Option<ShopTier> {
        match v {
            0 => Some(ShopTier::Zakka),
            1 => Some(ShopTier::Combini),
            2 => Some(ShopTier::Super),
            3 => Some(ShopTier::Dsuper),
            _ => None,
        }
    }

    /// Upgrade threshold that leaves this tier (`mSP_PlusSales` clamp).
    pub fn sales_cap(self) -> Option<u32> {
        match self {
            ShopTier::Zakka => Some(COMBINI_SUM),
            ShopTier::Combini => Some(SUPER_SUM),
            ShopTier::Super => Some(DSUPER_SUM),
            ShopTier::Dsuper => None,
        }
    }

    /// Numeric tier index for level comparisons.
    pub fn tier_idx(self) -> u8 {
        self as u8
    }
}

/// Days in month (non-leap; retail uses lbRTC_GetDaysByMonth).
fn days_in_month(y: u16, m: u8) -> u8 {
    match m {
        2 => {
            if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) {
                29
            } else {
                28
            }
        }
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// Add n days to a (y,m,d) date.
fn add_days((y, m, d): (u16, u8, u8), n: u8) -> (u16, u8, u8) {
    let (mut y, mut m, mut d) = (y, m, d);
    for _ in 0..n {
        d += 1;
        if d > days_in_month(y, m) {
            d = 1;
            m += 1;
            if m > 12 {
                m = 1;
                y += 1;
            }
        }
    }
    (y, m, d)
}

/// True when a >= b as calendar dates.
fn ymd_ge(a: (u16, u8, u8), b: (u16, u8, u8)) -> bool {
    a.0 > b.0 || (a.0 == b.0 && (a.1 > b.1 || (a.1 == b.1 && a.2 >= b.2)))
}

/// Town-owned Nook shop state (mirrors the persistent `Shop_c` fields).
#[derive(Clone, Debug)]
pub struct ShopState {
    /// Current money towards upgrading the shop (`sales_sum`).
    pub sales_sum: u32,
    /// Displayed shop tier (saved `shop_info.shop_level`).
    pub shop_level: ShopTier,
    /// Set when a foreign-town player shops (Nookington's gate).
    pub visitor_flag: bool,
    /// The shop is undergoing renovations today.
    pub upgrading_today: bool,
    /// Retail: Nook has been notified an upgrade is pending.
    pub send_upgrade_notice: bool,
    /// Today's stock: 39 persistent inventory slots.
    pub items: [u16; GOODS_COUNT],
    /// Spotlight rare item.
    pub rare_item: u16,
    /// Lottery items.
    pub lottery_items: [u16; LOTTERY_ITEM_COUNT],
    /// Last stock update (year, month, day).
    pub exchange_ymd: (u16, u8, u8),
    /// Last tier renewal (year, month, day).
    pub renewal_ymd: (u16, u8, u8),
    /// PC-port toggle: skip the foreign-town shopper requirement.
    /// PC ENHANCEMENT -- not retail. Retail requires visitor_flag.
    pub disable_visitor_req: bool,
}

impl Default for ShopState {
    fn default() -> Self {
        Self {
            sales_sum: 0,
            shop_level: ShopTier::Zakka,
            visitor_flag: false,
            upgrading_today: false,
            send_upgrade_notice: false,
            items: [0; GOODS_COUNT],
            rare_item: 0,
            lottery_items: [0; LOTTERY_ITEM_COUNT],
            exchange_ymd: (0, 0, 0),
            renewal_ymd: (0, 0, 0),
            disable_visitor_req: false,
        }
    }
}

impl ShopState {
    /// Add `sum` to the sales total, clamping to the current tier's upgrade
    /// threshold. Mirrors `mSP_PlusSales`: retail uses plain u32 `+=`
    /// (wrapping), then the clamp; NOT saturating_add.
    pub fn plus_sales(&mut self, sum: u32) {
        self.sales_sum = self.sales_sum.wrapping_add(sum);
        if let Some(cap) = self.shop_level.sales_cap() {
            if self.sales_sum > cap {
                self.sales_sum = cap;
            }
        }
    }

    /// Record a purchase: the full price counts toward the total.
    pub fn record_purchase(&mut self, price: u32) {
        self.plus_sales(price);
    }

    /// Record a sale to Nook: half of Nook's payout counts
    /// (`mSP_PlusSales(money / 2)`).
    pub fn record_sale(&mut self, payout: u32) {
        self.plus_sales(payout / 2);
    }

    /// Record a catalog/furniture order: the full price counts.
    pub fn record_catalog_order(&mut self, price: u32) {
        self.plus_sales(price);
    }

    /// The tier the sales total (and visitor flag) currently earns.
    /// Mirrors `mSP_GetRealShopLevel`, including the PC-port visitor
    /// requirement toggle.
    pub fn real_level(&self) -> ShopTier {
        if self.sales_sum >= DSUPER_SUM
            && (self.visitor_flag || self.disable_visitor_req)
        {
            ShopTier::Dsuper
        } else if self.sales_sum >= SUPER_SUM {
            ShopTier::Super
        } else if self.sales_sum >= COMBINI_SUM {
            ShopTier::Combini
        } else {
            ShopTier::Zakka
        }
    }

    /// Sync the displayed tier to the earned tier. Mirrors
    /// `mSP_RenewShopLevel`; returns true when the tier changed.
    /// This is the FINAL step of renovation, not the whole process.
    pub fn renew_level(&mut self) -> bool {
        let real = self.real_level();
        if self.shop_level != real {
            self.shop_level = real;
            true
        } else {
            false
        }
    }

    /// Schedule a renovation (aSL_JudgeRenewShop): if the earned tier
    /// exceeds the displayed tier, set the renewal date to today + 2
    /// days at opening time. Blocked when a bargain day falls on today,
    /// tomorrow, or the +2 day date. Returns true when scheduled.
    /// `bargain_ymd`: Some((y,m,d)) of Nook's sale day, or None.
    pub fn schedule_renewal(
        &mut self,
        today: (u16, u8, u8),
        open_hour: u8,
        bargain_ymd: Option<(u16, u8, u8)>,
    ) -> bool {
        if self.shop_level.tier_idx() >= self.real_level().tier_idx() {
            return false;
        }
        let plus1 = add_days(today, 1);
        let plus2 = add_days(today, 2);
        if let Some(b) = bargain_ymd {
            if b == today || b == plus1 || b == plus2 {
                return false; // Nook sale collides; skip scheduling.
            }
        }
        self.renewal_ymd = plus2;
        self.send_upgrade_notice = true;
        self.upgrading_today = true;
        let _ = open_hour; // opening time anchors the renewal RTC timestamp
        true
    }

    /// True when today has reached the scheduled renewal date.
    pub fn renewal_due(&self, today: (u16, u8, u8)) -> bool {
        self.upgrading_today && ymd_ge(today, self.renewal_ymd)
    }

    /// Complete a due renovation: rewrite the building (abstracted),
    /// then sync the tier. Clears the upgrade notice state.
    pub fn complete_renewal(&mut self, today: (u16, u8, u8)) -> bool {
        if !self.renewal_due(today) {
            return false;
        }
        let changed = self.renew_level();
        self.upgrading_today = false;
        self.send_upgrade_notice = false;
        changed
    }

    /// Mark a foreign-town shopper visit. Mirrors `mSP_SetNewVisitor`
    /// (the C version also checks the player is from another town).
    pub fn set_new_visitor(&mut self, is_foreign: bool) -> bool {
        if is_foreign {
            self.visitor_flag = true;
            true
        } else {
            false
        }
    }

    /// How many of the four tools (shovel, net, rod, axe) the shop may
    /// stock. Mirrors the `mSP_SelectTool` lockout: thresholds apply only
    /// in Nook's Cranny; higher tiers always allow all four.
    pub fn tool_slots(&self) -> u8 {
        if self.shop_level > ShopTier::Zakka {
            4
        } else if self.sales_sum < NET_SALES_SUM {
            1
        } else if self.sales_sum < ROD_SALES_SUM {
            2
        } else if self.sales_sum < AXE_SALES_SUM {
            3
        } else {
            4
        }
    }
}

/// Per-tier daily category slot counts.
///
/// Guide-derived (contemporary GameCube documentation), NOT decomp-traced:
/// [Cranny, Nook 'n' Go, Nookway, Nookington's].
#[derive(Clone, Copy, Debug)]
pub struct CategorySlots {
    pub furniture: [u8; 4],
    pub rare_furniture: [u8; 4],
    pub clothing: [u8; 4],
    pub stationery: [u8; 4],
    pub carpet: [u8; 4],
    pub wallpaper: [u8; 4],
    pub flowers: [u8; 4],
    pub saplings: [u8; 4],
    pub tools: [u8; 4],
}

pub const CATEGORY_SLOTS: CategorySlots = CategorySlots {
    furniture: [1, 2, 3, 5],
    rare_furniture: [0, 0, 1, 1],
    clothing: [1, 2, 3, 5],
    stationery: [1, 2, 2, 4],
    carpet: [1, 1, 2, 3],
    wallpaper: [1, 1, 2, 3],
    flowers: [2, 3, 4, 5],
    saplings: [1, 1, 2, 3],
    tools: [2, 3, 2, 3],
};

/// C ABI: deserved shop tier for a sales total. Mirrors
/// `mSP_GetRealShopLevel`. Returns 255 on no match (unreachable).
#[no_mangle]
pub extern "C" fn pc_shop_real_level(
    sales_sum: u32,
    visitor_flag: i32,
    disable_visitor_req: i32,
) -> u8 {
    let state = ShopState {
        sales_sum,
        visitor_flag: visitor_flag != 0,
        disable_visitor_req: disable_visitor_req != 0,
        ..ShopState::default()
    };
    state.real_level() as u8
}

/// C ABI: add `sum` to a sales total with the tier clamp applied.
/// Mirrors the arithmetic of `mSP_PlusSales`.
#[no_mangle]
pub extern "C" fn pc_shop_plus_sales(sales_sum: u32, tier: u8, sum: u32) -> u32 {
    let mut state = ShopState {
        sales_sum,
        shop_level: ShopTier::from_u8(tier).unwrap_or(ShopTier::Zakka),
        ..ShopState::default()
    };
    state.plus_sales(sum);
    state.sales_sum
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sales_clamp_discards_excess_before_remodel() {
        let mut shop = ShopState::default();
        shop.plus_sales(40_000);
        assert_eq!(shop.sales_sum, COMBINI_SUM); // clamped, not banked
        assert_eq!(shop.real_level(), ShopTier::Combini);
    }

    #[test]
    fn cumulative_thresholds() {
        let mut shop = ShopState::default();
        shop.plus_sales(24_999);
        assert_eq!(shop.real_level(), ShopTier::Zakka);
        shop.plus_sales(1);
        assert_eq!(shop.real_level(), ShopTier::Combini);
        // The displayed tier must renew before the cap rises (source:
        // mSP_PlusSales clamps to the current tier's threshold).
        assert!(shop.renew_level());
        shop.plus_sales(65_000); // 25k + 65k = 90k
        assert_eq!(shop.real_level(), ShopTier::Super);
    }

    #[test]
    fn nookingtons_needs_visitor_flag() {
        let mut shop = ShopState::default();
        shop.sales_sum = DSUPER_SUM;
        assert_eq!(shop.real_level(), ShopTier::Super); // no visitor yet
        shop.set_new_visitor(true);
        assert_eq!(shop.real_level(), ShopTier::Dsuper);
        // PC toggle skips the requirement.
        let mut shop2 = ShopState::default();
        shop2.sales_sum = DSUPER_SUM;
        shop2.disable_visitor_req = true;
        assert_eq!(shop2.real_level(), ShopTier::Dsuper);
    }

    #[test]
    fn sales_add_half_payout() {
        let mut shop = ShopState::default();
        shop.record_sale(1_000); // Nook pays 1000 -> +500 progress
        assert_eq!(shop.sales_sum, 500);
        shop.record_purchase(2_000); // full price
        assert_eq!(shop.sales_sum, 2_500);
    }

    #[test]
    fn tool_lockout_is_cranny_only() {
        let mut shop = ShopState::default();
        assert_eq!(shop.tool_slots(), 1); // shovel only
        shop.plus_sales(3_000);
        assert_eq!(shop.tool_slots(), 2); // + net
        shop.plus_sales(5_000);
        assert_eq!(shop.tool_slots(), 3); // + rod (8000 cumulative)
        shop.plus_sales(4_000);
        assert_eq!(shop.tool_slots(), 4); // + axe (12000 cumulative)
        // Higher tiers ignore the lockout.
        shop.shop_level = ShopTier::Combini;
        shop.sales_sum = 0;
        assert_eq!(shop.tool_slots(), 4);
    }

    #[test]
    fn renew_level_syncs_displayed_tier() {
        let mut shop = ShopState::default();
        // Sales clamp to the displayed tier's threshold until it renews.
        shop.plus_sales(25_000);
        assert!(shop.renew_level());
        assert_eq!(shop.shop_level, ShopTier::Combini);
        shop.plus_sales(65_000); // 25k + 65k = 90k
        assert!(shop.renew_level());
        assert_eq!(shop.shop_level, ShopTier::Super);
        assert!(!shop.renew_level()); // no change second time
    }

    #[test]
    fn goods_slots_match_save() {
        let shop = ShopState::default();
        assert_eq!(shop.items.len(), GOODS_COUNT);
        assert_eq!(GOODS_COUNT, 39);
    }

    #[test]
    fn renovation_scheduler() {
        let mut shop = ShopState::default();
        shop.plus_sales(25_000); // earn Nook 'n Go
        // Schedule: +2 days, no bargain collision.
        assert!(shop.schedule_renewal((2026, 10, 7), 9, None));
        assert_eq!(shop.renewal_ymd, (2026, 10, 9));
        assert!(shop.send_upgrade_notice);
        assert!(shop.upgrading_today);
        // Not due yet.
        assert!(!shop.renewal_due((2026, 10, 8)));
        assert!(!shop.complete_renewal((2026, 10, 8)));
        assert_eq!(shop.shop_level, ShopTier::Zakka); // still old tier
        // Due on the renewal date.
        assert!(shop.renewal_due((2026, 10, 9)));
        assert!(shop.complete_renewal((2026, 10, 9)));
        assert_eq!(shop.shop_level, ShopTier::Combini);
        assert!(!shop.upgrading_today);
        assert!(!shop.send_upgrade_notice);
        // Bargain day blocks scheduling.
        let mut shop2 = ShopState::default();
        shop2.plus_sales(25_000);
        assert!(!shop2.schedule_renewal((2026, 10, 7), 9, Some((2026, 10, 9))));
        assert!(!shop2.schedule_renewal((2026, 10, 7), 9, Some((2026, 10, 7))));
        assert!(!shop2.upgrading_today);
    }

    #[test]
    fn sales_wrap_not_saturate() {
        // Retail uses plain u32 += (wrapping); the clamp is what bounds it.
        let mut shop = ShopState::default();
        shop.shop_level = ShopTier::Dsuper; // no cap
        shop.sales_sum = u32::MAX - 10;
        shop.plus_sales(20);
        assert_eq!(shop.sales_sum, 9); // wrapped, not saturated
    }
}
