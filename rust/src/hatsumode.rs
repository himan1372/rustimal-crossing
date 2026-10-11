//! The First Shrine Visit actors (`ac_hatumode_control.c`,
//! `ac_hatumode_npc0.c`).
//!
//! Verified against `src/actor/ac_hatumode_control.c`,
//! `ac_hatumode_control_move.c_inc`,
//! `src/actor/npc/ac_hatumode_npc0.c`,
//! `ac_hatumode_npc0_talk.c_inc`,
//! `ac_hatumode_npc0_schedule.c_inc`,
//! `src/actor/ac_hatumode_clip_move.c_inc`,
//! `src/actor/ac_hatumode_clip_think.c_inc`,
//! `src/actor/ac_shrine.c`, `src/game/m_event_map_npc_data.c_inc`,
//! `src/game/m_event_schedule.c_inc`, `src/actor/ac_event_manager.c`,
//! `include/ac_hatumode_control.h`, `include/ac_hatumode_npc0.h`,
//! `include/m_name_table.h` (GAFE01_00 Rev 0 USA). All 10 brief
//! claims PROVEN, with brief-missed nuances folded in.
//!
//! Scope split: the NEW_YEARS_DAY Jan 1 06:00–10:00 schedule row
//! already lives in the sibling `event_day` module; this module
//! adds the SONCHO_NEW_YEARS_DAY row and the Dec 25–31 rumor row
//! there. This module owns the controller/NPC pure kernels: the
//! 9-state offering/prayer dispatch constants, the shrine-relative
//! position math, the save-area layout, the dialogue group
//! formulas, the start-offset and route tables, and the flag
//! handshake masks. Actor lifecycle, the NPC clip, message/demo
//! ownership, RNG calls, player-action requests, shrine
//! animation, the global clip, save-area allocation, and the
//! `mSP_money_check`/`mSP_get_sell_price` semantics stay in C.
//!
//! Nuances carried: `yashiro_pos[3]` is stored [X, Z, Y], NOT
//! [X, Y, Z] — `[0]`→X, `[1]`→Z, `[2]`→Y; the controller home is
//! (20+yp[0], yp[2], 40+yp[1]) while prayer/offering use
//! (20+yp[0], yp[2], 85+yp[1]); `aHN0_make_msg` uses a bitwise
//! `& 3`, not `% 4`; the route wrap pre-increments (`root++`,
//! then 10→5, 5→0) — not plain modulo; `root_table`/`proc_idx_table`
//! have only 4 entries but 5 HATUMODE NPCs spawn, so index 4 is an
//! open retail question (the helper reports invalid rather than
//! reproducing an out-of-bounds read); the flag masks are consumed
//! on read; think 28's 0x4000 detour goes to think 43 (0x2B);
//! `aHTC_inori_end` clears `flags0 & ~0x8000` (brief-missed).

/// First event-NPC ID of the contiguous range
/// (`m_name_table.h`).
pub const SP_NPC_EV_HATUMODE_0: u16 = 0xD058;
/// Number of HATUMODE event NPCs in the spawn table.
pub const HATUMODE_NPC_COUNT: usize = 5;
/// Shrine-maiden special NPC ID (`m_name_table.h`).
pub const SP_NPC_EV_MIKO: u16 = 0xD03D;
/// New Year table structure actor ID (`m_name_table.h`).
pub const NEWYEAR_TABLE: u16 = 0x5831;

/// Save-area layout mirror (`aEv_hatumode_save_c`,
/// `include/ac_hatumode_control.h`): `{ u16 state; u16 flags0;
/// u16 flags1; s16 yashiro_pos[3]; u8 cur_npc_idx; }`, retrieved
/// via `mEv_get_save_area(1, 7)`. The area is C-owned; this is
/// documentation for the kernels below.
pub const HATUMODE_SAVE_EVENT: u8 = 1;
pub const HATUMODE_SAVE_SLOT: u8 = 7;

