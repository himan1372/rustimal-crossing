//! Town buildings: Nook's Nook 'n' Go / Nookway (`ac_conveni`, `ac_super`),
//! the lighthouse and its switch (`ac_toudai`, `ac_lighthouse_switch`),
//! island cottages (`ac_cottage`), Redd's / visitor tent (`ac_tent`),
//! paid-off house statues (`ac_douzou`), the windmill (`ac_windmill`) and
//! the structure-actor manager (`ac_structure` + `_clip.c_inc`).
//!
//! Source-verified against ac-decomp (primary) and ACGC-PC-Port
//! (secondary). Message ids, door angles and the structure table were
//! checked against the headers / compiled with mingw32 gcc.
//!
//! ## PC-port changes
//! - Window-light fades (`0.019532442` per frame) and the statue sparkle
//!   timers scale by `dt`; lighthouse colour/pole counters run in 60 Hz
//!   steps; lighthouse colours are clamped to u8.
//! - `ac_structure`: actor slots enlarged to 0x300 bytes because
//!   `TOUDAI_ACTOR` outgrew `STRUCTURE_ACTOR` with the PC dt fields;
//!   structures spawn with their own block numbers (borderless).
//! - `ac_cottage`: `y > -DEG2SHORT_ANGLE(180.0f)` is unreachable under GCC
//!   (`(s16)32768.0f` = -32768, negated in int = +32768). Verified with
//!   mingw32 gcc at -O0 and -O2. The PC port special-cases it with
//!   `y > -32768`; retail evidently works (players enter the cottage), so
//!   Metrowerks must fold that constant differently. It is the only
//!   comparison against `DEG2SHORT_ANGLE(180)` in the source (grep);
//!   argument uses like `-DEG2SHORT_ANGLE(180.0f)` wrap back to 0x8000.
//!
//! ## Facts
//! - Shops (Nook 'n' Go / Nookway): doors open when the shop is OPEN or
//!   OPENEVENT and the player is near (d^2 < 800 / 820); entering needs the
//!   player facing 90-180 deg and moving within d^2 < 260 / 240 of the door.
//!   Closed shops talk (window 145,60,40): raffle days say "raffle at 6:00" /
//!   "raffle ended"; otherwise closed, pre-event, end-event, renewal.
//!   Nookway's fall-through "open" message is its *closed* text
//!   (MSG_SHOP2_CLOSED_NORMAL) where Nook 'n' Go uses MSG_SHOP_OPEN.
//!   Window lights on while open or while the player walks in.
//! - Lighthouse: beam animation (1..100, repeat) runs from 18:00 to 5:00
//!   once the switch quest is done (`mSC_LightHouse_Switch_Check`); the
//!   beam colour pulses around keyframe 51 (`aTOU_color_ctrl`). Door: |dx|
//!   < 20, -65 < dz < 0, facing +-45 deg, only when entry is allowed.
//! - Lighthouse switch: night (18:00-5:00) auto-ON outside the quest
//!   period; during the January/February quest (`PERIOD_1`) the player
//!   must press A at the switch (within ~33.57 units of (180,100), facing
//!   135 deg +-45). Pole spins up in two cranks (speeds 0.065 then 0.05525,
//!   each easing out over 30 frames, 48 frames total) then accelerates
//!   +0.0035/frame to 0.5; switching off decelerates, reverses, and flips
//!   the switch back after 49 frames.
//! - Cottages: player cottage light follows the room light switch; the
//!   islander's light is on at night when they are inside and awake
//!   (US/JP) or simply not asleep (Australian build). A sleeping islander
//!   comes out to talk instead of letting you in.
//! - Tent: lights before 5:00 / from 18:00. Despawn check bug: both block
//!   coordinates are computed from the *tent's* position, so the tent never
//!   despawns on an acre change (PC keeps this).
//! - Statues: one per paid-off house (`next_size == STATUE`), at
//!   (0,200)/(200,200)/(-40,280)/(160,280) from the statue anchor; sparkles
//!   every 5-15 / 15-45 / 20-60 / 30-90 frames by rank. Talking needs the
//!   player facing north within 50 units and +-0x1800. A pending Resetti
//!   visit hijacks the talk (`reset_type = 4`). Data quirk: girl face type
//!   7 uses the boy's face-7 texture (`obj_[sw]_douzou_b7`) for the eyes.
//! - Windmill: no constructor, empty draw, never sets `action_proc` - its
//!   move would call NULL; a dead N64-era structure (would crash if spawned).
//! - Structure manager: 9 shared actor slots + overlay buffers;
//!   `setupInfo_table` maps structure FG ids to profiles (the tent reuses
//!   the Kamakura type and palette).

