//! K.K. Slider (internal name Totakeke) — pure data and kernels.
//!
//! Verified against `src/game/m_event.c`,
//! `src/game/m_event_schedule.c_inc`, `src/actor/ac_event_manager.c`,
//! `src/actor/npc/ac_npc_totakeke_talk.c_inc`,
//! `src/actor/npc/ac_npc_totakeke_think.c_inc`,
//! `src/actor/ac_mikanbox_clip.c_inc`, `include/m_name_table.h`,
//! `include/ac_npc_totakeke.h`, `include/m_private.h`,
//! `include/audio_defs.h` (GAFE01_00 Rev 0 USA decomp).
//! All 12 brief claims checked verbatim; no corrections.
//!
//! Architecture: the actor, the weekly event scheduler, the event
//! manager, the talk/think state machines, the mikanbox clip, the
//! inventory/aircheck save writes, RNG, and the staff-roll audio and
//! weather side effects all stay in C. This module holds only the
//! source-proven pure data and calculations with primitive arguments.
//!
//! The headline kernel is the collection selector: `count_uncollected`
//! + `select_song` reimplement `aMKBC_count_merody` /
//! `aMKBC_clip_search_merody` bit-for-bit, given the two collection
//! words and the already-generated random index (RNG stays in C).

/// K.K. identities (`include/m_name_table.h`). Shared metadata
/// `{ mPr_SEX_OTHER, 0x214, 2 }` does not make them interchangeable.
pub mod id {
    /// The Saturday performance actor.
    pub const TOTAKEKE: u16 = 0xD05D;
    pub const P_SEL: u16 = 0xD018;
    pub const RTC: u16 = 0xD066;
}

/// Event save flags (`ac_npc_totakeke_talk.c_inc`).
pub mod flag {
    pub const HELLO: u16 = 1 << 0;
    pub const AIRCHECK_RECEIVED: u16 = 1 << 4;
    pub const SP_DELETE_MAJIN: u16 = 1 << 13;
    pub const SP_ROLL_DRAW: u16 = 1 << 14;
    pub const SP_ROLL_END: u16 = 1 << 15;
}

/// Think states (`include/ac_npc_totakeke.h`).
pub mod think {
    pub const NONE: u8 = 0;
    pub const WAIT: u8 = 1;
    pub const ROLL: u8 = 2;
    pub const ROLL1: u8 = 3;
    pub const ROLL2: u8 = 4;
    pub const ROLL4: u8 = 5;
    pub const ROLL_END: u8 = 6;
    pub const _7: u8 = 7;
    pub const _8: u8 = 8;
    pub const _9: u8 = 9;
    pub const GIVE_MERODY: u8 = 10;
    pub const POCKETS_FULL: u8 = 11;
    pub const GIVE_LATER: u8 = 12;
}

/// Schedule and placement (`m_event_schedule.c_inc`, `staffroll_start`).
pub mod schedule {
    /// Saturday, 20:00–23:00.
    pub const WEEKDAY: u8 = 6;
    pub const START_HOUR: u8 = 20;
    pub const END_HOUR: u8 = 23;
    /// Fixed placement: block {1,3} (A-3), unit {7,7}, actor arg 0x51.
    pub const BLOCK: (u8, u8) = (1, 3);
    pub const UNIT: (u8, u8) = (7, 7);
    pub const ACTOR_ARG: u8 = 0x51;
}

/// Song collection (`ac_mikanbox_clip.c_inc`, `m_name_table.h`).
pub mod collection {
    /// `ITM_MINIDISK_START`.
    pub const MINIDISK_START: u16 = 0x2A00;
    /// `ITM_MINIDISK_END` = START + 55.
    pub const MINIDISK_END: u16 = 0x2A37;
    /// `ITM_MINIDISK52` = START + 52; the selector only covers 52 songs.
    pub const MINIDISK52: u16 = 0x2A34;
    /// `aMKBC_NUM_GOOD_MD` = 52.
    pub const NUM_GOOD_MD: u8 = 52;
    /// The bitmap is exactly two u32 words
    /// (`m_private.h`: `u32 aircheck_collect_bitfield[2]`).
    pub const WORDS: usize = 2;
}

/// Greeting message base (`MSG_TOTAKEKE_START`).
pub const MSG_START: u16 = 0x1B93;

/// Live BGM base (`BGM_TOTAKEKE_LIVE0`, `audio_defs.h` = 192).
pub const BGM_LIVE0: u16 = 192;

/// Greeting message: `MSG_TOTAKEKE_START + msg[talk]` with
/// `msg[4] = { 0x0, 0x1, 0x2, 0xd }`, indexed directly by talk state.
pub fn greet_msg(talk: u8) -> u16 {
    const MSG: [u8; 4] = [0x0, 0x1, 0x2, 0xd];
    MSG_START + MSG[(talk & 3) as usize] as u16
}

