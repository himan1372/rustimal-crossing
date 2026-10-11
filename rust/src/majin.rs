//! Mr. Resetti (internal name Majin) — pure data and kernels.
//!
//! Verified against `src/actor/ac_reset_demo.c`,
//! `src/actor/npc/ac_npc_majin.c`,
//! `src/actor/npc/ac_npc_majin_schedule.c_inc`,
//! `src/actor/npc/ac_npc_majin_talk.c_inc`,
//! `src/actor/npc/ac_npc_majin3_talk.c_inc`,
//! `src/actor/npc/ac_npc_majin4_talk.c_inc`,
//! `src/actor/npc/ac_npc_majin4_schedule.c_inc`,
//! `src/actor/npc/event/ac_ev_majin_move.c_inc`,
//! `include/m_name_table.h` (GAFE01_00 Rev 0 USA decomp).
//! All 8 brief claims checked verbatim; no corrections.
//!
//! Architecture: the reset-demo controller, the Resetti/Don actor
//! variants, their think/talk state machines, the text-entry overlay,
//! Don's reset/fade sequence, the lighting, and the Groundhog Day
//! event actor all stay in C. This module holds only the source-proven
//! pure data and calculations with primitive arguments.
//!
//! The word-entry scan (`check_ng_word`) is ported bit-for-bit as a
//! pure function over caller-provided buffers: the ROM string tables
//! stay in C. `mem_cmp` in the decomp returns TRUE on equality —
//! inverted vs. standard `memcmp` — so the comparisons below test
//! equality directly rather than substituting a standard memcmp.

/// Resetti identities (`include/m_name_table.h`).
pub mod id {
    pub const MAJIN: u16 = 0xD03E;
    pub const MAJIN2: u16 = 0xD065;
    pub const MAJIN3: u16 = 0xD068;
    pub const MAJIN4: u16 = 0xD069;
    pub const MAJIN5: u16 = 0xD06B;
    pub const MAJIN_D07C: u16 = 0xD07C;
    pub const MAJIN_D07D: u16 = 0xD07D;
    /// Don, Resetti's brother.
    pub const MAJIN_BROTHER: u16 = 0xD07E;
    pub const MAJIN_D080: u16 = 0xD080;
    /// Groundhog Day event actor (distinct from the reset encounter).
    pub const EV_MAJIN: u16 = 0xD081;
}

/// Think states (`ac_npc_majin.c`).
pub mod think {
    pub const START_WAIT: u8 = 0;
    pub const START_WAIT_ST: u8 = 1;
    pub const CALL: u8 = 2;
    pub const CALL_2: u8 = 3;
    pub const CALL_3: u8 = 4;
    pub const CALL_4: u8 = 5;
    pub const CALL_5: u8 = 6;
    pub const EXIT: u8 = 7;
}

/// Word-classification outcomes (`ac_npc_majin3.c`).
pub mod word {
    pub const GOOD: u8 = 0;
    pub const NG: u8 = 1;
    pub const WRONG: u8 = 2;
    /// Outcome messages: GOOD → 0x23EC, NG → 0x23EF, WRONG → 0x23EE.
    pub const MSG: [u16; 3] = [0x23EC, 0x23EF, 0x23EE];
    /// 16 good words (`reply_idx = RANDOM(16)`), 32 ng words.
    pub const GOOD_COUNT: u8 = 16;
    pub const NG_COUNT: u8 = 32;
    /// Fixed string buffer size (`mString_DEFAULT_STR_SIZE`).
    pub const STR_SIZE: usize = 16;
}

/// Reset-count identity table (`aRSD_first_set_init`).
pub const MAJIN_NAME: [u16; 8] = [
    id::MAJIN_D07C,
    id::MAJIN_D07D,
    id::MAJIN,
    id::MAJIN2,
    id::MAJIN_BROTHER,
    id::MAJIN3,
    id::MAJIN4,
    id::MAJIN_D080,
];

