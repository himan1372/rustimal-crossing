//! Player-house interiors: room shell, clocks, arrangement room, and exterior.
//!
//! Verified against `src/actor/ac_my_indoor.c`,
//! `include/ac_my_indoor.h`, `src/actor/ac_house_clock.c`,
//! `include/ac_house_clock.h`, `src/actor/ac_arrange_room.c`,
//! `src/actor/ac_my_house.c`, `src/actor/ac_arrange_ftr.c`,
//! `src/actor/ac_my_room.c` (the mini-disk comment),
//! `include/m_scene_table.h`, `include/m_name_table_floor_decl.h`,
//! `include/audio_defs.h` (GAFE01_00 Rev 0 USA).
//!
//! Scope split: the sibling `furniture` module already owns the
//! furniture placement judge, switch bitfields, haniwa steps, and
//! shape logic. This module owns the room-kind scene
//! classification, texture-bank geometry, floor-sound selection,
//! wall/floor validation, step flags, the clock model/position
//! selection and chime timing, the arrange-room packed-index
//! handling, and the exterior door states. `ac_arrange_ftr.c` is
//! an empty actor stub — there is nothing to port there.
//!
//! Nuances carried: the scene table has exactly 10 entries (no
//! LL2-basement, no second-floor scene); the clock timing is a
//! crossing test on a 120-frame phase accumulator, not a raw
//! modulo; the clock hands ARE RTC-driven (`0x4000 -
//! rad_hour/rad_min` — only the rad_* computation lives in the
//! time system); the arrange-room clamps BOTH wall and floor to
//! 0..0x42; shop clocks hide during the monthly raffle; the door
//! state enum carries the source's own `SCEEN` typo.

/// Scene IDs used by the room-kind table (`include/m_scene_table.h`).
pub mod scene {
    pub const FG: u16 = 7;
    pub const SHOP0: u16 = 9;
    pub const POST_OFFICE: u16 = 14;
    pub const POLICE_BOX: u16 = 17;
    pub const MY_ROOM_S: u16 = 20;
    pub const MY_ROOM_M: u16 = 21;
    pub const MY_ROOM_L: u16 = 22;
    pub const CONVENI: u16 = 23;
    pub const SUPER: u16 = 24;
    pub const DEPART: u16 = 25;
    pub const DEPART_2: u16 = 29;
    pub const MUSEUM_ENTRANCE: u16 = 35;
    pub const MY_ROOM_LL1: u16 = 40;
    pub const MY_ROOM_LL2: u16 = 41;
    pub const MY_ROOM_BASEMENT_S: u16 = 42;
    pub const MY_ROOM_BASEMENT_M: u16 = 43;
    pub const MY_ROOM_BASEMENT_L: u16 = 44;
    pub const MY_ROOM_BASEMENT_LL1: u16 = 45;
    pub const NEEDLEWORK: u16 = 46;
    pub const COTTAGE_MY: u16 = 47;
}

/// Room kinds (`ac_my_indoor.c`).
pub mod room_kind {
    pub const S: u8 = 0;
    pub const M: u8 = 1;
    pub const L: u8 = 2;
    pub const LL1: u8 = 3;
    pub const LL2: u8 = 4;
    pub const BASEMENT: u8 = 5;
    pub const NUM: u8 = 6;

    /// Scene → room kind (`aMR_GetSceneRoomKind`), verbatim: the
    /// 10-entry table, first match wins, fallback `S`.
    pub fn for_scene(scene: u16) -> u8 {
        use super::scene as sc;
        const TABLE: [(u16, u8); 10] = [
            (sc::MY_ROOM_S, S),
            (sc::MY_ROOM_M, M),
            (sc::MY_ROOM_L, L),
            (sc::MY_ROOM_LL1, LL1),
            (sc::MY_ROOM_LL2, LL2),
            (sc::MY_ROOM_BASEMENT_S, BASEMENT),
            (sc::MY_ROOM_BASEMENT_M, BASEMENT),
            (sc::MY_ROOM_BASEMENT_L, BASEMENT),
            (sc::COTTAGE_MY, L),
            (sc::MY_ROOM_BASEMENT_LL1, BASEMENT),
        ];
        for &(s, k) in &TABLE {
            if scene == s {
                return k;
            }
        }
        S
    }
}

