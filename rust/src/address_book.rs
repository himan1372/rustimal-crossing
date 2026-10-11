//! Address book contacts UI (m_address_ovl.c) pure kernels.
//!
//! Verified against GAFE01_00 Rev 0 decomp. The address book is
//! rebuilt on open; it is not a saved list of user contacts. The
//! player page holds the other valid player identities plus an
//! optional museum entry; the two villager pages hold animals
//! that have a memory entry for the current player.
//!
//! The following stay in C until Rust/C equivalence is proven:
//! `Save_Get`/`Now_Private` access, `mPr_NullCheckPersonalID`,
//! `mPr_CopyPersonalID`, museum eligibility and
//! `mMsm_GetMuseumMailName`, NPC memory checks,
//! `mMl_set_mail_name_npcinfo`, `mMl_copy_header_name`,
//! `mMl_strlen`, the shared editor and board overlays, sounds,
//! display lists, transitions, `chase_f` easing, static overlay
//! allocation, and the editor display-list handoff.

#![allow(non_snake_case)]

use std::os::raw::c_int;

/// Page indices (`m_address_ovl.h`).
pub const AD_PAGE_PLAYER: i32 = 0;
pub const AD_PAGE_NPC0: i32 = 1;
pub const AD_PAGE_NPC1: i32 = 2;
pub const AD_PAGE_NUM: i32 = 3;
/// Villager entries per NPC page (`mAD_PAGE_MAX_ENTRIES`).
pub const AD_PAGE_MAX_ENTRIES: i32 = 8;
/// Overlay procedures (`m_address_ovl.h`).
pub const AD_PROC_REFUSE: i32 = 0;
pub const AD_PROC_START: i32 = 1;
pub const AD_PROC_SELECT: i32 = 2;
pub const AD_PROC_TURN_PAGE: i32 = 3;
pub const AD_PROC_TURN_PAGE2: i32 = 4;
/// Open modes.
pub const AD_OPEN_ADDRESS: i32 = 0;
pub const AD_OPEN_WAIT: i32 = 1;
/// Maximum saved player slots.
pub const AD_PLAYER_NUM: i32 = 4;

/// `mAD_make_player_address` (`m_address_ovl.c`): counts the other
/// valid player identities in ascending slot order, skipping the
/// active slot. `valid[i]` is the pre-fetched
/// `mPr_NullCheckPersonalID` result for slot `i`. When `museum_ok`
/// is nonzero the museum entry is appended after the players.
/// Returns the player-page entry count; `out_museum_shown` reports
/// the `show_museum_address` flag (explicitly FALSE when the
/// museum is ineligible).
#[no_mangle]
pub extern "C" fn pc_ad_player_list(
    active_slot: c_int,
    valid: *const c_int,
    museum_ok: c_int,
    out_museum_shown: *mut c_int,
) -> c_int {
    if valid.is_null() {
        return 0;
    }
    let valid = unsafe { std::slice::from_raw_parts(valid, AD_PLAYER_NUM as usize) };
    let mut count = 0;
    for i in 0..AD_PLAYER_NUM {
        if i == active_slot {
            continue;
        }
        if valid[i as usize] != 0 {
            count += 1;
        }
    }
    let museum_shown = if museum_ok != 0 { 1 } else { 0 };
    if museum_shown != 0 {
        count += 1;
    }
    if !out_museum_shown.is_null() {
        unsafe {
            *out_museum_shown = museum_shown;
        }
    }
    count
}

/// `mAD_make_npc_address` paging (`m_address_ovl.c`): splits the
/// accepted villager count into the two NPC pages. The first eight
/// names occupy page 0; the remainder go into page 1.
#[no_mangle]
pub extern "C" fn pc_ad_npc_pages(
    accepted: c_int,
    out_npc0: *mut c_int,
    out_npc1: *mut c_int,
) {
    let (npc0, npc1) = if accepted < AD_PAGE_MAX_ENTRIES {
        (accepted, 0)
    } else {
        (AD_PAGE_MAX_ENTRIES, accepted - AD_PAGE_MAX_ENTRIES)
    };
    if !out_npc0.is_null() {
        unsafe {
            *out_npc0 = npc0;
        }
    }
    if !out_npc1.is_null() {
        unsafe {
            *out_npc1 = npc1;
        }
    }
}

