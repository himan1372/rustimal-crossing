//! Villager movement for the Rust rewrite.
//!
//! Source-verified (upstream `include/ac_npc.h`,
//! `src/actor/npc/ac_npc_move.c_inc`,
//! `src/actor/npc/ac_npc_think_wander.c_inc`,
//! `src/actor/npc/ac_npc_think.c_inc`,
//! `src/actor/npc/ac_npc_think_go_home.c_inc`,
//! `src/actor/npc/ac_npc2_act_walk.c_inc`,
//! `src/actor/npc/ac_npc2_think.c_inc`):
//!
//! The GameCube villagers do NOT run a global pathfinder. There is no
//! evidenced A*, BFS, navmesh, or waypoint graph for normal villagers.
//! Movement is goal-directed reactive steering:
//!
//! * The think layer picks a destination: a random world-space point
//!   inside the movement range for wandering
//!   (`center + (sin(angle), cos(angle)) * radius`), an actor's position
//!   for pursuit, or `house + (20, 60)` for going home.
//! * The action layer steers directly toward it (`dst_pos` is the goal,
//!   `avoid_pos` the current steering target; both start equal).
//! * Every update runs background collision checks; when blocked, the
//!   thinker probes ±22.5°, then ±45°, then ±90° at ~2 unit-widths ahead,
//!   temporarily replacing `avoid_pos`. If all probes fail, the villager
//!   turns (180°, or a random ±112.5° turn when badly stuck).
//! * Movement is continuous (float position, acceleration/deceleration),
//!   while map reasoning is discrete (world pos → block/unit/foreground).
//! * Wandering is range-constrained (block/circle/square types), and the
//!   wait/walk/run choice uses per-personality probability tables.
//! * Friendship feeds movement: same block + friendship < 0 → avoid the
//!   player; friendship > 128 → seek the player.
//! * The fish-catch clap is a checked reaction: normal feel, player
//!   catching fish/bug, within 3 units, facing within 67.5°.
//!
//! Rewrite-owned: float math uses `f32`/`libm`-style helpers written
//! here; exact fixed-point angle tables from the decomp are not
//! reproduced.

use crate::npc::Personality;

/// Movement range types, mirroring `aNPC_MOVE_RANGE_TYPE_*`.
/// `Square` exists only in later revisions
/// (`#if VERSION >= VER_GAFU01_00`).
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MoveRangeType {
    Block = 0,
    Circle = 1,
    Square = 2,
}

/// Friendship-driven movement mode, mirroring `aNPC_FRIENDSHIP_*`.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FriendshipMode {
    Normal = 0,
    Avoid = 1,
    Search = 2,
}

/// Which movement behavior the walk action runs, mirroring the
/// `aNPC_ACT_WALK_PROC` table.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WalkProc {
    Move = 0,
    AvoidMove = 1,
    SearchMove = 2,
    ToPointMove = 3,
}

/// Wander choice, mirroring the `aNPC_ACT_*` values the decide roll can
/// produce.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WanderChoice {
    Wait = 0,
    Walk = 1,
    Run = 2,
}

/// Per-personality wander decide borders, verbatim from
/// `ac_npc_think_wander.c_inc` (roll is `RANDOM(10)`; roll <= border[0]
/// → wait, <= border[1] → walk, else run). Source comments give the
/// resulting probabilities.
pub const DECIDE_BORDERS: [(Personality, [i32; 2]); 6] = [
    (Personality::Girl, [3, 6]),       // 40% wait, 30% walk, 30% run
    (Personality::KoGirl, [6, 8]),    // 70% wait, 20% walk, 10% run
    (Personality::Boy, [5, 7]),       // 60% wait, 20% walk, 20% run
    (Personality::SportMan, [2, 4]),  // 30% wait, 20% walk, 50% run
    (Personality::GrimMan, [3, 6]),   // 40% wait, 30% walk, 30% run
    (Personality::NaniwaLady, [4, 8]), // 50% wait, 40% walk, 10% run
];

