//! Museum building and exhibit actors — shared display state.
//!
//! Verified against `include/m_museum_display.h`,
//! `src/game/m_museum_display.c`, `src/actor/ac_museum_fossil.c`,
//! `src/actor/ac_museum_picture.c`, `src/actor/ac_museum_fish.c`,
//! `src/actor/ac_museum_insect.c`, `src/actor/ac_museum_indoor.c`,
//! `src/actor/ac_museum.c`, `include/m_name_table.h`,
//! `include/m_room_type.h` (GAFE01_00 Rev 0 USA decomp). All 12
//! brief claims checked; no behavioral corrections.
//!
//! Nuances carried: the picture actor's talk check uses different
//! offsets (+60/+40, not +20/+20) and is gated on a 16-entry
//! facing-direction table; the fossil talk-position table is a
//! SEPARATE table from the floor-placement table (coordinates
//! differ — never derive one from the other); fish/insect actors use
//! MSG_12194/12195 for non-final continuation entries; the fish
//! station arrays resolve to raw 0-based indices; the fish actor's
//! world position orbits each frame (preserve, don't "fix").
//!
//! Scope split: the sibling `museum` module owns the fossil
//! assessment kernels and the donor constants. This module owns the
//! nibble-packed display record, the placement/talk tables, the
//! station lists, the message selection, the room alpha, and the
//! exterior values. Species simulation, rendering, camera, player
//! rotation, and the save stay in C.

/// Category counts and packed sizes (`m_museum_display.h`).
pub mod category {
    pub const FOSSIL_NUM: usize = 25;
    pub const ART_NUM: usize = 15;
    pub const FISH_NUM: usize = 40;
    pub const INSECT_NUM: usize = 40;
    pub const FOSSIL_BIT_NUM: usize = 13;
    pub const ART_BIT_NUM: usize = 8;
    pub const FISH_BIT_NUM: usize = 21;
    pub const INSECT_BIT_NUM: usize = 21;
    /// `sizeof(mMmd_info_c)`.
    pub const RECORD_SIZE: usize = 0x3F;
}

/// Donor status values (shared with the sibling `museum` module).
pub mod donor {
    pub const NONE: u8 = 0;
    pub const PLAYER1: u8 = 1;
    pub const PLAYER2: u8 = 2;
    pub const PLAYER3: u8 = 3;
    pub const PLAYER4: u8 = 4;
    pub const DELETED_PLAYER: u8 = 5;
}

/// Read a 4-bit donor field (`mMmd_BIT_INFO`), verbatim: even indices
/// read the low nibble, odd indices the high nibble.
pub fn bit_info(bits: &[u8], index: u8) -> u8 {
    match bits.get((index >> 1) as usize) {
        Some(&b) => (b >> (((index & 1) << 2))) & 0x0F,
        None => 0,
    }
}

/// Store a 4-bit donor field: the retail setters clear the old value
/// first (`mMmd_*_CLR`), then OR the new one (`mMmd_*_SET`).
pub fn bit_store(bits: &mut [u8], index: u8, value: u8) {
    if let Some(b) = bits.get_mut((index >> 1) as usize) {
        let shift = ((index & 1) * 4) as u32;
        *b &= !(0x0F << shift);
        *b |= (value & 0x0F) << shift;
    }
}

/// Bounds-checked query (`mMmd_*Info`), verbatim: an out-of-range
/// index returns 0.
pub fn query(bits: &[u8], index: i32, count: usize) -> u8 {
    if index >= 0 && (index as usize) < count {
        bit_info(bits, index as u8)
    } else {
        0
    }
}

/// Donor predicates (`mMmd_IS_DONATED` / `mMmd_DONATOR_EXISTS` /
/// `mMmd_DONATOR_PLR_IDX`), verbatim.
pub fn is_donated(donator: u8) -> bool {
    (donor::PLAYER1..=donor::DELETED_PLAYER).contains(&donator)
}

pub fn donator_exists(donator: u8) -> bool {
    (donor::PLAYER1..=donor::PLAYER4).contains(&donator)
}

pub fn donator_plr_idx(donator: u8) -> u8 {
    donator.wrapping_sub(1) & 3
}

