//! Quest request generation: type/kind tables, recipient selection,
//! set-data definitions, entrusted-item handling, and letter quests.
//!
//! Verified against `include/m_quest.h`, `src/game/m_quest.c`,
//! `include/ac_quest_manager.h`, `src/actor/ac_quest_manager.c`
//! (USA Rev. 0 decomp / PC port).
//!
//! This complements `quest.rs` (quest state, rewards, timeouts, completion):
//! that module is the quest-state/reward layer, while this module is the
//! quest-*generation* layer. `request_selector.rs` is possession/trade
//! dialogue-request selection, a different system again.
//!
//! ## Pipeline (retail)
//!
//! ```text
//! NPC asks for work
//!   -> pending quest? use it
//!   -> RANDOM(4) != 0 ? (75% attempt gate)
//!   -> choose type table (first-job vs normal) -> uniform type
//!   -> choose kind table -> uniform kind
//!   -> aQMgr_actor_check_occur() eligibility (season/time/etc.)
//!   -> find free quest storage
//!   -> set_data = &l_set_data[type][kind]
//!   -> resolve recipient (one of six target modes)
//!   -> choose item (item source)
//!   -> optional pocket handover (entrusted item)
//!   -> set time/progress, persist
//! ```
//!
//! ## Confidence
//!
//! Source-proven: type/kind tables and uniform selection, the 75% gate, the
//! six recipient modes, the set-data tables (verbatim below), entrusted-item
//! handover, ITEM_COND_QUEST, delivery[i]<->pocket[i], errand pockets_idx,
//! grab/put preservation, letter-quest rank/reply/first-job specifics.
//!
//! Inferred: the Rust-side recipient-context plumbing (the C code threads
//! live game pointers); modeled here with plain slices/ids.

use crate::quest::{ckind, dkind, ekind, qtype};

/// Empty item id (`EMPTY_NO`, `include/m_name_table.h`).
pub const EMPTY_NO: u16 = 0x0000;

/// First-job type table (`l_quest_type_table_fj`).
pub const QUEST_TYPE_TABLE_FJ: [u8; 2] = [qtype::DELIVERY, qtype::ERRAND];
/// Normal type table (`l_quest_type_table_qst`).
pub const QUEST_TYPE_TABLE_QST: [u8; 3] = [qtype::DELIVERY, qtype::ERRAND, qtype::CONTEST];

/// Choose the generation table: first-job when the player is not man-kind
/// and the first-job event flag is set for them
/// (`!mLd_PlayerManKindCheck() && mEv_CheckEvent(mEv_SAVED_FIRSTJOB_PLR0 + player_no)`).
pub fn use_first_job_table(is_man_kind: bool, firstjob_event: bool) -> bool {
    !is_man_kind && firstjob_event
}

/// Uniform type selection: `type = table[mQst_GetRandom(type_count)]`.
pub fn select_quest_type(first_job: bool, roll: u32) -> u8 {
    if first_job {
        QUEST_TYPE_TABLE_FJ[(roll as usize) % QUEST_TYPE_TABLE_FJ.len()]
    } else {
        QUEST_TYPE_TABLE_QST[(roll as usize) % QUEST_TYPE_TABLE_QST.len()]
    }
}

/// First-job delivery kinds: { NORMAL, LOST }.
pub const DELIVERY_KIND_TABLE_FJ: [u8; 2] = [dkind::NORMAL, dkind::LOST];
/// Normal delivery kinds: { NORMAL, FOREIGN, REMOVE, LOST }.
pub const DELIVERY_KIND_TABLE_QST: [u8; 4] =
    [dkind::NORMAL, dkind::FOREIGN, dkind::REMOVE, dkind::LOST];
/// Errand kinds (both modes): { REQUEST }.
pub const ERRAND_KIND_TABLE: [u8; 1] = [ekind::REQUEST];
/// Normal contest kinds.
pub const CONTEST_KIND_TABLE_QST: [u8; 7] = [
    ckind::FRUIT,
    ckind::SOCCER,
    ckind::SNOWMAN,
    ckind::FLOWER,
    ckind::FISH,
    ckind::INSECT,
    ckind::LETTER,
];

/// Kind count for (type, first_job); 0 when the combination has no table
/// (e.g. contests in first-job mode).
pub fn quest_kind_count(quest_type: u8, first_job: bool) -> usize {
    match quest_type {
        qtype::DELIVERY => {
            if first_job {
                DELIVERY_KIND_TABLE_FJ.len()
            } else {
                DELIVERY_KIND_TABLE_QST.len()
            }
        }
        qtype::ERRAND => ERRAND_KIND_TABLE.len(),
        qtype::CONTEST => {
            if first_job {
                0
            } else {
                CONTEST_KIND_TABLE_QST.len()
            }
        }
        _ => 0,
    }
}

/// Uniform kind selection: `kind = table[RANDOM(kind_count)]`.
/// Returns `None` for table-less combinations.
pub fn select_quest_kind(quest_type: u8, first_job: bool, roll: u32) -> Option<u8> {
    let table: &[u8] = match quest_type {
        qtype::DELIVERY => {
            if first_job {
                &DELIVERY_KIND_TABLE_FJ
            } else {
                &DELIVERY_KIND_TABLE_QST
            }
        }
        qtype::ERRAND => &ERRAND_KIND_TABLE,
        qtype::CONTEST => {
            if first_job {
                return None;
            } else {
                &CONTEST_KIND_TABLE_QST
            }
        }
        _ => return None,
    };
    Some(table[(roll as usize) % table.len()])
}

/// First-job errand kinds: `mQst_ERRAND_FIRSTJOB_CHANGE_CLOTH` etc.
/// (`include/m_quest.h:131-142`); values 3..14, `mQst_ERRAND_NUM = 15`.
pub mod fjekind {
    pub const CHANGE_CLOTH: u8 = 3;
    pub const PLANT_FLOWER: u8 = 4;
    pub const DELIVER_FTR: u8 = 5;
    pub const SEND_LETTER: u8 = 6;
    pub const DELIVER_CARPET: u8 = 7;
    pub const DELIVER_AXE: u8 = 8;
    pub const POST_NOTICE: u8 = 9;
    pub const SEND_LETTER2: u8 = 10;
    pub const DELIVER_AXE2: u8 = 11;
    pub const INTRODUCTIONS: u8 = 12;
    pub const OPEN: u8 = 13;
    pub const START: u8 = 14;
}

/// Context for `aQMgr_actor_check_occur()` kind-eligibility checks.
#[derive(Clone, Copy, Debug, Default)]
pub struct OccurCtx {
    pub month: u8,         // 1..12
    pub day: u8,           // 1..31
    pub hour: u8,          // 0..23
    pub empty_acre_spaces: u8,
    pub flower_count: u8,
    pub local_player: bool, // mLd_PlayerManKindCheck() == FALSE (resident player)
    pub foreign_ok: bool, // no existing foreign quest + valid stored_anm_id
    pub removed_ok: bool, // valid last_removed_animal_id
    /// Whether this contest kind is already active
    /// (`mQst_GetOccuredContestIdx(kind) != -1` blocks generation).
    pub contest_active: bool,
}

/// Kind occurrence gating (`aQMgr_actor_check_occur`).
///
/// A failed check fails the request attempt; retail does not reroll the kind.
pub fn occur_ok(quest_type: u8, kind: u8, ctx: &OccurCtx) -> bool {
    // Every contest kind first requires the kind to not already be active.
    if quest_type == qtype::CONTEST && ctx.contest_active {
        return false;
    }
    match (quest_type, kind) {
        (qtype::DELIVERY, dkind::FOREIGN) => ctx.foreign_ok,
        (qtype::DELIVERY, dkind::REMOVE) => ctx.removed_ok,
        (qtype::CONTEST, ckind::SNOWMAN) => {
            // Jan, Feb 1-17, Dec 25-31, hour 8 through 16 (16:59).
            let date_ok = ctx.month == 1
                || (ctx.month == 2 && ctx.day <= 17)
                || (ctx.month == 12 && ctx.day >= 25);
            date_ok && (8..=16).contains(&ctx.hour)
        }
        (qtype::CONTEST, ckind::FLOWER) => {
            // Feb 25 onward through August, >= 4 empty acre spaces,
            // no more than 20 flowers.
            let date_ok = (ctx.month == 2 && ctx.day >= 25) || (3..=8).contains(&ctx.month);
            date_ok && ctx.empty_acre_spaces >= 4 && ctx.flower_count <= 20
        }
        (qtype::CONTEST, ckind::INSECT) => {
            // March-October, November 1-28.
            (3..=10).contains(&ctx.month) || (ctx.month == 11 && ctx.day <= 28)
        }
        (qtype::CONTEST, ckind::LETTER) => {
            // Local resident player (`mLd_PlayerManKindCheck() == FALSE`).
            ctx.local_player
        }
        _ => true,
    }
}

