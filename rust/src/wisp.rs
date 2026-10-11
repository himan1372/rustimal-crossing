//! Wisp the ghost — pure classification kernels.
//!
//! Verified against `src/actor/npc/event/ac_ev_ghost_talk.c_inc`,
//! `src/actor/npc/event/ac_ev_ghost_schedule.c_inc`,
//! `src/actor/npc/event/ac_ev_ghost.c`, `src/actor/ac_event_manager.c`,
//! `src/game/m_event.c`, `include/m_event.h`, `include/m_name_table.h`,
//! `include/ac_ev_ghost.h` (GAFE01_00 Rev 0 USA decomp).
//!
//! Architecture: the weekly scheduler (`init_weekly_event`), the event
//! manager (`ghost_start`/`ghost_stop`), Wisp's actor lifecycle and talk
//! state machines, spirit block tracking and spawn overrides, the RNG,
//! inventory mutation, and the reward selection (`aEGH_not_collect_get`)
//! all stay in C. This module holds only the source-proven pure kernels
//! with primitive arguments.
//!
//! Retail flow, for reference:
//! 1. `init_weekly_event` picks Wisp's date (`after_n_day(today, 2 +
//!    RANDOM(3))`); he becomes eligible when the saved date falls in
//!    [today−7, today].
//! 2. `ghost_start` places Wisp (`make_actor_in_free_block(..., 0x51, 5)`)
//!    and five spirit blocks (`bx = 1+RANDOM(5)`, `bz = 2+RANDOM(4)`,
//!    unique).
//! 3. Wisp is invisible (alpha 0) until found; the insect-spawn system
//!    is overridden in spirit blocks to spawn hitodama.
//! 4. Caught spirits stack-encode in the item id: ITM_SPIRIT0..4 hold
//!    1..5 spirits. `aEGH_hitodama_num` weights by stack size.
//! 5. At >= 5 spirits Wisp takes them (handover choreography in C) and
//!    offers three favors: clear weeds, paint roof, or an item.

/// Wisp constants.
pub mod wisp {
    /// First spirit item id (`ITM_INSECT_START + 40`).
    /// Spirits begin exactly where the exclusive-end insect range stops.
    pub const SPIRIT0: u16 = 0x2D28;
    /// Last spirit item id.
    pub const SPIRIT4: u16 = 0x2D2C;
    /// Max spirits per stack / number of spirits.
    pub const STACK_MAX: u8 = 5;
    pub const SPIRIT_NUM: u8 = 5;
    /// Wisp's special-NPC id (`SP_NPC_START + 111`).
    pub const NPC_ID: u16 = 0xD06F;
    /// Minimum grass for Wisp's actor to exist when not yet found.
    pub const MINIMUM_GRASS_COUNT: u8 = 8;
    /// Alpha when found vs hidden.
    pub const ALPHA_FOUND: u8 = 140;
    pub const ALPHA_HIDDEN: u8 = 0;
}

/// Ghost event flags (`include/m_event.h`).
pub mod flag {
    /// Spirits are active (shared event record).
    pub const ACTIVE: u16 = 0x4000;
    /// Player returned the spirits (per-event save).
    pub const RETURNED_SPIRITS: u16 = 0x8000;
    /// Spirit blocks already spawned (shared).
    pub const COMMON_SPAWNED_SPIRITS: u16 = 0x8000;
}

/// Weed-count thresholds → response messages (`aEGH_select_wait`,
/// "Clear weeds" branch). Strict `<` comparisons, verbatim.
const WEED_MSGS: [(u32, u16); 5] = [
    (50, 0x2EF1),
    (150, 0x2EF2),
    (450, 0x2EF3),
    (900, 0x2EF4),
    (u32::MAX, 0x2EF5),
];

/// Greeting message bases (`ac_ev_ghost_talk.c_inc`).
const GREET_NONE_BASE: u16 = 0x2EE7; // + RANDOM(5), roll stays in C
const GREET_SOME_BASE: u16 = 0x2EEB; // + hitodama_num (1..4)
const GREET_DONE: u16 = 0x2EF0; // >= 5 spirits

/// 4 AM timeout, strict `>` (`aEGH_time_over`).
const TIME_OVER_SEC: u32 = 4 * 3600;

/// `ITEM_IS_WISP`: spirit item range check.
pub fn item_is_spirit(item: u16) -> bool {
    (wisp::SPIRIT0..=wisp::SPIRIT4).contains(&item)
}

/// `WISP_COUNT`: spirits encoded in a stack item id (1..5), else 0.
pub fn stack_count(item: u16) -> u8 {
    if item_is_spirit(item) {
        (1 + item - wisp::SPIRIT0) as u8
    } else {
        0
    }
}

/// `aEGH_hitodama_num`: weighted spirit total from the five per-id
/// inventory counts. `counts[i]` = normal-condition count of
/// `ITM_SPIRIT0 + i`; each contributes `counts[i] * (i + 1)`.
pub fn spirit_count(counts: &[u8]) -> u32 {
    counts
        .iter()
        .take(wisp::SPIRIT_NUM as usize)
        .enumerate()
        .map(|(i, &n)| n as u32 * (i as u32 + 1))
        .sum()
}

