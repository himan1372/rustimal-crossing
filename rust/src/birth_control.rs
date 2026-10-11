//! Birth Control actor: per-acre actor spawning (`ac_birth_control.c`).
//!
//! Despite the name this is not about birthdays: "birth" is the decomp's
//! word for spawning. `Birth_Control_Profile` is an always-alive item-part
//! actor whose move proc (re)spawns the actors that live in the current
//! acre (block) whenever `mFI_ActorisBorn()` signals a block change.
//!
//! Source-verified against ac-decomp `src/actor/ac_birth_control.c` +
//! `include/ac_birth_control.h` (primary) and the ACGC-PC-Port copy
//! (secondary; diverges, see below).
//!
//! ## Retail behaviour (`aBC_actor_move`)
//! 1. If `bg_item_type == 0` (outdoor field): OR `mFI_ActorisBorn()` into
//!    `setup_actor_flag`, then run the boat logic (`aBC_set_boat`).
//! 2. On `mFI_ActorisBorn()`: DMA the block's move-actor list
//!    (`mFI_MoveActorListDma`), add resident NPCs (`mNpc_AddActor_inBlock`)
//!    and load the "still to spawn" bitfield (`mFI_GetMoveActorBitData`).
//! 3. Clear `g_fdinfo->born_actor`.
//! 4. Once the pad is initialised:
//!    - `setup_actor_flag`: delete stale ITEM-part actors, then walk all
//!      16x16 units of the block's FG items and spawn:
//!        * `NAME_TYPE_ITEM2` (0x8xxx) -> `move_obj_profile_table[id-0x8000]`,
//!          clearing the unit to `EMPTY_NO`;
//!        * `NAME_TYPE_PROPS` (0xAxxx) -> `props_profile_table[id-0xA000]`,
//!          clearing the unit to `EMPTY_NO` for snowmen (`SNOWMAN0..8`)
//!          and `RSV_NO` for everything else;
//!        * `NAME_TYPE_STRUCT` (0x5xxx) -> `structure_clip->setup_actor_proc`.
//!
//!      A failed spawn sets `setup_actor_flag` again so it retries next frame.
//!    - pending move actors: delete stale NPC-part actors and spawn each
//!      set bit of `move_actor_bitfield` (ITEM2 / ACTOR via profile tables,
//!      SPNPC / NPC via `npc_clip->setupActor_proc`); spawned bits clear.
//! 5. If the player is not wading between acres and no fade is active,
//!    `move_actor_list_exists_flag` resets.
//!
//! Units spawn at unit centres: `block_pos + 40*u + 20`
//! (`aBC_pos_table`, 20..620).
//!
//! ### Stale-actor deletion (`aBC_deleteActor_part`)
//! An actor is deleted only when **both** its block X and block Z are
//! valid (>= 0) and differ from the last and current block. Note the `&&`
//! between the axes: an actor that shares *either* coordinate with the
//! current/last block survives (retail quirk, preserved).
//!
//! ### Boat (`aBC_set_boat`)
//! Outside the title demo, in block (5,5) or (4,6) the GBA link state is
//! polled (`mGcgba_InitVar` once on entry): success places `BOAT` at
//! unit (5*16+5, 6*16+10), failure clears it, "transmitting" leaves it.
//! Leaving those blocks re-arms the first-contact init.
//!
//! ## PC-port divergences (ACGC-PC-Port)
//! - Borderless mode (`g_mPlib_wade_disabled`): deletion keeps every actor
//!   within one block (Chebyshev distance <= 1) of the current block and
//!   ignores the last block; spawning takes explicit `bx, bz` and reads FG
//!   via `mFI_BkNumtoUtFGTop` (NULL-checked) instead of `play->block_table`.
//! - Adds an `aBC_actor_ct` (port-only).

use std::ffi::c_int;

/// Units per block side (`UT_X_NUM`, `UT_Z_NUM`).
pub const UT_NUM: usize = 16;
/// Unit world size (`mFI_UT_WORLDSIZE_X_F`).
pub const UT_WORLDSIZE: f32 = 40.0;

