//! Post office clerks: Pelly, Phyllis, and Pete — pure data and kernels.
//!
//! Verified against `src/game/m_post_office.c`, `src/game/m_npc.c`,
//! `src/actor/npc/ac_npc_post_girl.c`,
//! `src/actor/npc/ac_npc_post_girl.c_inc`,
//! `src/actor/npc/ac_npc_post_man.c`,
//! `src/actor/npc/ac_npc_post_man_move.c_inc`,
//! `src/actor/ac_aprilfool_control.c`,
//! `include/m_post_office.h`, `include/m_name_table.h`,
//! `include/ac_npc_post_girl.h`, `include/ac_npc_post_man.h`
//! (GAFE01_00 Rev 0 USA decomp).
//! All 14 brief claims checked verbatim; no corrections.
//!
//! Architecture: the counter state machine (30 actions), the mail
//! subsystem (`mPO_receipt_proc`, delivery processing, the post-office
//! queue), Pete's movement/delivery actor, the text-entry and submenu
//! UI, the loan-balance font formatting, and the April Fools overrides
//! all stay in C. This module holds only the source-proven pure data
//! and calculations with primitive arguments.
//!
//! Warning carried from the brief: `status` is BOTH a bitmask and an
//! array index — do not replace it with an unrelated encoding.

/// Clerk/postman identities (`include/m_name_table.h`).
pub mod id {
    /// Pelly, the daytime clerk.
    pub const POST_GIRL: u16 = 0xD003;
    /// Phyllis, the nighttime clerk.
    pub const POST_GIRL2: u16 = 0xD012;
    /// Pete, the delivery postman.
    pub const POST_MAN: u16 = 0xD011;
}

/// NPC name indices (`m_npc.c`).
pub mod name_idx {
    pub const PELLY: u16 = 0x20D;
    pub const PHYLLIS: u16 = 0x20E;
    pub const PETE: u16 = 0x20F;
}

/// Clerk status bits (`include/ac_npc_post_girl.h`).
pub mod status {
    pub const NONE: u8 = 0;
    pub const DESK_FULL: u8 = 1 << 0;
    pub const DONE_FIRST_JOB: u8 = 1 << 1;
    pub const HAS_BANK_ACCOUNT: u8 = 1 << 2;
}

/// Post-office storage capacities.
pub mod cap {
    /// Five `Mail_c` slots (`m_post_office.h`).
    pub const MAIL_STORAGE_SIZE: u8 = 5;
    /// Player mailbox size (`m_home_h.h`).
    pub const HOME_MAILBOX_SIZE: u8 = 10;
}

/// Destination-validation outcomes (`aPG_check_destination`).
pub mod dest {
    pub const OK: u8 = 0;
    /// Player recipient has no house match, or NPC identity lookup fails.
    pub const NO_RECIPIENT: u8 = 1;
    /// Target player mailbox has no free slot.
    pub const MAILBOX_FULL: u8 = 2;
    /// Combined mailbox+queue count at capacity.
    pub const COMBINED_FULL: u8 = 3;
}

/// Pete's April Fools data indices (`ac_aprilfool_control.c`).
pub mod april {
    pub const PELLY: u16 = 0x0007;
    pub const PHYLLIS: u16 = 0x0008;
    pub const PETE: u16 = 0x0009;
}

/// Clerk status computation (`aPG_set_post_status`), verbatim logic.
/// The decomp also calls `aPG_set_loan_balance()` inside the
/// DONE_FIRST_JOB branch; that font formatting stays in C.
/// `status` is both bitmask and 8-entry-table index.
pub fn clerk_status(desk_full: bool, loan: u32, first_job_done: bool, has_bank_account: bool) -> u8 {
    let mut status: u8 = status::NONE;
    if desk_full {
        status |= status::DESK_FULL;
    }
    if loan != 0 {
        if !first_job_done {
            status |= status::DONE_FIRST_JOB;
        }
    } else if has_bank_account {
        status |= status::HAS_BANK_ACCOUNT;
    }
    status
}

/// Opening greeting (`aPG_set_talk_info`): `msg_no[status] + draw_type`.
/// `draw_type` is 0 for Pelly, 1 for Phyllis
/// (`npc_id == SP_NPC_POST_GIRL ? 0 : 1`).
pub const GREET_MSG: [u16; 8] = [0x31D3, 0x31D1, 0x08B1, 0x08AD, 0x08D1, 0x08CF, 0x08B1, 0x08AD];

pub fn greet_msg(status: u8, draw_type: u8) -> u16 {
    GREET_MSG[(status % 8) as usize] + draw_type as u16
}

