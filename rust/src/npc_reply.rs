//! NPC reply generation, the mMck letter scorer, and the mHandbillz composer.
//!
//! Verified against `src/game/m_npc.c`, `src/game/m_mail_check_ovl.c`,
//! `src/game/m_handbill.c`, `include/m_handbill.h`, `src/game/m_font.c`,
//! `src/game/m_font_main.c_inc` and `src/game/m_msg_main.c_inc`
//! (GAFE01_00 Rev. 0).
//!
//! Pipeline: the player sends a letter -> the NPC scores it with the
//! deterministic 7-component `mMck_check_key_hit_nes` heuristic (<50 BAD,
//! 50-99 no reply, >=100 GOOD) -> a qualifying letter sets `send_reply` ->
//! at the next startup on a different calendar day `mNpc_Remail` generates
//! the reply (BAD = canned personality template; GOOD = five randomized
//! ROM fragments composed by `mHandbillz_load`) -> post-office storage.
//!
//! Two documented retail deviations the source cannot resolve:
//! * `mMck_cmp_key` scans past each key table looking for a 0x7F
//!   terminator that retail tables lack; the bytes it reads depend on the
//!   retail linker layout. The Rust search stops at the table end.
//! * The final English text lives in retail ROM resources (SUPERZ, MAILA/B/C,
//!   PSZ, string tables) which the decomp excludes; the composer is exact
//!   at the algorithm level and takes resources from a provider trait.

use crate::mck_key_tables::{KEY_TABLES, KEY_TABLE_LENS};

// ---------------------------------------------------------------------------
// Part 1: mMck letter scorer (m_mail_check_ovl.c)
// ---------------------------------------------------------------------------

pub const MAIL_BODY_LEN: usize = 192;

const CHAR_SPACE: u8 = 32;
const CHAR_PERIOD: u8 = 46;
const CHAR_QUESTIONMARK: u8 = 63;
const CHAR_EXCLAMATION: u8 = 33;
const CHAR_COMMA: u8 = 44;
const CHAR_NEW_LINE: u8 = 10;
const CHAR_INTERPUNCT: u8 = 133;
const CHAR_CONTROL_CODE: u8 = 0x7F;

/// mMck_strlen_new: trim trailing CHAR_SPACE bytes. No NUL involved.
pub fn strlen_new(body: &[u8], len: usize) -> usize {
    let mut len = len;
    if len > 0 {
        let mut end = len;
        while len != 0 {
            end -= 1;
            if body[end] != CHAR_SPACE {
                break;
            }
            len -= 1;
        }
    }
    len
}

/// General separator: space, comma, ?, !, ., interpunct, newline.
fn cmp_sep(c: u8) -> bool {
    matches!(
        c,
        CHAR_SPACE | CHAR_COMMA | CHAR_QUESTIONMARK | CHAR_EXCLAMATION | CHAR_PERIOD | CHAR_INTERPUNCT | CHAR_NEW_LINE
    )
}

/// NES sentence separator: only `.`, `?`, `!`.
fn cmp_sep_nes(c: u8) -> bool {
    matches!(c, CHAR_PERIOD | CHAR_QUESTIONMARK | CHAR_EXCLAMATION)
}

fn check_alpha(c: u8, upper: bool) -> bool {
    let base = if upper { b'A' } else { b'a' };
    (base..base + 26).contains(&c)
}

/// mMck_cmp_key: 3-byte vocabulary test. The first byte selects one of the
/// 26 tables case-insensitively; bytes 2-3 must match a lowercase pair
/// exactly.
///
/// Documented deviation: retail scans past the table end looking for a
/// 0x7F terminator the USA tables lack (they end `0, 0` or nothing), so the
/// retail result can depend on linker-adjacent bytes. The Rust search stops
/// at the nominal table end.
pub fn cmp_key(s: &[u8]) -> bool {
    let (c0, c1, c2) = (s[0], s[1], s[2]);
    let lower = c0.to_ascii_lowercase();
    if !(b'a'..=b'z').contains(&lower) {
        return false;
    }
    let t = (lower - b'a') as usize;
    for i in 0..KEY_TABLE_LENS[t] {
        let (b, c) = KEY_TABLES[t][i];
        if c1 == b && c2 == c {
            return true;
        }
    }
    false
}

/// mMck_search_sep: find a separator whose next char is not a separator.
fn search_sep(body: &[u8], from: usize, range: usize) -> usize {
    let mut p = from;
    let mut left = range;
    while left != 0 {
        left -= 1;
        let next_ok = p + 1 >= body.len() || !cmp_sep(body[p + 1]);
        if p < body.len() && cmp_sep(body[p]) && next_ok {
            break;
        }
        p += 1;
    }
    p
}

/// mMck_check_key_get_hit_count: +1 per vocabulary hit, advancing past each
/// separator run. Scans at most 189 nominal bytes.
fn check_key_get_hit_count(body: &[u8]) -> i32 {
    let str_len = strlen_new(body, MAIL_BODY_LEN - 3);
    if str_len == 0 {
        return 0;
    }
    let mut hits = 0;
    let mut i = 0usize;
    let mut p = 0usize;
    while i <= str_len {
        if p + 3 <= body.len() && cmp_key(&body[p..p + 3]) {
            hits += 1;
        }
        let sep = search_sep(body, p, str_len.saturating_sub(i)) + 1;
        i += sep - p;
        p = sep;
    }
    hits
}