/// `aBC_pos_table`: unit-centre offsets within a block.
pub const POS_TABLE: [f32; UT_NUM] = {
    let mut t = [0.0f32; UT_NUM];
    let mut i = 0;
    while i < UT_NUM {
        t[i] = UT_WORLDSIZE * i as f32 + UT_WORLDSIZE / 2.0;
        i += 1;
    }
    t
};

/// Item name ranges (`include/m_name_table.h`).
pub const EMPTY_NO: u16 = 0x0000;
pub const RSV_NO: u16 = 0xFFFF;
pub const ETC_START: u16 = 0x8000;
pub const MISC_ACTOR_START: u16 = 0x9000;
pub const ACTOR_PROP_START: u16 = 0xA000;
pub const SNOWMAN0: u16 = ACTOR_PROP_START + 8;
pub const SNOWMAN8: u16 = ACTOR_PROP_START + 16;
pub const STRUCTURE_START: u16 = 0x5800;
pub const BOAT: u16 = STRUCTURE_START + 79;

/// `NAME_TYPE_*` (`ITEM_NAME_GET_TYPE(n) = (n & 0xF000) >> 12`).
pub const NAME_TYPE_STRUCT: u8 = 5;
pub const NAME_TYPE_ITEM2: u8 = 8;
pub const NAME_TYPE_ACTOR: u8 = 9;
pub const NAME_TYPE_PROPS: u8 = 10;
pub const NAME_TYPE_SPNPC: u8 = 13;
pub const NAME_TYPE_NPC: u8 = 14;

/// Move-actor list length (`mFM_MOVE_ACTOR_NUM`).
pub const MOVE_ACTOR_NUM: usize = 16;

/// Blocks that poll the GBA for the island boat: (bx, bz).
pub const BOAT_BLOCKS: [(i32, i32); 2] = [(5, 5), (4, 6)];
/// Unit coordinates (`mFI_UtNum2UtFG` args) where the boat item lives.
pub const BOAT_UNIT: (i32, i32) = (5 * UT_NUM as i32 + 5, 6 * UT_NUM as i32 + 10);

/// `GBA2_GBA_STATE_*` (`include/GBA2/gba2.h`). Only SUCCESS and
/// TRANSMITTING are special-cased; ERROR and ALREADY_EXIST clear the boat.
pub const GBA_STATE_ERROR: c_int = -1;
pub const GBA_STATE_ALREADY_EXIST: c_int = 0;
pub const GBA_STATE_SUCCESS: c_int = 1;
pub const GBA_STATE_TRANSMITTING: c_int = 9;

#[inline]
pub const fn name_type(name: u16) -> u8 {
    ((name & 0xF000) >> 12) as u8
}

/// How a block FG unit is spawned by `aBC_setupActor`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitSpawn {
    /// Not an actor-backed unit.
    None,
    /// `move_obj_profile_table[index]`, unit cleared to `clear_item`.
    MoveObj { index: u16, clear_item: u16 },
    /// `props_profile_table[index]`, unit cleared to `clear_item`.
    Props { index: u16, clear_item: u16 },
    /// `structure_clip->setup_actor_proc` (FG left as is).
    Structure,
}

/// Classify one FG unit the way `aBC_setupActor`'s switch does.
pub fn classify_unit(item: u16) -> UnitSpawn {
    match name_type(item) {
        NAME_TYPE_ITEM2 => UnitSpawn::MoveObj { index: item - ETC_START, clear_item: EMPTY_NO },
        NAME_TYPE_PROPS => {
            let clear_item = if (SNOWMAN0..=SNOWMAN8).contains(&item) { EMPTY_NO } else { RSV_NO };
            UnitSpawn::Props { index: item - ACTOR_PROP_START, clear_item }
        }
        NAME_TYPE_STRUCT => UnitSpawn::Structure,
        _ => UnitSpawn::None,
    }
}

