//! Inventory screen logic (`m_inventory_ovl.c`) and the action-menu and
//! mark rules it uses from the tag overlay (`m_tag_ovl.c`
//! `mTG_select_tag_decide_*`, `mTG_mark_*`). The submenu lifecycle and the
//! per-mode pocket eligibility are in `submenu.rs`; hand stacking, money
//! bags and the card-mail page order are in `menu_overlays.rs`.
//!
//! Source-verified against ac-decomp 09ca8e8b (primary) and ACGC-PC-Port
//! (secondary). Item predicates come from mingw32 gcc over the
//! `m_name_table.h` macros. Inventory/mail mutation, the hand transfer,
//! animation, sound and drawing stay in C.
//!
//! ## Opening (`mIV_inventory_ovl_init`)
//! Selectable masks per open mode: gyroid-take none; quest only slot
//! `data1`; password = `data1` as the mask; send-mail = letters that exist
//! and can be sent; card mail = the **opposite** (empty or unsendable);
//! mailbox = all letters; otherwise the `submenu.rs` eligibility mask,
//! with all letters also selectable in normal and exchange modes. Mailbox,
//! card mail and gyroid modes immediately open their companion menu and
//! the inventory follows it (OBEY). Pages start [inventory, fish, insect].
//! The cursor starts on the first selectable slot (else slot 0).
//!
//! ## Page turn
//! 40-frame turn, `y = 100 sin(t * 4.5 deg)`; at t = 20 the new page moves
//! to the front, and the back page is only replaced if it was the new one.
//!
//! ## Action menu for an item (`mTG_select_tag_decide_item_normal`)
//! In priority order: a marked item -> "put all"; a present -> present
//! menu; a quest item -> catch only; the signboard outdoors -> sign menu;
//! exercise cards and the knife and fork -> card menu; furniture uses the
//! clothing column; spirits; letters (stacks use the multi-letter menus);
//! balloons; otherwise by place (outside / my room / other rooms) and
//! category. Tickets whose id is a multiple of 8 count as ETC.
//! Afterwards: a fish with water nearby -> release; with a shovel equipped
//! and a diggable spot, the outdoor menus become their "bury" variants
//! (not for bugs or the signboard); on the first-job day planting becomes
//! the job-plant menu.
//!
//! ## Quirks
//! - The basement carpet/wallpaper exclusion is marked `@BUG` for using `|`
//!   instead of `||`; both operands are 0/1, so it behaves the same.
//! - The one-spirit and many-spirits tables are identical.
//! - `mTG_bit_chk_all0` returns true if **any** byte is zero, not all;
//!   no caller uses that mode.
//! - Debug (zurumode 2, controller 2, normal mode): C-left +1000 Bells,
//!   C-down fills free slots with tools, C-right with 30,000-Bell bags.
//! - Selling: money can't be marked, and turnips can't on Sundays.

