//! Player action state machines: scoop family, wade, pitfall climb.
//!
//! Verified against `include/m_player.h`, `m_player_common.c_inc`,
//! `m_player_main_dig_scoop.c_inc`, `m_player_main_get_scoop.c_inc`,
//! `m_player_main_fill_scoop.c_inc`, `m_player_main_wade.c_inc`,
//! `m_player_main_climbup_pitfall.c_inc`, `m_player_lib.c`
//! (USA Rev. 0 decomp).
//!
//! Architecture: the player has `now_main_index` (current action) plus a
//! requested-action triple (`requested_main_index`,
//! `requested_main_index_priority`, `requested_main_index_changed`).
//! Systems don't set the state directly; they call
//! `Player_actor_request_main_index`, and the request wins only if its
//! priority exceeds the pending one (`priority - requested_priority > 0`).
//! Animation frames are the gameplay clock: world mutations and
//! notifications fire at exact keyframes.

/// `mPlayer_INDEX_*` — the full main-action enum in exact C order so
/// indices line up with the decomp.
pub mod index {
    pub const DMA: i32 = 0;
    pub const INTRO: i32 = 1;
    pub const REFUSE: i32 = 2;
    pub const REFUSE_PICKUP: i32 = 3;
    pub const RETURN_DEMO: i32 = 4;
    pub const RETURN_OUTDOOR: i32 = 5;
    pub const RETURN_OUTDOOR2: i32 = 6;
    pub const WAIT: i32 = 7;
    pub const WALK: i32 = 8;
    pub const RUN: i32 = 9;
    pub const DASH: i32 = 10;
    pub const TUMBLE: i32 = 11;
    pub const TUMBLE_GETUP: i32 = 12;
    pub const TURN_DASH: i32 = 13;
    pub const FALL: i32 = 14;
    pub const WADE: i32 = 15;
    pub const DOOR: i32 = 16;
    pub const OUTDOOR: i32 = 17;
    pub const INVADE: i32 = 18;
    pub const HOLD: i32 = 19;
    pub const PUSH: i32 = 20;
    pub const PULL: i32 = 21;
    pub const ROTATE_FURNITURE: i32 = 22;
    pub const OPEN_FURNITURE: i32 = 23;
    pub const WAIT_OPEN_FURNITURE: i32 = 24;
    pub const CLOSE_FURNITURE: i32 = 25;
    pub const LIE_BED: i32 = 26;
    pub const WAIT_BED: i32 = 27;
    pub const ROLL_BED: i32 = 28;
    pub const STANDUP_BED: i32 = 29;
    pub const PICKUP: i32 = 30;
    pub const PICKUP_JUMP: i32 = 31;
    pub const PICKUP_FURNITURE: i32 = 32;
    pub const PICKUP_EXCHANGE: i32 = 33;
    pub const SITDOWN: i32 = 34;
    pub const SITDOWN_WAIT: i32 = 35;
    pub const STANDUP: i32 = 36;
    pub const SWING_AXE: i32 = 37;
    pub const AIR_AXE: i32 = 38;
    pub const REFLECT_AXE: i32 = 39;
    pub const BROKEN_AXE: i32 = 40;
    pub const SLIP_NET: i32 = 41;
    pub const READY_NET: i32 = 42;
    pub const READY_WALK_NET: i32 = 43;
    pub const SWING_NET: i32 = 44;
    pub const PULL_NET: i32 = 45;
    pub const STOP_NET: i32 = 46;
    pub const NOTICE_NET: i32 = 47;
    pub const PUTAWAY_NET: i32 = 48;
    pub const READY_ROD: i32 = 49;
    pub const CAST_ROD: i32 = 50;
    pub const AIR_ROD: i32 = 51;
    pub const RELAX_ROD: i32 = 52;
    pub const COLLECT_ROD: i32 = 53;
    pub const VIB_ROD: i32 = 54;
    pub const FLY_ROD: i32 = 55;
    pub const NOTICE_ROD: i32 = 56;
    pub const PUTAWAY_ROD: i32 = 57;
    pub const DIG_SCOOP: i32 = 58;
    pub const FILL_SCOOP: i32 = 59;
    pub const REFLECT_SCOOP: i32 = 60;
    pub const AIR_SCOOP: i32 = 61;
    pub const GET_SCOOP: i32 = 62;
    pub const PUTAWAY_SCOOP: i32 = 63;
    pub const PUTIN_SCOOP: i32 = 64;
    pub const TALK: i32 = 65;
    pub const RECIEVE_WAIT: i32 = 66;
    pub const RECIEVE_STRETCH: i32 = 67;
    pub const RECIEVE: i32 = 68;
    pub const RECIEVE_PUTAWAY: i32 = 69;
    pub const GIVE: i32 = 70;
    pub const GIVE_WAIT: i32 = 71;
    pub const TAKEOUT_ITEM: i32 = 72;
    pub const PUTIN_ITEM: i32 = 73;
    pub const DEMO_WAIT: i32 = 74;
    pub const DEMO_WALK: i32 = 75;
    pub const DEMO_GETON_TRAIN: i32 = 76;
    pub const DEMO_GETON_TRAIN_WAIT: i32 = 77;
    pub const DEMO_GETOFF_TRAIN: i32 = 78;
    pub const DEMO_STANDING_TRAIN: i32 = 79;
    pub const DEMO_WADE: i32 = 80;
    pub const HIDE: i32 = 81;
    pub const GROUNDHOG: i32 = 82;
    pub const RELEASE_CREATURE: i32 = 83;
    pub const WASH_CAR: i32 = 84;
    pub const TIRED: i32 = 85;
    pub const ROTATE_OCTAGON: i32 = 86;
    pub const THROW_MONEY: i32 = 87;
    pub const PRAY: i32 = 88;
    pub const SHAKE_TREE: i32 = 89;
    pub const MAIL_JUMP: i32 = 90;
    pub const MAIL_LAND: i32 = 91;
    pub const READY_PITFALL: i32 = 92;
    pub const FALL_PITFALL: i32 = 93;
    pub const STRUGGLE_PITFALL: i32 = 94;
    pub const CLIMBUP_PITFALL: i32 = 95;
    pub const STUNG_BEE: i32 = 96;
    pub const NOTICE_BEE: i32 = 97;
    pub const REMOVE_GRASS: i32 = 98;
    pub const SHOCK: i32 = 99;
    pub const KNOCK_DOOR: i32 = 100;
    pub const CHANGE_CLOTH: i32 = 101;
    pub const PUSH_SNOWBALL: i32 = 102;
    pub const ROTATE_UMBRELLA: i32 = 103;
    pub const WADE_SNOWBALL: i32 = 104;
    pub const COMPLETE_PAYMENT: i32 = 105;
    pub const FAIL_EMU: i32 = 106;
    pub const STUNG_MOSQUITO: i32 = 107;
    pub const NOTICE_MOSQUITO: i32 = 108;
    pub const SWING_FAN: i32 = 109;
    pub const SWITCH_ON_LIGHTHOUSE: i32 = 110;
    pub const RADIO_EXERCISE: i32 = 111;
    pub const DEMO_GETON_BOAT: i32 = 112;
    pub const DEMO_GETON_BOAT_SITDOWN: i32 = 113;
    pub const DEMO_GETON_BOAT_WAIT: i32 = 114;
    pub const DEMO_GETON_BOAT_WADE: i32 = 115;
    pub const DEMO_GETOFF_BOAT_STANDUP: i32 = 116;
    pub const DEMO_GETOFF_BOAT: i32 = 117;
    pub const DEMO_GET_GOLDEN_ITEM: i32 = 118;
    pub const DEMO_GET_GOLDEN_ITEM2: i32 = 119;
    pub const DEMO_GET_GOLDEN_AXE_WAIT: i32 = 120;
    pub const NUM: i32 = 121;

