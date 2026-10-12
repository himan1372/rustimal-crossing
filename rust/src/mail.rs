//! Mail system beyond letter scoring: data model, slot operations,
//! routing, Post Office delivery, and archive storage.
//!
//! Verified against `include/m_mail.h` (Mail_c layout, font states,
//! name types, mail types), `include/m_home_h.h` (HOME_MAILBOX_SIZE=10),
//! `include/m_private.h` (mPr_INVENTORY_MAIL_COUNT=10),
//! `include/m_post_office.h` (mPO_MAIL_STORAGE_SIZE=5),
//! `src/game/m_post_office.c` (delivery scheduler, queue semantics),
//! `include/m_card.h` (8x20 archive)
//! (USA Rev. 0 decomp / PC port).
//!
//! Architecture: writing, inventory storage, Post Office submission,
//! queued delivery, mailbox receipt, reading, present attach/remove,
//! and archiving are SEPARATE operations. Player mail goes through the
//! Post Office; automatic mail tries the recipient mailbox first, then
//! the 5-slot PO queue. Delivery is twice daily (09:00/17:00) and mail
//! stays queued when the recipient mailbox is full.

/// Fixed mail sizes.
pub mod size {
    pub const HEADER_BASE: usize = 32;
    pub const PLAYER_NAME: usize = 8;
    pub const HEADER: usize = HEADER_BASE - PLAYER_NAME; // 24
    pub const FOOTER: usize = 32;
    pub const BODY: usize = 192;
    pub const MAIL: usize = 0x12A; // 298
}

/// Recipient/sender name types.
pub mod name_type {
    pub const PLAYER: u8 = 0;
    pub const NPC: u8 = 1;
    pub const MUSEUM: u8 = 2;
    pub const CLEAR: u8 = 0xFF;
}

/// Mail font = lifecycle state.
pub mod font {
    pub const RECV: u8 = 0; // received, unread
    pub const SEND: u8 = 1; // player-written outgoing
    pub const RECV_READ: u8 = 2; // received, read
    pub const RECV_PLAYER_PRESENT: u8 = 3; // received unread with present
    pub const RECV_PLAYER_PRESENT_READ: u8 = 4; // received/read with present
    pub const UNUSED: u8 = 0xFF; // cleared slot (mMl_clear_mail sets -1)
}

/// Mail types.
pub mod mtype {
    pub const MAIL: u8 = 0;
    pub const XMAS: u8 = 1;
    pub const SHOP_SALE_LEAFLET: u8 = 2;
    pub const BROKER_SALE_LEAFLET: u8 = 3;
    pub const MOTHER: u8 = 4;
    pub const OMIKUJI: u8 = 5;
    pub const HRA: u8 = 6;
    pub const SHOP: u8 = 7;
    pub const SNOWMAN: u8 = 8;
    pub const FISHING_CONTEST: u8 = 9;
    pub const POSTOFFICE: u8 = 10;
    pub const SPNPC_PASSWORD: u8 = 11;
}

/// Store capacities.
pub mod cap {
    pub const MAILBOX: usize = 10; // per house
    pub const INVENTORY: usize = 10; // per player
    pub const POST_OFFICE: usize = 5; // transit queue
    pub const ARCHIVE_PAGES: usize = 8;
    pub const ARCHIVE_PER_PAGE: usize = 20; // 160 total
}

/// A mail address: personal ID + name-type discriminator.
/// (PersonalID_c fields are abstracted; the name_type is what matters
/// for routing.)
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MailName {
    pub name_type: u8,
}

/// Mail content: font/state + paper + fixed text fields.
#[derive(Clone, Copy, Debug)]
pub struct MailContent {
    pub font: u8,
    pub header_back_start: u8,
    pub mail_type: u8,
    pub paper_type: u8,
    pub header: [u8; size::HEADER],
    pub body: [u8; size::BODY],
    pub footer: [u8; size::FOOTER],
}

impl Default for MailContent {
    fn default() -> Self {
        Self {
            font: font::UNUSED,
            header_back_start: 0,
            mail_type: mtype::MAIL,
            paper_type: 0,
            header: [0x20; size::HEADER], // CHAR_SPACE
            body: [0x20; size::BODY],
            footer: [0x20; size::FOOTER],
        }
    }
}

/// Mail_c: fixed 298-byte letter. present = EMPTY_NO (0xFFFF) when none.
#[derive(Clone, Copy, Debug, Default)]
pub struct Mail {
    pub recipient_type: u8,
    pub sender_type: u8,
    pub present: u16, // EMPTY_NO = none
    pub content: MailContent,
}

