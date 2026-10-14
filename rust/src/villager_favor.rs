//! Villager favor (errand/delivery/contest) acceptance, completion and reward
//! kernels, verified against `m_quest.c` and `ac_quest_talk_init.c`
//! (GAFE01_00 Rev 0 USA decomp).
//!
//! Only the pure decision/table logic lives here. Stateful work stays in C:
//! quest-slot allocation writes into the player save (`priv->deliveries`,
//! `priv->errands`, the `l_mqst_grab` record), RNG draws (`RANDOM`, `fqrand`),
//! RTC reads, dialogue selection order, shop-item selection
//! (`mSP_SelectRandomItem_New`), worn-cloth fallback actor lookups, and the
//! actual bells/item grants.
//!
//! Time limits are computed on integer day numbers: the C side converts
//! `lbRTC_time_c` to a day count before calling. This keeps the kernel free of
//! the RTC struct layout while reproducing the decomp's comparisons exactly.
//!
//! ## Quirks preserved
//!
//! - `pc_favor_errand_reward`: `npc_num--` is done on a `u8`, so
//!   `used_num == 0` wraps to 255 and is then clamped to row 3.
//! - `pc_favor_errand_next`: a stored `next` value above 2 falls through both
//!   branches, leaving `next_type = 0` (stores 1).
//! - `pc_favor_limit_over`: the 28-day backstop (`interval >= 28`) is
//!   unreachable while the deadline lies in the future —
//!   `lbRTC_GetIntervalDays` saturates at 0 when the first date is later —
//!   and the per-kind branch fires when `now <= limit - table_days`, i.e. it
//!   can only trigger at or before acceptance for typical values. Both
//!   branches are reproduced verbatim rather than "fixed".
//! - `pc_favor_match_errand`: on a match the grab record's pocket index is
//!   overwritten with the *errand slot* index, not the pocket index.
//! - `pc_favor_reward_kind`: percentage rows summing under 100 leave the rest
//!   of the 100-slot table zeroed (`bzero`), biasing toward kind 0.

/// Quest types (`mQst_QUEST_TYPE_*`, `include/m_quest.h`).
pub const QUEST_TYPE_DELIVERY: u32 = 0;
pub const QUEST_TYPE_ERRAND: u32 = 1;
pub const QUEST_TYPE_CONTEST: u32 = 2;
pub const QUEST_TYPE_NONE: u32 = 3;

/// Errand request kinds (`mQst_ERRAND_REQUEST*`, `include/m_quest.h`).
pub const ERRAND_REQUEST: u8 = 0;
pub const ERRAND_REQUEST_CONTINUE: u8 = 1;
pub const ERRAND_REQUEST_FINAL: u8 = 2;

/// Letter ranks and bonuses (`mQst_LETTER_*`, `include/m_quest.h`).
pub const LETTER_RANK_MIN: u8 = 0;
pub const LETTER_RANK_MAX: u8 = 12; // LETTER_RANK_11 + 1
pub const LETTER_SCORE_BONUS: u8 = 3;
pub const LETTER_PRESENT_BONUS: u8 = 6;
pub const LETTER_OKAY_LENGTH: usize = 17;
pub const LETTER_GOOD_LENGTH: usize = 49;

/// Backstop in `mQst_CheckLimitOver` (`mQst_MAX_TIME_LIMIT_DAYS`).
pub const MAX_TIME_LIMIT_DAYS: i32 = 28;

/// Item id used for bell rewards (`ITM_MONEY_1000`, `include/m_name_table.h`).
pub const ITM_MONEY_1000: u16 = 0x2100;

/// Empty pocket slot (`EMPTY_NO`).
pub const EMPTY_NO: u16 = 0x0000;

/// Shop item kinds (`mSP_KIND_*`, `include/m_shop.h`).
pub const KIND_FURNITURE: u8 = 0;
pub const KIND_CLOTH: u8 = 2;
pub const KIND_CARPET: u8 = 3;
pub const KIND_WALLPAPER: u8 = 4;

/// Shop list types (`mSP_LISTTYPE_*`, `include/m_shop.h`).
pub const LISTTYPE_RARE: u8 = 2;
pub const LISTTYPE_ABC: u8 = 8;