/// Draw-type offset: Pelly 0, Phyllis 1.
pub fn draw_type(npc_id: u16) -> u8 {
    if npc_id == id::POST_GIRL {
        0
    } else {
        1
    }
}

/// Second status-indexed table, for the repay-after path
/// (`aPG_repay_after_init`): passed directly as
/// `msg_no[postgirl->status]` — do not confuse with GREET_MSG.
pub const REPAY_AFTER_MSG: [u16; 8] = [0x31D7, 0x31D5, 0x08DB, 0x08D9, 0x08DF, 0x08DD, 0x08DB, 0x08D9];

pub fn repay_after_msg(status: u8) -> u16 {
    REPAY_AFTER_MSG[(status % 8) as usize]
}

/// Business-menu choice adjustment (`aPG_ask_for_business`), verbatim:
/// an EXACT-VALUE switch on status — only NONE (0) and DESK_FULL (1)
/// get the adjustment; status 3 (DESK_FULL|DONE_FIRST_JOB) does NOT.
/// `mChoice_CHOICE0` is the first choice enum, i.e. 0.
pub fn adjust_choice(choice: u8, status: u8) -> u8 {
    match status {
        status::NONE | status::DESK_FULL => {
            if choice > 0 {
                choice + 1
            } else {
                choice
            }
        }
        _ => choice,
    }
}

/// Refusal messages (`aPG_refuse_msg_end_wait_init`), indexed by
/// `dest - 1`. `aPG_ChangeMsgData` adds `draw_type`, so Phyllis's IDs
/// differ by 1 — same offset mechanism as the greeting.
pub const REFUSE_MSG: [u16; 3] = [0x08E1, 0x08B5, 0x1BD7];

pub fn refuse_msg(dest: u8) -> u16 {
    REFUSE_MSG[((dest - 1) % 3) as usize]
}

/// The receive path's own 5-entry table (`aPG_receive_menu_close_wait`),
/// selected with `next = dest + 1` (dest 1→0x8e1, 2→0x8b5, 3→0x1bd7).
/// Same three refusal IDs embedded in a wider table — keep separate.
pub const RECEIVE_MSG: [u16; 5] = [0x08E3, 0x08B9, 0x08E1, 0x08B5, 0x1BD7];

pub fn receive_msg(next: u8) -> u16 {
    RECEIVE_MSG[(next % 5) as usize]
}

/// Bank-account eligibility (`aPG_actor_ct`), verbatim: the player must
/// be NATIVE, have no outstanding loan, own a house of size >= 3, and
/// `renew == FALSE`. Do not simplify to "no loan".
pub fn bank_account_eligible(is_native: bool, loan: u32, house_size: u8, renew: bool) -> bool {
    is_native && loan == 0 && house_size >= 3 && !renew
}

/// Loan-balance split (`aPG_set_loan_balance`): millions go to FREE_STR1
/// with a comma byte, the remainder to FREE_STR2. The formatting itself
/// (`mFont_UnintToString`) and the free-string setters stay in C.
///
/// Nuance carried from the decomp: the comma is written at `str[l]` but
/// `mMsg_Set_free_str(..., str, l)` receives length `l` — the comma byte
/// sits OUTSIDE the copied length. Do not "fix" this by passing `l + 1`.
pub fn loan_millions(loan: u32) -> u32 {
    loan / 1_000_000
}

pub fn loan_remainder(loan: u32) -> u32 {
    loan - loan_millions(loan) * 1_000_000
}

/// Delivery schedule (`mPO_set_next_delivery_time`), verbatim:
/// hour < 9 → 09:00; 9 <= hour < 17 → 17:00; hour >= 17 → next-day
/// 09:00. Minutes and seconds are zeroed. Returns (hour, next_day).
pub fn next_delivery_time(hour: u8) -> (u8, bool) {
    if hour < 9 {
        (9, false)
    } else if hour < 17 {
        (17, false)
    } else {
        (9, true)
    }
}

/// Pete's dialogue tables (`aPMAN_set_talk_info_talk_check`):
/// talk_type 0 → `0x08BF + RANDOM(12.0f)`; talk_type 1 → `0x08CB +
/// RANDOM(4.0f)`. The RANDOM rolls and the April Fools override stay
/// in C. `talk_type = 1` is set at talk end.
pub const PETE_MSG_BASE: [u16; 2] = [0x08BF, 0x08CB];
pub const PETE_RND_BOUND: [f32; 2] = [12.0, 4.0];

pub fn pete_msg_base(talk_type: u8) -> u16 {
    PETE_MSG_BASE[(talk_type % 2) as usize]
}

pub fn pete_rnd_bound(talk_type: u8) -> f32 {
    PETE_RND_BOUND[(talk_type % 2) as usize]
}

