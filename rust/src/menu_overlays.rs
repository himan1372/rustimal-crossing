//! Submenu overlay logic: clock setting (`m_timeIn_ovl.c`), warning
//! windows (`m_warning_ovl.c`), the diary privacy prompt
//! (`m_cpwarning_ovl.c`), the inventory hand cursor and its stacking
//! rules (`m_hand_ovl.c`), bell bags from the tag menu (`m_tag_ovl.c`)
//! and the card-mail page turn (`m_cpmail_ovl.c`).
//!
//! Source-verified against ac-decomp (primary) and ACGC-PC-Port
//! (secondary). PC changes: the year range of the clock menu is
//! 2001-2099 (`mTI_MAX_YEAR` 99, retail 30, so 2001-2030); the clock-hand
//! spin, warning/prompt scale, hand offsets and page turn advance in
//! 60 Hz ticks.
//!
//! ## Clock setting (`mTI`)
//! Fields hour, min, month, day, year (as years after 2000), then OK.
//! C-up/C-down edit with wrap-around (year 1 <-> max, hour 0 -> 23,
//! minute 0 -> 59) and re-validation like the birthday picker (a year
//! change on Feb 29 drops the day to 28). Pressing A into OK starts a
//! clock-hand animation when the 12-hour dial minute changed: the hand
//! moves 12 dial-minutes per frame and input is locked until it reaches
//! the target. Closing from the normal (non-initial) menu with any
//! difference in year/month/day/hour/minute sets **`Save cheated_flag`**
//! and requests the time change; the first-boot menu writes the RTC.
//!
//! ## Warnings (`mWR`)
//! 28 messages ("You can't hold any more letters...", "You can't carry
//! more than 99,999 Bells!", ...); slots 5-8 (including
//! `SELL_QUEST_ITEM`) are empty `mWR_NULL` entries that would draw
//! nothing. Only the mailbox warning is a Yes/No question (`data0 == 0`):
//! C-left/right move between Yes/No, B forces No, A on Yes plays SE 0x435.
//!
//! ## Diary prompt (`mCW`)
//! "Do you want to let anyone else see this diary?" Yes/No with
//! C-left/C-right, A stores `calendar.edit`. Cursor alpha pulses with a
//! 30-frame triangle wave; the window scales in/out by 0.2 per frame.
//!
//! ## Hand cursor stacking (`mHD_prepare_drop_*`)
//! Dropping a held item on a pocket item of the same kind stacks it when
//! the held item has no condition (the C checks the same condition twice;
//! the target's condition is never checked):
//! - tickets of the same month, up to 5;
//! - wisp (spirit) pieces, up to 5;
//! - stationery of the same design, up to 4;
//!
//! with the overflow left in the pocket.

use crate::birthday_ovl::is_valid_date;
use crate::game_math::sinf_table;

// ---------------------------------------------------------------------------
// Clock setting
// ---------------------------------------------------------------------------

pub const TI_IDX_HOUR: usize = 0;
pub const TI_IDX_MIN: usize = 1;
pub const TI_IDX_MONTH: usize = 2;
pub const TI_IDX_DAY: usize = 3;
pub const TI_IDX_YEAR: usize = 4;
pub const TI_IDX_OK: usize = 5;
pub const TI_MIN_YEAR: u16 = 1;
pub const TI_MAX_YEAR_RETAIL: u16 = 30;
pub const TI_MAX_YEAR_PC: u16 = 99;
pub const GAME_YEAR_MIN: u16 = 2000;
pub const DIAL_MINUTES: i32 = 12 * 60;
pub const SPIN_STEP: i32 = 12;

const BUTTON_CDOWN: u32 = 0x0004;
const BUTTON_CUP: u32 = 0x0008;

fn ti_valid(v: &[u16; 5]) -> bool {
    v[TI_IDX_HOUR] <= 23
        && v[TI_IDX_MIN] <= 59
        && is_valid_date(GAME_YEAR_MIN + v[TI_IDX_YEAR], v[TI_IDX_MONTH] as u8, v[TI_IDX_DAY] as u8)
}