/// Controller dialogue: the 50-Bell offering prompt
/// (`aHTC_offer`).
pub const MSG_OFFER_ASK: i32 = 4396;
/// Controller dialogue: the no-money path (`aHTC_offer`).
pub const MSG_NO_MONEY: i32 = 4398;
/// Dialogue continuation after accepting the offering: 0x112D.
pub const MSG_CONT_YES: i32 = 0x112D;
/// Dialogue continuation after declining: 0x112E.
pub const MSG_CONT_NO: i32 = 0x112E;
/// The offering price in Bells.
pub const OFFER_PRICE: i32 = 50;
/// `flags0` bit the NPC flow sets to release the controller's
/// wait state (`aHTC_wait`).
pub const FLAG_OFFER_READY: u16 = 0x8000;
/// `flags0` bit consumed by think 28's detour to think 43.
pub const FLAG_DETOUR_43: u16 = 0x4000;
/// Controller home offsets from the saved shrine position
/// (`aHTC_ready`): x = 20+yp[0], z = 40+yp[1], y = yp[2].
pub const CTRL_HOME_DX: i32 = 20;
pub const CTRL_HOME_DZ: i32 = 40;
/// Prayer/offering target offsets (`aHTC_inori`,
/// `aHTC_saisen`): x = 20+yp[0], y = yp[2], z = 85+yp[1].
pub const ACT_POS_DX: i32 = 20;
pub const ACT_POS_DZ: i32 = 85;
/// `DEG2SHORT_ANGLE2(180.0f)` — the NPC start orientation and the
/// prayer/offering facing (`aHN0_set_start_position`).
pub const ANGLE_180: i32 = 32768;

/// Selects the controller's offering message: 4396 when the
/// player has the Bells, 4398 otherwise. The money check runs only
/// after the speech demo is active (`aHTC_offer`).
#[no_mangle]
pub extern "C" fn pc_hatsumode_offer_msg(has_money: bool) -> i32 {
    if has_money {
        MSG_OFFER_ASK
    } else {
        MSG_NO_MONEY
    }
}

/// Selects the dialogue continuation and next controller state
/// for the offering choice: choice 0 → continuation 0x112D and
/// state 7 (pay); anything else → 0x112E and state 4
/// (`aHTC_select`). Returns the continuation message ID; the
/// state follows the same branch.
#[no_mangle]
pub extern "C" fn pc_hatsumode_choice_continue(choice0: bool) -> i32 {
    if choice0 {
        MSG_CONT_YES
    } else {
        MSG_CONT_NO
    }
}

/// Returns the controller state entered after the offering
/// choice: 7 (pay) for choice 0, 4 (no-money/alternate)
/// otherwise.
#[no_mangle]
pub extern "C" fn pc_hatsumode_choice_state(choice0: bool) -> i32 {
    if choice0 { 7 } else { 4 }
}

/// Returns true when the controller may leave its wait state:
/// `flags0 & 0x8000` is set (`aHTC_wait`).
#[no_mangle]
pub extern "C" fn pc_hatsumode_wait_ready(flags0: u16) -> bool {
    flags0 & FLAG_OFFER_READY != 0
}

/// Clears the offer-ready bit at prayer end
/// (`aHTC_inori_end`: `flags0 &= ~0x8000`).
#[no_mangle]
pub extern "C" fn pc_hatsumode_inori_end_flags(flags0: u16) -> u16 {
    flags0 & !FLAG_OFFER_READY
}

/// Writes the controller's home position from the saved shrine
/// coordinates to `out[0..2]` as (x, z) and returns y. Layout is
/// [X, Z, Y]: x = 20+yp[0], z = 40+yp[1], y = yp[2]
/// (`aHTC_ready`).
#[no_mangle]
pub extern "C" fn pc_hatsumode_ctrl_home(yp0: i32, yp1: i32, yp2: i32, out: *mut i32) -> i32 {
    if !out.is_null() {
        let out = unsafe { std::slice::from_raw_parts_mut(out, 2) };
        out[0] = CTRL_HOME_DX + yp0;
        out[1] = CTRL_HOME_DZ + yp1;
    }
    yp2
}

