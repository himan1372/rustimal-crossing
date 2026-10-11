//! Train arrival/departure, Rover, Porter, station scenes — pure data and kernels.
//!
//! Verified against `src/game/m_train_control.c`,
//! `include/m_train_control.h`, `src/actor/ac_train1_move.c_inc`,
//! `src/actor/ac_train_door.c`, `src/actor/ac_train_window.c`,
//! `src/actor/ac_station.c`, `src/actor/npc/ac_npc_station_master_talk.c_inc`,
//! `src/actor/ac_intro_demo_move.c_inc`, `include/m_name_table.h`,
//! `include/ac_station_clip.h`, `include/m_camera2.h`
//! (GAFE01_00 Rev 0 USA decomp). All 14 brief claims checked verbatim;
//! no corrections, five precision nuances carried below.
//!
//! Architecture: the train controller (`mTRC_move`), both train actors,
//! door/window/station actors, Porter's NPC state machine, the intro
//! demo, and the Rover/Blanca scene selection all stay in C. This module
//! holds only the source-proven pure data and calculations with
//! primitive arguments.
//!
//! IMPORTANT frame-rate nuance (brief prose drifted here): the
//! controller's per-frame X advance is `0.5f * speed * dt`, all
//! `chase_f` rates are multiplied by `dt`, and timer countdowns consume
//! `graph_dt_60hz_ticks()` units — not plain `timer--`. A port that
//! hard-codes 60 Hz or drops `dt` will drift. The thresholds below are
//! exact; the integration loop stays in C.

/// Controller actions (`include/m_train_control.h`).
pub mod action {
    pub const NONE: u8 = 0;
    pub const SPAWN_MOVING: u8 = 1;
    pub const BEGIN_SLOWDOWN: u8 = 2;
    pub const BEGIN_STOP: u8 = 3;
    pub const SIGNAL_STOPPED: u8 = 4;
    pub const WAIT_STOPPED: u8 = 5;
    pub const SIGNAL_STARTING: u8 = 6;
    pub const BEGIN_PULL_OUT: u8 = 7;
    pub const SPEED_UP: u8 = 8;
    pub const NUM: u8 = 9;
}

/// USA speed constants (`#if VERSION >= VER_GAFU01_00` branch;
/// the `#else` values are 2.0/6.0/0.01/0.005/0.00345).
pub mod speed {
    pub const SLOW: f32 = 2.6;
    pub const FAST: f32 = 7.2;
    /// Keep the literal; do not "clean up" to 0.21.
    pub const SLOW_RATE: f32 = 0.21000001;
    pub const STOP_RATE: f32 = 0.0084;
    pub const START_RATE: f32 = 0.0047;
    pub const SPEEDUP_RATE: f32 = 0.0047;
}

/// Controller transition thresholds (`mTRC_trainControl`), all strict
/// comparisons as in the decomp.
pub mod threshold {
    /// Slowdown → stop when `pos.x > 2165.0f`.
    pub const SLOWDOWN_STOP_X: f32 = 2165.0;
    /// Stop when `fabsf(speed) < 0.008f`.
    pub const STOP_SPEED: f32 = 0.008;
    /// Stopped-signal tick count.
    pub const STOPPED_TIMER: u8 = 48;
    /// Seconds added to the start timer when entering wait.
    pub const WAIT_ADD_SECS: u32 = 310;
    /// Wait → starting-signal tick count.
    pub const STARTING_TIMER: u8 = 84;
    /// Starting → pull-out tick count.
    pub const PULL_OUT_TIMER: u8 = 180;
    /// Speed-up → done when `pos.x > 4400.0f`.
    pub const DONE_X: f32 = 4400.0;
    /// Per-frame X advance factor: `pos.x += 0.5f * speed * dt`.
    pub const X_ADVANCE: f32 = 0.5;
}

/// Seconds in a day (`mTM_SECONDS_IN_DAY`).
pub const SECONDS_IN_DAY: u32 = 24 * 60 * 60;

/// Timetable (`mTRC_get_depart_time`): 25 entries, hours 00–24, each at
/// hour:19:00. The scan takes the first entry `>=` now and subtracts
/// 4 minutes 10 seconds. The 24:19:00 entry always exceeds max now_sec,
/// so the scan terminates.
pub fn timetable_entry(i: u8) -> u32 {
    i as u32 * 3600 + 19 * 60
}