/// A: +20 if the letter ends with `.`/`?`/`!`; then per separator, +10 if
/// any of the next 3 bytes is uppercase A-Z, else -10.
fn key_type_a(body: &[u8], pos: usize) -> i32 {
    let mut points = 0;
    if pos < MAIL_BODY_LEN && pos > 0 && cmp_sep_nes(body[pos - 1]) {
        points = 20;
    }
    let mut str_p = 0usize;
    let mut pos = pos;
    while pos > 3 {
        while !cmp_sep_nes(body[str_p]) && pos > 3 {
            str_p += 1;
            pos -= 1;
        }
        if pos > 3 {
            let mut sz = 3;
            str_p += 1;
            pos -= 1;
            loop {
                if check_alpha(body[str_p], true) {
                    break;
                }
                sz -= 1;
                str_p += 1;
                pos -= 1;
                if sz <= 0 {
                    break;
                }
            }
            if sz == 0 {
                points -= 10;
            } else {
                points += 10;
            }
        }
    }
    points
}

/// B: +3 per vocabulary hit.
fn key_type_b(body: &[u8]) -> i32 {
    check_key_get_hit_count(body) * 3
}

/// C: first non-space char of the whole body: uppercase +20, else -10,
/// empty body 0.
fn key_type_c(body: &[u8], len: usize) -> i32 {
    for i in 0..len {
        if body[i] != CHAR_SPACE {
            return if check_alpha(body[i], true) { 20 } else { -10 };
        }
    }
    0
}

/// D: -50 once for the first run of 3 identical alpha chars.
fn key_type_d(body: &[u8], len: usize) -> i32 {
    let mut i = 0;
    let mut left = len;
    while left > 2 {
        if check_alpha(body[i], false) || check_alpha(body[i], true) {
            if body[i] == body[i + 1] && body[i] == body[i + 2] {
                return -50;
            }
        }
        left -= 1;
        i += 1;
    }
    0
}

/// E: spaces*100/non_spaces >= 20 -> +20 else -20; no non-space -> -20.
fn key_type_e(body: &[u8], len: usize) -> i32 {
    let spaces = body[..len].iter().filter(|&&c| c == CHAR_SPACE).count();
    let non_spaces = len - spaces;
    if non_spaces > 0 && spaces * 100 / non_spaces >= 20 {
        20
    } else {
        -20
    }
}

/// F: 75+ chars without `.`/`?`/`!` after a separator -> -150, once.
/// Retail's inner scan has no length bound; the Rust port clamps to the
/// body (documented deviation: retail could read past the buffer).
fn key_type_f(body: &[u8], len: usize) -> i32 {
    let mut i = 0usize;
    let mut left = len;
    while left > 76 && i < body.len() {
        if cmp_sep_nes(body[i]) {
            let mut sentence_len = 0;
            left -= 1;
            i += 1;
            while i < body.len() && !cmp_sep_nes(body[i]) {
                sentence_len += 1;
                if sentence_len >= 75 {
                    break;
                }
                i += 1;
                left = left.saturating_sub(1);
            }
            if sentence_len >= 75 {
                return -150;
            }
        }
        left = left.saturating_sub(1);
        i += 1;
    }
    0
}

/// G: -20 for each complete 32-byte block with no space.
fn key_type_g(body: &[u8], len: usize) -> i32 {
    let mut points = 0;
    for block in body[..len].chunks(32) {
        if block.len() == 32 && !block.contains(&CHAR_SPACE) {
            points -= 20;
        }
    }
    points
}

/// mMck_check_key_hit_nes: the full 7-component deterministic scorer.
pub fn check_key_hit_nes(body: &[u8; MAIL_BODY_LEN]) -> i32 {
    let len = strlen_new(body, MAIL_BODY_LEN);
    key_type_a(body, len)
        + key_type_b(body)
        + key_type_c(body, len)
        + key_type_d(body, len)
        + key_type_e(body, len)
        + key_type_f(body, len)
        + key_type_g(body, len)
}

// ---------------------------------------------------------------------------
// Part 2: reply rank
// ---------------------------------------------------------------------------

/// mNpc_LETTER_RANK_*: 0 = BAD, 1 = OK, 2 = NUM (no reply).
pub mod rank {
    pub const BAD: u8 = 0;
    pub const OK: u8 = 1;
    pub const NUM: u8 = 2;
}

/// mNpc_CheckNormalMail_nes: <50 BAD, 50-99 NUM (no reply), >=100 OK.
/// The USA path always uses this; the _length variant is unused.
pub fn check_normal_mail_nes(body: &[u8; MAIL_BODY_LEN]) -> u8 {
    let key_hit = check_key_hit_nes(body);
    if key_hit >= 100 {
        rank::OK
    } else if key_hit < 50 {
        rank::BAD
    } else {
        rank::NUM
    }
}

// ---------------------------------------------------------------------------
// Part 3: receive path (mNpc_SendMailtoNpc)
// ---------------------------------------------------------------------------

/// Bitfield: exists:1, cond:1, send_reply:1, has_present_cloth:1,
/// wearing_present_cloth:1, bit5_7:3.
#[derive(Clone, Copy, Debug, Default)]
pub struct LetterInfo {
    pub bits: u8,
}