/// Writes the prayer/offering target position from the saved
/// shrine coordinates to `out[0..2]` as (x, z) and returns y:
/// x = 20+yp[0], y = yp[2], z = 85+yp[1] (`aHTC_inori`,
/// `aHTC_saisen`).
#[no_mangle]
pub extern "C" fn pc_hatsumode_act_pos(yp0: i32, yp1: i32, yp2: i32, out: *mut i32) -> i32 {
    if !out.is_null() {
        let out = unsafe { std::slice::from_raw_parts_mut(out, 2) };
        out[0] = ACT_POS_DX + yp0;
        out[1] = ACT_POS_DZ + yp1;
    }
    yp2
}

/// NPC dialogue bases per looks index
/// (`ac_hatumode_npc0.c`: `base_msg_table`).
pub const NPC_MSG_BASE: [i32; 6] = [7679, 7697, 7661, 7715, 7733, 7751];
/// Per-group offsets added to the base in `aHN0_make_msg`,
/// indexed by the group value 0–3: {+12, +3, +6, +9}.
pub const MSG_GROUP_OFFSET: [i32; 4] = [12, 3, 6, 9];
/// `change_msg == 1` offset: `base_msg + 15 + RANDOM(3)`.
pub const MSG_CHANGE1_OFFSET: i32 = 15;
/// Offering-talk continuations: choice 0 → base+1 with
/// next_think 0x22; otherwise base+2 (`aHN0_talk_saisen_suru`).
pub const SAISEN_NEXT_THINK_YES: i32 = 0x22;

/// Computes the `aHN0_make_msg` group index:
/// `(4 + (npc_id - SP_NPC_EV_HATUMODE_0) - cur_npc_idx) & 3`.
/// Bitwise AND, not modulo (`ac_hatumode_clip_move.c_inc`).
#[no_mangle]
pub extern "C" fn pc_hatsumode_msg_group(npc_id: u16, cur_npc_idx: u8) -> i32 {
    let rel = npc_id.wrapping_sub(SP_NPC_EV_HATUMODE_0) as i32;
    (4 + rel - cur_npc_idx as i32) & 3
}

/// Computes the final `aHN0_make_msg` message:
/// `base + group_offset[group] + rand3`. The `rand3` value is
/// supplied by the C caller so RNG order stays in C.
#[no_mangle]
pub extern "C" fn pc_hatsumode_make_msg(base: i32, group: i32, rand3: i32) -> i32 {
    let g = (group & 3) as usize;
    base + MSG_GROUP_OFFSET[g] + rand3
}

/// Computes the `aHTC_clip_talk_info` message for the simple
/// branches: `change_msg` 0 → base; 1 → base + 15 + rand3.
/// (`change_msg` 2 routes through `aHN0_make_msg` instead.)
#[no_mangle]
pub extern "C" fn pc_hatsumode_talk_msg(base: i32, change: i32, rand3: i32) -> i32 {
    match change {
        1 => base + MSG_CHANGE1_OFFSET + rand3,
        _ => base,
    }
}

/// Selects the offering-talk continuation message: choice 0 →
/// base+1 (with next_think 0x22), otherwise base+2.
#[no_mangle]
pub extern "C" fn pc_hatsumode_saisen_continue(base: i32, choice0: bool) -> i32 {
    if choice0 {
        base + 1
    } else {
        base + 2
    }
}

/// NPC start-position offsets added to the actor's world X/Z, per
/// NPC index 0–4 (`aHTMD_clip_set_start`).
pub const START_OFFSET_X: [i16; 5] = [20, 40, -40, 40, -40];
pub const START_OFFSET_Z: [i16; 5] = [-10, 40, 40, 0, 0];
/// Start route roots per NPC index 0–4.
pub const START_ROOT: [u8; 5] = [0, 4, 9, 3, 8];
/// `aHN0_ready2` tables: route roots and think indices for
/// additional NPCs. Only 4 entries exist in the source; index 4
/// is an open retail question (see module docs).
pub const READY2_ROOT: [u8; 4] = [5, 4, 9, 3];
pub const READY2_PROC_IDX: [u8; 4] = [4, 27, 27, 20];

