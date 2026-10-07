//! Per-scene NPC house state layouts for the Rust rewrite.
//!
//! Source-verified (upstream `include/m_npc.h`, `src/game/m_npc.c`,
//! `src/game/m_quest.c`):
//!
//! The game separates three state domains:
//!
//! * `Animal_c` — persistent NPC state (identity, memories, home
//!   position, mood). It does NOT hold the full house layout.
//! * `mNpc_NpcList_c` — runtime per-NPC/per-house state
//!   (`m_npc.h:268`): name, field_name, house_position, position,
//!   appear_flag, conversation_flags, quest_info, house_data, and
//!   reward_furniture. `mNpc_SetNpcList` populates `house_data` from
//!   `npc_house_list[npc_id & 0xFFF]`.
//! * The NPC actor itself holds BOTH: `mNpc_SetNpcinfo`
//!   (`m_npc.c:2852`) sets `npc->npc_info.animal` (persistent) and
//!   `npc->npc_info.list` (runtime house state) from one
//!   `npc_info_idx`; the island fallback uses `-ANIMAL_NUM_MAX`.
//!
//! Scene wiring:
//!
//! * `mNpc_AddNpc_inBlock` (`m_npc.c:2925`) dispatches on
//!   `SCENE_NPC_HOUSE`, `SCENE_KAMAKURA`, `SCENE_COTTAGE_NPC` into
//!   `mNpc_AddNpc_inNpcRoom` / `...Island`.
//! * `mNpc_AddNpc_inNpcRoom` (`m_npc.c:2876`): reads
//!   `house_owner_name`, resolves it with `mNpc_SearchAnimalinfo`,
//!   and places the move actor at unit (4, 7) with that
//!   `npc_info_idx`.
//! * `mNpc_RenewalNpcRoom`: for an `mFI_FIELD_NPCROOM0` field with a
//!   valid owner, the room's wall/floor come from
//!   `npclist->house_data.wall_id` / `floor_id`.
//!
//! Furniture scan shape (verbatim, `m_npc.c:3083`):
//! `data_idx = main_layer_id - fg_base_id` (clamped at 0),
//! `fg_items = fg_data_table[data_idx]->items[0]`, then a two-pass
//! 10x10 scan with row stride `UT_X_NUM - 10`: first pass counts
//! eligible furniture, `num = RANDOM(num)`, second pass returns the
//! num-th eligible item.
//!
//! Request dispatch (untraced implementation):
//! `mQst_NextSoccer` (`m_quest.c:980`) calls
//! `(*Common_Get(clip).npc_clip->force_call_req_proc)(npc_actor,
//! 0x0D8B + looks)` — a request-procedure ID built from a base plus
//! the NPC's looks category, returning success/failure. The function
//! behind the pointer is not recovered; it is modeled as a callback.

use crate::inventory::{selectable_furniture, ExcludedFurniture};
use crate::item_prefs::NpcHouseData;

/// Scene kinds that carry an NPC-house context
/// (`SCENE_NPC_HOUSE`, `SCENE_KAMAKURA`, `SCENE_COTTAGE_NPC`).
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HouseSceneKind {
    NpcHouse = 0,
    Kamakura = 1,
    CottageNpc = 2,
    Other = 3,
}

/// Runtime per-NPC/per-house state (`mNpc_NpcList_c`).
#[derive(Clone, Debug)]
pub struct NpcListEntry {
    pub name: u16,
    pub field_name: u16,
    pub house_position: [f32; 3],
    pub position: [f32; 3],
    pub appear_flag: u8,
    /// `mNpc_NpcConversation_c` — opaque here; layout untraced.
    pub conversation_flags: u32,
    /// `mQst_base_c` — opaque here; quest state lives in behavior.rs.
    pub quest_info: u32,
    pub house_data: NpcHouseData,
    pub reward_furniture: u16,
}

/// The NPC actor's dual state links (`npc_info.animal` /
/// `npc_info.list` in `mNpc_SetNpcinfo`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NpcActorLinks {
    /// Index into the persistent animals array.
    pub animal_idx: i32,
    /// Index into the runtime npclist array.
    pub list_idx: i32,
    /// True for the island fallback branch.
    pub is_island: bool,
}

