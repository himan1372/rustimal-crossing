//! Blanca the faceless cat — pure classification kernels.
//!
//! Verified against `src/game/m_event.c`, `src/actor/ac_event_manager.c`,
//! `src/actor/npc/ac_npc_mask_cat_move.c_inc`, `src/game/m_scene.c`,
//! `include/m_name_table.h`, `include/m_npc.h`, `include/m_private.h`,
//! `include/m_mask_cat.h` (GAFE01_00 Rev 0 USA decomp). All 10 brief
//! claims checked verbatim before porting; no corrections needed.
//!
//! Architecture: the weekly scheduler (`init_weekly_event`), the
//! mask-NPC event (`gohome_mask_start`/`gohome_mask_in`), the fixed-size
//! mask registry (`mNpc_RegistMaskNpc`), the actor lifecycle and talk
//! state machine, the travel-scene selection, RNG, and all save
//! mutations stay in C. This module holds only the source-proven pure
//! kernels with primitive arguments.
//!
//! Identity: two special-NPC ids, one mask-cat actor family.
//! `SP_NPC_MASK_CAT` (0xD075) is the regular event visitor;
//! `SP_NPC_MASK_CAT2` (0xD076) is the travel-scene slot (Blanca or Rover).
//! Do not collapse them — retail uses different registration paths.

/// Blanca identity (`include/m_name_table.h`, `src/game/m_npc.c`).
pub mod id {
    /// Regular event visitor (`SP_NPC_START + 117`).
    pub const MASK_CAT: u16 = 0xD075;
    /// Travel-scene slot (`SP_NPC_START + 118`).
    pub const MASK_CAT2: u16 = 0xD076;
    /// Max talks before the saved visit resets (`mMC_TALK_IDX_MAX`).
    pub const TALK_IDX_MAX: u8 = 10;
    /// Bit in `spnpc_first_talk_flags` (`aNPC_SPNPC_BIT_MASK_CAT`).
    pub const FIRST_TALK_BIT: u8 = 2;
}

/// Weekday values (`include/lb_rtc.h`).
pub mod weekday {
    pub const SUNDAY: u8 = 0;
    pub const MONDAY: u8 = 1;
    pub const TUESDAY: u8 = 2;
    pub const WEDNESDAY: u8 = 3;
    pub const THURSDAY: u8 = 4;
    pub const FRIDAY: u8 = 5;
    pub const SATURDAY: u8 = 6;
}

/// Clothing items (`include/m_name_table.h`).
pub mod cloth {
    pub const START: u16 = 0x2400;
    pub const END: u16 = 0x24FF;
    pub const EMPTY_NO: u16 = 0x0000;
}

/// Talk message bases (`aNMC_set_talk_info`).
pub mod msg {
    /// First talk: `talk_idx * 4 + FIRST_BASE` (pre-increment idx).
    pub const FIRST_BASE: u16 = 0x31E4;
    /// Repeat: `REPEAT_BASE + 4 * (talk_idx - 1) + RANDOM(3)`.
    pub const REPEAT_BASE: u16 = 0x31E5;
}

/// Scheduled-travel flag (`mPr_FLAG_MASK_CAT_SCHEDULED`).
pub const FLAG_MASK_CAT_SCHEDULED: u32 = 1 << 0;

/// First-talk message id. Uses the PRE-increment `talk_idx`, exactly like
/// the decomp (`msg_num = mask_cat_talk_idx * 4 + 0x31E4` before `++`).
pub fn first_talk_msg(talk_idx: u8) -> u16 {
    talk_idx as u16 * 4 + msg::FIRST_BASE
}

/// Repeat-talk message id. `rand_roll` is the C-side `RANDOM(3)` result.
/// `talk_idx <= 0` is clamped to 1, verbatim.
pub fn repeat_talk_msg(talk_idx: u8, rand_roll: u8) -> u16 {
    let idx = talk_idx.max(1);
    msg::REPEAT_BASE + 4 * (idx as u16 - 1) + (rand_roll % 3) as u16
}