/// Reset-count normalization, verbatim: counts ≥ 9 fold into 6–8.
/// The demo's creation path guarantees `reset_count >= 1`, so
/// `MAJIN_NAME[reset - 1]` is always in bounds (verified).
pub fn normalize_reset_count(reset: u8) -> u8 {
    if reset >= 9 {
        6 + ((reset - 9) % 3)
    } else {
        reset
    }
}

/// Resetti identity for a reset count.
pub fn reset_identity(reset_count: u8) -> u16 {
    MAJIN_NAME[(normalize_reset_count(reset_count) - 1) as usize]
}

/// Initial call state per identity (`aMJN_setup_start_think`), verbatim:
/// MAJIN→CALL, D07C→CALL_2, D07D→CALL_3, BROTHER→CALL_4, D080→CALL_5,
/// and `default:` → CALL_4 (covers MAJIN2/3/4). Don's `arg = 1` feeds
/// the reset-hole effect and stays in C.
pub fn initial_call_state(npc_id: u16) -> u8 {
    match npc_id {
        id::MAJIN => think::CALL,
        id::MAJIN_D07C => think::CALL_2,
        id::MAJIN_D07D => think::CALL_3,
        id::MAJIN_BROTHER => think::CALL_4,
        id::MAJIN_D080 => think::CALL_5,
        _ => think::CALL_4,
    }
}

/// Force-talk messages (`aMJN_set_force_talk_info`), indexed directly by
/// `talk_idx` (no bounds check in the decomp).
pub const FORCE_TALK_MSG: [u16; 5] = [0x1B3B, 0x3A38, 0x3A42, 0x3A4C, 0x3B05];

pub fn force_talk_msg(talk_idx: u8) -> u16 {
    FORCE_TALK_MSG[(talk_idx % 5) as usize]
}

/// Don's force-talk messages (`aMJN4_set_force_talk_info`).
pub const DON_TALK_MSG: [u16; 2] = [0x250F, 0x2512];

/// Don's forced-reset timers (`aMJN4_force_reset_end_wait_init`).
pub mod don {
    pub const RESET_TIMER: i16 = 180;
    pub const SE_CLEAR_TIMER: i16 = 8;
    pub const FADE_ALPHA_INIT: i32 = 255;
    pub const FADE_STEP: i32 = 8;
}

/// Don's fade step, verbatim: `alpha -= 8; if (alpha < 0) alpha = 0`
/// (reaching 0 advances to the second call state, in C).
pub fn don_fade_step(alpha: i32) -> i32 {
    (alpha - don::FADE_STEP).max(0)
}

/// Good-word check (`aMJN3_check_good_word`): compares the FULL 16-byte
/// reply buffer against the selected good phrase — trailing bytes and
/// padding are part of the comparison.
pub fn check_good_word(reply: &[u8; 16], good_word: &[u8; 16]) -> bool {
    reply == good_word
}

/// Per-index ng-word comparison length. The boundary check runs BEFORE
/// loading word `i`, so: i=0→2, i=1→3, i=2,3→4, i=4..7→5, i=8,9→6,
/// i=10..16→7, i=17..23→8, i=24..27→9, i=28..31→10. The `-1` sentinel
/// can never match (`i >= 0`), so `len` never exceeds 10.
pub fn ng_word_len(i: u8) -> u8 {
    // The decomp checks `i == *chg_length_p` with i ascending from 0,
    // incrementing len before loading the word. Equivalent: count the
    // real boundaries {1,2,4,8,10,17,24,28} that are <= i. The -1
    // sentinel can never match (i >= 0).
    const BOUNDS: [i32; 8] = [1, 2, 4, 8, 10, 17, 24, 28];
    let mut extra = 0u8;
    for b in BOUNDS {
        if (i as i32) >= b {
            extra += 1;
        }
    }
    2 + extra
}

/// Forbidden-word scan (`aMJN3_check_ng_word`), bit-for-bit. For each of
/// the 32 ng words, checks `17 - len` successive offsets of the reply
/// buffer with a `len`-byte equality test; first match returns TRUE.
/// `mem_cmp` returns TRUE on equality, so the comparisons test equality
/// directly.
pub fn check_ng_word(reply: &[u8; 16], ng_words: &[[u8; 16]; 32]) -> bool {
    for i in 0..32usize {
        let len = ng_word_len(i as u8) as usize;
        let word = &ng_words[i];
        // for (j = (16 + 1) - len; j != 0; j--)
        let mut offsets = (word::STR_SIZE + 1) - len;
        let mut reply_p = 0;
        while offsets != 0 {
            if reply[reply_p..reply_p + len] == word[..len] {
                return true;
            }
            reply_p += 1;
            offsets -= 1;
        }
    }
    false
}

