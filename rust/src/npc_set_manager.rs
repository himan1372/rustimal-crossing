//! Villager placement and off-screen movement (`ac_set_npc_manager.c`):
//! which villagers are spawned in the player's acre, where they stand, how
//! walkers advance between acres while unseen, and how they are sent home.
//!
//! Source-verified against ac-decomp `src/actor/ac_set_npc_manager.c` +
//! `include/ac_set_npc_manager.h` (primary) and ACGC-PC-Port (secondary).
//! Uses the walk table from `npc_walk.rs`.
//!
//! ## Facts
//! - Spawn scope: the player's next acre (`aSNMgr_renewal_set_scope`); the
//!   guest scope extends it by one unit (40) on every side. Regular
//!   villagers standing in the scope are made at a safe unit; walking
//!   villagers in a neighbouring acre appear as guests at a gate.
//! - Safe unit (regular): step off a broker tent / designer car footprint,
//!   then the 3x3 neighbourhood (order below), then a random safe unit
//!   in the acre (units 1..14 only). Guest: a random gate of the villager's
//!   own acre on the side facing the player.
//! - Off-screen walking: every 7 s (420 frames; PC uses a dt accumulator)
//!   each walker moves one unit toward its next acre along the larger axis.
//!   It never steps into the acre the player stands in. An acre already
//!   holding 3 villagers makes walkers wait (STAY_IN_BLOCK).
//! - On arrival a walker stays 108000 frames (30 min). Then, if its goal
//!   was its own home, the walk slot passes to another villager; otherwise
//!   it heads home.
//! - Going home (schedule no longer FIELD): try the home acre, then N, S, W,
//!   E of it, skipping the event acre and the player's acre; status ARRIVE
//!   for the home acre, WALKING otherwise; the stay counter is set to
//!   108001 so it rotates at once. With no candidate it is put inside its
//!   house.
//! - Spawning is suppressed during part-time work, the first intro, demos
//!   and Halloween, or when the player's acre already holds 3 villagers.
//!
//! ## PC-port changes
//! - Borderless: the scope is the full 3x3 acres (+-1 block), regular
//!   villagers appear within +-1 acre, and the spawn list holds 15 (not 5).
//!
//! ## Quirks
//! - A guest that leaves without being made gets timer 10800 (3 min); the
//!   decomp suspects 108001 was intended.
//! - The per-acre count saturates at 14 when it reaches 15.

use crate::npc_walk::{status, APPEAR_GUEST, APPEAR_REGULAR, WAY_DOWN, WAY_LEFT, WAY_RIGHT, WAY_UP};

pub const BLOCK_SIZE: f32 = 640.0;
pub const HALF_BLOCK: f32 = 320.0;
pub const UNIT: f32 = 40.0;
pub const WALK_TICK_FRAMES: i32 = 7 * 60;
pub const ARRIVE_STAY_FRAMES: u32 = 108000;
pub const GO_HOME_COUNT: u32 = 108001;
pub const GUEST_RESET_TIMER: i32 = 10800;
pub const BLOCK_NPC_CAP: u8 = 3;
pub const SET_NPC_MAX_RETAIL: usize = 5;
pub const SET_NPC_MAX_PC: usize = 15;

/// Scope rectangle: (x0, x1, z0, z1), half-open on the far edge.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Scope {
    pub x: (f32, f32),
    pub z: (f32, f32),
}

impl Scope {
    /// `aSNMgr_check_in_scope`.
    pub fn contains(&self, x: f32, z: f32) -> bool {
        x >= self.x.0 && x < self.x.1 && z >= self.z.0 && z < self.z.1
    }
}

