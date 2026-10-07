//! NPC force-call state machine (`aNPC_force_call_req_proc` family).
//!
//! Verified against `ac_npc_talk.c_inc`, `ac_npc_think.c_inc`,
//! `ac_npc_move.c_inc`, `ac_npc_ct.c_inc` (USA Rev. 0 decomp).
//!
//! `aNPC_force_call_req_proc` is a generic NPC-clip callback — a
//! transactional "reserve this NPC for a forced conversation" operation,
//! NOT a message selector. The caller supplies the message ID; the
//! function only checks availability and queues the request.
//!
//! ```text
//! external system ──force_call_req_proc(npc, msg)──▶ [gates] ──▶ REQUEST
//!                                                              │
//! friendship SEARCH ───────────────────────────────────────────┘
//!         │
//!         ▼
//! aNPC_force_talk_request()
//!    ├─ forced msg_no != -1 ──▶ SPEAK demo + install msg
//!    └─ else friendship>0x80, SEARCH/PLAYER, timer<=0,
//!       dist<80 XZ, <60 Y ──▶ SPEAK demo + 0x075F/0x34AC greeting
//!         │
//!         ▼
//! FORCE_CALL_SET ──SPEAK demo active──▶ setup_talk_start ──▶ START
//!         │
//!         ▼ (talk end)
//! force_call_timer = 300 (≈5 s cooldown), flag = NONE
//! ```
//!
//! Friendship chain (source-proven):
//! player/NPC same block → memory friendship (+over) > 128 →
//! FRIENDSHIP_SEARCH → `aNPC_love_player` (only when player sex !=
//! NPC looks-sex) → REQUEST (msg_no left -1) → NPC approaches
//! (RUN > 3 units, WALK > 1.5 units) → within 80 XZ / 60 Y while
//! SEARCHing the player → automatic greeting.

/// `aNPC_FORCE_CALL_*` (ac_npc.h).
pub mod force_call {
    pub const NONE: u8 = 0;
    pub const REQUEST: u8 = 1;
    pub const SET: u8 = 2;
    pub const START: u8 = 3;
}

/// `aNPC_FRIENDSHIP_*` outcomes of `aNPC_chk_avoid_and_search`.
pub mod friendship {
    pub const NORMAL: u8 = 0;
    pub const AVOID: u8 = 1;
    pub const SEARCH: u8 = 2;
}

/// `mDemo_CAN_ACTOR_TALK`: not the actor of a SPEAK or TALK demo.
pub fn demo_can_actor_talk(in_speak_demo: bool, in_talk_demo: bool) -> bool {
    !in_speak_demo && !in_talk_demo
}

/// `aNPC_force_call_req_proc` verbatim: the three hard gates are
/// `force_call_flag == NONE`, `talk_condition == NONE`, and
/// `mDemo_CAN_ACTOR_TALK`. On success the flag becomes REQUEST and the
/// supplied message is stored; returns TRUE/FALSE.
pub fn force_call_req_proc(
    force_call_flag: u8,
    talk_condition_none: bool,
    can_talk: bool,
) -> Option<()> {
    if force_call_flag == force_call::NONE && talk_condition_none && can_talk {
        Some(())
    } else {
        None
    }
}

/// `aNPC_chk_avoid_and_search` verbatim: requires the player and NPC in
/// the same block and a known friendship pointer; friendship < 0 →
/// AVOID, > 128 → SEARCH, else NORMAL.
pub fn chk_avoid_and_search(
    same_block: bool,
    friendship_known: bool,
    effective_friendship: i32,
) -> u8 {
    if same_block && friendship_known {
        if effective_friendship < 0 {
            return friendship::AVOID;
        } else if effective_friendship > 128 {
            return friendship::SEARCH;
        }
    }
    friendship::NORMAL
}

/// `aNPC_love_player` request gate, verbatim: only when player sex !=
/// NPC looks-sex, flag == NONE, and timer <= 0 does it raise REQUEST.
/// (The movement selection — RUN > 3 units, WALK > 1.5 units — is a
/// separate concern of the same function.)
pub fn love_player_request_gate(
    sex_mismatch: bool,
    force_call_flag: u8,
    force_call_timer: f32,
) -> bool {
    sex_mismatch && force_call_flag == force_call::NONE && force_call_timer <= 0.0
}

/// `aNPC_force_talk_request` verbatim. Returns which SPEAK-demo path to
/// take:
/// - `ForcedMessage`: `force_call_msg_no != -1` → install the stored msg.
/// - `FriendshipGreeting`: all six friendship gates hold → the
///   0x075F/0x34AC greeting selector.
/// - `None`: fall back to normal talk request.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ForceTalkPath {
    ForcedMessage,
    FriendshipGreeting,
}

