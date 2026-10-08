//! Mother-mail scheduler.
//!
//! Verified against `src/game/m_private.c`, `include/m_private.h`,
//! `include/m_common_data.h`, `src/game/m_start_data_init.c`,
//! `include/m_mail.h` and `src/game/m_event_schedule.c_inc`
//! (GAFE01_00 Rev. 0).
//!
//! Mom's mail is a persistent per-player collection scheduler, not a flat
//! daily chance. Three pools:
//!
//! * Pool A — fixed-date letters (birthday, month==day, April Fools,
//!   Mother's/Father's Day, Toy Day). Stateless; checked first.
//! * Pool B — 56 normal letters (0x12C-0x163), no-repeat via a 56-bit
//!   field. Daily `RANDOM(100) < 20` gate.
//! * Pool C — monthly/seasonal letters (2 per month, 8 in August), entered
//!   only when pool B is exhausted, which clears the whole data block.
//!
//! Retail quirks preserved: no missed-day catch-up (single date != today
//! check); the first-ever startup only stamps the date; normal-mail
//! delivery failure still consumes the day; special-date failure does NOT
//! (same-day retry on next boot); `RANDOM(1)` is kept as a real RNG call.

use crate::mail::{mtype, Mail, EMPTY_NO};

/// Players with independent mother-mail state.
pub const PLAYER_NUM: usize = 4;
/// Normal-letter pool size (`mPr_MOTHER_MAIL_NORMAL_NUM` = 7 bytes).
pub const NORMAL_NUM: usize = 56;
pub const NORMAL_BYTES: usize = 7;
pub const MONTHLY_BYTES: usize = 2;
/// August has 8 monthly variants instead of 2.
pub const AUGUST_VARIANTS: usize = 8;

/// Mail number bases.
pub mod mail_no {
    /// Normal pool: 0x12C + event_no.
    pub const NORMAL_BASE: u16 = 0x12C;
    /// Month==day pool: 0x164 + letter_num (24 variants).
    pub const MONTH_DAY_BASE: u16 = 0x164;
    /// April Fools: 0x180 + RANDOM(2).
    pub const APRIL_FOOLS: u16 = 0x180;
    /// Mother's Day: 0x17C + RANDOM(2).
    pub const MOTHERS_DAY: u16 = 0x17C;
    /// Father's Day: 0x17E + RANDOM(2).
    pub const FATHERS_DAY: u16 = 0x17E;
    /// Toy Day (Dec 24): 0x182 + RANDOM(2).
    pub const TOY_DAY: u16 = 0x182;
    /// Birthday: 0x184 + RANDOM(2).
    pub const BIRTHDAY: u16 = 0x184;
}

/// Seasonal monthly mail: `mail_start_no_table[mTM_SEASON_NUM]`.
pub const MAIL_START_NO_TABLE: [u16; 4] = [0x18C, 0x192, 0x186, 0x19E];

/// Present item ids.
pub mod present {
    pub const MONEY_1000: u16 = 0x2100;
    pub const MONEY_10000: u16 = 0x2101;
    pub const CLOTH105: u16 = 0x2400 + 105; // fortune shirt
    pub const CLOTH108: u16 = 0x2400 + 108; // aurora knit
    pub const CLOTH109: u16 = 0x2400 + 109; // winter sweater
    pub const CLOTH110: u16 = 0x2400 + 110; // go-go shirt
    pub const CLOTH144: u16 = 0x2400 + 144; // deer shirt
    pub const CLOTH145: u16 = 0x2400 + 145; // blue check shirt
    pub const CLOTH156: u16 = 0x2400 + 156; // fish knit
    pub const FOOD_APPLE: u16 = 0x2800;
    pub const FOOD_MUSHROOM: u16 = 0x2805;
    /// December monthly second-variant shirt table.
    pub const DECEMBER_SHIRTS: [u16; 6] = [
        CLOTH108, CLOTH109, CLOTH110, CLOTH144, CLOTH145, CLOTH156,
    ];
}

/// Months (lbRTC_*).
pub mod month {
    pub const JANUARY: u8 = 1;
    pub const FEBRUARY: u8 = 2;
    pub const APRIL: u8 = 4;
    pub const MAY: u8 = 5;
    pub const AUGUST: u8 = 8;
    pub const NOVEMBER: u8 = 11;
    pub const DECEMBER: u8 = 12;
}