/// Fossil floor-placement table (`mMmd_museum_fossil_data`), verbatim
/// (x, z, rotation). The dummy furniture items are C-side symbols
/// (`FTR_START(FTR_DIN_*_DUMMY)`); the caller passes the right one.
/// Rotations use the 0-based direction enum: SOUTH=0, EAST=1,
/// NORTH=2, WEST=3.
pub mod fossil_floor {
    pub const ENTRIES: [(u8, u8, u8); 25] = [
        (5, 2, 2), (1, 2, 2), (3, 2, 2),
        (9, 2, 0), (13, 2, 0), (11, 2, 0),
        (5, 10, 2), (1, 10, 2), (3, 10, 2),
        (10, 6, 1), (13, 5, 0), (11, 5, 0),
        (6, 6, 0), (6, 5, 0), (6, 7, 0),
        (9, 10, 0), (11, 10, 0), (13, 10, 0),
        (2, 7, 1), (2, 5, 1),
        (6, 8, 0), (7, 8, 0), (8, 8, 0), (9, 8, 0), (10, 8, 0),
    ];

    /// Pack a floor unit (`mMmd_UT`): `((z & 0xF) << 4) | (x & 0xF)`.
    pub fn ut(x: u8, z: u8) -> u8 {
        ((z & 0xF) << 4) | (x & 0xF)
    }
}

/// Donated-fossil item formula (`mMmd_MuseumFossilProcess_MakeFgData`),
/// verbatim: `FTR_START(FTR_DIN_TRIKERA_HEAD) + FTR_NO_2_IDX(idx)`,
/// then `+ (rotation & 3)`. The base 0x1EEC is derived (FTR0 enum at
/// 0x1000, 4 ids per entry, TRIKERA_HEAD the 956th entry);
/// `FTR_NO_2_IDX(f) = (f) << 2`.
pub mod fossil_item {
    pub const DONATED_BASE: u16 = 0x1EEC;

    pub fn donated(fossil_idx: u8, rotation: u8) -> u16 {
        DONATED_BASE + ((fossil_idx as u16) << 2) + ((rotation & 3) as u16)
    }

    /// Undonated entries use the table's dummy item `+ (rotation & 3)`.
    pub fn dummy(dummy_item: u16, rotation: u8) -> u16 {
        dummy_item + ((rotation & 3) as u16)
    }
}

/// Fossil talk-position table (`aMF_fossil_data_table`), verbatim —
/// a SEPARATE table from the floor placement (coordinates differ;
/// never derive one from the other). (x, z, direction).
pub mod fossil_talk {
    pub const ENTRIES: [(u8, u8, u8); 25] = [
        (6, 3, 0), (2, 3, 0), (4, 3, 0),
        (9, 3, 0), (13, 3, 0), (11, 3, 0),
        (6, 11, 0), (2, 11, 0), (4, 11, 0),
        (10, 6, 0), (13, 6, 0), (11, 6, 0),
        (6, 6, 3), (6, 5, 3), (6, 7, 3),
        (9, 11, 0), (11, 11, 0), (13, 11, 0),
        (3, 8, 1), (3, 6, 1),
        (6, 8, 0), (7, 8, 0), (8, 8, 0), (9, 8, 0), (10, 8, 0),
    ];
    /// Talk proximity distance (`mRmTp_CheckTalkAble(..., 42.5f)`).
    pub const DIST: f32 = 42.5;
}

/// Artwork positions (`aMP_art_data_table`), verbatim (x, z). The
/// display-list pointers are C-side; donated → real model,
/// undonated → dummy model.
pub mod art {
    pub const POSITIONS: [(u8, u8); 15] = [
        (7, 1), (1, 9), (11, 5), (9, 1), (5, 9),
        (3, 1), (3, 9), (1, 5), (3, 5), (11, 9),
        (11, 1), (13, 5), (13, 9), (9, 5), (5, 1),
    ];

    /// Draw position: `x * 40 + 20`, `z * 40 + 20`, `y = 40`.
    pub fn draw_pos(x: u8, z: u8) -> (f32, f32, f32) {
        (x as f32 * 40.0 + 20.0, 40.0, z as f32 * 40.0 + 20.0)
    }

