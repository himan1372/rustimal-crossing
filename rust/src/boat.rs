//! Kapp'n's boat ferry — pure data and kernels.
//!
//! Verified against `src/actor/ac_boat.c`,
//! `src/actor/ac_boat_move.c_inc`, `src/actor/ac_boat_demo.c`,
//! `src/actor/ac_boat_demo_move.c_inc`, `include/ac_boat_demo.h`,
//! `include/m_name_table.h`, `include/m_lib.h`, `include/types.h`
//! (GAFE01_00 Rev 0 USA decomp). All 10 brief claims checked verbatim;
//! only two precision nuances vs the brief:
//!
//! - The rudder-step halving is `(s16)(step * 0.5f32)` (float multiply,
//!   truncate), not `/ 2.0f32`.
//! - The vertical "reversal" sets the velocity *cap* (`max_velocity_y`
//!   ±0.1f32) and re-rolls gravity; it does not flip the current velocity.
//!
//! Architecture: the boat actor, the boat-demo controller, Kapp'n's NPC
//! actor, the player demo requests, the frame-driven handshakes, the
//! GBA transfer, and the island data transition all stay in C. This
//! module holds only the source-proven pure data and calculations with
//! primitive arguments.

/// Kapp'n's identity (`include/m_name_table.h`).
pub const SP_NPC_SENDO: u16 = 0xD072;

/// GBA island transfer sizes, USA (`include/ac_boat_demo.h`).
pub mod gba {
    pub const LOADER_SIZE: u32 = 0xC83C;
    /// USA program size; the Australian build uses 0x15248 (+4).
    pub const PROGRAM_SIZE: u32 = 0x15244;
    pub const PROGRAM_SIZE_AUS: u32 = 0x15248;
}

/// `DEG2SHORT_ANGLE` (`include/m_lib.h`).
pub fn deg2short(deg: f32) -> i16 {
    (deg * (65536.0f32 / 360.0f32)) as i16
}

/// Fixed route checkpoints (`aBT_chk_point[]`), (x, z) with the source's
/// F/G/H-5 unit comments.
pub const CHK_POINTS: [(f32, f32); 8] = [
    (3420.0, 4260.0), // F-5 unit  5.5 - 10.5
    (3380.0, 4300.0), // F-5 unit  4.5 - 11.5
    (3340.0, 4380.0), // F-5 unit  3.5 - 13.5
    (3340.0, 4620.0), // G-5 unit  3.5 -  3.5
    (3740.0, 4980.0), // G-5 unit 13.5 - 12.5
    (3740.0, 5220.0), // H-5 unit 13.5 -  2.5
    (3700.0, 5300.0), // H-5 unit 12.5 -  4.5
    (3660.0, 5340.0), // H-5 unit 11.5 -  5.5
];

/// Route direction data (`ac_boat.c`, `ac_boat_move.c_inc`).
pub mod route {
    /// `aBT_last_point_idx`: index 0 = to-island, 1 = from-island.
    pub const LAST_POINT_IDX: [i32; 2] = [7, 0];
    /// First checkpoint per direction (`aBT_move_init`).
    pub const FIRST_CHK_POINT: [i32; 2] = [1, 6];
    /// Initial Y angles: `DEG2SHORT_ANGLE(-45.0f32)` = 0xE000,
    /// `DEG2SHORT_ANGLE(135.0f32)` = 0x6000.
    pub const INIT_ANGLE_Y: [i16; 2] = [-8192, 24576];
    /// `aBT_actor_ct`: `block_z == 6` → to-island, otherwise from-island.
    pub const TO_ISLAND_BLOCK_Z: u8 = 6;
}

/// True when the boat heads to the island (`block_z == 6`).
pub fn to_island(block_z: u8) -> bool {
    block_z == route::TO_ISLAND_BLOCK_Z
}