    pub fn valid(idx: i32) -> bool {
        idx >= 0 && idx < NUM
    }
}

/// `Player_actor_check_request_main_priority` verbatim:
/// `priority - requested_priority`. The request is admissible only
/// when this is > 0 (plus the two cancel/reset gates, which need
/// game state and are left to the C side).
pub fn check_request_main_priority(priority: i32, requested_priority: i32) -> i32 {
    priority - requested_priority
}

/// Admissibility of a request given the priority comparison result.
pub fn request_admissible(priority_delta: i32) -> bool {
    priority_delta > 0
}

/// `mPlayer_REQUEST_PRIORITY_*` exist as PRIORITY_0..44 (45 levels);
/// named ones used by the scoop family (verified at the call sites).
pub mod priority {
    pub const DIG_SCOOP: i32 = 21; // used by get->putaway/putin continuations
}

/// DIG_SCOOP animation-frame events (mPlayer_ANIM_DIG1 path), verbatim.
/// Frame 14/15/16 → DIG_HOLE effect args 0/1/2; frame 22 → DIG_SCOOP
/// effect (arg 0, or 3 when the main index is GET_SCOOP).
/// Returns the (effect, arg) pair for the given frame, if any.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DigEffect {
    DigHole(u8),
    DigScoop(u8),
}