/// Resolve the dual links from an `npc_info_idx`
/// (`mNpc_SetNpcinfo`): normal indices link both arrays at the same
/// slot; `-ANIMAL_NUM_MAX` selects the island fallback.
pub fn resolve_npc_links(npc_info_idx: i32, animal_num_max: i32) -> Option<NpcActorLinks> {
    if (0..animal_num_max).contains(&npc_info_idx) {
        Some(NpcActorLinks { animal_idx: npc_info_idx, list_idx: npc_info_idx, is_island: false })
    } else if npc_info_idx == -animal_num_max {
        Some(NpcActorLinks { animal_idx: -1, list_idx: 0, is_island: true })
    } else {
        None
    }
}

/// Resolve the house owner into an NPC index
/// (`mNpc_AddNpc_inNpcRoom`): `house_owner_name` searched in the
/// animals array; the move actor is placed at unit (4, 7).
/// Returns the npc index, or `None` for reserved/empty/joint-event owners.
pub fn resolve_house_owner(
    owner_id: u32,
    empty_no: u16,
    rsv_no: u16,
    search_animalinfo: &dyn Fn(u32) -> i32,
    is_joint_event: &dyn Fn(u32) -> bool,
) -> Option<(i32, u8, u8)> {
    if owner_id == rsv_no as u32 {
        return None;
    }
    if owner_id == empty_no as u32 || is_joint_event(owner_id) {
        return None;
    }
    let idx = search_animalinfo(owner_id);
    if idx == -1 {
        return None;
    }
    Some((idx, 4, 7))
}

/// Room wall/floor from the owner's runtime house data
/// (`mNpc_RenewalNpcRoom`): for an NPC-room field with a valid owner,
/// `(wall_id, floor_id)`; otherwise `None`.
pub fn renewal_npc_room(
    is_npc_room_field: bool,
    owner: Option<&NpcListEntry>,
) -> Option<(u8, u8)> {
    if !is_npc_room_field {
        return None;
    }
    let e = owner?;
    Some((e.house_data.wall_id as u8, e.house_data.floor_id as u8))
}

/// Verbatim two-pass house furniture scan
/// (`mNpc_DecideNpcFurniture`): `data_idx = main_layer_id -
/// fg_base_id` clamped at 0, then a 10x10 window walked with
/// `row_stride` cells per row (`fg_items += UT_X_NUM - 10` in the
/// source). First pass counts eligible furniture, `rng` selects one,
/// second pass returns it. With `row_stride == 10` this reduces to a
/// contiguous 10x10 scan.
pub fn scan_house_furniture(
    grid: &[u16],
    row_stride: usize,
    main_layer_id: u32,
    fg_base_id: i32,
    classify: &dyn Fn(u16) -> (bool, Option<ExcludedFurniture>),
    rng_value: u32,
) -> u16 {
    let data_idx = (main_layer_id as i32 - fg_base_id).max(0) as usize;
    let base = data_idx * row_stride * 10;
    let cell = |z: usize, x: usize| -> Option<u16> {
        grid.get(base + z * row_stride + x).copied()
    };

    let mut num = 0u32;
    for z in 0..10usize {
        for x in 0..10usize {
            if let Some(it) = cell(z, x) {
                if it != 0 {
                    let (is_ftr, excl) = classify(it);
                    if selectable_furniture(is_ftr, excl) {
                        num += 1;
                    }
                }
            }
        }
    }
    if num == 0 {
        return 0;
    }
    let mut sel = rng_value % num;
    for z in 0..10usize {
        for x in 0..10usize {
            if let Some(it) = cell(z, x) {
                if it != 0 {
                    let (is_ftr, excl) = classify(it);
                    if selectable_furniture(is_ftr, excl) {
                        if sel == 0 {
                            return it;
                        }
                        sel -= 1;
                    }
                }
            }
        }
    }
    0
}

/// Base request-procedure ID (`0x0D8B + looks` in `mQst_NextSoccer`).
pub const REQ_PROC_BASE: u32 = 0x0D8B;

/// Request-procedure ID for an NPC's looks category.
pub fn request_proc_id(looks: u8) -> u32 {
    REQ_PROC_BASE + looks as u32
}

/// The `force_call_req_proc` dispatcher: the implementation behind
/// the function pointer is untraced, so it is modeled as a caller
/// supplied callback `(npc_actor, req_id) -> success`.
pub fn force_call_req_proc(
    npc_actor: u32,
    req_id: u32,
    proc_: &dyn Fn(u32, u32) -> bool,
) -> bool {
    proc_(npc_actor, req_id)
}

/// C ABI: request-procedure ID for a looks category.
#[no_mangle]
pub extern "C" fn pc_request_proc_id(looks: u8) -> u32 {
    request_proc_id(looks)
}

