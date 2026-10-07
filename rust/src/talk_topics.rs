//! Normal-villager topic taxonomy (`aQMgr_talk_normal_select_talk` family).
//!
//! Verified against `ac_quest_talk_normal_init.c` (USA Rev. 0 decomp).
//!
//! This is the hierarchical probabilistic topic generator behind
//! ordinary villager conversation:
//!
//! ```text
//! NORMAL VILLAGER TALK
//! ├─ FIRST-JOB HINT (absolute priority)
//! │    └─ 0x0841 + hint_type + looks*10
//! └─ OTHERWISE
//!     ├─ HAPPY mood → KI tree      {40,30,10,10,10}
//!     │    ├─ normal        base[looks] + RANDOM(10)
//!     │    ├─ weather/time  base + time_kind*6 + weather*2 + RANDOM(2)
//!     │    ├─ free item     base[looks] + RANDOM(3)  (needs empty pocket)
//!     │    ├─ furniture     base[looks] + RANDOM(3)  (needs ftr/cpt/wall)
//!     │    └─ free item+money (needs empty pocket AND wallet >= 3000)
//!     └─ other moods → NORMAL tree {70,30}
//!          ├─ 70% normal-2 {15,35,35,15}
//!          │    ├─ letter      base[looks] + mail_selection_type
//!          │    ├─ normal-3 {49,17,17,17}
//!          │    │    ├─ ordinary     base + RANDOM(10)
//!          │    │    ├─ weather/time base + time*6 + weather*2 + RANDOM(2)
//!          │    │    ├─ weather      base + weather*5 + RANDOM(5|4) + ofs
//!          │    │    └─ season       base + add_table[month-1]
//!          │    ├─ trade {25,25,25,25} (each base[looks] + RANDOM(5))
//!          │    └─ memory       base + mem_idx*2 + (letter? 0 : 1)
//!          └─ 30% game {40,60}
//!               ├─ game hint  base[looks] + RANDOM(5)
//!               └─ game/event (removal / special-event / calendar-rumor)
//! ```
//!
//! Key properties (source-proven):
//! - The probability chooser builds a 100-entry table from the weights,
//!   shuffles it 30 times, then picks one entry — the weights are exact.
//! - Every leaf is `table[looks] + variant`; `looks` (0-5) is personality,
//!   not appearance.
//! - Eligibility failures cascade: KI leaf -1 → KI normal; normal-2 -1 →
//!   normal-3. (Deeper chains — memory→trade, trade→normal-3,
//!   event→special→game-hint — are described in docs but their exact
//!   fallback edges are not yet individually verified; see gaps.)
//! - SAKURA weather is folded to CLEAR before weather formulas.
//! - `ret_msg` is uninitialized in three deciders (@BUG comments in the
//!   source: `aQMgr_decide_normal_2_msg_no`, `aQMgr_decide_msg_trade`,
//!   `aQMgr_decide_msg_normal_3_msg_no`, `aQMgr_decide_normal_msg_no`);
//!   the port initializes to -1, which matches the observed fallback
//!   behavior and avoids UB.

/// Personality index (`mNpc_LOOKS_*`): 0 normal, 1 peppy, 2 lazy,
/// 3 jock, 4 cranky, 5 snooty.
pub const LOOKS_NUM: usize = 6;

/// KI topic categories (`aQMgr_MSG_KI_*`).
pub mod ki {
    pub const NORMAL: usize = 0;
    pub const WEATHER_TIME: usize = 1;
    pub const FREE_ITEM: usize = 2;
    pub const FTR: usize = 3;
    pub const FREE_ITEM_MONEY: usize = 4;
    pub const NUM: usize = 5;
}

/// Normal-2 categories.
pub mod normal2 {
    pub const LETTER: usize = 0;
    pub const NORMAL_3: usize = 1;
    pub const TRADE: usize = 2;
    pub const MEMORY: usize = 3;
    pub const NUM: usize = 4;
}

/// Trade categories.
pub mod trade {
    pub const FREE_ITEM: usize = 0;
    pub const FTR: usize = 1;
    pub const ITEM: usize = 2; // fish/insect
    pub const FREE_ITEM_MONEY: usize = 3;
    pub const NUM: usize = 4;
}

/// Normal-3 categories.
pub mod normal3 {
    pub const NORMAL: usize = 0;
    pub const WEATHER_TIME: usize = 1;
    pub const WEATHER: usize = 2;
    pub const SEASON: usize = 3;
    pub const NUM: usize = 4;
}

/// Game categories.
pub mod game {
    pub const HINT: usize = 0;
    pub const EVENT: usize = 1;
    pub const NUM: usize = 2;
}

