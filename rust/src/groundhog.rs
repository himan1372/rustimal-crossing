//! Groundhog Day event — Layer A: pure data and helpers.
//!
//! Verified against `src/actor/ac_groundhog_control.c`,
//! `include/ac_groundhog_control.h`, `include/ac_groundhog_control_h.h`,
//! `src/actor/npc/ac_groundhog_npc0.c`,
//! `src/actor/npc/ac_groundhog_npc0_talk.c_inc`,
//! `src/actor/npc/ac_groundhog_npc0_anime.c_inc`,
//! `src/actor/npc/event/ac_ev_majin.c`,
//! `src/actor/npc/event/ac_ev_speech_soncho_talk.c_inc`,
//! `src/actor/ac_event_manager.c`, and
//! `src/game/m_event_schedule.c_inc`
//! (GAFE01_00 Rev 0 USA, via public decomp).
//!
//! Scope: the pure kernels of the Groundhog Day controller — time-term
//! buckets, the clock threshold predicate, initial-action selection,
//! the NPC message-index math, animation timing, spawn-gate and
//! event-manager return trees. The controller lifecycle, scene/fade
//! transitions, common-area allocation, NPC clip, the MAJIN actor and
//! its retire handshake, Tortimer's speech machine, BGM/camera calls,
//! and all actor spawning stay in C.
//!
//! Naming trap (source-proven): the controller spawns SP_NPC_EV_MAJIN
//! (`Ev_Majin_Profile`, the mole actor). `Groundhog_Npc0_Profile` is a
//! DIFFERENT actor, spawned by the special-NPC scheduler — never
//! conflate the two.
//!
//! RNG note: `RANDOM(n)` is `(int)RANDOM_F(n)` (`sys_math.h`), so
//! `RANDOM(3.0f)` yields only {0, 1, 2} — the helpers below take the
//! already-drawn integer sample.

/// Controller actions (`ac_groundhog_control.h`).
pub const ACTION_BEFORE_800: i32 = 0;
pub const ACTION_BIRTH_RESET_WAIT: i32 = 1;
pub const ACTION_BIRTH_RESET: i32 = 2;
pub const ACTION_RETIRE_RESET_WAIT: i32 = 3;
pub const ACTION_RESET_SPEECH_BGM_START_WAIT: i32 = 4;
pub const ACTION_SONCHO_SPEECH_START_WAIT: i32 = 5;
pub const ACTION_SONCHO_SPEECH_END_WAIT: i32 = 6;
pub const ACTION_FADE_OUT_START_WAIT: i32 = 7;
pub const ACTION_AFTER_800: i32 = 8;
/// Per-update completion handshake (`event_state`).
pub const EVENT_STATE_NONE: i32 = 0;
pub const EVENT_STATE_MAJIN_DONE: i32 = 1;
pub const EVENT_STATE_SONCHO_DONE: i32 = 2;
/// Time terms, ascending values over descending thresholds.
pub const TERM_1_HOUR: i32 = 0;
pub const TERM_30_MIN: i32 = 1;
pub const TERM_15_MIN: i32 = 2;
pub const TERM_5_MIN: i32 = 3;
pub const TERM_1_MIN: i32 = 4;
pub const TERM_EVENT_TIME: i32 = 5;
/// Attention targets.
pub const ATTENTION_SONCHO: i32 = 0;
pub const ATTENTION_GROUNDHOG: i32 = 1;
/// Schedule event IDs (`m_event_schedule.c_inc`).
pub const EVENT_RUMOR_GROUNDHOG_DAY: i32 = 6;
pub const EVENT_GROUNDHOG_DAY: i32 = 7;
pub const EVENT_SONCHO_GROUNDHOG_DAY: i32 = 81;
/// Spawned NPC IDs.
pub const SP_NPC_EV_MAJIN: u16 = 0xD081;
pub const SP_NPC_EV_GROUNDHOG_0: u16 = 0xD088;
/// Controller frame timers (decremented by `dt_num_60fps_frames`,
/// i.e. dt-scaled, not 1/frame).
pub const TIMER_BIRTH_WAIT: i32 = 600;
pub const TIMER_SPEECH_BGM: i32 = 240;
pub const TIMER_SONCHO_WAIT: i32 = 60;
pub const TIMER_FADE_OUT: i32 = 20;
/// 08:00 in seconds — also Tortimer's literal `0x7080` threshold.
pub const SEC_8_00: i32 = 28800;
/// Groundhog NPC dialogue bases, indexed by looks (`mNpc_LOOKS_NUM` = 6).
pub const MSG_BASE: [i32; 6] = [15698, 15729, 15605, 15636, 15667, 15760];
/// NPC placement yaw table (`ac_groundhog_npc0.c`): the 0x8000/0x9500
/// entries exceed i16 range and wrap per the C implementation.
pub const DEF_ANGLE: [i16; 4] = [0x6B00, 0x8000u16 as i16, 0x8000u16 as i16, 0x9500u16 as i16];
/// NPC placement X/Z offsets, parallel to `DEF_ANGLE`.
pub const DEF_POS: [[f32; 2]; 4] = [
    [0.0, -20.0],
    [20.0, 0.0],
    [20.0, 0.0],
    [0.0, -20.0],
];

