//! Stalk market (turnips) — pure data and kernels.
//!
//! Verified against `src/game/m_kabu_manager.c`,
//! `src/actor/npc/event/ac_ev_kabuPeddler_move.c_inc`,
//! `src/actor/npc/ac_npc_shop_common.c`, `src/game/m_all_grow_ovl.c`,
//! `src/actor/ac_event_manager.c`,
//! `include/m_kabu_manager.h`, `include/m_name_table.h`,
//! `include/m_event.h`, `include/ac_ev_kabuPeddler.h`,
//! `include/lb_rtc.h` (GAFE01_00 Rev 0 USA decomp).
//! All 13 brief claims checked verbatim; no corrections.
//!
//! Nuances carried: trend B has an EXTRA probability clamp the brief
//! missed — after the ×0.3 drop, `price_decrease_percent` is clamped
//! to ≤ 0.1; the trend-B minimum comparison is literally `>` (not
//! `<`) — do not "fix" it; trend C keeps `0.14999998f` verbatim; the
//! `Kabu_manager()` week window is bidirectional (±1 week); the
//! trend-B decomp comment has a typo (`//0.1f` labeled `(20%)`) but
//! the behavior is residual 20% as briefed.
//!
//! Architecture: the schedule generation, the date-window logic, Joan's
//! actor and transaction, Nook's shop transaction, the field-growth
//! spoilage passes, and all RNG stay in C. This module holds only the
//! source-proven pure data and calculations with primitive arguments,
//! built as a deterministic differential harness: every kernel takes
//! the roll results as inputs and never rolls itself.

/// Weekday constants (`include/lb_rtc.h`).
pub mod weekday {
    pub const SUNDAY: u8 = 0;
    pub const MONDAY: u8 = 1;
    pub const SATURDAY: u8 = 6;
    pub const MAX: u8 = 7;
}

/// Market trends (`include/m_kabu_manager.h`).
pub mod trend {
    pub const A: u8 = 0; // spike
    pub const B: u8 = 1; // random
    pub const C: u8 = 2; // falling
}

/// Save-layout constants: `Kabu_price_c` is 0x18 bytes
/// (7×u16 prices + u16 trend + `lbRTC_time_c`), inside common save
/// data at offset 0x020480. C remains authoritative for the layout.
pub mod save {
    pub const SIZE: u8 = 0x18;
    pub const PRICE_MIN: u16 = 10;
    pub const PRICE_MAX: u16 = 2000;
}

/// Turnip item IDs (`include/m_name_table.h`).
pub mod item {
    pub const KABU_START: u16 = 0x2F00;
    pub const KABU_10: u16 = 0x2F00;
    pub const KABU_50: u16 = 0x2F01;
    pub const KABU_100: u16 = 0x2F02;
    pub const KABU_SPOILED: u16 = 0x2F03;
}

/// Joan's identity.
pub mod joan {
    pub const NPC: u16 = 0xD007;
    /// Purchase quantities and their turnip items.
    pub const SUM: [u32; 4] = [1, 10, 50, 100];
    pub const KABU_NO: [u16; 3] = [super::item::KABU_10, super::item::KABU_50, super::item::KABU_100];
    /// Transaction quantities; index 3 (the exit choice) is 0.
    pub const SELL_SUM: [u32; 4] = [10, 50, 100, 0];
    pub const TYPE_NUM: u8 = 4;
    /// Greetings: first talk vs repeat (spoken_pids in C).
    pub const GREET_FIRST: u16 = 0x06FD;
    pub const GREET_AGAIN: u16 = 0x0718;
}

/// Nook's turnip stack-size table (also `{10, 50, 100, 0}`).
pub const NOOK_KABU_SUM: [u32; 4] = [10, 50, 100, 0];

/// Nook's turnip branch order (`aNSC_check_buy_item_single`), verbatim:
/// quest-protected → spoiled → Sunday → ordinary. Rust-side markers;
/// the C enum values stay in the shop code.
pub mod nook_branch {
    pub const REFUSE_QUEST: u8 = 0;
    pub const TAKE_OFF_HANDS: u8 = 1;
    pub const NO_SUNDAY_TURNIPS: u8 = 2;
    pub const ORDINARY: u8 = 3;
}

