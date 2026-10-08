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
}
