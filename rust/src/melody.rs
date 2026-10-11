//! Town tune composer — melody encoding and editor transitions.
//!
//! Verified against `src/game/m_melody.c`, `include/m_melody.h`,
//! `src/game/m_mscore_ovl.c`, `src/actor/ac_mscore_control.c`,
//! `include/ac_mscore_control.h`, `include/m_name_table.h`,
//! `src/game/m_bgm.c`, `src/game/m_start_data_init.c`
//! (GAFE01_00 Rev 0 USA decomp). All 12 brief claims checked; no
//! behavioral corrections.
//!
//! Nuances carried: values 3–6 are the LOW-octave variants
//! (C_LOW/D_LOW/E_LOW/F_LOW); the 0→15 jump is silent while the
//! 15→14 decrement plays `sAdo_SysTrgStart(0)`; the
//! `#if VERSION >= VER_GAFU01_00` force-next branch does NOT compile
//! for GAFE01_00; and an unmentioned consumer exists — `m_bgm.c`'s
//! hourly time-signal plays the saved tune on instruments 0xFE
//! (island) / 0xFF (mainland).
//!
//! Deliberately NOT ported: the overlay input polling, the audio
//! calls, the rendering, the actor's facing/interaction checks, the
//! submenu lifecycle, the GBA transfer dialogue/state machine, and
//! the save itself. The saved u64 stays C-owned; these are the pure
//! encoding and transition kernels.

/// Melody constants (`m_melody.h`, `m_mscore_ovl.c`).
pub mod melody {
    /// Sixteen steps.
    pub const LEN: usize = 16;
    /// Special note values.
    pub const RANDOM: u8 = 13;
    pub const HOLD: u8 = 14;
    pub const OFF: u8 = 15;
}

/// Packed u64 → 16 unpacked steps
/// (`mMld_TransformMelodyData_u64_2_u8`), verbatim: step 0 comes
/// from bits 63–60, step 15 from bits 3–0.
pub fn unpack(packed: u64) -> [u8; melody::LEN] {
    let mut notes = [0u8; melody::LEN];
    for (i, note) in notes.iter_mut().enumerate() {
        *note = ((packed >> (60 - i * 4)) & 0xF) as u8;
    }
    notes
}

/// 16 steps → packed u64 (`mMld_TransformMelodyData_u8_2_u64`),
/// verbatim: the destination is cleared first, and only each
/// value's low four bits are stored.
pub fn pack(notes: &[u8; melody::LEN]) -> u64 {
    let mut packed = 0u64;
    for (i, note) in notes.iter().enumerate() {
        packed |= u64::from(*note & 0xF) << (60 - i as u64 * 4);
    }
    packed
}

/// The default tune (`mMld_SetDefaultMelody`), verbatim. Applied once
/// during new-game setup (`m_start_data_init.c:330` for GAFE01_00;
/// the line-221 twin is in the non-GAFE01_00 branch). Do not
/// substitute a reconstructed familiar melody.
pub const DEFAULT_MELODY: [u8; melody::LEN] = [
    0x7, 0xC, 0xF, 0x7, 0x6, 0xB, 0xF, 0x9,
    0xA, 0xE, 0xD, 0xE, 0x3, 0xF, 0xE, 0xE,
];

/// Clear action (`mMS_move_Obey`), verbatim: the affirmative clear
/// fills all 16 steps with HOLD (14) — not OFF (15) and not 0.
pub fn clear() -> [u8; melody::LEN] {
    [melody::HOLD; melody::LEN]
}

/// Note names for the editor display (`note_moji` ordering). Values
/// 0–6 are the low octave (G A B C D E F low), 7–12 the upper run
/// (G A B C D E), 13 random, 14 hold, 15 off.
pub const NOTE_NAMES: [&str; melody::LEN] = [
    "G", "A", "B", "C", "D", "E", "F",
    "G", "A", "B", "C", "D", "E",
    "Random", "Hold", "Off",
];

/// What the editor does about the note sound after a step change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditSound {
    /// The value did not change; no sound.
    Unchanged,
    /// The value changed but no note sound plays.
    Silent,
    /// The value changed; play the note's sound effect.
    Play,
}

/// C-up transition (`mMS_move_Play`), verbatim: blocked at 13
/// (random); 15 → 0; otherwise increment. The note sound plays only
/// when the result is not 14 or 15.
pub fn step_up(note: u8) -> (u8, EditSound) {
    if note == melody::RANDOM {
        (note, EditSound::Unchanged)
    } else {
        let new = if note == melody::OFF { 0 } else { note + 1 };
        let sound = if new == melody::HOLD || new == melody::OFF {
            EditSound::Silent
        } else {
            EditSound::Play
        };
        (new, sound)
    }
}

