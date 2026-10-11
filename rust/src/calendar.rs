//! Player calendar (`m_calendar.c`) and the calendar submenu
//! (`m_calendar_ovl.c`): played-day / event-attendance bookkeeping in the
//! save, the 3-year month grid with event days and icons, page flipping,
//! the day cursor and the hand-off to the diary.
//!
//! Source-verified against ac-decomp (primary) and ACGC-PC-Port
//! (secondary). `mSC_EVENT_*` values come from `m_soncho.h` (enum from 0).
//! The date helpers reuse `game_time` (`lbRTC_Week`,
//! `lbRTC_GetDaysByMonth`, `lbRk_*`).
//!
//! ## PC-port changes
//! - Page slides, the panel swap and the escape/entrance slides go through
//!   the dt-aware `add_calc`; the multi-page slide steps 37 * dt per
//!   frame; the cursor blink timer adds dt and wraps with `while`.
//!   `m_calendar.c` is unchanged.
//!
//! ## Facts
//! - Save (`mCD_player_calendar_c`, 0x68 bytes): a played-day bit per day
//!   for 12 months, an attended-event bit per day, and `event_flags` for
//!   events whose day moves (Father's Day, Town Day, Meteor Shower,
//!   Founder's Day, Harvest Moon in Sep/Oct, Officer's Day).
//!   It also stores the year/month last written; on each write the months
//!   in between are wiped (more than a year apart = full wipe) and so are
//!   later days of the current month (time travel backwards).
//! - The grid shows last year, this year and next year (36 months) but the
//!   stick only pages +-11 months from now; C-up flips back to the current
//!   month, sliding through every month in between.
//! - Icons are shown only for the last 12 months: 2 = attended an event
//!   that day, 1 = played that day.
//! - Fixed events: 16 dated entries (the cherry blossom festival takes
//!   3 days). Floating ones: Mother's Day (2nd Sun May), Graduation (2nd Fri
//!   Jun), Father's Day (3rd Sun Jun), Labor Day (1st Mon Sep), Explorer's
//!   Day (2nd Mon Oct), Mayor's Day (Tue after 1st Mon Nov), Harvest
//!   Festival (4th Thu Nov) and Sale Day (the Friday after). Fishing
//!   tourneys fall on every June and November Sunday. Sports fairs fall
//!   on the equinoxes, plus the town's own day in July, the harvest moon
//!   and the player's birthday (which does not recolour the cell).
//! - A day lists at most 6 events, a month holds at most 12. C-up/C-down
//!   scroll the event list before moving the cursor a week. Town Day's
//!   name is prefixed with the town name.
//! - A on a day opens the diary at that month: your own (arg 0), someone
//!   else's (arg 1), or the LOCK_DIARY warning if they locked it (`edit`).
//!
//! ## Bugs and quirks
//! - The decomp names flag bit 0 `MOTHERS_DAY`, but it is set by event 11
//!   = `mSC_EVENT_FATHERS_DAY` and lives in June.
//! - Father's Day attended icon (US/JP): `lbRTC_Week(day + 1)` makes the
//!   Saturday *before* the third Sunday carry the icon. The Australian
//!   build uses `lbRTC_Week(day)`, which misses a Father's Day on the 21st
//!   (`1 + day / 7 == 3` needs day 14-20).
//! - `mCD_calendar_check_delete(player_no, ...)` wipes the *current*
//!   player's calendar (`mCD_calendar_clear(-1)`) when the target
//!   calendar was never written or is a year or more out of date. Opening
//!   another resident's stale calendar therefore erases your own history
//!   and leaves theirs untouched.
//! - A Feb 29 birthday in a non-leap year resolves to cell 37 (one past
//!   `day_data`); the event is never selectable, and the attendance check
//!   reads `_25[0]` as the day number.
//! - The `p6` path of `mDC_set_event_day_data` is dead (always 0).
//! - The Harvest Moon icon check (`default:` case) tests both Sep and Oct
//!   flag bits.

use crate::game_math::add_calc;
use crate::game_time::{autumnal_equinox_day, get_days_by_month, harvest_moon_day, rtc_week, vernal_equinox_day};

pub const MONTHS: usize = 12;
pub const YEARS_SHOWN: usize = 3;
pub const GRID: usize = 37;
pub const MAX_MONTH_EVENTS: usize = 12;
pub const MAX_DAY_EVENTS: usize = 6;

