//! Villager behavior engine for the Rust rewrite.
//!
//! Source-verified architecture (upstream `include/ac_npc.h`,
//! `src/actor/npc/ac_npc2_action.c_inc`,
//! `src/actor/npc/ac_npc2_think.c_inc`, `src/game/m_npc.c`,
//! `include/m_quest.h`, plus the existing `npc.rs` port):
//!
//! The original game does not run a general-purpose utility AI. Behavior
//! is a priority-based action arbiter over a fixed action vocabulary:
//!
//! * `aNPC_set_request_act` records a requested action only if its
//!   priority is >= the currently pending priority; the action proc then
//!   dispatches through a per-action function table (`aNPC_act_proc`).
//! * The action vocabulary (`aNPC_ACT_*`) covers locomotion (wait, walk,
//!   run, turn), social acts (greeting, talk, clap), house transitions
//!   (into/leave house), reactions (react to tool, pitfall, revive),
//!   and chores (umbrella open/close, change cloth, get).
//! * Requests carry a kind (`DEFAULT`, `AVOID`, `SEARCH`, `TO_POINT`), a
//!   target object (player, any NPC, target NPC, ball, insect, fish), six
//!   argument words, and a separate head-tracking request (look target).
//!
//! Move-out selection (`mNpc_SetRemoveAnimalNo`) is the opposite of the
//! popular myth: the game first tries to remove a villager *all* players
//! have met, then one *some* player has met, and only falls back to a
//! fully random pick. Ignoring a villager does not force departure.
//!
//! Move-in validation rejects a transferring villager that is already in
//! town, was the most recently removed, or is the current summer camper;
//! the transfer keeps the villager's record (identity, personality,
//! catchphrase, memories), which is why moved villagers remember their
//! former town.
//!
//! Letter friendship consequences (`m_npc.c`, mail-receive path): every
//! letter starts at +3; a BAD rank applies -5; an attached present adds
//! +3. (The `m_quest.h` `mQst_LETTER_SCORE_BONUS` (3) /
//! `mQst_LETTER_PRESENT_BONUS` (6) constants belong to the quest *rank*
//! calculation in `m_quest.c`, not to this friendship delta.)
//!
//! Rewrite-owned: the visible mood names (Normal/Happy/Angry/Sad) come
//! from contemporary player documentation; the decomp tracks mood as an
//! opaque index (`npc.rs` `Mood`). The favor state machine, world-event
//! table, and update ordering below model the brief's architecture and
//! are not literal decomp identifiers.

use crate::npc::{Catchphrase, Mood, Personality};

/// Behavior action vocabulary, mirroring `aNPC_ACT_*` in order.
/// (`aNPC_ACT_NONE` = 0xFF is the "no action" sentinel.)
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BehaviorAction {
    Wait = 0,
    Walk = 1,
    Run = 2,
    Turn = 3,
    Turn2 = 4,
    ChaseInsect = 5,
    ChaseFish = 6,
    Greeting = 7,
    Talk = 8,
    IntoHouse = 9,
    LeaveHouse = 10,
    UmbrellaOpen = 11,
    UmbrellaClose = 12,
    PlayMusic = 13,
    Talk2 = 14,
    ReactTool = 15,
    Clap = 16,
    Trans = 17,
    Get = 18,
    ChangeCloth = 19,
    Pitfall = 20,
    Revive = 21,
    Special = 22,
}

pub const BEHAVIOR_ACTION_NUM: usize = 23;
/// "No action" sentinel (`aNPC_ACT_NONE`).
pub const BEHAVIOR_ACTION_NONE: u8 = 0xFF;

/// Action kind, mirroring `aNPC_ACT_TYPE_*`.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActionKind {
    Default = 0,
    Avoid = 1,
    Search = 2,
    ToPoint = 3,
}

/// What the action is aimed at, mirroring `aNPC_ACT_OBJ_*`.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActionTarget {
    Default = 0,
    Player = 1,
    AnyNpc = 2,
    TargetNpc = 3,
    Ball = 6,
    Insect = 7,
    Fish = 8,
}

/// Argument word count for an action request
/// (`aNPC_REQUEST_ARG_NUM`).
pub const ACTION_ARG_NUM: usize = 6;

/// A pending action request, mirroring `aNPC_request_c`. `request()`
/// implements `aNPC_set_request_act`: a new request only replaces the
/// pending one when its priority is greater or equal.
#[derive(Clone, Debug)]
pub struct ActionRequest {
    pub priority: u8,
    pub action: BehaviorAction,
    pub kind: ActionKind,
    pub target: ActionTarget,
    pub args: [u16; ACTION_ARG_NUM],
    pub head_priority: u8,
    pub head_target: ActionTarget,
}