/// Paper table: `paper_table[month - 1]`, result used as `paper - 1`.
pub const PAPER_TABLE: [u8; 12] = [13, 49, 32, 12, 62, 14, 19, 11, 59, 46, 47, 17];

/// Y/M/D stamp. The clear sentinel is FFFF/FF/FF.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(C)]
pub struct RtcYmd {
    pub year: u16,
    pub month: u8,
    pub day: u8,
}

pub const CLEAR_DATE: RtcYmd = RtcYmd { year: 0xFFFF, month: 0xFF, day: 0xFF };

/// `mPr_mother_mail_data_c`: the 10-byte persistent bitfield block.
#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct MotherMailData {
    /// 56 normal letters, bit i = letter i sent.
    pub normal: [u8; NORMAL_BYTES],
    /// 2 bits per month (Jan-Jul, Sep-Dec packed).
    pub monthly: [u8; MONTHLY_BYTES],
    /// August's 8 variants get their own byte.
    pub august: u8,
}

/// `mPr_mother_mail_info_c`: 14 bytes per player, in Save_t (not Private_c).
#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct MotherMailInfo {
    pub date: RtcYmd,
    pub data: MotherMailData,
}

/// Per-player save array: `mother_mail[PLAYER_NUM]`.
pub type MotherMailSave = [MotherMailInfo; PLAYER_NUM];

/// mPr_ClearMotherMailInfo.
pub fn clear_mother_mail_info(info: &mut MotherMailInfo) {
    info.date = CLEAR_DATE;
    info.data = MotherMailData::default();
}

/// Present selection. Engine-resolved variants (random clothing/umbrella,
/// random furniture, other-fruit) stay symbolic: retail calls
/// `mSP_SelectRandomItem_New` / `mSP_RandomUmbSelect` / `mFI_GetOtherFruit`
/// at selection time, and their RNG consumption belongs to the engine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresentSpec {
    None,
    Item(u16),
    RandomClothing,   // mSP_KIND_CLOTH, mSP_LISTTYPE_ABC
    RandomUmbrella,   // mSP_RandomUmbSelect
    RandomFurniture,  // mSP_KIND_FURNITURE, mSP_LISTTYPE_ABC (Toy Day)
    OtherFruit,       // mFI_GetOtherFruit
    BirthdayCake,     // FTR_START(FTR_SUM_BDCAKE01)
    Doll,             // FTR_START(FTR_SUM_DOLL02)
    Dracaena,         // FTR_START(FTR_SUM_PL_DRACAENA)
}

impl PresentSpec {
    pub fn item_no(self) -> u16 {
        match self {
            PresentSpec::Item(n) => n,
            _ => EMPTY_NO,
        }
    }
}

/// Where a mother letter landed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Delivery {
    HouseMailbox,
    PostOffice,
    Failed,
}

/// A composed mother letter (text itself loads from the ROM handbill).
#[derive(Clone, Copy, Debug)]
pub struct MotherMail {
    pub mail_no: u16,
    pub present: PresentSpec,
    /// Stationery index (`paper - 1`).
    pub paper: u8,
    pub delivery: Delivery,
}

/// Engine-owned facts for one scheduler step.
#[derive(Clone, Copy, Debug)]
pub struct MotherMailCtx {
    /// `mLd_PlayerManKindCheckNo(player_no) == FALSE`
    pub is_local_player: bool,
    pub has_private: bool,
    pub has_player_id: bool,
    pub birthday: (u8, u8), // (month, day)
    pub mothers_day_active: bool,
    pub fathers_day_active: bool,
    /// House mailbox has a free slot AND pid matches the house owner.
    pub house_mailbox_has_room: bool,
    /// `mPO_get_keep_mail_sum() < mPO_MAIL_STORAGE_SIZE`
    pub post_office_has_room: bool,
}

/// Outcome of one scheduler step.
#[derive(Clone, Debug)]
pub enum StepOutcome {
    /// Entry gates failed (foreigner / no private / no player id).
    Inactive,
    /// Clear sentinel: stamped today's date, no mail (first ever startup).
    DateStamped,
    /// `date == today`: nothing to do.
    AlreadyProcessed,
    /// Special-date letter. `date_updated` is true only if delivery
    /// succeeded; on failure the same date retries next boot and the
    /// normal path is NOT attempted.
    Special(MotherMail, bool),
    /// Normal path. The scheduler date is ALWAYS updated, even when the
    /// 20% roll fails or delivery fails. `None` = no letter (roll failed).
    Normal(Option<MotherMail>),
}

