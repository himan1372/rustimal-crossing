//! Museum fossil assessment + donation kernels.
//!
//! Verified against `src/game/m_museum.c`, `src/game/m_museum_display.c`,
//! `src/actor/npc/ac_npc_curator_move.c_inc` (GAFE01_00 Rev 0 USA decomp).
//!
//! Architecture: the mail queue, saved museum record (`museum_record`),
//! display bitfields, RNG (`RANDOM`), and all donation/inventory side
//! effects stay in C. This module holds the pure table-lookup and
//! classification kernels with primitive arguments.
//!
//! The retail flow, for reference:
//! 1. Player digs up `ITM_FOSSIL` (generic, 0x2511) and mails it to the
//!    Museum (`mMl_NAME_TYPE_MUSEUM`).
//! 2. `mMsm_SendMuseumMail` increments `stored_fossil_num` (cap 30).
//! 3. At the daily 6 AM grow tick, `mMsm_DepositFossil` calls
//!    `mMsm_SendResultMail`, which round-robins fossil replies (max 3
//!    mails/player/pass). Each reply carries a *different*, randomly
//!    selected identified fossil — the mailed generic fossil is never
//!    transformed in place.
//! 4. Donating the identified fossil to Blathers is a separate operation
//!    via `mMmd_RequestMuseumDisplay`; inventory is cleared only on success.

/// Museum constants (`include/m_museum.h`).
pub mod limit {
    /// Max stored fossil submissions / remail slots.
    pub const REMAIL_SLOTS: u8 = 30;
    /// Max museum response mails per player per result pass.
    pub const MAX_MAIL: u8 = 3;
    /// Max buried fossils the daily generator keeps.
    pub const DEPOSIT_FOSSIL_MAX: u8 = 5;
}

/// Fossil furniture range (`FTR_DINO_START`..`FTR_DINO_END`).
pub mod range {
    /// `FTR_START(FTR_DIN_TRIKERA_HEAD)`.
    pub const DINO_START: u16 = 0x1EEC;
    /// `FTR_END(FTR_DIN_TRILOBITE)`.
    pub const DINO_END: u16 = 0x1F4F;
    /// Generic buried fossil item.
    pub const ITM_FOSSIL: u16 = 0x2511;
    /// Number of fossil exhibits.
    pub const FOSSIL_NUM: u8 = 25;
}

/// Museum-display classification (`m_museum_display.h`).
pub mod display {
    pub const CANNOT_DONATE: u8 = 0;
    pub const CAN_DONATE: u8 = 1;
    pub const ALREADY_DONATED: u8 = 2;
}

/// Donator ids, 4 bits per fossil (`m_museum_display.h`).
pub mod donator {
    pub const NONE: u8 = 0;
    pub const PLAYER1: u8 = 1;
    pub const PLAYER2: u8 = 2;
    pub const PLAYER3: u8 = 3;
    pub const PLAYER4: u8 = 4;
    pub const DELETED_PLAYER: u8 = 5;
}

/// Remail kinds (`m_museum.h`).
pub mod remail_kind {
    pub const CLEAR: u8 = 0;
    pub const CANNOT_DONATE: u8 = 1;
    pub const DONATED: u8 = 2;
    pub const ALREADY_DONATED: u8 = 3;
    pub const FOREIGNER: u8 = 4;
}

/// Reply-message table for fossil mail (`mMsm_GetFossilMailNo`).
/// Order is deliberately non-sequential in the decomp — preserved exactly.
const FOSSIL_MAIL_NO_TABLE: [u16; 25] = [
    0x10E, 0x110, 0x10F, 0x111, 0x113, 0x112, 0x114, 0x116, 0x115, 0x117,
    0x119, 0x118, 0x11A, 0x11B, 0x11C, 0x11D, 0x11E, 0x11F, 0x120, 0x121,
    0x126, 0x125, 0x123, 0x124, 0x122,
];

/// Reply-message table for remail kinds (`mMsm_SendResultMail`).
/// Indexed by `kind - 1`.
const REMAIL_NO_TABLE: [u16; 4] = [0x22D, 0x22B, 0x22C, 0x22E];

/// Skeleton-complete message per dinosaur group
/// (`aCR_chk_fossil_parts_complete`); default 0x2F84 when incomplete.
const SKELETON_MSG_TABLE: [u16; 7] =
    [0x2F78, 0x2F79, 0x2F7A, 0x2F7B, 0x2F7C, 0x2F7D, 0x2F7E];
const SKELETON_MSG_DEFAULT: u16 = 0x2F84;

/// The 7 dinosaur groups as (start, end) item-id ranges, derived from
/// `FTR_DINO_START` (`aCR_get_fossil_type`). Each part occupies 4 ids.
const GROUP_RANGES: [(u16, u16); 7] = [
    (range::DINO_START, range::DINO_START + 11),      // trikera HEAD..BODY
    (range::DINO_START + 12, range::DINO_START + 23), // trex HEAD..BODY
    (range::DINO_START + 24, range::DINO_START + 35), // bront HEAD..BODY
    (range::DINO_START + 36, range::DINO_START + 47), // stego HEAD..BODY
    (range::DINO_START + 48, range::DINO_START + 59), // ptera HEAD..LWING
    (range::DINO_START + 60, range::DINO_START + 71), // hutaba HEAD..BODY
    (range::DINO_START + 72, range::DINO_START + 79), // mammoth HEAD..BODY
];