// mSC_EVENT_* (m_soncho.h).
pub const EV_NEW_YEARS_DAY: u8 = 0;
pub const EV_FOUNDERS_DAY: u8 = 1;
pub const EV_GRADUATION_DAY: u8 = 2;
pub const EV_APRILFOOLS_DAY: u8 = 3;
pub const EV_TOWN_DAY: u8 = 4;
pub const EV_MOTHERS_DAY: u8 = 5;
pub const EV_SALE_DAY: u8 = 6;
pub const EV_CHERRY_BLOSSOM_FESTIVAL: u8 = 7;
pub const EV_SPRING_SPORTS_FAIR: u8 = 8;
pub const EV_NATURE_DAY: u8 = 9;
pub const EV_SPRING_CLEANING: u8 = 10;
pub const EV_FATHERS_DAY: u8 = 11;
pub const EV_FISHING_TOURNEY_1: u8 = 12;
pub const EV_GROUNDHOG_DAY: u8 = 13;
pub const EV_EXPLORERS_DAY: u8 = 14;
pub const EV_FIREWORKS_SHOW: u8 = 15;
pub const EV_METEOR_SHOWER: u8 = 16;
pub const EV_HARVEST_MOON_FESTIVAL: u8 = 17;
pub const EV_MAYORS_DAY: u8 = 18;
pub const EV_OFFICERS_DAY: u8 = 19;
pub const EV_FALL_SPORTS_FAIR: u8 = 20;
pub const EV_HALLOWEEN: u8 = 21;
pub const EV_FISHING_TOURNEY_2: u8 = 22;
pub const EV_SNOW_DAY: u8 = 23;
pub const EV_LABOR_DAY: u8 = 24;
pub const EV_TOY_DAY: u8 = 25;
pub const EV_NEW_YEARS_EVE_COUNTDOWN: u8 = 26;
pub const EV_HARVEST_FESTIVAL: u8 = 27;
pub const EV_PLAYER_BIRTHDAY: u8 = 28;
pub const EV_NONE: u8 = 255;

// mCD_FLAG_* (bit 0 is named MOTHERS_DAY in the decomp; it is Father's Day).
pub const FLAG_FATHERS_DAY: u16 = 1 << 0;
pub const FLAG_TOWN_DAY: u16 = 1 << 1;
pub const FLAG_METEOR_SHOWER: u16 = 1 << 2;
pub const FLAG_FOUNDERS_DAY: u16 = 1 << 3;
pub const FLAG_HARVEST_MOON_9: u16 = 1 << 4;
pub const FLAG_HARVEST_MOON_10: u16 = 1 << 5;
pub const FLAG_OFFICERS_DAY: u16 = 1 << 6;

// mCD_DAY_* (top 3 bits of a grid cell).
pub const DAY_NONE: u8 = 0;
pub const DAY_NORMAL: u8 = 1;
pub const DAY_FIRST_OF_WEEK: u8 = 2;
pub const DAY_EVENT: u8 = 3;
pub const DAY_CURRENT: u8 = 4;
pub const DAY_UNDEFINED: u8 = 5;

/// `mCD_player_calendar_c` (save data, 0x68 bytes).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PlayerCalendar {
    pub played_days: [u32; MONTHS],
    pub event_days: [u32; MONTHS],
    pub event_flags: u16,
    pub edit: u8,
    pub pad_63: u8,
    pub year: u16,
    pub month: u8,
}

const _: () = assert!(core::mem::size_of::<PlayerCalendar>() == 0x68);

/// Flag for a flag-tracked event, or `None` for day-bit events.
fn event_flag(event: u8, month: u8) -> Option<u16> {
    match event {
        EV_FATHERS_DAY => Some(FLAG_FATHERS_DAY),
        EV_TOWN_DAY => Some(FLAG_TOWN_DAY),
        EV_METEOR_SHOWER => Some(FLAG_METEOR_SHOWER),
        EV_FOUNDERS_DAY => Some(FLAG_FOUNDERS_DAY),
        EV_HARVEST_MOON_FESTIVAL => Some(match month {
            9 => FLAG_HARVEST_MOON_9,
            10 => FLAG_HARVEST_MOON_10,
            _ => 0,
        }),
        EV_OFFICERS_DAY => Some(FLAG_OFFICERS_DAY),
        _ => None,
    }
}

/// `mCD_calendar_clear_interval` (`interval` = months since last write).
pub fn clear_interval(cal: &mut PlayerCalendar, interval: i32) {
    let mut months = [false; MONTHS];
    let m = cal.month as i32;
    for i in 0..interval.max(0) {
        months[((m + i) % 12) as usize] = true;
    }
    let mut i = 0;
    while i > interval {
        months[((m + 11 + i).rem_euclid(12)) as usize] = true;
        i -= 1;
    }
    for (k, &hit) in months.iter().enumerate() {
        if hit {
            cal.played_days[k] = 0;
            cal.event_days[k] = 0;
        }
    }
    let clears = [
        (5, FLAG_FATHERS_DAY),
        (6, FLAG_TOWN_DAY),
        (7, FLAG_METEOR_SHOWER | FLAG_FOUNDERS_DAY),
        (8, FLAG_HARVEST_MOON_9),
        (9, FLAG_HARVEST_MOON_10),
        (10, FLAG_OFFICERS_DAY),
    ];
    for (idx, flag) in clears {
        if months[idx] {
            cal.event_flags &= !flag;
        }
    }
}

