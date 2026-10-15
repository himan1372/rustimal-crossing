//! Villager walking between acres (`m_npc_walk.c`) and the schedule-slot
//! bookkeeping missing from `npc_ai.rs` (`m_npc_schedule.c`
//! `mNPS_get/set/reset_schedule_area`, island slot, all-area setup).
//!
//! Source-verified against ac-decomp `src/game/m_npc_walk.c`,
//! `include/m_npc_walk.h`, `src/game/m_npc_schedule.c` (primary) and
//! ACGC-PC-Port (secondary: `m_npc_walk.c` identical; the schedule file
//! only adds a `looks >= LOOKS_NUM` guard and tick-scaled forced timers).
//!
//! ## How it works
//! - `now_npc_max / 3` villagers (5 for a full town) are "walkers". Each has
//!   a goal picked from its personality's time table: the wishing-well acre,
//!   a random acre holding another villager's house, a random empty acre, or
//!   its own home acre (`MY_HOME`, the default when the current time slot
//!   has no goals).
//! - `DecideNpc` picks uniformly among existing villagers that are not
//!   already used and whose current slot has goals. When none are left, the
//!   used set is rebuilt from the current walkers and the pick is retried.
//! - The shrine and "home" goal acres are chosen once per walk init
//!   (`InitGoalBlockSource`). The "alone" acre is rolled per walker.
//! - The out-of-range fallback acre everywhere is (4, 3).
//!
//! ## Quirks (kept)
//! - `GetAloneBlock` draws `RANDOM(30 - villager_count)`, counting
//!   villagers, not occupied acres. When villagers share an acre, the last
//!   empty acres can never be chosen.
//! - `GetNearGate` tests `WEST || WEST` (decomp-marked BUG), so the east
//!   side compares the wrong axis. Its east/south distance is `16 - ut`, not
//!   `15 - ut`. Because `UT_BASE_NUM` is 16 the `difference < 16` test
//!   always passes, so the LAST gate on that side always wins.

pub const ANIMAL_NUM_MAX: usize = 15;
pub const WALK_MAX: usize = ANIMAL_NUM_MAX / 3;
pub const FG_BLOCK_X_NUM: i32 = 5;
pub const FG_BLOCK_Z_NUM: i32 = 6;
pub const FG_BLOCK_TOTAL_NUM: i32 = FG_BLOCK_X_NUM * FG_BLOCK_Z_NUM;
pub const BLOCK_X_NUM: i32 = 7;
pub const BLOCK_Z_NUM: i32 = 10;
pub const UT_NUM: i32 = 16;
pub const FALLBACK_BLOCK: (u8, u8) = (4, 3);

/// `mNpcW_GOAL_*`.
pub mod goal {
    pub const SHRINE: u8 = 0;
    pub const HOME: u8 = 1;
    pub const ALONE: u8 = 2;
    pub const MY_HOME: u8 = 3;
    pub const NONE: u8 = 4;
}

/// `mNpcW_INFO_STATUS_*`.
pub mod status {
    pub const NONE: u8 = 0;
    pub const WALKING: u8 = 1;
    pub const STAY_IN_BLOCK: u8 = 2;
    pub const ARRIVE: u8 = 3;
    pub const STATUS_4: u8 = 4;
    pub const UNSET: u8 = 5;
}

/// `mNpcW_APPEAR_STATUS_*` / `mNpcW_APPEAR_WAY_*`.
pub const APPEAR_REGULAR: u8 = 0;
pub const APPEAR_GUEST: u8 = 1;
pub const APPEAR_STATUS_NONE: u8 = 2;
pub const WAY_UP: u8 = 0;
pub const WAY_DOWN: u8 = 1;
pub const WAY_LEFT: u8 = 2;
pub const WAY_RIGHT: u8 = 3;
pub const WAY_NONE: u8 = 4;

const fn t(h: i32, m: i32) -> i32 {
    h * 3600 + m * 60
}

