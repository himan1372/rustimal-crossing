//! Dialogue topic tables for the Rust rewrite.
//!
//! Source-verified (upstream `src/actor/npc/ac_npc_talk.c_inc`,
//! `src/game/m_npc.c`, `src/game/m_msg_main.c_inc`,
//! `include/m_npc.h`, `include/m_msg_data.h`):
//!
//! There is no monolithic `topic_table[]`. Dialogue resolves through
//! layered selection into a numeric message ID, which indexes an
//! ARAM offset table of encoded message scripts:
//!
//! * `MSG_MAX = 0x3F91` (`m_msg_data.h:22`).
//! * `mMsg_Get_BodyParam` maps `index → (addr, size)` from the offset
//!   table: entry 0 starts at the data base with size `table[0]`;
//!   entry i starts at `table[i-1]` with size
//!   `table[i] - table[i-1]`. NOTE: the PC port byte-swaps the u32
//!   table on little-endian (`TARGET_PC` in `m_msg_main.c_inc`).
//!
//! Confirmed selection mechanisms:
//!
//! * `aNPC_set_talk_info_talk_request_check`
//!   (`ac_npc_talk.c_inc:539`): island →
//!   `0x34AC + looks*3 + RANDOM(3)`; mainland →
//!   `0x075F + looks*3 + RANDOM(3)`. Six personalities × three
//!   variants per pool.
//! * `aNPC_force_talk_request`: a forced message wins if set;
//!   otherwise spontaneous talk needs friendship > 0x80, SEARCH
//!   action targeting the player, force_call_timer <= 0,
//!   dist_xz < 80.0, |dist_y| < 60.0.
//! * `mNpc_Talk_Info_c` (`m_npc.c:4865`): per-villager timer,
//!   talk_num, quest_request, unlock_timer, reset_timer
//!   (ANIMAL_NUM_MAX + islanders entries).
//! * `l_npc_temper` (`m_npc.c:4874`): per-feeling
//!   (unlock_timer, over_impatient_num, talk_num_max) — verbatim:
//!   Normal {4000,12,15}, Happy {3000,10,13}, Angry {4000,12,15},
//!   Sad {4000,10,13}, Sleepy {5000,9,12}, Pitfall {5000,9,12}.
//! * Quest-request gating: `mNpc_CheckQuestRequest` /
//!   `mNpc_SetQuestRequestOFF` (sets FALSE + unlock timer);
//!   `mNpc_TalkEndMove` sets timer = 1000 and counts the talk.
//! * `mNpc_NpcConversation_c` (`m_npc.h:261`): beesting:1,
//!   fish_complete:1, insect_complete:1 — explicit world-state
//!   topic flags.
//!
//! The category taxonomy below (personality / islander / quest /
//! forced / state-triggered / context) is a rewrite-owned
//! organization of these confirmed mechanisms, not a decomp struct.

/// Maximum message ID (`MSG_MAX`).
pub const MSG_MAX: u32 = 0x3F91;

/// Mainland talk-check pool base.
pub const TALK_CHECK_BASE_MAINLAND: u32 = 0x075F;
/// Islander talk-check pool base.
pub const TALK_CHECK_BASE_ISLAND: u32 = 0x34AC;
/// Variants per personality in the talk-check pools.
pub const TALK_CHECK_VARIANTS: u32 = 3;

/// Resolve a message ID to `(offset, size)` in the message blob
/// (`mMsg_Get_BodyParam` logic; offsets are the u32 table).
pub fn msg_body_param(offsets: &[u32], index: usize) -> Option<(u32, u32)> {
    if index >= offsets.len() {
        return None;
    }
    if index == 0 {
        Some((0, offsets[0]))
    } else {
        let addr = offsets[index - 1];
        Some((addr, offsets[index] - addr))
    }
}

/// Personality-pool message selection
/// (`aNPC_set_talk_info_talk_request_check`):
/// `base + looks * 3 + RANDOM(3)`.
pub fn talk_check_msg(looks: u8, rng3: u32, island: bool) -> u32 {
    let base = if island { TALK_CHECK_BASE_ISLAND } else { TALK_CHECK_BASE_MAINLAND };
    base + looks as u32 * TALK_CHECK_VARIANTS + (rng3 % TALK_CHECK_VARIANTS)
}

/// Spontaneous-talk gate (`aNPC_force_talk_request`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TalkGate {
    /// A forced message is pending.
    Forced,
    /// Spontaneous talk-check may proceed.
    Spontaneous,
    /// No talk.
    None,
}