/// Word classification (`aMJN3_check_word`): GOOD is checked first, so a
/// reply that is both the exact good phrase and contains an ng word
/// classifies GOOD. Priority: GOOD > NG > WRONG.
pub fn check_word(reply: &[u8; 16], good_word: &[u8; 16], ng_words: &[[u8; 16]; 32]) -> u8 {
    if check_good_word(reply, good_word) {
        word::GOOD
    } else if check_ng_word(reply, ng_words) {
        word::NG
    } else {
        word::WRONG
    }
}

/// Outcome message.
pub fn word_msg(outcome: u8) -> u16 {
    word::MSG[(outcome % 3) as usize]
}

/// Groundhog Day event-actor messages (`ac_ev_majin_move.c_inc`):
/// clear → `0x3DAF + RANDOM(3)`, snow → `0x3DB2 + RANDOM(3)`,
/// rain/default → `0x3DAF` (explicit `default: case RAIN:` fallthrough).
/// Rolls stay in C. Weather values: CLEAR=0, SNOW=2 (see tortimer).
pub fn ev_majin_msg(weather: u8, rand3: u8) -> u16 {
    match weather {
        0 => 0x3DAF + (rand3 % 3) as u16,
        2 => 0x3DB2 + (rand3 % 3) as u16,
        _ => 0x3DAF,
    }
}

/// Reset-demo point-light window (`aRSD_set_point_light`), verbatim:
/// active after 19:00:00 or before 05:00:00 — both comparisons STRICT,
/// so the boundary seconds are excluded.
pub fn point_light_active(now_sec: u32) -> bool {
    now_sec > 19 * 3600 || now_sec < 5 * 3600
}

// ---- C ABI ----
// All pc_-prefixed: the C originals stay compiled; these are staged kernels.

#[no_mangle]
pub extern "C" fn pc_majin_reset_identity(reset_count: u8) -> u16 {
    reset_identity(reset_count)
}

#[no_mangle]
pub extern "C" fn pc_majin_initial_call_state(npc_id: u16) -> u8 {
    initial_call_state(npc_id)
}

#[no_mangle]
pub extern "C" fn pc_majin_force_talk_msg(talk_idx: u8) -> u16 {
    force_talk_msg(talk_idx)
}

#[no_mangle]
pub extern "C" fn pc_majin_ng_word_len(i: u8) -> u8 {
    ng_word_len(i)
}

#[no_mangle]
pub extern "C" fn pc_majin_word_msg(outcome: u8) -> u16 {
    word_msg(outcome)
}

#[no_mangle]
pub extern "C" fn pc_majin_ev_msg(weather: u8, rand3: u8) -> u16 {
    ev_majin_msg(weather, rand3)
}