/// `aSNMgr_renewal_set_scope`: (scope, guest scope) for the player's next
/// acre. `borderless` = PC `g_mPlib_wade_disabled` (one full acre each way).
pub fn scopes(next_block: (i32, i32), borderless: bool) -> (Scope, Scope) {
    let r = if borderless { BLOCK_SIZE } else { HALF_BLOCK };
    let edge = |b: i32| {
        let c = b as f32 * BLOCK_SIZE + HALF_BLOCK;
        (c - r, c + r)
    };
    let (x, z) = (edge(next_block.0), edge(next_block.1));
    let s = Scope { x, z };
    let g = Scope { x: (x.0 - UNIT, x.1 + UNIT), z: (z.0 - UNIT, z.1 + UNIT) };
    (s, g)
}

/// `aSNMgr_set_appear_info_regular` acre test (PC: within +-1 acre).
pub fn regular_appears_here(npc_block: (i32, i32), player_next: (i32, i32), borderless: bool) -> bool {
    let r = if borderless { 1 } else { 0 };
    (npc_block.0 - player_next.0).abs() <= r && (npc_block.1 - player_next.1).abs() <= r
}

/// Walk statuses that may appear as regular villagers.
pub fn regular_status_ok(s: u8) -> bool {
    matches!(s, status::WALKING | status::STAY_IN_BLOCK | status::ARRIVE)
}

/// `aSNMgr_set_appear_info_guest`: a WALKING villager in an acre directly
/// N/S/W/E of the player's next acre appears as a guest, entering from that
/// side. Returns (status, way).
pub fn guest_appear(walk_status: u8, npc_block: (i32, i32), player_next: (i32, i32)) -> Option<(u8, u8)> {
    if walk_status != status::WALKING {
        return None;
    }
    let (dx, dz) = (npc_block.0 - player_next.0, npc_block.1 - player_next.1);
    let way = match (dx, dz) {
        (0, -1) => WAY_DOWN,
        (0, 1) => WAY_UP,
        (-1, 0) => WAY_RIGHT,
        (1, 0) => WAY_LEFT,
        _ => return None,
    };
    Some((APPEAR_GUEST, way))
}

/// Gate side used by a guest for each way (`l_gate_direct`): UP->NORTH,
/// DOWN->SOUTH, LEFT->WEST, RIGHT->EAST (`mRF_DIRECT_*` values).
pub const GUEST_GATE_DIRECT: [u8; 4] = [0, 2, 1, 3];

/// Entry/exit appear status for a non-walker (`mNpcW_GetAppearStatusWay`).
pub const DEFAULT_APPEAR: u8 = APPEAR_REGULAR;

/// `aSNMgr_get_safe_sp_event_structure_4x4`: if (ux,uz) (the tent/car
/// unit) is near the current safe unit, move the safe unit beside and 4
/// units below the structure.
pub fn step_off_structure(safe: (i32, i32), structure: (i32, i32)) -> (i32, i32) {
    let (cx, cz) = safe;
    let (ux, uz) = structure;
    if ux >= 1 && ux <= 15 && ux - 1 <= cx && ux + 1 >= cx && uz >= 1 && uz <= 14 && uz - 1 <= cz && uz + 2 >= cz {
        let nx = if ux >= cx { ux - 1 } else { ux + 1 };
        (nx, uz + 4)
    } else {
        safe
    }
}

/// `aSNMgr_get_safe_utnum_3x3` probe order (dx, dz).
pub const SAFE_3X3: [(i32, i32); 9] = [(0, 0), (1, 0), (-1, 0), (0, 1), (1, 1), (-1, 1), (0, -1), (1, -1), (-1, -1)];

/// First safe unit around `safe` (units 1..14 only).
pub fn safe_3x3(safe: (i32, i32), is_safe: impl Fn(i32, i32) -> bool) -> Option<(i32, i32)> {
    SAFE_3X3
        .iter()
        .map(|&(dx, dz)| (safe.0 + dx, safe.1 + dz))
        .find(|&(x, z)| (1..15).contains(&x) && (1..15).contains(&z) && is_safe(x, z))
}

