//! The Harvest Festival ambient NPCs (`ac_harvest_npc0.c`, `ac_harvest_npc1.c`).
//!
//! Verified against `src/actor/npc/ac_harvest_npc0.c`,
//! `src/actor/npc/ac_harvest_npc0.c_inc`,
//! `src/actor/npc/ac_harvest_npc1.c`,
//! `src/actor/npc/ac_harvest_npc1.c_inc`,
//! `src/game/m_event_map_npc_data.c_inc`,
//! `src/game/m_event_schedule.c_inc`, `src/actor/ac_event_manager.c`,
//! `src/actor/npc/event/ac_ev_turkey.c`
//! (GAFE01_00 Rev 0 USA). All 12 brief claims PROVEN.
//!
//! Scope split: the HARVEST_FESTIVAL schedule row and
//! Franklin's present-mask selection logic are already ported
//! in the sibling `event_day` module. This module owns the
//! two ambient NPCs' pure kernels: the action selections, the
//! default-angle tables, the message arithmetic, and the
//! nine-slot shrine layout. Franklin himself (save/common
//! area, talk state machine), the tumbler tool actor, the
//! event-manager handlers, and RNG calls stay in C.
//!
//! Nuances carried: NPC0's `_9A4` holds the RAW event index —
//! the fifth NPC (raw 4) has even parity but angle index 0;
//! keep the two distinct. NPC1's think has NO action-selection
//! branch when `_998 == -1` (unlike NPC0) — don't add one.
//! The real `first_angle` table lives inside
//! `aHT0_GetDefaultAngle` in the `.c`; the `.c_inc`'s
//! `def_angle` copy is a leftover. The map array is named
//! `l_hf_map`, not `l_hf_pule1_map`.

/// NPC0: the seated participant (wait / merry / drink / to-default).
pub mod npc0 {
    pub const ACT_WAIT: u8 = 0;
    pub const ACT_MERRY: u8 = 1;
    pub const ACT_DRINK: u8 = 2;
    pub const ACT_TO_DEFAULT: u8 = 3;
    pub const ACT_NUM: u8 = 4;

    pub const ACTIONS: [u8; 4] = [ACT_WAIT, ACT_MERRY, ACT_DRINK, ACT_TO_DEFAULT];

    /// The loop tables (`aHT0_setupAction`).
    pub const LOOP_BASE: [i32; 4] = [1, 2, 1, 1];
    pub const LOOP_RND: [f32; 4] = [0.0, 3.0, 1.0, 0.0];

    /// Clap sounds: NA_SE_2F, NA_SE_31, NA_SE_32, NA_SE_33,
    /// rolled when `index == 1` (MERRY) only.
    pub const CLAP_SOUNDS: [&str; 4] = ["NA_SE_2F", "NA_SE_31", "NA_SE_32", "NA_SE_33"];

    /// `SP_NPC_EV_HARVEST_0` = `SP_NPC_START + 130`.
    pub const EV_HARVEST_0: u16 = 0xD082;

    /// `aHT0_GetMyIdx`: the masked angle index.
    pub fn angle_idx(raw_idx: u8) -> u8 {
        raw_idx & 3
    }

    /// `first_angle[]` in int short-angle units
    /// (`DEG2SHORT_ANGLE2`): {45, 270, 315, 90}.
    pub fn default_angle(raw_idx: u8) -> i32 {
        let idx = angle_idx(raw_idx) as usize;
        [(45.0f32, 8192), (270.0, 49152), (315.0, 57344), (90.0, 16384)][idx].1
    }

    /// The pure think selection: `_998 != -1` → `action[_998]`
    /// and clear; otherwise `temp = RANDOM(2); temp <<=
    /// (_9A4 & 1)` → `action[temp]`. RNG stays in C — this
    /// preserves the stored-vs-shift branching. Returns the
    /// next action and whether the stored slot clears.
    pub fn select_action(stored: i8, random2: u8, raw_idx: u8) -> (u8, bool) {
        if stored >= 0 {
            (ACTIONS[stored as usize % ACTIONS.len()], true)
        } else {
            let idx = ((random2 as u8) << (raw_idx & 1)) as usize;
            (ACTIONS[idx % ACTIONS.len()], false)
        }
    }

    /// Talk message bases, indexed by `mNpc_GetNpcLooks`.
    pub const MSG_BASES: [u16; 6] = [15512, 15543, 15419, 15450, 15481, 15574];

    /// `msg_base[looks] + _9A4 * 3 + RANDOM(3)`.
    pub fn talk_msg(looks: usize, raw_idx: u8, random3: u16) -> u16 {
        MSG_BASES[looks % MSG_BASES.len()] + raw_idx as u16 * 3 + random3
    }

    /// The tumbler is attempted only for odd raw indices with a
    /// NULL right-hand item actor (the C then creates
    /// TOOL_TUMBLER and assigns the pointer only if non-NULL).
    pub fn should_make_tumbler(raw_idx: u8, right_hand_null: bool) -> bool {
        (raw_idx & 1) == 1 && right_hand_null
    }
}

