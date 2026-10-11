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

/// nesinfo tag-stream parser kernels (`famicom_nesinfo.cpp`).
///
/// The stream is a compact binary command stream: each tag is a 3-byte
/// identifier (`NESTAG_CMD_SIZE` = 3) plus a 1-byte payload length, so the
/// full header is `NESTAG_SIZE` = 4 bytes. The next-tag helper advances by
/// `data + data[3] + 4` and terminates on the END tag or the stream end.
/// (Do not conflate the two constants: the brief's "NESTAG_CMD_SIZE is 4"
/// merged them; the traversal formula is unaffected.)
///
/// VEQ/VNE skip the NEXT TAG when their condition fails (not an arbitrary
/// byte count). OFS resets the save-data cursor; HSC/BBR/QDS advance it.
/// In `update_highscore`, the HSC offset word packs a flag bit:
/// `ofs = ofs_data & 0x7FF`, `bit = (ofs_data >> 15) & 1` — mask before
/// using the offset. `special_zelda()` runs only in process1's load path
/// (mode 1, SPE tag, first payload byte 1, first_play == FALSE) — never
/// during save serialization, where SPE is a no-op log.
/// Stays in C: all three processing passes, ROM patching, save/memory-card
/// IO, BBRAM/ROM buffer ownership.

/// Tag identifier width in bytes (`NESTAG_CMD_SIZE`).
pub const NESTAG_CMD_SIZE: i32 = 3;
/// Full tag header width: 3-byte id + 1-byte length (`NESTAG_SIZE`).
pub const NESTAG_SIZE: i32 = 4;

/// Next-tag advance: `cur + payload_len + NESTAG_SIZE`.
#[no_mangle]
pub extern "C" fn pc_nesinfo_next_tag(cur: usize, payload_len: u8) -> usize {
    cur + payload_len as usize + NESTAG_SIZE as usize
}

/// HSC default-data length: the tag length includes the 2-byte offset
/// word, so defaults are `tag_len - 2` bytes starting at `data[6]`.
#[no_mangle]
pub extern "C" fn pc_nesinfo_hsc_default_len(tag_len: u8) -> u8 {
    tag_len.wrapping_sub(2)
}

/// MOV offset/size and ESZ expansion: values are shifted left by 4
/// (ESZ is in 16-byte units).
#[no_mangle]
pub extern "C" fn pc_nesinfo_shift4(x: u16) -> u32 {
    (x as u32) << 4
}

/// GNO first-play bit: `1 << game_no` in the save-data flags word.
#[no_mangle]
pub extern "C" fn pc_nesinfo_gno_bit(game_no: u8) -> u32 {
    1u32 << game_no
}

/// `update_highscore` HSC offset word: low 11 bits are the offset.
#[no_mangle]
pub extern "C" fn pc_nesinfo_hs_ofs(ofs_data: u16) -> u16 {
    ofs_data & 0x7FF
}

/// `update_highscore` HSC offset word: bit 15 is a flag.
#[no_mangle]
pub extern "C" fn pc_nesinfo_hs_flag(ofs_data: u16) -> u16 {
    (ofs_data >> 15) & 1
}

/// `special_zelda()` BBRAM repair (`famicom_nesinfo.cpp`): marker resets
/// plus the three-slot checksum repair. Pure over the caller-supplied
/// BBRAM buffer; returns 0 and does nothing when the buffer is shorter
/// than 0x530 bytes. Repairs in place: sets [0x000]=0x00, [0x001]=0x5A
/// ('Z'), fills 0x52A-0x52C with 0xFF, zeroes 0x52D-0x52F, fills
/// 0x51E-0x520 with 0x5A, fills 0x521-0x523 with 0xA5 — then for each of
/// the 3 slots sums the name (8B @ +2+8i), items (0x28B @ +0x1A+0x28i),
/// world flags (0x180B @ +0x92+0x180i), the slot-active byte (+0x512+i),
/// and bytes at 0x515+i/0x518+i/0x51B+i, narrows to u16, and rewrites the
/// embedded checksum at 0x524+2i ONLY on mismatch. It does NOT clear slot
/// contents wholesale.
/// Byte-sum helper for the Zelda checksum (`calcSum`).
fn zelda_calc_sum(b: &[u8], off: usize, n: usize) -> u16 {
    b[off..off + n].iter().fold(0u32, |a, &x| a + x as u32) as u16
}

