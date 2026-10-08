//! Catalog orders, lottery tickets, and the special-delivery mail path.
//!
//! Ports the retail USA decomp for GAFE01_00 Rev. 0:
//! `include/m_private.h` (order/ticket save fields),
//! `src/actor/npc/ac_npc_shop_common.c` (order placement, ticket overflow),
//! `src/actor/npc/ac_npc_shop_mastersp_talk.c_inc` (lottery play),
//! `src/game/m_shop.c` (lottery lineup), and
//! `src/game/m_post_office.c` (special delivery).
//!
//! Three separate persistent mechanisms, not one generic queue:
//! - `Private_c.catalog_orders[5]` — per-player pending furniture orders
//!   (item + shop level at order time).
//! - `Shop_c.lottery_items[3]` — per-town monthly lottery prizes.
//! - `Private_c` ticket overflow — `lotto_ticket_expiry_month` +
//!   `lotto_ticket_mail_storage` (a u8 count, not an item).
//!
//! The delivery pipeline (`mPO_delivery_one_address_special_mail`) mails
//! pending tickets FIRST (in stacks of at most five), then catalog orders
//! in ascending slot order. Each generated mail is transactional: the
//! order slot / ticket count only clears after the mail lands in a free
//! mailbox slot; on failure the remainder stays pending.
//!
//! Caller resolution (new finding vs. earlier tracing): the decomp does
//! contain the trigger. `mPO_first_work()` (game start) ->
//! `mPO_first_delivery_proc()` calls
//! `mPO_delivery_one_address_special_mail(house_no)` for the current
//! player's house whenever the player is a local (non-foreigner) player,
//! independent of whether ordinary mail was due.

/// Pending catalog furniture orders per player (`mPr_CATALOG_ORDER_NUM`).
pub const CATALOG_ORDER_NUM: usize = 5;

/// Pending lottery prizes per shop (`mSP_LOTTERY_ITEM_COUNT`).
pub const LOTTERY_ITEM_COUNT: usize = 3;

/// Lottery tickets required per play (`aSHM_REQ_TICKET_NUM`).
pub const LOTTERY_REQ_TICKETS: u8 = 5;

/// Pending ticket mail storage cap (`aNSC_MAX_TICKETS`).
pub const MAX_TICKETS: u8 = 255;

/// Ticket item id range (`ITM_TICKET_START` .. `ITM_TICKET_END`).
pub const TICKET_START: u16 = 0x2C00;
pub const TICKET_END: u16 = 0x2C00 + 95;

/// Mailbox slots per house (`HOME_MAILBOX_SIZE`).
pub const MAILBOX_SIZE: usize = 10;

/// Empty item id.
pub const EMPTY_NO: u16 = 0x0000;

/// Consumed lottery prize marker.
pub const RSV_SHOP_SOLD_FTR: u16 = 0xFE10;

/// Special-delivery mail: paper and type (`ITM_PAPER55`, mail_type 7).
pub const DELIVERY_PAPER: u16 = 0x2037;
pub const DELIVERY_MAIL_TYPE: u8 = 7;
/// Ticket mail template id (0x057); catalog uses 0x049 + shop_level.
pub const TICKET_MAIL_TEMPLATE: u16 = 0x057;
pub const CATALOG_MAIL_TEMPLATE_BASE: u16 = 0x049;

/// Lottery result tiers.
pub mod lottery_result {
    pub const FIRST: u8 = 0;
    pub const SECOND: u8 = 1;
    pub const THIRD: u8 = 2;
    pub const DIDNT_WIN: u8 = 3;
}

/// Lottery odds: cumulative thresholds on RANDOM(100).
/// 0-4 first (5%), 5-14 second (10%), 15-34 third (20%), 35-99 no win (65%).
pub const FIRST_PLACE_PERCENT: u8 = 5;
pub const SECOND_PLACE_PERCENT: u8 = 15; // 10 + 5
pub const THIRD_PLACE_PERCENT: u8 = 35; // 20 + 15

/// One pending catalog order: the item plus the shop level captured at
/// order time (the delivery letter uses `0x049 + shop_level`, so a later
/// shop upgrade does not change the letter).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CatalogOrder {
    pub item: u16,
    pub shop_level: u8,
}

