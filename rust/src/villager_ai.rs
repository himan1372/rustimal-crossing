//! Villager core-AI pure kernels: the daily schedule tables, the schedule
//! manager's saved/current/forced update, and the talk-cooldown temper table.
//!
//! Verified against `src/game/m_npc_schedule.c`,
//! `include/m_npc_schedule_h.h`, `src/game/m_npc.c` (talk-info section),
//! `include/m_npc_personal_id.h` (GAFE01_00 Rev 0 USA).
//!
//! These are the stateless kernels the research brief recommends porting
//! first. The stateful flows stay in C: the schedule-slot bookkeeping in
//! `Common` (`mNPS_get/set/reset_schedule_area` — partly in `npc_walk.rs`),
//! the per-villager schedule records, walk-record assignment and the
//! `ac_set_npc_manager.c` actor placement/travel state machine.
//!
//! Schedule tables: one per personality (`looks` 0-5), each a list of
//! (schedule type, end time in seconds since midnight). The manager picks the
//! FIRST entry whose end time is STRICTLY greater than now. The strict `>`
//! matters at the exact boundary second.

/// Personality ids (`mNpc_LOOKS_*`).
pub const LOOKS_GIRL: i32 = 0; // 'normal'
pub const LOOKS_KO_GIRL: i32 = 1; // 'peppy'
pub const LOOKS_BOY: i32 = 2; // 'lazy'
pub const LOOKS_SPORT_MAN: i32 = 3; // 'jock'
pub const LOOKS_GRIM_MAN: i32 = 4; // 'cranky'
pub const LOOKS_NANIWA_LADY: i32 = 5; // 'snooty'
pub const LOOKS_NUM: i32 = 6;

/// Scheduled states (`mNPS_SCHED_*`).
pub const SCHED_FIELD: i32 = 0;
pub const SCHED_IN_HOUSE: i32 = 1;
pub const SCHED_SLEEP: i32 = 2;
pub const SCHED_STAND: i32 = 3;
pub const SCHED_WANDER: i32 = 4;
pub const SCHED_WALK_WANDER: i32 = 5;
pub const SCHED_SPECIAL: i32 = 6;

const fn hms(h: i32, m: i32, s: i32) -> i32 {
    h * 3600 + m * 60 + s
}

/// (schedule type, end time) entries, verbatim from `m_npc_schedule.c`.
/// Index = `LOOKS_*`.
pub const SCHEDULE_TABLES: [&[(i32, i32)]; 6] = [
    &[
        (SCHED_SLEEP, hms(5, 0, 0)),
        (SCHED_IN_HOUSE, hms(6, 0, 0)),
        (SCHED_FIELD, hms(12, 0, 0)),
        (SCHED_IN_HOUSE, hms(13, 0, 0)),
        (SCHED_FIELD, hms(18, 30, 0)),
        (SCHED_IN_HOUSE, hms(21, 0, 0)),
        (SCHED_SLEEP, hms(24, 0, 0)),
    ],
    &[
        (SCHED_SLEEP, hms(7, 0, 0)),
        (SCHED_IN_HOUSE, hms(8, 0, 0)),
        (SCHED_FIELD, hms(13, 0, 0)),
        (SCHED_IN_HOUSE, hms(14, 0, 0)),
        (SCHED_FIELD, hms(22, 0, 0)),
        (SCHED_IN_HOUSE, hms(23, 30, 0)),
        (SCHED_SLEEP, hms(24, 0, 0)),
    ],
    &[
        (SCHED_SLEEP, hms(8, 0, 0)),
        (SCHED_IN_HOUSE, hms(9, 0, 0)),
        (SCHED_FIELD, hms(12, 0, 0)),
        (SCHED_IN_HOUSE, hms(14, 0, 0)),
        (SCHED_FIELD, hms(19, 30, 0)),
        (SCHED_IN_HOUSE, hms(22, 0, 0)),
        (SCHED_SLEEP, hms(24, 0, 0)),
    ],
    &[
        (SCHED_IN_HOUSE, hms(1, 0, 0)),
        (SCHED_SLEEP, hms(5, 30, 0)),
        (SCHED_IN_HOUSE, hms(6, 30, 0)),
        (SCHED_FIELD, hms(12, 0, 0)),
        (SCHED_IN_HOUSE, hms(12, 30, 0)),
        (SCHED_FIELD, hms(23, 0, 0)),
        (SCHED_IN_HOUSE, hms(24, 0, 0)),
    ],
    &[
        (SCHED_FIELD, hms(4, 0, 0)),
        (SCHED_IN_HOUSE, hms(5, 0, 0)),
        (SCHED_SLEEP, hms(10, 0, 0)),
        (SCHED_IN_HOUSE, hms(11, 0, 0)),
        (SCHED_FIELD, hms(15, 0, 0)),
        (SCHED_IN_HOUSE, hms(16, 0, 0)),
        (SCHED_FIELD, hms(22, 0, 0)),
        (SCHED_IN_HOUSE, hms(23, 0, 0)),
        (SCHED_FIELD, hms(24, 0, 0)),
    ],
    &[
        (SCHED_FIELD, hms(1, 30, 0)),
        (SCHED_IN_HOUSE, hms(2, 30, 0)),
        (SCHED_SLEEP, hms(9, 0, 0)),
        (SCHED_IN_HOUSE, hms(10, 0, 0)),
        (SCHED_FIELD, hms(13, 0, 0)),
        (SCHED_IN_HOUSE, hms(14, 0, 0)),
        (SCHED_FIELD, hms(21, 0, 0)),
        (SCHED_IN_HOUSE, hms(22, 0, 0)),
        (SCHED_FIELD, hms(24, 0, 0)),
    ],
];

