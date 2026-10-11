//! Event NPC placement: which villagers join a town event and where every
//! event NPC / structure stands (`m_event_map_npc.c`).
//!
//! Source-verified against ac-decomp `src/game/m_event_map_npc.c`,
//! `src/game/m_event_map_npc_data.c_inc`, `include/m_event_map_npc.h`
//! (primary) and the ACGC-PC-Port copies (secondary, 0-line diff).
//! Event ids checked by compiling `enum event_table` from
//! `include/m_event.h` with MSYS2 mingw32 gcc; actor names resolved from
//! `include/m_name_table.h` by `gen_evmn.py` (the tables at the bottom of
//! this file are generated, not hand-copied).
//!
//! ## Model
//! Seventeen map types (`mEvMN_TYPE_*`) map 1:1 to `mEv_EVENT_*` ids.
//! Each has an `mEvMN_data_c`:
//! - `normal_npc_max`: villagers pulled into the event (4 or 5; 1 for
//!   Kamakura). Their animal indices live in the event save area
//!   (`aSNMgr_event_save_c::animal_idx`, 0xFF = empty).
//! - `npc_max`: total placed actors (villager stand-ins, special NPCs and
//!   structures), each with a unit position inside the event block.
//! - `block_kind`: `SHRINE` (wishing-well acre), `POOL` (pond acre;
//!   seven layouts, picked by `mFI_GetPuleIdx()` = pond block type - 69)
//!   or `NONE` (Kamakura).
//! - `are_npc_flags` / `cloth`: bit i set means slot i is a villager
//!   stand-in (`SP_NPC_EV_*_n` drawn with the villager's model/texture);
//!   `cloth` (if not EMPTY_NO) is the event shirt, +1 for male villagers
//!   (looks lazy / jock / cranky).
//!
//! Halloween has no map (`NULL`): its villagers are registered as
//! `SP_NPC_EV_HALLOWEEN_0 + free event-npc slot` instead.
//!
//! | type | event (mEv id) | block | villagers | actors |
//! |---|---|---|---|---|
//! | 0 | Fireworks Show (32) | pond x7 | 4 | 7 (incl. stall) |
//! | 1 | Cherry Blossom Festival (20) | well | 5 | 8 |
//! | 2-5 | Sports Fair foot race / aerobics / ball toss / tug of war (15/13/12/14) | well | 5/4/5/5 | 6/7/10/6 |
//! | 6 | Halloween (49) | - | - | - |
//! | 7 | New Year's Day (1) | well | 4 | 8 |
//! | 8-9 | Fishing Tourney (29/54) | pond x7 | 4 | 7 |
//! | 10 | Morning Aerobics (35) | well | 4 | 7 |
//! | 11 | Harvest Moon Festival (43) | pond x7 | 4 | 5 |
//! | 12 | Harvest Festival (56) | well | 5 | 9 |
//! | 13 | New Year's Eve Countdown (64) | pond x7 | 4 | 8 |
//! | 14 | Kamakura (3) | none | 1 | 1 |
//! | 15 | Groundhog Day (7) | well | 4 | 6 |
//! | 16 | Meteor Shower (37) | pond x7 | 4 | 5 |
//!
//! ## Villager selection
//! `mEvMN_GetNpcIdxRandom` (used when the event save area is created):
//! candidates = occupied animal slots. If there are more candidates than
//! needed and the player lives in this town, villagers with no memory of
//! the player (`mNpc_GetAnimalMemoryIdx == -1`) are dropped one at a time
//! in slot order until the surplus is gone. Then `min(max, count)`
//! picks are made with `RANDOM(count)` over the remaining set bits.
//!
//! `mEvMN_SetNpcJointEvRandom` (on reload) first frees slots whose animal
//! moved out (`mEvMN_ClearRemoveNpcJoint`), then refills only `0xFF` slots
//! with the **lowest** remaining candidate. It still calls
//! `RANDOM(count)` and discards the result (decomp `@cleanup`), which
//! advances the RNG; preserved.
//!
//! ## Quirks preserved / noted
//! - `mEvMN_GetJointEventRandomNpc` indexes `animals[animal_idx[RANDOM(n)]]`
//!   without checking for 0xFF: with fewer villagers than
//!   `normal_npc_max` it reads past the 15-entry array (garbage npc id).
//!   The Rust version returns `None` instead of reading out of bounds.
//! - With `block_kind == POOL` and no pond block (`mFI_GetPuleIdx() == -1`)
//!   C indexes `actor_info[-1]`; Rust returns `None`.
//! - `mEvMN_CheckLapPlayer`: 2 if the unit is inside a structure's area,
//!   1 if within the 3x3 units around a placed NPC, else 0.

use std::ffi::c_int;

/// Number of map types (`mEvMN_TYPE_NUM`).
pub const TYPE_NUM: usize = 17;
/// `ANIMAL_NUM_MAX`.
pub const ANIMAL_NUM_MAX: usize = 15;
/// Empty slot marker in `animal_idx`.
pub const NO_ANIMAL: u8 = 0xFF;
/// `SP_NPC_EV_HALLOWEEN_0` (`SP_NPC_START + 32`).
pub const SP_NPC_EV_HALLOWEEN_0: u16 = 0xD020;
/// `ITEM_NAME_GET_TYPE` values used here.
pub const NAME_TYPE_STRUCT: u8 = 5;
pub const NAME_TYPE_NPC: u8 = 14;

/// `mEvMN_TYPE_*` -> `mEv_EVENT_*` (`l_event_map_type_table`).
pub const EVENT_MAP_TYPE_TABLE: [i32; TYPE_NUM] = [
    32, // FIREWORKS_SHOW
    20, // CHERRY_BLOSSOM_FESTIVAL
    15, // SPORTS_FAIR_FOOT_RACE
    13, // SPORTS_FAIR_AEROBICS
    12, // SPORTS_FAIR_BALL_TOSS
    14, // SPORTS_FAIR_TUG_OF_WAR
    49, // HALLOWEEN
    1,  // NEW_YEARS_DAY
    29, // FISHING_TOURNEY_1
    54, // FISHING_TOURNEY_2
    35, // MORNING_AEROBICS
    43, // HARVEST_MOON_FESTIVAL
    56, // HARVEST_FESTIVAL
    64, // NEW_YEARS_EVE_COUNTDOWN
    3,  // KAMAKURA
    7,  // GROUNDHOG_DAY
    37, // METEOR_SHOWER
];

/// Map type of Halloween (the only one with custom naming).
pub const TYPE_HALLOWEEN: usize = 6;

/// `mEvMN_actor_pos_c`: unit inside the event block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActorPos {
    pub ux: u8,
    pub uz: u8,
}

impl ActorPos {
    pub const fn new(ux: u8, uz: u8) -> Self {
        Self { ux, uz }
    }
}

/// `mEvMN_actor_info_c`.
#[derive(Debug)]
pub struct ActorInfo {
    pub names: &'static [u16],
    pub positions: &'static [ActorPos],
}

/// `mEvMN_npc_data_c`.
#[derive(Debug)]
pub struct NpcData {
    pub are_npc_flags: u16,
    pub cloth: u16,
}

/// `mEvMN_data_c`.
#[derive(Debug)]
pub struct EventMapData {
    pub get_npc_idx_type: u8,
    pub normal_npc_max: u8,
    pub map_max: u8,
    pub npc_max: u8,
    pub actor_info: &'static [&'static ActorInfo],
    pub npc_data: &'static NpcData,
    pub block_kind: u32,
}

#[inline]
const fn name_type(name: u16) -> u8 {
    ((name & 0xF000) >> 12) as u8
}

/// `mEvMN_GetMapIdx_sub`.
pub fn map_idx_sub(event_type: i32) -> Option<usize> {
    EVENT_MAP_TYPE_TABLE.iter().position(|&e| e == event_type)
}

/// `mEvMN_GetMapIdx`: like `_sub` but `None` for map-less events.
pub fn map_idx(event_type: i32) -> Option<usize> {
    map_idx_sub(event_type).filter(|&i| EVENT_SET_TABLE[i].is_some())
}

/// `mEvMN_GetEventTypeMap`: first active event in table order.
pub fn event_type_map(mut is_active: impl FnMut(i32) -> bool) -> Option<i32> {
    EVENT_MAP_TYPE_TABLE.iter().copied().find(|&e| is_active(e))
}

fn data(map_type: usize) -> Option<&'static EventMapData> {
    EVENT_SET_TABLE.get(map_type).copied().flatten()
}

/// `mEvMN_GetEventBlockKind`.
pub fn event_block_kind(map_type: usize) -> u32 {
    data(map_type).map_or(BLOCKKIND_NONE, |d| d.block_kind)
}

/// `mEvMN_GetJointAllNpcMax`.
pub fn joint_all_npc_max(map_type: usize) -> usize {
    data(map_type).map_or(0, |d| d.npc_max as usize)
}

/// `mEvMN_GetJointNpcMax`.
pub fn joint_npc_max(map_type: usize) -> usize {
    data(map_type).map_or(0, |d| d.normal_npc_max as usize)
}

