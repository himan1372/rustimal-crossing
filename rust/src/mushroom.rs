//! Mushroom season (`m_mushroom.c`, `m_mushroom.h`).
//!
//! Verified against `src/game/m_mushroom.c`,
//! `include/m_mushroom.h`, `src/game/m_event_schedule.c_inc`,
//! `include/m_name_table.h` (GAFE01_00 Rev 0 USA). 16/16
//! sections PROVEN with DIFFERS-level nuances (see below).
//!
//! Scope split: this module owns the pure kernels — the
//! target count, the time conversions and rounding, the
//! zero-time check, tree eligibility, the placement offsets
//! and boundary, the RNG index selection, the two
//! player-exclusion predicates, the cleanup eligibility,
//! the clear-all/share decision, and the share-distribution
//! math. Save-pointer access, the foreground grid and
//! deposit data, the `mFI_*` queries, RTC access, all
//! `fqrand()` call order, `mFI_SetFGUpData()`, and the
//! `lbRTC_*` time helpers stay in C. The mushroom-season
//! schedule row (Oct 15 – Oct 25) lives in the sibling
//! `event_day` module.
//!
//! Nuances carried: the daily schedule is 8:00→5, 8:15→4,
//! 8:30→3, 8:45→2, 9:00→1, 9:15→0, gated on
//! `active || saved date != today`, and the place path
//! also requires the count ≤ 5; the all-zero timestamp
//! takes TWO distinct init paths — `mMsr_SetMushroom`
//! goes to the previous day while `mMsr_FirstClearMushroom`
//! substitutes the current RTC time; placement excludes
//! the whole player X column AND Z row
//! (`bx != pbx - 1 && bz != pbz - 1` — &&, not ||) while
//! cleanup excludes only the single player acre — two
//! separate kernels, do not unify; the "clear all" branch
//! triggers on strict `total < clear_num`; the
//! quarter-hour rounding table {45, 30, 15, 0} rounds DOWN
//! despite the source comment; `Rtc2MushTime` does not
//! touch `active`; there is NO mushroom furniture
//! mechanism — the file only ever writes
//! ITM_FOOD_MUSHROOM, and no mushroom-series furniture
//! IDs exist in the decomp. The startup cleanup passes
//! (0, 0) as the player acre and does NOT check the event
//! status.

/// Active hour and target count (`m_mushroom.h`).
pub const ACTIVE_HOUR: i32 = 8;
pub const NUM_MUSHROOMS: i32 = 5;
/// Saved time size in bytes.
pub const MUSHROOM_TIME_SIZE: usize = 6;
/// Acre grid totals.
pub const FG_BLOCK_TOTAL_NUM: usize = 30;
pub const UT_X_NUM: i32 = 16;
pub const UT_Z_NUM: i32 = 16;
/// Boundary for candidate trees: ut_x < 15 && ut_z < 15.
pub const TREE_BOUND: i32 = 15;
/// The placed item.
pub const ITM_FOOD_MUSHROOM: i32 = 0x2805;
pub const ITM_FOOD_START: i32 = 0x2800;
/// Event id.
pub const MUSHROOM_SEASON: u16 = 48;

/// Five candidate offsets around a tree, in source order
/// (left, right, below-left, below, below-right).
pub const AREA_OFFSETS: [i32; 5] = [-1, 1, 15, 16, 17];
/// Quarter-hour rounding table — rounds DOWN.
pub const MIN_15_TABLE: [i32; 4] = [45, 30, 15, 0];

/// Eligible tree item ids, in source order.
pub const ELIGIBLE_TREES: [i32; 14] = [
    0x0804, // TREE
    0x080C, // TREE_APPLE_FRUIT
    0x0814, // TREE_ORANGE_FRUIT
    0x081C, // TREE_PEACH_FRUIT
    0x0824, // TREE_PEAR_FRUIT
    0x082C, // TREE_CHERRY_FRUIT
    0x0853, // TREE_100BELLS (source order)
    0x0831, // TREE_1000BELLS
    0x0836, // TREE_10000BELLS
    0x083B, // TREE_30000BELLS
    0x085B, // TREE_PALM_FRUIT
    0x0861, // CEDAR_TREE
    0x0868, // GOLD_TREE_SHOVEL (source order)
    0x0867, // GOLD_TREE
];