pub const EMPTY_NO: u16 = 0xFFFF;

impl Mail {
    /// mMl_clear_mail: zero + space-fill text + font = -1 (0xFF).
    pub fn clear(&mut self) {
        *self = Mail::default();
        self.recipient_type = name_type::CLEAR;
        self.sender_type = name_type::CLEAR;
        self.present = EMPTY_NO;
    }

    /// Unused slot check: font == 0xFF.
    pub fn is_unused(&self) -> bool {
        self.content.font == font::UNUSED
    }

    /// mMl_copy_mail: raw full-structure copy.
    pub fn copy_from_mail(&mut self, src: &Mail) {
        *self = *src;
    }

    pub fn has_present(&self) -> bool {
        self.present != EMPTY_NO
    }

    pub fn is_outgoing(&self) -> bool {
        self.content.font == font::SEND
    }

    pub fn is_received_unread(&self) -> bool {
        matches!(
            self.content.font,
            font::RECV | font::RECV_PLAYER_PRESENT
        )
    }

    /// Reading transitions RECV -> RECV_READ (present variants likewise).
    /// Returns true if the state changed.
    pub fn mark_read(&mut self) -> bool {
        match self.content.font {
            font::RECV => {
                self.content.font = font::RECV_READ;
                true
            }
            font::RECV_PLAYER_PRESENT => {
                self.content.font = font::RECV_PLAYER_PRESENT_READ;
                true
            }
            _ => false,
        }
    }

    /// Present can be attached to outgoing or already-present mail.
    pub fn can_attach_present(&self) -> bool {
        matches!(
            self.content.font,
            font::SEND | font::RECV_PLAYER_PRESENT | font::RECV_PLAYER_PRESENT_READ
        )
    }
}

/// Find the first free slot in a mail array. Returns index or None.
pub fn find_free_slot<const N: usize>(slots: &[Mail; N]) -> Option<usize> {
    slots.iter().position(|m| m.is_unused())
}

/// Count used slots.
pub fn count_used<const N: usize>(slots: &[Mail; N]) -> usize {
    slots.iter().filter(|m| !m.is_unused()).count()
}

/// House mailbox: 10 slots. Flag animation derives from count_used > 0.
pub type Mailbox = [Mail; cap::MAILBOX];
/// Player inventory mail: 10 slots.
pub type InventoryMail = [Mail; cap::INVENTORY];
/// Post Office transit queue: 5 slots.
pub type PostOfficeQueue = [Mail; cap::POST_OFFICE];

/// Post Office state: queue + recipient bitfield + delivery time.
#[derive(Clone, Debug, Default)]
pub struct PostOffice {
    pub queue: PostOfficeQueue,
    /// Bitmask of houses (0..3) with queued player mail.
    pub mail_recipient_flags: u16,
    pub keep_mail_sum_players: i16,
    pub keep_mail_sum_npcs: i16,
}

impl PostOffice {
    /// Total queued mail (players + NPCs).
    pub fn keep_mail_sum(&self) -> i16 {
        self.keep_mail_sum_players + self.keep_mail_sum_npcs
    }

    /// Queue is full at mPO_MAIL_STORAGE_SIZE (5).
    pub fn is_full(&self) -> bool {
        self.keep_mail_sum() >= cap::POST_OFFICE as i16
    }

    /// Submit a letter to the queue. Returns the slot or None if full.
    pub fn receipt(&mut self, mail: &Mail, house_no: u8) -> Option<usize> {
        if self.is_full() {
            return None;
        }
        let slot = find_free_slot(&self.queue)?;
        self.queue[slot].copy_from_mail(mail);
        self.keep_mail_sum_players += 1;
        if house_no < 4 {
            self.mail_recipient_flags |= 1 << house_no;
        }
        Some(slot)
    }

    /// Deliver one queued letter to a house mailbox. On success the
    /// queue slot is cleared and counts decremented; on mailbox-full
    /// the letter REMAINS queued.
    pub fn deliver_to_house(&mut self, slot: usize, mailbox: &mut Mailbox) -> bool {
        if slot >= cap::POST_OFFICE || self.queue[slot].is_unused() {
            return false;
        }
        let dst = match find_free_slot(mailbox) {
            Some(i) => i,
            None => return false, // mailbox full: stays queued
        };
        mailbox[dst].copy_from_mail(&self.queue[slot]);
        self.queue[slot].clear();
        self.keep_mail_sum_players -= 1;
        true
    }

