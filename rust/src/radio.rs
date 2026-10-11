//! Radios and room music: the sports-fair aerobics radio (`ac_radio`), the
//! aerobics radio furniture (`ac_radio_test`), the boom boxes
//! (`ac_sum_radio01/02`) and the music-disk (K.K. song) playback that all
//! stereos share in `ac_my_room` (`aMR_MiniDiskCommon*`,
//! `aMR_RadioCommon*`, `aMR_ChangeMDBgm`, `aMR_Fname2ConpoIndex`).
//!
//! Source-verified against ac-decomp (primary) and ACGC-PC-Port
//! (secondary). Furniture indices and BGM ids were evaluated with mingw32
//! gcc from `m_ftr_def.h` / `audio_defs.h`.
//!
//! ## PC-port changes
//! - `ac_radio` and `ac_radio_test`: the 36-frame effect counters run in
//!   fixed 60 Hz ticks (`graph_dt_60hz_ticks`).
//! - `ac_radio`: despawns only when the player is more than one acre away
//!   (retail: any other acre), for the borderless map.
//!
//! ## Facts
//! - K.K. songs: a music-disk furniture item holds `ITM_MINIDISK_START +
//!   n` and plays `BGM_MD0 (128) + n`; K.K.'s live versions are
//!   `BGM_TOTAKEKE_LIVE0 (192) + n` (see `kk_credits.rs`). 55 discs.
//! - Placing a stereo picks a random song from the house's music box
//!   (`homes[].music_box`, or `island.cottage.music_box` in the player's
//!   cottage). The decomp marks a missing `break` there, but it is
//!   harmless because the counter only matches once.
//! - Entering a house drops songs the music box no longer owns; the
//!   islander's cottage plays `mNpc_GetIslandMDIdx()` on its first stereo.
//! - Only one stereo plays at a time. Switching one on switches the others
//!   off and reserves its BGM, which is applied when the timer reaches 0,
//!   no message window is up, and the room is not a shop. Stereos (not the
//!   aerobics radio) also feed a "player position" voice so the music is
//!   positional; each model has its own speaker preset (`Conpo` index,
//!   1-16).
//! - The aerobics radio furniture plays `BGM_SPORTSFAIR_AEROBICS` (27) and
//!   puffs a KONPU effect every 36 frames, 3 units down, angled -22.5 deg.
//! - The sports-fair radio actor leaves `DUMMY_RADIO` (0xF109) on the
//!   ground, raises collision 3 units (`offs == 0 ? 3 : 3`), and makes a
//!   note effect (32) every 36 frames at offset (2, 0, -10).
//!
//! ## Quirks
//! - `aMR_ChangeMDBgm` stopping music always deletes the player-position
//!   voice, even if the stopped music was the aerobics track that never
//!   created one.
//! - `aMR_MiniDiskCommonDt` deletes `md_no` (the *reserved* song), not
//!   `last_md_no`.

pub const MINIDISK_NUM: u16 = 55;
pub const ITM_MINIDISK_START: u16 = 0x2A00;
pub const BGM_MD0: i32 = 128;
pub const BGM_SPORTSFAIR_AEROBICS: i32 = 27;
pub const BGM_TOTAKEKE_LIVE0: i32 = 192;
pub const DUMMY_RADIO: u16 = 0xF109;
/// `AEROBICS_RADIO` = STRUCTURE_START + 44.
pub const AEROBICS_RADIO: u16 = 0x5800 + 44;

pub const RADIO_EFFECT_PERIOD: i16 = 36;
pub const RADIO_NOTE_OFFSET: [f32; 3] = [2.0, 0.0, -10.0];
/// Effect id `32` (music note) with angle 0x6000.
pub const RADIO_NOTE_EFFECT: i32 = 32;
pub const RADIO_NOTE_ANGLE: i16 = 0x6000;
/// `DEG2SHORT_ANGLE(337.5f)` (gcc: -4096) added to the furniture angle.
pub const RADIO_TEST_EFFECT_ANGLE: i16 = -4096;
pub const RADIO_TEST_EFFECT_DROP: f32 = 3.0;
pub const RADIO_BG_OFFSET: i32 = 3;