/// `aSNMgr_get_safe_utnum_in_block`: the `RANDOM(n)`-th safe unit in
/// row-major order (units 1..14).
pub fn safe_in_block(is_safe: impl Fn(i32, i32) -> bool, random: impl FnOnce(i32) -> i32) -> Option<(i32, i32)> {
    let cands: Vec<(i32, i32)> = (1..15).flat_map(|z| (1..15).map(move |x| (x, z))).filter(|&(x, z)| is_safe(x, z)).collect();
    if cands.is_empty() {
        return None;
    }
    cands.get(random(cands.len() as i32) as usize).copied()
}

/// `aSNMgr_set_to_block`: advance the "next acre" one step toward the goal
/// along the axis with the larger difference (x on ties).
pub fn step_to_block(to_block: (i32, i32), goal: (i32, i32)) -> (i32, i32) {
    let d = (goal.0 - to_block.0, goal.1 - to_block.1);
    let mut t = to_block;
    if d.0.abs() < d.1.abs() {
        t.1 += if d.1 > 0 { 1 } else { -1 };
    } else {
        t.0 += if d.0 > 0 { 1 } else { -1 };
    }
    t
}

/// `aSNMgr_walk_to_next_block` unit step. Returns (block, unit, changed
/// block). The caller only applies it when the new block is not the
/// player's current acre.
pub fn walk_step(block: (i32, i32), unit: (i32, i32), to_block: (i32, i32)) -> ((i32, i32), (i32, i32), bool) {
    let mut b = [block.0, block.1];
    let mut u = [unit.0 + (to_block.0 - block.0), unit.1 + (to_block.1 - block.1)];
    let mut changed = false;
    for i in 0..2 {
        if u[i] < 0 {
            b[i] -= 1;
            u[i] = 14;
            changed = true;
        } else if u[i] >= 16 {
            b[i] += 1;
            u[i] = 1;
            changed = true;
        }
    }
    ((b[0], b[1]), (u[0], u[1]), changed)
}

/// Per-acre villager count after an arrival (saturates at 14 on reaching
/// 15).
pub fn in_block_after_enter(count: u8) -> u8 {
    let c = count + 1;
    if c >= 15 {
        14
    } else {
        c
    }
}

/// `aSNMgr_check_into_block_npc_sum`: WALKING <-> STAY_IN_BLOCK by the
/// next acre's head count.
pub fn crowd_status(s: u8, next_acre_count: u8) -> u8 {
    if next_acre_count >= BLOCK_NPC_CAP {
        if s == status::WALKING {
            return status::STAY_IN_BLOCK;
        }
    } else if s == status::STAY_IN_BLOCK {
        return status::WALKING;
    }
    s
}

/// What `aSNMgr_go_back_home` does for an arrived walker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArriveStep {
    /// Keep waiting (counter incremented).
    Stay,
    /// Goal was home: give the walk slot to another villager.
    RotateWalker,
    /// Head home now (status WALKING, goal MY_HOME, counter reset).
    HeadHome,
}

pub fn arrive_step(count: u32, goal_is_my_home: bool) -> ArriveStep {
    if count > ARRIVE_STAY_FRAMES {
        if goal_is_my_home {
            ArriveStep::RotateWalker
        } else {
            ArriveStep::HeadHome
        }
    } else {
        ArriveStep::Stay
    }
}

/// `aSNMgr_set_go_home_status` candidate acres and the status each gives.
pub const GO_HOME_OFFSETS: [((i32, i32), u8); 5] =
    [((0, 0), status::ARRIVE), ((0, -1), status::WALKING), ((0, 1), status::WALKING), ((-1, 0), status::WALKING), ((1, 0), status::WALKING)];

