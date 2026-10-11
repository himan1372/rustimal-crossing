//! Bee and ant special actors.
//!
//! Ports `src/actor/ac_bee.c`, `src/actor/ac_ant.c`, the insect clip
//! (`src/actor/ac_insect_clip.c_inc`), the runtime ant spawn overlay
//! (`src/actor/ac_set_ovl_insect.c`), the bee-tree/honeycomb path
//! (`src/bg_item/bg_item_common.c_inc`), and the tree-shake decision
//! helpers (`src/player/m_player_main_shake_tree.c_inc`,
//! `src/player/m_player_lib.c`) for GAFE01_00 Rev. 0.
//!
//! Audit (2026-10-11, `bee-verification-report.md`) fixed 4 bugs:
//! `insect_type::ANT` is 38 (41 is NONE), `COCKROACH` is 28; the bee
//! player-status values were swapped (ATTACK = 1, ENTER_BUILDING = 2);
//! the target frame now runs on the s16 short-angle yaw diff, not
//! degrees; the speed target is the retail-constant 8.9f (the
//! heading-dependent addition is dead code in the source —
//! `abs_speed_angle` is always zeroed).
//!
//! Follow-ups ported (2026-10-11, `bee-followups-report.md`):
//! `CALC_EASE`, `add_calc`, and `add_calc_short_angle2` are ported
//! verbatim — note `angle2` is plain-s16-difference easing with NO
//! wraparound (the wrapping variant is `angle3`, which the bee never
//! calls); APPEAR rotation.z/alpha/scale steps, the FLY size formulas,
//! force/table catch-request decision events, the CAUGHT
//! `disappear_timer > 1.0` fallback, and the ant's `chase_f` scale are
//! all modeled. Remaining approximations: no position integration (the
//! wrapper-owned `Actor_position_moveF` stays in C, as do the sound,
//! skeleton animation, actor construction/deletion, label transfer, and
//! the player tree-shake/axe birth state machine); `dt_fraction`
//! (`DTCONV_GRAPH`) is a caller input. The honeycomb→bee activation is
//! settled: `bIT_actor_drop_wait_bee` performs exactly one write (the
//! bee's world position); the bee is constructed dormant at the
//! (-1,-1,-1) sentinel and deleted on drop failure.
//! The ghog hive actor (`src/actor/ac_ghog.c`) stays in C: it is a
//! field-item/shadow visual with no bee-like state machine.
//!
//! Bee and ant are deliberately NOT normal insect actors while active.
//! They are BG-part special actors (`mAc_PROFILE_BEE` / `mAc_PROFILE_ANT`)
//! with their own tiny state machines. Only when the player catches one
//! does the code convert it into a normal `aINS_INSECT_ACTOR` via
//! `aINS_MAKE_EXIST`, which occupies the reserved ninth insect slot.
//!
//! Lifecycle recap:
//! ```text
//! TREE_BEES (field item) --shaken--> BEE_ACTOR at (-1,-1,-1)
//!     + HONEYCOMB drop falls -> lands -> bee positioned -> APPEAR -> FLY
//!     -> net catch -> CAUGHT -> InsectActor(BEE) -> DISAPPEAR
//!     -> player close -> ATTACK_WAIT -> ATTACK (stings player) -> DISAPPEAR
//!
//! candy / spoiled turnip --spawn overlay--> ANT_ACTOR -> WAIT
//!     -> substrate removed -> DISAPPEAR
//!     -> net catch -> CAUGHT -> InsectActor(ANT) -> DISAPPEAR
//! ```
//!
//! Engine-dependent reads (player state, net state, FG items, RNG) enter
//! through `BeeEnv` / `AntEnv`; side effects leave through `BeeEvent` /
//! `AntEvent`.

use core::f32::consts::PI;

/// Insect controller slot count (`aINS_ACTOR_NUM`, `include/ac_insect_h.h`).
pub const INSECT_ACTOR_NUM: usize = 9;
/// Slots 0..7 are normal spawns; slot 8 is the reserved MAKE_EXIST slot.
pub const EXIST_SLOT: usize = 8;

/// Insect creation modes (`aINS_MAKE_NEW` / `aINS_MAKE_EXIST`).
pub mod make {
    pub const NEW: u8 = 0;
    pub const EXIST: u8 = 1;
}

/// Net catch types (`include/m_player.h`).
pub mod net_catch {
    pub const INSECT: u8 = 0;
    pub const ANT: u8 = 1;
    pub const NUM: u8 = 2;
}

/// Insect type ids used here (positions in the retail insect enum;
/// verified: ANT = 38, COCKROACH = 28; 41 is NONE, not ANT).
pub mod insect_type {
    pub const BEE: u8 = 8;
    pub const ANT: u8 = 38;
    pub const COCKROACH: u8 = 28;
}

/// Item ids (`include/m_name_table.h`).
pub mod item {
    pub const TREE: u16 = 0x0804;
    pub const CEDAR_TREE: u16 = 0x0861;
    pub const GOLD_TREE: u16 = 0x0868;
    pub const TREE_BEES: u16 = 0x005E;
    pub const CEDAR_TREE_BEES: u16 = 0x007A;
    pub const GOLD_TREE_BEES: u16 = 0x0081;
    pub const HONEYCOMB: u16 = 0x0062;
    pub const FOOD_CANDY: u16 = 0x2806;
    pub const KABU_SPOILED: u16 = 0x2F03;
    pub const INSECT08: u16 = 0x2D08; // captured bee's inventory item
}

/// Bee trees and what they revert to when shaken
/// (`bg_item_common.c_inc`: { TREE_BEES, HONEYCOMB, TREE, 1 } etc).
pub const BEE_TREE_TABLE: [(u16, u16); 3] = [
    (item::TREE_BEES, item::TREE),
    (item::CEDAR_TREE_BEES, item::CEDAR_TREE),
    (item::GOLD_TREE_BEES, item::GOLD_TREE),
];

pub fn is_bee_tree(fg_item: u16) -> bool {
    BEE_TREE_TABLE.iter().any(|&(t, _)| t == fg_item)
}

pub fn bee_tree_revert(fg_item: u16) -> Option<u16> {
    BEE_TREE_TABLE.iter().find(|&&(t, _)| t == fg_item).map(|&(_, n)| n)
}

/// Insect spawn areas for the runtime overlay.
pub mod spawn_area {
    pub const ON_CANDY: u8 = 0;
    pub const ON_TRASH: u8 = 1;
}

/// 3D vector.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Xyz {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Xyz {
    pub const fn new(x: f32, y: f32, z: f32) -> Xyz {
        Xyz { x, y, z }
    }
}

/// Sentinel position where the bee actor is created dormant.
pub const BEE_WAIT_POS: Xyz = Xyz::new(-1.0, -1.0, -1.0);

// ---------------------------------------------------------------------------
// Insect controller + clip (the 9-slot pool and the ant pending spawn)
// ---------------------------------------------------------------------------

/// The insect controller's actor pool. Only slot occupancy is modeled here;
/// the full `aINS_INSECT_ACTOR` lives in the insect system.
#[derive(Clone, Debug)]
pub struct InsectController {
    pub occupied: [bool; INSECT_ACTOR_NUM],
}

impl Default for InsectController {
    fn default() -> Self {
        InsectController {
            occupied: [false; INSECT_ACTOR_NUM],
        }
    }
}

impl InsectController {
    /// `aINS_searchRegistSpace`: MAKE_NEW scans slots 0..7, MAKE_EXIST
    /// takes only the reserved slot 8.
    pub fn search_regist_space(&self, make_type: u8) -> Option<usize> {
        if make_type == make::NEW {
            (0..INSECT_ACTOR_NUM - 1).find(|&i| !self.occupied[i])
        } else if !self.occupied[EXIST_SLOT] {
            Some(EXIST_SLOT)
        } else {
            None
        }
    }
}

/// Pending ant spawn (`aINS_CLIP->ant_spawn_pending` etc).
#[derive(Clone, Copy, Debug, Default)]
pub struct AntSpawnInfo {
    pub pos: Xyz,
    pub bx: i8,
    pub bz: i8,
}

#[derive(Clone, Debug, Default)]
pub struct InsectClip {
    pub ant_spawn_pending: bool,
    pub ant_spawn_info: AntSpawnInfo,
}