/// Time-term bucket (`aGHC_get_now_term`): descending thresholds —
/// 08:00 → 5, 07:59 → 4, 07:55 → 3, 07:45 → 2, 07:30 → 1, else 0.
/// These are buckets, not a countdown (07:57 is the five-minute term).
#[no_mangle]
pub extern "C" fn pc_ghc_now_term(now_sec: i32) -> i32 {
    if now_sec >= 28800 {
        TERM_EVENT_TIME
    } else if now_sec >= 28740 {
        TERM_1_MIN
    } else if now_sec >= 28500 {
        TERM_5_MIN
    } else if now_sec >= 27900 {
        TERM_15_MIN
    } else if now_sec >= 27000 {
        TERM_30_MIN
    } else {
        TERM_1_HOUR
    }
}

/// `aGHC_TIME_CHECK(hour, min, sec)`: nonzero iff `now_sec` has reached
/// the given clock time. `mTM_SECONDS_IN_HOUR` = 3600, `_MINUTE` = 60.
#[no_mangle]
pub extern "C" fn pc_ghc_time_check(now_sec: i32, hour: i32, min: i32, sec: i32) -> i32 {
    (now_sec >= hour * 3600 + min * 60 + sec) as i32
}

/// Initial controller action: ≥ 08:00 → AFTER_800; an existing common
/// area with `awaiting_birth` set → BIRTH_RESET_WAIT; else BEFORE_800.
/// (An existing area with `awaiting_birth == FALSE` takes the default
/// branch — the phase depends on shared history, not only the clock.)
#[no_mangle]
pub extern "C" fn pc_ghc_initial_action(
    now_sec: i32,
    area_exists: i32,
    awaiting_birth: i32,
) -> i32 {
    if now_sec >= SEC_8_00 {
        ACTION_AFTER_800
    } else if area_exists != 0 && awaiting_birth != 0 {
        ACTION_BIRTH_RESET_WAIT
    } else {
        ACTION_BEFORE_800
    }
}

/// NPC message index (`ac_groundhog_npc0_talk.c_inc`):
/// `msg_base[looks] + now_term * 3 + RANDOM(3)`, with the term
/// decremented FIRST when `now_term >= TERM_1_MIN`. `rand3` is the
/// already-drawn `RANDOM(3)` sample ∈ {0, 1, 2}.
#[no_mangle]
pub extern "C" fn pc_ghc_msg_no(looks: i32, now_term: i32, rand3: i32) -> i32 {
    let term = if now_term >= TERM_1_MIN {
        now_term - 1
    } else {
        now_term
    };
    MSG_BASE[(looks as usize) % MSG_BASE.len()] + term * 3 + rand3
}

/// Actor-creation animation frame speed:
/// `speed = (RANDOM(3.0f) * 0.5f) / 2.0f; frame_speed = speed + 0.4f`.
/// `rand3` ∈ {0, 1, 2}, so the speed takes only {0.4, 0.65, 0.9}.
#[no_mangle]
pub extern "C" fn pc_ghc_frame_speed(rand3: i32) -> f32 {
    (rand3 as f32 * 0.5) / 2.0 + 0.4
}

