//! Police officers: Copper and Booker — pure data and kernels.
//!
//! Verified against `src/actor/npc/ac_npc_police.c`,
//! `src/actor/npc/ac_npc_police_move.c_inc`,
//! `src/actor/npc/ac_npc_police2.c`,
//! `src/actor/npc/ac_npc_police2_move.c_inc`,
//! `src/game/m_police_box.c`, `src/game/m_npc.c`,
//! `src/actor/ac_aprilfool_control.c`,
//! `include/m_police_box.h`, `include/m_name_table.h`,
//! `include/ac_npc_police2.h`, `include/ac_npc_post_man.h`
//! (GAFE01_00 Rev 0 USA decomp).
//! All 20 brief claims checked verbatim; no corrections.
//!
//! Architecture: both actors (Copper's schedule/idle/dialogue, Booker's
//! movement/lost-and-found interaction), the PoliceBox_c save store, the
//! field-item relocation, the item-generation tables, the text-entry and
//! message formatting, and the April Fools overrides all stay in C.
//! This module holds only the source-proven pure data and calculations
//! with primitive arguments.
//!
//! RNG fidelity: the RANDOM/RANDOM_F/qrand/fqrand calls stay in C.
//! The exported gates describe the CONDITIONS under which those rolls
//! happen; they never roll themselves.

/// Officer identities (`include/m_name_table.h`).
pub mod id {
    /// Copper, outside the station.
    pub const POLICE: u16 = 0xD00D;
    /// Booker, inside the station.
    pub const POLICE2: u16 = 0xD010;
}

/// NPC name indices (`m_npc.c`).
pub mod name_idx {
    pub const COPPER: u16 = 0x210;
    pub const BOOKER: u16 = 0x6D9;
}

/// April Fools data indices (`ac_aprilfool_control.c`).
pub mod april {
    pub const COPPER: u16 = 0x0005;
    pub const BOOKER: u16 = 0x0006;
}

/// Lost-and-found storage (`include/m_police_box.h`).
pub mod store {
    /// 20 slots of `mActor_name_t` (u16); `sizeof(PoliceBox_c) == 0x28`.
    pub const ITEM_STORAGE_COUNT: u8 = 20;
    /// Grow cap for forced generation.
    pub const MAX_GROW_SIZE: u8 = 5;
}

/// Forced-generation categories (`m_police_box.h`).
pub mod category {
    pub const GOODS: u8 = 0;
    pub const ITEM: u8 = 1;
    pub const FLOWER: u8 = 2;
    pub const UMBRELLA: u8 = 3;
}

/// Goods sub-categories (`mPB_get_force_set_item_goods`).
pub mod goods {
    pub const FURNITURE: u8 = 0;
    pub const STATIONERY: u8 = 1;
    pub const CLOTHING: u8 = 2;
    pub const CARPET: u8 = 3;
    pub const WALLPAPER: u8 = 4;
}

/// Booker's action states (`ac_npc_police2.c`).
pub mod act {
    pub const GREET: u8 = 0;
    pub const WAIT: u8 = 1;
    pub const WALK_PL_SAME_ZONE: u8 = 2;
    pub const WALK_PL_OTHER_ZONE: u8 = 3;
    pub const RUN_PL_SAME_ZONE: u8 = 4;
    pub const RUN_PL_OTHER_ZONE: u8 = 5;
    pub const TURN: u8 = 6;
    pub const CHECK_ANSWER: u8 = 7;
    pub const TALK_END_WAIT: u8 = 8;
}

/// Reserved police-item base (`m_name_table.h`). The decomp's own
/// comment flags that 0xF128 is shared with DUMMY_BOAT — the constant
/// is duplicated, not a typo.
pub const RSV_POLICE_ITEM_0: u16 = 0xF128;

/// 67.5 degrees in the s16 short-angle unit used by the actor code:
/// `DEG2SHORT_ANGLE2(67.5f32)` = 67.5 * 65536 / 360 = 12288.
pub const ANGLE_67_5_DEG: i16 = 12288;

/// Copper's ordinary talk table (`aPOL_set_norm_talk_info_talk_request`).
pub const COPPER_MSG_TABLE: [u16; 8] =
    [0x0772, 0x0773, 0x0774, 0x0775, 0x18C6, 0x18C7, 0x18C8, 0x18C9];

/// Copper's entrance greeting.
pub const COPPER_GREET_MSG: u16 = 0x0771;