/// mPr_CheckMotherMailNormal / mPr_SetMotherMailNormal.
pub fn check_normal(data: &MotherMailData, idx: usize) -> bool {
    let slot = idx / 8;
    let bit = idx - slot * 8;
    (data.normal[slot] >> bit) & 1 == 1
}

pub fn set_normal(data: &mut MotherMailData, idx: usize) {
    let slot = idx / 8;
    let bit = idx - slot * 8;
    data.normal[slot] |= 1 << bit;
}

/// mPr_GetMotherMailNormalNotSendNum.
pub fn normal_not_send_num(data: &MotherMailData) -> usize {
    (0..NORMAL_NUM).filter(|&i| !check_normal(data, i)).count()
}

/// mPr_CheckMotherMailMonthly / mPr_SetMotherMailMonthly.
pub fn check_monthly(data: &MotherMailData, month: u8, idx: usize) -> bool {
    if month == month::AUGUST {
        (data.august >> idx) & 1 == 1
    } else {
        let shift = (month - 1) as usize * 2;
        let slot = shift / 8;
        let bit = shift - slot * 8 + idx;
        (data.monthly[slot] >> bit) & 1 == 1
    }
}

pub fn set_monthly(data: &mut MotherMailData, month: u8, idx: usize) {
    if month == month::AUGUST {
        data.august |= 1 << idx;
    } else {
        let shift = (month - 1) as usize * 2;
        let slot = shift / 8;
        let bit = shift - slot * 8 + idx;
        data.monthly[slot] |= 1 << bit;
    }
}

/// mPr_GetMotherMailMonthlyNotSendNum.
pub fn monthly_not_send_num(data: &MotherMailData, month: u8) -> usize {
    let max = if month == month::AUGUST { AUGUST_VARIANTS } else { 2 };
    (0..max).filter(|&i| !check_monthly(data, month, i)).count()
}

/// Uniform selection among unsent indices (retail's count + RANDOM + scan).
fn select_unsent(max: usize, is_sent: &dyn Fn(usize) -> bool, rng: &mut dyn FnMut(u32) -> u32) -> usize {
    let mut selected = rng(max as u32 - (0..max).filter(|&i| is_sent(i)).count() as u32) as usize;
    for i in 0..max {
        if !is_sent(i) {
            if selected == 0 {
                return i;
            }
            selected -= 1;
        }
    }
    0 // unreachable when at least one is unsent
}

/// mPr_GetMotherMailPaperType.
pub fn mother_mail_paper(month: u8, day: u8, birthday: (u8, u8)) -> u8 {
    let paper = if birthday.0 == month && birthday.1 == day {
        1
    } else if month == month::JANUARY && day == 1 {
        63
    } else if month == month::AUGUST && day == 8 {
        48
    } else if month == month::DECEMBER && day == 24 {
        23
    } else {
        PAPER_TABLE[(month - 1) as usize]
    };
    paper - 1
}

/// mPr_GetMotherMailNormalData: pick the normal letter and its present.
/// `rng` supplies RANDOM(no_send_num); engine RNG inside the present
/// resolvers is the engine's business.
pub fn normal_letter_data(
    data: &MotherMailData,
    rng: &mut dyn FnMut(u32) -> u32,
) -> (u16, PresentSpec, usize) {
    let event_no = select_unsent(NORMAL_NUM, &|i| check_normal(data, i), rng);
    let mail_no = mail_no::NORMAL_BASE + event_no as u16;
    let present = match event_no {
        1 | 16 => PresentSpec::RandomClothing,
        3 | 21 | 22 | 47 => PresentSpec::OtherFruit,
        12 => PresentSpec::Item(present::MONEY_1000),
        37 => PresentSpec::Doll,
        38 => PresentSpec::Dracaena,
        40 => PresentSpec::RandomUmbrella,
        _ => PresentSpec::None,
    };
    (mail_no, present, event_no)
}