/// `mCD_calendar_clear_day`: forget days after `day` in this month.
pub fn clear_day(cal: &mut PlayerCalendar, year: u16, month: u8, day: u8, town_day: u8) {
    if get_days_by_month(year, month) == day {
        return;
    }
    let mut bits = 0u32;
    for i in day as u32..31 {
        bits |= 1 << i;
    }
    let mi = (month - 1) as usize;
    cal.played_days[mi] &= !bits;
    cal.event_days[mi] &= !bits;
    match month {
        6 => {
            let wd = rtc_week(year, month, day + 1) as i32;
            if (day as i32 - wd) / 7 + 1 < 3 {
                cal.event_flags &= !FLAG_FATHERS_DAY;
            }
        }
        7 if day < town_day => cal.event_flags &= !FLAG_TOWN_DAY,
        8 => {
            if day < 12 {
                cal.event_flags &= !FLAG_METEOR_SHOWER;
            }
            if day < 21 {
                cal.event_flags &= !FLAG_FOUNDERS_DAY;
            }
        }
        9 | 10 => {
            if let Some((hm, hd)) = harvest_moon_day(year as i32) {
                if month == hm && day < hd {
                    cal.event_flags &= !if month == 9 { FLAG_HARVEST_MOON_9 } else { FLAG_HARVEST_MOON_10 };
                }
            }
        }
        11 if day < 11 => cal.event_flags &= !FLAG_OFFICERS_DAY,
        _ => {}
    }
}

/// Result of `mCD_calendar_check_delete`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckDelete {
    /// The target calendar was trimmed in place.
    Trimmed,
    /// C calls `mCD_calendar_clear(-1)`: the *current* player's calendar
    /// is wiped, whichever calendar was being checked.
    ClearCurrentPlayer,
}

/// `mCD_calendar_check_delete` on `cal` (the target player's calendar).
pub fn check_delete(cal: &mut PlayerCalendar, year: u16, month: u8, day: u8, town_day: u8) -> CheckDelete {
    if cal.year == 0 {
        return CheckDelete::ClearCurrentPlayer;
    }
    let interval = (month as i32 - cal.month as i32) + (year as i32 - cal.year as i32) * 12;
    if !(-11..=11).contains(&interval) {
        return CheckDelete::ClearCurrentPlayer;
    }
    if interval != 0 {
        clear_interval(cal, interval);
    }
    clear_day(cal, year, month, day, town_day);
    CheckDelete::Trimmed
}

/// `check_delete` for the current player (`welcome_on` / `event_on`),
/// where the wipe hits the same calendar.
fn check_delete_current(cal: &mut PlayerCalendar, year: u16, month: u8, day: u8, town_day: u8) {
    if check_delete(cal, year, month, day, town_day) == CheckDelete::ClearCurrentPlayer {
        *cal = PlayerCalendar::default();
    }
}

/// `mCD_calendar_wellcome_on`: mark today as played.
pub fn welcome_on(cal: &mut PlayerCalendar, year: u16, month: u8, day: u8, town_day: u8) {
    check_delete_current(cal, year, month, day, town_day);
    cal.played_days[(month - 1) as usize] |= 1 << (day - 1);
    cal.year = year;
    cal.month = month;
}

/// `mCD_calendar_event_on`: record attendance of `event` on that day.
pub fn event_on(cal: &mut PlayerCalendar, year: u16, month: u8, day: u8, event: u8, town_day: u8) {
    check_delete_current(cal, year, month, day, town_day);
    if event == EV_NONE {
        return;
    }
    match event_flag(event, month) {
        Some(flag) => cal.event_flags |= flag,
        None => cal.event_days[(month - 1) as usize] |= 1 << (day - 1),
    }
}

/// `mCD_calendar_event_check` (`now` = RTC year/month).
pub fn event_check(cal: &PlayerCalendar, now: (u16, u8), year: u16, month: u8, day: u8, event: u8) -> bool {
    let interval = (now.1 as i32 - month as i32) + (now.0 as i32 - year as i32) * 12;
    if !(0..12).contains(&interval) || event == EV_NONE {
        return false;
    }
    match event_flag(event, month) {
        Some(flag) => cal.event_flags & flag != 0,
        None => cal.event_days[(month - 1) as usize] & (1 << (day - 1)) != 0,
    }
}

// ---------------------------------------------------------------------------
// Overlay grid
// ---------------------------------------------------------------------------

