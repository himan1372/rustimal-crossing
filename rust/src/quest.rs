//! Quest system: request flow, completion, rewards.
//!
//! Verified against `include/m_quest.h` (mQst_base_c, delivery/errand/
//! contest structs, kind enums), `src/game/m_quest.c` (timeout tables,
//! mQst_CheckLimitOver), `include/ac_quest_manager.h`
//! (aQMgr_REGIST_NUM=35, reward enum), `src/actor/ac_quest_manager.c`
//! (l_set_delivery_data, l_set_contest_data, registration rebuild),
//! `src/actor/ac_quest_talk_init.c` (aQMgr_actor_get_errand_reward,
//! aQMgr_actor_set_reward prob_tbl, aQMgr_actor_get_pay, give_reward)
//! (USA Rev. 0 decomp / PC port).
//!
//! Architecture: m_quest.c owns persistent quest state + generic
//! helpers; ac_quest_manager.c owns quest definitions, the 35-slot
//! runtime registration table (rebuilt every check cycle from the
//! persistent records), and periodic checks; ac_quest_talk_init.c owns
//! the request/completion/reward transaction.
//!
//! CORRECTION vs brief: the errand reward pay table is
//! reward_pay = {0, 500, 750, 1000} indexed by (used_num-1) clamped to
//! 0..3. So used_num=1 -> 0 bells (not 500 as the brief stated),
//! used_num=2 -> 500, used_num=3 -> 750, used_num>=4 -> 1000.

/// Quest types (mQst_QUEST_TYPE_*).
pub mod qtype {
    pub const DELIVERY: u8 = 0;
    pub const ERRAND: u8 = 1;
    pub const CONTEST: u8 = 2;
    pub const NONE: u8 = 3;
}

/// Delivery kinds.
pub mod dkind {
    pub const NORMAL: u8 = 0;
    pub const FOREIGN: u8 = 1;
    pub const REMOVE: u8 = 2;
    pub const LOST: u8 = 3;
}

/// Contest kinds.
pub mod ckind {
    pub const FRUIT: u8 = 0;
    pub const SOCCER: u8 = 1;
    pub const SNOWMAN: u8 = 2;
    pub const FLOWER: u8 = 3;
    pub const FISH: u8 = 4;
    pub const INSECT: u8 = 5;
    pub const LETTER: u8 = 6;
}

/// Errand kinds (request chain + first-job).
pub mod ekind {
    pub const REQUEST: u8 = 0;
    pub const REQUEST_CONTINUE: u8 = 1;
    pub const REQUEST_FINAL: u8 = 2;
}

/// Reward categories (aQMgr_QUEST_REWARD_*).
pub mod reward {
    pub const FTR: u8 = 0;
    pub const STATIONERY: u8 = 1;
    pub const CLOTH: u8 = 2;
    pub const CARPET: u8 = 3;
    pub const WALLPAPER: u8 = 4;
    pub const MONEY: u8 = 5;
    pub const WORN_CLOTH: u8 = 6;
    pub const NUM: u8 = 8;
}

/// Runtime registration slots.
pub const REGIST_NUM: usize = 35;

/// mQst_base_c: 12 bytes. quest_type:2, quest_kind:6,
/// time_limit_enabled:1, progress:4, give_reward:1, unused:2,
//  + 10-byte RTC time_limit.
#[derive(Clone, Copy, Default)]
pub struct QuestBase {
    pub quest_type: u8,       // 2 bits
    pub quest_kind: u8,       // 6 bits
    pub time_limit_enabled: bool,
    pub progress: u8,         // 4 bits; meaning is kind-dependent
    pub give_reward: bool,    // reward could not be delivered; retry later
}

/// Generic completion rule: progress == 0 means complete (delivery and
/// ordinary errands; contests use dedicated checks).
pub fn is_complete(base: &QuestBase) -> bool {
    base.progress == 0
}

/// A quest is free when quest_type == NONE.
pub fn is_free(base: &QuestBase) -> bool {
    base.quest_type == qtype::NONE
}

/// Delivery reward percentages: [FTR, STATIONERY, CLOTH, CARPET,
/// WALLPAPER, MONEY, WORN_CLOTH, _] per delivery kind, plus max pay.
/// From l_set_delivery_data.
pub const DELIVERY_REWARDS: [[u8; 8]; 4] = [
    [40, 0, 0, 0, 0, 30, 30, 0],   // NORMAL
    [40, 0, 0, 10, 10, 40, 0, 0],  // FOREIGN
    [20, 0, 0, 20, 20, 40, 0, 0],  // REMOVE
    [40, 0, 40, 10, 10, 0, 0, 0],  // LOST
];
pub const DELIVERY_MAX_PAY: [u32; 4] = [200, 1000, 1000, 0];