/// The five-slot pending catalog queue (`Private_c.catalog_orders`).
#[derive(Clone, Debug)]
pub struct CatalogOrders {
    pub slots: [CatalogOrder; CATALOG_ORDER_NUM],
}

impl Default for CatalogOrders {
    fn default() -> Self {
        CatalogOrders {
            slots: [CatalogOrder::default(); CATALOG_ORDER_NUM],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OrderError {
    /// All five slots occupied (`aNSC_MSG_ORDER_FULL`).
    Full,
}

impl CatalogOrders {
    /// `aNSC_getP_free_ftr_order`: first slot whose item is EMPTY_NO,
    /// scanned in ascending index order.
    pub fn free_slot(&self) -> Option<usize> {
        self.slots.iter().position(|o| o.item == EMPTY_NO)
    }

    /// `aNSC_set_ftr_order`: commit an already-paid order.
    pub fn place(&mut self, item: u16, shop_level: u8) -> Result<usize, OrderError> {
        match self.free_slot() {
            Some(i) => {
                self.slots[i] = CatalogOrder { item, shop_level };
                Ok(i)
            }
            None => Err(OrderError::Full),
        }
    }

    /// Delivery clears a slot only after its mail lands.
    pub fn clear(&mut self, idx: usize) {
        self.slots[idx].item = EMPTY_NO;
    }
}

/// Ticket item encoding (`include/m_name_table.h`).
pub fn ticket_month(item: u16) -> u8 {
    (1 + (((item) >> 3) & 0xF)) as u8
}

pub fn ticket_count(item: u16) -> u8 {
    (1 + ((item) & 7)) as u8
}

/// Build a ticket item id for `month` (1-12) holding `count` (1-5) tickets.
/// The low 3 bits store `count - 1` (retail forms these as
/// `ticket + minus_ticket - 1`).
pub fn ticket_get_item(month: u8, count: u8) -> u16 {
    TICKET_START | (((month as u16) - 1) << 3) | (((count as u16) - 1) & 7)
}

pub fn is_ticket_item(item: u16) -> bool {
    (TICKET_START..=TICKET_END).contains(&item)
}

/// Pending ticket mail state (`lotto_ticket_expiry_month` +
/// `lotto_ticket_mail_storage`).
#[derive(Clone, Copy, Debug, Default)]
pub struct TicketOverflow {
    pub expiry_month: u8,
    pub mail_storage: u8,
}

impl TicketOverflow {
    /// `aNSC_setup_ticket_remain`: month change resets the pending count;
    /// then +1, capped at 255.
    pub fn add_ticket(&mut self, current_month: u8) {
        let mut tickets = self.mail_storage;
        if current_month != self.expiry_month {
            tickets = 0;
            self.expiry_month = current_month;
        }
        tickets = tickets.saturating_add(1).min(MAX_TICKETS);
        self.mail_storage = tickets;
    }
}

/// Lottery roll: `RANDOM(100)` against the cumulative thresholds.
pub fn lottery_roll(rng_100: u8) -> u8 {
    if rng_100 < FIRST_PLACE_PERCENT {
        lottery_result::FIRST
    } else if rng_100 < SECOND_PLACE_PERCENT {
        lottery_result::SECOND
    } else if rng_100 < THIRD_PLACE_PERCENT {
        lottery_result::THIRD
    } else {
        lottery_result::DIDNT_WIN
    }
}

/// `to_atari` availability: a prize slot that is sold out or empty
/// converts the win to DIDNT_WIN (no reroll).
pub fn prize_available(prize: u16) -> bool {
    prize != RSV_SHOP_SOLD_FTR && prize != EMPTY_NO
}

/// Monthly lineup (`mSP_MakeLotteryList`): if an uncollected lottery
/// furniture item exists it takes slot 0 and slots 1-2 are random
/// excluding it; otherwise all three are random.
pub fn make_lottery_lineup(
    unobtained: Option<u16>,
    random_rest: &[u16],
) -> [u16; LOTTERY_ITEM_COUNT] {
    let mut items = [EMPTY_NO; LOTTERY_ITEM_COUNT];
    match unobtained {
        Some(u) => {
            items[0] = u;
            for (i, &r) in random_rest.iter().take(2).enumerate() {
                items[i + 1] = r;
            }
        }
        None => {
            for (i, &r) in random_rest.iter().take(3).enumerate() {
                items[i] = r;
            }
        }
    }
    items
}

/// Count valid tickets for `month` across the pockets (slot order).
/// A ticket counts when its month matches; condition checks are engine-side.
pub fn count_tickets(pockets: &[u16], month: u8) -> u8 {
    let mut count = 0u8;
    for &item in pockets {
        if count >= LOTTERY_REQ_TICKETS {
            break;
        }
        if is_ticket_item(item) && ticket_month(item) == month {
            count = count.saturating_add(ticket_count(item));
        }
    }
    count.min(LOTTERY_REQ_TICKETS + 10) // raw sum; caller caps at need
}

/// Consume `req` tickets scanning slots 0 upward, mirroring the retail
/// loop: `req -= count` runs even when `count > req` (negative req ends
/// the loop via the `req > 0` condition). Returns per-slot removals as
/// (slot, removed_count, leftover_count).
pub fn consume_tickets(
    pockets: &mut [u16],
    month: u8,
    mut req: i16,
) -> Vec<(usize, u8, u8)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < pockets.len() && req > 0 {
        let item = pockets[i];
        if is_ticket_item(item) && ticket_month(item) == month {
            let count = ticket_count(item) as i16;
            if count <= req {
                pockets[i] = EMPTY_NO;
                out.push((i, count as u8, 0));
            } else {
                let leftover = (count - req) as u8;
                pockets[i] = ticket_get_item(month, leftover);
                out.push((i, req as u8, leftover));
            }
            req -= count; // retail does NOT clamp to min(count, req)
        }
        i += 1;
    }
    out
}

/// A generated special-delivery mail.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpecialMail {
    /// Template id: 0x057 for tickets, 0x049 + shop_level for catalog.
    pub template: u16,
    /// The attached present (furniture or ticket item).
    pub present: u16,
}

/// Minimal mailbox model: `mPO_copy_contents` needs one free slot.
#[derive(Clone, Debug)]
pub struct Mailbox {
    pub slots: [Option<SpecialMail>; MAILBOX_SIZE],
}

impl Default for Mailbox {
    fn default() -> Self {
        Mailbox {
            slots: [None, None, None, None, None, None, None, None, None, None],
        }
    }
}

impl Mailbox {
    pub fn free_slot(&self) -> Option<usize> {
        self.slots.iter().position(|s| s.is_none())
    }