impl LetterInfo {
    fn get(&self, shift: u8) -> bool {
        (self.bits >> shift) & 1 == 1
    }
    fn set(&mut self, shift: u8, v: bool) {
        if v {
            self.bits |= 1 << shift;
        } else {
            self.bits &= !(1 << shift);
        }
    }
    pub fn exists(&self) -> bool {
        self.get(0)
    }
    pub fn cond(&self) -> u8 {
        self.get(1) as u8
    }
    pub fn send_reply(&self) -> bool {
        self.get(2)
    }
    pub fn set_exists(&mut self, v: bool) {
        self.set(0, v)
    }
    pub fn set_cond(&mut self, v: u8) {
        self.set(1, v != 0)
    }
    pub fn set_send_reply(&mut self, v: bool) {
        self.set(2, v)
    }
}

/// Anmremail_c: single pending foreign-villager reply (0x16 bytes).
#[derive(Clone, Copy, Debug)]
pub struct AnimalRemail {
    pub date: (u16, u8, u8),
    pub name: [u8; 8],
    pub land_name: [u8; 8],
    /// bit0 = cond, bits1-7 = looks; 0x7F looks = empty.
    pub flags: u8,
}

impl Default for AnimalRemail {
    fn default() -> Self {
        let mut r = AnimalRemail {
            date: (0xFFFF, 0xFF, 0xFF),
            name: [0x20; 8],
            land_name: [0x20; 8],
            flags: 0,
        };
        r.clear();
        r
    }
}

impl AnimalRemail {
    /// mNpc_ClearRemail: cond = BAD, looks = 0x7F (empty sentinel).
    pub fn clear(&mut self) {
        self.date = (0xFFFF, 0xFF, 0xFF);
        self.name = [0x20; 8];
        self.land_name = [0x20; 8];
        self.flags = (0x7F << 1) | (rank::BAD & 1);
    }
    pub fn cond(&self) -> u8 {
        self.flags & 1
    }
    pub fn looks(&self) -> u8 {
        (self.flags >> 1) & 0x7F
    }
    pub fn is_empty(&self) -> bool {
        self.looks() == 0x7F
    }
    pub fn set(&mut self, cond: u8, looks: u8) {
        self.flags = ((looks & 0x7F) << 1) | (cond & 1);
    }
}

/// Outcome of scoring a received letter (mNpc_SetMailCondThisLand).
#[derive(Clone, Copy, Debug)]
pub struct ReceiveScore {
    pub rank: u8,
    /// Reply scheduled (rank was BAD or OK).
    pub send_reply: bool,
}

/// Score a received letter body: stamp cond/send_reply per the rank.
pub fn score_received_letter(body: &[u8; MAIL_BODY_LEN]) -> ReceiveScore {
    let rank = check_normal_mail_nes(body);
    ReceiveScore {
        rank,
        send_reply: rank < rank::NUM,
    }
}

/// Friendship change on receiving a letter: +3, -5 if BAD, +3 if a present
/// was attached. (Skipped entirely during the first job.)
pub fn receive_friendship_delta(rank: u8, has_present: bool) -> i32 {
    let mut d = 3;
    if rank == rank::BAD {
        d -= 5;
    }
    if has_present {
        d += 3;
    }
    d
}

// ---------------------------------------------------------------------------
// Part 4: startup scan (mNpc_Remail)
// ---------------------------------------------------------------------------

/// mNpc_CheckLetterTime: eligible when the stored letter date is valid
/// (day != 0xFF) and differs from today. No multi-day catch-up.
pub fn letter_time_eligible(letter_date: (u16, u8, u8), today: (u16, u8, u8)) -> bool {
    letter_date.0 != 0xFF && letter_date != today
}

/// One NPC's pending reply as seen by the startup scan.
#[derive(Clone, Copy, Debug)]
pub struct PendingReply {
    pub send_reply: bool,
    pub letter_date: (u16, u8, u8),
    pub cond: u8,
    pub looks: u8,
    pub foreign: bool,
}

/// Result of attempting one pending reply.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplyAttempt {
    /// Not eligible (no reply flag or same-day letter): continue scanning.
    Skipped,
    /// Generated and stored: clear the pending flag, continue scanning.
    Sent,
    /// Post-office full: keep pending and STOP the scan (retail `break`).
    Blocked,
}

/// Attempt one pending reply. `post_office_has_room` mirrors
/// `mPO_get_keep_mail_sum() < mPO_MAIL_STORAGE_SIZE`.
pub fn attempt_reply(pending: &PendingReply, today: (u16, u8, u8), post_office_has_room: bool) -> ReplyAttempt {
    if !pending.send_reply {
        return ReplyAttempt::Skipped;
    }
    // Foreign replies skip the letter-date check (retail has none there).
    if !pending.foreign && !letter_time_eligible(pending.letter_date, today) {
        return ReplyAttempt::Skipped;
    }
    if post_office_has_room {
        ReplyAttempt::Sent
    } else {
        ReplyAttempt::Blocked
    }
}

// ---------------------------------------------------------------------------
// Part 5: reply generation
// ---------------------------------------------------------------------------

