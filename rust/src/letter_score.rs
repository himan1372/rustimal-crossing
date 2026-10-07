//! Animal Crossing GameCube letter scoring engine (clean-room Rust port).
//!
//! Ports the normal-letter 7-check scorer (`mMck_check_key_hit_nes`) and the
//! quest-letter ranker (`mQst_GetMailRank`, via `mNpc_CheckNormalMail_length`)
//! from `src/game/m_mail_check_ovl.c`, `src/game/m_quest.c` and
//! `src/game/m_npc.c` of the decompilation.
//!
//! # What is scored
//!
//! The game never understands the letter. It runs a lightweight heuristic
//! asking "does this character stream look like a letter?", rewarding
//! capitalization, spaces, recognizable word beginnings and sentence
//! punctuation, and punishing repeated characters, huge unpunctuated runs
//! and long space-free stretches.
//!
//! Two systems share the trigram infrastructure:
//!
//! * **Normal letters**: seven checks A-G summed (`S = A+B+C+D+E+F+G`).
//!   Score >= 100 gets a positive reply, 50-99 gets no reply, < 50 gets a
//!   negative reply. Friendship moves -2/+1/+3/+6 (bad/good x present or
//!   not), clamped to 0-127.
//! * **Quest letters** (a villager asked for a letter): rank 0-11 =
//!   length (0/1/2) + trigram bonus (0/3) + present bonus (0/6), mapped to a
//!   reward table.
//!
//! # Trigram modes
//!
//! [`TrigramMode::Intended`] uses the 776 designed pairs with proper table
//! terminators. This matches the PC port, which builds the C decompilation
//! with `BUGFIXES` (installing the missing `0x7F` terminators).
//!
//! [`TrigramMode::NtscU`] reproduces the North American/Australian bug: the
//! tables lack the `0x7F` terminator, so the lookup scan runs past each
//! table's end into the following tables, and past the Z table into the RAM
//! tail (5,858 bytes up to the `0x7F` at `0x806A102E` in the original ROM).
//! The tail is modeled via `tables::TRIGRAM_RAM_EXTRA`: 758 byte pairs
//! reconstructed from Hunter R.'s published `trigrams-bugged.txt` `~~~`
//! section (779 entries) using the decomp's `m_font.h` charset. 21 entries
//! use obscure symbols whose byte mapping could not be determined reliably
//! and are omitted. With the 758, per-letter effective trigram counts match
//! Hunter's published table (A=1000 ... Z=780, total 23,670) to within those
//! 21 omitted pairs.
//!
//! # Source quirks reproduced here
//!
//! * Check A only awards per-separator points when more than 3 characters
//!   remain after the separator; separators in the final 3 characters score
//!   nothing either way.
//! * Check D scans the raw body (spaces are *not* stripped first).
//! * Check F only penalizes runs of 75+ characters that *follow* a `.`/`?`/`!`
//!   separator; a separator-free letter never triggers it. Spaces count
//!   toward the 75.
//! * The quest run-on helper (`mNpc_CheckNormalMail_sub`, already ported in
//!   `villager_mail`) needs 4 consecutive ordinary characters (or 9 of the
//!   symbol set) to trigger, because it resets its run counter to 0 rather
//!   than 1 on character change. Prose summaries saying "3+"/"up to 7" are
//!   simplifications; the code (C and this port) agrees on 4/9.
//! * `mMck_strlen` (used only by the percentage scorer) returns the *index*
//!   of the last non-space byte and never examines index 0, so a one-byte
//!   letter scores a 0% hit rate.
//! * An empty body makes check A read one byte *before* the buffer in C
//!   (undefined behavior, harmless in practice). This port scores it as 0
//!   instead of performing the out-of-bounds read.
//!
//! # Provenance
//!
//! Algorithms verified against `flyngmt/ACGC-PC-Port`
//! (`src/game/m_mail_check_ovl.c`, `src/game/m_npc.c`, `src/game/m_quest.c`).
//! Trigram pair data extracted from the decomp's `str_a_table..str_z_table`
//! (see `letter_score_tables.rs`); per-table counts match the published
//! research (57/49/44/.../1, 776 total). The NTSC-U RAM tail
//! (`TRIGRAM_RAM_EXTRA`) is reconstructed from Hunter R.'s
//! `trigrams-bugged.txt` (https://github.com/HunterRDev/AC-Letter-Scorer,
//! `Resources/trigrams-bugged.txt`), whose `~~~` section holds the 779
//! deduplicated tail pairs; each entry was mapped back to game bytes via the
//! decomp's `m_font.h` charset. Article:
//! https://hunter-r.com/posts/ac-trigrams/. No game code is copied: only the
//! short functional pair data and reimplemented algorithms are used.

