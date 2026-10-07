//! NPC behavior & conversation system for the Rust rewrite.
//!
//! Source-verified architecture (upstream `include/m_npc_personal_id.h`,
//! `include/m_npc.h`, `src/game/m_npc_schedule.c`,
//! `include/m_npc_schedule_h.h`, `src/game/m_msg_main.c_inc`,
//! `include/m_msg_data.h`):
//!
//! * Six personality classes, in `mNpc_LOOKS_*` order: GIRL (normal),
//!   KO_GIRL (peppy), BOY (lazy), SPORT_MAN (jock), GRIM_MAN (cranky),
//!   NANIWA_LADY (snooty). (The Japanese bo/fu/ge/ha/ko/ta labels are
//!   widely documented but do not appear in these decomp identifiers.)
//! * Each personality has its own daily schedule table
//!   (`mNPS_schedule[mNpc_LOOKS_NUM]`), ported verbatim below as
//!   (state, end-time) pairs. Schedule states: FIELD, IN_HOUSE, SLEEP,
//!   STAND, WANDER, WALK_WANDER, SPECIAL.
//! * Mood is a 9-value index (`mNpc_MOOD_0`..`mNpc_MOOD_8`) with a timer
//!   (`Animal_c.mood` / `mood_time`); the index-to-meaning mapping is not
//!   established from the decomp, so moods stay opaque here.
//! * The message engine is a real interpreter: `mMsg_ChangeMsgData` /
//!   `mMsg_LoadMsgData` load one of `MSG_MAX` (0x3F91) messages into a
//!   window object with cursor state and a 20.0s timer (`mMsg_SetTimer`).
//!   Selection (NPC AI) and rendering (`m_msg`) are separate layers.
//! * Catchphrases are mutable per-villager state, 10 chars
//!   (`ANIMAL_CATCHPHRASE_LEN`).
//!
//! Rewrite-owned: dialogue categories, the message-op interpreter, the
//! conversation tree, and mood/refusal semantics are modeled on the
//! documented architecture, not ported line-by-line. No authored message
//! text or message IDs are reproduced.

// Public NPC/conversation API for the rewrite and future adapters. The crate
// builds as a staticlib, so unused public items would warn as dead code.
#![allow(dead_code)]

/// Personality classes in `mNpc_LOOKS_*` order.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Personality {
    /// Normal.
    Girl = 0,
    /// Peppy.
    KoGirl = 1,
    /// Lazy.
    Boy = 2,
    /// Jock.
    SportMan = 3,
    /// Cranky.
    GrimMan = 4,
    /// Snooty.
    NaniwaLady = 5,
}

pub const PERSONALITY_NUM: usize = 6;

impl Personality {
    pub fn english_name(self) -> &'static str {
        match self {
            Personality::Girl => "normal",
            Personality::KoGirl => "peppy",
            Personality::Boy => "lazy",
            Personality::SportMan => "jock",
            Personality::GrimMan => "cranky",
            Personality::NaniwaLady => "snooty",
        }
    }

    pub fn from_looks(looks: u8) -> Option<Personality> {
        match looks {
            0 => Some(Personality::Girl),
            1 => Some(Personality::KoGirl),
            2 => Some(Personality::Boy),
            3 => Some(Personality::SportMan),
            4 => Some(Personality::GrimMan),
            5 => Some(Personality::NaniwaLady),
            _ => None,
        }
    }
}

/// NPC schedule states (`mNPS_SCHED_*`).
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScheduleState {
    /// Out in the town (same acre as their home).
    Field = 0,
    /// Inside their house.
    InHouse = 1,
    /// Asleep in their house.
    Sleep = 2,
    /// Standing around town.
    Stand = 3,
    /// Wandering around town.
    Wander = 4,
    /// Walking wander.
    WalkWander = 5,
    /// Unique per-NPC-actor schedule.
    Special = 6,
}