/// Birthday-branch weekday eligibility (`mMC_check_birth_day` without the
/// save read): Sunday, Monday, Wednesday, Friday are rejected.
pub fn birth_day_ok(weekday: u8) -> bool {
    !matches!(
        weekday,
        weekday::SUNDAY | weekday::MONDAY | weekday::WEDNESDAY | weekday::FRIDAY
    )
}

/// Cloth item id from a saved cloth number
/// (`regist_mask_maskcat`: `ITM_CLOTH_START + cloth_no`).
pub fn cloth_item(cloth_no: u16) -> u16 {
    cloth::START + cloth_no
}

/// Travel-scene cloth guard: the saved `cloth_no` is only written back
/// when the selected cloth is inside the valid clothing range, else
/// `EMPTY_NO` (`m_scene.c` SCENE_START_DEMO3 block).
pub fn travel_cloth_no(blanca_cloth: u16) -> u16 {
    if (cloth::START..cloth::END).contains(&blanca_cloth) {
        blanca_cloth - cloth::START
    } else {
        cloth::EMPTY_NO
    }
}

// ---- C ABI ----
// All pc_-prefixed: the C originals stay compiled; these are staged kernels.

#[no_mangle]
pub extern "C" fn pc_blanca_first_talk_msg(talk_idx: u8) -> u16 {
    first_talk_msg(talk_idx)
}

#[no_mangle]
pub extern "C" fn pc_blanca_repeat_talk_msg(talk_idx: u8, rand_roll: u8) -> u16 {
    repeat_talk_msg(talk_idx, rand_roll)
}

#[no_mangle]
pub extern "C" fn pc_blanca_birth_day_ok(weekday: u8) -> u8 {
    birth_day_ok(weekday) as u8
}

#[no_mangle]
pub extern "C" fn pc_blanca_cloth_item(cloth_no: u16) -> u16 {
    cloth_item(cloth_no)
}

#[no_mangle]
pub extern "C" fn pc_blanca_travel_cloth_no(blanca_cloth: u16) -> u16 {
    travel_cloth_no(blanca_cloth)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity() {
        assert_eq!(id::MASK_CAT, 0xD075);
        assert_eq!(id::MASK_CAT2, 0xD076);
        assert_eq!(id::TALK_IDX_MAX, 10);
    }

    #[test]
    fn talk_messages() {
        // first talk uses pre-increment idx
        assert_eq!(first_talk_msg(0), 0x31E4);
        assert_eq!(first_talk_msg(3), 0x31E4 + 12);
        // repeat: base + 4*(idx-1) + roll; idx 0 clamps to 1
        assert_eq!(repeat_talk_msg(1, 2), 0x31E5 + 2);
        assert_eq!(repeat_talk_msg(0, 1), 0x31E5 + 1);
        assert_eq!(repeat_talk_msg(4, 0), 0x31E5 + 12);
    }

    #[test]
    fn weekdays() {
        assert!(!birth_day_ok(weekday::SUNDAY));
        assert!(!birth_day_ok(weekday::MONDAY));
        assert!(birth_day_ok(weekday::TUESDAY));
        assert!(!birth_day_ok(weekday::WEDNESDAY));
        assert!(birth_day_ok(weekday::THURSDAY));
        assert!(!birth_day_ok(weekday::FRIDAY));
        assert!(birth_day_ok(weekday::SATURDAY));
    }

    #[test]
    fn cloth() {
        assert_eq!(cloth_item(0), 0x2400);
        assert_eq!(cloth_item(7), 0x2407);
        assert_eq!(travel_cloth_no(0x2407), 7);
        assert_eq!(travel_cloth_no(0x24FE), 0x24FE - 0x2400);
        assert_eq!(travel_cloth_no(0x24FF), 0x0000); // strict < END
        assert_eq!(travel_cloth_no(0x2500), 0x0000); // out of range
        assert_eq!(travel_cloth_no(0x2300), 0x0000);
    }
}