/// One C-up/C-down edit of field `sel` (`mTI_move_Play`).
pub fn ti_edit(values: &mut [u16; 5], sel: usize, trigger: u32, max_year: u16) {
    if sel >= TI_IDX_OK {
        return;
    }
    let mut v = *values;
    let up = trigger & BUTTON_CUP != 0;
    let down = !up && trigger & BUTTON_CDOWN != 0;
    if up {
        v[sel] = v[sel].wrapping_add(1);
        if sel == TI_IDX_YEAR && v[TI_IDX_YEAR] > max_year {
            v[TI_IDX_YEAR] = TI_MIN_YEAR;
        }
    } else if down {
        if v[sel] != 0 {
            v[sel] -= 1;
            if sel == TI_IDX_YEAR && v[sel] < TI_MIN_YEAR {
                v[TI_IDX_YEAR] = max_year;
            }
        } else if sel == TI_IDX_HOUR {
            v[TI_IDX_HOUR] = 23;
        } else if sel == TI_IDX_MIN {
            v[TI_IDX_MIN] = 59;
        }
    }
    if !ti_valid(&v) {
        let month = v[TI_IDX_MONTH];
        if sel == TI_IDX_YEAR {
            v[TI_IDX_DAY] = 28;
        } else if (sel == TI_IDX_MONTH && month > 0 && month < 13) || (down && sel == TI_IDX_DAY) {
            let year = GAME_YEAR_MIN + v[TI_IDX_YEAR];
            let mut day: u8 = 31;
            while day > 0 && !is_valid_date(year, month as u8, day) {
                day -= 1;
            }
            v[TI_IDX_DAY] = day as u16;
        } else if up {
            v[sel] = if sel == TI_IDX_MONTH || sel == TI_IDX_DAY { 1 } else { 0 };
        } else if sel == TI_IDX_MONTH {
            v[TI_IDX_MONTH] = 12;
        }
    }
    *values = v;
}

/// Dial minute (`mTI_get_now_min`): position on the 12-hour face.
pub fn dial_minute(hour: u16, min: u16) -> i32 {
    let h = if hour >= 12 { hour - 12 } else { hour };
    min as i32 + h as i32 * 60
}

/// Clock-hand spin state (`saved_min`, `init_flag`, `input_disabled`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ClockSpin {
    pub saved_min: i32,
    pub init_flag: bool,
    pub input_disabled: bool,
}

impl ClockSpin {
    /// Entering OK (`mTI_move_Play`): start spinning when the dial moved.
    pub fn start_if_changed(&mut self, target: i32) -> bool {
        if self.saved_min != target {
            self.input_disabled = true;
            self.init_flag = false;
        }
        self.input_disabled
    }

    /// `mTI_set_time`: returns true on the frame the hand arrives.
    pub fn step(&mut self, target: i32) -> bool {
        let old = self.saved_min;
        let mut now = old + SPIN_STEP;
        if now >= DIAL_MINUTES {
            now -= DIAL_MINUTES;
        }
        let mut arrived = false;
        if (old < target && target <= now) || (old > now && (old < target || target <= now)) {
            if self.init_flag {
                now = target;
                self.input_disabled = false;
                arrived = true;
            } else {
                now = old;
            }
        }
        self.init_flag = true;
        self.saved_min = now;
        arrived
    }
}

/// `mTI_window_close` (normal open): did the player change the clock?
pub fn ti_sets_cheated_flag(new: &[u16; 5], original: (u16, u8, u8, u8, u8)) -> bool {
    let (y, mo, d, h, mi) = original;
    GAME_YEAR_MIN + new[TI_IDX_YEAR] != y
        || new[TI_IDX_MONTH] as u8 != mo
        || new[TI_IDX_DAY] as u8 != d
        || new[TI_IDX_HOUR] as u8 != h
        || new[TI_IDX_MIN] as u8 != mi
}

// ---------------------------------------------------------------------------
// Warnings
// ---------------------------------------------------------------------------