/// `saved_type`: the first table entry whose end time is strictly greater
/// than `now_sec` (`mNPS_schedule_manager_sub`).
#[no_mangle]
pub extern "C" fn pc_npc_schedule_saved(looks: i32, now_sec: i32) -> i32 {
    let table = SCHEDULE_TABLES[(looks.clamp(0, LOOKS_NUM - 1)) as usize];
    for &(sched_type, end_time) in table {
        if end_time > now_sec {
            return sched_type;
        }
    }
    table[table.len() - 1].0
}

/// One `mNPS_schedule_manager_sub` pass. `saved_type` is always recomputed
/// from the table; while `forced_timer > 0` the current state is the forced
/// one and the timer decrements by `ticks` (clamped at 0), otherwise the
/// current state follows the saved state. Returns the current state and
/// writes the new timer through `out_timer` (may be null).
#[no_mangle]
pub extern "C" fn pc_npc_schedule_step(
    looks: i32,
    now_sec: i32,
    forced_type: i32,
    forced_timer: i32,
    ticks: i32,
    out_timer: *mut i32,
) -> i32 {
    let saved = pc_npc_schedule_saved(looks, now_sec);
    let (current, timer) = if forced_timer > 0 {
        (forced_type, (forced_timer - ticks).max(0))
    } else {
        (saved, 0)
    };
    if !out_timer.is_null() {
        unsafe {
            *out_timer = timer;
        }
    }
    current
}

// ---------------------------------------------------------------------------
// Talk cooldown (`m_npc.c`: `l_npc_talk_info`, `l_npc_temper`)
// ---------------------------------------------------------------------------

/// Temper row: (unlock timer, impatient threshold, max counted talks).
/// Declared as `l_npc_temper[mNpc_FEEL_NUM]` but indexed by `looks`
/// (personality) in `mNpc_CountTalkNum` / `mNpc_CheckOverImpatient` /
/// `mNpc_SetUnlockTimer` — preserve the looks indexing, do not "correct" it
/// to a mood index.
pub const TEMPER: [(u16, u8, u8); 6] = [
    (4000, 12, 15), // FEEL_NORMAL row, used for LOOKS_GIRL
    (3000, 10, 13), // FEEL_HAPPY row, used for LOOKS_KO_GIRL
    (4000, 12, 15), // FEEL_ANGRY row, used for LOOKS_BOY
    (4000, 10, 13), // FEEL_SAD row, used for LOOKS_SPORT_MAN
    (5000, 9, 12), // FEEL_SLEEPY row, used for LOOKS_GRIM_MAN
    (5000, 9, 12), // FEEL_PITFALL row, used for LOOKS_NANIWA_LADY
];