/// `mTG_TYPE_*` (subset used here; values verified against the header).
pub mod tag {
    pub const NONE: u8 = 0;
    pub const FIELD_DEFAULT: u8 = 1;
    pub const FIELD_DEFAULT_BURY: u8 = 2;
    pub const FIELD_LETTER: u8 = 3;
    pub const FIELD_LETTER_BURY: u8 = 4;
    pub const FIELD_HUKUBUKURO: u8 = 5;
    pub const FIELD_HUKUBUKURO_BURY: u8 = 6;
    pub const FIELD_RELEASE: u8 = 7;
    pub const CATCH_ITEM: u8 = 8;
    pub const FIELD_PLANT: u8 = 9;
    pub const FIELD_PLANT_JOB: u8 = 10;
    pub const PRESENT_ITEM: u8 = 11;
    pub const ROOM_DEFAULT: u8 = 12;
    pub const ROOM_WALL: u8 = 13;
    pub const ROOM_CARPET: u8 = 14;
    pub const ROOM_LETTER: u8 = 15;
    pub const ROOM_HUKUBUKURO: u8 = 16;
    pub const OTHER_LETTER: u8 = 17;
    pub const OTHER_HUKUBUKURO: u8 = 18;
    pub const DEF_RMAIL: u8 = 19;
    pub const ROOM_RMAIL: u8 = 20;
    pub const DEF_RMAIL_PRE: u8 = 21;
    pub const DEF_SMAIL: u8 = 22;
    pub const ROOM_SMAIL: u8 = 23;
    pub const DEF_SMAIL_PRE: u8 = 24;
    pub const MONEY_SACK: u8 = 26;
    pub const QUEST_ITEM: u8 = 27;
    pub const SELL_ITEM: u8 = 28;
    pub const GIVE_ITEM: u8 = 29;
    pub const SEND_MAIL: u8 = 30;
    pub const FIELD_TICKET: u8 = 31;
    pub const FIELD_TICKET_BURY: u8 = 32;
    pub const ROOM_TICKET: u8 = 33;
    pub const CATCH_TICKET: u8 = 34;
    pub const PUTIN_ITEM: u8 = 35;
    pub const SHRINE_ITEM: u8 = 36;
    pub const FIELD_MAIL_MARK: u8 = 44;
    pub const FIELD_MAIL_MARK2: u8 = 46;
    pub const CPACK_MAIL_MARK: u8 = 47;
    pub const SELL_ALL_ITEM: u8 = 48;
    pub const TAG_HAND_OVER_CURATOR: u8 = 53;
    pub const TAG_PUT_ALL: u8 = 63;
    pub const FIELD_LETTERS: u8 = 65;
    pub const FIELD_LETTERS_BURY: u8 = 66;
    pub const ROOM_LETTERS: u8 = 67;
    pub const OTHER_LETTERS: u8 = 68;
    pub const TAISOU_CARD: u8 = 70;
    pub const FIELD_SIGN: u8 = 72;
    pub const FIELD_BALLOON: u8 = 73;
    pub const TAG_PASSWORD_ITEM: u8 = 77;
}

use crate::submenu::{iv_open, ovl};

pub const POCKETS: usize = 15;
pub const MAIL_SLOTS: usize = 10;
pub const PAGE_FISH: u8 = 0;
pub const PAGE_INVENTORY: u8 = 1;
pub const PAGE_INSECT: u8 = 2;
pub const PAGE_MOVE_FRAMES: i16 = 40;
pub const PAGE_SWAP_FRAME: i16 = 20;

/// Field types (`mFI_FIELDTYPE2_*`).
pub const FIELD_FG: u8 = 0;
pub const FIELD_PLAYER_ROOM: u8 = 1;
pub const FIELD_NPC_ROOM: u8 = 2;
pub const FIELD_ROOM: u8 = 3;

/// Letter state for selection rules.
#[derive(Clone, Copy, Debug, Default)]
pub struct Letter {
    pub used: bool,
    pub sendable: bool,
}

/// Selectable (item, mail) masks at open. `item_mask` = the `submenu.rs`
/// eligibility mask for modes that use it.
pub fn open_masks(mode: u8, data1: i32, item_mask: u16, letters: &[Letter; MAIL_SLOTS]) -> (u16, u16) {
    let mails = |f: &dyn Fn(&Letter) -> bool| letters.iter().enumerate().filter(|(_, l)| f(l)).fold(0u16, |m, (i, _)| m | 1 << i);
    match mode {
        iv_open::HANIWA_TAKE => (0, 0),
        iv_open::QUEST => (1u16.wrapping_shl(data1 as u32), 0),
        iv_open::PASSWORD => (data1 as u16, 0),
        iv_open::SEND_MAIL => (0, mails(&|l| l.used && l.sendable)),
        iv_open::MAILBOX => (0, 0xFFFF),
        iv_open::CPMAIL => (0, mails(&|l| !l.used || !l.sendable)),
        iv_open::NORMAL | iv_open::EXCHANGE => (item_mask, 0xFFFF),
        _ => (item_mask, 0),
    }
}

/// Companion menu opened with the inventory (then OBEY).
pub fn companion_menu(mode: u8) -> Option<u8> {
    match mode {
        iv_open::CPMAIL => Some(ovl::CPMAIL),
        iv_open::MAILBOX => Some(ovl::MAILBOX),
        iv_open::HANIWA_ENTRUST | iv_open::HANIWA_TAKE => Some(ovl::HANIWA),
        _ => None,
    }
}

