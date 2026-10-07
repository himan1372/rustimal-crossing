//! Real-time/calendar engine for the Animal Crossing rewrite.
//!
//! This module ports the GameCube time stack that Animal Crossing is built on:
//!
//! ```text
//! GAMECUBE RTC (hardware, battery-backed, epoch 2000-01-01)
//!   │  OSTime ticks (40.5 MHz)
//!   ▼
//! OS CALENDAR (OSTime.c: Gregorian conversion, leap years, weekday)
//!   │  CalendarTime
//!   ▼
//! GAME RTC (lb_rtc.c: RtcTime, weekday, intervals, nth-weekday)
//!   │  + save's time_delta (in-game Set Clock offset; hardware untouched)
//!   ▼
//! GAME CLOCK (m_time.c: seasons/terms, renewal flags, 6 AM boundary)
//!   │
//!   ├──► DAILY CYCLE (6:00 AM reset, renewal flags)
//!   ├──► CALENDAR (events, seasons, weekdays)
//!   └──► TIME OF DAY (hour -> music/lighting/schedules)
//! ```
//!
//! # Provenance
//!
//! Algorithms ported from `flyngmt/ACGC-PC-Port`:
//! - `src/static/dolphin/os/OSTime.c` (ticks <-> calendar, `GetDates`)
//! - `src/lb_rtc.c` (game RTC: `lbRTC_Week`, `lbRTC_GetIntervalDays`,
//!   `lbRTC_Weekly_day`, `lbRTC_GetDaysByMonth`, time add/sub)
//! - `src/game/m_time.c` (`mTM_*`: seasons/terms, renewal, limits)
//! - `src/lb_reki.c` (`lbRk_*`: equinoxes, harvest moon)
//! - `src/pc_os.c` (PC port: GC epoch = 946684800 Unix seconds, timer clock)
//!
//! Quirks are reproduced faithfully, including `lbRTC_GetIntervalDays`'s
//! simplified (century-blind) leap-year counting and the float equinox
//! formulas from the Japanese astronomy reference.

// Public API surface for the rewrite; not every item is referenced inside
// this crate yet.
#![allow(dead_code)]

// ---------------------------------------------------------------------------
// OS time layer (OSTime.c)
// ---------------------------------------------------------------------------

/// GameCube timer clock: bus clock (162 MHz) / 4.
pub const TIMER_CLOCK: i64 = 40_500_000;
/// Seconds from the Unix epoch (1970-01-01) to the GameCube epoch
/// (2000-01-01). `OSTime` 0 == 2000-01-01 00:00:00.
pub const GC_EPOCH_UNIX_DIFF: i64 = 946_684_800;
/// Days from the proleptic Gregorian day 0 to the GameCube epoch.
/// `0xB2575` in the OS source.
const CALENDAR_BIAS: i64 = 0xB2575;

/// Calendar representation (`OSCalendarTime`). Month is 0-based, weekday is
/// 0=Sunday..6=Saturday, matching the OS struct field order.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct CalendarTime {
    pub sec: i32,
    pub min: i32,
    pub hour: i32,
    pub mday: i32,
    pub mon: i32, // 0-based
    pub year: i32,
    pub wday: i32, // 0=Sunday
    pub yday: i32,
    pub msec: i32,
    pub usec: i32,
}

const fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

/// Days elapsed in each month at month start (non-leap / leap).
const YEAR_DAYS: [i32; 12] = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];
const LEAP_YEAR_DAYS: [i32; 12] = [0, 31, 60, 91, 121, 152, 182, 213, 244, 274, 305, 335];

/// Leap days before `year` (proleptic Gregorian).
const fn leap_days_before(year: i32) -> i32 {
    if year < 1 {
        0
    } else {
        (year + 3) / 4 - (year - 1) / 100 + (year - 1) / 400
    }
}

/// `GetDates`: day count -> year/month/day/weekday/yearday. Ported exactly,
/// including `wday = (days + 6) % 7` (2000-01-01 was a Saturday).
fn get_dates(mut days: i64, td: &mut CalendarTime) {
    td.wday = ((days + 6) % 7) as i32;
    // year = days/365, corrected downward while the leap-day count overshoots
    let mut year = (days / 365) as i32;
    loop {
        let n = year as i64 * 365 + leap_days_before(year) as i64;
        if days >= n {
            break;
        }
        year -= 1;
    }
    days -= year as i64 * 365 + leap_days_before(year) as i64;
    td.year = year;
    td.yday = days as i32;
    let md = if is_leap_year(year) {
        LEAP_YEAR_DAYS
    } else {
        YEAR_DAYS
    };
    let mut month = 11i32;
    while days < md[month as usize] as i64 {
        month -= 1;
    }
    td.mon = month;
    td.mday = (days - md[month as usize] as i64 + 1) as i32;
}

