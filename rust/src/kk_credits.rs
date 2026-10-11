//! K.K. Slider's Saturday show helpers (`ac_mikanbox` + `_clip.c_inc`):
//! the orange-crate stage prop doubles as the clip that runs K.K.'s
//! concert: the text roll drawn over the show, song picking from the
//! aircheck collection bits, the show timer and the hidden 20:45-20:55
//! cameo of `SP_NPC_MAJIN5` (0xD06B).
//!
//! Source-verified against ac-decomp `src/actor/ac_mikanbox_clip.c_inc`
//! (primary) and ACGC-PC-Port (secondary). `FRAMERATE_TIMER` /
//! `FRAMERATE_SELECT` come from `include/types.h`: 60 Hz builds use the
//! first value, 50 Hz (PAL/AUS) the second or n*50/60.
//!
//! ## Facts
//! - The roll text comes from strings `mString_MIKANBOX_START`
//!   (0x77B)..`END` (0x7FE), ten lines per page; `PAGE_TABLE` gives each
//!   page's first line, `INDEX_LINES` lines are drawn at 0.85x (0.9 * 0.85).
//! - Page alpha: 0 before frame 40 and after 446 (50 Hz: 372); fades in
//!   at 3.1875/frame until 120, fades out after 366 (292).
//! - Song choice: random among the first 52 songs (`ITM_MINIDISK_START`
//!   0x2A00 .. `ITM_MINIDISK52`) the player has *not* yet received; once all
//!   are collected the bits reset. The BGM is `BGM_TOTAKEKE_LIVE0 + song`.
//! - Show length 4800 frames (50 Hz: 4000); at the end any cameo NPC is
//!   flagged for deletion and the save timer field set to 120 (550 while
//!   the cameo is present).
//! - Cameo: if the clock reads 20:45:00-20:55:00 inclusive and the show
//!   timer is in [3600, 3610) (50 Hz: [3000, 3010)), `SP_NPC_MAJIN5` is
//!   spawned at the stage's unit.
//! - K.K.'s strum frame follows the staff-roll beat: 48 * pct for
//!   3-beat songs, 64 * pct for 4-beat songs.

pub const STRING_START: i32 = 0x77B;
pub const STRING_END: i32 = 0x7FE;
pub const LINES_PER_PAGE: usize = 10;
pub const NUM_GOOD_MD: u32 = 52;
pub const MINIDISK_START: u16 = 0x2A00;
pub const SP_NPC_MAJIN5: u16 = 0xD06B;

pub const PAGE_TABLE: [u8; 17] = [0, 9, 18, 27, 37, 45, 53, 57, 66, 74, 82, 89, 96, 103, 112, 122, 124];
pub const INDEX_LINES: [u8; 32] = [
    3, 6, 9, 12, 16, 18, 21, 25, 27, 30, 37, 40, 43, 45, 48, 51, 53, 57, 60, 66, 74, 82, 86, 89, 91, 96, 100, 103, 112,
    116, 119, 122,
];

/// `(f60, f50)` pick for the build's frame rate.
fn sel(pal50: bool, f60: i32, f50: i32) -> i32 {
    if pal50 {
        f50
    } else {
        f60
    }
}

/// `FRAMERATE_TIMER(n)`.
pub fn frame_timer(pal50: bool, n: i32) -> i32 {
    if pal50 {
        n * 50 / 60
    } else {
        n
    }
}

/// Text-roll alpha for the page timer.
pub fn roll_alpha(timer: i32, pal50: bool) -> u8 {
    let end = sel(pal50, 446, 372);
    if !(40..=end).contains(&timer) {
        0
    } else if timer < 120 {
        ((timer - 40) as f32 * 3.1875) as u8
    } else if timer > sel(pal50, 366, 292) {
        ((end - timer) as f32 * 3.1875) as u8
    } else {
        255
    }
}

/// String ids loaded for a page (`aMKBC_clip_set_string`, clamped at END).
pub fn page_strings(page: usize) -> [i32; LINES_PER_PAGE] {
    let mut id = STRING_START + PAGE_TABLE[page.min(PAGE_TABLE.len() - 1)] as i32;
    let mut out = [0; LINES_PER_PAGE];
    for slot in out.iter_mut() {
        *slot = id;
        id = (id + 1).min(STRING_END);
    }
    out
}

/// Draw scale of roll line `idx`.
pub fn line_scale(idx: u8) -> f32 {
    let hits = INDEX_LINES.iter().filter(|&&l| l == idx).count();
    0.9 * 0.85f32.powi(hits as i32)
}

