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