/// Contest reward percentages per contest kind, plus max pay.
/// From l_set_contest_data.
pub const CONTEST_REWARDS: [[u8; 8]; 7] = [
    [0, 0, 0, 30, 30, 40, 0, 0],  // FRUIT
    [40, 0, 0, 30, 30, 0, 0, 0],  // SOCCER
    [60, 0, 0, 20, 20, 0, 0, 0],  // SNOWMAN
    [60, 0, 0, 20, 20, 0, 0, 0],  // FLOWER
    [80, 0, 0, 10, 10, 0, 0, 0],  // FISH
    [80, 0, 0, 10, 10, 0, 0, 0],  // INSECT
    [80, 0, 0, 10, 10, 0, 0, 0],  // LETTER
];
pub const CONTEST_MAX_PAY: [u32; 7] = [500, 0, 0, 0, 0, 0, 0];

/// Errand reward percentages by chain tier (used_num clamped to 1..4),
/// plus base pay. From aQMgr_actor_get_errand_reward.
/// NOTE: tier 1 (used_num=1) pays 0, not 500.
pub const ERRAND_REWARDS: [[u8; 8]; 4] = [
    [0, 75, 25, 0, 0, 0, 0, 0],   // used_num=1
    [25, 25, 25, 0, 0, 25, 0, 0], // used_num=2
    [50, 0, 25, 0, 0, 25, 0, 0],  // used_num=3
    [65, 0, 0, 5, 5, 25, 0, 0],   // used_num>=4
];
pub const ERRAND_PAY: [u32; 4] = [0, 500, 750, 1000];

/// Errand reward tier from chain used_num: (used_num-1) clamped 0..3.
pub fn errand_tier(used_num: u8) -> usize {
    let t = used_num.saturating_sub(1) as usize;
    if t > 3 { 3 } else { t }
}

/// Build the 100-slot 1%-granularity probability table from 8
/// percentages, then select with a 0..99 roll. Mirrors
/// aQMgr_actor_set_reward's prob_tbl construction.
pub fn prob_table_select(percentages: &[u8; 8], roll: u8) -> u8 {
    let mut tbl = [0u8; 100];
    let mut idx = 0usize;
    for (kind, &pct) in percentages.iter().enumerate() {
        for _ in 0..pct {
            if idx >= 100 {
                break;
            }
            tbl[idx] = kind as u8;
            idx += 1;
        }
    }
    tbl[(roll % 100) as usize]
}