/// FTR indices (gcc-evaluated) -> Conpo speaker preset.
pub const CONPO_TABLE: [(u16, u16); 16] = [
    (85, 1),   // FTR_SUM_STEREO01 retro stereo
    (894, 2),  // FTR_SUM_CHIKUON01 phonograph
    (895, 3),  // FTR_SUM_CHIKUON02 turntable
    (896, 4),  // FTR_SUM_JUKEBOX jukebox
    (901, 5),  // FTR_SUM_LV_STEREO lovely stereo
    (897, 6),  // FTR_SUM_RADIO01 red boom box
    (898, 7),  // FTR_SUM_RADIO02 white boom box
    (918, 8),  // FTR_SUM_CASSE01 tape deck
    (917, 9),  // FTR_SUM_RECO01 reel-to-reel
    (121, 10), // FTR_SUM_CONPO01 gold stereo
    (899, 11), // FTR_SUM_CONPO02 high-end stereo
    (900, 12), // FTR_SUM_STEREO02 hi-fi stereo
    (911, 13), // FTR_SUM_ROBOCONPO robo-stereo
    (912, 14), // FTR_SUM_SAICONPO dice stereo
    (919, 15), // FTR_SUM_MD01 CD player
    (948, 16), // FTR_NOG_TRI_AUDIO01 kiddie stereo
];
pub const FTR_RADIO_TEST: u16 = 1011;

/// `aMR_Fname2ConpoIndex` (default 1).
pub fn conpo_index(ftr_name: u16) -> u16 {
    CONPO_TABLE.iter().find(|e| e.0 == ftr_name).map_or(1, |e| e.1)
}

/// BGM for a music-disk item, if it is one.
pub fn md_bgm(item: u16) -> Option<i32> {
    let idx = item.checked_sub(ITM_MINIDISK_START)?;
    (idx < MINIDISK_NUM).then_some(BGM_MD0 + idx as i32)
}

fn music_box_has(music_box: &[u32; 2], i: u16) -> bool {
    (music_box[((i / 32) & 1) as usize] >> (i & 31)) & 1 != 0
}

/// `aMR_MiniDiskCommonCt` song pick; `random` = `RANDOM(count)`.
pub fn pick_placed_song(music_box: &[u32; 2], random: impl FnOnce(u32) -> u32) -> Option<u16> {
    let owned: Vec<u16> = (0..MINIDISK_NUM).filter(|&i| music_box_has(music_box, i)).collect();
    if owned.is_empty() {
        return None;
    }
    let sel = random(owned.len() as u32) as usize;
    owned.get(sel).map(|&i| ITM_MINIDISK_START + i)
}

/// `aMR_DeleteMusicWhichMusicBoxDontHave`: true if the disk must be removed.
pub fn song_not_owned(music_box: &[u32; 2], item: u16) -> bool {
    match item.checked_sub(ITM_MINIDISK_START) {
        Some(i) if i < MINIDISK_NUM => !music_box_has(music_box, i),
        _ => false,
    }
}

/// Sound-system call produced by the BGM state machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BgmOp {
    MakeRoom(i32),
    DeleteRoom(i32),
    PlayerPosMake,
    PlayerPosDelete,
}

/// `my_room->bgm_info`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BgmInfo {
    pub reserve_flag: bool,
    pub active_flag: bool,
    pub md_no: i32,
    pub last_md_no: i32,
    pub timer: i16,
    /// Furniture slot of the playing stereo (`active_ftr_actor`).
    pub active_ftr: Option<usize>,
    pub reserved_ftr: Option<usize>,
}

impl Default for BgmInfo {
    fn default() -> Self {
        Self { reserve_flag: false, active_flag: false, md_no: -1, last_md_no: -1, timer: 0, active_ftr: None, reserved_ftr: None }
    }
}

