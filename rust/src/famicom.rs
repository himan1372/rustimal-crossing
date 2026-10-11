//! Famicom emulator subsystem: NES game selection,
//! metadata/save-header validation, the emulator
//! exit wrapper, and the framebuffer transition
//! effects (`m_fbdemo*.c`).
//!
//! Verified against GAFE01_00 Rev 0 USA (see
//! `nes-verification-report.md`; 5/5 sections
//! PROVEN or DIFFERS-with-correction).
//!
//! Scope: the actual 6502 NES core, the GAME state
//! handoff, XFB/display-manager switching, the audio
//! lifecycle, the THA/zelda arena, controller
//! polling, card I/O, the tag-parser side effects,
//! fbdemo rendering, and the house message
//! controller stay in C. This module ports the pure
//! kernels: the furniture→game mapping, the ROM
//! index/metadata mapping, the debug ROM override,
//! the exit combo and countdown machine, the
//! save-header/record checksum validation and
//! repair, the result→message mapping, and the
//! retail-form fade/triforce/wipe1 transition math.
//!
//! Brief corrections carried here: fbdemo draw binds
//! segments 0x0A/0x0B, not 3/4; the fade start
//! frames come from an s8 table — type 1 starts at
//! −6, type 6 at −85; the decomp's `move()`
//! functions carry PC-port dt-accumulators, but a
//! retail-faithful port uses the simpler per-frame
//! arithmetic (used here); the Excitebike/Soccer
//! enum-vs-table ordering is kept as-is (table index
//! and GNO stay separate concepts); NOFILE falls to
//! `default:` → message 0x3071; `special_zelda()`
//! BBRAM repair stays in C; `scene.rs` omits
//! `famicom_emu_cleanup` for the FamicomEmu scene
//! entry (wiring follow-up).

/// Internal ROM count.
pub const INTERNAL_ROM_NUM: i32 = 19;
/// Item-name range for Famicom furniture.
pub const FTR_NAME_MIN: u16 = 0x36A;
pub const FTR_NAME_MAX: u16 = 0x37D;

/// Furniture game-id table (`fFC_game_table`).
pub const FFC_GAME_TABLE: [u8; 20] = [
    1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 14,
];
/// Furniture GBA game-id table (`fFC_agb_game_table`);
/// 0xFF = cannot play on GBA (message 0x3B57).
pub const FFC_AGB_TABLE: [u8; 20] = [
    1, 2, 3, 4, 5, 6, 7, 0xFF, 9, 0xFF, 11, 12, 13, 14, 0xFF, 16, 17, 18, 0xFF, 0xFF,
];
/// No-GBA marker.
pub const AGB_NONE: u8 = 0xFF;

/// The 19 internal titles in metadata-table order.
pub const ROM_TITLES: [&str; 19] = [
    "Clu Clu Land",
    "Balloon Fight",
    "Donkey Kong",
    "Donkey Jr Math",
    "Pinball",
    "Tennis",
    "Golf",
    "Punch Out",
    "Base Ball",
    "New Clu Clu Land",
    "Donkey Kong 3",
    "Donkey Kong Jr",
    "Soccer",
    "Excitebike",
    "Wario's Woods",
    "Ice Climber",
    "Mario Bros",
    "Super Mario Bros",
    "Legend of Zelda",
];

/// Pad button bits (`include/types.h`).
pub const BUTTON_L: u32 = 0x0020;
pub const BUTTON_R: u32 = 0x0010;
pub const BUTTON_X: u32 = 0x0040;
pub const BUTTON_Y: u32 = 0x0080;
pub const BUTTON_Z: u32 = 0x2000;
/// Exit combos.
pub const EXIT_COMBO_A: u32 = BUTTON_L | BUTTON_R | BUTTON_Z;
pub const EXIT_COMBO_B: u32 = BUTTON_L | BUTTON_R | BUTTON_X | BUTTON_Y;
/// Exit countdown in frames.
pub const EXIT_COUNTDOWN: i32 = 60;

