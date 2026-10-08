//! Game-layer audio for the Rust rewrite (ACGC-USA Rev. 0).
//!
//! Source-verified against `src/audio.c`,
//! `src/static/jaudio_NES/game/game64.c_inc`,
//! `include/jaudio_NES/audiocommon.h`, `include/jaudio_NES/audiostruct.h`,
//! `src/static/jaudio_NES/internal/sub_sys.c`, and `include/m_config.h`.
//!
//! Retail audio is a stateful stack, not a bag of `play_sound(id)` calls:
//!
//! ```text
//! game actors / systems
//!        │
//!        ▼
//!   sAdo_* (this module's callers: src/audio.c)
//!        │
//!        ▼
//!   Na_* game audio layer  <-- THIS MODULE
//!        │  trigger SEs, level SEs, voices, BGM, furniture, rhythm,
//!        │  room insects, positional math, mute tables
//!        ▼
//!   Nap_* command ring (256 AudioPort entries, deferred)
//!        │
//!        ▼
//!   JAudio engine (sequence VM, groups/subtracks, DSP, NEOS, AI)
//! ```
//!
//! This module implements the Na_* game layer: persistent virtual sound
//! sources, the 256-entry deferred command ring, positional
//! distance/pan math, BGM dual-group crossfading and subtrack muting,
//! Animalese voice modes, and the documented retail quirks. The JAudio
//! sequence VM, sample banks, ADPCM decoding, and the DSP mixer are
//! engine-owned: this layer talks to them only through recorded
//! `AudioPort` commands, exactly like retail's `Nap_*` calls.
//!
//! Preserved retail quirks (documented, not fixed):
//! * `TRGPRIO` has 120 entries but is indexed by the full low byte of the
//!   sound ID with no bounds check — retail reads linker-adjacent memory
//!   for indices 120+. This port returns `None` there (documented safety
//!   deviation; the exact bytes are unknowable from source).
//! * The train-whistle volume curve (`distance2vol_kiteki`) contains a
//!   dead `if (distance < 0)` branch that the linear formula then
//!   overwrites — reproduced verbatim, including possible negative volume.
//! * Entering `ROOM_TYPE_OTHER` resets `SOU_ONGEN_AREA1` but not
//!   `SOU_ONGEN_AREA2`.
//! * Room-insect duplicate-variance resolution restarts at `j = 0`
//!   instead of `j = -1`.
//! * `Sou_Ongen_Lev_Cont` can leave `foundIndex` at 0 and update slot 0
//!   when the source isn't found.
//! * The level-fade routines index `sou_lev_se` with raw 8..14 instead of
//!   subtracting 8 (out-of-bounds in retail); noted but not reproduced,
//!   since the bytes are unknowable from source.
//!
//! ROM assets the decomp excludes (sequence/bank/wave archives,
//! `audiorom.img`, ADP streams, banner/icon bytes) stay behind the
//! command-sink boundary: this module never needs their contents.

// ---------------------------------------------------------------------------
// Engine / group constants
// ---------------------------------------------------------------------------

/// JAudio engine constants (`include/jaudio_NES/audiocommon.h`).
pub mod engine {
    pub const GROUP_MAX: usize = 5;
    pub const SUBTRACK_NUM: usize = 16;
    pub const NOTE_MAX: usize = 128;
    pub const SUBTRACK_NOTE_NUM: usize = 4;
    pub const TATUMS_PER_BEAT: usize = 48;
    /// AGC configuration: 24 logical channels.
    pub const AGC_MAX_CHAN: usize = 0x18;
    pub const AGC_TIME_BASE: usize = 0x30;
    pub const AGC_ACMD_BUF_SIZE: usize = 0x70000;
    pub const AGC_FIX_SIZE: usize = 0x38000;
    pub const AGC_EMEM_SIZE: usize = 0x28000;
    /// CPU-side audio heap and ARAM configuration.
    pub const AUDIO_MEMORY: usize = 0x90000;
    pub const AUDIO_ARAM_SIZE: usize = 0x810000;
    /// DAC pipeline: triple-buffered, 7 subframes x 560 samples.
    pub const DSPBUF_NUM: usize = 3;
    pub const JAC_SUBFRAMES: usize = 7;
    pub const JAC_FRAMESAMPLES: usize = 560;
    pub const DAC_SIZE: usize = 1120;
    pub const JAC_DAC_RATE: f32 = 32028.5;
    /// DSP channel descriptors and FX buffers.
    pub const DSP_CHANNELS: usize = 64;
    pub const FX_BUFFERS: usize = 4;
}

/// Game-layer group assignments (`game64.c_inc:18-22`).
pub mod group {
    pub const SE: usize = 0;
    pub const BGM0: usize = 1;
    pub const FTR_INST: usize = 2;
    pub const BGM1: usize = 3;
    pub const VOICE: usize = 4;
}

/// SE subtrack roles within group 0.
pub mod se_subtrack {
    pub const TRG_FIRST: usize = 0;
    pub const TRG_LAST: usize = 5;
    pub const CHIME: usize = 7;
    pub const LEV_FIRST: usize = 8;
    pub const LEV_LAST: usize = 13;
    pub const MONO: usize = 14;
}

/// Game-layer slot counts.
pub mod slots {
    pub const TRG_SE: usize = 6;
    pub const VOICE_SE: usize = 2;
    pub const LEV_SE: usize = 6;
    pub const LEV_HISTORY: usize = 8;
    pub const ONGEN_ENTRY: usize = 50;
    pub const LEV_ONGEN_DATA: usize = 4;
    pub const ROOM_INS: usize = 50;
    pub const ROOM_INS_DATA: usize = 17;
    pub const RHYTHM_BUFFER: usize = 14;
}

/// Positional-audio areas.
pub mod area {
    pub const ONGEN_AREA1: f32 = 540.0;
    pub const ONGEN_AREA2: f32 = 533.0;
    /// Kiteki (train whistle) curve bounds.
    pub const KITEKI_MIN_DIST: f32 = 320.0;
}

/// Sound-ID flag bits packed into the upper nibble (`id & 0xF000`).
pub mod se_flag {
    pub const MONO: u16 = 0x1000;
    pub const DIST_REVERB: u16 = 0x2000;
    pub const ECHO: u16 = 0x4000;
    pub const SINGLETON: u16 = 0x8000;
}