/// Copper's choice messages: choice 1 → 0x0782/0x0783 by nonempty,
/// choice 2 → 0x0777, anything else (default) → 0x0776.
/// Choice 0 is the visitor/event hint path and stays in C.
pub mod copper_choice {
    pub const NONEMPTY: u16 = 0x0782;
    pub const EMPTY: u16 = 0x0783;
    pub const CHOICE2: u16 = 0x0777;
    pub const DEFAULT: u16 = 0x0776;
}

/// Copper's time-of-day index: before 05:00 → 3, 05:00–10:00 → 0,
/// 10:00–17:00 → 1, 17:00 onward → 2.
pub fn copper_time_idx(now_sec: u32) -> u8 {
    if now_sec < 5 * 3600 {
        3
    } else if now_sec < 10 * 3600 {
        0
    } else if now_sec < 17 * 3600 {
        1
    } else {
        2
    }
}

/// Copper's talk message: `msg_table[idx]`, +4 when `aPOL_check_player()`
/// is true (the player-kind/map-flag test stays in C, as does the
/// April Fools override).
pub fn copper_talk_msg(now_sec: u32, is_player: bool) -> u16 {
    let idx = copper_time_idx(now_sec) as usize + if is_player { 4 } else { 0 };
    COPPER_MSG_TABLE[idx]
}

/// Copper's choice message. Returns 0 for choice 0 — the hint path,
/// which stays in C.
pub fn copper_choice_msg(choice: u8, nonempty: bool) -> u16 {
    match choice {
        0 => 0,
        1 => {
            if nonempty {
                copper_choice::NONEMPTY
            } else {
                copper_choice::EMPTY
            }
        }
        2 => copper_choice::CHOICE2,
        _ => copper_choice::DEFAULT,
    }
}

/// Event-hint tables (`aPOL_get_hint_msg_no`), indexed by
/// `event − mEv_EVENT_ARTIST` (0–5).
pub const HINT_RUN_MSG: [u16; 6] = [0x2C24, 0x2C23, 0x2C22, 0x2C26, 0x2C21, 0x2C25];
pub const HINT_ACT_MSG: [u16; 6] = [0x2C34, 0x2C33, 0x2C32, 0x2C36, 0x2C31, 0x2C35];
pub const HINT_RSV_MSG: [u16; 6] = [0x2C2A, 0x2C29, 0x2C28, 0x2C2C, 0x2C27, 0x2C2B];
/// Initial fallback and the unscheduled/other-state message.
pub const HINT_FALLBACK: u16 = 0x2C2D;
pub const HINT_UNSCHEDULED: u16 = 0x2C37;

/// Active event: `run_msg_no[idx]` if the shop-sale event or the
/// block-string gate succeeds, else 0x2C37.
pub fn hint_active_msg(idx: u8, shop_sale: bool, block_str_ok: bool) -> u16 {
    if shop_sale || block_str_ok {
        HINT_RUN_MSG[(idx % 6) as usize]
    } else {
        HINT_UNSCHEDULED
    }
}

/// Scheduled-today event: `act_msg_no[idx]`.
pub fn hint_today_msg(idx: u8) -> u16 {
    HINT_ACT_MSG[(idx % 6) as usize]
}

/// Scheduled-later event: `rsv_msg_no[idx]` (the day-string formatting
/// stays in C).
pub fn hint_later_msg(idx: u8) -> u16 {
    HINT_RSV_MSG[(idx % 6) as usize]
}

/// Aerobics entry gate (`aPOL_think_main_proc`), verbatim: not rain,
/// 06:00 ≤ now < 07:00, player outside 67.5° (`ABS(d_angle) > 12288`).
/// The `RANDOM_F(1.0f32) < 0.05f32` roll stays in C — this gate decides
/// whether the roll happens.
pub fn copper_aerobics_gate(now_sec: u32, not_rain: bool, abs_angle: i16) -> bool {
    not_rain
        && now_sec >= 6 * 3600
        && now_sec < 7 * 3600
        && abs_angle > ANGLE_67_5_DEG
}

/// Aerobics animation selection (`aPOL_taisou_act_init_proc`): three
/// sequences with loop counts {2, 4, 1}, morph −8.0f32. The RANDOM(3)
/// index stays in C.
pub const AEROBICS_LOOPS: [u8; 3] = [2, 4, 1];
pub const AEROBICS_MORPH: f32 = -8.0f32;

/// Aerobics exit: loop count exhausted, or the player back within 67.5°.
pub fn copper_aerobics_done(loop_flag: i16, abs_angle: i16) -> bool {
    loop_flag == 0 || abs_angle <= ANGLE_67_5_DEG
}