/// Reward kinds (`aQMgr_QUEST_REWARD_*`, `include/ac_quest_manager.h`).
pub const REWARD_FTR: u8 = 0;
pub const REWARD_STATIONERY: u8 = 1;
pub const REWARD_CLOTH: u8 = 2;
pub const REWARD_CARPET: u8 = 3;
pub const REWARD_WALLPAPER: u8 = 4;
pub const REWARD_MONEY: u8 = 5;
pub const REWARD_WORN_CLOTH: u8 = 6;
pub const REWARD_NUM: usize = 8;

/// Per-kind day limits, verbatim (`l_delivery_limit`, `l_errand_limit`,
/// `l_contest_limit` in `m_quest.c`).
pub const DELIVERY_LIMIT: [i32; 4] = [2, 2, 2, 2];
pub const ERRAND_LIMIT: [i32; 15] = [2, 2, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
/// Indexed by `mQst_CONTEST_KIND_*`: FRUIT, SOCCER, SNOWMAN, FLOWER, FISH,
/// INSECT, LETTER.
pub const CONTEST_LIMIT: [i32; 7] = [1, 1, 3, 3, 3, 3, 2];
pub const CONTEST_FIN_LIMIT: [i32; 7] = [3, 3, 3, 3, 3, 3, 2];

/// Errand reward percentage rows and base pays, verbatim
/// (`aQMgr_actor_get_errand_reward` in `ac_quest_talk_init.c`).
/// Rows are chosen by chain `used_num - 1` (see quirk above).
pub const ERRAND_REWARD_PCT: [[u8; REWARD_NUM]; 4] = [
    [0, 75, 25, 0, 0, 0, 0, 0], // 75% stationery, 25% clothing
    [25, 25, 25, 0, 0, 25, 0, 0], // 25% ftr/stationery/clothing/money
    [50, 0, 25, 0, 0, 25, 0, 0], // 50% ftr, 25% clothing, 25% money
    [65, 0, 0, 5, 5, 25, 0, 0], // 65% ftr, 5% carpet, 5% wallpaper, 25% money
];
pub const ERRAND_REWARD_PAY: [u32; 4] = [0, 500, 750, 1000];

/// Bell-pay rate table, verbatim (`aQMgr_actor_get_rate`).
/// Each row is (spread, base); selected by `money_power / 100`, clamped.
pub const PAY_RATE_TABLE: [(f32, f32); 7] = [
    (0.3000, 0.0),
    (0.7000, -40.0),
    (2.7000, -440.0),
    (1.3000, -20.0),
    (1.0000, 100.0),
    (0.6000, 300.0),
    (0.0012, 660.0),
];

/// Result of [`pc_favor_errand_next`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FavorNext {
    /// 0 or 1: random on first ask, otherwise `stored - 1`.
    pub next_type: u8,
    /// `progress - 1` when `next_type != 0`, else 0.
    pub stage: u32,
    /// `ERRAND_REQUEST_FINAL` when stage is 0, else `ERRAND_REQUEST_CONTINUE`.
    pub kind: u8,
    /// Value written back to the per-villager `next` slot (`next_type + 1`).
    pub store: u8,
}

/// What [`pc_favor_present_pick`] selects for a letter rank.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FavorPresent {
    /// Pick via the shop random-item selector with this kind/list.
    ShopPick { kind: u8, list: u8 },
    /// The town's native fruit (`Save_Get(fruit)` — C side).
    NativeFruit,
    /// A non-native fruit (`mFI_GetOtherFruit()` — C side).
    OtherFruit,
    /// Ranks 0–2: no present.
    None,
}

/// `mQst_CheckFreeQuest`: a quest slot is free when its type is NONE.
///
/// # C reference
/// `m_quest.c`
pub fn pc_favor_free_quest(quest_type: u32) -> bool {
    quest_type == QUEST_TYPE_NONE
}

/// First delivery slot whose quest is free *and* whose pocket is empty.
///
/// # C reference
/// `aQMgr_actor_get_free_quest_p`, `mQst_QUEST_TYPE_DELIVERY` case
/// (`ac_quest_talk_init.c`)
pub fn pc_favor_find_delivery_slot(quest_types: &[u32], pocket_items: &[u16]) -> Option<usize> {
    quest_types
        .iter()
        .zip(pocket_items.iter())
        .position(|(&qt, &item)| pc_favor_free_quest(qt) && item == EMPTY_NO)
}