    /// Talk-check position: DIFFERENT offsets from the draw position
    /// (`(x & 15) * 40 + 60`, `(z & 15) * 40 + 40`, y = 40).
    pub fn talk_pos(x: u8, z: u8) -> (f32, f32, f32) {
        ((x & 15) as f32 * 40.0 + 60.0, 40.0, (z & 15) as f32 * 40.0 + 40.0)
    }

    /// The 16-entry facing-direction gate (`direct_table`), indexed
    /// by `(rotation.y >> 12) & 0xF`; -1 = no valid facing.
    /// Values are the 0-based directions (SOUTH=0, EAST=1, NORTH=2,
    /// WEST=3), checked as `(direct & 3) == table[...]`.
    pub const DIRECT_TABLE: [i8; 16] = [
        2, -1, -1, 3, 3, -1, -1, 0, 0, -1, -1, 1, 1, -1, -1, 2,
    ];

    /// Talk proximity: radial 33.0f, gated on the direction table.
    pub const DIST: f32 = 33.0;
}

/// Donor-sensitive talk message selection (fossil and picture
/// actors), verbatim: no donor → 12193; deleted donor → 12192;
/// otherwise 12191.
pub mod talk_msg {
    pub const NO_DONOR: u16 = 12193;
    pub const DELETED_DONOR: u16 = 12192;
    pub const DONATED: u16 = 12191;
    /// Fish/insect continuation entries (not the last one):
    /// 12194 when the donor still exists, 12195 when deleted.
    pub const CONTINUE: u16 = 12194;
    pub const CONTINUE_DELETED: u16 = 12195;
}

pub fn talk_msg(donator: u8) -> u16 {
    if donator == donor::NONE {
        talk_msg::NO_DONOR
    } else if donator == donor::DELETED_PLAYER {
        talk_msg::DELETED_DONOR
    } else {
        talk_msg::DONATED
    }
}

/// Fish/insect station message: the non-final entries use the
/// 12194/12195 pair; the final entry uses 12191/12192.
pub fn station_msg(donator: u8, is_last: bool) -> u16 {
    let exists = donator_exists(donator);
    if is_last {
        if exists { talk_msg::DONATED } else { talk_msg::DELETED_DONOR }
    } else if exists {
        talk_msg::CONTINUE
    } else {
        talk_msg::CONTINUE_DELETED
    }
}

/// Room wall alpha transitions (`aMI_AlphaToON` / `aMI_AlphaToOFF`),
/// verbatim: ±24.0f per update, clamped to [0, 255].
pub mod alpha {
    pub fn to_on(alpha: f32) -> f32 {
        (alpha + 24.0).clamp(0.0, 255.0)
    }

    pub fn to_off(alpha: f32) -> f32 {
        (alpha - 24.0).clamp(0.0, 255.0)
    }

    /// Whether the wall fades out at the given camera state.
    /// Type 0 (fish room): states 3 or 7; type 1 (insect room):
    /// states 1 or 5 (`ac_museum_indoor.c`).
    pub fn fade_out(room_type: u8, camera_state: u8) -> bool {
        match room_type {
            0 => camera_state == 3 || camera_state == 7,
            1 => camera_state == 1 || camera_state == 5,
            _ => false,
        }
    }
}

/// Fish tank centers (`suisou_pos`), verbatim. The last is the
/// large/front tank.
pub mod tank {
    pub const CENTERS: [(f32, f32, f32); 5] = [
        (220.0, 40.0, 220.0),
        (420.0, 40.0, 220.0),
        (220.0, 40.0, 460.0),
        (420.0, 40.0, 460.0),
        (320.0, 40.0, 20.0),
    ];

    /// Aquarium light power, verbatim:
    /// `((60 - CLAMP(dist, 0, 60)) / 60) * 55`, X/Z distance only.
    /// Normal tanks clamp the light position to ±60 units; the large
    /// tank uses ±200 X and +35/−20 Z (C-side).
    pub fn light_power(dist_xz: f32) -> u8 {
        let d = dist_xz.clamp(0.0, 60.0);
        (((60.0 - d) / 60.0) * 55.0) as u8
    }
}