/// One time slot of a goal table: goals valid until `end_time` (seconds).
pub type GoalSlot = (&'static [u8], i32);

use goal::{ALONE as A, HOME as H, SHRINE as S};

/// Per-looks goal tables (girl, ko_girl, boy, sport_man, grim_man,
/// naniwa_lady), verbatim from `l_*_goal_data`.
pub const GOAL_TABLES: [&[GoalSlot]; 6] = [
    &[(&[], t(6, 0)), (&[S, H, A], t(12, 0)), (&[], t(13, 0)), (&[S, H], t(18, 30)), (&[], t(24, 0))],
    &[(&[S, H], t(24, 0))],
    &[(&[], t(9, 0)), (&[A], t(12, 0)), (&[], t(14, 0)), (&[S, H], t(19, 30)), (&[], t(24, 0))],
    &[(&[], t(6, 30)), (&[S, H, A, A], t(12, 0)), (&[], t(12, 30)), (&[S, S, H, H, A], t(23, 0)), (&[], t(24, 0))],
    &[(&[S, S, S, A, A, A, A, A, A, A], t(24, 0))],
    &[
        (&[S, S, S, S, S, S, S, A, A, A], t(1, 30)),
        (&[], t(10, 0)),
        (&[H], t(13, 0)),
        (&[], t(14, 0)),
        (&[S, S, H, H, A], t(21, 0)),
        (&[], t(22, 0)),
        (&[S, S, S, S, S, S, S, A, A, A], t(24, 0)),
    ],
];

/// `mNpcW_GetGoalDataInfo`: the first slot whose end time is after
/// `now_sec`, else the last slot.
pub fn goal_slot(looks: usize, now_sec: i32) -> &'static [u8] {
    let table = GOAL_TABLES[looks.min(5)];
    table.iter().find(|s| s.1 > now_sec).unwrap_or(&table[table.len() - 1]).0
}

/// One villager as the walk code sees it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WalkAnimal {
    pub exists: bool,
    pub looks: u8,
    /// Home acre (1-based block numbers; 0xFF = none).
    pub home_block: (u8, u8),
    /// Acre the villager currently stands in, if any.
    pub now_block: Option<(i32, i32)>,
}

/// `mNpcW_info_c` (the persistent part).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WalkInfo {
    /// Animal index, or -1 for a free slot.
    pub idx: i32,
    pub status: u8,
    pub goal_type: u8,
    pub goal_block: (u8, u8),
    pub appear_status: u8,
    pub appear_way: u8,
}

impl Default for WalkInfo {
    /// `mNpcW_ClearNpcWalkInfo`.
    fn default() -> Self {
        Self {
            idx: -1,
            status: status::UNSET,
            goal_type: goal::NONE,
            goal_block: (0, 0),
            appear_status: APPEAR_STATUS_NONE,
            appear_way: WAY_NONE,
        }
    }
}

/// `mNpcW_DecideNpc`: `random(n)` = `RANDOM(n)`. Returns the animal index.
pub fn decide_npc(animals: &[WalkAnimal; ANIMAL_NUM_MAX], used: u16, now_sec: i32, random: &mut impl FnMut(i32) -> i32) -> Option<usize> {
    let mut candidates = Vec::new();
    for (i, a) in animals.iter().enumerate() {
        if a.exists && !goal_slot(a.looks as usize, now_sec).is_empty() && (used >> i) & 1 == 0 {
            candidates.push(i);
        }
    }
    if candidates.is_empty() {
        return None;
    }
    let pick = random(candidates.len() as i32) as usize;
    candidates.get(pick).copied()
}

/// The walk manager (`mNpc_walk_c` + the file-static goal sources).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NpcWalk {
    pub info: [WalkInfo; WALK_MAX],
    pub used: u16,
    pub shrine_block: (u8, u8),
    pub home_block: (u8, u8),
    pub arrive_stay_count: [u32; ANIMAL_NUM_MAX],
}

impl NpcWalk {
    /// `mNpcW_SetNpcWalkInfo`.
    fn set_info(slot: &mut WalkInfo, animal: &WalkAnimal, idx: usize, now_sec: i32, random: &mut impl FnMut(i32) -> i32) {
        slot.idx = idx as i32;
        let goals = goal_slot(animal.looks as usize, now_sec);
        slot.goal_type = if goals.is_empty() { goal::MY_HOME } else { goals[random(goals.len() as i32) as usize] };
        slot.status = status::WALKING;
    }

