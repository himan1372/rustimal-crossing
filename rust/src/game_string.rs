//! String/number formatting core (`m_string.c`, `mFont_UnintToString` and
//! `mMsg_CutLeftSpace` from `m_font.c`, `mMsg_Get_Length_String`), town
//! name helpers (`m_land.c`) and the e-Reader helpers (`m_eappli.c`).
//!
//! Source-verified against ac-decomp `src/game/m_string.c`,
//! `src/game/m_land.c`, `src/game/m_eappli.c` (primary) and ACGC-PC-Port
//! (identical). Font/character tables already live in `npc_reply.rs`;
//! this module only adds the formatting routines.
//!
//! ## Strings ROM (`mString_Load_StringFromRom`)
//! Strings live in ARAM (`RESOURCE_STRING` + `RESOURCE_STRING_TABLE`, up to
//! 0x7FF entries). A zero-size entry fills the destination with spaces; a
//! zero address leaves it untouched; otherwise the bytes are copied
//! (truncated to the destination) and right-padded with spaces. Indexes:
//! AM 0x001, PM 0x002, weekdays from 0x009, days from 0x64E, months from
//! 0x66D, "village" suffix 484 (`mLd_AddMuraString`).
//!
//! ## Number formatting (`mFont_UnintToString`)
//! Right-aligned into `figure` cells, `figure_start` significant digits;
//! leading cells are spaces (or '0' when `fill_zero` and not `left_cut`);
//! zero prints a single '0'. `separator` inserts commas every 3 digits by
//! shifting the string left (dropping the first cell). `left_cut` moves
//! the text to the start and returns its length.
//!
//! ## Clock strings
//! Hour: 0 and 12 print "12", others `h % 12`, then " AM"/" PM"; out of
//! range values clamp to 0 / Jan / Sunday / day 1 / year 2000.
//!
//! ## Land (`m_land.c`)
//! Town ids are `0x30xx` (`mLd_BITMASK`); `mLd_MakeLandId` = random low
//! byte | 0x3000. A name of 8 spaces is "null". Two towns compare equal
//! only when ids match and both names are non-null and byte-equal.
//! Player numbers >= 4 are foreigners (`mLd_PlayerManKindCheckNo`).
//!
//! ## e-Reader (`m_eappli.c`)
//! - CRC32 (poly 0xEDB88320, reflected) but seeded with **0xAA478422**
//!   and no final xor; applied over the e-Reader program (`my_original.bin`
//!   minus 4 x 16 x 32 bytes of design data) from byte 4, stored LE at 0;
//!   the payload size minus 0x34 is stored LE at 0x2C.
//! - Card letter flags: one bit per e-card; reset when the saved
//!   send-date differs from today. `mEA_CheckLetterCardE` returns **2**
//!   instead of 1 for the last card index (and clamps out-of-range card
//!   numbers onto that last index).

pub const CHAR_SPACE: u8 = 32;
pub const CHAR_COMMA: u8 = 44;
pub const CHAR_ZERO: u8 = 48;

pub const STRING_MAX: i32 = 0x7FF;
pub const STRING_AM: i32 = 0x001;
pub const STRING_PM: i32 = 0x002;
pub const STRING_WEEKDAY_START: i32 = 0x009;
pub const STRING_DAY_START: i32 = 0x64E;
pub const STRING_MONTH_START: i32 = 0x66D;
pub const STRING_MURA: i32 = 484;

pub const LAND_NAME_SIZE: usize = 8;
pub const LAND_NAME_MURA_SIZE: usize = LAND_NAME_SIZE + 2;
pub const LAND_BITMASK: u16 = 0x3000;
pub const PLAYER_NUM: u8 = 4;

/// `mMsg_Get_Length_String`: length without trailing spaces.
pub fn length_string(s: &[u8]) -> usize {
    s.iter().rposition(|&c| c != CHAR_SPACE).map_or(0, |i| i + 1)
}

/// `mMsg_CutLeftSpace`: move text left; returns the new length.
pub fn cut_left_space(s: &mut [u8]) -> usize {
    let len = s.len();
    let Some(i) = s.iter().position(|&c| c != CHAR_SPACE) else { return 0 };
    if i == 0 {
        return len;
    }
    s.copy_within(i.., 0);
    for c in &mut s[len - i..] {
        *c = CHAR_SPACE;
    }
    len - i
}

