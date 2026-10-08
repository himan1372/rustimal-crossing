//! Player movement for the Rust rewrite.
//!
//! Source-verified (upstream `src/game/m_player_controller.c_inc`,
//! `src/game/m_player_main_walk.c_inc`,
//! `src/game/m_player_main_run.c_inc`,
//! `src/game/m_player_main_dash.c_inc`,
//! `src/game/m_player_common.c_inc`, `include/ac_museum_insect_priv.h`):
//!
//! The player pipeline is: GameCube pad → `padmgr` (stick correction) →
//! `mcon` (movement percent/angle) → player controller getters →
//! locomotion state (WALK/RUN/DASH) → `Player_actor_Movement_Walk` core
//! → actor movement → background/object collision. No pathfinding.
//!
//! * The controller layer exposes `move_pR` (stick magnitude),
//!   `move_angle`/`last_move_angle`, and `adjusted_pR`/`last_adjusted_pR`
//!   from `gamePT->mcon` (title demo uses its own copy).
//! * `Player_actor_Movement_Walk` smooths the facing toward the stick
//!   angle with a magnitude-dependent turn coefficient:
//!   `movePR >= 1.0 → 0.5`, `<= 0.05 → 0.01`, else
//!   `0.01 + 0.5157895 * (movePR - 0.05)`, eased by
//!   `CALC_EASE(x) = 1 - sqrt(1 - x)` through `add_calc_short_angle2`.
//! * RUN is literally `Player_actor_Movement_Walk`; DASH is literally
//!   `Player_actor_Movement_Run`. The states differ in speed regime,
//!   animation, effects, and checks — not in the steering core.
//! * Dash speed: `movePR = (7.5 * movePR) / over_norm` when the dash
//!   button (B/L/R) is held.
//! * Animation speed derives from actual speed:
//!   `sp = 0.6 * sqrt((speed * over_norm) / 7.5)`, adjusted near walls
//!   by the wall-angle vs facing (`sp *= sqrt(|sin(wall - facing)|)`,
//!   clamped to a 0.22 minimum).
//! * Braking exists as its own layer
//!   (`Player_actor_Movement_Base_Braking`, amount `0.32625001`).
//! * Dash validates terrain with 12 flat-place samples ahead
//!   (`mCoBG_GetBgNorm_FromWpos` per sample).
//!
//! Rewrite-owned: angles are `f32` degrees here; the decomp uses wrapped
//! `s16` game angles. Exact stick-correction curves and max speeds per
//! state are not yet traced.

/// Controller movement interpretation, mirroring the `mcon` fields the
/// player getters read.
#[derive(Clone, Copy, Debug, Default)]
pub struct ControllerMove {
    pub move_px: f32,
    pub move_py: f32,
    /// Stick magnitude.
    pub move_pr: f32,
    /// Requested movement angle, degrees.
    pub move_angle_deg: f32,
    pub last_move_angle_deg: f32,
    /// Filtered magnitude.
    pub adjusted_pr: f32,
    pub last_adjusted_pr: f32,
}

/// Turn coefficient from stick magnitude, mirroring
/// `Player_actor_Movement_Walk`'s `mod` computation.
pub fn turn_mod(move_pr: f32) -> f32 {
    // NOTE: retail writes `0.01f`; some decompilers print the exact
    // decimal expansion `0.0099999998f`. Both are the same f32
    // (0x3C23D70A) — `0.01f32` here is already bit-identical.
    if move_pr >= 1.0 {
        0.5
    } else if move_pr <= 0.05 {
        0.01
    } else {
        0.01 + 0.5157895 * (move_pr - 0.05)
    }
}

/// `CALC_EASE(x) = 1 - sqrt(1 - x)`.
pub fn calc_ease(x: f32) -> f32 {
    1.0 - (1.0 - x).sqrt()
}