/// Texture-bank geometry (`include/ac_my_indoor.h`), verbatim.
pub mod bank {
    pub const PAL_SIZE: usize = 16 * 2;
    pub const TEX_SIZE: usize = (64 * 64) / 2;
    pub const WALL_TEX_NUM: usize = 2;
    pub const FLOOR_TEX_NUM: usize = 4;
    pub const WALL_BANK_SIZE: usize = PAL_SIZE + WALL_TEX_NUM * TEX_SIZE;
    pub const FLOOR_BANK_SIZE: usize = PAL_SIZE + FLOOR_TEX_NUM * TEX_SIZE;
    /// `aMI_BANK_NUM` — two wall banks and two floor banks; the
    /// initial load copies the current data into BOTH.
    pub const BANK_NUM: usize = 2;
}

/// Floor-sound indices (`m_name_table_floor_decl.h`).
pub mod floor_snd {
    pub const BASEMENT: u8 = 82;
    pub const ORIGINAL_SE: u8 = 83;
    /// `FLOOR_PLAYER_ROOM_END` — the exclusive upper bound for the
    /// direct mapping.
    pub const PLAYER_ROOM_END: i16 = 71;

    /// `aMI_MyIndoorDma` tail / `aNI_SetFloorSE`, verbatim:
    /// basement → FLOOR_BASEMENT; `0 <= floor_num < 71` →
    /// floor_num; otherwise FLOOR_ORIGINAL_SE.
    pub fn select(floor_num: i16, is_basement: bool) -> u8 {
        if is_basement {
            BASEMENT
        } else if floor_num >= 0 && floor_num < PLAYER_ROOM_END {
            floor_num as u8
        } else {
            ORIGINAL_SE
        }
    }
}

/// Wallpaper/flooring index validation
/// (`aMI_CheckFloorWallIndex`), verbatim: valid when
/// `0 <= idx < 67` or an original design; otherwise reset to 0.
pub mod validate {
    pub const ETC_START: i16 = 67;

    pub fn index(idx: i16, is_original: bool) -> u16 {
        if (idx >= 0 && idx < ETC_START) || is_original {
            idx as u16
        } else {
            0
        }
    }
}

/// Room step flags (`aMI_MakeStepData`), verbatim.
/// `aMI_STEP_TYPE_NUM` = 4.
pub mod steps {
    pub const NUM: usize = 4;

    /// (scene, has_basement, house_floor_no) → the four step flags.
    pub fn make(scene: u16, has_basement: bool, floor_no: u8) -> [bool; NUM] {
        use super::scene as sc;
        let mut step = [false; NUM];
        if scene == sc::MY_ROOM_M || scene == sc::MY_ROOM_L || scene == sc::MY_ROOM_LL1 {
            if has_basement {
                step[0] = true;
            }
        }
        if scene == sc::MY_ROOM_LL1 {
            step[1] = true;
        }
        if scene == sc::MY_ROOM_LL2 {
            step[2] = true;
        }
        if floor_no == 2 {
            step[3] = true;
        }
        step
    }
}

/// House clocks (`ac_house_clock.c`).
pub mod clock {
    use super::scene as sc;

    pub const TYPE_SHOP1: u8 = 0;
    pub const TYPE_SHOP2: u8 = 1;
    pub const TYPE_SHOP3: u8 = 2;
    pub const TYPE_SHOP4: u8 = 3;
    pub const TYPE_SHOP4_2: u8 = 4;
    pub const TYPE_POST_OFFICE: u8 = 5;
    pub const TYPE_POLICE_BOX: u8 = 6;
    pub const TYPE_MUSEUM: u8 = 7;
    pub const TYPE_TAILORS: u8 = 8;
    pub const TYPE_NUM: u8 = 9;

    /// Clock chime sound (`include/audio_defs.h`).
    pub const NA_SE_12D: u16 = 0x12D;

    /// The 120-frame phase and the chime targets.
    pub const PHASE: f32 = 120.0;
    pub const CHIME_A: f32 = 15.0;
    pub const CHIME_B: f32 = 45.0;
    /// Keyframe animation speed.
    pub const ANIM_SPEED: f32 = 0.5;

    /// Short-hand/long-hand joint indices per clock type
    /// (from the draw-data table).
    pub const HAND_JOINTS: [(u8, u8); 9] = [
        (5, 6), // shop1
        (3, 4), // shop2
        (3, 4), // shop3
        (5, 6), // shop4
        (5, 6), // shop4_2 (same data as shop4)
        (3, 4), // post office
        (3, 4), // police box
        (3, 4), // museum
        (5, 6), // tailors
    ];