/// Decide wait/walk/run from a 0-9 roll, mirroring the decomp loop.
pub fn decide_wander(personality: Personality, roll: i32) -> WanderChoice {
    let borders = DECIDE_BORDERS
        .iter()
        .find(|(p, _)| *p == personality)
        .map(|(_, b)| *b)
        .unwrap_or([4, 8]);
    if roll <= borders[0] {
        WanderChoice::Wait
    } else if roll <= borders[1] {
        WanderChoice::Walk
    } else {
        WanderChoice::Run
    }
}

/// Angular avoidance probes, verbatim from `aNPC_avoid_wall`'s
/// `add_angl` table (degrees; each tried in + then - direction).
pub const AVOID_PROBE_ANGLES: [f32; 3] = [22.5, 45.0, 90.0];
/// Probe distance: two unit-widths ahead (`2 * mFI_UT_WORLDSIZE_*_F`).
pub const AVOID_PROBE_DISTANCE_UNITS: f32 = 2.0;
/// Random backward turn angles when badly stuck
/// (`turn_angl_table`).
pub const TURN_BACKWARD_ANGLES: [f32; 2] = [112.5, -112.5];

/// Go-home destination offset from the house position
/// (`ac_npc_think_go_home.c_inc`).
pub const GO_HOME_OFFSET: (f32, f32) = (20.0, 60.0);

/// Clap-reaction limits from `aNPC_check_clap`: the player must be
/// catching fish/bugs within 3 unit-widths and within 67.5° of facing,
/// and the villager's feel must be normal.
pub const CLAP_DISTANCE_UNITS: f32 = 3.0;
pub const CLAP_FACING_DEGREES: f32 = 67.5;

/// Friendship thresholds from `aNPC_chk_avoid_and_search` (only applies
/// when the player is in the villager's block).
pub const FRIENDSHIP_AVOID_BELOW: i32 = 0;
pub const FRIENDSHIP_SEARCH_ABOVE: i32 = 128;

pub fn friendship_mode(friendship: i32, player_same_block: bool) -> FriendshipMode {
    if !player_same_block {
        return FriendshipMode::Normal;
    }
    if friendship < FRIENDSHIP_AVOID_BELOW {
        FriendshipMode::Avoid
    } else if friendship > FRIENDSHIP_SEARCH_ABOVE {
        FriendshipMode::Search
    } else {
        FriendshipMode::Normal
    }
}

/// Movement state, mirroring the fields of `npc_movement_s`.
#[derive(Clone, Copy, Debug)]
pub struct Movement {
    pub max_speed: f32,
    pub acceleration: f32,
    pub deceleration: f32,
    pub speed: f32,
    /// Ultimate destination.
    pub dst_x: f32,
    pub dst_z: f32,
    /// Current steering target (diverges from dst when avoiding).
    pub avoid_x: f32,
    pub avoid_z: f32,
    pub move_timer: f32,
    pub range_type: MoveRangeType,
    pub range_center_x: f32,
    pub range_center_z: f32,
    pub range_radius: f32,
    /// Steering angle toward `avoid_pos`.
    pub mv_angle_deg: f32,
    pub arrival_radius: f32,
    pub pos_x: f32,
    pub pos_z: f32,
}

impl Movement {
    /// Set a destination (`aNPC_set_dst_pos`): both targets start equal
    /// and the move timer resets.
    pub fn set_dst(&mut self, x: f32, z: f32) {
        self.dst_x = x;
        self.dst_z = z;
        self.avoid_x = x;
        self.avoid_z = z;
        self.move_timer = 0.0;
    }

    /// Temporarily steer around an obstacle (`aNPC_set_avoid_pos`).
    pub fn set_avoid(&mut self, x: f32, z: f32) {
        self.avoid_x = x;
        self.avoid_z = z;
    }

    /// Restore the true destination after reaching the avoidance point
    /// (`aNPC_check_arrive_destination`).
    pub fn restore_dst(&mut self) {
        self.avoid_x = self.dst_x;
        self.avoid_z = self.dst_z;
    }

