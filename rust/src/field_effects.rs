//! Field-level effects: acre drawing helpers and ambient sound
//! (`ac_field_draw.c`), tree shake/chop effects (`ac_effectbg.c`) and
//! Katrina's fortune tent (`ac_buggy.c`, FG `FORTUNE_TENT`).
//!
//! Source-verified against ac-decomp (primary) and ACGC-PC-Port
//! (secondary). PC changes: field_draw's marine animation runs off an
//! accumulated `anim_frame` (wraps at 300) instead of `game_frame % 300`;
//! effectbg's timer advances in 60 Hz ticks and the Christmas-light colour
//! uses `graph_dt_frame_time`; the tent's window light fades by
//! `0.019532442 * dt`.
//!
//! ## Field draw
//! - Draws the 3x3 acres around the player (`draw_block_offset_table`).
//! - Ambient water: in marine (beach) acres, ongen kind 2 (waves, SE 28)
//!   searched in the current row (3 acres), offset z +175; elsewhere kind 1
//!   (river, SE 11) searched in all 9 acres. The two closest sources (by
//!   XZ distance^2) play; ongen id =
//!   `actor + (block_x_max * bz + bx) * 6 + source_no`.
//! - Frogs (SE 0xA1) near a pond May-August between 18:00-21:59 and
//!   4:00-6:59; pond SE 0x16 always.
//! - Marine animation (300-frame period): wave radians = frame/300 * 2pi;
//!   `tile1 = -32cos + 32`, `tile0 = int(frame * -0.42666667) - 7`; beach
//!   env colour `(144, 128, 96) + (21, 18, 14) * (1 - cos(rad - 1.2 + reg))`.
//!
//! ## Tree effects (`EffectBg`, 3 slots)
//! - Variant folding: tree/palm/cedar/golden sizes map onto
//!   small(-1)/med/large/full with status flags; Christmas variants become
//!   "full" + XMAS.
//! - Lifetime: shake 22, small shake 18, large shake 82, cut 98 frames.
//! - Leaf bursts: cut trees at timer 70 (5/7/9 leaves by size), every 16
//!   frames before that, 3 leaves at timer 2; small shakes every 16 frames
//!   (type 3), large shakes every 16 frames (type 4). A felled trunk that
//!   hits the ground drops leaves and starts the sway (`leaf_angle` 0x2BC,
//!   `add_angle += 0xDAC`).
//! - Cherry-blossom leaves in term 4 (Apr 1-8): bush leaf type 6/7 instead
//!   of 4/5. Snow puffs also spawn in winter. Small shakes (burst type 3)
//!   spawn no leaves at all, only the winter snow.
//! - Latent: `_SMALL` variants fold to index -1 of the 3-entry model
//!   tables; only med/large/full trees ever reach this actor.
//!
//! ## Fortune tent (`ac_buggy.c`)
//! - Doors open when the Gypsy event (`mEv_SPNPC_GYPSY`) is active and
//!   the player is within sqrt(1200) of the door (z + 60); a player inside
//!   sqrt(480) facing north (rotation 0x6000..0xA000) and moving is led
//!   through the door. Closed: talking within 45 deg shows "sold out"
//!   (window colour 145,60,40).
//! - Window light lit before 5:00 or from 18:00 (`aBGY_ctrl_light`).
//! - Exit door data puts the player 100 units south of the tent.
//! - Despawns when the player leaves its acre (no borderless fix on PC).

use crate::game_math::cosf_table;

// ---------------------------------------------------------------------------
// Field draw
// ---------------------------------------------------------------------------

pub const SOUND_SOURCE_NUM: usize = 6;
pub const UNIT_BASE_SIZE: f32 = 40.0;
pub const MARINE_PERIOD: i32 = 300;

/// `aFD_block_offset_table` (search order for water sounds).
pub const SOUND_BLOCK_OFFSETS: [(i32, i32); 9] =
    [(0, 0), (-1, 0), (1, 0), (0, -1), (-1, -1), (1, -1), (0, 1), (-1, 1), (1, 1)];
/// Blocks searched per mode (full, current, row).
pub const SOUND_SEARCH_COUNT: [usize; 3] = [9, 1, 3];