/// Sunday buy price (`Kabu_decide_price_sunday`), verbatim:
/// `price = 1.0 + (fqrand() − 0.5) × 0.6`, stored as `price × 100.0`
/// truncated into the u16 field → 70–129 Bells. The roll stays in C.
pub fn sunday_price(roll: f32) -> u16 {
    ((1.0f32 + (roll - 0.5) * 0.6) * 100.0f32) as u16
}

/// Next trend (`Kabu_decide_trade_market`), verbatim: one fqrand() draw,
/// subtract-until-select over the previous trend's two probabilities,
/// residual → C. The defensive `i = 0` clamp is preserved (it cannot
/// trigger, i is always 0–2).
pub fn next_trend(prev: u8, roll: f32) -> u8 {
    const ODDS: [[f32; 2]; 3] = [
        [0.5, 0.3], // from A: A 50%, B 30%, C residual 20%
        [0.6, 0.2], // from B: A 60%, B 20%, C residual 20%
        [0.6, 0.3], // from C: A 60%, B 30%, C residual 10%
    ];
    let table = ODDS[(prev % 3) as usize];
    let mut chosen = roll;
    let mut i = 0u8;
    for t in 0..2usize {
        if chosen < table[t] {
            break;
        }
        chosen -= table[t];
        i += 1;
    }
    // for-loop exit without break leaves i = 2
    if i >= 3 {
        i = 0;
    }
    i
}

/// Trend A spike price: `Sunday × 8.0`, truncated into the u16 field.
/// Sunday is an integer u16, so no fraction is introduced.
pub fn spike_price(sunday: u16) -> u16 {
    (sunday as f32 * 8.0f32) as u16
}

/// Trend A spike day: `(int)(fqrand() × 5) + MONDAY` → Monday–Friday.
/// Saturday is not eligible. The roll stays in C.
pub fn spike_day(roll: f32) -> u8 {
    (roll * 5.0f32) as u8 + weekday::MONDAY
}

/// Trend B state, carried across the six weekday rolls.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct TrendBState {
    pub price: f32,
    pub inc_mult: f32,
    pub dec_prob: f32,
}

impl TrendBState {
    pub fn init(sunday: u16) -> Self {
        TrendBState {
            price: sunday as f32,
            inc_mult: 1.05,
            dec_prob: 0.1,
        }
    }
}

/// Trend B minimum: 80% of the Sunday price.
pub fn trend_b_min_price(sunday: u16) -> f32 {
    sunday as f32 * 0.8
}

/// One trend-B weekday step (`Kabu_decide_price_schedule_typeB`),
/// verbatim operation order. The roll stays in C.
///
/// Preserved exactly: the literal `current_price > min_price`
/// comparison (NOT `<` — do not "fix" it into a conventional clamp),
/// and the probability clamp the brief missed — after the ×0.3 drop,
/// `dec_prob` is clamped to ≤ 0.1 (fires whenever the pre-drop chance
/// exceeded 1/3, e.g. two rises → 0.38 → 0.114 → 0.1).
pub fn trend_b_step(state: &mut TrendBState, min_price: f32, roll: f32) -> u16 {
    if roll < state.dec_prob {
        state.price *= 0.7;
        if state.price > min_price {
            state.price = min_price;
        }
        state.dec_prob *= 0.3;
        if state.dec_prob > 0.1 {
            state.dec_prob = 0.1;
        }
    } else {
        state.price *= state.inc_mult;
        state.inc_mult += 0.01;
        state.dec_prob += 0.14;
    }
    state.price as u16
}

/// Trend C factor, verbatim: `0.8 + 0.14999998 × fqrand()`. The
/// compiler-matching constant is retained, not normalized to 0.15.
/// Range [80%, 95%).
pub fn trend_c_factor(roll: f32) -> f32 {
    0.8f32 + 0.14999998f32 * roll
}

/// One trend-C weekday step: chained multiplication, u16 store.
pub fn trend_c_step(price: f32, roll: f32) -> (f32, u16) {
    let next = price * trend_c_factor(roll);
    (next, next as u16)
}

/// Joan's transaction total: `Sunday price × sell_sum[kabu_type]`.
pub fn joan_total(sunday_price: u16, kabu_type: u8) -> u32 {
    sunday_price as u32 * joan::SELL_SUM[(kabu_type % 4) as usize]
}