/// Quest initialization from set data (`aQMgr_actor_set_quest_data`):
/// `progress = set_data.last_step`; the time limit is enabled iff
/// `day_limit != 0` (and then set to now + `day_limit` days).
pub fn quest_init_from_set_data(d: &QuestSetData) -> (u8, bool) {
    (d.last_step, d.day_limit != 0)
}

/// Fallback item when an errand has no current item
/// (`ITM_CLOTH001 = ITM_CLOTH_START + 1`, `include/m_name_table.h`).
pub const ITM_CLOTH001: u16 = 0x2401;

/// Resolve the requested item from the set-data item source
/// (`aQMgr_actor_set_quest_data` item switch):
/// - RANDOM -> `aQMgr_actor_decide_item()`
/// - FRUIT -> `mFI_GetOtherFruit()` (the town's *non-native* fruit)
/// - CLOTH -> `aQMgr_actor_decide_cloth()`
/// - FROM_DATA -> `set_data.item`
/// - CURRENT_ITEM -> the errand's current item (or `ITM_CLOTH001`)
/// - NONE -> `EMPTY_NO`
pub fn resolve_item_source(
    source: QuestItemSource,
    set_data_item: u16,
    errand_item: Option<u16>,
    other_fruit: u16,
    decided_item: u16,
    decided_cloth: u16,
) -> u16 {
    match source {
        QuestItemSource::Random => decided_item,
        QuestItemSource::Fruit => other_fruit,
        QuestItemSource::Cloth => decided_cloth,
        QuestItemSource::FromData => set_data_item,
        QuestItemSource::CurrentItem => errand_item.unwrap_or(ITM_CLOTH001),
        QuestItemSource::None => EMPTY_NO,
    }
}

/// The six quest recipient-selection modes (`aQMgr_QUEST_TARGET_*`).
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QuestTarget {
    Random = 0,
    RandomExcluded = 1,
    OriginalTarget = 2,
    Foreign = 3,
    LastRemove = 4,
    Client = 5,
}

/// Minimal villager record for recipient selection.
#[derive(Clone, Copy, Debug)]
pub struct VillagerRef {
    pub id: u32,
    pub acre_x: i32,
    pub acre_z: i32,
}

/// Context for [`resolve_recipient`].
#[derive(Clone, Copy, Debug)]
pub struct RecipientCtx<'a> {
    pub villagers: &'a [VillagerRef],
    /// Index of the quest-giving NPC.
    pub giver_idx: usize,
    /// Villagers already used in the errand chain (RANDOM_EXCLUDED).
    pub chain_used: [Option<u32>; 3],
    /// `Common_Get(now_private)->stored_anm_id` (FOREIGN).
    pub foreign_id: Option<u32>,
    /// `Save_Get(last_removed_animal_id)` (LAST_REMOVE).
    pub last_removed_id: Option<u32>,
    /// Index of the client animal (CLIENT) — normally the giver.
    pub client_idx: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecipientError {
    NoForeignId,
    NoRemoveAnimalId,
    NoEligible,
}

/// Uniform selection among eligible villagers
/// (`mNpc_GetOtherAnimalPersonalIDOtherBlock` semantics): build the eligible
/// set (excluding null IDs, the giver, chain members, and same-acre
/// residents when `exclude_same_acre`), then take the `roll`-th eligible
/// villager in array order.
fn uniform_eligible(
    villagers: &[VillagerRef],
    giver_idx: usize,
    extra_exclude: &[u32],
    exclude_same_acre: bool,
    roll: u32,
) -> Result<u32, RecipientError> {
    let giver = villagers.get(giver_idx).ok_or(RecipientError::NoEligible)?;
    let eligible: Vec<u32> = villagers
        .iter()
        .enumerate()
        .filter(|(i, v)| {
            if *i == giver_idx {
                return false;
            }
            if v.id == 0 {
                return false;
            }
            if extra_exclude.contains(&v.id) {
                return false;
            }
            if exclude_same_acre && v.acre_x == giver.acre_x && v.acre_z == giver.acre_z {
                return false;
            }
            true
        })
        .map(|(_, v)| v.id)
        .collect();
    if eligible.is_empty() {
        return Err(RecipientError::NoEligible);
    }
    Ok(eligible[(roll as usize) % eligible.len()])
}

/// Resolve the quest recipient for a target mode.
///
/// - RANDOM: random eligible villager outside the giver's home acre.
/// - RANDOM_EXCLUDED: also excludes chain members and the giver; the caller
///   should set `errand_type = mQst_ERRAND_TYPE_CHAIN`.
/// - ORIGINAL_TARGET: first villager in the chain, else the giver (defensive).
/// - FOREIGN / LAST_REMOVE: stored IDs, or an error when empty.
/// - CLIENT: the client animal (the quest-giving NPC itself).
pub fn resolve_recipient(
    target: QuestTarget,
    ctx: &RecipientCtx,
    roll: u32,
) -> Result<u32, RecipientError> {
    match target {
        QuestTarget::Random => uniform_eligible(ctx.villagers, ctx.giver_idx, &[], true, roll),
        QuestTarget::RandomExcluded => {
            let mut excl: Vec<u32> = ctx.chain_used.iter().filter_map(|&id| id).collect();
            if let Some(giver) = ctx.villagers.get(ctx.giver_idx) {
                excl.push(giver.id);
            }
            uniform_eligible(ctx.villagers, ctx.giver_idx, &excl, true, roll)
        }
        QuestTarget::OriginalTarget => {
            if let Some(id) = ctx.chain_used[0] {
                Ok(id)
            } else if let Some(giver) = ctx.villagers.get(ctx.giver_idx) {
                Ok(giver.id)
            } else {
                Err(RecipientError::NoEligible)
            }
        }
        QuestTarget::Foreign => ctx.foreign_id.ok_or(RecipientError::NoForeignId),
        QuestTarget::LastRemove => ctx.last_removed_id.ok_or(RecipientError::NoRemoveAnimalId),
        QuestTarget::Client => ctx
            .villagers
            .get(ctx.client_idx)
            .map(|v| v.id)
            .ok_or(RecipientError::NoEligible),
    }
}

/// Quest item sources (`aQMgr_QUEST_ITEM_*`, `include/ac_quest_manager.h`).
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QuestItemSource {
    Random = 0,
    Fruit = 1,
    Cloth = 2,
    FromData = 3,
    CurrentItem = 4,
    None = 5,
}

/// Message-kind count (`aQMgr_MSG_KIND_NUM`): 13 `msg_start` entries.
pub const MSG_KIND_NUM: usize = 13;

/// Quest definition record (`aQMgr_set_data_c`):
/// `to_type:3, day_limit:6, last_step:4, handover_item:1, src_item_type:3`,
/// `item`, `reward_percentages[8]`, `max_pay`, `msg_start[13]`.
///
/// `l_set_data[type][kind]` — the actual behavioral-definition layer behind
/// the quest type/kind identifier layer.
#[derive(Clone, Copy, Debug)]
pub struct QuestSetData {
    pub target: QuestTarget,
    pub day_limit: u8,
    pub last_step: u8,
    pub handover_item: bool,
    pub item_source: QuestItemSource,
    pub item: u16,
    pub reward_percentages: [u8; 8],
    pub max_pay: u32,
    pub msg_start: [i32; MSG_KIND_NUM],
}