/// `draw_block_offset_table` (draw order bottom-left .. top-right).
pub const DRAW_BLOCK_OFFSETS: [(i32, i32); 9] =
    [(-1, 1), (0, 1), (1, 1), (-1, 0), (0, 0), (1, 0), (-1, -1), (0, -1), (1, -1)];

/// Water sound parameters: (ongen kind, SE, z offset, search mode).
pub fn water_sound_params(marine_block: bool) -> (i16, u8, f32, usize) {
    if marine_block {
        (2, 28, 175.0, 2)
    } else {
        (1, 11, 0.0, 0)
    }
}

/// One candidate sound source (block + slot + squared XZ distance).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SoundCandidate {
    pub bx: i32,
    pub bz: i32,
    pub slot: usize,
    pub dist2: f32,
}

/// `aFD_OperateWaterSound` selection: the closest and second-closest
/// (strict `<`, first wins ties), each `None` if absent.
pub fn pick_two_closest(c: &[SoundCandidate]) -> [Option<SoundCandidate>; 2] {
    let mut best = None;
    let mut best_i = None;
    let mut d = 10_000_000.0f32;
    for (i, s) in c.iter().enumerate() {
        if s.dist2 < d {
            d = s.dist2;
            best = Some(*s);
            best_i = Some(i);
        }
    }
    let mut second = None;
    let mut d = 10_000_000.0f32;
    for (i, s) in c.iter().enumerate() {
        if Some(i) != best_i && s.dist2 < d {
            d = s.dist2;
            second = Some(*s);
        }
    }
    [best, second]
}

/// Ongen id offset for a source (`(block_x_max * bz + bx) * 6 + slot`).
pub fn water_ongen_offset(block_x_max: i32, bx: i32, bz: i32, slot: usize) -> u32 {
    ((block_x_max * bz + bx) * SOUND_SOURCE_NUM as i32 + slot as i32) as u32
}

/// Frog chorus time window.
pub fn frog_active(month: u8, hour: u8) -> bool {
    (5..=8).contains(&month) && ((18..=21).contains(&hour) || (4..=6).contains(&hour))
}

/// `aFD_MakeMarinScrollInfo` output.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MarineInfo {
    pub frame: i32,
    pub tile0_scroll: i32,
    pub tile1_scroll: i32,
    pub wave_cos: f32,
    pub beach_env: [u8; 4],
}

/// `aFD_MakeMarinScrollInfo` for a frame counter (retail: `game_frame`;
/// PC: accumulated `anim_frame`). `reg53` is the `MYKREG 53` debug knob (0).
pub fn marine_info(game_frame: f32, reg53: i32) -> MarineInfo {
    let frame = (game_frame as i32).rem_euclid(MARINE_PERIOD);
    let rad = (frame as f32 / 300.0) * std::f32::consts::PI * 2.0;
    let wave_cos = cosf_table(rad);
    let beach_cos = cosf_table((rad - 1.2) + reg53 as f32 * 0.01);
    MarineInfo {
        frame,
        tile0_scroll: (game_frame * -0.42666667) as i32 - 7,
        tile1_scroll: (wave_cos * -32.0 - -32.0) as i32,
        wave_cos,
        beach_env: [
            (144.0 + (beach_cos * -21.0 + 21.0)) as i32 as u8,
            (128.0 + (beach_cos * -18.0 + 18.0)) as i32 as u8,
            (96.0 + (beach_cos * -14.0 + 14.0)) as i32 as u8,
            255,
        ],
    }
}

// ---------------------------------------------------------------------------
// Tree effects
// ---------------------------------------------------------------------------

/// `EffectBG_EFFECT_*`.
pub const EFFECT_SHAKE: i16 = 0;
pub const EFFECT_CUT_LEFT: i16 = 1;
pub const EFFECT_CUT_RIGHT: i16 = 2;
pub const EFFECT_SHAKE_SMALL: i16 = 3;
pub const EFFECT_SHAKE_LARGE: i16 = 4;