/// C-down transition (`mMS_move_Play`), verbatim: blocked at 14
/// (hold); 0 → 15 silently; otherwise decrement with the note sound.
/// Note the asymmetry: decrementing 15 → 14 plays
/// `sAdo_SysTrgStart(0)` (the table's literal hold entry), while the
/// 0 → 15 jump plays nothing at all.
pub fn step_down(note: u8) -> (u8, EditSound) {
    if note == melody::HOLD {
        (note, EditSound::Unchanged)
    } else if note == 0 {
        (melody::OFF, EditSound::Silent)
    } else {
        (note - 1, EditSound::Play)
    }
}

/// Cursor advance (A / C-right), verbatim: stops at index 16 — the
/// separate end/confirmation position, not a seventeenth note.
pub fn cursor_advance(idx: i8) -> i8 {
    if idx != 16 { idx + 1 } else { idx }
}

/// Cursor back (B / C-left), verbatim: stops at step 0.
pub fn cursor_back(idx: i8) -> i8 {
    if idx != 0 { idx - 1 } else { idx }
}

/// Edit-end confirmation result (`mMS_move_Wait`), verbatim:
/// `data1 == 0` → save the working melody and exit; `== 1` →
/// return to editing (reset animation and cursor to step 0); any
/// other value → restore the original entry-time melody (carried
/// through `menu_info->data3` from the actor's buffer) and exit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditEnd {
    Save = 0,
    Continue = 1,
    Restore = 2,
}

pub fn edit_end(data1: u32) -> EditEnd {
    match data1 {
        0 => EditEnd::Save,
        1 => EditEnd::Continue,
        _ => EditEnd::Restore,
    }
}

/// Music-board item IDs (`m_name_table.h`).
pub mod board {
    pub const MUSIC_BOARD0: u16 = 0x000E;
    pub const MUSIC_BOARD1: u16 = 0x000F;
}

/// Whether the item in front is one of the two music boards
/// (the interaction check also requires the ±45° facing test and the
/// submenu preconditions, which stay in C).
pub fn is_music_board(item: u16) -> bool {
    item == board::MUSIC_BOARD0 || item == board::MUSIC_BOARD1
}

/// GBA transfer dialogue/message constants (`ac_mscore_control.c`).
/// The transfer state machine itself stays in C.
pub mod gba {
    /// Forced dialogue message id.
    pub const MSG_FORCE_TALK: u16 = 15834;
    /// Read failure.
    pub const MSG_READ_FAIL: u16 = 0x3DDC;
    /// Connection timeout (`counter > 60`).
    pub const MSG_TIMEOUT: u16 = 0x3DDF;
    /// Send-program success.
    pub const MSG_SEND_OK: u16 = 0x3DE0;
    /// Send-program failure / no GBA.
    pub const MSG_SEND_FAIL: u16 = 0x3DDD;
    /// Connection-wait give-up threshold.
    pub const CONNECT_TIMEOUT: i32 = 60;
}

/// Saved-tune consumers (complete audit, `m_melody.c` and friends):
/// the composer overlay (edit-end save/restore), the GBA read path,
/// `mMld_ActorMakeMelody` hooks (quest manager, NPC talk, Booker ×2,
/// postman, shop, broker), quest-local melodies via
/// `mMld_ActorMakeThisMelody` (island/normal-init talk), and
/// `m_bgm.c`'s hourly time-signal (instruments 0xFE island / 0xFF
/// mainland). Keep the shared save representation authoritative —
/// do not maintain an unrelated Rust-only tune.
pub mod consumers {
    pub const HOURLY_SIGNAL_INST_ISLAND: u16 = 0xFE;
    pub const HOURLY_SIGNAL_INST_MAINLAND: u16 = 0xFF;
}

// ---- C ABI ----
// All pc_-prefixed: the C originals stay compiled; these are staged kernels.

#[no_mangle]
pub extern "C" fn pc_ml_unpack(packed: u64, out: *mut u8) {
    if !out.is_null() {
        let notes = unpack(packed);
        unsafe {
            std::ptr::copy_nonoverlapping(notes.as_ptr(), out, melody::LEN);
        }
    }
}

#[no_mangle]
pub extern "C" fn pc_ml_pack(notes: *const u8) -> u64 {
    if notes.is_null() {
        return 0;
    }
    let notes = unsafe { &*(notes as *const [u8; melody::LEN]) };
    pack(notes)
}

#[no_mangle]
pub extern "C" fn pc_ml_clear(out: *mut u8) {
    if !out.is_null() {
        let notes = clear();
        unsafe {
            std::ptr::copy_nonoverlapping(notes.as_ptr(), out, melody::LEN);
        }
    }
}

/// New note value after C-up.
#[no_mangle]
pub extern "C" fn pc_ml_step_up(note: u8) -> u8 {
    step_up(note).0
}