/// One schedule row: (state, end time in seconds since midnight).
/// Ported verbatim from `m_npc_schedule.c`; each row's state holds until
/// its end time, then the next row takes over (wrapping at 24:00).
pub static SCHEDULE_TABLES: [[(u8, u32); 9]; PERSONALITY_NUM] = [
    // girl (normal)
    [
        (ScheduleState::Sleep as u8, 5 * 3600),
        (ScheduleState::InHouse as u8, 6 * 3600),
        (ScheduleState::Field as u8, 12 * 3600),
        (ScheduleState::InHouse as u8, 13 * 3600),
        (ScheduleState::Field as u8, 18 * 3600 + 1800),
        (ScheduleState::InHouse as u8, 21 * 3600),
        (ScheduleState::Sleep as u8, 24 * 3600),
        (ScheduleState::Sleep as u8, 24 * 3600),
        (ScheduleState::Sleep as u8, 24 * 3600),
    ],
    // ko_girl (peppy)
    [
        (ScheduleState::Sleep as u8, 7 * 3600),
        (ScheduleState::InHouse as u8, 8 * 3600),
        (ScheduleState::Field as u8, 13 * 3600),
        (ScheduleState::InHouse as u8, 14 * 3600),
        (ScheduleState::Field as u8, 22 * 3600),
        (ScheduleState::InHouse as u8, 23 * 3600 + 1800),
        (ScheduleState::Sleep as u8, 24 * 3600),
        (ScheduleState::Sleep as u8, 24 * 3600),
        (ScheduleState::Sleep as u8, 24 * 3600),
    ],
    // boy (lazy)
    [
        (ScheduleState::Sleep as u8, 8 * 3600),
        (ScheduleState::InHouse as u8, 9 * 3600),
        (ScheduleState::Field as u8, 12 * 3600),
        (ScheduleState::InHouse as u8, 14 * 3600),
        (ScheduleState::Field as u8, 19 * 3600 + 1800),
        (ScheduleState::InHouse as u8, 22 * 3600),
        (ScheduleState::Sleep as u8, 24 * 3600),
        (ScheduleState::Sleep as u8, 24 * 3600),
        (ScheduleState::Sleep as u8, 24 * 3600),
    ],
    // sport_man (jock)
    [
        (ScheduleState::InHouse as u8, 1 * 3600),
        (ScheduleState::Sleep as u8, 5 * 3600 + 1800),
        (ScheduleState::InHouse as u8, 6 * 3600 + 1800),
        (ScheduleState::Field as u8, 12 * 3600),
        (ScheduleState::InHouse as u8, 12 * 3600 + 1800),
        (ScheduleState::Field as u8, 23 * 3600),
        (ScheduleState::InHouse as u8, 24 * 3600),
        (ScheduleState::InHouse as u8, 24 * 3600),
        (ScheduleState::InHouse as u8, 24 * 3600),
    ],
    // grim_man (cranky)
    [
        (ScheduleState::Field as u8, 4 * 3600),
        (ScheduleState::InHouse as u8, 5 * 3600),
        (ScheduleState::Sleep as u8, 10 * 3600),
        (ScheduleState::InHouse as u8, 11 * 3600),
        (ScheduleState::Field as u8, 15 * 3600),
        (ScheduleState::InHouse as u8, 16 * 3600),
        (ScheduleState::Field as u8, 22 * 3600),
        (ScheduleState::InHouse as u8, 23 * 3600),
        (ScheduleState::Field as u8, 24 * 3600),
    ],
    // naniwa_lady (snooty)
    [
        (ScheduleState::Field as u8, 1 * 3600 + 1800),
        (ScheduleState::InHouse as u8, 2 * 3600 + 1800),
        (ScheduleState::Sleep as u8, 9 * 3600),
        (ScheduleState::InHouse as u8, 10 * 3600),
        (ScheduleState::Field as u8, 13 * 3600),
        (ScheduleState::InHouse as u8, 14 * 3600),
        (ScheduleState::Field as u8, 21 * 3600),
        (ScheduleState::InHouse as u8, 22 * 3600),
        (ScheduleState::Field as u8, 24 * 3600),
    ],
];

