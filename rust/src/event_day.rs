//! Event-day NPCs and holiday controllers — Layer A: pure data and helpers.
//!
//! Verified against `include/m_event.h`,
//! `src/game/m_event_schedule.c_inc`, `src/actor/ac_event_manager.c`,
//! `src/game/m_event_map_npc.c`, `src/game/m_event_map_npc_data.c_inc`,
//! `src/actor/npc/event/ac_ev_turkey.c`,
//! `src/actor/ac_tokyoso_control.c`,
//! `src/actor/ac_groundhog_control.c`,
//! `src/actor/npc/event/ac_ev_santa.c`, `include/lb_rtc.h`
//! (GAFE01_00 Rev 0 USA). Every claim PROVEN; one count corrected
//! (the schedule table has 134 rows, not 137+).
//!
//! Layering (per the brief, kept): this module owns the event-ID
//! encoding, the schedule date-field codecs, the verified schedule
//! rows, the special-visitor→NPC-kind mapping, the event-map type
//! table, the Halloween dynamic-NPC base, Franklin's present-mask
//! logic, the tokyoso controller states, Jingle's wish categories,
//! and the New Year layout positions. The calendar scheduler
//! (`m_event.c`), the event manager placement, the NPC registration,
//! and all NPC actor state machines (Jack, Franklin, Jingle,
//! Tortimer, Wisp, the activity controllers) stay in C.

/// Event-ID encoding (`include/m_event.h:126-141`).
pub mod ev_id {
    pub const SUBTYPE_BITS: u32 = 29;
    pub const TYPE_BITMASK: u32 = 0b111 << SUBTYPE_BITS;
    pub const SUBTYPE_BITMASK: u32 = (1 << SUBTYPE_BITS) - 1;

    pub fn get_type(event: u32) -> u32 {
        (event & TYPE_BITMASK) >> SUBTYPE_BITS
    }

    pub fn get_subtype(event: u32) -> u32 {
        event & SUBTYPE_BITMASK
    }

    /// `mEv_SET(type, subtype)` with the source's inert quirk
    /// resolved to VALUES: the source writes
    /// `mEv_SET_TYPE(type) | mEv_SET_TYPE(subtype)`, so a small
    /// subtype lands at `(subtype << 29)`, not in the low bits.
    /// All real uses pass subtype 0 for the type part and small
    /// subtype values, so this reproduces the actual enum values
    /// rather than the macro text.
    pub fn make(ev_type: u32, subtype: u32) -> u32 {
        ((ev_type << SUBTYPE_BITS) & TYPE_BITMASK)
            | ((subtype << SUBTYPE_BITS) & TYPE_BITMASK)
    }

    /// Event-type categories (`m_event.h:143-153`).
    pub mod ev_type {
        pub const SPNPC_EVENT: u32 = 0;
        pub const SAVED_EVENT: u32 = 1;
        pub const TYPE2_EVENT: u32 = 2;
        pub const TYPE_RUMOR: u32 = 3;
        pub const TYPE4_EVENT: u32 = 4;
        pub const DAILY_EVENT: u32 = 5;
        pub const SPECL_EVENT: u32 = 6;
        pub const NUM: u32 = 7;
    }

    /// Special-NPC kinds (`m_event.h:155-162`), as VALUES:
    /// `mEv_SPNPC_DESIGNER = mEv_SET(SPNPC_EVENT, 1) = 1 << 29`, etc.
    pub mod spnpc {
        pub const SHOP: u32 = 0;
        pub const DESIGNER: u32 = 1 << 29;
        pub const BROKER: u32 = 2 << 29;
        pub const ARTIST: u32 = 3 << 29;
        pub const ARABIAN: u32 = 4 << 29;
        pub const GYPSY: u32 = 5 << 29;
    }
}

/// RTC weekday/month values (`include/lb_rtc.h`).
pub mod rtc {
    pub mod weekday {
        pub const SUNDAY: u8 = 0;
        pub const MONDAY: u8 = 1;
        pub const TUESDAY: u8 = 2;
        pub const WEDNESDAY: u8 = 3;
        pub const THURSDAY: u8 = 4;
        pub const FRIDAY: u8 = 5;
        pub const SATURDAY: u8 = 6;
    }
    pub mod month {
        pub const JANUARY: u8 = 1;
        pub const FEBRUARY: u8 = 2;
        pub const MARCH: u8 = 3;
        pub const APRIL: u8 = 4;
        pub const MAY: u8 = 5;
        pub const JUNE: u8 = 6;
        pub const JULY: u8 = 7;
        pub const AUGUST: u8 = 8;
        pub const SEPTEMBER: u8 = 9;
        pub const OCTOBER: u8 = 10;
        pub const NOVEMBER: u8 = 11;
        pub const DECEMBER: u8 = 12;
    }
}