pub fn force_talk_gate(
    force_call_msg_no: i32,
    friendship: i32,
    over_friendship: i32,
    is_search_for_player: bool,
    force_call_timer: f32,
    dist_xz: f32,
    dist_y: f32,
) -> TalkGate {
    if force_call_msg_no != -1 {
        TalkGate::Forced
    } else if friendship + over_friendship > 0x80
        && is_search_for_player
        && force_call_timer <= 0.0
        && dist_xz < 80.0
        && dist_y.abs() < 60.0
    {
        TalkGate::Spontaneous
    } else {
        TalkGate::None
    }
}

/// Per-villager conversation state (`mNpc_Talk_Info_c`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TalkInfo {
    pub timer: u16,
    pub talk_num: u8,
    pub quest_request: bool,
    pub unlock_timer: u16,
    pub reset_timer: u16,
}

impl TalkInfo {
    pub fn new() -> Self {
        Self { timer: 0, talk_num: 0, quest_request: true, unlock_timer: 0, reset_timer: 0 }
    }

    /// `mNpc_TalkEndMove`: timer = 1000, count the talk.
    pub fn talk_end(&mut self) {
        self.timer = 1000;
        self.talk_num = self.talk_num.saturating_add(1);
    }

    /// `mNpc_SetQuestRequestOFF`.
    pub fn set_quest_request_off(&mut self) {
        self.quest_request = false;
    }
}

/// Conversation-frequency parameters (`mNpc_Temper_c`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Temper {
    pub unlock_timer: u16,
    pub over_impatient_num: u8,
    pub talk_num_max: u8,
}

/// Verbatim `l_npc_temper` table, indexed by feeling
/// (Normal/Happy/Angry/Sad/Sleepy/Pitfall).
pub const NPC_TEMPER: [Temper; 6] = [
    Temper { unlock_timer: 4000, over_impatient_num: 12, talk_num_max: 15 },
    Temper { unlock_timer: 3000, over_impatient_num: 10, talk_num_max: 13 },
    Temper { unlock_timer: 4000, over_impatient_num: 12, talk_num_max: 15 },
    Temper { unlock_timer: 4000, over_impatient_num: 10, talk_num_max: 13 },
    Temper { unlock_timer: 5000, over_impatient_num: 9, talk_num_max: 12 },
    Temper { unlock_timer: 5000, over_impatient_num: 9, talk_num_max: 12 },
];

/// World-state conversation flags (`mNpc_NpcConversation_c`).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ConversationFlags {
    pub beesting: bool,
    pub fish_complete: bool,
    pub insect_complete: bool,
}

impl ConversationFlags {
    pub fn pack(&self) -> u8 {
        (self.beesting as u8) | ((self.fish_complete as u8) << 1) | ((self.insect_complete as u8) << 2)
    }
}

/// Rewrite-owned taxonomy of dialogue topic categories.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TopicCategory {
    PersonalityPool = 0,
    IslanderPool = 1,
    QuestRequest = 2,
    ForcedMessage = 3,
    StateTriggered = 4,
    ContextSensitive = 5,
}

/// C ABI: talk-check message ID for a personality.
#[no_mangle]
pub extern "C" fn pc_topic_talk_check(looks: u8, rng3: u32, island: i32) -> u32 {
    talk_check_msg(looks, rng3, island != 0)
}

/// C ABI: maximum message ID.
#[no_mangle]
pub extern "C" fn pc_msg_max() -> u32 {
    MSG_MAX
}

/// Patience classification derived from talk counts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TalkPatience {
    Normal = 0,
    Impatient = 1,
    OverImpatient = 2,
}

/// Pure talk-count gate: the C side keeps `mNpc_Talk_Info_c` state
/// (talk_num, timer) and passes the values in; Rust owns only the
/// comparison (`talk_num < talk_num_max`).
pub fn talk_count_allowed(talk_num: u8, talk_num_max: u8, timer: u16) -> bool {
    timer == 0 && talk_num < talk_num_max
}

/// Pure patience classification from the temper table values.
pub fn talk_patience(talk_num: u8, over_impatient_num: u8, talk_num_max: u8) -> TalkPatience {
    if talk_num >= talk_num_max {
        TalkPatience::OverImpatient
    } else if talk_num >= over_impatient_num {
        TalkPatience::Impatient
    } else {
        TalkPatience::Normal
    }
}