/// Timeout day tables (m_quest.c).
pub const DELIVERY_LIMIT_DAYS: [i32; 4] = [2, 2, 2, 2];
pub const ERRAND_LIMIT_DAYS: [i32; 15] =
    [2, 2, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
pub const CONTEST_LIMIT_DAYS: [i32; 7] = [1, 1, 1, 3, 3, 3, 2];
/// Extra days added for contests whose progress == 0.
pub const CONTEST_FIN_LIMIT_DAYS: [i32; 7] = [3, 3, 3, 3, 3, 3, 2];
pub const MAX_TIME_LIMIT_DAYS: i32 = 28;

/// Kind-specific day limit for a quest (None if kind out of range).
pub fn limit_days(quest_type: u8, quest_kind: u8, progress: u8) -> Option<i32> {
    let days = match quest_type {
        qtype::DELIVERY => DELIVERY_LIMIT_DAYS.get(quest_kind as usize).copied(),
        qtype::ERRAND => ERRAND_LIMIT_DAYS.get(quest_kind as usize).copied(),
        qtype::CONTEST => CONTEST_LIMIT_DAYS.get(quest_kind as usize).copied(),
        _ => None,
    }?;
    if quest_type == qtype::CONTEST && progress == 0 {
        Some(days + CONTEST_FIN_LIMIT_DAYS[quest_kind as usize])
    } else {
        Some(days)
    }
}

/// Contest completion: (progress, player_qualified) -> complete.
/// Per-kind rules from ac_quest_contest.c_inc:
/// - fruit: progress == 1 AND requested item possessed
/// - soccer: progress == 1
/// - snowman: progress == 1 AND contest.player_id set AND == current player
/// - letter: progress == 1
/// - flower: flower goal met and player-ID qualification
/// - fish/insect: progress == 1 AND player_id null AND owns category item
///   (category-based, NOT exact requested-item match).
///
/// CORRECTION vs the earlier port: fruit and snowman were previously lumped
/// with soccer/letter as "progress == 1"; retail requires the extra
/// conditions above.
pub struct ContestQual {
    /// Fruit: the requested item is in the player's pockets.
    pub item_possessed: bool,
    /// Snowman: contest.player_id is set.
    pub player_id_set: bool,
    /// Snowman: contest.player_id == current player.
    pub player_id_current: bool,
    /// Flower: flower goal met with player-ID qualification.
    pub flower_ok: bool,
    /// Fish/insect: contest.player_id is null.
    pub player_id_absent: bool,
    /// Fish/insect: player owns an item of the contest category.
    pub owns_category_item: bool,
}

impl ContestQual {
    pub fn none() -> Self {
        ContestQual {
            item_possessed: false,
            player_id_set: false,
            player_id_current: false,
            flower_ok: false,
            player_id_absent: false,
            owns_category_item: false,
        }
    }
}

/// Bit flags for [`ContestQual`] used by the C ABI.
pub mod cqual {
    pub const ITEM_POSSESSED: u8 = 0x01;
    pub const PLAYER_ID_SET: u8 = 0x02;
    pub const PLAYER_ID_CURRENT: u8 = 0x04;
    pub const FLOWER_OK: u8 = 0x08;
    pub const PLAYER_ID_ABSENT: u8 = 0x10;
    pub const OWNS_CATEGORY_ITEM: u8 = 0x20;
}

pub fn contest_qual_from_flags(flags: u8) -> ContestQual {
    ContestQual {
        item_possessed: flags & cqual::ITEM_POSSESSED != 0,
        player_id_set: flags & cqual::PLAYER_ID_SET != 0,
        player_id_current: flags & cqual::PLAYER_ID_CURRENT != 0,
        flower_ok: flags & cqual::FLOWER_OK != 0,
        player_id_absent: flags & cqual::PLAYER_ID_ABSENT != 0,
        owns_category_item: flags & cqual::OWNS_CATEGORY_ITEM != 0,
    }
}

pub fn contest_complete(kind: u8, progress: u8, q: &ContestQual) -> bool {
    if progress != 1 {
        return false;
    }
    match kind {
        ckind::FRUIT => q.item_possessed,
        ckind::SOCCER => true,
        ckind::SNOWMAN => q.player_id_set && q.player_id_current,
        ckind::LETTER => true,
        ckind::FLOWER => q.flower_ok,
        ckind::FISH | ckind::INSECT => q.player_id_absent && q.owns_category_item,
        _ => false,
    }
}

/// Money reward scaling: base_pay * (scale * (100 + rate)) / 10000,
/// scale = 100 +/- up to 10 (random sign * fqrand fraction),
/// rate from money_power/100 bucket clamped to 700.
/// dir: 0 = +, 1 = -; frac: 0.0..1.0; money_power: player's.
pub fn scaled_pay(base_pay: u32, dir: u8, frac: f32, money_power: u32) -> u32 {
    let sign = if dir == 0 { 1.0 } else { -1.0 };
    let scale = 100.0 + sign * 10.0 * frac;
    let mut rate = money_power / 100;
    if rate > 700 {
        rate = 700;
    }
    let pay_f = 100.0 + rate as f32;
    (base_pay as f32 * (scale * pay_f) / 10000.0) as u32
}

/// Friendship effects for quest dialogue outcomes.
pub mod friendship {
    pub const REJECT_QUEST: i32 = -3;
    pub const NORMAL_REWARD: i32 = 3;
    pub const FAIL_MSG_0X2B8: i32 = -5;
    pub const FAIL_MSG_0X452: i32 = -2;
    pub const FAIL_MSG_0X2CA: i32 = -1;
}

/// Ordinary new-quest attempt roll: 75% attempt (mQst_GetRandom(4) != 0).
pub fn quest_attempt_roll(roll: u8) -> bool {
    roll % 4 != 0
}

// ---- C ABI ----

/// C ABI: 1 if quest base is complete (progress == 0).
#[no_mangle]
pub extern "C" fn pc_quest_complete(progress: u8) -> u8 {
    (progress == 0) as u8
}

/// C ABI: 1 if quest type is NONE (free slot).
#[no_mangle]
pub extern "C" fn pc_quest_free(quest_type: u8) -> u8 {
    (quest_type == qtype::NONE) as u8
}

/// C ABI: reward category from percentages + roll (0..99).
#[no_mangle]
pub extern "C" fn pc_reward_select(
    quest_type: u8,
    kind: u8,
    used_num: u8,
    roll: u8,
) -> u8 {
    let pcts: &[u8; 8] = match quest_type {
        qtype::DELIVERY => &DELIVERY_REWARDS[(kind % 4) as usize],
        qtype::CONTEST => &CONTEST_REWARDS[(kind % 7) as usize],
        qtype::ERRAND => &ERRAND_REWARDS[errand_tier(used_num)],
        _ => return 255,
    };
    prob_table_select(pcts, roll)
}

/// C ABI: base pay for a quest (before money_power scaling).
#[no_mangle]
pub extern "C" fn pc_quest_base_pay(quest_type: u8, kind: u8, used_num: u8) -> u32 {
    match quest_type {
        qtype::DELIVERY => DELIVERY_MAX_PAY[(kind % 4) as usize],
        qtype::CONTEST => CONTEST_MAX_PAY[(kind % 7) as usize],
        qtype::ERRAND => ERRAND_PAY[errand_tier(used_num)],
        _ => 0,
    }
}

/// C ABI: kind-specific timeout days, or -1 if not applicable.
#[no_mangle]
pub extern "C" fn pc_quest_limit_days(quest_type: u8, quest_kind: u8, progress: u8) -> i32 {
    limit_days(quest_type, quest_kind, progress).unwrap_or(-1)
}

/// C ABI: contest completion check. `flags` is a bitmask of [`cqual`]
/// qualification bits; see [`contest_complete`].
#[no_mangle]
pub extern "C" fn pc_contest_complete(kind: u8, progress: u8, flags: u8) -> u8 {
    contest_complete(kind, progress, &contest_qual_from_flags(flags)) as u8
}

/// C ABI: scaled money reward.
#[no_mangle]
pub extern "C" fn pc_scaled_pay(base_pay: u32, dir: u8, frac: f32, money_power: u32) -> u32 {
    scaled_pay(base_pay, dir, frac, money_power)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quest_state() {
        let q = QuestBase {
            quest_type: qtype::DELIVERY,
            quest_kind: dkind::NORMAL,
            time_limit_enabled: true,
            progress: 2,
            give_reward: false,
        };
        assert!(!is_complete(&q));
        assert!(!is_free(&q));
        let done = QuestBase { progress: 0, ..q };
        assert!(is_complete(&done));
        let free = QuestBase { quest_type: qtype::NONE, ..q };
        assert!(is_free(&free));
        assert_eq!(REGIST_NUM, 35);
        // Attempt roll: 75%.
        assert!(!quest_attempt_roll(0));
        assert!(quest_attempt_roll(1));
        assert!(quest_attempt_roll(3));
    }

    #[test]
    fn reward_tables() {
        // Delivery NORMAL: 40% FTR / 30% money / 30% worn cloth.
        assert_eq!(DELIVERY_REWARDS[0], [40, 0, 0, 0, 0, 30, 30, 0]);
        assert_eq!(DELIVERY_MAX_PAY[0], 200);
        assert_eq!(DELIVERY_MAX_PAY[1], 1000);
        // Contest fruit: 30/30/40 carpet/wallpaper/money.
        assert_eq!(CONTEST_REWARDS[0], [0, 0, 0, 30, 30, 40, 0, 0]);
        // Errand tiers: used_num=1 -> pay 0 (brief correction).
        assert_eq!(errand_tier(1), 0);
        assert_eq!(ERRAND_PAY[errand_tier(1)], 0);
        assert_eq!(ERRAND_PAY[errand_tier(2)], 500);
        assert_eq!(ERRAND_PAY[errand_tier(4)], 1000);
        assert_eq!(ERRAND_PAY[errand_tier(9)], 1000); // clamped
        assert_eq!(ERRAND_REWARDS[0], [0, 75, 25, 0, 0, 0, 0, 0]);
        // prob_tbl: 100 slots, 1% granularity.
        assert_eq!(prob_table_select(&DELIVERY_REWARDS[0], 0), reward::FTR);
        assert_eq!(prob_table_select(&DELIVERY_REWARDS[0], 39), reward::FTR);
        assert_eq!(prob_table_select(&DELIVERY_REWARDS[0], 40), reward::MONEY);
        assert_eq!(prob_table_select(&DELIVERY_REWARDS[0], 69), reward::MONEY);
        assert_eq!(prob_table_select(&DELIVERY_REWARDS[0], 70), reward::WORN_CLOTH);
        assert_eq!(prob_table_select(&DELIVERY_REWARDS[0], 99), reward::WORN_CLOTH);
        // C ABI.
        assert_eq!(pc_reward_select(qtype::DELIVERY, 0, 0, 0), reward::FTR);
        assert_eq!(pc_reward_select(qtype::ERRAND, 0, 1, 0), reward::STATIONERY);
        assert_eq!(pc_reward_select(qtype::ERRAND, 0, 2, 0), reward::FTR);
        assert_eq!(pc_quest_base_pay(qtype::ERRAND, 0, 1), 0);
        assert_eq!(pc_quest_base_pay(qtype::ERRAND, 0, 3), 750);
    }

    #[test]
    fn timeouts_and_completion() {
        assert_eq!(limit_days(qtype::DELIVERY, 0, 2), Some(2));
        assert_eq!(limit_days(qtype::CONTEST, ckind::FRUIT, 1), Some(1));
        assert_eq!(limit_days(qtype::CONTEST, ckind::FRUIT, 0), Some(4)); // +3 fin
        assert_eq!(limit_days(qtype::ERRAND, 0, 2), Some(2));
        assert_eq!(limit_days(qtype::ERRAND, 5, 2), Some(0)); // first-job: no limit
        assert_eq!(limit_days(qtype::DELIVERY, 9, 2), None);
        assert_eq!(MAX_TIME_LIMIT_DAYS, 28);
        // Contest completion (retail per-kind rules).
        let q_none = ContestQual::none();
        // Fruit needs the requested item possessed.
        assert!(!contest_complete(ckind::FRUIT, 1, &q_none));
        let mut q = ContestQual::none();
        q.item_possessed = true;
        assert!(contest_complete(ckind::FRUIT, 1, &q));
        assert!(!contest_complete(ckind::FRUIT, 2, &q));
        // Soccer: progress == 1 only.
        assert!(contest_complete(ckind::SOCCER, 1, &q_none));
        // Snowman: player_id set and == current player.
        assert!(!contest_complete(ckind::SNOWMAN, 1, &q_none));
        let mut qs = ContestQual::none();
        qs.player_id_set = true;
        assert!(!contest_complete(ckind::SNOWMAN, 1, &qs));
        qs.player_id_current = true;
        assert!(contest_complete(ckind::SNOWMAN, 1, &qs));
        // Letter: progress == 1 only.
        assert!(contest_complete(ckind::LETTER, 1, &q_none));
        // Flower: goal + qualification.
        assert!(!contest_complete(ckind::FLOWER, 1, &q_none));
        let mut qf = ContestQual::none();
        qf.flower_ok = true;
        assert!(contest_complete(ckind::FLOWER, 1, &qf));
        // Fish/insect: player_id null + owns category item.
        let mut qi = ContestQual::none();
        qi.player_id_absent = true;
        assert!(!contest_complete(ckind::FISH, 1, &qi));
        qi.owns_category_item = true;
        assert!(contest_complete(ckind::FISH, 1, &qi));
        assert!(contest_complete(ckind::INSECT, 1, &qi));
        // Flag roundtrip.
        let flags = cqual::ITEM_POSSESSED | cqual::PLAYER_ID_ABSENT | cqual::OWNS_CATEGORY_ITEM;
        let qf2 = contest_qual_from_flags(flags);
        assert!(qf2.item_possessed && qf2.player_id_absent && qf2.owns_category_item);
        assert!(!qf2.flower_ok);
        assert_eq!(pc_quest_limit_days(qtype::CONTEST, 6, 1), 2); // letter
        assert_eq!(pc_contest_complete(ckind::SOCCER, 1, 0), 1);
        assert_eq!(pc_contest_complete(ckind::FRUIT, 1, 0), 0); // needs item
        assert_eq!(pc_contest_complete(ckind::FRUIT, 1, cqual::ITEM_POSSESSED), 1);
        assert_eq!(
            pc_contest_complete(ckind::SNOWMAN, 1, cqual::PLAYER_ID_SET | cqual::PLAYER_ID_CURRENT),
            1
        );
        // Money scaling: base 200, +10%, no money power.
        let p = scaled_pay(200, 0, 1.0, 0);
        assert_eq!(p, (200.0 * (110.0 * 100.0) / 10000.0) as u32);
        // Friendship.
        assert_eq!(friendship::REJECT_QUEST, -3);
        assert_eq!(friendship::NORMAL_REWARD, 3);
        assert_eq!(friendship::FAIL_MSG_0X2B8, -5);
        // C ABI.
        assert_eq!(pc_quest_complete(0), 1);
        assert_eq!(pc_quest_free(3), 1);
        assert_eq!(pc_scaled_pay(200, 0, 1.0, 0), p);
    }
}