/// `mNpc_TimerCountDown`: `timer > 0` → `ticks >= timer ? 0 : timer - ticks`.
#[no_mangle]
pub extern "C" fn pc_npc_timer_countdown(timer: u16, ticks: u16) -> u16 {
    if timer > 0 {
        if ticks >= timer {
            0
        } else {
            timer - ticks
        }
    } else {
        timer
    }
}

/// `mNpc_CountTalkNum` gate: increments only while `talk_num < max` (the
/// looks-indexed temper row) and `timer > 0`. Returns the new talk count.
#[no_mangle]
pub extern "C" fn pc_npc_count_talk(talk_num: u8, timer: u16, looks: i32) -> u8 {
    let max = TEMPER[(looks.clamp(0, LOOKS_NUM - 1)) as usize].2;
    if talk_num < max && timer > 0 {
        talk_num + 1
    } else {
        talk_num
    }
}

/// `mNpc_CheckOverImpatient`: `talk_num >=` the looks-indexed threshold.
#[no_mangle]
pub extern "C" fn pc_npc_over_impatient(talk_num: u8, looks: i32) -> bool {
    talk_num >= TEMPER[(looks.clamp(0, LOOKS_NUM - 1)) as usize].1
}

/// `mNpc_SetUnlockTimer`: both `unlock_timer` and `reset_timer` are set to
/// the looks-indexed row's unlock timer.
#[no_mangle]
pub extern "C" fn pc_npc_unlock_timer_value(looks: i32) -> u16 {
    TEMPER[(looks.clamp(0, LOOKS_NUM - 1)) as usize].0
}