/// Fossil indices per dinosaur group (`aCR_chk_fossil_parts_complete_sub`).
const GROUP_PARTS: [&[u8]; 7] = [
    &[0, 1, 2],    // trikera
    &[3, 4, 5],    // trex
    &[6, 7, 8],    // bront
    &[9, 10, 11],  // stego
    &[12, 13, 14], // ptera
    &[15, 16, 17], // hutaba
    &[18, 19],     // mammoth (2 parts)
];

/// Single-part fossil START ids for the curator's no-donator branch
/// (`aCR_get_idx_to_donate_fossil`).
const SINGLE_START_AMBER: u16 = range::DINO_START + 80;
const SINGLE_START_STUMP: u16 = range::DINO_START + 84;
const SINGLE_START_AMMONITE: u16 = range::DINO_START + 88;
const SINGLE_START_EGG: u16 = range::DINO_START + 92;
const SINGLE_START_TRILOBITE: u16 = range::DINO_START + 96;

/// Mail-message id for a returned fossil (`mMsm_GetFossilMailNo`).
/// Out-of-range items fall back to index 0, exactly like the decomp.
pub fn fossil_mail_no(fossil: u16) -> u16 {
    let idx = fossil_index(fossil).unwrap_or(0) as usize;
    FOSSIL_MAIL_NO_TABLE[idx]
}

/// Zero-based fossil index, or `None` outside the dino range.
/// `(item - 0x1EEC) >> 2`.
pub fn fossil_index(item: u16) -> Option<u8> {
    if (range::DINO_START..=range::DINO_END).contains(&item) {
        Some(((item - range::DINO_START) >> 2) as u8)
    } else {
        None
    }
}

/// Reply-message id for a remail kind (`mMsm_SendResultMail`).
/// Kinds are 1..=4 (`CANNOT_DONATE`..`FOREIGNER`); anything else → 0.
pub fn remail_mail_no(kind: u8) -> u16 {
    if (1..=4).contains(&kind) {
        REMAIL_NO_TABLE[(kind - 1) as usize]
    } else {
        0
    }
}

/// Dinosaur group 0..=6 for an item, or -1 (`aCR_get_fossil_type`).
/// Singles and non-fossils return -1.
pub fn fossil_type(item: u16) -> i8 {
    for (i, &(lo, hi)) in GROUP_RANGES.iter().enumerate() {
        if (lo..=hi).contains(&item) {
            return i as i8;
        }
    }
    -1
}

/// Blathers' response index for an offered fossil
/// (`aCR_get_idx_to_donate_fossil`). Takes the pre-read donator nibble
/// and the offering player's number; the "another player" name-string
/// side effect stays in C.
pub fn donate_response(item: u16, donator: u8, player_no: u8) -> u8 {
    if donator == player_no + 1 {
        return 4; // you already donated it
    }
    match donator {
        donator::NONE => match item {
            SINGLE_START_TRILOBITE => 17,
            SINGLE_START_AMMONITE => 18,
            SINGLE_START_EGG => 19,
            SINGLE_START_STUMP => 20,
            SINGLE_START_AMBER => 21,
            _ => 16,
        },
        donator::DELETED_PLAYER => 12,
        _ => 8, // another player donated it
    }
}

/// True when every part of a dinosaur group has a donator in
/// `PLAYER1..=DELETED_PLAYER` (`aCR_chk_fossil_parts_complete_sub`).
/// `donators` is the 25-entry fossil donator array from C.
pub fn parts_complete(donators: &[u8], group: u8) -> bool {
    let parts = match GROUP_PARTS.get(group as usize) {
        Some(p) => p,
        None => return false,
    };
    parts.iter().all(|&idx| {
        matches!(
            donators.get(idx as usize),
            Some(donator::PLAYER1..=donator::DELETED_PLAYER)
        )
    })
}

/// Display classification from a donator nibble
/// (`mMmd_GetDisplayInfo` fossil branch).
pub fn display_classify(donator: u8) -> u8 {
    match donator {
        donator::PLAYER1..=donator::DELETED_PLAYER => display::ALREADY_DONATED,
        _ => display::CAN_DONATE,
    }
}

/// Skeleton-complete message for a group (`aCR_chk_fossil_parts_complete`).
pub fn skeleton_msg_no(group: u8, complete: bool) -> u16 {
    if complete {
        SKELETON_MSG_TABLE
            .get(group as usize)
            .copied()
            .unwrap_or(SKELETON_MSG_DEFAULT)
    } else {
        SKELETON_MSG_DEFAULT
    }
}

// ---- C ABI ----
// All pc_-prefixed: the C originals (`m_museum.c`, `m_museum_display.c`,
// `ac_npc_curator_move.c_inc`) stay compiled; these are staged kernels.

#[no_mangle]
pub extern "C" fn pc_museum_fossil_mail_no(fossil: u16) -> u16 {
    fossil_mail_no(fossil)
}