/// Wrap an angle in degrees to (-180, 180].
pub fn wrap_deg(a: f32) -> f32 {
    let mut v = a % 360.0;
    if v <= -180.0 {
        v += 360.0;
    } else if v > 180.0 {
        v -= 360.0;
    }
    v
}

/// Move `current` toward `target` by the eased turn coefficient,
/// mirroring `add_calc_short_angle2`'s shortest-arc approach.
pub fn smooth_turn_toward(current_deg: f32, target_deg: f32, move_pr: f32) -> f32 {
    let diff = wrap_deg(target_deg - current_deg);
    current_deg + diff * calc_ease(turn_mod(move_pr))
}

/// Locomotion states. WALK/RUN/DASH share the movement core; the rest
/// are the surrounding actor state machine.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocomotionState {
    Wait = 0,
    Walk = 1,
    Run = 2,
    Dash = 3,
    Fall = 4,
    Tumble = 5,
    Wade = 6,
}

impl LocomotionState {
    /// The movement core each state ultimately runs: DASH → RUN → WALK.
    pub fn movement_core(self) -> LocomotionState {
        match self {
            LocomotionState::Dash => LocomotionState::Run,
            LocomotionState::Run => LocomotionState::Walk,
            other => other,
        }
    }
}

/// Dash speed regime: `movePR = (7.5 * movePR) / over_norm`.
pub const DASH_SPEED_NUMERATOR: f32 = 7.5;

/// Animation speed from actor speed:
/// `sp = 0.6 * sqrt((speed * over_norm) / 7.5)`.
pub fn anim_speed(speed: f32, over_norm: f32) -> f32 {
    0.6 * ((speed * over_norm) / 7.5).sqrt()
}

/// Wall adjustment: with one wall hit,
/// `sp *= sqrt(|sin(wall_angle - facing)|)`, minimum 0.22.
pub fn anim_speed_near_wall(sp: f32, wall_angle_deg: f32, facing_deg: f32) -> f32 {
    let m = (wall_angle_deg - facing_deg).to_radians().sin().abs();
    (sp * m.sqrt()).max(0.22)
}

/// Braking amount from `Player_actor_Movement_Base_Braking`.
pub const BRAKE_AMOUNT: f32 = 0.32625001;

/// Dash flat-place sample offsets (forward, lateral), verbatim from
/// `m_player_main_dash.c_inc`.
pub const DASH_SAMPLE_OFFSETS: [(f32, f32); 12] = [
    (0.0, 0.0),
    (0.0, 20.0),
    (0.0, -20.0),
    (28.284271, 0.0),
    (28.284271, 20.0),
    (28.284271, -20.0),
    (56.568542, 0.0),
    (56.568542, 20.0),
    (56.568542, -20.0),
    (84.85281, 0.0),
    (84.85281, 20.0),
    (84.85281, -20.0),
];

/// Player locomotion state: facing smoothing + speed.
#[derive(Clone, Copy, Debug)]
pub struct PlayerMovement {
    pub facing_deg: f32,
    pub speed: f32,
    pub state: LocomotionState,
    pub pos_x: f32,
    pub pos_z: f32,
}

impl PlayerMovement {
    /// One tick of the shared movement core: smooth facing toward the
    /// stick angle, then advance. `dash_held` selects the dash speed
    /// regime; `over_norm` is the speed normalizer.
    pub fn step_core(&mut self, ctrl: &ControllerMove, dash_held: bool, over_norm: f32, dt: f32) {
        self.facing_deg = smooth_turn_toward(self.facing_deg, ctrl.move_angle_deg, ctrl.move_pr);
        let mut pr = ctrl.move_pr;
        if dash_held {
            pr = (DASH_SPEED_NUMERATOR * pr) / over_norm.max(0.0001);
        }
        self.speed = pr;
        let rad = self.facing_deg.to_radians();
        self.pos_x += rad.sin() * self.speed * dt;
        self.pos_z += rad.cos() * self.speed * dt;
    }