/// Initial cursor (`mIV_move_Play` first frame): first set bit, else 0.
/// Send-mail mode searches the 10 letters, others the 15 pockets.
pub fn initial_cursor(mode: u8, item_mask: u16, mail_mask: u16) -> usize {
    let (mask, n) = if mode == iv_open::SEND_MAIL { (mail_mask, MAIL_SLOTS) } else { (item_mask, POCKETS) };
    (0..n).find(|&i| mask & (1 << i) != 0).unwrap_or(0)
}

/// Inventory page swap at frame 20.
pub fn page_swap(order: &mut [u8; 3], next: u8) {
    if order[2] == next {
        order[2] = order[1];
    }
    order[1] = order[0];
    order[0] = next;
}

/// `mIV_up_page_draw_check`.
pub fn page_drawn_on_top(order: &[u8; 3], next: u8, timer: i16, page: u8) -> bool {
    if timer > PAGE_SWAP_FRAME {
        next == page || order[0] == page
    } else if timer != 0 {
        order[2] != page
    } else {
        order[0] == page
    }
}

/// Item facts the action-menu rules need (gcc-verified ranges).
pub mod item {
    pub fn kind(i: u16) -> u16 {
        (i & 0xF000) >> 12
    }
    pub fn cat(i: u16) -> u16 {
        (i & 0x0F00) >> 8
    }
    pub fn is_ftr(i: u16) -> bool {
        matches!(kind(i), 1 | 3)
    }
    pub fn is_wisp(i: u16) -> bool {
        (0x2D28..=0x2D2C).contains(&i)
    }
    pub fn is_paper(i: u16) -> bool {
        (0x2000..=0x20FF).contains(&i)
    }
    /// `PAPER2STACK((i - ITM_PAPER_START))`, 64 designs per stack size.
    pub fn paper_stack(i: u16) -> u16 {
        (i - 0x2000) / 64
    }
    pub fn is_balloon(i: u16) -> bool {
        (0x2244..=0x224B).contains(&i)
    }
    pub fn is_card_or_fork(i: u16) -> bool {
        (0x2523..=0x2530).contains(&i)
    }
    pub fn is_cloth(i: u16) -> bool {
        (0x2400..=0x24FE).contains(&i)
    }
    pub fn is_umbrella(i: u16) -> bool {
        (0x2204..=0x222B).contains(&i)
    }
    pub fn is_shovel(i: u16) -> bool {
        i == 0x2202 || i == 0x223B
    }
    pub const SIGNBOARD: u16 = 0x251E;
    pub const ITEM1: u16 = 2;
    pub const CAT_PAPER: u16 = 0;
    pub const CAT_MONEY: u16 = 1;
    pub const CAT_FISH: u16 = 3;
    pub const CAT_CLOTH: u16 = 4;
    pub const CAT_ETC: u16 = 5;
    pub const CAT_CARPET: u16 = 6;
    pub const CAT_WALL: u16 = 7;
    pub const CAT_PLANT: u16 = 9;
    pub const CAT_TICKET: u16 = 12;
    pub const CAT_INSECT: u16 = 13;
    pub const CAT_KABU: u16 = 15;
}

use tag::*;

/// Per-category action menus outside, in my room, in other rooms.
pub const TAG_FIELD: [u8; 16] = [
    FIELD_LETTER, FIELD_DEFAULT, FIELD_DEFAULT, CATCH_ITEM, FIELD_DEFAULT, FIELD_DEFAULT, FIELD_DEFAULT, FIELD_DEFAULT,
    FIELD_DEFAULT, FIELD_PLANT, FIELD_DEFAULT, FIELD_DEFAULT, FIELD_TICKET, FIELD_RELEASE, FIELD_HUKUBUKURO, FIELD_DEFAULT,
];
pub const TAG_MY_ROOM: [u8; 16] = [
    ROOM_LETTER, ROOM_DEFAULT, ROOM_DEFAULT, ROOM_DEFAULT, ROOM_DEFAULT, ROOM_DEFAULT, ROOM_CARPET, ROOM_WALL,
    ROOM_DEFAULT, ROOM_DEFAULT, ROOM_DEFAULT, ROOM_DEFAULT, ROOM_TICKET, ROOM_DEFAULT, ROOM_DEFAULT, ROOM_DEFAULT,
];
pub const TAG_OTHER_ROOM: [u8; 16] = [
    OTHER_LETTER, CATCH_ITEM, CATCH_ITEM, CATCH_ITEM, CATCH_ITEM, CATCH_ITEM, CATCH_ITEM, CATCH_ITEM,
    CATCH_ITEM, CATCH_ITEM, CATCH_ITEM, CATCH_ITEM, CATCH_TICKET, CATCH_ITEM, OTHER_HUKUBUKURO, CATCH_ITEM,
];
pub const TAG_WISP: [u8; 4] = [FIELD_RELEASE, CATCH_ITEM, CATCH_ITEM, CATCH_ITEM];
pub const TAG_LETTERS: [u8; 4] = [FIELD_LETTERS, ROOM_LETTERS, OTHER_LETTERS, OTHER_LETTERS];