#[no_mangle]
pub extern "C" fn pc_majin_point_light_active(now_sec: u32) -> u8 {
    point_light_active(now_sec) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identities() {
        assert_eq!(id::MAJIN, 0xD03E);
        assert_eq!(id::MAJIN_BROTHER, 0xD07E);
        assert_eq!(id::EV_MAJIN, 0xD081);
    }

    #[test]
    fn reset_table() {
        assert_eq!(normalize_reset_count(1), 1);
        assert_eq!(normalize_reset_count(8), 8);
        assert_eq!(normalize_reset_count(9), 6);
        assert_eq!(normalize_reset_count(10), 7);
        assert_eq!(normalize_reset_count(11), 8);
        assert_eq!(normalize_reset_count(12), 6);
        assert_eq!(normalize_reset_count(255), 6);
        assert_eq!(reset_identity(1), id::MAJIN_D07C);
        assert_eq!(reset_identity(3), id::MAJIN);
        assert_eq!(reset_identity(5), id::MAJIN_BROTHER);
        assert_eq!(reset_identity(9), id::MAJIN3);
        assert_eq!(reset_identity(10), id::MAJIN4);
        assert_eq!(reset_identity(11), id::MAJIN_D080);
    }

    #[test]
    fn call_states() {
        assert_eq!(initial_call_state(id::MAJIN), think::CALL);
        assert_eq!(initial_call_state(id::MAJIN_D07C), think::CALL_2);
        assert_eq!(initial_call_state(id::MAJIN_D07D), think::CALL_3);
        assert_eq!(initial_call_state(id::MAJIN_BROTHER), think::CALL_4);
        assert_eq!(initial_call_state(id::MAJIN_D080), think::CALL_5);
        assert_eq!(initial_call_state(id::MAJIN2), think::CALL_4); // default
        assert_eq!(initial_call_state(id::MAJIN3), think::CALL_4); // default
    }

    #[test]
    fn talk_messages() {
        assert_eq!(force_talk_msg(0), 0x1B3B);
        assert_eq!(force_talk_msg(4), 0x3B05);
        assert_eq!(DON_TALK_MSG, [0x250F, 0x2512]);
        assert_eq!(word_msg(word::GOOD), 0x23EC);
        assert_eq!(word_msg(word::NG), 0x23EF);
        assert_eq!(word_msg(word::WRONG), 0x23EE);
    }

    #[test]
    fn ng_lengths() {
        assert_eq!(ng_word_len(0), 2);
        assert_eq!(ng_word_len(1), 3);
        assert_eq!(ng_word_len(2), 4);
        assert_eq!(ng_word_len(3), 4);
        assert_eq!(ng_word_len(4), 5);
        assert_eq!(ng_word_len(7), 5);
        assert_eq!(ng_word_len(8), 6);
        assert_eq!(ng_word_len(10), 7);
        assert_eq!(ng_word_len(16), 7);
        assert_eq!(ng_word_len(17), 8);
        assert_eq!(ng_word_len(23), 8);
        assert_eq!(ng_word_len(24), 9);
        assert_eq!(ng_word_len(27), 9);
        assert_eq!(ng_word_len(28), 10);
        assert_eq!(ng_word_len(31), 10);
    }

    #[test]
    fn word_scan() {
        let mut ng = [[0u8; 16]; 32];
        // ng word 0 (len 2): bytes "ab"
        ng[0][0] = b'a';
        ng[0][1] = b'b';
        // reply containing "ab" at offset 3 -> NG
        let mut reply = [0u8; 16];
        reply[3] = b'a';
        reply[4] = b'b';
        assert!(check_ng_word(&reply, &ng));
        // reply without it -> not NG
        let clean = [b'z'; 16];
        assert!(!check_ng_word(&clean, &ng));
        // good-word exact full-buffer match -> GOOD even with ng present
        let good = [b'g'; 16];
        assert!(check_good_word(&good, &good));
        assert_eq!(check_word(&good, &good, &ng), word::GOOD);
        assert_eq!(check_word(&reply, &good, &ng), word::NG);
        assert_eq!(check_word(&clean, &good, &ng), word::WRONG);
        // good-word check is full-buffer: trailing byte differs -> not good
        let mut almost = [b'g'; 16];
        almost[15] = b'h';
        assert!(!check_good_word(&almost, &good));
    }

    #[test]
    fn misc() {
        assert_eq!(ev_majin_msg(0, 2), 0x3DAF + 2);
        assert_eq!(ev_majin_msg(2, 1), 0x3DB2 + 1);
        assert_eq!(ev_majin_msg(1, 2), 0x3DAF); // rain -> default
        assert_eq!(ev_majin_msg(3, 0), 0x3DAF);
        assert!(point_light_active(19 * 3600 + 1));
        assert!(point_light_active(4 * 3600));
        assert!(!point_light_active(19 * 3600)); // strict
        assert!(!point_light_active(5 * 3600)); // strict
        assert!(!point_light_active(12 * 3600));
        assert_eq!(don_fade_step(255), 247);
        assert_eq!(don_fade_step(3), 0);
    }
}