/// `l_set_delivery_data` (verbatim, `src/actor/ac_quest_manager.c:39`).
pub const DELIVERY_SET_DATA: [QuestSetData; 4] = [
    QuestSetData {
        // NORMAL
        target: QuestTarget::Random,
        day_limit: 2,
        last_step: 0,
        handover_item: true,
        item_source: QuestItemSource::Cloth,
        item: EMPTY_NO,
        reward_percentages: [40, 0, 0, 0, 0, 30, 30, 0],
        max_pay: 200,
        msg_start: [
            0x0151, 0x024C, 0x0163, 0x025E, 0x0175, 0x0294, 0x0187, 0x02B8, 0x0440, 0x035E,
            0x034C, 0x0370, 0x033A,
        ],
    },
    QuestSetData {
        // FOREIGN
        target: QuestTarget::Foreign,
        day_limit: 2,
        last_step: 0,
        handover_item: true,
        item_source: QuestItemSource::Random,
        item: EMPTY_NO,
        reward_percentages: [40, 0, 0, 10, 10, 40, 0, 0],
        max_pay: 1000,
        msg_start: [
            0x0199, 0x024C, 0x01AB, 0x025E, 0x01BD, 0x0294, 0x01CF, 0x02CA, 0x0440, 0x035E,
            0x034C, 0x0370, 0x033A,
        ],
    },
    QuestSetData {
        // REMOVE (last-removed animal)
        target: QuestTarget::LastRemove,
        day_limit: 2,
        last_step: 0,
        handover_item: true,
        item_source: QuestItemSource::Random,
        item: EMPTY_NO,
        reward_percentages: [20, 0, 0, 20, 20, 40, 0, 0],
        max_pay: 1000,
        msg_start: [
            0x0205, 0x024C, 0x0217, 0x025E, 0x10BF, 0x0294, 0x023A, 0x02CA, 0x0440, 0x035E,
            0x034C, 0x0370, 0x033A,
        ],
    },
    QuestSetData {
        // LOST
        target: QuestTarget::Random,
        day_limit: 2,
        last_step: 0,
        handover_item: true,
        item_source: QuestItemSource::Random,
        item: EMPTY_NO,
        reward_percentages: [40, 0, 40, 10, 10, 0, 0, 0],
        max_pay: 0,
        msg_start: [
            0x0A74, 0x024C, 0x0A86, 0x025E, 0x0A98, 0x0294, 0x0AAA, 0x02B8, 0x0440, 0x035E,
            0x034C, 0x0370, 0x033A,
        ],
    },
];

/// `l_set_errand_data` (verbatim, `src/actor/ac_quest_manager.c:86`).
///
/// Rows 0-2 are the chain stages (REQUEST / REQUEST_CONTINUE /
/// REQUEST_FINAL); rows 3-14 are the first-job definitions, all
/// CLIENT-targeted with no handover — they are constructed directly by the
/// `mQst_SetFirstJob*()` functions, not by the random kind generator.
pub const ERRAND_SET_DATA: [QuestSetData; 15] = [
    QuestSetData {
        // REQUEST
        target: QuestTarget::RandomExcluded,
        day_limit: 2,
        last_step: 4,
        handover_item: false,
        item_source: QuestItemSource::Random,
        item: EMPTY_NO,
        reward_percentages: [50, 0, 0, 0, 0, 0, 0, 0],
        max_pay: 500,
        msg_start: [
            0x038C, 0x024C, 0x03D4, 0x025E, 0x03F8, 0x0294, 0x2B73, 0x02CA, 0x0440, 0x035E,
            0x034C, 0x0370, 0x033A,
        ],
    },
    QuestSetData {
        // REQUEST_CONTINUE
        target: QuestTarget::RandomExcluded,
        day_limit: 2,
        last_step: 1,
        handover_item: false,
        item_source: QuestItemSource::CurrentItem,
        item: EMPTY_NO,
        reward_percentages: [50, 0, 0, 0, 0, 0, 0, 0],
        max_pay: 500,
        msg_start: [
            0x03B0, 0x024C, 0x03E6, 0x025E, 0x03F8, 0x0294, 0x041C, 0x02CA, 0x0440, 0x035E,
            0x034C, 0x0370, 0x033A,
        ],
    },
    QuestSetData {
        // REQUEST_FINAL
        target: QuestTarget::OriginalTarget,
        day_limit: 2,
        last_step: 0,
        handover_item: true,
        item_source: QuestItemSource::CurrentItem,
        item: EMPTY_NO,
        reward_percentages: [50, 0, 0, 0, 0, 0, 0, 0],
        max_pay: 500,
        msg_start: [
            0x039E, 0x024C, 0x03C2, 0x025E, 0x03F8, 0x0294, 0x040A, 0x0452, 0x17B8, 0x035E,
            0x034C, 0x0370, 0x033A,
        ],
    },
    QuestSetData {
        // FIRSTJOB_CHANGE_CLOTH
        target: QuestTarget::Client,
        day_limit: 0,
        last_step: 0,
        handover_item: false,
        item_source: QuestItemSource::CurrentItem,
        item: EMPTY_NO,
        reward_percentages: [0, 0, 0, 0, 0, 0, 0, 100],
        max_pay: 0,
        msg_start: [
            0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0440, 0x035E,
            0x034C, 0x0370, 0x033A,
        ],
    },
    QuestSetData {
        // FIRSTJOB_PLANT_FLOWER
        target: QuestTarget::Client,
        day_limit: 0,
        last_step: 0,
        handover_item: false,
        item_source: QuestItemSource::CurrentItem,
        item: EMPTY_NO,
        reward_percentages: [0, 0, 0, 0, 0, 0, 0, 100],
        max_pay: 0,
        msg_start: [
            0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0440, 0x035E,
            0x034C, 0x0370, 0x033A,
        ],
    },
    QuestSetData {
        // FIRSTJOB_DELIVER_FTR
        target: QuestTarget::Client,
        day_limit: 0,
        last_step: 0,
        handover_item: false,
        item_source: QuestItemSource::CurrentItem,
        item: EMPTY_NO,
        reward_percentages: [0, 0, 0, 0, 0, 0, 0, 100],
        max_pay: 0,
        msg_start: [
            0x0000, 0x0000, 0x0000, 0x0000, 0x08EF, 0x08F0, 0x0000, 0x0000, 0x0440, 0x035E,
            0x034C, 0x0370, 0x033A,
        ],
    },
    QuestSetData {
        // FIRSTJOB_SEND_LETTER
        target: QuestTarget::Client,
        day_limit: 0,
        last_step: 0,
        handover_item: false,
        item_source: QuestItemSource::CurrentItem,
        item: EMPTY_NO,
        reward_percentages: [0, 0, 0, 0, 0, 0, 0, 100],
        max_pay: 0,
        msg_start: [
            0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0440, 0x035E,
            0x034C, 0x0370, 0x033A,
        ],
    },
    QuestSetData {
        // FIRSTJOB_DELIVER_CARPET
        target: QuestTarget::Client,
        day_limit: 0,
        last_step: 0,
        handover_item: false,
        item_source: QuestItemSource::CurrentItem,
        item: EMPTY_NO,
        reward_percentages: [0, 0, 0, 0, 0, 0, 0, 100],
        max_pay: 0,
        msg_start: [
            0x0000, 0x0000, 0x0000, 0x0000, 0x08FB, 0x08FC, 0x0000, 0x0000, 0x0440, 0x035E,
            0x034C, 0x0370, 0x033A,
        ],
    },
    QuestSetData {
        // FIRSTJOB_DELIVER_AXE
        target: QuestTarget::Client,
        day_limit: 0,
        last_step: 0,
        handover_item: false,
        item_source: QuestItemSource::CurrentItem,
        item: EMPTY_NO,
        reward_percentages: [0, 0, 0, 0, 0, 0, 0, 100],
        max_pay: 0,
        msg_start: [
            0x0000, 0x0000, 0x0000, 0x0000, 0x0907, 0x0908, 0x0000, 0x0000, 0x0440, 0x035E,
            0x034C, 0x0370, 0x033A,
        ],
    },
    QuestSetData {
        // FIRSTJOB_POST_NOTICE
        target: QuestTarget::Client,
        day_limit: 0,
        last_step: 0,
        handover_item: false,
        item_source: QuestItemSource::CurrentItem,
        item: EMPTY_NO,
        reward_percentages: [0, 0, 0, 0, 0, 0, 0, 100],
        max_pay: 0,
        msg_start: [
            0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0440, 0x035E,
            0x034C, 0x0370, 0x033A,
        ],
    },
    QuestSetData {
        // FIRSTJOB_SEND_LETTER2
        target: QuestTarget::Client,
        day_limit: 0,
        last_step: 0,
        handover_item: false,
        item_source: QuestItemSource::CurrentItem,
        item: EMPTY_NO,
        reward_percentages: [0, 0, 0, 0, 0, 0, 0, 100],
        max_pay: 0,
        msg_start: [
            0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0440, 0x035E,
            0x034C, 0x0370, 0x033A,
        ],
    },
    QuestSetData {
        // FIRSTJOB_DELIVER_AXE2
        target: QuestTarget::Client,
        day_limit: 0,
        last_step: 0,
        handover_item: false,
        item_source: QuestItemSource::CurrentItem,
        item: EMPTY_NO,
        reward_percentages: [0, 0, 0, 0, 0, 0, 0, 100],
        max_pay: 0,
        msg_start: [
            0x0000, 0x0000, 0x0000, 0x0000, 0x0907, 0x0908, 0x0000, 0x0000, 0x0440, 0x035E,
            0x034C, 0x0370, 0x033A,
        ],
    },
    QuestSetData {
        // FIRSTJOB_INTRODUCTIONS
        target: QuestTarget::Client,
        day_limit: 0,
        last_step: 0,
        handover_item: false,
        item_source: QuestItemSource::CurrentItem,
        item: EMPTY_NO,
        reward_percentages: [0, 0, 0, 0, 0, 0, 0, 100],
        max_pay: 0,
        msg_start: [
            0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0440, 0x035E,
            0x034C, 0x0370, 0x033A,
        ],
    },
    QuestSetData {
        // FIRSTJOB_OPEN
        target: QuestTarget::Client,
        day_limit: 0,
        last_step: 0,
        handover_item: false,
        item_source: QuestItemSource::CurrentItem,
        item: EMPTY_NO,
        reward_percentages: [0, 0, 0, 0, 0, 0, 0, 100],
        max_pay: 0,
        msg_start: [
            0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0440, 0x035E,
            0x034C, 0x0370, 0x033A,
        ],
    },
    QuestSetData {
        // FIRSTJOB_START
        target: QuestTarget::Client,
        day_limit: 0,
        last_step: 0,
        handover_item: false,
        item_source: QuestItemSource::CurrentItem,
        item: EMPTY_NO,
        reward_percentages: [0, 0, 0, 0, 0, 0, 0, 100],
        max_pay: 0,
        msg_start: [
            0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0440, 0x035E,
            0x034C, 0x0370, 0x033A,
        ],
    },
];