/// `OSTicksToCalendarTime`: 64-bit tick count -> calendar fields.
pub fn ticks_to_calendar_time(ticks: i64) -> CalendarTime {
    let mut td = CalendarTime::default();
    let mut d = ticks % TIMER_CLOCK;
    if d < 0 {
        d += TIMER_CLOCK;
    }
    // usec = ticks*8 / (TIMER_CLOCK/125000); msec = ticks / (TIMER_CLOCK/1000)
    td.usec = (((d * 8) / (TIMER_CLOCK / 125_000)) % 1000) as i32;
    td.msec = ((d / (TIMER_CLOCK / 1000)) % 1000) as i32;
    let ticks = ticks - d;
    let total_secs = ticks / TIMER_CLOCK;
    let mut days = total_secs / 86_400 + CALENDAR_BIAS;
    let mut secs = total_secs % 86_400;
    if secs < 0 {
        days -= 1;
        secs += 86_400;
    }
    get_dates(days, &mut td);
    td.hour = (secs / 3600) as i32;
    td.min = ((secs / 60) % 60) as i32;
    td.sec = (secs % 60) as i32;
    td
}

/// `OSCalendarTimeToTicks`: calendar fields -> 64-bit tick count.
/// Handles month overflow the way the OS does.
pub fn calendar_time_to_ticks(td: &CalendarTime) -> i64 {
    let ov_mon = td.mon.div_euclid(12);
    let mon = td.mon.rem_euclid(12);
    let year = td.year + ov_mon;
    let md = if is_leap_year(year) {
        LEAP_YEAR_DAYS
    } else {
        YEAR_DAYS
    };
    let secs = 365i64 * 86_400 * year as i64
        + 86_400i64 * (leap_days_before(year) as i64 + md[mon as usize] as i64 + td.mday as i64 - 1)
        + 3600i64 * td.hour as i64
        + 60i64 * td.min as i64
        + td.sec as i64
        - 0xEB1E1BF80i64;
    secs * TIMER_CLOCK + (td.msec as i64) * (TIMER_CLOCK / 1000) + ((td.usec as i64) * (TIMER_CLOCK / 125_000)) / 8
}

// ---------------------------------------------------------------------------
// Game RTC layer (lb_rtc.c)
// ---------------------------------------------------------------------------

/// Game calendar time (`lbRTC_time_c`). Month is 1-based, weekday 0=Sunday.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct RtcTime {
    pub sec: u8,
    pub min: u8,
    pub hour: u8,
    pub day: u8,
    pub weekday: u8, // 0=Sunday
    pub month: u8,   // 1-based
    pub year: u16,
}

/// Date only (`lbRTC_ymd_c`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct RtcYmd {
    pub year: u16,
    pub month: u8,
    pub day: u8,
}

pub const SUNDAY: u8 = 0;
pub const MONDAY: u8 = 1;
pub const TUESDAY: u8 = 2;
pub const WEDNESDAY: u8 = 3;
pub const THURSDAY: u8 = 4;
pub const FRIDAY: u8 = 5;
pub const SATURDAY: u8 = 6;

/// Game year bounds (`GAME_YEAR_MIN/MAX` in lb_rtc.h).
pub const GAME_YEAR_MIN: u16 = 2000;
/// 2032 on GameCube; the PC port extends this to 2100.
pub const GAME_YEAR_MAX_GC: u16 = 2032;
pub const GAME_YEAR_MAX_PC: u16 = 2100;

/// Days per month, index 1..=12.
const fn days_in_month_table(leap: bool, month: u8) -> u8 {
    match month {
        1 => 31,
        2 => {
            if leap {
                29
            } else {
                28
            }
        }
        3 => 31,
        4 => 30,
        5 => 31,
        6 => 30,
        7 => 31,
        8 => 31,
        9 => 30,
        10 => 31,
        11 => 30,
        12 => 31,
        _ => 0,
    }
}

/// `lbRTC_GetDaysByMonth`: leap-aware month length.
pub const fn get_days_by_month(year: u16, month: u8) -> u8 {
    days_in_month_table(is_leap_year(year as i32), month)
}

/// `lbRTC_Week`: weekday via days since 1901-01-01 (a Tuesday) + 2.
pub fn rtc_week(year: u16, month: u8, day: u8) -> u8 {
    let a = RtcTime {
        year: 1901,
        month: 1,
        day: 1,
        ..RtcTime::default()
    };
    let b = RtcTime {
        year,
        month,
        day,
        ..RtcTime::default()
    };
    ((interval_days(&a, &b) + 2).rem_euclid(7)) as u8
}