/// Checkpoint advance rule (`aBT_ctrl_direction` tail): progress depends
/// on the sign of dZ only, not distance. To-island advances when
/// `dZ < 0`; from-island when `dZ > 0`.
pub fn should_advance_checkpoint(to_island: bool, dz: f32) -> bool {
    if to_island {
        dz < 0.0
    } else {
        dz > 0.0
    }
}

/// Kapp'n song tables (`aBTD_sing_sendo_start_wait_init`,
/// `aBTD_sing_sendo_start_wait2_init`, `aBTD_sendo_dialogue_start_wait_init`).
pub mod song {
    /// `[at_island][gender]` → player pattern 0–3.
    pub const PLAYER_PATTERN: [[u8; 2]; 2] = [[0, 1], [2, 3]];
    /// First-part BGM per pattern per song (5 songs).
    pub const FIRST_PART_BGM: [[u8; 5]; 4] = [
        [0x5C, 0x5D, 0x5E, 0x5F, 0x60],
        [0x66, 0x67, 0x68, 0x69, 0x6A],
        [0x61, 0x62, 0x63, 0x64, 0x65],
        [0x6B, 0x6C, 0x6D, 0x6E, 0x6F],
    ];
    /// First-part message base per pattern.
    pub const FIRST_PART_MSG_BASE: [u16; 4] = [0x3091, 0x3163, 0x3096, 0x3168];
    /// Post-first-song dialogue message base per pattern (bonus table,
    /// not in the brief): `base[pattern] + sing_dialog_ofs`.
    pub const DIALOGUE_MSG_BASE: [u16; 4] = [0x309B, 0x316D, 0x30A5, 0x3177];
    /// Second-part BGM per `sing_2nd_part`.
    pub const SECOND_PART_BGM: [u8; 10] =
        [0x70, 0x71, 0x72, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79];
    /// Second-part message base per pattern.
    pub const SECOND_PART_MSG_BASE: [u16; 4] = [0x30AF, 0x3181, 0x30AF, 0x3181];
    /// Initial value of `last_song_to_island` / `last_song_from_island`
    /// (`m_start_data_init.c`): a u8 field set to -1, i.e. 0xFF, which can
    /// never equal a song index 0–4 — so any song is accepted first ride.
    pub const LAST_SONG_INIT: u8 = 0xFF;
}

/// Player song pattern from `at_island` and player gender.
pub fn player_pattern(at_island: bool, gender: u8) -> u8 {
    song::PLAYER_PATTERN[at_island as usize][(gender & 1) as usize]
}

/// Repeat-rejection acceptance: a rolled song is kept only if it differs
/// from the saved last song for that direction. The decomp loop is
/// `do { kappn_song = RANDOM(5); ... } while (TRUE);` — the rolls stay in
/// C; this is the per-roll acceptance test.
pub fn song_accepted(kappn_song: u8, last_song: u8) -> bool {
    kappn_song != last_song
}

/// First-part BGM number for a pattern and song.
pub fn first_part_bgm(pattern: u8, kappn_song: u8) -> u8 {
    song::FIRST_PART_BGM[(pattern & 3) as usize][(kappn_song % 5) as usize]
}

/// First-part message: `sing_1st_part_msg_base_no[pattern] + kappn_song`.
pub fn first_part_msg(pattern: u8, kappn_song: u8) -> u16 {
    song::FIRST_PART_MSG_BASE[(pattern & 3) as usize] + (kappn_song % 5) as u16
}

/// Post-first-song dialogue message: `base[pattern] + sing_dialog_ofs`.
pub fn dialogue_msg(pattern: u8, dialog_ofs: u8) -> u16 {
    song::DIALOGUE_MSG_BASE[(pattern & 3) as usize] + (dialog_ofs % 10) as u16
}

/// Second-part BGM number.
pub fn second_part_bgm(sing_2nd_part: u8) -> u8 {
    song::SECOND_PART_BGM[(sing_2nd_part % 10) as usize]
}