/// mPr_GetMotherMailMonthlyData: pick the seasonal letter and its present.
/// `rng` supplies RANDOM(not_send_num) then any present-table rolls.
pub fn monthly_letter_data(
    data: &MotherMailData,
    month: u8,
    rng: &mut dyn FnMut(u32) -> u32,
) -> (u16, PresentSpec, usize) {
    let mail_start_idx = if month <= month::FEBRUARY {
        3
    } else if month <= month::MAY {
        0
    } else if month <= month::AUGUST {
        1
    } else if month <= month::NOVEMBER {
        2
    } else {
        3
    };
    let max = if month == month::AUGUST { AUGUST_VARIANTS } else { 2 };
    let event_no = select_unsent(max, &|i| check_monthly(data, month, i), rng);
    let mail_no =
        MAIL_START_NO_TABLE[mail_start_idx] + event_no as u16 + (month as u16 - 1 - mail_start_idx as u16 * 3) * 2;
    let present = if month == month::MAY && event_no == 1 {
        let _ = rng(1); // RANDOM(1): kept, retail consumes RNG even for one entry
        PresentSpec::Item(present::CLOTH105)
    } else if month == month::DECEMBER {
        if event_no == 0 {
            PresentSpec::Item(present::FOOD_APPLE)
        } else {
            PresentSpec::Item(present::DECEMBER_SHIRTS[rng(6) as usize])
        }
    } else if month == month::NOVEMBER {
        PresentSpec::Item(present::FOOD_MUSHROOM)
    } else {
        PresentSpec::None
    };
    (mail_no, present, event_no)
}

/// mPr_SendMotherMailPost: house mailbox first, post-office fallback.
/// Returns where the mail landed (mail composition itself is the caller's).
fn send_post(ctx: &MotherMailCtx) -> Delivery {
    if ctx.house_mailbox_has_room {
        Delivery::HouseMailbox
    } else if ctx.post_office_has_room {
        Delivery::PostOffice
    } else {
        Delivery::Failed
    }
}

/// mPr_SendMotherMailDate: special-date letters. Returns the outcome;
/// the normal path is never attempted afterwards.
fn send_date(
    info: &mut MotherMailInfo,
    today: &RtcYmd,
    ctx: &MotherMailCtx,
    rng: &mut dyn FnMut(u32) -> u32,
) -> Option<StepOutcome> {
    let mut mail_no: i32 = -1;
    let mut present = PresentSpec::None;

    if ctx.birthday.0 == today.month && ctx.birthday.1 == today.day {
        mail_no = mail_no::BIRTHDAY as i32 + rng(2) as i32;
        present = PresentSpec::BirthdayCake;
    } else if today.month == today.day {
        let letter_num = (today.month as usize - 1) * 2 + rng(2) as usize;
        mail_no = mail_no::MONTH_DAY_BASE as i32 + letter_num as i32;
        if today.month == month::JANUARY {
            present = PresentSpec::Item(present::MONEY_10000);
        } else if letter_num == 18 {
            present = PresentSpec::Item(present::FOOD_MUSHROOM);
        }
    } else {
        if today.month == month::APRIL && today.day == 1 {
            mail_no = mail_no::APRIL_FOOLS as i32;
        } else if ctx.mothers_day_active {
            mail_no = mail_no::MOTHERS_DAY as i32;
        } else if ctx.fathers_day_active {
            mail_no = mail_no::FATHERS_DAY as i32;
        } else if today.month == month::DECEMBER && today.day == 24 {
            mail_no = mail_no::TOY_DAY as i32;
            // Present selected BEFORE the RANDOM(2) variant roll (retail order).
            present = PresentSpec::RandomFurniture;
        }
        if mail_no != -1 {
            mail_no += rng(2) as i32;
        }
    }

    if mail_no == -1 {
        return None;
    }
    let delivery = send_post(ctx);
    let date_updated = delivery != Delivery::Failed;
    if date_updated {
        info.date = *today;
    }
    Some(StepOutcome::Special(
        MotherMail {
            mail_no: mail_no as u16,
            present,
            paper: mother_mail_paper(today.month, today.day, ctx.birthday),
            delivery,
        },
        date_updated,
    ))
}