    /// Returns false when the mailbox is full (mail not inserted).
    pub fn insert(&mut self, mail: SpecialMail) -> bool {
        match self.free_slot() {
            Some(i) => {
                self.slots[i] = Some(mail);
                true
            }
            None => false,
        }
    }
}

/// Outcome of the special-delivery pass.
#[derive(Clone, Debug, Default)]
pub struct DeliveryOutcome {
    pub ticket_mails: u8,
    pub order_mails: u8,
    /// Mailbox filled mid-run; remaining items stay pending.
    pub stopped_early: bool,
}

/// `mPO_delivery_one_address_special_mail`: pending tickets FIRST
/// (stacks of at most five: `ticket_id = base + minus - 1`), then catalog
/// orders in ascending slot order. Each mail is transactional.
pub fn deliver_special_mail(
    tickets: &mut TicketOverflow,
    orders: &mut CatalogOrders,
    mailbox: &mut Mailbox,
) -> DeliveryOutcome {
    let mut out = DeliveryOutcome::default();

    // Ticket overflow path.
    let mut ticket_num = tickets.mail_storage;
    if ticket_num != 0 {
        let base = TICKET_START + ((tickets.expiry_month as u16) - 1) * 8;
        while ticket_num != 0 {
            let minus = ticket_num.min(5);
            let ticket_id = base + (minus as u16) - 1;
            let mail = SpecialMail {
                template: TICKET_MAIL_TEMPLATE,
                present: ticket_id,
            };
            if !mailbox.insert(mail) {
                out.stopped_early = true;
                break;
            }
            ticket_num -= minus;
            out.ticket_mails += 1;
        }
        tickets.mail_storage = ticket_num;
    }

    // Catalog order path.
    for i in 0..CATALOG_ORDER_NUM {
        if orders.slots[i].item != EMPTY_NO {
            let mail = SpecialMail {
                template: CATALOG_MAIL_TEMPLATE_BASE + orders.slots[i].shop_level as u16,
                present: orders.slots[i].item,
            };
            if !mailbox.insert(mail) {
                out.stopped_early = true;
                break;
            }
            orders.clear(i);
            out.order_mails += 1;
        }
    }

    out
}

/// The resolved delivery trigger (`mPO_first_delivery_proc`): at game
/// start, special mail is delivered to the current player's house when the
/// player is a local (non-foreigner) player. Returns the house to deliver
/// to, or None when no delivery happens.
pub fn first_delivery_special_mail_house(player_no: u8, house_no: u8) -> Option<u8> {
    // mLd_PlayerManKindCheckNo: foreigner when player_no >= PLAYER_NUM.
    // Retail PLAYER_NUM is 4.
    if player_no < 4 {
        Some(house_no)
    } else {
        None
    }
}

// ---- C ABI ----

#[no_mangle]
pub extern "C" fn pc_catalog_order_num() -> u8 {
    CATALOG_ORDER_NUM as u8
}

#[no_mangle]
pub extern "C" fn pc_lottery_item_count() -> u8 {
    LOTTERY_ITEM_COUNT as u8
}

#[no_mangle]
pub extern "C" fn pc_lottery_req_tickets() -> u8 {
    LOTTERY_REQ_TICKETS
}

#[no_mangle]
pub extern "C" fn pc_ticket_month(item: u16) -> u8 {
    ticket_month(item)
}

#[no_mangle]
pub extern "C" fn pc_ticket_count(item: u16) -> u8 {
    ticket_count(item)
}

#[no_mangle]
pub extern "C" fn pc_lottery_roll(rng_100: u8) -> u8 {
    lottery_roll(rng_100)
}

#[no_mangle]
pub extern "C" fn pc_catalog_mail_template(shop_level: u8) -> u16 {
    CATALOG_MAIL_TEMPLATE_BASE + shop_level as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_queue_capacity() {
        let mut q = CatalogOrders::default();
        for i in 0..5 {
            assert_eq!(q.place(0x1000 + i as u16, 2), Ok(i));
        }
        // Sixth order fails: ORDER_FULL.
        assert_eq!(q.place(0x2000, 2), Err(OrderError::Full));
        // Freeing a slot reopens it (ascending scan).
        q.clear(2);
        assert_eq!(q.place(0x2000, 3), Ok(2));
        assert_eq!(q.slots[2].shop_level, 3);
    }

    #[test]
    fn ticket_encoding() {
        // month 3, count 5 -> item; decode round-trips.
        let item = ticket_get_item(3, 5);
        assert!(is_ticket_item(item));
        assert_eq!(ticket_month(item), 3);
        assert_eq!(ticket_count(item), 5);
        assert_eq!(ticket_get_item(1, 1), TICKET_START);
        assert!(!is_ticket_item(0x1000));
    }

    #[test]
    fn ticket_overflow_month_reset_and_cap() {
        let mut t = TicketOverflow::default();
        t.add_ticket(5);
        t.add_ticket(5);
        assert_eq!(t.mail_storage, 2);
        assert_eq!(t.expiry_month, 5);
        // Month change resets.
        t.add_ticket(6);
        assert_eq!(t.mail_storage, 1);
        assert_eq!(t.expiry_month, 6);
        // Cap at 255.
        t.mail_storage = 255;
        t.add_ticket(6);
        assert_eq!(t.mail_storage, 255);
    }

    #[test]
    fn lottery_odds_boundaries() {
        assert_eq!(lottery_roll(0), lottery_result::FIRST);
        assert_eq!(lottery_roll(4), lottery_result::FIRST);
        assert_eq!(lottery_roll(5), lottery_result::SECOND);
        assert_eq!(lottery_roll(14), lottery_result::SECOND);
        assert_eq!(lottery_roll(15), lottery_result::THIRD);
        assert_eq!(lottery_roll(34), lottery_result::THIRD);
        assert_eq!(lottery_roll(35), lottery_result::DIDNT_WIN);
        assert_eq!(lottery_roll(99), lottery_result::DIDNT_WIN);
        assert!(!prize_available(RSV_SHOP_SOLD_FTR));
        assert!(!prize_available(EMPTY_NO));
        assert!(prize_available(0x1234));
    }

    #[test]
    fn lineup_uncollected_first() {
        let l = make_lottery_lineup(Some(0x1111), &[0x2222, 0x3333]);
        assert_eq!(l, [0x1111, 0x2222, 0x3333]);
        let l2 = make_lottery_lineup(None, &[0x2222, 0x3333, 0x4444]);
        assert_eq!(l2, [0x2222, 0x3333, 0x4444]);
    }

    #[test]
    fn ticket_consumption_slot_order() {
        let mut pockets = [EMPTY_NO; 15];
        pockets[0] = ticket_get_item(5, 3);
        pockets[2] = ticket_get_item(5, 4);
        pockets[3] = ticket_get_item(6, 5); // wrong month
        let used = consume_tickets(&mut pockets, 5, 5);
        // Slot 0 fully consumed (3), slot 2 partially (2 of 4), req went negative.
        assert_eq!(used, vec![(0, 3, 0), (2, 2, 2)]);
        assert_eq!(pockets[0], EMPTY_NO);
        assert_eq!(ticket_count(pockets[2]), 2);
        assert_eq!(pockets[3], ticket_get_item(6, 5));
    }

    #[test]
    fn special_delivery_tickets_first_then_orders() {
        let mut tickets = TicketOverflow { expiry_month: 5, mail_storage: 7 };
        let mut orders = CatalogOrders::default();
        orders.place(0x3001, 2).unwrap();
        orders.place(0x3002, 4).unwrap();
        let mut mb = Mailbox::default();
        let out = deliver_special_mail(&mut tickets, &mut orders, &mut mb);
        // 7 tickets -> 5 + 2 (two mails), then two order mails.
        assert_eq!(out.ticket_mails, 2);
        assert_eq!(out.order_mails, 2);
        assert!(!out.stopped_early);
        assert_eq!(tickets.mail_storage, 0);
        assert!(orders.slots.iter().all(|o| o.item == EMPTY_NO));
        // Ticket mail used 0x057; order mail used 0x049 + shop_level.
        let mut templates: Vec<u16> = mb.slots.iter().filter_map(|s| s.as_ref().map(|m| m.template)).collect();
        templates.sort();
        assert_eq!(templates, vec![0x049 + 2, 0x049 + 4, 0x057, 0x057]);
        // Ticket presents encode stacks 5 and 2.
        let mut counts: Vec<u8> = mb.slots.iter()
            .filter_map(|s| s.as_ref())
            .filter(|m| m.template == 0x057)
            .map(|m| ticket_count(m.present))
            .collect();
        counts.sort();
        assert_eq!(counts, vec![2, 5]);
    }

    #[test]
    fn special_delivery_stops_at_full_mailbox() {
        let mut tickets = TicketOverflow { expiry_month: 5, mail_storage: 12 };
        let mut orders = CatalogOrders::default();
        orders.place(0x3001, 1).unwrap();
        let mut mb = Mailbox::default();
        // Fill 9 of 10 slots: only one ticket mail fits.
        for i in 0..9 {
            mb.slots[i] = Some(SpecialMail { template: 0, present: 0 });
        }
        let out = deliver_special_mail(&mut tickets, &mut orders, &mut mb);
        assert_eq!(out.ticket_mails, 1);
        assert_eq!(tickets.mail_storage, 7); // remainder preserved
        assert_eq!(out.order_mails, 0); // not attempted
        assert!(out.stopped_early);
        assert_eq!(orders.slots[0].item, 0x3001); // slot intact
    }

    #[test]
    fn delivery_trigger_only_for_locals() {
        assert_eq!(first_delivery_special_mail_house(0, 2), Some(2));
        assert_eq!(first_delivery_special_mail_house(3, 1), Some(1));
        assert_eq!(first_delivery_special_mail_house(4, 0), None);
    }
}