use crate::game_math::Xyz;

// ---------------------------------------------------------------------------
// Shops (conveni / super)
// ---------------------------------------------------------------------------

/// `mSP_SHOP_STATUS_*`.
pub const SHOP_PRE: i32 = 0;
pub const SHOP_END: i32 = 1;
pub const SHOP_OPEN: i32 = 2;
pub const SHOP_RENEW: i32 = 3;
pub const SHOP_PREEVENT: i32 = 4;
pub const SHOP_ENDEVENT: i32 = 5;
pub const SHOP_OPENEVENT: i32 = 6;

/// Per-building parameters of the shared shop-door state machine.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShopParams {
    pub door_offset: (f32, f32),
    pub enter_dist2: f32,
    pub near_dist2: f32,
    pub exit_offset: (f32, f32),
    pub walk_in_offset: (f32, f32),
    pub talk_distance: f32,
    pub cull: f32,
    pub msg_open: i32,
    pub msg_closed: i32,
    pub msg_preevent: i32,
}

/// Nook 'n' Go (`ac_conveni`).
pub const CONVENI: ShopParams = ShopParams {
    door_offset: (-14.0, 76.0),
    enter_dist2: 260.0,
    near_dist2: 800.0,
    exit_offset: (-42.0, 98.57),
    walk_in_offset: (-25.0, 82.5),
    talk_distance: 100.0,
    cull: 550.0,
    msg_open: 1997,
    msg_closed: 1998,
    msg_preevent: 2002,
};

/// Nookway (`ac_super`): its "open" fall-through uses the closed text.
pub const SUPER: ShopParams = ShopParams {
    door_offset: (-38.0, 96.0),
    enter_dist2: 240.0,
    near_dist2: 820.0,
    exit_offset: (-62.0, 118.57),
    walk_in_offset: (-45.0, 102.5),
    talk_distance: 120.0,
    cull: 600.0,
    msg_open: 1999,
    msg_closed: 1999,
    msg_preevent: 2003,
};

pub const MSG_RAFFLE_TIME_ANNOUNCE: i32 = 2006;
pub const MSG_RAFFLE_ENDED: i32 = 2007;
pub const MSG_SHOP_RENEW: i32 = 2008;
pub const MSG_SHOP_CLOSED_ENDEVENT: i32 = 2009;

pub fn shop_is_open(status: i32) -> bool {
    status == SHOP_OPEN || status == SHOP_OPENEVENT
}

/// `aCNV/aSPR_check_player`: 0 far, 1 near, 2 at the door facing in.
pub fn shop_check_player(p: &ShopParams, dx: f32, dz: f32, rot_y: u16) -> i32 {
    let d2 = dx * dx + dz * dz;
    if rot_y > 0x4000 && rot_y < 0x8000 && d2 < p.enter_dist2 {
        2
    } else if d2 < p.near_dist2 {
        1
    } else {
        0
    }
}

/// `aCNV/aSPR_set_talk_info_close_wait` message choice.
pub fn shop_closed_message(p: &ShopParams, raffle_day: bool, now_sec: i32, status: i32) -> i32 {
    if raffle_day {
        if now_sec >= 6 * 3600 {
            if status == SHOP_PRE {
                MSG_RAFFLE_TIME_ANNOUNCE
            } else {
                MSG_RAFFLE_ENDED
            }
        } else {
            p.msg_closed
        }
    } else {
        match status {
            SHOP_PRE | SHOP_END => p.msg_closed,
            SHOP_PREEVENT => p.msg_preevent,
            SHOP_ENDEVENT => MSG_SHOP_CLOSED_ENDEVENT,
            SHOP_RENEW => MSG_SHOP_RENEW,
            _ => p.msg_open,
        }
    }
}

/// Door animation (start, end, speed) per action: close-wait, open-wait,
/// close-door, open-door, player-into-wait.
pub const SHOP_DOOR_ANIM: [(f32, f32, f32); 5] =
    [(1.0, 1.0, 0.0), (16.0, 16.0, 0.0), (16.0, 1.0, 0.5), (1.0, 16.0, 0.5), (16.0, 16.0, 0.0)];