/// mPr_SendMotherMailNormal.
fn send_normal(
    info: &mut MotherMailInfo,
    today: &RtcYmd,
    ctx: &MotherMailCtx,
    rng: &mut dyn FnMut(u32) -> u32,
) -> StepOutcome {
    let mut out = None;
    if rng(100) < 20 {
        let not_send_num = normal_not_send_num(&info.data);
        let mut monthly_num = 0;
        if not_send_num == 0 {
            // Pool B exhausted: clear the ENTIRE data block (normal +
            // monthly + august), then consider this month's pool C.
            info.data = MotherMailData::default();
            monthly_num = monthly_not_send_num(&info.data, today.month);
        }
        if monthly_num > 0 {
            let (mail_no, present, event_no) = monthly_letter_data(&info.data, today.month, rng);
            let delivery = send_post(ctx);
            if delivery != Delivery::Failed {
                set_monthly(&mut info.data, today.month, event_no);
            }
            out = Some(MotherMail {
                mail_no,
                present,
                paper: mother_mail_paper(today.month, today.day, ctx.birthday),
                delivery,
            });
        } else {
            let (mail_no, present, event_no) = normal_letter_data(&info.data, rng);
            let delivery = send_post(ctx);
            if delivery != Delivery::Failed {
                set_normal(&mut info.data, event_no);
            }
            out = Some(MotherMail {
                mail_no,
                present,
                paper: mother_mail_paper(today.month, today.day, ctx.birthday),
                delivery,
            });
        }
    }
    // The date is updated even when the roll failed or delivery failed:
    // no same-letter retry tomorrow.
    info.date = *today;
    StepOutcome::Normal(out)
}

/// mPr_SendMailFromMother: one scheduler step for the current player.
/// `rng(n)` must return a value in [0, n), matching RANDOM's semantics.
pub fn send_mail_from_mother(
    info: &mut MotherMailInfo,
    today: &RtcYmd,
    ctx: &MotherMailCtx,
    rng: &mut dyn FnMut(u32) -> u32,
) -> StepOutcome {
    if !ctx.is_local_player || !ctx.has_private || !ctx.has_player_id {
        return StepOutcome::Inactive;
    }
    if info.date == CLEAR_DATE {
        // First-ever startup: stamp the date, send nothing.
        info.date = *today;
        return StepOutcome::DateStamped;
    }
    if info.date == *today {
        return StepOutcome::AlreadyProcessed;
    }
    // No catch-up: process only the current day, once.
    if let Some(special) = send_date(info, today, ctx, rng) {
        return special;
    }
    send_normal(info, today, ctx, rng)
}

/// mPr_GetMotherMail: fill a `Mail` from the composed letter. The ROM
/// handbill text load is engine-side; font/type/name/present/paper are set
/// here (font = 0, mail_type = mMl_TYPE_MOTHER = 4).
pub fn build_mail(mail: &mut Mail, composed: &MotherMail) {
    mail.clear();
    mail.content.font = 0; // retail sets font = 0 (TODO: enum in decomp)
    mail.content.mail_type = mtype::MOTHER;
    mail.present = composed.present.item_no();
    mail.content.paper_type = composed.paper;
    // Retail: mHandbill_Load_HandbillFromRom(..., mail_no) then
    // mMl_set_to_plname(mail, pid) — the engine completes those.
}

// ---------------------------------------------------------------------------
// C ABI
// ---------------------------------------------------------------------------

/// C ABI: run one scheduler step. Returns a packed outcome code; see
/// `pc_mother_mail_last` for the composed letter details.
#[no_mangle]
pub extern "C" fn pc_mother_mail_paper(month: u8, day: u8, bday_month: u8, bday_day: u8) -> i32 {
    mother_mail_paper(month, day, (bday_month, bday_day)) as i32
}

/// C ABI: normal-pool unsent count.
#[no_mangle]
pub extern "C" fn pc_mother_mail_normal_not_send(num: *const u8) -> i32 {
    let data = unsafe { &*(num as *const MotherMailData) };
    normal_not_send_num(data) as i32
}

/// C ABI: monthly-pool unsent count for `month`.
#[no_mangle]
pub extern "C" fn pc_mother_mail_monthly_not_send(num: *const u8, month: u8) -> i32 {
    let data = unsafe { &*(num as *const MotherMailData) };
    monthly_not_send_num(data, month) as i32
}