/// Sleepy entry gate (`aPOL_think_main_proc`), verbatim: 02:00 ≤ now <
/// 04:00 and the player at least 60 units away. It is an `else if` on
/// the aerobics condition — evaluated only when NOT (not-rain AND
/// 06:00–07:00). The `RANDOM_F(1.0f32) < 0.05f32` roll stays in C.
pub fn copper_sleepy_gate(now_sec: u32, dist_xz: f32) -> bool {
    now_sec >= 2 * 3600 && now_sec < 4 * 3600 && dist_xz >= 60.0f32
}

/// Sleepy animation sequencing (`aPOL_set_animation`): step 0 → NEMU1
/// (morph −16.0f32), step 1 → GYAFUN1 (morph −2.0f32), 5 loops each.
pub const SLEEPY_MORPH: [f32; 2] = [-16.0f32, -2.0f32];
pub const SLEEPY_LOOPS: u8 = 5;

/// Booker's zone (`aPOL2_get_zone`), verbatim: `uz` clamped to 2–6,
/// `zone = (uz − 2) * 4`, then strict `>` at 7/4/1. Range 0–19.
pub fn booker_zone(ux: i32, uz: i32) -> u8 {
    let uz = uz.clamp(2, 6);
    let mut zone = (uz - 2) * 4;
    if ux > 7 {
        zone += 3;
    } else if ux > 4 {
        zone += 2;
    } else if ux > 1 {
        zone += 1;
    }
    zone as u8
}

/// Booker's movement decision (`aPOL2_decide_next_move_act`), verbatim
/// against the SQUARED distance — the expressions are kept as written:
/// `dist < (SQ(50.0f32)−50.0f32)` = 2450.0 and `dist < (SQ(70.0f32)+100.0f32)` =
/// 5000.0. Do not simplify.
///
/// Quirk preserved: the same-zone far case returns RUN_PL_OTHER_ZONE,
/// and RUN_PL_SAME_ZONE is never produced by this function.
pub fn booker_move_action(dist_sq: f32, same_zone: bool, collided: bool) -> u8 {
    if collided {
        return act::WAIT;
    }
    if dist_sq < (50.0f32 * 50.0f32 - 50.0f32) {
        act::WAIT
    } else if !same_zone {
        if dist_sq < (70.0f32 * 70.0f32 + 100.0f32) {
            act::WALK_PL_OTHER_ZONE
        } else {
            act::RUN_PL_OTHER_ZONE
        }
    } else if dist_sq < (70.0f32 * 70.0f32 + 100.0f32) {
        act::WALK_PL_SAME_ZONE
    } else {
        act::RUN_PL_OTHER_ZONE
    }
}

/// Booker movement speeds (max, acceleration, deceleration).
pub mod speed {
    pub const WALK: (f32, f32, f32) = (1.0, 0.1, 0.2);
    pub const RUN: (f32, f32, f32) = (4.0, 0.4, 0.8);
}

/// Lost-and-found slot from the reserved prop: `idx = rsv_item −
/// RSV_POLICE_ITEM_0`. The range check (`ITEM_IS_RSVPOLICE`) and the
/// EMPTY_NO guard stay in C; `item_idx` is `s16` with −1 as the empty
/// sentinel.
pub fn slot_idx(rsv_item: u16) -> i16 {
    (rsv_item as i16).wrapping_sub(RSV_POLICE_ITEM_0 as i16)
}

/// Booker's entrance greeting: empty store → 0x0784; nonempty → 0x0785
/// and `_99C` is set. Returns (message, set_99c).
pub fn booker_greet_msg(keep_sum: u8) -> (u16, bool) {
    if keep_sum == 0 {
        (0x0784, false)
    } else {
        (0x0785, true)
    }
}

/// Booker's normal talk (`aPOL2_set_norm_talk_info_message_ctrl`): the
/// April Fools override stays in C.
pub fn booker_talk_msg(keep_sum: u8, flag_99c: bool) -> u16 {
    if keep_sum == 0 {
        if flag_99c {
            0x0787
        } else {
            0x0786
        }
    } else {
        0x077D
    }
}

/// Item-name message for the selected lost-and-found slot.
pub const BOOKER_ITEM_MSG: u16 = 0x077E;
/// Continuation message when the player's inventory is full.
pub const BOOKER_FULL_MSG: u16 = 0x0781;
/// Dummy foreground position used on a successful pickup.
pub const PICKUP_DUMMY_POS: (f32, f32, f32) = (20.0, 0.0, 20.0);