// Public API surface for the rewrite (consumed via the C ABI below and by
// future Rust callers); not every item is referenced inside this crate yet.
#![allow(dead_code)]
#[path = "letter_score_tables.rs"]
mod tables;

/// Fixed letter body size (`MAIL_BODY_LEN`).
pub const MAIL_BODY_LEN: usize = 192;
/// Trigram scans are capped at `MAIL_BODY_LEN - 3`.
const TRIGRAM_SCAN_CAP: usize = MAIL_BODY_LEN - 3;

const CHAR_SPACE: u8 = 32;
const CHAR_EXCLAMATION: u8 = 33;
const CHAR_COMMA: u8 = 44;
const CHAR_PERIOD: u8 = 46;
const CHAR_QUESTIONMARK: u8 = 63;
const CHAR_INTERPUNCT: u8 = 133;
const CHAR_NEW_LINE: u8 = 205;

/// Which trigram tables the lookup may consult.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TrigramMode {
    /// Designed behavior: each table properly terminated (776 pairs total).
    /// Matches the PC port's `BUGFIXES` build.
    Intended,
    /// Faithful North American/Australian behavior: the scan runs past each
    /// table's missing terminator into the following tables, then through
    /// the post-Z RAM tail (`TRIGRAM_RAM_EXTRA`).
    NtscU,
}

/// Word separators for trigram extraction (`mMck_cmp_sep`).
fn is_sep(c: u8) -> bool {
    matches!(
        c,
        CHAR_SPACE | CHAR_COMMA | CHAR_QUESTIONMARK | CHAR_EXCLAMATION | CHAR_PERIOD | CHAR_INTERPUNCT | CHAR_NEW_LINE
    )
}

/// Sentence separators (`.`, `?`, `!`) used by checks A and F.
fn is_nes_sep(c: u8) -> bool {
    matches!(c, CHAR_PERIOD | CHAR_QUESTIONMARK | CHAR_EXCLAMATION)
}

fn is_upper(c: u8) -> bool {
    c.is_ascii_uppercase()
}

fn is_alpha(c: u8) -> bool {
    c.is_ascii_alphabetic()
}

/// `mMck_strlen_new`: trailing-space-trimmed length, capped at `cap`.
fn trimmed_len(body: &[u8; MAIL_BODY_LEN], cap: usize) -> usize {
    let mut len = cap.min(MAIL_BODY_LEN);
    while len > 0 && body[len - 1] == CHAR_SPACE {
        len -= 1;
    }
    len
}

/// `mMck_strlen`: index of the last non-space byte in `body[1..=cap]`,
/// or 0 when there is none. Index 0 is never examined by the original.
fn last_nonspace_index(body: &[u8; MAIL_BODY_LEN], cap: usize) -> usize {
    let cap = cap.min(MAIL_BODY_LEN - 1);
    for idx in (1..=cap).rev() {
        if body[idx] != CHAR_SPACE {
            return idx;
        }
    }
    0
}

/// `mMck_search_sep`: scan for a separator followed by a non-separator
/// (a word boundary), at most `max` bytes from `start`.
fn search_sep(body: &[u8; MAIL_BODY_LEN], start: usize, max: usize) -> usize {
    let mut p = start;
    let mut n = max;
    while n != 0 {
        n -= 1;
        // p + 1 <= start + max <= 190 < 192, so this never reads out of bounds.
        if is_sep(body[p]) && !is_sep(body[p + 1]) {
            break;
        }
        p += 1;
    }
    p
}