/// NPC schedule index: `npc_id - SP_NPC_EV_GROUNDHOG_0`, falling back
/// to 0 when outside [0, 3].
#[no_mangle]
pub extern "C" fn pc_ghc_event_idx(npc_id: u16) -> i32 {
    let idx = npc_id.wrapping_sub(SP_NPC_EV_GROUNDHOG_0);
    if idx > 3 {
        0
    } else {
        idx as i32
    }
}

/// One-time WAIT-animation frame randomization:
/// current frame = start frame + `RANDOM(frames)`. `rand_frames` is the
/// already-drawn sample; `start_kf_pos_randomized` guards it to once.
#[no_mangle]
pub extern "C" fn pc_ghc_wait_frame(start_frame: f32, rand_frames: i32) -> f32 {
    start_frame + rand_frames as f32
}

/// Talk availability: the one-minute term replaces the talk-request
/// callback with `none_proc1` (both in talk init and schedule init).
#[no_mangle]
pub extern "C" fn pc_ghc_talk_allowed(now_term: i32) -> i32 {
    (now_term != TERM_1_MIN) as i32
}

/// Tortimer's think-mode selection (`aESS_think_init_proc`): the
/// DEFAULT is 4, not 0. Before 08:00 → 0, except the one-minute term
/// (with the controller clip present) → 1 (no-talk). At/after 08:00 →
/// 2 only when the common area exists and its word is 1.
/// `common_ok` is nonzero iff `mEv_get_common_area(...) != NULL &&
/// *word == 1`.
#[no_mangle]
pub extern "C" fn pc_ghc_soncho_think(
    now_sec: i32,
    now_term: i32,
    has_clip: i32,
    common_ok: i32,
) -> i32 {
    if now_sec < SEC_8_00 {
        if has_clip != 0 && now_term == TERM_1_MIN {
            1
        } else {
            0
        }
    } else if common_ok != 0 {
        2
    } else {
        4
    }
}

/// `groundhog_start` return tree (`ac_event_manager.c`): 0 when the
/// shrine block is missing (STATUS_ERROR); 2 when already kept; 1 when
/// the title fade succeeds (then FG placement 5 + actor placement 4);
/// 0 when the fade is still pending.
#[no_mangle]
pub extern "C" fn pc_ghc_start(shrine_exists: i32, kept: i32, fade_ok: i32) -> i32 {
    if shrine_exists == 0 {
        0
    } else if kept != 0 {
        2
    } else if fade_ok != 0 {
        1
    } else {
        0
    }
}

/// `groundhog_stop` return tree: 2 when not kept; 1 when kept and the
/// closing title fade completes (then FG placement 5 is deleted);
/// 0 when the fade is pending.
#[no_mangle]
pub extern "C" fn pc_ghc_stop(kept: i32, fade_done: i32) -> i32 {
    if kept == 0 {
        2
    } else if fade_done != 0 {
        1
    } else {
        0
    }
}

/// `groundhog_in` placement mapping: `shown` is -1 for
/// `aEvMgr_SHOW_ACTOR_RESULT_NOT_SHOWN`, 0 for NULL, nonzero for a
/// valid placement.
#[no_mangle]
pub extern "C" fn pc_ghc_in(shown: i32) -> i32 {
    if shown == -1 {
        2
    } else if shown != 0 {
        1
    } else {
        0
    }
}