pub fn dig_scoop_frame_event(frame: f32, is_get_scoop: bool) -> Option<DigEffect> {
    if frame == 14.0 {
        Some(DigEffect::DigHole(0))
    } else if frame == 15.0 {
        Some(DigEffect::DigHole(1))
    } else if frame == 16.0 {
        Some(DigEffect::DigHole(2))
    } else if frame == 22.0 {
        Some(DigEffect::DigScoop(if is_get_scoop { 3 } else { 0 }))
    } else {
        None
    }
}

/// Tree-stump dig variant: single event at frame 42.
pub fn dig_kabu_frame_event(frame: f32, is_get_scoop: bool) -> Option<DigEffect> {
    if frame == 42.0 {
        Some(DigEffect::DigScoop(if is_get_scoop { 3 } else { 0 }))
    } else {
        None
    }
}

/// World hole registration: `mod + 20.0` (mod accounts for the animation
/// variant); decal circle radius 19, arg 12. Returns TRUE at the frame.
pub fn dig_hole_register_frame(frame: f32, anim_mod: f32) -> bool {
    frame == anim_mod + 20.0
}

/// GET_SCOOP item scale timeline, verbatim:
/// `frame <= 21` → 0; `21 < frame < 27` → `0.0016666666 * (frame-21)`;
/// `frame >= 27` → 0.01.
pub fn get_scoop_scale(frame: f32) -> f32 {
    if frame <= 21.0 {
        0.0
    } else if frame < 27.0 {
        0.0016666666 * (frame - 21.0)
    } else {
        0.01
    }
}

/// GET_SCOOP continuation protocol: control 0x3F → PUTAWAY_SCOOP
/// (priority 21), 0x40 → PUTIN_SCOOP (priority 21).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GetScoopContinuation {
    Putaway,
    Putin,
}

pub fn get_scoop_continuation(control: i32) -> Option<GetScoopContinuation> {
    match control {
        0x3F => Some(GetScoopContinuation::Putaway),
        0x40 => Some(GetScoopContinuation::Putin),
        _ => None,
    }
}

/// FILL_SCOOP frame events, verbatim: hole removal at `18 + mod`;
/// scoop impact effects at 13/19/25 (+mod) with args 3/4/5;
/// final DIG_SCOOP effect at 40 (+mod).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FillEffect {
    RemoveHole,
    Impact(u8),
    Final,
}

pub fn fill_scoop_frame_event(frame: f32, anim_mod: f32) -> Option<FillEffect> {
    if frame == 18.0 + anim_mod {
        Some(FillEffect::RemoveHole)
    } else if frame == 13.0 + anim_mod {
        Some(FillEffect::Impact(3))
    } else if frame == 19.0 + anim_mod {
        Some(FillEffect::Impact(4))
    } else if frame == 25.0 + anim_mod {
        Some(FillEffect::Impact(5))
    } else if frame == 40.0 + anim_mod {
        Some(FillEffect::Final)
    } else {
        None
    }
}

/// `Player_actor_Check_DigScoop` verbatim: TRUE (with the scoop target
/// position) when `now_main_index` is one of DIG/REFLECT/GET/FILL/
/// PUTIN_SCOOP. AIR and PUTAWAY are NOT included.
pub fn check_dig_scoop(now_main_index: i32) -> bool {
    matches!(
        now_main_index,
        index::DIG_SCOOP
            | index::REFLECT_SCOOP
            | index::GET_SCOOP
            | index::FILL_SCOOP
            | index::PUTIN_SCOOP
    )
}

/// WADE constants: end position ~18 units along the direction;
/// movement runs on a 36-frame accel/brake curve; when the timer
/// exceeds 36 the state requests WALK to the end position.
pub const WADE_DISTANCE: f32 = 18.00001;
pub const WADE_FRAMES: f32 = 36.0;
pub const WADE_ACCEL: f32 = 1.1999999;
pub const WADE_BRAKE: f32 = 34.8;

/// WADE camera: `Camera2_request_main_wade(play, &eye_pos, 9, 36.0)`.
pub const WADE_CAMERA_ARG: i32 = 9;

