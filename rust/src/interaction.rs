//! Villager interest & interaction for the Rust rewrite.
//!
//! Source-verified (upstream `include/m_npc.h`, `src/game/m_npc.c`,
//! `include/m_quest.h`):
//!
//! * The decomp names the moods as "feels" (`mNpc_FEEL_*`): Normal, Happy,
//!   Angry, Sad, Sleepy, Pitfall, plus two "uzai" (pestering) feels —
//!   9 total, matching the opaque mood count in `npc.rs`.
//! * Talk frequency is a real mechanic (`m_npc.c`): each villager has
//!   talk info (timer, talk_num, quest_request flag, unlock/reset
//!   timers), and a per-feel temper table (`l_npc_temper`) gives
//!   (unlock_timer, over_impatient_num, talk_num_max). Talking past the
//!   impatient threshold yields `MILDLY_ANNOYED`; past the max yields
//!   `ANNOYED` (refuse to talk). Happy villagers lose patience faster
//!   than normal ones.
//! * Quest types (`m_quest.h`): DELIVERY (normal/foreign/removed/lost),
//!   ERRAND (chain/first-job), CONTEST (fruit, ball, snowman, flower,
//!   fish, insect, letter). Delivery quests carry sender + recipient
//!   IDs, which is the chained-favor machinery.
//!
//! The brief's core warning is honored: the GameCube has no formal
//! hobby field (that is a Wild World invention). "Interest" is produced
//! by personality dialogue pools, individual preferences, current
//! requests, inventory context, and world events — modeled here as the
//! five interest layers. Inventory inspection ("impulse buying") is
//! player-documented but not yet traced to a source function, and is
//! marked as such.

/// Mood "feels", mirroring `mNpc_FEEL_*` in order.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Feel {
    Normal = 0,
    Happy = 1,
    Angry = 2,
    Sad = 3,
    Sleepy = 4,
    Pitfall = 5,
    Uzai0 = 6,
    Uzai1 = 7,
}

pub const FEEL_NUM: usize = 6;
pub const FEEL_ALL_NUM: usize = 8;

/// Patience states, mirroring `mNpc_PATIENCE_*`.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Patience {
    MildlyAnnoyed = 0,
    Annoyed = 1,
    Normal = 2,
}

/// Per-feel temper: (unlock_timer, over_impatient_num, talk_num_max),
/// verbatim from `l_npc_temper` in `m_npc.c`.
pub const TEMPER_TABLE: [(u16, u8, u8); FEEL_NUM] = [
    (4000, 12, 15), // Normal
    (3000, 10, 13), // Happy
    (4000, 12, 15), // Angry
    (4000, 10, 13), // Sad
    (5000, 9, 12),  // Sleepy
    (5000, 9, 12),  // Pitfall
];

/// Per-villager talk state, mirroring `mNpc_Talk_Info_c`.
#[derive(Clone, Copy, Debug, Default)]
pub struct TalkInfo {
    pub timer: u16,
    pub talk_num: u8,
    pub quest_request: bool,
    pub unlock_timer: u16,
    pub reset_timer: u16,
}

impl TalkInfo {
    pub fn new() -> Self {
        Self { quest_request: true, ..Self::default() }
    }

    /// Count one conversation (`mNpc_CountTalkNum`). Returns false when
    /// the villager has hit the talk cap.
    pub fn count_talk(&mut self, feel: Feel) -> bool {
        let (_, _, talk_num_max) = TEMPER_TABLE[feel as usize % FEEL_NUM];
        if self.talk_num < talk_num_max && self.timer > 0 {
            self.talk_num += 1;
            true
        } else {
            false
        }
    }

    /// Patience level (`mNpc_GetOverImpatient`).
    pub fn patience(&self, feel: Feel) -> Patience {
        let (_, over_impatient, talk_num_max) = TEMPER_TABLE[feel as usize % FEEL_NUM];
        if self.talk_num >= talk_num_max {
            Patience::Annoyed // refuse to talk
        } else if self.talk_num >= over_impatient {
            Patience::MildlyAnnoyed
        } else {
            Patience::Normal
        }
    }

    pub fn over_impatient(&self, feel: Feel) -> bool {
        let (_, over_impatient, _) = TEMPER_TABLE[feel as usize % FEEL_NUM];
        self.talk_num >= over_impatient
    }
}

/// Quest types, mirroring `mQst_QUEST_TYPE_*`.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QuestType {
    Delivery = 0,
    Errand = 1,
    Contest = 2,
    None = 3,
}

/// Delivery kinds, mirroring `mQst_DELIVERY_KIND_*`.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeliveryKind {
    Normal = 0,
    Foreign = 1,
    Removed = 2,
    Lost = 3,
}

/// Contest kinds, mirroring `mQst_CONTEST_KIND_*`.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContestKind {
    Fruit = 0,
    Soccer = 1,
    Snowman = 2,
    Flower = 3,
    Fish = 4,
    Insect = 5,
    Letter = 6,
}

/// Errand types, mirroring the `mQst_ERRAND_TYPE_*` values.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ErrandType {
    None = 0,
    Chain = 1,
    FirstJob = 2,
}