/// First free errand slot.
///
/// # C reference
/// `aQMgr_actor_get_free_quest_p`, `mQst_QUEST_TYPE_ERRAND` case
/// (`ac_quest_talk_init.c`)
pub fn pc_favor_find_errand_slot(quest_types: &[u32]) -> Option<usize> {
    quest_types.iter().position(|&qt| pc_favor_free_quest(qt))
}

/// Chain-errand next-kind step (`aQMgr_actor_new_quest`, errand path).
///
/// `stored_next` is the villager's saved `next` value (0 = unset),
/// `progress` the current quest progress, `rng_0_1` the C-side `RANDOM(2)`
/// result used when `stored_next == 0`.
pub fn pc_favor_errand_next(stored_next: u8, progress: u32, rng_0_1: u32) -> FavorNext {
    let next_type: u8 = if stored_next == 0 {
        (rng_0_1 & 1) as u8
    } else if stored_next <= 2 {
        stored_next - 1
    } else {
        0 // quirk: values above 2 fall through, keeping the init 0
    };
    let stage: u32 = if next_type == 0 {
        0
    } else {
        progress.wrapping_sub(1)
    };
    let kind = if stage == 0 {
        ERRAND_REQUEST_FINAL
    } else {
        ERRAND_REQUEST_CONTINUE
    };
    FavorNext {
        next_type,
        stage,
        kind,
        store: next_type + 1,
    }
}

/// Per-kind day limit from the limit tables, plus the contest finish-limit
/// extension when `quest_type == CONTEST && progress == 0`.
///
/// Returns `None` when `quest_kind` is outside the table.
///
/// # C reference
/// `mQst_CheckLimitOver` table lookup (`m_quest.c`)
pub fn pc_favor_limit_days(quest_type: u32, quest_kind: u32, progress: u32) -> Option<i32> {
    let k = quest_kind as usize;
    let days = match quest_type {
        QUEST_TYPE_DELIVERY => DELIVERY_LIMIT.get(k).copied(),
        QUEST_TYPE_ERRAND => ERRAND_LIMIT.get(k).copied(),
        QUEST_TYPE_CONTEST => CONTEST_LIMIT.get(k).copied(),
        _ => None,
    }?;
    if quest_type == QUEST_TYPE_CONTEST && progress == 0 {
        Some(days + CONTEST_FIN_LIMIT[k])
    } else {
        Some(days)
    }
}

/// Full time-limit decision (`mQst_CheckLimitOver`).
///
/// `limit_day` / `now_day` are integer day numbers (C converts the RTC
/// structs). `lbRTC_GetIntervalDays` saturates at 0 when the limit lies in
/// the future, reproduced here with `saturating_sub`.
pub fn pc_favor_limit_over(
    enabled: bool,
    limit_day: i32,
    now_day: i32,
    quest_type: u32,
    quest_kind: u32,
    progress: u32,
) -> bool {
    if !enabled {
        return false;
    }
    // lbRTC_IsOverTime(&limit, &now) == OVER  <=>  now >= limit
    if now_day >= limit_day {
        return true;
    }
    if quest_type < QUEST_TYPE_NONE {
        // Saturating interval; unreachable while the deadline is in the future.
        if now_day.saturating_sub(limit_day) >= MAX_TIME_LIMIT_DAYS {
            return true;
        }
        if let Some(days) = pc_favor_limit_days(quest_type, quest_kind, progress) {
            // lbRTC_Sub_DD(&temp, days); IsOverTime(&now, &temp) == OVER
            //   <=>  temp >= now
            if limit_day.saturating_sub(days) >= now_day {
                return true;
            }
        }
    }
    false
}

/// Find the errand matching a grabbed pocket item (`mQst_CheckGrabItem`).
///
/// Returns the errand slot index of the first active errand whose recorded
/// pocket and item match. The C side then overwrites the grab record's pocket
/// index with this slot index (see quirk note above).
pub fn pc_favor_match_errand(
    quest_types: &[u32],
    pockets_idx: &[i8],
    items: &[u16],
    pocket_idx: i32,
    item: u16,
) -> Option<usize> {
    quest_types
        .iter()
        .zip(pockets_idx.iter())
        .zip(items.iter())
        .position(|((&qt, &pi), &it)| {
            !pc_favor_free_quest(qt) && pi as i32 == pocket_idx && it == item
        })
}