/// `mCD_month_entry_c`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MonthEntry {
    /// Low 5 bits day number (0 = blank), top 3 bits `DAY_*`.
    pub day_data: [u8; GRID],
    /// Icon per cell (`_25`): 0 none, 1 played, 2 attended an event.
    pub icons: [u8; GRID],
    /// (cell, event | 0x80 if attended) (`_4A`).
    pub events: [[u8; 2]; MAX_MONTH_EVENTS],
    pub event_count: u8,
}

impl Default for MonthEntry {
    fn default() -> Self {
        Self { day_data: [0; GRID], icons: [0; GRID], events: [[0; 2]; MAX_MONTH_EVENTS], event_count: 0 }
    }
}

impl MonthEntry {
    /// Cell read with the C out-of-range behaviour: cell 37 aliases `_25[0]`.
    fn cell(&self, pos: usize) -> u8 {
        if pos < GRID {
            self.day_data[pos]
        } else {
            self.icons[0]
        }
    }
}

/// Inputs the grid builder reads from the save / common data.
#[derive(Clone, Copy, Debug)]
pub struct CalendarContext {
    /// RTC year, month, day.
    pub now: (u16, u8, u8),
    pub town_day: u8,
    /// Viewed player's birthday (month 0xFF = unset).
    pub birthday: (u8, u8),
    /// GAFU01 (Australia) Father's Day weekday rule.
    pub aus: bool,
    /// The viewing player is a foreigner (no icons).
    pub foreigner: bool,
}

/// `mCD_sp_soncho_chk`.
fn sp_soncho_chk(year: u16, month: u8, day: u8, cal: &PlayerCalendar, ctx: &CalendarContext) -> bool {
    let chk: u16 = match month {
        6 => {
            let wd = rtc_week(year, month, if ctx.aus { day } else { day + 1 }) as i32;
            if wd == 0 && 1 + (day as i32 - wd) / 7 == 3 {
                FLAG_FATHERS_DAY
            } else {
                0
            }
        }
        7 if day == ctx.town_day => FLAG_TOWN_DAY,
        7 => 0,
        8 => match day {
            21 => FLAG_FOUNDERS_DAY,
            12 => FLAG_METEOR_SHOWER,
            _ => 0,
        },
        11 if day == 11 => FLAG_OFFICERS_DAY,
        11 => 0,
        _ => match harvest_moon_day(year as i32) {
            Some((hm, hd)) if hm == month && hd == day => FLAG_HARVEST_MOON_9 | FLAG_HARVEST_MOON_10,
            _ => 0,
        },
    };
    cal.event_flags & chk != 0
}

/// `mCD_make_icon`.
pub fn make_icon(year: u16, month: u8, day: u8, cal: &PlayerCalendar, ctx: &CalendarContext) -> u8 {
    let delta = (ctx.now.0 as i32 - year as i32) * 12 + (ctx.now.1 as i32 - month as i32);
    if !(0..12).contains(&delta) || ctx.foreigner || day == 0 {
        return 0;
    }
    let mi = (month - 1) as usize;
    let bit = 1u32 << (day - 1);
    if sp_soncho_chk(year, month, day, cal, ctx) || cal.event_days[mi] & bit != 0 {
        2
    } else if cal.played_days[mi] & bit != 0 {
        1
    } else {
        0
    }
}

/// `mDC_weekly_to_day_data`: day number of the `week`-th populated cell
/// in weekday column `col`.
pub fn weekly_to_day(entry: &MonthEntry, week: i32, col: usize) -> u8 {
    let mut week = week;
    let mut i = col;
    while i < GRID {
        if entry.day_data[i] & 0x1F > 0 {
            week -= 1;
        }
        if week <= 0 {
            return entry.day_data[i] & 0x1F;
        }
        i += 7;
    }
    0
}

/// `mDC_day_to_day_data`: cell of a day number (37 if absent).
pub fn day_to_cell(entry: &MonthEntry, day: u8) -> usize {
    (0..GRID).find(|&i| entry.day_data[i] & 0x1F == day).unwrap_or(GRID)
}

/// `mDC_set_event_day_data` (`p6` is always 0 and omitted).
fn set_event_day(year_entries: &mut [MonthEntry], month: u8, day: u8, event: u8, day_type: u8) {
    let e = &mut year_entries[(month - 1) as usize];
    let cell = day_to_cell(e, day);
    if day_type != DAY_UNDEFINED && cell < GRID {
        e.day_data[cell] = (e.day_data[cell] & 0x1F) | (day_type << 5);
    }
    if event != EV_NONE && (e.event_count as usize) < MAX_MONTH_EVENTS {
        e.events[e.event_count as usize] = [cell as u8, event];
        e.event_count += 1;
    }
}