/// `lbRTC_GetIntervalDays`: days from t0 to t1 (0 if t0 is later).
///
/// Faithfully reproduces the original's simplified leap-year counting:
/// it divides the year span by 4 and does *not* apply the century rule,
/// so it drifts from the true Gregorian count across 1900/2000-style
/// boundaries. Do not "fix" this; game logic depends on it.
pub fn interval_days(t0: &RtcTime, t1: &RtcTime) -> i32 {
    // Return 0 when t0 is strictly later than t1 (date, then time).
    if t0.year > t1.year {
        return 0;
    }
    if t0.year == t1.year {
        if t0.month > t1.month {
            return 0;
        }
        if t0.month == t1.month {
            if t0.day > t1.day {
                return 0;
            }
            if t0.day == t1.day {
                if t0.hour > t1.hour {
                    return 0;
                }
                if t0.hour == t1.hour && t0.min > t1.min {
                    return 0;
                }
            }
        }
    }
    // Cumulative month-day tables (0-based month index), standard and
    // "leap" (the leap table here is the plain month table shifted by the
    // flawed leap logic below; values match the decomp's total_days).
    const TOTAL_DAYS: [[i32; 13]; 2] = [
        [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334, 365],
        [0, 31, 60, 91, 121, 152, 182, 213, 244, 274, 305, 335, 366],
    ];
    let year_leap_period = (t1.year as i32 - t0.year as i32) / 4;
    let extra_years = (t1.year as i32 - t0.year as i32) % 4;
    let less_leap = if t0.year % 4 == 0 { 1 } else { 0 };
    let over_leap = if t1.year % 4 == 0 { 1 } else { 0 };
    let leap_add = if (((4 - (t0.year % 4)) % 4) as i32) < extra_years {
        1
    } else {
        0
    };
    let mut days = year_leap_period * 1461 + extra_years * 365 + leap_add;
    days += t1.day as i32 - 1;
    days += TOTAL_DAYS[over_leap as usize][t1.month as usize - 1];
    days -= t0.day as i32 - 1;
    days -= TOTAL_DAYS[less_leap as usize][t0.month as usize - 1];
    days
}

/// `lbRTC_GetIntervalDays2`: signed day difference between two dates.
pub fn interval_days_ymd(ymd0: &RtcYmd, ymd1: &RtcYmd) -> i32 {
    let t0 = RtcTime {
        year: ymd0.year,
        month: ymd0.month,
        day: ymd0.day,
        ..RtcTime::default()
    };
    let t1 = RtcTime {
        year: ymd1.year,
        month: ymd1.month,
        day: ymd1.day,
        ..RtcTime::default()
    };
    if ymd0.year == ymd1.year && ymd0.month == ymd1.month && ymd0.day == ymd1.day {
        0
    } else if (ymd0.year, ymd0.month, ymd0.day) > (ymd1.year, ymd1.month, ymd1.day) {
        -interval_days(&t1, &t0)
    } else {
        interval_days(&t0, &t1)
    }
}

/// Special `weeks` value meaning "last <weekday> of the month".
pub const LAST_WEEKDAY_OF_MONTH: i32 = 6;

/// `lbRTC_Weekly_day`: day-of-month of the `weeks`-th `weekday`.
/// `weeks` is 1-based; `LAST_WEEKDAY_OF_MONTH` (6) gives the last one.
pub fn weekly_day(year: u16, month: u8, weeks: i32, weekday: u8) -> u8 {
    let month_first_weekday = rtc_week(year, month, 1);
    let month_days = get_days_by_month(year, month);
    let mut weeks = weeks - 1;
    let mut t_weekday =
        (((weekday as i32 - month_first_weekday as i32) + 7) % 7) + 1;
    while weeks > 0 {
        t_weekday += 7;
        if t_weekday > month_days as i32 {
            t_weekday -= 7;
            break;
        }
        weeks -= 1;
    }
    t_weekday as u8
}

// --- time arithmetic (lbRTC_Add_*/lbRTC_Sub_*) ---

fn normalize_time(t: &mut RtcTime) {
    // Carry seconds/minutes/hours; day/month/year via month lengths.
    let mut carry = t.sec as i32 / 60;
    t.sec = (t.sec as i32 % 60) as u8;
    carry += t.min as i32 / 60;
    t.min = ((t.min as i32 % 60) + carry % 60) as u8;
    carry /= 60;
    carry += t.hour as i32 / 24;
    t.hour = ((t.hour as i32 % 24) + carry % 24) as u8;
    carry /= 24;
    // Day overflow/underflow by walking months.
    let mut day = t.day as i32 + carry;
    let mut month = t.month as i32;
    let mut year = t.year as i32;
    while day > get_days_by_month(year as u16, month as u8) as i32 {
        day -= get_days_by_month(year as u16, month as u8) as i32;
        month += 1;
        if month > 12 {
            month = 1;
            year += 1;
        }
    }
    while day < 1 {
        month -= 1;
        if month < 1 {
            month = 12;
            year -= 1;
        }
        day += get_days_by_month(year as u16, month as u8) as i32;
    }
    t.day = day as u8;
    t.month = month as u8;
    t.year = year as u16;
    t.weekday = rtc_week(t.year, t.month, t.day);
}