/// `mEvMN_GetDataBaseIdx` + `actor_info[base]`; `pule_idx` is
/// `mFI_GetPuleIdx()` and is only consulted for pond events.
fn actor_info(d: &EventMapData, pule_idx: i32) -> Option<&'static ActorInfo> {
    let base = if d.block_kind == BLOCKKIND_POOL { pule_idx } else { 0 };
    usize::try_from(base).ok().and_then(|b| d.actor_info.get(b).copied())
}

/// `mEvMN_GetEventSetUtInBlockMapIdx`: unit of actor `idx` by map type.
pub fn set_ut_in_block_map_idx(map_type: usize, pule_idx: i32, idx: usize) -> Option<ActorPos> {
    let d = data(map_type)?;
    actor_info(d, pule_idx)?.positions.get(idx).copied()
}

/// `mEvMN_GetEventSetUtInBlock`: (actor name, unit) by event id.
pub fn set_ut_in_block(event_type: i32, pule_idx: i32, idx: usize) -> Option<(u16, ActorPos)> {
    let d = data(map_idx(event_type)?)?;
    let info = actor_info(d, pule_idx)?;
    Some((*info.names.get(idx)?, *info.positions.get(idx)?))
}

/// Per-villager facts the selection routines need.
/// Bit i of `occupied`: `mNpc_CheckFreeAnimalPersonalID(&animals[i].id)
/// == FALSE`. Bit i of `knows_player`: `mNpc_GetAnimalMemoryIdx(player)
/// != -1` for animal i.
#[derive(Clone, Copy, Debug, Default)]
pub struct AnimalMasks {
    pub occupied: u16,
    pub knows_player: u16,
    /// `mLd_PlayerManKindCheck()`: player is a visitor from another town.
    pub player_is_foreign: bool,
}

fn drop_strangers(mut bits: u16, mut surplus: i32, m: &AnimalMasks) -> u16 {
    if surplus > 0 && !m.player_is_foreign {
        for i in 0..ANIMAL_NUM_MAX {
            if (bits >> i) & 1 == 1 && (m.knows_player >> i) & 1 == 0 {
                surplus -= 1;
                bits &= !(1 << i);
                if surplus <= 0 {
                    break;
                }
            }
        }
    }
    bits
}

/// `mEvMN_GetNpcIdxRandom`. Writes up to `out.len()` picks; untouched
/// entries keep their value. `random(n)` is `RANDOM(n)`.
pub fn npc_idx_random(out: &mut [u8], m: &AnimalMasks, mut random: impl FnMut(i32) -> i32) {
    let mut max = out.len() as i32;
    let mut bits = m.occupied & 0x7FFF;
    let surplus = bits.count_ones() as i32 - max;
    bits = drop_strangers(bits, surplus, m);
    let mut count = bits.count_ones() as i32;
    if count < max {
        max = count;
    }
    let mut k = 0;
    while count > 0 && max > 0 {
        if bits == 0 {
            break;
        }
        let mut idx = random(count);
        let mut picked = false;
        for i in 0..ANIMAL_NUM_MAX {
            if (bits >> i) & 1 == 1 {
                if idx <= 0 {
                    out[k] = i as u8;
                    count -= 1;
                    bits &= !(1 << i);
                    picked = true;
                    break;
                }
                idx -= 1;
            }
        }
        if !picked {
            break;
        }
        max -= 1;
        k += 1;
    }
}

/// `mEvMN_ClearRemoveNpcJoint`: `is_free(i)` is
/// `mNpc_CheckFreeAnimalInfo(&animals[i])`.
pub fn clear_removed_npcs(list: &mut [u8], mut is_free: impl FnMut(u8) -> bool) {
    for a in list.iter_mut() {
        if *a != NO_ANIMAL && (*a as usize) < ANIMAL_NUM_MAX && is_free(*a) {
            *a = NO_ANIMAL;
        }
    }
}

/// `mEvMN_SetNpcJointEvRandom`: refill empty slots deterministically
/// (lowest candidate first); `random` is called once per refilled slot
/// and its result ignored, like the original.
pub fn set_npc_joint_random(list: &mut [u8], m: &AnimalMasks, mut random: impl FnMut(i32) -> i32) {
    let max = list.len() as i32;
    let mut bits = m.occupied & 0x7FFF;
    let mut animal_count = bits.count_ones() as i32;
    let mut req_count = 0;
    for &a in list.iter() {
        if a != NO_ANIMAL && (a as usize) < ANIMAL_NUM_MAX {
            if (bits >> a) & 1 == 1 {
                animal_count -= 1;
            }
            bits &= !(1 << a);
        } else {
            req_count += 1;
        }
    }
    animal_count -= max;
    bits = drop_strangers(bits, animal_count, m);
    let mut count = bits.count_ones() as i32;
    if count < req_count {
        req_count = count;
    }
    if req_count > 0 {
        for a in list.iter_mut() {
            if bits == 0 || count <= 0 {
                break;
            }
            if *a == NO_ANIMAL {
                let _ = random(count);
                if let Some(j) = (0..ANIMAL_NUM_MAX).find(|&j| (bits >> j) & 1 == 1) {
                    *a = j as u8;
                    count -= 1;
                    bits &= !(1 << j);
                }
            }
        }
    }
}

/// `mNpc_RegistEventNpc(event_id, texture_id, anime_id, cloth)` request.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EventNpcRegistration {
    pub event_id: u16,
    pub texture_id: u16,
    pub anime_id: u16,
    pub cloth: u16,
}

/// `mNpc_GetLooks2Sex(looks) == mPr_SEX_MALE` (lazy, jock, cranky).
pub fn looks_is_male(looks: u8) -> bool {
    matches!(looks, 2..=4)
}

/// `mEvMN_GetFireWorksNpcName` for an NPC not yet in the event-NPC list
/// (the caller handles the "already registered -> use its event_id" case).
pub fn fireworks_npc_registration(
    map_type: usize,
    pule_idx: i32,
    idx: usize,
    villager: u16,
    villager_looks: u8,
) -> Option<EventNpcRegistration> {
    let d = data(map_type)?;
    if idx >= d.normal_npc_max as usize {
        return None;
    }
    let info = actor_info(d, pule_idx)?;
    let spnpc = *info.names.get(idx)?;
    let (texture_id, cloth) = if (d.npc_data.are_npc_flags >> idx) & 1 == 1 {
        let mut cloth = d.npc_data.cloth;
        if cloth != EMPTY_NO && looks_is_male(villager_looks) {
            cloth += 1;
        }
        (villager, cloth)
    } else {
        (spnpc, EMPTY_NO)
    };
    Some(EventNpcRegistration { event_id: spnpc, texture_id, anime_id: villager, cloth })
}

/// `mEvMN_GetHalloweenNpcName` for an unregistered NPC given
/// `mNpc_GetFreeEventNpcIdx()`.
pub fn halloween_npc_registration(villager: u16, free_idx: i32) -> Option<EventNpcRegistration> {
    let free = u16::try_from(free_idx).ok()?;
    let id = SP_NPC_EV_HALLOWEEN_0 + free;
    Some(EventNpcRegistration { event_id: id, texture_id: id, anime_id: villager, cloth: EMPTY_NO })
}

/// `mEvNM_CheckJointEvent` core: is villager `animal_idx` (from
/// `mNpc_SearchAnimalinfo`) in the event's joint list?
pub fn is_joint_npc(npc_name: u16, animal_idx: Option<u8>, list: &[u8]) -> bool {
    if name_type(npc_name) != NAME_TYPE_NPC {
        return false;
    }
    match animal_idx {
        Some(a) => list.iter().any(|&x| x != NO_ANIMAL && x == a),
        None => false,
    }
}

/// `mEvMN_GetJointEventRandomNpc` index choice; `None` where C would read
/// past `animals[]` (empty slot).
pub fn joint_event_random_animal(list: &[u8], mut random: impl FnMut(i32) -> i32) -> Option<u8> {
    let sel = random(list.len() as i32);
    let a = *list.get(usize::try_from(sel).ok()?)?;
    ((a as usize) < ANIMAL_NUM_MAX).then_some(a)
}

/// `mEvMN_CheckLapPlayer`: 0 free, 1 near an event NPC, 2 inside an
/// event structure (`in_structure(name, ux, uz)` is
/// `mFI_CheckStructureArea(block_ux, block_uz, name, ux, uz)`).
pub fn check_lap_player(
    event_type: Option<i32>,
    pule_idx: i32,
    block_ux: i32,
    block_uz: i32,
    mut in_structure: impl FnMut(u16, i32, i32) -> bool,
) -> i32 {
    let Some(ev) = event_type else { return 0 };
    let Some(mt) = map_idx(ev) else { return 0 };
    for i in 0..joint_all_npc_max(mt) {
        let Some((name, p)) = set_ut_in_block(ev, pule_idx, i) else { continue };
        let (ux, uz) = (p.ux as i32, p.uz as i32);
        if name_type(name) == NAME_TYPE_STRUCT {
            if in_structure(name, ux, uz) {
                return 2;
            }
        } else if (block_ux - ux).abs() <= 1 && (block_uz - uz).abs() <= 1 {
            return 1;
        }
    }
    0
}

