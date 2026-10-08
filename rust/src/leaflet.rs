//! Leaflet broadcast system.
//!
//! Verified against `include/m_post_office.h`, `src/game/m_post_office.c`,
//! `src/actor/npc/ac_npc_post_man_move.c_inc`, `src/actor/ac_event_manager.c`,
//! `src/game/m_event.c`, `src/game/m_event_schedule.c_inc` and
//! `src/game/m_shop.c` (GAFE01_00 Rev. 0).
//!
//! Architecture: the post office holds TWO persistent singleton broadcast
//! messages, not queues:
//!
//! * `leaflet`       -- normal broadcast channel
//! * `event_leaflet` -- event broadcast channel (shop sale / broker sale)
//!
//! Each has its own 4-bit recipient mask. Flag polarity is the INVERSE of
//! normal mail: **0 = pending, 1 = delivered**. Receiving a new leaflet
//! overwrites the singleton and resets its mask to 0 (all four recipients
//! pending). Delivery copies the singleton into each house mailbox one by
//! one, setting recipient bits; a full mailbox leaves the bit at 0 so the
//! postman retries later.
//!
//! Engine-owned pieces (house list, mailboxes, special event state, Arbeit
//! state) enter through [`HouseInfo`] / [`LeafletMailbox`] / function inputs;
//! side effects that the engine must perform (setting the recipient name,
//! clearing the PO queue) are left to the C side.

use crate::mail::{font, mtype, Mail};

/// Number of player/house slots the leaflet masks cover.
pub const HOUSE_NUM: usize = 4;
/// Valid bits of the recipient masks (save checker rejects anything else).
pub const HOUSE_MASK: i16 = 0x000F;
/// Unclaimed house marker: `house->ownerID.land_id == 0xFFFF`.
pub const UNCLAIMED_LAND_ID: u16 = 0xFFFF;

/// `mPO_SENDTYPE_*` from `include/m_post_office.h`.
pub mod send_type {
    pub const MAIL: u8 = 0;
    pub const LEAFLET: u8 = 1;
    pub const EVENT_LEAFLET: u8 = 2;
    pub const NUM: u8 = 3;
}

/// Special event ids (position in the `mEv_EVENT_*` enum, m_event.h).
pub mod event {
    /// `mEv_EVENT_BROKER_SALE`
    pub const BROKER_SALE: i32 = 26;
    /// `mEv_EVENT_SHOP_SALE`
    pub const SHOP_SALE: i32 = 29;
}

/// Handbill template selection for the shop-sale event:
/// `handbill_table[shop_level][category]`, 16 templates.
pub const SHOP_SALE_HANDBILLS: [[u16; 4]; 4] = [
    [0x002, 0x003, 0x004, 0x005],
    [0x006, 0x007, 0x008, 0x009],
    [0x00A, 0x00B, 0x00C, 0x00D],
    [0x00E, 0x00F, 0x010, 0x011],
];

/// Item categories for the shop-sale handbill table
/// (`mSP_KIND_FURNITURE/CARPET/WALLPAPER/CLOTH`).
pub mod shop_cat {
    pub const FURNITURE: usize = 0;
    pub const CARPET: usize = 1;
    pub const WALLPAPER: usize = 2;
    pub const CLOTH: usize = 3;
}

/// Broker-sale handbill templates; one chosen with `RANDOM(3)`.
pub const BROKER_HANDBILLS: [u16; 3] = [0x031, 0x032, 0x033];

/// Rare-furniture chirashi templates: `rare_chirashi_bunmen[shop_level][type & 1]`.
pub const RARE_CHIRASHI: [[u16; 2]; 4] = [
    [18, 18],
    [19, 19],
    [21, 20],
    [23, 22],
];

/// Paper offsets relative to `ITM_PAPER_START` (0x2000) used for handbills.
pub mod paper_offset {
    /// `ITM_PAPER55 - ITM_PAPER_START` (shop-sale / rare chirashi)
    pub const PAPER55: u8 = 55;
    /// `ITM_PAPER54 - ITM_PAPER_START` (broker handbill)
    pub const PAPER54: u8 = 54;
}