/// GOOD reply base tables per looks (0-5): local then foreign.
pub const GOOD_THIS_START: [u16; 6] = [0x020, 0x040, 0x000, 0x060, 0x080, 0x0A0];
pub const GOOD_OTHER_START: [u16; 6] = [0x0E0, 0x100, 0x0C0, 0x120, 0x140, 0x160];
/// BAD reply bases: local 0xC5, foreign 0xD8; + looks*3 + RANDOM(3).
pub const BAD_THIS_BASE: u16 = 0xC5;
pub const BAD_OTHER_BASE: u16 = 0xD8;

/// mNpc_GetRemailWrongData message selection.
pub fn bad_reply_msg_no(foreign: bool, looks: usize, rng: &mut dyn FnMut(u32) -> u32) -> u16 {
    let base = if foreign { BAD_OTHER_BASE } else { BAD_THIS_BASE };
    base + looks as u16 * 3 + rng(3) as u16
}

/// The five fragment indices + present decision for a GOOD reply.
#[derive(Clone, Copy, Debug)]
pub struct GoodReplyPlan {
    pub give_present: bool,
    /// Present category roll (RANDOM(4) & 1): false = furniture, true = cloth.
    /// Only meaningful when give_present; the actual item is engine-resolved.
    pub present_is_cloth: bool,
    pub super_no: u16,
    pub maila_no: u16,
    pub mailb_no: u16,
    pub mailc_no: u16,
    pub ps_no: u16,
}

/// mNpc_GetRemailGoodData fragment selection. `rng` supplies RANDOM(4),
/// then the fragment rolls; `rng_f` supplies the 11 RANDOM_F calls (done by
/// the caller via [`free_string_indices`]).
pub fn good_reply_plan(
    foreign: bool,
    looks: usize,
    rng: &mut dyn FnMut(u32) -> u32,
) -> GoodReplyPlan {
    let msg_no = if foreign {
        GOOD_OTHER_START[looks]
    } else {
        GOOD_THIS_START[looks]
    };
    let give_present = rng(4) & 1 == 0;
    let present_is_cloth = if give_present { rng(4) & 1 == 1 } else { false };
    // NOTE: the 11 free-string RANDOM_F calls happen here in retail
    // (mNpc_SetRemailFreeString), between the present rolls and the
    // fragment rolls. The caller must invoke rng_f 11 times in between.
    GoodReplyPlan {
        give_present,
        present_is_cloth,
        super_no: msg_no + rng(32) as u16,
        maila_no: msg_no + rng(32) as u16,
        mailb_no: msg_no + rng(16) as u16 + if give_present { 16 } else { 0 },
        mailc_no: msg_no + rng(32) as u16,
        ps_no: msg_no + rng(32) as u16,
    }
}

/// Free-string category base ROM string numbers (FREE_STR3..13).
pub const FREE_STR_BASES: [u16; 11] = [
    0x314, // food
    0x334, // sports
    0x2F4, // hobby games
    0x6A1, // fish
    0x679, // insects
    0x354, // food tastes
    0x374, // feelings
    0x394, // music genres
    0x3D4, // food satisfaction feelings
    0x3F4, // "good" descriptors
    0x3B4, // "bad" descriptors
];

/// Per-category RANDOM_F ranges.
pub const FREE_STR_RANGES: [f32; 11] = [
    32.0, 32.0, 32.0, 40.0, 40.0, 32.0, 32.0, 32.0, 32.0, 32.0, 32.0,
];

/// mNpc_SetRemailFreeString category rolls: 11 RANDOM_F calls returning the
/// selected ROM string number per category (base + roll).
pub fn free_string_indices(rng_f: &mut dyn FnMut(f32) -> u32) -> [u16; 11] {
    let mut out = [0u16; 11];
    for i in 0..11 {
        out[i] = FREE_STR_BASES[i] + rng_f(FREE_STR_RANGES[i]) as u16;
    }
    out
}

// ---------------------------------------------------------------------------
// Part 6: mHandbillz composer (m_handbill.c)
// ---------------------------------------------------------------------------

pub const FREE_STR_NUM: usize = 20;
pub const FREE_STR_LEN: usize = 16;
pub const HEADER2_LEN: usize = 40;
pub const FOOTER2_LEN: usize = 48;
pub const HEADER_LEN: usize = 24;
pub const FOOTER_LEN: usize = 32;
pub const SUPER_TMP_LEN: usize = 43;

/// Article ids (mIN_ARTICLE_*).
pub mod article {
    pub const NONE: u8 = 0;
    pub const NUM: u8 = 5; // A, AN, THE, SOME
}

/// Resource banks composed by mHandbillz_load.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HandbillzType {
    Super,
    MailA,
    MailB,
    MailC,
    Ps,
}

/// Maximum payload bytes per resource type (mHandbillz_dummy_size_tbl).
pub fn handbillz_max_payload(typ: HandbillzType) -> usize {
    match typ {
        HandbillzType::Super => 27,
        HandbillzType::MailA => 200,
        HandbillzType::MailB => 200,
        HandbillzType::MailC => 200,
        HandbillzType::Ps => 34,
    }
}

/// Retail resources + article strings. Payloads over the max are rejected.
pub trait HandbillzResources {
    fn load(&mut self, typ: HandbillzType, num: u16) -> Option<Vec<u8>>;
    fn article(&mut self, article: u8) -> Vec<u8>;
}

