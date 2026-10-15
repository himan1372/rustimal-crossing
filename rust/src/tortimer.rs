//! Tortimer the mayor (internal name SONCHO) — pure data and kernels.
//!
//! Verified against `src/game/m_event.c`, `src/actor/ac_event_manager.c`,
//! `src/actor/npc/event/ac_ev_soncho.c`,
//! `src/actor/npc/event/ac_ev_soncho_talk.c_inc`,
//! `src/actor/npc/event/ac_ev_soncho2.c`,
//! `src/actor/npc/event/ac_ev_soncho2_think.c_inc`,
//! `src/actor/npc/event/ac_ev_speech_soncho_talk.c_inc`,
//! `include/m_name_table.h`, `include/m_event.h`, `include/m_kankyo.h`,
//! `include/ac_groundhog_control.h` (GAFE01_00 Rev 0 USA decomp).
//! 10 of 11 brief claims checked verbatim; 1 headline correction:
//!
//! - The brief assigned the bridge dialogue backwards. The talk-info
//!   callback checks the first-talk bit while `aESC_talk_init` sets it
//!   (the identical pattern to Blanca's mask-cat actor), so the FIRST
//!   conversation shows the time-of-day greeting and SUBSEQUENT
//!   conversations show the proposal 0x2F3E. Likewise, declining on the
//!   first conversation plays the rotating follow-up; later declines
//!   use the secondary dialogue. A port following the brief would show
//!   the proposal on the wrong conversation.
//!
//! Architecture: the three actor paths (bridge proposal, event-day,
//! speech), the calendar/event scheduler, bridge candidate generation,
//! bridge acceptance writes, the NPC state machines, dialogue, and save
//! mutations all stay in C. This module holds only the source-proven
//! pure data and calculations with primitive arguments.

/// Tortimer identities (`include/m_name_table.h`; the `// Tortimer`
/// comment in `m_npc.c` proves the English name). All six share
/// `{ mPr_SEX_MALE, 0x4E3, 2 }` — but the identities are NOT
/// interchangeable: actor profiles and callers differ.
pub mod id {
    /// Bridge-event Tortimer.
    pub const EV_SONCHO: u16 = 0xD06E;
    /// Event-day Tortimer.
    pub const EV_SONCHO2: u16 = 0xD074;
    /// Alternate mayor identity.
    pub const SONCHO_D078: u16 = 0xD078;
    /// Alternate mayor identity.
    pub const SONCHO_D079: u16 = 0xD079;
    /// Mayor identity.
    pub const SONCHO: u16 = 0xD07F;
    /// Speech-oriented mayor actor.
    pub const EV_SPEECH_SONCHO: u16 = 0xD087;
    /// Bit in `spnpc_first_talk_flags` (`aNPC_SPNPC_BIT_EV_SONCHO`).
    pub const FIRST_TALK_BIT: u8 = 5;
}

/// Shared name-table metadata (`m_npc.c`).
pub mod meta {
    pub const NAME_STRING: u16 = 0x4E3;
    pub const PARAM: u8 = 2;
}

/// Weekday values (`include/lb_rtc.h`).
pub mod weekday {
    pub const SUNDAY: u8 = 0;
    pub const SATURDAY: u8 = 6;
}

/// Weather values (`include/m_kankyo.h`).
pub mod weather {
    pub const CLEAR: u8 = 0;
    pub const RAIN: u8 = 1;
    pub const SNOW: u8 = 2;
    pub const SAKURA: u8 = 3;
    pub const LEAVES: u8 = 4;
}

/// Bridge proposal data.
pub mod bridge {
    /// Villager count gate is `>= ANIMAL_NUM_MAX` (not `==`).
    pub const ANIMAL_NUM_MAX: u8 = 15;
    /// Proposal message on repeat conversations (corrected semantics).
    pub const PROPOSAL_MSG: u16 = 0x2F3E;
    /// Pending-bridge message base: `0x2F44 + RANDOM(3)` (roll in C).
    pub const PENDING_MSG_BASE: u16 = 0x2F44;
    /// Follow-up bases: high bit set → 0x2F47, else 0x2F39.
    pub const FOLLOWUP_HIGH: u16 = 0x2F47;
    pub const FOLLOWUP_LOW: u16 = 0x2F39;
    /// `msg_no` wraps to 0 at 5.
    pub const FOLLOWUP_MAX: u8 = 5;
    /// Tug-of-war timer.
    pub const TUG_TIMER: u16 = 0x12C;
}

/// Speech data.
pub mod speech {
    /// `mString_SPEECH_SONCHO_START`.
    pub const MSG_START: u16 = 0x3DB5;
    /// `aGHC_TERM_EVENT_TIME`.
    pub const GHC_TERM_EVENT_TIME: u8 = 5;
}

/// Bridge-event weekday gate (`init_weekly_event`): Saturday, or a
/// non-Sunday when the weekly date isn't today. Pure given the values;
/// the save reads stay in C.
pub fn bridge_day_eligible(weekday: u8, sched_date: u16, today_raw: u16) -> bool {
    weekday == weekday::SATURDAY
        || (weekday != weekday::SUNDAY && sched_date != today_raw)
}