    /// `mNpcW_InitNpcWalk` (+ `InitGoalBlockSource`). `shrine` = the shrine
    /// acre if the town has one.
    pub fn init(&mut self, animals: &[WalkAnimal; ANIMAL_NUM_MAX], now_npc_max: usize, now_sec: i32, shrine: Option<(u8, u8)>, random: &mut impl FnMut(i32) -> i32) {
        *self = Self::default();
        for i in 0..(now_npc_max / 3).min(WALK_MAX) {
            let Some(idx) = decide_npc(animals, self.used, now_sec, random) else { break };
            Self::set_info(&mut self.info[i], &animals[idx], idx, now_sec, random);
            self.used |= 1 << idx;
        }
        self.shrine_block = shrine.unwrap_or(FALLBACK_BLOCK);
        self.home_block = self.home_block_source(animals, random);
    }

    /// `mNpcW_SetHomeBlockSource`: a random villager home acre that no
    /// current walker lives in, else an acre in a row/column with no homes.
    fn home_block_source(&self, animals: &[WalkAnimal; ANIMAL_NUM_MAX], random: &mut impl FnMut(i32) -> i32) -> (u8, u8) {
        let walker_home = |b: (u8, u8)| {
            self.info.iter().any(|w| w.idx >= 0 && animals[w.idx as usize].home_block == b)
        };
        let cands: Vec<(u8, u8)> = animals
            .iter()
            .filter(|a| a.exists && a.home_block.0 != 0xFF && a.home_block.1 != 0xFF && !walker_home(a.home_block))
            .map(|a| a.home_block)
            .collect();
        let mut b = (0u8, 0u8);
        if !cands.is_empty() {
            b = cands[random(cands.len() as i32) as usize];
        }
        if b.0 == 0 || b.1 == 0 {
            b = block_except_home(animals, random);
        }
        clamp_goal(b)
    }

    /// `mNpcW_SetGoalBlock`.
    pub fn goal_block(&self, slot: &WalkInfo, animals: &[WalkAnimal; ANIMAL_NUM_MAX], random: &mut impl FnMut(i32) -> i32) -> (u8, u8) {
        let b = match slot.goal_type {
            goal::SHRINE => self.shrine_block,
            goal::HOME => self.home_block,
            goal::ALONE => alone_block(animals, random),
            goal::MY_HOME => animals[slot.idx.max(0) as usize].home_block,
            _ => FALLBACK_BLOCK,
        };
        clamp_goal(b)
    }

    /// `mNpcW_ChangeNpcWalk`: free `slot` and hand a walk to another
    /// villager. Returns the chosen animal index.
    pub fn change(&mut self, slot: usize, animals: &[WalkAnimal; ANIMAL_NUM_MAX], now_sec: i32, random: &mut impl FnMut(i32) -> i32) -> Option<usize> {
        self.info[slot] = WalkInfo::default();
        let free = self.info.iter().position(|w| w.idx < 0)?;
        let mut pick = decide_npc(animals, self.used, now_sec, random);
        if pick.is_none() {
            self.used = self.info.iter().filter(|w| (0..15).contains(&w.idx)).fold(0, |u, w| u | 1 << w.idx);
            pick = decide_npc(animals, self.used, now_sec, random);
        }
        let idx = pick?;
        let mut info = WalkInfo::default();
        Self::set_info(&mut info, &animals[idx], idx, now_sec, random);
        info.goal_block = self.goal_block(&info, animals, random);
        self.info[free] = info;
        self.used |= 1 << idx;
        Some(idx)
    }
}

fn clamp_goal(b: (u8, u8)) -> (u8, u8) {
    let (x, z) = (b.0 as i32, b.1 as i32);
    if x <= 0 || x >= BLOCK_X_NUM || z <= 0 || z >= BLOCK_Z_NUM - 2 {
        FALLBACK_BLOCK
    } else {
        b
    }
}