/// Reroll acceptance (`aNTT_talk_select1`): retry while the song equals
/// the previous one, up to 3 retries. The retail loop permits the final
/// song to equal `_99f` if all three retries also match — do not replace
/// with a "pick a different song" algorithm.
pub fn needs_reroll(song: u8, prev_song: u8, retries_used: u8) -> bool {
    retries_used < 3 && song == prev_song
}

/// Not-understood request branch: `song = 0x37 + (int)RANDOM_F(3)`
/// (roll stays in C, passed in); `post_roll_think = 0xc` stays in C.
pub fn request_fallback_song(rand3: u8) -> u8 {
    0x37 + (rand3 % 3)
}

/// Requested item → song index: `item - ITM_MINIDISK_START`.
pub fn request_song_index(item: u16) -> u8 {
    (item - collection::MINIDISK_START) as u8
}

/// The continuation differs when `song >= 0x34`.
pub fn request_is_long(song: u8) -> bool {
    song >= 0x34
}

/// Live BGM: `BGM_TOTAKEKE_LIVE0 + song`.
pub fn live_bgm(song: u8) -> u16 {
    BGM_LIVE0 + song as u16
}

/// Count uncollected songs (`aMKBC_count_merody`), verbatim: walks the
/// two bitmap words over `MINIDISK_START..MINIDISK52`, counting CLEAR
/// bits. A clear bit = uncollected.
pub fn count_uncollected(word0: u32, word1: u32) -> u32 {
    let words = [word0, word1];
    let mut j = collection::MINIDISK_START;
    let mut count = 0u32;
    for i in 0..collection::WORDS {
        let mut bit = 1u32;
        let mut k = 0;
        while j < collection::MINIDISK52 && k < 32 {
            if (bit & words[i]) == 0 {
                count += 1;
            }
            bit <<= 1;
            j += 1;
            k += 1;
        }
    }
    count
}

/// Select a song (`aMKBC_clip_search_merody`), verbatim, given the
/// already-generated random index (the single `RANDOM(count)` stays in
/// C). Walks the range in retail order, decrementing the index only on
/// uncollected entries; returns the zero-based song index.
///
/// The all-collected fallback (clear both words, count = 52) is a save
/// mutation and stays in C — the caller must apply it before calling
/// this with the fresh words.
pub fn select_song(word0: u32, word1: u32, rand_idx: u32) -> u8 {
    let words = [word0, word1];
    let mut j = collection::MINIDISK_START;
    let mut count = rand_idx;
    for i in 0..collection::WORDS {
        let mut bit = 1u32;
        let mut k = 0;
        while j < collection::MINIDISK52 && k < 32 {
            if (bit & words[i]) == 0 {
                if count > 0 {
                    count -= 1;
                } else {
                    return (i as u8) * 32 + k as u8;
                }
            }
            bit <<= 1;
            j += 1;
            k += 1;
        }
    }
    0
}

/// Mark a song collected (`aMKBC_clip_check_merody`), verbatim:
/// `md_no < 52` → word0 bit `md_no`, or word1 bit `md_no - 32`.
/// Returns the updated words.
pub fn collect(word0: u32, word1: u32, md_no: u8) -> (u32, u32) {
    if md_no < collection::NUM_GOOD_MD {
        if md_no < 32 {
            (word0 | (1 << md_no), word1)
        } else {
            (word0, word1 | (1 << (md_no - 32)))
        }
    } else {
        (word0, word1)
    }
}

// ---- C ABI ----
// All pc_-prefixed: the C originals stay compiled; these are staged kernels.

#[no_mangle]
pub extern "C" fn pc_totakeke_greet_msg(talk: u8) -> u16 {
    greet_msg(talk)
}

#[no_mangle]
pub extern "C" fn pc_totakeke_needs_reroll(song: u8, prev_song: u8, retries_used: u8) -> u8 {
    needs_reroll(song, prev_song, retries_used) as u8
}

#[no_mangle]
pub extern "C" fn pc_totakeke_request_fallback_song(rand3: u8) -> u8 {
    request_fallback_song(rand3)
}

#[no_mangle]
pub extern "C" fn pc_totakeke_request_song_index(item: u16) -> u8 {
    request_song_index(item)
}

#[no_mangle]
pub extern "C" fn pc_totakeke_live_bgm(song: u8) -> u16 {
    live_bgm(song)
}

#[no_mangle]
pub extern "C" fn pc_totakeke_count_uncollected(word0: u32, word1: u32) -> u32 {
    count_uncollected(word0, word1)
}