// ---------------------------------------------------------------------------
// C ABI
// ---------------------------------------------------------------------------

#[no_mangle]
pub extern "C" fn pc_evmn_get_map_idx(event_type: c_int) -> c_int {
    map_idx(event_type).map_or(-1, |i| i as c_int)
}

#[no_mangle]
pub extern "C" fn pc_evmn_get_event_block_kind(map_type: c_int) -> u32 {
    usize::try_from(map_type).map_or(BLOCKKIND_NONE, event_block_kind)
}

#[no_mangle]
pub extern "C" fn pc_evmn_get_joint_npc_max(map_type: c_int) -> c_int {
    usize::try_from(map_type).map_or(0, joint_npc_max) as c_int
}

/// `mEvMN_GetEventSetUtInBlock` with `mFI_GetPuleIdx()` passed in.
#[no_mangle]
pub extern "C" fn pc_evmn_get_event_set_ut_in_block(
    actor_name: *mut u16,
    block_ux: *mut c_int,
    block_uz: *mut c_int,
    event_type: c_int,
    idx: c_int,
    pule_idx: c_int,
) -> c_int {
    let Ok(i) = usize::try_from(idx) else { return 0 };
    let Some((name, p)) = set_ut_in_block(event_type, pule_idx, i) else { return 0 };
    unsafe {
        if let Some(n) = actor_name.as_mut() {
            *n = name;
        }
        if let Some(x) = block_ux.as_mut() {
            *x = p.ux as c_int;
        }
        if let Some(z) = block_uz.as_mut() {
            *z = p.uz as c_int;
        }
    }
    1
}

/// `mEvMN_GetNpcIdxRandom` with masks precomputed by C and `RANDOM`
/// supplied as a callback (`fqrand`-backed, to keep the RNG stream).
#[no_mangle]
pub extern "C" fn pc_evmn_get_npc_idx_random(
    out: *mut u8,
    max: c_int,
    occupied: u16,
    knows_player: u16,
    player_is_foreign: c_int,
    random: Option<extern "C" fn(c_int) -> c_int>,
) {
    let (Some(rnd), Ok(n)) = (random, usize::try_from(max)) else { return };
    if out.is_null() {
        return;
    }
    let list = unsafe { std::slice::from_raw_parts_mut(out, n) };
    let m = AnimalMasks { occupied, knows_player, player_is_foreign: player_is_foreign != 0 };
    npc_idx_random(list, &m, |c| rnd(c));
}

/// `mEvMN_SetNpcJointEvRandom` (after `mEvMN_ClearRemoveNpcJoint`).
#[no_mangle]
pub extern "C" fn pc_evmn_set_npc_joint_random(
    list: *mut u8,
    max: c_int,
    occupied: u16,
    knows_player: u16,
    player_is_foreign: c_int,
    random: Option<extern "C" fn(c_int) -> c_int>,
) {
    let (Some(rnd), Ok(n)) = (random, usize::try_from(max)) else { return };
    if list.is_null() {
        return;
    }
    let l = unsafe { std::slice::from_raw_parts_mut(list, n) };
    let m = AnimalMasks { occupied, knows_player, player_is_foreign: player_is_foreign != 0 };
    set_npc_joint_random(l, &m, |c| rnd(c));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_idx_matches_table() {
        assert_eq!(map_idx(32), Some(0));
        assert_eq!(map_idx(49), None); // Halloween has no map
        assert_eq!(map_idx_sub(49), Some(TYPE_HALLOWEEN));
        assert_eq!(map_idx(37), Some(16));
        assert_eq!(map_idx(999), None);
    }

    #[test]
    fn tables_are_consistent() {
        for d in EVENT_SET_TABLE.iter().flatten() {
            assert_eq!(d.actor_info.len(), d.map_max as usize);
            for info in d.actor_info {
                assert_eq!(info.names.len(), d.npc_max as usize);
                assert_eq!(info.positions.len(), d.npc_max as usize);
            }
        }
    }

    #[test]
    fn pond_layout_uses_pule_idx() {
        // Fishing tourney pond layout 2 places the angler at (5, 13).
        assert_eq!(set_ut_in_block(29, 1, 5), Some((SP_NPC_ANGLER, ActorPos::new(5, 13))));
        assert_eq!(set_ut_in_block(29, -1, 5), None);
        // New Year's Day is a well event: pule idx ignored.
        assert_eq!(set_ut_in_block(1, -1, 6), Some((NEWYEAR_TABLE, ActorPos::new(3, 3))));
    }

    #[test]
    fn fireworks_cloth_male_variant() {
        // Fishing tourney: villager stand-ins wear ITM_CLOTH022, +1 if male.
        let r = fireworks_npc_registration(8, 0, 0, 0xE001, 2).unwrap();
        assert_eq!(
            r,
            EventNpcRegistration { event_id: SP_NPC_EV_TURI_0, texture_id: 0xE001, anime_id: 0xE001, cloth: ITM_CLOTH022 + 1 }
        );
        assert_eq!(fireworks_npc_registration(8, 0, 0, 0xE001, 0).unwrap().cloth, ITM_CLOTH022);
        assert!(fireworks_npc_registration(8, 0, 4, 0xE001, 0).is_none());
    }

    #[test]
    fn random_pick_prefers_friends_when_surplus() {
        // 6 villagers, need 4; slots 0 and 1 don't know the player.
        let m = AnimalMasks { occupied: 0b111111, knows_player: 0b111100, player_is_foreign: false };
        let mut out = [NO_ANIMAL; 4];
        npc_idx_random(&mut out, &m, |_| 0);
        assert_eq!(out, [2, 3, 4, 5]);
    }

    #[test]
    fn refill_takes_lowest_and_consumes_rng() {
        let m = AnimalMasks { occupied: 0b1111, knows_player: 0xFFFF, player_is_foreign: false };
        let mut list = [1, NO_ANIMAL, 3, NO_ANIMAL];
        let mut calls = 0;
        set_npc_joint_random(&mut list, &m, |_| {
            calls += 1;
            0
        });
        assert_eq!(list, [1, 0, 3, 2]);
        assert_eq!(calls, 2);
    }

    #[test]
    fn lap_player() {
        // Groundhog Day: villager 0 at (5, 12); GHOG structure at index 5.
        let ev = Some(7);
        assert_eq!(check_lap_player(ev, 0, 6, 11, |_, _, _| false), 1);
        assert_eq!(check_lap_player(ev, 0, 0, 0, |_, _, _| false), 0);
        assert_eq!(check_lap_player(ev, 0, 0, 0, |n, _, _| n == GHOG), 2);
        assert_eq!(check_lap_player(None, 0, 6, 11, |_, _, _| true), 0);
    }
}