/// `FAMICOM_RESULT_*` values.
pub const RESULT_OK: i32 = 0;
pub const RESULT_NOSPACE: i32 = 1;
pub const RESULT_NOENTRY: i32 = 2;
pub const RESULT_BROKEN: i32 = 3;
pub const RESULT_WRONGDEVICE: i32 = 4;
pub const RESULT_WRONGENCODING: i32 = 5;
pub const RESULT_NOCARD: i32 = 6;
pub const RESULT_NOFILE: i32 = 7;

/// Save header size.
pub const SAVE_HEADER_SIZE: usize = 0x40;

/// Furniture index: `name − 0x36A` for names in
/// 0x36A..=0x37D, else 0.
#[no_mangle]
pub extern "C" fn pc_famicom_ftr_idx(name: u16) -> u8 {
    if (FTR_NAME_MIN..=FTR_NAME_MAX).contains(&name) {
        (name - FTR_NAME_MIN) as u8
    } else {
        0
    }
}

/// Game id for a furniture index.
#[no_mangle]
pub extern "C" fn pc_famicom_game_id(ftr_idx: u8) -> u8 {
    FFC_GAME_TABLE[(ftr_idx as usize).min(19)]
}

/// GBA game id for a furniture index (0xFF = no GBA).
#[no_mangle]
pub extern "C" fn pc_famicom_agb_id(ftr_idx: u8) -> u8 {
    FFC_AGB_TABLE[(ftr_idx as usize).min(19)]
}

/// Metadata table index: `nesinfo_tags_set(rom_no − 1)`.
/// The table index and the record's GNO stay separate
/// concepts (e.g. furniture game id 14 selects the
/// record whose GNO is 12).
#[no_mangle]
pub extern "C" fn pc_famicom_meta_idx(rom_no: i32) -> i32 {
    rom_no - 1
}

/// Debug ROM override: `rom_id > 0 && debug != 0 →
/// debug > 0 ? debug : 0`.
#[no_mangle]
pub extern "C" fn pc_famicom_debug_rom(rom_id: i32, debug: i32) -> i32 {
    if rom_id > 0 && debug != 0 {
        if debug > 0 {
            debug
        } else {
            0
        }
    } else {
        rom_id
    }
}

/// Exit combo: `combo = now.button | on.button`
/// equals exactly L+R+Z or L+R+X+Y.
#[no_mangle]
pub extern "C" fn pc_famicom_exit_combo(combo: u32) -> bool {
    combo == EXIT_COMBO_A || combo == EXIT_COMBO_B
}

/// Exit state machine. `load_ok` is the ROM-load
/// check result; `combo_hit` is whether any pad hit
/// an exit combo this frame. Returns (done,
/// countdown, should_return): a failed load marks
/// done with countdown 0; a combo marks done with
/// the 60-frame countdown; once done and the
/// countdown reaches 0 the wrapper returns to the
/// game, otherwise the countdown decrements.
#[no_mangle]
pub extern "C" fn pc_famicom_exit_step(
    done: bool,
    countdown: i32,
    load_ok: bool,
    combo_hit: bool,
    out: *mut i32,
) -> bool {
    let (mut done, mut countdown) = (done, countdown);
    if !done {
        if !load_ok {
            done = true;
            countdown = 0;
        } else if combo_hit {
            done = true;
            countdown = EXIT_COUNTDOWN;
        }
    }
    let should_return = if done {
        if countdown == 0 {
            true
        } else {
            countdown -= 1;
            false
        }
    } else {
        false
    };
    if !out.is_null() {
        unsafe {
            *out = if done { 1 } else { 0 };
            *out.add(1) = countdown;
        }
    }
    should_return
}

/// ROM kind: 0 → filer/memory-card path, >0 →
/// internal ROM, <0 → external (the source negates
/// the index for external games).
#[no_mangle]
pub extern "C" fn pc_famicom_rom_kind(rom_no: i32) -> i32 {
    if rom_no == 0 {
        0
    } else if rom_no > 0 {
        1
    } else {
        2
    }
}