#[no_mangle]
pub extern "C" fn pc_zelda_repair_bbram(bbram: *mut u8, len: usize) -> u8 {
    const NEED: usize = 0x530;
    if bbram.is_null() || len < NEED {
        return 0;
    }
    let b = unsafe { std::slice::from_raw_parts_mut(bbram, len) };
    b[0x000] = 0x00;
    b[0x001] = 0x5A;
    b[0x52A..0x52D].fill(0xFF);
    b[0x52D..0x530].fill(0x00);
    b[0x51E..0x521].fill(0x5A);
    b[0x521..0x524].fill(0xA5);
    for i in 0..3usize {
        let mut checksum: u32 = 0;
        checksum += zelda_calc_sum(b, 2 + i * 8, 8) as u32;
        checksum += zelda_calc_sum(b, 0x1A + i * 0x28, 0x28) as u32;
        checksum += zelda_calc_sum(b, 0x92 + i * 0x180, 0x180) as u32;
        checksum += zelda_calc_sum(b, 0x512 + i, 1) as u32;
        checksum += b[0x515 + i] as u32;
        checksum += b[0x518 + i] as u32;
        checksum += b[0x51B + i] as u32;
        let chksum16 = checksum as u16;
        if b[0x524 + i * 2] != (chksum16 >> 8) as u8 || b[0x525 + i * 2] != chksum16 as u8 {
            b[0x524 + i * 2] = (chksum16 >> 8) as u8;
            b[0x525 + i * 2] = chksum16 as u8;
        }
    }
    1
}

/// Acquisition pools for the 19 NES furniture items (`ftr_list.c`).
/// Pool membership is source-proven; it identifies the acquisition
/// mechanism, not probabilities or availability dates.
pub const POOL_SHOP_A: u8 = 0;
pub const POOL_LOTTERY: u8 = 1;
pub const POOL_HOMEPAGE: u8 = 2;
pub const POOL_ISLAND: u8 = 3;
pub const POOL_SPECIAL_PRESENT: u8 = 4;
pub const POOL_NONE: u8 = 0xFF;

/// Acquisition pool for a `FTR_FAMICOM_COMMONnn` furniture index
/// (0-18): 00/01/04/06 shop-A, 02/03/05/13 lottery, 07/09/10/11/12
/// homepage, 08/14 island, 15-18 special-present.
#[no_mangle]
pub extern "C" fn pc_famicom_pool(ftr_idx: u8) -> u8 {
    match ftr_idx {
        0 | 1 | 4 | 6 => POOL_SHOP_A,
        2 | 3 | 5 | 13 => POOL_LOTTERY,
        7 | 9 | 10 | 11 | 12 => POOL_HOMEPAGE,
        8 | 14 => POOL_ISLAND,
        15 | 16 | 17 | 18 => POOL_SPECIAL_PRESENT,
        _ => POOL_NONE,
    }
}

/// Furniture index of the adjacent Disk System item accepted by the
/// shared profile: `FTR_KOB_DISKSYSTEM8` maps to the 20th table entries —
/// Excitebike texture, Excitebike palette, emulator game ID 14, no
/// GBA-image mapping (0xFF). The `fFC_reference_model_idx_table` is
/// `@unused` (a DnM/DnM+ leftover), all zeros.
pub const FFC_DISKSYSTEM_IDX: u8 = 19;

