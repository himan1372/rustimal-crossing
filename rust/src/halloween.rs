//! The Halloween special NPC (`ac_halloween_npc.c`).
//!
//! Verified against `src/actor/npc/ac_halloween_npc.c`,
//! `include/ac_halloween_npc.h`,
//! `src/actor/npc/ac_halloween_npc_talk.c_inc`,
//! `src/actor/npc/ac_halloween_npc_schedule.c_inc`,
//! `src/actor/ac_event_manager.c`, `src/game/m_event_map_npc.c`,
//! `src/game/m_event_schedule.c_inc`, `include/m_name_table.h`
//! (GAFE01_00 Rev 0 USA). All 10 brief claims PROVEN.
//!
//! This is NOT Jack: Jack is the separate Pumpkin event actor
//! (`ac_ev_pumpkin.c`). This actor is the special Halloween NPC
//! layered on the ordinary NPC engine, doing trick-or-treat
//! interactions. Jack's actor is research-only for now — its
//! similar trick logic does not imply an identical state
//! machine, so no shared helper is attempted here.
//!
//! Scope split: the Halloween EVENT schedule (Oct 31 18:00 →
//! Nov 1 00:00) and the dynamic-NPC naming base are already
//! ported in the sibling `event_day` module. This module owns
//! the actor's pure kernels: the trick-type candidate scan, the
//! talk and think states, the forced-talk trigger, the message
//! ID expressions, the tool ranges, and the event-manager spawn
//! constants. The talk state machine (message windows, submenu
//! lifetime, demo order values, handover actors), the RNG calls
//! (`RANDOM_F` / `RANDOM` ordering), the schedule callbacks, and
//! the save stay in C.

/// Number of pocket slots scanned by `aHWN_get_trick_type`.
pub const POCKETS_SLOT_COUNT: usize = 15;

pub mod name {
    /// From `m_name_table.h`: NAME_TYPE enum (0 = ITEM0, ...).
    pub const TYPE_FTR0: u8 = 1;
    pub const TYPE_ITEM1: u8 = 2;
    pub const TYPE_FTR1: u8 = 3;
    pub const ITEM1_CAT_TOOL: u8 = 2;
}

/// The two furniture items excluded from the item trick, from
/// the `X(...)` furniture enum (`m_name_table.h:876+`, which
/// starts at 0).
pub mod ftr {
    /// Jack-o'-lantern.
    pub const SUM_HAL_PKIN: u16 = 295;
    /// Jack-in-the-box.
    pub const SUM_HAL_BOX01: u16 = 305;
}

/// The trick outcome (`include/ac_halloween_npc.h:53-59`).
pub mod trick {
    pub const CHG_CLOTH: u8 = 0;
    pub const CHG_ITEM: u8 = 1;
    pub const NUM: u8 = 2;
}

/// One pocket entry, reduced to what the candidate scan reads:
/// the item's name type, ITEM1 category, quest-condition bit,
/// and furniture index (for the Halloween exclusions).
pub struct TrickPocket {
    pub item_type: u8,
    pub cat: u8,
    pub is_quest: bool,
    pub ftr_idx: u16,
}

/// The pure candidate scan of `aHWN_get_trick_type`, verbatim:
/// NAME_TYPE_FTR0/FTR1 eligible if not quest and not the two
/// excluded Halloween furniture; NAME_TYPE_ITEM1 eligible if
/// not ITEM1_CAT_TOOL and not quest. Scans slots 0..15 in order
/// and returns the eligible slot indices (order preserved).
pub fn scan_trick_candidates(pockets: &[TrickPocket; POCKETS_SLOT_COUNT]) -> (usize, [u8; POCKETS_SLOT_COUNT]) {
    let mut idx = [0u8; POCKETS_SLOT_COUNT];
    let mut count = 0usize;
    for (i, p) in pockets.iter().enumerate() {
        let eligible = match p.item_type {
            name::TYPE_FTR0 | name::TYPE_FTR1 => {
                !p.is_quest
                    && p.ftr_idx != ftr::SUM_HAL_PKIN
                    && p.ftr_idx != ftr::SUM_HAL_BOX01
            }
            name::TYPE_ITEM1 => p.cat != name::ITEM1_CAT_TOOL && !p.is_quest,
            _ => false,
        };
        if eligible {
            idx[count] = i as u8;
            count += 1;
        }
    }
    (count, idx)
}