/// Fish information stations: positions and species-index lists
/// (`Museum_Fish_Set_MsgFishInfo`), verbatim. The source uses the
/// 0-based `aGYO_TYPE_*` enum; the resolved raw indices are below.
/// Proximity: |dx| < 20, |dz| < 20, facing within 0x3000 (67.5°).
pub mod fish_station {
    pub const POSITIONS: [(f32, f32, f32); 5] = [
        (260.0, 40.0, 300.0),
        (460.0, 40.0, 300.0),
        (260.0, 40.0, 540.0),
        (460.0, 40.0, 540.0),
        (500.0, 40.0, 60.0),
    ];
    pub const A: [u8; 10] = [0, 1, 2, 3, 11, 14, 26, 23, 30, 34];
    pub const B: [u8; 9] = [4, 9, 15, 8, 5, 6, 7, 27, 33];
    pub const C: [u8; 9] = [12, 13, 16, 17, 18, 20, 19, 21, 32];
    pub const D: [u8; 6] = [10, 29, 28, 24, 25, 39];
    pub const E: [u8; 6] = [22, 31, 35, 36, 37, 38];
}

/// Insect information stations (`Museum_Insect_Check_Talk_Distance`
/// / `Museum_Insect_Set_MsgInsectInfo`), verbatim. Proximity:
/// |dx| < 20, |dz| < 20, facing within 67.5° of the expected
/// direction (uses `world.angle.y`). The actor's own world Y is set
/// to 4000.0f — preserve, don't "fix".
pub mod insect_station {
    pub const POSITIONS: [(f32, f32, f32); 6] = [
        (140.0, 40.0, 60.0),
        (420.0, 40.0, 60.0),
        (220.0, 40.0, 300.0),
        (380.0, 40.0, 300.0),
        (180.0, 40.0, 540.0),
        (340.0, 40.0, 540.0),
    ];
    pub const G1: [u8; 7] = [22, 4, 21, 29, 38, 20, 7];
    pub const G2: [u8; 6] = [31, 30, 19, 5, 23, 6];
    pub const G3: [u8; 6] = [0, 25, 26, 1, 27, 2];
    pub const G4: [u8; 6] = [32, 34, 37, 3, 24, 39];
    pub const G5: [u8; 7] = [35, 8, 33, 9, 10, 11, 12];
    pub const G6: [u8; 7] = [15, 16, 18, 17, 13, 14, 36];
    pub const ACTOR_WORLD_Y: f32 = 4000.0;
}

/// Build a station's filtered donation list: the station's fixed
/// species ordering, keeping only entries whose donor status is
/// nonzero (`mMmd_*Info(i)`). Order is observable — copied literally.
pub fn station_donated(list: &[u8], donor_of: impl Fn(u8) -> u8) -> Vec<u8> {
    list.iter().copied().filter(|&i| donor_of(i) != 0).collect()
}

/// Exterior museum actor values (`ac_museum.c`), verbatim.
pub mod exterior {
    /// Entrance door: SCENE_MUSEUM_ENTRANCE, north, {240, 0, 440}.
    pub const ENTER_POS: (i16, i16, i16) = (240, 0, 440);
    /// Exit: home Z + 120, WIPE_TYPE_TRIFORCE, wipe 0x2168.
    pub const EXIT_Z_OFFSET: f32 = 120.0;
    pub const EXIT_WIPE: u16 = 0x2168;
    /// Front-door check: squared X/Z distance < 1100 with Z
    /// reference offset +100, facing within 135°–225°.
    pub const DOOR_DIST_SQ: f32 = 1100.0;
    pub const DOOR_Z_OFFSET: f32 = 100.0;
    /// Night light: ON before 06:00 or from 18:00
    /// (`!(now_sec < 64800 && now_sec >= 21600)` — note the 06:00
    /// boundary, unlike the police station's 05:00).
    pub fn light_on(now_sec: u32) -> bool {
        !(now_sec < 64800 && now_sec >= 21600)
    }
    /// Light chase rate for `arg0_f`.
    pub const CHASE_RATE: f32 = 0.019532442;
}