/// (month, day, event) of the fixed-date table.
pub const FIXED_EVENTS: [(u8, u8, u8); 16] = [
    (1, 1, EV_NEW_YEARS_DAY),
    (2, 2, EV_GROUNDHOG_DAY),
    (4, 1, EV_APRILFOOLS_DAY),
    (4, 5, EV_CHERRY_BLOSSOM_FESTIVAL),
    (4, 6, EV_CHERRY_BLOSSOM_FESTIVAL),
    (4, 7, EV_CHERRY_BLOSSOM_FESTIVAL),
    (4, 22, EV_NATURE_DAY),
    (5, 1, EV_SPRING_CLEANING),
    (7, 4, EV_FIREWORKS_SHOW),
    (8, 12, EV_METEOR_SHOWER),
    (8, 21, EV_FOUNDERS_DAY),
    (10, 31, EV_HALLOWEEN),
    (11, 11, EV_OFFICERS_DAY),
    (12, 1, EV_SNOW_DAY),
    (12, 23, EV_TOY_DAY),
    (12, 31, EV_NEW_YEARS_EVE_COUNTDOWN),
];

/// (month, week, weekday, days_after, event) of the floating table.
pub const WEEKLY_EVENTS: [(u8, u8, u8, u8, u8); 8] = [
    (5, 2, 0, 0, EV_MOTHERS_DAY),
    (6, 2, 5, 0, EV_GRADUATION_DAY),
    (6, 3, 0, 0, EV_FATHERS_DAY),
    (9, 1, 1, 0, EV_LABOR_DAY),
    (10, 2, 1, 0, EV_EXPLORERS_DAY),
    (11, 1, 1, 1, EV_MAYORS_DAY),
    (11, 4, 4, 0, EV_HARVEST_FESTIVAL),
    (11, 4, 4, 1, EV_SALE_DAY),
];

/// `mCD_make_calendar_data_year`.
pub fn build_year(year: u16, cal: &PlayerCalendar, ctx: &CalendarContext) -> [MonthEntry; MONTHS] {
    let mut e = [MonthEntry::default(); MONTHS];
    for (mi, entry) in e.iter_mut().enumerate() {
        let month = mi as u8 + 1;
        let first = rtc_week(year, month, 1) as usize;
        let days = get_days_by_month(year, month) as usize;
        for (d, i) in (first..first + days).enumerate() {
            let day = d as u8 + 1;
            entry.icons[i] = make_icon(year, month, day, cal, ctx);
            let ty = if i % 7 == 0 { DAY_FIRST_OF_WEEK } else { DAY_NORMAL };
            entry.day_data[i] = day | (ty << 5);
        }
    }
    for &(m, d, ev) in FIXED_EVENTS.iter() {
        set_event_day(&mut e, m, d, ev, DAY_EVENT);
    }
    for &(m, week, wd, after, ev) in WEEKLY_EVENTS.iter() {
        let day = weekly_to_day(&e[(m - 1) as usize], week as i32, wd as usize) + after;
        set_event_day(&mut e, m, day, ev, DAY_EVENT);
    }
    for (m, ev) in [(6u8, EV_FISHING_TOURNEY_1), (11, EV_FISHING_TOURNEY_2)] {
        for j in (0..GRID).step_by(7) {
            let day = e[(m - 1) as usize].day_data[j] & 0x1F;
            if day > 0 {
                set_event_day(&mut e, m, day, ev, DAY_EVENT);
            }
        }
    }
    set_event_day(&mut e, 3, vernal_equinox_day(year as i32) as u8, EV_SPRING_SPORTS_FAIR, DAY_EVENT);
    set_event_day(&mut e, 9, autumnal_equinox_day(year as i32) as u8, EV_FALL_SPORTS_FAIR, DAY_EVENT);
    set_event_day(&mut e, 7, ctx.town_day, EV_TOWN_DAY, DAY_EVENT);
    if let Some((hm, hd)) = harvest_moon_day(year as i32) {
        set_event_day(&mut e, hm, hd, EV_HARVEST_MOON_FESTIVAL, DAY_EVENT);
    }
    if ctx.birthday.0 != 0xFF && (1..=12).contains(&ctx.birthday.0) {
        set_event_day(&mut e, ctx.birthday.0, ctx.birthday.1, EV_PLAYER_BIRTHDAY, DAY_UNDEFINED);
    }
    // mCD_make_calendar_event_sanka_check
    for (mi, entry) in e.iter_mut().enumerate() {
        let month = mi as u8 + 1;
        for k in 0..entry.event_count as usize {
            let day = entry.cell(entry.events[k][0] as usize) & 0x1F;
            let ev = entry.events[k][1] & 0x7F;
            let hit = if ev == EV_PLAYER_BIRTHDAY {
                !ctx.foreigner && day > 0 && cal.played_days[mi] & (1 << (day - 1)) != 0
            } else {
                !ctx.foreigner && day > 0 && event_check(cal, (ctx.now.0, ctx.now.1), year, month, day, ev)
            };
            if hit {
                entry.events[k][1] |= 0x80;
            }
        }
    }
    e
}