    /// Clear a house's recipient flag after delivery.
    pub fn clear_recipient_flag(&mut self, house_no: u8) {
        if house_no < 4 {
            self.mail_recipient_flags &= !(1 << house_no);
        }
    }
}

/// Next delivery time: <09:00 -> 09:00 today; <17:00 -> 17:00 today;
/// else 09:00 tomorrow. Returns (hour, next_day).
pub fn next_delivery_time(hour: u8) -> (u8, bool) {
    if hour < 9 {
        (9, false)
    } else if hour < 17 {
        (17, false)
    } else {
        (9, true)
    }
}

/// Automatic mail routing: try the recipient mailbox first; fall back
/// to the Post Office queue. Returns true if placed anywhere.
pub fn send_mail_auto(
    mail: &Mail,
    mailbox: &mut Mailbox,
    po: &mut PostOffice,
    house_no: u8,
) -> bool {
    if let Some(dst) = find_free_slot(mailbox) {
        mailbox[dst].copy_from_mail(mail);
        return true;
    }
    po.receipt(mail, house_no).is_some()
}

/// NPC-mail friendship effects (mNpc_SendMailtoNpc).
pub mod npc_friendship {
    pub const SENT_LETTER: i32 = 3;
    pub const BAD_RANK: i32 = -5;
    pub const WITH_PRESENT: i32 = 3;
}

/// Mailbox -> inventory transfer. Returns true on success.
pub fn mailbox_to_inventory(
    mailbox: &mut Mailbox,
    mbox_slot: usize,
    inv: &mut InventoryMail,
) -> bool {
    if mbox_slot >= cap::MAILBOX || mailbox[mbox_slot].is_unused() {
        return false;
    }
    let dst = match find_free_slot(inv) {
        Some(i) => i,
        None => return false,
    };
    inv[dst].copy_from_mail(&mailbox[mbox_slot]);
    mailbox[mbox_slot].clear();
    true
}

// ---- C ABI ----

/// C ABI: 1 if the mail slot is unused.
#[no_mangle]
pub extern "C" fn pc_mail_unused(font_byte: u8) -> u8 {
    (font_byte == font::UNUSED) as u8
}

/// C ABI: mark-read transition; returns the new font byte.
#[no_mangle]
pub extern "C" fn pc_mail_mark_read(font_byte: u8) -> u8 {
    match font_byte {
        font::RECV => font::RECV_READ,
        font::RECV_PLAYER_PRESENT => font::RECV_PLAYER_PRESENT_READ,
        _ => font_byte,
    }
}

/// C ABI: next delivery hour for the current hour; high bit set if tomorrow.
#[no_mangle]
pub extern "C" fn pc_next_delivery(hour: u8) -> u16 {
    let (h, next_day) = next_delivery_time(hour);
    (h as u16) | ((next_day as u16) << 15)
}

/// C ABI: 1 if the PO queue is full given player+npc counts.
#[no_mangle]
pub extern "C" fn pc_po_full(players: i16, npcs: i16) -> u8 {
    (players + npcs >= cap::POST_OFFICE as i16) as u8
}

// ---------------------------------------------------------------------------
// Mailbox overlay selection (`m_mailbox_ovl.c`)
// ---------------------------------------------------------------------------

/// C ABI: `mMB_get_last_mail_idx` — scan the 10-slot house mailbox backward
/// for the last used slot. `used[i]` is 1 when slot `i` holds mail. Always
/// decrements at least once (do-while); returns 0 when every slot is empty.
#[no_mangle]
pub extern "C" fn pc_mailbox_last_idx(used: *const u8, n: usize) -> usize {
    if used.is_null() || n == 0 {
        return 0;
    }
    let mut idx = n;
    loop {
        idx -= 1;
        let is_used = unsafe { *used.add(idx) } != 0;
        if !is_used && idx != 0 {
            continue;
        }
        break;
    }
    idx
}

// ---------------------------------------------------------------------------
// Mail check: the non-NES dictionary scorer (`m_mail_check_ovl.c`,
// `m_mail_check.c` `mMC_get_mail_hit_rate`)
// ---------------------------------------------------------------------------