/// The pure trick decision: `count > 0 && random_f < 0.5f` →
/// the item trick (the C then selects the Halloween2 furniture
/// and `idx[RANDOM(count)]` — RNG and item selection stay in C
/// to preserve call order), otherwise the clothing trick.
pub fn trick_decision(count: usize, random_f: f32) -> u8 {
    if count > 0 && random_f < 0.5 {
        trick::CHG_ITEM
    } else {
        trick::CHG_CLOTH
    }
}

/// The 14 talk states (`include/ac_halloween_npc.h:29-46`).
/// The state table maps END_WAIT to `none_proc1`.
pub mod talk {
    pub const FIRST_CALL_TALK: u8 = 0;
    pub const TRICK_OR_TREAT_TALK: u8 = 1;
    pub const MENU_OPEN_WAIT_TALK: u8 = 2;
    pub const MENU_CLOSE_WAIT_TALK: u8 = 3;
    pub const GET_OTHER_ITEM_WAIT_TALK: u8 = 4;
    pub const RECEIVE_TOOL_ITEM_START_WAIT_TALK: u8 = 5;
    pub const RECEIVE_TOOL_ITEM_END_WAIT_TALK: u8 = 6;
    pub const TRICK_TIMING_WAIT_TALK: u8 = 7;
    pub const TRICK_CHG_CLOTH_TALK: u8 = 8;
    pub const TRICK_CHG_CLOTH_END_WAIT_TALK: u8 = 9;
    pub const GET_AME_WAIT_TALK: u8 = 10;
    pub const GET_AME_TALK: u8 = 11;
    pub const PL_DEMO_CODE_END_WAIT_TALK: u8 = 12;
    pub const END_WAIT: u8 = 13;
    pub const NUM: u8 = 14;
}

/// The think states (approach / approach-wait), not named in
/// the brief prose.
pub mod think {
    pub const APPROACH: u8 = 0;
    pub const APPROACH_WAIT: u8 = 1;
    pub const NUM: u8 = 2;
}

/// The forced-talk trigger (`ac_halloween_npc_talk.c_inc:253`),
/// verbatim: horizontal distance STRICTLY < 70.0f, absolute
/// vertical STRICTLY < 20.0f, and the NPC's block must match the
/// player's block. (The source zeroes the local block coords
/// when the world position can't be converted to a block.)
pub fn force_talk(
    dist_h: f32,
    dist_v: f32,
    npc_block: (i32, i32),
    player_block: (i32, i32),
) -> bool {
    dist_h < 70.0 && dist_v.abs() < 20.0 && npc_block == player_block
}

/// Message IDs and looks-scaled expressions
/// (`m_msg_enum.h`: MSG_2442 = 2442, MSG_2479 = 2479; the
/// looks value comes from `mNpc_GetLooks(event npc_id)`).
/// Note: the restart IDs get their `looks * 6` scaling INSIDE
/// `aHWN_restart_msg_win`, not at the call sites.
pub mod msg {
    pub const FIRST_GREETING: u16 = 2442;
    pub const NORMAL_TALK_BASE: u16 = 2479;

    /// First-call continuing message: `0x098B + looks * 6`.
    pub fn first_call_continue(looks: u8) -> u16 {
        0x098B + looks as u16 * 6
    }
    /// Clothing-trick completion / trick animation end: `0x098D + looks * 6`.
    pub fn trick_cloth_done(looks: u8) -> u16 {
        0x098D + looks as u16 * 6
    }
    /// Item-selection restarts (after `aHWN_restart_msg_win` applies looks): 0x098C (empty), 0x098E (ordinary), 0x098F (candy), 0x0990 (tool).
    pub fn restart(looks: u8, base: u16) -> u16 {
        base + looks as u16 * 6
    }
    /// Normal talk request: `MSG_2479 + looks`.
    pub fn normal_talk(looks: u8) -> u16 {
        NORMAL_TALK_BASE + looks as u16
    }
}

/// The tool item ranges and handover constants
/// (`m_name_table.h`).
pub mod items {
    pub const EMPTY_NO: u16 = 0x0000;
    pub const NET: u16 = 0x2200;
    pub const ROD: u16 = 0x2203;
    pub const GOLDEN_NET: u16 = 0x2239;
    pub const GOLDEN_ROD: u16 = 0x223C;
    pub const CANDY: u16 = 0x2806;
    /// The clothing the Halloween trick puts on the player.
    pub const TRICK_CLOTH: u16 = 0x2411;