/// Letter rank from body length, scorer bonus and present
/// (`mQst_GetMailRank`).
///
/// `score_ok` = the NPC mail scorer met its OK threshold
/// (`mNpc_CheckNormalMail_length` bonus path — C side).
pub fn pc_favor_mail_rank(body_len: usize, score_ok: bool, has_present: bool) -> u8 {
    let mut rank = LETTER_RANK_MIN;
    if body_len >= LETTER_GOOD_LENGTH {
        rank = 2;
    } else if body_len >= LETTER_OKAY_LENGTH {
        rank = 1;
    }
    if score_ok {
        rank += LETTER_SCORE_BONUS;
    }
    if has_present {
        rank += LETTER_PRESENT_BONUS;
    }
    rank
}

/// Present category for a letter rank (`mQst_GetPresent`).
///
/// `rng_coin` is the C-side `mQst_GetRandom(4)` result; only its low bit is
/// used (carpet vs wallpaper on ranks 5 and 11).
pub fn pc_favor_present_pick(rank: u8, rng_coin: u32) -> FavorPresent {
    let carpet_or_wallpaper = if rng_coin & 1 == 0 {
        KIND_CARPET
    } else {
        KIND_WALLPAPER
    };
    match rank {
        3 | 7 => FavorPresent::ShopPick {
            kind: KIND_CLOTH,
            list: LISTTYPE_ABC,
        },
        4 => FavorPresent::ShopPick {
            kind: KIND_FURNITURE,
            list: LISTTYPE_ABC,
        },
        5 => FavorPresent::ShopPick {
            kind: carpet_or_wallpaper,
            list: LISTTYPE_ABC,
        },
        6 => FavorPresent::NativeFruit,
        8 => FavorPresent::ShopPick {
            kind: KIND_CLOTH,
            list: LISTTYPE_RARE,
        },
        9 => FavorPresent::OtherFruit,
        10 => FavorPresent::ShopPick {
            kind: KIND_FURNITURE,
            list: LISTTYPE_RARE,
        },
        11 => FavorPresent::ShopPick {
            kind: carpet_or_wallpaper,
            list: LISTTYPE_RARE,
        },
        _ => FavorPresent::None, // ranks 0-2: item stays EMPTY_NO
    }
}

/// Errand reward percentages and base pay from the chain length
/// (`aQMgr_actor_get_errand_reward`).
///
/// `npc_used_num` is the chain `used_num`; the `u8` wrapping quirk is
/// preserved: 0 wraps to 255 and clamps to row 3.
pub fn pc_favor_errand_reward(npc_used_num: u8) -> ([u8; REWARD_NUM], u32) {
    let mut n = npc_used_num.wrapping_sub(1);
    if n as usize >= ERRAND_REWARD_PAY.len() {
        n = ERRAND_REWARD_PAY.len() as u8 - 1;
    }
    (ERRAND_REWARD_PCT[n as usize], ERRAND_REWARD_PAY[n as usize])
}

/// Reward-kind roll (`aQMgr_actor_set_reward`).
///
/// Builds the 100-slot table from the 8 percentage entries (rows summing
/// under 100 leave zeroed slots) and returns the entry at `rng_100`, the
/// C-side `RANDOM(100)` result.
pub fn pc_favor_reward_kind(reward_percentages: &[u8; REWARD_NUM], rng_100: u32) -> u8 {
    let mut prob_tbl = [0u8; 100];
    let mut idx = 0usize;
    for (i, &pct) in reward_percentages.iter().enumerate() {
        let mut j = pct;
        while j != 0 {
            if idx >= 100 {
                break;
            }
            prob_tbl[idx] = i as u8;
            idx += 1;
            j -= 1;
        }
    }
    prob_tbl[(rng_100 % 100) as usize]
}

/// Bell-pay rate from money power (`aQMgr_actor_get_rate`).
pub fn pc_favor_pay_rate(money_power: i32) -> u32 {
    if money_power <= 0 {
        return 0;
    }
    let mut idx = (money_power / 100) as usize;
    if idx >= PAY_RATE_TABLE.len() {
        idx = PAY_RATE_TABLE.len() - 1;
    }
    let (spread, base) = PAY_RATE_TABLE[idx];
    (base + spread * money_power as f32) as u32
}