/// `mMck_cmp_key`: is the trigram at `pos` (first 3 bytes of a word) valid?
fn trigram_hit(body: &[u8; MAIL_BODY_LEN], pos: usize, mode: TrigramMode) -> bool {
    let c0 = body[pos];
    let table = if c0.is_ascii_lowercase() {
        (c0 - b'a') as usize
    } else if c0.is_ascii_uppercase() {
        (c0 - b'A') as usize
    } else {
        return false;
    };
    let (lo, hi) = match mode {
        TrigramMode::Intended => (
            tables::TRIGRAM_TABLE_STARTS[table],
            tables::TRIGRAM_TABLE_STARTS[table + 1],
        ),
        // The NTSC-U scan never terminates per table: it keeps reading into
        // the following tables (the missing-0x7F bug), then into the RAM tail
        // past the Z table (tables::TRIGRAM_RAM_EXTRA).
        TrigramMode::NtscU => (
            tables::TRIGRAM_TABLE_STARTS[table],
            tables::TRIGRAM_TABLE_STARTS[26],
        ),
    };
    // pos + 2 <= 191: callers only pass word starts within the trimmed body.
    let want = [body[pos + 1], body[pos + 2]];
    if tables::TRIGRAM_PAIRS[lo..hi].iter().any(|&p| p == want) {
        return true;
    }
    // NTSC-U only: after the Z table, the scan continues through the RAM tail.
    if mode == TrigramMode::NtscU {
        return tables::TRIGRAM_RAM_EXTRA.iter().any(|&p| p == want);
    }
    false
}

/// Run `f` at the start of every word in the trimmed body, mirroring the
/// word-advance loop shared by the hit-count and percentage scorers.
fn for_each_word(body: &[u8; MAIL_BODY_LEN], str_len: usize, mut f: impl FnMut(usize)) {
    let mut pos = 0usize;
    let mut i = 0usize;
    while i <= str_len {
        f(pos);
        let next = search_sep(body, pos, str_len - i) + 1;
        i += next - pos;
        pos = next;
    }
}

/// `mMck_check_key_get_hit_count`: number of words with a valid trigram.
fn hit_count(body: &[u8; MAIL_BODY_LEN], mode: TrigramMode) -> u32 {
    let str_len = trimmed_len(body, TRIGRAM_SCAN_CAP);
    if str_len == 0 {
        return 0;
    }
    let mut hits = 0u32;
    for_each_word(body, str_len, |pos| {
        if trigram_hit(body, pos, mode) {
            hits += 1;
        }
    });
    hits
}

/// `mMck_check_key_hit`: trigram hit percentage (`matches*100/words`);
/// writes the word count to `words_out`.
fn hit_rate(body: &[u8; MAIL_BODY_LEN], words_out: &mut u32, mode: TrigramMode) -> u32 {
    let str_len = last_nonspace_index(body, TRIGRAM_SCAN_CAP);
    if str_len == 0 {
        *words_out = 0;
        return 0;
    }
    let mut matches = 0u32;
    let mut words = 0u32;
    for_each_word(body, str_len, |pos| {
        words += 1;
        if trigram_hit(body, pos, mode) {
            matches += 1;
        }
    });
    *words_out = words;
    matches * 10000 / (words * 100)
}