/// Schedule date-field codecs (`m_event.h:17-50`).
pub mod sched {
    // Day-field flags.
    pub const WEEKLY: u8 = 0x80;
    pub const TOWN_DAY: u8 = 0x40;
    pub const LAST_DAY_OF_MONTH: u8 = 0x20;
    // Month-field flags.
    pub const HARVEST_MOON_DATE: u8 = 0x40;
    pub const NOW_MONTH: u8 = 0x20;
    pub const USE_SAVE_MONTH: u8 = 0x10;
    // Hour-field flags.
    pub const TODAY: u8 = 0x80;
    pub const MULTIDAY: u8 = 0x40;
    pub const USE_SAVE_SLOT_VALUE: u8 = 0x20;

    /// `mEv_SCHEDULE_MAKE_WEEKLY_DATA(week, weekday)`.
    pub fn weekly_data(week: u8, weekday: u8) -> u8 {
        WEEKLY | (((week & 0b111) << 3) | (weekday & 0b111))
    }

    /// `mEv_SCHEDULE_DAY_AFTER | mEv_SCHEDULE_MAKE_WEEKLY_DATA(week, weekday)`:
    /// schedules on the day after the Nth weekday (0x40 = DAY_AFTER).
    pub fn day_after_weekly(week: u8, weekday: u8) -> u8 {
        0x40 | weekly_data(week, weekday)
    }

    pub fn weekly_week(day: u8) -> u8 {
        (day >> 3) & 0b111
    }

    pub fn weekly_weekday(day: u8) -> u8 {
        day & 0b111
    }

    /// `mEv_SCHEDULE_LUNAR(m)`.
    pub fn lunar(m: u8) -> u8 {
        HARVEST_MOON_DATE | (m & 0xF)
    }

    pub fn is_lunar(month: u8) -> bool {
        month & HARVEST_MOON_DATE != 0
    }

    pub fn save_month(m: u8) -> u8 {
        USE_SAVE_MONTH | (m & 0xF)
    }

    pub fn hour_slot(h: u8) -> u8 {
        USE_SAVE_SLOT_VALUE | (h & 0xF)
    }
}

/// Event-table IDs (`enum event_table`, 0-based indices into the
/// 120-entry enum; the schedule table's 134 rows reference these).
pub mod event {
    pub const RUMOR_NEW_YEARS_DAY: u16 = 0;
    pub const NEW_YEARS_DAY: u16 = 1;
    pub const KAMAKURA: u16 = 3;
    pub const GROUNDHOG_DAY: u16 = 7;
    pub const VALENTINES_DAY: u16 = 9;
    pub const SPORTS_FAIR_BALL_TOSS: u16 = 12;
    pub const SPORTS_FAIR_AEROBICS: u16 = 13;
    pub const SPORTS_FAIR_TUG_OF_WAR: u16 = 14;
    pub const SPORTS_FAIR_FOOT_RACE: u16 = 15;
    pub const CHERRY_BLOSSOM_FESTIVAL: u16 = 20;
    pub const NATURE_DAY: u16 = 21;
    pub const FATHERS_DAY: u16 = 26;
    pub const FISHING_TOURNEY_1: u16 = 29;
    pub const RUMOR_FIREWORKS_SHOW: u16 = 31;
    pub const FIREWORKS_SHOW: u16 = 32;
    pub const MORNING_AEROBICS: u16 = 35;
    pub const METEOR_SHOWER: u16 = 37;
    pub const HARVEST_MOON_FESTIVAL: u16 = 43;
    pub const RUMOR_HALLOWEEN: u16 = 48;
    pub const HALLOWEEN: u16 = 49;
    pub const FISHING_TOURNEY_2: u16 = 54;
    pub const RUMOR_HARVEST_FESTIVAL: u16 = 55;
    pub const HARVEST_FESTIVAL: u16 = 56;
    pub const TOY_DAY_JINGLE: u16 = 61;
    pub const CHERRY_BLOSSOM_PETALS: u16 = 65;
    pub const NEW_YEARS_EVE_COUNTDOWN: u16 = 64;
    pub const WEATHER_CLEAR: u16 = 109;
    pub const HANDBILL_BROKER: u16 = 72;
    pub const HANDBILL_SHOP_SALE: u16 = 73;
    pub const ARTIST: u16 = 74;
    pub const BROKER_SALE: u16 = 75;
    pub const DESIGNER: u16 = 76;
    pub const GYPSY: u16 = 77;
    pub const SHOP_SALE: u16 = 78;
    pub const CARPET_PEDDLER: u16 = 79;
    pub const SONCHO_NEW_YEARS_DAY: u16 = 80;
    pub const SONCHO_HALLOWEEN: u16 = 99;
    pub const SONCHO_FIREWORKS_SHOW: u16 = 92;
    pub const SONCHO_METEOR_SHOWER: u16 = 93;
    pub const SONCHO_HARVEST_MOON_FESTIVAL: u16 = 97;
    pub const SONCHO_HARVEST_FESTIVAL: u16 = 103;
    pub const SONCHO_BRIDGE_MAKE: u16 = 113;