/// Fish/insect collection tracking (`m_museum_display.c`).
///
/// The museum donation state: 40 fish + 40 insect species, each a 4-bit donor
/// status (see `donor`). This is town-wide collection data with player
/// attribution — NOT a per-player "caught" history. There is no separate
/// encyclopedia-style catch table in the decomp.
pub mod collection {
    use super::{bit_store, is_donated, query};
    use super::category::{FISH_NUM, INSECT_NUM};
    use super::donor;

    /// `mMmd_FishInfo`: validated getter. Out-of-range returns 0.
    pub fn fish_info(fish_bit: &[u8], fish_no: i32) -> u8 {
        query(fish_bit, fish_no, FISH_NUM)
    }

    /// `mMmd_InsectInfo`: validated getter. Out-of-range returns 0.
    pub fn insect_info(insect_bit: &[u8], insect_no: i32) -> u8 {
        query(insect_bit, insect_no, INSECT_NUM)
    }

    /// `mMmd_SetFish` core: validate 0..40, clear nibble, write donor.
    /// (The C version also requires a local player; the caller passes the
    /// already-validated donor value.) Returns false if index invalid.
    pub fn set_fish_donor(fish_bit: &mut [u8], fish_no: i32, donor: u8) -> bool {
        if fish_no >= 0 && (fish_no as usize) < FISH_NUM {
            bit_store(fish_bit, fish_no as u8, donor);
            true
        } else {
            false
        }
    }

    /// `mMmd_SetInsect` core: same contract as `set_fish_donor`.
    pub fn set_insect_donor(insect_bit: &mut [u8], insect_no: i32, donor: u8) -> bool {
        if insect_no >= 0 && (insect_no as usize) < INSECT_NUM {
            bit_store(insect_bit, insect_no as u8, donor);
            true
        } else {
            false
        }
    }

    /// `mMmd_CountDisplayedFish` / `mMmd_CountDisplayedInsect` core:
    /// counts species whose donor is PLAYER1..=DELETED_PLAYER (deleted
    /// donors still count toward the total).
    pub fn count_displayed(bits: &[u8], count: usize) -> u32 {
        (0..count)
            .filter(|&i| is_donated(query(bits, i as i32, count)))
            .count() as u32
    }

    /// `mMmd_DeletePresentedByPlayer` core (fish+insect): rewrites any entry
    /// donated by `player_no` to DELETED_PLAYER. The species stays counted
    /// as donated; only the attribution changes.
    pub fn delete_presented_by_player(
        fish_bit: &mut [u8],
        insect_bit: &mut [u8],
        player_no: u8,
    ) {
        let target = player_no + 1; // donor value = player index + 1
        for i in 0..FISH_NUM {
            if query(fish_bit, i as i32, FISH_NUM) == target {
                bit_store(fish_bit, i as u8, donor::DELETED_PLAYER);
            }
        }
        for i in 0..INSECT_NUM {
            if query(insect_bit, i as i32, INSECT_NUM) == target {
                bit_store(insect_bit, i as u8, donor::DELETED_PLAYER);
            }
        }
    }
}

// ---- C ABI ----
// All pc_-prefixed: the C originals stay compiled; these are staged kernels.

#[no_mangle]
pub extern "C" fn pc_md_query(bits: *const u8, index: i32, count: usize) -> u8 {
    if bits.is_null() {
        return 0;
    }
    let bits = unsafe { std::slice::from_raw_parts(bits, count.div_ceil(2)) };
    query(bits, index, count)
}

#[no_mangle]
pub extern "C" fn pc_md_bit_store(bits: *mut u8, index: u8, value: u8, bytes: usize) {
    if !bits.is_null() {
        let bits = unsafe { std::slice::from_raw_parts_mut(bits, bytes) };
        bit_store(bits, index, value);
    }
}

#[no_mangle]
pub extern "C" fn pc_md_talk_msg(donator: u8) -> u16 {
    talk_msg(donator)
}

#[no_mangle]
pub extern "C" fn pc_md_station_msg(donator: u8, is_last: u8) -> u16 {
    station_msg(donator, is_last != 0)
}

#[no_mangle]
pub extern "C" fn pc_md_alpha_on(alpha: f32) -> f32 {
    alpha::to_on(alpha)
}

#[no_mangle]
pub extern "C" fn pc_md_alpha_off(alpha: f32) -> f32 {
    alpha::to_off(alpha)
}

