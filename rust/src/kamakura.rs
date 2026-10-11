//! Igloo (`ac_kamakura.c`, `ac_kamakura_indoor.c`,
//! `ac_kamakura_npc0.c`).
//!
//! Verified against `src/actor/ac_kamakura.c`,
//! `src/actor/ac_kamakura_move.c_inc`, `src/actor/ac_kamakura_indoor.c`,
//! `include/ac_kamakura_indoor.h`, `src/actor/ac_kamakura_npc0.c`,
//! `src/game/m_event_schedule.c_inc`, `src/actor/ac_event_manager.c`,
//! `src/game/m_npc.c`, `src/actor/ac_set_npc_manager.c`,
//! `src/actor/ac_quest_manager.c`, `src/actor/ac_quest_talk_greeting.c`,
//! `src/actor/ac_quest_talk_normal_init.c` (GAFE01_00 Rev 0 USA).
//! 17/17 sections PROVEN with DIFFERS-level nuances.
//!
//! Scope split: this module owns the pure kernels — the
//! entry check, the exit position, the lighting target, the
//! candle phase and cosine interpolation, the greeting
//! selection, and the furniture-list branch. Actor
//! lifecycle, foreground placement (`make_FG_somewhere_lot4sale`/
//! `delete_FG`), the door request/demo/wipe, save-area
//! access, NPC/move-actor creation, quest-manager state,
//! RNG call order, effects/audio dispatch, the `anim_frame`
//! accumulator, the `graph_dt_period_elapsed` steam cadence,
//! and the villager-selection policy inside
//! `mEvMN_GetNpcJointEv` stay in C. The kamakura schedule
//! row (Jan 2 – Feb 23) lives in the sibling `event_day`
//! module.
//!
//! Nuances carried: steam is gated by
//! `graph_dt_period_elapsed(game, &dt_accum, 8.0f)`, NOT
//! `game_frame & 7` (brief was wrong); candle phases use
//! `(u32)k_indoor->anim_frame` (wraps at 1024), NOT
//! `game->frame_counter` (brief was wrong); the chase rate
//! is `0.019532442f * dt_num_60fps_frames` (frame-rate
//! independent); `aKI_MakeSinPointHalf` is a midpoint/
//! cosine interpolation, not linear; the exit record also
//! sets `exit_type = 0; extra_data = 3`; `cos_s` here is
//! the f32 cosine equivalent (retail uses a short-angle
//! lookup table); `aKI_mochi_anime_data` is unused.

/// Foreground actor placed by the event (placement ID 0x51).
pub const KAMAKURA: u16 = 0x5829;
/// Dummy marker placed at the igloo home position.
pub const DUMMY_KAMAKURA: u16 = 0xF107;
/// Event NPC id for the kamakura camper.
pub const SP_NPC_EV_KAMAKURA_0: u16 = 0xD05E;
/// Indoor ambience actor.
pub const ETC_KAMAKURA_INDOOR: u16 = 0x8014;
/// Indoor scene number.
pub const SCENE_KAMAKURA: i32 = 31;
/// Indoor door record arrival position: (160, 0, 300), facing north.
pub const INDOOR_ENTRY: [f32; 3] = [160.0, 0.0, 300.0];
/// Exit offsets: position = (actor X, ground Y, actor Z + 86).
pub const EXIT_Z_OFFSET: f32 = 86.0;
/// Door request is made at (actor X, player Y, actor Z + 68).
pub const DOOR_REQUEST_Z_OFFSET: f32 = 68.0;
/// Entry check measures distance from (actor X, actor Z + 40).
pub const CHECK_Z_OFFSET: f32 = 40.0;
/// Squared-distance threshold: 40 world units.
pub const CHECK_DIST2_MAX: f32 = 1600.0;
/// Strict facing interval: (DEG2SHORT_ANGLE2(135°), DEG2SHORT_ANGLE2(225°)).
pub const ENTRY_ANGLE_LO: i32 = 24576; // 0x6000
pub const ENTRY_ANGLE_HI: i32 = 40960; // 0xA000;
/// Door request faces -DEG2SHORT_ANGLE(180°).
pub const DOOR_REQUEST_ANGLE: i32 = -32768;
/// Lighting: day is 06:00 inclusive – 18:00 exclusive.
pub const LIGHT_DAY_START_SEC: i32 = 21600;
pub const LIGHT_DAY_END_SEC: i32 = 64800;
/// Indoor move-callback sound.
pub const NA_SE_KOKORO_TOGURU: u16 = 0x54;
/// Steam spawns at mochi Y + 42.
pub const STEAM_Y_OFFSET: f32 = 42.0;
/// Candle counts.
pub const CANDLE_NUM: i32 = 2;
/// Candle animation periods.
pub const CANDLE_RING_PERIOD: u32 = 34;
pub const CANDLE_FIRE_PERIOD: u32 = 24;
/// Ring/fire stagger offsets.
pub const CANDLE_RING_LATE: [u32; 2] = [0, 7];
pub const CANDLE_FIRE_LATE: [u32; 2] = [0, 6];
/// `anim_frame` wraps at 1024.
pub const ANIM_FRAME_WRAP: f32 = 1024.0;