/// `mMck` dictionary pair tables (`str_a_table` … `str_z_table`), verbatim.
/// DOCUMENTED DEVIATION: the US retail tables lack the 0x7F terminator and
/// `mMck_cmp_key` scans past the table end into adjacent memory (see the
/// BUG comment in the decomp). The Rust scan stops at the table end —
/// safer, but not bit-identical to the retail binary for bodies whose
/// segments would match bytes past a table.
pub const MCK_PAIR_TABLES: [&[u8]; 26] = [
    // 'a': 57 pairs
    &[98, 108, 98, 111, 98, 114, 98, 115, 99, 99, 99, 104, 99, 114, 99, 116, 100, 100, 100, 109, 100, 118, 101, 114, 102, 102, 102, 114, 102, 116, 103, 97, 103, 101, 103, 111, 104, 101, 105, 114, 108, 105, 108, 108, 108, 109, 108, 111, 108, 114, 108, 115, 108, 116, 108, 119, 109, 32, 109, 101, 109, 111, 110, 100, 110, 103, 110, 105, 110, 111, 110, 115, 110, 121, 112, 97, 112, 112, 112, 114, 114, 101, 114, 103, 114, 109, 114, 114, 114, 116, 115, 105, 115, 107, 115, 108, 116, 101, 116, 109, 116, 116, 117, 100, 117, 103, 117, 116, 118, 101, 118, 111, 119, 97],
    // 'b': 49 pairs
    &[97, 98, 97, 99, 97, 100, 97, 103, 97, 108, 97, 110, 97, 115, 97, 116, 101, 32, 101, 97, 101, 99, 101, 100, 101, 101, 101, 102, 101, 103, 101, 104, 101, 108, 101, 115, 101, 116, 101, 121, 105, 99, 105, 103, 105, 107, 105, 108, 105, 114, 105, 116, 108, 97, 108, 101, 108, 111, 108, 117, 111, 97, 111, 100, 111, 110, 111, 111, 111, 114, 111, 116, 111, 117, 111, 120, 111, 121, 114, 97, 114, 101, 114, 105, 114, 111, 117, 105, 117, 114, 117, 115, 117, 116, 117, 121, 121, 32],
    // 'c': 44 pairs
    &[97, 107, 97, 108, 97, 109, 97, 110, 97, 112, 97, 114, 97, 115, 97, 116, 97, 117, 101, 110, 101, 114, 104, 97, 104, 101, 104, 105, 104, 111, 104, 105, 104, 117, 105, 114, 105, 116, 108, 97, 108, 101, 108, 105, 108, 111, 111, 97, 111, 102, 111, 105, 111, 108, 111, 109, 111, 110, 111, 111, 111, 112, 111, 114, 111, 115, 111, 117, 111, 118, 111, 119, 114, 101, 114, 105, 114, 111, 114, 121, 117, 112, 117, 114, 117, 115, 117, 116],
    // 'd': 49 pairs
    &[97, 100, 97, 105, 97, 109, 97, 110, 97, 114, 97, 116, 97, 117, 97, 121, 101, 97, 101, 99, 101, 101, 101, 102, 101, 103, 101, 108, 101, 109, 101, 110, 101, 112, 101, 115, 101, 116, 101, 118, 105, 99, 105, 100, 105, 101, 105, 102, 105, 103, 105, 110, 105, 114, 105, 115, 105, 118, 111, 32, 111, 99, 111, 101, 111, 103, 111, 108, 111, 110, 111, 111, 111, 117, 111, 119, 111, 122, 114, 97, 114, 101, 114, 105, 114, 111, 114, 117, 114, 121, 117, 101, 117, 114, 117, 115, 117, 116],
    // 'e': 28 pairs
    &[97, 99, 97, 114, 97, 115, 97, 116, 100, 117, 102, 102, 103, 103, 105, 103, 105, 116, 108, 101, 108, 115, 109, 112, 110, 100, 110, 101, 110, 103, 110, 106, 110, 111, 110, 116, 113, 117, 114, 114, 115, 112, 117, 114, 118, 101, 120, 97, 120, 99, 120, 101, 120, 112, 121, 101],
    // 'f': 39 pairs
    &[97, 99, 97, 105, 97, 108, 97, 109, 97, 114, 97, 115, 97, 116, 101, 97, 101, 98, 101, 100, 101, 101, 101, 108, 101, 119, 105, 101, 105, 102, 105, 103, 105, 108, 105, 110, 105, 114, 105, 115, 105, 118, 105, 120, 108, 97, 108, 101, 108, 105, 108, 111, 108, 121, 111, 108, 111, 111, 111, 114, 111, 117, 114, 97, 114, 101, 114, 105, 114, 111, 114, 117, 117, 108, 117, 110, 117, 116],
    // 'g': 26 pairs
    &[97, 105, 97, 109, 97, 114, 97, 115, 97, 116, 97, 118, 101, 110, 101, 114, 101, 116, 105, 114, 105, 118, 108, 97, 111, 32, 111, 100, 111, 105, 111, 110, 111, 111, 111, 116, 111, 118, 114, 97, 114, 101, 114, 111, 117, 97, 117, 101, 117, 105, 117, 110],
    // 'h': 36 pairs
    &[97, 98, 97, 100, 97, 105, 97, 108, 97, 110, 97, 112, 97, 114, 97, 115, 97, 116, 97, 118, 101, 32, 101, 97, 101, 105, 101, 108, 101, 114, 105, 32, 105, 100, 105, 103, 105, 108, 105, 109, 105, 114, 105, 115, 105, 116, 111, 108, 111, 109, 111, 110, 111, 112, 111, 114, 111, 115, 111, 116, 111, 117, 111, 119, 117, 109, 117, 110, 117, 114, 117, 115],
    // 'i': 19 pairs
    &[32, 32, 99, 101, 100, 101, 102, 32, 109, 97, 109, 109, 109, 112, 110, 32, 110, 99, 110, 100, 110, 102, 110, 115, 110, 116, 110, 118, 114, 111, 115, 32, 115, 108, 116, 32, 116, 115],
    // 'j': 9 pairs
    &[97, 110, 97, 112, 111, 98, 111, 105, 117, 100, 117, 108, 117, 109, 117, 110, 117, 115],
    // 'k': 10 pairs
    &[101, 101, 101, 112, 101, 121, 105, 99, 105, 108, 105, 110, 105, 116, 110, 101, 110, 105, 110, 111],
    // 'l': 40 pairs
    &[97, 98, 97, 100, 97, 105, 97, 107, 97, 110, 97, 114, 97, 115, 97, 116, 97, 117, 97, 119, 97, 121, 97, 122, 101, 97, 101, 100, 101, 102, 101, 103, 101, 110, 101, 115, 101, 116, 101, 118, 105, 98, 105, 101, 105, 102, 105, 103, 105, 107, 105, 109, 105, 110, 105, 115, 105, 116, 105, 118, 111, 99, 111, 110, 111, 111, 111, 115, 111, 116, 111, 117, 111, 118, 111, 119, 117, 110, 121, 105],
    // 'm': 38 pairs
    &[97, 99, 97, 100, 97, 103, 97, 105, 97, 106, 97, 107, 97, 110, 97, 112, 97, 114, 97, 116, 97, 121, 101, 32, 101, 97, 101, 100, 101, 101, 101, 109, 101, 110, 101, 115, 101, 116, 105, 100, 105, 103, 105, 108, 105, 110, 105, 115, 111, 100, 111, 109, 111, 110, 111, 111, 111, 114, 111, 115, 111, 116, 111, 117, 111, 118, 117, 99, 117, 109, 117, 115, 121, 32, 121, 115],
    // 'n': 24 pairs
    &[97, 109, 97, 114, 97, 116, 101, 97, 101, 99, 101, 101, 101, 105, 101, 118, 101, 119, 101, 120, 105, 99, 105, 103, 105, 110, 111, 32, 111, 98, 111, 105, 111, 110, 111, 111, 111, 114, 111, 115, 111, 116, 111, 118, 111, 119, 117, 109],
    // 'o': 25 pairs
    &[98, 106, 39, 99, 99, 116, 102, 32, 102, 102, 102, 116, 104, 32, 105, 108, 107, 97, 108, 100, 110, 32, 110, 99, 110, 101, 110, 108, 112, 101, 112, 105, 112, 112, 114, 32, 114, 97, 114, 100, 116, 104, 117, 114, 117, 116, 118, 101, 119, 110],
    // 'p': 38 pairs
    &[97, 99, 97, 103, 97, 105, 97, 112, 97, 114, 97, 115, 97, 116, 97, 121, 101, 97, 101, 110, 101, 111, 101, 114, 104, 111, 105, 99, 105, 101, 105, 110, 105, 112, 108, 97, 108, 101, 111, 99, 111, 105, 111, 108, 111, 111, 111, 112, 111, 115, 111, 116, 111, 117, 111, 119, 114, 97, 114, 101, 114, 105, 114, 111, 117, 98, 117, 108, 117, 112, 117, 114, 117, 115, 117, 116],
    // 'q': 3 pairs
    &[117, 97, 117, 101, 117, 105],
    // 'r': 34 pairs
    &[97, 99, 97, 100, 97, 105, 97, 110, 97, 112, 97, 116, 101, 97, 101, 99, 101, 100, 101, 102, 101, 103, 101, 108, 101, 109, 101, 112, 101, 113, 101, 115, 101, 116, 105, 99, 105, 100, 105, 103, 105, 110, 105, 115, 105, 118, 111, 97, 111, 99, 111, 100, 111, 108, 111, 111, 111, 115, 111, 117, 111, 119, 117, 108, 117, 110, 117, 115],
    // 's': 87 pairs
    &[97, 100, 97, 102, 97, 105, 97, 108, 97, 109, 97, 110, 97, 116, 97, 118, 97, 119, 97, 121, 99, 101, 99, 104, 99, 105, 99, 111, 101, 97, 101, 99, 101, 101, 101, 108, 101, 110, 101, 112, 101, 114, 101, 116, 101, 118, 101, 120, 104, 97, 104, 101, 104, 105, 104, 111, 104, 117, 105, 99, 105, 100, 105, 103, 105, 108, 105, 109, 105, 110, 105, 115, 105, 116, 105, 120, 105, 122, 107, 105, 107, 121, 108, 101, 108, 105, 108, 111, 109, 97, 109, 101, 109, 105, 109, 111, 110, 111, 111, 32, 111, 97, 111, 99, 111, 102, 111, 105, 111, 108, 111, 109, 111, 110, 111, 111, 111, 114, 111, 117, 112, 97, 112, 101, 112, 105, 112, 111, 112, 114, 113, 117, 116, 97, 116, 101, 116, 105, 116, 111, 116, 114, 116, 117, 116, 121, 117, 98, 117, 99, 117, 100, 117, 102, 117, 103, 117, 109, 117, 110, 117, 112, 117, 114, 119, 97, 119, 101, 119, 105, 119, 117, 121, 115],
    // 't': 51 pairs
    &[97, 98, 97, 107, 97, 108, 97, 115, 97, 117, 97, 120, 101, 97, 101, 101, 101, 108, 101, 109, 101, 110, 101, 114, 101, 115, 104, 97, 104, 101, 104, 105, 104, 111, 104, 114, 104, 117, 105, 99, 105, 101, 105, 108, 105, 109, 105, 114, 105, 116, 111, 32, 111, 100, 111, 103, 111, 108, 111, 109, 111, 110, 111, 111, 111, 112, 111, 114, 111, 116, 111, 117, 111, 119, 114, 97, 114, 101, 114, 105, 114, 111, 114, 117, 114, 121, 117, 101, 117, 114, 118, 32, 119, 101, 119, 105, 119, 111, 121, 105, 121, 112],
    // 'u': 11 pairs
    &[110, 97, 110, 99, 110, 100, 110, 105, 110, 108, 110, 116, 112, 32, 112, 111, 115, 32, 115, 101, 115, 117],
    // 'v': 10 pairs
    &[97, 108, 97, 114, 101, 103, 101, 114, 105, 101, 105, 108, 105, 115, 111, 105, 111, 108, 111, 116],
    // 'w': 40 pairs
    &[97, 105, 97, 107, 97, 108, 97, 110, 97, 114, 97, 115, 97, 116, 97, 118, 97, 121, 101, 32, 101, 97, 101, 100, 101, 101, 101, 105, 101, 108, 101, 110, 101, 114, 101, 115, 101, 116, 104, 97, 104, 101, 104, 105, 104, 111, 104, 121, 105, 100, 105, 102, 105, 108, 105, 110, 105, 114, 105, 115, 105, 116, 105, 118, 111, 107, 111, 109, 111, 110, 111, 111, 111, 114, 111, 117, 114, 105, 114, 111],
    // 'x': 1 pairs
    &[109, 97],
    // 'y': 7 pairs
    &[97, 114, 101, 97, 101, 108, 101, 110, 101, 115, 101, 116, 111, 117],
    // 'z': 1 pairs
    &[101, 114],
];