/// Departure start timer from the RTC time, verbatim:
/// first timetable entry `>=` now_sec, minus 250 seconds.
/// (`train_day` is saved by the C caller.)
pub fn depart_timer(hour: u8, min: u8, sec: u8) -> u32 {
    let now_sec = hour as u32 * 3600 + min as u32 * 60 + sec as u32;
    let mut i = 0u8;
    loop {
        let entry = timetable_entry(i);
        if entry >= now_sec {
            return entry - 250;
        }
        i += 1;
    }
}

/// Day-change rule: when the day changes and `start_timer >= 86400`,
/// subtract one day. Comparison is `>=`.
pub fn day_rollover(start_timer: u32) -> u32 {
    if start_timer >= SECONDS_IN_DAY {
        start_timer - SECONDS_IN_DAY
    } else {
        start_timer
    }
}

/// The four init paths (`m_train_control.c`): starting X, initial action,
/// initial control state. All use Z 740.0f; Y is
/// `mRF_CheckFieldStep3() ? 300 : 180` on USA (a C-side check).
pub mod init {
    pub const Z: f32 = 740.0;
    /// `mTRC_mati_init`: stopped/waiting.
    pub const MATI_X: f32 = 2367.0;
    pub const MATI_ACTION: u8 = super::action::WAIT_STOPPED;
    pub const MATI_CTRL: u8 = 1;
    /// `mTRC_demo_init`: slowdown path, speed = SLOW. Also sets
    /// `train_start_timer = now_sec − 4:50` — a different literal from
    /// the −4:10 in `depart_timer`; keep both.
    pub const DEMO_X: f32 = 2037.0;
    pub const DEMO_ACTION: u8 = super::action::BEGIN_SLOWDOWN;
    pub const DEMO_CTRL: u8 = 0;
    pub const DEMO_TIMER_SUB_SECS: u32 = 4 * 60 + 50;
    /// `mTRC_call_init`: slowdown path, zero initial speed.
    pub const CALL_X: f32 = 1904.0;
    pub const CALL_ACTION: u8 = super::action::BEGIN_SLOWDOWN;
    pub const CALL_CTRL: u8 = 1;
    /// `mTRC_norm_init`: moving spawn path.
    pub const NORM_X: f32 = 320.0;
    pub const NORM_ACTION: u8 = super::action::SPAWN_MOVING;
    pub const NORM_CTRL: u8 = 0;
}

/// Train1 (coupled car) animation tables (`aTR1_setupAction`).
/// Indexing asymmetry, verbatim: `anime_table`/`anime_end` index by
/// `seq_no` (0/1); `anime_spd` indexes by `action` (0–8).
pub mod train1 {
    pub const SEQ_NO_TABLE: [u8; 9] = [0, 0, 0, 0, 0, 1, 1, 0, 0];
    pub const ANIM_SPD: [f32; 9] = [0.0, 0.0, 0.0, 0.0, 0.5, 0.0, 0.5, 0.0, 0.0];
    /// Animation end frames by seq_no: open = 24.0, close = 32.0.
    pub const ANIM_END: [f32; 2] = [24.0, 32.0];
}

/// Resolve a train1 action to (seq_no, anim_speed, anim_end).
pub fn train1_anim(action: u8) -> (u8, f32, f32) {
    let a = (action % 9) as usize;
    let seq = train1::SEQ_NO_TABLE[a];
    (seq, train1::ANIM_SPD[a], train1::ANIM_END[seq as usize])
}

/// Passenger forced positions (`ac_train1_move.c_inc`).
/// arg0==1 → (x+60, y+20, z+20) with a zeroed rotation vector and the
/// ROTY force flag; arg1==1 → (x+2, y+16, z+20) with `rot = NULL` and
/// no ROTY flag. Offsets are relative to the car's world position.
pub mod passenger {
    pub const ARG0_OFFSET: (f32, f32, f32) = (60.0, 20.0, 20.0);
    /// arg0 passes a zeroed rot and forces ROTY.
    pub const ARG0_FORCE_ROTY: bool = true;
    pub const ARG1_OFFSET: (f32, f32, f32) = (2.0, 16.0, 20.0);
    /// arg1 passes rot = NULL, no ROTY flag.
    pub const ARG1_FORCE_ROTY: bool = false;
}