/// `mNpcW_GetAloneBlock` (with the villager-count quirk). Default D-3.
pub fn alone_block(animals: &[WalkAnimal; ANIMAL_NUM_MAX], random: &mut impl FnMut(i32) -> i32) -> (u8, u8) {
    let mut field = [0u8; (FG_BLOCK_Z_NUM + 2) as usize];
    let mut count = 0;
    for a in animals.iter().filter(|a| a.exists) {
        if let Some((bx, bz)) = a.now_block {
            count += 1;
            if (0..8).contains(&bz) && (0..8).contains(&bx) {
                field[bz as usize] |= 1 << bx;
            }
        }
    }
    let mut selected = random(FG_BLOCK_TOTAL_NUM - count);
    for bz in 1..(BLOCK_Z_NUM - 3) {
        for bx in 1..(BLOCK_X_NUM - 1) {
            if (field[bz as usize] >> bx) & 1 == 0 {
                if selected <= 0 {
                    return (bx as u8, bz as u8);
                }
                selected -= 1;
            }
        }
    }
    (3, 4)
}

/// `mNpcW_GetBlockXZNumExceptHome`: a column and a row containing no
/// villager home, chosen independently.
pub fn block_except_home(animals: &[WalkAnimal; ANIMAL_NUM_MAX], random: &mut impl FnMut(i32) -> i32) -> (u8, u8) {
    let (mut px, mut pz) = (0xFFu8, 0xFFu8);
    let (mut xn, mut zn) = (0, 0);
    for a in animals.iter().filter(|a| a.exists) {
        let hx = a.home_block.0.wrapping_sub(1);
        let hz = a.home_block.1.wrapping_sub(1);
        if hx < 8 && (px >> hx) & 1 == 1 {
            xn += 1;
            px &= !(1 << hx);
        }
        if hz < 8 && (pz >> hz) & 1 == 1 {
            zn += 1;
            pz &= !(1 << hz);
        }
    }
    let pick = |bits: u8, n: i32, sel: i32| {
        let mut s = sel;
        let mut i = 0;
        while i < n {
            if (bits >> i) & 1 == 1 {
                if s <= 0 {
                    break;
                }
                s -= 1;
            }
            i += 1;
        }
        i + 1
    };
    let sx = random(FG_BLOCK_X_NUM - xn);
    let sz = random(FG_BLOCK_Z_NUM - zn);
    let x = pick(px, FG_BLOCK_X_NUM, sx);
    let z = pick(pz, FG_BLOCK_Z_NUM, sz);
    if x <= 0 || x >= BLOCK_X_NUM || z <= 0 || z >= BLOCK_Z_NUM - 2 {
        (4, 3)
    } else {
        (x as u8, z as u8)
    }
}

/// `mRF_DIRECT_*`.
pub const DIRECT_NORTH: u8 = 0;
pub const DIRECT_WEST: u8 = 1;
pub const DIRECT_SOUTH: u8 = 2;
pub const DIRECT_EAST: u8 = 3;

/// `mNpcW_GetNearGate` side choice: the nearest edge of unit (ux, uz).
pub fn near_gate_side(ux: i32, uz: i32) -> u8 {
    let tx = if ux < UT_NUM / 2 { ux } else { UT_NUM - ux };
    let tz = if uz < UT_NUM / 2 { uz } else { UT_NUM - uz };
    if tx < tz {
        if ux < UT_NUM / 2 {
            DIRECT_WEST
        } else {
            DIRECT_EAST
        }
    } else if uz < UT_NUM / 2 {
        DIRECT_NORTH
    } else {
        DIRECT_SOUTH
    }
}

/// `mNpcW_GetNearGate` with the side's gate list (`ut1` packed z<<4|x).
/// Retail keeps the last gate whose difference is < 16, which is every
/// gate. `bugfix` = the decomp's BUGFIXES axis choice (it doesn't change
/// the result, since the threshold always passes).
pub fn near_gate(ux: i32, uz: i32, gates_ut1: &[i32], bugfix: bool) -> Option<(i32, i32)> {
    if !(0..UT_NUM).contains(&ux) || !(0..UT_NUM).contains(&uz) {
        return None;
    }
    let side = near_gate_side(ux, uz);
    let horizontal = side == DIRECT_WEST || (bugfix && side == DIRECT_EAST);
    let mut res = None;
    for &g in gates_ut1 {
        let (x, z) = (g & 0xF, g >> 4);
        let diff = if horizontal { (uz - z).abs() } else { (ux - x).abs() };
        if diff < UT_NUM {
            res = Some((x, z));
        }
    }
    res
}