/// Schedule state for a personality at `seconds` past midnight.
/// Mirrors the schedule-manager lookup over `mNPS_schedule[looks]`.
pub fn schedule_state_at(personality: Personality, seconds: u32) -> ScheduleState {
    let table = &SCHEDULE_TABLES[personality as usize];
    let t = seconds % 86400;
    for (state, end) in table.iter() {
        if t < *end {
            return match *state {
                0 => ScheduleState::Field,
                1 => ScheduleState::InHouse,
                2 => ScheduleState::Sleep,
                3 => ScheduleState::Stand,
                4 => ScheduleState::Wander,
                5 => ScheduleState::WalkWander,
                _ => ScheduleState::Special,
            };
        }
    }
    ScheduleState::Field
}

/// True when the personality's schedule has them asleep at `seconds`.
pub fn is_asleep(personality: Personality, seconds: u32) -> bool {
    schedule_state_at(personality, seconds) == ScheduleState::Sleep
}

/// Mood value count (`mNpc_MOOD_NUM`). The decomp does not name the moods;
/// they are tracked as an opaque index plus timer (`mood`/`mood_time`).
pub const MOOD_NUM: usize = 9;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Mood {
    pub index: u8,
    pub time: u8,
}

impl Mood {
    pub fn new(index: u8) -> Self {
        Self { index: index % MOOD_NUM as u8, time: 0 }
    }
}

/// Catchphrase storage: mutable per-villager state, 10 chars
/// (`ANIMAL_CATCHPHRASE_LEN`).
pub const CATCHPHRASE_LEN: usize = 10;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Catchphrase {
    pub chars: [u8; CATCHPHRASE_LEN],
    pub len: u8,
}

impl Default for Catchphrase {
    fn default() -> Self {
        Self { chars: [0; CATCHPHRASE_LEN], len: 0 }
    }
}

impl Catchphrase {
    /// Store a new catchphrase, truncating to the 10-char limit.
    pub fn set(&mut self, text: &[u8]) {
        let n = text.len().min(CATCHPHRASE_LEN);
        self.chars[..n].copy_from_slice(&text[..n]);
        self.len = n as u8;
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.chars[..self.len as usize]
    }
}

/// Maximum message ID (`MSG_MAX` = 0x3F91).
pub const MSG_MAX: usize = 0x3F91;
/// Default message window timer, seconds (`mMsg_SetTimer(msg_p, 20.0f)`).
pub const MSG_WINDOW_TIMER: f32 = 20.0;

/// Rewrite-owned dialogue categories, shaped by the message-file research.
/// The source organizes messages into many situational banks; these names
/// are our abstraction, not decomp identifiers.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MessageCategory {
    Greeting = 0,
    Weather = 1,
    TimeOfDay = 2,
    Favor = 3,
    Delivery = 4,
    Trade = 5,
    ItemRequest = 6,
    Event = 7,
    Rumor = 8,
    Farewell = 9,
}

/// Context the dialogue selector reads. Mirrors the documented selector
/// inputs: personality picks the pool, situation picks within it.
pub struct DialogueContext {
    pub personality: Personality,
    pub mood: Mood,
    pub hour: u8,
    pub is_raining: bool,
    pub is_event_day: bool,
    pub has_favor_available: bool,
    pub friendship: u8,
}

/// Choose a dialogue category from context. Personality selects the pool;
/// situation and random roll select within it. Rewrite-owned logic.
pub fn select_category(ctx: &DialogueContext, roll: u32) -> MessageCategory {
    if ctx.has_favor_available && roll % 3 == 0 {
        return MessageCategory::Favor;
    }
    if ctx.is_event_day && roll % 4 == 0 {
        return MessageCategory::Event;
    }
    if ctx.is_raining && roll % 5 == 0 {
        return MessageCategory::Weather;
    }
    match roll % 6 {
        0 => MessageCategory::Greeting,
        1 => MessageCategory::TimeOfDay,
        2 => MessageCategory::Rumor,
        3 => MessageCategory::ItemRequest,
        4 => MessageCategory::Delivery,
        _ => MessageCategory::Trade,
    }
}

/// Message script operations. Models the control-code architecture of the
/// message engine (pauses, substitutions, input waits) without reproducing
/// any authored message bytes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MsgOp {
    /// Literal text span (rewrite-owned sample text, never game script).
    Text(&'static str),
    /// Pause `frames` ticks.
    Pause(u16),
    /// Wait for player confirmation.
    WaitInput,
    /// Line break.
    NewLine,
    /// Dynamic substitution.
    Subst(MsgVar),
    /// End of message.
    End,
}

