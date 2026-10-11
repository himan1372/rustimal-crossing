//! The Fishing Tourney event (`ac_turi.c`, `ac_turi_npc0.c`,
//! `ac_ev_angler.c`, `ac_turi_clip.c_inc`, `m_fishrecord.c`).
//!
//! Verified against `src/actor/ac_turi.c`,
//! `src/actor/npc/ac_turi_npc0.c`, `ac_turi_npc0_move.c_inc`,
//! `ac_turi_npc0_anime.c_inc`, `include/ac_turi_npc0.h`,
//! `src/actor/npc/event/ac_ev_angler.c`,
//! `ac_ev_angler_move.c_inc`, `include/ac_ev_angler.h`,
//! `src/actor/ac_turi_clip.c_inc`, `src/game/m_fishrecord.c`,
//! `include/m_fishrecord.h`, `src/game/m_event_schedule.c_inc`,
//! `src/game/m_event_map_npc_data.c_inc`,
//! `src/actor/ac_event_manager.c` (GAFE01_00 Rev 0 USA).
//! All 7 sections PROVEN with one DIFFERS (see below).
//!
//! Scope split: the eight schedule rows (June/November Sunday
//! windows, SONCHO, RUMOR, TALK) live in the sibling
//! `event_day` module. This module owns the pure kernels: the
//! fish-size formulas, the NPC record rank selection, the
//! strict record comparison, the time/entry gates, the item
//! classification and message-number table, the stand variant
//! and block-deletion rule, the participant angle table, the
//! dialogue arithmetic, and the animation frame speeds. Actor
//! lifecycle, the structure-actor framework, NPC clip/schedule/
//! demo/talk, the rod tool-actor, the save/common areas, the
//! record database insertion/recycling, mail, and the
//! event-manager pool orchestration stay in C.
//!
//! Nuances carried: the stand's deletion test is
//! `ABS(diff) > 1`, NOT `!=` — adjacent blocks do NOT delete
//! the stand (DIFFERS-level correction, same pattern as the
//! Hatsumode brief fix); `mFR_fish_rndsize` is f32 arithmetic
//! with truncation, never rounding; the large formula squares
//! the RNG sample while the small formula subtracts the
//! squared contribution; `mFR_make_NpcRecord` draws
//! RANDOM(2)/RANDOM(3) BEFORE the fqrand() inside
//! mFR_fish_rndsize, and `mEv_fishRecord_local` makes an extra
//! apparently-unused fqrand() call — so ALL RNG stays in C and
//! helpers take samples as inputs; the record comparison is
//! strictly `<` (ties do not beat); the host's time check is
//! `hour < 6 || hour >= 18` while the schedule window is
//! nominally 06:00–17:00; the 40-entry message table has two
//! out-of-order entries (FISH16 → 0x110C, FISH33 → 0x2FAF)
//! that a formulaic table would get wrong; the stand draw uses
//! `shadow[arg0]` (raw, not `arg0 & 1`) — safe only because
//! exactly two stand variants exist; the 180° table entry wraps
//! to −32768 under C's (s16) cast, reproduced here with
//! wrapping semantics.

/// First tournament participant NPC ID (`m_name_table.h`).
pub const SP_NPC_EV_TURI_0: u16 = 0xD049;
/// The event host (Chip) NPC ID.
pub const SP_NPC_ANGLER: u16 = 0xD01D;
/// First fishing-stand structure actor ID.
pub const FISHCHECK_STAND0: u16 = 0x5838;
/// First fish item ID (`m_name_table.h`).
pub const ITM_FISH_START: u16 = 0x2300;
pub const ITM_FISH05: u16 = 0x2305;
pub const ITM_FISH06: u16 = 0x2306;
pub const ITM_FISH07: u16 = 0x2307;
/// Number of tournament participants.
pub const TURI_NPC_MAX: usize = 5;

/// Fish size ranks (`mFR_SIZE_*`).
pub const FISH_SIZE_SMALL: i32 = 0;
pub const FISH_SIZE_MEDIUM: i32 = 1;
pub const FISH_SIZE_LARGE: i32 = 2;