/// One warning line: (text, x, y advance).
pub type WarnLine = (&'static str, f32, f32);

/// `wr_win_data`: (lines, scale_x, scale_y); empty slots are `None`.
pub const WARNINGS: [Option<(&[WarnLine], f32, f32)>; 28] = [
    Some((&[("You can't hold", 70.0, 28.0), ("any more letters! Want to", 40.0, 24.0), ("throw away some old ones?", 36.0, 24.0), ("Yes", 94.0, 24.0), ("No", 140.0, 0.0)], 1.0, 1.2)),
    Some((&[("You can't pull out a present", 44.0, 40.0), ("when your item screen is full!", 40.0, 24.0)], 1.15, 1.0)),
    Some((&[("You can't carry", 66.0, 40.0), ("more than 99,999 Bells!", 44.0, 24.0)], 1.0, 1.0)),
    Some((&[("You can't mail", 70.0, 40.0), ("turnips or living things!", 42.0, 24.0)], 1.0, 1.0)),
    Some((&[("You can't mail", 74.0, 40.0), ("someone else's belongings!", 38.0, 24.0)], 1.0, 1.0)),
    None,
    None,
    None,
    None,
    Some((&[("You can't put", 68.0, 40.0), ("any more items out!", 50.0, 24.0)], 0.95, 1.0)),
    Some((&[("You can't put", 68.0, 40.0), ("any more items there!", 46.0, 24.0)], 0.95, 1.0)),
    Some((&[("You can't drop", 72.0, 40.0), ("any more items here!", 54.0, 24.0)], 1.0, 1.0)),
    Some((&[("You can't plant anything here!", 39.0, 46.0)], 1.15, 0.9)),
    Some((&[("You can't open a bag", 48.0, 37.0), ("unless you have room", 50.0, 24.0), ("for three more items!", 52.0, 24.0)], 1.0, 1.15)),
    Some((&[("You can't mail", 74.0, 40.0), ("a wrapped present!", 62.0, 24.0)], 1.0, 1.0)),
    Some((&[("You can't carry", 85.0, 40.0), ("any more letters,", 82.0, 24.0), ("so you can't write a new one!", 45.0, 24.0)], 1.15, 1.2)),
    Some((&[("You can't pull anything out", 47.0, 40.0), ("when your item screen is full!", 41.0, 24.0)], 1.15, 1.0)),
    Some((&[("There's already music in there!", 38.0, 46.0)], 1.15, 0.9)),
    Some((&[("Your item screen is full,", 54.0, 40.0), ("so you can't use a pattern!", 48.0, 24.0)], 1.13, 1.0)),
    Some((&[("You can't put a sign up here!", 40.0, 46.0)], 1.13, 0.9)),
    Some((&[("You can put signs up only", 44.0, 40.0), ("in your own village!", 64.0, 24.0)], 1.1, 1.0)),
    Some((&[("You can't put signs on the island!", 31.0, 46.0)], 1.15, 0.9)),
    Some((&[("You can't mail", 60.0, 40.0), ("an exercise card!", 54.0, 24.0)], 0.9, 1.0)),
    Some((&[("This diary is locked!", 38.0, 46.0)], 0.9, 0.9)),
    Some((&[("You can't leave", 72.0, 40.0), ("this NES on the island!", 44.0, 24.0)], 1.0, 1.0)),
    Some((&[("You don't have room", 54.0, 40.0), ("to write any more.", 63.0, 24.0)], 1.0, 1.0)),
    Some((&[("You entered letters", 59.0, 40.0), ("that have nothing to do", 45.0, 24.0), ("with secret codes!", 63.0, 24.0)], 1.0, 1.2)),
    Some((&[("You can't mail", 60.0, 40.0), ("this item!", 80.0, 24.0)], 0.9, 1.0)),
];

/// Text colour index per line (`mWR_set_strings`): 0 base (yellow),
/// 1 selected choice (pink), 2 unselected choice (white).
pub fn warning_line_color(line: usize, selected: usize, yes_no: bool) -> u8 {
    if line == 3 + selected {
        1
    } else if line >= 3 && yes_no {
        2
    } else {
        0
    }
}

// ---------------------------------------------------------------------------
// Diary prompt
// ---------------------------------------------------------------------------

/// Cursor alpha for the 30-frame pulse counter (`_00`, already advanced).
pub fn diary_prompt_alpha(counter: i32) -> i32 {
    if counter < 15 {
        (counter as f32 * 255.0 / 15.0) as i32
    } else {
        ((30 - counter) as f32 * 255.0 / 15.0) as i32
    }
}

// ---------------------------------------------------------------------------
// Hand cursor
// ---------------------------------------------------------------------------

/// `mHD_ACTION_*`: SASU (point), SIDE, CLOSE, CLOSE_KEEP, OPEN, CLOSE2, SASU2.
pub const HAND_ANIME_SPEED: [f32; 7] = [0.275, 0.275, 0.45, 0.45, 0.45, 0.45, 0.275];
pub const HAND_OFFSET_X: [f32; 7] = [16.0, 16.0, 10.0, 10.0, 10.0, 10.0, 16.0];
pub const HAND_OFFSET_Y: [f32; 7] = [6.0, 6.0, 2.0, 2.0, 2.0, 2.0, 6.0];
/// `chase_f` step for the hand offset per frame.
pub const HAND_OFFSET_STEP: f32 = 0.5;

pub const TICKET_STACK_MAX: i32 = 5;
pub const WISP_STACK_MAX: i32 = 5;
pub const PAPER_STACK_MAX: i32 = 4;
pub const PAPER_UNIQUE_NUM: u16 = 64;
pub const ITM_TICKET_START: u16 = 0x2C00;
pub const ITM_SPIRIT0: u16 = 0x2D28;
pub const ITM_SPIRIT4: u16 = 0x2D2C;
pub const ITM_PAPER_START: u16 = 0x2000;
pub const ITM_PAPER_END: u16 = ITM_PAPER_START + 256;
pub const ITM_PAPER_STACK_FOUR_START: u16 = 0x20C0;

fn is_ticket(i: u16) -> bool {
    (i & 0xF000) >> 12 == 2 && (i & 0x0F00) >> 8 == 12
}

/// Result of a drop on a pocket: (new pocket item, new held item).
pub type StackResult = (u16, u16);

/// Stack two items if the rules allow; `None` = no stacking (plain swap).
pub fn try_stack(held: u16, held_cond_normal: bool, target: u16) -> Option<StackResult> {
    if !held_cond_normal {
        return None;
    }
    // Tickets (same month; count in the low 3 bits).
    if is_ticket(held) && is_ticket(target) {
        let month = |i: u16| 1 + ((i >> 3) & 0xF);
        if month(held) != month(target) {
            return None;
        }
        let (h, t) = ((held & 7) as i32 + 1, (target & 7) as i32 + 1);
        if h == TICKET_STACK_MAX || t == TICKET_STACK_MAX {
            return None;
        }
        let base = ITM_TICKET_START | ((month(held) - 1) << 3);
        let total = h + t;
        return Some(if total > TICKET_STACK_MAX {
            (base | (total - TICKET_STACK_MAX - 1) as u16, base | (TICKET_STACK_MAX - 1) as u16)
        } else {
            (0, base | (total - 1) as u16)
        });
    }
    // Wisp pieces.
    let wisp = |i: u16| (ITM_SPIRIT0..=ITM_SPIRIT4).contains(&i);
    if wisp(held) && wisp(target) {
        let (h, t) = ((held - ITM_SPIRIT0 + 1) as i32, (target - ITM_SPIRIT0 + 1) as i32);
        if h == WISP_STACK_MAX || t == WISP_STACK_MAX {
            return None;
        }
        let total = h + t;
        return Some(if total > WISP_STACK_MAX {
            (ITM_SPIRIT0 + (total - WISP_STACK_MAX - 1) as u16, ITM_SPIRIT4)
        } else {
            (0, ITM_SPIRIT0 + (total - 1) as u16)
        });
    }
    // Stationery of the same design.
    let paper = |i: u16| (ITM_PAPER_START..ITM_PAPER_END).contains(&i);
    if paper(held) && paper(target) {
        let (a, b) = (held - ITM_PAPER_START, target - ITM_PAPER_START);
        let ty = a % PAPER_UNIQUE_NUM;
        if ty != b % PAPER_UNIQUE_NUM {
            return None;
        }
        let (h, t) = ((a / PAPER_UNIQUE_NUM) as i32 + 1, (b / PAPER_UNIQUE_NUM) as i32 + 1);
        if h == PAPER_STACK_MAX || t == PAPER_STACK_MAX {
            return None;
        }
        let total = h + t;
        return Some(if total > PAPER_STACK_MAX {
            (ITM_PAPER_START + ty + (total - PAPER_STACK_MAX - 1) as u16 * PAPER_UNIQUE_NUM, ITM_PAPER_STACK_FOUR_START + ty)
        } else {
            (0, ITM_PAPER_START + ty + (total - 1) as u16 * PAPER_UNIQUE_NUM)
        });
    }
    None
}

// ---------------------------------------------------------------------------
// Bell bags (tag menu)
// ---------------------------------------------------------------------------

/// Frames the hand waits while making a bell bag.
pub const MONEY_SACK_WAIT: i32 = 42;

/// `mTG_make_money_sack`: (new wallet, per-frame display step) or `None`
/// when the wallet is short.
pub fn make_money_sack(wallet: u32, bag_amount: u32) -> Option<(u32, i32)> {
    if bag_amount == 0 || (wallet as i32) < bag_amount as i32 {
        return None;
    }
    Some((wallet - bag_amount, -(bag_amount as i32 / MONEY_SACK_WAIT)))
}

// ---------------------------------------------------------------------------
// Card mail page turn
// ---------------------------------------------------------------------------

pub const PAGE_MOVE_FRAMES: i32 = 40;
pub const PAGE_SWAP_FRAME: i32 = 20;

/// Page Y offset for a remaining page-turn timer (`100 sin(t * 4.5 deg)`).
pub fn page_offset_y(timer: i32) -> f32 {
    100.0 * sinf_table(timer as f32 * 4.5f32.to_radians())
}

/// Move `next_page` to the front of the 8-entry page order (at timer 20).
pub fn page_bring_to_front(order: &mut [u8; 8], next_page: u8) {
    let i = order.iter().position(|&p| p == next_page).unwrap_or(order.len() - 1);
    for k in (1..=i).rev() {
        order[k] = order[k - 1];
    }
    order[0] = next_page;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_edit_wraps() {
        let mut v = [0u16, 0, 1, 31, 1];
        ti_edit(&mut v, TI_IDX_HOUR, BUTTON_CDOWN, TI_MAX_YEAR_RETAIL);
        assert_eq!(v[TI_IDX_HOUR], 23);
        ti_edit(&mut v, TI_IDX_MIN, BUTTON_CDOWN, TI_MAX_YEAR_RETAIL);
        assert_eq!(v[TI_IDX_MIN], 59);
        ti_edit(&mut v, TI_IDX_MONTH, BUTTON_CUP, TI_MAX_YEAR_RETAIL);
        assert_eq!((v[TI_IDX_MONTH], v[TI_IDX_DAY]), (2, 28)); // 2001 not leap
        let mut v = [0u16, 0, 2, 29, 4]; // 2004-02-29
        ti_edit(&mut v, TI_IDX_YEAR, BUTTON_CUP, TI_MAX_YEAR_RETAIL);
        assert_eq!((v[TI_IDX_YEAR], v[TI_IDX_DAY]), (5, 28));
        let mut v = [0u16, 0, 1, 1, 30];
        ti_edit(&mut v, TI_IDX_YEAR, BUTTON_CUP, TI_MAX_YEAR_RETAIL);
        assert_eq!(v[TI_IDX_YEAR], 1);
        let mut v = [0u16, 0, 1, 1, 30];
        ti_edit(&mut v, TI_IDX_YEAR, BUTTON_CUP, TI_MAX_YEAR_PC);
        assert_eq!(v[TI_IDX_YEAR], 31);
    }

    #[test]
    fn clock_spin_arrives() {
        let mut s = ClockSpin { saved_min: 0, ..Default::default() };
        assert!(s.start_if_changed(60));
        let mut frames = 0;
        while s.input_disabled {
            s.step(60);
            frames += 1;
            assert!(frames < 100);
        }
        assert_eq!(s.saved_min, 60);
        assert_eq!(frames, 5); // 12, 24, 36, 48, then snap to 60
        assert!(ti_sets_cheated_flag(&[10, 0, 1, 1, 1], (2001, 1, 1, 9, 0)));
        assert!(!ti_sets_cheated_flag(&[9, 0, 1, 1, 1], (2001, 1, 1, 9, 0)));
    }

    #[test]
    fn stacking_rules() {
        // Two 3-stacks of May tickets: 5 in hand, 1 left.
        let may3 = ITM_TICKET_START | (4 << 3) | 2;
        assert_eq!(try_stack(may3, true, may3), Some((ITM_TICKET_START | (4 << 3), ITM_TICKET_START | (4 << 3) | 4)));
        // Different month: no stack.
        assert_eq!(try_stack(may3, true, ITM_TICKET_START | 2), None);
        assert_eq!(try_stack(ITM_SPIRIT0, true, ITM_SPIRIT0 + 1), Some((0, ITM_SPIRIT0 + 2)));
        assert_eq!(try_stack(0x2005, true, 0x2045), Some((0, 0x2085)));
        assert_eq!(try_stack(0x2005, false, 0x2045), None);
    }

    #[test]
    fn misc_overlays() {
        assert_eq!(WARNINGS.iter().filter(|w| w.is_none()).count(), 4);
        assert_eq!(warning_line_color(4, 1, true), 1);
        assert_eq!(warning_line_color(3, 1, true), 2);
        assert_eq!(diary_prompt_alpha(15), 255);
        assert_eq!(make_money_sack(999, 1000), None);
        assert_eq!(make_money_sack(30000, 30000), Some((0, -714)));
        let mut o = [0, 1, 2, 3, 4, 5, 6, 7];
        page_bring_to_front(&mut o, 5);
        assert_eq!(o, [5, 0, 1, 2, 3, 4, 6, 7]);
        assert!((page_offset_y(20) - 100.0).abs() < 0.01);
    }
}