/// Check A (`mMck_check_key_type_A`): +20 for terminal `.`/`?`/`!`
/// (unless the body fills all 192 bytes), then +10/-10 per separator
/// depending on whether a capital follows within 3 characters.
fn check_a(body: &[u8; MAIL_BODY_LEN], len: usize) -> i32 {
    let mut points = 0;
    if len > 0 && len < MAIL_BODY_LEN && is_nes_sep(body[len - 1]) {
        points += 20;
    }
    // `idx + rem == len` is invariant, so `body[idx]` is always in bounds.
    let (mut idx, mut rem) = (0usize, len);
    while rem > 3 {
        while !is_nes_sep(body[idx]) && rem > 3 {
            idx += 1;
            rem -= 1;
        }
        if rem > 3 {
            let mut sz = 3;
            idx += 1;
            rem -= 1;
            loop {
                if is_upper(body[idx]) {
                    break;
                }
                sz -= 1;
                idx += 1;
                rem -= 1;
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

/// Check B (`mMck_check_key_type_B`): +3 per valid trigram.
fn check_b(body: &[u8; MAIL_BODY_LEN], mode: TrigramMode) -> i32 {
    hit_count(body, mode) as i32 * 3
}

/// Check C (`mMck_check_key_type_C`): +20 if the first non-space character
/// is uppercase, -10 otherwise (0 for an empty body).
fn check_c(body: &[u8; MAIL_BODY_LEN], len: usize) -> i32 {
    for i in 0..len {
        if body[i] != CHAR_SPACE {
            return if is_upper(body[i]) { 20 } else { -10 };
        }
    }
    0
}

/// Check D (`mMck_check_key_type_D`): -50 if any alpha character repeats
/// three times consecutively. The raw body is scanned; spaces are not
/// stripped first.
fn check_d(body: &[u8; MAIL_BODY_LEN], len: usize) -> i32 {
    let mut i = 0usize;
    while i + 2 < len {
        if is_alpha(body[i]) && body[i] == body[i + 1] && body[i] == body[i + 2] {
            return -50;
        }
        i += 1;
    }
    0
}

/// Check E (`mMck_check_key_type_E`): +20 when spaces are >= 20% of
/// non-space characters, else -20 (empty bodies score -20).
fn check_e(body: &[u8; MAIL_BODY_LEN], len: usize) -> i32 {
    let spaces = body[..len].iter().filter(|&&c| c == CHAR_SPACE).count();
    let non_spaces = len - spaces;
    if non_spaces > 0 && spaces * 100 / non_spaces >= 20 {
        20
    } else {
        -20
    }
}

/// Check F (`mMck_check_key_type_F`): -150 if 75+ characters follow a
/// `.`/`?`/`!` separator with no further separator. Spaces count toward
/// the 75; a separator-free letter never triggers this check.
fn check_f(body: &[u8; MAIL_BODY_LEN], len: usize) -> i32 {
    // `idx + rem == len` is invariant, keeping `body[idx]` in bounds.
    let (mut idx, mut rem) = (0usize, len);
    while rem > 76 {
        if is_nes_sep(body[idx]) {
            let mut run = 0;
            idx += 1;
            rem -= 1;
            while !is_nes_sep(body[idx]) {
                run += 1;
                if run >= 75 {
                    break;
                }
                idx += 1;
                rem -= 1;
            }
            if run >= 75 {
                return -150;
            }
        }
        idx += 1;
        rem -= 1;
    }
    0
}

/// Check G (`mMck_check_key_type_G`): -20 for every complete 32-character
/// window containing no space. A trailing partial window is not penalized.
fn check_g(body: &[u8; MAIL_BODY_LEN], len: usize) -> i32 {
    let mut window = 32i32;
    let mut no_space = true;
    let mut points = 0;
    for i in 0..len {
        window -= 1;
        if no_space {
            if body[i] == CHAR_SPACE {
                no_space = false;
            } else if window == 0 {
                points -= 20;
            }
        }
        if window == 0 {
            window = 32;
            no_space = true;
        }
    }
    points
}

/// Per-check breakdown of the normal-letter score.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct CheckScores {
    pub a: i32,
    pub b: i32,
    pub c: i32,
    pub d: i32,
    pub e: i32,
    pub f: i32,
    pub g: i32,
}

impl CheckScores {
    /// `S = A+B+C+D+E+F+G`.
    pub fn total(&self) -> i32 {
        self.a + self.b + self.c + self.d + self.e + self.f + self.g
    }
}

/// Score a normal letter (`mMck_check_key_hit_nes`): the seven checks A-G.
pub fn score_letter(body: &[u8; MAIL_BODY_LEN], mode: TrigramMode) -> CheckScores {
    let len = trimmed_len(body, MAIL_BODY_LEN);
    CheckScores {
        a: check_a(body, len),
        b: check_b(body, mode),
        c: check_c(body, len),
        d: check_d(body, len),
        e: check_e(body, len),
        f: check_f(body, len),
        g: check_g(body, len),
    }
}

/// Response tier for a normal-letter score.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LetterRank {
    /// Score < 50: negative reply.
    Bad,
    /// Score 50-99: no reply, but friendship still rises.
    Middle,
    /// Score >= 100: positive reply, ~50% gift chance.
    Ok,
}

impl LetterRank {
    /// Rank a normal-letter total (`mNpc_CheckNormalMail_nes` thresholds).
    pub fn of_score(score: i32) -> LetterRank {
        if score >= 100 {
            LetterRank::Ok
        } else if score < 50 {
            LetterRank::Bad
        } else {
            LetterRank::Middle
        }
    }

    /// Whether the villager sends a reply letter (the middle tier does not).
    pub fn sends_reply(self) -> bool {
        self != LetterRank::Middle
    }

    /// Friendship delta: base +3, -5 when bad, +3 more with a present
    /// (-2/+1/+3/+6).
    pub fn friendship_delta(self, present: bool) -> i32 {
        match (self, present) {
            (LetterRank::Bad, false) => -2,
            (LetterRank::Bad, true) => 1,
            (_, false) => 3,
            (_, true) => 6,
        }
    }
}

/// Clamp a friendship value to the game's 0-127 range
/// (`mNpc_AddFriendship`). Note: infographics saying 0-255 are wrong.
pub fn clamp_friendship(value: i32) -> i32 {
    value.clamp(0, 127)
}

/// Quest trigram/length component (`mNpc_CheckNormalMail_length`):
/// 0 = bad, 1 = ok, 2 = default. Also reports the non-space length.
pub fn quest_trigram_component(
    body: &[u8; MAIL_BODY_LEN],
    nonspace_len: &mut u32,
    mode: TrigramMode,
) -> u8 {
    const BAD: u8 = 0;
    const OK: u8 = 1;
    const NUM: u8 = 2;
    let mut words = 0u32;
    let rate = hit_rate(body, &mut words, mode);
    // Reuses the already-ported run-on/character-count helper, which keeps
    // the game's C ABI (`mNpc_CheckNormalMail_sub`).
    let (count, run_on) = crate::villager_mail::check_normal_mail(body);
    *nonspace_len = count as u32;
    let mut rank = NUM;
    if words < 3 {
        if count < 5 {
            rank = BAD;
        } else if run_on != 0 {
            rank = BAD;
        }
    } else if rate >= 30 {
        rank = OK;
    } else if run_on != 0 {
        rank = BAD;
    }
    rank
}

/// Quest letter rank 0-11 (`mQst_GetMailRank`): length tier (0/1/2) +
/// trigram bonus (0/3) + present bonus (0/6).
pub fn quest_rank(body: &[u8; MAIL_BODY_LEN], present: bool, mode: TrigramMode) -> u8 {
    let mut length = 0u32;
    let bonus = quest_trigram_component(body, &mut length, mode);
    let mut rank: u8 = if length >= 49 {
        2
    } else if length >= 17 {
        1
    } else {
        0
    };
    if bonus >= 1 {
        rank += 3;
    }
    if present {
        rank += 6;
    }
    rank
}

/// `mMck_check_key_hit_nes`: full 7-check normal-letter score.
///
/// Uses [`TrigramMode::Intended`], matching the PC port's `BUGFIXES` build.
/// Returns 0 for a null body.
///
/// # Safety
/// `body` must point to at least 192 readable bytes.
#[no_mangle]
pub unsafe extern "C" fn mMck_check_key_hit_nes(body: *const u8) -> i32 {
    if body.is_null() {
        return 0;
    }
    // SAFETY: the caller guarantees a 192-byte body per the C ABI contract.
    let body = unsafe { std::slice::from_raw_parts(body, MAIL_BODY_LEN) };
    let Ok(body) = <&[u8; MAIL_BODY_LEN]>::try_from(body) else {
        return 0;
    };
    score_letter(body, TrigramMode::Intended).total()
}

/// `mMck_check_key_hit`: trigram hit percentage; writes the word count to
/// `words_out` when non-null. Returns 0 for a null body.
///
/// # Safety
/// `body` must point to at least 192 readable bytes; `words_out` must be
/// writable when non-null.
#[no_mangle]
pub unsafe extern "C" fn mMck_check_key_hit(body: *const u8, words_out: *mut i32) -> i32 {
    if body.is_null() {
        return 0;
    }
    // SAFETY: the caller guarantees a 192-byte body per the C ABI contract.
    let body = unsafe { std::slice::from_raw_parts(body, MAIL_BODY_LEN) };
    let Ok(body) = <&[u8; MAIL_BODY_LEN]>::try_from(body) else {
        return 0;
    };
    let mut words = 0u32;
    let rate = hit_rate(body, &mut words, TrigramMode::Intended);
    if !words_out.is_null() {
        // SAFETY: checked non-null; writable per the C ABI contract.
        *words_out = words as i32;
    }
    rate as i32
}

/// Quest letter rank 0-11 (mirrors the static `mQst_GetMailRank`).
/// `present` is nonzero when a gift is attached. Returns 0 for a null body.
///
/// # Safety
/// `body` must point to at least 192 readable bytes.
#[no_mangle]
pub unsafe extern "C" fn mQst_GetMailRank(body: *const u8, present: i32) -> u8 {
    if body.is_null() {
        return 0;
    }
    // SAFETY: the caller guarantees a 192-byte body per the C ABI contract.
    let body = unsafe { std::slice::from_raw_parts(body, MAIL_BODY_LEN) };
    let Ok(body) = <&[u8; MAIL_BODY_LEN]>::try_from(body) else {
        return 0;
    };
    quest_rank(body, present != 0, TrigramMode::Intended)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body_of(text: &[u8]) -> [u8; MAIL_BODY_LEN] {
        let mut body = [CHAR_SPACE; MAIL_BODY_LEN];
        let n = text.len().min(MAIL_BODY_LEN);
        body[..n].copy_from_slice(&text[..n]);
        body
    }

    #[test]
    fn empty_body_scores_minus_twenty() {
        // A:0 B:0 C:0 D:0 E:-20 F:0 G:0
        let scores = score_letter(&body_of(b""), TrigramMode::Intended);
        assert_eq!(scores.total(), -20);
        assert_eq!(LetterRank::of_score(-20), LetterRank::Bad);
    }

    #[test]
    fn terminal_punctuation_bonus() {
        assert_eq!(check_a(&body_of(b"Hi!"), 3), 20);
        assert_eq!(check_a(&body_of(b"Hi"), 2), 0);
    }

    #[test]
    fn capitalization_after_separator() {
        // "Hello! How": +10 for the capital H within 3 chars of '!'.
        assert_eq!(check_a(&body_of(b"Hello! How"), 10), 10);
        // Separators in the final 3 characters score nothing either way.
        assert_eq!(check_a(&body_of(b"ab!c"), 4), 0);
    }

    #[test]
    fn full_body_gets_no_terminal_bonus() {
        let mut body = [b'a'; MAIL_BODY_LEN];
        body[MAIL_BODY_LEN - 1] = CHAR_PERIOD;
        assert_eq!(check_a(&body, MAIL_BODY_LEN), 0);
    }

    #[test]
    fn trigram_bleed_differs_by_mode() {
        // "aab": ('a','b') is not an intended A-table pair, but the NTSC-U
        // scan bleeds into the B table where it exists.
        let body = body_of(b"aab");
        assert_eq!(check_b(&body, TrigramMode::Intended), 0);
        assert_eq!(check_b(&body, TrigramMode::NtscU), 3);
    }

    #[test]
    fn ram_tail_pair_scores_in_ntsc_u_only() {
        // "A! ": ('!',' ') is not in any intended table, but the NTSC-U scan
        // reaches the post-Z RAM tail where the pair exists.
        let body = body_of(b"A! ");
        assert_eq!(check_b(&body, TrigramMode::Intended), 0);
        assert_eq!(check_b(&body, TrigramMode::NtscU), 3);
    }

    #[test]
    fn common_trigram_scores_in_both_modes() {
        // "the": ('h','e') is a genuine T-table pair.
        let body = body_of(b"the");
        assert_eq!(check_b(&body, TrigramMode::Intended), 3);
        assert_eq!(check_b(&body, TrigramMode::NtscU), 3);
    }

    #[test]
    fn initial_capitalization() {
        assert_eq!(check_c(&body_of(b"Hello"), 5), 20);
        assert_eq!(check_c(&body_of(b"hello"), 5), -10);
        assert_eq!(check_c(&body_of(b""), 0), 0);
    }

    #[test]
    fn repeated_letters_no_space_stripping() {
        assert_eq!(check_d(&body_of(b"aaa"), 3), -50);
        assert_eq!(check_d(&body_of(b"aab"), 3), 0);
        // "a a a" has no three-in-a-row in the raw body.
        assert_eq!(check_d(&body_of(b"a a a"), 5), 0);
    }

    #[test]
    fn space_ratio() {
        assert_eq!(check_e(&body_of(b"a b"), 3), 20); // 1/2 = 50%
        assert_eq!(check_e(&body_of(b"ab"), 2), -20); // 0/2 = 0%
        assert_eq!(check_e(&body_of(b""), 0), -20);
    }

    #[test]
    fn run_on_penalty_needs_separator_first() {
        let long = [b'a'; 100];
        // No separator at all: no penalty.
        assert_eq!(check_f(&body_of(&long), 100), 0);
        // ". " followed by 75+ non-separators: -150 (spaces count).
        let mut text = vec![CHAR_PERIOD, CHAR_SPACE];
        text.extend([b'a'; 75]);
        assert_eq!(check_f(&body_of(&text), 77), -150);
        // 74 after the separator: no penalty.
        let mut text = vec![CHAR_PERIOD, CHAR_SPACE];
        text.extend([b'a'; 74]);
        assert_eq!(check_f(&body_of(&text), 76), 0);
    }

    #[test]
    fn thirty_two_char_blocks() {
        assert_eq!(check_g(&body_of(&[b'a'; 32]), 32), -20);
        let mut text = vec![b'a'; 31];
        text.push(CHAR_SPACE);
        assert_eq!(check_g(&body_of(&text), 32), 0);
        assert_eq!(check_g(&body_of(&[b'a'; 64]), 64), -40);
        // Partial trailing block is not penalized.
        assert_eq!(check_g(&body_of(&[b'a'; 33]), 33), -20);
    }

    #[test]
    fn quest_bang_string_gets_trigram_bonus() {
        // "!!!!!": fewer than 3 words, but 5 non-space chars and no run-on
        // (symbols tolerate up to 8 in a row), so the component defaults up.
        let body = body_of(b"!!!!!");
        let mut len = 0;
        assert_eq!(quest_trigram_component(&body, &mut len, TrigramMode::Intended), 2);
        assert_eq!(len, 5);
        assert_eq!(quest_rank(&body, false, TrigramMode::Intended), 3);
        assert_eq!(quest_rank(&body, true, TrigramMode::Intended), 9);
    }

    #[test]
    fn quest_rank_extremes() {
        let empty = body_of(b"");
        assert_eq!(quest_rank(&empty, false, TrigramMode::Intended), 0);
        assert_eq!(quest_rank(&empty, true, TrigramMode::Intended), 6);
    }

    #[test]
    fn friendship_and_reply_rules() {
        assert_eq!(LetterRank::Bad.friendship_delta(false), -2);
        assert_eq!(LetterRank::Bad.friendship_delta(true), 1);
        assert_eq!(LetterRank::Middle.friendship_delta(false), 3);
        assert_eq!(LetterRank::Ok.friendship_delta(true), 6);
        assert!(!LetterRank::Middle.sends_reply());
        assert!(LetterRank::Ok.sends_reply());
        assert!(LetterRank::Bad.sends_reply());
        assert_eq!(clamp_friendship(200), 127);
        assert_eq!(clamp_friendship(-5), 0);
    }

    #[test]
    fn realistic_letter_scores_well() {
        let text = b"Hello! How are you today? I found a nice fossil near the river. Do you want to see it?";
        let scores = score_letter(&body_of(text), TrigramMode::Intended);
        assert!(scores.a > 0);
        assert!(scores.b > 0);
        assert_eq!(scores.c, 20);
        assert_eq!(scores.d, 0);
        assert!(scores.total() >= 100);
        assert_eq!(LetterRank::of_score(scores.total()), LetterRank::Ok);
    }
}