/// `mFont_UnintToString` into `s` (`s.len()` = `figure`).
pub fn unint_to_string(
    s: &mut [u8],
    num: u32,
    figure_start: usize,
    left_cut: bool,
    fill_zero: bool,
    separator: bool,
) -> usize {
    let figure = s.len();
    let l = figure as i32 - figure_start as i32;
    let fill = if fill_zero { CHAR_ZERO } else { CHAR_SPACE };
    let left_fill = if fill_zero && !left_cut { CHAR_ZERO } else { CHAR_SPACE };
    let mut cur = num;
    let mut last = figure as i32 - 1;
    while last >= l {
        let digit = (cur % 10) as u8;
        if last >= 0 {
            s[last as usize] = if cur == 0 {
                if num == 0 && last == figure as i32 - 1 {
                    CHAR_ZERO
                } else {
                    fill
                }
            } else {
                CHAR_ZERO + digit
            };
        }
        cur /= 10;
        last -= 1;
    }
    let mut i = last;
    while i >= 0 {
        s[i as usize] = left_fill;
        i -= 1;
    }
    if separator {
        let mut count = 0;
        let mut i = figure as i32 - 1;
        while i >= 0 {
            if s[i as usize].is_ascii_digit() {
                count += 1;
                if count > 3 {
                    let iu = i as usize;
                    s.copy_within(1..=iu, 0);
                    s[iu] = CHAR_COMMA;
                    count = 0;
                }
            }
            i -= 1;
        }
    }
    if left_cut {
        cut_left_space(s)
    } else {
        figure
    }
}

/// One `mString` ROM entry as seen by `mString_Load_StringFromRom`.
#[derive(Clone, Copy, Debug)]
pub enum RomString<'a> {
    /// `string_size == 0`: fill with spaces.
    Empty,
    /// `string_data_addr == 0`: leave `dst` untouched.
    Missing,
    Data(&'a [u8]),
}

/// `mString_Load_StringFromRom`.
pub fn load_string(dst: &mut [u8], entry: RomString<'_>) {
    match entry {
        RomString::Empty => dst.fill(CHAR_SPACE),
        RomString::Missing => {}
        RomString::Data(src) => {
            let n = src.len().min(dst.len());
            dst[..n].copy_from_slice(&src[..n]);
            dst[n..].fill(CHAR_SPACE);
        }
    }
}

/// 12-hour display value (`mString_Load_HourStringFromRom{,2}`).
pub fn hour_12(hour: u8) -> (u32, bool) {
    let h = if hour > 23 { 0 } else { hour };
    let shown = match h {
        0 | 12 => 12,
        _ => (h % 12) as u32,
    };
    (shown, h < 12)
}

/// `mString_Load_HourStringFromRom`: "<h> AM" / "<h> PM" (15 cells).
pub fn hour_string(hour: u8, am: &[u8], pm: &[u8]) -> ([u8; 15], usize) {
    let (h, is_am) = hour_12(hour);
    let mut buf = [CHAR_SPACE; 15];
    let mut n = unint_to_string(&mut buf, h, 15, true, false, false);
    buf[n] = CHAR_SPACE;
    n += 1;
    load_string(&mut buf[n..], RomString::Data(if is_am { am } else { pm }));
    let len = length_string(&buf);
    (buf, len)
}

/// `mString_Load_MinStringFromRom`: two zero-filled digits.
pub fn min_string(min: u8) -> ([u8; 4], usize) {
    let mut buf = [CHAR_SPACE; 4];
    let n = unint_to_string(&mut buf, if min > 59 { 0 } else { min } as u32, 2, true, true, true);
    (buf, n)
}

/// ROM indexes used by the date loaders (with the C clamps).
pub fn month_string_no(month: u8) -> i32 {
    let m = if (1..=12).contains(&month) { month } else { 1 };
    STRING_MONTH_START + m as i32 - 1
}
pub fn week_string_no(weekday: u8) -> i32 {
    STRING_WEEKDAY_START + if weekday > 6 { 0 } else { weekday as i32 }
}
pub fn day_string_no(day: u8) -> i32 {
    STRING_DAY_START + if (1..=31).contains(&day) { day as i32 } else { 1 } - 1
}

// ---------------------------------------------------------------------------
// Land
// ---------------------------------------------------------------------------

/// `mLd_NullCheckLandName`.
pub fn land_name_is_null(name: &[u8; LAND_NAME_SIZE]) -> bool {
    name.iter().all(|&c| c == CHAR_SPACE)
}

/// `mLd_CheckId`.
pub fn land_id_valid(id: u16) -> bool {
    id & LAND_BITMASK == LAND_BITMASK
}

/// `mLd_CheckCmpLandName`.
pub fn land_names_equal(a: &[u8; LAND_NAME_SIZE], b: &[u8; LAND_NAME_SIZE]) -> bool {
    !land_name_is_null(a) && !land_name_is_null(b) && a == b
}

/// `mLd_CheckCmpLand`.
pub fn lands_equal(a: &[u8; LAND_NAME_SIZE], id_a: u16, b: &[u8; LAND_NAME_SIZE], id_b: u16) -> bool {
    id_a == id_b && land_names_equal(a, b)
}

/// `mLd_MakeLandId` from `fqrand()` in [0, 1).
pub fn make_land_id(r: f32) -> u16 {
    ((r * 256.0) as i32 as u16) | LAND_BITMASK
}

/// `mLd_PlayerManKindCheckNo`.
pub fn player_is_foreigner(player_no: u8) -> bool {
    player_no >= PLAYER_NUM
}