/// Word separators for the dictionary scorer: space, comma, ?, !, .,
/// interpunct, newline.
fn mck_is_sep(c: u8) -> bool {
    matches!(c, 32 | 44 | 63 | 33 | 46 | 133 | 10)
}

/// `mMck_strlen`: trim trailing spaces over `len` bytes.
fn mck_strlen(body: &[u8], len: usize) -> usize {
    let mut n = len.min(body.len());
    while n > 0 && body[n - 1] == 32 {
        n -= 1;
    }
    n
}

/// `mMck_search_sep`: scan up to `len` bytes for a separator whose next byte
/// is not a separator; returns the separator index (may be `len`).
fn mck_search_sep(body: &[u8], start: usize, len: usize) -> usize {
    let mut i = start;
    let end = (start + len).min(body.len());
    while i < end {
        if mck_is_sep(body[i]) && (i + 1 >= body.len() || !mck_is_sep(body[i + 1])) {
            break;
        }
        i += 1;
    }
    i
}

/// `mMck_cmp_key`: the segment's first byte selects one of the 26 pair
/// tables (lower- or upper-case trigger); a hit is a matching
/// (second, third) byte pair. See the `MCK_PAIR_TABLES` deviation note.
fn mck_cmp_key(seg: &[u8]) -> bool {
    if seg.len() < 3 {
        return false;
    }
    let (c0, c1, c2) = (seg[0], seg[1], seg[2]);
    let table_idx = if (97..=122).contains(&c0) {
        c0 - 97
    } else if (65..=90).contains(&c0) {
        c0 - 65
    } else {
        return false;
    };
    let table = MCK_PAIR_TABLES[table_idx as usize];
    table.chunks_exact(2).any(|p| p[0] == c1 && p[1] == c2)
}