/// Candidate acres for going home, in order, skipping `avoid` (event acre)
/// and the player's acre; `valid` = `mFI_BlockCheck`.
pub fn go_home_candidates(home: (i32, i32), avoid: (i32, i32), player: (i32, i32), valid: impl Fn(i32, i32) -> bool) -> Vec<((i32, i32), u8)> {
    GO_HOME_OFFSETS
        .iter()
        .map(|&((dx, dz), s)| ((home.0 + dx, home.1 + dz), s))
        .filter(|&(b, _)| valid(b.0, b.1) && b != avoid && b != player)
        .collect()
}

/// `aSNMgr_chk_arbeit_and_demo_and_halloween`: TRUE blocks spawning and
/// off-screen walking.
pub fn spawning_blocked(arbeit: bool, first_intro: bool, demo_active: bool, halloween: bool) -> bool {
    arbeit || first_intro || demo_active || halloween
}

/// `aSNMgr_check_move_npc_schedule`: only FIELD villagers outside walk.
pub fn may_walk(schedule_is_field: bool, is_home: bool) -> bool {
    schedule_is_field && !is_home
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scopes_and_appear() {
        let (s, g) = scopes((2, 3), false);
        assert_eq!(s.x, (1280.0, 1920.0));
        assert_eq!(g.x, (1240.0, 1960.0));
        assert!(s.contains(1280.0, 1920.0) && !s.contains(1920.0, 1920.0));
        let (pc, _) = scopes((2, 3), true);
        assert_eq!(pc.x, (960.0, 2240.0));
        assert!(regular_appears_here((3, 3), (2, 3), true) && !regular_appears_here((3, 3), (2, 3), false));
        assert_eq!(guest_appear(status::WALKING, (2, 2), (2, 3)), Some((APPEAR_GUEST, WAY_DOWN)));
        assert_eq!(guest_appear(status::ARRIVE, (2, 2), (2, 3)), None);
        assert_eq!(guest_appear(status::WALKING, (3, 2), (2, 3)), None);
    }

    #[test]
    fn placement() {
        assert_eq!(step_off_structure((5, 5), (5, 5)), (4, 9));
        assert_eq!(step_off_structure((10, 10), (5, 5)), (10, 10));
        assert_eq!(safe_3x3((5, 5), |x, _| x == 4), Some((4, 5)));
        assert_eq!(safe_in_block(|x, z| x == 3 && z > 10, |_| 1), Some((3, 12)));
        assert_eq!(safe_in_block(|_, _| false, |_| 0), None);
    }

    #[test]
    fn walking() {
        assert_eq!(step_to_block((2, 2), (2, 5)), (2, 3));
        assert_eq!(step_to_block((2, 2), (4, 3)), (3, 2));
        assert_eq!(walk_step((2, 2), (15, 7), (3, 2)), ((3, 2), (1, 7), true));
        assert_eq!(walk_step((2, 2), (0, 7), (1, 2)), ((1, 2), (14, 7), true));
        assert_eq!(walk_step((2, 2), (7, 7), (2, 3)), ((2, 2), (7, 8), false));
        assert_eq!(crowd_status(status::WALKING, 3), status::STAY_IN_BLOCK);
        assert_eq!(crowd_status(status::STAY_IN_BLOCK, 2), status::WALKING);
        assert_eq!(in_block_after_enter(14), 14);
        assert_eq!(arrive_step(108000, true), ArriveStep::Stay);
        assert_eq!(arrive_step(108001, true), ArriveStep::RotateWalker);
        assert_eq!(arrive_step(108001, false), ArriveStep::HeadHome);
    }

    #[test]
    fn going_home() {
        let c = go_home_candidates((3, 3), (3, 2), (2, 3), |x, z| (1..6).contains(&x) && (1..7).contains(&z));
        assert_eq!(c, vec![((3, 3), status::ARRIVE), ((3, 4), status::WALKING), ((4, 3), status::WALKING)]);
        assert!(spawning_blocked(false, false, false, true) && !spawning_blocked(false, false, false, false));
        assert!(may_walk(true, false) && !may_walk(true, true));
    }
}