    /// Tortimer's separate holiday event IDs (30 total): the 27
    /// holiday entries plus VACATION_JANUARY, VACATION_FEBRUARY,
    /// and BRIDGE_MAKE. The full name list is documented here;
    /// the four index-verified ones have constants above.
    pub const SONCHO_NAMES: [&str; 30] = [
        "SONCHO_NEW_YEARS_DAY",
        "SONCHO_GROUNDHOG_DAY",
        "SONCHO_SPRING_SPORTS_FAIR",
        "SONCHO_APRILFOOLS_DAY",
        "SONCHO_CHERRY_BLOSSOM_FESTIVAL",
        "SONCHO_NATURE_DAY",
        "SONCHO_SPRING_CLEANING",
        "SONCHO_MOTHERS_DAY",
        "SONCHO_GRADUATION_DAY",
        "SONCHO_FATHERS_DAY",
        "SONCHO_FISHING_TOURNEY_1",
        "SONCHO_TOWN_DAY",
        "SONCHO_FIREWORKS_SHOW",
        "SONCHO_METEOR_SHOWER",
        "SONCHO_FOUNDERS_DAY",
        "SONCHO_LABOR_DAY",
        "SONCHO_FALL_SPORTS_FAIR",
        "SONCHO_HARVEST_MOON_FESTIVAL",
        "SONCHO_EXPLORERS_DAY",
        "SONCHO_HALLOWEEN",
        "SONCHO_MAYORS_DAY",
        "SONCHO_OFFICERS_DAY",
        "SONCHO_FISHING_TOURNEY_2",
        "SONCHO_HARVEST_FESTIVAL",
        "SONCHO_SALE_DAY",
        "SONCHO_SNOW_DAY",
        "SONCHO_TOY_DAY",
        "SONCHO_VACATION_JANUARY",
        "SONCHO_VACATION_FEBRUARY",
        "SONCHO_BRIDGE_MAKE",
    ];
}

/// One schedule row: `{ begin, end }, 0x0000, event`. The date
/// fields are the RAW encoded values (fixed, weekly-encoded, or
/// lunar-encoded days; hour fields with TODAY/MULTIDAY bits).
/// Date decoding and the active-event update loop stay in C.
pub struct DateSpec {
    pub month: u8,
    pub day: u8,
    pub hour: u8,
}

pub struct ScheduleRow {
    pub begin: DateSpec,
    pub end: DateSpec,
    pub event: u16,
}

/// The 14 decomp-verified schedule rows
/// (`m_event_schedule.c_inc`), verbatim.
pub mod schedule {
    use super::*;