/// `mAD_address_ovl_init` page model (`m_address_ovl.c`): builds
/// the page bitfield (bit `i` = page `i` nonempty), the page count,
/// the initial page, and the starting procedure. `counts` holds
/// the three page entry counts. The initial page is the player
/// page when nonempty, else NPC page 0 when nonempty; when both
/// are empty `curIdx` keeps its cleared value (0) and the overlay
/// enters `mAD_PROC_REFUSE`.
#[no_mangle]
pub extern "C" fn pc_ad_page_model(
    counts: *const c_int,
    out_bitfield: *mut c_int,
    out_page_count: *mut c_int,
    out_init_page: *mut c_int,
    out_proc: *mut c_int,
) {
    if counts.is_null() {
        return;
    }
    let counts = unsafe { std::slice::from_raw_parts(counts, AD_PAGE_NUM as usize) };
    let init_page = if counts[AD_PAGE_PLAYER as usize] != 0 {
        AD_PAGE_PLAYER
    } else if counts[AD_PAGE_NPC0 as usize] != 0 {
        AD_PAGE_NPC0
    } else {
        0
    };
    let mut bitfield = 0;
    let mut count = 0;
    for i in (0..AD_PAGE_NUM).rev() {
        bitfield <<= 1;
        if counts[i as usize] != 0 {
            bitfield |= 1;
            count += 1;
        }
    }
    let proc = if count == 0 { AD_PROC_REFUSE } else { AD_PROC_START };
    if !out_bitfield.is_null() {
        unsafe {
            *out_bitfield = bitfield;
        }
    }
    if !out_page_count.is_null() {
        unsafe {
            *out_page_count = count;
        }
    }
    if !out_init_page.is_null() {
        unsafe {
            *out_init_page = init_page;
        }
    }
    if !out_proc.is_null() {
        unsafe {
            *out_proc = proc;
        }
    }
}

/// `mAD_get_nextIdx` (`m_address_ovl.c`): page-turn target.
/// Returns -1 when there is only one nonempty page; otherwise
/// advances `add` (±1) with wrap (3→0, <0→2), trying at most two
/// candidates and stopping at the first set bit in `bitfield`.
#[no_mangle]
pub extern "C" fn pc_ad_get_next_idx(
    bitfield: c_int,
    page_count: c_int,
    idx: c_int,
    add: c_int,
) -> c_int {
    let mut next_idx = idx;
    if page_count <= 1 {
        next_idx = -1;
    } else {
        for _ in 0..2 {
            next_idx += add;
            if next_idx == 3 {
                next_idx = 0;
            } else if next_idx < 0 {
                next_idx = 2;
            }
            if ((bitfield >> next_idx) & 1) != 0 {
                break;
            }
        }
    }
    next_idx
}

/// `mAD_select_proc` recipient resolution (`m_address_ovl.c`):
/// maps `(cur_idx, selected_entry)` to the backing array and
/// index. Returns the array selector (0 = player_mail_name,
/// 1 = animal_mail_name) and writes the index: player page →
/// `selected`; NPC page 1 → `8 + selected`; otherwise → `selected`.
/// The branch compares against the raw 0/2 values, as in the C.
#[no_mangle]
pub extern "C" fn pc_ad_resolve(
    cur_idx: c_int,
    selected_entry: c_int,
    out_index: *mut c_int,
) -> c_int {
    let (array, index) = if cur_idx == 0 {
        (0, selected_entry)
    } else if cur_idx == 2 {
        (1, AD_PAGE_MAX_ENTRIES + selected_entry)
    } else {
        (1, selected_entry)
    };
    if !out_index.is_null() {
        unsafe {
            *out_index = index;
        }
    }
    array
}