/// `mCD_make_calendar_data`: 36 months (last year..next year), the index
/// of the current month (`_1056` / `_1059`), today's cell marked CURRENT.
pub fn build_calendar(cal: &PlayerCalendar, ctx: &CalendarContext) -> (Vec<MonthEntry>, usize) {
    let mut all = Vec::with_capacity(YEARS_SHOWN * MONTHS);
    for i in 0..YEARS_SHOWN as u16 {
        all.extend_from_slice(&build_year(ctx.now.0 + i - 1, cal, ctx));
    }
    let cur = ctx.now.1 as usize + MONTHS - 1;
    let pos = day_to_cell(&all[cur], ctx.now.2);
    if pos < GRID {
        let c = &mut all[cur].day_data[pos];
        *c = (*c & 0x1F) | (DAY_CURRENT << 5);
    }
    (all, cur)
}

// ---------------------------------------------------------------------------
// Overlay input
// ---------------------------------------------------------------------------

pub const STICK_LEFT: u8 = 0;
pub const STICK_TOP: u8 = 2;
pub const STICK_RIGHT: u8 = 4;
pub const STICK_BOTTOM: u8 = 6;
pub const STICK_CENTER: u8 = 8;

/// `mCD_stick_area_check`: (area, left/right/center summary `_105C`).
/// Diagonals count as centre.
pub fn stick_area(adjusted_pr: f32, move_angle: i16) -> (u8, u8) {
    let area = if adjusted_pr < 0.2 {
        STICK_CENTER
    } else {
        // DEG2SHORT_ANGLE2 bounds at +-22.5 / 67.5 / 112.5 / 157.5 deg.
        match move_angle as i32 {
            a if a < -28672 => STICK_LEFT,
            a if a < -20480 => STICK_CENTER,
            a if a < -12288 => STICK_BOTTOM,
            a if a < -4096 => STICK_CENTER,
            a if a < 4096 => STICK_RIGHT,
            a if a < 12288 => STICK_CENTER,
            a if a < 20480 => STICK_TOP,
            a if a < 28672 => STICK_CENTER,
            _ => STICK_LEFT,
        }
    };
    let lr = match area {
        0 | 1 | 7 => STICK_LEFT,
        3..=5 => STICK_RIGHT,
        _ => STICK_CENTER,
    };
    (area, lr)
}

/// A page-flip request from the month view (`mCD_MVPL_select`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageFlip {
    /// +1 = next month (MV0, slides in from x = 320), -1 = previous (MV1).
    pub dir: i8,
    /// Pages to slide (`_105A`).
    pub pages: u8,
}

/// Month-view action.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectAction {
    None,
    Close,
    Flip(PageFlip),
    /// A: swap to the day view (`CHG0`).
    OpenDays,
}

/// `mCD_MVPL_select`. `page` = shown month index, `home` = current month
/// index. Updates `page` for the first step like the C.
pub fn month_select(page: &mut usize, home: usize, lr: u8, b_or_start: bool, c_up: bool, a: bool) -> SelectAction {
    if b_or_start {
        return SelectAction::Close;
    }
    if lr == STICK_LEFT {
        if *page + 11 > home {
            *page -= 1;
            return SelectAction::Flip(PageFlip { dir: -1, pages: 1 });
        }
    } else if lr == STICK_RIGHT {
        if *page < home + 11 {
            *page += 1;
            return SelectAction::Flip(PageFlip { dir: 1, pages: 1 });
        }
    } else if c_up {
        if home > *page {
            let pages = (home - *page) as u8;
            *page += 1;
            return SelectAction::Flip(PageFlip { dir: 1, pages });
        } else if home < *page {
            let pages = (*page - home) as u8;
            *page -= 1;
            return SelectAction::Flip(PageFlip { dir: -1, pages });
        }
    } else if a {
        return SelectAction::OpenDays;
    }
    SelectAction::None
}

/// `mCD_MVPL_mv` frame. `pos` = menu x (starts at +-320). Returns true
/// when the flip finished. `dt` = 1.0 on retail.
pub fn page_flip_step(pos: &mut f32, flip: &mut PageFlip, page: &mut usize, dt: f32) -> bool {
    let landed = if flip.pages == 1 {
        add_calc(pos, 0.0, 0.4, 37.0, 1.25).abs() < 0.1
    } else if *pos > 0.0 {
        *pos -= 37.0 * dt;
        *pos <= 0.0
    } else {
        *pos += 37.0 * dt;
        *pos >= 0.0
    };
    if !landed {
        return false;
    }
    flip.pages -= 1;
    if flip.pages == 0 {
        *pos = 0.0;
        return true;
    }
    if flip.dir < 0 {
        *pos -= 320.0;
        *page -= 1;
    } else {
        *pos += 320.0;
        *page += 1;
    }
    false
}