/// `EffectBG_VARIANT_*`.
pub const VARIANT_TREE_SMALL: i16 = -1;
pub const VARIANT_TREE_MED: i16 = 0;
pub const VARIANT_TREE_LARGE: i16 = 1;
pub const VARIANT_TREE_FULL: i16 = 2;
pub const VARIANT_TREE_XMAS: i16 = 3;
pub const VARIANT_PALM_SMALL: i16 = 4;
pub const VARIANT_CEDAR_SMALL: i16 = 8;
pub const VARIANT_CEDAR_XMAS: i16 = 12;
pub const VARIANT_GOLD_SMALL: i16 = 13;

pub const STATUS_ACTIVE: u8 = 1 << 0;
pub const STATUS_FELLED: u8 = 1 << 1;
pub const STATUS_XMAS: u8 = 1 << 2;
pub const STATUS_PALM: u8 = 1 << 3;
pub const STATUS_CEDAR: u8 = 1 << 4;
pub const STATUS_GOLD: u8 = 1 << 5;

pub fn is_shake(t: i16) -> bool {
    matches!(t, EFFECT_SHAKE | EFFECT_SHAKE_SMALL | EFFECT_SHAKE_LARGE)
}

/// `EffectBG_object_ct` variant folding: (base variant, status flags).
pub fn fold_variant(mut variant: i16) -> (i16, u8) {
    let mut status = 0;
    if variant == VARIANT_TREE_XMAS || variant == VARIANT_CEDAR_XMAS {
        variant -= 1;
        status |= STATUS_XMAS;
    }
    if variant >= VARIANT_GOLD_SMALL {
        variant -= VARIANT_GOLD_SMALL - VARIANT_TREE_SMALL;
        status |= STATUS_GOLD;
    } else if variant >= VARIANT_CEDAR_SMALL {
        variant -= VARIANT_CEDAR_SMALL - VARIANT_TREE_SMALL;
        status |= STATUS_CEDAR;
    } else if variant >= VARIANT_PALM_SMALL {
        variant -= VARIANT_PALM_SMALL - VARIANT_TREE_SMALL;
        status |= STATUS_PALM;
    }
    (variant, status)
}

/// Effect lifetime in frames (`timer_max`).
pub fn effect_lifetime(kind: i16) -> i32 {
    match kind {
        EFFECT_SHAKE_SMALL => 18,
        EFFECT_SHAKE_LARGE => 82,
        EFFECT_SHAKE => 22,
        _ => 98,
    }
}

/// Leaves dropped when a cut tree lands / at timer 70.
pub fn felled_leaf_count(variant: i16) -> i16 {
    match variant {
        VARIANT_TREE_LARGE => 7,
        VARIANT_TREE_FULL => 9,
        _ => 5,
    }
}

/// Leaf burst scheduled for this timer value (`EffectBG_object_move`):
/// (count, `EffectBG_Make_Leafs` type) or `None`.
pub fn leaf_burst(kind: i16, variant: i16, timer: i32, felled: bool) -> Option<(i16, i16)> {
    let cut = kind == EFFECT_CUT_RIGHT || kind == EFFECT_CUT_LEFT;
    let small = kind == EFFECT_SHAKE_SMALL;
    let large = kind == EFFECT_SHAKE_LARGE;
    if timer == 70 && cut && !felled {
        Some((felled_leaf_count(variant), 1))
    } else if timer == 2 && !small && !large {
        Some((3, 0))
    } else if timer & 0xF == 0 && timer != 0 && timer < 70 && !small && !large {
        Some((1, 0))
    } else if timer & 0xF == 0 && small {
        Some((1, 3))
    } else if timer & 0xF == 0 && timer != 0 && large {
        Some((1, 4))
    } else {
        None
    }
}