/// Joan's sale messages (`aEKPD_sell_check_after`): two tables,
/// `{0x0711, 0x0710, 0x070F}` and `{0x0711, 0x0717, 0x0716}`,
/// indexed 0 = success, 1 = not enough money, 2 = no inventory space.
/// The empty-slot check runs before the money check (in C).
pub fn joan_sale_msg(has_space: bool, has_money: bool, alt_table: bool) -> u16 {
    let table: [u16; 3] = if alt_table {
        [0x0711, 0x0717, 0x0716]
    } else {
        [0x0711, 0x0710, 0x070F]
    };
    if !has_space {
        table[2]
    } else if !has_money {
        table[1]
    } else {
        table[0]
    }
}

/// Nook's turnip payout (`aNSC_buy_check_init`), verbatim:
/// `Kabu_get_price() × kabu_sum[item − ITM_KABU_START] × counter`.
/// `counter` is the per-transaction counter (set to 1 when more than
/// one item is selected).
pub fn nook_payout(daily_price: u16, item: u16, counter: u32) -> u32 {
    let stack = NOOK_KABU_SUM[((item - item::KABU_START) % 4) as usize];
    daily_price as u32 * stack * counter
}

/// Nook's turnip branch order. Spoiled is checked BEFORE the Sunday
/// restriction — checking Sunday first would show the wrong dialogue
/// for spoiled turnips.
pub fn nook_turnip_branch(is_quest: bool, is_spoiled: bool, is_sunday: bool) -> u8 {
    if is_quest {
        nook_branch::REFUSE_QUEST
    } else if is_spoiled {
        nook_branch::TAKE_OFF_HANDS
    } else if is_sunday {
        nook_branch::NO_SUNDAY_TURNIPS
    } else {
        nook_branch::ORDINARY
    }
}

/// Spoilage decision (`mAGrw_SpoilKabu`), verbatim: turnip-category
/// items spoil when the grow flag is true, or unconditionally when
/// `grow_info` is NULL (the inventory/house/haniwa path). The
/// ITEM1/KABU category test stays in C — pass its result.
pub fn spoil_kabu(is_kabu_item: bool, grow_info_present: bool, spoil_flag: bool) -> bool {
    if !is_kabu_item {
        return false;
    }
    if grow_info_present {
        spoil_flag
    } else {
        true
    }
}

/// Spoilage timing (`mAGrw_CheckSpoilKabuTime`): the grow time is
/// advanced to the coming Sunday — `days = 7 − weekday` when the
/// weekday is not Sunday — and spoilage fires when that Sunday is
/// overdue relative to now. The date comparison stays in C.
pub fn spoil_weekday_offset(grow_weekday: u8) -> u8 {
    if grow_weekday != weekday::SUNDAY && grow_weekday < weekday::MAX {
        weekday::MAX - grow_weekday
    } else {
        0
    }
}

// ---- C ABI ----
// All pc_-prefixed: the C originals stay compiled; these are staged kernels.

#[no_mangle]
pub extern "C" fn pc_kabu_sunday_price(roll: f32) -> u16 {
    sunday_price(roll)
}

#[no_mangle]
pub extern "C" fn pc_kabu_next_trend(prev: u8, roll: f32) -> u8 {
    next_trend(prev, roll)
}

#[no_mangle]
pub extern "C" fn pc_kabu_spike_price(sunday: u16) -> u16 {
    spike_price(sunday)
}

#[no_mangle]
pub extern "C" fn pc_kabu_spike_day(roll: f32) -> u8 {
    spike_day(roll)
}

#[no_mangle]
pub extern "C" fn pc_kabu_trend_b_init(state: *mut TrendBState, sunday: u16) {
    if !state.is_null() {
        unsafe {
            *state = TrendBState::init(sunday);
        }
    }
}

#[no_mangle]
pub extern "C" fn pc_kabu_trend_b_min_price(sunday: u16) -> f32 {
    trend_b_min_price(sunday)
}

#[no_mangle]
pub extern "C" fn pc_kabu_trend_b_step(state: *mut TrendBState, min_price: f32, roll: f32) -> u16 {
    if state.is_null() {
        return 0;
    }
    unsafe { trend_b_step(&mut *state, min_price, roll) }
}

#[no_mangle]
pub extern "C" fn pc_kabu_trend_c_factor(roll: f32) -> f32 {
    trend_c_factor(roll)
}

#[no_mangle]
pub extern "C" fn pc_kabu_joan_total(sunday_price: u16, kabu_type: u8) -> u32 {
    joan_total(sunday_price, kabu_type)
}