/// Whether the wade timer has elapsed and the state may transition.
pub fn wade_finished(timer: f32) -> bool {
    timer > WADE_FRAMES
}

/// CLIMBUP_PITFALL animation selection, verbatim:
/// umbrella held → DERU2, otherwise DERU1.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ClimbAnim {
    Deru1,
    Deru2,
}

pub fn climbup_pitfall_anim(holding_umbrella: bool) -> ClimbAnim {
    if holding_umbrella {
        ClimbAnim::Deru2
    } else {
        ClimbAnim::Deru1
    }
}

// ---- C ABI ----

/// C ABI: priority comparison; >0 means the request may proceed.
#[no_mangle]
pub extern "C" fn pc_player_request_priority_delta(priority: i32, requested_priority: i32) -> i32 {
    check_request_main_priority(priority, requested_priority)
}

/// C ABI: DIG_SCOOP frame event. Returns 0 = none, 1 = DigHole(arg),
/// 2 = DigScoop(arg); arg written to `out_arg`.
#[no_mangle]
pub extern "C" fn pc_dig_scoop_frame_event(frame: f32, is_get_scoop: u8, out_arg: *mut u8) -> u8 {
    match dig_scoop_frame_event(frame, is_get_scoop != 0) {
        None => 0,
        Some(DigEffect::DigHole(a)) => {
            if !out_arg.is_null() {
                unsafe { *out_arg = a };
            }
            1
        }
        Some(DigEffect::DigScoop(a)) => {
            if !out_arg.is_null() {
                unsafe { *out_arg = a };
            }
            2
        }
    }
}

/// C ABI: FILL_SCOOP frame event. 0 = none, 1 = RemoveHole,
/// 2 = Impact(arg), 3 = Final.
#[no_mangle]
pub extern "C" fn pc_fill_scoop_frame_event(frame: f32, anim_mod: f32, out_arg: *mut u8) -> u8 {
    match fill_scoop_frame_event(frame, anim_mod) {
        None => 0,
        Some(FillEffect::RemoveHole) => 1,
        Some(FillEffect::Impact(a)) => {
            if !out_arg.is_null() {
                unsafe { *out_arg = a };
            }
            2
        }
        Some(FillEffect::Final) => 3,
    }
}

/// C ABI: GET_SCOOP continuation. 0 = none, 1 = Putaway, 2 = Putin.
#[no_mangle]
pub extern "C" fn pc_get_scoop_continuation(control: i32) -> u8 {
    match get_scoop_continuation(control) {
        None => 0,
        Some(GetScoopContinuation::Putaway) => 1,
        Some(GetScoopContinuation::Putin) => 2,
    }
}

/// C ABI: `Player_actor_Check_DigScoop`.
#[no_mangle]
pub extern "C" fn pc_check_dig_scoop(now_main_index: i32) -> u8 {
    check_dig_scoop(now_main_index) as u8
}

/// C ABI: WADE finished check.
#[no_mangle]
pub extern "C" fn pc_wade_finished(timer: f32) -> u8 {
    wade_finished(timer) as u8
}