/// Second-part message: `sing_2nd_part_msg_base_no[pattern] + sing_2nd_part`.
pub fn second_part_msg(pattern: u8, sing_2nd_part: u8) -> u16 {
    song::SECOND_PART_MSG_BASE[(pattern & 3) as usize] + (sing_2nd_part % 10) as u16
}

/// USA FRAMERATE identity values (`include/types.h`; the PAL branch
/// adjusts, USA is identity).
pub mod timer {
    /// `aBT_calc_speed` travel target: `FRAMERATE_ADJ(0.625f32)`.
    pub const TRAVEL_SPEED: f32 = 0.625;
    /// `aBT_calc_speed` endpoint approach step: `FRAMERATE_SELECT(0.0017f32, 0.0021f32)` picks the 60 Hz value.
    pub const APPROACH_STEP: f32 = 0.0017;
    /// Song timer: `FRAMERATE_TIMER(930)`.
    pub const SONG_TIMER: f32 = 930.0;
    /// Wharf-touch timer: `FRAMERATE_TIMER(190)`.
    pub const WHARF_TIMER: f32 = 190.0;
}

// ---- C ABI ----
// All pc_-prefixed: the C originals stay compiled; these are staged kernels.

#[no_mangle]
pub extern "C" fn pc_boat_to_island(block_z: u8) -> u8 {
    to_island(block_z) as u8
}

#[no_mangle]
pub extern "C" fn pc_boat_should_advance_checkpoint(to_island: u8, dz: f32) -> u8 {
    should_advance_checkpoint(to_island != 0, dz) as u8
}

#[no_mangle]
pub extern "C" fn pc_boat_player_pattern(at_island: u8, gender: u8) -> u8 {
    player_pattern(at_island != 0, gender)
}

#[no_mangle]
pub extern "C" fn pc_boat_song_accepted(kappn_song: u8, last_song: u8) -> u8 {
    song_accepted(kappn_song, last_song) as u8
}

#[no_mangle]
pub extern "C" fn pc_boat_first_part_bgm(pattern: u8, kappn_song: u8) -> u8 {
    first_part_bgm(pattern, kappn_song)
}

#[no_mangle]
pub extern "C" fn pc_boat_first_part_msg(pattern: u8, kappn_song: u8) -> u16 {
    first_part_msg(pattern, kappn_song)
}

#[no_mangle]
pub extern "C" fn pc_boat_dialogue_msg(pattern: u8, dialog_ofs: u8) -> u16 {
    dialogue_msg(pattern, dialog_ofs)
}

#[no_mangle]
pub extern "C" fn pc_boat_second_part_bgm(sing_2nd_part: u8) -> u8 {
    second_part_bgm(sing_2nd_part)
}