/// Runtime substitution variables for message scripts.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MsgVar {
    PlayerName = 0,
    NpcName = 1,
    TownName = 2,
    Catchphrase = 3,
    ItemName = 4,
}

/// Message window state machine, mirroring the `mMsg_Window_c` fields used
/// by `mMsg_ChangeMsgData`: loaded script, cursor, and timer.
pub struct MsgWindow {
    pub script: &'static [MsgOp],
    pub cursor: usize,
    pub timer: f32,
    pub waiting_input: bool,
    pub finished: bool,
}

impl MsgWindow {
    /// Load a script into the window, resetting cursor state and the timer.
    /// Mirrors `mMsg_ChangeMsgData` (cursor reset + 20.0s timer).
    pub fn load(&mut self, script: &'static [MsgOp]) -> bool {
        if script.is_empty() {
            return false;
        }
        self.script = script;
        self.cursor = 0;
        self.timer = MSG_WINDOW_TIMER;
        self.waiting_input = false;
        self.finished = false;
        true
    }

    /// Advance one interpreter step. Returns the op to present, if any.
    pub fn step(&mut self) -> Option<MsgOp> {
        if self.finished || self.waiting_input {
            return None;
        }
        let op = *self.script.get(self.cursor)?;
        self.cursor += 1;
        match op {
            MsgOp::End => {
                self.finished = true;
                None
            }
            MsgOp::WaitInput => {
                self.waiting_input = true;
                Some(op)
            }
            _ => Some(op),
        }
    }

    /// Player confirmed; resume after a `WaitInput`.
    pub fn confirm(&mut self) {
        self.waiting_input = false;
    }
}

/// Conversation entry points offered to the player.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConvoOption {
    Talk = 0,
    Favor = 1,
    GiveItem = 2,
    Trade = 3,
    Bye = 4,
}

/// Player answers that can change NPC state.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlayerAnswer {
    Good = 0,
    Bad = 1,
    Neutral = 2,
}

/// Outcome of one conversation exchange: mood/friendship deltas applied to
/// persistent NPC state. Rewrite-owned; magnitudes are placeholders, not
/// source values.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ConvoOutcome {
    pub mood_delta: i8,
    pub friendship_delta: i8,
    pub conversation_over: bool,
}

/// Resolve a player's answer. A bad answer sours mood; a good one warms it.
/// Friendship effects stay small: the source clamps friendship to 0..=127
/// (`mNpc_AddFriendship`) and dialogue is pool-driven, not friendship-gated.
pub fn resolve_answer(answer: PlayerAnswer, mood: &mut Mood) -> ConvoOutcome {
    match answer {
        PlayerAnswer::Good => {
            mood.time = mood.time.saturating_add(10);
            ConvoOutcome { mood_delta: 1, friendship_delta: 1, conversation_over: false }
        }
        PlayerAnswer::Bad => {
            mood.time = 0;
            ConvoOutcome { mood_delta: -2, friendship_delta: -1, conversation_over: true }
        }
        PlayerAnswer::Neutral => ConvoOutcome::default(),
    }
}

/// Runtime NPC actor state: persistent record + live simulation state.
/// Mirrors the decomp's split between saved villager data and the actor.
pub struct NpcActor {
    pub personality: Personality,
    pub mood: Mood,
    pub catchphrase: Catchphrase,
    pub friendship: u8,
    /// Seconds past midnight for schedule lookup.
    pub clock: u32,
}

impl NpcActor {
    pub fn new(personality: Personality) -> Self {
        Self {
            personality,
            mood: Mood::new(0),
            catchphrase: Catchphrase::default(),
            friendship: 0,
            clock: 0,
        }
    }

    pub fn schedule_state(&self) -> ScheduleState {
        schedule_state_at(self.personality, self.clock)
    }

    /// Whether the NPC can currently be talked to. Sleep always refuses;
    /// mood-based refusal follows the documented angry/sad behavior
    /// (guide-observed; exact mood indices not established from the decomp).
    pub fn can_talk(&self) -> bool {
        self.schedule_state() != ScheduleState::Sleep
    }