/// Target count (`mMsr_GetMushroomNum`): when the update is
/// due (`active || saved date != today`), hour 8 gives
/// `5 − min/15` (C integer division), hour 9 with min < 15
/// gives 1, otherwise 0. Returns 0 when not due.
#[no_mangle]
pub extern "C" fn pc_mushroom_target_count(
    hour: i32,
    min: i32,
    active: bool,
    dates_differ: bool,
) -> i32 {
    if !(active || dates_differ) {
        return 0;
    }
    if hour == ACTIVE_HOUR {
        NUM_MUSHROOMS - min / 15
    } else if hour == ACTIVE_HOUR + 1 && min < 15 {
        NUM_MUSHROOMS - 4
    } else {
        0
    }
}

/// Saves RTC fields to the quarter-hour representation
/// (`mMsr_Rtc2MushTime`): year/month/day/hour and
/// `min / 15` (truncation). Does NOT touch `active`.
/// Writes to `out[0..5]`; returns 1 on success.
#[no_mangle]
pub extern "C" fn pc_mushroom_rtc_to_time(
    year: i32,
    month: i32,
    day: i32,
    hour: i32,
    min: i32,
    out: *mut i32,
) -> u8 {
    if out.is_null() {
        return 0;
    }
    let out = unsafe { std::slice::from_raw_parts_mut(out, 5) };
    out[0] = year;
    out[1] = month;
    out[2] = day;
    out[3] = hour;
    out[4] = min / 15;
    1
}

/// Restores the saved time to RTC fields
/// (`mMsr_Mushtime2Rtc`): minutes = hour_quarter × 15,
/// seconds = 0. Writes to `out[0..6]` as
/// (year, month, day, hour, min, sec); returns 1 on
/// success. The weekday derivation stays in C.
#[no_mangle]
pub extern "C" fn pc_mushroom_time_to_rtc(
    year: i32,
    month: i32,
    day: i32,
    hour: i32,
    hour_quarter: i32,
    out: *mut i32,
) -> u8 {
    if out.is_null() {
        return 0;
    }
    let out = unsafe { std::slice::from_raw_parts_mut(out, 6) };
    out[0] = year;
    out[1] = month;
    out[2] = day;
    out[3] = hour;
    out[4] = hour_quarter * 15;
    out[5] = 0;
    1
}

/// Quarter-hour rounding (`mMsr_Set15Minites`): the first
/// table value ≤ the minute. Rounds down.
#[no_mangle]
pub extern "C" fn pc_mushroom_round_quarter(min: i32) -> i32 {
    for &v in MIN_15_TABLE.iter() {
        if v <= min {
            return v;
        }
    }
    0
}

/// True when the saved timestamp is all zero (compared
/// against `l_mmsr_zeto_time`). The two init paths
/// (previous day vs current time) stay in C.
#[no_mangle]
pub extern "C" fn pc_mushroom_is_zero_time(
    year: i32,
    month: i32,
    day: i32,
    hour: i32,
    hour_quarter: i32,
    active: bool,
) -> bool {
    year == 0 && month == 0 && day == 0 && hour == 0 && hour_quarter == 0 && !active
}

/// Tree eligibility (`mMsr_CheckAroundTree` item check):
/// true for the 14 accepted tree ids.
#[no_mangle]
pub extern "C" fn pc_mushroom_tree_eligible(item_id: i32) -> bool {
    ELIGIBLE_TREES.contains(&item_id)
}