/// Pete's talk permission by action (`aPMAN_set_talk_permit`): TRUE only
/// while looking around (KYORO), turning, and walking.
pub const PETE_TALK_PERMIT: [bool; 14] = [
    false, // ACT_ENTER
    false, // ACT_BREAK
    false, // ACT_HOVER
    false, // ACT_LANDING
    true,  // ACT_KYORO
    true,  // ACT_TURN
    true,  // ACT_WALK
    false, // ACT_DELIVERY_TURN
    false, // ACT_DELIVERY
    false, // ACT_TALK_TURN
    false, // ACT_TALK_END_WAIT
    false, // ACT_FLY_UP
    false, // ACT_EXIT
    false, // ACT_EXIT2
];

pub fn pete_talk_permit(action: u8) -> bool {
    PETE_TALK_PERMIT.get(action as usize).copied().unwrap_or(false)
}

/// Pete's birth-type-indexed start values (`aPMAN_actor_ct`):
/// PLAYER_HOUSE (0) starts at Y 160.0 with ACT_ENTER;
/// POST_OFFICE (1) starts at Y 0.0 with ACT_FLY_UP.
/// Action IDs 0=ENTER, 11=FLY_UP in the actor's action enum.
pub mod pete_birth {
    pub const PLAYER_HOUSE: u8 = 0;
    pub const POST_OFFICE: u8 = 1;
    pub const START_YPOS: [f32; 2] = [160.0, 0.0];
    /// ACT_ENTER = 0, ACT_FLY_UP = 11.
    pub const START_ACTION: [u8; 2] = [0, 11];
}

pub fn pete_start_ypos(birth_type: u8) -> f32 {
    pete_birth::START_YPOS[(birth_type % 2) as usize]
}

pub fn pete_start_action(birth_type: u8) -> u8 {
    pete_birth::START_ACTION[(birth_type % 2) as usize]
}

// ---- C ABI ----
// All pc_-prefixed: the C originals stay compiled; these are staged kernels.

#[no_mangle]
pub extern "C" fn pc_post_clerk_status(
    desk_full: u8,
    loan: u32,
    first_job_done: u8,
    has_bank_account: u8,
) -> u8 {
    clerk_status(desk_full != 0, loan, first_job_done != 0, has_bank_account != 0)
}

#[no_mangle]
pub extern "C" fn pc_post_greet_msg(status: u8, draw_type: u8) -> u16 {
    greet_msg(status, draw_type)
}

#[no_mangle]
pub extern "C" fn pc_post_repay_after_msg(status: u8) -> u16 {
    repay_after_msg(status)
}

#[no_mangle]
pub extern "C" fn pc_post_adjust_choice(choice: u8, status: u8) -> u8 {
    adjust_choice(choice, status)
}

#[no_mangle]
pub extern "C" fn pc_post_refuse_msg(dest: u8) -> u16 {
    refuse_msg(dest)
}

#[no_mangle]
pub extern "C" fn pc_post_bank_account_eligible(
    is_native: u8,
    loan: u32,
    house_size: u8,
    renew: u8,
) -> u8 {
    bank_account_eligible(is_native != 0, loan, house_size, renew != 0) as u8
}

#[no_mangle]
pub extern "C" fn pc_post_next_delivery_hour(hour: u8) -> u8 {
    next_delivery_time(hour).0
}

#[no_mangle]
pub extern "C" fn pc_post_next_delivery_next_day(hour: u8) -> u8 {
    next_delivery_time(hour).1 as u8
}

#[no_mangle]
pub extern "C" fn pc_post_pete_msg_base(talk_type: u8) -> u16 {
    pete_msg_base(talk_type)
}

#[no_mangle]
pub extern "C" fn pc_post_pete_talk_permit(action: u8) -> u8 {
    pete_talk_permit(action) as u8
}