/// Result → house message id. NOFILE falls to
/// `default:` → 0x3071; anything unlisted does the
/// same.
#[no_mangle]
pub extern "C" fn pc_famicom_result_msg(result: i32) -> u16 {
    match result {
        RESULT_NOSPACE => 0x3B50,
        RESULT_NOENTRY => 0x3B51,
        RESULT_BROKEN => 0x3B52,
        RESULT_WRONGDEVICE => 0x3B53,
        RESULT_WRONGENCODING => 0x3B54,
        RESULT_NOCARD => 0x3B55,
        _ => 0x3071,
    }
}

/// u8 wrapping checksum over a byte slice.
pub fn calcsum(data: &[u8]) -> u8 {
    data.iter().fold(0u8, |a, &b| a.wrapping_add(b))
}

/// Save-header validation: 8-byte name match,
/// `calcsum(header, 0x40) == 0`, `_08 == 1`,
/// `_09 == 0`, `headerSize == 0x40`, and the
/// record size matches. `expected_name` points at
/// 8 bytes (e.g. `b"GAFEFSC\x1A"`).
#[no_mangle]
pub extern "C" fn pc_famicom_header_valid(
    header: *const u8,
    expected_name: *const u8,
    data_size: u16,
) -> bool {
    if header.is_null() || expected_name.is_null() {
        return false;
    }
    let h = unsafe { core::slice::from_raw_parts(header, SAVE_HEADER_SIZE) };
    let n = unsafe { core::slice::from_raw_parts(expected_name, 8) };
    h[0..8] == n[..]
        && calcsum(h) == 0
        && h[8] == 1
        && h[9] == 0
        && h[10] == SAVE_HEADER_SIZE as u8
        && u16::from_le_bytes([h[12], h[13]]) == data_size
}

/// Header repair: `checksum −= calcsum(header, 0x40)`
/// (byte 11).
#[no_mangle]
pub extern "C" fn pc_famicom_header_repair(header: *mut u8) {
    if header.is_null() {
        return;
    }
    let h = unsafe { core::slice::from_raw_parts_mut(header, SAVE_HEADER_SIZE) };
    let s = calcsum(h);
    h[11] = h[11].wrapping_sub(s);
}

/// Save-check (looser than load): passes with the
/// load version (`_08 == 1 && _09 == 0`) or the
/// alternate version (`_08 == 0 && _09 == 1`).
/// Returns 0 on success, −1 on failure.
#[no_mangle]
pub extern "C" fn pc_famicom_save_check(
    header: *const u8,
    expected_name: *const u8,
    data_size: u16,
) -> i32 {
    if header.is_null() || expected_name.is_null() {
        return -1;
    }
    let h = unsafe { core::slice::from_raw_parts(header, SAVE_HEADER_SIZE) };
    let n = unsafe { core::slice::from_raw_parts(expected_name, 8) };
    let version_ok = (h[8] == 1 && h[9] == 0) || (h[8] == 0 && h[9] == 1);
    if h[0..8] == n[..]
        && calcsum(h) == 0
        && version_ok
        && h[10] == SAVE_HEADER_SIZE as u8
        && u16::from_le_bytes([h[12], h[13]]) == data_size
    {
        0
    } else {
        -1
    }
}

/// Per-player record validation: `data[0] == 'D'`,
/// checksum 0, `(data[1] >> 4) == 3`,
/// `(data[1] & 0xF) <= 1`, `data[3] == i`.
#[no_mangle]
pub extern "C" fn pc_famicom_record_valid(data: *const u8, len: usize, i: u8) -> bool {
    if data.is_null() || len < 4 {
        return false;
    }
    let d = unsafe { core::slice::from_raw_parts(data, len) };
    d[0] == b'D'
        && calcsum(d) == 0
        && (d[1] >> 4) == 3
        && (d[1] & 0xF) <= 1
        && d[3] == i
}