fn by_place(field: u8) -> &'static [u8; 16] {
    match field {
        FIELD_FG => &TAG_FIELD,
        FIELD_PLAYER_ROOM => &TAG_MY_ROOM,
        _ => &TAG_OTHER_ROOM,
    }
}

/// Context for the normal-mode decision.
#[derive(Clone, Copy, Debug, Default)]
pub struct ItemCtx {
    pub field: u8,
    pub marked: bool,
    pub cond: u8,
    pub basement: bool,
    pub release_ok: bool,
    /// Shovel equipped and `shovel_flag` (a diggable spot ahead).
    pub can_bury: bool,
    pub first_job: bool,
}

/// `mTG_select_tag_decide_item_normal`.
pub fn decide_item_normal(it: u16, c: &ItemCtx) -> u8 {
    use item::*;
    let f = (c.field as usize).min(3);
    let ty = kind(it);
    let mut cat = cat(it);
    let mut r = if c.marked {
        TAG_PUT_ALL
    } else if c.cond & 1 != 0 {
        PRESENT_ITEM
    } else if c.cond & 2 != 0 {
        CATCH_ITEM
    } else if it == SIGNBOARD && c.field == FIELD_FG {
        FIELD_SIGN
    } else if is_card_or_fork(it) {
        TAISOU_CARD
    } else if is_ftr(it) {
        by_place(c.field)[CAT_CLOTH as usize]
    } else if is_wisp(it) {
        TAG_WISP[f]
    } else if is_paper(it) {
        if paper_stack(it) == 0 {
            cat = CAT_PAPER;
            by_place(c.field)[CAT_PAPER as usize]
        } else {
            TAG_LETTERS[f]
        }
    } else if is_balloon(it) {
        match c.field {
            FIELD_FG => FIELD_BALLOON,
            FIELD_PLAYER_ROOM => ROOM_DEFAULT,
            _ => CATCH_ITEM,
        }
    } else {
        if cat == CAT_TICKET && it & 7 == 0 {
            cat = CAT_ETC;
        }
        let mut t = by_place(c.field)[cat as usize];
        // `|` instead of `||` in the source: same result for 0/1 operands.
        if c.field == FIELD_PLAYER_ROOM && c.basement && ((cat == CAT_CARPET) as u8 | (cat == CAT_WALL) as u8) != 0 {
            t = ROOM_DEFAULT;
        }
        t
    };
    if ty == ITEM1 && cat == CAT_FISH && c.release_ok {
        r = FIELD_RELEASE;
    } else if c.can_bury && it != SIGNBOARD && (ty != ITEM1 || cat != CAT_INSECT) {
        r = match r {
            FIELD_DEFAULT => FIELD_DEFAULT_BURY,
            FIELD_LETTER => FIELD_LETTER_BURY,
            FIELD_LETTERS => FIELD_LETTERS_BURY,
            FIELD_HUKUBUKURO => FIELD_HUKUBUKURO_BURY,
            FIELD_TICKET => FIELD_TICKET_BURY,
            x => x,
        };
    }
    if c.first_job && r == FIELD_PLANT {
        r = FIELD_PLANT_JOB;
    }
    r
}