/// `mNpc_TalkEndMove` core: the talk timer is set to 1000 by the caller;
/// the talk count only increments when `pc_npc_count_talk` allows it; the
/// unlock timer starts only when the count incremented AND the impatience
/// threshold is reached. Returns the new talk count and writes whether the
/// unlock timer should start through `out_start_unlock` (may be null).
#[no_mangle]
pub extern "C" fn pc_npc_talk_end(
    talk_num: u8,
    timer: u16,
    looks: i32,
    out_start_unlock: *mut bool,
) -> u8 {
    let counted = pc_npc_count_talk(talk_num, timer, looks);
    let incremented = counted != talk_num;
    let start_unlock = incremented && pc_npc_over_impatient(counted, looks);
    if !out_start_unlock.is_null() {
        unsafe {
            *out_start_unlock = start_unlock;
        }
    }
    counted
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sec(h: i32, m: i32) -> i32 {
        h * 3600 + m * 60
    }

    #[test]
    fn schedule_tables() {
        // Strict `end_time > now_sec`: at the exact boundary the NEXT entry wins.
        assert_eq!(pc_npc_schedule_saved(LOOKS_GIRL, sec(4, 59)), SCHED_SLEEP);
        assert_eq!(pc_npc_schedule_saved(LOOKS_GIRL, sec(5, 0)), SCHED_IN_HOUSE);
        assert_eq!(pc_npc_schedule_saved(LOOKS_GIRL, sec(12, 0)), SCHED_IN_HOUSE);
        assert_eq!(pc_npc_schedule_saved(LOOKS_GIRL, sec(18, 30)), SCHED_IN_HOUSE);
        assert_eq!(pc_npc_schedule_saved(LOOKS_GIRL, sec(21, 0)), SCHED_SLEEP);
        // Jock starts the day in-house, sleeps 05:30.
        assert_eq!(pc_npc_schedule_saved(LOOKS_SPORT_MAN, sec(0, 30)), SCHED_IN_HOUSE);
        assert_eq!(pc_npc_schedule_saved(LOOKS_SPORT_MAN, sec(5, 30)), SCHED_IN_HOUSE);
        assert_eq!(pc_npc_schedule_saved(LOOKS_SPORT_MAN, sec(5, 29)), SCHED_SLEEP);
        // Cranky is a night owl: field at 03:00 and at 23:30.
        assert_eq!(pc_npc_schedule_saved(LOOKS_GRIM_MAN, sec(3, 0)), SCHED_FIELD);
        assert_eq!(pc_npc_schedule_saved(LOOKS_GRIM_MAN, sec(23, 30)), SCHED_FIELD);
        assert_eq!(pc_npc_schedule_saved(LOOKS_GRIM_MAN, sec(10, 30)), SCHED_IN_HOUSE);
        // Snooty: field at 01:00, house at 02:00.
        assert_eq!(pc_npc_schedule_saved(LOOKS_NANIWA_LADY, sec(1, 0)), SCHED_FIELD);
        assert_eq!(pc_npc_schedule_saved(LOOKS_NANIWA_LADY, sec(2, 0)), SCHED_IN_HOUSE);
        assert_eq!(SCHEDULE_TABLES.len(), 6);
    }

    #[test]
    fn schedule_forced() {
        let mut timer = -1;
        // Forced state wins while the timer is positive; the timer decrements.
        assert_eq!(
            pc_npc_schedule_step(LOOKS_GIRL, sec(10, 0), SCHED_FIELD, 30, 1, &mut timer),
            SCHED_FIELD
        );
        assert_eq!(timer, 29);
        // Timer clamps at zero instead of going negative.
        assert_eq!(
            pc_npc_schedule_step(LOOKS_GIRL, sec(10, 0), SCHED_FIELD, 1, 5, &mut timer),
            SCHED_FIELD
        );
        assert_eq!(timer, 0);
        // No forced timer → the saved table state.
        assert_eq!(
            pc_npc_schedule_step(LOOKS_GIRL, sec(10, 0), SCHED_FIELD, 0, 1, &mut timer),
            SCHED_FIELD
        );
        assert_eq!(timer, 0);
        assert_eq!(
            pc_npc_schedule_step(LOOKS_GIRL, sec(22, 0), SCHED_FIELD, 0, 1, &mut timer),
            SCHED_SLEEP
        );
        // Null out-pointer is safe.
        assert_eq!(
            pc_npc_schedule_step(LOOKS_GIRL, sec(10, 0), SCHED_FIELD, 30, 1, std::ptr::null_mut()),
            SCHED_FIELD
        );
    }

    #[test]
    fn talk_cooldown() {
        // Countdown: saturates at zero.
        assert_eq!(pc_npc_timer_countdown(100, 30), 70);
        assert_eq!(pc_npc_timer_countdown(30, 30), 0);
        assert_eq!(pc_npc_timer_countdown(30, 50), 0);
        assert_eq!(pc_npc_timer_countdown(0, 50), 0);
        // Count gate: talk_num < max (looks-indexed row) and timer > 0.
        assert_eq!(pc_npc_count_talk(14, 100, LOOKS_GIRL), 15);
        assert_eq!(pc_npc_count_talk(15, 100, LOOKS_GIRL), 15); // at max
        assert_eq!(pc_npc_count_talk(14, 0, LOOKS_GIRL), 14); // timer dead
        assert_eq!(pc_npc_count_talk(12, 100, LOOKS_KO_GIRL), 13);
        assert_eq!(pc_npc_count_talk(13, 100, LOOKS_KO_GIRL), 13); // peppy max 13
        // Impatience thresholds come from the same looks-indexed row.
        assert!(pc_npc_over_impatient(12, LOOKS_GIRL));
        assert!(!pc_npc_over_impatient(11, LOOKS_GIRL));
        assert!(pc_npc_over_impatient(10, LOOKS_KO_GIRL));
        assert!(pc_npc_over_impatient(9, LOOKS_GRIM_MAN));
        // Unlock timer values.
        assert_eq!(pc_npc_unlock_timer_value(LOOKS_GIRL), 4000);
        assert_eq!(pc_npc_unlock_timer_value(LOOKS_KO_GIRL), 3000);
        assert_eq!(pc_npc_unlock_timer_value(LOOKS_GRIM_MAN), 5000);
        // Talk-end: timer is set to 1000 by the caller; the count only
        // increments under the gate, and the unlock starts only when the
        // count incremented AND hit the impatience threshold.
        let mut start = false;
        assert_eq!(pc_npc_talk_end(11, 100, LOOKS_GIRL, &mut start), 12);
        assert!(start);
        assert_eq!(pc_npc_talk_end(10, 100, LOOKS_GIRL, &mut start), 11);
        assert!(!start);
        assert_eq!(pc_npc_talk_end(15, 100, LOOKS_GIRL, &mut start), 15); // no increment
        assert!(!start);
        assert_eq!(pc_npc_talk_end(11, 0, LOOKS_GIRL, &mut start), 11); // timer dead
        assert!(!start);
    }
}