/// Submitted-item classes (`aEANG_TYPE_*`).
pub const ITEM_TYPE_BASS: i32 = 0;
pub const ITEM_TYPE_FISH: i32 = 1;
pub const ITEM_TYPE_OTHER: i32 = 2;
pub const ITEM_TYPE_TOOL: i32 = 3;

/// Computes a fish size from a pre-sampled RNG value
/// (`mFR_fish_rndsize`): LARGE = (50 + 20·rng²)/2.54, MEDIUM =
/// (30 + 20·rng)/2.54, SMALL = (30 − 20·rng²)/2.54, f32
/// arithmetic truncated toward zero. The `fqrand()` sample
/// stays in C. Returns -1 for an invalid rank.
#[no_mangle]
pub extern "C" fn pc_fish_size_from_rank(rank: i32, rng: f32) -> i32 {
    let size = match rank {
        FISH_SIZE_SMALL => (30.0 - 20.0 * rng * rng) / 2.54,
        FISH_SIZE_MEDIUM => (30.0 + 20.0 * rng) / 2.54,
        FISH_SIZE_LARGE => (50.0 + 20.0 * rng * rng) / 2.54,
        _ => return -1,
    };
    size as i32
}

/// Selects the NPC record rank branch for an hour
/// (`mFR_make_NpcRecord`): before 09:00 → 0 (fixed SMALL, no
/// RNG); 09:00–14:59 → 1 (the C caller draws RANDOM(2));
/// 15:00 onward → 2 (the C caller draws RANDOM(3)). The rank
/// from the C-side RANDOM call then feeds
/// `pc_fish_size_from_rank` with the fqrand() sample, in that
/// order.
#[no_mangle]
pub extern "C" fn pc_fish_npc_rank_branch(hour: i32) -> i32 {
    if hour < 9 {
        0
    } else if hour < 15 {
        1
    } else {
        2
    }
}

/// Returns true when a submitted fish beats the live record:
/// strictly `current < submitted` — a tie does not beat
/// (`aEANG_demo_measure`).
#[no_mangle]
pub extern "C" fn pc_fish_record_beaten(current: i32, submitted: i32) -> bool {
    current < submitted
}

/// Returns true while the tournament host accepts entries:
/// the host's own check is `hour < 6 || hour >= 18` (the
/// schedule window is nominally 06:00–17:00).
#[no_mangle]
pub extern "C" fn pc_fish_time_open(hour: i32) -> bool {
    hour >= 6 && hour < 18
}

/// Measurement rank for a submitted item (`aEANG_demo_measure`):
/// FISH07 → LARGE, FISH06 → MEDIUM, everything else → SMALL.
#[no_mangle]
pub extern "C" fn pc_fish_measure_rank(item: i32) -> i32 {
    if item == ITM_FISH07 as i32 {
        FISH_SIZE_LARGE
    } else if item == ITM_FISH06 as i32 {
        FISH_SIZE_MEDIUM
    } else {
        FISH_SIZE_SMALL
    }
}

/// Classifies a submitted item (`aEANG_item_check`): FISH05–07
/// take the bass path; the fish/tool distinction comes from
/// the C-side `ITEM_NAME_GET_CAT` result passed as flags.
#[no_mangle]
pub extern "C" fn pc_angler_item_type(is_fish: bool, is_tool: bool, item: i32) -> i32 {
    if is_fish {
        if item == ITM_FISH05 as i32 || item == ITM_FISH06 as i32 || item == ITM_FISH07 as i32 {
            ITEM_TYPE_BASS
        } else {
            ITEM_TYPE_FISH
        }
    } else if is_tool {
        ITEM_TYPE_TOOL
    } else {
        ITEM_TYPE_OTHER
    }
}