/// Window light fade step per frame (`chase_f`; PC multiplies by dt).
pub const WINDOW_LIGHT_STEP: f32 = 0.019532442;

// ---------------------------------------------------------------------------
// Lighthouse
// ---------------------------------------------------------------------------

/// Night window for the beam / switch (`sec >= 18:00 || sec < 5:00`).
pub fn lighthouse_night(now_sec: i32) -> bool {
    !(18000..64800).contains(&now_sec)
}

/// `aTOU_check_door_pos`.
pub fn lighthouse_at_door(dx: f32, dz: f32, rot_y: i16) -> bool {
    dx.abs() < 20.0 && dz > -65.0 && dz < 0.0 && rot_y > -0x2000 && rot_y < 0x2000
}

/// Beam colour state (`unk2C8`, `unk2CC`, `unk2D0`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BeamColor {
    pub b: f32,
    pub a: f32,
    pub env: f32,
}

/// `aTOU_color_ctrl` (retail: `dt = 1`).
pub fn beam_color_step(c: &mut BeamColor, ready: bool, current_frame: i32, dt: f32) {
    use crate::game_math::add_calc;
    let k7 = 1.0 - 0.7f32.sqrt();
    let k9 = 1.0 - 0.9f32.sqrt();
    if !ready {
        add_calc(&mut c.b, 0.0, k7, 50.0, 0.5);
        add_calc(&mut c.a, 0.0, k7, 50.0, 0.5);
        add_calc(&mut c.env, 0.0, k9, 50.0, 0.5);
        return;
    }
    let t = current_frame - 51;
    let f = if t < 0 { 1.0 } else { -1.0 };
    let t = t.abs();
    if t == 0 {
        c.b = 220.0;
        c.a = 240.0;
    } else if t >= 40 {
        add_calc(&mut c.b, 0.0, k7, 50.0, 0.5);
        add_calc(&mut c.a, 0.0, k7, 50.0, 0.5);
    } else if t < 10 {
        c.b += 4.5 * f * dt;
        c.a += 7.0 * f * dt;
    } else if t < 30 {
        c.b += 1.25 * f * dt;
        c.a += 2.25 * f * dt;
    } else {
        c.b += 4.0 * f * dt;
        c.a += 0.5 * f * dt;
    }
    add_calc(&mut c.env, 255.0, k9, 50.0, 0.5);
}

/// `aLS_NICE_STATUS_*`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SwitchNice {
    Off,
    SwitchOn,
    LighthousePeriod,
}

/// `aLS_GetNiceStatus`: hour from the RTC; `quest_period` =
/// `mSC_LightHouse_get_period == PERIOD_1`.
pub fn switch_nice_status(hour: u8, quest_period: bool) -> SwitchNice {
    if !(5..18).contains(&hour) {
        if quest_period {
            SwitchNice::LighthousePeriod
        } else {
            SwitchNice::SwitchOn
        }
    } else {
        SwitchNice::Off
    }
}

/// `aLS_CheckPlayerSwitchPositionAngle` (debug regs 0).
pub fn switch_reachable(x: f32, z: f32, rot_y: i16) -> bool {
    let (dx, dz) = (x - 180.0, z - 100.0);
    if dx * dx + dz * dz >= 33.5684f32 * 33.5684 {
        return false;
    }
    let a = rot_y as u16;
    // DIFF_USHORT_ANGLE(135 deg, 45 deg) .. DIFF_USHORT_ANGLE(45 deg, -135 deg)
    let lo = 0x6000u16.wrapping_sub(0x2000);
    let hi = 0x2000u16.wrapping_sub(0xA000);
    a > lo && a < hi
}

/// Pole state (`aLS_pole_c` speed bookkeeping).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Pole {
    /// 1 = running, 0 = stopping.
    pub running: bool,
    pub keyframe_state: u8,
    pub timer: i32,
    pub off_timer: i32,
    pub speed: f32,
    /// Animation direction flipped (start 100 -> end 1).
    pub reversed: bool,
}

/// What a pole frame asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PoleEvent {
    None,
    /// Play the switch-off animation (`aLS_RequestSwitchOFF`).
    SwitchOff,
}

