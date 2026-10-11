//! Birthday entry submenu ("When's your birthday?").
//!
//! Source-verified against ac-decomp `src/game/m_birthday_ovl.c` +
//! `include/m_birthday_ovl.h` (primary) and the ACGC-PC-Port copy
//! (identical, 0-line diff), plus `lbRTC_IsValidTime` from `src/lb_rtc.c`.
//!
//! ## Flow
//! - `mBR_birthday_ovl_construct` points `overlay->birthday_ovl` at a
//!   static `mBR_Ovl_c` (cleared once), then `mBR_birthday_ovl_init`
//!   slides the window in from the top, sets the picker to
//!   January 1 and plays SE `0x17C`.
//! - `mBR_move_Play` handles input (cursor `idx` = 0 month, 1 day, 2 OK):
//!   * B / C-left: step the cursor back (SE `0x1003`), no-op at month.
//!   * START: close immediately, saving the date.
//!   * A / C-right: on OK, A closes (C-right does nothing); elsewhere
//!     advance the cursor (`NA_SE_SENTAKU_KETTEI`).
//!   * C-up / C-down: +1 / -1 on the selected field, then re-validate
//!     against a fixed **leap year (2000)** so Feb 29 is always allowed.
//!     Invalid results are repaired:
//!       - month changed to a valid month (1-12), or day decremented
//!         below 1 -> clamp day to the last valid day of the month;
//!       - otherwise C-up wraps the field to 1 (day past month end,
//!         month 13 -> 1);
//!       - otherwise (C-down on month 1 -> 0) month wraps to December.
//!
//!     Plays `NA_SE_CURSOL`; the whole branch is skipped when the
//!     cursor is on OK.
//! - `mBR_window_close` slides out, writes `now_private->birthday`
//!   (month, day) and plays `NA_SE_MENU_EXIT`.
//!
//! ## Draw details (kept in C)
//! - Highlight colors: selected = `{195, 0, 0}` (red), unselected =
//!   `{70, 145, 225}` (blue); `color_type[idx != field]`.
//! - Month is drawn from `tim_win_<month>_tex_rgb_i4` via segment 8
//!   (`month_tex[month - 1]`); the day is a 2-figure, space-filled,
//!   right-aligned number; strings are placed at `x + 119, -y + 88`
//!   (prompt, 0.875 scale), `x + 171, -y + 124` (day),
//!   `x + 198, -y + 124` ("OK").
//!
//! ## Quirk preserved
//! `birthday_data` is `u16` but `lbRTC_time_c.day/month` are `u8`, so the
//! validity probe sees the truncated low byte. Unreachable through the
//! repair rules above (values never leave 0..=32 / 0..=13), but kept.

use std::ffi::c_int;

/// `BUTTON_*` masks from `include/types.h`.
pub const BUTTON_CRIGHT: u32 = 0x0001;
pub const BUTTON_CLEFT: u32 = 0x0002;
pub const BUTTON_CDOWN: u32 = 0x0004;
pub const BUTTON_CUP: u32 = 0x0008;
pub const BUTTON_START: u32 = 0x1000;
pub const BUTTON_B: u32 = 0x4000;
pub const BUTTON_A: u32 = 0x8000;

/// Sound effects played by the overlay (`audio_defs.h` enum values).
pub const SE_CURSOL: u16 = 1; // NA_SE_CURSOL
pub const SE_MENU_EXIT: u16 = 2; // NA_SE_MENU_EXIT
pub const SE_SENTAKU_KETTEI: u16 = 0x0D; // NA_SE_SENTAKU_KETTEI
pub const SE_CURSOR_BACK: u16 = 0x1003;
pub const SE_OPEN: u16 = 0x17C;

/// Cursor positions (`mBR_Ovl_IDX_*`).
pub const IDX_MONTH: i32 = 0;
pub const IDX_DAY: i32 = 1;
pub const IDX_OK: i32 = 2;

/// Year the validity probe uses (`work_time.year = 2000`, a leap year).
pub const PROBE_YEAR: u16 = 2000;

/// Highlight colors (`color_type`): `[selected, unselected]`.
pub const COLOR_TYPE: [[u8; 3]; 2] = [[195, 0, 0], [70, 145, 225]];