/// Fish-item → message-number table (`aTRC_clip_random_topsize`'s
/// sibling `aTRC_clip_get_msgno`), indexed by
/// `item − ITM_FISH_START`. Two entries are out of order
/// (FISH16 → 0x110C, FISH33 → 0x2FAF); the table is copied
/// verbatim, not derived. Default → 0x10F2.
pub const FISH_MSG_NO: [i32; 40] = [
    0x10F6, 0x10F7, 0x10F8, 0x10F9, 0x10FA, // FISH00–04
    0x1112, 0x1116, 0x111A, // FISH05–07 (bass)
    0x10FB, 0x10FC, 0x10FD, 0x10FE, 0x10FF, 0x1100, 0x1101, 0x1102, // FISH08–15
    0x110C, // FISH16 (out of order)
    0x1103, 0x1104, 0x1105, 0x1106, 0x1107, 0x1108, 0x1109, 0x110A, 0x110B, // FISH17–25
    0x110D, // FISH26
    0x17E6, 0x17E7, 0x17E8, 0x17E9, 0x17EA, // FISH27–31
    0x2FB0, 0x2FAF, // FISH32–33 (out of order)
    0x2FB2, 0x2FB3, 0x2FB4, 0x2FB5, 0x2FB6, 0x2FB1, // FISH34–39
];
pub const FISH_MSG_NO_DEFAULT: i32 = 0x10F2;

/// Looks up the win/lose message base for a fish item; the C
/// caller adds +1 (zannen) or +2 (omedeto).
#[no_mangle]
pub extern "C" fn pc_angler_fish_msgno(item: i32) -> i32 {
    let idx = item.wrapping_sub(ITM_FISH_START as i32);
    if idx < 0 || idx >= FISH_MSG_NO.len() as i32 {
        FISH_MSG_NO_DEFAULT
    } else {
        FISH_MSG_NO[idx as usize]
    }
}

/// Entry bit for a player number: `1 << player_no`
/// (`entry_check`/`entry_on`); the foreigner special case
/// stays in C.
#[no_mangle]
pub extern "C" fn pc_fish_entry_bit(player_no: i32) -> u8 {
    1u8 << ((player_no as u32) & 7)
}

/// Returns true when the reward draw picks clothing:
/// `RANDOM_F(1.0f) > 0.5f` → cloth, else furniture
/// (`aEANG_SelectRandomItem`; the sample stays in C).
#[no_mangle]
pub extern "C" fn pc_fish_reward_is_cloth(rand_float: f32) -> bool {
    rand_float > 0.5
}

/// Stand variant: `npc_id − FISHCHECK_STAND0`
/// (`aTUR_actor_ct`); the structure type/palette add the
/// aSTR_TYPE_TURI / aSTR_PAL_TURI bases in C, the ground-offset
/// selector uses `arg0 + 2`, and the draw picks left/right by
/// `arg0 & 1`.
#[no_mangle]
pub extern "C" fn pc_turi_stand_variant(npc_id: u16) -> i32 {
    npc_id.wrapping_sub(FISHCHECK_STAND0) as i32
}

/// Stand lifetime (`aTUR_actor_move`): the stand self-deletes
/// when the player is outside every scroll-demo state AND the
/// block distance exceeds 1 in either axis — adjacent blocks
/// do NOT delete it (brief correction, verified verbatim).
#[no_mangle]
pub extern "C" fn pc_turi_should_delete(scroll_demo: bool, bx_diff: i32, bz_diff: i32) -> bool {
    !scroll_demo && (bx_diff.abs() > 1 || bz_diff.abs() > 1)
}

/// Participant NPC index: `npc_id − SP_NPC_EV_TURI_0`.
#[no_mangle]
pub extern "C" fn pc_turi_npc_index(npc_id: u16) -> i32 {
    npc_id.wrapping_sub(SP_NPC_EV_TURI_0) as i32
}

/// Participant facing angles in degrees
/// (`def_angle[aTR0_NPC_MAX][7]`, indexed by participant
/// index and `mFI_GetPuleIdx()` pool-layout index).
pub const DEF_ANGLE_DEG: [[f32; 7]; 5] = [
    [225.0, 45.0, 135.0, 315.0, 315.0, 45.0, 225.0],
    [45.0, 225.0, 45.0, 45.0, 315.0, 315.0, 270.0],
    [45.0, 225.0, 45.0, 45.0, 315.0, 315.0, 270.0],
    [315.0, 315.0, 180.0, 0.0, 45.0, 90.0, 315.0],
    [90.0, 0.0, 0.0, 225.0, 225.0, 135.0, 135.0],
];