impl Default for ActionRequest {
    fn default() -> Self {
        Self {
            priority: 0,
            action: BehaviorAction::Wait,
            kind: ActionKind::Default,
            target: ActionTarget::Default,
            args: [0; ACTION_ARG_NUM],
            head_priority: 0,
            head_target: ActionTarget::Default,
        }
    }
}

impl ActionRequest {
    /// Returns true when the request was accepted (priority won).
    pub fn request(
        &mut self,
        prio: u8,
        action: BehaviorAction,
        kind: ActionKind,
        target: ActionTarget,
        args: Option<&[u16; ACTION_ARG_NUM]>,
    ) -> bool {
        if prio >= self.priority {
            self.priority = prio;
            self.action = action;
            self.kind = kind;
            self.target = target;
            if let Some(a) = args {
                self.args = *a;
            }
            true
        } else {
            false
        }
    }

    /// Look at something, mirroring the head-tracking request fields.
    pub fn look_at(&mut self, prio: u8, target: ActionTarget) -> bool {
        if prio >= self.head_priority {
            self.head_priority = prio;
            self.head_target = target;
            true
        } else {
            false
        }
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

/// Visible mood states from contemporary player documentation. The decomp
/// tracks mood as an opaque index (`Mood`); this is the rewrite-owned
/// presentation layer. Mood gates interaction: angry and sad villagers
/// can refuse to talk.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VisibleMood {
    Normal = 0,
    Happy = 1,
    Angry = 2,
    Sad = 3,
}

impl VisibleMood {
    /// Whether the villager will currently converse. Angry villagers
    /// refuse to speak; sad villagers withdraw.
    pub fn willing_to_talk(self) -> bool {
        matches!(self, VisibleMood::Normal | VisibleMood::Happy)
    }
}

/// World events villagers react to (the brief's event input channel).
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorldEvent {
    FishCaughtNearby = 0,
    BugCaughtNearby = 1,
    PitfallSprung = 2,
    ToolUsedNearby = 3,
    PlayerDisturbed = 4,
    FellInPitfall = 5,
}

/// Reaction selection for a world event. Documented examples: villagers
/// clap/cheer when the player catches a fish nearby; a villager that
/// steps on a pitfall enters the trapped state until talked to.
pub fn reaction_for(event: WorldEvent) -> BehaviorAction {
    match event {
        WorldEvent::FishCaughtNearby | WorldEvent::BugCaughtNearby => BehaviorAction::Clap,
        WorldEvent::PitfallSprung => BehaviorAction::Turn,
        WorldEvent::ToolUsedNearby => BehaviorAction::ReactTool,
        WorldEvent::PlayerDisturbed => BehaviorAction::Greeting,
        WorldEvent::FellInPitfall => BehaviorAction::Pitfall,
    }
}

/// Favor/errand state machine. Requests are little task graphs: they can
/// chain A -> player -> B -> player -> A (delivery/retrieval through
/// another villager) rather than requiring NPC-to-NPC AI.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FavorState {
    None = 0,
    Offered = 1,
    Accepted = 2,
    InProgress = 3,
    Completed = 4,
    Rewarded = 5,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Favor {
    pub state: FavorState,
    /// Chained favor: the item currently sits with another villager.
    pub via_npc: bool,
    pub days_left: u8,
}

impl Default for Favor {
    fn default() -> Self {
        Self { state: FavorState::None, via_npc: false, days_left: 0 }
    }
}

impl Favor {
    pub fn offer(&mut self, via_npc: bool) {
        self.state = FavorState::Offered;
        self.via_npc = via_npc;
        self.days_left = super::quest_time_limit_days();
    }

    pub fn accept(&mut self) -> bool {
        if self.state == FavorState::Offered {
            self.state = FavorState::Accepted;
            true
        } else {
            false
        }
    }

    pub fn complete(&mut self) -> bool {
        if matches!(self.state, FavorState::Accepted | FavorState::InProgress) {
            self.state = FavorState::Completed;
            true
        } else {
            false
        }
    }