/// `aLS_GetNowPoleAnimeSpeed` (retail, debug regs 0).
pub fn pole_step(p: &mut Pole) -> PoleEvent {
    use crate::game_math::add_calc;
    let base = 0.1f32;
    let mut ev = PoleEvent::None;
    if !p.running {
        match p.keyframe_state {
            0 => {
                p.speed -= 0.001;
                if p.speed < base {
                    p.keyframe_state = 1;
                }
            }
            1 => {
                let target = base + 0.07;
                add_calc(&mut p.speed, target, 0.4, 1.0, 0.001);
                if (p.speed - target).abs() < 0.001 {
                    p.keyframe_state = 2;
                    p.reversed = true;
                    p.speed = base * 1.7;
                    p.off_timer = 49;
                }
            }
            _ => {
                add_calc(&mut p.speed, 0.0, 0.4, 1.0, 0.001);
                if p.off_timer > 1 {
                    p.off_timer -= 1;
                } else if p.off_timer == 1 {
                    ev = PoleEvent::SwitchOff;
                    p.off_timer = 0;
                } else {
                    p.off_timer = 0;
                }
            }
        }
    } else if p.timer < 48 {
        let frame = p.timer % 30;
        let kind = p.timer / 30;
        if frame == 0 {
            p.speed = if kind == 0 { 0.065 } else { 0.85 * 0.065 };
        } else {
            add_calc(&mut p.speed, 0.0, 0.09, 0.09 * 0.065, 0.00001);
        }
        p.timer += 1;
    } else {
        p.speed += 0.0035;
    }
    p.speed = p.speed.clamp(0.0, 0.5);
    ev
}

// ---------------------------------------------------------------------------
// Cottages
// ---------------------------------------------------------------------------

/// Door SE frame checkpoints (`Cottage_set_doorSE`), SE 6..9.
pub const COTTAGE_DOOR_SE_IN: [f32; 4] = [10.0, 14.0, 35.0, 50.0];
pub const COTTAGE_DOOR_SE_OUT: [f32; 4] = [2.0, 8.0, 33.0, 40.0];

/// `Cottage_my_check_door_pl_in` geometry + angle. `retail_metrowerks`
/// = the shipping behaviour; `false` reproduces a plain GCC build where the
/// `-DEG2SHORT_ANGLE(180)` bound is +32768 and the check never passes.
pub fn my_cottage_can_enter(dx: f32, dz: f32, rot_y: i32, a_pressed: bool, retail_metrowerks: bool) -> bool {
    if !(dx * dx + dz * dz < 1600.0 && dx + dz < 40.0 && a_pressed) {
        return false;
    }
    let lower = if retail_metrowerks { -32768 } else { 32768 };
    rot_y > lower && rot_y < -0x4000
}

/// `Cottage_npc_light_aim`. `aus` selects the GAFU01 rule.
pub fn npc_cottage_light(sleeping: bool, in_house_sched: bool, at_home: bool, now_sec: i32, player_leaving: bool, aus: bool) -> bool {
    let awake_ok = if aus { !sleeping } else { in_house_sched };
    awake_ok && at_home && (now_sec >= 18 * 3600 || now_sec <= 5 * 3600) && !player_leaving
}

// ---------------------------------------------------------------------------
// Tent
// ---------------------------------------------------------------------------

/// `aTnt_ControlLight`.
pub fn tent_light(now_sec: i32) -> bool {
    !(18000..64800).contains(&now_sec)
}

/// `aTnt_check_player`: facing north within 40 of (x, z + 40).
pub fn tent_check_player(dx: f32, dz_from_door: f32, rot_y: u16) -> bool {
    rot_y > 0x6000 && rot_y < 0xA000 && dx * dx + dz_from_door * dz_from_door < 1600.0
}

/// The tent's (buggy) despawn test: both blocks come from the tent itself.
pub fn tent_despawns(_player_block: (i32, i32), tent_block: (i32, i32)) -> bool {
    let (bx1, bz1) = tent_block;
    let (bx2, bz2) = tent_block;
    bx1 != bx2 || bz1 != bz2
}

// ---------------------------------------------------------------------------
// Statues
// ---------------------------------------------------------------------------

/// Statue offsets per house (x, z) from the anchor.
pub const STATUE_OFFSETS: [(f32, f32); 4] = [(0.0, 200.0), (200.0, 200.0), (-40.0, 280.0), (160.0, 280.0)];
/// Sparkle intervals by statue rank: base + random * range frames.
pub const STATUE_SPARKLE_BASE: [f32; 4] = [5.0, 15.0, 20.0, 30.0];
pub const STATUE_SPARKLE_RANGE: [f32; 4] = [10.0, 30.0, 40.0, 60.0];
pub const MSG_DOZOU: i32 = 4971;

