//! Title-screen flow: trademark demo -> title demo -> title.
//!
//! Source-verified against the PC-port tree
//! (`src/game/m_trademark.c`, `src/game/m_titledemo.c`,
//! `src/data/scene/title_demo.c`, `src/data/titledemo/pact0.c`..`pact4.c`,
//! `include/m_titledemo.h`, `include/m_trademark.h`):
//!
//! ## Flow
//! 1. `trademark_init` builds the `GAME_TRADEMARK` game: exec =
//!    `trademark_main`, fade starts opaque (`alpha = 0xFF00`), Nintendo-logo
//!    fade-in state (`alpha2 = 0`), `logo_timer = 60`, `move_timer = 16`,
//!    `stage = 0`. On first boot `mTR_first_flag` forces `stage = 5`,
//!    skipping the logo straight to the demo scene.
//! 2. `trademark_main` runs every frame: `fqrand()` seed increment,
//!    `trademark_cancel` (START at stage 4 with a connected pad),
//!    `trademark_move` (stage machine below), `trademark_draw`
//!    (Nintendo logo + black fade). At `stage == 5` it calls
//!    `trademark_goto_demo_scene` and clears `mTR_first_flag`.
//! 3. `trademark_goto_demo_scene` picks one of 5 demos
//!    (`mEv_CheckTitleDemo`), sets `door_data` from the demo door table
//!    (next scene = `SCENE_TITLE_DEMO` variant + 1), stamps a preset
//!    date/time/weather (`mTM_demotime_set`), randomizes player data and
//!    the 14 demo villagers, then `GAME_GOTO_NEXT(play, PLAY)`.
//! 4. The title demo plays back 60 s of recorded controller input
//!    (`pactN_key_data`, 1800 samples at 30 fps) with a zero-order hold,
//!    then `mTD_game_end_init` wipes to the next demo.
//!
//! ## Verified details
//! - Demo ids: `mEv_TITLEDEMO_LOGO = -1`, `mEv_TITLEDEMO_NONE = 0`,
//!   `mEv_TITLEDEMO_START1 = 1`; `mTD_demono_get` cycles
//!   `LOGO -> 1 -> 2 -> 3 -> 4 -> 5 -> 1 ...`.
//! - Key-data bit layout is documented in `pact0.c` itself:
//!   `XXXXXXXB YYYYYYYA` — bit 0 = A, bits 7:1 = stick Y (7-bit signed),
//!   bit 8 = B, bits 15:9 = stick X (7-bit signed). Decoded as
//!   `(s16)(keydata & 0xFE00) / 512` etc., i.e. sign-extended then
//!   truncated toward zero.
//! - The interpolation version of the key-data applier
//!   (`set_player_demo_keydata`) is `#if 0`'d out in source
//!   ("@fakematch?"); the active version is the zero-order hold
//!   (`set_player_demo_keydata_hold`), which clamps `delta_time` to
//!   1/30 s and advances a 30 fps sample index through an accumulator.
//! - `mTD_player_keydata_init`'s tool filter (`ITM_AXE`/`ITM_ROD`/
//!   `ITM_UMBRELLA00`) is a verified no-op: every branch assigns the
//!   value back to itself, so the header tool id passes through unchanged.
//!   Header tools: pact0 none, pact1 `0x2204` (umbrella), pact2 `0x2203`
//!   (rod), pact3 none, pact4 `0x2201` (axe).
//! - `mTD_tdemo_button_ok_check` returns FALSE once the 30 fps frame
//!   index reaches `mTD_START_END_FRAME` (3530), disabling button input
//!   near the end of the demo.
//! - `@BUG` in `m_trademark.c`: `mNpc_SetAnimalTitleDemo` uses
//!   `ANIMAL_NUM_MAX` (15) but only 14 villagers are listed; fixed under
//!   `BUGFIXES`/`TARGET_PC` by appending an `EMPTY_NO` entry.
//!
//! ## TARGET_PC divergences (kept in C, not ported)
//! - `trademark_goto_demo_scene` saves/restores `Save_t` around the demo
//!   randomization (on GameCube the memory-card save is reloaded anyway);
//!   see `fixes/m_trademark.c`.
//! - `title_demo_move` holds the demo (feeds zero input, freezes the
//!   wipe timer) while the PC settings overlay owns the screen.
//! - Display-list work (`trademark_draw`, Nintendo-logo DL, fades),
//!   `GAME_PLAY`/`PLAYER_ACTOR` struct writes, and RTC/common-data calls
//!   stay on the C side; this module ports the pure state machines,
//!   data tables, and decoders.