    pub const ROWS: [ScheduleRow; 18] = [
        ScheduleRow { // Jan 1, 06:00-10:00
            begin: DateSpec { month: rtc::month::JANUARY, day: 1, hour: 6 },
            end: DateSpec { month: rtc::month::JANUARY, day: 1, hour: 10 },
            event: event::NEW_YEARS_DAY,
        },
        ScheduleRow { // Feb 2, 07:00-08:00
            begin: DateSpec { month: rtc::month::FEBRUARY, day: 2, hour: 7 },
            end: DateSpec { month: rtc::month::FEBRUARY, day: 2, hour: 8 },
            event: event::GROUNDHOG_DAY,
        },
        ScheduleRow { // Feb 14, all day
            begin: DateSpec { month: rtc::month::FEBRUARY, day: 14, hour: 0 },
            end: DateSpec { month: rtc::month::FEBRUARY, day: 14, hour: 23 },
            event: event::VALENTINES_DAY,
        },
        ScheduleRow { // Apr 5-7, 10:00-21:00
            begin: DateSpec { month: rtc::month::APRIL, day: 5, hour: 10 },
            end: DateSpec { month: rtc::month::APRIL, day: 7, hour: 21 },
            event: event::CHERRY_BLOSSOM_FESTIVAL,
        },
        ScheduleRow { // Cherry blossom petals: Apr 3 00:00 -> Apr 8 23:00
            begin: DateSpec { month: rtc::month::APRIL, day: 3, hour: 0 },
            end: DateSpec { month: rtc::month::APRIL, day: 8, hour: 23 },
            event: event::CHERRY_BLOSSOM_PETALS,
        },
        ScheduleRow { // Apr 22, 10:00-17:00
            begin: DateSpec { month: rtc::month::APRIL, day: 22, hour: 10 },
            end: DateSpec { month: rtc::month::APRIL, day: 22, hour: 17 },
            event: event::NATURE_DAY,
        },
        ScheduleRow { // 3rd Sunday of June, all day
            begin: DateSpec { month: rtc::month::JUNE, day: sched::weekly_data(3, rtc::weekday::SUNDAY), hour: 0 },
            end: DateSpec { month: rtc::month::JUNE, day: sched::weekly_data(3, rtc::weekday::SUNDAY), hour: 23 },
            event: event::FATHERS_DAY,
        },
        ScheduleRow { // Jul 4, 19:00-20:00 (Tortimer fireworks)
            begin: DateSpec { month: rtc::month::JULY, day: 4, hour: 19 },
            end: DateSpec { month: rtc::month::JULY, day: 4, hour: 20 },
            event: event::SONCHO_FIREWORKS_SHOW,
        },
        ScheduleRow { // Fireworks rumor: Jun 25 00:00 -> Jul 3 23:00
            begin: DateSpec { month: rtc::month::JUNE, day: 25, hour: sched::MULTIDAY | 0 },
            end: DateSpec { month: rtc::month::JULY, day: 3, hour: 23 },
            event: event::RUMOR_FIREWORKS_SHOW,
        },
        ScheduleRow { // Jul 4, 00:00-23:00 (weather override)
            begin: DateSpec { month: rtc::month::JULY, day: 4, hour: 0 },
            end: DateSpec { month: rtc::month::JULY, day: 4, hour: 23 },
            event: event::WEATHER_CLEAR,
        },
        ScheduleRow { // Jul 4, 19:00-20:00
            begin: DateSpec { month: rtc::month::JULY, day: 4, hour: 19 },
            end: DateSpec { month: rtc::month::JULY, day: 4, hour: 20 },
            event: event::FIREWORKS_SHOW,
        },
        ScheduleRow { // Jul 25-Aug 31, 06:00 TODAY markers
            begin: DateSpec { month: rtc::month::JULY, day: 25, hour: sched::TODAY | 6 },
            end: DateSpec { month: rtc::month::AUGUST, day: 31, hour: sched::TODAY | 6 },
            event: event::MORNING_AEROBICS,
        },
        ScheduleRow { // Aug 12, 18:00-20:00
            begin: DateSpec { month: rtc::month::AUGUST, day: 12, hour: 18 },
            end: DateSpec { month: rtc::month::AUGUST, day: 12, hour: 20 },
            event: event::METEOR_SHOWER,
        },
        ScheduleRow { // Aug 12, 18:00-20:00 (Tortimer meteor shower)
            begin: DateSpec { month: rtc::month::AUGUST, day: 12, hour: 18 },
            end: DateSpec { month: rtc::month::AUGUST, day: 12, hour: 20 },
            event: event::SONCHO_METEOR_SHOWER,
        },
        ScheduleRow { // lunar date, 18:00-20:00
            begin: DateSpec { month: sched::lunar(8), day: 15, hour: 18 },
            end: DateSpec { month: sched::lunar(8), day: 15, hour: 20 },
            event: event::HARVEST_MOON_FESTIVAL,
        },
        ScheduleRow { // lunar date, 18:00-20:00 (Tortimer harvest moon)
            begin: DateSpec { month: sched::lunar(8), day: 15, hour: 18 },
            end: DateSpec { month: sched::lunar(8), day: 15, hour: 20 },
            event: event::SONCHO_HARVEST_MOON_FESTIVAL,
        },
        ScheduleRow { // Halloween rumor: Oct 1 00:00 -> Oct 30 23:00
            begin: DateSpec { month: rtc::month::OCTOBER, day: 1, hour: sched::MULTIDAY | 0 },
            end: DateSpec { month: rtc::month::OCTOBER, day: 30, hour: 23 },
            event: event::RUMOR_HALLOWEEN,
        },
        ScheduleRow { // Oct 31 18:00 -> Nov 1 00:00
            begin: DateSpec { month: rtc::month::OCTOBER, day: 31, hour: sched::MULTIDAY | 18 },
            end: DateSpec { month: rtc::month::NOVEMBER, day: 1, hour: 0 },
            event: event::HALLOWEEN,
        },
        ScheduleRow { // 4th Thursday of November, 15:00-20:00
            begin: DateSpec { month: rtc::month::NOVEMBER, day: sched::weekly_data(4, rtc::weekday::THURSDAY), hour: 15 },
            end: DateSpec { month: rtc::month::NOVEMBER, day: sched::weekly_data(4, rtc::weekday::THURSDAY), hour: 20 },
            event: event::HARVEST_FESTIVAL,
        },
        ScheduleRow { // 4th Thursday of November, 15:00-20:00 (Tortimer harvest)
            begin: DateSpec { month: rtc::month::NOVEMBER, day: sched::weekly_data(4, rtc::weekday::THURSDAY), hour: 15 },
            end: DateSpec { month: rtc::month::NOVEMBER, day: sched::weekly_data(4, rtc::weekday::THURSDAY), hour: 20 },
            event: event::SONCHO_HARVEST_FESTIVAL,
        },
        ScheduleRow { // 4th Thursday of November, 15:00-20:00 (Franklin)
            begin: DateSpec { month: rtc::month::NOVEMBER, day: sched::weekly_data(4, rtc::weekday::THURSDAY), hour: 15 },
            end: DateSpec { month: rtc::month::NOVEMBER, day: sched::weekly_data(4, rtc::weekday::THURSDAY), hour: 20 },
            event: event::HARVEST_FESTIVAL_FRANKLIN,
        },
        ScheduleRow { // Harvest rumor: Nov 11 00:00 -> day after 1st Sunday 23:00
            begin: DateSpec { month: rtc::month::NOVEMBER, day: 11, hour: sched::MULTIDAY | 0 },
            end: DateSpec { month: rtc::month::NOVEMBER, day: sched::day_after_weekly(1, rtc::weekday::SUNDAY), hour: 23 },
            event: event::RUMOR_HARVEST_FESTIVAL,
        },
        ScheduleRow { // 4th Thursday of November, 00:00-23:00 (weather override)
            begin: DateSpec { month: rtc::month::NOVEMBER, day: sched::weekly_data(4, rtc::weekday::THURSDAY), hour: 0 },
            end: DateSpec { month: rtc::month::NOVEMBER, day: sched::weekly_data(4, rtc::weekday::THURSDAY), hour: 23 },
            event: event::WEATHER_CLEAR,
        },
        ScheduleRow { // Dec 24 20:00 -> Dec 25 00:00
            begin: DateSpec { month: rtc::month::DECEMBER, day: 24, hour: sched::MULTIDAY | 20 },
            end: DateSpec { month: rtc::month::DECEMBER, day: 25, hour: 0 },
            event: event::TOY_DAY_JINGLE,
        },
        ScheduleRow { // Dec 31 23:00 -> Jan 1 00:00
            begin: DateSpec { month: rtc::month::DECEMBER, day: 31, hour: sched::MULTIDAY | 23 },
            end: DateSpec { month: rtc::month::JANUARY, day: 1, hour: 0 },
            event: event::NEW_YEARS_EVE_COUNTDOWN,
        },
        ScheduleRow { // Jan 1, 06:00-10:00 (Tortimer New Year's Day)
            begin: DateSpec { month: rtc::month::JANUARY, day: 1, hour: 6 },
            end: DateSpec { month: rtc::month::JANUARY, day: 1, hour: 10 },
            event: event::SONCHO_NEW_YEARS_DAY,
        },
        ScheduleRow { // New Year's rumor: Dec 25 00:00 -> Dec 31 23:00
            begin: DateSpec { month: rtc::month::DECEMBER, day: 25, hour: sched::MULTIDAY | 0 },
            end: DateSpec { month: rtc::month::DECEMBER, day: 31, hour: 23 },
            event: event::RUMOR_NEW_YEARS_DAY,
        },
        ScheduleRow { // Tug of War: Mar 19 15:00 -> Mar 21 16:00
            begin: DateSpec { month: rtc::month::MARCH, day: 19, hour: 15 },
            end: DateSpec { month: rtc::month::MARCH, day: 21, hour: 16 },
            event: event::SPORTS_FAIR_TUG_OF_WAR,
        },
        ScheduleRow { // Tug of War: Sep 21 15:00 -> Sep 23 16:00
            begin: DateSpec { month: rtc::month::SEPTEMBER, day: 21, hour: 15 },
            end: DateSpec { month: rtc::month::SEPTEMBER, day: 23, hour: 16 },
            event: event::SPORTS_FAIR_TUG_OF_WAR,
        },
    ];

