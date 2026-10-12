//! First-job errands and the quest bookkeeping missing from `quest.rs`
//! (`m_quest.c`): Tom Nook's first-day errand chain (setters for every
//! step), the first-job slot lookup, pocket-item -> quest lookups,
//! deadline checks by pocket slot, and the snowman / soccer contest hooks.
//!
//! Source-verified against ac-decomp `src/game/m_quest.c` + `include/m_quest.h`
//! (primary) and ACGC-PC-Port (secondary; only cosmetic differences in
//! these functions). Reuses `quest::QuestBase` / `qtype` / `ckind`.
//!
//! ## Facts
//! - `mQst_GetFirstJobData`: prefer an existing first-job errand among the
//!   5 errand slots, otherwise the first free slot (may be none).
//! - Every first-job step marks the errand as an ERRAND of type FIRST_JOB,
//!   turns the time limit off, sets progress 2 and clears `give_reward`.
//!   The steps and what they record:
//!   - furniture delivery: the recipient, `used_ids[0]` (used_num 1), the
//!     pocket slot and the item
//!   - letter (1 and 2): the recipient, `used_ids[1]` (used_num 2)
//!   - carpet / axe (1 and 2): the recipient, pocket slot and item, but no
//!     used id
//!   - notice / open-quest: the recipient is cleared; open-quest also sets
//!     the per-player `FJOPENQUEST` event flag
//!   - change-cloth: only the item; plant-flower and introductions: nothing
//!     extra
//! - Deliveries are indexed by pocket slot (`deliveries[pocket_idx]`).
//!   Errands are found by pocket slot AND item, and only when the pocket
//!   item is in the QUEST condition.
//! - Snowman contest (progress 1): finishing a snowman in the requester's
//!   home acre records the player; pushing that snowman apart clears it.
//!   Soccer contest (progress 2): kicking the ball to the requester counts.
//!
//! ## Quirks
//! - `mQst_SetFirstJobStart` leaves progress at 0, and `mQst_CheckFirstJobFin`
//!   means progress == 0, so a just-started chain reads as finished.
//! - Only the furniture and letter steps record a villager in `used_ids`.

use crate::quest::{is_free, qtype, QuestBase};

pub const ERRAND_QUEST_NUM: usize = 5;
pub const POCKETS_SLOT_COUNT: usize = 15;
pub const FIRST_JOB_ANIMAL_NUM: usize = 2;

/// `mQst_ERRAND_TYPE_*`.
pub const ERRAND_TYPE_NONE: u8 = 0;
pub const ERRAND_TYPE_CHAIN: u8 = 1;
pub const ERRAND_TYPE_FIRST_JOB: u8 = 2;

/// `mQst_ERRAND_FIRSTJOB_*` kinds (after the three request kinds).
pub mod fj {
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

/// Villager identity as quests store it (`AnmPersonalID_c`, reduced to the
/// npc id; 0 = cleared).
pub type AnimalId = u16;

/// `mQst_errand_c` (the fields the first-job steps touch).
#[derive(Clone, Copy, Default)]
pub struct Errand {
    pub base: QuestBase,
    pub errand_type: u8,
    pub recipient: AnimalId,
    pub used_ids: [AnimalId; FIRST_JOB_ANIMAL_NUM],
    pub used_num: u8,
    pub pockets_idx: i32,
    pub item: u16,
}

impl Errand {
    /// `mQst_ClearErrand`.
    pub fn cleared() -> Self {
        let mut e = Self::default();
        e.base.quest_type = qtype::NONE;
        e.pockets_idx = -1;
        e
    }

    fn first_job_common(&mut self, kind: u8) {
        self.base.quest_type = qtype::ERRAND;
        self.base.quest_kind = kind;
        self.base.time_limit_enabled = false;
        self.base.progress = 2;
        self.base.give_reward = false;
        self.errand_type = ERRAND_TYPE_FIRST_JOB;
    }