    /// Whether the villager reached the current steering target.
    pub fn arrived(&self) -> bool {
        let dx = self.avoid_x - self.pos_x;
        let dz = self.avoid_z - self.pos_z;
        dx * dx + dz * dz <= self.arrival_radius * self.arrival_radius
    }

    /// Steer toward the avoidance position and advance with
    /// acceleration/deceleration (`aNPC_position_move` model).
    /// Returns the distance moved this tick.
    pub fn step(&mut self, dt: f32) -> f32 {
        let dx = self.avoid_x - self.pos_x;
        let dz = self.avoid_z - self.pos_z;
        self.mv_angle_deg = dz.atan2(dx).to_degrees();
        if self.speed < self.max_speed {
            self.speed = (self.speed + self.acceleration * 0.5 * dt).min(self.max_speed);
        } else {
            self.speed = (self.speed - self.deceleration * 0.5 * dt).max(self.max_speed);
        }
        let rad = self.mv_angle_deg.to_radians();
        let dist = self.speed * dt;
        self.pos_x += rad.cos() * dist;
        self.pos_z += rad.sin() * dist;
        dist
    }

    /// Generate a wander destination: random angle around the range
    /// center (`aNPC_think_wander_move_next`).
    pub fn wander_destination(&self, angle_deg: f32) -> (f32, f32) {
        let rad = angle_deg.to_radians();
        (
            self.range_center_x + rad.sin() * self.range_radius,
            self.range_center_z + rad.cos() * self.range_radius,
        )
    }

    /// Probe a candidate avoidance position at `angle_offset_deg` from
    /// the current heading, two unit-widths ahead (`aNPC_avoid_wall`).
    pub fn probe_avoid(&self, angle_offset_deg: f32, unit_world_size: f32) -> (f32, f32) {
        let rad = (self.mv_angle_deg + angle_offset_deg).to_radians();
        let d = AVOID_PROBE_DISTANCE_UNITS * unit_world_size;
        (self.pos_x + rad.sin() * d, self.pos_z + rad.cos() * d)
    }

    /// Circle containment check: is (x, z) inside the movement circle?
    pub fn in_circle(&self, x: f32, z: f32) -> bool {
        let dx = x - self.range_center_x;
        let dz = z - self.range_center_z;
        dx * dx + dz * dz <= self.range_radius * self.range_radius
    }
}

impl Default for Movement {
    fn default() -> Self {
        Self {
            max_speed: 0.0,
            acceleration: 0.0,
            deceleration: 0.0,
            speed: 0.0,
            dst_x: 0.0,
            dst_z: 0.0,
            avoid_x: 0.0,
            avoid_z: 0.0,
            move_timer: 0.0,
            range_type: MoveRangeType::Block,
            range_center_x: 0.0,
            range_center_z: 0.0,
            range_radius: 0.0,
            mv_angle_deg: 0.0,
            arrival_radius: 1.0,
            pos_x: 0.0,
            pos_z: 0.0,
        }
    }
}

/// Whether the clap reaction fires (`aNPC_check_clap`).
pub fn check_clap(
    feel_normal: bool,
    player_catching: bool,
    distance_units: f32,
    facing_diff_deg: f32,
) -> bool {
    feel_normal
        && player_catching
        && distance_units < CLAP_DISTANCE_UNITS
        && facing_diff_deg.abs() < CLAP_FACING_DEGREES
}

/// C ABI: wander choice for a personality and a 0-9 roll
/// (0 = wait, 1 = walk, 2 = run).
#[no_mangle]
pub extern "C" fn pc_wander_choice(personality: u8, roll: i32) -> u8 {
    let p = match personality {
        0 => Personality::Girl,
        1 => Personality::KoGirl,
        2 => Personality::Boy,
        3 => Personality::SportMan,
        4 => Personality::GrimMan,
        _ => Personality::NaniwaLady,
    };
    decide_wander(p, roll) as u8
}