/// `lbRTC_YEAR_MIN` / `lbRTC_YEAR_MAX`.
pub const RTC_YEAR_MIN: u16 = 1901;
pub const RTC_YEAR_MAX: u16 = 2099;

/// `mBR_Ovl_c`. `#[repr(C)]` matches `struct birthday_ovl_s`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BirthdayOvl {
    /// `[month, day]`.
    pub birthday_data: [u16; 2],
    pub idx: c_int,
}

/// What the C side must do after one `mBR_move_Play` step.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BirthdayAction {
    /// No input handled.
    None = 0,
    /// Play `sound`; stay open.
    Sound = 1,
    /// `mBR_window_close`: slide out, store month/day, play MENU_EXIT.
    Close = 2,
}

/// Result of one input step.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BirthdayStep {
    pub action: BirthdayAction,
    /// Sound to start (`sAdo_SysTrgStart`), 0 = none.
    pub sound: u16,
}

const fn is_leap_year(year: u16) -> bool {
    (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400)
}

/// `lbRTC_IsValidTime` restricted to the fields the overlay sets
/// (sec/min/hour are 0 in `work_time`).
pub fn is_valid_date(year: u16, month: u8, day: u8) -> bool {
    const DAY_TBL: [u8; 13] = [0, 31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    if !(RTC_YEAR_MIN..=RTC_YEAR_MAX).contains(&year) || month == 0 || month > 12 || day == 0 {
        return false;
    }
    if day == 29 && month == 2 {
        return is_leap_year(year);
    }
    day <= DAY_TBL[month as usize]
}

impl BirthdayOvl {
    /// `mBR_birthday_ovl_construct` + `mBR_birthday_ovl_init`: the
    /// static struct is `mem_clear`ed whenever `overlay->birthday_ovl`
    /// is NULL, which `destruct` guarantees on every reopen, so the
    /// cursor starts on the month; then the date is set to January 1.
    pub fn init(&mut self) {
        self.idx = IDX_MONTH;
        self.birthday_data = [1, 1];
    }

    fn probe_valid(&self) -> bool {
        // work_time.day/month are u8: truncation is part of the original.
        is_valid_date(
            PROBE_YEAR,
            self.birthday_data[IDX_MONTH as usize] as u8,
            self.birthday_data[IDX_DAY as usize] as u8,
        )
    }

    /// `mBR_move_Play` for one frame of `trigger` input.
    pub fn play(&mut self, trigger: u32) -> BirthdayStep {
        let none = BirthdayStep { action: BirthdayAction::None, sound: 0 };
        if trigger & (BUTTON_B | BUTTON_CLEFT) != 0 {
            if self.idx != 0 {
                self.idx -= 1;
                return BirthdayStep { action: BirthdayAction::Sound, sound: SE_CURSOR_BACK };
            }
            return none;
        }
        if trigger & BUTTON_START != 0 {
            return BirthdayStep { action: BirthdayAction::Close, sound: SE_MENU_EXIT };
        }
        if trigger & (BUTTON_A | BUTTON_CRIGHT) != 0 {
            if self.idx == IDX_OK {
                if trigger & BUTTON_CRIGHT == 0 {
                    return BirthdayStep { action: BirthdayAction::Close, sound: SE_MENU_EXIT };
                }
                return none;
            }
            self.idx += 1;
            return BirthdayStep { action: BirthdayAction::Sound, sound: SE_SENTAKU_KETTEI };
        }
        if trigger & (BUTTON_CUP | BUTTON_CDOWN) != 0 {
            if self.idx == IDX_OK || !(0..IDX_OK).contains(&self.idx) {
                return none;
            }
            let i = self.idx as usize;
            let down = trigger & BUTTON_CDOWN != 0;
            if down {
                self.birthday_data[i] = self.birthday_data[i].wrapping_sub(1);
            } else {
                self.birthday_data[i] = self.birthday_data[i].wrapping_add(1);
            }

            if !self.probe_valid() {
                let month = self.birthday_data[IDX_MONTH as usize];
                if (self.idx == IDX_MONTH && month != 0 && month < 13) || (down && self.idx == IDX_DAY) {
                    let mut day: u8 = 31;
                    while !is_valid_date(PROBE_YEAR, month as u8, day) {
                        day = day.wrapping_sub(1);
                        if day == 0 {
                            break; // unreachable for months 1-12; guards the C infinite loop
                        }
                    }
                    self.birthday_data[IDX_DAY as usize] = day as u16;
                } else if trigger & BUTTON_CUP != 0 {
                    self.birthday_data[i] = 1;
                } else {
                    self.birthday_data[IDX_MONTH as usize] = 12; // lbRTC_DECEMBER
                }
            }
            return BirthdayStep { action: BirthdayAction::Sound, sound: SE_CURSOL };
        }
        none
    }

    /// Index into `month_tex` for the month graphic (`month - 1`).
    pub fn month_tex_index(&self) -> usize {
        (self.birthday_data[IDX_MONTH as usize] as usize).wrapping_sub(1)
    }

    /// `color_type[idx != field]`.
    pub fn field_color(&self, field: i32) -> [u8; 3] {
        COLOR_TYPE[(self.idx != field) as usize]
    }
}

// ---------------------------------------------------------------------------
// C ABI (pc_* names only; the C overlay is not rewired)
// ---------------------------------------------------------------------------

/// `mBR_birthday_ovl_init` data reset. Null-safe.
#[no_mangle]
pub extern "C" fn pc_birthday_ovl_init(ovl: *mut BirthdayOvl) {
    if let Some(o) = unsafe { ovl.as_mut() } {
        o.init();
    }
}

/// `mBR_move_Play` input step. Returns `BirthdayAction::None` for null.
#[no_mangle]
pub extern "C" fn pc_birthday_ovl_play(ovl: *mut BirthdayOvl, trigger: u32) -> BirthdayStep {
    match unsafe { ovl.as_mut() } {
        Some(o) => o.play(trigger),
        None => BirthdayStep { action: BirthdayAction::None, sound: 0 },
    }
}

/// `lbRTC_IsValidTime` for a date (time fields assumed zero).
#[no_mangle]
pub extern "C" fn pc_birthday_is_valid_date(year: u16, month: u8, day: u8) -> c_int {
    is_valid_date(year, month, day) as c_int
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ovl(month: u16, day: u16, idx: i32) -> BirthdayOvl {
        BirthdayOvl { birthday_data: [month, day], idx }
    }

    #[test]
    fn month_down_from_january_wraps_to_december() {
        let mut o = ovl(1, 15, IDX_MONTH);
        assert_eq!(o.play(BUTTON_CDOWN).sound, SE_CURSOL);
        assert_eq!(o.birthday_data, [12, 15]);
    }

    #[test]
    fn month_up_from_december_wraps_to_january() {
        let mut o = ovl(12, 31, IDX_MONTH);
        o.play(BUTTON_CUP);
        assert_eq!(o.birthday_data, [1, 31]);
    }

    #[test]
    fn month_change_clamps_day_to_month_end_leap_feb() {
        let mut o = ovl(1, 31, IDX_MONTH);
        o.play(BUTTON_CUP);
        assert_eq!(o.birthday_data, [2, 29]);
        let mut o = ovl(5, 31, IDX_MONTH);
        o.play(BUTTON_CUP);
        assert_eq!(o.birthday_data, [6, 30]);
    }

    #[test]
    fn day_wraps_both_ways() {
        let mut o = ovl(4, 30, IDX_DAY);
        o.play(BUTTON_CUP);
        assert_eq!(o.birthday_data, [4, 1]);
        o.play(BUTTON_CDOWN);
        assert_eq!(o.birthday_data, [4, 30]);
    }

    #[test]
    fn buttons_move_cursor_and_close() {
        let mut o = ovl(3, 3, IDX_MONTH);
        assert_eq!(o.play(BUTTON_B).action, BirthdayAction::None);
        o.play(BUTTON_A);
        o.play(BUTTON_A);
        assert_eq!(o.idx, IDX_OK);
        assert_eq!(o.play(BUTTON_CRIGHT).action, BirthdayAction::None);
        assert_eq!(o.play(BUTTON_A).action, BirthdayAction::Close);
        assert_eq!(o.play(BUTTON_CUP).action, BirthdayAction::None);
        assert_eq!(o.play(BUTTON_START).action, BirthdayAction::Close);
    }
}