#[no_mangle]
pub extern "C" fn pc_totakeke_select_song(word0: u32, word1: u32, rand_idx: u32) -> u8 {
    select_song(word0, word1, rand_idx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identities() {
        assert_eq!(id::TOTAKEKE, 0xD05D);
        assert_eq!(id::P_SEL, 0xD018);
        assert_eq!(id::RTC, 0xD066);
    }

    #[test]
    fn greeting_and_bgm() {
        assert_eq!(greet_msg(0), 0x1B93);
        assert_eq!(greet_msg(3), 0x1B93 + 0xD);
        assert_eq!(live_bgm(0), 192);
        assert_eq!(live_bgm(51), 243);
    }

    #[test]
    fn request_path() {
        assert_eq!(request_fallback_song(2), 0x39);
        assert_eq!(request_song_index(0x2A05), 5);
        assert!(!request_is_long(0x33));
        assert!(request_is_long(0x34));
        assert!(needs_reroll(7, 7, 0));
        assert!(needs_reroll(7, 7, 2));
        assert!(!needs_reroll(7, 7, 3)); // retries exhausted
        assert!(!needs_reroll(7, 8, 0));
    }

    #[test]
    fn collection() {
        // nothing collected: 52 uncollected
        assert_eq!(count_uncollected(0, 0), 52);
        // all 52 collected
        assert_eq!(count_uncollected(0xFFFFFFFF, 0x000FFFFF), 52 - 52);
        // one collected (song 0)
        assert_eq!(count_uncollected(1, 0), 51);
        // select: first uncollected with idx 0
        assert_eq!(select_song(0, 0, 0), 0);
        // select: idx 5 -> song 5
        assert_eq!(select_song(0, 0, 5), 5);
        // select: song 0 collected, idx 0 -> song 1
        assert_eq!(select_song(1, 0, 0), 1);
        // select: songs 0..=31 collected, idx 0 -> song 32
        assert_eq!(select_song(0xFFFFFFFF, 0, 0), 32);
        // collect: song 5 -> word0 bit 5
        assert_eq!(collect(0, 0, 5), (1 << 5, 0));
        // collect: song 40 -> word1 bit 8
        assert_eq!(collect(0, 0, 40), (0, 1 << 8));
        // collect: song 52+ ignored
        assert_eq!(collect(0, 0, 52), (0, 0));
        // bit-for-bit: collect then count
        let (w0, w1) = collect(0, 0, 51);
        assert_eq!(count_uncollected(w0, w1), 51);
        assert_eq!(select_song(w0, w1, 50), 50);
    }

    #[test]
    fn flags_and_think_states() {
        // Event save flags.
        assert_eq!(flag::HELLO, 1 << 0);
        assert_eq!(flag::AIRCHECK_RECEIVED, 1 << 4);
        assert_eq!(flag::SP_DELETE_MAJIN, 1 << 13);
        assert_eq!(flag::SP_ROLL_DRAW, 1 << 14);
        assert_eq!(flag::SP_ROLL_END, 1 << 15);
        // Think states are a dense 0-12 sequence.
        assert_eq!(think::NONE, 0);
        assert_eq!(think::WAIT, 1);
        assert_eq!(think::ROLL, 2);
        assert_eq!(think::GIVE_MERODY, 10);
        assert_eq!(think::POCKETS_FULL, 11);
        assert_eq!(think::GIVE_LATER, 12);
        // Schedule and placement.
        assert_eq!(schedule::WEEKDAY, 6);
        assert_eq!(schedule::START_HOUR, 20);
        assert_eq!(schedule::END_HOUR, 23);
        assert_eq!(schedule::BLOCK, (1, 3));
        assert_eq!(schedule::UNIT, (7, 7));
        assert_eq!(schedule::ACTOR_ARG, 0x51);
        // Collection bounds.
        assert_eq!(collection::MINIDISK_START, 0x2A00);
        assert_eq!(collection::MINIDISK_END, 0x2A37);
        assert_eq!(collection::MINIDISK52, 0x2A34);
        assert_eq!(collection::NUM_GOOD_MD, 52);
        assert_eq!(collection::WORDS, 2);
    }

    #[test]
    fn song_kernels_edges() {
        // Greeting talk wraps with & 3.
        assert_eq!(greet_msg(4), greet_msg(0));
        assert_eq!(greet_msg(5), MSG_START + 0x1);
        // Fallback song roll folds mod 3.
        assert_eq!(request_fallback_song(3), 0x37);
        assert_eq!(request_fallback_song(5), 0x39);
        // Long-song boundary at 0x34.
        assert!(!request_is_long(0x33));
        assert!(request_is_long(0x34));
        // Reroll: different songs never reroll; 3 retries max.
        assert!(!needs_reroll(7, 8, 2));
        assert!(!needs_reroll(7, 7, 3));
        // collect song 51 sets word1 bit 19.
        let (w0, w1) = collect(0, 0, 51);
        assert_eq!((w0, w1), (0, 1 << 19));
        // Round trip: collect 0..=31 then select idx 0 -> song 32.
        let mut words = (0u32, 0u32);
        for s in 0..32u8 {
            words = collect(words.0, words.1, s);
        }
        assert_eq!(count_uncollected(words.0, words.1), 20);
        assert_eq!(select_song(words.0, words.1, 0), 32);
        assert_eq!(select_song(words.0, words.1, 19), 51);
    }

}