    /// Clamp friendship to the source 0..=127 range.
    pub fn add_friendship(&mut self, delta: i8) {
        let v = self.friendship as i16 + delta as i16;
        self.friendship = v.clamp(0, 127) as u8;
    }
}

/// C ABI: schedule state for a personality (`mNpc_LOOKS_*` index) at
/// `seconds` past midnight. Returns 255 for an unknown personality.
#[no_mangle]
pub extern "C" fn pc_npc_schedule_state(looks: u8, seconds: u32) -> u8 {
    match Personality::from_looks(looks) {
        Some(p) => schedule_state_at(p, seconds) as u8,
        None => 255,
    }
}

/// C ABI: 1 when the personality is asleep at `seconds`, else 0.
#[no_mangle]
pub extern "C" fn pc_npc_is_asleep(looks: u8, seconds: u32) -> i32 {
    match Personality::from_looks(looks) {
        Some(p) => i32::from(is_asleep(p, seconds)),
        None => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schedule_tables_match_source_sleep_windows() {
        // lazy (boy): asleep 22:00-08:00
        assert!(is_asleep(Personality::Boy, 23 * 3600));
        assert!(is_asleep(Personality::Boy, 7 * 3600));
        assert!(!is_asleep(Personality::Boy, 12 * 3600));
        // normal (girl): asleep 21:00-05:00
        assert!(is_asleep(Personality::Girl, 22 * 3600));
        assert!(!is_asleep(Personality::Girl, 12 * 3600));
        // snooty (naniwa_lady): asleep 02:30-09:00
        assert!(is_asleep(Personality::NaniwaLady, 3 * 3600));
        assert!(!is_asleep(Personality::NaniwaLady, 12 * 3600));
        // jock (sport_man): asleep 01:00-05:30
        assert!(is_asleep(Personality::SportMan, 2 * 3600));
        assert!(!is_asleep(Personality::SportMan, 12 * 3600));
    }

    #[test]
    fn schedule_state_transitions() {
        assert_eq!(schedule_state_at(Personality::Girl, 4 * 3600), ScheduleState::Sleep);
        assert_eq!(schedule_state_at(Personality::Girl, 5 * 3600 + 1800), ScheduleState::InHouse);
        assert_eq!(schedule_state_at(Personality::Girl, 7 * 3600), ScheduleState::Field);
    }

    #[test]
    fn catchphrase_truncates_to_ten() {
        let mut c = Catchphrase::default();
        c.set(b"this phrase is way too long");
        assert_eq!(c.len, 10);
        assert_eq!(c.as_slice(), b"this phras");
    }

    #[test]
    fn msg_window_load_and_step() {
        static SCRIPT: &[MsgOp] = &[
            MsgOp::Text("Hi"),
            MsgOp::Subst(MsgVar::PlayerName),
            MsgOp::WaitInput,
            MsgOp::End,
        ];
        let mut w = MsgWindow { script: &[], cursor: 0, timer: 0.0, waiting_input: false, finished: true };
        assert!(w.load(SCRIPT));
        assert_eq!(w.timer, MSG_WINDOW_TIMER);
        assert_eq!(w.step(), Some(MsgOp::Text("Hi")));
        assert_eq!(w.step(), Some(MsgOp::Subst(MsgVar::PlayerName)));
        assert_eq!(w.step(), Some(MsgOp::WaitInput));
        assert_eq!(w.step(), None); // waiting for input
        w.confirm();
        assert_eq!(w.step(), None); // End -> finished
        assert!(w.finished);
    }

    #[test]
    fn friendship_clamps_to_source_range() {
        let mut npc = NpcActor::new(Personality::GrimMan);
        npc.add_friendship(100);
        npc.add_friendship(100);
        assert_eq!(npc.friendship, 127);
        npc.add_friendship(-100);
        npc.add_friendship(-100);
        assert_eq!(npc.friendship, 0);
    }

    #[test]
    fn sleeping_npc_cannot_talk() {
        let mut npc = NpcActor::new(Personality::Boy);
        npc.clock = 23 * 3600;
        assert!(!npc.can_talk());
        npc.clock = 12 * 3600;
        assert!(npc.can_talk());
    }
}