/// First drawn y of a page (`30 + ((start + 10) - end) * 10`, 18 px per line).
pub fn page_top_y(page: usize) -> f32 {
    let start = PAGE_TABLE[page] as i32;
    let end = PAGE_TABLE[page + 1] as i32;
    30.0 + ((start + 10) - end) as f32 * 10.0
}

fn is_collected(bits: &[u32; 2], md: u32) -> bool {
    bits[(md / 32) as usize] & (1 << (md % 32)) != 0
}

/// `aMKBC_count_merody`: songs among the first 52 not yet collected.
pub fn count_uncollected(bits: &[u32; 2]) -> u32 {
    (0..NUM_GOOD_MD).filter(|&m| !is_collected(bits, m)).count() as u32
}

/// `aMKBC_clip_search_merody`; `random` = `RANDOM(count)` draw given the
/// count (callers pass a closure over `fqrand`). Resets the bits when
/// every song is collected.
pub fn pick_song(bits: &mut [u32; 2], random: impl FnOnce(u32) -> u32) -> u32 {
    let mut count = count_uncollected(bits);
    if count == 0 {
        count = NUM_GOOD_MD;
        *bits = [0, 0];
    }
    let mut n = random(count);
    for m in 0..NUM_GOOD_MD {
        if !is_collected(bits, m) {
            if n == 0 {
                return m;
            }
            n -= 1;
        }
    }
    0
}

/// `aMKBC_clip_check_merody`.
pub fn mark_song(bits: &mut [u32; 2], md: u32) {
    if md < NUM_GOOD_MD {
        bits[(md / 32) as usize] |= 1 << (md % 32);
    }
}

/// What the per-frame show timer step wants done.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShowEvent {
    None,
    SpawnCameo,
    End,
}

/// Show timer step after the increment (`aMKBC_clip_timer_proc`).
pub fn show_event(show_timer: i32, now_sec: i32, cameo_present: bool, pal50: bool) -> ShowEvent {
    if show_timer >= frame_timer(pal50, 4800) {
        ShowEvent::End
    } else if (20 * 3600 + 45 * 60..=20 * 3600 + 55 * 60).contains(&now_sec)
        && !cameo_present
        && show_timer >= frame_timer(pal50, 3600)
        && show_timer < sel(pal50, 3610, 3010)
    {
        ShowEvent::SpawnCameo
    } else {
        ShowEvent::None
    }
}

/// K.K.'s guitar frame from the staff-roll beat percentage.
pub fn strum_frame(four_beat: bool, percent: f32) -> f32 {
    if four_beat {
        64.0 * percent
    } else {
        48.0 * percent
    }
}

/// C ABI: credits roll alpha (`pal50` nonzero on 50 Hz builds).
#[no_mangle]
pub extern "C" fn pc_mkbc_roll_alpha(timer: i32, pal50: i32) -> u8 {
    roll_alpha(timer, pal50 != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alpha_envelope() {
        assert_eq!(roll_alpha(39, false), 0);
        assert_eq!(roll_alpha(120, false), 255);
        assert_eq!(roll_alpha(80, false), 127);
        assert_eq!(roll_alpha(447, false), 0);
        assert_eq!(roll_alpha(373, true), 0);
    }

    #[test]
    fn pages() {
        assert_eq!(page_strings(0)[0], 0x77B);
        assert_eq!(*page_strings(15).last().unwrap(), 0x77B + 122 + 9);
        assert!((line_scale(3) - 0.765).abs() < 1e-6);
        assert_eq!(line_scale(4), 0.9);
        assert_eq!(page_top_y(0), 40.0);
    }

    #[test]
    fn songs() {
        let mut bits = [0u32; 2];
        assert_eq!(count_uncollected(&bits), 52);
        assert_eq!(pick_song(&mut bits, |_| 0), 0);
        mark_song(&mut bits, 0);
        assert_eq!(pick_song(&mut bits, |_| 0), 1);
        mark_song(&mut bits, 40);
        assert_eq!(bits[1], 1 << 8);
        let mut full = [u32::MAX, (1 << 20) - 1];
        assert_eq!(count_uncollected(&full), 0);
        assert_eq!(pick_song(&mut full, |c| c - 1), 51);
        assert_eq!(full, [0, 0]);
    }

    #[test]
    fn show_timer() {
        let t = 20 * 3600 + 50 * 60;
        assert_eq!(show_event(3605, t, false, false), ShowEvent::SpawnCameo);
        assert_eq!(show_event(3610, t, false, false), ShowEvent::None);
        assert_eq!(show_event(3005, t, false, true), ShowEvent::SpawnCameo);
        assert_eq!(show_event(4800, t, true, false), ShowEvent::End);
        assert_eq!(show_event(3605, 21 * 3600, false, false), ShowEvent::None);
    }
}