/// C ABI: seasonal monthly mail number for month + event index.
#[no_mangle]
pub extern "C" fn pc_mother_mail_monthly_no(month: u8, event_no: u8) -> i32 {
    let idx = if month <= month::FEBRUARY {
        3
    } else if month <= month::MAY {
        0
    } else if month <= month::AUGUST {
        1
    } else if month <= month::NOVEMBER {
        2
    } else {
        3
    };
    (MAIL_START_NO_TABLE[idx] + event_no as u16 + (month as u16 - 1 - idx as u16 * 3) * 2) as i32
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> MotherMailCtx {
        MotherMailCtx {
            is_local_player: true,
            has_private: true,
            has_player_id: true,
            birthday: (6, 15),
            mothers_day_active: false,
            fathers_day_active: false,
            house_mailbox_has_room: true,
            post_office_has_room: true,
        }
    }

    /// Deterministic RNG: always returns 0.
    fn rng0() -> impl FnMut(u32) -> u32 {
        |_| 0
    }

    #[test]
    fn first_startup_stamps_date_only() {
        let mut info = MotherMailInfo::default();
        clear_mother_mail_info(&mut info);
        let today = RtcYmd { year: 2026, month: 10, day: 8 };
        let mut r = rng0();
        let c = ctx();
        match send_mail_from_mother(&mut info, &today, &c, &mut r) {
            StepOutcome::DateStamped => {}
            o => panic!("unexpected {:?}", o),
        }
        assert_eq!(info.date, today);
    }

    #[test]
    fn same_day_noop() {
        let today = RtcYmd { year: 2026, month: 10, day: 8 };
        let mut info = MotherMailInfo { date: today, ..Default::default() };
        let mut r = rng0();
        let c = ctx();
        assert!(matches!(
            send_mail_from_mother(&mut info, &today, &c, &mut r),
            StepOutcome::AlreadyProcessed
        ));
    }

    #[test]
    fn birthday_letter_and_cake() {
        let mut info = MotherMailInfo {
            date: RtcYmd { year: 2026, month: 6, day: 14 },
            ..Default::default()
        };
        let today = RtcYmd { year: 2026, month: 6, day: 15 };
        let mut r = rng0();
        let c = ctx();
        match send_mail_from_mother(&mut info, &today, &c, &mut r) {
            StepOutcome::Special(m, true) => {
                assert_eq!(m.mail_no, 0x184);
                assert_eq!(m.present, PresentSpec::BirthdayCake);
                assert_eq!(m.paper, 0); // birthday paper = 1 - 1
            }
            o => panic!("unexpected {:?}", o),
        }
        assert_eq!(info.date, today);
    }

    #[test]
    fn month_day_letter_jan1_bells() {
        let mut info = MotherMailInfo {
            date: RtcYmd { year: 2026, month: 1, day: 2 },
            ..Default::default()
        };
        let today = RtcYmd { year: 2026, month: 1, day: 1 };
        let mut r = rng0();
        let mut c = ctx();
        c.birthday = (3, 3);
        match send_mail_from_mother(&mut info, &today, &c, &mut r) {
            StepOutcome::Special(m, true) => {
                assert_eq!(m.mail_no, 0x164);
                assert_eq!(m.present, PresentSpec::Item(present::MONEY_10000));
                assert_eq!(m.paper, 62); // 63 - 1
            }
            o => panic!("unexpected {:?}", o),
        }
    }

    #[test]
    fn birthday_beats_month_day() {
        let mut info = MotherMailInfo {
            date: RtcYmd { year: 2026, month: 1, day: 2 },
            ..Default::default()
        };
        let today = RtcYmd { year: 2026, month: 1, day: 1 };
        let mut r = rng0();
        let mut c = ctx();
        c.birthday = (1, 1);
        match send_mail_from_mother(&mut info, &today, &c, &mut r) {
            StepOutcome::Special(m, true) => {
                assert_eq!(m.mail_no, 0x184);
                assert_eq!(m.present, PresentSpec::BirthdayCake);
            }
            o => panic!("unexpected {:?}", o),
        }
    }

    #[test]
    fn normal_pool_no_duplicates() {
        let today = RtcYmd { year: 2026, month: 10, day: 8 };
        let mut info = MotherMailInfo {
            date: RtcYmd { year: 2026, month: 10, day: 7 },
            ..Default::default()
        };
        let mut c = ctx();
        c.birthday = (3, 3);
        // Force the 20% roll to succeed every day.
        let mut seen = [false; NORMAL_NUM];
        let mut day = 8u8;
        for _ in 0..NORMAL_NUM {
            let t = RtcYmd { year: 2026, month: 10, day };
            let mut r = rng0();
            match send_mail_from_mother(&mut info, &t, &c, &mut r) {
                StepOutcome::Normal(Some(m)) => {
                    let idx = (m.mail_no - mail_no::NORMAL_BASE) as usize;
                    assert!(!seen[idx], "duplicate normal letter {}", idx);
                    seen[idx] = true;
                }
                o => panic!("unexpected {:?}", o),
            }
            day += 1;
            info.date = RtcYmd { year: 2026, month: 10, day: day - 1 };
        }
        assert!(seen.iter().all(|&s| s));
    }

    #[test]
    fn exhaustion_clears_all_and_sends_monthly() {
        let mut info = MotherMailInfo {
            date: RtcYmd { year: 2026, month: 10, day: 7 },
            ..Default::default()
        };
        // Mark all 56 normal letters sent.
        for i in 0..NORMAL_NUM {
            set_normal(&mut info.data, i);
        }
        // And mark October's monthly letters sent too, to prove the clear.
        set_monthly(&mut info.data, 10, 0);
        let today = RtcYmd { year: 2026, month: 10, day: 8 };
        let mut r = rng0();
        let c = ctx();
        match send_mail_from_mother(&mut info, &today, &c, &mut r) {
            StepOutcome::Normal(Some(m)) => {
                // October: idx 2 -> 0x186 + 0 + (10-1-6)*2 = 0x18C
                assert_eq!(m.mail_no, 0x18C);
                assert!(check_monthly(&info.data, 10, 0));
                // Whole block was cleared: normal bits are empty again.
                assert_eq!(normal_not_send_num(&info.data), NORMAL_NUM);
            }
            o => panic!("unexpected {:?}", o),
        }
    }

    #[test]
    fn normal_failure_still_consumes_day() {
        let mut info = MotherMailInfo {
            date: RtcYmd { year: 2026, month: 10, day: 7 },
            ..Default::default()
        };
        let today = RtcYmd { year: 2026, month: 10, day: 8 };
        let mut r = rng0(); // 20% roll succeeds
        let mut c = ctx();
        c.house_mailbox_has_room = false;
        c.post_office_has_room = false;
        match send_mail_from_mother(&mut info, &today, &c, &mut r) {
            StepOutcome::Normal(Some(m)) => assert_eq!(m.delivery, Delivery::Failed),
            o => panic!("unexpected {:?}", o),
        }
        assert_eq!(info.date, today); // day consumed anyway
        assert_eq!(normal_not_send_num(&info.data), NORMAL_NUM); // letter NOT marked
    }

    #[test]
    fn special_failure_retries_same_day() {
        let mut info = MotherMailInfo {
            date: RtcYmd { year: 2026, month: 6, day: 14 },
            ..Default::default()
        };
        let today = RtcYmd { year: 2026, month: 6, day: 15 };
        let mut r = rng0();
        let mut c = ctx();
        c.house_mailbox_has_room = false;
        c.post_office_has_room = false;
        match send_mail_from_mother(&mut info, &today, &c, &mut r) {
            StepOutcome::Special(_, false) => {}
            o => panic!("unexpected {:?}", o),
        }
        assert_ne!(info.date, today); // date NOT updated -> retry next boot
    }

    #[test]
    fn monthly_numbering() {
        // October seasonal: idx 2, event 0 -> 0x186 + 0 + (10-1-6)*2 = 0x18C
        assert_eq!(super::pc_mother_mail_monthly_no(10, 0), 0x18C);
        // August: idx 1, event 7 -> 0x192 + 7 + (8-1-3)*2 = 0x19E + 1
        assert_eq!(super::pc_mother_mail_monthly_no(8, 7), 0x19F);
        // January: idx 3, event 0 -> 0x19E + 0 + (1-1-9)*2 = 0x18C
        assert_eq!(super::pc_mother_mail_monthly_no(1, 0), 0x18C);
    }

    #[test]
    fn paper_overrides() {
        assert_eq!(mother_mail_paper(1, 1, (3, 3)), 62);
        assert_eq!(mother_mail_paper(8, 8, (3, 3)), 47);
        assert_eq!(mother_mail_paper(12, 24, (3, 3)), 22);
        assert_eq!(mother_mail_paper(6, 15, (6, 15)), 0);
        assert_eq!(mother_mail_paper(10, 8, (3, 3)), PAPER_TABLE[9] - 1);
    }

    #[test]
    fn inactive_for_foreigner() {
        let mut info = MotherMailInfo::default();
        clear_mother_mail_info(&mut info);
        let today = RtcYmd { year: 2026, month: 10, day: 8 };
        let mut r = rng0();
        let mut c = ctx();
        c.is_local_player = false;
        assert!(matches!(
            send_mail_from_mother(&mut info, &today, &c, &mut r),
            StepOutcome::Inactive
        ));
    }
}