/// NPC1: the simpler participant (wait / to-default).
pub mod npc1 {
    pub const ACT_WAIT: u8 = 0;
    pub const ACT_TO_DEFAULT: u8 = 1;
    pub const ACT_NUM: u8 = 2;

    /// The default angle is the constant zero.
    pub const DEFAULT_ANGLE: i32 = 0;

    /// The creation-time position adjustment: a literal
    /// `world.position.x += 20.f`.
    pub const CREATION_X_OFFSET: f32 = 20.0;

    /// The pure think: `_998 != -1` → use it as the next action
    /// and clear it. Otherwise NPC1 selects NOTHING — it only
    /// re-applies flags (unlike NPC0). Returns the next action
    /// and whether the stored slot clears, or None.
    pub fn think(stored: i8) -> Option<(u8, bool)> {
        if stored >= 0 {
            Some((stored as u8, true))
        } else {
            None
        }
    }

    /// `aHT1_to_default`: when the shape rotation matches the
    /// zero default angle, queue WAIT (`_998 = 0`) and end the
    /// action. Returns the queued action or None.
    pub fn to_default(shape_angle: i32) -> Option<u8> {
        if shape_angle == DEFAULT_ANGLE {
            Some(ACT_WAIT)
        } else {
            None
        }
    }

    /// Talk message bases in decimals (`0x3CA4..0x3CE2`).
    pub const MSG_BASES: [u16; 6] = [15524, 15555, 15431, 15462, 15493, 15586];

    /// `RANDOM(3) + msg_base[looks]` — no event-index multiplier.
    pub fn talk_msg(looks: usize, random3: u16) -> u16 {
        MSG_BASES[looks % MSG_BASES.len()] + random3
    }
}

/// The festival layout (`m_event_map_npc_data.c_inc`).
/// Nine slots on a shrine block: the five harvest NPCs,
/// Tortimer, and the three tables. A single map variant.
pub mod layout {
    pub const SLOT_NUM: usize = 9;
    pub const NPC_MASK: u8 = 0b011111;
    /// Record fields: `{ 0, 5, 1, 9, ..., mRF_BLOCKKIND_SHRINE }`.
    pub const NORMAL_NPC_MAX: u8 = 5;
    pub const MAP_MAX: u8 = 1;
    pub const NPC_MAX: u8 = 9;

    /// `l_hf_map` positions in field-unit X/Z.
    pub const SLOTS: [(u8, u8); 9] = [
        (10, 6),  // SP_NPC_EV_HARVEST_0
        (12, 8),  // SP_NPC_EV_HARVEST_1
        (5, 4),   // SP_NPC_EV_HARVEST_2
        (3, 6),   // SP_NPC_EV_HARVEST_3
        (4, 12),  // SP_NPC_EV_HARVEST_4
        (9, 10),  // SP_NPC_EV_SONCHO2
        (4, 5),   // HTABLE0
        (11, 7),  // HTABLE1
        (5, 10),  // HTABLE2
    ];

    pub fn slot_pos(slot: usize) -> Option<(u8, u8)> {
        SLOTS.get(slot).copied()
    }
}

/// Actor and object IDs.
pub mod ids {
    /// `SP_NPC_EV_HARVEST_0..4` = `SP_NPC_START + 130..+134`.
    pub const EV_HARVEST_0: u16 = 0xD082;
    pub const EV_HARVEST_4: u16 = 0xD086;
    /// `SP_NPC_EV_SONCHO2`.
    pub const EV_SONCHO2: u16 = 0xD074;
    /// The harvest tables.
    pub const HTABLE0: u16 = 0x5846;
    pub const HTABLE1: u16 = 0x5847;
    pub const HTABLE2: u16 = 0x5848;
    /// `SP_NPC_TURKEY` (Franklin).
    pub const TURKEY: u16 = 0xD08D;
}

// ---- C ABI ----
// All pc_-prefixed: the C originals stay compiled; these are staged kernels.

/// Returns the next NPC0 action; `clears` (out) reports whether the stored slot resets.
#[no_mangle]
pub extern "C" fn pc_hv_npc0_next(stored: i8, random2: u8, raw_idx: u8, clears: *mut u8) -> u8 {
    let (act, clear) = npc0::select_action(stored, random2, raw_idx);
    if !clears.is_null() {
        unsafe { *clears = clear as u8 };
    }
    act
}

#[no_mangle]
pub extern "C" fn pc_hv_npc0_msg(looks: u8, raw_idx: u8, random3: u16) -> u16 {
    npc0::talk_msg(looks as usize, raw_idx, random3)
}

#[no_mangle]
pub extern "C" fn pc_hv_npc0_default_angle(raw_idx: u8) -> i32 {
    npc0::default_angle(raw_idx)
}

#[no_mangle]
pub extern "C" fn pc_hv_npc0_make_tumbler(raw_idx: u8, right_hand_null: u8) -> u8 {
    npc0::should_make_tumbler(raw_idx, right_hand_null != 0) as u8
}

/// Returns 1 and the next action in `out` when `_998 != -1`; otherwise 0.
#[no_mangle]
pub extern "C" fn pc_hv_npc1_think(stored: i8, out: *mut u8) -> u8 {
    match npc1::think(stored) {
        Some((act, _)) if !out.is_null() => {
            unsafe { *out = act };
            1
        }
        _ => 0,
    }
}