    /// `mQst_CheckFirstJobQuest`.
    pub fn is_first_job(&self) -> bool {
        self.base.quest_type == qtype::ERRAND && self.errand_type == ERRAND_TYPE_FIRST_JOB
    }
}

/// `mQst_GetFirstJobData`: index of an existing first-job errand, else the
/// first free errand slot.
pub fn first_job_slot(errands: &[Errand; ERRAND_QUEST_NUM]) -> Option<usize> {
    errands.iter().position(|e| e.is_first_job()).or_else(|| errands.iter().position(|e| is_free(&e.base)))
}

/// `mQst_SetFirstJobStart`.
pub fn set_start(e: &mut Errand) {
    *e = Errand::cleared();
    e.base.quest_type = qtype::ERRAND;
    e.base.quest_kind = fj::START;
    e.base.progress = 0;
    e.errand_type = ERRAND_TYPE_FIRST_JOB;
    e.used_ids = [0; FIRST_JOB_ANIMAL_NUM];
}

/// `mQst_SetFirstJobChangeCloth`.
pub fn set_change_cloth(e: &mut Errand, item: u16) {
    e.first_job_common(fj::CHANGE_CLOTH);
    e.item = item;
}

/// `mQst_SetFirstJobSeed` / `mQst_SetFirstJobHello`.
pub fn set_plant_flower(e: &mut Errand) {
    e.first_job_common(fj::PLANT_FLOWER);
}
pub fn set_introductions(e: &mut Errand) {
    e.first_job_common(fj::INTRODUCTIONS);
}

/// `mQst_SetFirstJobFurniture`: records the recipient in `used_ids[0]`.
pub fn set_furniture(e: &mut Errand, recipient: AnimalId, item: u16, slot: i32) {
    e.first_job_common(fj::DELIVER_FTR);
    e.recipient = recipient;
    e.used_ids[0] = recipient;
    e.used_num = 1;
    e.pockets_idx = slot;
    e.item = item;
}

/// `mQst_SetFirstJobLetter` / `Letter2`: records the recipient in `used_ids[1]`.
pub fn set_letter(e: &mut Errand, recipient: AnimalId, second: bool) {
    e.first_job_common(if second { fj::SEND_LETTER2 } else { fj::SEND_LETTER });
    e.recipient = recipient;
    e.used_ids[1] = recipient;
    e.used_num = 2;
}

/// `mQst_SetFirstJobCarpet` / `Axe` / `Axe2` (no used id recorded).
pub fn set_delivery_step(e: &mut Errand, kind: u8, recipient: AnimalId, item: u16, slot: i32) {
    e.first_job_common(kind);
    e.recipient = recipient;
    e.pockets_idx = slot;
    e.item = item;
}

/// `mQst_SetFirstJobNotice` / `OpenQuest` (recipient cleared). Returns
/// whether the per-player FJOPENQUEST event flag must be set.
pub fn set_notice_or_open(e: &mut Errand, open: bool) -> bool {
    e.first_job_common(if open { fj::OPEN } else { fj::POST_NOTICE });
    e.recipient = 0;
    open
}

/// `mQst_CheckFirstJobFin` (progress == 0; true right after `set_start`).
pub fn first_job_finished(e: &Errand) -> bool {
    e.base.progress == 0
}

/// Item condition values (`mPr_ITEM_COND_*`).
pub const ITEM_COND_NORMAL: u8 = 0;
pub const ITEM_COND_PRESENT: u8 = 1;
pub const ITEM_COND_QUEST: u8 = 2;

/// `mQst_GetErrandIdxbyItemIdx`.
pub fn errand_for_pocket(errands: &[Errand; ERRAND_QUEST_NUM], pocket: usize, item: u16, cond: u8) -> Option<usize> {
    if pocket >= POCKETS_SLOT_COUNT || cond != ITEM_COND_QUEST {
        return None;
    }
    errands.iter().position(|e| !is_free(&e.base) && e.pockets_idx == pocket as i32 && e.item == item)
}

/// `mQst_CheckFirstJobQuestbyItemIdx`.
pub fn pocket_is_first_job(errands: &[Errand; ERRAND_QUEST_NUM], pocket: usize, item: u16, cond: u8) -> bool {
    errand_for_pocket(errands, pocket, item, cond).is_some_and(|i| errands[i].is_first_job())
}

/// `mQst_GetDeliveryIdxbyItemIdx`: deliveries share the pocket index.
pub fn delivery_for_pocket(deliveries: &[QuestBase; POCKETS_SLOT_COUNT], pocket: usize) -> Option<usize> {
    (pocket < POCKETS_SLOT_COUNT && !is_free(&deliveries[pocket])).then_some(pocket)
}

/// `mQst_GetOccuredDeliveryIdx`.
pub fn occurred_delivery(deliveries: &[QuestBase; POCKETS_SLOT_COUNT], kind: u8) -> Option<usize> {
    deliveries.iter().position(|d| !is_free(d) && d.quest_type == qtype::DELIVERY && d.quest_kind == kind)
}

/// `mQst_CheckLimitbyPossessionIdx`: the pocket item is overdue as a
/// delivery or as a matching errand. `over(base)` = `mQst_CheckLimitOver`.
pub fn pocket_overdue(
    deliveries: &[QuestBase; POCKETS_SLOT_COUNT],
    errands: &[Errand; ERRAND_QUEST_NUM],
    pocket: usize,
    item: u16,
    over: impl Fn(&QuestBase) -> bool,
) -> bool {
    if pocket >= POCKETS_SLOT_COUNT || item == 0 {
        return false;
    }
    let d = &deliveries[pocket];
    if !is_free(d) && over(d) {
        return true;
    }
    errands.iter().any(|e| !is_free(&e.base) && e.pockets_idx == pocket as i32 && e.item == item && over(&e.base))
}

/// What a snowman event does to the snowman contest (`mQst_Next/BackSnowman`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SnowmanContest {
    Unchanged,
    RecordPlayer,
    ClearPlayer,
}