/// `mMck_check_key_hit`: scan the 192-byte body in `MAIL_BODY_LEN - 3`
/// (189) bytes, counting word-like segments and dictionary matches.
/// Returns `(hit_rate, segment_count)`; the rate is
/// `(matches * 10000) / (chars * 100)`.
pub fn mck_check_key_hit(body: &[u8; 192]) -> (i32, i32) {
    let str_len = mck_strlen(body, 189);
    if str_len == 0 {
        return (0, 0);
    }
    let mut matches = 0i32;
    let mut chars = 0i32;
    let mut i = 0usize;
    let mut str_ofs = 0usize; // `str` pointer as an offset into body
    while i <= str_len {
        chars += 1;
        if mck_cmp_key(&body[str_ofs..]) {
            matches += 1;
        }
        // str = mMck_search_sep(sep, str_len - i) + 1; i += str - sep;
        let sep_ofs = str_ofs;
        let found = mck_search_sep(body, sep_ofs, str_len - i);
        str_ofs = (found + 1).min(body.len());
        i += str_ofs - sep_ofs;
    }
    ((matches * 10000) / (chars * 100), chars)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mail_lifecycle() {
        let mut m = Mail::default();
        assert!(m.is_unused());
        // Write a letter.
        m.content.font = font::SEND;
        m.present = 1234;
        assert!(!m.is_unused());
        assert!(m.is_outgoing());
        assert!(m.has_present());
        assert!(m.can_attach_present());
        // Simulate receipt: SEND -> RECV_PLAYER_PRESENT.
        m.content.font = font::RECV_PLAYER_PRESENT;
        assert!(m.is_received_unread());
        assert!(m.mark_read());
        assert_eq!(m.content.font, font::RECV_PLAYER_PRESENT_READ);
        assert!(!m.mark_read()); // already read
        // Clear.
        m.clear();
        assert!(m.is_unused());
        assert_eq!(m.content.font, font::UNUSED);
        assert_eq!(m.present, EMPTY_NO);
        // Sizes.
        assert_eq!(size::HEADER, 24);
        assert_eq!(size::BODY, 192);
        assert_eq!(size::MAIL, 0x12A);
    }

    #[test]
    fn slot_ops() {
        let mut mbox: Mailbox = Default::default();
        assert_eq!(count_used(&mbox), 0);
        assert_eq!(find_free_slot(&mbox), Some(0));
        let mut m = Mail::default();
        m.content.font = font::RECV;
        mbox[0].copy_from_mail(&m);
        assert_eq!(count_used(&mbox), 1);
        assert_eq!(find_free_slot(&mbox), Some(1));
        // Mailbox -> inventory.
        let mut inv: InventoryMail = Default::default();
        assert!(mailbox_to_inventory(&mut mbox, 0, &mut inv));
        assert_eq!(count_used(&mbox), 0);
        assert_eq!(count_used(&inv), 1);
        assert_eq!(inv[0].content.font, font::RECV);
    }

    #[test]
    fn post_office() {
        let mut po = PostOffice::default();
        let mut m = Mail::default();
        m.content.font = font::RECV;
        // Fill the 5-slot queue.
        for i in 0..5 {
            assert_eq!(po.receipt(&m, 0), Some(i));
        }
        assert!(po.is_full());
        assert_eq!(po.receipt(&m, 0), None); // rejected when full
        assert_eq!(po.keep_mail_sum(), 5);
        assert_eq!(po.mail_recipient_flags & 1, 1);
        // Deliver to a house with room.
        let mut mbox: Mailbox = Default::default();
        assert!(po.deliver_to_house(0, &mut mbox));
        assert_eq!(count_used(&mbox), 1);
        assert_eq!(po.keep_mail_sum(), 4);
        assert!(po.queue[0].is_unused()); // cleared after success
        // Mailbox full: stays queued.
        let mut full: Mailbox = Default::default();
        for s in full.iter_mut() {
            s.content.font = font::RECV;
        }
        assert!(!po.deliver_to_house(1, &mut full));
        assert!(!po.queue[1].is_unused()); // still queued
        po.clear_recipient_flag(0);
        assert_eq!(po.mail_recipient_flags & 1, 0);
        // Auto routing prefers the mailbox.
        let mut po2 = PostOffice::default();
        let mut mbox2: Mailbox = Default::default();
        assert!(send_mail_auto(&m, &mut mbox2, &mut po2, 1));
        assert_eq!(count_used(&mbox2), 1);
        assert_eq!(po2.keep_mail_sum(), 0);
    }

    #[test]
    fn delivery_schedule() {
        assert_eq!(next_delivery_time(8), (9, false));
        assert_eq!(next_delivery_time(9), (17, false));
        assert_eq!(next_delivery_time(16), (17, false));
        assert_eq!(next_delivery_time(17), (9, true));
        assert_eq!(next_delivery_time(23), (9, true));
        // Capacities.
        assert_eq!(cap::MAILBOX, 10);
        assert_eq!(cap::INVENTORY, 10);
        assert_eq!(cap::POST_OFFICE, 5);
        assert_eq!(cap::ARCHIVE_PAGES * cap::ARCHIVE_PER_PAGE, 160);
        // Friendship.
        assert_eq!(npc_friendship::SENT_LETTER, 3);
        assert_eq!(npc_friendship::BAD_RANK, -5);
        // C ABI.
        assert_eq!(pc_mail_unused(0xFF), 1);
        assert_eq!(pc_mail_mark_read(font::RECV), font::RECV_READ);
        assert_eq!(pc_mail_mark_read(font::RECV_READ), font::RECV_READ);
        assert_eq!(pc_next_delivery(8), 9);
        assert_eq!(pc_next_delivery(18), 9 | (1 << 15));
        assert_eq!(pc_po_full(3, 2), 1);
        assert_eq!(pc_po_full(2, 2), 0);
    }

    #[test]
    fn mailbox_last_idx() {
        // Backward scan: last used slot wins; all-empty returns 0.
        let used = [0u8, 1, 0, 1, 0, 0, 0, 0, 0, 0];
        assert_eq!(pc_mailbox_last_idx(used.as_ptr(), 10), 3);
        let used = [1u8, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        assert_eq!(pc_mailbox_last_idx(used.as_ptr(), 10), 0);
        let used = [0u8; 10];
        assert_eq!(pc_mailbox_last_idx(used.as_ptr(), 10), 0);
        let used = [0u8, 0, 0, 0, 0, 0, 0, 0, 0, 1];
        assert_eq!(pc_mailbox_last_idx(used.as_ptr(), 10), 9);
        assert_eq!(pc_mailbox_last_idx(std::ptr::null(), 10), 0);
    }

    fn mck_body(text: &[u8]) -> [u8; 192] {
        let mut b = [32u8; 192];
        b[..text.len()].copy_from_slice(text);
        b
    }

    #[test]
    fn mck_key_hit_rate() {
        // Empty body → (0, 0).
        assert_eq!(mck_check_key_hit(&mck_body(b"")), (0, 0));
        // "all " → segments: "all" matches the a-table (l,l pair)? "all":
        // c0='a' → a-table; pairs include (l,l)? The a-table has l,l? It has
        // l,i l,l l,m l,o l,r l,s l,t l,w — yes (l,l) is there → match.
        let (rate, chars) = mck_check_key_hit(&mck_body(b"all "));
        assert_eq!((rate, chars), (100, 1));
        // "all xyz " → "all" matches, "xyz" does not → 50%.
        let (rate, chars) = mck_check_key_hit(&mck_body(b"all xyz "));
        assert_eq!((rate, chars), (50, 2));
        // Trailing spaces are trimmed from the 189-byte window.
        let (rate, chars) = mck_check_key_hit(&mck_body(b"all    "));
        assert_eq!((rate, chars), (100, 1));
        // Case-insensitive trigger: "All" also matches.
        let (rate, _) = mck_check_key_hit(&mck_body(b"All "));
        assert_eq!(rate, 100);
        // "the " → c0='t' → t-table has (h,e) → match.
        let (rate, _) = mck_check_key_hit(&mck_body(b"the "));
        assert_eq!(rate, 100);
        // Digits never match (no table for them).
        let (rate, chars) = mck_check_key_hit(&mck_body(b"123 "));
        assert_eq!((rate, chars), (0, 1));
    }
}