/// Converts degrees to the retail short angle with C's
/// `(s16)(deg * 65536.0f/360.0f)` semantics: the f32→i32 step
/// is exact for these values and the i32→i16 step wraps, so
/// 180° yields −32768 just like retail.
fn deg_to_short_angle(deg: f32) -> i16 {
    (deg * 65536.0 / 360.0) as i32 as i16
}

/// Returns the participant's facing short-angle for a
/// participant index and pool-layout index, or i16::MIN when
/// out of range.
#[no_mangle]
pub extern "C" fn pc_turi_def_angle(idx: i32, pool_idx: i32) -> i16 {
    if idx < 0 || idx >= 5 || pool_idx < 0 || pool_idx >= 7 {
        return i16::MIN;
    }
    deg_to_short_angle(DEF_ANGLE_DEG[idx as usize][pool_idx as usize])
}

/// Participant dialogue bases per looks index
/// (`ac_turi_npc0_move.c_inc`).
pub const TURI_MSG_BASE: [i32; 6] = [0x1F0A, 0x1F19, 0x1EFB, 0x1F28, 0x1F37, 0x1F46];

/// Computes the participant dialogue message:
/// `msg_base[looks] + rand3 + idx * 3`. The RANDOM(3) sample
/// stays in C.
#[no_mangle]
pub extern "C" fn pc_turi_msg_no(looks: i32, rand3: i32, idx: i32) -> i32 {
    TURI_MSG_BASE[looks.rem_euclid(6) as usize] + rand3 + idx * 3
}

/// Animation frame speeds per looks index
/// (`aTR0_anime_init_proc`).
pub const TURI_FRAME_SPD: [f32; 6] = [0.5, 0.525, 0.5, 0.55, 0.6, 0.45];