/// Add seconds (wrapping into minutes/hours/days).
pub fn add_seconds(t: &mut RtcTime, secs: i32) {
    let total = t.sec as i32 + secs;
    t.sec = total.rem_euclid(60) as u8;
    add_minutes(t, total.div_euclid(60));
}

/// Add minutes.
pub fn add_minutes(t: &mut RtcTime, mins: i32) {
    let total = t.min as i32 + mins;
    t.min = total.rem_euclid(60) as u8;
    add_hours(t, total.div_euclid(60));
}

/// Add hours.
pub fn add_hours(t: &mut RtcTime, hours: i32) {
    let total = t.hour as i32 + hours;
    t.hour = total.rem_euclid(24) as u8;
    add_days(t, total.div_euclid(24));
}

/// Add days (handles month/year rollover and weekday).
pub fn add_days(t: &mut RtcTime, days: i32) {
    let mut day = t.day as i32 + days;
    let mut month = t.month as i32;
    let mut year = t.year as i32;
    while day > get_days_by_month(year as u16, month as u8) as i32 {
        day -= get_days_by_month(year as u16, month as u8) as i32;
        month += 1;
        if month > 12 {
            month = 1;
            year += 1;
        }
    }
    while day < 1 {
        month -= 1;
        if month < 1 {
            month = 12;
            year -= 1;
        }
        day += get_days_by_month(year as u16, month as u8) as i32;
    }
    t.day = day as u8;
    t.month = month as u8;
    t.year = year as u16;
    t.weekday = rtc_week(t.year, t.month, t.day);
}

/// Subtract days.
pub fn sub_days(t: &mut RtcTime, days: i32) {
    add_days(t, -days);
}

// ---------------------------------------------------------------------------
// Game clock layer (m_time.c)
// ---------------------------------------------------------------------------

/// Hour at which daily things reset (`mTM_FIELD_RENEW_HOUR`).
pub const FIELD_RENEW_HOUR: u8 = 6;
/// UI year limits (`mTM_MIN_YEAR`, `mTM_MAX_YEAR`).
pub const MIN_YEAR: u16 = 2001;
pub const MAX_YEAR_GC: u16 = 2030;
pub const MAX_YEAR_PC: u16 = 2100;

/// Seasons (`mTM_SEASON_*`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Season {
    Spring = 0,
    Summer = 1,
    Autumn = 2,
    Winter = 3,
}

/// A calendar term: first date *after* which this term applies is the
/// previous entry; the table is scanned for the first entry with
/// `month/day >= current`, exactly like `mTM_get_termIdx`.
#[derive(Clone, Copy, Debug)]
pub struct CalendarTerm {
    pub month: u8,
    pub day: u8,
    pub season: Season,
    pub bgitem_profile: i16,
    pub bgitem_bank: i16,
}

/// The 18-term season calendar (`mTM_calender` in m_time.c).
pub const CALENDAR_TERMS: [CalendarTerm; 18] = [
    CalendarTerm { month: 2, day: 3, season: Season::Winter, bgitem_profile: 0x0050, bgitem_bank: 0x0025 },
    CalendarTerm { month: 2, day: 17, season: Season::Winter, bgitem_profile: 0x0050, bgitem_bank: 0x0025 },
    CalendarTerm { month: 2, day: 24, season: Season::Winter, bgitem_profile: 0x0050, bgitem_bank: 0x0025 },
    CalendarTerm { month: 3, day: 31, season: Season::Spring, bgitem_profile: 0x0001, bgitem_bank: 0x0004 },
    CalendarTerm { month: 4, day: 8, season: Season::Spring, bgitem_profile: 0x004f, bgitem_bank: 0x0024 },
    CalendarTerm { month: 5, day: 25, season: Season::Spring, bgitem_profile: 0x0001, bgitem_bank: 0x0004 },
    CalendarTerm { month: 7, day: 22, season: Season::Summer, bgitem_profile: 0x0001, bgitem_bank: 0x0004 },
    CalendarTerm { month: 8, day: 31, season: Season::Summer, bgitem_profile: 0x0001, bgitem_bank: 0x0004 },
    CalendarTerm { month: 9, day: 15, season: Season::Summer, bgitem_profile: 0x0001, bgitem_bank: 0x0004 },
    CalendarTerm { month: 9, day: 30, season: Season::Autumn, bgitem_profile: 0x0001, bgitem_bank: 0x0004 },
    CalendarTerm { month: 10, day: 15, season: Season::Autumn, bgitem_profile: 0x0001, bgitem_bank: 0x0004 },
    CalendarTerm { month: 10, day: 29, season: Season::Autumn, bgitem_profile: 0x0001, bgitem_bank: 0x0004 },
    CalendarTerm { month: 11, day: 12, season: Season::Autumn, bgitem_profile: 0x0001, bgitem_bank: 0x0004 },
    CalendarTerm { month: 11, day: 28, season: Season::Autumn, bgitem_profile: 0x0001, bgitem_bank: 0x0004 },
    CalendarTerm { month: 12, day: 9, season: Season::Autumn, bgitem_profile: 0x0001, bgitem_bank: 0x0004 },
    CalendarTerm { month: 12, day: 17, season: Season::Winter, bgitem_profile: 0x0051, bgitem_bank: 0x0026 },
    CalendarTerm { month: 12, day: 25, season: Season::Winter, bgitem_profile: 0x0051, bgitem_bank: 0x0026 },
    CalendarTerm { month: 12, day: 31, season: Season::Winter, bgitem_profile: 0x0050, bgitem_bank: 0x0025 },
];