    /// Clock position table (`aHC_position_data`), verbatim:
    /// scene → (clock type, position).
    pub fn for_scene(scene: u16) -> Option<(u8, (f32, f32, f32))> {
        const TABLE: [(u16, u8, (f32, f32, f32)); 9] = [
            (sc::SHOP0, TYPE_SHOP1, (200.0, 40.0, 40.0)),
            (sc::CONVENI, TYPE_SHOP2, (240.0, 0.0, 40.0)),
            (sc::SUPER, TYPE_SHOP3, (160.0, 0.0, 40.0)),
            (sc::DEPART, TYPE_SHOP4, (240.0, 0.0, 120.0)),
            (sc::DEPART_2, TYPE_SHOP4_2, (240.0, 0.0, 120.0)),
            (sc::POST_OFFICE, TYPE_POST_OFFICE, (120.0, 55.0, 135.0)),
            (sc::POLICE_BOX, TYPE_POLICE_BOX, (200.0, 0.0, 30.0)),
            (sc::MUSEUM_ENTRANCE, TYPE_MUSEUM, (0.0, 0.0, 0.0)),
            (sc::NEEDLEWORK, TYPE_TAILORS, (0.0, 0.0, 0.0)),
        ];
        for &(s, t, p) in &TABLE {
            if scene == s {
                return Some((t, p));
            }
        }
        None
    }

    /// The 120-frame phase accumulator (`aHC_OperateAnime`),
    /// verbatim: advance by dt, wrap at 120.
    pub fn phase_step(prev: f32, dt_frames: f32) -> f32 {
        let mut frame = prev + dt_frames;
        while frame >= PHASE {
            frame -= PHASE;
        }
        frame
    }

    /// `aHC_CrossedFrame`, verbatim.
    pub fn crossed_frame(prev: f32, now: f32, target: f32) -> bool {
        if prev <= now {
            prev < target && now >= target
        } else {
            prev < target || now >= target
        }
    }

    /// Chime test: frames 15 and 45 crossed within the phase.
    pub fn chime(prev: f32, now: f32) -> bool {
        crossed_frame(prev, now, CHIME_A) || crossed_frame(prev, now, CHIME_B)
    }

    /// Hand angles are RTC-driven (`aHC_DrawClockAfter`): the
    /// short hand rotates `90° - rad_hour`, the long hand
    /// `90° - rad_min`, in binary short angles
    /// (`DEG2SHORT_ANGLE2(90.0f)` = 0x4000). The `rad_*`
    /// computation itself lives in the time system.
    pub fn hand_angle(rad: i16) -> i16 {
        0x4000_i16.wrapping_sub(rad)
    }

    /// Shop clocks hide during the monthly raffle
    /// (`tanuki_shop_status == FUKUBIKI && clock_num <= 4`).
    pub fn hidden_by_raffle(shop_status: u8, clock_num: u8) -> bool {
        shop_status == super::shop_status_fukubiki() && clock_num <= 4
    }
}

fn shop_status_fukubiki() -> u8 {
    3 // mSP_TANUKI_SHOP_STATUS_FUKUBIKI
}

/// Arrange room (`ac_arrange_room.c`): the packed
/// `wall_floor_num` has the wall index in the high byte and the
/// floor index in the low byte; BOTH clamp to 0..0x42.
pub mod arrange {
    pub const MAX_INDEX: u8 = 0x42;

    pub fn extract(packed: u16) -> (u8, u8) {
        let wall = (((packed >> 8) & 0xFF) as u8).min(MAX_INDEX);
        let floor = ((packed & 0xFF) as u8).min(MAX_INDEX);
        (wall, floor)
    }
}

/// Exterior house actor (`ac_my_house.c`).
pub mod exterior {
    use super::scene as sc;

    /// Door action states (`ac_my_house.c:19-25`); the fourth name
    /// carries the source's own `SCEEN` typo.
    pub const ACTION_WAIT: u8 = 0;
    pub const ACTION_OPEN_DOOR_WAIT: u8 = 1;
    pub const ACTION_OPEN_DOOR: u8 = 2;
    pub const ACTION_GOTO_NEXT_SCEEN_WAIT: u8 = 3;
    pub const ACTION_NUM: u8 = 4;

    /// Entry door data per house size
    /// (`aMHS_goto_next_pl_scene`), verbatim: next scene,
    /// startX, startZ. Orientation is north, wipe is
    /// WIPE_TYPE_TRIFORCE, fade is FADE_TYPE_DEMO.
    pub const ENTRY: [(u16, f32, f32); 4] = [
        (sc::MY_ROOM_S, 120.0, 220.0),
        (sc::MY_ROOM_M, 160.0, 300.0),
        (sc::MY_ROOM_L, 200.0, 380.0),
        (sc::MY_ROOM_LL1, 200.0, 380.0),
    ];