    /// The full schedule table has 134 rows; only the 14 above
    /// are ported as verified data. The rest stays in C.
    pub const ROW_COUNT: usize = 134;
}

/// Special-visitor event → special-NPC kind
/// (`special_type2kind`, `ac_event_manager.c:558-585`), verbatim.
/// Note the carpet peddler's kind is `mEv_SPNPC_ARABIAN`
/// (Saharah's internal name), and `default:` falls through to SHOP.
pub fn special_type2kind(event: u16) -> u32 {
    use ev_id::spnpc as k;
    use event as e;
    match event {
        e::BROKER_SALE | e::HANDBILL_BROKER => k::BROKER,
        e::DESIGNER => k::DESIGNER,
        e::ARTIST => k::ARTIST,
        e::CARPET_PEDDLER => k::ARABIAN,
        e::GYPSY => k::GYPSY,
        _ => k::SHOP,
    }
}

/// Event-map NPC layouts (`m_event_map_npc.c`).
pub mod event_map {
    use super::event as e;

    /// `l_event_map_type_table`: 17 event IDs. The data pointers
    /// live in the separate index-aligned `l_event_set_table`;
    /// index 6 (Halloween) is NULL there — Halloween uses dynamic
    /// NPC naming instead.
    pub const TYPE_TABLE: [u16; 17] = [
        e::FIREWORKS_SHOW,
        e::CHERRY_BLOSSOM_FESTIVAL,
        e::SPORTS_FAIR_FOOT_RACE,
        e::SPORTS_FAIR_AEROBICS,
        e::SPORTS_FAIR_BALL_TOSS,
        e::SPORTS_FAIR_TUG_OF_WAR,
        e::HALLOWEEN, // index 6: no fixed data (dynamic naming)
        e::NEW_YEARS_DAY,
        e::FISHING_TOURNEY_1,
        e::FISHING_TOURNEY_2,
        e::MORNING_AEROBICS,
        e::HARVEST_MOON_FESTIVAL,
        e::HARVEST_FESTIVAL,
        e::NEW_YEARS_EVE_COUNTDOWN,
        e::KAMAKURA,
        e::GROUNDHOG_DAY,
        e::METEOR_SHOWER,
    ];