/// `groundhog_in` re-creation: when placement returned nonzero AND the
/// common-area word is 1, the controller and FG placement are
/// re-created. `save_ok` is nonzero iff the common area exists.
#[no_mangle]
pub extern "C" fn pc_ghc_in_recreate(in_ret: i32, save_ok: i32, word: i32) -> i32 {
    (in_ret != 0 && save_ok != 0 && word == 1) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_terms() {
        // Boundaries: 07:29:59, 07:30, 07:45, 07:55, 07:59, 08:00.
        assert_eq!(pc_ghc_now_term(7 * 3600 + 29 * 60 + 59), TERM_1_HOUR);
        assert_eq!(pc_ghc_now_term(7 * 3600 + 30 * 60), TERM_30_MIN);
        assert_eq!(pc_ghc_now_term(7 * 3600 + 45 * 60), TERM_15_MIN);
        assert_eq!(pc_ghc_now_term(7 * 3600 + 55 * 60), TERM_5_MIN);
        assert_eq!(pc_ghc_now_term(7 * 3600 + 59 * 60), TERM_1_MIN);
        assert_eq!(pc_ghc_now_term(8 * 3600), TERM_EVENT_TIME);
        // 07:57 is the five-minute term (buckets, not a countdown).
        assert_eq!(pc_ghc_now_term(7 * 3600 + 57 * 60), TERM_5_MIN);
        // Threshold predicate.
        assert_eq!(pc_ghc_time_check(28800, 8, 0, 0), 1);
        assert_eq!(pc_ghc_time_check(28799, 8, 0, 0), 0);
        // Initial action.
        assert_eq!(pc_ghc_initial_action(28800, 0, 0), ACTION_AFTER_800);
        assert_eq!(pc_ghc_initial_action(27000, 1, 1), ACTION_BIRTH_RESET_WAIT);
        assert_eq!(pc_ghc_initial_action(27000, 1, 0), ACTION_BEFORE_800);
        assert_eq!(pc_ghc_initial_action(27000, 0, 0), ACTION_BEFORE_800);
    }

    #[test]
    fn npc_kernels() {
        // Message index: term decremented first when >= TERM_1_MIN.
        assert_eq!(pc_ghc_msg_no(0, TERM_1_MIN, 0), 15698 + 3 * 3);
        assert_eq!(pc_ghc_msg_no(0, TERM_5_MIN, 2), 15698 + 3 * 3 + 2);
        assert_eq!(pc_ghc_msg_no(5, TERM_1_HOUR, 1), 15760 + 0 + 1);
        // Frame speed: only three discrete values.
        assert_eq!(pc_ghc_frame_speed(0), 0.4);
        assert_eq!(pc_ghc_frame_speed(1), 0.65);
        assert_eq!(pc_ghc_frame_speed(2), 0.9);
        // Event index with [0,3]-else-0 fallback.
        assert_eq!(pc_ghc_event_idx(0xD088), 0);
        assert_eq!(pc_ghc_event_idx(0xD08B), 3);
        assert_eq!(pc_ghc_event_idx(0xD08C), 0);
        // Placement tables (0x8000/0x9500 wrap into i16 range).
        assert_eq!(DEF_ANGLE, [0x6B00, -32768, -32768, -27392]);
        assert_eq!(DEF_POS[1], [20.0, 0.0]);
        // WAIT frame randomization.
        assert_eq!(pc_ghc_wait_frame(10.0, 7), 17.0);
        // Talk disabled in the one-minute term.
        assert_eq!(pc_ghc_talk_allowed(TERM_1_MIN), 0);
        assert_eq!(pc_ghc_talk_allowed(TERM_5_MIN), 1);
    }

    #[test]
    fn orchestration() {
        // Tortimer think: default is 4, not 0.
        assert_eq!(pc_ghc_soncho_think(27000, TERM_1_MIN, 1, 0), 1);
        assert_eq!(pc_ghc_soncho_think(27000, TERM_5_MIN, 1, 0), 0);
        assert_eq!(pc_ghc_soncho_think(27000, TERM_1_MIN, 0, 0), 0); // no clip
        assert_eq!(pc_ghc_soncho_think(28800, 0, 1, 1), 2);
        assert_eq!(pc_ghc_soncho_think(28800, 0, 1, 0), 4);
        // Event-manager return trees.
        assert_eq!(pc_ghc_start(0, 0, 1), 0);
        assert_eq!(pc_ghc_start(1, 1, 1), 2);
        assert_eq!(pc_ghc_start(1, 0, 1), 1);
        assert_eq!(pc_ghc_start(1, 0, 0), 0);
        assert_eq!(pc_ghc_stop(0, 1), 2);
        assert_eq!(pc_ghc_stop(1, 1), 1);
        assert_eq!(pc_ghc_stop(1, 0), 0);
        assert_eq!(pc_ghc_in(-1), 2);
        assert_eq!(pc_ghc_in(1), 1);
        assert_eq!(pc_ghc_in(0), 0);
        assert_eq!(pc_ghc_in_recreate(1, 1, 1), 1);
        assert_eq!(pc_ghc_in_recreate(1, 1, 0), 0);
        assert_eq!(pc_ghc_in_recreate(0, 1, 1), 0);
    }
}