/// Forced-generation category from the `(int)(fqrand() * 100.0f32)` roll,
/// verbatim thresholds: ≤85 goods, ≤90 item, ≤95 flower, ≤100 umbrella.
pub fn force_category(roll: u8) -> u8 {
    if roll <= 85 {
        category::GOODS
    } else if roll <= 90 {
        category::ITEM
    } else if roll <= 95 {
        category::FLOWER
    } else {
        category::UMBRELLA
    }
}

/// Goods sub-category from the same roll form, verbatim thresholds:
/// ≤35 furniture, ≤58 stationery, ≤88 clothing, ≤94 carpet, else wallpaper.
pub fn goods_category(roll: u8) -> u8 {
    if roll <= 35 {
        goods::FURNITURE
    } else if roll <= 58 {
        goods::STATIONERY
    } else if roll <= 88 {
        goods::CLOTHING
    } else if roll <= 94 {
        goods::CARPET
    } else {
        goods::WALLPAPER
    }
}

/// Forced-generation eligibility (`mPB_force_set_keep_item`): the count
/// must be ≤ MAX_GROW_SIZE and `(int)qrand() >= 0`. The qrand() call
/// stays in C — pass its evaluated sign; the expression's exact
/// semantics are deliberately NOT resolved here.
pub fn force_set_eligible(keep_sum: u8, qrand_nonneg: bool) -> bool {
    keep_sum <= store::MAX_GROW_SIZE && qrand_nonneg
}

/// `mPB_keep_item()` append index: append at the current sum when under
/// capacity; when full the oldest entry is shifted out and the new item
/// lands at the last index (19). The shift effect: slot i gets old
/// slot i+1. ITEM1/furniture acceptance stays in C.
pub fn keep_append_index(keep_sum: u8) -> u8 {
    if keep_sum >= store::ITEM_STORAGE_COUNT {
        store::ITEM_STORAGE_COUNT - 1
    } else {
        keep_sum
    }
}

/// `mPB_keep_all_item_in_block()` merge: given the existing sum and the
/// collected count, returns (dst_start, drop_oldest) — how many oldest
/// saved entries to drop and where the new batch lands.
pub fn block_merge(keep_sum: u8, count: u8) -> (u8, u8) {
    let total = keep_sum as u16 + count as u16;
    if total <= store::ITEM_STORAGE_COUNT as u16 {
        (keep_sum, 0)
    } else {
        let drop = (total - store::ITEM_STORAGE_COUNT as u16) as u8;
        (keep_sum - drop, drop)
    }
}

/// Station-exit constants (`aPOL2_player_getout_check`).
pub mod getout {
    pub const SCENE_MODE: u8 = 14;
    pub const WIPE: u16 = 0x195;
}

// ---- C ABI ----
// All pc_-prefixed: the C originals stay compiled; these are staged kernels.

#[no_mangle]
pub extern "C" fn pc_police_copper_talk_msg(now_sec: u32, is_player: u8) -> u16 {
    copper_talk_msg(now_sec, is_player != 0)
}

#[no_mangle]
pub extern "C" fn pc_police_copper_choice_msg(choice: u8, nonempty: u8) -> u16 {
    copper_choice_msg(choice, nonempty != 0)
}

#[no_mangle]
pub extern "C" fn pc_police_copper_aerobics_gate(now_sec: u32, not_rain: u8, abs_angle: i16) -> u8 {
    copper_aerobics_gate(now_sec, not_rain != 0, abs_angle) as u8
}

#[no_mangle]
pub extern "C" fn pc_police_copper_sleepy_gate(now_sec: u32, dist_xz: f32) -> u8 {
    copper_sleepy_gate(now_sec, dist_xz) as u8
}

#[no_mangle]
pub extern "C" fn pc_police_hint_active_msg(idx: u8, shop_sale: u8, block_ok: u8) -> u16 {
    hint_active_msg(idx, shop_sale != 0, block_ok != 0)
}

#[no_mangle]
pub extern "C" fn pc_police_hint_today_msg(idx: u8) -> u16 {
    hint_today_msg(idx)
}

#[no_mangle]
pub extern "C" fn pc_police_hint_later_msg(idx: u8) -> u16 {
    hint_later_msg(idx)
}

#[no_mangle]
pub extern "C" fn pc_police_booker_zone(ux: i32, uz: i32) -> u8 {
    booker_zone(ux, uz)
}