pub struct ForceTalkInputs {
    pub force_call_msg_no: i32, // -1 = none queued
    pub friendship_known: bool,
    pub effective_friendship: i32,
    pub action_is_search: bool,
    pub act_obj_is_player: bool,
    pub force_call_timer: f32,
    pub player_distance_xz: f32,
    pub player_distance_y: f32, // ABS'd by the source
}

pub fn force_talk_request(inp: &ForceTalkInputs) -> Option<ForceTalkPath> {
    if inp.force_call_msg_no != -1 {
        return Some(ForceTalkPath::ForcedMessage);
    }
    if inp.friendship_known
        && inp.effective_friendship > 0x80
        && inp.action_is_search
        && inp.act_obj_is_player
        && inp.force_call_timer <= 0.0
        && inp.player_distance_xz < 80.0
        && inp.player_distance_y.abs() < 60.0
    {
        return Some(ForceTalkPath::FriendshipGreeting);
    }
    None
}

/// `aNPC_set_talk_info_talk_request_check` message formula:
/// island → `0x34AC + looks*3 + RANDOM(3)`,
/// mainland → `0x075F + looks*3 + RANDOM(3)`.
pub fn auto_greeting_msg_no(is_island: bool, looks: usize, rand3: i32) -> i32 {
    let base = if is_island { 0x34AC } else { 0x075F };
    base + looks as i32 * 3 + rand3
}

/// `aNPC_set_talk_info_force_call` effects, verbatim: installs the
/// stored message + camera into the demo, then clears them and moves
/// the flag to SET.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ForceCallInstall {
    pub msg_no: i32,
    pub camera_type: u8,
}

pub fn set_talk_info_force_call(msg_no: i32, camera_type: u8) -> (ForceCallInstall, u8) {
    (
        ForceCallInstall { msg_no, camera_type },
        force_call::SET,
    )
}

/// `aNPC_setup_talk_end` force-call part, verbatim: when the flag was
/// not NONE, `force_call_timer = 300` (≈5 s at 60 fps) and flag = NONE.
pub fn talk_end_force_call_reset(was_active: bool) -> (f32, u8) {
    if was_active {
        (300.0, force_call::NONE)
    } else {
        (0.0, force_call::NONE)
    }
}

/// Approach speeds in `aNPC_love_player`: RUN beyond 3 units, WALK
/// beyond 1.5 units (unit = 40 world units), else keep current pace.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ApproachPace {
    Run,
    Walk,
    Keep,
}

pub fn love_player_pace(player_distance_xz: f32) -> ApproachPace {
    if player_distance_xz > 3.0 * 40.0 {
        ApproachPace::Run
    } else if player_distance_xz > 1.5 * 40.0 {
        ApproachPace::Walk
    } else {
        ApproachPace::Keep
    }
}

// ---- C ABI ----

/// C ABI: `aNPC_force_call_req_proc` gates; 1 = request queued.
#[no_mangle]
pub extern "C" fn pc_force_call_req_proc(
    force_call_flag: u8,
    talk_condition_none: u8,
    in_speak_demo: u8,
    in_talk_demo: u8,
) -> u8 {
    let can_talk = demo_can_actor_talk(in_speak_demo != 0, in_talk_demo != 0);
    force_call_req_proc(force_call_flag, talk_condition_none != 0, can_talk)
        .is_some() as u8
}

/// C ABI: `aNPC_force_talk_request` path. 0 = none/fallback,
/// 1 = ForcedMessage, 2 = FriendshipGreeting.
#[no_mangle]
pub extern "C" fn pc_force_talk_request(
    force_call_msg_no: i32,
    friendship_known: u8,
    effective_friendship: i32,
    action_is_search: u8,
    act_obj_is_player: u8,
    force_call_timer: f32,
    player_distance_xz: f32,
    player_distance_y: f32,
) -> u8 {
    let inp = ForceTalkInputs {
        force_call_msg_no,
        friendship_known: friendship_known != 0,
        effective_friendship,
        action_is_search: action_is_search != 0,
        act_obj_is_player: act_obj_is_player != 0,
        force_call_timer,
        player_distance_xz,
        player_distance_y,
    };
    match force_talk_request(&inp) {
        None => 0,
        Some(ForceTalkPath::ForcedMessage) => 1,
        Some(ForceTalkPath::FriendshipGreeting) => 2,
    }
}