    pub fn reward(&mut self) -> bool {
        if self.state == FavorState::Completed {
            self.state = FavorState::Rewarded;
            true
        } else {
            false
        }
    }
}

/// The per-frame villager update order from the brief's architecture:
/// schedule -> world events -> mood -> activity -> movement -> interaction.
/// These are pipeline stages, not decomp function names.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BehaviorStage {
    Schedule = 0,
    WorldEvents = 1,
    Mood = 2,
    Activity = 3,
    Movement = 4,
    Interaction = 5,
}

pub const BEHAVIOR_STAGES: [BehaviorStage; 6] = [
    BehaviorStage::Schedule,
    BehaviorStage::WorldEvents,
    BehaviorStage::Mood,
    BehaviorStage::Activity,
    BehaviorStage::Movement,
    BehaviorStage::Interaction,
];

/// Runtime behavior state for one villager, combining the persistent
/// record (`npc.rs`) with the behavior engine's working state.
#[derive(Clone, Debug)]
pub struct VillagerBehavior {
    pub personality: Personality,
    pub mood: Mood,
    pub visible_mood: VisibleMood,
    pub catchphrase: Catchphrase,
    pub favor: Favor,
    pub request: ActionRequest,
    /// Friendship/social state. The GameCube does NOT use the New
    /// Horizons 0-255 scale; the decomp clamps friendship at 0..=127
    /// (see `npc.rs`).
    pub friendship: i16,
}

impl VillagerBehavior {
    pub fn new(personality: Personality) -> Self {
        Self {
            personality,
            mood: Mood::new(0),
            visible_mood: VisibleMood::Normal,
            catchphrase: Catchphrase::default(),
            favor: Favor::default(),
            request: ActionRequest::default(),
            friendship: 0,
        }
    }

    /// Run one behavior tick: pick the highest-priority pending action.
    /// Returns the action to execute, or `Wait` when nothing is pending.
    pub fn tick(&mut self) -> BehaviorAction {
        let action = self.request.action;
        self.request.clear();
        action
    }

    /// Apply letter consequences, clamped to the 0..=127 friendship range.
    pub fn apply_letter(&mut self, good_score: bool, present: bool) {
        let delta = letter_friendship_delta(good_score, present) as i16;
        self.friendship = (self.friendship + delta).clamp(0, 127);
    }

    /// Adopt another villager's catchphrase (propagation), respecting the
    /// 10-char limit.
    pub fn adopt_catchphrase(&mut self, other: &Catchphrase) {
        self.catchphrase = *other;
    }
}

/// Move-out selection, mirroring `mNpc_SetRemoveAnimalNo`:
/// prefer a villager ALL players have met, then one SOME player has met,
/// then fall back to a uniform random pick. `met` is per-villager:
/// bit 0 = met by all players, bit 1 = met by at least one player.
/// `rand` returns a value in `[0, n)`. Returns `None` when no villager
/// may leave.
pub fn select_move_out(
    met: &[u8],
    may_leave: &[bool],
    rand: &mut dyn FnMut(usize) -> usize,
) -> Option<usize> {
    let eligible: Vec<usize> =
        may_leave.iter().enumerate().filter(|(_, m)| **m).map(|(i, _)| i).collect();
    if eligible.is_empty() {
        return None;
    }
    for mask in [0x01u8, 0x02u8] {
        let pool: Vec<usize> =
            eligible.iter().copied().filter(|&i| met[i] & mask != 0).collect();
        if !pool.is_empty() {
            return Some(pool[rand(pool.len())]);
        }
    }
    Some(eligible[rand(eligible.len())])
}

/// Move-in validation for a transferring villager. Rejects the transfer
/// when the villager is already in town, was the most recently removed,
/// or is the current summer camper.
pub fn transfer_allowed(
    npc_id: u16,
    town_ids: &[u16],
    last_removed_id: u16,
    summer_camper_id: Option<u16>,
) -> bool {
    if town_ids.contains(&npc_id) {
        return false;
    }
    if npc_id == last_removed_id {
        return false;
    }
    if summer_camper_id == Some(npc_id) {
        return false;
    }
    true
}

/// Portable villager record for cross-town moves: identity, personality,
/// catchphrase, and social memories travel with the villager, which is
/// why moved villagers can mention their former town.
#[derive(Clone, Debug)]
pub struct TransferRecord {
    pub npc_id: u16,
    pub personality: Personality,
    pub catchphrase: Catchphrase,
    pub former_town_name: [u8; 8],
}