/// Spawn position of unit (ut_x, ut_z) in a block whose origin is
/// (base_x, base_z).
pub fn unit_world_pos(base_x: f32, base_z: f32, ut_x: usize, ut_z: usize) -> (f32, f32) {
    (base_x + POS_TABLE[ut_x & 15], base_z + POS_TABLE[ut_z & 15])
}

/// Retail `aBC_deleteActor_part` predicate (s8 block coords).
pub fn should_delete_retail(check: (i8, i8), last: (i8, i8), now: (i8, i8)) -> bool {
    let (cx, cz) = check;
    (cx >= 0 && cx != last.0 && cx != now.0) && (cz >= 0 && cz != last.1 && cz != now.1)
}

/// PC-port borderless predicate: delete beyond one block of `now`.
pub fn should_delete_borderless(check: (i8, i8), now: (i8, i8)) -> bool {
    let (cx, cz) = check;
    if cx < 0 || cz < 0 {
        return false;
    }
    let dx = (cx as i32 - now.0 as i32).abs();
    let dz = (cz as i32 - now.1 as i32).abs();
    dx > 1 || dz > 1
}

/// How a pending move actor is spawned by `aBC_setupMvActor`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoveActorSpawn {
    MoveObj { index: u16 },
    MiscActor { index: u16 },
    Npc,
    /// Unknown type: never spawns, bit stays set.
    Never,
}

pub fn classify_move_actor(name: u16) -> MoveActorSpawn {
    match name_type(name) {
        NAME_TYPE_ITEM2 => MoveActorSpawn::MoveObj { index: name - ETC_START },
        NAME_TYPE_ACTOR => MoveActorSpawn::MiscActor { index: name - MISC_ACTOR_START },
        NAME_TYPE_SPNPC | NAME_TYPE_NPC => MoveActorSpawn::Npc,
        _ => MoveActorSpawn::Never,
    }
}

/// `aBC_setupMvActor` bitfield bookkeeping: `born(i)` reports whether
/// slot `i` spawned; spawned bits are cleared. Slots are visited in order
/// 0..16 and only set bits are attempted.
pub fn step_move_actor_bits(mut bits: u16, mut born: impl FnMut(usize) -> bool) -> u16 {
    for i in 0..MOVE_ACTOR_NUM {
        if (bits >> i) & 1 == 1 && born(i) {
            bits &= !(1 << i);
        }
    }
    bits
}

/// Boat poll state carried in `BIRTH_CONTROL_ACTOR::boat_spawned`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BoatState {
    pub boat_spawned: bool,
}

/// Outcome of one `aBC_set_boat` frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BoatStep {
    /// Call `mGcgba_InitVar` because we just entered a boat block.
    pub init_on_enter: bool,
    /// Poll `mGcgba_ConnectEnabled` and update the boat unit.
    pub poll: bool,
}

impl BoatState {
    /// `aBC_chk_near_boat_block` + the else-branch of `aBC_set_boat`.
    pub fn step(&mut self, not_title_demo: bool, bx: i32, bz: i32) -> BoatStep {
        if not_title_demo && BOAT_BLOCKS.contains(&(bx, bz)) {
            let init_on_enter = !self.boat_spawned;
            self.boat_spawned = true;
            BoatStep { init_on_enter, poll: true }
        } else {
            self.boat_spawned = false;
            BoatStep { init_on_enter: false, poll: false }
        }
    }
}

/// New boat-unit item for a GBA link state (`aBC_set_boat` switch).
pub fn boat_item_for_state(state: c_int, current: u16) -> u16 {
    match state {
        GBA_STATE_SUCCESS => BOAT,
        GBA_STATE_TRANSMITTING => current,
        _ => EMPTY_NO,
    }
}

// ---------------------------------------------------------------------------
// C ABI
// ---------------------------------------------------------------------------