/// Returns 1 when the default angle is reached (queues WAIT); otherwise 0.
#[no_mangle]
pub extern "C" fn pc_hv_npc1_to_default(shape_angle: i32) -> u8 {
    npc1::to_default(shape_angle).is_some() as u8
}

#[no_mangle]
pub extern "C" fn pc_hv_npc1_msg(looks: u8, random3: u16) -> u16 {
    npc1::talk_msg(looks as usize, random3)
}

/// Writes the (x, z) unit coordinates of a layout slot; returns 1 on success.
#[no_mangle]
pub extern "C" fn pc_hv_layout_pos(slot: u8, out: *mut u8) -> u8 {
    match layout::slot_pos(slot as usize) {
        Some((x, z)) if !out.is_null() => {
            let out = unsafe { std::slice::from_raw_parts_mut(out, 2) };
            out[0] = x;
            out[1] = z;
            1
        }
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn npc0_think() {
        // stored slot wins and clears
        assert_eq!(npc0::select_action(2, 0, 0), (npc0::ACT_DRINK, true));
        assert_eq!(npc0::select_action(0, 0, 1), (npc0::ACT_WAIT, true));
        // even raw index: WAIT or MERRY
        assert_eq!(npc0::select_action(-1, 0, 0), (npc0::ACT_WAIT, false));
        assert_eq!(npc0::select_action(-1, 1, 0), (npc0::ACT_MERRY, false));
        // odd raw index: WAIT or DRINK
        assert_eq!(npc0::select_action(-1, 0, 1), (npc0::ACT_WAIT, false));
        assert_eq!(npc0::select_action(-1, 1, 1), (npc0::ACT_DRINK, false));
        // the fifth NPC (raw 4) behaves as even, despite angle index 0
        assert_eq!(npc0::angle_idx(4), 0);
        assert_eq!(npc0::select_action(-1, 1, 4), (npc0::ACT_MERRY, false));
    }

    #[test]
    fn npc0_angles() {
        assert_eq!(npc0::default_angle(0), 8192);    // 45°
        assert_eq!(npc0::default_angle(1), 49152);   // 270°
        assert_eq!(npc0::default_angle(2), 57344);   // 315°
        assert_eq!(npc0::default_angle(3), 16384);   // 90°
        assert_eq!(npc0::default_angle(4), 8192);    // raw 4 wraps to angle index 0
        assert_eq!(npc0::LOOP_BASE, [1, 2, 1, 1]);
        assert_eq!(npc0::LOOP_RND, [0.0, 3.0, 1.0, 0.0]);
    }

    #[test]
    fn npc0_msgs() {
        assert_eq!(npc0::talk_msg(0, 0, 2), 15514);
        assert_eq!(npc0::talk_msg(0, 2, 0), 15518); // + raw_idx * 3
        assert_eq!(npc0::talk_msg(5, 4, 1), 15587);
        assert!(npc0::should_make_tumbler(1, true));
        assert!(!npc0::should_make_tumbler(0, true));
        assert!(!npc0::should_make_tumbler(3, false));
    }

    #[test]
    fn npc1_think() {
        // stored slot wins; otherwise NO selection (unlike NPC0)
        assert_eq!(npc1::think(0), Some((0, true)));
        assert_eq!(npc1::think(1), Some((1, true)));
        assert_eq!(npc1::think(-1), None);
        assert_eq!(npc1::to_default(0), Some(npc1::ACT_WAIT));
        assert_eq!(npc1::to_default(100), None);
        assert_eq!(npc1::DEFAULT_ANGLE, 0);
        assert_eq!(npc1::CREATION_X_OFFSET, 20.0);
    }

    #[test]
    fn npc1_msgs() {
        assert_eq!(npc1::talk_msg(0, 2), 15526);
        assert_eq!(npc1::talk_msg(5, 0), 15586);
    }

    #[test]
    fn festival_layout() {
        assert_eq!(layout::SLOT_NUM, 9);
        assert_eq!(layout::NPC_MASK, 0b011111);
        assert_eq!(layout::MAP_MAX, 1);
        assert_eq!(layout::NORMAL_NPC_MAX, 5);
        assert_eq!(layout::slot_pos(0), Some((10, 6)));
        assert_eq!(layout::slot_pos(5), Some((9, 10)));  // SONCHO2
        assert_eq!(layout::slot_pos(6), Some((4, 5)));   // HTABLE0
        assert_eq!(layout::slot_pos(8), Some((5, 10)));  // HTABLE2
        assert_eq!(layout::slot_pos(9), None);
        assert_eq!(ids::EV_HARVEST_0, 0xD082);
        assert_eq!(ids::EV_HARVEST_4, 0xD086);
        assert_eq!(ids::EV_SONCHO2, 0xD074);
        assert_eq!(ids::HTABLE0, 0x5846);
        assert_eq!(ids::HTABLE2, 0x5848);
        assert_eq!(ids::TURKEY, 0xD08D);
    }
}