/// `mTG_select_tag_decide_item` for the other open modes (`None` = use
/// `decide_item_normal`). Empty pockets give NONE; any other menu than the
/// inventory gives CATCH_ITEM.
pub fn decide_item_mode(menu: u8, mode: u8, it: u16, marked: bool) -> Option<u8> {
    if it == 0 {
        return Some(NONE);
    }
    if menu != ovl::INVENTORY {
        return Some(CATCH_ITEM);
    }
    Some(match mode {
        iv_open::NORMAL => return None,
        iv_open::SELL if marked => SELL_ALL_ITEM,
        iv_open::SELL => SELL_ITEM,
        iv_open::GIVE => GIVE_ITEM,
        iv_open::TAKE => QUEST_ITEM,
        iv_open::SHRINE => SHRINE_ITEM,
        iv_open::PUTIN_FTR | iv_open::MINIDISK => PUTIN_ITEM,
        iv_open::CURATOR => TAG_HAND_OVER_CURATOR,
        iv_open::PASSWORD => TAG_PASSWORD_ITEM,
        _ => NONE,
    })
}

/// Letter facts for the mail menu.
#[derive(Clone, Copy, Debug, Default)]
pub struct MailCtx {
    pub used: bool,
    pub sendable: bool,
    pub has_present: bool,
    /// Font is RECV or RECV_PLAYER_PRESENT (an unread received letter).
    pub recv_font: bool,
}

/// `mTG_select_tag_decide_mail` (inventory letters, unmarked).
pub fn decide_mail_inventory(mode: u8, field: u8, m: &MailCtx) -> u8 {
    if !m.used {
        return NONE;
    }
    if mode == iv_open::SEND_MAIL {
        return if m.sendable { SEND_MAIL } else { NONE };
    }
    const NEXT: [u8; 4] = [DEF_SMAIL, DEF_SMAIL_PRE, DEF_RMAIL, DEF_RMAIL_PRE];
    let flags = (m.has_present as usize) | ((!m.sendable as usize) << 1);
    let r = NEXT[flags];
    if field != FIELD_FG || m.recv_font {
        match r {
            DEF_SMAIL => ROOM_SMAIL,
            DEF_RMAIL => ROOM_RMAIL,
            x => x,
        }
    } else {
        r
    }
}

/// `mTG_select_tag_decide_money`: number of bag options (excl. "quit"),
/// 0 = no menu under 100 Bells.
pub fn money_bag_options(wallet: u32) -> usize {
    match wallet {
        30000.. => 4,
        10000.. => 3,
        1000.. => 2,
        100.. => 1,
        _ => 0,
    }
}

/// `mTG_MARK_*` modes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkMode {
    Chk,
    On,
    Off,
    Rvs,
    Clr,
    ChkAll0,
}

/// `mTG_bit_*` on a little bitfield; returns the CHK/CHK_ALL0 result.
pub fn mark_bits(field: &mut [u8], mask: &[u8], mode: MarkMode) -> bool {
    let mut res = false;
    for (f, m) in field.iter_mut().zip(mask.iter()) {
        match mode {
            MarkMode::Chk => res |= *f & *m != 0,
            MarkMode::On => *f |= *m,
            MarkMode::Off => *f &= !*m,
            MarkMode::Rvs => *f ^= *m,
            MarkMode::Clr => *f = 0,
            MarkMode::ChkAll0 => res |= *f == 0,
        }
    }
    res
}

/// Byte count of a mark field: `((max + 15) / 16) * 2`.
pub fn mark_bytes(max: usize) -> usize {
    max.div_ceil(16) * 2
}

/// Whether an inventory item can be marked (ON/RVS) in normal mode
/// outside (`fg`) or in the player's room; CHK/OFF/CLR always pass.
pub fn item_markable(it: u16, cond: u8, fg: bool, first_job: bool) -> bool {
    use item::*;
    let (ty, c) = (kind(it), cat(it));
    it != 0
        && cond == 0
        && !is_card_or_fork(it)
        && !is_balloon(it)
        && (ty != ITEM1 || (c != CAT_FISH && c != CAT_INSECT))
        && (!fg || it != SIGNBOARD)
        && (fg || (!is_ftr(it) && !is_umbrella(it) && !is_cloth(it)))
        && (!first_job || !fg || ty != ITEM1 || c != CAT_PLANT)
}