/// Record repair: sets 'D'/'1'/i and fixes the
/// checksum byte `data[2]`.
#[no_mangle]
pub extern "C" fn pc_famicom_record_repair(data: *mut u8, len: usize, i: u8) {
    if data.is_null() || len < 4 {
        return;
    }
    let d = unsafe { core::slice::from_raw_parts_mut(data, len) };
    d[0] = b'D';
    d[1] = b'1';
    d[3] = i;
    let s = calcsum(d);
    d[2] = d[2].wrapping_sub(s);
}

/// Fade start values: type 1 → (frame −6, alpha
/// 255, timer 10); type 6 → (frame −85, alpha 0,
/// timer 0); others → (0, 0, 0). Writes into
/// `out[3]` = (frame, alpha, timer).
#[no_mangle]
pub extern "C" fn pc_fbdemo_fade_start(ftype: i32, out: *mut f32) {
    let (frame, alpha, timer) = match ftype {
        1 => (-6.0, 255.0, 10.0),
        6 => (-85.0, 0.0, 0.0),
        _ => (0.0, 0.0, 0.0),
    };
    if !out.is_null() {
        unsafe {
            *out = frame;
            *out.add(1) = alpha;
            *out.add(2) = timer;
        }
    }
}

/// Fade step (retail form): if the timer is nonzero
/// it decrements and nothing else changes. Type 7
/// skips frame/alpha progression and never sets
/// done. Otherwise `frame += rate`, clamped at
/// `fade_rate` with done; `alpha = (255*ftimer) /
/// fade_rate` with ftimer ≥ 0; type 1 inverts to
/// `255 − alpha`. Writes (frame, timer, alpha) into
/// `out[3]`; returns done.
#[no_mangle]
pub extern "C" fn pc_fbdemo_fade_step(
    frame: f32,
    timer: u16,
    ftype: i32,
    rate: f32,
    fade_rate: f32,
    out: *mut f32,
) -> bool {
    let (frame, timer, done, alpha) = if timer != 0 {
        (frame, timer - 1, false, -1.0)
    } else if ftype == 7 {
        (frame, timer, false, -1.0)
    } else {
        let mut frame = frame + rate;
        let mut done = false;
        if frame >= fade_rate {
            frame = fade_rate;
            done = true;
        }
        let ftimer = frame.max(0.0);
        let mut alpha = (255.0 * ftimer) / fade_rate;
        if ftype == 1 {
            alpha = 255.0 - alpha;
        }
        (frame, timer, done, alpha)
    };
    if !out.is_null() {
        unsafe {
            *out = frame;
            *out.add(1) = timer as f32;
            *out.add(2) = alpha;
        }
    }
    done
}

/// Triforce type decode: bit 0x80 → texture 1, else
/// bit 0x40 → texture 2, else texture 0; direction
/// = `(type & 0xF) != 1`. Writes (texture,
/// direction) into `out[2]`.
#[no_mangle]
pub extern "C" fn pc_fbdemo_triforce_type(ftype: i32, out: *mut i32) {
    let texture = if ftype & 0x80 != 0 {
        1
    } else if ftype & 0x40 != 0 {
        2
    } else {
        0
    };
    let direction = if (ftype & 0xF) != 1 { 1 } else { 0 };
    if !out.is_null() {
        unsafe {
            *out = texture;
            *out.add(1) = direction;
        }
    }
}

/// Triforce step (retail form): `t = 1 − txt/548`,
/// `d = 0.5*(14+12t)` for textures 1/2 or
/// `0.5*(16+12t)` for texture 0, truncated to int;
/// forward clamps at 548, backward at 0, done at
/// the endpoint. Writes the new txt into `out`;
/// returns done.
#[no_mangle]
pub extern "C" fn pc_fbdemo_triforce_step(
    txt: f32,
    texture: i32,
    forward: bool,
    out: *mut f32,
) -> bool {
    let t = 1.0 - txt / 548.0;
    let d = if texture == 0 {
        (0.5 * (16.0 + 12.0 * t)) as i32
    } else {
        (0.5 * (14.0 + 12.0 * t)) as i32
    } as f32;
    let (txt, done) = if forward {
        let txt = txt + d;
        if txt >= 548.0 {
            (548.0, true)
        } else {
            (txt, false)
        }
    } else {
        let txt = txt - d;
        if txt <= 0.0 {
            (0.0, true)
        } else {
            (txt, false)
        }
    };
    if !out.is_null() {
        unsafe {
            *out = txt;
        }
    }
    done
}