/// Acre-boundary check for candidate trees:
/// `ut_x < 15 && ut_z < 15`.
#[no_mangle]
pub extern "C" fn pc_mushroom_boundary_ok(ut_x: i32, ut_z: i32) -> bool {
    ut_x < TREE_BOUND && ut_z < TREE_BOUND
}

/// Hierarchical index selection: `(fqrand() * count)` with
/// C integer truncation. The RNG sample stays in C.
#[no_mangle]
pub extern "C" fn pc_mushroom_select_index(rand_unit: f32, count: i32) -> i32 {
    (rand_unit * count as f32) as i32
}

/// Placement exclusion: the acre survives only when
/// `bx != player_bx - 1 && bz != player_bz - 1` — so it is
/// excluded when its X matches the player's X column OR
/// its Z matches the player's Z row.
#[no_mangle]
pub extern "C" fn pc_mushroom_placement_excluded(
    bx: i32,
    bz: i32,
    player_bx: i32,
    player_bz: i32,
) -> bool {
    bx == player_bx - 1 || bz == player_bz - 1
}

/// Cleanup exclusion: only the player's single acre. The
/// bounds check stays in C.
#[no_mangle]
pub extern "C" fn pc_mushroom_cleanup_excluded(
    bx: i32,
    bz: i32,
    player_bx: i32,
    player_bz: i32,
) -> bool {
    bx == player_bx && bz == player_bz
}

/// Cleanup eligibility: a mushroom item that is NOT buried.
#[no_mangle]
pub extern "C" fn pc_mushroom_clearable(item: i32, deposit_set: bool) -> bool {
    item == ITM_FOOD_MUSHROOM && !deposit_set
}

/// True when the "clear all candidates" branch is taken:
/// strict `total_candidate_num < clear_num` (equality
/// takes the share path).
#[no_mangle]
pub extern "C" fn pc_mushroom_clear_all(total_candidate_num: i32, clear_num: i32) -> bool {
    total_candidate_num < clear_num
}

/// The spread phase of `mMsr_SetShareNum` (verbatim): while
/// there are clearable mushrooms, candidate acres, and
/// `clear_num > candidate_block_num`, give one removal per
/// eligible acre. Mutates `share_num[0..30]` in place.
/// Writes (candidate_block_num, total_clearable, clear_num)
/// to `out[0..3]`; returns 1 on success.
#[no_mangle]
pub extern "C" fn pc_mushroom_share_spread(
    share_num: *mut u8,
    candidate_num: *const u8,
    candidate_block_num: i32,
    total_clearable: i32,
    clear_num: i32,
    out: *mut i32,
) -> u8 {
    if share_num.is_null() || candidate_num.is_null() || out.is_null() {
        return 0;
    }
    let share = unsafe { std::slice::from_raw_parts_mut(share_num, FG_BLOCK_TOTAL_NUM) };
    let cand = unsafe { std::slice::from_raw_parts(candidate_num, FG_BLOCK_TOTAL_NUM) };
    let mut blocks = candidate_block_num;
    let mut total = total_clearable;
    let mut clear = clear_num;
    while total != 0 && blocks != 0 && clear > blocks {
        let mut clearable = 0;
        for i in 0..FG_BLOCK_TOTAL_NUM {
            if cand[i] as i32 - share[i] as i32 > 0 {
                clearable += 1;
                share[i] = share[i].saturating_add(1);
                if cand[i] as i32 <= share[i] as i32 {
                    blocks -= 1;
                }
            }
        }
        if clearable == 0 {
            break;
        }
        clear -= clearable;
        total -= clearable;
    }
    let out = unsafe { std::slice::from_raw_parts_mut(out, 3) };
    out[0] = blocks;
    out[1] = total;
    out[2] = clear;
    1
}