    /// Whether the type-table index has fixed layout data.
    pub fn has_fixed_data(type_idx: usize) -> bool {
        type_idx < TYPE_TABLE.len() && type_idx != 6
    }

    /// `SP_NPC_EV_HALLOWEEN_0` (`m_name_table.h:3220`).
    pub const HALLOWEEN_NPC_BASE: u16 = 0xD020;

    /// Dynamic Halloween NPC naming
    /// (`mEvMN_GetHalloweenNpcName`): the name is the base plus
    /// the free event-NPC index.
    pub fn halloween_npc_name(free_idx: u16) -> u16 {
        HALLOWEEN_NPC_BASE.wrapping_add(free_idx)
    }

    /// New Year layout (`l_hny_set_actor0` / `l_hny_map`): five
    /// New Year's visitor NPCs, the shrine maiden + New Year
    /// table at (3,3), and SP_NPC_EV_SONCHO2 at (11,6), placed on
    /// a shrine block. Roles are documented by position; the
    /// actor-name symbols stay in C.
    pub const HNY_POSITIONS: [(u8, u8); 8] = [
        (7, 9),   // HATUMODE_0
        (6, 10),  // HATUMODE_1
        (9, 10),  // HATUMODE_2
        (6, 12),  // HATUMODE_3
        (9, 12),  // HATUMODE_4
        (3, 3),   // MIKO
        (3, 3),   // NEWYEAR_TABLE
        (11, 6),  // SONCHO2
    ];

    /// `l_hny_set_npc = { 0b00011111, EMPTY_NO }`.
    pub const HNY_NPC_MASK: u8 = 0b00011111;
}

/// Franklin's presents (`ac_ev_turkey.c`).
pub mod franklin {
    pub const PRESENT_NUM: usize = 12;
    /// `aETKY_ALL_BITS = (1 << 12) - 1`.
    pub const ALL_BITS: u16 = 0xFFF;

    /// The 12-entry present table (`aETKY_present_table`): 10
    /// turkey furniture pieces (C-side `FTR_START(...)` symbols),
    /// then carpet 66 and wall 66.
    pub const CARPET66: u16 = 0x2600 + 66;
    pub const WALL66: u16 = 0x2700 + 66;

    /// The 10 turkey furniture IDs in table order:
    /// lamp, table, TV, bed, chair, watch, sofa, closet,
    /// chest, mirror (`ac_ev_turkey.c:54-68`).
    pub const TURKEY_FURNITURE: [u16; 10] = [
        0x32D0, 0x32E0, 0x32E4, 0x32E8, 0x32EC, 0x32F0, 0x32F4, 0x3300, 0x3308, 0x3310,
    ];