/// `mNpcW_GetAppearStatusWay` for a villager not in the walk table.
pub const NOT_WALKING_APPEAR: (u8, u8) = (APPEAR_REGULAR, WAY_NONE);

// ---------------------------------------------------------------------------
// Schedule slots (m_npc_schedule.c)
// ---------------------------------------------------------------------------

/// `SCHEDULE_NUM` = 15 town slots + 1 islander slot (index 15).
pub const SCHEDULE_NUM: usize = ANIMAL_NUM_MAX + 1;
pub const ISLAND_SLOT: usize = ANIMAL_NUM_MAX;

/// Which villager (by animal index) owns each schedule slot.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SchedulePool {
    pub owner: [Option<u8>; SCHEDULE_NUM],
}

impl SchedulePool {
    /// `mNPS_set_schedule_area`: first free slot (the island slot can be
    /// taken too, since the search covers all 16). The PC port skips looks
    /// >= 6.
    pub fn set(&mut self, animal: u8, looks: u8, pc: bool) -> Option<usize> {
        if pc && looks >= 6 {
            return None;
        }
        let i = self.owner.iter().position(|o| o.is_none())?;
        self.owner[i] = Some(animal);
        Some(i)
    }

    /// `mNPS_set_island_schedule_area`: only if the island slot is free.
    pub fn set_island(&mut self, animal: u8) -> bool {
        if self.owner[ISLAND_SLOT].is_some() {
            return false;
        }
        self.owner[ISLAND_SLOT] = Some(animal);
        true
    }

    /// `mNPS_reset_schedule_area`.
    pub fn reset(&mut self, animal: u8) {
        if let Some(o) = self.owner.iter_mut().find(|o| **o == Some(animal)) {
            *o = None;
        }
    }
}