/// Wipe1 type: type == 1 → (direction 1, texY
/// 0x14D); else (direction 0, texY 0x264). Writes
/// into `out[2]`.
#[no_mangle]
pub extern "C" fn pc_fbdemo_wipe1_type(ftype: i32, out: *mut i32) {
    let (direction, tex_y) = if ftype == 1 { (1, 0x14D) } else { (0, 0x264) };
    if !out.is_null() {
        unsafe {
            *out = direction;
            *out.add(1) = tex_y;
        }
    }
}

/// Wipe1 step (retail form): `texY ±=
/// (wipe_rate*3)/rate` with integer division;
/// forward clamps at 612 (= 153*4), reverse at 333
/// (= (int)(83.25*4)), done at the endpoint.
/// Writes the new texY into `out`; returns done.
#[no_mangle]
pub extern "C" fn pc_fbdemo_wipe1_step(
    tex_y: i32,
    direction: i32,
    wipe_rate: i32,
    rate: i32,
    out: *mut i32,
) -> bool {
    let q = (wipe_rate * 3) / rate;
    let (tex_y, done) = if direction != 0 {
        let tex_y = tex_y + q;
        if tex_y >= 612 {
            (612, true)
        } else {
            (tex_y, false)
        }
    } else {
        let tex_y = tex_y - q;
        if tex_y <= 333 {
            (333, true)
        } else {
            (tex_y, false)
        }
    };
    if !out.is_null() {
        unsafe {
            *out = tex_y;
        }
    }
    done
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn furniture_tables() {
        assert_eq!(pc_famicom_ftr_idx(0x36A), 0);
        assert_eq!(pc_famicom_ftr_idx(0x37D), 19);
        assert_eq!(pc_famicom_ftr_idx(0x369), 0);
        assert_eq!(pc_famicom_ftr_idx(0x37E), 0);
        assert_eq!(pc_famicom_game_id(0), 1);
        assert_eq!(pc_famicom_game_id(18), 19);
        assert_eq!(pc_famicom_game_id(19), 14); // repeated final entry
        assert_eq!(pc_famicom_agb_id(7), 0xFF);
        assert_eq!(pc_famicom_agb_id(9), 0xFF);
        assert_eq!(pc_famicom_agb_id(14), 0xFF);
        assert_eq!(pc_famicom_agb_id(18), 0xFF);
        assert_eq!(pc_famicom_agb_id(19), 0xFF);
        assert_eq!(pc_famicom_agb_id(0), 1);
        assert_eq!(FFC_GAME_TABLE.len(), 20);
        assert_eq!(FFC_AGB_TABLE.len(), 20);
        assert_eq!(ROM_TITLES.len(), 19);
    }

    #[test]
    fn rom_and_exit() {
        assert_eq!(pc_famicom_meta_idx(14), 13);
        assert_eq!(pc_famicom_debug_rom(5, 0), 5);
        assert_eq!(pc_famicom_debug_rom(5, 7), 7);
        assert_eq!(pc_famicom_debug_rom(5, -3), 0);
        assert_eq!(pc_famicom_debug_rom(0, 7), 0);
        assert!(pc_famicom_exit_combo(EXIT_COMBO_A));
        assert!(pc_famicom_exit_combo(EXIT_COMBO_B));
        assert!(!pc_famicom_exit_combo(BUTTON_L | BUTTON_R));
        assert!(!pc_famicom_exit_combo(EXIT_COMBO_A | BUTTON_X)); // exact match only
        assert_eq!(pc_famicom_rom_kind(0), 0);
        assert_eq!(pc_famicom_rom_kind(14), 1);
        assert_eq!(pc_famicom_rom_kind(-3), 2);
        assert_eq!(pc_famicom_result_msg(RESULT_NOSPACE), 0x3B50);
        assert_eq!(pc_famicom_result_msg(RESULT_NOCARD), 0x3B55);
        assert_eq!(pc_famicom_result_msg(RESULT_NOFILE), 0x3071);
        assert_eq!(pc_famicom_result_msg(99), 0x3071);
        // Exit machine.
        let mut out = [0i32; 2];
        assert!(!pc_famicom_exit_step(false, 0, true, false, out.as_mut_ptr()));
        assert_eq!((out[0], out[1]), (0, 0));
        assert!(!pc_famicom_exit_step(false, 0, true, true, out.as_mut_ptr())); // combo
        assert_eq!((out[0], out[1]), (1, 59)); // countdown decremented same frame
        assert!(!pc_famicom_exit_step(true, 59, true, false, out.as_mut_ptr()));
        assert_eq!((out[0], out[1]), (1, 58));
        assert!(pc_famicom_exit_step(true, 0, true, false, out.as_mut_ptr()));
        assert!(pc_famicom_exit_step(false, 0, false, false, out.as_mut_ptr())); // load fail → returns now
        assert_eq!((out[0], out[1]), (1, 0));
    }

    #[test]
    fn save_headers() {
        // Build a valid header.
        let mut h = [0u8; 0x40];
        h[0..8].copy_from_slice(b"GAFEFSC\x1A");
        h[8] = 1;
        h[9] = 0;
        h[10] = 0x40;
        h[12] = 0x60;
        h[13] = 0x06; // size 0x660
        pc_famicom_header_repair(h.as_mut_ptr());
        assert!(pc_famicom_header_valid(
            h.as_ptr(),
            b"GAFEFSC\x1A".as_ptr(),
            0x660
        ));
        assert_eq!(pc_famicom_save_check(h.as_ptr(), b"GAFEFSC\x1A".as_ptr(), 0x660), 0);
        // Wrong size.
        assert!(!pc_famicom_header_valid(h.as_ptr(), b"GAFEFSC\x1A".as_ptr(), 0x600));
        // Corrupt checksum.
        h[20] ^= 0xFF;
        assert!(!pc_famicom_header_valid(h.as_ptr(), b"GAFEFSC\x1A".as_ptr(), 0x660));
        assert_eq!(pc_famicom_save_check(h.as_ptr(), b"GAFEFSC\x1A".as_ptr(), 0x660), -1);
        // Alternate version passes save_check but not header_valid.
        let mut h2 = h;
        h2[8] = 0;
        h2[9] = 1;
        pc_famicom_header_repair(h2.as_mut_ptr());
        assert!(!pc_famicom_header_valid(h2.as_ptr(), b"GAFEFSC\x1A".as_ptr(), 0x660));
        assert_eq!(pc_famicom_save_check(h2.as_ptr(), b"GAFEFSC\x1A".as_ptr(), 0x660), 0);
        // Records.
        let mut r = [0u8; 16];
        r[0] = b'D';
        r[1] = b'1';
        r[3] = 2;
        pc_famicom_record_repair(r.as_mut_ptr(), r.len(), 2);
        assert!(pc_famicom_record_valid(r.as_ptr(), r.len(), 2));
        assert!(!pc_famicom_record_valid(r.as_ptr(), r.len(), 3));
        r[1] = 0x32; // low nibble 2 > 1
        assert!(!pc_famicom_record_valid(r.as_ptr(), r.len(), 2));
    }

    #[test]
    fn transitions() {
        // Fade.
        let mut out = [0.0f32; 3];
        pc_fbdemo_fade_start(1, out.as_mut_ptr());
        assert_eq!((out[0], out[1], out[2]), (-6.0, 255.0, 10.0));
        pc_fbdemo_fade_start(6, out.as_mut_ptr());
        assert_eq!((out[0], out[1], out[2]), (-85.0, 0.0, 0.0));
        pc_fbdemo_fade_start(3, out.as_mut_ptr());
        assert_eq!((out[0], out[1], out[2]), (0.0, 0.0, 0.0));
        // Timer gate: nothing else changes (alpha stays -1 = unchanged).
        let mut o = [0.0f32; 3];
        assert!(!pc_fbdemo_fade_step(5.0, 3, 1, 1.0, 60.0, o.as_mut_ptr()));
        assert_eq!((o[0], o[1], o[2]), (5.0, 2.0, -1.0));
        // Type 7 never progresses.
        assert!(!pc_fbdemo_fade_step(5.0, 0, 7, 1.0, 60.0, o.as_mut_ptr()));
        assert_eq!((o[0], o[1]), (5.0, 0.0));
        // Normal progression: frame 0 → alpha 0 at rate 1.
        assert!(!pc_fbdemo_fade_step(0.0, 0, 0, 1.0, 60.0, o.as_mut_ptr()));
        assert!((o[2] - (255.0 * 1.0) / 60.0).abs() < 1e-4);
        // Completion boundary.
        assert!(pc_fbdemo_fade_step(59.0, 0, 0, 1.0, 60.0, o.as_mut_ptr()));
        assert_eq!((o[0], o[2]), (60.0, 255.0));
        // Type 1 inversion: frame 60 → alpha 0.
        assert!(pc_fbdemo_fade_step(59.0, 0, 1, 1.0, 60.0, o.as_mut_ptr()));
        assert_eq!(o[2], 0.0);
        // Negative frame clamped for alpha.
        assert!(!pc_fbdemo_fade_step(-6.0, 0, 0, 1.0, 60.0, o.as_mut_ptr()));
        assert_eq!(o[2], 0.0);
        // Triforce.
        let mut t = [0i32; 2];
        pc_fbdemo_triforce_type(0x80 | 0x02, t.as_mut_ptr());
        assert_eq!(t, [1, 1]);
        pc_fbdemo_triforce_type(0x40 | 0x01, t.as_mut_ptr());
        assert_eq!(t, [2, 0]);
        pc_fbdemo_triforce_type(0x03, t.as_mut_ptr());
        assert_eq!(t, [0, 1]);
        let mut x = [0.0f32; 1];
        assert!(!pc_fbdemo_triforce_step(0.0, 0, true, x.as_mut_ptr()));
        assert!((x[0] - 14.0).abs() < 1e-5); // 0.5*(16+12) = 14
        assert!(pc_fbdemo_triforce_step(547.0, 1, true, x.as_mut_ptr()));
        assert_eq!(x[0], 548.0);
        assert!(pc_fbdemo_triforce_step(1.0, 1, false, x.as_mut_ptr()));
        assert_eq!(x[0], 0.0);
        // Wipe1.
        pc_fbdemo_wipe1_type(1, t.as_mut_ptr());
        assert_eq!(t, [1, 0x14D]);
        pc_fbdemo_wipe1_type(2, t.as_mut_ptr());
        assert_eq!(t, [0, 0x264]);
        let mut y = [0i32; 1];
        assert!(!pc_fbdemo_wipe1_step(333, 1, 60, 30, y.as_mut_ptr())); // q=6
        assert_eq!(y[0], 339);
        assert!(pc_fbdemo_wipe1_step(610, 1, 60, 30, y.as_mut_ptr()));
        assert_eq!(y[0], 612);
        assert!(!pc_fbdemo_wipe1_step(612, 0, 60, 30, y.as_mut_ptr()));
        assert_eq!(y[0], 606);
        assert!(pc_fbdemo_wipe1_step(335, 0, 60, 30, y.as_mut_ptr()));
        assert_eq!(y[0], 333);
    }
}