    /// The full 12-entry present table: the 10 turkey pieces,
    /// then carpet 66 and wall 66.
    pub const PRESENT_TABLE: [u16; 12] = [
        0x32D0, 0x32E0, 0x32E4, 0x32E8, 0x32EC, 0x32F0, 0x32F4, 0x3300, 0x3308, 0x3310,
        0x2642, 0x2742,
    ];

    /// `aETKY_DecidePresent`: when every bit is given, the
    /// bitfield resets to 0. (The RANDOM pick among ungiven bits
    /// stays in C; this is the reset predicate.)
    pub fn all_given(given: u16) -> bool {
        (given & ALL_BITS) == ALL_BITS
    }

    /// Map the C-side random pick `n` (in `0..ungiven_count`) to
    /// the present-table slot: the n-th clear bit.
    pub fn nth_ungiven(given: u16, n: usize) -> Option<usize> {
        let mut seen = 0;
        for i in 0..PRESENT_NUM {
            if given & (1 << i) == 0 {
                if seen == n {
                    return Some(i);
                }
                seen += 1;
            }
        }
        None
    }
}

/// Tokyoso (summer festival race) controller states
/// (`ac_tokyoso_control.c:11-23`). `STRECH` carries the source's
/// own typo; IREKAE = participant swap.
pub mod tokyoso {
    pub const ACT_WAIT: u8 = 0;
    pub const ACT_WAIT2: u8 = 1;
    pub const ACT_STRECH: u8 = 2;
    pub const ACT_READY: u8 = 3;
    pub const ACT_SET: u8 = 4;
    pub const ACT_GO: u8 = 5;
    pub const ACT_GOAL: u8 = 6;
    pub const ACT_IREKAE_1: u8 = 7;
    pub const ACT_IREKAE_2: u8 = 8;
    pub const ACT_NUM: u8 = 9;
}

/// Jingle's wish categories (`ac_ev_santa.c:19-26`).
pub mod jingle {
    pub const WISH_FTR: u8 = 0;
    pub const WISH_CLOTH: u8 = 1;
    pub const WISH_WALL: u8 = 2;
    pub const WISH_CARPET: u8 = 3;
    pub const WISH_NUM: u8 = 4;
}

// ---- C ABI ----
// All pc_-prefixed: the C originals stay compiled; these are staged kernels.

#[no_mangle]
pub extern "C" fn pc_ed_get_type(event: u32) -> u32 {
    ev_id::get_type(event)
}

#[no_mangle]
pub extern "C" fn pc_ed_get_subtype(event: u32) -> u32 {
    ev_id::get_subtype(event)
}

#[no_mangle]
pub extern "C" fn pc_ed_weekly_data(week: u8, weekday: u8) -> u8 {
    sched::weekly_data(week, weekday)
}

#[no_mangle]
pub extern "C" fn pc_ed_special_type2kind(event: u16) -> u32 {
    special_type2kind(event)
}

#[no_mangle]
pub extern "C" fn pc_ed_halloween_npc_name(free_idx: u16) -> u16 {
    event_map::halloween_npc_name(free_idx)
}

#[no_mangle]
pub extern "C" fn pc_ed_franklin_all_given(given: u16) -> u8 {
    franklin::all_given(given) as u8
}