/// C `famicom_emu_cleanup` responsibilities (the checklist a Rust
/// cleanup callback must cover): display fade-in, `famicom_cleanup()`
/// (bit 2 in room-message-control flags on nonzero), `my_alloc_cleanup()`
/// → `zelda_CleanupArena()`, single-XFB → double-XFB restore plus clearing
/// the framebuffer bookkeeping, `sAdo_SubGameEnd()`. Init failure sets bit
/// 1 (as does a ROM-load check failure in `famicom_emu_main`); cleanup
/// failure sets bit 2. TARGET_PC differs: cleanup frees the 4MB malloc'd
/// heap and toggles `g_pc_nes_active`.

    #[test]
    fn nesinfo_tag_kernels() {
        // Header: 3-byte id + 1-byte length = 4.
        assert_eq!(NESTAG_CMD_SIZE, 3);
        assert_eq!(NESTAG_SIZE, 4);
        // Next-tag advance: cur + len + 4.
        assert_eq!(pc_nesinfo_next_tag(100, 6), 110);
        assert_eq!(pc_nesinfo_next_tag(0, 0), 4);
        // HSC default length = tag_len - 2.
        assert_eq!(pc_nesinfo_hsc_default_len(8), 6);
        assert_eq!(pc_nesinfo_hsc_default_len(2), 0);
        // MOV/ESZ shift.
        assert_eq!(pc_nesinfo_shift4(0x10), 0x100);
        // GNO bit.
        assert_eq!(pc_nesinfo_gno_bit(0), 1);
        assert_eq!(pc_nesinfo_gno_bit(18), 1 << 18);
        // High-score offset word: low 11 bits + flag bit.
        assert_eq!(pc_nesinfo_hs_ofs(0x87FF), 0x7FF);
        assert_eq!(pc_nesinfo_hs_flag(0x87FF), 1);
        assert_eq!(pc_nesinfo_hs_flag(0x07FF), 0);
    }

    #[test]
    fn zelda_repair() {
        let mut b = vec![0xAAu8; 0x800];
        // Too short -> no-op.
        assert_eq!(pc_zelda_repair_bbram(b.as_mut_ptr(), 0x52F), 0);
        assert_eq!(b[0], 0xAA);
        assert_eq!(pc_zelda_repair_bbram(b.as_mut_ptr(), 0x800), 1);
        // Markers.
        assert_eq!(b[0], 0x00);
        assert_eq!(b[1], 0x5A);
        assert_eq!(&b[0x52A..0x52D], &[0xFF; 3]);
        assert_eq!(&b[0x52D..0x530], &[0x00; 3]);
        assert_eq!(&b[0x51E..0x521], &[0x5A; 3]);
        assert_eq!(&b[0x521..0x524], &[0xA5; 3]);
        // Untouched slot data is preserved (not cleared wholesale).
        assert_eq!(b[0x100], 0xAA);
        // Checksum repair: corrupt slot 0's embedded checksum, re-run.
        b[0x524] ^= 0xFF;
        let before = (b[0x524], b[0x525]);
        assert_eq!(pc_zelda_repair_bbram(b.as_mut_ptr(), 0x800), 1);
        // Embedded checksum now matches the computed u16 sum.
        let mut sum: u32 = 0;
        for &x in &b[2..10] { sum += x as u32; }
        for &x in &b[0x1A..0x1A + 0x28] { sum += x as u32; }
        for &x in &b[0x92..0x92 + 0x180] { sum += x as u32; }
        sum += b[0x512] as u32 + b[0x515] as u32 + b[0x518] as u32 + b[0x51B] as u32;
        let s16 = sum as u16;
        assert_eq!((b[0x524], b[0x525]), ((s16 >> 8) as u8, s16 as u8));
        assert_ne!(before, (b[0x524], b[0x525]));
        // Idempotent: a valid checksum is not rewritten.
        let after = (b[0x524], b[0x525]);
        b[0x524] = after.0; b[0x525] = after.1;
        assert_eq!(pc_zelda_repair_bbram(b.as_mut_ptr(), 0x800), 1);
        assert_eq!((b[0x524], b[0x525]), after);
    }

    #[test]
    fn famicom_pools() {
        assert_eq!(pc_famicom_pool(0), POOL_SHOP_A);
        assert_eq!(pc_famicom_pool(2), POOL_LOTTERY);
        assert_eq!(pc_famicom_pool(7), POOL_HOMEPAGE);
        assert_eq!(pc_famicom_pool(8), POOL_ISLAND);
        assert_eq!(pc_famicom_pool(15), POOL_SPECIAL_PRESENT);
        assert_eq!(pc_famicom_pool(18), POOL_SPECIAL_PRESENT);
        assert_eq!(pc_famicom_pool(19), POOL_NONE);
        assert_eq!(FFC_DISKSYSTEM_IDX, 19);
        // 20th table entry: game 14, no AGB mapping.
        assert_eq!(FFC_GAME_TABLE[19], 14);
        assert_eq!(FFC_AGB_TABLE[19], 0xFF);
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