/// Probability tables, verbatim.
pub const KI_PROB: [u8; ki::NUM] = [40, 30, 10, 10, 10];
pub const NORMAL_1_PROB: [u8; 2] = [70, 30];
pub const NORMAL_2_PROB: [u8; normal2::NUM] = [15, 35, 35, 15];
pub const TRADE_PROB: [u8; trade::NUM] = [25, 25, 25, 25];
pub const NORMAL_3_PROB: [u8; normal3::NUM] = [49, 17, 17, 17];
pub const GAME_PROB: [u8; game::NUM] = [40, 60];

/// Message base tables, verbatim (indexed by looks 0-5).
pub mod base {
    pub const KI_NORMAL: [i32; 6] = [0x20C3, 0x2099, 0x25B0, 0x257E, 0x29F0, 0x0F8C];
    pub const KI_WEATHER_TIME: [i32; 6] = [0x1FE8, 0x1DD0, 0x25DF, 0x2621, 0x2A6A, 0x0F60];
    pub const KI_FREE_ITEM: [i32; 6] = [0x1396, 0x1CBE, 0x2020, 0x202D, 0x2947, 0x0B36];
    pub const KI_FTR: [i32; 6] = [0x13C7, 0x1CC9, 0x2049, 0x205D, 0x2974, 0x0B49];
    pub const KI_FREE_ITEM_MONEY: [i32; 6] = [0x13DE, 0x1CF9, 0x2134, 0x203A, 0x295A, 0x0B5D];
    pub const LETTER: [i32; 6] = [0x136C, 0x1BEB, 0x189D, 0x1C5F, 0x2919, 0x0A12];
    pub const MEMORY: [i32; 6] = [0x1386, 0x1C78, 0x18B6, 0x1CE9, 0x2933, 0x0D4D];
    pub const TRADE_FREE_ITEM: [i32; 6] = [0x1205, 0x150C, 0x1772, 0x1C88, 0x2868, 0x0AD7];
    pub const TRADE_FTR: [i32; 6] = [0x1311, 0x1839, 0x17CA, 0x1CA5, 0x28C9, 0x0ABC];
    pub const TRADE_ITEM: [i32; 6] = [0x12FC, 0x1BA8, 0x1868, 0x2390, 0x2903, 0x0B03];
    pub const TRADE_FREE_ITEM_MONEY: [i32; 6] = [0x134B, 0x1BBD, 0x184E, 0x236F, 0x28E4, 0x0B19];
    pub const NORMAL3_NORMAL: [i32; 6] = [0x1526, 0x169A, 0x16BD, 0x16DF, 0x16F9, 0x0EE7];
    pub const NORMAL3_WEATHER_TIME: [i32; 6] = [0x13A6, 0x1D08, 0x1F81, 0x1FA0, 0x2A9A, 0x0F00];
    pub const NORMAL3_WEATHER: [i32; 6] = [0x1FC1, 0x1D29, 0x206C, 0x20AA, 0x2A38, 0x0F2C];
    pub const NORMAL3_SEASON: [i32; 6] = [0x1FD7, 0x1D41, 0x1F55, 0x1F6B, 0x2A13, 0x0F4A];
    pub const GAME_HINT: [i32; 6] = [0x11F1, 0x14FA, 0x2005, 0x2014, 0x2858, 0x0FAB];
    pub const REMOVE_YES: [i32; 6] = [0x11FB, 0x1504, 0x1CD5, 0x1CDF, 0x2862, 0x0FA5];
    pub const EV_SPECIAL: [i32; 6] = [0x11D9, 0x14E2, 0x1885, 0x211C, 0x273A, 0x0D61];
    pub const EV_CAL: [i32; 6] = [0x118E, 0x17ED, 0x27B2, 0x2806, 0x26EE, 0x0B87];
}

/// Month -> season message offset (`add_table`, verbatim). Indexed by
/// `month - 1` (source clamps out-of-range months to 0).
pub const SEASON_ADD_TABLE: [i32; 12] = [10, 11, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9];

/// First-job hint base message ID.
pub const FJ_HINT_BASE: i32 = 0x0841;

/// `aQMgr_decide_idx_prob_table` verbatim: build the 100-entry table
/// from weights, shuffle 30 times (two RANDOM(100) swaps each), pick
/// one entry via RANDOM(100). `rng(n)` must return a value in [0, n).
pub fn decide_idx_prob_table(prob: &[u8], rng: &mut dyn FnMut(u32) -> u32) -> usize {
    let mut decide_table = [0u8; 100];
    let mut j = 0usize;
    for (i, &w) in prob.iter().enumerate() {
        let mut p = w;
        while p != 0 {
            if j >= 100 {
                break;
            }
            decide_table[j] = i as u8;
            j += 1;
            p -= 1;
        }
    }
    for _ in 0..30 {
        let idx0 = rng(100) as usize;
        let idx1 = rng(100) as usize;
        decide_table.swap(idx0, idx1);
    }
    decide_table[rng(100) as usize] as usize
}