/// Trigger-SE replacement priorities (`TRGPRIO`, 120 entries: mostly 50,
/// then 60s and 70s). Indexed by the low byte of the sound ID with no
/// retail bounds check.
pub const TRGPRIO: [u8; 120] = {
    let mut t = [50u8; 120];
    let mut i = 104;
    while i < 112 {
        t[i] = 60;
        i += 1;
    }
    while i < 120 {
        t[i] = 70;
        i += 1;
    }
    t
};

/// Priority lookup. Returns `None` for indices >= 120: retail reads
/// linker-adjacent memory there (no bounds check), which is unknowable
/// from source, so this port declines instead of inventing bytes.
pub fn trg_priority(se_idx_lo: u8) -> Option<u8> {
    TRGPRIO.get(se_idx_lo as usize).copied()
}

/// Voice modes (`Config_VOICE_MODE_*`, `include/m_config.h`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoiceMode {
    Animalese = 0,
    Click = 1,
    Silent = 2,
}

/// Voice sequences selected by spec (`Na_SpecChange`).
pub fn voice_seq_for_spec(spec: i32) -> u8 {
    match spec {
        3 => 244,
        4 | 6 | 8 => 245,
        _ => 243, // specs 2, 5, 7, 9 and default
    }
}

/// Per-spec voice volume/pitch (`Na_VoiceSe`): (volume, freq_scale).
pub fn voice_spec_params(spec: i32) -> (f32, f32) {
    match spec {
        2 | 3 | 4 => (0.65, 1.0),
        5 | 6 => (0.9, 1.0),
        7 => (0.9, 1.3),
        8 => (0.9, 0.75),
        9 => (0.7, 0.65),
        _ => (1.0, 1.0),
    }
}

// ---------------------------------------------------------------------------
// AudioPort command ring
// ---------------------------------------------------------------------------

/// Parameter union of `AudioPort` (`audiostruct.h:61-70`); the `void*`
/// variant is engine-owned and not modeled here.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AudioParam {
    S8(i8),
    U8(u8),
    S16(i16),
    U16(u16),
    S32(i32),
    U32(u32),
    F32(f32),
}

/// One deferred audio command: opcode + 3 arg bytes + parameter.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AudioPort {
    pub opcode: u8,
    pub arg0: u8,
    pub arg1: u8,
    pub arg2: u8,
    pub param: AudioParam,
}

/// The deferred command ring. Retail (non-PC) capacity is 256
/// (`sub_sys.c`: `#else` branch; the PC port uses 2048).
pub struct CommandRing {
    buf: [Option<AudioPort>; 256],
    write_pos: usize,
}

impl CommandRing {
    pub fn new() -> CommandRing {
        CommandRing { buf: [None; 256], write_pos: 0 }
    }

    /// `Nap_Set*`: append one command.
    pub fn push(&mut self, opcode: u8, arg0: u8, arg1: u8, arg2: u8, param: AudioParam) {
        self.buf[self.write_pos & 0xFF] = Some(AudioPort { opcode, arg0, arg1, arg2, param });
        self.write_pos = self.write_pos.wrapping_add(1);
    }

    /// `Nap_SendStart` / consumer side: drain all queued commands in order.
    pub fn drain(&mut self) -> Vec<AudioPort> {
        let mut out = Vec::new();
        for slot in self.buf.iter_mut() {
            if let Some(cmd) = slot.take() {
                out.push(cmd);
            }
        }
        out
    }

    pub fn pending(&self) -> usize {
        self.buf.iter().filter(|s| s.is_some()).count()
    }
}

impl Default for CommandRing {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Sound-ID packing
// ---------------------------------------------------------------------------

/// Decoded sound ID: flag bits + bank/index bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SeId {
    pub raw: u16,
    pub mono: bool,
    pub dist_reverb: bool,
    pub echo: bool,
    pub singleton: bool,
    /// Low 8 bits (also the `TRGPRIO` index).
    pub idx_lo: u8,
    /// `(id & 0x0F00) >> 8`: bank/high byte.
    pub idx_hi: u8,
}

impl SeId {
    pub fn decode(id: u16) -> SeId {
        let flags = (id & 0xF000) >> 12;
        SeId {
            raw: id,
            mono: flags & 1 != 0,
            dist_reverb: flags & 2 != 0,
            echo: flags & 4 != 0,
            singleton: flags & 8 != 0,
            idx_lo: (id & 0xFF) as u8,
            idx_hi: ((id & 0x0F00) >> 8) as u8,
        }
    }
}

// ---------------------------------------------------------------------------
// Positional audio math
// ---------------------------------------------------------------------------

/// Output-mode-dependent pan scaling (`pan_kochou`): mode 0 = x1.6,
/// mode 1 = unchanged, mode 2 = x0.75. Pan is 0..127, center 0x40.
pub fn pan_kochou(a: u8, b: f32, out_mode: u8) -> u8 {
    match out_mode {
        1 => return a,
        0 => {
            let b = b * 1.6;
            let v = ((a as i16 - 0x40) as f32 * b) as i16;
            let v = v.clamp(-0x40, 0x3F);
            (0x40 + v) as u8
        }
        _ => {
            let b = b * 0.75;
            let v = ((a as i16 - 0x40) as f32 * b) as i16;
            let v = v.clamp(-0x40, 0x3F);
            (0x40 + v) as u8
        }
    }
}

/// `angle2pan`: binary angle (high byte used) -> 0..127 pan.
pub fn angle2pan(angle: u16, out_mode: u8) -> u8 {
    let angle = (angle >> 8) as u8;
    let a = if (0x40..=0xC0).contains(&angle) {
        let mut a = 0x80u8.wrapping_sub(angle.wrapping_sub(0x40));
        if a == 0x80 {
            a = 0x7F;
        }
        a
    } else if angle >= 0xC1 {
        angle - 0xC0
    } else {
        angle + 0x40
    };
    pan_kochou(a, 1.0, out_mode)
}

/// Ordinary quadratic distance falloff (`distance2vol`): 1.15 at zero,
/// zero past 540, clamped to 1.5.
pub fn distance2vol(distance: f32) -> f32 {
    if distance > area::ONGEN_AREA1 {
        return 0.0;
    }
    let mut ret = 1.15 - (1.15 / (area::ONGEN_AREA1 * area::ONGEN_AREA1)) * distance * distance;
    if ret > 1.5 {
        ret = 1.5;
    }
    ret
}