/// C ABI: room wall/floor packed as `(wall << 8) | floor`, or -1 when
/// the scene/owner does not supply them.
#[no_mangle]
pub extern "C" fn pc_house_wall_floor(
    is_npc_room_field: i32,
    wall_id: u16,
    floor_id: u16,
    has_owner: i32,
) -> i32 {
    let s = house_surface_lookup(is_npc_room_field != 0, has_owner != 0, wall_id, floor_id);
    match s {
        Some(v) => (((v.wall_id & 0xFF) << 8) | (v.floor_id & 0xFF)) as i32,
        None => -1,
    }
}

/// C-compatible house surface for the ABI boundary.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PcHouseSurface {
    pub wall_id: u16,
    pub floor_id: u16,
}

/// Pure wall/floor lookup kernel: the C side gathers field type,
/// owner validity, and house data; Rust owns only the selection
/// (`mNpc_GetNpcFloorNo` / `mNpc_GetNpcWallNo` core).
pub fn house_surface_lookup(
    is_npc_room_field: bool,
    has_owner: bool,
    wall_id: u16,
    floor_id: u16,
) -> Option<PcHouseSurface> {
    if !is_npc_room_field || !has_owner {
        return None;
    }
    Some(PcHouseSurface { wall_id, floor_id })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn npc_links_branches() {
        let n = resolve_npc_links(3, 15).unwrap();
        assert_eq!((n.animal_idx, n.list_idx, n.is_island), (3, 3, false));
        let isl = resolve_npc_links(-15, 15).unwrap();
        assert!(isl.is_island);
        assert!(resolve_npc_links(15, 15).is_none());
        assert!(resolve_npc_links(-7, 15).is_none());
    }

    #[test]
    fn house_owner_resolution() {
        let search = |id: u32| if id == 42 { 5 } else { -1 };
        let joint = |_: u32| false;
        assert_eq!(resolve_house_owner(42, 0, 0xFFFF, &search, &joint), Some((5, 4, 7)));
        assert_eq!(resolve_house_owner(0, 0, 0xFFFF, &search, &joint), None);
        assert_eq!(resolve_house_owner(0xFFFF, 0, 0xFFFF, &search, &joint), None);
        assert_eq!(resolve_house_owner(99, 0, 0xFFFF, &search, &joint), None);
        let joint_true = |_: u32| true;
        assert_eq!(resolve_house_owner(42, 0, 0xFFFF, &search, &joint_true), None);
    }

    fn classify(item: u16) -> (bool, Option<ExcludedFurniture>) {
        match item {
            1 | 2 => (true, None),
            _ => (false, None),
        }
    }

    #[test]
    fn strided_scan_matches_source_shape() {
        // 10 rows of stride 12: eligible items at (0,0)=1 and (1,5)=2.
        let mut grid = vec![0u16; 2 * 12 * 10];
        grid[0] = 1;
        grid[12 + 5] = 2;
        assert_eq!(scan_house_furniture(&grid, 12, 0, 0, &classify, 0), 1);
        assert_eq!(scan_house_furniture(&grid, 12, 0, 0, &classify, 1), 2);
        // main_layer_id below base clamps to data_idx 0.
        assert_eq!(scan_house_furniture(&grid, 12, 3, 9, &classify, 0), 1);
        // Empty grid -> 0.
        let empty = vec![0u16; 12 * 10];
        assert_eq!(scan_house_furniture(&empty, 12, 0, 0, &classify, 0), 0);
    }

    #[test]
    fn req_proc_id_and_dispatch() {
        assert_eq!(request_proc_id(0), 0x0D8B);
        assert_eq!(request_proc_id(5), 0x0D90);
        let ok = |actor: u32, req: u32| actor == 7 && req == 0x0D8B;
        assert!(force_call_req_proc(7, 0x0D8B, &ok));
        assert!(!force_call_req_proc(7, 0x0D8C, &ok));
    }

    #[test]
    fn house_surface_kernel() {
        let s = house_surface_lookup(true, true, 0x12, 0x34).unwrap();
        assert_eq!((s.wall_id, s.floor_id), (0x12, 0x34));
        assert!(house_surface_lookup(false, true, 1, 2).is_none());
        assert!(house_surface_lookup(true, false, 1, 2).is_none());
        assert_eq!(pc_house_wall_floor(1, 0x12, 0x34, 1), 0x1234);
        assert_eq!(pc_house_wall_floor(0, 0x12, 0x34, 1), -1);
    }
}