/// Greeting message bases per villager looks (identity macros).
pub const HELLO_MSG_BASE: [i32; 6] = [6367, 6376, 6358, 6385, 6394, 6403];
/// Greeting hello_type bits.
pub const GREETING_GAME_TYPE_MONEY: i32 = 1 << 0;
pub const GREETING_GAME_TYPE_ITEM: i32 = 1 << 1;
pub const GREETING_GAME_TYPE_ALL: i32 = 3;
/// Minimum wallet bells for the money greeting flag.
pub const GREETING_GAME_BELL_MIN: i32 = 3000;

/// Furniture list types.
pub const LISTTYPE_KAMAKURA: i32 = 19;
pub const LISTTYPE_TENT: i32 = 23;

/// Entry eligibility (`aKKR_check_player`): the player must
/// face into (24576, 40960) strictly and be within squared
/// distance 1600 of (actor X, actor Z + 40). The player-NULL
/// check and the door request stay in C.
#[no_mangle]
pub extern "C" fn pc_kamakura_entry_check(
    px: f32,
    pz: f32,
    ax: f32,
    az: f32,
    facing_y: i32,
) -> bool {
    let dx = px - ax;
    let dz = pz - (az + CHECK_Z_OFFSET);
    facing_y > ENTRY_ANGLE_LO && facing_y < ENTRY_ANGLE_HI && dx * dx + dz * dz < CHECK_DIST2_MAX
}

/// Exit position (`aKKR_rewrite_out_data`): (actor X,
/// ground Y, actor Z + 86). Writes to `out[0..3]`; returns
/// 1 on success. The wipe-mode gate, scene selection,
/// orientation, and wipe call stay in C.
#[no_mangle]
pub extern "C" fn pc_kamakura_exit_position(
    ax: f32,
    az: f32,
    ground_y: f32,
    out: *mut f32,
) -> u8 {
    if out.is_null() {
        return 0;
    }
    let out = unsafe { std::slice::from_raw_parts_mut(out, 3) };
    out[0] = ax;
    out[1] = ground_y;
    out[2] = az + EXIT_Z_OFFSET;
    1
}

/// Window-lighting target (`aKKR_ctrl_light`): true outside
/// 06:00 inclusive – 18:00 exclusive. The chase toward the
/// target stays in C.
#[no_mangle]
pub extern "C" fn pc_kamakura_light_target(now_sec: i32) -> bool {
    !(now_sec < LIGHT_DAY_END_SEC && now_sec >= LIGHT_DAY_START_SEC)
}

/// Candle phase: `((u32)anim_frame − late) % period`.
/// The `anim_frame` accumulator stays in C.
#[no_mangle]
pub extern "C" fn pc_kamakura_candle_phase(anim_frame: f32, late: u32, period: u32) -> u32 {
    (anim_frame as u32).wrapping_sub(late) % period
}

/// Short-angle cosine: retail `cos_s` is a lookup table;
/// this is the f32 equivalent.
fn cos_s(angle: i16) -> f32 {
    (angle as f32 * core::f32::consts::TAU / 65536.0).cos()
}

/// The cosine interpolation (`aKI_MakeSinPointHalf`)
/// verbatim, including the 3.14f phase constant and the
/// start/end clamp behavior.
#[no_mangle]
pub extern "C" fn pc_kamakura_sin_point_half(
    start_frame: i16,
    start: f32,
    end_frame: i16,
    end: f32,
    now_frame: i16,
) -> f32 {
    if start_frame > end_frame || start_frame == end_frame {
        return start;
    }
    if now_frame <= start_frame {
        return start;
    }
    if now_frame >= end_frame {
        return end;
    }
    let sf = start_frame as f32;
    let ef = end_frame as f32;
    let m = ef - sf;
    let midpoint = (start + end) * 0.5;
    let nf = now_frame as f32;
    let halfpoint = (end - midpoint).abs();
    let percent = ((nf - sf) * 3.14) / m;
    let angle = (percent * 10430.378) as i16;
    if start >= end {
        midpoint + cos_s(angle) * halfpoint
    } else {
        midpoint + cos_s(angle) * -halfpoint
    }
}

/// Greeting message selection: `looks` indexes the base
/// table; `money_flag`/`item_flag` set the hello_type
/// bits. When both are set the message is
/// `1 + base[looks] + rng2` (the RANDOM(2) sample stays in
/// C); otherwise `base[looks] + hello_type`. A
/// `first_meeting` greeting returns -1 — the generic
/// how-do-you-do helper stays in C.
#[no_mangle]
pub extern "C" fn pc_kamakura_greeting_msg(
    looks: i32,
    money_flag: bool,
    item_flag: bool,
    first_meeting: bool,
    rng2: i32,
) -> i32 {
    if first_meeting {
        return -1;
    }
    if looks < 0 || looks >= HELLO_MSG_BASE.len() as i32 {
        return -1;
    }
    let mut hello_type = 0;
    if money_flag {
        hello_type |= GREETING_GAME_TYPE_MONEY;
    }
    if item_flag {
        hello_type |= GREETING_GAME_TYPE_ITEM;
    }
    let base = HELLO_MSG_BASE[looks as usize];
    if hello_type == GREETING_GAME_TYPE_ALL {
        1 + base + rng2
    } else {
        base + hello_type
    }
}