/// `mAD_select_proc` B behavior (`m_address_ovl.c`): returns 1
/// when B cancels to the previous overlay (`first == TRUE`),
/// 0 when B returns focus to the mail header (`header_pos = 0`
/// + WAIT).
#[no_mangle]
pub extern "C" fn pc_ad_b_cancel(first: c_int) -> c_int {
    (first != 0) as c_int
}

/// `mAD_move_Wait` page reset (`m_address_ovl.c`): when the board's
/// `header_pos` reaches 1, the address overlay returns to PLAY
/// with the cursor reset and the page re-initialized to the
/// player page when available, else NPC page 0. Returns 1 when
/// the reset applies (`header_pos == 1`).
#[no_mangle]
pub extern "C" fn pc_ad_wait_reset(
    header_pos: c_int,
    player_count: c_int,
    npc0_count: c_int,
    out_page: *mut c_int,
) -> c_int {
    if header_pos != 1 {
        return 0;
    }
    let page = if player_count != 0 {
        AD_PAGE_PLAYER
    } else if npc0_count != 0 {
        AD_PAGE_NPC0
    } else {
        0
    };
    if !out_page.is_null() {
        unsafe {
            *out_page = page;
        }
    }
    1
}

/// `mAD_pile_init` (`m_address_ovl.c`): page-stack targets. Two
/// pages → idx==0 gives {0,5,0}, else {5,0,5}; three pages →
/// {0,5,10} rotated so page `idx` is at 0; otherwise all zero.
#[no_mangle]
pub extern "C" fn pc_ad_pile_init(page_count: c_int, idx: c_int, out: *mut f32) {
    if out.is_null() {
        return;
    }
    let out = unsafe { std::slice::from_raw_parts_mut(out, AD_PAGE_NUM as usize) };
    if page_count == 2 {
        if idx == 0 {
            out.copy_from_slice(&[0.0, 5.0, 0.0]);
        } else {
            out.copy_from_slice(&[5.0, 0.0, 5.0]);
        }
    } else if page_count == 3 {
        out[idx as usize % 3] = 0.0;
        out[(idx as usize + 1) % 3] = 5.0;
        out[(idx as usize + 2) % 3] = 10.0;
    } else {
        out.copy_from_slice(&[0.0, 0.0, 0.0]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn player_list_rules() {
        let mut shown = 0;
        // slots 0..4, slot 1 active; slots 0,2 valid
        let valid = [1, 1, 1, 0];
        let mut count = pc_ad_player_list(1, valid.as_ptr(), 0, &mut shown);
        assert_eq!((count, shown), (2, 0)); // slots 0 and 2
        // museum eligible -> appended
        count = pc_ad_player_list(1, valid.as_ptr(), 1, &mut shown);
        assert_eq!((count, shown), (3, 1));
        // active player never appears even when valid
        let valid = [1, 1, 1, 1];
        count = pc_ad_player_list(0, valid.as_ptr(), 0, &mut shown);
        assert_eq!(count, 3);
        // empty slots excluded
        let valid = [0, 0, 0, 0];
        count = pc_ad_player_list(3, valid.as_ptr(), 1, &mut shown);
        assert_eq!((count, shown), (1, 1)); // museum only
    }

    #[test]
    fn npc_paging() {
        let (mut n0, mut n1) = (0, 0);
        pc_ad_npc_pages(7, &mut n0, &mut n1);
        assert_eq!((n0, n1), (7, 0));
        pc_ad_npc_pages(8, &mut n0, &mut n1);
        assert_eq!((n0, n1), (8, 0));
        pc_ad_npc_pages(9, &mut n0, &mut n1);
        assert_eq!((n0, n1), (8, 1));
        pc_ad_npc_pages(15, &mut n0, &mut n1);
        assert_eq!((n0, n1), (8, 7));
    }

    #[test]
    fn page_model_rules() {
        let (mut bf, mut pc, mut ip, mut pr) = (0, 0, 0, 0);
        // all pages nonempty
        let counts = [3, 8, 2];
        pc_ad_page_model(counts.as_ptr(), &mut bf, &mut pc, &mut ip, &mut pr);
        assert_eq!((bf, pc, ip, pr), (0b111, 3, AD_PAGE_PLAYER, AD_PROC_START));
        // empty player page -> start on NPC0
        let counts = [0, 5, 0];
        pc_ad_page_model(counts.as_ptr(), &mut bf, &mut pc, &mut ip, &mut pr);
        assert_eq!((bf, pc, ip, pr), (0b010, 1, AD_PAGE_NPC0, AD_PROC_START));
        // empty middle page -> bit unset
        let counts = [2, 0, 4];
        pc_ad_page_model(counts.as_ptr(), &mut bf, &mut pc, &mut ip, &mut pr);
        assert_eq!((bf, pc, ip, pr), (0b101, 2, AD_PAGE_PLAYER, AD_PROC_START));
        // everything empty -> refusal, init page stays 0
        let counts = [0, 0, 0];
        pc_ad_page_model(counts.as_ptr(), &mut bf, &mut pc, &mut ip, &mut pr);
        assert_eq!((bf, pc, ip, pr), (0, 0, 0, AD_PROC_REFUSE));
    }

    #[test]
    fn next_idx_rules() {
        // single page -> -1
        assert_eq!(pc_ad_get_next_idx(0b010, 1, 1, 1), -1);
        // advance with wrap
        assert_eq!(pc_ad_get_next_idx(0b111, 3, 2, 1), 0);
        assert_eq!(pc_ad_get_next_idx(0b111, 3, 0, -1), 2);
        // skip empty middle page
        assert_eq!(pc_ad_get_next_idx(0b101, 2, 0, 1), 2);
        assert_eq!(pc_ad_get_next_idx(0b101, 2, 2, -1), 0);
        // two candidates tried at most: from 0 backward with only page 0 set
        assert_eq!(pc_ad_get_next_idx(0b001, 1, 0, -1), -1);
    }

    #[test]
    fn resolve_rules() {
        let mut idx = 0;
        assert_eq!(pc_ad_resolve(0, 3, &mut idx), 0);
        assert_eq!(idx, 3);
        assert_eq!(pc_ad_resolve(2, 1, &mut idx), 1);
        assert_eq!(idx, 9); // 8 + 1
        assert_eq!(pc_ad_resolve(1, 5, &mut idx), 1);
        assert_eq!(idx, 5);
    }

    #[test]
    fn cancel_and_wait() {
        assert_eq!(pc_ad_b_cancel(1), 1);
        assert_eq!(pc_ad_b_cancel(0), 0);
        let mut page = -1;
        assert_eq!(pc_ad_wait_reset(1, 2, 5, &mut page), 1);
        assert_eq!(page, AD_PAGE_PLAYER);
        assert_eq!(pc_ad_wait_reset(1, 0, 5, &mut page), 1);
        assert_eq!(page, AD_PAGE_NPC0);
        assert_eq!(pc_ad_wait_reset(2, 2, 5, &mut page), 0);
    }

    #[test]
    fn pile_layouts() {
        let mut out = [0.0f32; 3];
        pc_ad_pile_init(2, 0, out.as_mut_ptr());
        assert_eq!(out, [0.0, 5.0, 0.0]);
        pc_ad_pile_init(2, 1, out.as_mut_ptr());
        assert_eq!(out, [5.0, 0.0, 5.0]);
        pc_ad_pile_init(3, 0, out.as_mut_ptr());
        assert_eq!(out, [0.0, 5.0, 10.0]);
        pc_ad_pile_init(3, 2, out.as_mut_ptr());
        assert_eq!(out, [5.0, 10.0, 0.0]);
        pc_ad_pile_init(1, 0, out.as_mut_ptr());
        assert_eq!(out, [0.0, 0.0, 0.0]);
    }
}