/// C ABI: CLIMBUP_PITFALL animation; 0 = DERU1, 1 = DERU2.
#[no_mangle]
pub extern "C" fn pc_climbup_pitfall_anim(holding_umbrella: u8) -> u8 {
    match climbup_pitfall_anim(holding_umbrella != 0) {
        ClimbAnim::Deru1 => 0,
        ClimbAnim::Deru2 => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn main_index_enum() {
        // Spot-check indices against the C enum order.
        assert_eq!(index::WAIT, 7);
        assert_eq!(index::WADE, 15);
        assert_eq!(index::DIG_SCOOP, 58);
        assert_eq!(index::FILL_SCOOP, 59);
        assert_eq!(index::REFLECT_SCOOP, 60);
        assert_eq!(index::AIR_SCOOP, 61);
        assert_eq!(index::GET_SCOOP, 62);
        assert_eq!(index::PUTAWAY_SCOOP, 63);
        assert_eq!(index::PUTIN_SCOOP, 64);
        assert_eq!(index::CLIMBUP_PITFALL, 95);
        assert_eq!(index::WADE_SNOWBALL, 104);
        assert_eq!(index::NUM, 121);
        assert!(index::valid(0));
        assert!(index::valid(120));
        assert!(!index::valid(121));
        assert!(!index::valid(-1));
    }

    #[test]
    fn request_priority() {
        // Strictly greater priority required.
        assert_eq!(check_request_main_priority(21, 20), 1);
        assert_eq!(check_request_main_priority(20, 20), 0);
        assert_eq!(check_request_main_priority(19, 20), -1);
        assert!(request_admissible(1));
        assert!(!request_admissible(0));
    }

    #[test]
    fn scoop_frames() {
        // DIG_SCOOP timeline.
        assert_eq!(dig_scoop_frame_event(14.0, false), Some(DigEffect::DigHole(0)));
        assert_eq!(dig_scoop_frame_event(15.0, false), Some(DigEffect::DigHole(1)));
        assert_eq!(dig_scoop_frame_event(16.0, false), Some(DigEffect::DigHole(2)));
        assert_eq!(dig_scoop_frame_event(22.0, false), Some(DigEffect::DigScoop(0)));
        assert_eq!(dig_scoop_frame_event(22.0, true), Some(DigEffect::DigScoop(3)));
        assert_eq!(dig_scoop_frame_event(21.0, false), None);
        // Stump variant.
        assert_eq!(dig_kabu_frame_event(42.0, false), Some(DigEffect::DigScoop(0)));
        assert_eq!(dig_kabu_frame_event(41.0, false), None);
        // Hole registration.
        assert!(dig_hole_register_frame(20.0, 0.0));
        assert!(!dig_hole_register_frame(19.0, 0.0));
        // GET_SCOOP scale.
        assert_eq!(get_scoop_scale(21.0), 0.0);
        assert!((get_scoop_scale(24.0) - 0.0016666666 * 3.0).abs() < 1e-7);
        assert_eq!(get_scoop_scale(27.0), 0.01);
        // Continuation protocol.
        assert_eq!(get_scoop_continuation(0x3F), Some(GetScoopContinuation::Putaway));
        assert_eq!(get_scoop_continuation(0x40), Some(GetScoopContinuation::Putin));
        assert_eq!(get_scoop_continuation(0x41), None);
        // FILL_SCOOP timeline.
        assert_eq!(fill_scoop_frame_event(18.0, 0.0), Some(FillEffect::RemoveHole));
        assert_eq!(fill_scoop_frame_event(13.0, 0.0), Some(FillEffect::Impact(3)));
        assert_eq!(fill_scoop_frame_event(19.0, 0.0), Some(FillEffect::Impact(4)));
        assert_eq!(fill_scoop_frame_event(25.0, 0.0), Some(FillEffect::Impact(5)));
        assert_eq!(fill_scoop_frame_event(40.0, 0.0), Some(FillEffect::Final));
        assert_eq!(fill_scoop_frame_event(20.0, 0.0), None);
        // Check_DigScoop membership.
        for idx in [index::DIG_SCOOP, index::REFLECT_SCOOP, index::GET_SCOOP, index::FILL_SCOOP, index::PUTIN_SCOOP] {
            assert!(check_dig_scoop(idx));
        }
        assert!(!check_dig_scoop(index::AIR_SCOOP));
        assert!(!check_dig_scoop(index::PUTAWAY_SCOOP));
        assert!(!check_dig_scoop(index::WAIT));
        // C ABI.
        let mut a = 0u8;
        assert_eq!(pc_dig_scoop_frame_event(14.0, 0, &mut a), 1);
        assert_eq!(a, 0);
        assert_eq!(pc_fill_scoop_frame_event(13.0, 0.0, &mut a), 2);
        assert_eq!(a, 3);
        assert_eq!(pc_get_scoop_continuation(0x3F), 1);
        assert_eq!(pc_get_scoop_continuation(0x40), 2);
        assert_eq!(pc_check_dig_scoop(index::DIG_SCOOP), 1);
        assert_eq!(pc_check_dig_scoop(index::AIR_SCOOP), 0);
    }

    #[test]
    fn wade_and_climb() {
        assert!((WADE_DISTANCE - 18.00001).abs() < 1e-5);
        assert_eq!(WADE_FRAMES, 36.0);
        assert!(!wade_finished(36.0));
        assert!(wade_finished(36.1));
        assert_eq!(pc_wade_finished(37.0), 1);
        assert_eq!(climbup_pitfall_anim(false), ClimbAnim::Deru1);
        assert_eq!(climbup_pitfall_anim(true), ClimbAnim::Deru2);
        assert_eq!(pc_climbup_pitfall_anim(0), 0);
        assert_eq!(pc_climbup_pitfall_anim(1), 1);
        assert_eq!(pc_player_request_priority_delta(21, 20), 1);
    }
}