    /// `(item >= ITM_NET && item <= ITM_ROD) ||
    ///  (item >= ITM_GOLDEN_NET && item <= ITM_GOLDEN_ROD)`.
    pub fn is_tool_range(item: u16) -> bool {
        (item >= NET && item <= ROD) || (item >= GOLDEN_NET && item <= GOLDEN_ROD)
    }
}

/// Event-manager spawn and registration
/// (`src/actor/ac_event_manager.c`, `src/game/m_event_map_npc.c`).
pub mod spawn {
    /// `SP_NPC_START` = 0xD000.
    pub const SP_NPC_START: u16 = 0xD000;
    /// `SP_NPC_HALLOWEEN` = SP_NPC_START + 37.
    pub const HALLOWEEN_NPC: u16 = 0xD025;
    /// `SP_NPC_EV_HALLOWEEN_0` = SP_NPC_START + 32.
    pub const EV_HALLOWEEN_0: u16 = 0xD020;
    /// `SP_NPC_EV_HALLOWEEN_4` = SP_NPC_START + 36.
    pub const EV_HALLOWEEN_4: u16 = 0xD024;
    /// The event parameter used by make_actor_in_free_block /
    /// show_actor_at_wade / walk_actor_at_wade.
    pub const EVENT_PARAM: u8 = 0x51;

    /// `mEvMN_GetHalloweenNpcName`: `event_name =
    /// SP_NPC_EV_HALLOWEEN_0 + free_idx` (the five event NPC
    /// slots are a contiguous block D020–D024).
    pub fn event_npc_name(free_idx: u8) -> u16 {
        EV_HALLOWEEN_0 + free_idx as u16
    }
}

/// Schedule model constants (`ac_halloween_npc_schedule.c_inc`).
pub mod schedule {
    /// The step stops incrementing at 2.
    pub const MAX_STEP: u8 = 2;
    /// Both run and wait requests use priority 4.
    pub const REQUEST_PRIORITY: u8 = 4;
    /// The approach distance passed as `args[5] = 70` in `aHWN_set_request_act`.
    pub const REQUEST_ARG_DISTANCE: u8 = 70;
    /// The 70.0f forced-talk horizontal threshold (also used for the approach request).
    pub const FORCE_TALK_DISTANCE: f32 = 70.0;
    /// The 20.0f forced-talk vertical threshold.
    pub const FORCE_TALK_HEIGHT: f32 = 20.0;
}

// ---- C ABI ----
// All pc_-prefixed: the C originals stay compiled; these are staged kernels.

/// Fills `out` with the eligible trick-candidate slot indices
/// and returns the count. Input arrays must have 15 entries each.
#[no_mangle]
pub extern "C" fn pc_hwn_trick_candidates(
    item_types: *const u8,
    cats: *const u8,
    quest_flags: *const u8,
    ftr_idxs: *const u16,
    out: *mut u8,
) -> u8 {
    if item_types.is_null() || cats.is_null() || quest_flags.is_null() || ftr_idxs.is_null() || out.is_null() {
        return 0;
    }
    let (types, cats, quests, ftrs, out) = unsafe {
        (
            std::slice::from_raw_parts(item_types, POCKETS_SLOT_COUNT),
            std::slice::from_raw_parts(cats, POCKETS_SLOT_COUNT),
            std::slice::from_raw_parts(quest_flags, POCKETS_SLOT_COUNT),
            std::slice::from_raw_parts(ftr_idxs, POCKETS_SLOT_COUNT),
            std::slice::from_raw_parts_mut(out, POCKETS_SLOT_COUNT),
        )
    };
    let mut pockets = [TrickPocket { item_type: 0, cat: 0, is_quest: false, ftr_idx: 0 }; POCKETS_SLOT_COUNT];
    for i in 0..POCKETS_SLOT_COUNT {
        pockets[i] = TrickPocket {
            item_type: types[i],
            cat: cats[i],
            is_quest: quests[i] != 0,
            ftr_idx: ftrs[i],
        };
    }
    let (count, idx) = scan_trick_candidates(&pockets);
    out[..count].copy_from_slice(&idx[..count]);
    count as u8
}

#[no_mangle]
pub extern "C" fn pc_hwn_trick_decision(count: u8, random_f: f32) -> u8 {
    trick_decision(count as usize, random_f)
}

#[no_mangle]
pub extern "C" fn pc_hwn_force_talk(
    dist_h: f32,
    dist_v: f32,
    npc_bx: i32,
    npc_bz: i32,
    pl_bx: i32,
    pl_bz: i32,
) -> u8 {
    force_talk(dist_h, dist_v, (npc_bx, npc_bz), (pl_bx, pl_bz)) as u8
}