/// `l_set_contest_data` (verbatim, `src/actor/ac_quest_manager.c:254`).
///
/// All seven use `aQMgr_QUEST_TARGET_CLIENT`: contests are always
/// "do something for the NPC who asked".
pub const CONTEST_SET_DATA: [QuestSetData; 7] = [
    QuestSetData {
        // FRUIT
        target: QuestTarget::Client,
        day_limit: 1,
        last_step: 1,
        handover_item: false,
        item_source: QuestItemSource::Fruit,
        item: EMPTY_NO,
        reward_percentages: [0, 0, 0, 30, 30, 40, 0, 0],
        max_pay: 500,
        msg_start: [
            0x01E1, 0x0000, 0x01E1, 0x025E, 0x01F3, 0x117C, 0x0000, 0x02CA, 0x0440, 0x035E,
            0x034C, 0x0370, 0x033A,
        ],
    },
    QuestSetData {
        // SOCCER
        target: QuestTarget::Client,
        day_limit: 1,
        last_step: 2,
        handover_item: false,
        item_source: QuestItemSource::None,
        item: EMPTY_NO,
        reward_percentages: [40, 0, 0, 30, 30, 0, 0, 0],
        max_pay: 0,
        msg_start: [
            0x0D79, 0x0000, 0x0D79, 0x025E, 0x0D91, 0x0DD9, 0x0000, 0x02CA, 0x0440, 0x0DFD,
            0x0DD9, 0x0DEB, 0x0E0F,
        ],
    },
    QuestSetData {
        // SNOWMAN
        target: QuestTarget::Client,
        day_limit: 1,
        last_step: 1,
        handover_item: false,
        item_source: QuestItemSource::None,
        item: EMPTY_NO,
        reward_percentages: [60, 0, 0, 20, 20, 0, 0, 0],
        max_pay: 0,
        msg_start: [
            0x0E33, 0x0000, 0x0E33, 0x025E, 0x0E45, 0x0E8D, 0x0000, 0x02CA, 0x0440, 0x0EB1,
            0x0E8D, 0x0E9F, 0x0EC3,
        ],
    },
    QuestSetData {
        // FLOWER
        target: QuestTarget::Client,
        day_limit: 3,
        last_step: 1,
        handover_item: false,
        item_source: QuestItemSource::None,
        item: EMPTY_NO,
        reward_percentages: [60, 0, 0, 20, 20, 0, 0, 0],
        max_pay: 0,
        msg_start: [
            0x0FB5, 0x0000, 0x0FB5, 0x025E, 0x0FC7, 0x100F, 0x0000, 0x02CA, 0x0440, 0x1033,
            0x100F, 0x1021, 0x1045,
        ],
    },
    QuestSetData {
        // FISH
        target: QuestTarget::Client,
        day_limit: 3,
        last_step: 1,
        handover_item: false,
        item_source: QuestItemSource::None,
        item: EMPTY_NO,
        reward_percentages: [80, 0, 0, 10, 10, 0, 0, 0],
        max_pay: 0,
        msg_start: [
            0x158C, 0x0000, 0x158C, 0x025E, 0x159E, 0x15B0, 0x0000, 0x02CA, 0x0440, 0x035E,
            0x034C, 0x0370, 0x033A,
        ],
    },
    QuestSetData {
        // INSECT
        target: QuestTarget::Client,
        day_limit: 3,
        last_step: 1,
        handover_item: false,
        item_source: QuestItemSource::None,
        item: EMPTY_NO,
        reward_percentages: [80, 0, 0, 10, 10, 0, 0, 0],
        max_pay: 0,
        msg_start: [
            0x160A, 0x0000, 0x160A, 0x025E, 0x161C, 0x162E, 0x0000, 0x02CA, 0x0440, 0x035E,
            0x034C, 0x0370, 0x033A,
        ],
    },
    QuestSetData {
        // LETTER
        target: QuestTarget::Client,
        day_limit: 2,
        last_step: 2,
        handover_item: false,
        item_source: QuestItemSource::None,
        item: EMPTY_NO,
        reward_percentages: [80, 0, 0, 10, 10, 0, 0, 0],
        max_pay: 0,
        msg_start: [
            0x1AE1, 0x0000, 0x1AE1, 0x025E, 0x1B17, 0x0294, 0x0000, 0x02CA, 0x0440, 0x035E,
            0x034C, 0x0370, 0x033A,
        ],
    },
];

/// `l_set_data[type][kind]` lookup. Returns `None` for out-of-range kinds.
pub fn set_data(quest_type: u8, kind: u8) -> Option<&'static QuestSetData> {
    match quest_type {
        qtype::DELIVERY => DELIVERY_SET_DATA.get(kind as usize),
        qtype::ERRAND => ERRAND_SET_DATA.get(kind as usize),
        qtype::CONTEST => CONTEST_SET_DATA.get(kind as usize),
        _ => None,
    }
}

/// Inventory condition values (`mPr_ITEM_COND_*`): a 2-bit-per-pocket
/// persistent bitfield. `QUEST` is effectively part of an entrusted item's
/// identity.
pub mod item_cond {
    pub const NORMAL: u8 = 0;
    pub const PRESENT: u8 = 1;
    pub const QUEST: u8 = 2;
}

/// Pocket count (`mPr_POCKETS_SLOT_COUNT`).
pub const POCKET_COUNT: usize = 15;
/// Delivery quest records (`mPr_DELIVERY_QUEST_NUM` = pockets count).
pub const DELIVERY_SLOT_COUNT: usize = 15;
/// Errand quest records.
pub const ERRAND_SLOT_COUNT: usize = 5;

/// One delivery quest record.
///
/// Retail invariant: **delivery record `i` corresponds to pocket `i`**
/// (`mQst_delivery_c deliveries[mPr_DELIVERY_QUEST_NUM]`).
#[derive(Clone, Copy, Debug, Default)]
pub struct DeliveryRecord {
    pub occupied: bool,
    pub item: u16,
}