/// mPO_delivery_one_address_event_leaflet's validity filter: translate the
/// stored event leaflet's mail type into the required special event type.
/// Anything else maps to -1 and can never match, so it is never delivered.
pub fn event_leaflet_event_type(mail_type: u8) -> i32 {
    match mail_type {
        mtype::SHOP_SALE_LEAFLET => event::SHOP_SALE,
        mtype::BROKER_SALE_LEAFLET => event::BROKER_SALE,
        _ => -1,
    }
}

/// Persistent post-office leaflet state.
///
/// Retail layout (m_post_office.h):
/// `leaflet` @ 0x5DA, `event_leaflet` @ 0x704, `leaflet_flags` @ 0x830,
/// `event_flags` @ 0x832, `delivery_time` @ 0x834, `sizeof(PostOffice_c)`
/// = 0x83C.
#[derive(Clone, Debug)]
pub struct LeafletState {
    pub leaflet: Mail,
    pub event_leaflet: Mail,
    /// Bit i = house i has RECEIVED the normal leaflet (0 = pending).
    pub leaflet_flags: i16,
    /// Bit i = house i has RECEIVED the event leaflet (0 = pending).
    pub event_flags: i16,
}

impl LeafletState {
    /// mPO_post_office_init: both masks start at 0xF (all recipients pending,
    /// per the retail init `leaflet_recipient_flags.raw = 0x000F000F`).
    pub fn init() -> Self {
        let mut s = LeafletState {
            leaflet: Mail::default(),
            event_leaflet: Mail::default(),
            leaflet_flags: 0,
            event_flags: 0,
        };
        s.leaflet.clear();
        s.event_leaflet.clear();
        s.leaflet_flags = HOUSE_MASK;
        s.event_flags = HOUSE_MASK;
        s
    }

    /// mPO_receipt_proc for the two leaflet send types: overwrite the
    /// singleton and reset its mask to 0 (all recipients pending). There is
    /// no leaflet queue; a new leaflet replaces an undelivered one.
    pub fn receive(&mut self, mail: &Mail, send_type: u8) -> bool {
        match send_type {
            send_type::LEAFLET => {
                self.leaflet.copy_from_mail(mail);
                self.leaflet_flags = 0;
                true
            }
            send_type::EVENT_LEAFLET => {
                self.event_leaflet.copy_from_mail(mail);
                self.event_flags = 0;
                true
            }
            _ => false,
        }
    }

    /// Special-event setup (m_event.c): a new special event sets
    /// `event_flags = 0b1111`. With the leaflet polarity (0 = pending,
    /// 1 = delivered) this marks all four recipients DELIVERED, i.e. it
    /// suppresses event-leaflet delivery by default; the handbill
    /// registration's `mPO_receipt_proc(EVENT_LEAFLET)` is what later sets
    /// the mask to 0 (all pending).
    pub fn on_special_event_started(&mut self) {
        self.event_flags = HOUSE_MASK;
    }
}

/// Per-house facts the delivery routines read from the save.
#[derive(Clone, Copy, Debug, Default)]
pub struct HouseInfo {
    /// `house->ownerID.land_id`; `0xFFFF` = unclaimed.
    pub land_id: u16,
    /// `player_no = mHS_get_pl_no(house_no)`; foreign players (>=
    /// `mPr_FOREIGNER`) are skipped.
    pub is_foreign_player: bool,
    /// `mEv_ArbeitPlayer(player_no)`: part-time workers are skipped.
    pub is_arbeit: bool,
}

/// A house mailbox as seen by leaflet delivery (10 slots, `HOME_MAILBOX_SIZE`).
#[derive(Clone, Debug)]
pub struct LeafletMailbox {
    pub slots: [Option<Mail>; HOUSE_MAILBOX_SIZE],
}

pub const HOUSE_MAILBOX_SIZE: usize = 10;