/// Control-code ids (mFont_CONT_CODE_*; FREE codes sit at fixed positions).
pub mod cont_code {
    pub const FREE0: u8 = 37;
    // FREE1..FREE9 follow (38..46); FREE10..FREE19 at 55..64
    pub const CUT_ARTICLE: u8 = 117;
    pub const CAPITAL_LETTER: u8 = 118;
}

fn free_code_to_str(code: u8) -> Option<usize> {
    match code {
        37..=46 => Some((code - 37) as usize),
        55..=64 => Some((code - 55) as usize + 10),
        _ => None,
    }
}

/// Composed reply buffers.
#[derive(Clone, Debug)]
pub struct HandbillzOut {
    pub header: [u8; HEADER2_LEN],
    pub header_back_start: usize,
    pub body: [u8; MAIL_BODY_LEN],
    pub footer: [u8; FOOTER2_LEN],
}

/// mHandbillz_load as an in-place buffer editor over byte buffers.
pub struct HandbillzComposer {
    free_str: [[u8; FREE_STR_LEN]; FREE_STR_NUM],
    free_str_art: [u8; FREE_STR_NUM],
    force_art: u8,
    capital_flag: bool,
}

impl HandbillzComposer {
    pub fn new() -> Self {
        HandbillzComposer {
            free_str: [[CHAR_SPACE; FREE_STR_LEN]; FREE_STR_NUM],
            free_str_art: [article::NONE; FREE_STR_NUM],
            force_art: article::NUM,
            capital_flag: false,
        }
    }

    /// mHandbill_load_init. Retail bug preserved: the init writes
    /// `force_art = mIN_ARTICLE_NUM` instead of clearing `capital_flag`
    /// (copy-paste error in the decomp's #ifndef BUGFIXES branch).
    fn load_init(&mut self) {
        self.force_art = article::NUM;
        // BUG (retail): capital_flag is NOT cleared here.
    }

    /// mHandbill_Set_free_str: fixed-width slot, space-padded; article reset.
    pub fn set_free_str(&mut self, str_num: usize, s: &[u8]) {
        if str_num >= FREE_STR_NUM {
            return;
        }
        let n = s.len().min(FREE_STR_LEN);
        self.free_str[str_num][..n].copy_from_slice(&s[..n]);
        for b in &mut self.free_str[str_num][n..] {
            *b = CHAR_SPACE;
        }
        self.free_str_art[str_num] = article::NONE;
    }

    /// mMsg_Get_Length_String: trailing-space trim.
    fn msg_len(s: &[u8]) -> usize {
        let mut i = s.len();
        while i > 0 && s[i - 1] == CHAR_SPACE {
            i -= 1;
        }
        i
    }

    /// mHandbill_MoveDataCut: shift the tail to make room (or close a gap).
    /// Returns the new data length.
    fn move_data_cut(
        buf: &mut [u8],
        dst_idx: usize,
        src_idx: usize,
        data_len: usize,
        fill: u8, // FILL_NONE = 0xFF sentinel here; else the fill char
    ) -> usize {
        let buf_size = buf.len();
        let mut new_len = data_len;
        if dst_idx < src_idx {
            let mut s = src_idx;
            let mut d = dst_idx;
            while s < data_len {
                buf[d] = buf[s];
                d += 1;
                s += 1;
            }
            new_len -= s - d;
            if fill != 0xFF {
                while d < data_len {
                    buf[d] = fill;
                    d += 1;
                }
            }
        } else if dst_idx > src_idx {
            let mut move_size = data_len - src_idx;
            new_len += dst_idx - src_idx;
            if new_len > buf_size {
                let over = new_len - buf_size;
                let data_len2 = data_len - over;
                move_size -= over;
                new_len = buf_size;
                let mut d = new_len;
                let mut s = data_len2;
                for _ in 0..move_size {
                    d -= 1;
                    s -= 1;
                    buf[d] = buf[s];
                }
            } else {
                let mut d = new_len;
                let mut s = data_len;
                for _ in 0..move_size {
                    d -= 1;
                    s -= 1;
                    buf[d] = buf[s];
                }
            }
        }
        new_len
    }

    /// small_to_capital for the ASCII range (the retail 56-pair table also
    /// covers accented characters; ASCII is what the reply path needs).
    fn small_to_capital(c: u8) -> u8 {
        if (b'a'..=b'z').contains(&c) {
            c - 32
        } else {
            c
        }
    }

    /// One FREE-code substitution at `start` (code is 2 bytes: 0x7F, type).
    fn put_string_free(
        &mut self,
        res: &mut dyn HandbillzResources,
        buf: &mut [u8],
        start: usize,
        data_len: usize,
        str_no: usize,
        fill: u8,
    ) -> usize {
        let code_size = 2usize;
        let free_len = Self::msg_len(&self.free_str[str_no]);
        let mut cut_len = Self::move_data_cut(buf, start + free_len, start + code_size, data_len, fill);
        let room = buf.len() - start;
        let copy_len = if cut_len >= buf.len() && free_len > room {
            room
        } else {
            free_len
        };
        buf[start..start + copy_len].copy_from_slice(&self.free_str[str_no][..copy_len]);

        let article = if self.force_art != article::NUM {
            self.force_art
        } else {
            self.free_str_art[str_no]
        };
        if article != article::NONE {
            let mut abuf = res.article(article);
            let mut alen = Self::msg_len(&abuf);
            abuf.resize(alen + 1, CHAR_SPACE);
            abuf[alen] = CHAR_SPACE;
            alen += 1;
            cut_len = Self::move_data_cut(buf, start + alen, start, cut_len, 0xFF);
            buf[start..start + alen].copy_from_slice(&abuf[..alen]);
        }

        if self.capital_flag {
            buf[start] = Self::small_to_capital(buf[start]);
        }

        self.force_art = article::NUM;
        // Retail bug: this clears force_art again instead of capital_flag,
        // so a set capital_flag persists (see load_init).
        self.capital_flag = self.capital_flag;

        cut_len
    }