/// Writes the start X/Z offsets for an NPC index to
/// `out[0..2]`; returns 1 on success, 0 for an invalid index.
/// The offsets are ADDED to the actor's world position; the NPC
/// is then oriented to 180 degrees (`aHN0_set_start_position`).
#[no_mangle]
pub extern "C" fn pc_hatsumode_start_offset(npc_idx: u8, out: *mut i16) -> u8 {
    if out.is_null() {
        return 0;
    }
    let i = npc_idx as usize;
    if i >= START_OFFSET_X.len() {
        return 0;
    }
    let out = unsafe { std::slice::from_raw_parts_mut(out, 2) };
    out[0] = START_OFFSET_X[i];
    out[1] = START_OFFSET_Z[i];
    1
}

/// Returns the start route root for an NPC index 0–4, or -1 when
/// invalid.
#[no_mangle]
pub extern "C" fn pc_hatsumode_start_root(npc_idx: u8) -> i32 {
    let i = npc_idx as usize;
    if i >= START_ROOT.len() {
        return -1;
    }
    START_ROOT[i] as i32
}

/// Returns the `aHN0_ready2` route root and think index for an
/// additional-NPC index via `out[0..2]`; returns 1 on success.
/// The source tables have only 4 entries while 5 NPCs can spawn,
/// so index 4 reports invalid instead of reproducing a retail
/// out-of-bounds read.
#[no_mangle]
pub extern "C" fn pc_hatsumode_ready2(npc_idx: u8, out: *mut u8) -> u8 {
    if out.is_null() {
        return 0;
    }
    let i = npc_idx as usize;
    if i >= READY2_ROOT.len() {
        return 0;
    }
    let out = unsafe { std::slice::from_raw_parts_mut(out, 2) };
    out[0] = READY2_ROOT[i];
    out[1] = READY2_PROC_IDX[i];
    1
}

/// Route destination tables (`aHTMD_next_position`).
pub const ROUTE_POS_X: [i16; 9] = [20, 0, 40, 0, 40, -60, -60, 100, 100];
pub const ROUTE_POS_Z: [i16; 9] = [75, 160, 160, 200, 200, 75, 200, 75, 200];
/// Route-index remap table: 10 entries, indexed by the wrapped
/// root value.
pub const ROUTE_ROOT: [u8; 10] = [0, 5, 6, 3, 1, 0, 7, 8, 4, 2];

/// Advances the route root: `root++`, then 10→5, else 5→0 — the
/// increment happens BEFORE the special cases, so this is not
/// plain modulo (`aHTMD_next_position`). Returns the new root,
/// or -1 for an input outside 0–9.
#[no_mangle]
pub extern "C" fn pc_hatsumode_next_root(root: i32) -> i32 {
    if root < 0 || root > 9 {
        return -1;
    }
    let mut r = root + 1;
    if r == 10 {
        r = 5;
    } else if r == 5 {
        r = 0;
    }
    r
}

/// Writes the next route destination to `out[0..2]` as (x, z)
/// from the saved shrine coordinates:
/// `pos = yashiro_pos + table[ROUTE_ROOT[new_root]]`, and
/// returns the new root (or -1 on invalid input).
#[no_mangle]
pub extern "C" fn pc_hatsumode_next_pos(
    root: i32,
    yashiro_x: i32,
    yashiro_z: i32,
    out: *mut i32,
) -> i32 {
    let new_root = pc_hatsumode_next_root(root);
    if new_root < 0 || out.is_null() {
        return -1;
    }
    let ri = ROUTE_ROOT[new_root as usize] as usize;
    let out = unsafe { std::slice::from_raw_parts_mut(out, 2) };
    out[0] = yashiro_x + ROUTE_POS_X[ri] as i32;
    out[1] = yashiro_z + ROUTE_POS_Z[ri] as i32;
    new_root
}

/// Computes the `aHN0_flag2_wait_check` bit for a think index:
/// think 3 → `0x01 << (idx & 1)`; think 13 → `0x04 << ((idx & 1)
/// ^ 1)`; think 28 → `0x10 << ((idx & 1) ^ 1)`; anything else →
/// 0. The bit is consumed (cleared) on read by the C caller.
#[no_mangle]
pub extern "C" fn pc_hatsumode_wait_mask(think_idx: i32, npc_id: u16) -> u16 {
    let idx = npc_id.wrapping_sub(SP_NPC_EV_HATUMODE_0) as u16;
    match think_idx {
        3 => 1u16 << (idx & 1),
        13 => 4u16 << ((idx & 1) ^ 1),
        28 => 16u16 << ((idx & 1) ^ 1),
        _ => 0,
    }
}