/// Friendship delta for a received letter, verbatim from the retail
/// mail-receive path (`m_npc.c`):
/// ```text
/// friendship += 3;
/// if (letter_rank == mNpc_LETTER_RANK_BAD) { friendship += -5; }
/// if (mail->present != EMPTY_NO) { friendship += 3; }
/// ```
/// So good/no-present = +3, good/present = +6, bad/no-present = -2,
/// bad/present = +1. C passes `memory->letter_info.cond ==
/// mNpc_LETTER_RANK_OK` as `good` and `mail->present != EMPTY_NO` as
/// `present`; the `mNpc_AddFriendship` call itself stays C-side.
pub fn letter_friendship_delta(good: bool, present: bool) -> i32 {
    let mut delta = 3;
    if !good {
        delta -= 5;
    }
    if present {
        delta += 3;
    }
    delta
}

/// C ABI: friendship delta for a received letter (see
/// [`letter_friendship_delta`]).
#[no_mangle]
pub extern "C" fn pc_letter_friendship_delta(good: u8, present: u8) -> i32 {
    letter_friendship_delta(good != 0, present != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_request_priority_wins() {
        let mut r = ActionRequest::default();
        assert!(r.request(1, BehaviorAction::Walk, ActionKind::Default, ActionTarget::Default, None));
        // Lower priority is rejected.
        assert!(!r.request(0, BehaviorAction::Run, ActionKind::Default, ActionTarget::Default, None));
        assert_eq!(r.action, BehaviorAction::Walk);
        // Equal priority replaces (matches `>=` in the C code).
        assert!(r.request(1, BehaviorAction::Greeting, ActionKind::Default, ActionTarget::Player, None));
        assert_eq!(r.action, BehaviorAction::Greeting);
        assert_eq!(r.target, ActionTarget::Player);
    }

    #[test]
    fn mood_gates_interaction() {
        assert!(VisibleMood::Normal.willing_to_talk());
        assert!(VisibleMood::Happy.willing_to_talk());
        assert!(!VisibleMood::Angry.willing_to_talk());
        assert!(!VisibleMood::Sad.willing_to_talk());
    }

    #[test]
    fn world_events_map_to_reactions() {
        assert_eq!(reaction_for(WorldEvent::FishCaughtNearby), BehaviorAction::Clap);
        assert_eq!(reaction_for(WorldEvent::BugCaughtNearby), BehaviorAction::Clap);
        assert_eq!(reaction_for(WorldEvent::FellInPitfall), BehaviorAction::Pitfall);
        assert_eq!(reaction_for(WorldEvent::ToolUsedNearby), BehaviorAction::ReactTool);
    }

    #[test]
    fn favor_state_machine() {
        let mut f = Favor::default();
        assert!(!f.accept());
        f.offer(true);
        assert_eq!(f.state, FavorState::Offered);
        assert!(f.via_npc);
        assert!(f.accept());
        assert!(f.complete());
        assert!(f.reward());
        assert_eq!(f.state, FavorState::Rewarded);
    }

    #[test]
    fn move_out_prefers_met_villagers() {
        // Villager 1 was met by all players: chosen over unmet villager 0.
        let met = [0x00u8, 0x01u8];
        let may_leave = [true, true];
        let mut rand = |n: usize| n - 1; // would pick the last eligible
        assert_eq!(select_move_out(&met, &may_leave, &mut rand), Some(1));
        // No one met: falls back to random.
        let met_none = [0x00u8, 0x00u8];
        assert_eq!(select_move_out(&met_none, &may_leave, &mut rand), Some(1));
        // Nobody may leave.
        assert_eq!(select_move_out(&met, &[false, false], &mut rand), None);
    }

    #[test]
    fn transfer_validation() {
        assert!(!transfer_allowed(5, &[5, 6], 9, None)); // already in town
        assert!(!transfer_allowed(9, &[5, 6], 9, None)); // last removed
        assert!(!transfer_allowed(7, &[5, 6], 9, Some(7))); // summer camper
        assert!(transfer_allowed(8, &[5, 6], 9, Some(7)));
    }

    #[test]
    fn letter_friendship_clamps() {
        let mut v = VillagerBehavior::new(Personality::Boy);
        v.friendship = 125;
        v.apply_letter(true, true); // +6 would exceed 127
        assert_eq!(v.friendship, 127);
        let mut w = VillagerBehavior::new(Personality::Girl);
        w.apply_letter(true, false); // +3
        assert_eq!(w.friendship, 3);
        w.apply_letter(false, true); // +1 (bad letter with present)
        assert_eq!(w.friendship, 4);
        w.apply_letter(false, false); // -2 (bad letter, no present)
        assert_eq!(w.friendship, 2);
    }
}
