//! Weather, seasons, wind, and the daily renewal boundary.
//!
//! Verified against `m_time.c` / `m_time.h` (calendar terms, renewal),
//! `m_kankyo_weather.c_inc` (weather/wind tables, event overrides),
//! `m_kankyo.h` (enums, save packing), `m_event.c`
//! (mEv_GetEventWeather), `m_start_data_init.c` (startup ordering)
//! (USA Rev. 0 decomp / PC port).
//!
//! Architecture: RTC -> 18-term seasonal calendar (season + term +
//! BG resources), 20-term weather probability calendar (10-unit roll),
//! event schedule compiler -> event_today[]; event weather overrides
//! random weather; first-job rain is cleared; weather is saved as one
//! packed byte. Wind is a separate 5-term system with a Koinobori
//! event override.

/// Season enums (mTM_SEASON_*).
pub mod season {
    pub const SPRING: u8 = 0;
    pub const SUMMER: u8 = 1;
    pub const AUTUMN: u8 = 2;
    pub const WINTER: u8 = 3;
}

/// 18 calendar terms: (end month, end day, season, bgitem_profile,
/// bgitem_bank). End dates, not start dates (mTM_calender).
pub const CALENDAR_TERMS: [(u8, u8, u8, u16, u16); 18] = [
    (2, 3, season::WINTER, 0x0050, 0x0025),
    (2, 17, season::WINTER, 0x0050, 0x0025),
    (2, 24, season::WINTER, 0x0050, 0x0025),
    (3, 31, season::SPRING, 0x0001, 0x0004),
    (4, 8, season::SPRING, 0x004F, 0x0024),
    (5, 25, season::SPRING, 0x0001, 0x0004),
    (7, 22, season::SUMMER, 0x0001, 0x0004),
    (8, 31, season::SUMMER, 0x0001, 0x0004),
    (9, 15, season::SUMMER, 0x0001, 0x0004),
    (9, 30, season::AUTUMN, 0x0001, 0x0004),
    (10, 15, season::AUTUMN, 0x0001, 0x0004),
    (10, 29, season::AUTUMN, 0x0001, 0x0004),
    (11, 12, season::AUTUMN, 0x0001, 0x0004),
    (11, 28, season::AUTUMN, 0x0001, 0x0004),
    (12, 9, season::AUTUMN, 0x0001, 0x0004),
    (12, 17, season::WINTER, 0x0051, 0x0026),
    (12, 25, season::WINTER, 0x0051, 0x0026),
    (12, 31, season::WINTER, 0x0050, 0x0025),
];

/// mTM_TERM_7: the island's forced term (summer).
pub const ISLAND_TERM: usize = 7;

/// Term lookup: first term with month < end_month ||
/// (month == end_month && day <= end_day). Island bypasses the table.
pub fn term_idx(month: u8, day: u8, is_island: bool) -> Option<usize> {
    if is_island {
        return Some(ISLAND_TERM);
    }
    CALENDAR_TERMS
        .iter()
        .position(|&(m, d, _, _, _)| month < m || (month == m && day <= d))
}

/// Weather types (mEnv_WEATHER_*).
pub mod weather {
    pub const CLEAR: u8 = 0;
    pub const RAIN: u8 = 1;
    pub const SNOW: u8 = 2;
    pub const SAKURA: u8 = 3;
}

/// Weather intensities (mEnv_WEATHER_INTENSITY_*).
pub mod intensity {
    pub const NONE: u8 = 0;
    pub const LIGHT: u8 = 1;
    pub const NORMAL: u8 = 2;
    pub const HEAVY: u8 = 3;
}

/// 20 weather-term end dates (mEnv_GetWeatherChangeStep).
pub const WEATHER_TERMS: [(u8, u8); 20] = [
    (1, 7), (2, 24), (3, 31), (4, 4), (4, 7), (4, 8), (4, 19), (4, 20),
    (6, 25), (7, 15), (8, 31), (9, 30), (10, 30), (10, 31), (11, 15),
    (12, 9), (12, 10), (12, 23), (12, 30), (12, 31),
];

/// Weather probability entries: (clear, rain, thunder, snow, blizzard,
/// sakura, heavy_sakura); each row sums to 10.
pub const WEATHER_TABLE: [(u8, u8, u8, u8, u8, u8, u8); 20] = [
    (10, 0, 0, 0, 0, 0, 0),
    (7, 0, 0, 2, 1, 0, 0),
    (8, 2, 0, 0, 0, 0, 0),
    (10, 0, 0, 0, 0, 0, 0),
    (0, 0, 0, 0, 0, 10, 0),
    (0, 0, 0, 0, 0, 0, 10),
    (8, 2, 0, 0, 0, 0, 0),
    (10, 0, 0, 0, 0, 0, 0),
    (7, 2, 1, 0, 0, 0, 0),
    (5, 3, 2, 0, 0, 0, 0),
    (9, 0, 1, 0, 0, 0, 0),
    (6, 0, 4, 0, 0, 0, 0),
    (8, 2, 0, 0, 0, 0, 0),
    (10, 0, 0, 0, 0, 0, 0),
    (8, 1, 0, 1, 0, 0, 0),
    (7, 0, 0, 3, 0, 0, 0),
    (0, 0, 0, 0, 10, 0, 0),
    (4, 0, 0, 4, 2, 0, 0),
    (0, 0, 0, 6, 4, 0, 0),
    (10, 0, 0, 0, 0, 0, 0),
];