/// Bridge candidate index (`bridge_man_start`): `remain = (spots - 1) - used`.
/// The high bit (`1 << 7`, top bit of the 7-bit `locations_used` field —
/// not the 1-bit `used_all_locations`) is set iff `remain == 0` (final
/// candidate in the cycle).
pub fn bridge_remain(spots: u8, used: u8) -> u8 {
    (spots - 1) - used
}

pub fn bridge_final_candidate(remain: u8) -> bool {
    remain == 0
}

/// Time-of-day greeting (`aESC_time_talk`), verbatim boundaries.
pub fn time_talk_msg(hour: u8) -> u16 {
    if hour < 6 || hour >= 23 {
        0x2F36
    } else if hour < 10 {
        0x2F33
    } else if hour < 17 {
        0x2F34
    } else {
        0x2F35
    }
}

/// Initial bridge message, CORRECTED semantics: the pending case uses
/// `0x2F44 + roll` (roll stays in C); with the first-talk bit already
/// set (repeat conversation) → proposal 0x2F3E; first conversation →
/// the time-of-day greeting. Foreign visitors always take the greeting
/// path (both bit helpers early-out for `mPr_FOREIGNER`).
pub fn initial_msg(bit_set: bool, hour: u8) -> u16 {
    if bit_set {
        bridge::PROPOSAL_MSG
    } else {
        time_talk_msg(hour)
    }
}

/// Pending-bridge message: `0x2F44 + RANDOM(3)` (roll stays in C).
pub fn pending_msg(rand3: u8) -> u16 {
    bridge::PENDING_MSG_BASE + (rand3 % 3) as u16
}

/// Rotating follow-up message (`aESC_owari_message`): base depends on
/// the bridge_flags high bit, plus the saved `msg_no`.
pub fn followup_msg(high_bit: bool, msg_no: u8) -> u16 {
    let base = if high_bit {
        bridge::FOLLOWUP_HIGH
    } else {
        bridge::FOLLOWUP_LOW
    };
    base + (msg_no % bridge::FOLLOWUP_MAX) as u16
}

/// `msg_no` increment with wrap to 0 at 5.
pub fn next_followup_msg(msg_no: u8) -> u8 {
    let next = msg_no + 1;
    if next >= bridge::FOLLOWUP_MAX {
        0
    } else {
        next
    }
}

/// Decline path, CORRECTED semantics: declining on the first
/// conversation (`first_talk == TRUE`) plays the rotating follow-up;
/// later declines use the secondary dialogue (`aESC_TALK_SECAND`).
pub fn decline_uses_followup(first_talk: bool) -> bool {
    first_talk
}

/// Speech message (`aESS_set_norm_talk_info`), verbatim. The Groundhog
/// Day pre-branch comes first: when the groundhog clip exists and
/// `now_term != aGHC_TERM_EVENT_TIME` → `START + 1 + roll`, regardless
/// of weather. `groundhog_active` is that compound condition, computed
/// by the C caller; `rand5` is the C-side `(int)RANDOM_F(5)` result.
pub fn speech_msg(groundhog_active: bool, weather: u8, rand5: u8) -> u16 {
    if groundhog_active {
        speech::MSG_START + 1 + (rand5 % 5) as u16
    } else {
        match weather {
            weather::CLEAR => speech::MSG_START + 6 + (rand5 % 5) as u16,
            weather::SNOW => speech::MSG_START + 11 + (rand5 % 5) as u16,
            _ => speech::MSG_START + 6,
        }
    }
}

/// Forced speech message: the base, `mDemo_TYPE_SPEECH`.
pub fn speech_force_msg() -> u16 {
    speech::MSG_START
}

// ---- C ABI ----
// All pc_-prefixed: the C originals stay compiled; these are staged kernels.

#[no_mangle]
pub extern "C" fn pc_tortimer_bridge_day_eligible(weekday: u8, sched_date: u16, today_raw: u16) -> u8 {
    bridge_day_eligible(weekday, sched_date, today_raw) as u8
}

#[no_mangle]
pub extern "C" fn pc_tortimer_bridge_remain(spots: u8, used: u8) -> u8 {
    bridge_remain(spots, used)
}

#[no_mangle]
pub extern "C" fn pc_tortimer_time_talk_msg(hour: u8) -> u16 {
    time_talk_msg(hour)
}

#[no_mangle]
pub extern "C" fn pc_tortimer_initial_msg(bit_set: u8, hour: u8) -> u16 {
    initial_msg(bit_set != 0, hour)
}

#[no_mangle]
pub extern "C" fn pc_tortimer_pending_msg(rand3: u8) -> u16 {
    pending_msg(rand3)
}

#[no_mangle]
pub extern "C" fn pc_tortimer_followup_msg(high_bit: u8, msg_no: u8) -> u16 {
    followup_msg(high_bit != 0, msg_no)
}

#[no_mangle]
pub extern "C" fn pc_tortimer_next_followup_msg(msg_no: u8) -> u8 {
    next_followup_msg(msg_no)
}