/// One errand quest record (`mQst_errand_c`).
///
/// Unlike delivery, the errand index is independent of the pocket; the
/// record carries its own `pockets_idx` (5 bits).
#[derive(Clone, Copy, Debug, Default)]
pub struct ErrandRecord {
    pub occupied: bool,
    pub item: u16,
    pub pockets_idx: i8,
}

/// First empty pocket (`mPr_GetPossessionItemIdx(priv, EMPTY_NO)`).
/// Returns `None` when the pockets are full — the request is then not
/// created (`aQMgr_NEW_QUEST_NO_SPACE`).
pub fn first_empty_pocket(pockets: &[u16; POCKET_COUNT]) -> Option<usize> {
    pockets.iter().position(|&item| item == EMPTY_NO)
}

/// Free delivery slot: record `i` free AND pocket `i` empty.
pub fn find_free_delivery(
    records: &[DeliveryRecord; DELIVERY_SLOT_COUNT],
    pockets: &[u16; POCKET_COUNT],
) -> Option<usize> {
    (0..DELIVERY_SLOT_COUNT)
        .find(|&i| !records[i].occupied && pockets[i] == EMPTY_NO)
}

/// Errors from quest generation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QuestGenError {
    NoSpace,
    Recipient(RecipientError),
}

/// Bind an entrusted item: find the pocket, record the quest/pocket binding.
/// Mirrors the handover half of quest generation.
pub fn handover_item(
    pockets: &mut [u16; POCKET_COUNT],
    item: u16,
) -> Result<usize, QuestGenError> {
    let idx = first_empty_pocket(pockets).ok_or(QuestGenError::NoSpace)?;
    pockets[idx] = item;
    Ok(idx)
}

/// Temporary grab state (`l_mqst_grab`).
///
/// Retail quirk, preserved: for delivery, `index` is the **pocket** index;
/// for errands, `index` is the **errand record** index.
pub struct QuestGrab {
    pub is_errand: bool,
    pub index: usize,
    pub item: u16,
}

/// `mQst_CheckGrabItem`: when an inventory item is picked up/moved, remember
/// the quest association of the source pocket.
pub fn check_grab(
    item: u16,
    pocket_idx: usize,
    deliveries: &[DeliveryRecord; DELIVERY_SLOT_COUNT],
    errands: &[ErrandRecord; ERRAND_SLOT_COUNT],
) -> Option<QuestGrab> {
    if pocket_idx < DELIVERY_SLOT_COUNT && deliveries[pocket_idx].occupied {
        return Some(QuestGrab {
            is_errand: false,
            index: pocket_idx,
            item,
        });
    }
    for (i, e) in errands.iter().enumerate() {
        if e.occupied && e.pockets_idx as usize == pocket_idx && e.item == item {
            return Some(QuestGrab {
                is_errand: true,
                index: i,
                item,
            });
        }
    }
    None
}

/// `mQst_CheckPutItem`: when the item is put down, restore the quest
/// association at the new pocket.
///
/// Retail first *displaces* the destination slot: it re-grabs whatever quest
/// item sits at the target pocket (`mQst_CheckGrabItem(slot_item,
/// pocket_idx)`) before restoring the moved record there. The displaced
/// grab is returned so the caller can observe it; retail itself overwrites
/// the temp grab, dropping the displaced association after the restore.
pub fn check_put(
    grab: &QuestGrab,
    new_pocket: usize,
    pockets: &[u16; POCKET_COUNT],
    deliveries: &mut [DeliveryRecord; DELIVERY_SLOT_COUNT],
    errands: &mut [ErrandRecord; ERRAND_SLOT_COUNT],
) -> Option<QuestGrab> {
    if new_pocket >= POCKET_COUNT {
        return None;
    }
    let displaced = check_grab(pockets[new_pocket], new_pocket, deliveries, errands);
    if grab.is_errand {
        if let Some(e) = errands.get_mut(grab.index) {
            e.pockets_idx = new_pocket as i8;
        }
    } else {
        // Delivery: the record moves with the pocket index.
        let src = deliveries[grab.index];
        deliveries[grab.index] = DeliveryRecord::default();
        deliveries[new_pocket] = src;
    }
    displaced
}

/// Contest letter-quest runtime data (`mQst_contest_c` letter fields).
///
/// Starts with `progress = 2`, `player_id` empty, `score = 0`,
/// `present = EMPTY_NO`; the recipient is the quest-giving NPC.
#[derive(Clone, Copy, Debug, Default)]
pub struct LetterContest {
    pub player_id: Option<u32>,
    pub score: u8,
    pub present: u16,
}

/// Gate for `mQst_SetReceiveLetter`: the quest must be an active
/// CONTEST/LETTER quest with `progress == 2` and no player yet.
pub fn letter_receive_ok(progress: u8, player_id: &Option<u32>) -> bool {
    progress == 2 && player_id.is_none()
}

/// Apply an incoming letter to the contest: `player_id = sender`,
/// `progress = 1`, `score`/`present` recorded.
pub fn letter_receive(
    contest: &mut LetterContest,
    progress: u8,
    sender_id: u32,
    rank: u8,
    present: u16,
) -> bool {
    if !letter_receive_ok(progress, &contest.player_id) {
        return false;
    }
    contest.player_id = Some(sender_id);
    contest.score = rank;
    contest.present = present;
    true
}

/// Letter-quest rank 0-11.
///
/// Reuses `letter_score::quest_rank` (`mQst_GetMailRank`): length tier
/// (17 -> +1, 49 -> +2), trigram/quality bonus (+3), attached present (+6).
pub fn letter_quest_rank(body: &[u8; 192], present: bool) -> u8 {
    crate::letter_score::quest_rank(body, present, crate::letter_score::TrigramMode::Intended)
}

/// Reply-present categories (`mQst_GetPresent`).
pub mod letter_present {
    pub const NONE: u8 = 0;
    pub const NORMAL_CLOTHING: u8 = 1;
    pub const NORMAL_FURNITURE: u8 = 2;
    pub const CARPET_OR_WALLPAPER: u8 = 3;
    pub const NATIVE_FRUIT: u8 = 4;
    pub const RARE_CLOTHING: u8 = 5;
    pub const FOREIGN_FRUIT: u8 = 6;
    pub const RARE_FURNITURE: u8 = 7;
}

/// `mQst_GetPresent(rank)`: rank 0-2 -> none; 3 -> normal clothing;
/// 4 -> normal furniture; 5 -> carpet or wallpaper (50/50); 6 -> native
/// fruit; 7 -> normal clothing; 8 -> rare clothing; 9 -> foreign fruit;
/// 10 -> rare furniture; 11 -> rare carpet or wallpaper (50/50).
pub fn letter_reply_present(rank: u8) -> u8 {
    match rank {
        0 | 1 | 2 => letter_present::NONE,
        3 | 7 => letter_present::NORMAL_CLOTHING,
        4 => letter_present::NORMAL_FURNITURE,
        5 => letter_present::CARPET_OR_WALLPAPER,
        6 => letter_present::NATIVE_FRUIT,
        8 => letter_present::RARE_CLOTHING,
        9 => letter_present::FOREIGN_FRUIT,
        10 => letter_present::RARE_FURNITURE,
        _ => letter_present::CARPET_OR_WALLPAPER, // rank 11
    }
}

/// Carpet vs wallpaper 50/50 for ranks 5 and 11:
/// `(mQst_GetRandom(4) & 1) == 0` -> carpet. Returns true for carpet.
pub fn carpet_or_wallpaper(coin: u8) -> bool {
    coin & 1 == 0
}

/// Reply handbill: `0x75 + (rank * mNpc_LOOKS_NUM + looks)` — 12 ranks x 6
/// looks = 72 rank/personality message combinations, festive paper.
pub fn letter_reply_handbill(rank: u8, looks: u8) -> u16 {
    0x75 + (rank as u16) * 6 + (looks as u16)
}

/// First-job letter kinds (`mQst_ERRAND_FIRSTJOB_SEND_LETTER[_2]`).
///
/// These are ERRAND/FIRST_JOB quests, not CONTEST_KIND_LETTER quests.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FirstJobLetterKind {
    SendLetter,
    SendLetter2,
}