// ---------------------------------------------------------------------------
// Data tables: generated by gen_evmn.py from src/game/m_event_map_npc_data.c_inc
// (names resolved via include/m_name_table.h; do not hand-edit)
// ---------------------------------------------------------------------------
pub const AEROBICS_RADIO: u16 = 0x582C; // (STRUCTURE_START + 44)
pub const EMPTY_NO: u16 = 0x0000; // 0x0000
pub const FIREWORKS_STALL0: u16 = 0x582D; // (STRUCTURE_START + 45)
pub const FIREWORKS_STALL1: u16 = 0x582E; // (STRUCTURE_START + 46)
pub const FISHCHECK_STAND0: u16 = 0x5838; // (STRUCTURE_START + 56)
pub const FISHCHECK_STAND1: u16 = 0x5839; // (STRUCTURE_START + 57)
pub const GHOG: u16 = 0x5845; // (STRUCTURE_START + 69)
pub const HTABLE0: u16 = 0x5846; // (STRUCTURE_START + 70)
pub const HTABLE1: u16 = 0x5847; // (STRUCTURE_START + 71)
pub const HTABLE2: u16 = 0x5848; // (STRUCTURE_START + 72)
pub const ITM_CLOTH020: u16 = 0x2414; // (ITM_CLOTH_START +  20)
pub const ITM_CLOTH022: u16 = 0x2416; // (ITM_CLOTH_START +  22)
pub const NEWYEAR_COUNTDOWN0: u16 = 0x5832; // (STRUCTURE_START + 50)
pub const NEWYEAR_COUNTDOWN1: u16 = 0x5833; // (STRUCTURE_START + 51)
pub const NEWYEAR_TABLE: u16 = 0x5831; // (STRUCTURE_START + 49)
pub const SAKURA_TABLE0: u16 = 0x582A; // (STRUCTURE_START + 42)
pub const SAKURA_TABLE1: u16 = 0x582B; // (STRUCTURE_START + 43)
pub const SPORTSFAIR_BALLS_RED: u16 = 0x5834; // (STRUCTURE_START + 52)
pub const SPORTSFAIR_BALLS_WHITE: u16 = 0x5835; // (STRUCTURE_START + 53)
pub const SPORTSFAIR_BASKET_RED: u16 = 0x5836; // (STRUCTURE_START + 54)
pub const SPORTSFAIR_BASKET_WHITE: u16 = 0x5837; // (STRUCTURE_START + 55)
pub const SP_NPC_ANGLER: u16 = 0xD01D; // (SP_NPC_START + 29)
pub const SP_NPC_EV_COUNTDOWN_0: u16 = 0xD044; // (SP_NPC_START + 68)
pub const SP_NPC_EV_COUNTDOWN_1: u16 = 0xD045; // (SP_NPC_START + 69)
pub const SP_NPC_EV_COUNTDOWN_2: u16 = 0xD046; // (SP_NPC_START + 70)
pub const SP_NPC_EV_COUNTDOWN_3: u16 = 0xD047; // (SP_NPC_START + 71)
pub const SP_NPC_EV_COUNTDOWN_4: u16 = 0xD048; // (SP_NPC_START + 72)
pub const SP_NPC_EV_GROUNDHOG_0: u16 = 0xD088; // (SP_NPC_START + 136)
pub const SP_NPC_EV_GROUNDHOG_1: u16 = 0xD089; // (SP_NPC_START + 137)
pub const SP_NPC_EV_GROUNDHOG_2: u16 = 0xD08A; // (SP_NPC_START + 138)
pub const SP_NPC_EV_GROUNDHOG_3: u16 = 0xD08B; // (SP_NPC_START + 139)
pub const SP_NPC_EV_HANABI_0: u16 = 0xD027; // (SP_NPC_START + 39)
pub const SP_NPC_EV_HANABI_1: u16 = 0xD028; // (SP_NPC_START + 40)
pub const SP_NPC_EV_HANABI_2: u16 = 0xD029; // (SP_NPC_START + 41)
pub const SP_NPC_EV_HANABI_3: u16 = 0xD02A; // (SP_NPC_START + 42)
pub const SP_NPC_EV_HANABI_4: u16 = 0xD02B; // (SP_NPC_START + 43)
pub const SP_NPC_EV_HANAMI_0: u16 = 0xD032; // (SP_NPC_START + 50)
pub const SP_NPC_EV_HANAMI_1: u16 = 0xD033; // (SP_NPC_START + 51)
pub const SP_NPC_EV_HANAMI_2: u16 = 0xD034; // (SP_NPC_START + 52)
pub const SP_NPC_EV_HANAMI_3: u16 = 0xD035; // (SP_NPC_START + 53)
pub const SP_NPC_EV_HANAMI_4: u16 = 0xD036; // (SP_NPC_START + 54)
pub const SP_NPC_EV_HARVEST_0: u16 = 0xD082; // (SP_NPC_START + 130)
pub const SP_NPC_EV_HARVEST_1: u16 = 0xD083; // (SP_NPC_START + 131)
pub const SP_NPC_EV_HARVEST_2: u16 = 0xD084; // (SP_NPC_START + 132)
pub const SP_NPC_EV_HARVEST_3: u16 = 0xD085; // (SP_NPC_START + 133)
pub const SP_NPC_EV_HARVEST_4: u16 = 0xD086; // (SP_NPC_START + 134)
pub const SP_NPC_EV_HATUMODE_0: u16 = 0xD058; // (SP_NPC_START + 88)
pub const SP_NPC_EV_HATUMODE_1: u16 = 0xD059; // (SP_NPC_START + 89)
pub const SP_NPC_EV_HATUMODE_2: u16 = 0xD05A; // (SP_NPC_START + 90)
pub const SP_NPC_EV_HATUMODE_3: u16 = 0xD05B; // (SP_NPC_START + 91)
pub const SP_NPC_EV_HATUMODE_4: u16 = 0xD05C; // (SP_NPC_START + 92)
pub const SP_NPC_EV_KAMAKURA_0: u16 = 0xD05E; // (SP_NPC_START + 94)
pub const SP_NPC_EV_MIKO: u16 = 0xD03D; // (SP_NPC_START + 61)
pub const SP_NPC_EV_SONCHO2: u16 = 0xD074; // (SP_NPC_START + 116)
pub const SP_NPC_EV_SPEECH_SONCHO: u16 = 0xD087; // (SP_NPC_START + 135)
pub const SP_NPC_EV_TAISOU_0: u16 = 0xD04E; // (SP_NPC_START + 78)
pub const SP_NPC_EV_TAISOU_1: u16 = 0xD04F; // (SP_NPC_START + 79)
pub const SP_NPC_EV_TAISOU_2: u16 = 0xD050; // (SP_NPC_START + 80)
pub const SP_NPC_EV_TAISOU_3: u16 = 0xD051; // (SP_NPC_START + 81)
pub const SP_NPC_EV_TAISOU_4: u16 = 0xD052; // (SP_NPC_START + 82)
pub const SP_NPC_EV_TAMAIRE_0: u16 = 0xD053; // (SP_NPC_START + 83)
pub const SP_NPC_EV_TAMAIRE_1: u16 = 0xD054; // (SP_NPC_START + 84)
pub const SP_NPC_EV_TAMAIRE_2: u16 = 0xD055; // (SP_NPC_START + 85)
pub const SP_NPC_EV_TAMAIRE_3: u16 = 0xD056; // (SP_NPC_START + 86)
pub const SP_NPC_EV_TAMAIRE_4: u16 = 0xD057; // (SP_NPC_START + 87)
pub const SP_NPC_EV_TOKYOSO_0: u16 = 0xD02D; // (SP_NPC_START + 45)
pub const SP_NPC_EV_TOKYOSO_1: u16 = 0xD02E; // (SP_NPC_START + 46)
pub const SP_NPC_EV_TOKYOSO_2: u16 = 0xD02F; // (SP_NPC_START + 47)
pub const SP_NPC_EV_TOKYOSO_3: u16 = 0xD030; // (SP_NPC_START + 48)
pub const SP_NPC_EV_TOKYOSO_4: u16 = 0xD031; // (SP_NPC_START + 49)
pub const SP_NPC_EV_TUKIMI_0: u16 = 0xD03F; // (SP_NPC_START + 63)
pub const SP_NPC_EV_TUKIMI_1: u16 = 0xD040; // (SP_NPC_START + 64)
pub const SP_NPC_EV_TUKIMI_2: u16 = 0xD041; // (SP_NPC_START + 65)
pub const SP_NPC_EV_TUKIMI_3: u16 = 0xD042; // (SP_NPC_START + 66)
pub const SP_NPC_EV_TUNAHIKI_0: u16 = 0xD05F; // (SP_NPC_START + 95)
pub const SP_NPC_EV_TUNAHIKI_1: u16 = 0xD060; // (SP_NPC_START + 96)
pub const SP_NPC_EV_TUNAHIKI_2: u16 = 0xD061; // (SP_NPC_START + 97)
pub const SP_NPC_EV_TUNAHIKI_3: u16 = 0xD062; // (SP_NPC_START + 98)
pub const SP_NPC_EV_TUNAHIKI_4: u16 = 0xD063; // (SP_NPC_START + 99)
pub const SP_NPC_EV_TURI_0: u16 = 0xD049; // (SP_NPC_START + 73)
pub const SP_NPC_EV_TURI_1: u16 = 0xD04A; // (SP_NPC_START + 74)
pub const SP_NPC_EV_TURI_2: u16 = 0xD04B; // (SP_NPC_START + 75)
pub const SP_NPC_EV_TURI_3: u16 = 0xD04C; // (SP_NPC_START + 76)
pub const SP_NPC_EV_TURI_4: u16 = 0xD04D; // (SP_NPC_START + 77)
pub const SP_NPC_EV_YOMISE: u16 = 0xD02C; // (SP_NPC_START + 44)
pub const SP_NPC_EV_YOMISE2: u16 = 0xD039; // (SP_NPC_START + 57)
pub const SP_NPC_SONCHO_D078: u16 = 0xD078; // (SP_NPC_START + 120)
pub const BLOCKKIND_NONE: u32 = 0x0000; // (0 << 0)
pub const BLOCKKIND_POOL: u32 = 0x8000; // (1 << 15)
pub const BLOCKKIND_SHRINE: u32 = 0x0004; // (1 << 2)