#[no_mangle]
pub extern "C" fn pc_museum_fossil_index(item: u16) -> i16 {
    fossil_index(item).map(|i| i as i16).unwrap_or(-1)
}

#[no_mangle]
pub extern "C" fn pc_museum_remail_mail_no(kind: u8) -> u16 {
    remail_mail_no(kind)
}

#[no_mangle]
pub extern "C" fn pc_museum_fossil_type(item: u16) -> i8 {
    fossil_type(item)
}

#[no_mangle]
pub extern "C" fn pc_museum_donate_response(item: u16, donator: u8, player_no: u8) -> u8 {
    donate_response(item, donator, player_no)
}

/// `donators` must point to 25 bytes; null or short reads are guarded.
#[no_mangle]
pub extern "C" fn pc_museum_parts_complete(donators: *const u8, group: u8) -> u8 {
    if donators.is_null() || group as usize >= GROUP_PARTS.len() {
        return 0;
    }
    let need = GROUP_PARTS[group as usize].iter().map(|&i| i as usize).max().unwrap_or(0) + 1;
    let slice = unsafe { core::slice::from_raw_parts(donators, need) };
    parts_complete(slice, group) as u8
}

#[no_mangle]
pub extern "C" fn pc_museum_display_classify(donator: u8) -> u8 {
    display_classify(donator)
}

#[no_mangle]
pub extern "C" fn pc_museum_skeleton_msg_no(group: u8, complete: u8) -> u16 {
    skeleton_msg_no(group, complete != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fossil_index_edges() {
        assert_eq!(fossil_index(0x1EEC), Some(0));
        assert_eq!(fossil_index(0x1EEF), Some(0)); // EAST variant, same idx
        assert_eq!(fossil_index(0x1F4F), Some(24));
        assert_eq!(fossil_index(0x1F50), None);
        assert_eq!(fossil_index(0x2511), None); // generic fossil, not furniture
    }

    #[test]
    fn mail_no_table_spot() {
        assert_eq!(fossil_mail_no(0x1EEC), 0x10E); // idx 0
        assert_eq!(fossil_mail_no(0x1EF0), 0x110); // idx 1
        assert_eq!(fossil_mail_no(0x1F4C), 0x122); // idx 24, trilobite
        assert_eq!(fossil_mail_no(0x1234), 0x10E); // out of range → idx 0
    }

    #[test]
    fn remail_table() {
        assert_eq!(remail_mail_no(1), 0x22D);
        assert_eq!(remail_mail_no(2), 0x22B);
        assert_eq!(remail_mail_no(3), 0x22C);
        assert_eq!(remail_mail_no(4), 0x22E);
        assert_eq!(remail_mail_no(0), 0);
        assert_eq!(remail_mail_no(5), 0);
    }

    #[test]
    fn group_classification() {
        assert_eq!(fossil_type(0x1EEC), 0);
        assert_eq!(fossil_type(0x1EF7), 0);
        assert_eq!(fossil_type(0x1EF8), 1);
        assert_eq!(fossil_type(0x1F34), 6);
        assert_eq!(fossil_type(0x1F3B), 6);
        assert_eq!(fossil_type(0x1F3C), -1); // amber: single
        assert_eq!(fossil_type(0x2511), -1); // generic fossil
    }

    #[test]
    fn curator_responses() {
        assert_eq!(donate_response(0x1EEC, 1, 0), 4); // self
        assert_eq!(donate_response(0x1EEC, 0, 0), 16); // none, multi-part
        assert_eq!(donate_response(0x1F4C, 0, 0), 17); // trilobite
        assert_eq!(donate_response(0x1F44, 0, 0), 18); // ammonite
        assert_eq!(donate_response(0x1F48, 0, 0), 19); // egg
        assert_eq!(donate_response(0x1F40, 0, 0), 20); // stump
        assert_eq!(donate_response(0x1F3C, 0, 0), 21); // amber
        assert_eq!(donate_response(0x1EEC, 5, 0), 12); // deleted player
        assert_eq!(donate_response(0x1EEC, 2, 0), 8); // another player
    }

    #[test]
    fn skeleton_completion() {
        let mut d = [0u8; 25];
        assert!(!parts_complete(&d, 0));
        d[0] = 1;
        d[1] = 2;
        d[2] = 5; // deleted still counts
        assert!(parts_complete(&d, 0));
        assert!(!parts_complete(&d, 1));
        assert!(!parts_complete(&d, 7)); // bad group
        assert_eq!(skeleton_msg_no(0, true), 0x2F78);
        assert_eq!(skeleton_msg_no(6, true), 0x2F7E);
        assert_eq!(skeleton_msg_no(0, false), 0x2F84);
        assert_eq!(skeleton_msg_no(9, true), 0x2F84);
    }

    #[test]
    fn display_classify_cases() {
        assert_eq!(display_classify(0), display::CAN_DONATE);
        assert_eq!(display_classify(1), display::ALREADY_DONATED);
        assert_eq!(display_classify(5), display::ALREADY_DONATED);
        assert_eq!(display_classify(6), display::CAN_DONATE);
    }
}