/// First-job letter quest state.
#[derive(Clone, Copy, Debug)]
pub struct FirstJobLetter {
    pub kind: FirstJobLetterKind,
    /// Setup: progress = 2, no time limit, no reward, no entrusted item.
    pub progress: u8,
    pub recipient: u32,
    /// `used_ids[1]` = letter recipient; `used_num` = 2.
    pub used_ids: [Option<u32>; 2],
    pub used_num: u8,
}

impl FirstJobLetter {
    pub fn new(kind: FirstJobLetterKind, recipient: u32, furniture_user: Option<u32>) -> Self {
        FirstJobLetter {
            kind,
            progress: 2,
            recipient,
            used_ids: [furniture_user, Some(recipient)],
            used_num: 2,
        }
    }

    /// Completion trigger: the relevant letter event sets progress = 3 and
    /// clears `memory->letter_info.send_reply`.
    pub fn complete(&mut self) {
        self.progress = 3;
    }
}

// ---- C ABI ----

/// C ABI: 1 if the first-job generation table applies.
#[no_mangle]
pub extern "C" fn pc_quest_use_first_job_table(is_man_kind: u8, firstjob_event: u8) -> u8 {
    use_first_job_table(is_man_kind != 0, firstjob_event != 0) as u8
}

/// C ABI: quest type from the generation table + roll (0xFF if invalid).
#[no_mangle]
pub extern "C" fn pc_quest_type_select(first_job: u8, roll: u32) -> u8 {
    select_quest_type(first_job != 0, roll)
}

/// C ABI: kind count for (type, first_job).
#[no_mangle]
pub extern "C" fn pc_quest_kind_count(quest_type: u8, first_job: u8) -> u8 {
    quest_kind_count(quest_type, first_job != 0) as u8
}

/// C ABI: quest kind from the kind table + roll (0xFF if none).
#[no_mangle]
pub extern "C" fn pc_quest_kind_select(quest_type: u8, first_job: u8, roll: u32) -> u8 {
    select_quest_kind(quest_type, first_job != 0, roll).unwrap_or(0xFF)
}

/// C ABI: kind occurrence check (`aQMgr_actor_check_occur`).
#[no_mangle]
pub extern "C" fn pc_quest_occur_ok(
    quest_type: u8,
    kind: u8,
    month: u8,
    day: u8,
    hour: u8,
    empty_spaces: u8,
    flower_count: u8,
    local_player: u8,
    foreign_ok: u8,
    removed_ok: u8,
    contest_active: u8,
) -> u8 {
    let ctx = OccurCtx {
        month,
        day,
        hour,
        empty_acre_spaces: empty_spaces,
        flower_count,
        local_player: local_player != 0,
        foreign_ok: foreign_ok != 0,
        removed_ok: removed_ok != 0,
        contest_active: contest_active != 0,
    };
    occur_ok(quest_type, kind, &ctx) as u8
}

/// C ABI: recipient target mode from `l_set_data[type][kind]` (0xFF if none).
#[no_mangle]
pub extern "C" fn pc_quest_target(quest_type: u8, kind: u8) -> u8 {
    set_data(quest_type, kind)
        .map(|d| d.target as u8)
        .unwrap_or(0xFF)
}

/// C ABI: day limit from `l_set_data[type][kind]` (-1 if none).
#[no_mangle]
pub extern "C" fn pc_quest_set_day_limit(quest_type: u8, kind: u8) -> i8 {
    set_data(quest_type, kind)
        .map(|d| d.day_limit as i8)
        .unwrap_or(-1)
}

/// C ABI: 1 if `l_set_data[type][kind]` hands over an entrusted item.
#[no_mangle]
pub extern "C" fn pc_quest_set_handover(quest_type: u8, kind: u8) -> u8 {
    set_data(quest_type, kind)
        .map(|d| d.handover_item as u8)
        .unwrap_or(0)
}

/// C ABI: item source from `l_set_data[type][kind]` (0xFF if none).
#[no_mangle]
pub extern "C" fn pc_quest_item_source(quest_type: u8, kind: u8) -> u8 {
    set_data(quest_type, kind)
        .map(|d| d.item_source as u8)
        .unwrap_or(0xFF)
}

/// C ABI: max pay from `l_set_data[type][kind]` (0 if none).
#[no_mangle]
pub extern "C" fn pc_quest_set_max_pay(quest_type: u8, kind: u8) -> u32 {
    set_data(quest_type, kind).map(|d| d.max_pay).unwrap_or(0)
}

/// C ABI: first empty pocket index, or -1 when full.
///
/// # Safety
/// `pockets` must point to 15 readable `u16` pocket slots.
#[no_mangle]
pub unsafe extern "C" fn pc_first_empty_pocket(pockets: *const u16) -> i8 {
    if pockets.is_null() {
        return -1;
    }
    // SAFETY: caller guarantees 15 readable u16s per the C ABI contract.
    let slice = unsafe { std::slice::from_raw_parts(pockets, POCKET_COUNT) };
    let mut arr = [EMPTY_NO; POCKET_COUNT];
    arr.copy_from_slice(slice);
    first_empty_pocket(&arr).map(|i| i as i8).unwrap_or(-1)
}

/// C ABI: `mPr_ITEM_COND_QUEST`.
#[no_mangle]
pub extern "C" fn pc_item_condition_quest() -> u8 {
    item_cond::QUEST
}

/// C ABI: reply-present category for a letter-quest rank (`mQst_GetPresent`).
#[no_mangle]
pub extern "C" fn pc_letter_present_category(rank: u8) -> u8 {
    letter_reply_present(rank)
}

/// C ABI: 1 if the 50/50 roll picks carpet (ranks 5/11).
#[no_mangle]
pub extern "C" fn pc_letter_carpet_or_wallpaper(coin: u8) -> u8 {
    carpet_or_wallpaper(coin) as u8
}