/// Bush leaf effect type for a burst (`EffectBG_Make_Leafs`), before the
/// palm/cedar/gold flag bits are OR'd in. `cherry` = term 4.
pub fn bush_leaf_type(burst_type: i16, cherry: bool) -> u16 {
    match burst_type {
        EFFECT_SHAKE | EFFECT_SHAKE_SMALL | EFFECT_SHAKE_LARGE | EFFECT_CUT_RIGHT => {
            if cherry {
                6
            } else {
                4
            }
        }
        _ => {
            if cherry {
                7
            } else {
                5
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Fortune tent (ac_buggy)
// ---------------------------------------------------------------------------

/// `aBGY_ACTION_*`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TentAction {
    Wait = 0,
    OpenWait = 1,
    CloseDoor = 2,
    OpenDoor = 3,
}

/// Door animation (start frame, end frame, speed) per action.
pub fn tent_anim(action: TentAction) -> (f32, f32, f32) {
    const START: [f32; 4] = [1.0, 17.0, 17.0, 1.0];
    const END: [f32; 4] = [1.0, 17.0, 1.0, 17.0];
    const SPEED: [f32; 4] = [0.0, 0.0, 0.5, 0.5];
    let i = action as usize;
    (START[i], END[i], SPEED[i])
}

/// `aBGY_check_player`: 0 far, 1 near (d^2 < 1200), 2 at the door facing
/// north (d^2 < 480). Distance is to (x, z + 60) of the tent.
pub fn tent_check_player(dx: f32, dz_from_door: f32, rot_y: u16) -> i32 {
    let t = dx * dx + dz_from_door * dz_from_door;
    if rot_y > 0x6000 && rot_y < 0xA000 && t < 480.0 {
        2
    } else if t < 1200.0 {
        1
    } else {
        0
    }
}

/// `aBGY_check_player2`: walking north into the door.
pub fn tent_entering(angle_y: u16, speed: f32) -> bool {
    angle_y > 0x6000 && angle_y < 0xA000 && speed > 0.0
}

/// `aBGY_ctrl_light`.
pub fn tent_light_on(now_sec: i32) -> bool {
    !(18000..64800).contains(&now_sec)
}

/// Window light fade step per frame (`chase_f` step).
pub const TENT_LIGHT_STEP: f32 = 0.019532442;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closest_two() {
        let c = [
            SoundCandidate { bx: 0, bz: 0, slot: 0, dist2: 50.0 },
            SoundCandidate { bx: 0, bz: 0, slot: 1, dist2: 10.0 },
            SoundCandidate { bx: 1, bz: 0, slot: 0, dist2: 20.0 },
        ];
        let [a, b] = pick_two_closest(&c);
        assert_eq!(a.unwrap().slot, 1);
        assert_eq!(b.unwrap().dist2, 20.0);
        assert_eq!(pick_two_closest(&c[..1])[1], None);
    }

    #[test]
    fn frogs_and_marine() {
        assert!(frog_active(6, 19));
        assert!(!frog_active(6, 12));
        assert!(!frog_active(9, 19));
        let m = marine_info(0.0, 0);
        assert_eq!(m.tile1_scroll, 0);
        assert_eq!(m.tile0_scroll, -7);
        let m = marine_info(150.0, 0); // half period: cos(pi) ~ -1 through the s16 table
        assert!((63..=64).contains(&m.tile1_scroll));
    }

    #[test]
    fn tree_variants_and_schedule() {
        assert_eq!(fold_variant(3), (2, STATUS_XMAS));
        assert_eq!(fold_variant(12), (2, STATUS_XMAS | STATUS_CEDAR));
        assert_eq!(fold_variant(5), (0, STATUS_PALM));
        assert_eq!(fold_variant(16), (2, STATUS_GOLD));
        assert_eq!(leaf_burst(EFFECT_CUT_RIGHT, 2, 70, false), Some((9, 1)));
        assert_eq!(leaf_burst(EFFECT_SHAKE, 0, 2, false), Some((3, 0)));
        assert_eq!(leaf_burst(EFFECT_SHAKE_SMALL, 0, 0, false), Some((1, 3)));
        assert_eq!(leaf_burst(EFFECT_SHAKE_LARGE, 0, 0, false), None);
        assert_eq!(effect_lifetime(EFFECT_CUT_LEFT), 98);
    }

    #[test]
    fn tent_rules() {
        assert_eq!(tent_check_player(0.0, 10.0, 0x8000), 2);
        assert_eq!(tent_check_player(0.0, 30.0, 0x8000), 1);
        assert_eq!(tent_check_player(0.0, 40.0, 0x0000), 0);
        assert!(tent_light_on(4 * 3600));
        assert!(!tent_light_on(5 * 3600));
        assert!(tent_light_on(18 * 3600));
        assert_eq!(tent_anim(TentAction::OpenDoor), (1.0, 17.0, 0.5));
    }
}
