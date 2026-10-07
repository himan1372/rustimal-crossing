//! NPC talk-request driver (`aNPC_TALK_REQUEST_PROC` family).
//!
//! Verified against `ac_npc_talk.c_inc`, `ac_npc_ct.c_inc`,
//! `ac_countdown_npc0_talk.c_inc`, and `ac_quest_manager.c`
//! (USA Rev. 0 decomp).
//!
//! CORRECTION to the brief: the symbol family EXISTS in the current
//! source. `talk_request_proc` is not one function but a per-NPC
//! strategy hook:
//!
//! ```c
//! typedef void (*aNPC_TALK_REQUEST_PROC)(ACTOR*, GAME*);
//! ```
//!
//! defaulted from `aNPC_ct_data_c.talk_request_proc`
//! (`ac_npc_ct.c_inc:315`) and swappable at runtime (the countdown NPC
//! swaps `none_proc1` / `force_talk_request` / `norm_talk_request`
//! based on the time term).
//!
//! The driver (`aNPC_talk_request_event_npc`) decides WHEN the hook
//! fires; the hook decides WHAT kind of talk to request. The actual
//! message flow is owned by the demo system (`mDemo_Request`), not by
//! this layer — the brief's "process/state driver, not the dialogue
//! database" framing was right, but the symbol was recoverable.
//!
//! Dispatch (`aNPC_talk_request_event_npc`, verbatim):
//! - a SPEAK/SPEECH/TALK demo is active and NOT listenable
//!   -> `aNPC_setup_talk_start` directly.
//! - else, submenu idle (WAIT, timer 0):
//!   - hook installed -> call it.
//!   - no hook -> `mDemo_Request(mDemo_TYPE_TALK, actorx, NULL)`.
//! - otherwise -> nothing this frame.
//!
//! Concrete hook behaviors (source):
//! - `aCD0_norm_talk_request`: `mDemo_Request(TYPE_TALK, actorx,
//!   set_norm_talk_info)` — message = `msg_base[looks] + RANDOM(3) +
//!   term_offset`.
//! - `aCD0_force_talk_request`: `mDemo_Request(TYPE_SPEAK, ...)` —
//!   forced/cutscene talk.
//! - quest-manager clip (`aNPC_normal_talk_request`): the clip's own
//!   `talk_request_proc(actorx)` returns bool; TRUE -> clear
//!   force-call flag and `setup_talk_start`.
//! - `none_proc1`: installed as the hook to mean "no talk request".
//!
//! Session setup/teardown (`aNPC_setup_talk_start` / `_end`):
//! - start: palActor = player; if turn == NORMAL, face the player;
//!   umb_flag = FALSE; greeting_flag = FALSE;
//!   talk_condition = TALK_TYPE_START; save demo flags.
//! - end: palActor = NULL; palActorIgnoreTimer = 600 (if it was >= 0);
//!   talk_condition = NONE; clear force-call state; feel = 0xFF;
//!   restore demo flags.

/// `aNPC_TALK_TYPE_*` (ac_npc.h:901).
pub mod talk_condition {
    pub const NONE: u8 = 0;
    pub const START: u8 = 1;
    pub const CONTINUE: u8 = 2;
}

/// `aNPC_TALK_TURN_*` (ac_npc.h:909).
pub mod talk_turn {
    pub const NORMAL: u8 = 0;
    pub const HEAD: u8 = 1;
    pub const NONE: u8 = 2;
}

/// Demo types relevant to the dispatch (`mDemo_TYPE_*`).
pub mod demo_type {
    pub const TALK: u8 = 0;
    pub const SPEAK: u8 = 1;
    pub const SPEECH: u8 = 2;
}

/// What the talk-request driver should do this frame.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TalkRequestAction {
    /// A talk demo is already active and not listenable: run
    /// `aNPC_setup_talk_start` immediately.
    SetupTalkStart,
    /// Submenu idle and the NPC has a hook: invoke
    /// `talk_info.talk_request_proc(actorx, game)`.
    InvokeProc,
    /// Submenu idle, no hook: `mDemo_Request(TYPE_TALK, actorx, NULL)`.
    RequestTalkDemo,
    /// Preconditions not met: do nothing this frame.
    Wait,
}