/// `mNPS_schedule_manager`: on the first-job day or during Halloween every
/// town villager's `current_type` is forced to FIELD (`saved_type` is not
/// updated); the islander always runs its normal schedule.
pub fn town_forced_field(first_job: bool, halloween: bool) -> bool {
    first_job || halloween
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seq(v: Vec<i32>) -> impl FnMut(i32) -> i32 {
        let mut it = v.into_iter();
        move |n| it.next().unwrap_or(0).min(n - 1).max(0)
    }

    #[test]
    fn goal_slots() {
        assert!(goal_slot(0, t(5, 0)).is_empty());
        assert_eq!(goal_slot(0, t(11, 0)), &[S, H, A]);
        assert_eq!(goal_slot(5, t(0, 30)).len(), 10);
        assert_eq!(goal_slot(5, t(23, 0)).len(), 10);
        assert_eq!(GOAL_TABLES.len(), 6);
    }

    #[test]
    fn walkers_and_goals() {
        let mut animals = [WalkAnimal::default(); ANIMAL_NUM_MAX];
        for (i, a) in animals.iter_mut().enumerate().take(6) {
            *a = WalkAnimal { exists: true, looks: 1, home_block: (1 + (i as u8 % 5), 1 + (i as u8 / 5)), now_block: Some((1, 1)) };
        }
        let mut w = NpcWalk::default();
        w.init(&animals, 6, t(12, 0), Some((3, 2)), &mut seq(vec![0, 0, 0, 0, 0, 0]));
        assert_eq!(w.info.iter().filter(|i| i.idx >= 0).count(), 2);
        assert_eq!(w.shrine_block, (3, 2));
        assert_eq!(w.used.count_ones(), 2);
        let next = w.change(0, &animals, t(12, 0), &mut seq(vec![0, 0, 0, 0]));
        assert!(next.is_some());
    }

    #[test]
    fn alone_and_gate_quirks() {
        let mut animals = [WalkAnimal::default(); ANIMAL_NUM_MAX];
        animals[0] = WalkAnimal { exists: true, looks: 0, home_block: (1, 1), now_block: Some((1, 1)) };
        assert_eq!(alone_block(&animals, &mut seq(vec![0])), (2, 1));
        assert_eq!(near_gate_side(1, 8), DIRECT_WEST);
        assert_eq!(near_gate_side(14, 8), DIRECT_EAST);
        // Every gate passes the < 16 test, so the last one wins.
        assert_eq!(near_gate(1, 8, &[0x30, 0xC0], false), Some((0, 12)));
        assert_eq!(near_gate(16, 0, &[0x30], false), None);
    }

    #[test]
    fn schedule_pool() {
        let mut p = SchedulePool::default();
        assert_eq!(p.set(3, 0, false), Some(0));
        assert_eq!(p.set(4, 7, true), None);
        assert!(p.set_island(9) && !p.set_island(10));
        p.reset(3);
        assert_eq!(p.owner[0], None);
        assert!(town_forced_field(false, true));
    }

    #[test]
    fn goal_and_status_constants() {
        // Goal ids are a dense 0-4 enum.
        assert_eq!(goal::SHRINE, 0);
        assert_eq!(goal::HOME, 1);
        assert_eq!(goal::ALONE, 2);
        assert_eq!(goal::MY_HOME, 3);
        assert_eq!(goal::NONE, 4);
        // Status ids.
        assert_eq!(status::NONE, 0);
        assert_eq!(status::WALKING, 1);
        assert_eq!(status::STAY_IN_BLOCK, 2);
        assert_eq!(status::ARRIVE, 3);
        assert_eq!(status::UNSET, 5);
        // Appear status/way vocabulary.
        assert_eq!(APPEAR_REGULAR, 0);
        assert_eq!(APPEAR_GUEST, 1);
        assert_eq!(APPEAR_STATUS_NONE, 2);
        assert_eq!(WAY_UP, 0);
        assert_eq!(WAY_DOWN, 1);
        assert_eq!(WAY_LEFT, 2);
        assert_eq!(WAY_RIGHT, 3);
        assert_eq!(WAY_NONE, 4);
        // Five walkers for a full town of 15.
        assert_eq!(WALK_MAX, 5);
        // Out-of-range fallback acre.
        assert_eq!(FALLBACK_BLOCK, (4, 3));
    }

    #[test]
    fn decide_npc_and_pool_edges() {
        // No candidates: everyone used or no goals at this hour.
        let mut animals = [WalkAnimal::default(); ANIMAL_NUM_MAX];
        animals[0] = WalkAnimal { exists: true, looks: 0, home_block: (1, 1), now_block: Some((1, 1)) };
        // looks 0 at 05:00 has no goals (first slot empty).
        assert_eq!(decide_npc(&animals, 0, t(5, 0), &mut seq(vec![0])), None);
        // All used -> None.
        assert_eq!(decide_npc(&animals, 0xFFFF, t(12, 0), &mut seq(vec![0])), None);
        // Non-existent animals are skipped.
        let empty = [WalkAnimal::default(); ANIMAL_NUM_MAX];
        assert_eq!(decide_npc(&empty, 0, t(12, 0), &mut seq(vec![0])), None);
        // Schedule pool: full pool returns None; reset of unknown is a no-op.
        let mut p = SchedulePool::default();
        for i in 0..SCHEDULE_NUM as u8 {
            assert!(p.set(i, 0, false).is_some());
        }
        assert_eq!(p.set(99, 0, false), None);
        p.reset(200); // unknown animal: no panic, no change
        assert!(p.owner.iter().all(|o| o.is_some()));
        // Town forced-field gate.
        assert!(town_forced_field(true, false));
        assert!(town_forced_field(false, true));
        assert!(!town_forced_field(false, false));
        // Goal clamp: out-of-range blocks fall back to (4,3).
        let w = NpcWalk::default();
        let slot = WalkInfo { goal_type: 99, ..WalkInfo::default() };
        assert_eq!(w.goal_block(&slot, &animals, &mut seq(vec![0])), FALLBACK_BLOCK);
    }

}