/// Weather-term lookup (same end-date walk as the calendar).
pub fn weather_term(month: u8, day: u8) -> usize {
    WEATHER_TERMS
        .iter()
        .position(|&(m, d)| month < m || (month == m && day <= d))
        .unwrap_or(19)
}

/// Daily weather roll: selected in [0,10), cumulative walk over the
/// entry. Returns (weather, intensity). `bugfix_sakura` selects the
/// BUGFIXES behavior (sakura field); without it the original bug reads
/// the snow field again for sakura probability.
pub fn weather_roll(entry: (u8, u8, u8, u8, u8, u8, u8), selected: f32, bugfix_sakura: bool) -> (u8, u8) {
    let (clear, rain, thunder, snow, blizzard, sakura, _) = entry;
    let c5 = if bugfix_sakura { sakura } else { snow }; // @BUG: original reads snow field
    let c = [clear, rain, thunder, snow, blizzard, c5];
    let mut acc = 0u8;
    for (i, &w) in c.iter().enumerate() {
        acc += w;
        if selected < acc as f32 {
            return match i {
                0 => (weather::CLEAR, intensity::LIGHT),
                1 => (weather::RAIN, intensity::LIGHT),
                2 => (weather::RAIN, intensity::HEAVY),
                3 => (weather::SNOW, intensity::LIGHT),
                4 => (weather::SNOW, intensity::HEAVY),
                _ => (weather::SAKURA, intensity::LIGHT),
            };
        }
    }
    (weather::SAKURA, intensity::HEAVY)
}

/// Event-weather override precedence (mEv_GetEventWeather):
/// WEATHER_CLEAR -> CLEAR; WEATHER_SNOW -> SNOW;
/// WEATHER_SPORTS_FAIR -> CLEAR; else none (-1).
/// Inputs are "is this event scheduled today" flags.
pub fn event_weather_override(clear: bool, snow: bool, sports_fair: bool) -> Option<(u8, u8)> {
    if clear {
        Some((weather::CLEAR, intensity::HEAVY))
    } else if snow {
        Some((weather::SNOW, intensity::HEAVY))
    } else if sports_fair {
        Some((weather::CLEAR, intensity::HEAVY))
    } else {
        None
    }
}

/// Weather save packing: intensity | (weather << 4).
pub fn pack_weather_save(w: u8, i: u8) -> u8 {
    i | (w << 4)
}
pub fn unpack_weather_type(saved: u8) -> u8 {
    (saved & 0xF0) >> 4
}
pub fn unpack_weather_intensity(saved: u8) -> u8 {
    saved & 0x0F
}

/// Daily renewal boundary hour (mTM_FIELD_RENEW_HOUR).
pub const FIELD_RENEW_HOUR: u8 = 6;

/// Wind: 5 terms (end dates), (calm, normal, gusty) percents, and the
/// base power ranges each class maps to.
pub const WIND_TERMS: [(u8, u8); 4] = [(1, 7), (4, 5), (8, 19), (9, 30)];
pub const WIND_PERCENTS: [(u8, u8, u8); 5] = [
    (10, 80, 10), (0, 70, 30), (10, 80, 10), (0, 70, 30), (10, 80, 10),
];
pub const WIND_POWER_RANGES: [(f32, f32); 3] = [(0.0, 0.4), (0.4, 0.6), (0.6, 1.0)];

/// Wind term lookup; falls through to the final term (Oct 1 - Dec 31).
pub fn wind_term(month: u8, day: u8) -> usize {
    WIND_TERMS
        .iter()
        .position(|&(m, d)| month < m || (month == m && day <= d))
        .unwrap_or(4)
}

/// Koinobori event wind override: 135 degrees (0x6000), power 1.0.
pub const KOINOBORI_WIND_ANGLE_DEG: f32 = 135.0;
pub const KOINOBORI_WIND_POWER: f32 = 1.0;

// ---- C ABI ----

/// C ABI: term index for a date; is_island nonzero forces term 7.
/// Returns 255 if no term matches.
#[no_mangle]
pub extern "C" fn pc_term_idx(month: u8, day: u8, is_island: u8) -> u8 {
    term_idx(month, day, is_island != 0).map(|t| t as u8).unwrap_or(255)
}

/// C ABI: weather term for a date.
#[no_mangle]
pub extern "C" fn pc_weather_term(month: u8, day: u8) -> u8 {
    weather_term(month, day) as u8
}