use std::ffi::c_int;
use std::sync::Mutex;

/// Number of title demos (`mTD_TITLE_DEMO_NUM`).
pub const TITLE_DEMO_NUM: usize = 5;
/// Demo playback length in seconds (`mTD_LENGTH_SECONDS`).
pub const DEMO_LENGTH_SECONDS: f32 = 60.0;
/// Recorded input samples per demo at 30 fps (`mTD_LENGTH_FRAMES_30FPS`).
pub const DEMO_LENGTH_FRAMES_30FPS: i32 = 1800;
/// Demo length measured in 60 fps frames (`mTD_LENGTH_FRAMES_60FPS`).
pub const DEMO_LENGTH_FRAMES_60FPS: i32 = 3600;
/// Frame index at which button input is disabled (`mTD_START_END_FRAME`).
pub const DEMO_START_END_FRAME: i32 = DEMO_LENGTH_FRAMES_60FPS - 70;

/// `mEv_TITLEDEMO_LOGO`.
pub const TITLEDEMO_LOGO: i32 = -1;
/// `mEv_TITLEDEMO_NONE`.
pub const TITLEDEMO_NONE: i32 = 0;
/// `mEv_TITLEDEMO_START1`.
pub const TITLEDEMO_START1: i32 = 1;

/// `pactN_head_table` key indices (`mTD_HEADER_*`).
pub mod demo_header_key {
    pub const POSX: usize = 0;
    pub const POSY: usize = 1;
    pub const POSZ: usize = 2;
    pub const ROTATION: usize = 3;
    pub const TOOL: usize = 4;
    pub const SIZE: usize = 5;
    pub const COUNT: usize = 6;
}

/// Title BGM ids played as the lost-fanfare at trademark stage 0
/// (`s_titlebgm` in `trademark_move`).
pub const TITLE_DEMO_BGM: [u8; TITLE_DEMO_NUM] = [83, 84, 85, 86, 87];

/// Fade increment per 60 fps frame (`0x880 * dt_frames`).
pub const TRADEMARK_FADE_RATE: f32 = 0x880 as f32;
/// Fully opaque fade value (`0xFF00`).
pub const TRADEMARK_ALPHA_MAX: f32 = 0xFF00 as f32;
/// Nintendo-logo hold time in frames (`logo_timer = 60`).
pub const TRADEMARK_LOGO_TIMER: f32 = 60.0;
/// Initial stage-1 wait in frames (`move_timer = 16`).
pub const TRADEMARK_MOVE_TIMER: f32 = 16.0;

/// Trademark stage ids from `trademark_move` / `nintendo_logo_move`.
pub mod trademark_stage {
    pub const PICK_DEMO: u8 = 0;
    pub const WAIT: u8 = 1;
    pub const LOGO_FADE_IN: u8 = 2;
    pub const LOGO_HOLD: u8 = 4;
    pub const FADE_OUT: u8 = 3;
    pub const GOTO_DEMO: u8 = 5;
}

/// Demo start positions from the five `demo_N_door_data` entries
/// (`{ x, y, z }`).
pub const DEMO_DOOR_POSITIONS: [(i32, i32, i32); TITLE_DEMO_NUM] = [
    (2180, 200, 824),
    (3218, 40, 3074),
    (2117, 160, 1488),
    (2899, 160, 1101),
    (1578, 40, 2472),
];