/// Classified unit: returns kind (0 none, 1 move obj, 2 props,
/// 3 structure) and writes table index / clear item.
#[no_mangle]
pub extern "C" fn pc_birth_control_classify_unit(item: u16, index: *mut u16, clear_item: *mut u16) -> c_int {
    let (kind, idx, clear) = match classify_unit(item) {
        UnitSpawn::None => (0, 0, EMPTY_NO),
        UnitSpawn::MoveObj { index, clear_item } => (1, index, clear_item),
        UnitSpawn::Props { index, clear_item } => (2, index, clear_item),
        UnitSpawn::Structure => (3, 0, EMPTY_NO),
    };
    unsafe {
        if let Some(p) = index.as_mut() {
            *p = idx;
        }
        if let Some(p) = clear_item.as_mut() {
            *p = clear;
        }
    }
    kind
}

/// `aBC_deleteActor_part` predicate; `borderless` selects the PC rule.
#[no_mangle]
pub extern "C" fn pc_birth_control_should_delete(
    check_bx: i8,
    check_bz: i8,
    last_bx: i8,
    last_bz: i8,
    now_bx: i8,
    now_bz: i8,
    borderless: c_int,
) -> c_int {
    if borderless != 0 {
        should_delete_borderless((check_bx, check_bz), (now_bx, now_bz)) as c_int
    } else {
        should_delete_retail((check_bx, check_bz), (last_bx, last_bz), (now_bx, now_bz)) as c_int
    }
}

/// Boat item for a GBA state.
#[no_mangle]
pub extern "C" fn pc_birth_control_boat_item(state: c_int, current: u16) -> u16 {
    boat_item_for_state(state, current)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pos_table_matches_decomp_comments() {
        assert_eq!(POS_TABLE[0], 20.0);
        assert_eq!(POS_TABLE[15], 620.0);
    }

    #[test]
    fn retail_delete_needs_both_axes_stale() {
        // Same column as the current block: survives.
        assert!(!should_delete_retail((2, 5), (1, 1), (2, 2)));
        assert!(should_delete_retail((4, 5), (1, 1), (2, 2)));
        assert!(!should_delete_retail((-1, 5), (1, 1), (2, 2)));
    }

    #[test]
    fn borderless_keeps_neighbours() {
        assert!(!should_delete_borderless((3, 3), (2, 2)));
        assert!(should_delete_borderless((4, 2), (2, 2)));
    }

    #[test]
    fn unit_classification() {
        assert_eq!(classify_unit(SNOWMAN0), UnitSpawn::Props { index: 8, clear_item: EMPTY_NO });
        assert_eq!(classify_unit(ACTOR_PROP_START), UnitSpawn::Props { index: 0, clear_item: RSV_NO });
        assert_eq!(classify_unit(0x8003), UnitSpawn::MoveObj { index: 3, clear_item: EMPTY_NO });
        assert_eq!(classify_unit(BOAT), UnitSpawn::Structure);
        assert_eq!(classify_unit(0x2000), UnitSpawn::None);
    }

    #[test]
    fn boat_init_once_per_entry() {
        let mut s = BoatState::default();
        assert!(s.step(true, 5, 5).init_on_enter);
        assert!(!s.step(true, 5, 5).init_on_enter);
        assert!(!s.step(true, 0, 0).poll);
        assert!(s.step(true, 4, 6).init_on_enter);
        assert!(!s.step(false, 4, 6).poll);
    }

    #[test]
    fn boat_states() {
        assert_eq!(boat_item_for_state(GBA_STATE_SUCCESS, EMPTY_NO), BOAT);
        assert_eq!(boat_item_for_state(GBA_STATE_TRANSMITTING, BOAT), BOAT);
        assert_eq!(boat_item_for_state(GBA_STATE_ALREADY_EXIST, BOAT), EMPTY_NO);
        assert_eq!(boat_item_for_state(GBA_STATE_ERROR, BOAT), EMPTY_NO);
    }

    #[test]
    fn move_bits_clear_on_birth() {
        assert_eq!(step_move_actor_bits(0b1011, |i| i != 1), 0b0010);
    }
}