#[no_mangle]
pub extern "C" fn pc_hwn_is_tool_range(item: u16) -> u8 {
    items::is_tool_range(item) as u8
}

#[no_mangle]
pub extern "C" fn pc_hwn_event_npc_name(free_idx: u8) -> u16 {
    spawn::event_npc_name(free_idx)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pocket(item_type: u8, cat: u8, quest: bool, ftr: u16) -> TrickPocket {
        TrickPocket { item_type, cat, is_quest: quest, ftr_idx: ftr }
    }

    fn empty() -> [TrickPocket; 15] {
        [TrickPocket { item_type: 0, cat: 0, is_quest: false, ftr_idx: 0 }; 15]
    }

    #[test]
    fn candidate_rules() {
        // ordinary furniture: eligible
        let mut p = empty();
        p[0] = pocket(name::TYPE_FTR0, 0, false, 100);
        p[1] = pocket(name::TYPE_FTR1, 0, false, 200);
        // Jack-o'-lantern and Jack-in-the-box: excluded
        p[2] = pocket(name::TYPE_FTR0, 0, false, ftr::SUM_HAL_PKIN);
        p[3] = pocket(name::TYPE_FTR1, 0, false, ftr::SUM_HAL_BOX01);
        // quest furniture: excluded
        p[4] = pocket(name::TYPE_FTR0, 0, true, 100);
        // ordinary non-tool ITEM1: eligible
        p[5] = pocket(name::TYPE_ITEM1, 0, false, 0);
        // tool ITEM1: excluded
        p[6] = pocket(name::TYPE_ITEM1, name::ITEM1_CAT_TOOL, false, 0);
        // quest ITEM1: excluded
        p[7] = pocket(name::TYPE_ITEM1, 0, true, 0);
        // ITEM0: never eligible
        p[8] = pocket(0, 0, false, 0);
        let (count, idx) = scan_trick_candidates(&p);
        assert_eq!(count, 3);
        assert_eq!(&idx[..3], &[0u8, 1, 5]);
    }

    #[test]
    fn trick_branch() {
        assert_eq!(trick_decision(0, 0.1), trick::CHG_CLOTH);
        assert_eq!(trick_decision(3, 0.1), trick::CHG_ITEM);
        assert_eq!(trick_decision(3, 0.5), trick::CHG_CLOTH);
        assert_eq!(trick_decision(3, 0.99), trick::CHG_CLOTH);
    }

    #[test]
    fn talk_states() {
        assert_eq!(talk::FIRST_CALL_TALK, 0);
        assert_eq!(talk::END_WAIT, 13);
        assert_eq!(talk::NUM, 14);
        assert_eq!(think::NUM, 2);
    }

    #[test]
    fn forced_talk_bounds() {
        // strict < : exactly 70.0 or 20.0 does NOT trigger
        assert!(force_talk(69.9, 19.9, (1, 1), (1, 1)));
        assert!(!force_talk(70.0, 19.9, (1, 1), (1, 1)));
        assert!(!force_talk(69.9, 20.0, (1, 1), (1, 1)));
        assert!(!force_talk(69.9, 19.9, (1, 1), (1, 2)));
    }

    #[test]
    fn messages() {
        assert_eq!(msg::FIRST_GREETING, 2442);
        assert_eq!(msg::normal_talk(3), 2482);
        assert_eq!(msg::first_call_continue(1), 0x098B + 6);
        assert_eq!(msg::trick_cloth_done(2), 0x098D + 12);
        assert_eq!(msg::restart(1, 0x098F), 0x098F + 6);
    }

    #[test]
    fn tools() {
        assert!(items::is_tool_range(items::NET));
        assert!(items::is_tool_range(items::ROD));
        assert!(items::is_tool_range(items::GOLDEN_NET));
        assert!(items::is_tool_range(items::GOLDEN_ROD));
        assert!(!items::is_tool_range(items::ROD + 1));
        assert!(!items::is_tool_range(items::CANDY));
        assert_eq!(items::TRICK_CLOTH, 0x2411);
        assert_eq!(items::CANDY, 0x2806);
    }

    #[test]
    fn spawn_ids() {
        assert_eq!(spawn::HALLOWEEN_NPC, 0xD025);
        assert_eq!(spawn::event_npc_name(0), 0xD020);
        assert_eq!(spawn::event_npc_name(4), 0xD024);
        assert_eq!(spawn::EVENT_PARAM, 0x51);
    }
}