#[no_mangle]
pub extern "C" fn pc_kabu_joan_sale_msg(has_space: u8, has_money: u8, alt_table: u8) -> u16 {
    joan_sale_msg(has_space != 0, has_money != 0, alt_table != 0)
}

#[no_mangle]
pub extern "C" fn pc_kabu_nook_payout(daily_price: u16, item: u16, counter: u32) -> u32 {
    nook_payout(daily_price, item, counter)
}

#[no_mangle]
pub extern "C" fn pc_kabu_nook_turnip_branch(is_quest: u8, is_spoiled: u8, is_sunday: u8) -> u8 {
    nook_turnip_branch(is_quest != 0, is_spoiled != 0, is_sunday != 0)
}

#[no_mangle]
pub extern "C" fn pc_kabu_spoil_kabu(is_kabu: u8, grow_present: u8, spoil_flag: u8) -> u8 {
    spoil_kabu(is_kabu != 0, grow_present != 0, spoil_flag != 0) as u8
}

#[no_mangle]
pub extern "C" fn pc_kabu_spoil_weekday_offset(grow_weekday: u8) -> u8 {
    spoil_weekday_offset(grow_weekday)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constants() {
        assert_eq!(weekday::SUNDAY, 0);
        assert_eq!(weekday::MONDAY, 1);
        assert_eq!(weekday::SATURDAY, 6);
        assert_eq!(weekday::MAX, 7);
        assert_eq!(trend::A, 0);
        assert_eq!(trend::B, 1);
        assert_eq!(trend::C, 2);
        assert_eq!(save::SIZE, 0x18);
        assert_eq!(item::KABU_START, 0x2F00);
        assert_eq!(item::KABU_SPOILED, 0x2F03);
        assert_eq!(joan::NPC, 0xD007);
        assert_eq!(joan::SUM, [1, 10, 50, 100]);
        assert_eq!(joan::SELL_SUM, [10, 50, 100, 0]);
        assert_eq!(joan::GREET_FIRST, 0x06FD);
        assert_eq!(joan::GREET_AGAIN, 0x0718);
    }

    #[test]
    fn sunday() {
        assert_eq!(sunday_price(0.0), 70);
        assert_eq!(sunday_price(0.5), 100);
        // fqrand's maximum (1 - 2^-23): 1.29999995 * 100 rounds to 130.0 in
        // f32, so 130 IS reachable despite the source's "[70, 130)" comment.
        assert_eq!(sunday_price(0.9999999), 130);
        assert_eq!(sunday_price(0.999), 129);
    }

    #[test]
    fn trends() {
        // from A: roll < 0.5 -> A; < 0.8 -> B; else C
        assert_eq!(next_trend(trend::A, 0.0), trend::A);
        assert_eq!(next_trend(trend::A, 0.499), trend::A);
        assert_eq!(next_trend(trend::A, 0.5), trend::B);
        assert_eq!(next_trend(trend::A, 0.799), trend::B);
        assert_eq!(next_trend(trend::A, 0.8), trend::C);
        assert_eq!(next_trend(trend::A, 0.999), trend::C);
        // from B: < 0.6 -> A; < 0.8 -> B; else C
        assert_eq!(next_trend(trend::B, 0.59), trend::A);
        assert_eq!(next_trend(trend::B, 0.6), trend::B);
        // f32: 0.8f - 0.6f = 0.19999999 < 0.2f, so roll 0.8 still picks B
        // (Kabu_decide_trade_market subtracts the odds in f32).
        assert_eq!(next_trend(trend::B, 0.8), trend::B);
        assert_eq!(next_trend(trend::B, 0.81), trend::C);
        // from C: < 0.6 -> A; < 0.9 -> B; else C
        assert_eq!(next_trend(trend::C, 0.6), trend::B);
        // f32 again: 0.9f - 0.6f = 0.29999995 < 0.3f, so roll 0.9 picks B.
        assert_eq!(next_trend(trend::C, 0.9), trend::B);
        assert_eq!(next_trend(trend::C, 0.91), trend::C);
        assert_eq!(spike_price(100), 800);
        assert_eq!(spike_price(129), 1032);
        assert_eq!(spike_day(0.0), 1); // Monday
        assert_eq!(spike_day(0.999), 5); // Friday
    }

    #[test]
    fn trend_b() {
        // rise path: 100 * 1.05f = 104.99999 in f32 -> 104; mult -> 1.06; prob -> 0.24
        let mut s = TrendBState::init(100);
        let min = trend_b_min_price(100);
        assert_eq!(min, 80.0);
        assert_eq!(trend_b_step(&mut s, min, 0.5), 104);
        assert!((s.inc_mult - 1.06).abs() < 1e-6);
        assert!((s.dec_prob - 0.24).abs() < 1e-6);
        // drop path: 105 * 0.7 = 73.5 -> 73; 73.5 > 80? no -> stays 73.5
        assert_eq!(trend_b_step(&mut s, min, 0.0), 73);
        // dec_prob: 0.24 * 0.3 = 0.072, no clamp
        assert!((s.dec_prob - 0.072).abs() < 1e-6);
        // the literal > comparison: price 90 * 0.7 = 63 < 80 -> stays 63
        let mut s2 = TrendBState::init(100);
        s2.price = 90.0;
        assert_eq!(trend_b_step(&mut s2, min, 0.0), 63);
        // price 120 * 0.7 = 84 > 80 -> clamped to 80
        let mut s3 = TrendBState::init(100);
        s3.price = 120.0;
        assert_eq!(trend_b_step(&mut s3, min, 0.0), 80);
        // the missed clamp: two rises -> 0.1+0.14+0.14 = 0.38; drop -> 0.114 -> 0.1
        let mut s4 = TrendBState::init(100);
        trend_b_step(&mut s4, min, 0.9);
        trend_b_step(&mut s4, min, 0.9);
        assert!((s4.dec_prob - 0.38).abs() < 1e-6);
        trend_b_step(&mut s4, min, 0.0);
        assert!((s4.dec_prob - 0.1).abs() < 1e-6);
    }

    #[test]
    fn trend_c() {
        // factor bounds: roll 0 -> 0.8; fqrand's max (1 - 2^-23) gives exactly
        // 0.95f32 (0.8f + 0.14999998f * r rounds to it), so the range is
        // [0.8, 0.95] in f32 rather than the comment's [80%, 95%).
        assert_eq!(trend_c_factor(0.0), 0.8);
        assert_eq!(trend_c_factor(0.9999999), 0.95f32);
        assert!(trend_c_factor(0.999) < 0.95);
        let (p, stored) = trend_c_step(100.0, 0.0);
        assert_eq!(p, 80.0);
        assert_eq!(stored, 80);
    }

    #[test]
    fn joan_nook() {
        assert_eq!(joan_total(100, 0), 1000);
        assert_eq!(joan_total(100, 1), 5000);
        assert_eq!(joan_total(100, 2), 10000);
        assert_eq!(joan_total(100, 3), 0); // exit choice
        assert_eq!(joan_sale_msg(true, true, false), 0x0711);
        assert_eq!(joan_sale_msg(true, false, false), 0x0710);
        assert_eq!(joan_sale_msg(false, true, false), 0x070F);
        assert_eq!(joan_sale_msg(true, false, true), 0x0717);
        assert_eq!(nook_payout(100, item::KABU_10, 1), 1000);
        assert_eq!(nook_payout(100, item::KABU_50, 2), 10000);
        assert_eq!(nook_payout(100, item::KABU_100, 1), 10000);
        // branch order: quest > spoiled > sunday > ordinary
        assert_eq!(nook_turnip_branch(true, true, true), nook_branch::REFUSE_QUEST);
        assert_eq!(nook_turnip_branch(false, true, true), nook_branch::TAKE_OFF_HANDS);
        assert_eq!(nook_turnip_branch(false, false, true), nook_branch::NO_SUNDAY_TURNIPS);
        assert_eq!(nook_turnip_branch(false, false, false), nook_branch::ORDINARY);
    }

    #[test]
    fn spoilage() {
        assert!(spoil_kabu(true, true, true));
        assert!(!spoil_kabu(true, true, false));
        assert!(spoil_kabu(true, false, false)); // null grow_info: unconditional
        assert!(!spoil_kabu(false, false, true)); // not a turnip
        assert_eq!(spoil_weekday_offset(0), 0); // Sunday
        assert_eq!(spoil_weekday_offset(1), 6); // Monday -> +6
        assert_eq!(spoil_weekday_offset(6), 1); // Saturday -> +1
    }
}