/// Door sound frames (`aTRD_set_door_SE`): frame 2.0 → NA_SE_TRAINDOOR0,
/// frame 27.0 → NA_SE_TRAINDOOR1, via `cKF_FrameControl_passCheck_now`
/// with first-hit break.
pub const DOOR_SOUND_FRAMES: [f32; 2] = [2.0, 27.0];

/// Window scenery (`ac_train_window.c`).
pub mod window {
    /// SCENE_START_DEMO / SCENE_START_DEMO3: NoDraw, scrolls start at 0.
    pub const SCROLL_SPEED_INTRO: f32 = 0.07;
    /// SCENE_START_DEMO2: DrawGoneOutTunnel, scrolls start at 1000.
    pub const SCROLL_SPEED_DEMO2: f32 = 1.0;
    pub const SCROLL_START_DEMO2: i32 = 1000;
    /// Tree scroll advances +5 per movement tick.
    pub const TREE_SCROLL_STEP: i32 = 5;
    /// 15-entry (month, day) threshold table, not evenly spaced.
    pub const PALETTE_TILL: [(u8, u8); 15] = [
        (0x02, 0x03), (0x02, 0x11), (0x02, 0x18), (0x04, 0x03), (0x04, 0x08),
        (0x07, 0x16), (0x09, 0x0F), (0x0A, 0x05), (0x0A, 0x17), (0x0B, 0x07),
        (0x0B, 0x0E), (0x0B, 0x1C), (0x0C, 0x0A), (0x0C, 0x11), (0x0C, 0x1F),
    ];
}

/// Tree palette index (`aTrainWindow_GetTreePalletIdx`), verbatim:
/// first entry with `month < t[0]`, or `month == t[0] && day <= t[1]`;
/// 0 if none match.
pub fn tree_palette_idx(month: u8, day: u8) -> u8 {
    for (i, &(m, d)) in window::PALETTE_TILL.iter().enumerate() {
        if month < m {
            return i as u8;
        }
        if month == m && day <= d {
            return i as u8;
        }
    }
    0
}

/// Station variant selection (`ac_station.c`): `arg0 = type / 5`.
pub fn station_arg0(station_type: u8) -> u8 {
    station_type / 5
}

/// Porter talk entries: (message, talk continuation, camera).
/// Continuations from `include/ac_station_clip.h`
/// (CHK_LEAVE_TALK=0, SAYONARA=2, END_WAIT=15);
/// cameras from `include/m_camera2.h` (NORMAL=1, TALK=3).
pub mod porter {
    /// Forced-talk table (`aSTM_set_force_talk_info`).
    pub const FORCE_TALK: [(u16, u8, u8); 3] = [
        (0x07DD, 15, 1),
        (0x0966, 15, 1),
        (0x0965, 2, 3),
    ];
    /// Ordinary-talk table (`aSTM_set_norm_talk_info`).
    pub const NORM_TALK: [(u16, u8, u8); 4] = [
        (0x0831, 15, 3),
        (0x07ED, 15, 3),
        (0x0943, 0, 3),
        (0x095C, 0, 3),
    ];
}

/// Special NPC identities (`include/m_name_table.h`).
pub mod id {
    /// Rover.
    pub const GUIDE: u16 = 0xD004;
    /// Depart master (distinct from Porter).
    pub const DEPART_MASTER: u16 = 0xD00B;
    /// Porter, the station master.
    pub const STATION_MASTER: u16 = 0xD00E;
    /// Raccoon guide (Tom Nook), used by the opening intro.
    pub const RCN_GUIDE: u16 = 0xD014;
}

/// Intro demo constants (`ac_intro_demo_move.c_inc`).
pub mod intro {
    /// `train_coming_flag = 3` starts the demo arrival.
    pub const TRAIN_COMING_FLAG: u8 = 3;
    /// `mBGMPsComp_scene_mode(9)`.
    pub const BGM_SCENE_MODE: u8 = 9;
    /// Doorway collision set to stone at (2180, 0, 820).
    pub const DOORWAY_POS: (f32, f32, f32) = (2180.0, 0.0, 820.0);
    /// Player walks to (2220, 840) at speed 2.5.
    pub const WALK_TARGET: (f32, f32) = (2220.0, 840.0);
    pub const WALK_SPEED: f32 = 2.5;
}