/// Island climate always uses term 7 (`mTM_TERM_7`).
pub const ISLAND_TERM: usize = 7;

/// `mTM_get_termIdx`: term for a month/day (None past Dec 31, like the
/// original returning -1).
pub fn term_index(month: u8, day: u8, island_climate: bool) -> Option<usize> {
    if island_climate {
        return Some(ISLAND_TERM);
    }
    for (i, term) in CALENDAR_TERMS.iter().enumerate() {
        if month < term.month || (month == term.month && day <= term.day) {
            return Some(i);
        }
    }
    None
}

/// Renewal flags (`mTM_RENEW_TIME_*`).
pub const RENEW_NONE: u8 = 0x00;
pub const RENEW_ALL: u8 = 0xFF;
pub const RENEW_WEATHER: u8 = 0;
pub const RENEW_DAILY: u8 = 1;

/// `mTM_renewal_renew_time`: true when the saved renewal date differs from
/// the current date, i.e. daily systems must refresh.
pub fn renewal_needed(saved: &RtcYmd, current: &RtcTime) -> bool {
    saved.year != current.year || saved.month != current.month || saved.day != current.day
}

/// `mTM_rtcTime_limit_check`: clamp a time into the UI-supported year range.
/// Pass `MAX_YEAR_PC` for the PC port's extended range.
pub fn clamp_year(t: &mut RtcTime, max_year: u16) {
    if t.year > max_year {
        t.year = max_year;
    } else if t.year < MIN_YEAR {
        t.year = MIN_YEAR;
    }
}

/// The game clock: hardware RTC plus the saved `time_delta`.
///
/// This is the architectural core Philip's research identified: the in-game
/// Set Clock never touches the hardware. `lbRTC_SetTime` stores
/// `time_delta = desired_ticks - hard_ticks`, and `lbRTC_GetTime` returns
/// `hard_ticks + time_delta`.
#[derive(Clone, Copy, Debug, Default)]
pub struct GameClock {
    /// Saved offset in OSTime ticks (from the save data's `time_delta`).
    pub time_delta: i64,
}

impl GameClock {
    /// `lbRTC_GetGameTime`: current game time = hardware ticks + delta.
    pub fn game_time(&self, hard_ticks: i64) -> RtcTime {
        let ct = ticks_to_calendar_time(hard_ticks + self.time_delta);
        RtcTime {
            sec: ct.sec as u8,
            min: ct.min as u8,
            hour: ct.hour as u8,
            day: ct.mday as u8,
            weekday: ct.wday as u8,
            month: (ct.mon + 1) as u8,
            year: ct.year as u16,
        }
    }

    /// `lbRTC_SetTime`: point the game clock at `t` by adjusting the delta.
    /// Returns the new delta (to be saved).
    pub fn set_game_time(&mut self, t: &RtcTime, hard_ticks: i64) -> i64 {
        let ct = CalendarTime {
            sec: t.sec as i32,
            min: t.min as i32,
            hour: t.hour as i32,
            mday: t.day as i32,
            mon: (t.month as i32) - 1,
            year: t.year as i32,
            ..CalendarTime::default()
        };
        self.time_delta = calendar_time_to_ticks(&ct) - hard_ticks;
        self.time_delta
    }

    /// Seconds since midnight for the current game time.
    pub fn now_seconds(&self, hard_ticks: i64) -> i64 {
        let t = self.game_time(hard_ticks);
        t.sec as i64 + t.min as i64 * 60 + t.hour as i64 * 3600
    }
}

/// Game-day boundary: the Animal Crossing "day" rolls at 06:00, not midnight.
/// Returns the game-day date for a given time: before 06:00 the game day is
/// still the previous calendar date's cycle.
pub fn game_day_ymd(t: &RtcTime) -> RtcYmd {
    if t.hour < FIELD_RENEW_HOUR {
        let mut shifted = *t;
        sub_days(&mut shifted, 1);
        RtcYmd { year: shifted.year, month: shifted.month, day: shifted.day }
    } else {
        RtcYmd { year: t.year, month: t.month, day: t.day }
    }
}

// ---------------------------------------------------------------------------
// Astronomical layer (lb_reki.c)
// ---------------------------------------------------------------------------