/// Eye texture index (into the 32-entry `eye_tbl`); `winter` picks the
/// second half. Reproduces the girl-face-7 -> boy texture quirk.
pub fn statue_eye_texture(sex: usize, face: usize, winter: bool) -> (usize, bool) {
    let idx = (winter as usize) * 16 + sex * 8 + face;
    let uses_boy_face7 = sex == 1 && face == 6;
    (idx, uses_boy_face7)
}

/// `aDOU_wait` talk test for one statue.
pub fn statue_can_talk(player_rot_y: i16, dist_xz: f32, angle_from_statue: i16) -> bool {
    (player_rot_y as i32).abs() >= 0x6000 && dist_xz <= 50.0 && (angle_from_statue as i32).abs() <= 0x1800
}

// ---------------------------------------------------------------------------
// Structure manager
// ---------------------------------------------------------------------------

/// `aSTR_ACTOR_TBL_COUNT`.
pub const STRUCTURE_SLOTS: usize = 9;
pub const STRUCTURE_START: u16 = 0x5800;
pub const NPC_HOUSE_END: u16 = 0x50EE;
/// Profile id of `mAc_PROFILE_HOUSE` (villager houses) and `STATION`.
pub const PROFILE_HOUSE: i16 = 40;

/// `aSTR_get_actor_area_proc` / free: a 9-slot pool.
#[derive(Clone, Copy, Debug, Default)]
pub struct StructurePool {
    pub used: [bool; STRUCTURE_SLOTS],
}

impl StructurePool {
    pub fn alloc(&mut self) -> Option<usize> {
        let i = self.used.iter().position(|u| !u)?;
        self.used[i] = true;
        Some(i)
    }
    pub fn free(&mut self, i: usize) {
        if i < STRUCTURE_SLOTS {
            self.used[i] = false;
        }
    }
}

/// `aSTR_setupActor_proc` profile choice for a structure FG id.
pub fn structure_profile(name: u16) -> Option<(i16, i16, i16)> {
    if name < NPC_HOUSE_END {
        return Some((PROFILE_HOUSE, -1, -1));
    }
    let idx = name.checked_sub(STRUCTURE_START)? as usize;
    STRUCTURE_SETUP.get(idx).copied()
}

/// Request type the structure manager posts to the door structure when
/// returning to the field (`aSTR_check_door_data`).
pub fn door_return_request(reset_exit: bool) -> i32 {
    if reset_exit {
        5
    } else {
        4
    }
}

/// Anchor helper used by tests.
pub fn statue_position(anchor: Xyz, house: usize) -> Xyz {
    let (dx, dz) = STATUE_OFFSETS[house & 3];
    Xyz::new(anchor.x + dx, anchor.y, anchor.z + dz)
}

/// C ABI: lighthouse beam night window (1/0).
#[no_mangle]
pub extern "C" fn pc_toudai_night(now_sec: i32) -> i32 {
    lighthouse_night(now_sec) as i32
}

/// C ABI: profile id for a structure FG id, or -1.
#[no_mangle]
pub extern "C" fn pc_str_setup_profile(name: u16) -> i32 {
    structure_profile(name).map_or(-1, |p| p.0 as i32)
}