/// C ABI: weather roll. entry_idx 0..20, selected in [0,10).
/// bugfix nonzero selects BUGFIXES sakura behavior.
/// Returns packed save byte (intensity | weather<<4).
#[no_mangle]
pub extern "C" fn pc_weather_roll(entry_idx: usize, selected: f32, bugfix: u8) -> u8 {
    if entry_idx >= 20 {
        return 0xFF;
    }
    let (w, i) = weather_roll(WEATHER_TABLE[entry_idx], selected, bugfix != 0);
    pack_weather_save(w, i)
}

/// C ABI: event weather override. Returns 1 and fills out[2] when an
/// override applies, else 0.
#[no_mangle]
pub extern "C" fn pc_event_weather(clear: u8, snow: u8, sports: u8, out: *mut u8) -> u8 {
    if out.is_null() {
        return 0;
    }
    match event_weather_override(clear != 0, snow != 0, sports != 0) {
        Some((w, i)) => unsafe {
            *out.add(0) = w;
            *out.add(1) = i;
            1
        },
        None => 0,
    }
}

/// C ABI: wind term for a date.
#[no_mangle]
pub extern "C" fn pc_wind_term(month: u8, day: u8) -> u8 {
    wind_term(month, day) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn term_lookup() {
        // End-date semantics: Jan 1 - Feb 3 -> 0.
        assert_eq!(term_idx(1, 1, false), Some(0));
        assert_eq!(term_idx(2, 3, false), Some(0));
        assert_eq!(term_idx(2, 4, false), Some(1));
        assert_eq!(term_idx(2, 24, false), Some(2));
        assert_eq!(term_idx(2, 25, false), Some(3));
        assert_eq!(term_idx(4, 8, false), Some(4));
        assert_eq!(term_idx(4, 9, false), Some(5));
        assert_eq!(term_idx(12, 31, false), Some(17));
        // Island bypass.
        assert_eq!(term_idx(12, 25, true), Some(ISLAND_TERM));
        assert_eq!(CALENDAR_TERMS[ISLAND_TERM].2, season::SUMMER);
        // C ABI.
        assert_eq!(pc_term_idx(2, 3, 0), 0);
        assert_eq!(pc_term_idx(6, 15, 1), 7);
    }

    #[test]
    fn weather_table_and_roll() {
        assert_eq!(WEATHER_TERMS.len(), 20);
        assert_eq!(WEATHER_TABLE.len(), 20);
        for e in WEATHER_TABLE {
            let sum = e.0 + e.1 + e.2 + e.3 + e.4 + e.5 + e.6;
            assert_eq!(sum, 10);
        }
        assert_eq!(weather_term(1, 1), 0);
        assert_eq!(weather_term(4, 8), 5);
        assert_eq!(weather_term(12, 31), 19);
        // Entry 1: {7,0,0,2,1,0,0}: 70% clear, 20% light snow, 10% blizzard.
        let e = WEATHER_TABLE[1];
        assert_eq!(weather_roll(e, 0.0, true), (weather::CLEAR, intensity::LIGHT));
        assert_eq!(weather_roll(e, 7.5, true), (weather::SNOW, intensity::LIGHT));
        assert_eq!(weather_roll(e, 9.5, true), (weather::SNOW, intensity::HEAVY));
        // Entry 4: 100% light sakura with the fix; without it the bug
        // reads the snow field (0) so it falls through to heavy sakura.
        let e4 = WEATHER_TABLE[4];
        assert_eq!(weather_roll(e4, 5.0, true), (weather::SAKURA, intensity::LIGHT));
        assert_eq!(weather_roll(e4, 5.0, false), (weather::SAKURA, intensity::HEAVY));
        // C ABI returns packed save byte.
        assert_eq!(pc_weather_roll(0, 5.0, 1), pack_weather_save(weather::CLEAR, intensity::LIGHT));
        assert_eq!(unpack_weather_type(0x21), 2);
        assert_eq!(unpack_weather_intensity(0x21), 1);
        // Event override precedence.
        assert_eq!(event_weather_override(true, true, true), Some((weather::CLEAR, intensity::HEAVY)));
        assert_eq!(event_weather_override(false, true, true), Some((weather::SNOW, intensity::HEAVY)));
        assert_eq!(event_weather_override(false, false, true), Some((weather::CLEAR, intensity::HEAVY)));
        assert_eq!(event_weather_override(false, false, false), None);
    }

    #[test]
    fn wind_terms() {
        assert_eq!(wind_term(1, 1), 0);
        assert_eq!(wind_term(4, 5), 1);
        assert_eq!(wind_term(4, 6), 2);
        assert_eq!(wind_term(8, 19), 2);
        assert_eq!(wind_term(8, 20), 3);
        assert_eq!(wind_term(12, 31), 4);
        assert_eq!(WIND_PERCENTS[0], (10, 80, 10));
        assert_eq!(WIND_PERCENTS[1], (0, 70, 30));
        assert_eq!(FIELD_RENEW_HOUR, 6);
        assert_eq!(pc_wind_term(9, 30), 3);
    }
}