// ---- C ABI ----
// All pc_-prefixed: the C originals stay compiled; these are staged kernels.

#[no_mangle]
pub extern "C" fn pc_train_depart_timer(hour: u8, min: u8, sec: u8) -> u32 {
    depart_timer(hour, min, sec)
}

#[no_mangle]
pub extern "C" fn pc_train_day_rollover(start_timer: u32) -> u32 {
    day_rollover(start_timer)
}

#[no_mangle]
pub extern "C" fn pc_train_tree_palette_idx(month: u8, day: u8) -> u8 {
    tree_palette_idx(month, day)
}

#[no_mangle]
pub extern "C" fn pc_train_station_arg0(station_type: u8) -> u8 {
    station_arg0(station_type)
}

#[no_mangle]
pub extern "C" fn pc_train1_anim_speed(action: u8) -> f32 {
    train1_anim(action).1
}

#[no_mangle]
pub extern "C" fn pc_train1_anim_end(action: u8) -> f32 {
    train1_anim(action).2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timetable() {
        // 10:00:00 -> first entry >= is 10:19:00, minus 4:10
        assert_eq!(depart_timer(10, 0, 0), 10 * 3600 + 19 * 60 - 250);
        // exactly at 10:19:00 -> that entry (>=), minus 4:10
        assert_eq!(depart_timer(10, 19, 0), 10 * 3600 + 19 * 60 - 250);
        // just after 10:19:00 -> next entry 11:19:00
        assert_eq!(depart_timer(10, 19, 1), 11 * 3600 + 19 * 60 - 250);
        // late night 23:59:59 -> 24:19:00 entry (rollover literal)
        assert_eq!(depart_timer(23, 59, 59), 24 * 3600 + 19 * 60 - 250);
        // midnight -> 0:19:00
        assert_eq!(depart_timer(0, 0, 0), 19 * 60 - 250);
    }

    #[test]
    fn rollover() {
        assert_eq!(day_rollover(86400), 0);
        assert_eq!(day_rollover(90000), 3600);
        assert_eq!(day_rollover(86399), 86399); // strict >= keeps it
    }

    #[test]
    fn anim_tables() {
        // action 5 -> seq 1 (close), speed 0, end 32.0
        assert_eq!(train1_anim(5), (1, 0.0, 32.0));
        // action 4 -> seq 0, speed 0.5 (close anim at half speed)
        assert_eq!(train1_anim(4), (0, 0.5, 24.0));
        assert_eq!(train1_anim(6), (1, 0.5, 32.0));
    }

    #[test]
    fn palette() {
        // Feb 3 -> entry 0 (month==2, day<=3)
        assert_eq!(tree_palette_idx(2, 3), 0);
        // Feb 4 -> entry 1
        assert_eq!(tree_palette_idx(2, 4), 1);
        // Jan -> first entry (month < 2)
        assert_eq!(tree_palette_idx(1, 15), 0);
        // Dec 31 -> entry 14 (0x0C, 0x1F)
        assert_eq!(tree_palette_idx(12, 31), 14);
        // Dec 10 -> entry 12
        assert_eq!(tree_palette_idx(12, 10), 12);
    }

    #[test]
    fn station_and_ids() {
        assert_eq!(station_arg0(0), 0);
        assert_eq!(station_arg0(7), 1);
        assert_eq!(id::GUIDE, 0xD004);
        assert_eq!(id::DEPART_MASTER, 0xD00B);
        assert_eq!(id::STATION_MASTER, 0xD00E);
        assert_eq!(id::RCN_GUIDE, 0xD014);
    }

    #[test]
    fn porter_tables() {
        assert_eq!(porter::FORCE_TALK[2], (0x0965, 2, 3)); // SAYONARA + TALK cam
        assert_eq!(porter::NORM_TALK[0].0, 0x0831);
        assert_eq!(DOOR_SOUND_FRAMES, [2.0, 27.0]);
    }
}
