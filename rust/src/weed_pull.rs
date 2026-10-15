//! Pulling weeds (`m_player_main_remove_grass.c_inc`), the file
//! `docs/need-to-port.md` listed as "grass wear".
//!
//! Source-verified against ac-decomp 09ca8e8b (primary) and ACGC-PC-Port
//! (secondary: byte-identical). Trigger from `m_player_common.c_inc`.
//!
//! ## There is no grass wear on GameCube
//! A full search of the decomp (save structures, player movement, daily
//! renewal) found no per-tile ground-health state, no wear on walking and
//! no regrowth table. Footprints (`eEC_EFFECT_FOOTPRINT`) are short-lived
//! effects. Persistent worn paths are a City Folk feature. The only
//! "grass" mechanic here is removing weed items, which the town rating
//! counts (`field_assessment.rs`).
//!
//! ## Facts
//! - Pressing the pick-up button facing a weed (`IS_ITEM_GRASS`: GRASS_A..C
//!   = 0x0008..=0x000A) requests REMOVE_GRASS at priority 8 with the target
//!   unit and the weed's position.
//! - The player stands still playing `mPlayer_ANIM_ZASSOU1` (morph -6).
//! - On animation frame 17, if the target unit **still** holds a weed, it
//!   flies off via the bg-item clip at the player's facing + 110 degrees,
//!   with a mud-dig effect, the pull sound and a rumble. If something else
//!   cleared it first, nothing happens.
//! - When the animation ends the player returns to waiting.

pub const GRASS_A: u16 = 0x0008;
pub const GRASS_C: u16 = 0x000A;
pub const REQUEST_PRIORITY: i32 = 8;
pub const PULL_FRAME: f32 = 17.0;
pub const ANIM_MORPH: f32 = -6.0;
/// `DEG2SHORT_ANGLE2(110.0f)` = (int)(110 * 65536 / 360) = 20024.
pub const FLY_ANGLE_OFFSET: i32 = 20024;

/// `IS_ITEM_GRASS`.
pub fn is_weed(item: u16) -> bool {
    (GRASS_A..=GRASS_C).contains(&item)
}

/// `Player_actor_ChangeFGNumber_Remove_grass`: returns the fly angle
/// (s16) when the weed is pulled on this frame.
pub fn pull_on_frame(frame_hit: bool, unit_item: Option<u16>, facing_y: i16) -> Option<i16> {
    match unit_item {
        Some(it) if frame_hit && is_weed(it) => Some((FLY_ANGLE_OFFSET + facing_y as i32) as i16),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weeds() {
        assert!(is_weed(0x0008) && is_weed(0x000A) && !is_weed(0x000B));
        assert_eq!(pull_on_frame(true, Some(0x0009), 0), Some(20024));
        assert_eq!(pull_on_frame(true, Some(0x0009), 20000), Some((40024i32) as i16));
        assert_eq!(pull_on_frame(false, Some(0x0009), 0), None);
        assert_eq!(pull_on_frame(true, Some(0x0000), 0), None);
    }
}

#[cfg(test)]
mod tests_wave4 {
    use super::*;

    #[test]
    fn weed_boundaries() {
        assert_eq!(GRASS_A, 0x0008);
        assert_eq!(GRASS_C, 0x000A);
        assert!(!is_weed(0x0007));
        assert!(is_weed(GRASS_A));
        assert!(is_weed(0x0009));
        assert!(is_weed(GRASS_C));
        assert!(!is_weed(0x000B));
        assert!(!is_weed(0xFFFF));
        assert!(!is_weed(0x0000));
    }

    #[test]
    fn fly_angle_offset_math() {
        // DEG2SHORT_ANGLE2(110.0) = trunc(110 * 65536 / 360).
        assert_eq!(FLY_ANGLE_OFFSET, (110.0f32 * 65536.0 / 360.0) as i32);
        assert_eq!(FLY_ANGLE_OFFSET, 20024);
    }

    #[test]
    fn pull_angle_wrapping() {
        // Fly angle is (20024 + facing) wrapped to s16.
        assert_eq!(pull_on_frame(true, Some(0x0009), -20024), Some(0));
        // Positive overflow: 20024 + 32767 = 52791 -> -12745.
        assert_eq!(pull_on_frame(true, Some(0x0008), 32767), Some(-12745));
        // Negative: 20024 - 32768 = -12744 (no wrap needed).
        assert_eq!(pull_on_frame(true, Some(0x000A), -32768), Some(-12744));
    }

    #[test]
    fn pull_conditions_and_consts() {
        // Frame miss or cleared unit -> nothing happens.
        assert_eq!(pull_on_frame(false, Some(0x0009), 0), None);
        assert_eq!(pull_on_frame(true, None, 0), None);
        assert_eq!(pull_on_frame(true, Some(0x000B), 0), None);
        assert_eq!(pull_on_frame(true, Some(0x0000), 0), None);
        // Non-weed grass-adjacent item ids are ignored.
        for item in [0x0007u16, 0x000B, 0x000C, 0x0100] {
            assert_eq!(pull_on_frame(true, Some(item), 100), None);
        }
        assert_eq!(REQUEST_PRIORITY, 8);
        assert_eq!(PULL_FRAME, 17.0);
        assert_eq!(ANIM_MORPH, -6.0);
    }
}