#[no_mangle]
pub extern "C" fn pc_tortimer_speech_msg(groundhog_active: u8, weather: u8, rand5: u8) -> u16 {
    speech_msg(groundhog_active != 0, weather, rand5)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identities() {
        assert_eq!(id::EV_SONCHO, 0xD06E);
        assert_eq!(id::EV_SONCHO2, 0xD074);
        assert_eq!(id::SONCHO_D078, 0xD078);
        assert_eq!(id::SONCHO_D079, 0xD079);
        assert_eq!(id::SONCHO, 0xD07F);
        assert_eq!(id::EV_SPEECH_SONCHO, 0xD087);
        assert_eq!(id::FIRST_TALK_BIT, 5);
    }

    #[test]
    fn eligibility_and_remain() {
        assert!(bridge_day_eligible(weekday::SATURDAY, 0x0101, 0x0101));
        assert!(bridge_day_eligible(3, 0x0101, 0x0102));
        assert!(!bridge_day_eligible(weekday::SUNDAY, 0x0101, 0x0102));
        assert!(!bridge_day_eligible(3, 0x0102, 0x0102));
        assert_eq!(bridge_remain(4, 1), 2);
        assert_eq!(bridge_remain(4, 3), 0);
        assert!(bridge_final_candidate(0));
        assert!(!bridge_final_candidate(2));
    }

    #[test]
    fn dialogue() {
        // time-of-day boundaries
        assert_eq!(time_talk_msg(5), 0x2F36);
        assert_eq!(time_talk_msg(23), 0x2F36);
        assert_eq!(time_talk_msg(6), 0x2F33);
        assert_eq!(time_talk_msg(9), 0x2F33);
        assert_eq!(time_talk_msg(10), 0x2F34);
        assert_eq!(time_talk_msg(16), 0x2F34);
        assert_eq!(time_talk_msg(17), 0x2F35);
        assert_eq!(time_talk_msg(22), 0x2F35);
        // corrected first/repeat assignment
        assert_eq!(initial_msg(false, 12), 0x2F34); // first -> greeting
        assert_eq!(initial_msg(true, 12), 0x2F3E); // repeat -> proposal
        assert_eq!(pending_msg(2), 0x2F46);
        assert_eq!(followup_msg(true, 3), 0x2F4A);
        assert_eq!(followup_msg(false, 0), 0x2F39);
        assert_eq!(next_followup_msg(4), 0);
        assert_eq!(next_followup_msg(2), 3);
        // decline: first conversation -> follow-up
        assert!(decline_uses_followup(true));
        assert!(!decline_uses_followup(false));
    }

    #[test]
    fn speech() {
        assert_eq!(speech_msg(true, weather::CLEAR, 3), 0x3DB5 + 1 + 3);
        assert_eq!(speech_msg(false, weather::CLEAR, 4), 0x3DB5 + 6 + 4);
        assert_eq!(speech_msg(false, weather::SNOW, 0), 0x3DB5 + 11);
        assert_eq!(speech_msg(false, weather::RAIN, 2), 0x3DB5 + 6);
        assert_eq!(speech_force_msg(), 0x3DB5);
    }

    #[test]
    fn constants_and_followup_math() {
        // Name-table metadata.
        assert_eq!(meta::NAME_STRING, 0x4E3);
        assert_eq!(meta::PARAM, 2);
        // Bridge data.
        assert_eq!(bridge::ANIMAL_NUM_MAX, 15);
        assert_eq!(bridge::TUG_TIMER, 0x12C);
        assert_eq!(bridge::FOLLOWUP_MAX, 5);
        assert_eq!(bridge::PENDING_MSG_BASE, 0x2F44);
        // Follow-up message wraps msg_no mod 5.
        assert_eq!(followup_msg(true, 7), 0x2F47 + 2);
        assert_eq!(followup_msg(false, 5), 0x2F39);
        assert_eq!(next_followup_msg(0), 1);
        assert_eq!(next_followup_msg(4), 0);
        // Speech: SAKURA/LEAVES take the default arm (+6, no roll).
        assert_eq!(speech_msg(false, weather::SAKURA, 3), 0x3DB5 + 6);
        assert_eq!(speech_msg(false, weather::LEAVES, 0), 0x3DB5 + 6);
        // Groundhog branch ignores weather entirely.
        assert_eq!(speech_msg(true, weather::SNOW, 4), 0x3DB5 + 1 + 4);
        // Bridge-remain math: spots-1-used.
        assert_eq!(bridge_remain(1, 0), 0);
        assert!(bridge_final_candidate(bridge_remain(1, 0)));
    }

    #[test]
    fn bridge_day_edges() {
        // Saturday is always eligible, even on the scheduled date.
        assert!(bridge_day_eligible(weekday::SATURDAY, 0x1234, 0x1234));
        // Sunday is never eligible.
        assert!(!bridge_day_eligible(weekday::SUNDAY, 0x0000, 0xFFFF));
        // A non-Sunday with no scheduled date (0) and a different today.
        assert!(bridge_day_eligible(5, 0x0000, 0x0101));
    }

}