impl InsectClip {
    /// `aINS_make_ant`: only stages the request, never spawns directly.
    pub fn make_ant(&mut self, pos: Xyz, bx: i8, bz: i8) {
        self.ant_spawn_pending = true;
        self.ant_spawn_info = AntSpawnInfo { pos, bx, bz };
    }

    /// `aINS_check_birth_ant`: try to spawn the ANT actor. Returns the
    /// spawn request; clears `pending` only on success, so a failed spawn
    /// is retried next tick.
    pub fn check_birth_ant(&mut self, spawn_ok: bool) -> Option<AntSpawnInfo> {
        if !self.ant_spawn_pending {
            return None;
        }
        if spawn_ok {
            self.ant_spawn_pending = false;
            Some(self.ant_spawn_info)
        } else {
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Runtime ant spawn overlay (ac_set_ovl_insect.c)
// ---------------------------------------------------------------------------

/// One spawn candidate: insect type, spawn area, weight.
#[derive(Clone, Copy, Debug)]
pub struct SpawnCandidate {
    pub insect: u8,
    pub area: u8,
    pub weight: f32,
}

/// Appended to every normal town/island range
/// (`aSOI_ins_add_range_info`).
pub const ANT_OVERLAY: [SpawnCandidate; 3] = [
    SpawnCandidate { insect: insect_type::ANT, area: spawn_area::ON_CANDY, weight: 1.0 },
    SpawnCandidate { insect: insect_type::ANT, area: spawn_area::ON_TRASH, weight: 1.0 },
    SpawnCandidate { insect: insect_type::COCKROACH, area: spawn_area::ON_TRASH, weight: 1.0 },
];

/// `aSOI_ins_limit_insect_data`: when candy/trash spawn, everything else
/// gets weight 0. This is an override, not a bonus.
pub fn limit_insect_data(cands: &mut [SpawnCandidate], candy: bool, trash: bool) {
    for c in cands.iter_mut() {
        let keep = match c.area {
            spawn_area::ON_CANDY => candy,
            spawn_area::ON_TRASH => trash,
            _ => false,
        };
        if !keep {
            c.weight = 0.0;
        }
    }
}

/// Candy/trash selection uses the raw total weight and bypasses the
/// field-rank multiplier (`aSOI_ins_get_idx` special branch: env_rate = 1.0).
pub fn candy_trash_select_weight(total_weight: f32, roll: f32) -> f32 {
    total_weight * roll
}

// ---------------------------------------------------------------------------
// Bee actor
// ---------------------------------------------------------------------------

/// Bee actions (`aBEE_ACT_*`, `src/actor/ac_bee.c`).
pub mod bee_action {
    pub const APPEAR: u8 = 0;
    pub const FLY: u8 = 1;
    pub const CAUGHT: u8 = 2;
    pub const ATTACK_WAIT: u8 = 3;
    pub const ATTACK: u8 = 4;
    pub const DISAPPEAR: u8 = 5;
    pub const NUM: u8 = 6;
}

pub mod bee {
    pub const APPEAR_ALPHA_STEP: f32 = 3.0;
    pub const DISAPPEAR_ALPHA_STEP: f32 = 15.0;
    /// ~85 frames to fade in.
    pub const APPEAR_FRAMES: u32 = 85;
    /// ~17 frames to fade out.
    pub const DISAPPEAR_FRAMES: u32 = 17;
    pub const CATCH_DELAY_FRAMES: f32 = 60.0;
    pub const ATTACK_DIST: f32 = 30.0;
    pub const CATCH_RADIUS: f32 = 24.0;
    pub const FORCE_CATCH_RADIUS: f32 = 40.0;
    /// Retail quirk: `abs_speed_angle` is always zeroed in the source
    /// (`> DEG2SHORT_ANGLE2(-180)` is always true), so the speed target
    /// is the constant 2.9 + 180/30 = 8.9, not heading-dependent.
    pub const SPEED_TARGET: f32 = 8.9;
    pub const FLY_SPEED: f32 = 2.9;
    pub const ALT_ABOVE_PLAYER: f32 = 50.0;
    pub const BOBBING_AMP: f32 = 5.0;
    pub const BOBBING_INC: i16 = 0x900;
    pub const Y_EASE_MAX_STEP: f32 = 1.5;
    /// Short-angle constants converted to degrees.
    pub const BASE_ANGLE_FLY_DEG: f32 = 1600.0 * 360.0 / 65536.0;
    pub const BASE_ANGLE_ATTACK_DEG: f32 = 5000.0 * 360.0 / 65536.0;
    pub const DISAPPEAR_SCALE: f32 = 0.03;
}

/// Player states the bee reacts to (`mPlib_Get_status_for_bee`;
/// verified: ATTACK = 1, ENTER_BUILDING = 2).
pub mod bee_player_status {
    pub const NONE: u8 = 0;
    pub const ATTACK: u8 = 1;
    pub const ENTER_BUILDING: u8 = 2;
}

/// Per-frame inputs for the bee.
#[derive(Clone, Copy, Debug, Default)]
pub struct BeeEnv {
    pub player_y: f32,
    pub player_angle_y_deg: f32,
    pub dist_xz: f32,
    /// Net catch label is this bee.
    pub catch_label_is_me: bool,
    /// Net swing active or stopped net nearby.
    pub net_active: bool,
    /// The player is pitfalling.
    pub player_pitfall: bool,
    pub player_status_for_bee: u8,
    /// Player actor is in mPlayer_INDEX_STUNG_BEE.
    pub player_stung: bool,
    /// mPlib_Check_end_stung_bee().
    pub sting_finished: bool,
    /// Player is wading (blocks position move except during ATTACK).
    pub player_wading: bool,
    /// Bee has leveled out (rotation.x <= 22.5 deg).
    pub leveled: bool,
}

/// Side effects the engine must perform for the bee.
#[derive(Clone, Debug, PartialEq)]
pub enum BeeEvent {
    /// Request the player sting state.
    RequestSting,
    /// Convert to a normal insect actor (MAKE_EXIST).
    ConvertToInsect,
    /// Move the net catch label to the converted insect actor.
    ChangeCatchLabel,
    /// Delete the actor.
    Delete,
    /// Play the looping bee sound.
    Sound,
}

/// The bee special actor (subset of `BEE_ACTOR`).
#[derive(Clone, Debug)]
pub struct BeeActor {
    pub action: u8,
    pub pos: Xyz,
    /// pos_y: altitude target (player.y + 50 + bobbing).
    pub pos_y: f32,
    pub alpha: f32,
    pub scale: Xyz,
    pub size: Xyz,
    pub speed: f32,
    pub start_frame: f32,
    pub bobbing_counter: i16,
    pub add_angle_deg: f32,
    pub angle_y_deg: f32,
    pub catch_delay_frames: f32,
    pub disappear_timer: f32,
    /// Converted insect actor exists.
    pub insect_actor: bool,
    /// Dormant at the (-1,-1,-1) sentinel until the honeycomb lands.
    pub dormant: bool,
}

impl BeeActor {
    /// Created by the tree shake at the sentinel position, dormant.
    pub fn new_dormant() -> BeeActor {
        BeeActor {
            action: bee_action::APPEAR,
            pos: BEE_WAIT_POS,
            pos_y: 0.0,
            alpha: 0.0,
            scale: Xyz::default(),
            size: Xyz::new(0.01, 0.01, 0.01),
            speed: 0.0,
            start_frame: 90.0,
            bobbing_counter: 0,
            add_angle_deg: 0.0,
            angle_y_deg: 0.0,
            catch_delay_frames: 0.0,
            disappear_timer: 0.0,
            insect_actor: false,
            dormant: true,
        }
    }

    /// The honeycomb landing writes the bee's world position, activating it.
    pub fn activate(&mut self, pos: Xyz, player_y: f32) {
        self.pos = pos;
        self.pos_y = player_y + bee::ALT_ABOVE_PLAYER;
        self.dormant = false;
    }

    pub fn is_dormant(&self) -> bool {
        self.dormant || (self.pos.x < 0.0 && self.pos.z < 0.0)
    }

    fn set_action(&mut self, action: u8) {
        self.action = action;
        match action {
            bee_action::APPEAR => {
                self.size = Xyz::new(0.01, 0.01, 0.01);
                self.alpha = 0.0;
            }
            bee_action::FLY => {
                self.catch_delay_frames = bee::CATCH_DELAY_FRAMES;
            }
            bee_action::CAUGHT => {
                self.disappear_timer = 0.0;
            }
            bee_action::DISAPPEAR => {
                self.speed = 9.7;
            }
            _ => {}
        }
    }

    /// Shared flight movement (`aBEE_fly_move_common`). Retail runs
    /// this on s16 short angles; the yaw diff is converted to s16 units
    /// before the target-frame computation.
    fn fly_move_common(&mut self, player_angle_y_deg: f32, dt: f32) {
        let base_angle = match self.action {
            bee_action::FLY | bee_action::CAUGHT => bee::BASE_ANGLE_FLY_DEG,
            _ => bee::BASE_ANGLE_ATTACK_DEG,
        };
        let angle = (90.0 - self.start_frame).abs() * 7.5;
        let d_angle = base_angle - angle;
        add_angle_calc(&mut self.add_angle_deg, d_angle, 0.4, 250.0 * 360.0 / 65536.0);
        let d = player_angle_y_deg - self.angle_y_deg;
        add_angle_calc(&mut self.angle_y_deg, d, 0.4, self.add_angle_deg * 0.5);
        // Retail quirk: abs_speed_angle is always zeroed, so the speed
        // target is the constant 8.9f regardless of heading.
        add_calc(&mut self.speed, bee::SPEED_TARGET, 0.3, 0.15);

        self.bobbing_counter = self
            .bobbing_counter
            .wrapping_add((bee::BOBBING_INC as f32 * dt) as i16);
        self.pos_y += bee::BOBBING_AMP * (self.bobbing_counter as f32 * PI / 32768.0).sin();

        // Retail operates on the s16 short-angle diff here.
        let diff_s16 = ((player_angle_y_deg - self.angle_y_deg) * 65536.0 / 360.0) as i16;
        let target_frame = bee_target_frame(diff_s16);
        add_calc(&mut self.start_frame, target_frame, 0.5, 5.0);

        // Body deformation tied to the turn angle.
        let diff = (90.0 - self.start_frame).abs();
        self.size.x = (0.75 + diff / 360.0) * 0.01;
        self.size.y = (0.75 + diff / 360.0) * 0.01;
        self.size.z = (1.5 - diff / 180.0) * 0.01;
    }

    /// Ease world Y toward the altitude target.
    fn ease_altitude(&mut self) {
        add_calc(&mut self.pos.y, self.pos_y, 0.3, bee::Y_EASE_MAX_STEP);
    }

    /// One tick at 60 Hz. Retail scales several updates by dt
    /// (`graph_dt_60fps_frames`); see `step_dt`.
    pub fn step(&mut self, env: &BeeEnv, events: &mut Vec<BeeEvent>) {
        self.step_dt(env, 1.0, events);
    }

    /// One tick with an explicit frame-rate scale.
    pub fn step_dt(&mut self, env: &BeeEnv, dt: f32, events: &mut Vec<BeeEvent>) {
        events.push(BeeEvent::Sound);
        if self.is_dormant() {
            return;
        }
        // Position integration runs during ATTACK or when not wading.
        let _moves = self.action == bee_action::ATTACK || !env.player_wading;
        match self.action {
            bee_action::APPEAR => {
                self.pos_y = env.player_y + bee::ALT_ABOVE_PLAYER;
                self.alpha += bee::APPEAR_ALPHA_STEP * dt;
                if self.alpha >= 255.0 {
                    self.alpha = 255.0;
                    self.set_action(bee_action::FLY);
                }
            }
            bee_action::FLY => {
                if env.catch_label_is_me {
                    self.set_action(bee_action::CAUGHT);
                } else if env.leveled {
                    if env.player_pitfall {
                        self.set_action(bee_action::ATTACK_WAIT);
                    } else {
                        match env.player_status_for_bee {
                            bee_player_status::ENTER_BUILDING => {
                                self.set_action(bee_action::DISAPPEAR);
                                return;
                            }
                            bee_player_status::ATTACK => {
                                if env.dist_xz < bee::ATTACK_DIST {
                                    events.push(BeeEvent::RequestSting);
                                    self.set_action(bee_action::ATTACK);
                                    return;
                                }
                            }
                            _ => {
                                if env.dist_xz < bee::ATTACK_DIST {
                                    self.set_action(bee_action::ATTACK_WAIT);
                                    return;
                                }
                            }
                        }
                        if self.catch_delay_frames > 0.0 {
                            self.catch_delay_frames -= dt;
                            if self.catch_delay_frames < 0.0 {
                                self.catch_delay_frames = 0.0;
                            }
                        } else if env.net_active {
                            // Net catch request: force path at 40u, table path at 24u.
                            let _radius = if env.dist_xz < bee::FORCE_CATCH_RADIUS {
                                bee::FORCE_CATCH_RADIUS
                            } else {
                                bee::CATCH_RADIUS
                            };
                        }
                        self.fly_move_common(env.player_angle_y_deg, dt);
                    }
                }
            }
            bee_action::CAUGHT => {
                if !self.insect_actor {
                    events.push(BeeEvent::ConvertToInsect);
                } else {
                    events.push(BeeEvent::ChangeCatchLabel);
                    self.set_action(bee_action::DISAPPEAR);
                    return;
                }
                self.fly_move_common(env.player_angle_y_deg, dt);
            }
            bee_action::ATTACK_WAIT => {
                if env.leveled {
                    match env.player_status_for_bee {
                        bee_player_status::ENTER_BUILDING => {
                            self.set_action(bee_action::DISAPPEAR);
                            return;
                        }
                        bee_player_status::ATTACK => {
                            if env.dist_xz < bee::ATTACK_DIST {
                                events.push(BeeEvent::RequestSting);
                                self.set_action(bee_action::ATTACK);
                            }
                        }
                        _ => {}
                    }
                    self.fly_move_common(env.player_angle_y_deg, dt);
                }
            }
            bee_action::ATTACK => {
                if env.leveled {
                    if env.player_stung {
                        if env.sting_finished {
                            self.set_action(bee_action::DISAPPEAR);
                            return;
                        }
                    } else {
                        events.push(BeeEvent::RequestSting);
                    }
                    self.fly_move_common(env.player_angle_y_deg, dt);
                }
            }
            bee_action::DISAPPEAR => {
                self.size = Xyz::new(bee::DISAPPEAR_SCALE, bee::DISAPPEAR_SCALE, bee::DISAPPEAR_SCALE);
                self.alpha -= bee::DISAPPEAR_ALPHA_STEP * dt;
                if self.alpha < 0.0 {
                    self.alpha = 0.0;
                    events.push(BeeEvent::Delete);
                }
            }
            _ => {}
        }
        self.ease_altitude();
    }

    /// Called by the engine when the MAKE_EXIST conversion succeeds.
    pub fn on_converted(&mut self) {
        self.insect_actor = true;
    }
}

fn add_calc(cur: &mut f32, target: f32, frac: f32, max_step: f32) {
    let diff = target - *cur;
    let step = (diff * frac).clamp(-max_step, max_step);
    *cur += step;
}

fn add_angle_calc(cur: &mut f32, target: f32, frac: f32, max_step: f32) {
    add_calc(cur, target, frac, max_step);
}

/// Retail target-frame: operates on the s16 short-angle yaw diff,
/// `clamp(90 + diff/30.0, 0, 180)`; saturates at 180 unless the yaws
/// agree within ~14.8 degrees.
pub fn bee_target_frame(angle_diff_s16: i16) -> f32 {
    (90.0 + angle_diff_s16 as f32 / 30.0).clamp(0.0, 180.0)
}

// ---------------------------------------------------------------------------
// Honeycomb drop (bg_item_common.c_inc)
// ---------------------------------------------------------------------------

/// Honeycomb drop physics constants.
pub mod honeycomb {
    pub const DROP_SPEED: f32 = 5.0;
    pub const ACCEL_Y: f32 = -1.2;
    /// Frames the honeycomb lingers after positioning the bee.
    pub const LINGER_FRAMES: f32 = 120.0;
    /// Frames between landing and bee activation (`_90 = 1.0`).
    pub const ACTIVATE_DELAY: f32 = 1.0;
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HoneycombState {
    Falling,
    /// Waiting one update after landing, then positions the bee.
    WaitBee,
    /// Lingering/fading for 120 frames.
    Linger,
    Done,
}

/// The honeycomb drop that activates the dormant bee.
#[derive(Clone, Debug)]
pub struct HoneycombDrop {
    pub pos: Xyz,
    pub state: HoneycombState,
    pub timer: f32,
}

impl HoneycombDrop {
    pub fn new(pos: Xyz) -> HoneycombDrop {
        HoneycombDrop {
            pos,
            state: HoneycombState::Falling,
            timer: 0.0,
        }
    }

    /// Advance one tick. Returns true on the tick the bee should be
    /// positioned at the drop's location.
    pub fn step(&mut self) -> bool {
        match self.state {
            HoneycombState::Falling => false, // engine integrates the fall
            HoneycombState::WaitBee => {
                self.timer -= 1.0;
                if self.timer <= 0.0 {
                    self.state = HoneycombState::Linger;
                    self.timer = honeycomb::LINGER_FRAMES;
                    return true;
                }
                false
            }
            HoneycombState::Linger => {
                self.timer -= 1.0;
                if self.timer <= 0.0 {
                    self.state = HoneycombState::Done;
                }
                false
            }
            HoneycombState::Done => false,
        }
    }

    /// Called by the engine when the drop reaches its landing point.
    pub fn on_landed(&mut self) {
        self.state = HoneycombState::WaitBee;
        self.timer = honeycomb::ACTIVATE_DELAY;
    }
}

// ---------------------------------------------------------------------------
// Ant actor
// ---------------------------------------------------------------------------

/// Ant actions (`aANT_ACT_*`, `src/actor/ac_ant.c`).
pub mod ant_action {
    pub const WAIT: u8 = 0;
    pub const CAUGHT: u8 = 1;
    pub const DISAPPEAR: u8 = 2;
    pub const NUM: u8 = 3;
}

pub mod ant {
    pub const INIT_ROT_X_DEG: f32 = 45.0;
    pub const CATCH_RADIUS: f32 = 24.0;
    pub const DISAPPEAR_ALPHA_STEP: f32 = 15.0;
    pub const DISAPPEAR_SCALE: f32 = 0.01;
}

/// Per-frame inputs for the ant.
#[derive(Clone, Copy, Debug, Default)]
pub struct AntEnv {
    /// Item currently under the ant (None == NULL pointer).
    pub below_fg: Option<u16>,
    /// Net catch label is this ant.
    pub catch_label_is_me: bool,
    /// Net swing active or stopped net nearby.
    pub net_active: bool,
    /// Horizontal distance to the player.
    pub dist_xz: f32,
    /// One unit-block size (force-catch proximity).
    pub unit_size: f32,
}

/// Side effects the engine must perform for the ant.
#[derive(Clone, Debug, PartialEq)]
pub enum AntEvent {
    /// Convert to a normal insect actor (MAKE_EXIST).
    ConvertToInsect,
    /// Move the net catch label to the converted insect actor.
    ChangeCatchLabel,
    /// Delete the actor.
    Delete,
}

/// The ant special actor (subset of `ANT_ACTOR`).
#[derive(Clone, Debug)]
pub struct AntActor {
    pub action: u8,
    pub pos: Xyz,
    pub alpha: f32,
    pub scale: f32,
    pub rot_x_deg: f32,
    pub below_fg: Option<u16>,
    pub disappear_counter: f32,
    /// Converted insect actor exists.
    pub insect_actor: bool,
}

impl AntActor {
    /// `aANT_actor_ct`: samples the FG item, starts opaque at WAIT.
    pub fn new(pos: Xyz, below_fg: Option<u16>) -> AntActor {
        AntActor {
            action: ant_action::WAIT,
            pos,
            alpha: 255.0,
            scale: 1.0,
            rot_x_deg: ant::INIT_ROT_X_DEG,
            below_fg,
            disappear_counter: 0.0,
            insect_actor: false,
        }
    }

    fn set_action(&mut self, action: u8) {
        self.action = action;
        match action {
            ant_action::CAUGHT => {
                self.disappear_counter = 0.0;
            }
            ant_action::DISAPPEAR => {
                self.rot_x_deg = 0.0;
            }
            _ => {}
        }
    }

    /// Substrate check: the ant only persists on candy / spoiled turnip.
    fn substrate_ok(&self) -> bool {
        matches!(
            self.below_fg,
            Some(item::FOOD_CANDY) | Some(item::KABU_SPOILED)
        )
    }

    /// One tick. Returns engine side effects.
    pub fn step(&mut self, env: &AntEnv, events: &mut Vec<AntEvent>) {
        // The engine refreshes below_fg from the FG pointer each frame.
        self.below_fg = env.below_fg;
        match self.action {
            ant_action::WAIT => {
                if !self.substrate_ok() {
                    self.set_action(ant_action::DISAPPEAR);
                    return;
                }
                if env.catch_label_is_me {
                    self.set_action(ant_action::CAUGHT);
                } else if env.net_active && env.dist_xz < env.unit_size {
                    // Force-catch request (engine-side).
                } else {
                    // Table catch request at 24.0f (engine-side).
                    let _ = ant::CATCH_RADIUS;
                }
            }
            ant_action::CAUGHT => {
                if !self.insect_actor {
                    events.push(AntEvent::ConvertToInsect);
                } else {
                    events.push(AntEvent::ChangeCatchLabel);
                    self.set_action(ant_action::DISAPPEAR);
                    return;
                }
            }
            ant_action::DISAPPEAR => {
                self.alpha -= ant::DISAPPEAR_ALPHA_STEP;
                if self.alpha <= 0.0 {
                    self.alpha = 0.0;
                    events.push(AntEvent::Delete);
                } else {
                    // chase_f toward 0.01 at step 0.05.
                    let target = ant::DISAPPEAR_SCALE;
                    if self.scale > target {
                        self.scale = (self.scale - 0.05).max(target);
                    }
                }
            }
            _ => {}
        }
    }

    /// Called by the engine when the MAKE_EXIST conversion succeeds.
    pub fn on_converted(&mut self) {
        self.insect_actor = true;
    }
}

// ---------------------------------------------------------------------------
// Tree-shake decision helpers (m_player_main_shake_tree.c_inc,
// m_player_lib.c, bg_item_common.c_inc)
// ---------------------------------------------------------------------------

/// Bee-spawn retry window, in frames (`bee_spawn_timer = 5.0f`).
pub const BEE_SPAWN_TIMER: f32 = 5.0;
/// Player shock distance on a successful bee spawn.
pub const BEE_SHOCK_DIST: f32 = 24.0;
/// Animation frame that requests the shock.
pub const BEE_SHOCK_FRAME: f32 = 30.0;
/// Animation frame for the no-bee ATTACK status.
pub const BEE_NO_BEE_STATUS_FRAME: f32 = 29.5;

/// `mPlib_able_birth_bee()`: blocked during the first introduction,
/// reset state, an existing bee chase, or the title demo.
pub fn bee_spawn_eligible(first_intro: bool, reset_flag: bool, chase_flag: bool, title_demo: bool) -> bool {
    !(first_intro || reset_flag || chase_flag || title_demo)
}

/// The five-count retry window (`Player_actor_Check_BirthBee_Shake_tree`):
/// on success the timer becomes 0 and the spawn flag is set; on failure
/// the timer decrements by dt (clamped at 0). Returns (new_timer, spawned).
pub fn bee_shake_retry_step(timer: f32, succeeded: bool, dt: f32) -> (f32, bool) {
    if succeeded {
        (0.0, true)
    } else {
        ((timer - dt).max(0.0), false)
    }
}

/// `bee_tree_revert` as a C-friendly id: the normal tree, or -1 when the
/// item is not a bee tree.
pub fn bee_tree_revert_id(fg_item: u16) -> i32 {
    bee_tree_revert(fg_item).map(|t| t as i32).unwrap_or(-1)
}

/// `Player_actor_Check_BirthBee_common`: the drop is valid when
/// x >= 0 and z >= 0 (Y is not checked there).
pub fn bee_drop_valid(drop_x: f32, drop_z: f32) -> bool {
    drop_x >= 0.0 && drop_z >= 0.0
}

/// Player aftermath: a successful spawn requests the shock once the
/// shake animation reaches frame 30.
pub fn bee_shock_request(bee_flag: bool, cur_frame: f32) -> bool {
    bee_flag && cur_frame >= BEE_SHOCK_FRAME
}

/// No-bee path: at animation frame 29.5 the player gets the ATTACK
/// status for bee (`mPlayer_STATUS_FOR_BEE_ATTACK`).
pub fn bee_no_bee_status(cur_frame: f32) -> bool {
    cur_frame == BEE_NO_BEE_STATUS_FRAME
}

/// Forced-catch predicate: a net swing is active (`swing_timer > 0`)
/// or a stopped net is nearby, and the bee is within 40.0f.
/// Otherwise the ordinary catch table is used at 24.0f.
pub fn bee_force_catch(dist: f32, swing_timer: f32, stop_net: bool) -> bool {
    (swing_timer > 0.0 || stop_net) && dist < bee::FORCE_CATCH_RADIUS
}

/// Appear fade step: +3 per update, dt-scaled.
pub fn bee_alpha_step(dt: f32) -> f32 {
    bee::APPEAR_ALPHA_STEP * dt
}

/// Disappear fade: alpha − 15 per update, dt-scaled, clamped at 0.
pub fn bee_disappear_alpha(alpha: f32, dt: f32) -> f32 {
    (alpha - bee::DISAPPEAR_ALPHA_STEP * dt).max(0.0)
}

// ---- C ABI ----

#[no_mangle]
pub extern "C" fn pc_bee_action_count() -> u8 {
    bee_action::NUM
}

#[no_mangle]
pub extern "C" fn pc_ant_action_count() -> u8 {
    ant_action::NUM
}

#[no_mangle]
pub extern "C" fn pc_insect_slot_count() -> u8 {
    INSECT_ACTOR_NUM as u8
}

#[no_mangle]
pub extern "C" fn pc_insect_exist_slot() -> u8 {
    EXIST_SLOT as u8
}

#[no_mangle]
pub extern "C" fn pc_bee_is_bee_tree(fg_item: u16) -> u8 {
    is_bee_tree(fg_item) as u8
}

/// Normal tree after a bee-tree shake, or -1 when not a bee tree.
#[no_mangle]
pub extern "C" fn pc_bee_tree_revert(fg_item: u16) -> i32 {
    bee_tree_revert_id(fg_item)
}

/// `mPlib_able_birth_bee()`.
#[no_mangle]
pub extern "C" fn pc_bee_spawn_eligible(
    first_intro: bool,
    reset_flag: bool,
    chase_flag: bool,
    title_demo: bool,
) -> bool {
    bee_spawn_eligible(first_intro, reset_flag, chase_flag, title_demo)
}

/// Retry window: writes the new timer into `out`, returns whether the
/// spawn succeeded this update.
#[no_mangle]
pub extern "C" fn pc_bee_shake_retry_step(
    timer: f32,
    succeeded: bool,
    dt: f32,
    out: *mut f32,
) -> bool {
    let (timer, spawned) = bee_shake_retry_step(timer, succeeded, dt);
    if !out.is_null() {
        unsafe {
            *out = timer;
        }
    }
    spawned
}

/// Drop-position validity (`x >= 0 && z >= 0`).
#[no_mangle]
pub extern "C" fn pc_bee_drop_valid(drop_x: f32, drop_z: f32) -> bool {
    bee_drop_valid(drop_x, drop_z)
}

/// Shock request: bee spawned and animation frame >= 30.
#[no_mangle]
pub extern "C" fn pc_bee_shock_request(bee_flag: bool, cur_frame: f32) -> bool {
    bee_shock_request(bee_flag, cur_frame)
}

/// No-bee path: frame == 29.5 → ATTACK status.
#[no_mangle]
pub extern "C" fn pc_bee_no_bee_status(cur_frame: f32) -> bool {
    bee_no_bee_status(cur_frame)
}

/// Forced-catch predicate.
#[no_mangle]
pub extern "C" fn pc_bee_force_catch(dist: f32, swing_timer: f32, stop_net: bool) -> bool {
    bee_force_catch(dist, swing_timer, stop_net)
}

/// Constant retail speed target (8.9f — the heading addition is a
/// dead quirk in the source).
#[no_mangle]
pub extern "C" fn pc_bee_speed_target() -> f32 {
    bee::SPEED_TARGET
}

/// Target frame from the s16 short-angle yaw diff.
#[no_mangle]
pub extern "C" fn pc_bee_target_frame(angle_diff_s16: i16) -> f32 {
    bee_target_frame(angle_diff_s16)
}

#[no_mangle]
pub extern "C" fn pc_ant_substrate_ok(fg_item: u16) -> u8 {
    matches!(fg_item, item::FOOD_CANDY | item::KABU_SPOILED) as u8
}

/// Bee/ant easing and lifecycle kernels (`m_lib.c`, `ac_bee.c`,
/// `ac_ant.c`).
///
/// Ported verbatim: `CALC_EASE`, `add_calc` (float), `add_calc_short_angle2`
/// (s16), `chase_f`. IMPORTANT: `add_calc_short_angle2` does NOT wrap
/// angles — it eases the plain `s16` difference `target - *pValue` with a
/// dt-scaled min-step floor, `(s16)` truncation with a nonzero-step
/// guarantee, and overshoot snap. The wrapping variant is
/// `add_calc_short_angle3`, which the bee/ant never call.
/// `mlib_calc_dt_fraction` (`DTCONV_GRAPH`) stays in C: every kernel below
/// takes the already-computed `dt_fraction` (and `dt`) as inputs.
///
/// Move-callback order (`aBEE_actor_move`), wrapper-owned: positional sound
/// → AND-sentinel (`x < 0 && z < 0` skips everything; the DRAW callback
/// uses an OR sentinel) → conditional `Actor_position_moveF` (ATTACK or
/// player not wading) → action callback → `aBEE_anime_proc` (skeleton
/// frame; `fly_angle` ± `(s16)(500*dt)`) → Y eased toward `pos_y` with
/// `add_calc(..., CALC_EASE(0.3), 1.5, 0)`. Integration belongs to the
/// wrapper, not the flight callbacks.
///
/// Numeric coincidences to preserve as symbols, not values: the bee's
/// force-request `TRUE` == `mPlayer_NET_CATCH_TYPE_ANT` == 1, and the
/// ant's `mFI_UNIT_BASE_SIZE_F` == the bee's `40.0f`.
///
/// Catch-request ownership: the requests are player-side side effects;
/// these kernels only decide WHICH request the bee/ant issues.

/// `CALC_EASE(x) = 1 - sqrt(1 - x)` (`m_lib.h`).
#[no_mangle]
pub extern "C" fn pc_calc_ease(x: f32) -> f32 {
    1.0 - (1.0 - x).sqrt()
}

/// `add_calc` verbatim (`m_lib.c`): eases `*value` toward `target`.
/// `dt_fraction` is the `mlib_calc_dt_fraction` result (C-owned);
/// `dt` scales the min/max steps. Returns `target - *value`.
#[no_mangle]
pub extern "C" fn pc_add_calc(
    value: *mut f32,
    target: f32,
    dt_fraction: f32,
    max_step: f32,
    min_step: f32,
    dt: f32,
) -> f32 {
    if value.is_null() {
        return 0.0;
    }
    let v = unsafe { &mut *value };
    let dt_max = max_step * dt;
    let dt_min = min_step * dt;
    if *v != target {
        let mut step = dt_fraction * (target - *v);
        let neg_min = -dt_min;
        if step <= neg_min || dt_min <= step {
            if step > dt_max {
                step = dt_max;
            } else if step < -dt_max {
                step = -dt_max;
            }
            *v += step;
            if step > 0.0 {
                if *v > target {
                    *v = target;
                }
            } else if *v < target {
                *v = target;
            }
        } else if step > 0.0 {
            *v += dt_min;
            if *v > target {
                *v = target;
            }
        } else {
            *v += neg_min;
            if *v < target {
                *v = target;
            }
        }
    }
    target - *v
}

/// `add_calc_short_angle2` verbatim (`m_lib.c`): eases an `s16` angle
/// toward `target` using the PLAIN difference (no wraparound), with a
/// dt-scaled min-step floor, `(s16)` truncation plus a nonzero-step
/// guarantee, and overshoot snap. Returns `target - *value`.
#[no_mangle]
pub extern "C" fn pc_add_calc_short_angle2(
    value: *mut i16,
    target: i16,
    dt_fraction: f32,
    max_step: i16,
    min_step: i16,
    dt: f32,
) -> i16 {
    if value.is_null() {
        return 0;
    }
    let v = unsafe { &mut *value };
    let dt_max = (max_step as f32 * dt).max(1.0);
    let dt_min = (min_step as f32 * dt).max(1.0);
    if *v != target {
        let diff = target.wrapping_sub(*v);
        let mut step_size = diff as f32 * dt_fraction;
        if step_size > dt_min || step_size < -dt_min {
            if step_size > dt_max {
                step_size = dt_max;
            } else if step_size < -dt_max {
                step_size = -dt_max;
            }
        } else if diff >= 0 {
            step_size = dt_min;
        } else {
            step_size = -dt_min;
        }
        let mut step = step_size as i16;
        if step == 0 && diff != 0 {
            step = if diff > 0 { 1 } else { -1 };
        }
        *v = v.wrapping_add(step);
        if step > 0 {
            if target.wrapping_sub(*v) < 0 {
                *v = target;
            }
        } else if target.wrapping_sub(*v) > 0 {
            *v = target;
        }
    }
    target.wrapping_sub(*v)
}

/// `chase_f` verbatim (`m_lib.c`): fixed-step approach with no overshoot.
/// The step is NOT dt-scaled internally — the caller scales it. Returns
/// nonzero when the target is reached.
#[no_mangle]
pub extern "C" fn pc_chase_f(value: *mut f32, target: f32, step: f32) -> bool {
    if value.is_null() {
        return false;
    }
    let v = unsafe { &mut *value };
    let mut m_step = step;
    if m_step != 0.0 {
        if *v > target {
            m_step = -m_step;
        }
        *v += m_step;
        if m_step * (*v - target) >= 0.0 {
            *v = target;
            return true;
        }
    } else if *v == target {
        return true;
    }
    false
}

/// APPEAR rotation.x: `DEG2SHORT_ANGLE2(180.0) - 1` = 32767.
pub const BEE_APPEAR_ROT_X: i16 = 32767;
/// APPEAR rotation.z increment per unit dt: `DEG2SHORT_ANGLE2(22.5)` = 4096.
pub const BEE_APPEAR_ROT_Z_STEP: i16 = 4096;
/// APPEAR alpha increment per unit dt.
pub const BEE_APPEAR_ALPHA_STEP: f32 = 3.0;

/// APPEAR rotation.z increment: `(s16)(4096 * dt)`.
#[no_mangle]
pub extern "C" fn pc_bee_appear_rot_z_inc(dt: f32) -> i16 {
    (BEE_APPEAR_ROT_Z_STEP as f32 * dt) as i16
}

/// APPEAR alpha step: advances `alpha` by `3*dt`; when it reaches 255 it
/// is clamped and the function returns true (transition to FLY — the
/// scale-easing else-branch is NOT taken that frame).
#[no_mangle]
pub extern "C" fn pc_bee_appear_alpha(alpha: *mut f32, dt: f32) -> bool {
    if alpha.is_null() {
        return false;
    }
    let a = unsafe { &mut *alpha };
    *a += BEE_APPEAR_ALPHA_STEP * dt;
    if *a >= 255.0 {
        *a = 255.0;
        return true;
    }
    false
}

/// FLY deformation sizes from `d = |90 - start_frame|`:
/// `size.x = size.y = (0.75 + d/360) * 0.01`,
/// `size.z = (1.5 - d/180) * 0.01`.
#[no_mangle]
pub extern "C" fn pc_bee_fly_size(start_frame: f32, out: *mut f32) {
    if out.is_null() {
        return;
    }
    let d = (90.0 - start_frame).abs();
    let o = unsafe { std::slice::from_raw_parts_mut(out, 3) };
    o[0] = (0.75 + d / 360.0) * 0.01;
    o[1] = o[0];
    o[2] = (1.5 - d / 180.0) * 0.01;
}

/// `aBEE_fly_move_common` angle target: `base - (s16)(|90 - start_frame|
/// * 7.5)`, where `base` is 1600 in FLY/CAUGHT and 5000 otherwise.
#[no_mangle]
pub extern "C" fn pc_bee_fly_base_angle(action: u8) -> i16 {
    if action == 1 || action == 2 {
        1600
    } else {
        5000
    }
}

#[no_mangle]
pub extern "C" fn pc_bee_fly_d_angle(base_angle: i16, start_frame: f32) -> i16 {
    let angle = (90.0 - start_frame).abs() * 7.5;
    base_angle.wrapping_sub(angle as i16)
}

/// Catch-request decision. Returns 0 = no request (delay still active),
/// 1 = net-catch label matched (go to CAUGHT), 2 = force-catch request
/// (`Set_Item_net_catch_request_force_proc`, no position/radius args),
/// 3 = table-catch request (`Set_Item_net_catch_request_table_proc` with
/// the actor's world position and 24.0f radius). The label check runs
/// before the request branch; the plain-else table branch has no
/// `dist < 24` gate at the call site. Bee force distance is `40.0f`.
pub const BEE_CATCH_DELAY: f32 = 60.0;
pub const BEE_FORCE_DIST: f32 = 40.0;
pub const CATCH_TABLE_RADIUS: f32 = 24.0;
pub const CATCH_REQ_NONE: u8 = 0;
pub const CATCH_REQ_CAUGHT: u8 = 1;
pub const CATCH_REQ_FORCE: u8 = 2;
pub const CATCH_REQ_TABLE: u8 = 3;

#[no_mangle]
pub extern "C" fn pc_bee_catch_request(
    catch_delay: f32,
    label_match: bool,
    swing_timer: f32,
    stop_net: bool,
    dist_xz: f32,
) -> u8 {
    if catch_delay > 0.0 {
        return CATCH_REQ_NONE;
    }
    if label_match {
        return CATCH_REQ_CAUGHT;
    }
    if (swing_timer > 0.0 || stop_net) && dist_xz < BEE_FORCE_DIST {
        CATCH_REQ_FORCE
    } else {
        CATCH_REQ_TABLE
    }
}

/// Ant catch-request decision: no catch delay (unlike the bee); force
/// distance is `mFI_UNIT_BASE_SIZE_F` (numerically 40.0, kept symbolic).
pub const ANT_FORCE_DIST: f32 = 40.0;

#[no_mangle]
pub extern "C" fn pc_ant_catch_request(
    label_match: bool,
    swing_timer: f32,
    stop_net: bool,
    dist_xz: f32,
) -> u8 {
    if label_match {
        return CATCH_REQ_CAUGHT;
    }
    if (swing_timer > 0.0 || stop_net) && dist_xz < ANT_FORCE_DIST {
        CATCH_REQ_FORCE
    } else {
        CATCH_REQ_TABLE
    }
}

/// CAUGHT failure fallback: advances `*timer` by `dt`; returns true when
/// it exceeds 1.0 (transition to DISAPPEAR). Called only when no insect
/// actor was created that update — creation and the net-catch
/// label-transfer are separate C-owned operations with independently
/// observable outcomes.
#[no_mangle]
pub extern "C" fn pc_caught_timer_expired(timer: *mut f32, dt: f32) -> bool {
    if timer.is_null() {
        return false;
    }
    let t = unsafe { &mut *timer };
    *t += dt;
    *t > 1.0
}

/// `aANT_calc_scale` verbatim: chases `scales[0]` toward `target` with
/// `chase_f(step * 0.5)`, then copies X into Y and Z. The caller passes
/// the dt-scaled step (`0.1 * dt` from `aANT_disappear`, so the effective
/// chase step is `0.05 * dt`). `aANT_disappear` uses `alpha <= 0.0f`
/// (not `<`) for the delete branch.
#[no_mangle]
pub extern "C" fn pc_ant_calc_scale(scales: *mut f32, step: f32, target: f32) {
    if scales.is_null() {
        return;
    }
    let s = unsafe { std::slice::from_raw_parts_mut(scales, 3) };
    pc_chase_f(s.as_mut_ptr(), target, step * 0.5);
    s[1] = s[0];
    s[2] = s[0];
}

    #[test]
    fn easing_kernels() {
        // CALC_EASE(0) = 0, CALC_EASE(1) = 1.
        assert!((pc_calc_ease(0.0) - 0.0).abs() < 1e-6);
        assert!((pc_calc_ease(1.0) - 1.0).abs() < 1e-6);
        assert!((pc_calc_ease(0.3) - (1.0 - 0.7f32.sqrt())).abs() < 1e-6);
        // add_calc: moves toward target, snaps on overshoot.
        let mut v = 0.0f32;
        let rem = pc_add_calc(&mut v, 10.0, 0.5, 100.0, 0.0, 1.0);
        assert!((v - 5.0).abs() < 1e-6 && (rem - 5.0).abs() < 1e-6);
        let mut v = 9.99f32;
        pc_add_calc(&mut v, 10.0, 0.5, 100.0, 0.05, 1.0);
        assert!((v - 10.0).abs() < 1e-6); // min-step nudge overshoots -> snap
        let mut v = 10.0f32;
        assert_eq!(pc_add_calc(&mut v, 10.0, 0.5, 100.0, 0.0, 1.0), 0.0);
        // add_calc_short_angle2: plain s16 diff, NO wraparound.
        let mut a: i16 = 0;
        let rem = pc_add_calc_short_angle2(&mut a, 1000, 0.5, 250, 0, 1.0);
        assert_eq!(a, 250); // 1000 * 0.5 = 500, clamped to max_step 250
        assert_eq!(rem, 750);
        // No wrap: the s16 diff -64000 truncates to +1536, so the step
        // goes the "wrong" way exactly as retail does.
        let mut a: i16 = 32000;
        pc_add_calc_short_angle2(&mut a, -32000, 1.0, 250, 0, 1.0);
        assert_eq!(a, 32250); // diff wraps to +1536 -> clamped step +250
        // Nonzero-step guarantee: tiny fraction still moves by 1.
        let mut a: i16 = 0;
        pc_add_calc_short_angle2(&mut a, 5, 0.0001, 250, 0, 1.0);
        assert_eq!(a, 1);
        // chase_f: fixed step, no overshoot, exact return.
        let mut v = 0.0f32;
        assert!(!pc_chase_f(&mut v, 1.0, 0.3));
        assert!((v - 0.3).abs() < 1e-6);
        assert!(pc_chase_f(&mut v, 0.35, 0.3)); // would overshoot -> snap
        assert!((v - 0.35).abs() < 1e-6);
        let mut v = 1.0f32;
        assert!(pc_chase_f(&mut v, 1.0, 0.0));
    }

    #[test]
    fn appear_and_fly_kernels() {
        // rotation.z increment is dt-scaled truncation.
        assert_eq!(pc_bee_appear_rot_z_inc(1.0), 4096);
        assert_eq!(pc_bee_appear_rot_z_inc(0.5), 2048);
        assert_eq!(BEE_APPEAR_ROT_X, 32767);
        // alpha: reaches 255 -> clamp + FLY transition, no scale path.
        let mut a = 250.0f32;
        assert!(!pc_bee_appear_alpha(&mut a, 1.0));
        assert!((a - 253.0).abs() < 1e-6);
        assert!(pc_bee_appear_alpha(&mut a, 1.0));
        assert_eq!(a, 255.0);
        // FLY sizes: d = |90 - start_frame|.
        let mut s = [0f32; 3];
        pc_bee_fly_size(90.0, s.as_mut_ptr());
        assert!((s[0] - 0.0075).abs() < 1e-6 && (s[2] - 0.015).abs() < 1e-6);
        pc_bee_fly_size(0.0, s.as_mut_ptr());
        assert!((s[0] - (0.75 + 0.25) * 0.01).abs() < 1e-6);
        assert!((s[2] - (1.5 - 0.5) * 0.01).abs() < 1e-6);
        // base angle: 1600 in FLY(1)/CAUGHT(2), else 5000.
        assert_eq!(pc_bee_fly_base_angle(1), 1600);
        assert_eq!(pc_bee_fly_base_angle(2), 1600);
        assert_eq!(pc_bee_fly_base_angle(4), 5000);
        assert_eq!(pc_bee_fly_d_angle(1600, 90.0), 1600);
        assert_eq!(pc_bee_fly_d_angle(1600, 0.0), 1600 - 675);
    }

    #[test]
    fn catch_and_caught_kernels() {
        // delay active -> no request.
        assert_eq!(pc_bee_catch_request(60.0, false, 1.0, false, 10.0), CATCH_REQ_NONE);
        assert_eq!(pc_bee_catch_request(0.5, false, 1.0, false, 10.0), CATCH_REQ_NONE);
        // label match -> CAUGHT (before the request branch).
        assert_eq!(pc_bee_catch_request(0.0, true, 0.0, false, 999.0), CATCH_REQ_CAUGHT);
        // force: swing timer + in range.
        assert_eq!(pc_bee_catch_request(0.0, false, 1.0, false, 39.9), CATCH_REQ_FORCE);
        // stop-net + in range.
        assert_eq!(pc_bee_catch_request(0.0, false, 0.0, true, 10.0), CATCH_REQ_FORCE);
        // plain else -> table (no dist<24 gate at the call site).
        assert_eq!(pc_bee_catch_request(0.0, false, 0.0, false, 10.0), CATCH_REQ_TABLE);
        assert_eq!(pc_bee_catch_request(0.0, false, 1.0, false, 40.0), CATCH_REQ_TABLE);
        assert_eq!(CATCH_TABLE_RADIUS, 24.0);
        // ant: no delay, same shape.
        assert_eq!(pc_ant_catch_request(true, 0.0, false, 999.0), CATCH_REQ_CAUGHT);
        assert_eq!(pc_ant_catch_request(false, 1.0, false, 10.0), CATCH_REQ_FORCE);
        assert_eq!(pc_ant_catch_request(false, 0.0, false, 10.0), CATCH_REQ_TABLE);
        // CAUGHT fallback: > 1.0, not >=.
        let mut t = 0.0f32;
        assert!(!pc_caught_timer_expired(&mut t, 1.0));
        assert!(!pc_caught_timer_expired(&mut t, 0.0)); // exactly 1.0 -> no
        assert!(pc_caught_timer_expired(&mut t, 0.1)); // 1.1 -> yes
        // ant scale: chase_f on X, copied to Y/Z.
        let mut s = [1.0f32, 0.5, 0.25];
        pc_ant_calc_scale(s.as_mut_ptr(), 0.1, 0.01);
        assert!((s[0] - 0.95).abs() < 1e-6); // 1.0 - 0.1*0.5
        assert_eq!(s[1], s[0]);
        assert_eq!(s[2], s[0]);
    }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slot_reservation() {
        let mut c = InsectController::default();
        // MAKE_NEW scans 0..7.
        for i in 0..8 {
            assert_eq!(c.search_regist_space(make::NEW), Some(i));
            c.occupied[i] = true;
        }
        assert_eq!(c.search_regist_space(make::NEW), None);
        // MAKE_EXIST takes only slot 8, even when 0..7 are full.
        assert_eq!(c.search_regist_space(make::EXIST), Some(8));
        c.occupied[8] = true;
        assert_eq!(c.search_regist_space(make::EXIST), None);
    }

    #[test]
    fn ant_pending_spawn_is_retryable() {
        let mut clip = InsectClip::default();
        clip.make_ant(Xyz::new(1.0, 2.0, 3.0), 4, 5);
        assert!(clip.ant_spawn_pending);
        // Failed spawn keeps the request pending.
        assert!(clip.check_birth_ant(false).is_none());
        assert!(clip.ant_spawn_pending);
        let info = clip.check_birth_ant(true).unwrap();
        assert!(!clip.ant_spawn_pending);
        assert_eq!((info.bx, info.bz), (4, 5));
    }

    #[test]
    fn ant_overlay_override() {
        let mut cands = vec![
            SpawnCandidate { insect: 3, area: 9, weight: 5.0 },
            SpawnCandidate { insect: insect_type::ANT, area: spawn_area::ON_CANDY, weight: 1.0 },
            SpawnCandidate { insect: insect_type::ANT, area: spawn_area::ON_TRASH, weight: 1.0 },
        ];
        limit_insect_data(&mut cands, true, false);
        assert_eq!(cands[0].weight, 0.0);
        assert_eq!(cands[1].weight, 1.0);
        assert_eq!(cands[2].weight, 0.0);
        assert_eq!(ANT_OVERLAY.len(), 3);
    }

    #[test]
    fn bee_lifecycle() {
        let mut bee = BeeActor::new_dormant();
        assert!(bee.is_dormant());
        // Dormant bee does nothing.
        let env = BeeEnv::default();
        let mut ev = Vec::new();
        bee.step(&env, &mut ev);
        assert_eq!(bee.action, bee_action::APPEAR);
        // Honeycomb lands: bee activates.
        bee.activate(Xyz::new(100.0, 10.0, 100.0), 0.0);
        assert!(!bee.is_dormant());
        // ~85 frames of APPEAR -> FLY.
        for _ in 0..90 {
            bee.step(&env, &mut ev);
        }
        assert_eq!(bee.action, bee_action::FLY);
        assert_eq!(bee.catch_delay_frames, bee::CATCH_DELAY_FRAMES);
        // Catch delay counts down once the bee has leveled out (retail
        // gates it on rotation.x <= 22.5 deg); keep the player far away.
        let fly_env = BeeEnv { leveled: true, dist_xz: 100.0, ..BeeEnv::default() };
        for _ in 0..60 {
            bee.step(&fly_env, &mut ev);
        }
        assert_eq!(bee.catch_delay_frames, 0.0);
        // Net catch -> CAUGHT -> conversion -> DISAPPEAR.
        let mut catch_env = BeeEnv { catch_label_is_me: true, leveled: true, ..BeeEnv::default() };
        bee.step(&catch_env, &mut ev);
        assert_eq!(bee.action, bee_action::CAUGHT);
        // Conversion is requested on the next tick (aBEE_caught).
        ev.clear();
        bee.step(&catch_env, &mut ev);
        assert!(ev.contains(&BeeEvent::ConvertToInsect));
        bee.on_converted();
        ev.clear();
        bee.step(&catch_env, &mut ev);
        assert!(ev.contains(&BeeEvent::ChangeCatchLabel));
        assert_eq!(bee.action, bee_action::DISAPPEAR);
        // ~17 frames -> delete.
        ev.clear();
        for _ in 0..20 {
            bee.step(&catch_env, &mut ev);
        }
        assert!(ev.contains(&BeeEvent::Delete));
    }

    #[test]
    fn bee_attack_path() {
        let mut bee = BeeActor::new_dormant();
        bee.activate(Xyz::new(100.0, 10.0, 100.0), 0.0);
        let env = BeeEnv::default();
        let mut ev = Vec::new();
        for _ in 0..90 {
            bee.step(&env, &mut ev);
        }
        assert_eq!(bee.action, bee_action::FLY);
        // Player close -> ATTACK_WAIT.
        let mut close = BeeEnv { leveled: true, dist_xz: 20.0, ..BeeEnv::default() };
        bee.step(&close, &mut ev);
        assert_eq!(bee.action, bee_action::ATTACK_WAIT);
        // Player enters BEE_ATTACK status -> ATTACK + sting request.
        close.player_status_for_bee = bee_player_status::ATTACK;
        ev.clear();
        bee.step(&close, &mut ev);
        assert_eq!(bee.action, bee_action::ATTACK);
        assert!(ev.contains(&BeeEvent::RequestSting));
        // Bee waits while the player is stung...
        let mut stung = close;
        stung.player_stung = true;
        stung.sting_finished = false;
        bee.step(&stung, &mut ev);
        assert_eq!(bee.action, bee_action::ATTACK);
        // ...then disappears when the sting ends.
        stung.sting_finished = true;
        ev.clear();
        bee.step(&stung, &mut ev);
        assert_eq!(bee.action, bee_action::DISAPPEAR);
    }

    #[test]
    fn ant_substrate_and_catch() {
        let mut ant = AntActor::new(Xyz::new(5.0, 0.0, 5.0), Some(item::FOOD_CANDY));
        let mut ev = Vec::new();
        // Persists on candy.
        let env = AntEnv { below_fg: Some(item::FOOD_CANDY), ..AntEnv::default() };
        ant.step(&env, &mut ev);
        assert_eq!(ant.action, ant_action::WAIT);
        // Substrate removed -> DISAPPEAR.
        let gone = AntEnv { below_fg: Some(0x1234), ..AntEnv::default() };
        ant.step(&gone, &mut ev);
        assert_eq!(ant.action, ant_action::DISAPPEAR);
        // Fresh ant, caught by the net.
        let mut ant2 = AntActor::new(Xyz::new(5.0, 0.0, 5.0), Some(item::KABU_SPOILED));
        let catch_env = AntEnv {
            below_fg: Some(item::KABU_SPOILED),
            catch_label_is_me: true,
            ..AntEnv::default()
        };
        ant2.step(&catch_env, &mut ev);
        assert_eq!(ant2.action, ant_action::CAUGHT);
        ev.clear();
        ant2.step(&catch_env, &mut ev);
        assert!(ev.contains(&AntEvent::ConvertToInsect));
        ant2.on_converted();
        ev.clear();
        ant2.step(&catch_env, &mut ev);
        assert!(ev.contains(&AntEvent::ChangeCatchLabel));
        assert_eq!(ant2.action, ant_action::DISAPPEAR);
    }

    #[test]
    fn honeycomb_chain() {
        let mut drop = HoneycombDrop::new(Xyz::new(50.0, 0.0, 50.0));
        drop.on_landed();
        // One update later the bee is positioned.
        assert!(drop.step());
        assert_eq!(drop.state, HoneycombState::Linger);
        // Lingers 120 frames independently.
        for _ in 0..120 {
            assert!(!drop.step());
        }
        assert_eq!(drop.state, HoneycombState::Done);
    }

    #[test]
    fn bee_tree_table() {
        assert!(is_bee_tree(item::TREE_BEES));
        assert!(is_bee_tree(item::CEDAR_TREE_BEES));
        assert!(is_bee_tree(item::GOLD_TREE_BEES));
        assert!(!is_bee_tree(item::TREE));
        assert_eq!(bee_tree_revert(item::TREE_BEES), Some(item::TREE));
        assert_eq!(bee_tree_revert(item::CEDAR_TREE_BEES), Some(item::CEDAR_TREE));
        assert_eq!(bee_tree_revert(item::GOLD_TREE_BEES), Some(item::GOLD_TREE));
        assert_eq!(item::HONEYCOMB, 0x0062);
        assert_eq!(item::FOOD_CANDY, 0x2806);
        assert_eq!(item::KABU_SPOILED, 0x2F03);
    }

    #[test]
    fn tree_shake_kernels() {
        // Eligibility predicate.
        assert!(bee_spawn_eligible(false, false, false, false));
        assert!(!bee_spawn_eligible(true, false, false, false));
        assert!(!bee_spawn_eligible(false, true, false, false));
        assert!(!bee_spawn_eligible(false, false, true, false));
        assert!(!bee_spawn_eligible(false, false, false, true));
        // Retry window.
        assert_eq!(bee_shake_retry_step(5.0, true, 1.0), (0.0, true));
        assert_eq!(bee_shake_retry_step(5.0, false, 1.0), (4.0, false));
        assert_eq!(bee_shake_retry_step(0.5, false, 1.0), (0.0, false));
        // Revert id.
        assert_eq!(bee_tree_revert_id(item::TREE_BEES), item::TREE as i32);
        assert_eq!(bee_tree_revert_id(item::TREE), -1);
        // Drop validity.
        assert!(bee_drop_valid(10.0, 20.0));
        assert!(!bee_drop_valid(-1.0, 20.0));
        assert!(!bee_drop_valid(10.0, -1.0));
        // Shock / no-bee status.
        assert!(bee_shock_request(true, 30.0));
        assert!(!bee_shock_request(true, 29.9));
        assert!(!bee_shock_request(false, 30.0));
        assert!(bee_no_bee_status(29.5));
        assert!(!bee_no_bee_status(29.4));
        // Force catch.
        assert!(bee_force_catch(30.0, 5.0, false));
        assert!(bee_force_catch(30.0, 0.0, true));
        assert!(!bee_force_catch(30.0, 0.0, false));
        assert!(!bee_force_catch(40.0, 5.0, false));
        // Speed target quirk.
        assert_eq!(bee::SPEED_TARGET, 8.9);
        // Target frame on s16 diffs.
        assert_eq!(bee_target_frame(0), 90.0);
        assert_eq!(bee_target_frame(5400), 180.0); // 90 + 180
        assert_eq!(bee_target_frame(-5400), 0.0); // 90 - 180
        assert!((bee_target_frame(900) - 120.0).abs() < 1e-5);
        // Alpha steps.
        assert_eq!(bee_alpha_step(1.0), 3.0);
        assert_eq!(bee_alpha_step(2.0), 6.0);
        assert_eq!(bee_disappear_alpha(20.0, 1.0), 5.0);
        assert_eq!(bee_disappear_alpha(10.0, 1.0), 0.0);
        // Corrected insect type ids.
        assert_eq!(insect_type::ANT, 38);
        assert_eq!(insect_type::COCKROACH, 28);
        // Corrected player status values.
        assert_eq!(bee_player_status::ATTACK, 1);
        assert_eq!(bee_player_status::ENTER_BUILDING, 2);
    }
}