/// 0 = unchanged, 1 = silent, 2 = play the note sound.
#[no_mangle]
pub extern "C" fn pc_ml_step_up_sound(note: u8) -> u8 {
    match step_up(note).1 {
        EditSound::Unchanged => 0,
        EditSound::Silent => 1,
        EditSound::Play => 2,
    }
}

/// New note value after C-down.
#[no_mangle]
pub extern "C" fn pc_ml_step_down(note: u8) -> u8 {
    step_down(note).0
}

/// 0 = unchanged, 1 = silent, 2 = play the note sound.
#[no_mangle]
pub extern "C" fn pc_ml_step_down_sound(note: u8) -> u8 {
    match step_down(note).1 {
        EditSound::Unchanged => 0,
        EditSound::Silent => 1,
        EditSound::Play => 2,
    }
}

#[no_mangle]
pub extern "C" fn pc_ml_cursor_advance(idx: i8) -> i8 {
    cursor_advance(idx)
}

#[no_mangle]
pub extern "C" fn pc_ml_cursor_back(idx: i8) -> i8 {
    cursor_back(idx)
}

/// 0 = save, 1 = continue editing, 2 = restore the original.
#[no_mangle]
pub extern "C" fn pc_ml_edit_end(data1: u32) -> u8 {
    edit_end(data1) as u8
}

#[no_mangle]
pub extern "C" fn pc_ml_is_music_board(item: u16) -> u8 {
    is_music_board(item) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_nibble_order() {
        let notes: [u8; 16] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];
        let packed = pack(&notes);
        // step 0 in bits 63-60, step 15 in bits 3-0
        assert_eq!((packed >> 60) & 0xF, 0);
        assert_eq!(packed & 0xF, 15);
        assert_eq!(unpack(packed), notes);
        // low-nibble masking only
        let mut wide = notes;
        wide[0] = 0x1F;
        assert_eq!(unpack(pack(&wide))[0], 0xF);
        // default tune round-trips
        assert_eq!(unpack(pack(&DEFAULT_MELODY)), DEFAULT_MELODY);
    }

    #[test]
    fn default_and_clear() {
        assert_eq!(DEFAULT_MELODY, [0x7, 0xC, 0xF, 0x7, 0x6, 0xB, 0xF, 0x9,
                                   0xA, 0xE, 0xD, 0xE, 0x3, 0xF, 0xE, 0xE]);
        // clear is all HOLD (14), not OFF (15) or 0
        assert_eq!(clear(), [14; 16]);
        assert_eq!(NOTE_NAMES[13], "Random");
        assert_eq!(NOTE_NAMES[14], "Hold");
        assert_eq!(NOTE_NAMES[15], "Off");
    }

    #[test]
    fn step_transitions() {
        // C-up endpoints
        assert_eq!(step_up(12), (13, EditSound::Play));
        assert_eq!(step_up(13), (13, EditSound::Unchanged));
        assert_eq!(step_up(14), (15, EditSound::Silent));
        assert_eq!(step_up(15), (0, EditSound::Play));
        assert_eq!(step_up(5), (6, EditSound::Play));
        // C-down endpoints
        assert_eq!(step_down(1), (0, EditSound::Play));
        assert_eq!(step_down(0), (15, EditSound::Silent)); // silent jump
        assert_eq!(step_down(15), (14, EditSound::Play)); // plays SysTrgStart(0)
        assert_eq!(step_down(14), (14, EditSound::Unchanged));
        assert_eq!(step_down(5), (4, EditSound::Play));
    }

    #[test]
    fn cursor_and_edit_end() {
        assert_eq!(cursor_advance(15), 16);
        assert_eq!(cursor_advance(16), 16); // stops at the end position
        assert_eq!(cursor_back(16), 15);
        assert_eq!(cursor_back(0), 0);
        assert_eq!(edit_end(0), EditEnd::Save);
        assert_eq!(edit_end(1), EditEnd::Continue);
        assert_eq!(edit_end(2), EditEnd::Restore);
        assert_eq!(edit_end(99), EditEnd::Restore);
    }

    #[test]
    fn board_and_gba() {
        assert!(is_music_board(0x000E));
        assert!(is_music_board(0x000F));
        assert!(!is_music_board(0x0010));
        assert_eq!(gba::MSG_READ_FAIL, 0x3DDC);
        assert_eq!(gba::MSG_TIMEOUT, 0x3DDF);
        assert_eq!(gba::MSG_SEND_OK, 0x3DE0);
        assert_eq!(gba::MSG_SEND_FAIL, 0x3DDD);
        assert_eq!(gba::CONNECT_TIMEOUT, 60);
        assert_eq!(consumers::HOURLY_SIGNAL_INST_ISLAND, 0xFE);
    }
}