/// `built` = Next (finished), else Back (pushed apart).
pub fn snowman_contest(progress: u8, snowman_acre: (i32, i32), requester_home: (i32, i32), built: bool) -> SnowmanContest {
    if progress == 1 && snowman_acre == requester_home {
        if built {
            SnowmanContest::RecordPlayer
        } else {
            SnowmanContest::ClearPlayer
        }
    } else {
        SnowmanContest::Unchanged
    }
}

/// `mQst_CheckSoccerTarget`: the kicked-to NPC is the soccer requester and
/// the contest is at progress 2.
pub fn soccer_target(is_npc_actor: bool, progress: u8, npc_is_requester: bool) -> bool {
    is_npc_actor && progress == 2 && npc_is_requester
}

#[cfg(test)]
mod tests {
    use super::*;

    fn free_errands() -> [Errand; ERRAND_QUEST_NUM] {
        [Errand::cleared(); ERRAND_QUEST_NUM]
    }

    #[test]
    fn first_job_chain() {
        let mut es = free_errands();
        assert_eq!(first_job_slot(&es), Some(0));
        es[0].base.quest_type = qtype::DELIVERY;
        set_start(&mut es[2]);
        assert_eq!(first_job_slot(&es), Some(2));
        assert!(first_job_finished(&es[2]));
        set_furniture(&mut es[2], 0xE001, 0x1234, 4);
        assert_eq!((es[2].used_ids, es[2].used_num, es[2].base.progress), ([0xE001, 0], 1, 2));
        set_letter(&mut es[2], 0xE002, false);
        assert_eq!((es[2].used_ids, es[2].used_num, es[2].base.quest_kind), ([0xE001, 0xE002], 2, fj::SEND_LETTER));
        set_delivery_step(&mut es[2], fj::DELIVER_AXE, 0xE003, 0x2201, 5);
        assert_eq!(es[2].used_ids, [0xE001, 0xE002]);
        assert!(set_notice_or_open(&mut es[2], true));
        assert_eq!(es[2].recipient, 0);
        assert!(!first_job_finished(&es[2]));
    }

    #[test]
    fn pocket_lookups() {
        let mut es = free_errands();
        set_furniture(&mut es[1], 0xE001, 0x1234, 4);
        assert_eq!(errand_for_pocket(&es, 4, 0x1234, ITEM_COND_QUEST), Some(1));
        assert_eq!(errand_for_pocket(&es, 4, 0x1234, ITEM_COND_NORMAL), None);
        assert!(pocket_is_first_job(&es, 4, 0x1234, ITEM_COND_QUEST));
        let mut d = [QuestBase { quest_type: qtype::NONE, ..Default::default() }; POCKETS_SLOT_COUNT];
        d[6].quest_type = qtype::DELIVERY;
        d[6].quest_kind = 2;
        assert_eq!(delivery_for_pocket(&d, 6), Some(6));
        assert_eq!(occurred_delivery(&d, 2), Some(6));
        assert!(pocket_overdue(&d, &es, 6, 0x9, |_| true));
        assert!(!pocket_overdue(&d, &es, 6, 0, |_| true));
    }

    #[test]
    fn contests() {
        assert_eq!(snowman_contest(1, (2, 3), (2, 3), true), SnowmanContest::RecordPlayer);
        assert_eq!(snowman_contest(1, (2, 3), (2, 3), false), SnowmanContest::ClearPlayer);
        assert_eq!(snowman_contest(2, (2, 3), (2, 3), true), SnowmanContest::Unchanged);
        assert!(soccer_target(true, 2, true) && !soccer_target(true, 1, true));
    }
}