/// Final bell payout (`aQMgr_actor_get_pay`).
///
/// `dir_sign` is +1.0/-1.0 from the C-side `RANDOM(2)`, `percent` the C-side
/// `fqrand()`, and `pay_rate` the (possibly 700-clamped)
/// `pc_favor_pay_rate` result. The f32 arithmetic order matches the decomp.
pub fn pc_favor_pay_amount(base_pay: u32, dir_sign: f32, percent: f32, pay_rate: u32) -> u32 {
    let scale = 100.0f32 + dir_sign * 10.0 * percent;
    let mut pay = pay_rate;
    if pay > 700 {
        pay = 700;
    }
    let pay_f = 100.0f32 + pay as f32;
    let final_pay = (scale * pay_f) / 10000.0f32;
    (base_pay as f32 * final_pay) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_quest_only_when_none() {
        assert!(pc_favor_free_quest(QUEST_TYPE_NONE));
        assert!(!pc_favor_free_quest(QUEST_TYPE_DELIVERY));
        assert!(!pc_favor_free_quest(QUEST_TYPE_ERRAND));
        assert!(!pc_favor_free_quest(QUEST_TYPE_CONTEST));
    }

    #[test]
    fn delivery_slot_needs_free_quest_and_empty_pocket() {
        // slot 0: quest active; slot 1: free quest but occupied pocket; slot 2: both free
        let quests = [QUEST_TYPE_DELIVERY, QUEST_TYPE_NONE, QUEST_TYPE_NONE];
        let pockets = [0x1234u16, 0x5678, EMPTY_NO];
        assert_eq!(pc_favor_find_delivery_slot(&quests, &pockets), Some(2));
        let full = [QUEST_TYPE_DELIVERY, QUEST_TYPE_ERRAND, QUEST_TYPE_CONTEST];
        assert_eq!(pc_favor_find_delivery_slot(&full, &pockets), None);
    }

    #[test]
    fn errand_slot_first_free() {
        let quests = [QUEST_TYPE_ERRAND, QUEST_TYPE_NONE, QUEST_TYPE_DELIVERY];
        assert_eq!(pc_favor_find_errand_slot(&quests), Some(1));
        assert_eq!(
            pc_favor_find_errand_slot(&[QUEST_TYPE_ERRAND, QUEST_TYPE_CONTEST]),
            None
        );
    }

    #[test]
    fn errand_next_chain() {
        // unset -> random bit 1 -> next_type 1, stage = progress-1, CONTINUE, store 2
        let r = pc_favor_errand_next(0, 5, 1);
        assert_eq!(
            r,
            FavorNext {
                next_type: 1,
                stage: 4,
                kind: ERRAND_REQUEST_CONTINUE,
                store: 2
            }
        );
        // stored 2 -> next_type 1 ... wait: stored 1 -> next_type 0 -> FINAL
        let r = pc_favor_errand_next(1, 5, 0);
        assert_eq!(r.kind, ERRAND_REQUEST_FINAL);
        assert_eq!(r.stage, 0);
        assert_eq!(r.store, 1);
        // stored 2 -> next_type 1 -> CONTINUE with stage progress-1
        let r = pc_favor_errand_next(2, 3, 0);
        assert_eq!(r.next_type, 1);
        assert_eq!(r.stage, 2);
        assert_eq!(r.kind, ERRAND_REQUEST_CONTINUE);
        // quirk: stored > 2 falls through to next_type 0
        let r = pc_favor_errand_next(9, 3, 1);
        assert_eq!(r.next_type, 0);
        assert_eq!(r.store, 1);
        assert_eq!(r.kind, ERRAND_REQUEST_FINAL);
    }

    #[test]
    fn limit_tables_verbatim() {
        assert_eq!(DELIVERY_LIMIT, [2, 2, 2, 2]);
        assert_eq!(ERRAND_LIMIT[0], 2);
        assert_eq!(ERRAND_LIMIT[3], 0);
        assert_eq!(CONTEST_LIMIT, [1, 1, 3, 3, 3, 3, 2]);
        assert_eq!(CONTEST_FIN_LIMIT, [3, 3, 3, 3, 3, 3, 2]);
        // contest with progress 0 extends by the finish limit
        assert_eq!(
            pc_favor_limit_days(QUEST_TYPE_CONTEST, 3, 0),
            Some(3 + 3) // FLOWER + fin
        );
        assert_eq!(pc_favor_limit_days(QUEST_TYPE_CONTEST, 3, 1), Some(3));
        assert_eq!(pc_favor_limit_days(QUEST_TYPE_DELIVERY, 0, 0), Some(2));
        assert_eq!(pc_favor_limit_days(QUEST_TYPE_DELIVERY, 9, 0), None); // kind >= max 4
        assert_eq!(pc_favor_limit_days(QUEST_TYPE_ERRAND, 14, 0), Some(0));
        assert_eq!(pc_favor_limit_days(QUEST_TYPE_ERRAND, 15, 0), None); // kind >= max 15
    }

    #[test]
    fn limit_over_branches() {
        // disabled -> never over
        assert!(!pc_favor_limit_over(false, 100, 200, QUEST_TYPE_DELIVERY, 0, 0));
        // now past the deadline -> over
        assert!(pc_favor_limit_over(true, 100, 100, QUEST_TYPE_DELIVERY, 0, 0));
        assert!(pc_favor_limit_over(true, 100, 150, QUEST_TYPE_DELIVERY, 0, 0));
        // before the deadline, type NONE -> not over
        assert!(!pc_favor_limit_over(true, 100, 50, QUEST_TYPE_NONE, 0, 0));
        // quirk branch: now <= limit - table_days -> over (fires at/before acceptance)
        // delivery kind 0: days=2, limit=100, now=98 -> 100-2=98 >= 98 -> true
        assert!(pc_favor_limit_over(true, 100, 98, QUEST_TYPE_DELIVERY, 0, 0));
        // now=99 -> 98 >= 99 false -> not over
        assert!(!pc_favor_limit_over(true, 100, 99, QUEST_TYPE_DELIVERY, 0, 0));
    }

    #[test]
    fn match_errand_first_hit() {
        let quests = [QUEST_TYPE_NONE, QUEST_TYPE_ERRAND, QUEST_TYPE_ERRAND];
        let pidx = [0i8, 3, 3];
        let items = [0u16, 0x2100, 0x2100];
        // pocket 3, item 0x2100 -> first matching errand slot 1
        assert_eq!(pc_favor_match_errand(&quests, &pidx, &items, 3, 0x2100), Some(1));
        // wrong item -> none
        assert_eq!(pc_favor_match_errand(&quests, &pidx, &items, 3, 0x9999), None);
        // free quest slot never matches even with same pocket/item
        let quests2 = [QUEST_TYPE_NONE, QUEST_TYPE_NONE, QUEST_TYPE_ERRAND];
        assert_eq!(
            pc_favor_match_errand(&quests2, &pidx, &items, 3, 0x2100),
            Some(2)
        );
    }

    #[test]
    fn mail_rank_arithmetic() {
        assert_eq!(pc_favor_mail_rank(0, false, false), 0);
        assert_eq!(pc_favor_mail_rank(16, false, false), 0);
        assert_eq!(pc_favor_mail_rank(17, false, false), 1);
        assert_eq!(pc_favor_mail_rank(48, false, false), 1);
        assert_eq!(pc_favor_mail_rank(49, false, false), 2);
        assert_eq!(pc_favor_mail_rank(49, true, false), 5); // +3 score bonus
        assert_eq!(pc_favor_mail_rank(49, true, true), 11); // +6 present bonus
        assert_eq!(pc_favor_mail_rank(10, false, true), 6);
    }

    #[test]
    fn present_pick_table() {
        use FavorPresent::*;
        assert_eq!(
            pc_favor_present_pick(3, 0),
            ShopPick { kind: KIND_CLOTH, list: LISTTYPE_ABC }
        );
        assert_eq!(
            pc_favor_present_pick(7, 0),
            ShopPick { kind: KIND_CLOTH, list: LISTTYPE_ABC }
        );
        assert_eq!(
            pc_favor_present_pick(4, 0),
            ShopPick { kind: KIND_FURNITURE, list: LISTTYPE_ABC }
        );
        // rank 5: coin bit 0 -> carpet, 1 -> wallpaper
        assert_eq!(
            pc_favor_present_pick(5, 0),
            ShopPick { kind: KIND_CARPET, list: LISTTYPE_ABC }
        );
        assert_eq!(
            pc_favor_present_pick(5, 3),
            ShopPick { kind: KIND_WALLPAPER, list: LISTTYPE_ABC }
        );
        assert_eq!(pc_favor_present_pick(6, 0), NativeFruit);
        assert_eq!(
            pc_favor_present_pick(8, 0),
            ShopPick { kind: KIND_CLOTH, list: LISTTYPE_RARE }
        );
        assert_eq!(pc_favor_present_pick(9, 0), OtherFruit);
        assert_eq!(
            pc_favor_present_pick(10, 0),
            ShopPick { kind: KIND_FURNITURE, list: LISTTYPE_RARE }
        );
        assert_eq!(
            pc_favor_present_pick(11, 2),
            ShopPick { kind: KIND_CARPET, list: LISTTYPE_RARE }
        );
        assert_eq!(pc_favor_present_pick(0, 0), None);
        assert_eq!(pc_favor_present_pick(2, 0), None);
        assert_eq!(pc_favor_present_pick(12, 0), None);
    }

    #[test]
    fn errand_reward_rows_and_wrap_quirk() {
        // used_num=1 -> npc_num=0 -> row 0, pay 0
        assert_eq!(
            pc_favor_errand_reward(1),
            ([0, 75, 25, 0, 0, 0, 0, 0], 0)
        );
        // used_num=2 -> row 1, pay 500
        assert_eq!(
            pc_favor_errand_reward(2),
            ([25, 25, 25, 0, 0, 25, 0, 0], 500)
        );
        // used_num=4 -> row 3, pay 1000
        assert_eq!(
            pc_favor_errand_reward(4),
            ([65, 0, 0, 5, 5, 25, 0, 0], 1000)
        );
        // used_num=9 -> 8 >= 4 -> clamped to row 3
        assert_eq!(pc_favor_errand_reward(9).1, 1000);
        // quirk: used_num=0 wraps to 255 -> clamped to row 3
        assert_eq!(
            pc_favor_errand_reward(0),
            ([65, 0, 0, 5, 5, 25, 0, 0], 1000)
        );
    }

    #[test]
    fn reward_kind_table_build() {
        // 75% stationery / 25% cloth row: slots 0-74 -> 1, 75-99 -> 2
        let pct = [0u8, 75, 25, 0, 0, 0, 0, 0];
        assert_eq!(pc_favor_reward_kind(&pct, 0), REWARD_STATIONERY);
        assert_eq!(pc_favor_reward_kind(&pct, 74), REWARD_STATIONERY);
        assert_eq!(pc_favor_reward_kind(&pct, 75), REWARD_CLOTH);
        assert_eq!(pc_favor_reward_kind(&pct, 99), REWARD_CLOTH);
        // rows summing under 100 leave zeroed tail -> kind 0
        let pct2 = [10u8, 0, 0, 0, 0, 0, 0, 0];
        assert_eq!(pc_favor_reward_kind(&pct2, 50), REWARD_FTR);
        // rng wraps mod 100 like the table index would
        assert_eq!(pc_favor_reward_kind(&pct, 100), REWARD_STATIONERY);
    }

    #[test]
    fn pay_rate_table() {
        assert_eq!(pc_favor_pay_rate(0), 0);
        assert_eq!(pc_favor_pay_rate(-5), 0);
        // idx 0: 0.0 + 0.3*29 = 8.7 -> 8
        assert_eq!(pc_favor_pay_rate(29), 8);
        // idx 0: 0.0 + 0.3*50 = 15.0 -> 15
        assert_eq!(pc_favor_pay_rate(50), 15);
        // idx 1: -40.0 + 0.7*150 = 65.0 -> 65
        assert_eq!(pc_favor_pay_rate(150), 65);
        // idx 6 clamp: 660 + 0.0012*9999 = 671.9988 -> 671
        assert_eq!(pc_favor_pay_rate(9999), 671);
    }

    #[test]
    fn pay_amount_f32_order() {
        // base 500, dir +1, percent 0.5, rate 100:
        // scale = 100 + 1*10*0.5 = 105; pay_f = 200;
        // final = 105*200/10000 = 2.1; result = 500*2.1 = 1050
        assert_eq!(pc_favor_pay_amount(500, 1.0, 0.5, 100), 1050);
        // rate clamped at 700: scale=100, pay_f=800 -> 8.0; 500*8=4000
        assert_eq!(pc_favor_pay_amount(500, 1.0, 0.0, 900), 4000);
        // dir -1, percent 1.0: scale = 90; pay_f=100 -> 0.9; 1000*0.9=900
        assert_eq!(pc_favor_pay_amount(1000, -1.0, 1.0, 0), 900);
    }
}