/// Day-view state (`_1057`, `_103C`, `_1042`, `_1043`, `day_type`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DayView {
    pub cell: usize,
    pub events: [u8; MAX_DAY_EVENTS],
    pub shown: u8,
    pub count: u8,
    pub day_type: u8,
}

impl DayView {
    /// `mDC_set_disp_table`.
    pub fn refresh(&mut self, entry: &MonthEntry) {
        self.shown = 0;
        self.count = 0;
        self.events = [0; MAX_DAY_EVENTS];
        for k in 0..entry.event_count as usize {
            if entry.events[k][0] as usize == self.cell {
                if self.count as usize >= MAX_DAY_EVENTS {
                    // C returns before refreshing the day type.
                    return;
                }
                self.events[self.count as usize] = entry.events[k][1];
                self.count += 1;
            }
        }
        self.day_type = (entry.cell(self.cell) >> 5) & 7;
    }

    /// Event shown (without the attended bit) and whether attended.
    pub fn current_event(&self) -> Option<(u8, bool)> {
        (self.count > 0).then(|| {
            let e = self.events[self.shown as usize];
            (e & 0x7F, e & 0x80 != 0)
        })
    }
}

/// Day-view C-button input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DayInput {
    Left,
    Right,
    Up,
    Down,
}

/// `mCD_MVPL_day` cursor movement. Returns true if something moved (the
/// C plays `NA_SE_CURSOL`).
pub fn day_input(view: &mut DayView, entry: &MonthEntry, input: DayInput) -> bool {
    let day = entry.cell(view.cell) & 0x1F;
    match input {
        DayInput::Left if day > 1 => view.cell -= 1,
        DayInput::Right if view.cell < 36 && entry.day_data[view.cell + 1] & 0x1F > 0 => view.cell += 1,
        DayInput::Up if view.count > 1 && view.shown != 0 => {
            view.shown -= 1;
            return true;
        }
        DayInput::Up if day > 7 => view.cell -= 7,
        DayInput::Down if view.count > 1 && view.shown + 1 < view.count => {
            view.shown += 1;
            return true;
        }
        DayInput::Down if view.cell < 30 && entry.day_data[view.cell + 7] & 0x1F > 0 => view.cell += 7,
        _ => return false,
    }
    view.refresh(entry);
    true
}

/// Day the cursor starts on when entering the day view.
pub fn day_view_start(entry: &MonthEntry, page: usize, home: usize, today: u8) -> usize {
    day_to_cell(entry, if page == home { today } else { 1 })
}

/// Cursor blink (`_1038` timer, 60-frame triangle wave) -> alpha.
pub fn cursor_blink(timer: &mut f32, dt: f32) -> u8 {
    *timer += dt;
    while *timer >= 60.0 {
        *timer -= 60.0;
    }
    let t0 = (*timer - 30.0) / 30.0;
    let t1 = if t0 <= 0.0 { 1.0 + t0 } else { 1.0 - t0 };
    (255.0 * t1.clamp(0.0, 1.0)) as i32 as u8
}

/// What A on a day does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiaryOpen {
    /// `mSM_open_submenu_new2(DIARY, arg0, owner, month)`.
    Open { arg0: i32, month_idx: u8 },
    /// Other player's diary is locked: LOCK_DIARY warning.
    Locked,
}

pub fn diary_open(viewer_is_owner: bool, owner_locked: bool, page: usize) -> DiaryOpen {
    let month_idx = (page % MONTHS) as u8;
    if viewer_is_owner {
        DiaryOpen::Open { arg0: 0, month_idx }
    } else if owner_locked {
        DiaryOpen::Locked
    } else {
        DiaryOpen::Open { arg0: 1, month_idx }
    }
}