/// Selects the think state after a successful flag wait
/// (`aHN0_flag2_wait`): 43 when think 28 consumed the 0x4000
/// detour bit, otherwise think_idx + 1. The C caller clears the
/// consumed bits.
#[no_mangle]
pub extern "C" fn pc_hatsumode_wait_advance(think_idx: i32, detour_4000: bool) -> i32 {
    if think_idx == 28 && detour_4000 {
        43
    } else {
        think_idx + 1
    }
}

/// New Year shrine spawn layout: eight slots of (field-unit X,
/// Z) (`m_event_map_npc_data.c_inc`): HATUMODE_0..4, MIKO,
/// NEWYEAR_TABLE, SONCHO2. Single map variant on a shrine block.
pub const SPAWN_LAYOUT: [(i32, i32); 8] = [
    (7, 9),
    (6, 10),
    (9, 10),
    (6, 12),
    (9, 12),
    (3, 3),
    (3, 3),
    (11, 6),
];
/// NPC slot mask `0b00011111` in the data record (5 NPC bits).
pub const SPAWN_NPC_MASK: u32 = 0b00011111;
pub const SPAWN_SLOT_COUNT: usize = 8;

/// Writes the (x, z) field-unit position of a spawn slot to
/// `out[0..2]`; returns 1 on success. Slots are 0-based.
#[no_mangle]
pub extern "C" fn pc_hatsumode_spawn_pos(slot: u8, out: *mut i32) -> u8 {
    if out.is_null() {
        return 0;
    }
    let s = slot as usize;
    if s >= SPAWN_LAYOUT.len() {
        return 0;
    }
    let (px, pz) = SPAWN_LAYOUT[s];
    let out = unsafe { std::slice::from_raw_parts_mut(out, 2) };
    out[0] = px;
    out[1] = pz;
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controller_offer_branches() {
        assert_eq!(pc_hatsumode_offer_msg(true), 4396);
        assert_eq!(pc_hatsumode_offer_msg(false), 4398);
        assert_eq!(pc_hatsumode_choice_continue(true), 0x112D);
        assert_eq!(pc_hatsumode_choice_continue(false), 0x112E);
        assert_eq!(pc_hatsumode_choice_state(true), 7);
        assert_eq!(pc_hatsumode_choice_state(false), 4);
        assert!(pc_hatsumode_wait_ready(0x8000));
        assert!(!pc_hatsumode_wait_ready(0x0001));
        assert_eq!(pc_hatsumode_inori_end_flags(0xFFFF), 0x7FFF);
    }

    #[test]
    fn shrine_relative_positions() {
        // yashiro_pos is [X, Z, Y]: yp = (100, 200, 300).
        let mut out = [0i32; 2];
        let y = pc_hatsumode_ctrl_home(100, 200, 300, out.as_mut_ptr());
        assert_eq!((out[0], out[1], y), (120, 240, 300));
        let y = pc_hatsumode_act_pos(100, 200, 300, out.as_mut_ptr());
        assert_eq!((out[0], out[1], y), (120, 285, 300));
    }

    #[test]
    fn dialogue_formulas() {
        // (4 + (id - 0xD058) - cur) & 3
        assert_eq!(pc_hatsumode_msg_group(0xD058, 0), 0);
        assert_eq!(pc_hatsumode_msg_group(0xD059, 0), 1);
        assert_eq!(pc_hatsumode_msg_group(0xD058, 1), 3);
        // group offsets {12, 3, 6, 9}
        assert_eq!(pc_hatsumode_make_msg(7679, 0, 2), 7679 + 12 + 2);
        assert_eq!(pc_hatsumode_make_msg(7679, 1, 2), 7679 + 3 + 2);
        assert_eq!(pc_hatsumode_make_msg(7679, 2, 2), 7679 + 6 + 2);
        assert_eq!(pc_hatsumode_make_msg(7679, 3, 2), 7679 + 9 + 2);
        assert_eq!(pc_hatsumode_talk_msg(7679, 0, 2), 7679);
        assert_eq!(pc_hatsumode_talk_msg(7679, 1, 2), 7679 + 15 + 2);
        assert_eq!(pc_hatsumode_saisen_continue(7679, true), 7680);
        assert_eq!(pc_hatsumode_saisen_continue(7679, false), 7681);
    }

    #[test]
    fn start_tables() {
        let mut out = [0i16; 2];
        assert_eq!(pc_hatsumode_start_offset(0, out.as_mut_ptr()), 1);
        assert_eq!(out, [20, -10]);
        assert_eq!(pc_hatsumode_start_offset(4, out.as_mut_ptr()), 1);
        assert_eq!(out, [-40, 0]);
        assert_eq!(pc_hatsumode_start_offset(5, out.as_mut_ptr()), 0);
        assert_eq!(pc_hatsumode_start_root(2), 9);
        assert_eq!(pc_hatsumode_start_root(5), -1);
        // ready2 tables have only 4 entries: index 4 is invalid.
        let mut b = [0u8; 2];
        assert_eq!(pc_hatsumode_ready2(0, b.as_mut_ptr()), 1);
        assert_eq!(b, [5, 4]);
        assert_eq!(pc_hatsumode_ready2(3, b.as_mut_ptr()), 1);
        assert_eq!(b, [3, 20]);
        assert_eq!(pc_hatsumode_ready2(4, b.as_mut_ptr()), 0);
    }

    #[test]
    fn route_wrap() {
        // Pre-increment, then 10->5, 5->0.
        assert_eq!(pc_hatsumode_next_root(0), 1);
        assert_eq!(pc_hatsumode_next_root(3), 4);
        assert_eq!(pc_hatsumode_next_root(4), 0); // 5 -> 0
        assert_eq!(pc_hatsumode_next_root(9), 5); // 10 -> 5
        assert_eq!(pc_hatsumode_next_root(10), -1);
        // Destination: root 0 -> 1 -> ROUTE_ROOT[1] = 5 -> (100, 75) + shrine.
        let mut out = [0i32; 2];
        assert_eq!(pc_hatsumode_next_pos(0, 1000, 2000, out.as_mut_ptr()), 1);
        assert_eq!(out, [1100, 2075]);
    }

    #[test]
    fn flag_masks() {
        // think 3: 0x01 << (idx & 1)
        assert_eq!(pc_hatsumode_wait_mask(3, 0xD058), 0x01);
        assert_eq!(pc_hatsumode_wait_mask(3, 0xD059), 0x02);
        // think 13: 0x04 << ((idx & 1) ^ 1)
        assert_eq!(pc_hatsumode_wait_mask(13, 0xD058), 0x08);
        assert_eq!(pc_hatsumode_wait_mask(13, 0xD059), 0x04);
        // think 28: 0x10 << ((idx & 1) ^ 1)
        assert_eq!(pc_hatsumode_wait_mask(28, 0xD058), 0x20);
        assert_eq!(pc_hatsumode_wait_mask(28, 0xD059), 0x10);
        assert_eq!(pc_hatsumode_wait_mask(7, 0xD058), 0);
        // think advance: 28 + detour -> 43, else +1.
        assert_eq!(pc_hatsumode_wait_advance(28, true), 43);
        assert_eq!(pc_hatsumode_wait_advance(28, false), 29);
        assert_eq!(pc_hatsumode_wait_advance(3, true), 4);
    }

    #[test]
    fn spawn_layout() {
        let mut out = [0i32; 2];
        assert_eq!(pc_hatsumode_spawn_pos(0, out.as_mut_ptr()), 1);
        assert_eq!(out, [7, 9]);
        assert_eq!(pc_hatsumode_spawn_pos(7, out.as_mut_ptr()), 1);
        assert_eq!(out, [11, 6]);
        assert_eq!(pc_hatsumode_spawn_pos(8, out.as_mut_ptr()), 0);
    }
}