/// `lbRk_VernalEquinoxDay`: March equinox day (formula from the Japanese
/// astronomy reference, valid 1980-2099).
pub fn vernal_equinox_day(year: i32) -> i32 {
    let y = (year - 1980) as f32;
    (20.8431 + 0.242194 * y) as i32 - (year - 1980) / 4
}

/// `lbRk_AutumnalEquinoxDay`: September equinox day.
pub fn autumnal_equinox_day(year: i32) -> i32 {
    let y = (year - 1980) as f32;
    (23.2488 + 0.242194 * y) as i32 - (year - 1980) / 4
}

/// Precomputed Harvest Moon (8th lunisolar month, 15th day) Gregorian dates
/// for 2002-2030, from the GameCube `ev_day` table in lb_reki.c.
/// (The PC port extends/corrects this to 2099; the GC range is reproduced
/// here. Out-of-range years return None; the original falls back to a
/// lunisolar conversion.)
pub fn harvest_moon_day(year: i32) -> Option<(u8, u8)> {
    const TABLE: [(u8, u8); 29] = [
        (9, 21), (9, 10), (9, 28), (9, 18), (10, 7), (9, 26), (9, 15),
        (10, 4), (9, 23), (9, 12), (9, 30), (9, 19), (9, 9), (9, 28),
        (9, 16), (10, 5), (9, 25), (9, 14), (10, 1), (9, 20), (9, 10),
        (9, 29), (9, 18), (10, 7), (9, 26), (9, 15), (10, 3), (9, 22),
        (9, 11),
    ];
    if (2002..=2030).contains(&year) {
        Some(TABLE[(year - 2002) as usize])
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Event scheduler primitives
// ---------------------------------------------------------------------------

/// A date predicate for the event scheduler, mirroring the kinds of rules
/// `m_event.c` builds with `lbRTC_Weekly_day`, fixed dates, and the
/// astronomical helpers above.
#[derive(Clone, Copy, Debug)]
pub enum DatePredicate {
    /// Fixed month/day (e.g. New Year's Day, Halloween).
    Fixed { month: u8, day: u8 },
    /// Nth weekday of a month (e.g. 4th Thursday of November).
    /// `weeks == LAST_WEEKDAY_OF_MONTH` means the last one.
    NthWeekday { month: u8, weeks: i32, weekday: u8 },
    /// Every `weekday` (e.g. Joan on Sundays).
    Weekly(u8),
    /// Harvest Moon day for the year (astronomical).
    HarvestMoon,
    /// Vernal equinox day (March).
    VernalEquinox,
    /// Autumnal equinox day (September).
    AutumnalEquinox,
}

impl DatePredicate {
    /// Day-of-month this predicate selects in `year`/`month`, if any.
    pub fn day_in(&self, year: u16, month: u8) -> Option<u8> {
        match *self {
            DatePredicate::Fixed { month: m, day: d } => {
                if m == month {
                    Some(d)
                } else {
                    None
                }
            }
            DatePredicate::NthWeekday { month: m, weeks, weekday } => {
                if m == month {
                    Some(weekly_day(year, month, weeks, weekday))
                } else {
                    None
                }
            }
            DatePredicate::Weekly(weekday) => {
                // First occurrence on/after the 1st; scheduler steps by 7.
                let first = weekly_day(year, month, 1, weekday);
                Some(first)
            }
            DatePredicate::HarvestMoon => {
                harvest_moon_day(year as i32).and_then(|(m, d)| {
                    if m == month {
                        Some(d)
                    } else {
                        None
                    }
                })
            }
            DatePredicate::VernalEquinox => {
                if month == 3 {
                    Some(vernal_equinox_day(year as i32) as u8)
                } else {
                    None
                }
            }
            DatePredicate::AutumnalEquinox => {
                if month == 9 {
                    Some(autumnal_equinox_day(year as i32) as u8)
                } else {
                    None
                }
            }
        }
    }

    /// True when `t`'s calendar date satisfies this predicate.
    pub fn matches(&self, t: &RtcTime) -> bool {
        self.day_in(t.year, t.month) == Some(t.day)
            || matches!(self, DatePredicate::Weekly(w) if *w == t.weekday)
    }
}

/// A scheduled event window: date predicate + daily time window.
/// Mirrors `mEv_schedule_c`'s date range + time range shape.
#[derive(Clone, Copy, Debug)]
pub struct EventWindow {
    pub predicate: DatePredicate,
    /// Start/end day offsets relative to the predicate day (for rumor
    /// windows like "10 days before the sports fair").
    pub day_offset_start: i32,
    pub day_offset_end: i32,
    /// Daily time window in seconds since midnight; (0, 86400) = all day.
    pub start_sec: i32,
    pub end_sec: i32,
}

impl EventWindow {
    /// True when the event is active at game time `t`.
    pub fn is_active(&self, t: &RtcTime) -> bool {
        let base_day = match self.predicate.day_in(t.year, t.month) {
            Some(d) => d as i32,
            None => return false,
        };
        // Build the window's date range by shifting the base day.
        let mut start = RtcTime { day: base_day as u8, month: t.month, year: t.year, ..RtcTime::default() };
        add_days(&mut start, self.day_offset_start);
        let mut end = RtcTime { day: base_day as u8, month: t.month, year: t.year, ..RtcTime::default() };
        add_days(&mut end, self.day_offset_end);
        let today = (t.year, t.month, t.day);
        let in_range = (start.year, start.month, start.day) <= today
            && today <= (end.year, end.month, end.day);
        if !in_range {
            return false;
        }
        let now = t.hour as i32 * 3600 + t.min as i32 * 60 + t.sec as i32;
        now >= self.start_sec && now < self.end_sec
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn ymd(year: u16, month: u8, day: u8) -> RtcTime {
        RtcTime { year, month, day, ..RtcTime::default() }
    }

    #[test]
    fn gc_epoch_is_2000_01_01_saturday() {
        let ct = ticks_to_calendar_time(0);
        assert_eq!((ct.year, ct.mon, ct.mday), (2000, 0, 1));
        assert_eq!(ct.wday, 6); // Saturday
        assert_eq!((ct.hour, ct.min, ct.sec), (0, 0, 0));
    }

    #[test]
    fn calendar_roundtrip() {
        let ct = CalendarTime {
            year: 2026, mon: 9, mday: 6, hour: 21, min: 30, sec: 15,
            ..CalendarTime::default()
        };
        let ticks = calendar_time_to_ticks(&ct);
        let back = ticks_to_calendar_time(ticks);
        assert_eq!((back.year, back.mon, back.mday), (2026, 9, 6));
        assert_eq!((back.hour, back.min, back.sec), (21, 30, 15));
        // 2026-10-06 was a Tuesday
        assert_eq!(back.wday, 2);
    }

    #[test]
    fn leap_day_roundtrip() {
        // 2000-02-29 (leap year) and the day after
        for (mon, mday, yday) in [(1, 29, 59), (2, 1, 60)] {
            let ct = CalendarTime { year: 2000, mon, mday, ..CalendarTime::default() };
            let back = ticks_to_calendar_time(calendar_time_to_ticks(&ct));
            assert_eq!((back.mon, back.mday, back.yday), (mon, mday, yday));
        }
        // 2001 is not a leap year: Feb has 28 days
        assert_eq!(get_days_by_month(2000, 2), 29);
        assert_eq!(get_days_by_month(2001, 2), 28);
        assert_eq!(get_days_by_month(1900, 2), 28); // century rule
    }

    #[test]
    fn known_weekdays() {
        // Cross-checked against the real calendar.
        assert_eq!(rtc_week(2000, 1, 1), SATURDAY);
        assert_eq!(rtc_week(2026, 10, 6), TUESDAY);
        assert_eq!(rtc_week(2001, 1, 1), MONDAY);
        assert_eq!(rtc_week(2030, 12, 31), TUESDAY);
    }

    #[test]
    fn interval_days_basic() {
        let a = ymd(2026, 10, 1);
        let b = ymd(2026, 10, 6);
        assert_eq!(interval_days(&a, &b), 5);
        assert_eq!(interval_days(&b, &a), 0); // later first -> 0
        assert_eq!(interval_days_ymd(
            &RtcYmd { year: 2026, month: 10, day: 6 },
            &RtcYmd { year: 2026, month: 10, day: 1 },
        ), -5);
    }

    #[test]
    fn interval_days_across_leap_day() {
        let a = ymd(2000, 2, 28);
        let b = ymd(2000, 3, 1);
        assert_eq!(interval_days(&a, &b), 2); // Feb 29 exists in 2000
    }

    #[test]
    fn nth_weekday_calculations() {
        // 4th Thursday of November 2026 -> Nov 26 (Thanksgiving)
        assert_eq!(weekly_day(2026, 11, 4, THURSDAY), 26);
        // 2nd Monday of October 2026 -> Oct 12
        assert_eq!(weekly_day(2026, 10, 2, MONDAY), 12);
        // Last Sunday of June 2026 -> Jun 28
        assert_eq!(weekly_day(2026, 6, LAST_WEEKDAY_OF_MONTH, SUNDAY), 28);
        // Last Sunday of November 2026 -> Nov 29
        assert_eq!(weekly_day(2026, 11, LAST_WEEKDAY_OF_MONTH, SUNDAY), 29);
    }

    #[test]
    fn season_terms_match_decomp() {
        // Spot checks against mTM_calender boundaries.
        assert_eq!(term_index(1, 15, false), Some(0)); // Jan -> first term (Feb 3)
        assert_eq!(term_index(2, 3, false), Some(0));
        assert_eq!(term_index(2, 4, false), Some(1));
        assert_eq!(term_index(5, 25, false), Some(5)); // May 25 -> May 25 term (spring)
        assert_eq!(term_index(5, 26, false), Some(6)); // May 26 -> Jul 22 term (summer)
        assert_eq!(term_index(6, 1, false), Some(6)); // Jun 1 -> summer term
        assert_eq!(term_index(6, 2, false), Some(6)); // Jun 2 -> Jul 22 term (summer)
        assert_eq!(term_index(12, 25, false), Some(16));
        assert_eq!(term_index(12, 26, false), Some(17));
        assert_eq!(term_index(12, 31, false), Some(17));
        assert_eq!(term_index(7, 4, true), Some(ISLAND_TERM));
        let t = &CALENDAR_TERMS[term_index(6, 2, false).unwrap()];
        assert_eq!(t.season, Season::Summer);
    }

    #[test]
    fn game_day_boundary_at_6am() {
        let mut t = ymd(2026, 10, 7);
        t.hour = 5;
        // 5:59 AM Oct 7 is still the Oct 6 game day
        assert_eq!(game_day_ymd(&t), RtcYmd { year: 2026, month: 10, day: 6 });
        t.hour = 6;
        assert_eq!(game_day_ymd(&t), RtcYmd { year: 2026, month: 10, day: 7 });
    }

    #[test]
    fn renewal_detection() {
        let saved = RtcYmd { year: 2026, month: 10, day: 5 };
        let mut now = ymd(2026, 10, 6);
        now.hour = 7;
        assert!(renewal_needed(&saved, &now));
        let same = ymd(2026, 10, 5);
        assert!(!renewal_needed(&saved, &same));
    }

    #[test]
    fn year_limits() {
        let mut t = ymd(2031, 6, 1);
        clamp_year(&mut t, MAX_YEAR_GC);
        assert_eq!(t.year, 2030);
        let mut t2 = ymd(1999, 6, 1);
        clamp_year(&mut t2, MAX_YEAR_GC);
        assert_eq!(t2.year, 2001);
    }

    #[test]
    fn set_clock_uses_delta_not_hardware() {
        let mut clock = GameClock::default();
        let hard = 1_000_000 * TIMER_CLOCK; // arbitrary hardware ticks
        // Point the game clock at a specific time; hardware is untouched.
        let target = RtcTime { year: 2004, month: 9, day: 15, hour: 20, ..RtcTime::default() };
        clock.set_game_time(&target, hard);
        let got = clock.game_time(hard);
        assert_eq!((got.year, got.month, got.day), (2004, 9, 15));
        assert_eq!(got.hour, 20);
        // Hardware advancing moves game time equally.
        let later = clock.game_time(hard + 3600 * TIMER_CLOCK);
        assert_eq!(later.hour, 21);
    }

    #[test]
    fn equinox_and_harvest_moon() {
        // Known values: 2026 vernal equinox Mar 20, autumnal Sep 23 (approx;
        // the game's float formula is what we test).
        assert_eq!(vernal_equinox_day(2026), 20);
        assert_eq!(autumnal_equinox_day(2026), 23);
        // Harvest Moon table spot checks (GameCube table; note the PC port
        // corrected 17 dates, e.g. 2026 is Sep 25 corrected vs Sep 26 GC).
        assert_eq!(harvest_moon_day(2026), Some((9, 26)));
        assert_eq!(harvest_moon_day(2030), Some((9, 11)));
        assert_eq!(harvest_moon_day(2031), None); // GC table ends at 2030
    }

    #[test]
    fn event_predicates() {
        let thanksgiving = DatePredicate::NthWeekday { month: 11, weeks: 4, weekday: THURSDAY };
        let mut t = ymd(2026, 11, 26);
        t.weekday = rtc_week(2026, 11, 26);
        assert!(thanksgiving.matches(&t));
        let t2 = ymd(2026, 11, 27);
        assert!(!thanksgiving.matches(&t2));
        // Joan: every Sunday 6:00-12:00
        let joan = EventWindow {
            predicate: DatePredicate::Weekly(SUNDAY),
            day_offset_start: 0,
            day_offset_end: 0,
            start_sec: 6 * 3600,
            end_sec: 12 * 3600,
        };
        let mut sunday = ymd(2026, 10, 4); // a Sunday
        sunday.weekday = rtc_week(2026, 10, 4);
        sunday.hour = 8;
        assert!(joan.is_active(&sunday));
        sunday.hour = 13;
        assert!(!joan.is_active(&sunday));
    }

    #[test]
    fn add_days_rolls_months() {
        let mut t = ymd(2026, 1, 31);
        add_days(&mut t, 1);
        assert_eq!((t.month, t.day), (2, 1));
        assert_eq!(t.weekday, rtc_week(2026, 2, 1));
        sub_days(&mut t, 2);
        assert_eq!((t.month, t.day), (1, 30));
    }
}