/// Inputs to the dispatch, mirroring the source's checks.
pub struct TalkRequestInputs {
    pub demo_speak_active: bool,
    pub demo_speech_active: bool,
    pub demo_talk_active: bool,
    pub demo_listenable: bool,
    pub submenu_wait: bool,
    pub submenu_wait_timer_zero: bool,
    pub has_talk_request_proc: bool,
}

/// `aNPC_talk_request_event_npc` dispatch, verbatim.
pub fn talk_request_dispatch(inp: &TalkRequestInputs) -> TalkRequestAction {
    if (inp.demo_speak_active || inp.demo_speech_active || inp.demo_talk_active)
        && !inp.demo_listenable
    {
        return TalkRequestAction::SetupTalkStart;
    }
    if inp.submenu_wait && inp.submenu_wait_timer_zero {
        if inp.has_talk_request_proc {
            return TalkRequestAction::InvokeProc;
        } else {
            return TalkRequestAction::RequestTalkDemo;
        }
    }
    TalkRequestAction::Wait
}

/// `aNPC_normal_talk_request` gate: the quest-manager clip's
/// `talk_request_proc(actorx)` returns bool. Returns TRUE (start talk)
/// only when the clip proc is present AND returns TRUE; the caller
/// then clears the force-call flag and runs `setup_talk_start`.
pub fn normal_talk_request_gate(clip_proc_present: bool, clip_proc_result: bool) -> bool {
    clip_proc_present && clip_proc_result
}

/// The countdown NPC's message-number selection
/// (`aCD0_set_norm_talk_info`): `msg_base[looks] + RANDOM(3)`, plus 17
/// for the NEW_YEAR / AFTER_10_SEC terms, else `term * 4`.
/// `random3` is the caller's `RANDOM(3)` sample.
pub fn countdown_norm_msg_no(msg_base_looks: i32, term: i32, random3: i32, term_new_year: i32, term_after_10_sec: i32) -> i32 {
    let mut msg_no = msg_base_looks + random3;
    if term == term_new_year || term == term_after_10_sec {
        msg_no += 17;
    } else {
        msg_no += term * 4;
    }
    msg_no
}

/// Persistent per-NPC talk-session state touched by
/// `aNPC_setup_talk_start` / `aNPC_setup_talk_end`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TalkSession {
    /// palActor set (start) / cleared (end).
    pub pal_actor: bool,
    /// Face the player on start (turn == TALK_TURN_NORMAL).
    pub face_player: bool,
    pub talk_condition: u8,
    /// feel reset to 0xFF on end.
    pub feel: u8,
    /// palActorIgnoreTimer set to 600 on end (when it was >= 0).
    pub ignore_timer: i32,
    /// force-call state cleared on end.
    pub force_call_cleared: bool,
}

impl TalkSession {
    /// `aNPC_setup_talk_start`: palActor = player; face the player iff
    /// turn == NORMAL; talk_condition = START.
    pub fn begin(turn: u8) -> Self {
        TalkSession {
            pal_actor: true,
            face_player: turn == talk_turn::NORMAL,
            talk_condition: talk_condition::START,
            feel: 0xFF,
            ignore_timer: -1,
            force_call_cleared: false,
        }
    }

    /// `aNPC_setup_talk_end`: palActor = NULL; ignore timer = 600 when
    /// it was >= 0; talk_condition = NONE; force-call cleared;
    /// feel = 0xFF; demo flags restored (caller-side).
    pub fn end(previous_ignore_timer: i32) -> Self {
        TalkSession {
            pal_actor: false,
            face_player: false,
            talk_condition: talk_condition::NONE,
            feel: 0xFF,
            ignore_timer: if previous_ignore_timer >= 0 { 600 } else { previous_ignore_timer },
            force_call_cleared: true,
        }
    }
}

// ---- C ABI ----