    /// Exit rewrite offsets (`aMHS_rewrite_pl_out_data`):
    /// world X ± 48.29f (by side), world Z + 48.29f.
    pub const EXIT_OFFSET: f32 = 48.29;
}

// ---- C ABI ----
// All pc_-prefixed: the C originals stay compiled; these are staged kernels.

#[no_mangle]
pub extern "C" fn pc_hi_room_kind(scene: u16) -> u8 {
    room_kind::for_scene(scene)
}

#[no_mangle]
pub extern "C" fn pc_hi_floor_se(floor_num: i16, is_basement: u8) -> u8 {
    floor_snd::select(floor_num, is_basement != 0)
}

#[no_mangle]
pub extern "C" fn pc_hi_validate_index(idx: i16, is_original: u8) -> u16 {
    validate::index(idx, is_original != 0)
}

#[no_mangle]
pub extern "C" fn pc_hi_room_steps(scene: u16, has_basement: u8, floor_no: u8, out: *mut u8) {
    if !out.is_null() {
        let steps = steps::make(scene, has_basement != 0, floor_no);
        let out = unsafe { std::slice::from_raw_parts_mut(out, steps::NUM) };
        for (i, &s) in steps.iter().enumerate() {
            out[i] = s as u8;
        }
    }
}

/// Returns the clock type, or -1 when the scene has no clock
/// (the actor switches to dummy callbacks). Writes the position
/// to `out` (3 floats) on success.
#[no_mangle]
pub extern "C" fn pc_hi_clock_pos(scene: u16, out: *mut f32) -> i8 {
    match clock::for_scene(scene) {
        Some((t, (x, y, z))) => {
            if !out.is_null() {
                let out = unsafe { std::slice::from_raw_parts_mut(out, 3) };
                out[0] = x;
                out[1] = y;
                out[2] = z;
            }
            t as i8
        }
        None => -1,
    }
}

#[no_mangle]
pub extern "C" fn pc_hi_crossed_frame(prev: f32, now: f32, target: f32) -> u8 {
    clock::crossed_frame(prev, now, target) as u8
}

#[no_mangle]
pub extern "C" fn pc_hi_clock_chime(prev: f32, now: f32) -> u8 {
    clock::chime(prev, now) as u8
}

#[no_mangle]
pub extern "C" fn pc_hi_clock_hidden(shop_status: u8, clock_num: u8) -> u8 {
    clock::hidden_by_raffle(shop_status, clock_num) as u8
}

/// Returns (wall << 8) | floor after the 0..0x42 clamps.
#[no_mangle]
pub extern "C" fn pc_hi_arrange_extract(packed: u16) -> u16 {
    let (wall, floor) = arrange::extract(packed);
    ((wall as u16) << 8) | (floor as u16)
}