/// Convenience: patience for a feeling index using `NPC_TEMPER`.
pub fn talk_patience_for_feeling(feeling: usize, talk_num: u8) -> TalkPatience {
    let t = NPC_TEMPER[feeling.min(NPC_TEMPER.len() - 1)];
    talk_patience(talk_num, t.over_impatient_num, t.talk_num_max)
}

/// C ABI: talk-count gate.
#[no_mangle]
pub extern "C" fn pc_talk_count_allowed(talk_num: u8, talk_num_max: u8, timer: u16) -> u8 {
    talk_count_allowed(talk_num, talk_num_max, timer) as u8
}

/// C ABI: patience classification (0/1/2).
#[no_mangle]
pub extern "C" fn pc_talk_patience_raw(talk_num: u8, over_impatient_num: u8, talk_num_max: u8) -> u8 {
    talk_patience(talk_num, over_impatient_num, talk_num_max) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn talk_check_pools() {
        // Mainland: base + looks*3 + variant.
        assert_eq!(talk_check_msg(0, 0, false), 0x075F);
        assert_eq!(talk_check_msg(0, 2, false), 0x0761);
        assert_eq!(talk_check_msg(1, 0, false), 0x0762);
        assert_eq!(talk_check_msg(5, 2, false), 0x075F + 15 + 2);
        // Island base.
        assert_eq!(talk_check_msg(0, 0, true), 0x34AC);
        assert_eq!(talk_check_msg(2, 1, true), 0x34AC + 6 + 1);
        // rng wraps to 3 variants.
        assert_eq!(talk_check_msg(0, 7, false), 0x075F + 1);
    }

    #[test]
    fn body_param_offsets() {
        let table = [100u32, 250, 400];
        assert_eq!(msg_body_param(&table, 0), Some((0, 100)));
        assert_eq!(msg_body_param(&table, 1), Some((100, 150)));
        assert_eq!(msg_body_param(&table, 2), Some((250, 150)));
        assert_eq!(msg_body_param(&table, 3), None);
    }

    #[test]
    fn talk_gate() {
        assert_eq!(force_talk_gate(5, 0, 0, false, 0.0, 0.0, 0.0), TalkGate::Forced);
        assert_eq!(
            force_talk_gate(-1, 0x81, 0, true, 0.0, 79.0, 59.0),
            TalkGate::Spontaneous
        );
        // Friendship exactly 0x80 is not enough.
        assert_eq!(
            force_talk_gate(-1, 0x80, 0, true, 0.0, 79.0, 59.0),
            TalkGate::None
        );
        // Too far.
        assert_eq!(
            force_talk_gate(-1, 200, 0, true, 0.0, 80.0, 0.0),
            TalkGate::None
        );
    }

    #[test]
    fn talk_info_flow() {
        let mut t = TalkInfo::new();
        assert!(t.quest_request);
        t.set_quest_request_off();
        assert!(!t.quest_request);
        t.talk_end();
        assert_eq!((t.timer, t.talk_num), (1000, 1));
    }

    #[test]
    fn temper_values() {
        assert_eq!(NPC_TEMPER[0], Temper { unlock_timer: 4000, over_impatient_num: 12, talk_num_max: 15 });
        assert_eq!(NPC_TEMPER[1], Temper { unlock_timer: 3000, over_impatient_num: 10, talk_num_max: 13 });
        assert_eq!(NPC_TEMPER[4], Temper { unlock_timer: 5000, over_impatient_num: 9, talk_num_max: 12 });
    }

    #[test]
    fn conversation_flags_pack() {
        let f = ConversationFlags { beesting: true, fish_complete: false, insect_complete: true };
        assert_eq!(f.pack(), 0b101);
    }

    #[test]
    fn talk_gates() {
        assert!(talk_count_allowed(3, 15, 0));
        assert!(!talk_count_allowed(15, 15, 0));
        assert!(!talk_count_allowed(3, 15, 100)); // timer running
        assert_eq!(talk_patience(3, 12, 15), TalkPatience::Normal);
        assert_eq!(talk_patience(12, 12, 15), TalkPatience::Impatient);
        assert_eq!(talk_patience(15, 12, 15), TalkPatience::OverImpatient);
        assert_eq!(talk_patience_for_feeling(0, 14), TalkPatience::Impatient);
        assert_eq!(pc_talk_count_allowed(3, 15, 0), 1);
        assert_eq!(pc_talk_patience_raw(15, 12, 15), 2);
    }
}