/// `mLd_GetLandNameStringAddMura`: "<name><suffix>" padded to 10 cells.
pub fn land_name_with_suffix(name: &[u8; LAND_NAME_SIZE], suffix: &[u8]) -> ([u8; LAND_NAME_MURA_SIZE], usize) {
    let mut buf = [CHAR_SPACE; LAND_NAME_MURA_SIZE];
    let n = length_string(name);
    buf[..n].copy_from_slice(&name[..n]);
    let mut sfx = [CHAR_SPACE; 16];
    load_string(&mut sfx, RomString::Data(suffix));
    let sl = length_string(&sfx).min(LAND_NAME_MURA_SIZE - n);
    buf[n..n + sl].copy_from_slice(&sfx[..sl]);
    (buf, n + sl)
}

// ---------------------------------------------------------------------------
// e-Reader
// ---------------------------------------------------------------------------

pub const EAPPLI_CRC_POLY: u32 = 0xEDB8_8320;
pub const EAPPLI_CRC_START: u32 = 0xAA47_8422;
/// Design data at the front of `my_original.bin` (`4 * 16 * 32`).
pub const MY_ORIGINAL_DESIGN_DATA_SIZE: usize = 4 * (32 / 2) * 32;

/// `mEA_GetCRC` (`crcgen` + `getcrc`).
pub fn eappli_crc(data: &[u8]) -> u32 {
    let mut table = [0u32; 256];
    for (i, t) in table.iter_mut().enumerate() {
        let mut c = i as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { (c >> 1) ^ EAPPLI_CRC_POLY } else { c >> 1 };
        }
        *t = c;
    }
    let mut crc = EAPPLI_CRC_START;
    for &b in data {
        crc = (crc >> 8) ^ table[((crc ^ b as u32) & 0xFF) as usize];
    }
    crc
}

/// `mEA_GetCardDLProgram` header fix-up on the program bytes.
pub fn eappli_patch_program(program: &mut [u8]) {
    if program.len() < 0x34 {
        return;
    }
    let size = (program.len() - 0x34) as u32;
    program[0x2C..0x30].copy_from_slice(&size.to_le_bytes());
    let crc = eappli_crc(&program[4..]);
    program[0..4].copy_from_slice(&crc.to_le_bytes());
}

/// `mEA_CheckLetterCardE`: 0/1, or 2 for the last card when set.
pub fn letter_card_check(sent: &[u8], card_no: usize, ecard_num: usize) -> u8 {
    let idx = card_no.min(ecard_num - 1);
    let bit = sent.get(idx >> 3).map_or(0, |b| (b >> (idx & 7)) & 1);
    if idx == ecard_num - 1 {
        bit * 2
    } else {
        bit
    }
}

/// `mEA_SetLetterCardE` bit part.
pub fn letter_card_set(sent: &mut [u8], card_no: usize, ecard_num: usize) {
    let idx = card_no.min(ecard_num - 1);
    if let Some(b) = sent.get_mut(idx >> 3) {
        *b |= 1 << (idx & 7);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fmt(fig: usize, num: u32, start: usize, cut: bool, zero: bool, sep: bool) -> (String, usize) {
        let mut b = vec![b'?'; fig];
        let n = unint_to_string(&mut b, num, start, cut, zero, sep);
        (String::from_utf8(b).unwrap(), n)
    }

    #[test]
    fn number_formatting() {
        assert_eq!(fmt(4, 7, 4, false, false, false), ("   7".into(), 4));
        assert_eq!(fmt(4, 7, 2, true, true, true), ("07  ".into(), 2));
        assert_eq!(fmt(4, 0, 4, true, false, false), ("0   ".into(), 1));
        assert_eq!(fmt(7, 12345, 6, true, false, true), ("12,345 ".into(), 6));
        assert_eq!(fmt(2, 105, 2, true, false, false), ("05".into(), 2));
    }

    #[test]
    fn clock_strings() {
        let (b, n) = hour_string(0, b"AM", b"PM");
        assert_eq!(&b[..n], b"12 AM");
        let (b, n) = hour_string(15, b"AM", b"PM");
        assert_eq!(&b[..n], b"3 PM");
        let (b, n) = min_string(5);
        assert_eq!(&b[..n], b"05");
        assert_eq!(month_string_no(13), STRING_MONTH_START);
    }

    #[test]
    fn land_rules() {
        assert!(land_name_is_null(b"        "));
        assert!(!land_names_equal(b"        ", b"        "));
        assert!(lands_equal(b"Tarra   ", 0x3012, b"Tarra   ", 0x3012));
        assert_eq!(make_land_id(0.5), 0x3080);
        assert!(player_is_foreigner(4));
        let (b, n) = land_name_with_suffix(b"Tarra   ", b"ville");
        assert_eq!(&b[..n], b"Tarraville");
    }

    #[test]
    fn eappli_crc_and_cards() {
        // Reflected CRC32 with a custom seed and no final xor.
        assert_eq!(eappli_crc(&[]), EAPPLI_CRC_START);
        let mut sent = [0u8; 2];
        letter_card_set(&mut sent, 3, 10);
        assert_eq!(letter_card_check(&sent, 3, 10), 1);
        letter_card_set(&mut sent, 99, 10);
        assert_eq!(letter_card_check(&sent, 9, 10), 2);
    }
}