/// C ABI: friendship movement mode (0 = normal, 1 = avoid, 2 = search).
#[no_mangle]
pub extern "C" fn pc_friendship_mode(friendship: i32, player_same_block: u8) -> u8 {
    friendship_mode(friendship, player_same_block != 0) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decide_borders_match_source_probabilities() {
        // Jock (sport_man): 30% wait, 20% walk, 50% run.
        let waits = (0..10).filter(|&r| decide_wander(Personality::SportMan, r) == WanderChoice::Wait).count();
        let walks = (0..10).filter(|&r| decide_wander(Personality::SportMan, r) == WanderChoice::Walk).count();
        let runs = (0..10).filter(|&r| decide_wander(Personality::SportMan, r) == WanderChoice::Run).count();
        assert_eq!((waits, walks, runs), (3, 2, 5));
        // Peppy (ko_girl): 70% wait, 20% walk, 10% run.
        let waits = (0..10).filter(|&r| decide_wander(Personality::KoGirl, r) == WanderChoice::Wait).count();
        let runs = (0..10).filter(|&r| decide_wander(Personality::KoGirl, r) == WanderChoice::Run).count();
        assert_eq!((waits, runs), (7, 1));
    }

    #[test]
    fn dst_and_avoid_start_equal() {
        let mut m = Movement::default();
        m.set_dst(100.0, 50.0);
        assert_eq!((m.dst_x, m.dst_z), (100.0, 50.0));
        assert_eq!((m.avoid_x, m.avoid_z), (100.0, 50.0));
        m.set_avoid(90.0, 45.0);
        assert_eq!((m.avoid_x, m.avoid_z), (90.0, 45.0));
        m.restore_dst();
        assert_eq!((m.avoid_x, m.avoid_z), (100.0, 50.0));
    }

    #[test]
    fn wander_destination_is_circular() {
        let m = Movement { range_center_x: 10.0, range_center_z: 20.0, range_radius: 30.0, ..Movement::default() };
        for deg in [0.0, 90.0, 180.0, 270.0] {
            let (x, z) = m.wander_destination(deg);
            let dx = x - 10.0;
            let dz = z - 20.0;
            let dist = (dx * dx + dz * dz).sqrt();
            assert!((dist - 30.0).abs() < 0.001);
            assert!(m.in_circle(x, z));
        }
    }

    #[test]
    fn probe_angles_match_source() {
        assert_eq!(AVOID_PROBE_ANGLES, [22.5, 45.0, 90.0]);
        assert_eq!(TURN_BACKWARD_ANGLES, [112.5, -112.5]);
        assert_eq!(GO_HOME_OFFSET, (20.0, 60.0));
    }

    #[test]
    fn friendship_drives_movement_mode() {
        assert_eq!(friendship_mode(-1, true), FriendshipMode::Avoid);
        assert_eq!(friendship_mode(129, true), FriendshipMode::Search);
        assert_eq!(friendship_mode(50, true), FriendshipMode::Normal);
        // Different block: no effect.
        assert_eq!(friendship_mode(-5, false), FriendshipMode::Normal);
        assert_eq!(friendship_mode(200, false), FriendshipMode::Normal);
    }

    #[test]
    fn clap_check_matches_source() {
        assert!(check_clap(true, true, 2.9, 60.0));
        assert!(!check_clap(false, true, 2.9, 60.0)); // wrong feel
        assert!(!check_clap(true, false, 2.9, 60.0)); // not catching
        assert!(!check_clap(true, true, 3.1, 60.0)); // too far
        assert!(!check_clap(true, true, 2.9, 70.0)); // facing away
    }

    #[test]
    fn step_accelerates_toward_avoid_pos() {
        let mut m = Movement {
            max_speed: 4.0,
            acceleration: 2.0,
            deceleration: 2.0,
            arrival_radius: 0.5,
            ..Movement::default()
        };
        m.set_dst(10.0, 0.0);
        let d0 = m.step(1.0);
        assert!(d0 > 0.0 && d0 < 4.0); // accelerating, below max
        assert!(m.pos_x > 0.0);
        assert!(!m.arrived());
    }
}