/// Sell-mode markability: no money; no turnips on Sundays.
pub fn sell_markable(it: u16, sunday: bool) -> bool {
    use item::*;
    let (ty, c) = (kind(it), cat(it));
    it != 0 && !is_card_or_fork(it) && (ty != ITEM1 || c != CAT_MONEY) && (!sunday || ty != ITEM1 || c != CAT_KABU)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_and_cursor() {
        let mut l = [Letter::default(); MAIL_SLOTS];
        l[2] = Letter { used: true, sendable: true };
        l[3] = Letter { used: true, sendable: false };
        assert_eq!(open_masks(iv_open::SEND_MAIL, 0, 0, &l).1, 1 << 2);
        assert_eq!(open_masks(iv_open::CPMAIL, 0, 0, &l).1, 0x3FF & !(1 << 2));
        assert_eq!(open_masks(iv_open::QUEST, 4, 0, &l), (1 << 4, 0));
        assert_eq!(open_masks(iv_open::EXCHANGE, 0, 0x12, &l), (0x12, 0xFFFF));
        assert_eq!(open_masks(iv_open::GIVE, 0, 0x12, &l), (0x12, 0));
        assert_eq!(companion_menu(iv_open::HANIWA_TAKE), Some(ovl::HANIWA));
        assert_eq!(initial_cursor(iv_open::GIVE, 0b1000, 0), 3);
        assert_eq!(initial_cursor(iv_open::GIVE, 0, 0), 0);
        assert_eq!(initial_cursor(iv_open::SEND_MAIL, 0, 1 << 9), 9);
    }

    #[test]
    fn pages() {
        let mut o = [PAGE_INVENTORY, PAGE_FISH, PAGE_INSECT];
        page_swap(&mut o, PAGE_INSECT);
        assert_eq!(o, [PAGE_INSECT, PAGE_INVENTORY, PAGE_FISH]);
        page_swap(&mut o, PAGE_FISH);
        assert_eq!(o, [PAGE_FISH, PAGE_INSECT, PAGE_INVENTORY]);
        assert!(page_drawn_on_top(&o, PAGE_FISH, 30, PAGE_FISH));
        assert!(page_drawn_on_top(&o, PAGE_FISH, 0, PAGE_FISH) && !page_drawn_on_top(&o, PAGE_FISH, 0, PAGE_INSECT));
    }

    #[test]
    fn item_menus() {
        let fg = ItemCtx { field: FIELD_FG, ..Default::default() };
        assert_eq!(decide_item_normal(0x2400, &fg), FIELD_DEFAULT);
        assert_eq!(decide_item_normal(0x2400, &ItemCtx { can_bury: true, ..fg }), FIELD_DEFAULT_BURY);
        assert_eq!(decide_item_normal(0x2D00, &ItemCtx { can_bury: true, ..fg }), FIELD_RELEASE); // bug
        assert_eq!(decide_item_normal(0x2300, &ItemCtx { release_ok: true, field: FIELD_PLAYER_ROOM, ..fg }), FIELD_RELEASE);
        assert_eq!(decide_item_normal(0x2900, &ItemCtx { first_job: true, ..fg }), FIELD_PLANT_JOB);
        assert_eq!(decide_item_normal(0x251E, &fg), FIELD_SIGN);
        assert_eq!(decide_item_normal(0x2530, &fg), TAISOU_CARD);
        assert_eq!(decide_item_normal(0x1000, &ItemCtx { field: FIELD_NPC_ROOM, ..fg }), CATCH_ITEM);
        assert_eq!(decide_item_normal(0x2000, &fg), FIELD_LETTER);
        assert_eq!(decide_item_normal(0x2040, &fg), FIELD_LETTERS);
        assert_eq!(decide_item_normal(0x2244, &ItemCtx { field: FIELD_PLAYER_ROOM, ..fg }), ROOM_DEFAULT);
        assert_eq!(decide_item_normal(0x2C08, &fg), FIELD_DEFAULT); // ticket id % 8 == 0 -> ETC
        assert_eq!(decide_item_normal(0x2C09, &fg), FIELD_TICKET);
        let room = ItemCtx { field: FIELD_PLAYER_ROOM, ..fg };
        assert_eq!(decide_item_normal(0x2600, &room), ROOM_CARPET);
        assert_eq!(decide_item_normal(0x2600, &ItemCtx { basement: true, ..room }), ROOM_DEFAULT);
        assert_eq!(decide_item_normal(0x2400, &ItemCtx { cond: 1, ..fg }), PRESENT_ITEM);
        assert_eq!(decide_item_normal(0x2400, &ItemCtx { marked: true, cond: 1, ..fg }), TAG_PUT_ALL);
        assert_eq!(decide_item_mode(ovl::INVENTORY, iv_open::SELL, 0x2400, true), Some(SELL_ALL_ITEM));
        assert_eq!(decide_item_mode(ovl::MAILBOX, iv_open::SELL, 0x2400, false), Some(CATCH_ITEM));
        assert_eq!(decide_item_mode(ovl::INVENTORY, iv_open::NORMAL, 0x2400, false), None);
        assert_eq!(decide_item_mode(ovl::INVENTORY, iv_open::GIVE, 0, false), Some(NONE));
    }

    #[test]
    fn mail_money_marks() {
        let m = MailCtx { used: true, sendable: true, has_present: false, recv_font: false };
        assert_eq!(decide_mail_inventory(iv_open::NORMAL, FIELD_FG, &m), DEF_SMAIL);
        assert_eq!(decide_mail_inventory(iv_open::NORMAL, FIELD_PLAYER_ROOM, &m), ROOM_SMAIL);
        let r = MailCtx { sendable: false, has_present: true, ..m };
        assert_eq!(decide_mail_inventory(iv_open::NORMAL, FIELD_FG, &r), DEF_RMAIL_PRE);
        assert_eq!(decide_mail_inventory(iv_open::SEND_MAIL, FIELD_FG, &r), NONE);
        assert_eq!([99, 100, 999, 1000, 10000, 30000].map(money_bag_options), [0, 1, 1, 2, 3, 4]);
        let mut f = [0u8, 0];
        mark_bits(&mut f, &[0, 4], MarkMode::On);
        assert!(mark_bits(&mut f, &[0, 4], MarkMode::Chk));
        assert!(mark_bits(&mut f, &[0, 0], MarkMode::ChkAll0)); // any-zero quirk
        mark_bits(&mut f, &[0, 4], MarkMode::Rvs);
        assert_eq!(f, [0, 0]);
        assert_eq!((mark_bytes(15), mark_bytes(10), mark_bytes(64)), (2, 2, 8));
        assert!(item_markable(0x2400, 0, true, false));
        assert!(!item_markable(0x2400, 0, false, false)); // clothes in my room
        assert!(!item_markable(0x251E, 0, true, false) && item_markable(0x251E, 0, false, false));
        assert!(!item_markable(0x2900, 0, true, true) && item_markable(0x2900, 0, false, true));
        assert!(!sell_markable(0x2F00, true) && sell_markable(0x2F00, false) && !sell_markable(0x2100, false));
    }
}