#[no_mangle]
pub extern "C" fn pc_md_fade_out(room_type: u8, camera_state: u8) -> u8 {
    alpha::fade_out(room_type, camera_state) as u8
}

#[no_mangle]
pub extern "C" fn pc_md_light_power(dist_xz: f32) -> u8 {
    tank::light_power(dist_xz)
}

#[no_mangle]
pub extern "C" fn pc_md_museum_light_on(now_sec: u32) -> u8 {
    exterior::light_on(now_sec) as u8
}

/// Donated-fossil display item: `0x1EEC + (idx << 2) + (rotation & 3)`.
#[no_mangle]
pub extern "C" fn pc_md_fossil_display_item(fossil_idx: u8, rotation: u8) -> u16 {
    fossil_item::donated(fossil_idx, rotation)
}


#[no_mangle]
pub extern "C" fn pc_md_fish_info(fish_bit: *const u8, fish_no: i32) -> u8 {
    if fish_bit.is_null() {
        return 0;
    }
    let bits = unsafe { std::slice::from_raw_parts(fish_bit, category::FISH_BIT_NUM) };
    collection::fish_info(bits, fish_no)
}

#[no_mangle]
pub extern "C" fn pc_md_insect_info(insect_bit: *const u8, insect_no: i32) -> u8 {
    if insect_bit.is_null() {
        return 0;
    }
    let bits = unsafe { std::slice::from_raw_parts(insect_bit, category::INSECT_BIT_NUM) };
    collection::insect_info(bits, insect_no)
}

#[no_mangle]
pub extern "C" fn pc_md_set_fish_donor(fish_bit: *mut u8, fish_no: i32, donor: u8) -> i32 {
    if fish_bit.is_null() {
        return 0;
    }
    let bits = unsafe { std::slice::from_raw_parts_mut(fish_bit, category::FISH_BIT_NUM) };
    collection::set_fish_donor(bits, fish_no, donor) as i32
}

#[no_mangle]
pub extern "C" fn pc_md_set_insect_donor(insect_bit: *mut u8, insect_no: i32, donor: u8) -> i32 {
    if insect_bit.is_null() {
        return 0;
    }
    let bits = unsafe { std::slice::from_raw_parts_mut(insect_bit, category::INSECT_BIT_NUM) };
    collection::set_insect_donor(bits, insect_no, donor) as i32
}

#[no_mangle]
pub extern "C" fn pc_md_count_displayed_fish(fish_bit: *const u8) -> i32 {
    if fish_bit.is_null() {
        return 0;
    }
    let bits = unsafe { std::slice::from_raw_parts(fish_bit, category::FISH_BIT_NUM) };
    collection::count_displayed(bits, category::FISH_NUM) as i32
}

#[no_mangle]
pub extern "C" fn pc_md_count_displayed_insect(insect_bit: *const u8) -> i32 {
    if insect_bit.is_null() {
        return 0;
    }
    let bits = unsafe { std::slice::from_raw_parts(insect_bit, category::INSECT_BIT_NUM) };
    collection::count_displayed(bits, category::INSECT_NUM) as i32
}