/// Returns the wait-animation frame speed for a looks index, or
/// -1.0 when out of range.
#[no_mangle]
pub extern "C" fn pc_turi_frame_speed(looks: i32) -> f32 {
    if looks < 0 || looks >= TURI_FRAME_SPD.len() as i32 {
        -1.0
    } else {
        TURI_FRAME_SPD[looks as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_formulas() {
        // rng = 0.0: SMALL = 30/2.54 = 11, MEDIUM = 30/2.54 = 11, LARGE = 50/2.54 = 19.
        assert_eq!(pc_fish_size_from_rank(0, 0.0), 11);
        assert_eq!(pc_fish_size_from_rank(1, 0.0), 11);
        assert_eq!(pc_fish_size_from_rank(2, 0.0), 19);
        // rng = 1.0: SMALL = (30-20)/2.54 = 3, MEDIUM = 50/2.54 = 19, LARGE = 70/2.54 = 27.
        assert_eq!(pc_fish_size_from_rank(0, 1.0), 3);
        assert_eq!(pc_fish_size_from_rank(1, 1.0), 19);
        assert_eq!(pc_fish_size_from_rank(2, 1.0), 27);
        // Truncation, not rounding: 11.81 -> 11.
        assert_eq!(pc_fish_size_from_rank(0, 0.0), (30.0f32 / 2.54) as i32);
        assert_eq!(pc_fish_size_from_rank(3, 0.5), -1);
    }

    #[test]
    fn rank_branches() {
        assert_eq!(pc_fish_npc_rank_branch(8), 0);
        assert_eq!(pc_fish_npc_rank_branch(9), 1);
        assert_eq!(pc_fish_npc_rank_branch(14), 1);
        assert_eq!(pc_fish_npc_rank_branch(15), 2);
        assert_eq!(pc_fish_npc_rank_branch(23), 2);
    }

    #[test]
    fn records_and_time() {
        assert!(pc_fish_record_beaten(100, 101));
        assert!(!pc_fish_record_beaten(100, 100)); // ties do not beat
        assert!(!pc_fish_record_beaten(101, 100));
        assert!(pc_fish_time_open(6));
        assert!(pc_fish_time_open(17));
        assert!(!pc_fish_time_open(5));
        assert!(!pc_fish_time_open(18));
        assert_eq!(pc_fish_measure_rank(0x2307), FISH_SIZE_LARGE);
        assert_eq!(pc_fish_measure_rank(0x2306), FISH_SIZE_MEDIUM);
        assert_eq!(pc_fish_measure_rank(0x2300), FISH_SIZE_SMALL);
    }

    #[test]
    fn item_classification() {
        assert_eq!(pc_angler_item_type(true, false, 0x2305), ITEM_TYPE_BASS);
        assert_eq!(pc_angler_item_type(true, false, 0x2307), ITEM_TYPE_BASS);
        assert_eq!(pc_angler_item_type(true, false, 0x2300), ITEM_TYPE_FISH);
        assert_eq!(pc_angler_item_type(false, true, 0x1234), ITEM_TYPE_TOOL);
        assert_eq!(pc_angler_item_type(false, false, 0x1234), ITEM_TYPE_OTHER);
    }

    #[test]
    fn msgno_table() {
        assert_eq!(pc_angler_fish_msgno(0x2300), 0x10F6);
        assert_eq!(pc_angler_fish_msgno(0x2304), 0x10FA);
        assert_eq!(pc_angler_fish_msgno(0x2305), 0x1112);
        assert_eq!(pc_angler_fish_msgno(0x2306), 0x1116);
        assert_eq!(pc_angler_fish_msgno(0x2307), 0x111A);
        assert_eq!(pc_angler_fish_msgno(0x2308), 0x10FB);
        assert_eq!(pc_angler_fish_msgno(0x2310), 0x110C); // out-of-order entry
        assert_eq!(pc_angler_fish_msgno(0x231B), 0x17E6);
        assert_eq!(pc_angler_fish_msgno(0x2321), 0x2FAF); // out-of-order entry
        assert_eq!(pc_angler_fish_msgno(0x2327), 0x2FB1);
        assert_eq!(pc_angler_fish_msgno(0x2328), 0x10F2); // default
        assert_eq!(pc_angler_fish_msgno(0x1000), 0x10F2);
    }

    #[test]
    fn stand_rules() {
        assert_eq!(pc_turi_stand_variant(0x5838), 0);
        assert_eq!(pc_turi_stand_variant(0x5839), 1);
        // Adjacent blocks do NOT delete (brief correction).
        assert!(!pc_turi_should_delete(false, 1, 0));
        assert!(!pc_turi_should_delete(false, 1, 1));
        assert!(pc_turi_should_delete(false, 2, 0));
        assert!(pc_turi_should_delete(false, 0, 2));
        assert!(!pc_turi_should_delete(true, 5, 5));
        assert_eq!(pc_turi_npc_index(0xD049), 0);
        assert_eq!(pc_turi_npc_index(0xD04D), 4);
    }

    #[test]
    fn angles_and_dialogue() {
        // 225 -> -24576, 45 -> 8192, 315 -> -8192, 180 wraps to -32768.
        assert_eq!(pc_turi_def_angle(0, 0), -24576);
        assert_eq!(pc_turi_def_angle(0, 1), 8192);
        assert_eq!(pc_turi_def_angle(3, 2), -32768);
        assert_eq!(pc_turi_def_angle(4, 0), 16384);
        assert_eq!(pc_turi_def_angle(5, 0), i16::MIN);
        assert_eq!(pc_turi_msg_no(0, 2, 3), 0x1F0A + 2 + 9);
        assert_eq!(pc_turi_msg_no(5, 0, 0), 0x1F46);
        assert_eq!(pc_turi_frame_speed(1), 0.525);
        assert_eq!(pc_turi_frame_speed(5), 0.45);
        assert_eq!(pc_turi_frame_speed(6), -1.0);
        assert_eq!(pc_fish_entry_bit(0), 0x01);
        assert_eq!(pc_fish_entry_bit(3), 0x08);
        assert!(pc_fish_reward_is_cloth(0.6));
        assert!(!pc_fish_reward_is_cloth(0.5));
    }
}