/// Demo date/time/weather presets from `mTM_demotime_set`
/// (`tradeday_table`): `(month, day, hour, weather)`.
/// Months are `lbRTC_*` (JANUARY = 1); weather is `mEnv_WEATHER_*`
/// (CLEAR = 0, RAIN = 1, SNOW = 2, SAKURA = 3).
pub const DEMO_TRADEDAYS: [(u8, u8, u8, u8); TITLE_DEMO_NUM] = [
    (4, 6, 13, 3),  // April 6th @ 1pm, cherry blossoms
    (6, 16, 13, 1), // June 16th @ 1pm, rain
    (8, 1, 6, 0),   // August 1st @ 6am, clear
    (11, 1, 16, 0), // November 1st @ 4pm, clear
    (2, 1, 2, 2),   // February 1st @ 2am, snow
];

/// Demo initial-state tables (`pactN_head_table`):
/// `[pos_x, pos_y, pos_z, angle, tool, size]`.
pub const DEMO_HEAD_TABLES: [[u16; demo_header_key::COUNT]; TITLE_DEMO_NUM] = [
    [0x0884, 0x00C8, 0x0338, 0xB77D, 0x0000, 0x0721],
    [0x0C92, 0x0028, 0x0C02, 0x5893, 0x2204, 0x0722],
    [0x0845, 0x00A0, 0x05D0, 0x1A8C, 0x2203, 0x0733],
    [0x0B53, 0x00A0, 0x044D, 0xE400, 0x0000, 0x0731],
    [0x062A, 0x0028, 0x09A8, 0xBE9F, 0x2201, 0x072A],
];

/// Read one field of a demo's head table (`get_demo_header`).
pub fn demo_header(titledemo_no: usize, key: usize) -> u16 {
    DEMO_HEAD_TABLES[titledemo_no % TITLE_DEMO_NUM][key % demo_header_key::COUNT]
}

/// Advance the demo id (`mTD_demono_get`): `LOGO -> START1`, otherwise
/// increment, wrapping past `TITLE_DEMO_NUM` back to `START1`.
pub fn demono_next(current: i32) -> i32 {
    if current == TITLEDEMO_LOGO {
        TITLEDEMO_START1
    } else {
        let next = current + 1;
        if next > TITLE_DEMO_NUM as i32 {
            TITLEDEMO_START1
        } else {
            next
        }
    }
}

/// Zero-based demo index (`mTD_get_titledemo_no`).
pub fn titledemo_index(demono: i32) -> usize {
    let d = if demono <= TITLEDEMO_NONE {
        TITLEDEMO_START1
    } else {
        demono
    };
    (d - TITLEDEMO_START1) as usize
}

/// Button input is accepted until the frame index reaches
/// `DEMO_START_END_FRAME` (`mTD_tdemo_button_ok_check`).
pub fn demo_button_ok(frame_30fps: i32) -> bool {
    frame_30fps < DEMO_START_END_FRAME
}

/// Decoded recorded controller input for one 30 fps sample.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DemoInput {
    pub stick_x: i8,
    pub stick_y: i8,
    pub btn_a: bool,
    pub btn_b: bool,
}

/// Decode one `pactN_key_data` entry.
///
/// Bit layout (documented in `pact0.c` as `XXXXXXXB YYYYYYYA`):
/// bit 0 = A, bits 7:1 = stick Y, bit 8 = B, bits 15:9 = stick X.
/// Matches the C exactly: sign-extended 16-bit halves divided by 512
/// with truncation toward zero.
pub fn decode_keydata(keydata: u16) -> DemoInput {
    let stick_x = (((keydata & 0xFE00) as i16) / 512) as i8;
    let stick_y = ((((keydata & 0x00FE) << 8) as i16) / 512) as i8;
    DemoInput {
        stick_x,
        stick_y,
        btn_a: (keydata & 1) != 0,
        btn_b: ((keydata >> 8) & 1) != 0,
    }
}

/// Clamp a 30 fps sample index to the recorded range, as
/// `set_player_demo_keydata_hold` does before reading.
pub fn demo_frame_clamp(frame: i32) -> i32 {
    frame.clamp(0, DEMO_LENGTH_FRAMES_30FPS - 1)
}