/// Weed-favor response message for a weed count (strict thresholds).
pub fn weed_msg(weed_count: u32) -> u16 {
    WEED_MSGS
        .iter()
        .find(|&&(limit, _)| weed_count < limit)
        .map(|&(_, msg)| msg)
        .unwrap_or(0x2EF5)
}

/// Greeting message for a spirit total. `rand_roll` is the C-side
/// `RANDOM(5)` result, used only when `spirit_num == 0`.
pub fn greet_msg(spirit_num: u32, rand_roll: u8) -> u16 {
    if spirit_num == 0 {
        GREET_NONE_BASE + (rand_roll % 5) as u16
    } else if spirit_num >= wisp::SPIRIT_NUM as u32 {
        GREET_DONE
    } else {
        GREET_SOME_BASE + spirit_num as u16
    }
}

/// `aEGH_time_over`: strict `now_sec > 4 * 3600`.
pub fn time_over(now_sec: u32) -> bool {
    now_sec > TIME_OVER_SEC
}

/// Wisp visibility alpha from the found flag (`aEGH_actor_ct`).
pub fn found_alpha(found: bool) -> u8 {
    if found {
        wisp::ALPHA_FOUND
    } else {
        wisp::ALPHA_HIDDEN
    }
}

// ---- C ABI ----
// All pc_-prefixed: the C originals stay compiled; these are staged kernels.

#[no_mangle]
pub extern "C" fn pc_wisp_item_is_spirit(item: u16) -> u8 {
    item_is_spirit(item) as u8
}

#[no_mangle]
pub extern "C" fn pc_wisp_stack_count(item: u16) -> u8 {
    stack_count(item)
}

/// `counts` must point to 5 bytes (per-id spirit inventory counts).
#[no_mangle]
pub extern "C" fn pc_wisp_spirit_count(counts: *const u8) -> u32 {
    if counts.is_null() {
        return 0;
    }
    let slice = unsafe { core::slice::from_raw_parts(counts, wisp::SPIRIT_NUM as usize) };
    spirit_count(slice)
}

#[no_mangle]
pub extern "C" fn pc_wisp_weed_msg(weed_count: u32) -> u16 {
    weed_msg(weed_count)
}

#[no_mangle]
pub extern "C" fn pc_wisp_greet_msg(spirit_num: u32, rand_roll: u8) -> u16 {
    greet_msg(spirit_num, rand_roll)
}

#[no_mangle]
pub extern "C" fn pc_wisp_time_over(now_sec: u32) -> u8 {
    time_over(now_sec) as u8
}

#[no_mangle]
pub extern "C" fn pc_wisp_found_alpha(found: u8) -> u8 {
    found_alpha(found != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spirit_items() {
        assert!(item_is_spirit(0x2D28));
        assert!(item_is_spirit(0x2D2C));
        assert!(!item_is_spirit(0x2D27)); // last insect
        assert!(!item_is_spirit(0x2D2D));
        assert_eq!(stack_count(0x2D28), 1);
        assert_eq!(stack_count(0x2D2C), 5);
        assert_eq!(stack_count(0x2D00), 0);
    }

    #[test]
    fn weighted_count() {
        // one single + one 5-stack = 1 + 5 = 6
        assert_eq!(spirit_count(&[1, 0, 0, 0, 1]), 6);
        assert_eq!(spirit_count(&[0, 0, 0, 0, 0]), 0);
        assert_eq!(spirit_count(&[2, 1, 0, 0, 0]), 4); // 2*1 + 1*2
    }

    #[test]
    fn weed_thresholds() {
        assert_eq!(weed_msg(0), 0x2EF1);
        assert_eq!(weed_msg(49), 0x2EF1);
        assert_eq!(weed_msg(50), 0x2EF2); // strict <
        assert_eq!(weed_msg(149), 0x2EF2);
        assert_eq!(weed_msg(150), 0x2EF3);
        assert_eq!(weed_msg(449), 0x2EF3);
        assert_eq!(weed_msg(450), 0x2EF4);
        assert_eq!(weed_msg(899), 0x2EF4);
        assert_eq!(weed_msg(900), 0x2EF5);
        assert_eq!(weed_msg(5000), 0x2EF5);
    }

    #[test]
    fn greetings() {
        assert_eq!(greet_msg(0, 3), 0x2EEA);
        assert_eq!(greet_msg(1, 0), 0x2EEC);
        assert_eq!(greet_msg(4, 0), 0x2EEF);
        assert_eq!(greet_msg(5, 0), 0x2EF0);
        assert_eq!(greet_msg(9, 0), 0x2EF0);
    }

    #[test]
    fn timeout_and_alpha() {
        assert!(!time_over(4 * 3600)); // strict >
        assert!(time_over(4 * 3600 + 1));
        assert_eq!(found_alpha(true), 140);
        assert_eq!(found_alpha(false), 0);
    }
}