#[no_mangle]
pub extern "C" fn pc_police_booker_move_action(dist_sq: f32, same_zone: u8, collided: u8) -> u8 {
    booker_move_action(dist_sq, same_zone != 0, collided != 0)
}

#[no_mangle]
pub extern "C" fn pc_police_booker_slot_idx(rsv_item: u16) -> i16 {
    slot_idx(rsv_item)
}

#[no_mangle]
pub extern "C" fn pc_police_booker_greet_msg(keep_sum: u8) -> u16 {
    booker_greet_msg(keep_sum).0
}

#[no_mangle]
pub extern "C" fn pc_police_booker_talk_msg(keep_sum: u8, flag_99c: u8) -> u16 {
    booker_talk_msg(keep_sum, flag_99c != 0)
}

#[no_mangle]
pub extern "C" fn pc_police_force_category(roll: u8) -> u8 {
    force_category(roll)
}

#[no_mangle]
pub extern "C" fn pc_police_goods_category(roll: u8) -> u8 {
    goods_category(roll)
}

#[no_mangle]
pub extern "C" fn pc_police_keep_append_index(keep_sum: u8) -> u8 {
    keep_append_index(keep_sum)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identities() {
        assert_eq!(id::POLICE, 0xD00D);
        assert_eq!(id::POLICE2, 0xD010);
        assert_eq!(name_idx::COPPER, 0x210);
        assert_eq!(name_idx::BOOKER, 0x6D9);
        assert_eq!(april::COPPER, 0x0005);
        assert_eq!(april::BOOKER, 0x0006);
        assert_eq!(RSV_POLICE_ITEM_0, 0xF128);
    }

    #[test]
    fn copper_time() {
        assert_eq!(copper_time_idx(4 * 3600 + 59 * 60), 3);
        assert_eq!(copper_time_idx(5 * 3600), 0);
        assert_eq!(copper_time_idx(9 * 3600 + 59 * 60), 0);
        assert_eq!(copper_time_idx(10 * 3600), 1);
        assert_eq!(copper_time_idx(16 * 3600 + 59 * 60), 1);
        assert_eq!(copper_time_idx(17 * 3600), 2);
        assert_eq!(copper_time_idx(23 * 3600), 2);
        assert_eq!(copper_talk_msg(8 * 3600, false), 0x0772);
        assert_eq!(copper_talk_msg(8 * 3600, true), 0x18C6);
        assert_eq!(copper_talk_msg(3 * 3600, false), 0x0775);
        assert_eq!(copper_talk_msg(3 * 3600, true), 0x18C9);
        assert_eq!(copper_talk_msg(20 * 3600, true), 0x18C8);
        assert_eq!(COPPER_GREET_MSG, 0x0771);
    }

    #[test]
    fn copper_choices() {
        assert_eq!(copper_choice_msg(0, true), 0); // hint path, stays in C
        assert_eq!(copper_choice_msg(1, true), 0x0782);
        assert_eq!(copper_choice_msg(1, false), 0x0783);
        assert_eq!(copper_choice_msg(2, true), 0x0777);
        assert_eq!(copper_choice_msg(9, true), 0x0776); // default
    }

    #[test]
    fn hints() {
        assert_eq!(HINT_FALLBACK, 0x2C2D);
        assert_eq!(HINT_UNSCHEDULED, 0x2C37);
        assert_eq!(hint_active_msg(0, true, false), 0x2C24);
        assert_eq!(hint_active_msg(2, false, true), 0x2C22);
        assert_eq!(hint_active_msg(2, false, false), 0x2C37);
        assert_eq!(hint_today_msg(5), 0x2C35);
        assert_eq!(hint_later_msg(3), 0x2C2C);
    }

    #[test]
    fn idle_gates() {
        assert!(copper_aerobics_gate(6 * 3600 + 30 * 60, true, 13000));
        assert!(!copper_aerobics_gate(6 * 3600, false, 13000)); // rain
        assert!(!copper_aerobics_gate(7 * 3600, true, 13000)); // too late
        assert!(!copper_aerobics_gate(6 * 3600, true, 12288)); // within 67.5
        assert!(!copper_aerobics_gate(6 * 3600, true, 10000));
        assert_eq!(ANGLE_67_5_DEG, 12288);
        assert!(copper_aerobics_done(0, 20000));
        assert!(copper_aerobics_done(3, 10000));
        assert!(!copper_aerobics_done(3, 20000));
        assert!(copper_sleepy_gate(3 * 3600, 60.0));
        assert!(copper_sleepy_gate(3 * 3600, 100.0));
        assert!(!copper_sleepy_gate(3 * 3600, 59.9));
        assert!(!copper_sleepy_gate(4 * 3600, 100.0));
        assert_eq!(AEROBICS_LOOPS, [2, 4, 1]);
    }

    #[test]
    fn zones() {
        assert_eq!(booker_zone(0, 2), 0);
        assert_eq!(booker_zone(1, 2), 0); // strict > 1
        assert_eq!(booker_zone(2, 2), 1);
        assert_eq!(booker_zone(4, 2), 1); // strict > 4
        assert_eq!(booker_zone(5, 2), 2);
        assert_eq!(booker_zone(8, 2), 3);
        assert_eq!(booker_zone(8, 6), 19);
        assert_eq!(booker_zone(8, 9), 19); // uz clamped to 6
        assert_eq!(booker_zone(8, 0), 3); // uz clamped to 2
    }

    #[test]
    fn movement() {
        // thresholds as written: 2450.0 and 5000.0 on squared distance
        assert_eq!(booker_move_action(2449.9, false, false), act::WAIT);
        assert_eq!(booker_move_action(2450.0, false, false), act::WALK_PL_OTHER_ZONE);
        assert_eq!(booker_move_action(4999.9, false, false), act::WALK_PL_OTHER_ZONE);
        assert_eq!(booker_move_action(5000.0, false, false), act::RUN_PL_OTHER_ZONE);
        assert_eq!(booker_move_action(3000.0, true, false), act::WALK_PL_SAME_ZONE);
        // same-zone far case returns RUN_PL_OTHER_ZONE (verbatim quirk)
        assert_eq!(booker_move_action(6000.0, true, false), act::RUN_PL_OTHER_ZONE);
        assert_eq!(booker_move_action(99999.0, false, true), act::WAIT); // collision first
        assert_eq!(speed::WALK, (1.0, 0.1, 0.2));
        assert_eq!(speed::RUN, (4.0, 0.4, 0.8));
    }

    #[test]
    fn booker_slots_and_talk() {
        assert_eq!(slot_idx(0xF128), 0);
        assert_eq!(slot_idx(0xF12B), 3);
        assert_eq!(booker_greet_msg(0), (0x0784, false));
        assert_eq!(booker_greet_msg(3), (0x0785, true));
        assert_eq!(booker_talk_msg(0, false), 0x0786);
        assert_eq!(booker_talk_msg(0, true), 0x0787);
        assert_eq!(booker_talk_msg(5, false), 0x077D);
        assert_eq!(BOOKER_ITEM_MSG, 0x077E);
        assert_eq!(BOOKER_FULL_MSG, 0x0781);
    }

    #[test]
    fn lost_and_found() {
        assert_eq!(store::ITEM_STORAGE_COUNT, 20);
        assert_eq!(store::MAX_GROW_SIZE, 5);
        assert_eq!(force_category(0), category::GOODS);
        assert_eq!(force_category(85), category::GOODS);
        assert_eq!(force_category(86), category::ITEM);
        assert_eq!(force_category(90), category::ITEM);
        assert_eq!(force_category(91), category::FLOWER);
        assert_eq!(force_category(95), category::FLOWER);
        assert_eq!(force_category(96), category::UMBRELLA);
        assert_eq!(force_category(100), category::UMBRELLA);
        assert_eq!(goods_category(35), goods::FURNITURE);
        assert_eq!(goods_category(36), goods::STATIONERY);
        assert_eq!(goods_category(58), goods::STATIONERY);
        assert_eq!(goods_category(59), goods::CLOTHING);
        assert_eq!(goods_category(88), goods::CLOTHING);
        assert_eq!(goods_category(89), goods::CARPET);
        assert_eq!(goods_category(94), goods::CARPET);
        assert_eq!(goods_category(95), goods::WALLPAPER);
        assert!(force_set_eligible(5, true));
        assert!(!force_set_eligible(6, true));
        assert!(!force_set_eligible(5, false));
        assert_eq!(keep_append_index(0), 0);
        assert_eq!(keep_append_index(19), 19);
        assert_eq!(keep_append_index(20), 19); // shift, append at 19
        assert_eq!(block_merge(10, 5), (10, 0));
        assert_eq!(block_merge(18, 5), (15, 3)); // drop 3 oldest, land at 15
        assert_eq!(block_merge(20, 20), (0, 20));
        assert_eq!(getout::SCENE_MODE, 14);
        assert_eq!(getout::WIPE, 0x195);
    }
}