#[no_mangle]
pub extern "C" fn pc_md_delete_presented_fish_insect(
    fish_bit: *mut u8,
    insect_bit: *mut u8,
    player_no: u8,
) {
    if fish_bit.is_null() || insect_bit.is_null() {
        return;
    }
    let fbits =
        unsafe { std::slice::from_raw_parts_mut(fish_bit, category::FISH_BIT_NUM) };
    let ibits =
        unsafe { std::slice::from_raw_parts_mut(insect_bit, category::INSECT_BIT_NUM) };
    collection::delete_presented_by_player(fbits, ibits, player_no)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_layout() {
        assert_eq!(category::RECORD_SIZE, 0x3F);
        assert_eq!(
            category::FOSSIL_BIT_NUM
                + category::ART_BIT_NUM
                + category::FISH_BIT_NUM
                + category::INSECT_BIT_NUM,
            0x3F
        );
        assert_eq!(category::FOSSIL_NUM, 25);
        assert_eq!(category::ART_NUM, 15);
        assert_eq!(category::FISH_NUM, 40);
        assert_eq!(category::INSECT_NUM, 40);
    }

    #[test]
    fn nibbles() {
        let mut bits = [0u8; 13];
        // even index -> low nibble, odd index -> high nibble
        bit_store(&mut bits, 0, 3);
        bit_store(&mut bits, 1, 5);
        assert_eq!(bit_info(&bits, 0), 3);
        assert_eq!(bit_info(&bits, 1), 5);
        assert_eq!(bits[0], 0x53);
        // adjacent entries don't clobber each other
        bit_store(&mut bits, 2, 7);
        assert_eq!(bit_info(&bits, 0), 3);
        assert_eq!(bit_info(&bits, 1), 5);
        assert_eq!(bit_info(&bits, 2), 7);
        // values are masked to 4 bits
        bit_store(&mut bits, 3, 0xFF);
        assert_eq!(bit_info(&bits, 3), 0xF);
        // bounds-checked query
        assert_eq!(query(&bits, 24, 25), 0);
        assert_eq!(query(&bits, 25, 25), 0);
        assert_eq!(query(&bits, -1, 25), 0);
    }

    #[test]
    fn donors() {
        assert!(is_donated(1));
        assert!(is_donated(5));
        assert!(!is_donated(0));
        assert!(!is_donated(6));
        assert!(donator_exists(4));
        assert!(!donator_exists(5));
        assert_eq!(donator_plr_idx(1), 0);
        assert_eq!(donator_plr_idx(4), 3);
        assert_eq!(talk_msg(0), 12193);
        assert_eq!(talk_msg(5), 12192);
        assert_eq!(talk_msg(2), 12191);
        assert_eq!(station_msg(2, false), 12194);
        assert_eq!(station_msg(5, false), 12195);
        assert_eq!(station_msg(2, true), 12191);
        assert_eq!(station_msg(5, true), 12192);
    }

    #[test]
    fn fossil_tables() {
        assert_eq!(fossil_floor::ENTRIES.len(), 25);
        assert_eq!(fossil_talk::ENTRIES.len(), 25);
        // the two tables are separate: entry 0 differs
        assert_ne!(
            (fossil_floor::ENTRIES[0].0, fossil_floor::ENTRIES[0].1),
            (fossil_talk::ENTRIES[0].0, fossil_talk::ENTRIES[0].1)
        );
        assert_eq!(fossil_floor::ut(5, 2), 0x25);
        assert_eq!(fossil_floor::ut(10, 8), 0x8A);
        // donated item formula
        assert_eq!(fossil_item::donated(0, 0), 0x1EEC);
        assert_eq!(fossil_item::donated(1, 2), 0x1EEC + 4 + 2);
        assert_eq!(fossil_item::donated(24, 7), 0x1EEC + 96 + 3);
        assert_eq!(fossil_item::dummy(0x2000, 1), 0x2001);
    }

    #[test]
    fn art_positions() {
        assert_eq!(art::POSITIONS.len(), 15);
        assert_eq!(art::draw_pos(7, 1), (300.0, 40.0, 60.0));
        // talk check uses different offsets than the draw position
        assert_eq!(art::talk_pos(7, 1), (340.0, 40.0, 80.0));
        assert_ne!(art::draw_pos(7, 1).0, art::talk_pos(7, 1).0);
        assert_eq!(art::DIRECT_TABLE.len(), 16);
        assert_eq!(art::DIST, 33.0);
        assert_eq!(fossil_talk::DIST, 42.5);
    }

    #[test]
    fn stations() {
        assert_eq!(fish_station::A.len(), 10);
        assert_eq!(fish_station::D.len(), 6);
        assert_eq!(insect_station::G1.len(), 7);
        assert_eq!(insect_station::G6.len(), 7);
        assert_eq!(insect_station::ACTOR_WORLD_Y, 4000.0);
        // filtered list preserves the station ordering
        let donors = |i: u8| if i == 0 || i == 14 { 1 } else { 0 };
        assert_eq!(station_donated(&fish_station::A, donors), vec![0, 14]);
        let none = |_: u8| 0;
        assert!(station_donated(&fish_station::B, none).is_empty());
    }

    #[test]
    fn room_and_exterior() {
        // alpha steps and clamps
        assert_eq!(alpha::to_on(240.0), 255.0);
        assert_eq!(alpha::to_off(10.0), 0.0);
        assert_eq!(alpha::to_on(100.0), 124.0);
        // camera-state fade sets
        assert!(alpha::fade_out(0, 3));
        assert!(alpha::fade_out(0, 7));
        assert!(!alpha::fade_out(0, 1));
        assert!(alpha::fade_out(1, 1));
        assert!(alpha::fade_out(1, 5));
        assert!(!alpha::fade_out(1, 3));
        // light power
        assert_eq!(tank::light_power(0.0), 55);
        assert_eq!(tank::light_power(60.0), 0);
        assert_eq!(tank::light_power(30.0), 27);
        assert_eq!(tank::light_power(1000.0), 0);
        // museum light: on before 06:00 or from 18:00 (not 05:00)
        assert!(exterior::light_on(5 * 3600 + 59 * 60));
        assert!(!exterior::light_on(6 * 3600));
        assert!(!exterior::light_on(12 * 3600));
        assert!(exterior::light_on(18 * 3600));
        assert_eq!(exterior::ENTER_POS, (240, 0, 440));
        assert_eq!(exterior::EXIT_WIPE, 0x2168);
    }

    #[test]
    fn collection_fish_insect() {
        use super::collection;
        use super::donor;

        // Nibble packing: index 0 = low nibble of byte 0, index 1 = high nibble.
        let mut fish = [0u8; 21];
        assert!(collection::set_fish_donor(&mut fish, 0, donor::PLAYER1));
        assert!(collection::set_fish_donor(&mut fish, 1, donor::PLAYER2));
        assert_eq!(fish[0], 0x21); // low=1, high=2
        assert_eq!(collection::fish_info(&fish, 0), donor::PLAYER1);
        assert_eq!(collection::fish_info(&fish, 1), donor::PLAYER2);
        // Adjacent species in the same byte don't clobber each other.
        assert!(collection::set_fish_donor(&mut fish, 0, donor::PLAYER3));
        assert_eq!(collection::fish_info(&fish, 1), donor::PLAYER2);

        // Invalid indices: getter returns 0, setter returns false.
        assert_eq!(collection::fish_info(&fish, 40), 0);
        assert_eq!(collection::fish_info(&fish, -1), 0);
        assert!(!collection::set_fish_donor(&mut fish, 40, donor::PLAYER1));
        assert_eq!(collection::insect_info(&[0u8; 21], 39), donor::NONE);

        // Highest index (39): high nibble of byte 19.
        let mut insect = [0u8; 21];
        assert!(collection::set_insect_donor(&mut insect, 39, donor::PLAYER4));
        assert_eq!(insect[19], 0x40);
        assert_eq!(collection::insect_info(&insect, 39), donor::PLAYER4);

        // Count: deleted donors still count.
        let mut f2 = [0u8; 21];
        collection::set_fish_donor(&mut f2, 0, donor::PLAYER1);
        collection::set_fish_donor(&mut f2, 1, donor::DELETED_PLAYER);
        collection::set_fish_donor(&mut f2, 2, donor::NONE);
        assert_eq!(collection::count_displayed(&f2, 40), 2);

        // DeletePresentedByPlayer: rewrites to DELETED_PLAYER, keeps count.
        let mut f3 = [0u8; 21];
        let mut i3 = [0u8; 21];
        collection::set_fish_donor(&mut f3, 5, donor::PLAYER2); // player_no 1
        collection::set_fish_donor(&mut f3, 6, donor::PLAYER1); // player_no 0
        collection::set_insect_donor(&mut i3, 7, donor::PLAYER2);
        collection::delete_presented_by_player(&mut f3, &mut i3, 1);
        assert_eq!(collection::fish_info(&f3, 5), donor::DELETED_PLAYER);
        assert_eq!(collection::fish_info(&f3, 6), donor::PLAYER1); // untouched
        assert_eq!(collection::insect_info(&i3, 7), donor::DELETED_PLAYER);
        assert_eq!(collection::count_displayed(&f3, 40), 2); // still counts
    }
}