    /// Dispatch one control code at `pos`. Returns the new data length.
    fn put_string(
        &mut self,
        res: &mut dyn HandbillzResources,
        buf: &mut [u8],
        pos: usize,
        data_len: usize,
        fill: u8,
    ) -> usize {
        let code = buf[pos + 1];
        if let Some(str_no) = free_code_to_str(code) {
            return self.put_string_free(res, buf, pos, data_len, str_no, fill);
        }
        match code {
            cont_code::CUT_ARTICLE => {
                self.force_art = article::NONE;
                Self::move_data_cut(buf, pos, pos + 2, data_len, 0xFF)
            }
            cont_code::CAPITAL_LETTER => {
                self.capital_flag = true;
                Self::move_data_cut(buf, pos, pos + 2, data_len, 0xFF)
            }
            _ => data_len,
        }
    }

    /// mHandbill_Change_ControlCode: expand codes in the first `len` bytes.
    fn change_control_code(
        &mut self,
        res: &mut dyn HandbillzResources,
        buf: &mut [u8],
        len: usize,
        fill: u8,
    ) -> usize {
        let mut pos = 0usize;
        let mut len = len;
        while pos < len && pos < buf.len() {
            if buf[pos] == CHAR_CONTROL_CODE {
                len = self.put_string(res, buf, pos, len, fill);
            } else {
                pos += 1;
            }
        }
        len
    }

    /// mHandbill_Change_ControlCode2: same, adjusting header_back_start when
    /// a substitution happens before it.
    fn change_control_code2(
        &mut self,
        res: &mut dyn HandbillzResources,
        buf: &mut [u8],
        len: usize,
        header_back_start: &mut usize,
        fill: u8,
    ) {
        let mut pos = 0usize;
        let mut len = len;
        while pos < len && pos < buf.len() {
            if buf[pos] == CHAR_CONTROL_CODE {
                let now = len;
                len = self.put_string(res, buf, pos, len, fill);
                if pos < *header_back_start {
                    *header_back_start = (*header_back_start as isize + (len as isize - now as isize)) as usize;
                }
            } else {
                pos += 1;
            }
        }
    }

    /// mHandbill_CheckSuperStringBorderAndCopy.
    fn check_super_border(dst: &mut [u8], header_back_start: &mut usize, src: &[u8]) {
        let mut lines = 0;
        let mut src_pos = 0;
        let mut d = 0;
        for &c in src {
            if c == CHAR_NEW_LINE {
                *header_back_start = src_pos;
                lines += 1;
            } else if d < dst.len() {
                dst[d] = c;
                d += 1;
            }
            src_pos += 1;
        }
        if lines != 1 {
            *header_back_start = src.len();
        }
    }

    fn load_fragment(
        &mut self,
        res: &mut dyn HandbillzResources,
        typ: HandbillzType,
        num: u16,
    ) -> Option<Vec<u8>> {
        let mut data = res.load(typ, num)?;
        if data.len() > handbillz_max_payload(typ) {
            return None;
        }
        data.truncate(handbillz_max_payload(typ));
        Some(data)
    }

    /// mHandbillz_load: compose the five fragments into header/body/footer.
    /// All three stages always run; success requires all three.
    pub fn z_load(
        &mut self,
        res: &mut dyn HandbillzResources,
        super_no: u16,
        maila_no: u16,
        mailb_no: u16,
        mailc_no: u16,
        ps_no: u16,
    ) -> Option<HandbillzOut> {
        self.load_init();

        // SUPER -> 43-byte temp -> 40-byte header.
        let super_res = (|| {
            let data = self.load_fragment(res, HandbillzType::Super, super_no)?;
            let mut tmp = [CHAR_SPACE; SUPER_TMP_LEN];
            let mut hbs = 0usize;
            Self::check_super_border(&mut tmp, &mut hbs, &data);
            let dlen = data.len().saturating_sub(1).min(tmp.len());
            self.change_control_code2(res, &mut tmp, dlen, &mut hbs, CHAR_SPACE);
            let mut header = [CHAR_SPACE; HEADER2_LEN];
            let n = header.len().min(tmp.len());
            header[..n].copy_from_slice(&tmp[..n]);
            Some((header, hbs))
        })();

        // MAILA + MAILB + MAILC concatenated; total > 192 fails.
        let mail_res = (|| {
            let a = self.load_fragment(res, HandbillzType::MailA, maila_no)?;
            let b = self.load_fragment(res, HandbillzType::MailB, mailb_no)?;
            let c = self.load_fragment(res, HandbillzType::MailC, mailc_no)?;
            let total = a.len() + b.len() + c.len();
            if total > MAIL_BODY_LEN {
                return None;
            }
            let mut body = [CHAR_NEW_LINE; MAIL_BODY_LEN];
            body[..a.len()].copy_from_slice(&a);
            body[a.len()..a.len() + b.len()].copy_from_slice(&b);
            body[a.len() + b.len()..total].copy_from_slice(&c);
            self.change_control_code(res, &mut body, total, CHAR_NEW_LINE);
            Some(body)
        })();

        // PS -> 48-byte footer, space-padded.
        let ps_res = (|| {
            let data = self.load_fragment(res, HandbillzType::Ps, ps_no)?;
            let mut footer = [CHAR_SPACE; FOOTER2_LEN];
            let n = data.len().min(footer.len());
            footer[..n].copy_from_slice(&data[..n]);
            self.change_control_code(res, &mut footer, n, CHAR_SPACE);
            Some(footer)
        })();

        match (super_res, mail_res, ps_res) {
            (Some((header, header_back_start)), Some(body), Some(footer)) => Some(HandbillzOut {
                header,
                header_back_start,
                body,
                footer,
            }),
            _ => None,
        }
    }
}