impl Default for LeafletMailbox {
    fn default() -> Self {
        LeafletMailbox { slots: [const { None }; HOUSE_MAILBOX_SIZE] }
    }
}

impl LeafletMailbox {
    /// mMl_chk_mail_free_space: first free slot index, or None.
    pub fn free_slot(&self) -> Option<usize> {
        self.slots.iter().position(|s| s.is_none())
    }

    /// mPO_copy_contents: copy into the first free slot. Mailbox full ->
    /// false and the recipient bit stays 0 (retryable).
    pub fn copy_contents(&mut self, mail: &Mail) -> bool {
        match self.free_slot() {
            Some(i) => {
                self.slots[i] = Some(*mail);
                true
            }
            None => false,
        }
    }
}

/// mPO_delivery_one_address_leaflet: deliver the singleton to one house.
/// Attempts only when the recipient bit is 0. Unclaimed houses are marked
/// delivered without receiving mail; foreign/Arbeit players are skipped
/// entirely (bit stays 0). On a full mailbox the bit stays 0 for retry.
pub fn deliver_leaflet_to_house(
    flags: &mut i16,
    leaflet: &Mail,
    house_no: usize,
    house: &HouseInfo,
    mailbox: &mut LeafletMailbox,
) {
    if (*flags & (1 << house_no)) == 0 {
        if house.land_id == UNCLAIMED_LAND_ID {
            *flags |= 1 << house_no;
        } else if !house.is_foreign_player && !house.is_arbeit {
            // retail: mMl_set_to_plname(leaflet, &house->ownerID) here;
            // the engine performs the name set, we do the copy + flag.
            if mailbox.copy_contents(leaflet) {
                *flags |= 1 << house_no;
            }
        }
    }
}

/// mPO_delivery_one_address_event_leaflet: the event leaflet is delivered
/// only when its mail type matches the currently active special event.
/// An expired event (no match) leaves the bit at 0 -- the handbill simply
/// becomes undeliverable rather than being delivered late.
pub fn deliver_event_leaflet_to_house(
    state: &mut LeafletState,
    house_no: usize,
    house: &HouseInfo,
    mailbox: &mut LeafletMailbox,
    special_event_type: i32,
) {
    if event_leaflet_event_type(state.event_leaflet.content.mail_type) == special_event_type {
        let leaflet = state.event_leaflet;
        deliver_leaflet_to_house(&mut state.event_flags, &leaflet, house_no, house, mailbox);
    }
}

/// mPO_delivery_leaflet: run both channels over every house (used by the
/// startup/first-work path; the postman path calls the one-address forms
/// per visited house).
pub fn deliver_leaflets(
    state: &mut LeafletState,
    houses: &[HouseInfo],
    mailboxes: &mut [LeafletMailbox],
    special_event_type: i32,
) {
    for (house_no, (house, mailbox)) in houses.iter().zip(mailboxes.iter_mut()).enumerate() {
        let leaflet = state.leaflet;
        deliver_leaflet_to_house(&mut state.leaflet_flags, &leaflet, house_no, house, mailbox);
        deliver_event_leaflet_to_house(state, house_no, house, mailbox, special_event_type);
    }
}

/// aPMAN_check_delivery: the postman visits a house when normal mail is
/// pending (flag SET) OR a leaflet is pending (flag CLEAR) OR an event
/// leaflet is pending (flag CLEAR). Part-time workers only get normal mail.
///
/// The retail source comment: "normal mail flags are set when mail is to be
/// delivered, leaflet & event flags are set when mail IS delivered."
pub fn postman_check_delivery(
    normal_mail_pending: bool,
    leaflet_flags: i16,
    event_flags: i16,
    house_no: usize,
    is_arbeit: bool,
) -> bool {
    if is_arbeit {
        normal_mail_pending
    } else {
        normal_mail_pending
            || ((leaflet_flags >> house_no) & 1) == 0
            || ((event_flags >> house_no) & 1) == 0
    }
}