impl BgmInfo {
    /// `aMR_ReserveBgm`.
    pub fn reserve(&mut self, bgm: i32, ftr: usize, timer: i16) {
        self.reserve_flag = true;
        self.timer = timer;
        self.md_no = bgm;
        self.reserved_ftr = Some(ftr);
    }

    /// `aMR_ReserveDefaultBgm` (stop).
    pub fn reserve_default(&mut self, ftr: usize) {
        self.reserve(-1, ftr, 0);
    }

    /// `aMR_ChangeMDBgm`.
    pub fn change(&mut self, ftr: usize, ops: &mut Vec<BgmOp>) {
        if !self.reserve_flag {
            return;
        }
        if !self.active_flag {
            if self.md_no != -1 {
                ops.push(BgmOp::MakeRoom(self.md_no));
                if self.md_no != BGM_SPORTSFAIR_AEROBICS {
                    ops.push(BgmOp::PlayerPosMake);
                }
                self.active_flag = true;
                self.active_ftr = Some(ftr);
            }
        } else if self.md_no == -1 {
            ops.push(BgmOp::DeleteRoom(self.last_md_no));
            ops.push(BgmOp::PlayerPosDelete);
            self.active_flag = false;
            self.active_ftr = None;
        } else {
            ops.push(BgmOp::DeleteRoom(self.last_md_no));
            if self.last_md_no != BGM_SPORTSFAIR_AEROBICS {
                ops.push(BgmOp::PlayerPosDelete);
            }
            ops.push(BgmOp::MakeRoom(self.md_no));
            if self.md_no != BGM_SPORTSFAIR_AEROBICS {
                ops.push(BgmOp::PlayerPosMake);
            }
            self.active_flag = true;
            self.active_ftr = Some(ftr);
        }
        self.reserve_flag = false;
        self.last_md_no = self.md_no;
    }

    /// Per-frame timer + deferred apply in `My_Room_Actor_move`.
    pub fn frame(&mut self, is_shop: bool, msg_hidden: bool, ops: &mut Vec<BgmOp>) {
        self.timer = (self.timer - 1).max(0);
        if !is_shop && self.timer == 0 && self.reserve_flag && msg_hidden {
            if let Some(f) = self.reserved_ftr {
                self.change(f, ops);
            }
        }
    }

    /// `aMR_MiniDiskCommonDt` for a playing stereo.
    pub fn stereo_removed(&mut self, ops: &mut Vec<BgmOp>) {
        ops.push(BgmOp::DeleteRoom(self.md_no));
        self.active_flag = false;
        self.active_ftr = None;
        self.last_md_no = -1;
        if self.md_no != BGM_SPORTSFAIR_AEROBICS {
            ops.push(BgmOp::PlayerPosDelete);
        }
    }
}

/// Switch state of a furniture radio/stereo.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FtrSwitch {
    pub switch_bit: bool,
    pub switch_changed: bool,
    /// 1 = start playing on the next move (`haniwa_state`).
    pub haniwa_state: u8,
}

/// `aMR_MiniDiskCommonMove` BGM part: a stereo that was on when the room
/// loaded reserves its song.
pub fn minidisk_move(sw: &mut FtrSwitch, bgm: &mut BgmInfo, ftr: usize, item: u16) {
    if sw.haniwa_state == 1 {
        bgm.reserve(BGM_MD0 + item.wrapping_sub(ITM_MINIDISK_START) as i32, ftr, 0);
        sw.haniwa_state = 0;
    }
}

/// `aMR_RadioCommonMove` (aerobics radio furniture). The caller must also
/// switch every other stereo off when this returns true
/// (`aMR_OneMDSwitchOn_TheOtherSwitchOff`).
pub fn radio_common_move(sw: &mut FtrSwitch, bgm: &mut BgmInfo, ftr: usize, ops: &mut Vec<BgmOp>) -> bool {
    if sw.haniwa_state == 1 {
        bgm.reserve(BGM_SPORTSFAIR_AEROBICS, ftr, 0);
        sw.haniwa_state = 0;
        false
    } else if sw.switch_changed {
        if sw.switch_bit {
            bgm.reserve(BGM_SPORTSFAIR_AEROBICS, ftr, 0);
        } else {
            bgm.reserve_default(ftr);
        }
        bgm.change(ftr, ops);
        true
    } else {
        false
    }
}