/// One allocation step of the random phase of
/// `mMsr_SetShareNum`: given the already-sampled
/// `selected_block` index (the `fqrand()` call stays in C),
/// give one removal to that eligible acre. Returns true
/// when a share was assigned. Mutates `share_num[0..30]`;
/// writes (candidate_block_num, total_clearable, clear_num)
/// to `out[0..3]`; returns 1 on success.
#[no_mangle]
pub extern "C" fn pc_mushroom_share_random_step(
    share_num: *mut u8,
    candidate_num: *const u8,
    candidate_block_num: i32,
    total_clearable: i32,
    clear_num: i32,
    selected_block: i32,
    out: *mut i32,
) -> u8 {
    if share_num.is_null() || candidate_num.is_null() || out.is_null() {
        return 0;
    }
    let share = unsafe { std::slice::from_raw_parts_mut(share_num, FG_BLOCK_TOTAL_NUM) };
    let cand = unsafe { std::slice::from_raw_parts(candidate_num, FG_BLOCK_TOTAL_NUM) };
    let mut blocks = candidate_block_num;
    let mut total = total_clearable;
    let mut clear = clear_num;
    let mut sel = selected_block;
    let mut assigned = 0u8;
    for i in 0..FG_BLOCK_TOTAL_NUM {
        if cand[i] as i32 - share[i] as i32 > 0 {
            if sel == 0 {
                clear -= 1;
                share[i] = share[i].saturating_add(1);
                total -= 1;
                if cand[i] as i32 <= share[i] as i32 {
                    blocks -= 1;
                }
                assigned = 1;
                break;
            }
            sel -= 1;
        }
    }
    let out = unsafe { std::slice::from_raw_parts_mut(out, 3) };
    out[0] = blocks;
    out[1] = total;
    out[2] = clear;
    assigned
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_count() {
        // Quarter-hour schedule from 8:00.
        assert_eq!(pc_mushroom_target_count(8, 0, true, false), 5);
        assert_eq!(pc_mushroom_target_count(8, 14, true, false), 5);
        assert_eq!(pc_mushroom_target_count(8, 15, true, false), 4);
        assert_eq!(pc_mushroom_target_count(8, 30, true, false), 3);
        assert_eq!(pc_mushroom_target_count(8, 45, true, false), 2);
        assert_eq!(pc_mushroom_target_count(8, 59, true, false), 2);
        assert_eq!(pc_mushroom_target_count(9, 0, true, false), 1);
        assert_eq!(pc_mushroom_target_count(9, 14, true, false), 1);
        assert_eq!(pc_mushroom_target_count(9, 15, true, false), 0);
        // Due-gate.
        assert_eq!(pc_mushroom_target_count(8, 0, false, false), 0);
        assert_eq!(pc_mushroom_target_count(8, 0, false, true), 5);
        assert_eq!(pc_mushroom_target_count(10, 0, true, false), 0);
    }

    #[test]
    fn time_conversions() {
        let mut out = [0i32; 5];
        assert_eq!(pc_mushroom_rtc_to_time(2002, 10, 20, 8, 37, out.as_mut_ptr()), 1);
        assert_eq!(out, [2002, 10, 20, 8, 2]); // 37 / 15 = 2
        let mut out = [0i32; 6];
        assert_eq!(pc_mushroom_time_to_rtc(2002, 10, 20, 8, 2, out.as_mut_ptr()), 1);
        assert_eq!(out, [2002, 10, 20, 8, 30, 0]);
        assert_eq!(pc_mushroom_round_quarter(0), 0);
        assert_eq!(pc_mushroom_round_quarter(14), 0);
        assert_eq!(pc_mushroom_round_quarter(15), 15);
        assert_eq!(pc_mushroom_round_quarter(29), 15);
        assert_eq!(pc_mushroom_round_quarter(30), 30);
        assert_eq!(pc_mushroom_round_quarter(44), 30);
        assert_eq!(pc_mushroom_round_quarter(45), 45);
        assert_eq!(pc_mushroom_round_quarter(59), 45);
        assert!(pc_mushroom_is_zero_time(0, 0, 0, 0, 0, false));
        assert!(!pc_mushroom_is_zero_time(0, 0, 0, 0, 0, true));
        assert!(!pc_mushroom_is_zero_time(2002, 0, 0, 0, 0, false));
    }

    #[test]
    fn trees_and_cells() {
        for &id in ELIGIBLE_TREES.iter() {
            assert!(pc_mushroom_tree_eligible(id), "{id:#x}");
        }
        assert!(!pc_mushroom_tree_eligible(0x0800));
        assert!(!pc_mushroom_tree_eligible(ITM_FOOD_MUSHROOM));
        assert_eq!(AREA_OFFSETS, [-1, 1, 15, 16, 17]);
        assert!(pc_mushroom_boundary_ok(14, 14));
        assert!(!pc_mushroom_boundary_ok(15, 0));
        assert!(!pc_mushroom_boundary_ok(0, 15));
        assert_eq!(pc_mushroom_select_index(0.99, 5), 4);
        assert_eq!(pc_mushroom_select_index(0.0, 5), 0);
        assert_eq!(ITM_FOOD_MUSHROOM, ITM_FOOD_START + 5);
    }

    #[test]
    fn exclusions_and_cleanup() {
        // Placement: whole column and row excluded.
        assert!(pc_mushroom_placement_excluded(3, 0, 4, 4)); // same column
        assert!(pc_mushroom_placement_excluded(0, 3, 4, 4)); // same row
        assert!(!pc_mushroom_placement_excluded(0, 0, 4, 4)); // neither
        // Cleanup: only the single acre.
        assert!(pc_mushroom_cleanup_excluded(4, 4, 4, 4));
        assert!(!pc_mushroom_cleanup_excluded(4, 3, 4, 4));
        assert!(!pc_mushroom_cleanup_excluded(3, 4, 4, 4));
        assert!(pc_mushroom_clearable(0x2805, false));
        assert!(!pc_mushroom_clearable(0x2805, true)); // buried
        assert!(!pc_mushroom_clearable(0x2800, false));
        assert!(pc_mushroom_clear_all(3, 5));
        assert!(!pc_mushroom_clear_all(5, 5)); // equality takes share path
        assert!(!pc_mushroom_clear_all(6, 5));
    }

    #[test]
    fn share_spread() {
        // 3 acres with 2 candidates each, clear 5: spread gives 1 each per pass.
        let mut share = [0u8; 30];
        let cand = {
            let mut c = [0u8; 30];
            c[0] = 2;
            c[1] = 2;
            c[2] = 2;
            c
        };
        let mut out = [0i32; 3];
        assert_eq!(
            pc_mushroom_share_spread(share.as_mut_ptr(), cand.as_ptr(), 3, 6, 5, out.as_mut_ptr()),
            1
        );
        // First pass: clear(5) > blocks(3) -> each acre +1, clear = 2.
        // Second pass: clear(2) > blocks(3)? no -> loop ends.
        assert_eq!(&share[..3], &[1, 1, 1]);
        assert_eq!(out, [3, 3, 2]);
        // Random step: sample acre index 1 -> acre 1 gets another share.
        let mut out = [0i32; 3];
        assert_eq!(
            pc_mushroom_share_random_step(
                share.as_mut_ptr(),
                cand.as_ptr(),
                3,
                3,
                2,
                1,
                out.as_mut_ptr()
            ),
            1
        );
        assert_eq!(&share[..3], &[1, 2, 1]);
        assert_eq!(out, [2, 2, 1]); // acre 1 now full -> blocks 2
        // Exhausted acre is skipped by the selector.
        let mut out = [0i32; 3];
        assert_eq!(
            pc_mushroom_share_random_step(
                share.as_mut_ptr(),
                cand.as_ptr(),
                2,
                2,
                1,
                0,
                out.as_mut_ptr()
            ),
            1
        );
        assert_eq!(&share[..3], &[2, 2, 1]);
    }
}