/// C ABI: automatic greeting message ID.
#[no_mangle]
pub extern "C" fn pc_auto_greeting_msg_no(is_island: u8, looks: u8, rand3: i32) -> i32 {
    auto_greeting_msg_no(is_island != 0, looks as usize, rand3)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn friendship_inputs() -> ForceTalkInputs {
        ForceTalkInputs {
            force_call_msg_no: -1,
            friendship_known: true,
            effective_friendship: 129,
            action_is_search: true,
            act_obj_is_player: true,
            force_call_timer: 0.0,
            player_distance_xz: 79.9,
            player_distance_y: -59.9,
        }
    }

    #[test]
    fn request_gates() {
        // All three gates must hold.
        assert!(force_call_req_proc(force_call::NONE, true, true).is_some());
        assert!(force_call_req_proc(force_call::REQUEST, true, true).is_none());
        assert!(force_call_req_proc(force_call::NONE, false, true).is_none());
        assert!(force_call_req_proc(force_call::NONE, true, false).is_none());
        // mDemo_CAN_ACTOR_TALK: only the two demo types block.
        assert!(demo_can_actor_talk(false, false));
        assert!(!demo_can_actor_talk(true, false));
        assert!(!demo_can_actor_talk(false, true));
        // Friendship classification.
        assert_eq!(chk_avoid_and_search(true, true, -1), friendship::AVOID);
        assert_eq!(chk_avoid_and_search(true, true, 129), friendship::SEARCH);
        assert_eq!(chk_avoid_and_search(true, true, 128), friendship::NORMAL);
        assert_eq!(chk_avoid_and_search(true, true, 0), friendship::NORMAL);
        assert_eq!(chk_avoid_and_search(false, true, 200), friendship::NORMAL);
        assert_eq!(chk_avoid_and_search(true, false, 200), friendship::NORMAL);
        // love_player gate: sex mismatch + NONE + timer<=0.
        assert!(love_player_request_gate(true, force_call::NONE, 0.0));
        assert!(!love_player_request_gate(false, force_call::NONE, 0.0));
        assert!(!love_player_request_gate(true, force_call::REQUEST, 0.0));
        assert!(!love_player_request_gate(true, force_call::NONE, 0.1));
        // C ABI.
        assert_eq!(pc_force_call_req_proc(0, 1, 0, 0), 1);
        assert_eq!(pc_force_call_req_proc(1, 1, 0, 0), 0);
        assert_eq!(pc_force_call_req_proc(0, 1, 1, 0), 0);
    }

    #[test]
    fn force_talk_paths() {
        // Stored message takes priority over everything.
        let mut i = friendship_inputs();
        i.force_call_msg_no = 0x0D8B;
        assert_eq!(force_talk_request(&i), Some(ForceTalkPath::ForcedMessage));
        // Friendship path at the boundary values.
        assert_eq!(force_talk_request(&friendship_inputs()), Some(ForceTalkPath::FriendshipGreeting));
        // Each gate failing kills it.
        let mut i = friendship_inputs();
        i.effective_friendship = 128;
        assert_eq!(force_talk_request(&i), None);
        let mut i = friendship_inputs();
        i.friendship_known = false;
        assert_eq!(force_talk_request(&i), None);
        let mut i = friendship_inputs();
        i.action_is_search = false;
        assert_eq!(force_talk_request(&i), None);
        let mut i = friendship_inputs();
        i.act_obj_is_player = false;
        assert_eq!(force_talk_request(&i), None);
        let mut i = friendship_inputs();
        i.force_call_timer = 0.1;
        assert_eq!(force_talk_request(&i), None);
        let mut i = friendship_inputs();
        i.player_distance_xz = 80.0;
        assert_eq!(force_talk_request(&i), None);
        let mut i = friendship_inputs();
        i.player_distance_y = 60.0;
        assert_eq!(force_talk_request(&i), None);
        // Greeting banks: mainland 0x075F, island 0x34AC.
        assert_eq!(auto_greeting_msg_no(false, 0, 0), 0x075F);
        assert_eq!(auto_greeting_msg_no(false, 5, 2), 0x075F + 15 + 2);
        assert_eq!(auto_greeting_msg_no(true, 3, 1), 0x34AC + 9 + 1);
        // Install + cooldown.
        let (inst, flag) = set_talk_info_force_call(0x0D8B, 2);
        assert_eq!((inst.msg_no, flag), (0x0D8B, force_call::SET));
        assert_eq!(talk_end_force_call_reset(true), (300.0, force_call::NONE));
        assert_eq!(talk_end_force_call_reset(false).0, 0.0);
        // Approach pace: RUN > 120, WALK > 60.
        assert_eq!(love_player_pace(121.0), ApproachPace::Run);
        assert_eq!(love_player_pace(61.0), ApproachPace::Walk);
        assert_eq!(love_player_pace(60.0), ApproachPace::Keep);
        // C ABI.
        let i = friendship_inputs();
        assert_eq!(
            pc_force_talk_request(i.force_call_msg_no, 1, 129, 1, 1, 0.0, 79.9, -59.9),
            2
        );
        assert_eq!(pc_force_talk_request(0x0D8B, 1, 129, 1, 1, 0.0, 79.9, -59.9), 1);
        assert_eq!(pc_force_talk_request(-1, 1, 128, 1, 1, 0.0, 79.9, -59.9), 0);
        assert_eq!(pc_auto_greeting_msg_no(0, 5, 2), 0x075F + 15 + 2);
    }
}