const L_HNY_SET_ACTOR0: [u16; 8] = [SP_NPC_EV_HATUMODE_0, SP_NPC_EV_HATUMODE_1, SP_NPC_EV_HATUMODE_2, SP_NPC_EV_HATUMODE_3, SP_NPC_EV_HATUMODE_4, SP_NPC_EV_MIKO, NEWYEAR_TABLE, SP_NPC_EV_SONCHO2];
const L_FSH_SET_ACTOR0: [u16; 7] = [SP_NPC_EV_TURI_0, SP_NPC_EV_TURI_1, SP_NPC_EV_TURI_2, SP_NPC_EV_TURI_3, SP_NPC_EV_TURI_4, SP_NPC_ANGLER, FISHCHECK_STAND0];
const L_FSH_SET_ACTOR1: [u16; 7] = [SP_NPC_EV_TURI_0, SP_NPC_EV_TURI_1, SP_NPC_EV_TURI_2, SP_NPC_EV_TURI_3, SP_NPC_EV_TURI_4, SP_NPC_ANGLER, FISHCHECK_STAND1];
const L_RC_SET_ACTOR0: [u16; 7] = [SP_NPC_EV_TAISOU_1, SP_NPC_EV_TAISOU_2, SP_NPC_EV_TAISOU_3, SP_NPC_EV_TAISOU_4, SP_NPC_EV_TAISOU_0, AEROBICS_RADIO, SP_NPC_SONCHO_D078];
const L_FWS_SET_ACTOR0: [u16; 7] = [SP_NPC_EV_HANABI_0, SP_NPC_EV_HANABI_1, SP_NPC_EV_HANABI_2, SP_NPC_EV_HANABI_3, SP_NPC_EV_HANABI_4, SP_NPC_EV_YOMISE, FIREWORKS_STALL0];
const L_FWS_SET_ACTOR1: [u16; 7] = [SP_NPC_EV_HANABI_0, SP_NPC_EV_HANABI_1, SP_NPC_EV_HANABI_2, SP_NPC_EV_HANABI_3, SP_NPC_EV_HANABI_4, SP_NPC_EV_YOMISE2, FIREWORKS_STALL1];
const L_FV_SET_ACTOR0: [u16; 8] = [SP_NPC_EV_HANAMI_0, SP_NPC_EV_HANAMI_1, SP_NPC_EV_HANAMI_2, SP_NPC_EV_HANAMI_3, SP_NPC_EV_HANAMI_4, SAKURA_TABLE1, SAKURA_TABLE0, SP_NPC_EV_SONCHO2];
const L_FDD_RADIO_SET_ACTOR0: [u16; 7] = [SP_NPC_EV_TAISOU_1, SP_NPC_EV_TAISOU_2, SP_NPC_EV_TAISOU_3, SP_NPC_EV_TAISOU_4, SP_NPC_EV_TAISOU_0, AEROBICS_RADIO, SP_NPC_SONCHO_D078];
const L_FDD_TOKYOSO_SET_ACTOR0: [u16; 6] = [SP_NPC_EV_TOKYOSO_0, SP_NPC_EV_TOKYOSO_1, SP_NPC_EV_TOKYOSO_2, SP_NPC_EV_TOKYOSO_3, SP_NPC_EV_TOKYOSO_4, SP_NPC_EV_SONCHO2];
const L_FDD_TAMA_SET_ACTOR0: [u16; 10] = [SP_NPC_EV_TAMAIRE_0, SP_NPC_EV_TAMAIRE_1, SP_NPC_EV_TAMAIRE_2, SP_NPC_EV_TAMAIRE_3, SP_NPC_EV_TAMAIRE_4, SPORTSFAIR_BALLS_RED, SPORTSFAIR_BASKET_RED, SPORTSFAIR_BALLS_WHITE, SPORTSFAIR_BASKET_WHITE, SP_NPC_EV_SONCHO2];
const L_FDD_TUNA_SET_ACTOR0: [u16; 6] = [SP_NPC_EV_TUNAHIKI_0, SP_NPC_EV_TUNAHIKI_1, SP_NPC_EV_TUNAHIKI_2, SP_NPC_EV_TUNAHIKI_3, SP_NPC_EV_TUNAHIKI_4, SP_NPC_EV_SONCHO2];
const L_CDN_SET_ACTOR0: [u16; 8] = [SP_NPC_EV_COUNTDOWN_0, SP_NPC_EV_COUNTDOWN_1, SP_NPC_EV_COUNTDOWN_2, SP_NPC_EV_COUNTDOWN_3, SP_NPC_EV_COUNTDOWN_4, NEWYEAR_COUNTDOWN0, NEWYEAR_COUNTDOWN1, SP_NPC_EV_SONCHO2];
const L_KK_SET_ACTOR0: [u16; 1] = [SP_NPC_EV_KAMAKURA_0];
const L_GH_SET_ACTOR0: [u16; 6] = [SP_NPC_EV_GROUNDHOG_0, SP_NPC_EV_GROUNDHOG_1, SP_NPC_EV_GROUNDHOG_2, SP_NPC_EV_GROUNDHOG_3, SP_NPC_EV_SPEECH_SONCHO, GHOG];
const L_HM_SET_ACTOR0: [u16; 5] = [SP_NPC_EV_TUKIMI_0, SP_NPC_EV_TUKIMI_1, SP_NPC_EV_TUKIMI_2, SP_NPC_EV_TUKIMI_3, SP_NPC_EV_SONCHO2];
const L_HF_SET_ACTOR0: [u16; 9] = [SP_NPC_EV_HARVEST_0, SP_NPC_EV_HARVEST_1, SP_NPC_EV_HARVEST_2, SP_NPC_EV_HARVEST_3, SP_NPC_EV_HARVEST_4, SP_NPC_EV_SONCHO2, HTABLE0, HTABLE1, HTABLE2];
const L_MS_SET_ACTOR0: [u16; 5] = [SP_NPC_EV_TUKIMI_0, SP_NPC_EV_TUKIMI_1, SP_NPC_EV_TUKIMI_2, SP_NPC_EV_TUKIMI_3, SP_NPC_EV_SONCHO2];
const L_MS_SET_ACTOR1: [u16; 5] = [SP_NPC_EV_TUKIMI_0, SP_NPC_EV_TUKIMI_1, SP_NPC_EV_TUKIMI_2, SP_NPC_EV_TUKIMI_3, SP_NPC_EV_SONCHO2];
const L_HNY_MAP: [ActorPos; 8] = [ActorPos::new(7, 9), ActorPos::new(6, 10), ActorPos::new(9, 10), ActorPos::new(6, 12), ActorPos::new(9, 12), ActorPos::new(3, 3), ActorPos::new(3, 3), ActorPos::new(11, 6)];
const L_FSH_PULE1_MAP: [ActorPos; 7] = [ActorPos::new(9, 7), ActorPos::new(3, 3), ActorPos::new(2, 4), ActorPos::new(12, 2), ActorPos::new(2, 11), ActorPos::new(10, 13), ActorPos::new(12, 12)];
const L_FSH_PULE2_MAP: [ActorPos; 7] = [ActorPos::new(6, 5), ActorPos::new(13, 12), ActorPos::new(12, 13), ActorPos::new(13, 2), ActorPos::new(2, 3), ActorPos::new(5, 13), ActorPos::new(3, 12)];
const L_FSH_PULE3_MAP: [ActorPos; 7] = [ActorPos::new(4, 10), ActorPos::new(4, 4), ActorPos::new(5, 3), ActorPos::new(13, 7), ActorPos::new(13, 3), ActorPos::new(10, 13), ActorPos::new(12, 12)];
const L_FSH_PULE4_MAP: [ActorPos; 7] = [ActorPos::new(9, 5), ActorPos::new(2, 3), ActorPos::new(3, 2), ActorPos::new(7, 2), ActorPos::new(13, 10), ActorPos::new(5, 13), ActorPos::new(3, 12)];
const L_FSH_PULE5_MAP: [ActorPos; 7] = [ActorPos::new(8, 7), ActorPos::new(12, 6), ActorPos::new(13, 7), ActorPos::new(2, 2), ActorPos::new(13, 13), ActorPos::new(10, 2), ActorPos::new(12, 3)];
const L_FSH_PULE6_MAP: [ActorPos; 7] = [ActorPos::new(6, 6), ActorPos::new(12, 2), ActorPos::new(13, 3), ActorPos::new(3, 2), ActorPos::new(2, 13), ActorPos::new(10, 13), ActorPos::new(12, 12)];
const L_FSH_PULE7_MAP: [ActorPos; 7] = [ActorPos::new(9, 10), ActorPos::new(13, 8), ActorPos::new(13, 9), ActorPos::new(13, 3), ActorPos::new(3, 13), ActorPos::new(2, 5), ActorPos::new(3, 3)];
const L_RC_MAP: [ActorPos; 7] = [ActorPos::new(4, 10), ActorPos::new(6, 10), ActorPos::new(8, 10), ActorPos::new(10, 10), ActorPos::new(7, 12), ActorPos::new(5, 13), ActorPos::new(9, 8)];
const L_FWS_PULE1_MAP: [ActorPos; 7] = [ActorPos::new(2, 3), ActorPos::new(8, 1), ActorPos::new(2, 11), ActorPos::new(13, 7), ActorPos::new(3, 3), ActorPos::new(12, 12), ActorPos::new(12, 12)];
const L_FWS_PULE2_MAP: [ActorPos; 7] = [ActorPos::new(13, 13), ActorPos::new(1, 3), ActorPos::new(13, 2), ActorPos::new(3, 8), ActorPos::new(14, 7), ActorPos::new(3, 12), ActorPos::new(3, 12)];
const L_FWS_PULE3_MAP: [ActorPos; 7] = [ActorPos::new(12, 2), ActorPos::new(2, 3), ActorPos::new(7, 2), ActorPos::new(12, 7), ActorPos::new(13, 7), ActorPos::new(12, 11), ActorPos::new(12, 11)];
const L_FWS_PULE4_MAP: [ActorPos; 7] = [ActorPos::new(13, 11), ActorPos::new(2, 3), ActorPos::new(7, 2), ActorPos::new(2, 8), ActorPos::new(9, 12), ActorPos::new(3, 12), ActorPos::new(3, 12)];
const L_FWS_PULE5_MAP: [ActorPos; 7] = [ActorPos::new(13, 6), ActorPos::new(1, 7), ActorPos::new(13, 13), ActorPos::new(2, 2), ActorPos::new(1, 11), ActorPos::new(12, 3), ActorPos::new(12, 3)];
const L_FWS_PULE6_MAP: [ActorPos; 7] = [ActorPos::new(13, 2), ActorPos::new(1, 8), ActorPos::new(8, 1), ActorPos::new(9, 13), ActorPos::new(6, 14), ActorPos::new(12, 11), ActorPos::new(12, 11)];
const L_FWS_PULE7_MAP: [ActorPos; 7] = [ActorPos::new(13, 12), ActorPos::new(12, 2), ActorPos::new(2, 11), ActorPos::new(3, 7), ActorPos::new(8, 1), ActorPos::new(3, 3), ActorPos::new(3, 3)];
const L_FV_MAP: [ActorPos; 8] = [ActorPos::new(10, 6), ActorPos::new(12, 8), ActorPos::new(6, 10), ActorPos::new(4, 12), ActorPos::new(4, 6), ActorPos::new(5, 11), ActorPos::new(11, 7), ActorPos::new(9, 11)];
const L_FDD_RADIO_MAP: [ActorPos; 7] = [ActorPos::new(4, 10), ActorPos::new(6, 10), ActorPos::new(8, 10), ActorPos::new(10, 10), ActorPos::new(7, 12), ActorPos::new(5, 13), ActorPos::new(9, 8)];
const L_FDD_TOKYOSO_MAP: [ActorPos; 6] = [ActorPos::new(13, 3), ActorPos::new(11, 7), ActorPos::new(12, 7), ActorPos::new(14, 7), ActorPos::new(14, 8), ActorPos::new(5, 8)];
const L_FDD_TAMA_MAP: [ActorPos; 10] = [ActorPos::new(7, 12), ActorPos::new(5, 6), ActorPos::new(2, 9), ActorPos::new(13, 6), ActorPos::new(10, 9), ActorPos::new(3, 8), ActorPos::new(3, 8), ActorPos::new(12, 8), ActorPos::new(12, 8), ActorPos::new(9, 4)];
const L_FDD_TUNA_MAP: [ActorPos; 6] = [ActorPos::new(7, 10), ActorPos::new(3, 11), ActorPos::new(4, 11), ActorPos::new(11, 11), ActorPos::new(12, 11), ActorPos::new(10, 8)];
const L_CDN_PULE1_MAP: [ActorPos; 8] = [ActorPos::new(10, 13), ActorPos::new(13, 9), ActorPos::new(12, 2), ActorPos::new(2, 3), ActorPos::new(2, 11), ActorPos::new(12, 12), ActorPos::new(12, 11), ActorPos::new(2, 11)];
const L_CDN_PULE2_MAP: [ActorPos; 8] = [ActorPos::new(5, 13), ActorPos::new(13, 13), ActorPos::new(13, 2), ActorPos::new(4, 3), ActorPos::new(13, 12), ActorPos::new(3, 12), ActorPos::new(3, 11), ActorPos::new(5, 2)];
const L_CDN_PULE3_MAP: [ActorPos; 8] = [ActorPos::new(10, 13), ActorPos::new(12, 8), ActorPos::new(2, 12), ActorPos::new(4, 3), ActorPos::new(12, 3), ActorPos::new(12, 12), ActorPos::new(12, 11), ActorPos::new(3, 13)];
const L_CDN_PULE4_MAP: [ActorPos; 8] = [ActorPos::new(5, 13), ActorPos::new(8, 13), ActorPos::new(13, 10), ActorPos::new(12, 3), ActorPos::new(2, 3), ActorPos::new(3, 12), ActorPos::new(3, 11), ActorPos::new(12, 4)];
const L_CDN_PULE5_MAP: [ActorPos; 8] = [ActorPos::new(11, 4), ActorPos::new(13, 6), ActorPos::new(2, 2), ActorPos::new(10, 2), ActorPos::new(13, 13), ActorPos::new(13, 4), ActorPos::new(13, 3), ActorPos::new(2, 13)];
const L_CDN_PULE6_MAP: [ActorPos; 8] = [ActorPos::new(10, 13), ActorPos::new(13, 8), ActorPos::new(12, 2), ActorPos::new(4, 4), ActorPos::new(2, 13), ActorPos::new(12, 12), ActorPos::new(12, 11), ActorPos::new(13, 3)];
const L_CDN_PULE7_MAP: [ActorPos; 8] = [ActorPos::new(5, 2), ActorPos::new(2, 7), ActorPos::new(13, 3), ActorPos::new(12, 13), ActorPos::new(3, 13), ActorPos::new(3, 3), ActorPos::new(3, 2), ActorPos::new(13, 11)];
const L_KK_MAP: [ActorPos; 1] = [ActorPos::new(4, 7)];
const L_GH_MAP: [ActorPos; 6] = [ActorPos::new(5, 12), ActorPos::new(6, 12), ActorPos::new(8, 12), ActorPos::new(10, 12), ActorPos::new(8, 9), ActorPos::new(7, 9)];
const L_HM_PULE1_MAP: [ActorPos; 5] = [ActorPos::new(13, 7), ActorPos::new(3, 3), ActorPos::new(2, 4), ActorPos::new(12, 2), ActorPos::new(7, 12)];
const L_HM_PULE2_MAP: [ActorPos; 5] = [ActorPos::new(3, 8), ActorPos::new(12, 13), ActorPos::new(13, 12), ActorPos::new(13, 2), ActorPos::new(14, 2)];
const L_HM_PULE3_MAP: [ActorPos; 5] = [ActorPos::new(9, 13), ActorPos::new(12, 3), ActorPos::new(13, 3), ActorPos::new(4, 4), ActorPos::new(11, 8)];
const L_HM_PULE4_MAP: [ActorPos; 5] = [ActorPos::new(9, 12), ActorPos::new(3, 2), ActorPos::new(2, 3), ActorPos::new(8, 2), ActorPos::new(11, 11)];
const L_HM_PULE5_MAP: [ActorPos; 5] = [ActorPos::new(9, 2), ActorPos::new(12, 6), ActorPos::new(13, 7), ActorPos::new(2, 2), ActorPos::new(2, 13)];
const L_HM_PULE6_MAP: [ActorPos; 5] = [ActorPos::new(12, 8), ActorPos::new(12, 2), ActorPos::new(13, 3), ActorPos::new(2, 13), ActorPos::new(11, 9)];
const L_HM_PULE7_MAP: [ActorPos; 5] = [ActorPos::new(3, 7), ActorPos::new(13, 8), ActorPos::new(13, 9), ActorPos::new(12, 2), ActorPos::new(2, 8)];
const L_HF_MAP: [ActorPos; 9] = [ActorPos::new(10, 6), ActorPos::new(12, 8), ActorPos::new(5, 4), ActorPos::new(3, 6), ActorPos::new(4, 12), ActorPos::new(9, 10), ActorPos::new(4, 5), ActorPos::new(11, 7), ActorPos::new(5, 10)];
const L_MS_PULE1_MAP: [ActorPos; 5] = [ActorPos::new(13, 7), ActorPos::new(3, 3), ActorPos::new(2, 4), ActorPos::new(12, 2), ActorPos::new(7, 12)];
const L_MS_PULE2_MAP: [ActorPos; 5] = [ActorPos::new(3, 8), ActorPos::new(12, 13), ActorPos::new(13, 12), ActorPos::new(13, 2), ActorPos::new(13, 2)];
const L_MS_PULE3_MAP: [ActorPos; 5] = [ActorPos::new(9, 13), ActorPos::new(12, 3), ActorPos::new(13, 3), ActorPos::new(4, 4), ActorPos::new(11, 8)];
const L_MS_PULE4_MAP: [ActorPos; 5] = [ActorPos::new(9, 12), ActorPos::new(3, 2), ActorPos::new(2, 3), ActorPos::new(8, 2), ActorPos::new(11, 11)];
const L_MS_PULE5_MAP: [ActorPos; 5] = [ActorPos::new(9, 2), ActorPos::new(12, 6), ActorPos::new(13, 7), ActorPos::new(2, 2), ActorPos::new(2, 13)];
const L_MS_PULE6_MAP: [ActorPos; 5] = [ActorPos::new(12, 8), ActorPos::new(12, 2), ActorPos::new(13, 3), ActorPos::new(2, 13), ActorPos::new(11, 9)];
const L_MS_PULE7_MAP: [ActorPos; 5] = [ActorPos::new(3, 7), ActorPos::new(13, 8), ActorPos::new(13, 9), ActorPos::new(12, 2), ActorPos::new(2, 8)];
const L_HNY_SET_DT: ActorInfo = ActorInfo { names: &L_HNY_SET_ACTOR0, positions: &L_HNY_MAP };
const L_FSH_PULE1_SET_DT: ActorInfo = ActorInfo { names: &L_FSH_SET_ACTOR0, positions: &L_FSH_PULE1_MAP };
const L_FSH_PULE2_SET_DT: ActorInfo = ActorInfo { names: &L_FSH_SET_ACTOR1, positions: &L_FSH_PULE2_MAP };
const L_FSH_PULE3_SET_DT: ActorInfo = ActorInfo { names: &L_FSH_SET_ACTOR0, positions: &L_FSH_PULE3_MAP };
const L_FSH_PULE4_SET_DT: ActorInfo = ActorInfo { names: &L_FSH_SET_ACTOR1, positions: &L_FSH_PULE4_MAP };
const L_FSH_PULE5_SET_DT: ActorInfo = ActorInfo { names: &L_FSH_SET_ACTOR1, positions: &L_FSH_PULE5_MAP };
const L_FSH_PULE6_SET_DT: ActorInfo = ActorInfo { names: &L_FSH_SET_ACTOR0, positions: &L_FSH_PULE6_MAP };
const L_FSH_PULE7_SET_DT: ActorInfo = ActorInfo { names: &L_FSH_SET_ACTOR0, positions: &L_FSH_PULE7_MAP };
const L_RC_SET_DT: ActorInfo = ActorInfo { names: &L_RC_SET_ACTOR0, positions: &L_RC_MAP };
const L_FWS_PULE1_SET_DT: ActorInfo = ActorInfo { names: &L_FWS_SET_ACTOR1, positions: &L_FWS_PULE1_MAP };
const L_FWS_PULE2_SET_DT: ActorInfo = ActorInfo { names: &L_FWS_SET_ACTOR0, positions: &L_FWS_PULE2_MAP };
const L_FWS_PULE3_SET_DT: ActorInfo = ActorInfo { names: &L_FWS_SET_ACTOR1, positions: &L_FWS_PULE3_MAP };
const L_FWS_PULE4_SET_DT: ActorInfo = ActorInfo { names: &L_FWS_SET_ACTOR0, positions: &L_FWS_PULE4_MAP };
const L_FWS_PULE5_SET_DT: ActorInfo = ActorInfo { names: &L_FWS_SET_ACTOR1, positions: &L_FWS_PULE5_MAP };
const L_FWS_PULE6_SET_DT: ActorInfo = ActorInfo { names: &L_FWS_SET_ACTOR1, positions: &L_FWS_PULE6_MAP };
const L_FWS_PULE7_SET_DT: ActorInfo = ActorInfo { names: &L_FWS_SET_ACTOR0, positions: &L_FWS_PULE7_MAP };
const L_FV_SET_DT: ActorInfo = ActorInfo { names: &L_FV_SET_ACTOR0, positions: &L_FV_MAP };
const L_FDD_RADIO_SET_DT: ActorInfo = ActorInfo { names: &L_FDD_RADIO_SET_ACTOR0, positions: &L_FDD_RADIO_MAP };
const L_FDD_TOKYOSO_SET_DT: ActorInfo = ActorInfo { names: &L_FDD_TOKYOSO_SET_ACTOR0, positions: &L_FDD_TOKYOSO_MAP };
const L_FDD_TAMA_SET_DT: ActorInfo = ActorInfo { names: &L_FDD_TAMA_SET_ACTOR0, positions: &L_FDD_TAMA_MAP };
const L_FDD_TUNA_SET_DT: ActorInfo = ActorInfo { names: &L_FDD_TUNA_SET_ACTOR0, positions: &L_FDD_TUNA_MAP };
const L_CDN_PULE1_SET_DT: ActorInfo = ActorInfo { names: &L_CDN_SET_ACTOR0, positions: &L_CDN_PULE1_MAP };
const L_CDN_PULE2_SET_DT: ActorInfo = ActorInfo { names: &L_CDN_SET_ACTOR0, positions: &L_CDN_PULE2_MAP };
const L_CDN_PULE3_SET_DT: ActorInfo = ActorInfo { names: &L_CDN_SET_ACTOR0, positions: &L_CDN_PULE3_MAP };
const L_CDN_PULE4_SET_DT: ActorInfo = ActorInfo { names: &L_CDN_SET_ACTOR0, positions: &L_CDN_PULE4_MAP };
const L_CDN_PULE5_SET_DT: ActorInfo = ActorInfo { names: &L_CDN_SET_ACTOR0, positions: &L_CDN_PULE5_MAP };
const L_CDN_PULE6_SET_DT: ActorInfo = ActorInfo { names: &L_CDN_SET_ACTOR0, positions: &L_CDN_PULE6_MAP };
const L_CDN_PULE7_SET_DT: ActorInfo = ActorInfo { names: &L_CDN_SET_ACTOR0, positions: &L_CDN_PULE7_MAP };
const L_KK_SET_DT: ActorInfo = ActorInfo { names: &L_KK_SET_ACTOR0, positions: &L_KK_MAP };
const L_GH_SET_DT: ActorInfo = ActorInfo { names: &L_GH_SET_ACTOR0, positions: &L_GH_MAP };
const L_HM_PULE1_SET_DT: ActorInfo = ActorInfo { names: &L_HM_SET_ACTOR0, positions: &L_HM_PULE1_MAP };
const L_HM_PULE2_SET_DT: ActorInfo = ActorInfo { names: &L_HM_SET_ACTOR0, positions: &L_HM_PULE2_MAP };
const L_HM_PULE3_SET_DT: ActorInfo = ActorInfo { names: &L_HM_SET_ACTOR0, positions: &L_HM_PULE3_MAP };
const L_HM_PULE4_SET_DT: ActorInfo = ActorInfo { names: &L_HM_SET_ACTOR0, positions: &L_HM_PULE4_MAP };
const L_HM_PULE5_SET_DT: ActorInfo = ActorInfo { names: &L_HM_SET_ACTOR0, positions: &L_HM_PULE5_MAP };
const L_HM_PULE6_SET_DT: ActorInfo = ActorInfo { names: &L_HM_SET_ACTOR0, positions: &L_HM_PULE6_MAP };
const L_HM_PULE7_SET_DT: ActorInfo = ActorInfo { names: &L_HM_SET_ACTOR0, positions: &L_HM_PULE7_MAP };
const L_HF_SET_DT: ActorInfo = ActorInfo { names: &L_HF_SET_ACTOR0, positions: &L_HF_MAP };
const L_MS_PULE1_SET_DT: ActorInfo = ActorInfo { names: &L_MS_SET_ACTOR1, positions: &L_MS_PULE1_MAP };
const L_MS_PULE2_SET_DT: ActorInfo = ActorInfo { names: &L_MS_SET_ACTOR0, positions: &L_MS_PULE2_MAP };
const L_MS_PULE3_SET_DT: ActorInfo = ActorInfo { names: &L_MS_SET_ACTOR1, positions: &L_MS_PULE3_MAP };
const L_MS_PULE4_SET_DT: ActorInfo = ActorInfo { names: &L_MS_SET_ACTOR0, positions: &L_MS_PULE4_MAP };
const L_MS_PULE5_SET_DT: ActorInfo = ActorInfo { names: &L_MS_SET_ACTOR1, positions: &L_MS_PULE5_MAP };
const L_MS_PULE6_SET_DT: ActorInfo = ActorInfo { names: &L_MS_SET_ACTOR1, positions: &L_MS_PULE6_MAP };
const L_MS_PULE7_SET_DT: ActorInfo = ActorInfo { names: &L_MS_SET_ACTOR0, positions: &L_MS_PULE7_MAP };
const L_HNY_SET_DT_TABLE: [&ActorInfo; 1] = [&L_HNY_SET_DT];
const L_FSH_SET_DT_TABLE: [&ActorInfo; 7] = [&L_FSH_PULE1_SET_DT, &L_FSH_PULE2_SET_DT, &L_FSH_PULE3_SET_DT, &L_FSH_PULE4_SET_DT, &L_FSH_PULE5_SET_DT, &L_FSH_PULE6_SET_DT, &L_FSH_PULE7_SET_DT];
const L_RC_SET_DT_TABLE: [&ActorInfo; 1] = [&L_RC_SET_DT];
const L_FWS_SET_DT_TABLE: [&ActorInfo; 7] = [&L_FWS_PULE1_SET_DT, &L_FWS_PULE2_SET_DT, &L_FWS_PULE3_SET_DT, &L_FWS_PULE4_SET_DT, &L_FWS_PULE5_SET_DT, &L_FWS_PULE6_SET_DT, &L_FWS_PULE7_SET_DT];
const L_FV_SET_DT_TABLE: [&ActorInfo; 1] = [&L_FV_SET_DT];
const L_FDD_RADIO_SET_DT_TABLE: [&ActorInfo; 1] = [&L_FDD_RADIO_SET_DT];
const L_FDD_TOKYOSO_SET_DT_TABLE: [&ActorInfo; 1] = [&L_FDD_TOKYOSO_SET_DT];
const L_FDD_TAMA_SET_DT_TABLE: [&ActorInfo; 1] = [&L_FDD_TAMA_SET_DT];
const L_FDD_TUNA_SET_DT_TABLE: [&ActorInfo; 1] = [&L_FDD_TUNA_SET_DT];
const L_CDN_SET_DT_TABLE: [&ActorInfo; 7] = [&L_CDN_PULE1_SET_DT, &L_CDN_PULE2_SET_DT, &L_CDN_PULE3_SET_DT, &L_CDN_PULE4_SET_DT, &L_CDN_PULE5_SET_DT, &L_CDN_PULE6_SET_DT, &L_CDN_PULE7_SET_DT];
const L_KK_SET_DT_TABLE: [&ActorInfo; 1] = [&L_KK_SET_DT];
const L_GH_SET_DT_TABLE: [&ActorInfo; 1] = [&L_GH_SET_DT];
const L_HM_SET_DT_TABLE: [&ActorInfo; 7] = [&L_HM_PULE1_SET_DT, &L_HM_PULE2_SET_DT, &L_HM_PULE3_SET_DT, &L_HM_PULE4_SET_DT, &L_HM_PULE5_SET_DT, &L_HM_PULE6_SET_DT, &L_HM_PULE7_SET_DT];
const L_HF_SET_DT_TABLE: [&ActorInfo; 1] = [&L_HF_SET_DT];
const L_MS_SET_DT_TABLE: [&ActorInfo; 7] = [&L_MS_PULE1_SET_DT, &L_MS_PULE2_SET_DT, &L_MS_PULE3_SET_DT, &L_MS_PULE4_SET_DT, &L_MS_PULE5_SET_DT, &L_MS_PULE6_SET_DT, &L_MS_PULE7_SET_DT];
const L_HNY_SET_NPC: NpcData = NpcData { are_npc_flags: 0b00011111, cloth: EMPTY_NO };
const L_FSH_SET_NPC: NpcData = NpcData { are_npc_flags: 0b0011111, cloth: ITM_CLOTH022 };
const L_RC_SET_NPC: NpcData = NpcData { are_npc_flags: 0b0011111, cloth: EMPTY_NO };
const L_FWS_SET_NPC: NpcData = NpcData { are_npc_flags: 0b0011111, cloth: EMPTY_NO };
const L_FV_SET_NPC: NpcData = NpcData { are_npc_flags: 0b0011111, cloth: EMPTY_NO };
const L_FDD_RADIO_SET_NPC: NpcData = NpcData { are_npc_flags: 0b0011111, cloth: ITM_CLOTH020 };
const L_FDD_TOKYOSO_SET_NPC: NpcData = NpcData { are_npc_flags: 0b0011111, cloth: ITM_CLOTH020 };
const L_FDD_TAMA_SET_NPC: NpcData = NpcData { are_npc_flags: 0b0011111, cloth: ITM_CLOTH020 };
const L_FDD_TUNA_SET_NPC: NpcData = NpcData { are_npc_flags: 0b0011111, cloth: ITM_CLOTH020 };
const L_CDN_SET_NPC: NpcData = NpcData { are_npc_flags: 0b0011111, cloth: EMPTY_NO };
const L_KK_SET_NPC: NpcData = NpcData { are_npc_flags: 0b1, cloth: EMPTY_NO };
const L_GH_SET_NPC: NpcData = NpcData { are_npc_flags: 0b011111, cloth: EMPTY_NO };
const L_HM_SET_NPC: NpcData = NpcData { are_npc_flags: 0b0011111, cloth: EMPTY_NO };
const L_HF_SET_NPC: NpcData = NpcData { are_npc_flags: 0b011111, cloth: EMPTY_NO };
const L_MS_SET_NPC: NpcData = NpcData { are_npc_flags: 0b0011111, cloth: EMPTY_NO };
const L_HNY_SET_DATA: EventMapData = EventMapData { get_npc_idx_type: 0, normal_npc_max: 4, map_max: 1, npc_max: 8, actor_info: &L_HNY_SET_DT_TABLE, npc_data: &L_HNY_SET_NPC, block_kind: BLOCKKIND_SHRINE };
const L_FSH_SET_DATA: EventMapData = EventMapData { get_npc_idx_type: 0, normal_npc_max: 4, map_max: 7, npc_max: 7, actor_info: &L_FSH_SET_DT_TABLE, npc_data: &L_FSH_SET_NPC, block_kind: BLOCKKIND_POOL };
const L_RC_SET_DATA: EventMapData = EventMapData { get_npc_idx_type: 0, normal_npc_max: 4, map_max: 1, npc_max: 7, actor_info: &L_RC_SET_DT_TABLE, npc_data: &L_RC_SET_NPC, block_kind: BLOCKKIND_SHRINE };
const L_FWS_SET_DATA: EventMapData = EventMapData { get_npc_idx_type: 0, normal_npc_max: 4, map_max: 7, npc_max: 7, actor_info: &L_FWS_SET_DT_TABLE, npc_data: &L_FWS_SET_NPC, block_kind: BLOCKKIND_POOL };
const L_FV_SET_DATA: EventMapData = EventMapData { get_npc_idx_type: 0, normal_npc_max: 5, map_max: 1, npc_max: 8, actor_info: &L_FV_SET_DT_TABLE, npc_data: &L_FV_SET_NPC, block_kind: BLOCKKIND_SHRINE };
const L_FDD_RADIO_SET_DATA: EventMapData = EventMapData { get_npc_idx_type: 0, normal_npc_max: 4, map_max: 1, npc_max: 7, actor_info: &L_FDD_RADIO_SET_DT_TABLE, npc_data: &L_FDD_RADIO_SET_NPC, block_kind: BLOCKKIND_SHRINE };
const L_FDD_TOKYOSO_SET_DATA: EventMapData = EventMapData { get_npc_idx_type: 0, normal_npc_max: 5, map_max: 1, npc_max: 6, actor_info: &L_FDD_TOKYOSO_SET_DT_TABLE, npc_data: &L_FDD_TOKYOSO_SET_NPC, block_kind: BLOCKKIND_SHRINE };
const L_FDD_TAMA_SET_DATA: EventMapData = EventMapData { get_npc_idx_type: 0, normal_npc_max: 5, map_max: 1, npc_max: 10, actor_info: &L_FDD_TAMA_SET_DT_TABLE, npc_data: &L_FDD_TAMA_SET_NPC, block_kind: BLOCKKIND_SHRINE };
const L_FDD_TUNA_SET_DATA: EventMapData = EventMapData { get_npc_idx_type: 0, normal_npc_max: 5, map_max: 1, npc_max: 6, actor_info: &L_FDD_TUNA_SET_DT_TABLE, npc_data: &L_FDD_TUNA_SET_NPC, block_kind: BLOCKKIND_SHRINE };
const L_CDN_SET_DATA: EventMapData = EventMapData { get_npc_idx_type: 0, normal_npc_max: 4, map_max: 7, npc_max: 8, actor_info: &L_CDN_SET_DT_TABLE, npc_data: &L_CDN_SET_NPC, block_kind: BLOCKKIND_POOL };
const L_KK_SET_DATA: EventMapData = EventMapData { get_npc_idx_type: 0, normal_npc_max: 1, map_max: 1, npc_max: 1, actor_info: &L_KK_SET_DT_TABLE, npc_data: &L_KK_SET_NPC, block_kind: BLOCKKIND_NONE };
const L_GH_SET_DATA: EventMapData = EventMapData { get_npc_idx_type: 0, normal_npc_max: 4, map_max: 1, npc_max: 6, actor_info: &L_GH_SET_DT_TABLE, npc_data: &L_GH_SET_NPC, block_kind: BLOCKKIND_SHRINE };
const L_HM_SET_DATA: EventMapData = EventMapData { get_npc_idx_type: 0, normal_npc_max: 4, map_max: 7, npc_max: 5, actor_info: &L_HM_SET_DT_TABLE, npc_data: &L_HM_SET_NPC, block_kind: BLOCKKIND_POOL };
const L_HF_SET_DATA: EventMapData = EventMapData { get_npc_idx_type: 0, normal_npc_max: 5, map_max: 1, npc_max: 9, actor_info: &L_HF_SET_DT_TABLE, npc_data: &L_HF_SET_NPC, block_kind: BLOCKKIND_SHRINE };
const L_MS_SET_DATA: EventMapData = EventMapData { get_npc_idx_type: 0, normal_npc_max: 4, map_max: 7, npc_max: 5, actor_info: &L_MS_SET_DT_TABLE, npc_data: &L_MS_SET_NPC, block_kind: BLOCKKIND_POOL };
/// `l_event_set_table`, indexed by `mEvMN_TYPE_*`.
pub const EVENT_SET_TABLE: [Option<&EventMapData>; 17] = [Some(&L_FWS_SET_DATA), Some(&L_FV_SET_DATA), Some(&L_FDD_TOKYOSO_SET_DATA), Some(&L_FDD_RADIO_SET_DATA), Some(&L_FDD_TAMA_SET_DATA), Some(&L_FDD_TUNA_SET_DATA), None, Some(&L_HNY_SET_DATA), Some(&L_FSH_SET_DATA), Some(&L_FSH_SET_DATA), Some(&L_RC_SET_DATA), Some(&L_HM_SET_DATA), Some(&L_HF_SET_DATA), Some(&L_CDN_SET_DATA), Some(&L_KK_SET_DATA), Some(&L_GH_SET_DATA), Some(&L_MS_SET_DATA)];