/// Select the shop-sale handbill template: `handbill_table[shop_level][category]`.
pub fn shop_sale_handbill(shop_level: usize, category: usize) -> u16 {
    SHOP_SALE_HANDBILLS[shop_level][category]
}

/// Broker handbill: `broker_handbill_no[RANDOM(3)]`; `rng3` is the 0..3 roll.
pub fn broker_handbill(rng3: usize) -> u16 {
    BROKER_HANDBILLS[rng3]
}

/// Rare-furniture chirashi template: `rare_chirashi_bunmen[shop_level][type & 1]`.
pub fn rare_chirashi_template(shop_level: usize, rare_type: usize) -> u16 {
    RARE_CHIRASHI[shop_level][rare_type & 1]
}

/// Build an event handbill mail (aEvMgr_actor_regist_handbill): handbill
/// template from ROM, RECV font, paper offset, mail type. Free-string
/// substitution (item names / dates) happens before this call.
pub fn make_event_handbill(template: u16, paper: u8, mail_type: u8) -> Mail {
    let mut mail = Mail::default();
    mail.clear();
    mail.content.font = font::RECV;
    mail.content.paper_type = paper;
    mail.content.mail_type = mail_type;
    // Retail loads header/body/footer text from the ROM handbill resource;
    // the engine performs that; the template id rides in header_back_start
    // low byte so the C side can complete the load.
    mail.content.header_back_start = (template & 0xFF) as u8;
    mail
}

/// Delivery schedule (mPO_set_next_delivery_time): hour < 9 -> 9:00,
/// hour < 17 -> 17:00, else 09:00 next day. Returns (next_day, hour).
pub fn next_delivery_time(hour: u8) -> (bool, u8) {
    if hour < 9 {
        (false, 9)
    } else if hour < 17 {
        (false, 17)
    } else {
        (true, 9)
    }
}

// ---------------------------------------------------------------------------
// C ABI
// ---------------------------------------------------------------------------

/// C ABI: 0 = pending, 1 = delivered for house `house_no` in `flags`.
#[no_mangle]
pub extern "C" fn pc_leaflet_house_pending(flags: i16, house_no: i32) -> i32 {
    (((flags >> house_no) & 1) == 0) as i32
}

/// C ABI: translate an event leaflet mail type to the required special
/// event type (-1 = not a recognized event leaflet).
#[no_mangle]
pub extern "C" fn pc_leaflet_event_filter(mail_type: u8) -> i32 {
    event_leaflet_event_type(mail_type)
}

/// C ABI: pick the shop-sale handbill template for shop level + category.
#[no_mangle]
pub extern "C" fn pc_leaflet_shop_sale_template(shop_level: i32, category: i32) -> i32 {
    shop_sale_handbill(shop_level as usize, category as usize) as i32
}

/// C ABI: pick the broker handbill template from a 0..3 roll.
#[no_mangle]
pub extern "C" fn pc_leaflet_broker_template(rng3: i32) -> i32 {
    broker_handbill(rng3 as usize) as i32
}

/// C ABI: pick the rare-chirashi template for shop level + rare type bit.
#[no_mangle]
pub extern "C" fn pc_leaflet_rare_chirashi_template(shop_level: i32, rare_type: i32) -> i32 {
    rare_chirashi_template(shop_level as usize, rare_type as usize) as i32
}

/// C ABI: postman visit decision. `normal_pending`: normal-mail flag set.
#[no_mangle]
pub extern "C" fn pc_leaflet_postman_check(
    normal_pending: i32,
    leaflet_flags: i16,
    event_flags: i16,
    house_no: i32,
    is_arbeit: i32,
) -> i32 {
    postman_check_delivery(
        normal_pending != 0,
        leaflet_flags,
        event_flags,
        house_no as usize,
        is_arbeit != 0,
    ) as i32
}