/// C ABI: reply handbill id for rank + looks.
#[no_mangle]
pub extern "C" fn pc_letter_handbill(rank: u8, looks: u8) -> u16 {
    letter_reply_handbill(rank, looks)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn villagers() -> Vec<VillagerRef> {
        vec![
            VillagerRef { id: 101, acre_x: 0, acre_z: 0 }, // giver
            VillagerRef { id: 102, acre_x: 0, acre_z: 0 }, // same acre -> excluded
            VillagerRef { id: 103, acre_x: 1, acre_z: 0 },
            VillagerRef { id: 104, acre_x: 2, acre_z: 1 },
        ]
    }

    #[test]
    fn type_kind_tables() {
        assert_eq!(QUEST_TYPE_TABLE_FJ, [qtype::DELIVERY, qtype::ERRAND]);
        assert_eq!(
            QUEST_TYPE_TABLE_QST,
            [qtype::DELIVERY, qtype::ERRAND, qtype::CONTEST]
        );
        assert!(use_first_job_table(false, true));
        assert!(!use_first_job_table(true, true));
        assert!(!use_first_job_table(false, false));
        assert_eq!(select_quest_type(true, 0), qtype::DELIVERY);
        assert_eq!(select_quest_type(true, 1), qtype::ERRAND);
        assert_eq!(select_quest_type(false, 2), qtype::CONTEST);
        assert_eq!(select_quest_type(false, 3), qtype::DELIVERY); // wraps
        // Kind tables.
        assert_eq!(quest_kind_count(qtype::DELIVERY, true), 2);
        assert_eq!(quest_kind_count(qtype::DELIVERY, false), 4);
        assert_eq!(quest_kind_count(qtype::ERRAND, false), 1);
        assert_eq!(quest_kind_count(qtype::CONTEST, true), 0); // no fj contests
        assert_eq!(quest_kind_count(qtype::CONTEST, false), 7);
        assert_eq!(
            select_quest_kind(qtype::DELIVERY, false, 3),
            Some(dkind::LOST)
        );
        assert_eq!(
            select_quest_kind(qtype::CONTEST, false, 6),
            Some(ckind::LETTER)
        );
        assert_eq!(select_quest_kind(qtype::CONTEST, true, 0), None);
        // C ABI.
        assert_eq!(pc_quest_type_select(1, 1), qtype::ERRAND);
        assert_eq!(pc_quest_kind_count(qtype::CONTEST, 0), 7);
        assert_eq!(pc_quest_kind_select(qtype::CONTEST, 0, 6), ckind::LETTER);
        assert_eq!(pc_quest_kind_select(qtype::CONTEST, 1, 0), 0xFF);
        assert_eq!(pc_quest_use_first_job_table(0, 1), 1);
    }

    #[test]
    fn occur_gating() {
        let base = OccurCtx {
            local_player: true,
            foreign_ok: true,
            removed_ok: true,
            ..OccurCtx::default()
        };
        // Snowman: Jan / Feb 1-17 / Dec 25-31, hour 8 through 16 (16:59).
        let jan = OccurCtx { month: 1, day: 15, hour: 10, ..base };
        assert!(occur_ok(qtype::CONTEST, ckind::SNOWMAN, &jan));
        let jan16 = OccurCtx { hour: 16, ..jan };
        assert!(occur_ok(qtype::CONTEST, ckind::SNOWMAN, &jan16));
        let jan17 = OccurCtx { hour: 17, ..jan };
        assert!(!occur_ok(qtype::CONTEST, ckind::SNOWMAN, &jan17));
        let feb18 = OccurCtx { month: 2, day: 18, hour: 10, ..base };
        assert!(!occur_ok(qtype::CONTEST, ckind::SNOWMAN, &feb18));
        let night = OccurCtx { month: 1, day: 15, hour: 20, ..base };
        assert!(!occur_ok(qtype::CONTEST, ckind::SNOWMAN, &night));
        let dec24 = OccurCtx { month: 12, day: 24, hour: 10, ..base };
        assert!(!occur_ok(qtype::CONTEST, ckind::SNOWMAN, &dec24));
        // Flower: Feb 25+ through Aug, >=4 empty, <=20 flowers.
        let mar = OccurCtx {
            month: 3, day: 1, empty_acre_spaces: 4, flower_count: 20, ..base
        };
        assert!(occur_ok(qtype::CONTEST, ckind::FLOWER, &mar));
        let crowded = OccurCtx { flower_count: 21, ..mar };
        assert!(!occur_ok(qtype::CONTEST, ckind::FLOWER, &crowded));
        let feb20 = OccurCtx { month: 2, day: 20, ..mar };
        assert!(!occur_ok(qtype::CONTEST, ckind::FLOWER, &feb20));
        // Insect: Mar-Oct, Nov 1-28.
        assert!(occur_ok(
            qtype::CONTEST,
            ckind::INSECT,
            &OccurCtx { month: 11, day: 28, ..base }
        ));
        assert!(!occur_ok(
            qtype::CONTEST,
            ckind::INSECT,
            &OccurCtx { month: 11, day: 29, ..base }
        ));
        assert!(!occur_ok(
            qtype::CONTEST,
            ckind::INSECT,
            &OccurCtx { month: 12, day: 1, ..base }
        ));
        // Letter needs a local player.
        assert!(!occur_ok(
            qtype::CONTEST,
            ckind::LETTER,
            &OccurCtx { local_player: false, ..base }
        ));
        // Foreign/remove gates.
        assert!(!occur_ok(
            qtype::DELIVERY,
            dkind::FOREIGN,
            &OccurCtx { foreign_ok: false, ..base }
        ));
        assert!(!occur_ok(
            qtype::DELIVERY,
            dkind::REMOVE,
            &OccurCtx { removed_ok: false, ..base }
        ));
        // A contest kind already active blocks generation.
        assert!(!occur_ok(
            qtype::CONTEST,
            ckind::FRUIT,
            &OccurCtx { contest_active: true, ..base }
        ));
        assert!(occur_ok(
            qtype::CONTEST,
            ckind::FRUIT,
            &OccurCtx { contest_active: false, ..base }
        ));
        // C ABI spot check.
        assert_eq!(pc_quest_occur_ok(qtype::CONTEST, ckind::SNOWMAN, 1, 15, 10, 0, 0, 1, 1, 1, 0), 1);
        assert_eq!(pc_quest_occur_ok(qtype::CONTEST, ckind::SNOWMAN, 1, 15, 20, 0, 0, 1, 1, 1, 0), 0);
    }

    #[test]
    fn recipient_modes() {
        let v = villagers();
        let ctx = RecipientCtx {
            villagers: &v,
            giver_idx: 0,
            chain_used: [Some(103), None, None],
            foreign_id: Some(201),
            last_removed_id: Some(202),
            client_idx: 0,
        };
        // RANDOM: giver (101) and same-acre 102 excluded -> {103, 104}.
        assert_eq!(resolve_recipient(QuestTarget::Random, &ctx, 0), Ok(103));
        assert_eq!(resolve_recipient(QuestTarget::Random, &ctx, 1), Ok(104));
        assert_eq!(resolve_recipient(QuestTarget::Random, &ctx, 2), Ok(103)); // wraps
        // RANDOM_EXCLUDED: also excludes chain 103 -> {104}.
        assert_eq!(
            resolve_recipient(QuestTarget::RandomExcluded, &ctx, 0),
            Ok(104)
        );
        // ORIGINAL_TARGET: chain head.
        assert_eq!(
            resolve_recipient(QuestTarget::OriginalTarget, &ctx, 0),
            Ok(103)
        );
        let ctx2 = RecipientCtx {
            chain_used: [None, None, None],
            ..ctx
        };
        // Falls back to the giver.
        assert_eq!(
            resolve_recipient(QuestTarget::OriginalTarget, &ctx2, 0),
            Ok(101)
        );
        // FOREIGN / LAST_REMOVE.
        assert_eq!(resolve_recipient(QuestTarget::Foreign, &ctx, 0), Ok(201));
        assert_eq!(
            resolve_recipient(QuestTarget::LastRemove, &ctx, 0),
            Ok(202)
        );
        let ctx3 = RecipientCtx {
            foreign_id: None,
            last_removed_id: None,
            ..ctx
        };
        assert_eq!(
            resolve_recipient(QuestTarget::Foreign, &ctx3, 0),
            Err(RecipientError::NoForeignId)
        );
        assert_eq!(
            resolve_recipient(QuestTarget::LastRemove, &ctx3, 0),
            Err(RecipientError::NoRemoveAnimalId)
        );
        // CLIENT: the giver itself.
        assert_eq!(resolve_recipient(QuestTarget::Client, &ctx, 0), Ok(101));
    }

    #[test]
    fn set_data_tables() {
        // Delivery NORMAL: RANDOM, 2d, handover, CLOTH, max 200.
        let d = set_data(qtype::DELIVERY, dkind::NORMAL).unwrap();
        assert_eq!(d.target, QuestTarget::Random);
        assert_eq!(d.day_limit, 2);
        assert!(d.handover_item);
        assert_eq!(d.item_source, QuestItemSource::Cloth);
        assert_eq!(d.max_pay, 200);
        assert_eq!(d.reward_percentages, [40, 0, 0, 0, 0, 30, 30, 0]);
        assert_eq!(d.msg_start.len(), MSG_KIND_NUM);
        let df = set_data(qtype::DELIVERY, dkind::FOREIGN).unwrap();
        assert_eq!(df.target, QuestTarget::Foreign);
        assert_eq!(df.max_pay, 1000);
        // Errand chain stages.
        let e0 = set_data(qtype::ERRAND, ekind::REQUEST).unwrap();
        assert_eq!(e0.target, QuestTarget::RandomExcluded);
        assert_eq!(e0.last_step, 4);
        assert!(!e0.handover_item);
        let e2 = set_data(qtype::ERRAND, ekind::REQUEST_FINAL).unwrap();
        assert_eq!(e2.target, QuestTarget::OriginalTarget);
        assert!(e2.handover_item);
        assert_eq!(e2.item_source, QuestItemSource::CurrentItem);
        // First-job rows are CLIENT with no handover.
        let fj = set_data(qtype::ERRAND, fjekind::SEND_LETTER).unwrap();
        assert_eq!(fj.target, QuestTarget::Client);
        assert!(!fj.handover_item);
        // Contest rows: all CLIENT.
        for k in 0..7u8 {
            assert_eq!(
                set_data(qtype::CONTEST, k).unwrap().target,
                QuestTarget::Client
            );
        }
        let letter = set_data(qtype::CONTEST, ckind::LETTER).unwrap();
        assert_eq!(letter.day_limit, 2);
        assert_eq!(letter.last_step, 2);
        let fruit = set_data(qtype::CONTEST, ckind::FRUIT).unwrap();
        assert_eq!(fruit.item_source, QuestItemSource::Fruit);
        assert_eq!(fruit.max_pay, 500);
        assert!(set_data(qtype::DELIVERY, 9).is_none());
        // Quest init from set data: progress = last_step, limit iff day_limit != 0.
        let (prog, limited) = quest_init_from_set_data(letter);
        assert_eq!(prog, 2);
        assert!(limited);
        let fj_init = quest_init_from_set_data(fj);
        assert_eq!(fj_init, (0, false));
        // Item-source resolution.
        assert_eq!(
            resolve_item_source(QuestItemSource::Fruit, 0, None, 0x1601, 0x9999, 0x2402),
            0x1601 // town's non-native fruit
        );
        assert_eq!(
            resolve_item_source(QuestItemSource::CurrentItem, 0, Some(0x3001), 0, 0, 0),
            0x3001
        );
        assert_eq!(
            resolve_item_source(QuestItemSource::CurrentItem, 0, None, 0, 0, 0),
            ITM_CLOTH001
        );
        assert_eq!(
            resolve_item_source(QuestItemSource::Random, 0, None, 0, 0x9999, 0),
            0x9999
        );
        assert_eq!(
            resolve_item_source(QuestItemSource::None, 0, None, 0, 0, 0),
            EMPTY_NO
        );
        assert_eq!(ITM_CLOTH001, 0x2401);
        // C ABI.
        assert_eq!(pc_quest_target(qtype::DELIVERY, dkind::FOREIGN), 3);
        assert_eq!(pc_quest_target(qtype::CONTEST, ckind::LETTER), 5);
        assert_eq!(pc_quest_target(qtype::DELIVERY, 9), 0xFF);
        assert_eq!(pc_quest_set_day_limit(qtype::CONTEST, ckind::LETTER), 2);
        assert_eq!(pc_quest_set_handover(qtype::DELIVERY, dkind::NORMAL), 1);
        assert_eq!(pc_quest_set_handover(qtype::CONTEST, ckind::LETTER), 0);
        assert_eq!(pc_quest_item_source(qtype::DELIVERY, dkind::NORMAL), 2);
        assert_eq!(pc_quest_set_max_pay(qtype::DELIVERY, dkind::FOREIGN), 1000);
    }

    #[test]
    fn entrusted_items() {
        let mut pockets = [EMPTY_NO; POCKET_COUNT];
        pockets[0] = 0x1234;
        assert_eq!(first_empty_pocket(&pockets), Some(1));
        let full = [0x1111u16; POCKET_COUNT];
        assert_eq!(first_empty_pocket(&full), None);

        // Delivery slot: record i <-> pocket i.
        let mut deliveries = [DeliveryRecord::default(); DELIVERY_SLOT_COUNT];
        deliveries[1].occupied = true;
        deliveries[1].item = 0x1234;
        assert_eq!(find_free_delivery(&deliveries, &pockets), Some(2));
        pockets[2] = 0x2222;
        assert_eq!(find_free_delivery(&deliveries, &pockets), Some(3));

        // Handover binds the first empty pocket.
        let idx = handover_item(&mut pockets, 0x3001).unwrap();
        assert_eq!(idx, 3);
        assert_eq!(pockets[3], 0x3001);
        assert_eq!(handover_item(&mut full.clone(), 0x3001), Err(QuestGenError::NoSpace));

        // Grab/put: delivery keeps the pocket index.
        let mut errands = [ErrandRecord::default(); ERRAND_SLOT_COUNT];
        errands[2].occupied = true;
        errands[2].item = 0x4001;
        errands[2].pockets_idx = 5;
        let g = check_grab(0x1234, 1, &deliveries, &errands).unwrap();
        assert!(!g.is_errand);
        assert_eq!(g.index, 1); // pocket index for delivery
        let ge = check_grab(0x4001, 5, &deliveries, &errands).unwrap();
        assert!(ge.is_errand);
        assert_eq!(ge.index, 2); // errand index, NOT pocket (retail quirk)
        assert!(check_put(&ge, 7, &pockets, &mut deliveries, &mut errands).is_none());
        assert_eq!(errands[2].pockets_idx, 7);
        assert!(check_put(&g, 9, &pockets, &mut deliveries, &mut errands).is_none());
        assert!(deliveries[9].occupied);
        assert_eq!(deliveries[9].item, 0x1234);
        assert!(!deliveries[1].occupied);
        // Displacement: putting into a quest-occupied pocket displaces it.
        deliveries[9].occupied = true;
        deliveries[9].item = 0x1234;
        let mut pockets2 = pockets;
        pockets2[9] = 0x1234;
        deliveries[4].occupied = true;
        deliveries[4].item = 0x5555;
        pockets2[4] = 0x5555;
        let g2 = check_grab(0x5555, 4, &deliveries, &errands).unwrap();
        let displaced = check_put(&g2, 9, &pockets2, &mut deliveries, &mut errands);
        let d = displaced.expect("destination quest item is displaced");
        assert!(!d.is_errand && d.index == 9 && d.item == 0x1234);
        assert_eq!(deliveries[9].item, 0x5555); // moved record wins the slot
        // Item-condition values.
        assert_eq!(item_cond::QUEST, 2);
        assert_eq!(pc_item_condition_quest(), 2);
        // C ABI pocket scan.
        let mut pk = [EMPTY_NO; POCKET_COUNT];
        pk[0] = 0x1111;
        assert_eq!(unsafe { pc_first_empty_pocket(pk.as_ptr()) }, 1);
        assert_eq!(unsafe { pc_first_empty_pocket(std::ptr::null()) }, -1);
    }

    #[test]
    fn letter_quest() {
        let mut lc = LetterContest::default();
        assert!(letter_receive_ok(2, &lc.player_id));
        assert!(!letter_receive_ok(1, &lc.player_id));
        assert!(letter_receive(&mut lc, 2, 777, 11, 0x2001));
        assert_eq!(lc.player_id, Some(777));
        assert_eq!(lc.score, 11);
        // Second letter does not re-trigger (player_id already set).
        assert!(!letter_receive(&mut lc, 2, 778, 5, 0x2002));
        // Present table.
        assert_eq!(letter_reply_present(0), letter_present::NONE);
        assert_eq!(letter_reply_present(3), letter_present::NORMAL_CLOTHING);
        assert_eq!(letter_reply_present(4), letter_present::NORMAL_FURNITURE);
        assert_eq!(letter_reply_present(5), letter_present::CARPET_OR_WALLPAPER);
        assert_eq!(letter_reply_present(6), letter_present::NATIVE_FRUIT);
        assert_eq!(letter_reply_present(8), letter_present::RARE_CLOTHING);
        assert_eq!(letter_reply_present(9), letter_present::FOREIGN_FRUIT);
        assert_eq!(letter_reply_present(10), letter_present::RARE_FURNITURE);
        assert_eq!(letter_reply_present(11), letter_present::CARPET_OR_WALLPAPER);
        assert!(carpet_or_wallpaper(0));
        assert!(!carpet_or_wallpaper(1));
        // Handbill: 0x75 + rank*6 + looks.
        assert_eq!(letter_reply_handbill(0, 0), 0x75);
        assert_eq!(letter_reply_handbill(11, 5), 0x75 + 66 + 5);
        // Rank pipeline: reuse of letter_score::quest_rank.
        let mut body = [b' '; 192];
        body[..60].fill(b'a');
        assert_eq!(letter_quest_rank(&body, false), 2); // 60 chars, no present
        assert_eq!(letter_quest_rank(&body, true), 8); // +6 present
        // First-job letters are ERRAND/FIRST_JOB, not contest letters.
        let mut fj = FirstJobLetter::new(FirstJobLetterKind::SendLetter, 555, Some(444));
        assert_eq!(fj.progress, 2);
        assert_eq!(fj.used_ids, [Some(444), Some(555)]);
        assert_eq!(fj.used_num, 2);
        fj.complete();
        assert_eq!(fj.progress, 3);
        // C ABI.
        assert_eq!(pc_letter_present_category(10), letter_present::RARE_FURNITURE);
        assert_eq!(pc_letter_carpet_or_wallpaper(2), 1);
        assert_eq!(pc_letter_carpet_or_wallpaper(3), 0);
        assert_eq!(pc_letter_handbill(11, 5), 0x75 + 66 + 5);
    }
}