/// Advance the 30 fps sample index with the zero-order hold
/// (`set_player_demo_keydata_hold`): `delta_time` is clamped to
/// 1/30 s (and to >= 0), converted to fractional 30 fps frames, and
/// accumulated; whole frames advance the index, which saturates at
/// the last recorded sample. Returns `(frame, accum)`.
pub fn advance_demo_frame(frame: i32, accum: f32, delta_time: f32) -> (i32, f32) {
    let dt = delta_time.clamp(0.0, 1.0 / 30.0);
    let mut acc = accum + dt * 30.0;
    let mut f = frame;
    if acc >= 1.0 {
        let whole = acc as i32;
        f += whole;
        acc -= whole as f32;
        if f >= DEMO_LENGTH_FRAMES_30FPS {
            f = DEMO_LENGTH_FRAMES_30FPS - 1;
        }
    }
    (f, acc)
}

/// True once the demo has played its full 60 s (`title_demo_move`
/// calls `mTD_game_end_init`).
pub fn demo_time_done(elapsed_seconds: f32) -> bool {
    elapsed_seconds >= DEMO_LENGTH_SECONDS
}

/// The header tool id passes through unchanged: `mTD_player_keydata_init`
/// compares against `ITM_AXE`/`ITM_ROD`/`ITM_UMBRELLA00` but every branch
/// assigns the value back to itself (verified no-op).
pub fn demo_tool_passthrough(tool: u16) -> u16 {
    tool
}

/// Pure trademark state machine (`trademark_move` + `nintendo_logo_move`
/// + the stage-5 transition in `trademark_main`).
#[derive(Debug, Clone)]
pub struct TrademarkState {
    pub stage: u8,
    pub alpha: f32,
    pub alpha2: f32,
    pub logo_timer: f32,
    pub move_timer: f32,
    pub cancel: bool,
    pub check: bool,
}

impl TrademarkState {
    /// Initial state from `trademark_init` (`first_boot` = `mTR_first_flag`).
    pub fn new(first_boot: bool) -> Self {
        Self {
            stage: if first_boot {
                trademark_stage::GOTO_DEMO
            } else {
                trademark_stage::PICK_DEMO
            },
            alpha: TRADEMARK_ALPHA_MAX,
            alpha2: 0.0,
            logo_timer: TRADEMARK_LOGO_TIMER,
            move_timer: TRADEMARK_MOVE_TIMER,
            cancel: false,
            check: false,
        }
    }
}

/// Side effects the C stage machine triggers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrademarkEvent {
    None,
    /// Play the lost-fanfare for the chosen demo (`s_titlebgm[no]`).
    PlayFanfare(u8),
    /// Stage 5 reached: run `trademark_goto_demo_scene`.
    GotoDemo,
}

/// Advance the trademark state machine one frame.
/// `dt_frames` is `graph->dt_num_60fps_frames`; `titledemo_no` is the
/// zero-based demo index for the stage-0 fanfare.
pub fn trademark_move_step(
    s: &mut TrademarkState,
    dt_frames: f64,
    titledemo_no: usize,
) -> TrademarkEvent {
    let dt = dt_frames as f32;
    let mut event = TrademarkEvent::None;

    if s.stage == trademark_stage::PICK_DEMO {
        event = TrademarkEvent::PlayFanfare(TITLE_DEMO_BGM[titledemo_no % TITLE_DEMO_NUM]);
        s.alpha = 0.0;
        s.stage = trademark_stage::WAIT;
    }

    if s.stage == trademark_stage::WAIT {
        s.move_timer -= dt;
        if s.move_timer <= 0.0 {
            s.stage = trademark_stage::LOGO_FADE_IN;
        }
    }

    // nintendo_logo_move
    if s.stage == trademark_stage::LOGO_FADE_IN {
        s.alpha2 += TRADEMARK_FADE_RATE * dt;
        if s.alpha2 >= TRADEMARK_ALPHA_MAX {
            s.stage = trademark_stage::LOGO_HOLD;
            s.alpha2 = TRADEMARK_ALPHA_MAX;
        }
    } else if s.stage == trademark_stage::LOGO_HOLD {
        s.logo_timer -= dt;
        if s.logo_timer <= 0.0 {
            s.stage = trademark_stage::FADE_OUT;
        }
    }

    if s.stage == trademark_stage::FADE_OUT || s.cancel {
        if s.alpha < TRADEMARK_ALPHA_MAX {
            s.alpha += TRADEMARK_FADE_RATE * dt;
        }
        if !s.check {
            s.check = true;
        }
        if s.alpha >= TRADEMARK_ALPHA_MAX && s.check {
            s.alpha = TRADEMARK_ALPHA_MAX;
            s.stage = trademark_stage::GOTO_DEMO;
        }
    }

    if s.stage == trademark_stage::GOTO_DEMO {
        event = TrademarkEvent::GotoDemo;
    }
    event
}