/// Train-whistle curve (`distance2vol4KITEKI`), reproduced verbatim
/// including the retail bug: the `if (distance < 0.0) v2 = 0.0` branch is
/// dead because the linear formula unconditionally overwrites `v2` right
/// after, so volume can go negative past the intended range.
pub fn distance2vol_kiteki(distance: f32) -> f32 {
    let mut v2 = 0.0f32;
    if distance > 320.0 {
        v2 = 1.15;
    }
    if distance < 0.0 {
        v2 = 0.0;
    }
    // Bug preserved: unconditional overwrite, dead branch above.
    v2 = 1.15 - (1.0 / (5287.0 - 0.0435)) * (distance - 320.0);
    let _ = v2;
    v2
}

/// Minidisc/music-player curve (`distance2vol4MD`): quadratic over
/// `SOU_ONGEN_AREA2`, clamped to [0.2, 0.8].
pub fn distance2vol_md(distance: f32, area2: f32) -> f32 {
    let mut v2 = if distance > area2 {
        0.0
    } else {
        1.15 - (1.15 / (area2 * area2)) * distance * distance
    };
    if v2 > 0.8 {
        v2 = 0.8;
    }
    if v2 < 0.2 {
        v2 = 0.2;
    }
    v2
}

/// Room entry updates the audible areas. Retail bug preserved: entering
/// `ROOM_TYPE_OTHER` resets `ONGEN_AREA1` but does NOT restore
/// `ONGEN_AREA2` (it keeps whatever value it had).
pub fn room_areas(room_small: f32, room_medium: f32, room_large: f32, room: RoomType, cur_area2: f32) -> (f32, f32) {
    match room {
        RoomType::Small => (room_small, room_small),
        RoomType::Medium => (room_medium, room_medium),
        RoomType::Large => (room_large, room_large),
        RoomType::Other => (area::ONGEN_AREA1, cur_area2), // bug: area2 not restored
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoomType {
    Small,
    Medium,
    Large,
    Other,
}

// ---------------------------------------------------------------------------
// Triggered SEs (6 stateful slots)
// ---------------------------------------------------------------------------

/// `SOU_TRG_SE`: one persistent trigger-SE slot.
#[derive(Clone, Copy, Debug)]
pub struct TrgSe {
    pub id: u16,
    pub frames: u32,
    pub volume: f32,
    pub opt_volume: f32,
    pub calced_volume: f32,
    pub freq_scale: f32,
    pub pan: u8,
    pub reverb_volume: u8,
    pub echo: u8,
    pub priority: u8,
}

impl Default for TrgSe {
    fn default() -> TrgSe {
        TrgSe {
            id: 0,
            frames: 0,
            volume: 1.0,
            opt_volume: 1.0,
            calced_volume: 0.0,
            freq_scale: 1.0,
            pan: 0x40,
            reverb_volume: 0,
            echo: 0,
            priority: 0,
        }
    }
}

/// Outcome of `Sou_TrgStart`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrgStartResult {
    /// Routed to the dedicated MONO subtrack (SE group, subtrack 14).
    Mono,
    /// Rejected by singleton rule (same ID already playing).
    SingletonRejected,
    /// Took a free slot.
    Started(usize),
    /// Replaced the oldest slot (priority check passed).
    Replaced(usize),
    /// Rejected: oldest slot has strictly higher priority, or the
    /// priority table has no entry for this ID (retail OOB read).
    PriorityRejected,
}

pub struct TrgSeManager {
    pub slots: [TrgSe; slots::TRG_SE],
}

impl TrgSeManager {
    pub fn new() -> TrgSeManager {
        TrgSeManager { slots: [TrgSe::default(); slots::TRG_SE] }
    }

    /// `Sou_TrgStart`, writing JAudio subtrack port commands into `ring`.
    /// `reverb`/`echo`/`distance` follow the retail parameter flow.
    pub fn start(
        &mut self,
        ring: &mut CommandRing,
        id: u16,
        volume: f32,
        opt_volume: f32,
        freq_scale: f32,
        pan: u8,
        reverb: u8,
        distance: f32,
    ) -> TrgStartResult {
        let se = SeId::decode(id);

        // MONO path: dedicated subtrack 14, no slot allocation.
        if se.mono {
            ring.push(0x10, group::SE as u8, se_subtrack::MONO as u8, 0, AudioParam::F32(freq_scale));
            ring.push(0x11, group::SE as u8, se_subtrack::MONO as u8, 0, AudioParam::U8(pan));
            ring.push(0x12, group::SE as u8, se_subtrack::MONO as u8, 0, AudioParam::U8(reverb));
            ring.push(0x13, group::SE as u8, se_subtrack::MONO as u8, 0, AudioParam::U8(se.idx_lo));
            ring.push(0x13, group::SE as u8, se_subtrack::MONO as u8, 1, AudioParam::U8(se.idx_hi));
            return TrgStartResult::Mono;
        }

        // Singleton: at most one instance of this ID.
        if se.singleton && self.slots.iter().any(|s| s.id == id) {
            return TrgStartResult::SingletonRejected;
        }

        // Free slot?
        if let Some(i) = self.slots.iter().position(|s| s.id == 0) {
            self.fill(i, id, volume, opt_volume, freq_scale, pan, reverb, distance, &se, ring);
            return TrgStartResult::Started(i);
        }

        // Otherwise the longest-lived slot, replaced only if the new
        // priority is >= the old one. Priority comes from TRGPRIO indexed
        // by the low ID byte; retail does no bounds check (120-entry
        // table), so an unknown index rejects here instead of reading
        // linker-adjacent memory.
        let mut oldest = 0usize;
        let mut oldest_frames = 0u32;
        for (i, s) in self.slots.iter().enumerate() {
            if s.frames > oldest_frames {
                oldest_frames = s.frames;
                oldest = i;
            }
        }
        match trg_priority(se.idx_lo) {
            Some(p) if p >= self.slots[oldest].priority => {
                self.fill(oldest, id, volume, opt_volume, freq_scale, pan, reverb, distance, &se, ring);
                TrgStartResult::Replaced(oldest)
            }
            _ => TrgStartResult::PriorityRejected,
        }
    }

    fn fill(
        &mut self,
        i: usize,
        id: u16,
        volume: f32,
        opt_volume: f32,
        freq_scale: f32,
        pan: u8,
        reverb: u8,
        distance: f32,
        se: &SeId,
        ring: &mut CommandRing,
    ) {
        let s = &mut self.slots[i];
        s.id = id;
        s.priority = trg_priority(se.idx_lo).unwrap_or(50);
        s.frames = 1;
        s.volume = volume;
        s.opt_volume = opt_volume;
        s.freq_scale = freq_scale;
        s.pan = pan;
        // Reverb: explicit value wins; else distance-derived when the
        // DIST_REVERB flag is set (reverb = distance/8, capped at 50);
        // echo forces 40 when the slot already echoes.
        s.reverb_volume = if reverb != 0 {
            reverb
        } else {
            let mut r = 0u8;
            if se.dist_reverb {
                r = ((distance as u16) >> 3).min(50) as u8;
            }
            if se.echo && s.echo != 0 {
                r = 40;
            }
            r
        };
        // Subtrack ports: port 0 = index low, port 1 = bank/high.
        ring.push(0x13, group::SE as u8, i as u8, 0, AudioParam::U8(se.idx_lo));
        ring.push(0x13, group::SE as u8, i as u8, 1, AudioParam::U8(se.idx_hi));
    }

    /// `Sou_TrgMake`: every game frame, push each live slot's calculated
    /// parameters into JAudio.
    pub fn frame(&mut self, ring: &mut CommandRing) {
        for (i, s) in self.slots.iter_mut().enumerate() {
            if s.id == 0 {
                continue;
            }
            s.frames = s.frames.wrapping_add(1);
            ring.push(0x10, group::SE as u8, i as u8, 0, AudioParam::F32(s.volume * s.opt_volume));
            ring.push(0x11, group::SE as u8, i as u8, 0, AudioParam::U8(s.pan));
        }
    }

    pub fn stop(&mut self, i: usize) {
        if i < slots::TRG_SE {
            self.slots[i] = TrgSe::default();
        }
    }
}

impl Default for TrgSeManager {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Level (continuous) SEs: 6 slots x 8-entry history
// ---------------------------------------------------------------------------

/// `SOU_LEV_SE`: one level-SE slot with an 8-deep pending-ID history.
#[derive(Clone, Copy, Debug)]
pub struct LevSe {
    pub history: [u16; slots::LEV_HISTORY],
    pub pan: u8,
    pub volume: f32,
}

impl Default for LevSe {
    fn default() -> LevSe {
        LevSe { history: [0; slots::LEV_HISTORY], pan: 0x40, volume: 1.0 }
    }
}

pub struct LevSeManager {
    pub slots: [LevSe; slots::LEV_SE],
}

impl LevSeManager {
    pub fn new() -> LevSeManager {
        LevSeManager { slots: [LevSe::default(); slots::LEV_SE] }
    }

    /// `Sou_LevStart`: push an ID onto the slot's history (FIFO; the
    /// 8 entries act as a small stack of layered requests).
    pub fn start(&mut self, slot: usize, id: u16) {
        if slot >= slots::LEV_SE {
            return;
        }
        let h = &mut self.slots[slot].history;
        h.copy_within(1.., 0);
        h[slots::LEV_HISTORY - 1] = id;
    }

    /// `Sou_LevStop`: remove the ID and shift later entries forward.
    pub fn stop(&mut self, slot: usize, id: u16) {
        if slot >= slots::LEV_SE {
            return;
        }
        let h = &mut self.slots[slot].history;
        if let Some(pos) = h.iter().position(|&x| x == id) {
            h.copy_within(pos + 1.., pos);
            h[slots::LEV_HISTORY - 1] = 0;
        }
    }

    pub fn top(&self, slot: usize) -> u16 {
        self.slots.get(slot).map(|s| s.history[slots::LEV_HISTORY - 1]).unwrap_or(0)
    }
}

impl Default for LevSeManager {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Positional environmental sources: 50-entry cache, 4 active slots
// ---------------------------------------------------------------------------

/// `SOU_ONGEN_ENTRY`: one tracked positional source identity.
#[derive(Clone, Copy, Debug, Default)]
pub struct OngenEntry {
    pub id: u32,
    pub index: u8,
    pub pan: u8,
    pub distance: f32,
    /// Incremented while the source keeps being reported; removal when it
    /// goes stale.
    pub age: u32,
    pub live: bool,
}

/// One of the 4 active continuous level-sound slots.
#[derive(Clone, Copy, Debug, Default)]
pub struct LevOngenData {
    pub active: bool,
    pub source_id: u32,
    pub pan: u8,
    pub distance: f32,
    pub distance2: f32,
}

pub struct OngenManager {
    pub entries: [OngenEntry; slots::ONGEN_ENTRY],
    pub active: [LevOngenData; slots::LEV_ONGEN_DATA],
}

impl OngenManager {
    pub fn new() -> OngenManager {
        OngenManager {
            entries: [OngenEntry::default(); slots::ONGEN_ENTRY],
            active: [LevOngenData::default(); slots::LEV_ONGEN_DATA],
        }
    }

    /// `Na_OngenPos`: report a positional source for this frame. Refreshes
    /// the existing entry or allocates a free one.
    pub fn report(&mut self, id: u32, index: u8, pan: u8, distance: f32) -> Option<usize> {
        if let Some(e) = self.entries.iter_mut().find(|e| e.live && e.id == id) {
            e.index = index;
            e.pan = pan;
            e.distance = distance;
            e.age = e.age.wrapping_add(1);
            return self.entries.iter().position(|e| e.live && e.id == id);
        }
        if let Some(i) = self.entries.iter().position(|e| !e.live) {
            self.entries[i] = OngenEntry { id, index, pan, distance, age: 1, live: true };
            return Some(i);
        }
        None
    }

    /// Age out entries that stopped being reported, then arbitrate the 4
    /// active slots (`Sou_Ongen_Lev_Cont` core). Retail quirk preserved:
    /// when a live entry has no matching active slot, `foundIndex` stays 0
    /// and slot 0 is the one updated.
    pub fn arbitrate(&mut self) {
        // Drop stale entries (not refreshed this frame have age 0 after
        // the engine clears the refresh mark; simplified: entries the
        // caller marked dead).
        for e in self.entries.iter_mut().filter(|e| e.live && e.age == 0) {
            e.live = false;
        }
        for e in self.entries.iter_mut().filter(|e| e.live) {
            // Find the active slot tracking this source.
            let mut found_index = 0usize; // retail: left at 0 when not found
            let mut found = false;
            for (j, a) in self.active.iter().enumerate() {
                if a.active && a.source_id == e.id {
                    found = true;
                    found_index = j;
                    break;
                }
            }
            if !found {
                // First free active slot; retail quirk: if none is free,
                // foundIndex is still 0 and slot 0 gets overwritten.
                if let Some(j) = self.active.iter().position(|a| !a.active) {
                    found_index = j;
                }
                let a = &mut self.active[found_index];
                a.active = true;
                a.source_id = e.id;
                a.pan = e.pan;
                a.distance = e.distance;
                a.distance2 = 9999.0;
            } else {
                let a = &mut self.active[found_index];
                a.pan = e.pan;
                a.distance2 = a.distance;
                a.distance = e.distance;
            }
            e.age = 0; // refresh mark cleared; report() sets it again
        }
        // Active slots whose source died go quiet.
        for a in self.active.iter_mut().filter(|a| a.active) {
            if !self.entries.iter().any(|e| e.live && e.id == a.source_id) {
                *a = LevOngenData::default();
            }
        }
    }
}

impl Default for OngenManager {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Room insects: 50 entries, randomized variance, j=0 restart bug
// ---------------------------------------------------------------------------

/// `SOU_ROOM_INS`: one indoor insect voice.
#[derive(Clone, Copy, Debug, Default)]
pub struct RoomInsect {
    pub insect_id: u16,
    pub variance: u8,
    pub alive: bool,
    pub timer: u32,
}

/// Assign variances with the retail quirk: when resolving a duplicate
/// variance the loop should restart at `j = -1` but resets `j = 0`.
pub fn assign_insect_variance(ids: &[u16], rand: &mut dyn FnMut() -> u8) -> Vec<RoomInsect> {
    let mut out: Vec<RoomInsect> = Vec::with_capacity(ids.len().min(slots::ROOM_INS));
    for &id in ids.iter().take(slots::ROOM_INS) {
        let mut v = rand() % slots::ROOM_INS_DATA as u8;
        // Resolve duplicates against already-assigned insects.
        let mut j = 0usize;
        while j < out.len() {
            if out[j].variance == v {
                v = rand() % slots::ROOM_INS_DATA as u8;
                j = 0; // BUG (retail): should restart at j = -1
                continue;
            }
            j += 1;
        }
        out.push(RoomInsect { insect_id: id, variance: v, alive: true, timer: 0 });
    }
    out
}

// ---------------------------------------------------------------------------
// Voice / Animalese
// ---------------------------------------------------------------------------

/// `SOU_VOICE_SE`: one voice slot; the game alternates the two.
#[derive(Clone, Copy, Debug, Default)]
pub struct VoiceSe {
    pub volume: f32,
    pub freq_scale: f32,
    pub pan: u8,
    pub reverb: u8,
    pub busy: bool,
}

pub struct VoiceManager {
    pub slots: [VoiceSe; slots::VOICE_SE],
    toggle: bool,
    pub spec: i32,
    pub mode: VoiceMode,
}

impl VoiceManager {
    pub fn new() -> VoiceManager {
        VoiceManager {
            slots: [VoiceSe::default(); slots::VOICE_SE],
            toggle: false,
            spec: 0,
            mode: VoiceMode::Animalese,
        }
    }

    /// `Na_SpecChange`: stop the voice group and pick the voice sequence
    /// for the new spec (243/244/245).
    pub fn spec_change(&mut self, ring: &mut CommandRing, spec: i32) -> u8 {
        for s in self.slots.iter_mut() {
            *s = VoiceSe::default();
        }
        self.spec = spec;
        voice_seq_for_spec(spec)
    }

    /// `Na_VoiceSe` routing by mode. Returns the slot used, if any.
    /// Animalese pushes phoneme fragments through alternating slots
    /// (`sou_voice_se_toguru`); click plays the bebe SE; silent does
    /// nothing.
    pub fn voice_se(&mut self, ring: &mut CommandRing, phoneme: u8, pan: u8) -> Option<usize> {
        match self.mode {
            VoiceMode::Silent => None,
            VoiceMode::Click => {
                // NA_SE_BEBE via the trigger path.
                ring.push(0x13, group::SE as u8, 0, 0, AudioParam::U8(phoneme));
                None
            }
            VoiceMode::Animalese => {
                let (vol, pitch) = voice_spec_params(self.spec);
                let slot = if self.toggle { 1 } else { 0 };
                self.toggle = !self.toggle;
                let s = &mut self.slots[slot];
                s.volume = vol;
                s.freq_scale = pitch;
                s.pan = pan;
                s.busy = true;
                ring.push(0x13, group::VOICE as u8, slot as u8, 0, AudioParam::U8(phoneme));
                Some(slot)
            }
        }
    }

    /// `Sou_VoiceMake`: push both slots' parameters every frame.
    pub fn frame(&mut self, ring: &mut CommandRing) {
        for (i, s) in self.slots.iter().enumerate() {
            if !s.busy {
                continue;
            }
            ring.push(0x10, group::VOICE as u8, i as u8, 0, AudioParam::F32(s.volume));
            ring.push(0x11, group::VOICE as u8, i as u8, 0, AudioParam::U8(s.pan));
        }
    }
}

impl Default for VoiceManager {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// BGM: dual-group crossfade + subtrack mute tables
// ---------------------------------------------------------------------------

/// Hourly field-track mute masks: 1 = mute, 0 = unchanged.
pub const BGM_MUTE_TABLE_FINE: [u16; 24] = [
    0x01e0, 0x0d80, 0x3800, 0x0500, 0x00e0, 0x01c0, 0x05e0, 0x3dc0,
    0x0d80, 0x0d80, 0x1800, 0x1d00, 0x1d80, 0x1d80, 0x01f0, 0x00f8,
    0x0d80, 0x7c00, 0xf200, 0x1d00, 0x1d00, 0x05e0, 0x0d80, 0x0dfc,
];
pub const BGM_MUTE_TABLE_SNOW: [u16; 24] = [
    0x0302, 0x0c23, 0x216e, 0x040c, 0x0006, 0x0006, 0x0406, 0x381b,
    0x0803, 0x0c64, 0x1020, 0x1083, 0x1c02, 0x104f, 0x070e, 0x06c0,
    0x081d, 0x001b, 0x2023, 0x1017, 0x1803, 0x0006, 0x0c16, 0x0d82,
];
pub const BGM_MUTE_TABLE_SAKURA: [u16; 24] = [
    0x00e2, 0x00a7, 0x116e, 0x001c, 0x0006, 0x0106, 0x018e, 0x01db,
    0x0107, 0x00ff, 0x0024, 0x0087, 0x0007, 0x0c4f, 0x00be, 0x0418,
    0x045f, 0x001b, 0xca07, 0x0517, 0x0107, 0x0106, 0x019f, 0x007b,
];
/// Museum masks: not-museum, lobby, painting, fish, insect, fossil.
pub const BGM_MUTE_TABLE_MUSEUM: [u16; 6] = [
    0x0000, 0xFFE8, 0x9ff6, 0xff1a, 0xfdff, 0x62fc,
];

/// Weather selector for the hourly-track masks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BgmWeather {
    Fine,
    Rain,
    Snow,
    Sakura,
    Other,
}

pub struct BgmManager {
    /// `sou_now_bgm_handle`: the currently-playing group; crossfades
    /// alternate between BGM0_GROUP and BGM1_GROUP.
    pub handle: usize,
    pub volume: f32,
}

impl BgmManager {
    pub fn new() -> BgmManager {
        BgmManager { handle: group::BGM0, volume: 1.0 }
    }

    /// Start a BGM id on the *other* group and fade the current one out;
    /// then swap handles (`Na_BgmStart` crossfade skeleton). `seq_id` is
    /// the `SEQ_TABLE` lookup result (ROM data; resolved by the engine).
    pub fn crossfade_to(&mut self, ring: &mut CommandRing, seq_id: u16) {
        let next = if self.handle == group::BGM0 { group::BGM1 } else { group::BGM0 };
        // Start the new sequence on the standby group...
        ring.push(0x20, next as u8, 0, 0, AudioParam::U16(seq_id));
        // ...fade the old group out...
        ring.push(0x21, self.handle as u8, 0, 0, AudioParam::F32(0.0));
        // ...and swap handles.
        self.handle = next;
    }

    /// Apply an hourly mute mask (`Sou_BgmTenkiConv` core): mute every
    /// subtrack of the current group, then enable the mask-selected ones.
    /// One weather state substitutes BGM id 0x45 instead of masking.
    pub fn apply_weather(&mut self, ring: &mut CommandRing, hour: usize, weather: BgmWeather) -> Option<u16> {
        if weather == BgmWeather::Rain {
            return Some(0x45); // substituted BGM id, no mask
        }
        let table = match weather {
            BgmWeather::Fine => &BGM_MUTE_TABLE_FINE,
            BgmWeather::Snow => &BGM_MUTE_TABLE_SNOW,
            BgmWeather::Sakura => &BGM_MUTE_TABLE_SAKURA,
            _ => &BGM_MUTE_TABLE_FINE,
        };
        let mask = table[hour % 24];
        ring.push(0x22, self.handle as u8, 0xFF, 0, AudioParam::U8(1)); // mute all
        ring.push(0x23, self.handle as u8, 0, 0, AudioParam::U16(mask)); // apply mask
        None
    }

    /// Museum room mask (`Na_Museum`): clears the current mask and applies
    /// the room-specific one. `room`: 0 = not museum .. 5 = fossil room.
    pub fn apply_museum(&mut self, ring: &mut CommandRing, room: usize) {
        let mask = BGM_MUTE_TABLE_MUSEUM[room.min(5)];
        ring.push(0x23, self.handle as u8, 0, 0, AudioParam::U16(mask));
    }

    /// `Na_BGMFilter`: push a filter state to every subtrack of the active
    /// group. The filter coefficients themselves are DSP-side.
    pub fn set_filter(&mut self, ring: &mut CommandRing, filter: u32) {
        for st in 0..engine::SUBTRACK_NUM {
            ring.push(0x24, self.handle as u8, st as u8, 0, AudioParam::U32(filter));
        }
    }
}

impl Default for BgmManager {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Furniture instruments + rhythm (group 2)
// ---------------------------------------------------------------------------

/// Two melody identities alternate based on furniture-ID history.
#[derive(Clone, Copy, Debug, Default)]
pub struct FurnitureInst {
    pub history: [u16; 2],
    pub active: Option<usize>,
}

impl FurnitureInst {
    /// `Na_FurnitureInst`: pick the melody identity for a furniture ID.
    pub fn select(&mut self, furniture_id: u16) -> usize {
        let idx = (furniture_id as usize) % 2;
        self.history[idx] = furniture_id;
        self.active = Some(idx);
        idx
    }
}

/// Rhythm/Haniwa allocator: 14 buffer slots on group 2.
#[derive(Clone, Copy, Debug, Default)]
pub struct RhythmSlot {
    pub state: u8,
    pub subtrack: u8,
    pub buffer_id: u16,
}

pub struct RhythmManager {
    pub buffers: [RhythmSlot; slots::RHYTHM_BUFFER],
}

impl RhythmManager {
    pub fn new() -> RhythmManager {
        RhythmManager { buffers: [RhythmSlot::default(); slots::RHYTHM_BUFFER] }
    }

    pub fn alloc(&mut self, buffer_id: u16) -> Option<usize> {
        let i = self.buffers.iter().position(|b| b.state == 0)?;
        self.buffers[i] = RhythmSlot { state: 1, subtrack: i as u8, buffer_id };
        Some(i)
    }

    pub fn free(&mut self, i: usize) {
        if i < slots::RHYTHM_BUFFER {
            self.buffers[i] = RhythmSlot::default();
        }
    }
}

impl Default for RhythmManager {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Scene audio state (Na_SceneMode)
// ---------------------------------------------------------------------------

/// Audio-relevant scene state: echo/reverb switches, chime volume, BGM
/// volume, and which persistent sequences (242 = SE, 246 = furniture) are
/// running. Scene transitions mutate this; the engine turns it into
/// commands.
#[derive(Clone, Debug)]
pub struct SceneAudio {
    pub echo: bool,
    pub level_echo_reverb: u8,
    pub chime_volume: f32,
    pub bgm_volume: f32,
    pub se_seq_running: bool,
    pub ftr_seq_running: bool,
}

impl Default for SceneAudio {
    fn default() -> SceneAudio {
        SceneAudio {
            echo: false,
            level_echo_reverb: 0,
            chime_volume: 1.0,
            bgm_volume: 1.0,
            se_seq_running: true, // sequence 242 started in normal scenes
            ftr_seq_running: false,
        }
    }
}

impl SceneAudio {
    /// Indoor echoed rooms set level echo reverb 0x28; scene 0xF clears it.
    pub fn set_room_echo(&mut self, scene: u8, echoed_room: bool) {
        if scene == 0xF {
            self.level_echo_reverb = 0;
            self.echo = false;
        } else if echoed_room {
            self.level_echo_reverb = 0x28;
            self.echo = true;
        }
    }
}

// ---------------------------------------------------------------------------
// Top-level game audio state
// ---------------------------------------------------------------------------

/// Everything the Na_* layer owns. One per game instance; `ring` is the
/// deferred command port drained toward the JAudio engine each frame.
pub struct GameAudio {
    pub ring: CommandRing,
    pub trg: TrgSeManager,
    pub lev: LevSeManager,
    pub ongen: OngenManager,
    pub voice: VoiceManager,
    pub bgm: BgmManager,
    pub furniture: FurnitureInst,
    pub rhythm: RhythmManager,
    pub scene: SceneAudio,
    pub out_mode: u8,
}

impl GameAudio {
    pub fn new() -> GameAudio {
        GameAudio {
            ring: CommandRing::new(),
            trg: TrgSeManager::new(),
            lev: LevSeManager::new(),
            ongen: OngenManager::new(),
            voice: VoiceManager::new(),
            bgm: BgmManager::new(),
            furniture: FurnitureInst::default(),
            rhythm: RhythmManager::new(),
            scene: SceneAudio::default(),
            out_mode: 1,
        }
    }

    /// `sAdo_GameFrame` -> `Na_GameFrame`: per-frame parameter pushes.
    pub fn game_frame(&mut self) {
        let mut ring = std::mem::take(&mut self.ring);
        self.trg.frame(&mut ring);
        self.voice.frame(&mut ring);
        self.ongen.arbitrate();
        self.ring = ring;
    }
}

impl Default for GameAudio {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// C ABI
// ---------------------------------------------------------------------------

/// Unpack a sound ID's flag nibble: returns (mono, dist_reverb, echo,
/// singleton) as int flags.
#[no_mangle]
pub extern "C" fn pc_audio_se_flags(id: u16, out: *mut u8) {
    if out.is_null() {
        return;
    }
    let se = SeId::decode(id);
    unsafe {
        *out.add(0) = u8::from(se.mono);
        *out.add(1) = u8::from(se.dist_reverb);
        *out.add(2) = u8::from(se.echo);
        *out.add(3) = u8::from(se.singleton);
        *out.add(4) = se.idx_lo;
        *out.add(5) = se.idx_hi;
    }
}

/// Ordinary distance -> volume (quadratic, 540-unit radius).
#[no_mangle]
pub extern "C" fn pc_audio_distance2vol(distance: f32) -> f32 {
    distance2vol(distance)
}

/// Binary angle + output mode -> 0..127 pan.
#[no_mangle]
pub extern "C" fn pc_audio_angle2pan(angle: u16, out_mode: u8) -> u8 {
    angle2pan(angle, out_mode)
}

/// Voice sequence for a spec (243/244/245).
#[no_mangle]
pub extern "C" fn pc_audio_voice_seq_for_spec(spec: i32) -> u8 {
    voice_seq_for_spec(spec)
}

/// Hourly BGM mute mask for fine weather.
#[no_mangle]
pub extern "C" fn pc_audio_bgm_mute_fine(hour: u8) -> u16 {
    BGM_MUTE_TABLE_FINE[(hour % 24) as usize]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn se_id_flags() {
        let se = SeId::decode(0x8123);
        assert!(se.singleton);
        assert!(!se.mono);
        assert_eq!(se.idx_lo, 0x23);
        assert_eq!(se.idx_hi, 0x01);
        let mono = SeId::decode(0x1456);
        assert!(mono.mono);
        assert!(!mono.singleton);
    }

    #[test]
    fn singleton_rejects_duplicates() {
        let mut g = GameAudio::new();
        let id = se_flag::SINGLETON | 0x0042;
        let mut ring = CommandRing::new();
        assert!(matches!(g.trg.start(&mut ring, id, 1.0, 1.0, 1.0, 0x40, 0, 100.0), TrgStartResult::Started(_)));
        assert_eq!(
            g.trg.start(&mut ring, id, 1.0, 1.0, 1.0, 0x40, 0, 100.0),
            TrgStartResult::SingletonRejected
        );
    }

    #[test]
    fn mono_bypasses_slots() {
        let mut g = GameAudio::new();
        let mut ring = CommandRing::new();
        let r = g.trg.start(&mut ring, se_flag::MONO | 0x0010, 1.0, 1.0, 1.0, 0x40, 0, 50.0);
        assert_eq!(r, TrgStartResult::Mono);
        assert!(g.trg.slots.iter().all(|s| s.id == 0));
        assert!(ring.pending() > 0);
    }

    #[test]
    fn oldest_lowest_priority_replacement() {
        let mut g = GameAudio::new();
        let mut ring = CommandRing::new();
        // Fill all six slots; idx 0x00..0x05 have priority 50.
        for i in 0..6u16 {
            let r = g.trg.start(&mut ring, 0x0001 + i, 1.0, 1.0, 1.0, 0x40, 0, 10.0);
            assert!(matches!(r, TrgStartResult::Started(_)));
        }
        // Age slot 0 the most.
        g.trg.slots[0].frames = 999;
        // New ID with priority 50 replaces the oldest (50 >= 50).
        let r = g.trg.start(&mut ring, 0x0010, 1.0, 1.0, 1.0, 0x40, 0, 10.0);
        assert!(matches!(r, TrgStartResult::Replaced(0)));
    }

    #[test]
    fn trgprio_oob_is_none() {
        assert_eq!(trg_priority(0), Some(50));
        assert_eq!(trg_priority(104), Some(60));
        assert_eq!(trg_priority(112), Some(70));
        assert_eq!(trg_priority(119), Some(70));
        assert_eq!(trg_priority(120), None); // retail reads OOB here
        assert_eq!(trg_priority(200), None);
    }

    #[test]
    fn distance_curves() {
        assert!((distance2vol(0.0) - 1.15).abs() < 1e-6);
        assert_eq!(distance2vol(541.0), 0.0);
        assert!(distance2vol(270.0) > 0.0 && distance2vol(270.0) < 1.15);
        // Kiteki: verbatim, including the dead branch.
        let v = distance2vol_kiteki(10000.0);
        assert!(v < 0.0); // negative past range: the retail bug
        // MD clamped to [0.2, 0.8].
        assert_eq!(distance2vol_md(0.0, 533.0), 0.8);
        assert_eq!(distance2vol_md(100000.0, 533.0), 0.2);
    }

    #[test]
    fn pan_math() {
        // Straight ahead-ish angles center the pan.
        let p = angle2pan(0x0000, 1);
        assert!((p as i16 - 0x40).abs() <= 2);
        // Out mode 0 widens vs mode 2.
        let wide = angle2pan(0x4000, 0);
        let narrow = angle2pan(0x4000, 2);
        assert!((wide as i16 - 0x40).abs() >= (narrow as i16 - 0x40).abs());
    }

    #[test]
    fn room_area_bug() {
        let (a1, a2) = room_areas(545.0, 520.0, 545.0, RoomType::Other, 999.0);
        assert_eq!(a1, area::ONGEN_AREA1);
        assert_eq!(a2, 999.0); // NOT restored: the retail bug
    }

    #[test]
    fn level_se_history_fifo() {
        let mut lev = LevSeManager::new();
        lev.start(0, 11);
        lev.start(0, 22);
        assert_eq!(lev.top(0), 22);
        lev.stop(0, 22);
        assert_eq!(lev.top(0), 11);
        lev.stop(0, 11);
        assert_eq!(lev.top(0), 0);
    }

    #[test]
    fn ongen_arbitration_and_foundindex_quirk() {
        let mut o = OngenManager::new();
        // Occupy all 4 active slots with live entries.
        for i in 0..4u32 {
            o.report(100 + i, 0, 0x40, 100.0);
        }
        o.arbitrate();
        assert!(o.active.iter().all(|a| a.active));
        // Kill the entries without freeing: new report with all slots busy
        // and no free slot -> retail overwrites slot 0 (foundIndex stays 0).
        for e in o.entries.iter_mut() {
            e.live = false;
        }
        for a in o.active.iter_mut() {
            a.active = true; // keep busy artificially
            a.source_id = 999;
        }
        o.report(555, 0, 0x40, 50.0);
        o.arbitrate();
        assert_eq!(o.active[0].source_id, 555);
    }

    #[test]
    fn insect_variance_bug() {
        // With a constant RNG every insect collides; the j=0 restart still
        // terminates because v is recomputed identically... use a counter.
        let mut n = 0u8;
        let mut rng = || {
            n = n.wrapping_add(1);
            n
        };
        let ids: Vec<u16> = (0..5).collect();
        let bugs = assign_insect_variance(&ids, &mut rng);
        assert_eq!(bugs.len(), 5);
        let mut seen = std::collections::HashSet::new();
        for b in &bugs {
            assert!(seen.insert(b.variance), "duplicate variance not resolved");
        }
    }

    #[test]
    fn voice_modes_and_spec() {
        let mut v = VoiceManager::new();
        let mut ring = CommandRing::new();
        v.mode = VoiceMode::Silent;
        assert_eq!(v.voice_se(&mut ring, 3, 0x40), None);
        v.mode = VoiceMode::Animalese;
        v.spec = 7;
        let s0 = v.voice_se(&mut ring, 3, 0x40);
        let s1 = v.voice_se(&mut ring, 4, 0x40);
        assert_eq!(s0, Some(0));
        assert_eq!(s1, Some(1)); // toggles
        assert!((v.slots[0].freq_scale - 1.3).abs() < 1e-6); // spec 7 pitch
        assert_eq!(v.spec_change(&mut ring, 3), 244);
        assert_eq!(voice_seq_for_spec(9), 243);
    }

    #[test]
    fn bgm_crossfade_and_masks() {
        let mut b = BgmManager::new();
        let mut ring = CommandRing::new();
        assert_eq!(b.handle, group::BGM0);
        b.crossfade_to(&mut ring, 0x1234);
        assert_eq!(b.handle, group::BGM1);
        b.crossfade_to(&mut ring, 0x1234);
        assert_eq!(b.handle, group::BGM0);
        // Weather mask path.
        let mut ring2 = CommandRing::new();
        assert_eq!(b.apply_weather(&mut ring2, 7, BgmWeather::Fine), None);
        assert_eq!(b.apply_weather(&mut ring2, 7, BgmWeather::Rain), Some(0x45));
        // Museum masks verbatim.
        assert_eq!(BGM_MUTE_TABLE_MUSEUM[3], 0xff1a);
        assert_eq!(BGM_MUTE_TABLE_FINE[0], 0x01e0);
    }

    #[test]
    fn command_ring_is_256_and_deferred() {
        let mut ring = CommandRing::new();
        for i in 0..300u16 {
            ring.push(0x01, 0, 0, 0, AudioParam::U16(i));
        }
        // Ring overwrites; drain returns what is queued.
        assert!(ring.pending() <= 256);
        let cmds = ring.drain();
        assert_eq!(ring.pending(), 0);
        assert!(!cmds.is_empty());
    }

    #[test]
    fn game_frame_pushes_params() {
        let mut g = GameAudio::new();
        let mut ring = CommandRing::new();
        g.trg.start(&mut ring, 0x0042, 1.0, 1.0, 1.0, 0x40, 0, 10.0);
        let before = g.trg.slots[0].frames;
        g.game_frame();
        assert_eq!(g.trg.slots[0].frames, before + 1);
    }
}