#[no_mangle]
pub extern "C" fn pc_boat_second_part_msg(pattern: u8, sing_2nd_part: u8) -> u16 {
    second_part_msg(pattern, sing_2nd_part)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_and_gba() {
        assert_eq!(SP_NPC_SENDO, 0xD072);
        assert_eq!(gba::LOADER_SIZE, 0xC83C);
        assert_eq!(gba::PROGRAM_SIZE, 0x15244);
        assert_eq!(gba::PROGRAM_SIZE_AUS, 0x15248);
    }

    #[test]
    fn deg2short_angles() {
        assert_eq!(deg2short(-45.0), -8192); // 0xE000
        assert_eq!(deg2short(135.0), 24576); // 0x6000
        assert_eq!(route::INIT_ANGLE_Y, [deg2short(-45.0), deg2short(135.0)]);
    }

    #[test]
    fn route_data() {
        assert_eq!(CHK_POINTS.len(), 8);
        assert_eq!(CHK_POINTS[0], (3420.0, 4260.0));
        assert_eq!(CHK_POINTS[7], (3660.0, 5340.0));
        assert_eq!(route::LAST_POINT_IDX, [7, 0]);
        assert_eq!(route::FIRST_CHK_POINT, [1, 6]);
        assert!(to_island(6));
        assert!(!to_island(5));
        assert!(should_advance_checkpoint(true, -1.0));
        assert!(!should_advance_checkpoint(true, 1.0));
        assert!(should_advance_checkpoint(false, 1.0));
        assert!(!should_advance_checkpoint(false, -1.0));
    }

    #[test]
    fn song_selection() {
        assert_eq!(player_pattern(false, 0), 0);
        assert_eq!(player_pattern(false, 1), 1);
        assert_eq!(player_pattern(true, 0), 2);
        assert_eq!(player_pattern(true, 1), 3);
        // 0xFF init accepts any song
        for s in 0..5u8 {
            assert!(song_accepted(s, song::LAST_SONG_INIT));
        }
        assert!(!song_accepted(2, 2));
        assert!(song_accepted(3, 2));
        assert_eq!(first_part_bgm(0, 0), 0x5C);
        assert_eq!(first_part_bgm(3, 4), 0x6F);
        assert_eq!(first_part_msg(0, 0), 0x3091);
        assert_eq!(first_part_msg(1, 4), 0x3163 + 4);
        assert_eq!(dialogue_msg(2, 7), 0x30A5 + 7);
        assert_eq!(second_part_bgm(0), 0x70);
        assert_eq!(second_part_bgm(9), 0x79);
        assert_eq!(second_part_msg(3, 9), 0x3181 + 9);
    }
}

#[cfg(test)]
mod tests_wave4 {
    use super::*;

    #[test]
    fn timer_and_gba_sizes() {
        assert_eq!(timer::TRAVEL_SPEED, 0.625);
        assert_eq!(timer::APPROACH_STEP, 0.0017);
        assert_eq!(timer::SONG_TIMER, 930.0);
        assert_eq!(timer::WHARF_TIMER, 190.0);
        // Australian program is exactly 4 bytes larger.
        assert_eq!(gba::PROGRAM_SIZE_AUS - gba::PROGRAM_SIZE, 4);
        assert!(gba::LOADER_SIZE < gba::PROGRAM_SIZE);
    }

    #[test]
    fn route_geometry_and_msg_wrap() {
        // Checkpoints run monotonically in z (the route heads "south").
        let mut prev = f32::NEG_INFINITY;
        for &(_, z) in CHK_POINTS.iter() {
            assert!(z > prev, "checkpoint z not increasing");
            prev = z;
        }
        // All checkpoints sit in the F/G/H-5 acre band.
        for &(x, z) in CHK_POINTS.iter() {
            assert!((3300.0..=3800.0).contains(&x), "x={x}");
            assert!((4200.0..=5400.0).contains(&z), "z={z}");
        }
        // Message helpers wrap their indices.
        assert_eq!(first_part_msg(0, 5), first_part_msg(0, 0));
        assert_eq!(second_part_bgm(10), second_part_bgm(0));
        assert_eq!(dialogue_msg(1, 10), dialogue_msg(1, 0));
        // second-part bases are paired: pattern 0 == 2, 1 == 3.
        assert_eq!(song::SECOND_PART_MSG_BASE[0], song::SECOND_PART_MSG_BASE[2]);
        assert_eq!(song::SECOND_PART_MSG_BASE[1], song::SECOND_PART_MSG_BASE[3]);
        // player_pattern masks gender to one bit.
        assert_eq!(player_pattern(true, 7), player_pattern(true, 1));
        // C ABI spot checks.
        assert_eq!(pc_boat_to_island(6), 1);
        assert_eq!(pc_boat_to_island(0), 0);
        assert_eq!(pc_boat_should_advance_checkpoint(1, 0.0), 0); // dz == 0: no advance
        assert_eq!(pc_boat_first_part_msg(0, 5), pc_boat_first_part_msg(0, 0));
    }
}