#[cfg(test)]
mod tests_wave4 {
    use super::*;

    #[test]
    fn tag_constants() {
        // Spot-check mTG_TYPE_* discriminants.
        assert_eq!(NONE, 0);
        assert_eq!(FIELD_DEFAULT, 1);
        assert_eq!(TAG_PUT_ALL, 63);
        assert_eq!(FIELD_SIGN, 72);
        assert_eq!(FIELD_BALLOON, 73);
        assert_eq!(TAG_PASSWORD_ITEM, 77);
        assert_eq!(SELL_ALL_ITEM, 48);
    }

    #[test]
    fn paper_stack_boundaries() {
        // 64 designs per stack size.
        assert_eq!(item::paper_stack(0x2000), 0);
        assert_eq!(item::paper_stack(0x203F), 0);
        assert_eq!(item::paper_stack(0x2040), 1);
        // Single letters -> letter menu; stacks -> multi-letter menu.
        let fg = ItemCtx { field: FIELD_FG, ..Default::default() };
        assert_eq!(decide_item_normal(0x203F, &fg), FIELD_LETTER);
        assert_eq!(decide_item_normal(0x2040, &fg), FIELD_LETTERS);
    }

    #[test]
    fn mail_inventory_edge() {
        // Unused letter -> NONE in every mode.
        let m = MailCtx::default();
        assert_eq!(decide_mail_inventory(iv_open::NORMAL, FIELD_FG, &m), NONE);
        assert_eq!(decide_mail_inventory(iv_open::SEND_MAIL, FIELD_FG, &m), NONE);
        // Received-font letter indoors maps to room menus.
        let r = MailCtx { used: true, sendable: true, has_present: false, recv_font: true };
        assert_eq!(decide_mail_inventory(iv_open::NORMAL, FIELD_FG, &r), ROOM_SMAIL);
    }
}