/// C ABI: talk-request dispatch. Returns 0=Wait, 1=SetupTalkStart,
/// 2=InvokeProc, 3=RequestTalkDemo.
#[no_mangle]
pub extern "C" fn pc_talk_request_dispatch(
    demo_speak_active: u8,
    demo_speech_active: u8,
    demo_talk_active: u8,
    demo_listenable: u8,
    submenu_wait: u8,
    submenu_wait_timer_zero: u8,
    has_talk_request_proc: u8,
) -> u8 {
    let inp = TalkRequestInputs {
        demo_speak_active: demo_speak_active != 0,
        demo_speech_active: demo_speech_active != 0,
        demo_talk_active: demo_talk_active != 0,
        demo_listenable: demo_listenable != 0,
        submenu_wait: submenu_wait != 0,
        submenu_wait_timer_zero: submenu_wait_timer_zero != 0,
        has_talk_request_proc: has_talk_request_proc != 0,
    };
    match talk_request_dispatch(&inp) {
        TalkRequestAction::Wait => 0,
        TalkRequestAction::SetupTalkStart => 1,
        TalkRequestAction::InvokeProc => 2,
        TalkRequestAction::RequestTalkDemo => 3,
    }
}

/// C ABI: `aNPC_normal_talk_request` gate; 1 = start talk.
#[no_mangle]
pub extern "C" fn pc_normal_talk_request_gate(clip_proc_present: u8, clip_proc_result: u8) -> u8 {
    normal_talk_request_gate(clip_proc_present != 0, clip_proc_result != 0) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    fn idle() -> TalkRequestInputs {
        TalkRequestInputs {
            demo_speak_active: false,
            demo_speech_active: false,
            demo_talk_active: false,
            demo_listenable: false,
            submenu_wait: true,
            submenu_wait_timer_zero: true,
            has_talk_request_proc: true,
        }
    }

    #[test]
    fn dispatch_branches() {
        // Idle submenu + hook -> InvokeProc.
        assert_eq!(talk_request_dispatch(&idle()), TalkRequestAction::InvokeProc);
        // Idle submenu, no hook -> RequestTalkDemo.
        let mut i = idle();
        i.has_talk_request_proc = false;
        assert_eq!(talk_request_dispatch(&i), TalkRequestAction::RequestTalkDemo);
        // Active non-listenable talk demo -> SetupTalkStart (even if submenu idle).
        let mut i = idle();
        i.demo_talk_active = true;
        assert_eq!(talk_request_dispatch(&i), TalkRequestAction::SetupTalkStart);
        // Active but listenable -> falls through to submenu path.
        let mut i = idle();
        i.demo_speak_active = true;
        i.demo_listenable = true;
        assert_eq!(talk_request_dispatch(&i), TalkRequestAction::InvokeProc);
        // Submenu busy -> Wait.
        let mut i = idle();
        i.submenu_wait = false;
        i.demo_talk_active = true;
        i.demo_listenable = true;
        assert_eq!(talk_request_dispatch(&i), TalkRequestAction::Wait);
        let mut i = idle();
        i.submenu_wait_timer_zero = false;
        assert_eq!(talk_request_dispatch(&i), TalkRequestAction::Wait);
        // Quest gate.
        assert!(normal_talk_request_gate(true, true));
        assert!(!normal_talk_request_gate(true, false));
        assert!(!normal_talk_request_gate(false, true));
        // C ABI.
        assert_eq!(pc_talk_request_dispatch(0, 0, 1, 0, 1, 1, 1), 1);
        assert_eq!(pc_talk_request_dispatch(0, 0, 0, 0, 1, 1, 0), 3);
        assert_eq!(pc_normal_talk_request_gate(1, 1), 1);
        assert_eq!(pc_normal_talk_request_gate(1, 0), 0);
    }

    #[test]
    fn session_lifecycle() {
        let s = TalkSession::begin(talk_turn::NORMAL);
        assert!(s.pal_actor && s.face_player);
        assert_eq!(s.talk_condition, talk_condition::START);
        let s = TalkSession::begin(talk_turn::HEAD);
        assert!(!s.face_player);
        let e = TalkSession::end(5);
        assert!(!e.pal_actor);
        assert_eq!(e.talk_condition, talk_condition::NONE);
        assert_eq!(e.ignore_timer, 600);
        assert_eq!(e.feel, 0xFF);
        assert!(e.force_call_cleared);
        let e = TalkSession::end(-1);
        assert_eq!(e.ignore_timer, -1);
        // Countdown message selection.
        assert_eq!(countdown_norm_msg_no(7528, 2, 1, 90, 91), 7528 + 1 + 8);
        assert_eq!(countdown_norm_msg_no(7528, 90, 1, 90, 91), 7528 + 1 + 17);
    }
}