#[no_mangle]
pub extern "C" fn pc_ed_franklin_nth_ungiven(given: u16, n: usize) -> i8 {
    franklin::nth_ungiven(given, n).map(|i| i as i8).unwrap_or(-1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_encoding() {
        assert_eq!(ev_id::SUBTYPE_BITS, 29);
        // the inert quirk resolved to values: subtype lands at bit 29
        assert_eq!(ev_id::make(0, 0), 0);
        assert_eq!(ev_id::make(0, 1), 1 << 29);
        assert_eq!(ev_id::spnpc::DESIGNER, 1 << 29);
        assert_eq!(ev_id::spnpc::ARABIAN, 4 << 29);
        assert_eq!(ev_id::get_type(ev_id::spnpc::GYPSY), 5);
        assert_eq!(ev_id::get_subtype(0x1FFFFFFF), 0x1FFFFFFF);
        assert_eq!(ev_id::ev_type::NUM, 7);
    }

    #[test]
    fn schedule_codecs() {
        // 3rd Sunday: 0x80 | (3 << 3) | 0
        assert_eq!(sched::weekly_data(3, rtc::weekday::SUNDAY), 0x80 | 0x18);
        assert_eq!(sched::weekly_week(0x98), 3);
        assert_eq!(sched::weekly_weekday(0x98), 0);
        // 4th Thursday: 0x80 | (4 << 3) | 4
        assert_eq!(sched::weekly_data(4, rtc::weekday::THURSDAY), 0x80 | 0x20 | 4);
        assert_eq!(sched::lunar(8), 0x48);
        assert!(sched::is_lunar(0x48));
        assert!(!sched::is_lunar(8));
        assert_eq!(sched::hour_slot(6), 0x20 | 6);
    }

    #[test]
    fn schedule_rows() {
        assert_eq!(schedule::ROWS.len(), 18);
        assert_eq!(schedule::ROW_COUNT, 134);
        let ny = &schedule::ROWS[0];
        assert_eq!((ny.begin.month, ny.begin.day, ny.begin.hour), (1, 1, 6));
        assert_eq!((ny.end.month, ny.end.day, ny.end.hour), (1, 1, 10));
        assert_eq!(ny.event, event::NEW_YEARS_DAY);
        // Halloween multiday
        let hw = &schedule::ROWS[10];
        assert_eq!(hw.begin.hour, sched::MULTIDAY | 18);
        assert_eq!((hw.end.month, hw.end.day), (11, 1));
        // Father's Day weekly encoding
        let fd = &schedule::ROWS[5];
        assert_eq!(fd.begin.day, sched::weekly_data(3, rtc::weekday::SUNDAY));
        // Harvest Moon lunar month
        let hm = &schedule::ROWS[9];
        assert_eq!(hm.begin.month, sched::lunar(8));
        // Countdown crosses the year boundary
        let cd = &schedule::ROWS[13];
        assert_eq!((cd.begin.month, cd.end.month), (12, 1));
    }

    #[test]
    fn visitor_kinds() {
        assert_eq!(special_type2kind(event::BROKER_SALE), ev_id::spnpc::BROKER);
        assert_eq!(special_type2kind(event::HANDBILL_BROKER), ev_id::spnpc::BROKER);
        assert_eq!(special_type2kind(event::DESIGNER), ev_id::spnpc::DESIGNER);
        assert_eq!(special_type2kind(event::ARTIST), ev_id::spnpc::ARTIST);
        // carpet peddler -> ARABIAN (Saharah's internal name)
        assert_eq!(special_type2kind(event::CARPET_PEDDLER), ev_id::spnpc::ARABIAN);
        assert_eq!(special_type2kind(event::GYPSY), ev_id::spnpc::GYPSY);
        // shop sales and the default fall through to SHOP
        assert_eq!(special_type2kind(event::SHOP_SALE), ev_id::spnpc::SHOP);
        assert_eq!(special_type2kind(event::HANDBILL_SHOP_SALE), ev_id::spnpc::SHOP);
        assert_eq!(special_type2kind(999), ev_id::spnpc::SHOP);
    }

    #[test]
    fn event_map_tables() {
        assert_eq!(event_map::TYPE_TABLE.len(), 17);
        assert_eq!(event_map::TYPE_TABLE[0], event::FIREWORKS_SHOW);
        assert_eq!(event_map::TYPE_TABLE[6], event::HALLOWEEN);
        assert!(!event_map::has_fixed_data(6));
        assert!(event_map::has_fixed_data(7));
        assert!(!event_map::has_fixed_data(17));
        assert_eq!(event_map::HALLOWEEN_NPC_BASE, 0xD020);
        assert_eq!(event_map::halloween_npc_name(3), 0xD023);
        assert_eq!(event_map::HNY_POSITIONS[7], (11, 6));
        assert_eq!(event_map::HNY_NPC_MASK, 0b00011111);
    }

    #[test]
    fn franklin_presents() {
        assert_eq!(franklin::PRESENT_NUM, 12);
        assert_eq!(franklin::ALL_BITS, 0xFFF);
        assert!(franklin::all_given(0xFFF));
        assert!(!franklin::all_given(0xFFE));
        assert_eq!(franklin::nth_ungiven(0, 0), Some(0));
        assert_eq!(franklin::nth_ungiven(0b111, 0), Some(3));
        assert_eq!(franklin::nth_ungiven(0xFFF, 0), None);
        assert_eq!(franklin::CARPET66, 0x2642);
        assert_eq!(franklin::WALL66, 0x2742);
    }

    #[test]
    fn controller_states() {
        assert_eq!(tokyoso::ACT_NUM, 9);
        assert_eq!(tokyoso::ACT_STRECH, 2); // source typo preserved
        assert_eq!(tokyoso::ACT_IREKAE_2, 8);
        assert_eq!(jingle::WISH_NUM, 4);
        assert_eq!(jingle::WISH_CARPET, 3);
        assert_eq!(event::SONCHO_NAMES.len(), 30);
    }
}