/// START-button cancel (`trademark_cancel`): only at stage
/// `LOGO_HOLD`, with a connected pad, START held, and not already
/// cancelled.
pub fn trademark_cancel_check(
    stage: u8,
    cancel: bool,
    pad_connected: bool,
    start_pressed: bool,
) -> bool {
    if cancel {
        return true;
    }
    stage == trademark_stage::LOGO_HOLD && pad_connected && start_pressed
}

// ---------------------------------------------------------------------------
// C ABI: stateful demo-id / frame-id accessors (backed by a Mutex, following
// the aram.rs pattern). Pure helpers above are also exported where the
// decomp declares them.
// ---------------------------------------------------------------------------

struct TitleDemoState {
    demono: i32,
    frame_30fps: i32,
}

static DEMO_STATE: Mutex<TitleDemoState> = Mutex::new(TitleDemoState {
    demono: TITLEDEMO_LOGO,
    frame_30fps: 0,
});

#[no_mangle]
pub extern "C" fn mTD_demono_get() -> c_int {
    let mut st = DEMO_STATE.lock().unwrap();
    st.demono = demono_next(st.demono);
    st.demono as c_int
}

#[no_mangle]
pub extern "C" fn mTD_get_titledemo_no() -> c_int {
    let st = DEMO_STATE.lock().unwrap();
    titledemo_index(st.demono) as c_int
}

#[no_mangle]
pub extern "C" fn mTD_tdemo_button_ok_check() -> c_int {
    let st = DEMO_STATE.lock().unwrap();
    demo_button_ok(st.frame_30fps) as c_int
}

/// Feed one decoded key-data sample into C-provided out-params
/// (new helper; the decomp calls `mPlib_SetData1_...` directly).
#[no_mangle]
pub extern "C" fn pc_titledemo_decode_keydata(
    keydata: u16,
    stick_x: *mut i8,
    stick_y: *mut i8,
    btn_a: *mut c_int,
    btn_b: *mut c_int,
) {
    let d = decode_keydata(keydata);
    unsafe {
        if !stick_x.is_null() {
            *stick_x = d.stick_x;
        }
        if !stick_y.is_null() {
            *stick_y = d.stick_y;
        }
        if !btn_a.is_null() {
            *btn_a = d.btn_a as c_int;
        }
        if !btn_b.is_null() {
            *btn_b = d.btn_b as c_int;
        }
    }
}

/// Advance the C-side 30 fps frame/accumulator pair in place
/// (new helper mirroring `set_player_demo_keydata_hold`'s advance).
#[no_mangle]
pub extern "C" fn pc_titledemo_advance_frame(
    frame: *mut c_int,
    accum: *mut f32,
    delta_time: f32,
) {
    unsafe {
        if frame.is_null() || accum.is_null() {
            return;
        }
        let (f, a) = advance_demo_frame(*frame, *accum, delta_time);
        *frame = f;
        *accum = a;
        DEMO_STATE.lock().unwrap().frame_30fps = demo_frame_clamp(f);
    }
}