/// Top-level talk kind from `aQMgr_talk_normal_select_talk`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TalkKind {
    /// First-job hint has absolute priority.
    FjHint,
    /// HAPPY mood: the KI distribution.
    Ki,
    /// All other moods: the normal/game distribution.
    Normal,
}

/// Top-level dispatch. `first_job_hint_pending` is the source's
/// `!PlayerManKind && ((CheckFirstJob && !CheckEvent(ev)) ||
/// !CheckFirstJobHint)`; `mood_happy` is `animal->mood == FEEL_HAPPY`.
pub fn select_talk_kind(first_job_hint_pending: bool, mood_happy: bool) -> TalkKind {
    if first_job_hint_pending {
        TalkKind::FjHint
    } else if mood_happy {
        TalkKind::Ki
    } else {
        TalkKind::Normal
    }
}

/// `aQMgr_get_fj_hint_msg` verbatim: `0x0841 + hint_type + looks*10`.
/// `hint_type` is `mPr_GetFirstJobHintTime()` = `hint_count & 0x7F`.
pub fn fj_hint_msg_no(hint_type: i32, looks: usize) -> i32 {
    FJ_HINT_BASE + hint_type + looks as i32 * 10
}

/// `aQMgr_get_msg_weather_time` core: `base + time_kind*6 +
/// weather*2 + RANDOM(2)`. SAKURA→CLEAR folding is caller-side.
pub fn weather_time_msg_no(base: i32, time_kind: i32, weather: i32, rand2: i32) -> i32 {
    base + time_kind * 6 + weather * 2 + rand2
}

/// Normal-3 weather leaf: `base + weather*5 + RANDOM(msg_cnt) + ofs`,
/// with `msg_cnt = 4, ofs = 1` when `player_man_kind`, else `5, 0`.
pub fn normal3_weather_msg_no(
    base: i32,
    weather: i32,
    player_man_kind: bool,
    rand_n: i32,
) -> i32 {
    let (msg_cnt, ofs) = if player_man_kind { (4, 1) } else { (5, 0) };
    let _ = msg_cnt;
    base + weather * 5 + rand_n + ofs
}

/// Normal-3 season leaf: `base + add_table[month - 1]` (month clamped).
pub fn normal3_season_msg_no(base: i32, month_1_12: i32) -> i32 {
    let m = month_1_12 - 1;
    let m = if m < 0 || m >= 12 { 0 } else { m };
    base + SEASON_ADD_TABLE[m as usize]
}

/// Letter leaf: `base[looks] + mail_selection_type` (0-5, NOT a random
/// range — the offset records which mail strategy was selected).
pub fn letter_msg_no(base: i32, mail_selection_type: i32) -> i32 {
    base + mail_selection_type
}

/// Memory leaf: `base + mem_idx*2 + (has_letter ? 0 : 1)`.
pub fn memory_msg_no(base: i32, mem_idx: i32, has_letter: bool) -> i32 {
    base + mem_idx * 2 + if has_letter { 0 } else { 1 }
}

/// Memory fallback (no category memory, own friendship > 80):
/// `base + MEMORY_NUM*2 + 2` = `base + 8`.
pub fn memory_fallback_msg_no(base: i32) -> i32 {
    base + 3 * 2 + 2
}

/// Verified fallback rules:
/// - KI leaf returns -1 → KI normal.
/// - normal-2 leaf returns -1 → normal-3.
pub fn ki_fallback() -> usize {
    ki::NORMAL
}

/// The KI free-item+money gate: the topic is valid only with an empty
/// pocket AND `wallet >= 3000`.
pub fn ki_free_item_money_ok(has_free_pocket: bool, wallet: i32) -> bool {
    has_free_pocket && wallet >= 3000
}

// ---- C ABI ----

/// C ABI: top-level talk kind. 0=FjHint, 1=Ki, 2=Normal.
#[no_mangle]
pub extern "C" fn pc_select_talk_kind(first_job_hint_pending: u8, mood_happy: u8) -> u8 {
    match select_talk_kind(first_job_hint_pending != 0, mood_happy != 0) {
        TalkKind::FjHint => 0,
        TalkKind::Ki => 1,
        TalkKind::Normal => 2,
    }
}

/// C ABI: first-job hint message ID.
#[no_mangle]
pub extern "C" fn pc_fj_hint_msg_no(hint_type: i32, looks: u8) -> i32 {
    fj_hint_msg_no(hint_type, looks as usize)
}