/// `setupInfo_table` (index = structure FG id - STRUCTURE_START):
/// (profile id, aSTR_TYPE, aSTR_PAL). Generated by tools/gen_structure_table.py.
pub const STRUCTURE_SETUP: [(i16, i16, i16); 83] = [
    (43, 5, 25), // MYHOUSE / MYHOME / MYHOME_A
    (43, 5, 26), // MYHOUSE / MYHOME / MYHOME_B
    (43, 5, 27), // MYHOUSE / MYHOME / MYHOME_C
    (43, 5, 28), // MYHOUSE / MYHOME / MYHOME_D
    (42, 8, 37), // SHOP / SHOP1 / SHOP1
    (95, 9, 38), // CONVENI / SHOP2 / SHOP2
    (96, 10, 39), // SUPER / SHOP3 / SHOP3
    (97, 11, 40), // DEPART / SHOP4 / SHOP4
    (56, 12, 41), // POST_OFFICE / POST_OFFICE / POST_OFFICE
    (60, 13, 42), // STATION / STATION1 / STATION1_A
    (72, 16, 57), // TRAIN0 / TRAIN0 / TRAIN1_A1
    (73, 17, 58), // TRAIN1 / TRAIN1 / TRAIN1_A2
    (77, 18, 59), // POLICE_BOX / POLICE_BOX / POLICE_BOX
    (83, 19, 60), // FALLS / FALLS / FALLS
    (84, 20, 61), // FALLSESW / FALLSESW / FALLSESW
    (84, 20, 61), // FALLSESW / FALLSESW / FALLSESW
    (63, 21, 62), // RESERVE / RESERVE / RESERVE
    (63, 21, 62), // RESERVE / RESERVE / RESERVE
    (63, 21, 62), // RESERVE / RESERVE / RESERVE
    (63, 21, 62), // RESERVE / RESERVE / RESERVE
    (63, 21, 62), // RESERVE / RESERVE / RESERVE
    (63, 21, 62), // RESERVE / RESERVE / RESERVE
    (63, 21, 62), // RESERVE / RESERVE / RESERVE
    (63, 21, 62), // RESERVE / RESERVE / RESERVE
    (63, 21, 62), // RESERVE / RESERVE / RESERVE
    (63, 21, 62), // RESERVE / RESERVE / RESERVE
    (63, 21, 62), // RESERVE / RESERVE / RESERVE
    (63, 21, 62), // RESERVE / RESERVE / RESERVE
    (63, 21, 62), // RESERVE / RESERVE / RESERVE
    (63, 21, 62), // RESERVE / RESERVE / RESERVE
    (63, 21, 62), // RESERVE / RESERVE / RESERVE
    (63, 21, 62), // RESERVE / RESERVE / RESERVE
    (63, 21, 62), // RESERVE / RESERVE / RESERVE
    (63, 21, 62), // RESERVE / RESERVE / RESERVE
    (63, 21, 62), // RESERVE / RESERVE / RESERVE
    (63, 21, 62), // RESERVE / RESERVE / RESERVE
    (63, 21, 62), // RESERVE / RESERVE / RESERVE
    (92, 22, 63), // SHRINE / SHRINE / SHRINE
    (54, 23, 64), // BRSHOP / BRSHOP / BR_SHOP
    (93, 24, 65), // BUGGY / BUGGY / BUGGY
    (99, 25, 66), // S_CAR / S_CAR / S_CAR
    (106, 26, 67), // KAMAKURA / KAMAKURA / KAMAKURA
    (124, 27, 68), // GOZA / GOZA / GOZA
    (124, 27, 68), // GOZA / GOZA / GOZA
    (125, 28, 69), // RADIO / RADIO / RADIO
    (126, 29, 70), // YATAI / YATAI / YATAI
    (126, 29, 70), // YATAI / YATAI / YATAI
    (130, 30, 71), // TUKIMI / TUKIMI / TUKIMI
    (130, 31, 72), // TUKIMI / TUKIMI2 / TUKIMI2
    (134, 32, 73), // MIKUJI / MIKUJI / MIKUJI
    (148, 33, 74), // COUNT / COUNT / COUNT
    (154, 34, 75), // COUNT02 / COUNT02 / COUNT02
    (156, 35, 76), // TAMA / SPORTSFAIR_A / KAGO_R
    (156, 36, 77), // TAMA / SPORTSFAIR_B / KAGO_W
    (157, 35, 76), // KAGO / SPORTSFAIR_A / KAGO_R
    (157, 36, 77), // KAGO / SPORTSFAIR_B / KAGO_W
    (158, 37, 78), // TURI / TURI / TURI
    (158, 38, 79), // TURI / TURI2 / TURI2
    (163, 39, 80), // KOINOBORI / KOINOBORI / KOINOBORI_A
    (166, 40, 81), // DUMP / DUMP / DUMP
    (169, 41, 82), // WINDMILL / WINDMILL / WINDMILL_A
    (169, 41, 83), // WINDMILL / WINDMILL / WINDMILL_B
    (169, 41, 84), // WINDMILL / WINDMILL / WINDMILL_C
    (169, 41, 85), // WINDMILL / WINDMILL / WINDMILL_D
    (169, 41, 86), // WINDMILL / WINDMILL / WINDMILL_E
    (170, 42, 87), // LOTUS / LOTUS / 01_LOTUS
    (172, 43, 88), // MIKANBOX / MIKANBOX / MIKANBOX
    (173, 44, 89), // DOUZOU / DOUZOU / DOUZOU_DAI
    (175, 45, 90), // TOUDAI / TOUDAI / TOUDAI
    (240, 46, 91), // GHOG / GHOG / MUSEUM
    (241, 47, 91), // HTABLE / HTABLE / MUSEUM
    (241, 47, 91), // HTABLE / HTABLE / MUSEUM
    (241, 47, 91), // HTABLE / HTABLE / MUSEUM
    (243, 26, 67), // TENT / KAMAKURA / KAMAKURA
    (182, 48, 91), // MUSEUM / MUSEUM / MUSEUM
    (188, 49, 92), // BRIDGE_A / BRIDGE_A / BRIDGE_A
    (188, 49, 92), // BRIDGE_A / BRIDGE_A / BRIDGE_A
    (143, 51, 93), // NEEDLEWORK_SHOP / NEEDLEWORK_SHOP / TAILOR
    (197, 52, 94), // FLAG / FLAG / FLAG
    (200, 53, 95), // BOAT / BOAT / BOAT
    (202, 54, 96), // COTTAGE_MY / COTTAGE_MY / MYHOME_ISLAND
    (203, 55, 97), // COTTAGE_NPC / COTTAGE_NPC / HOUSE_I
    (63, 21, 62), // RESERVE / RESERVE / RESERVE
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shops() {
        assert_eq!(shop_check_player(&CONVENI, 5.0, 5.0, 0x6000), 2);
        assert_eq!(shop_check_player(&CONVENI, 20.0, 15.0, 0x6000), 1);
        assert_eq!(shop_closed_message(&CONVENI, false, 0, SHOP_OPEN), 1997);
        assert_eq!(shop_closed_message(&SUPER, false, 0, SHOP_OPEN), 1999);
        assert_eq!(shop_closed_message(&CONVENI, true, 7 * 3600, SHOP_PRE), MSG_RAFFLE_TIME_ANNOUNCE);
        assert_eq!(shop_closed_message(&CONVENI, true, 5 * 3600, SHOP_PRE), 1998);
    }

    #[test]
    fn lighthouse() {
        assert!(lighthouse_night(19 * 3600) && lighthouse_night(4 * 3600) && !lighthouse_night(12 * 3600));
        assert!(lighthouse_at_door(5.0, -30.0, 0));
        assert_eq!(switch_nice_status(20, true), SwitchNice::LighthousePeriod);
        assert_eq!(switch_nice_status(12, true), SwitchNice::Off);
        assert!(switch_reachable(167.27, 112.73, 0x6000u16 as i16));
        let mut c = BeamColor::default();
        beam_color_step(&mut c, true, 51, 1.0);
        assert_eq!((c.b, c.a), (220.0, 240.0));
    }

    #[test]
    fn pole_spin_up() {
        let mut p = Pole { running: true, ..Default::default() };
        pole_step(&mut p);
        assert!((p.speed - 0.065).abs() < 1e-6);
        for _ in 0..47 {
            pole_step(&mut p);
        }
        assert_eq!(p.timer, 48);
        for _ in 0..500 {
            pole_step(&mut p);
        }
        assert_eq!(p.speed, 0.5);
    }

    #[test]
    fn cottage_angle_portability() {
        assert!(my_cottage_can_enter(0.0, 0.0, -20000, true, true));
        assert!(!my_cottage_can_enter(0.0, 0.0, -20000, true, false));
        assert!(npc_cottage_light(false, true, true, 19 * 3600, false, false));
        assert!(!npc_cottage_light(false, false, true, 19 * 3600, false, false));
        assert!(npc_cottage_light(false, false, true, 19 * 3600, false, true));
    }

    #[test]
    fn misc() {
        assert!(!tent_despawns((0, 0), (5, 5)));
        assert!(tent_light(4 * 3600) && !tent_light(12 * 3600));
        assert_eq!(statue_eye_texture(1, 6, false), (14, true));
        assert_eq!(structure_profile(0x5800 + 76), Some(STRUCTURE_SETUP[76]));
        assert_eq!(structure_profile(0x1000).map(|t| t.0), Some(PROFILE_HOUSE));
        let mut pool = StructurePool::default();
        for i in 0..9 {
            assert_eq!(pool.alloc(), Some(i));
        }
        assert_eq!(pool.alloc(), None);
        assert_eq!(statue_position(Xyz::ZERO, 2), Xyz::new(-40.0, 0.0, 280.0));
    }
}