    /// Brake toward a stop (`Player_actor_Movement_Base_Braking`).
    pub fn brake(&mut self) {
        self.speed = (self.speed - BRAKE_AMOUNT).max(0.0);
    }
}

impl Default for PlayerMovement {
    fn default() -> Self {
        Self { facing_deg: 0.0, speed: 0.0, state: LocomotionState::Wait, pos_x: 0.0, pos_z: 0.0 }
    }
}

/// C ABI: turn coefficient for a stick magnitude.
#[no_mangle]
pub extern "C" fn pc_turn_mod(move_pr: f32) -> f32 {
    turn_mod(move_pr)
}

/// C ABI: locomotion *state classification* helper (0 = wait, 1 = walk,
/// 2 = run, 3 = dash).
///
/// This is NOT a replacement for `Player_actor_Movement_Walk` (or any
/// retail movement routine): retail's walk frame also does reinput
/// force-position/angle, animation calc + search, lean angle, object
/// check, BG check, item handling, and the proc-index request. Do not
/// wire this as the movement core; the real Wave 2 bridge will be a
/// `#[repr(C)]` player-move state struct passed to a fuller kernel.
#[no_mangle]
pub extern "C" fn pc_locomotion_core(state: u8) -> u8 {
    let s = match state {
        1 => LocomotionState::Walk,
        2 => LocomotionState::Run,
        3 => LocomotionState::Dash,
        _ => LocomotionState::Wait,
    };
    s.movement_core() as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turn_mod_matches_source() {
        assert_eq!(turn_mod(1.0), 0.5);
        assert_eq!(turn_mod(2.0), 0.5);
        assert_eq!(turn_mod(0.05), 0.01);
        assert_eq!(turn_mod(0.0), 0.01);
        let mid = turn_mod(0.5);
        assert!((mid - (0.01 + 0.5157895 * 0.45)).abs() < 1e-6);
    }

    #[test]
    fn calc_ease_matches_macro() {
        assert!((calc_ease(0.5) - (1.0 - 0.5f32.sqrt())).abs() < 1e-6);
        assert_eq!(calc_ease(0.0), 0.0);
    }

    #[test]
    fn smooth_turn_takes_shortest_arc() {
        // 359° -> 1° should turn +2°, not -358°.
        let next = smooth_turn_toward(359.0, 1.0, 1.0);
        let moved = wrap_deg(next - 359.0);
        assert!(moved > 0.0 && moved < 2.0);
        // Stronger stick turns further per tick than a light touch.
        let full = wrap_deg(smooth_turn_toward(0.0, 90.0, 1.0)).abs();
        let light = wrap_deg(smooth_turn_toward(0.0, 90.0, 0.1)).abs();
        assert!(full > light);
    }

    #[test]
    fn locomotion_core_hierarchy() {
        assert_eq!(LocomotionState::Dash.movement_core(), LocomotionState::Run);
        assert_eq!(LocomotionState::Run.movement_core(), LocomotionState::Walk);
        assert_eq!(LocomotionState::Walk.movement_core(), LocomotionState::Walk);
    }

    #[test]
    fn anim_speed_wall_adjustment() {
        let sp = anim_speed(7.5, 1.0);
        assert!((sp - 0.6).abs() < 1e-5);
        // Facing straight into the wall: sin(90°)=1, unchanged.
        let into = anim_speed_near_wall(sp, 90.0, 0.0);
        assert!((into - sp).abs() < 1e-5);
        // Facing along the wall: clamped to the 0.22 minimum.
        assert_eq!(anim_speed_near_wall(sp, 0.0, 0.0), 0.22);
    }

    #[test]
    fn dash_samples_match_source() {
        assert_eq!(DASH_SAMPLE_OFFSETS.len(), 12);
        assert_eq!(DASH_SAMPLE_OFFSETS[3], (28.284271, 0.0));
        assert_eq!(DASH_SAMPLE_OFFSETS[11], (84.85281, -20.0));
    }
}