/// C ABI: weather/time message ID.
#[no_mangle]
pub extern "C" fn pc_weather_time_msg_no(base: i32, time_kind: i32, weather: i32, rand2: i32) -> i32 {
    weather_time_msg_no(base, time_kind, weather, rand2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prob_tables_verbatim() {
        assert_eq!(KI_PROB, [40, 30, 10, 10, 10]);
        assert_eq!(NORMAL_1_PROB, [70, 30]);
        assert_eq!(NORMAL_2_PROB, [15, 35, 35, 15]);
        assert_eq!(TRADE_PROB, [25, 25, 25, 25]);
        assert_eq!(NORMAL_3_PROB, [49, 17, 17, 17]);
        assert_eq!(GAME_PROB, [40, 60]);
        // Spot-check base tables against the brief's decoded ranges.
        assert_eq!(base::KI_NORMAL[0], 0x20C3);
        assert_eq!(base::KI_NORMAL[5], 0x0F8C);
        assert_eq!(base::REMOVE_YES[4], 0x2862);
        assert_eq!(base::EV_CAL[2], 0x27B2);
        assert_eq!(SEASON_ADD_TABLE, [10, 11, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9]);
    }

    #[test]
    fn prob_chooser_exact_weights() {
        // Deterministic RNG: always pick index 0 after "shuffling"
        // (swap i with i is identity here since we return fixed values).
        // Instead verify the built table: count occurrences per category.
        let mut counts = [0u32; 5];
        // Reproduce the build phase via a rigged chooser is complex;
        // verify distribution statistically with a simple LCG.
        let mut state = 0x12345678u32;
        let mut rng = |n: u32| {
            state = state.wrapping_mul(1103515245).wrapping_add(12345);
            (state >> 16) % n
        };
        for _ in 0..20000 {
            counts[decide_idx_prob_table(&KI_PROB, &mut rng)] += 1;
        }
        // Expect ~40/30/10/10/10 within tolerance.
        let pct = |c: u32| c as f64 / 200.0;
        assert!((pct(counts[0]) - 40.0).abs() < 2.0, "ki0={}", pct(counts[0]));
        assert!((pct(counts[1]) - 30.0).abs() < 2.0, "ki1={}", pct(counts[1]));
        assert!((pct(counts[2]) - 10.0).abs() < 2.0, "ki2={}", pct(counts[2]));
        assert!((pct(counts[3]) - 10.0).abs() < 2.0, "ki3={}", pct(counts[3]));
        assert!((pct(counts[4]) - 10.0).abs() < 2.0, "ki4={}", pct(counts[4]));
        // Degenerate: all weight on one category.
        let mut rng = |_: u32| 0u32;
        assert_eq!(decide_idx_prob_table(&[0, 100, 0], &mut rng), 1);
    }

    #[test]
    fn leaf_formulas() {
        assert_eq!(select_talk_kind(true, true), TalkKind::FjHint);
        assert_eq!(select_talk_kind(false, true), TalkKind::Ki);
        assert_eq!(select_talk_kind(false, false), TalkKind::Normal);
        // First-job hint: 0x0841 + hint_type + looks*10.
        assert_eq!(fj_hint_msg_no(3, 2), 0x0841 + 3 + 20);
        // Weather/time matrix.
        assert_eq!(weather_time_msg_no(0x1FE8, 2, 1, 1), 0x1FE8 + 12 + 2 + 1);
        // Normal-3 weather with/without the man-kind adjustment.
        assert_eq!(normal3_weather_msg_no(0x1FC1, 2, false, 3), 0x1FC1 + 10 + 3);
        assert_eq!(normal3_weather_msg_no(0x1FC1, 2, true, 3), 0x1FC1 + 10 + 3 + 1);
        // Season: month 1 -> add_table[0] = 10.
        assert_eq!(normal3_season_msg_no(0x1FD7, 1), 0x1FD7 + 10);
        assert_eq!(normal3_season_msg_no(0x1FD7, 12), 0x1FD7 + 9);
        // Letter: offset is the mail strategy, not a random range.
        assert_eq!(letter_msg_no(0x136C, 4), 0x136C + 4);
        // Memory provenance x letter flag.
        assert_eq!(memory_msg_no(0x1386, 2, true), 0x1386 + 4);
        assert_eq!(memory_msg_no(0x1386, 2, false), 0x1386 + 5);
        assert_eq!(memory_fallback_msg_no(0x1386), 0x1386 + 8);
        // KI money gate.
        assert!(ki_free_item_money_ok(true, 3000));
        assert!(!ki_free_item_money_ok(true, 2999));
        assert!(!ki_free_item_money_ok(false, 5000));
        assert_eq!(ki_fallback(), ki::NORMAL);
        // C ABI.
        assert_eq!(pc_select_talk_kind(0, 1), 1);
        assert_eq!(pc_fj_hint_msg_no(3, 2), 0x0841 + 3 + 20);
        assert_eq!(pc_weather_time_msg_no(0x1FE8, 2, 1, 1), 0x1FE8 + 15);
    }
}