/// C ABI: next delivery slot. Returns (next_day << 8) | hour.
#[no_mangle]
pub extern "C" fn pc_leaflet_next_delivery(hour: u8) -> i32 {
    let (next_day, h) = next_delivery_time(hour);
    ((next_day as i32) << 8) | h as i32
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_mail(mail_type: u8) -> Mail {
        let mut m = Mail::default();
        m.clear();
        m.content.mail_type = mail_type;
        m
    }

    fn houses(n: usize) -> (Vec<HouseInfo>, Vec<LeafletMailbox>) {
        let h = vec![HouseInfo { land_id: 1, ..Default::default() }; n];
        let mb = (0..n).map(|_| LeafletMailbox::default()).collect();
        (h, mb)
    }

    #[test]
    fn init_all_pending() {
        let s = LeafletState::init();
        assert_eq!(s.leaflet_flags, HOUSE_MASK);
        assert_eq!(s.event_flags, HOUSE_MASK);
        assert!(s.leaflet.is_unused());
    }

    #[test]
    fn receive_overwrites_and_resets() {
        let mut s = LeafletState::init();
        s.receive(&sample_mail(mtype::SHOP_SALE_LEAFLET), send_type::LEAFLET);
        assert_eq!(s.leaflet_flags, 0);
        assert_eq!(s.leaflet.content.mail_type, mtype::SHOP_SALE_LEAFLET);
        // A second leaflet replaces the first (no queue).
        s.leaflet_flags = HOUSE_MASK;
        s.receive(&sample_mail(mtype::MAIL), send_type::LEAFLET);
        assert_eq!(s.leaflet_flags, 0);
        assert_eq!(s.leaflet.content.mail_type, mtype::MAIL);
    }

    #[test]
    fn broadcast_one_copy_per_house() {
        let mut s = LeafletState::init();
        s.receive(&sample_mail(mtype::MAIL), send_type::LEAFLET);
        let (houses, mut mailboxes) = houses(4);
        deliver_leaflets(&mut s, &houses, &mut mailboxes, -1);
        assert_eq!(s.leaflet_flags, 0xF);
        for mb in &mailboxes {
            assert_eq!(mb.slots.iter().filter(|x| x.is_some()).count(), 1);
        }
    }

    #[test]
    fn mailbox_full_stays_pending_and_retries() {
        let mut s = LeafletState::init();
        s.receive(&sample_mail(mtype::MAIL), send_type::LEAFLET);
        let (houses, mut mailboxes) = houses(2);
        // Fill house 0's mailbox completely.
        let filler = sample_mail(mtype::XMAS);
        for _ in 0..HOUSE_MAILBOX_SIZE {
            assert!(mailboxes[0].copy_contents(&filler));
        }
        deliver_leaflets(&mut s, &houses, &mut mailboxes, -1);
        assert_eq!(s.leaflet_flags & 1, 0); // house 0 still pending
        assert_eq!(s.leaflet_flags & 2, 2); // house 1 delivered
        // Free a slot; next pass delivers.
        mailboxes[0].slots[0] = None;
        deliver_leaflets(&mut s, &houses, &mut mailboxes, -1);
        assert_eq!(s.leaflet_flags, 0x3); // both houses (2-house test)
    }

    #[test]
    fn unclaimed_house_marked_without_mail() {
        let mut s = LeafletState::init();
        s.receive(&sample_mail(mtype::MAIL), send_type::LEAFLET);
        let mut houses = vec![HouseInfo { land_id: UNCLAIMED_LAND_ID, ..Default::default() }];
        let mut mailboxes = vec![LeafletMailbox::default()];
        deliver_leaflets(&mut s, &houses, &mut mailboxes, -1);
        assert_eq!(s.leaflet_flags, 0x1); // single house
        assert!(mailboxes[0].slots.iter().all(|x| x.is_none()));
    }

    #[test]
    fn arbeit_and_foreign_skipped() {
        let mut s = LeafletState::init();
        s.receive(&sample_mail(mtype::MAIL), send_type::LEAFLET);
        let houses = vec![
            HouseInfo { land_id: 1, is_arbeit: true, ..Default::default() },
            HouseInfo { land_id: 2, is_foreign_player: true, ..Default::default() },
        ];
        let mut mailboxes = vec![LeafletMailbox::default(), LeafletMailbox::default()];
        deliver_leaflets(&mut s, &houses, &mut mailboxes, -1);
        assert_eq!(s.leaflet_flags, 0); // both stay pending
    }

    #[test]
    fn event_leaflet_needs_matching_event() {
        let mut s = LeafletState::init();
        s.receive(&sample_mail(mtype::SHOP_SALE_LEAFLET), send_type::EVENT_LEAFLET);
        let (houses, mut mailboxes) = houses(1);
        // Wrong / no event: not delivered, bit stays 0.
        deliver_leaflets(&mut s, &houses, &mut mailboxes, event::BROKER_SALE);
        assert_eq!(s.event_flags, 0);
        assert!(mailboxes[0].slots.iter().all(|x| x.is_none()));
        // Matching event: delivered.
        deliver_leaflets(&mut s, &houses, &mut mailboxes, event::SHOP_SALE);
        assert_eq!(s.event_flags, 0x1); // single house
        assert_eq!(mailboxes[0].slots.iter().filter(|x| x.is_some()).count(), 1);
    }

    #[test]
    fn event_filter_recognizes_only_two_types() {
        assert_eq!(event_leaflet_event_type(mtype::SHOP_SALE_LEAFLET), event::SHOP_SALE);
        assert_eq!(event_leaflet_event_type(mtype::BROKER_SALE_LEAFLET), event::BROKER_SALE);
        assert_eq!(event_leaflet_event_type(mtype::MAIL), -1);
        assert_eq!(event_leaflet_event_type(mtype::XMAS), -1);
    }

    #[test]
    fn special_event_start_suppresses_event_leaflet() {
        let mut s = LeafletState::init();
        s.event_flags = 0; // pending from an earlier handbill
        s.on_special_event_started();
        assert_eq!(s.event_flags, HOUSE_MASK); // all marked delivered
    }

    #[test]
    fn postman_visit_logic() {
        // No pending anything -> no visit.
        assert!(!postman_check_delivery(false, 0xF, 0xF, 0, false));
        // Normal mail pending -> visit.
        assert!(postman_check_delivery(true, 0xF, 0xF, 1, false));
        // Leaflet pending (bit clear) -> visit.
        assert!(postman_check_delivery(false, 0xF & !4, 0xF, 2, false));
        // Event leaflet pending -> visit.
        assert!(postman_check_delivery(false, 0xF, 0xF & !8, 3, false));
        // Arbeit worker: only normal mail counts.
        assert!(!postman_check_delivery(false, 0, 0, 0, true));
        assert!(postman_check_delivery(true, 0xF, 0xF, 0, true));
    }

    #[test]
    fn handbill_tables() {
        assert_eq!(shop_sale_handbill(0, 0), 0x002);
        assert_eq!(shop_sale_handbill(3, 3), 0x011);
        assert_eq!(broker_handbill(0), 0x031);
        assert_eq!(broker_handbill(2), 0x033);
        assert_eq!(rare_chirashi_template(2, 1), 20);
        assert_eq!(rare_chirashi_template(3, 0), 23);
    }

    #[test]
    fn delivery_schedule() {
        assert_eq!(next_delivery_time(8), (false, 9));
        assert_eq!(next_delivery_time(9), (false, 17));
        assert_eq!(next_delivery_time(16), (false, 17));
        assert_eq!(next_delivery_time(17), (true, 9));
        assert_eq!(next_delivery_time(23), (true, 9));
    }

    #[test]
    fn make_event_handbill_fields() {
        let m = make_event_handbill(0x005, paper_offset::PAPER55, mtype::SHOP_SALE_LEAFLET);
        assert_eq!(m.content.font, font::RECV);
        assert_eq!(m.content.paper_type, paper_offset::PAPER55);
        assert_eq!(m.content.mail_type, mtype::SHOP_SALE_LEAFLET);
        assert_eq!(m.content.header_back_start, 0x05);
    }
}