#[no_mangle]
pub extern "C" fn pc_post_draw_type(npc_id: u16) -> u8 {
    draw_type(npc_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identities() {
        assert_eq!(id::POST_GIRL, 0xD003);
        assert_eq!(id::POST_MAN, 0xD011);
        assert_eq!(id::POST_GIRL2, 0xD012);
        assert_eq!(name_idx::PELLY, 0x20D);
        assert_eq!(name_idx::PHYLLIS, 0x20E);
        assert_eq!(name_idx::PETE, 0x20F);
    }

    #[test]
    fn status_math() {
        assert_eq!(clerk_status(false, 0, true, false), status::NONE);
        assert_eq!(clerk_status(true, 0, true, false), status::DESK_FULL);
        assert_eq!(clerk_status(false, 100, false, false), status::DONE_FIRST_JOB);
        // loan nonzero but first job done -> no DONE_FIRST_JOB bit
        assert_eq!(clerk_status(false, 100, true, false), status::NONE);
        assert_eq!(clerk_status(false, 0, true, true), status::HAS_BANK_ACCOUNT);
        // desk full + done first job
        assert_eq!(
            clerk_status(true, 100, false, false),
            status::DESK_FULL | status::DONE_FIRST_JOB
        );
        // has_bank_account ignored when loan nonzero
        assert_eq!(clerk_status(false, 100, false, true), status::DONE_FIRST_JOB);
    }

    #[test]
    fn greetings() {
        assert_eq!(greet_msg(0, 0), 0x31D3); // Pelly, no status
        assert_eq!(greet_msg(0, 1), 0x31D4); // Phyllis, no status
        assert_eq!(greet_msg(4, 1), 0x08D2); // Phyllis, bank account
        assert_eq!(draw_type(id::POST_GIRL), 0);
        assert_eq!(draw_type(id::POST_GIRL2), 1);
        assert_eq!(repay_after_msg(0), 0x31D7);
        assert_eq!(repay_after_msg(7), 0x08D9);
    }

    #[test]
    fn choice_adjust() {
        // exact-value switch: only status 0 and 1 adjust
        assert_eq!(adjust_choice(1, 0), 2);
        assert_eq!(adjust_choice(2, 0), 3);
        assert_eq!(adjust_choice(0, 0), 0); // choice 0 not adjusted
        assert_eq!(adjust_choice(1, 1), 2);
        assert_eq!(adjust_choice(1, 2), 1); // DONE_FIRST_JOB: no adjustment
        assert_eq!(adjust_choice(1, 3), 1); // DESK_FULL|DONE_FIRST_JOB: no adjustment
        assert_eq!(adjust_choice(1, 4), 1); // HAS_BANK_ACCOUNT: no adjustment
    }

    #[test]
    fn refusals() {
        assert_eq!(refuse_msg(1), 0x08E1);
        assert_eq!(refuse_msg(2), 0x08B5);
        assert_eq!(refuse_msg(3), 0x1BD7);
        assert_eq!(receive_msg(0), 0x08E3);
        assert_eq!(receive_msg(2), 0x08E1);
        assert_eq!(receive_msg(4), 0x1BD7);
    }

    #[test]
    fn bank_init() {
        assert!(bank_account_eligible(true, 0, 3, false));
        assert!(!bank_account_eligible(false, 0, 3, false)); // not native
        assert!(!bank_account_eligible(true, 100, 3, false)); // loan
        assert!(!bank_account_eligible(true, 0, 2, false)); // house too small
        assert!(!bank_account_eligible(true, 0, 3, true)); // renew
    }

    #[test]
    fn loan_split() {
        assert_eq!(loan_millions(1_900_000), 1);
        assert_eq!(loan_remainder(1_900_000), 900_000);
        assert_eq!(loan_millions(500_000), 0);
        assert_eq!(loan_remainder(2_000_000), 0);
    }

    #[test]
    fn delivery() {
        assert_eq!(next_delivery_time(8), (9, false));
        assert_eq!(next_delivery_time(9), (17, false));
        assert_eq!(next_delivery_time(16), (17, false));
        assert_eq!(next_delivery_time(17), (9, true));
        assert_eq!(next_delivery_time(23), (9, true));
    }

    #[test]
    fn pete() {
        assert_eq!(pete_msg_base(0), 0x08BF);
        assert_eq!(pete_msg_base(1), 0x08CB);
        assert_eq!(pete_rnd_bound(0), 12.0);
        assert_eq!(pete_rnd_bound(1), 4.0);
        assert!(pete_talk_permit(4)); // KYORO
        assert!(pete_talk_permit(5)); // TURN
        assert!(pete_talk_permit(6)); // WALK
        assert!(!pete_talk_permit(0)); // ENTER
        assert!(!pete_talk_permit(8)); // DELIVERY
        assert!(!pete_talk_permit(11)); // FLY_UP
        assert_eq!(pete_start_ypos(0), 160.0);
        assert_eq!(pete_start_ypos(1), 0.0);
        assert_eq!(pete_start_action(0), 0); // ENTER
        assert_eq!(pete_start_action(1), 11); // FLY_UP
        assert_eq!(april::PELLY, 0x0007);
        assert_eq!(april::PHYLLIS, 0x0008);
        assert_eq!(april::PETE, 0x0009);
        assert_eq!(cap::MAIL_STORAGE_SIZE, 5);
        assert_eq!(cap::HOME_MAILBOX_SIZE, 10);
    }
}