impl Default for HandbillzComposer {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// C ABI
// ---------------------------------------------------------------------------

/// C ABI: score a 192-byte letter body, returning the mMck key_hit total.
#[no_mangle]
pub extern "C" fn pc_mck_key_hit(body: *const u8) -> i32 {
    let body = unsafe { &*(body as *const [u8; MAIL_BODY_LEN]) };
    check_key_hit_nes(body)
}

/// C ABI: classify a 192-byte letter body: 0 = BAD, 1 = OK, 2 = no reply.
#[no_mangle]
pub extern "C" fn pc_npc_letter_rank(body: *const u8) -> i32 {
    let body = unsafe { &*(body as *const [u8; MAIL_BODY_LEN]) };
    check_normal_mail_nes(body) as i32
}

/// C ABI: BAD-reply message number for foreign flag + looks + 0..3 roll.
#[no_mangle]
pub extern "C" fn pc_npc_bad_msg_no(foreign: i32, looks: i32, roll3: i32) -> i32 {
    (if foreign != 0 { BAD_OTHER_BASE } else { BAD_THIS_BASE } + looks as u16 * 3 + roll3 as u16) as i32
}

/// C ABI: GOOD-reply base message number for foreign flag + looks.
#[no_mangle]
pub extern "C" fn pc_npc_good_base_no(foreign: i32, looks: i32) -> i32 {
    (if foreign != 0 {
        GOOD_OTHER_START[looks as usize]
    } else {
        GOOD_THIS_START[looks as usize]
    }) as i32
}

/// C ABI: friendship delta for receiving a letter (rank 0/1, present flag).
#[no_mangle]
pub extern "C" fn pc_npc_receive_friendship(rank: i32, has_present: i32) -> i32 {
    receive_friendship_delta(rank as u8, has_present != 0)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn body_of(s: &[u8]) -> [u8; MAIL_BODY_LEN] {
        let mut b = [CHAR_SPACE; MAIL_BODY_LEN];
        b[..s.len()].copy_from_slice(s);
        b
    }

    #[test]
    fn strlen_trims_trailing_spaces() {
        let b = body_of(b"HELLO");
        assert_eq!(strlen_new(&b, MAIL_BODY_LEN), 5);
        let e = body_of(b"");
        assert_eq!(strlen_new(&e, MAIL_BODY_LEN), 0);
    }

    #[test]
    fn scorer_components() {
        // Well-formed short letter: "Hi. I am well."
        let b = body_of(b"Hi. I am well.");
        let score = check_key_hit_nes(&b);
        // A: ends with '.' -> +20; one separator, ' ' then 'I' uppercase within 3 -> +10
        // C: first non-space 'H' uppercase -> +20
        // E: spaces=3, non=11 -> 3*100/11=27 >= 20 -> +20
        // D/F/G: 0
        assert!(score >= 70, "score = {}", score);
    }

    #[test]
    fn triple_letter_penalty() {
        let b = body_of(b"Hello aaa world.");
        assert_eq!(key_type_d(&b, strlen_new(&b, MAIL_BODY_LEN)), -50);
    }

    #[test]
    fn runon_penalty() {
        let mut raw = vec![b'a'; 80];
        raw.extend_from_slice(b".");
        let b = body_of(&raw);
        assert_eq!(key_type_f(&b, strlen_new(&b, MAIL_BODY_LEN)), -150);
    }

    #[test]
    fn no_space_block_penalty() {
        let raw = vec![b'a'; 32];
        let b = body_of(&raw);
        assert_eq!(key_type_g(&b, strlen_new(&b, MAIL_BODY_LEN)), -20);
    }

    #[test]
    fn rank_thresholds() {
        // Empty body: C=0, E=-20, A: eof? pos=0 -> body[pos-1] underflow guard -> 0
        let e = body_of(b"");
        assert_eq!(check_normal_mail_nes(&e), rank::BAD); // very low score
    }

    #[test]
    fn letter_info_bits() {
        let mut li = LetterInfo::default();
        li.set_exists(true);
        li.set_cond(rank::OK);
        li.set_send_reply(true);
        assert!(li.exists() && li.send_reply() && li.cond() == 1);
        li.set_send_reply(false);
        assert!(!li.send_reply());
    }

    #[test]
    fn remail_clear_sentinel() {
        let mut r = AnimalRemail::default();
        assert!(r.is_empty());
        assert_eq!(r.cond(), rank::BAD);
        r.set(rank::OK, 3);
        assert!(!r.is_empty());
        assert_eq!(r.looks(), 3);
    }

    #[test]
    fn letter_time_gate() {
        let today = (2026u16, 10u8, 8u8);
        assert!(!letter_time_eligible(today, today));
        assert!(letter_time_eligible((2026, 10, 7), today));
        assert!(!letter_time_eligible((2026, 10, 0xFF), today));
    }

    #[test]
    fn reply_attempt_blocks_scan() {
        let p = PendingReply {
            send_reply: true,
            letter_date: (2026, 10, 7),
            cond: rank::OK,
            looks: 0,
            foreign: false,
        };
        let today = (2026u16, 10u8, 8u8);
        assert_eq!(attempt_reply(&p, today, true), ReplyAttempt::Sent);
        assert_eq!(attempt_reply(&p, today, false), ReplyAttempt::Blocked);
        let same_day = PendingReply { letter_date: today, ..p };
        assert_eq!(attempt_reply(&same_day, today, true), ReplyAttempt::Skipped);
        // Foreign skips the date check.
        let f = PendingReply { foreign: true, letter_date: today, ..p };
        assert_eq!(attempt_reply(&f, today, true), ReplyAttempt::Sent);
    }

    #[test]
    fn bad_msg_ranges() {
        let mut r = |_: u32| 0;
        assert_eq!(bad_reply_msg_no(false, 0, &mut r), 0xC5);
        assert_eq!(bad_reply_msg_no(false, 5, &mut r), 0xD4);
        assert_eq!(bad_reply_msg_no(true, 5, &mut r), 0xE7);
    }

    #[test]
    fn good_plan_fragment_math() {
        let mut r = |_: u32| 0;
        let plan = good_reply_plan(false, 2, &mut r);
        // looks 2 -> base 0x000; give_present: rng(4)&1 == 0 -> true
        assert!(plan.give_present);
        assert_eq!(plan.super_no, 0x000);
        assert_eq!(plan.mailb_no, 0x000 + 16); // present half
    }

    #[test]
    fn free_string_category_count() {
        let mut calls = 0;
        let mut rf = |_: f32| {
            calls += 1;
            0
        };
        let idx = free_string_indices(&mut rf);
        assert_eq!(calls, 11);
        assert_eq!(idx[0], 0x314);
        assert_eq!(idx[3], 0x6A1);
    }

    struct MapRes;
    impl HandbillzResources for MapRes {
        fn load(&mut self, typ: HandbillzType, _num: u16) -> Option<Vec<u8>> {
            Some(match typ {
                HandbillzType::Super => b"HELLO\x0aWORLD".to_vec(),
                HandbillzType::MailA => b"BODY-A ".to_vec(),
                HandbillzType::MailB => b"BODY-B ".to_vec(),
                HandbillzType::MailC => b"BODY-C".to_vec(),
                HandbillzType::Ps => b"PS!".to_vec(),
            })
        }
        fn article(&mut self, _article: u8) -> Vec<u8> {
            b"the".to_vec()
        }
    }

    #[test]
    fn composer_concatenates_and_borders() {
        let mut hz = HandbillzComposer::new();
        let mut res = MapRes;
        let out = hz.z_load(&mut res, 0, 0, 0, 0, 0).unwrap();
        // SUPER "HELLO\nWORLD": one newline at src_pos 5 -> back start 5, newline dropped.
        assert_eq!(out.header_back_start, 5);
        assert_eq!(&out.header[..10], b"HELLOWORLD");
        // Body = A+B+C then newline-padded.
        assert_eq!(&out.body[..20], b"BODY-A BODY-B BODY-C");
        assert_eq!(out.body[20], CHAR_NEW_LINE);
        assert_eq!(&out.footer[..3], b"PS!");
    }

    #[test]
    fn composer_free_string_substitution() {
        struct FreeRes;
        impl HandbillzResources for FreeRes {
            fn load(&mut self, typ: HandbillzType, _num: u16) -> Option<Vec<u8>> {
                Some(match typ {
                    HandbillzType::Super => b"S\x0aX".to_vec(),
                    HandbillzType::MailA => vec![0x7F, cont_code::FREE0],
                    _ => vec![],
                })
            }
            fn article(&mut self, _a: u8) -> Vec<u8> {
                vec![]
            }
        }
        let mut hz = HandbillzComposer::new();
        hz.set_free_str(0, b"PHIL");
        let mut res = FreeRes;
        let out = hz.z_load(&mut res, 0, 0, 0, 0, 0).unwrap();
        assert_eq!(&out.body[..4], b"PHIL");
    }

    #[test]
    fn composer_capital_flag_bug() {
        // Retail bug: load_init does NOT clear capital_flag.
        let mut hz = HandbillzComposer::new();
        hz.capital_flag = true;
        hz.load_init();
        assert!(hz.capital_flag);
    }
}