/// The five interest layers from the research. Layers 1-2 are persistent
/// villager data, layer 3 is current request state, layers 4-5 are
/// situational. There is no single "hobby" field.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InterestLayer {
    /// Personality dialogue pools (jock -> exercise/fishing, ...).
    Personality = 0,
    /// Per-villager item preferences (observed, structure untraced).
    Individual = 1,
    /// What the villager currently wants (active request).
    CurrentDesire = 2,
    /// The player is carrying something noticeable (player-documented;
    /// source function not yet traced).
    Opportunistic = 3,
    /// Something happening nearby (fish caught, event, ...).
    Environmental = 4,
}

/// Interaction categories the talk system can select.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InteractionKind {
    Conversation = 0,
    Request = 1,
    ItemTrade = 2,
    Quiz = 3,
    Gift = 4,
    Refused = 5,
}

/// Context for interaction selection. Rewrite-owned model of the brief's
/// interaction hierarchy: availability -> mood/personality/request ->
/// special interaction vs generic talk.
#[derive(Clone, Copy, Debug)]
pub struct InteractionContext {
    pub feel: Feel,
    pub patience: Patience,
    pub has_pending_request: bool,
    pub player_holding_item: bool,
    pub talked_recently: bool,
}

impl InteractionContext {
    /// Select the interaction kind. Annoyed villagers refuse; pending
    /// requests surface as requests; a held item can trigger item
    /// interaction; otherwise ordinary conversation.
    pub fn select(&self) -> InteractionKind {
        if self.patience == Patience::Annoyed {
            return InteractionKind::Refused;
        }
        if self.has_pending_request {
            return InteractionKind::Request;
        }
        if self.player_holding_item {
            return InteractionKind::ItemTrade;
        }
        InteractionKind::Conversation
    }
}

/// C ABI: patience level for a villager (0 = mildly annoyed,
/// 1 = annoyed/refuse, 2 = normal).
#[no_mangle]
pub extern "C" fn pc_npc_patience(talk_num: u8, feel: u8) -> u8 {
    let info = TalkInfo { talk_num, ..TalkInfo::default() };
    let feel = match feel {
        0 => Feel::Normal,
        1 => Feel::Happy,
        2 => Feel::Angry,
        3 => Feel::Sad,
        4 => Feel::Sleepy,
        _ => Feel::Pitfall,
    };
    info.patience(feel) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temper_table_matches_source() {
        assert_eq!(TEMPER_TABLE[Feel::Normal as usize], (4000, 12, 15));
        assert_eq!(TEMPER_TABLE[Feel::Happy as usize], (3000, 10, 13));
        assert_eq!(TEMPER_TABLE[Feel::Sleepy as usize], (5000, 9, 12));
    }

    #[test]
    fn talk_frequency_drives_patience() {
        let mut info = TalkInfo::new();
        info.timer = 100;
        // Normal villager: impatient at 12, refuses at 15.
        for _ in 0..12 {
            assert!(info.count_talk(Feel::Normal));
        }
        assert_eq!(info.patience(Feel::Normal), Patience::MildlyAnnoyed);
        assert!(info.over_impatient(Feel::Normal));
        for _ in 0..3 {
            assert!(info.count_talk(Feel::Normal));
        }
        assert_eq!(info.patience(Feel::Normal), Patience::Annoyed);
        // Cap reached: further talks rejected.
        assert!(!info.count_talk(Feel::Normal));
    }

    #[test]
    fn happy_villagers_lose_patience_faster() {
        let mut info = TalkInfo::new();
        info.timer = 100;
        for _ in 0..10 {
            info.count_talk(Feel::Happy);
        }
        assert_eq!(info.patience(Feel::Happy), Patience::MildlyAnnoyed);
        // Same count would still be normal for a normal-feel villager.
        let mut other = TalkInfo::new();
        other.timer = 100;
        for _ in 0..10 {
            other.count_talk(Feel::Normal);
        }
        assert_eq!(other.patience(Feel::Normal), Patience::Normal);
    }

    #[test]
    fn interaction_selection_hierarchy() {
        let annoyed = InteractionContext {
            feel: Feel::Normal,
            patience: Patience::Annoyed,
            has_pending_request: true,
            player_holding_item: true,
            talked_recently: false,
        };
        assert_eq!(annoyed.select(), InteractionKind::Refused);

        let request = InteractionContext { patience: Patience::Normal, has_pending_request: true, player_holding_item: true, feel: Feel::Normal, talked_recently: false };
        assert_eq!(request.select(), InteractionKind::Request);

        let item = InteractionContext { patience: Patience::Normal, has_pending_request: false, player_holding_item: true, feel: Feel::Normal, talked_recently: false };
        assert_eq!(item.select(), InteractionKind::ItemTrade);

        let plain = InteractionContext { patience: Patience::Normal, has_pending_request: false, player_holding_item: false, feel: Feel::Happy, talked_recently: false };
        assert_eq!(plain.select(), InteractionKind::Conversation);
    }

    #[test]
    fn quest_type_values_match_source() {
        assert_eq!(QuestType::Delivery as u8, 0);
        assert_eq!(QuestType::Errand as u8, 1);
        assert_eq!(QuestType::Contest as u8, 2);
        assert_eq!(ContestKind::Letter as u8, 6);
        assert_eq!(DeliveryKind::Foreign as u8, 1);
    }
}