/// Furniture trade-list choice: kamakura + rnd ≥ 90 → 19;
/// tent + rnd ≥ 80 → 23; otherwise the C-owned default
/// list. The RANDOM(100) sample, the scene check, the
/// category gate, and the actual item lookup stay in C.
#[no_mangle]
pub extern "C" fn pc_kamakura_trade_list(
    scene_is_kamakura: bool,
    scene_is_tent: bool,
    rnd100: i32,
    default_list: i32,
) -> i32 {
    if scene_is_kamakura && rnd100 >= 90 {
        LISTTYPE_KAMAKURA
    } else if scene_is_tent && rnd100 >= 80 {
        LISTTYPE_TENT
    } else {
        default_list
    }
}

/// Returns true when the event-save byte is not the 0xFF
/// sentinel (the save-area access stays in C).
#[no_mangle]
pub extern "C" fn pc_kamakura_save_byte_valid(save_byte: u8) -> bool {
    save_byte != 0xFF
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry() {
        // Player 20 units south of the check point, facing into the interval.
        assert!(pc_kamakura_entry_check(0.0, 60.0, 0.0, 0.0, 30000));
        // Strict angle bounds.
        assert!(!pc_kamakura_entry_check(0.0, 60.0, 0.0, 0.0, 24576));
        assert!(!pc_kamakura_entry_check(0.0, 60.0, 0.0, 0.0, 40960));
        // Strict distance: exactly 1600 fails.
        assert!(!pc_kamakura_entry_check(0.0, 80.0, 0.0, 0.0, 30000));
        // Z offset: check point is az + 40.
        assert!(pc_kamakura_entry_check(0.0, 79.0, 0.0, 0.0, 30000));
    }

    #[test]
    fn exit_and_light() {
        let mut out = [0.0f32; 3];
        assert_eq!(pc_kamakura_exit_position(100.0, 200.0, 12.5, out.as_mut_ptr()), 1);
        assert_eq!(out, [100.0, 12.5, 286.0]);
        // Day interval: 06:00 inclusive -> 18:00 exclusive.
        assert!(!pc_kamakura_light_target(21599));
        assert!(!pc_kamakura_light_target(21600));
        assert!(!pc_kamakura_light_target(64799));
        assert!(pc_kamakura_light_target(64800));
        assert!(pc_kamakura_light_target(0));
    }

    #[test]
    fn candle() {
        // (0 - 7) wraps to u32::MAX - 6; % 34 = 11 (literal unsigned C arithmetic).
        assert_eq!(pc_kamakura_candle_phase(0.0, 7, 34), 11);
        assert_eq!(pc_kamakura_candle_phase(7.0, 7, 34), 0);
        assert_eq!(pc_kamakura_candle_phase(34.0, 0, 34), 0);
        assert_eq!(pc_kamakura_candle_phase(6.0, 6, 24), 0);
        // Midpoint behavior of the cosine interpolation.
        let v = pc_kamakura_sin_point_half(0, 0.0, 34, 1.0, 17);
        assert!((v - 0.5).abs() < 0.05, "v = {v}");
        assert_eq!(pc_kamakura_sin_point_half(0, 0.0, 34, 1.0, 0), 0.0);
        assert_eq!(pc_kamakura_sin_point_half(0, 0.0, 34, 1.0, 34), 1.0);
        assert_eq!(pc_kamakura_sin_point_half(5, 2.0, 5, 9.0, 5), 2.0);
        assert_eq!(pc_kamakura_sin_point_half(0, 2.0, 34, 1.0, -1), 2.0);
    }

    #[test]
    fn greeting() {
        assert_eq!(pc_kamakura_greeting_msg(0, false, false, false, 0), 6367);
        assert_eq!(pc_kamakura_greeting_msg(0, true, false, false, 0), 6367 + 1);
        assert_eq!(pc_kamakura_greeting_msg(0, false, true, false, 0), 6367 + 2);
        // Both flags: base skipped, 1 + base + rng2.
        assert_eq!(pc_kamakura_greeting_msg(2, true, true, false, 1), 1 + 6358 + 1);
        // First meeting is C-owned.
        assert_eq!(pc_kamakura_greeting_msg(0, true, true, true, 0), -1);
        assert_eq!(pc_kamakura_greeting_msg(6, false, false, false, 0), -1);
    }

    #[test]
    fn trade_list() {
        assert_eq!(pc_kamakura_trade_list(true, false, 90, 0), 19);
        assert_eq!(pc_kamakura_trade_list(true, false, 89, 0), 0);
        assert_eq!(pc_kamakura_trade_list(false, true, 80, 0), 23);
        assert_eq!(pc_kamakura_trade_list(false, true, 79, 0), 0);
        assert_eq!(pc_kamakura_trade_list(false, false, 99, 7), 7);
        assert!(pc_kamakura_save_byte_valid(0));
        assert!(!pc_kamakura_save_byte_valid(0xFF));
    }
}