/// 36-frame effect counter shared by `ac_radio` and `ac_radio_test`
/// (retail: one tick per frame). Returns how many effects to spawn.
pub fn effect_ticks(counter: &mut i16, ticks: u32) -> u32 {
    let mut fired = 0;
    for _ in 0..ticks {
        if *counter >= RADIO_EFFECT_PERIOD {
            *counter = 0;
            fired += 1;
        }
        *counter += 1;
    }
    fired
}

/// `aRAD_actor_move` despawn rule (scroll demos excepted by the caller).
pub fn radio_despawns(radio_block: (i32, i32), player_block: (i32, i32), pc_borderless: bool) -> bool {
    let (dx, dz) = (radio_block.0 - player_block.0, radio_block.1 - player_block.1);
    if pc_borderless {
        dx.abs() > 1 || dz.abs() > 1
    } else {
        dx != 0 || dz != 0
    }
}

/// `aMR_MakeClockInfo`: (stop, tick0, tick1) for the room clocks on a
/// 120-frame cycle.
pub fn clock_ticks(game_frame: u32) -> (bool, bool, bool) {
    match game_frame % 120 {
        0 => (true, false, false),
        15 => (false, true, false),
        45 => (false, false, true),
        _ => (false, false, false),
    }
}

/// C ABI: Conpo speaker preset for a furniture index.
#[no_mangle]
pub extern "C" fn pc_mr_conpo_index(ftr_name: u16) -> u16 {
    conpo_index(ftr_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn songs() {
        assert_eq!(md_bgm(0x2A00), Some(128));
        assert_eq!(md_bgm(0x2A00 + 55), None);
        let mb = [0b1010u32, 1 << 22];
        assert_eq!(pick_placed_song(&mb, |n| { assert_eq!(n, 3); 2 }), Some(0x2A00 + 54));
        assert_eq!(pick_placed_song(&[0, 0], |_| 0), None);
        assert!(song_not_owned(&mb, 0x2A00));
        assert!(!song_not_owned(&mb, 0x2A01));
        assert_eq!(conpo_index(897), 6);
        assert_eq!(conpo_index(1), 1);
    }

    #[test]
    fn bgm_switching() {
        let mut b = BgmInfo::default();
        let mut ops = Vec::new();
        b.reserve(BGM_MD0 + 3, 0, 0);
        b.change(0, &mut ops);
        assert_eq!(ops, [BgmOp::MakeRoom(131), BgmOp::PlayerPosMake]);
        ops.clear();
        let mut sw = FtrSwitch { switch_bit: true, switch_changed: true, haniwa_state: 0 };
        assert!(radio_common_move(&mut sw, &mut b, 1, &mut ops));
        assert_eq!(ops, [BgmOp::DeleteRoom(131), BgmOp::PlayerPosDelete, BgmOp::MakeRoom(27)]);
        ops.clear();
        sw.switch_bit = false;
        radio_common_move(&mut sw, &mut b, 1, &mut ops);
        // Quirk: deletes the position voice the aerobics track never made.
        assert_eq!(ops, [BgmOp::DeleteRoom(27), BgmOp::PlayerPosDelete]);
        assert!(!b.active_flag);
    }

    #[test]
    fn timers() {
        let mut c = 0;
        let fired: u32 = (0..74).map(|_| effect_ticks(&mut c, 1)).sum();
        assert_eq!(fired, 2);
        assert!(radio_despawns((1, 1), (2, 1), false));
        assert!(!radio_despawns((1, 1), (2, 1), true));
        assert_eq!(clock_ticks(135), (false, true, false));
    }
}