/// Returns the entry scene for the house size (0-3), writing
/// startX/startZ to `out` (2 floats).
#[no_mangle]
pub extern "C" fn pc_hi_entry_start(house_size: u8, out: *mut f32) -> u16 {
    match exterior::ENTRY.get(house_size as usize) {
        Some(&(s, x, z)) => {
            if !out.is_null() {
                let out = unsafe { std::slice::from_raw_parts_mut(out, 2) };
                out[0] = x;
                out[1] = z;
            }
            s
        }
        None => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn room_kinds() {
        assert_eq!(room_kind::NUM, 6);
        assert_eq!(room_kind::for_scene(scene::MY_ROOM_S), room_kind::S);
        assert_eq!(room_kind::for_scene(scene::MY_ROOM_LL2), room_kind::LL2);
        assert_eq!(room_kind::for_scene(scene::MY_ROOM_BASEMENT_LL1), room_kind::BASEMENT);
        assert_eq!(room_kind::for_scene(scene::COTTAGE_MY), room_kind::L);
        // fallback: no second-floor or LL2-basement scene exists
        assert_eq!(room_kind::for_scene(scene::FG), room_kind::S);
        assert_eq!(room_kind::for_scene(999), room_kind::S);
    }

    #[test]
    fn bank_geometry() {
        assert_eq!(bank::PAL_SIZE, 32);
        assert_eq!(bank::TEX_SIZE, 2048);
        assert_eq!(bank::WALL_BANK_SIZE, 32 + 2 * 2048);
        assert_eq!(bank::FLOOR_BANK_SIZE, 32 + 4 * 2048);
        assert_eq!(bank::BANK_NUM, 2);
    }

    #[test]
    fn floor_sound() {
        assert_eq!(floor_snd::select(10, true), 82);
        assert_eq!(floor_snd::select(10, false), 10);
        assert_eq!(floor_snd::select(70, false), 70);
        assert_eq!(floor_snd::select(71, false), 83);
        assert_eq!(floor_snd::select(-1, false), 83);
    }

    #[test]
    fn validation() {
        assert_eq!(validate::index(66, false), 66);
        assert_eq!(validate::index(67, false), 0);
        assert_eq!(validate::index(-1, false), 0);
        // original designs bypass the range
        assert_eq!(validate::index(100, true), 100);
    }

    #[test]
    fn step_flags() {
        // M with basement -> step[0]
        assert_eq!(steps::make(scene::MY_ROOM_M, true, 1), [true, false, false, false]);
        // M without basement -> nothing
        assert_eq!(steps::make(scene::MY_ROOM_M, false, 1), [false, false, false, false]);
        // LL1 with basement -> step[0] and step[1]
        assert_eq!(steps::make(scene::MY_ROOM_LL1, true, 1), [true, true, false, false]);
        // LL2 -> step[2]
        assert_eq!(steps::make(scene::MY_ROOM_LL2, true, 1), [false, false, true, false]);
        // floor_no == 2 -> step[3]
        assert_eq!(steps::make(scene::MY_ROOM_S, false, 2), [false, false, false, true]);
    }

    #[test]
    fn clocks() {
        assert_eq!(clock::TYPE_NUM, 9);
        assert_eq!(
            clock::for_scene(scene::SHOP0),
            Some((clock::TYPE_SHOP1, (200.0, 40.0, 40.0)))
        );
        assert_eq!(
            clock::for_scene(scene::DEPART_2),
            Some((clock::TYPE_SHOP4_2, (240.0, 0.0, 120.0)))
        );
        assert_eq!(clock::for_scene(scene::FG), None);
        assert_eq!(clock::HAND_JOINTS[clock::TYPE_SHOP1 as usize], (5, 6));
        assert_eq!(clock::HAND_JOINTS[clock::TYPE_POST_OFFICE as usize], (3, 4));
        assert_eq!(clock::NA_SE_12D, 0x12D);
        // crossing semantics: no wrap
        assert!(clock::crossed_frame(10.0, 20.0, 15.0));
        assert!(!clock::crossed_frame(10.0, 14.0, 15.0));
        assert!(!clock::crossed_frame(15.0, 20.0, 15.0)); // prev < target is strict
        // wrap: prev > now crosses if target is past prev or at/before now
        assert!(clock::crossed_frame(115.0, 5.0, 0.0));
        assert!(clock::crossed_frame(115.0, 5.0, 118.0));
        assert!(!clock::crossed_frame(115.0, 5.0, 60.0));
        // chime at 15 and 45
        assert!(clock::chime(10.0, 20.0));
        assert!(clock::chime(40.0, 50.0));
        assert!(!clock::chime(50.0, 60.0));
        // phase wraps at 120
        assert_eq!(clock::phase_step(115.0, 10.0), 5.0);
        // hand angles: 90deg - rad, binary short
        assert_eq!(clock::hand_angle(0), 0x4000);
        // raffle hides shop clocks (types 0..=4)
        assert!(clock::hidden_by_raffle(3, 4));
        assert!(!clock::hidden_by_raffle(3, 5));
        assert!(!clock::hidden_by_raffle(0, 0));
    }

    #[test]
    fn arrange_extract() {
        // high byte = wall, low byte = floor, both clamp 0..0x42
        assert_eq!(arrange::extract(0x2030), (0x20, 0x30));
        assert_eq!(arrange::extract(0xFF50), (0x42, 0x42));
        assert_eq!(arrange::extract(0x0000), (0, 0));
    }

    #[test]
    fn exterior_states() {
        assert_eq!(exterior::ACTION_NUM, 4);
        assert_eq!(exterior::ENTRY[0], (scene::MY_ROOM_S, 120.0, 220.0));
        assert_eq!(exterior::ENTRY[3], (scene::MY_ROOM_LL1, 200.0, 380.0));
        assert_eq!(exterior::EXIT_OFFSET, 48.29);
    }
}