/// C ABI: `mCD_calendar_event_check` on a calendar pointer.
#[no_mangle]
pub extern "C" fn pc_cd_event_check(cal: *const PlayerCalendar, now_year: u16, now_month: u8, year: u16, month: u8, day: u8, event: u8) -> i32 {
    match unsafe { cal.as_ref() } {
        Some(c) if (1..=12).contains(&month) && (1..=31).contains(&day) => {
            event_check(c, (now_year, now_month), year, month, day, event) as i32
        }
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(now: (u16, u8, u8)) -> CalendarContext {
        CalendarContext { now, town_day: 15, birthday: (0xFF, 0), aus: false, foreigner: false }
    }

    #[test]
    fn save_bookkeeping() {
        let mut c = PlayerCalendar::default();
        welcome_on(&mut c, 2026, 10, 11, 15);
        assert_eq!(c.played_days[9], 1 << 10);
        assert_eq!((c.year, c.month), (2026, 10));
        event_on(&mut c, 2026, 10, 11, EV_HALLOWEEN, 15);
        assert!(event_check(&c, (2026, 10), 2026, 10, 11, EV_HALLOWEEN));
        // Two months later: Nov/Dec wiped, October kept.
        c.played_days[10] = 1;
        welcome_on(&mut c, 2026, 12, 1, 15);
        assert_eq!(c.played_days[10], 0);
        assert_eq!(c.played_days[9], 1 << 10);
        // Back in time within the month clears later days.
        welcome_on(&mut c, 2026, 12, 5, 15);
        welcome_on(&mut c, 2026, 12, 2, 15);
        assert_eq!(c.played_days[11], 0b11);
        // Over a year: wipe.
        welcome_on(&mut c, 2028, 1, 1, 15);
        assert_eq!(c.played_days[9], 0);
    }

    #[test]
    fn check_delete_hits_current_player() {
        let mut other = PlayerCalendar { year: 2020, month: 1, ..Default::default() };
        other.played_days[0] = 7;
        assert_eq!(check_delete(&mut other, 2026, 10, 11, 15), CheckDelete::ClearCurrentPlayer);
        assert_eq!(other.played_days[0], 7);
    }

    #[test]
    fn grid_and_events() {
        // Oct 2026 starts on a Thursday.
        let c = PlayerCalendar::default();
        let y = build_year(2026, &c, &ctx((2026, 10, 11)));
        let oct = &y[9];
        assert_eq!(oct.day_data[4] & 0x1F, 1);
        assert_eq!(oct.day_data[4] >> 5, DAY_NORMAL);
        // Halloween is an event day.
        let cell = day_to_cell(oct, 31);
        assert_eq!(oct.day_data[cell] >> 5, DAY_EVENT);
        // Explorer's Day 2026: 2nd Monday of October = Oct 12.
        assert!(oct.events[..oct.event_count as usize].iter().any(|e| e[1] == EV_EXPLORERS_DAY && oct.day_data[e[0] as usize] & 0x1F == 12));
        // Harvest Festival 2026 (4th Thursday) = Nov 26, Sale Day Nov 27.
        let nov = &y[10];
        let find = |ev| nov.events[..nov.event_count as usize].iter().find(|e| e[1] == ev).map(|e| nov.day_data[e[0] as usize] & 0x1F);
        assert_eq!(find(EV_HARVEST_FESTIVAL), Some(26));
        assert_eq!(find(EV_SALE_DAY), Some(27));
        assert_eq!(find(EV_MAYORS_DAY), Some(3));
        assert!(nov.event_count as usize <= MAX_MONTH_EVENTS);
        let (all, cur) = build_calendar(&c, &ctx((2026, 10, 11)));
        assert_eq!((all.len(), cur), (36, 21));
        assert_eq!(all[cur].day_data[day_to_cell(&all[cur], 11)] >> 5, DAY_CURRENT);
    }

    #[test]
    fn fathers_day_icon_shift() {
        // Father's Day 2026 = June 21 (3rd Sunday).
        let c = PlayerCalendar { event_flags: FLAG_FATHERS_DAY, ..Default::default() };
        let us = ctx((2026, 10, 11));
        assert_eq!(make_icon(2026, 6, 20, &c, &us), 2);
        assert_eq!(make_icon(2026, 6, 21, &c, &us), 0);
        let aus = CalendarContext { aus: true, ..us };
        assert_eq!(make_icon(2026, 6, 21, &c, &aus), 0);
    }

    #[test]
    fn navigation() {
        assert_eq!(stick_area(1.0, 0), (STICK_RIGHT, STICK_RIGHT));
        assert_eq!(stick_area(1.0, 8192), (STICK_CENTER, STICK_CENTER));
        assert_eq!(stick_area(0.1, 0).1, STICK_CENTER);
        let (home, mut page) = (21usize, 21usize);
        for _ in 0..11 {
            assert!(matches!(month_select(&mut page, home, STICK_RIGHT, false, false, false), SelectAction::Flip(_)));
        }
        assert_eq!(month_select(&mut page, home, STICK_RIGHT, false, false, false), SelectAction::None);
        let SelectAction::Flip(mut f) = month_select(&mut page, home, STICK_CENTER, false, true, false) else { panic!() };
        assert_eq!(f.pages, 11);
        let mut pos = -320.0;
        let mut frames = 0;
        while !page_flip_step(&mut pos, &mut f, &mut page, 1.0) {
            frames += 1;
            assert!(frames < 1000);
        }
        assert_eq!(page, home);
        let mut t = 29.0;
        assert_eq!(cursor_blink(&mut t, 1.0), 255);
        assert_eq!(diary_open(false, true, 22), DiaryOpen::Locked);
    }
}
