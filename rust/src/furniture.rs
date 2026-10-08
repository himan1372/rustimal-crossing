//! Furniture placement and house-interior runtime model.
//!
//! Source: GAFE01_00 Rev. 0 decomp, verified against the local tree.
//!
//! - `include/ac_furniture.h` — `FTR_ACTOR`, `aFTR_PROFILE`, shape/set/interaction
//!   enums, `aFTR_KEEP_ITEM_COUNT`, actor states.
//! - `src/actor/ac_my_room.c`, `src/actor/ac_my_room_move.c_inc` — My_Room actor,
//!   placement judge (`aMR_JudgeBreedNewFurniture`), footprint offset tables
//!   (`aMR_poccess_table`), save-back (`aMR_SetFurniture2FG`), switch save
//!   (`aMR_SaveSwitchData`), scene furniture limits (`aMR_GetSceneFurnitureMax`),
//!   parent/child fit tables.
//! - `include/m_room_type.h`, `src/game/m_room_type.c` — rotation encoding macros
//!   (`FTR_GET_ROTATION`, `FTR_IDX_2_NO`, `FTR_NO_2_IDX`, `FTR_NO_ROT_2_IDX`),
//!   `mRmTp_DIRECT_*`, `mRmTp_FTRSIZE_*`.
//! - `include/m_home_h.h` — `mHm_flr_c` (four FG layers + wall/floor +
//!   floor/wall original flags), `mHm_lyr_c` (`items`, `ftr_switch`, `haniwa_step`).
//! - `include/m_ftr_def.h` — furniture name indices (special orientation offsets).
//!
//! ## Architecture (retail)
//!
//! The saved house is a persistent foreground-grid representation; entering a
//! room deserializes it into runtime `FTR_ACTOR`s (layers 0 and 1 only), and
//! leaving the room serializes runtime state back. Storage furniture keeps its
//! three contained items in the higher FG layers while saved and in
//! `FTR_ACTOR.items[3]` while live. Furniture-on-furniture is an explicit
//! parent/child attachment (up to 4 fitted children), not two independent
//! actors. Switch (on/off) state is a `u64` bitfield keyed by 8x8 grid position,
//! not by actor identity.
//!
//! ## Confidence
//!
//! Source-proven: grid size 16x16, four layers, rotation bit encoding, footprint
//! offsets, RSV_FE1F reservation cells, 8x8 interior bounds, five-unit forward
//! placement search, player-facing initial orientation (+180 deg mapping), the
//! stego/balloon (+1) and frog (+2) orientation exceptions, NO_COLLISION
//! under-player placement, ON_SURFACE/SURFACE rules, three storage slots, the
//! get-save-angle sin/cos 0.8 quantization, 3 reservation slots / 46 frames,
//! the scene furniture-max table, weight always returning 1, the 17 actor
//! states, and the u64 switch bitfield with the (x-1)+(z-1)*8 mapping.
//!
//! Inferred: exact binary-angle mapping of the player's forward vector onto
//! grid cells in `aMR_GetPlayerLookAtUnit` (modeled here by quantizing the
//! placement direction); the numeric value of the place-table "free" sentinel
//! (200) and any other sentinel values beyond what the brief quotes; collision
//! edge geometry beyond profile-driven edges.

/// Unit grid dimensions of one foreground layer (`UT_X_NUM`/`UT_Z_NUM`
/// = `UT_BASE_NUM` = 16, `include/m_field_make.h`).
pub const UT_X_NUM: usize = 16;
pub const UT_Z_NUM: usize = 16;

/// Foreground layer indices (`mCoBG_LAYER*`).
pub mod layer {
    pub const MAIN: usize = 0;
    pub const SECONDARY: usize = 1;
    pub const STORAGE1: usize = 2;
    pub const STORAGE2: usize = 3;
    pub const NUM: usize = 4;
}

/// Empty foreground cell (`EMPTY_NO`, `include/m_name_table.h`).
pub const EMPTY_NO: u16 = 0x0000;
/// Reservation marker written into secondary cells of multi-unit furniture
/// (`RSV_FE1F`, `include/m_name_table.h`).
pub const RSV_FE1F: u16 = 0xFE1F;
/// Items kept per storage-capable furniture actor:
/// `aFTR_KEEP_ITEM_COUNT = mCoBG_LAYER_NUM - 1` = 3 (`include/ac_furniture.h`).
pub const FTR_KEEP_ITEM_COUNT: usize = 3;
/// Normal interior placement bounds (`aMR_MIN_BOUND`/`aMR_MAX_BOUND`).
pub const INTERIOR_MIN: i32 = 1;
pub const INTERIOR_MAX: i32 = 8;
/// Max fitted child furniture per parent (`aMR_FIT_FTR_MAX`).
pub const FIT_FTR_MAX: usize = 4;
/// Reservation slots for newly placed furniture (`aMR_RSV_FTR_NUM`).
pub const RSV_FTR_NUM: usize = 3;
/// Initial reservation/birth frame count (`ac_my_room_move.c_inc:1583`).
pub const RSV_FRAME_COUNT: u16 = 46;
/// Max runtime furniture actors in the largest rooms (large/upper/basement/cottage).
pub const MAX_ACTOR_SLOTS: usize = 64;

/// Cardinal furniture rotation (`mRmTp_DIRECT_*`, `include/m_room_type.h`).
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Direction {
    South = 0,
    East = 1,
    North = 2,
    West = 3,
}

/// `FTR_GET_ROTATION(f)`: low 2 bits of the saved item number.
pub fn ftr_get_rotation(item: u16) -> u8 {
    (item & 3) as u8
}
/// `FTR_IDX_2_NO(f)`: furniture index -> item-number base.
pub fn ftr_idx_2_no(idx: u16) -> u16 {
    idx >> 2
}
/// `FTR_NO_2_IDX(f)`: item-number base -> furniture index.
pub fn ftr_no_2_idx(no: u16) -> u16 {
    no << 2
}
/// `FTR_NO_ROT_2_IDX(f, rot)`: item-number base + rotation -> index form.
pub fn ftr_no_rot_2_idx(no: u16, rot: u8) -> u16 {
    (no << 2) | ((rot as u16) & 3)
}
/// `mRmTp_FtrIdx2FtrItemNo(idx, direct)`: index + cardinal direction -> saved item.
pub fn ftr_idx_2_item_no(idx: u16, dir: Direction) -> u16 {
    ((idx << 2) | (dir as u16)) & 0xFFFF
}
/// Inverse: saved item -> (furniture index, direction).
pub fn ftr_item_no_2_idx_dir(item: u16) -> (u16, Direction) {
    let rot = ftr_get_rotation(item);
    let dir = match rot {
        0 => Direction::South,
        1 => Direction::East,
        2 => Direction::North,
        _ => Direction::West,
    };
    (item >> 2, dir)
}

/// Runtime furniture shape (`aFTR_SHAPE_TYPE*`, `include/ac_furniture.h`).
///
/// The four `TypeB*` values are the four orientations of the 1x2 footprint.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShapeType {
    TypeB90 = 0,
    TypeB180 = 1,
    TypeB270 = 2,
    TypeB0 = 3,
    TypeA = 4,
    TypeC = 5,
}

/// Furniture size class (`mRmTp_FTRSIZE_*`, `include/m_room_type.h`).
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FurnitureSize {
    Size1x1 = 0,
    Size1x2 = 1,
    Size2x2 = 2,
}

impl ShapeType {
    pub fn size_class(self) -> FurnitureSize {
        match self {
            ShapeType::TypeA => FurnitureSize::Size1x1,
            ShapeType::TypeB90
            | ShapeType::TypeB180
            | ShapeType::TypeB270
            | ShapeType::TypeB0 => FurnitureSize::Size1x2,
            ShapeType::TypeC => FurnitureSize::Size2x2,
        }
    }

    /// Occupied unit offsets `(dx, dz)` relative to the primary cell.
    ///
    /// Source-verbatim from `aMR_poccess_table`
    /// (`src/actor/ac_my_room_move.c_inc:8-13`):
    /// B90 {0,-16}, B180 {0,-1}, B270 {0,+16}, B0 {0,+1}, A {0},
    /// C {0,1,16,17} with `ut = x + z*16`.
    pub fn unit_offsets(self) -> [(i32, i32); 4] {
        match self {
            ShapeType::TypeB90 => [(0, 0), (0, -1), (0, 0), (0, 0)],
            ShapeType::TypeB180 => [(0, 0), (-1, 0), (0, 0), (0, 0)],
            ShapeType::TypeB270 => [(0, 0), (0, 1), (0, 0), (0, 0)],
            ShapeType::TypeB0 => [(0, 0), (1, 0), (0, 0), (0, 0)],
            ShapeType::TypeA => [(0, 0), (0, 0), (0, 0), (0, 0)],
            ShapeType::TypeC => [(0, 0), (1, 0), (0, 1), (1, 1)],
        }
    }

    /// Number of occupied units.
    pub fn unit_count(self) -> usize {
        match self {
            ShapeType::TypeA => 1,
            ShapeType::TypeB90
            | ShapeType::TypeB180
            | ShapeType::TypeB270
            | ShapeType::TypeB0 => 2,
            ShapeType::TypeC => 4,
        }
    }

    /// Rotate a 1x2 shape 90 degrees clockwise (RROTATE path).
    /// Source: rotation changes `shape_type` among the four TypeB values.
    pub fn rotate_cw(self) -> ShapeType {
        match self {
            ShapeType::TypeB90 => ShapeType::TypeB180,
            ShapeType::TypeB180 => ShapeType::TypeB270,
            ShapeType::TypeB270 => ShapeType::TypeB0,
            ShapeType::TypeB0 => ShapeType::TypeB90,
            other => other,
        }
    }

    /// Rotate a 1x2 shape 90 degrees counter-clockwise (LROTATE path).
    pub fn rotate_ccw(self) -> ShapeType {
        match self {
            ShapeType::TypeB90 => ShapeType::TypeB0,
            ShapeType::TypeB180 => ShapeType::TypeB90,
            ShapeType::TypeB270 => ShapeType::TypeB180,
            ShapeType::TypeB0 => ShapeType::TypeB270,
            other => other,
        }
    }
}

/// Surface classification (`aFTR_SET_TYPE_*`, `include/ac_furniture.h`).
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SetType {
    Normal = 0,
    Surface = 1,
    OnSurface = 2,
}

/// Interaction bit flags (`aFTR_INTERACTION_*`, `include/ac_furniture.h`).
pub mod interaction {
    pub const NONE: u16 = 0;
    pub const STORAGE_DRAWERS: u16 = 1;
    pub const STORAGE_WARDROBE: u16 = 2;
    pub const STORAGE_CLOSET: u16 = 4;
    pub const MUSIC_DISK: u16 = 8;
    pub const NO_COLLISION: u16 = 0x10;
    pub const HANIWA: u16 = 0x20;
    pub const FISH: u16 = 0x40;
    pub const INSECT: u16 = 0x80;
    pub const MANNEKIN: u16 = 0x100;
    pub const UMBRELLA: u16 = 0x200;
    pub const FOSSIL: u16 = 0x400;
    pub const FAMICOM: u16 = 0x800;
    pub const START_DISABLED: u16 = 0x1000;
    pub const FAMICOM_ITEM: u16 = 0x2000;
    pub const RADIO_AEROBICS: u16 = 0x4000;
    pub const TOGGLE: u16 = 0x8000;
}

/// Contact-action bit flags (`aFTR_CONTACT_ACTION_*`, `include/ac_furniture.h`).
pub mod contact {
    pub const NONE: u8 = 0;
    pub const CHAIR_UNIDIRECTIONAL: u8 = 1;
    pub const CHAIR_MULTIDIRECTIONAL: u8 = 2;
    pub const CHAIR_SOFA: u8 = 4;
    pub const BED_SINGLE: u8 = 8;
    pub const BED_DOUBLE: u8 = 0x10;
}

/// `aFTR_IS_STORAGE(profile)`: drawers/wardrobe/closet/music-disk bits.
pub fn is_storage(interaction_type: u16) -> bool {
    interaction_type
        & (interaction::STORAGE_DRAWERS
            | interaction::STORAGE_WARDROBE
            | interaction::STORAGE_CLOSET
            | interaction::MUSIC_DISK)
        != 0
}

/// Static furniture definition (`aFTR_PROFILE`, `include/ac_furniture.h`).
///
/// `unique_behavior` stands in for the C `aFTR_vtable_c*`: set when the
/// furniture has per-type behavior callbacks. The actual callbacks are
/// runtime-owned elsewhere, so the profile only records their presence.
#[derive(Clone, Debug)]
pub struct FurnitureProfile {
    pub height: f32,
    pub scale: f32,
    pub shape: ShapeType,
    pub move_bg_type: u8,
    /// Non-zero => extra checks for items in the way during rotation.
    pub check_rotation: u8,
    pub kankyo_map: u8,
    pub contact_action: u8,
    pub interaction_type: u16,
    pub set_type: SetType,
    pub unique_behavior: bool,
}

/// One persistent foreground layer (`mHm_lyr_c`, `include/m_home_h.h`).
#[derive(Clone, Debug)]
pub struct RoomLayer {
    pub items: [[u16; UT_X_NUM]; UT_Z_NUM],
    /// On/off switch bitfield, keyed by 8x8 interior position.
    pub ftr_switch: u64,
    pub haniwa_step: [u32; 8],
}

impl Default for RoomLayer {
    fn default() -> Self {
        RoomLayer {
            items: [[EMPTY_NO; UT_X_NUM]; UT_Z_NUM],
            ftr_switch: 0,
            haniwa_step: [0; 8],
        }
    }
}

impl RoomLayer {
    /// `aMR_BOUNDS_OK`: the saved normal interior is the 8x8 region 1..=8.
    pub fn bounds_ok(x: i32, z: i32) -> bool {
        (INTERIOR_MIN..=INTERIOR_MAX).contains(&x) && (INTERIOR_MIN..=INTERIOR_MAX).contains(&z)
    }

    /// Switch-bit index for an interior cell: `(ut_x-1) + (ut_z-1)*8`
    /// (`aMR_SaveSwitchData`).
    pub fn switch_bit_index(x: i32, z: i32) -> Option<u8> {
        if Self::bounds_ok(x, z) {
            Some(((x - INTERIOR_MIN) + (z - INTERIOR_MIN) * 8) as u8)
        } else {
            None
        }
    }

    pub fn get(&self, x: i32, z: i32) -> Option<u16> {
        if (0..UT_X_NUM as i32).contains(&x) && (0..UT_Z_NUM as i32).contains(&z) {
            Some(self.items[z as usize][x as usize])
        } else {
            None
        }
    }

    pub fn set(&mut self, x: i32, z: i32, v: u16) -> bool {
        if (0..UT_X_NUM as i32).contains(&x) && (0..UT_Z_NUM as i32).contains(&z) {
            self.items[z as usize][x as usize] = v;
            true
        } else {
            false
        }
    }
}

/// Wall/floor indices per house floor (`mHm_wf_c`, `include/m_home_h.h`).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WallFloor {
    pub flooring_idx: u8,
    pub wallpaper_idx: u8,
}

/// Custom-design flags (`mHm_fllot_bit_c`, `include/m_home_h.h`).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FloorBitInfo {
    pub wall_original: bool,
    pub floor_original: bool,
}

/// One house floor: four FG layers plus surface data
/// (`mHm_flr_c`, `include/m_home_h.h`).
///
/// Layers 2/3 are the persistent "contents above furniture" layers that
/// storage-capable furniture converts into `FTR_ACTOR.items[3]` on room entry
/// and writes back on room exit.
#[derive(Clone, Debug, Default)]
pub struct HouseFloor {
    pub layer_main: RoomLayer,
    pub layer_secondary: RoomLayer,
    pub layer_storage1: RoomLayer,
    pub layer_storage2: RoomLayer,
    pub wall_floor: WallFloor,
    pub floor_bit_info: FloorBitInfo,
}

/// Runtime furniture actor state (`aFTR_STATE_*`, `include/ac_furniture.h`).
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FurnitureState {
    Stop = 0,
    WaitPush = 1,
    WaitPush2 = 2,
    WaitPush3 = 3,
    Push = 4,
    WaitPull = 5,
    WaitPull2 = 6,
    Pull = 7,
    WaitLRotate = 8,
    LRotate = 9,
    WaitRRotate = 10,
    RRotate = 11,
    BirthWait = 12,
    Birth = 13,
    Bye = 14,
    Death = 15,
}

/// One fitted child furniture record (`aMR_fit_ftr_c`).
///
/// Child position is stored relative to the parent so the child follows the
/// parent when it moves or rotates.
#[derive(Clone, Copy, Debug, Default)]
pub struct FitFurniture {
    pub exist: bool,
    pub ftr_id: i32,
    pub item_no: u16,
    pub angle_y: f32,
    /// Relative position to the parent actor.
    pub rel_pos: (f32, f32, f32),
    pub ut_x: i32,
    pub ut_z: i32,
}

/// Runtime furniture actor (`FTR_ACTOR`, `include/ac_furniture.h`).
///
/// Only the gameplay-persistent subset is modeled: position/animation details
/// (keyframes, skeletons, matrices) are deliberately excluded.
#[derive(Clone, Debug)]
pub struct FurnitureActor {
    pub name: u16,
    pub shape_type: ShapeType,
    pub original_shape_type: ShapeType,
    pub angle_y: f32,
    pub angle_y_target: f32,
    pub s_angle_y: i16,
    pub state: FurnitureState,
    pub switch_bit: u8,
    pub switch_changed: bool,
    pub haniwa_step: i8,
    pub haniwa_state: i16,
    pub birth_anim_counter: i16,
    /// Items held by storage-capable furniture (`items[aFTR_KEEP_ITEM_COUNT]`).
    pub items: [u16; FTR_KEEP_ITEM_COUNT],
    /// FG layer this actor was built from (0 or 1).
    pub layer: usize,
    pub cell_x: i32,
    pub cell_z: i32,
    pub demo_status: i16,
    pub dust_timer: i16,
    /// Fitted children when this actor is a surface parent.
    pub fit_children: [FitFurniture; FIT_FTR_MAX],
    pub dynamic_work_s: [i16; 5],
    pub dynamic_work_f: [f32; 2],
}

impl FurnitureActor {
    pub fn new(name: u16, shape: ShapeType, layer: usize, cell_x: i32, cell_z: i32) -> Self {
        FurnitureActor {
            name,
            shape_type: shape,
            original_shape_type: shape,
            angle_y: 0.0,
            angle_y_target: 0.0,
            s_angle_y: 0,
            state: FurnitureState::Stop,
            switch_bit: 0,
            switch_changed: false,
            haniwa_step: 0,
            haniwa_state: 0,
            birth_anim_counter: 0,
            items: [EMPTY_NO; FTR_KEEP_ITEM_COUNT],
            layer,
            cell_x,
            cell_z,
            demo_status: 0,
            dust_timer: 0,
            fit_children: [FitFurniture::default(); FIT_FTR_MAX],
            dynamic_work_s: [0; 5],
            dynamic_work_f: [0.0; 2],
        }
    }
}

/// Reservation slot for a furniture item waiting to be born
/// (`aMR_rsv_ftr_c`, 46-frame birth process).
#[derive(Clone, Copy, Debug, Default)]
pub struct FurnitureReservation {
    pub used: bool,
    pub ftr_name: u16,
    pub angle_idx: u8,
    pub layer: usize,
    pub ut_x: i32,
    pub ut_z: i32,
    pub frames: u16,
    pub initial_frames: u16,
}

/// Runtime room state (`l_aMR_work` + `MY_ROOM_ACTOR` furniture fields).
pub struct MyRoomRuntime {
    /// Actor budget for this scene (`aMR_GetSceneFurnitureMax`).
    pub list_size: usize,
    pub actors: Vec<Option<FurnitureActor>>,
    /// Unit -> actor slot occupying it (`aMR_place_table`), `None` = free.
    pub occupancy: [[Option<u16>; UT_X_NUM]; UT_Z_NUM],
    pub reservations: [FurnitureReservation; RSV_FTR_NUM],
}

impl MyRoomRuntime {
    pub fn new(list_size: usize) -> Self {
        let n = list_size.min(MAX_ACTOR_SLOTS);
        MyRoomRuntime {
            list_size: n,
            actors: vec![None; n],
            occupancy: [[None; UT_X_NUM]; UT_Z_NUM],
            reservations: [FurnitureReservation::default(); RSV_FTR_NUM],
        }
    }

    /// `aMR_CountFurniture()`: number of live actors.
    pub fn count_furniture(&self) -> usize {
        self.actors.iter().filter(|a| a.is_some()).count()
    }

    /// `aMR_SearchFreeFurnitureActorNumber()`: first free actor slot.
    pub fn search_free_actor(&self) -> Option<usize> {
        self.actors.iter().position(|a| a.is_none())
    }

    /// Occupy the footprint cells of `shape` at `(x, z)` with `actor_slot`.
    /// Returns false (leaving the table unchanged) if any cell is out of the
    /// 16x16 grid or already occupied.
    pub fn occupy(&mut self, shape: ShapeType, x: i32, z: i32, actor_slot: u16) -> bool {
        let mut cells = [(0i32, 0i32); 4];
        let n = shape.unit_count();
        for i in 0..n {
            let (dx, dz) = shape.unit_offsets()[i];
            let (cx, cz) = (x + dx, z + dz);
            if !(0..UT_X_NUM as i32).contains(&cx) || !(0..UT_Z_NUM as i32).contains(&cz) {
                return false;
            }
            if self.occupancy[cz as usize][cx as usize].is_some() {
                return false;
            }
            cells[i] = (cx, cz);
        }
        for i in 0..n {
            let (cx, cz) = cells[i];
            self.occupancy[cz as usize][cx as usize] = Some(actor_slot);
        }
        true
    }

    /// Release every cell owned by `actor_slot`.
    pub fn release(&mut self, actor_slot: u16) {
        for row in self.occupancy.iter_mut() {
            for cell in row.iter_mut() {
                if *cell == Some(actor_slot) {
                    *cell = None;
                }
            }
        }
    }
}

/// Room kinds for the scene furniture-max table.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SceneKind {
    NpcHouse,
    SmallPlayerRoom,
    MediumPlayerRoom,
    LargePlayerRoom,
    UpperRoom,
    Basement,
    Cottage,
    MuseumPainting,
    MuseumFossil,
    Shop,
}

/// `aMR_GetSceneFurnitureMax` (`src/actor/ac_my_room.c:1424`).
///
/// Source table (scene -> bank count): NPC house 30, shops 10, small 32,
/// medium 48, large 64, museum painting 20, museum fossil 25,
/// LL1/upper 64, LL2 48, basements 64, cottage (mine) 64, cottage (NPC) 30.
pub fn scene_furniture_max(kind: SceneKind) -> usize {
    match kind {
        SceneKind::NpcHouse => 30,
        SceneKind::SmallPlayerRoom => 32,
        SceneKind::MediumPlayerRoom => 48,
        SceneKind::LargePlayerRoom => 64,
        SceneKind::UpperRoom => 64,
        SceneKind::Basement => 64,
        SceneKind::Cottage => 64,
        SceneKind::MuseumPainting => 20,
        SceneKind::MuseumFossil => 25,
        SceneKind::Shop => 10,
    }
}

/// `aMR_GetWeight`: always returns 1 — the "weight" is effectively another
/// actor-count budget (`ac_my_room_move.c_inc`).
pub fn furniture_weight() -> u32 {
    1
}

/// `aMR_WeightPossible`: `weight + current_weight <= list_size`.
pub fn weight_possible(current_count: usize, list_size: usize) -> bool {
    current_count + furniture_weight() as usize <= list_size
}

/// Convert the player's binary-angle facing (`rotation.y`) into the initial
/// furniture `angle_idx`: `player_angle + 180deg`, then 45-135 -> EAST,
/// 135-225 -> NORTH, 225-315 -> WEST, else SOUTH
/// (`src/actor/ac_my_room_move.c_inc:1931-1946`).
pub fn player_facing_to_angle_idx(player_angle: u16) -> Direction {
    const DEG45: u16 = 0x2000;
    const DEG135: u16 = 0x6000;
    const DEG225: u16 = 0xA000;
    const DEG315: u16 = 0xE000;
    let angle = player_angle.wrapping_add(0x8000);
    if (DEG45..=DEG135).contains(&angle) {
        Direction::East
    } else if (DEG135..=DEG225).contains(&angle) {
        Direction::North
    } else if (DEG225..=DEG315).contains(&angle) {
        Direction::West
    } else {
        Direction::South
    }
}

/// Special orientation offsets (`ac_my_room_move.c_inc:1948-1954`).
///
/// Furniture indices are from `enum ftr_name` (`include/m_ftr_def.h`, first
/// entry = 0): stego skull = 964, balloons 1020..=1027, frog = 827.
pub mod special_furniture {
    pub const STEGO_HEAD: u16 = 964;
    pub const BALLOON_COMMON0: u16 = 1020;
    pub const BALLOON_COMMON7: u16 = 1027;
    pub const FROG: u16 = 827;
}

/// Extra `angle_idx` offset for special furniture: +1 for the stego skull and
/// balloons, +2 for the frog, else 0.
pub fn special_orientation_offset(ftr_idx: u16) -> i8 {
    if ftr_idx == special_furniture::STEGO_HEAD
        || (special_furniture::BALLOON_COMMON0..=special_furniture::BALLOON_COMMON7)
            .contains(&ftr_idx)
    {
        1
    } else if ftr_idx == special_furniture::FROG {
        2
    } else {
        0
    }
}

/// Candidate unit `i` steps in front of `(x, z)` along the placement direction
/// (the five-unit forward search, `i = 0..4`).
///
/// Retail computes this from the player's continuous world position/angle in
/// `aMR_GetPlayerLookAtUnit`; here the already-quantized placement direction
/// is used, which is the Rust model's documented simplification.
pub fn look_at_unit(x: i32, z: i32, dir: Direction, i: i32) -> (i32, i32) {
    match dir {
        Direction::South => (x, z + i),
        Direction::East => (x + i, z),
        Direction::North => (x, z - i),
        Direction::West => (x - i, z),
    }
}

/// Placement judge outcomes (`aMR_JUDGE_*`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlaceJudge {
    /// Success; carries the actor slot, cell, rotation, and layer.
    Success {
        actor_slot: usize,
        x: i32,
        z: i32,
        rotation: Direction,
        layer: usize,
    },
    CantPlace,
    MaxFurniture,
    OtherRoom,
}

/// Successful placement detail shared by the judge paths.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Placement {
    pub actor_slot: usize,
    pub x: i32,
    pub z: i32,
    pub rotation: Direction,
    pub layer: usize,
}

/// `aMR_CheckPlaceSituation` core: the footprint cells of `shape` at the
/// target unit must be inside the interior bounds, empty in the FG layer, and
/// free in the runtime occupancy table.
///
/// `fg` is the target layer's item grid; `occupied` marks runtime-owned cells.
pub fn check_place_situation(
    shape: ShapeType,
    x: i32,
    z: i32,
    fg: &RoomLayer,
    occupied: &[[bool; UT_X_NUM]; UT_Z_NUM],
) -> bool {
    let n = shape.unit_count();
    for i in 0..n {
        let (dx, dz) = shape.unit_offsets()[i];
        let (cx, cz) = (x + dx, z + dz);
        if !RoomLayer::bounds_ok(cx, cz) {
            return false;
        }
        if fg.get(cx, cz) != Some(EMPTY_NO) {
            return false;
        }
        if occupied[cz as usize][cx as usize] {
            return false;
        }
    }
    true
}

/// `aMR_JudgePlace2ndLayer`: a layer-1 item may be placed when layer 0 holds
/// furniture at the cell, that furniture is a surface, and the layer-1 cell is
/// empty.
pub fn judge_place_2nd_layer(below_is_surface: bool, layer1_empty: bool) -> bool {
    below_is_surface && layer1_empty
}

/// Judge placing furniture (`aMR_JudgeBreedNewFurniture` core).
///
/// Models: flat check, room-ownership/reservation check, actor budget,
/// free-slot search, player-facing initial orientation with special offsets,
/// the five-unit forward search for 1x2 shapes, the all-rotations fallback,
/// the ON_SURFACE-onto-surface path, and the NO_COLLISION under-player path.
///
/// `occupied` is the runtime occupancy grid (true = taken).
/// `surface_below` reports whether a layer-0 cell holds SURFACE furniture.
/// `set_type`/`no_collision` come from the furniture profile.
#[allow(clippy::too_many_arguments)]
pub fn judge_place_furniture(
    room: &MyRoomRuntime,
    player_cell: (i32, i32),
    player_angle: u16,
    ftr_idx: u16,
    shape: ShapeType,
    set_type: SetType,
    no_collision: bool,
    surface_below: &dyn Fn(i32, i32) -> bool,
    fg_layer0: &RoomLayer,
    fg_layer1: &RoomLayer,
    occupied: &[[bool; UT_X_NUM]; UT_Z_NUM],
    player_flat: bool,
    room_owned: bool,
    reserve_ok: bool,
) -> PlaceJudge {
    if !player_flat {
        return PlaceJudge::CantPlace;
    }
    if !room_owned || !reserve_ok {
        return PlaceJudge::OtherRoom;
    }
    if room.count_furniture() >= room.list_size {
        return PlaceJudge::MaxFurniture;
    }
    let actor_slot = match room.search_free_actor() {
        Some(s) => s,
        None => return PlaceJudge::MaxFurniture,
    };

    let mut angle_idx =
        (player_facing_to_angle_idx(player_angle) as i32 + special_orientation_offset(ftr_idx) as i32)
            & 3;
    let base_dir = match angle_idx {
        0 => Direction::South,
        1 => Direction::East,
        2 => Direction::North,
        _ => Direction::West,
    };
    let (px, pz) = player_cell;

    let is_1x2 = matches!(shape.size_class(), FurnitureSize::Size1x2);

    // 1x2 shapes: five-unit forward search with the base orientation, then the
    // all-rotations fallback (retail's @BUG-documented loop).
    if is_1x2 {
        for i in 0..5 {
            if !weight_possible(room.count_furniture(), room.list_size) {
                return PlaceJudge::MaxFurniture;
            }
            let (tx, tz) = look_at_unit(px, pz, base_dir, i);
            if check_place_situation(shape, tx, tz, fg_layer0, occupied) {
                return PlaceJudge::Success {
                    actor_slot,
                    x: tx,
                    z: tz,
                    rotation: base_dir,
                    layer: layer::MAIN,
                };
            }
            if no_collision && check_place_situation(shape, px, pz, fg_layer0, occupied) {
                return PlaceJudge::Success {
                    actor_slot,
                    x: px,
                    z: pz,
                    rotation: base_dir,
                    layer: layer::MAIN,
                };
            }
        }
        // Fallback: try every rotation at the forward-search cells.
        for j in 0..4u8 {
            let dir = match j {
                0 => Direction::South,
                1 => Direction::East,
                2 => Direction::North,
                _ => Direction::West,
            };
            let rot_shape = match j {
                1 => ShapeType::TypeB90,
                2 => ShapeType::TypeB180,
                3 => ShapeType::TypeB270,
                _ => ShapeType::TypeB0,
            };
            for i in 0..5 {
                let (tx, tz) = look_at_unit(px, pz, base_dir, i);
                if check_place_situation(rot_shape, tx, tz, fg_layer0, occupied) {
                    return PlaceJudge::Success {
                        actor_slot,
                        x: tx,
                        z: tz,
                        rotation: dir,
                        layer: layer::MAIN,
                    };
                }
                if no_collision && check_place_situation(rot_shape, px, pz, fg_layer0, occupied) {
                    return PlaceJudge::Success {
                        actor_slot,
                        x: px,
                        z: pz,
                        rotation: dir,
                        layer: layer::MAIN,
                    };
                }
            }
        }
        return PlaceJudge::CantPlace;
    }

    // 1x1 and 2x2 shapes.
    for i in 0..5 {
        angle_idx &= 3;
        if !weight_possible(room.count_furniture(), room.list_size) {
            return PlaceJudge::MaxFurniture;
        }
        let dir = match angle_idx {
            0 => Direction::South,
            1 => Direction::East,
            2 => Direction::North,
            _ => Direction::West,
        };
        let (tx, tz) = look_at_unit(px, pz, base_dir, i);

        // ON_SURFACE 1x1 items go to layer 1 above SURFACE furniture.
        if set_type == SetType::OnSurface && shape == ShapeType::TypeA {
            if fg_layer1.get(tx, tz) == Some(EMPTY_NO)
                && fg_layer0.get(tx, tz) != Some(EMPTY_NO)
                && surface_below(tx, tz)
            {
                return PlaceJudge::Success {
                    actor_slot,
                    x: tx,
                    z: tz,
                    rotation: dir,
                    layer: layer::SECONDARY,
                };
            }
        }

        if shape != ShapeType::TypeC {
            if check_place_situation(shape, tx, tz, fg_layer0, occupied) {
                return PlaceJudge::Success {
                    actor_slot,
                    x: tx,
                    z: tz,
                    rotation: dir,
                    layer: layer::MAIN,
                };
            }
            if no_collision && check_place_situation(shape, px, pz, fg_layer0, occupied) {
                return PlaceJudge::Success {
                    actor_slot,
                    x: px,
                    z: pz,
                    rotation: dir,
                    layer: layer::MAIN,
                };
            }
        } else {
            // 2x2: same checks; 2x2 offsets handled by the caller-provided
            // square offset in the full engine (retail `square_offset`).
            if check_place_situation(shape, tx, tz, fg_layer0, occupied) {
                return PlaceJudge::Success {
                    actor_slot,
                    x: tx,
                    z: tz,
                    rotation: dir,
                    layer: layer::MAIN,
                };
            }
        }
    }
    PlaceJudge::CantPlace
}

/// Quantize a runtime angle back to a saved cardinal rotation
/// (`aMR_GetSaveAngle`): `sin > 0.8` -> EAST, `sin < -0.8` -> WEST,
/// `cos > 0.8` -> SOUTH, `cos < -0.8` -> NORTH.
pub fn save_angle_quantize(sin_v: f32, cos_v: f32) -> Direction {
    if sin_v > 0.8 {
        Direction::East
    } else if sin_v < -0.8 {
        Direction::West
    } else if cos_v > 0.8 {
        Direction::South
    } else if cos_v < -0.8 {
        Direction::North
    } else {
        Direction::South
    }
}

/// Write a furniture actor back into its FG layer (`aMR_SetFurniture2FG`).
///
/// The primary cell gets `FTR_NO_ROT_2_IDX(ftr_idx, rotation)`; every other
/// occupied cell gets `RSV_FE1F`. With `on_flag == false` all cells are
/// cleared to `EMPTY_NO`.
pub fn set_furniture_to_fg(
    fg: &mut RoomLayer,
    ftr_idx: u16,
    shape: ShapeType,
    rotation: Direction,
    x: i32,
    z: i32,
    on_flag: bool,
) -> bool {
    let n = shape.unit_count();
    let mut cells = [(0i32, 0i32); 4];
    for i in 0..n {
        let (dx, dz) = shape.unit_offsets()[i];
        cells[i] = (x + dx, z + dz);
    }
    let primary_item = ftr_no_rot_2_idx(ftr_idx, rotation as u8);
    for i in 0..n {
        let (cx, cz) = cells[i];
        let v = if on_flag {
            if i == 0 {
                primary_item
            } else {
                RSV_FE1F
            }
        } else {
            EMPTY_NO
        };
        if !fg.set(cx, cz, v) {
            return false;
        }
    }
    true
}

/// Scan the 8x8 interior of a layer and rebuild the switch bitfield from the
/// runtime actors (`aMR_SaveSwitchData` core).
///
/// `switch_at(x, z)` returns the actor's switch bit at that interior cell, if
/// a switch-capable actor occupies it.
pub fn save_switch_data(layer: &mut RoomLayer, switch_at: &dyn Fn(i32, i32) -> Option<u8>) {
    layer.ftr_switch = 0;
    for uz in INTERIOR_MIN..=INTERIOR_MAX {
        for ux in INTERIOR_MIN..=INTERIOR_MAX {
            if let Some(bit) = switch_at(ux, uz) {
                if let Some(idx) = RoomLayer::switch_bit_index(ux, uz) {
                    if bit != 0 {
                        layer.ftr_switch |= 1u64 << idx;
                    }
                }
            }
        }
    }
}

/// Restore an actor's switch bit from the layer bitfield when the room loads.
pub fn restore_switch_bit(layer: &RoomLayer, x: i32, z: i32) -> u8 {
    match RoomLayer::switch_bit_index(x, z) {
        Some(idx) => ((layer.ftr_switch >> idx) & 1) as u8,
        None => 0,
    }
}

/// Build runtime actors from saved layers 0 and 1 (`aMR_MakeFurnitureActor`
/// for `mCoBG_LAYER0`/`mCoBG_LAYER1`).
///
/// Returns `(actor_index, ftr_idx, rotation, layer)` entries in scan order;
/// `RSV_FE1F` secondary cells are skipped since they belong to the primary
/// cell's actor.
pub fn make_furniture_actors_from_layers(
    layer0: &RoomLayer,
    layer1: &RoomLayer,
) -> Vec<(u16, Direction, usize)> {
    let mut out = Vec::new();
    for (layer_idx, layer) in [layer0, layer1].iter().enumerate() {
        for z in 0..UT_Z_NUM as i32 {
            for x in 0..UT_X_NUM as i32 {
                let item = layer.get(x, z).unwrap_or(EMPTY_NO);
                if item != EMPTY_NO && item != RSV_FE1F {
                    let (idx, dir) = ftr_item_no_2_idx_dir(item);
                    out.push((idx, dir, layer_idx));
                }
            }
        }
    }
    out
}

/// Return storage furniture contents to the higher FG layers on room exit
/// (`aMR_KeepItem2Fg` core).
///
/// `stored` holds up to 3 items; they are written into the first free cells
/// of the storage layers. Returns the number of items written back.
pub fn keep_items_to_fg(
    stored: &[u16; FTR_KEEP_ITEM_COUNT],
    storage1: &mut RoomLayer,
    storage2: &mut RoomLayer,
) -> usize {
    let mut written = 0;
    let mut layers: [&mut RoomLayer; 2] = [storage1, storage2];
    for &item in stored.iter() {
        if item == EMPTY_NO {
            continue;
        }
        let mut placed = false;
        for layer in layers.iter_mut() {
            'scan: for z in 0..UT_Z_NUM as i32 {
                for x in 0..UT_X_NUM as i32 {
                    if layer.get(x, z) == Some(EMPTY_NO) {
                        layer.set(x, z, item);
                        placed = true;
                        break 'scan;
                    }
                }
            }
            if placed {
                break;
            }
        }
        if placed {
            written += 1;
        }
    }
    written
}

/// Reserve a furniture birth slot (`aMR_ReserveFurniture`).
/// Returns the reservation index, or `None` when all 3 slots are busy.
pub fn reserve_furniture(
    reservations: &mut [FurnitureReservation; RSV_FTR_NUM],
    ftr_name: u16,
    angle_idx: u8,
    layer: usize,
    x: i32,
    z: i32,
) -> Option<usize> {
    let slot = reservations.iter().position(|r| !r.used)?;
    reservations[slot] = FurnitureReservation {
        used: true,
        ftr_name,
        angle_idx: angle_idx & 3,
        layer,
        ut_x: x,
        ut_z: z,
        frames: RSV_FRAME_COUNT,
        initial_frames: RSV_FRAME_COUNT,
    };
    Some(slot)
}

/// C ABI: low-2-bit rotation of a saved furniture item (`FTR_GET_ROTATION`).
#[no_mangle]
pub extern "C" fn pc_ftr_get_rotation(item: u16) -> u8 {
    ftr_get_rotation(item)
}

/// C ABI: build a saved item number from furniture index + rotation
/// (`FTR_NO_ROT_2_IDX`).
#[no_mangle]
pub extern "C" fn pc_ftr_no_rot_2_idx(ftr_no: u16, rot: u8) -> u16 {
    ftr_no_rot_2_idx(ftr_no, rot)
}

/// C ABI: quantize (sin, cos) of a runtime angle to a saved cardinal
/// direction (`aMR_GetSaveAngle`); returns the `mRmTp_DIRECT_*` value.
#[no_mangle]
pub extern "C" fn pc_ftr_save_angle(sin_v: f32, cos_v: f32) -> u8 {
    save_angle_quantize(sin_v, cos_v) as u8
}

/// C ABI: number of storage slots per storage-capable furniture actor
/// (`aFTR_KEEP_ITEM_COUNT`).
#[no_mangle]
pub extern "C" fn pc_furniture_storage_slots() -> usize {
    FTR_KEEP_ITEM_COUNT
}

/// C ABI: runtime furniture actor budget for a room kind
/// (`aMR_GetSceneFurnitureMax`). Kinds: 0=NPC house, 1=small, 2=medium,
/// 3=large, 4=upper, 5=basement, 6=cottage, 7=museum painting,
/// 8=museum fossil, 9=shop.
#[no_mangle]
pub extern "C" fn pc_scene_furniture_max(kind: u8) -> usize {
    let k = match kind {
        0 => SceneKind::NpcHouse,
        1 => SceneKind::SmallPlayerRoom,
        2 => SceneKind::MediumPlayerRoom,
        3 => SceneKind::LargePlayerRoom,
        4 => SceneKind::UpperRoom,
        5 => SceneKind::Basement,
        6 => SceneKind::Cottage,
        7 => SceneKind::MuseumPainting,
        8 => SceneKind::MuseumFossil,
        _ => SceneKind::Shop,
    };
    scene_furniture_max(k)
}

/// C ABI: 1 when `(x, z)` is inside the 8x8 normal interior (`aMR_BOUNDS_OK`).
#[no_mangle]
pub extern "C" fn pc_room_bounds_ok(x: i32, z: i32) -> u8 {
    RoomLayer::bounds_ok(x, z) as u8
}

/// C ABI: switch-bit index for an interior cell, or 0xFF when out of bounds.
#[no_mangle]
pub extern "C" fn pc_switch_bit_index(x: i32, z: i32) -> u8 {
    RoomLayer::switch_bit_index(x, z).unwrap_or(0xFF)
}

/// C ABI: second-layer placement query (`aMR_JudgePlace2ndLayer`).
#[no_mangle]
pub extern "C" fn pc_judge_place_2nd_layer(below_is_surface: u8, layer1_empty: u8) -> u8 {
    judge_place_2nd_layer(below_is_surface != 0, layer1_empty != 0) as u8
}

/// C ABI: storage predicate (`aFTR_IS_STORAGE`).
#[no_mangle]
pub extern "C" fn pc_ftr_is_storage(interaction_type: u16) -> u8 {
    is_storage(interaction_type) as u8
}

/// C ABI: furniture weight (`aMR_GetWeight` always returns 1).
#[no_mangle]
pub extern "C" fn pc_furniture_weight() -> u32 {
    furniture_weight()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotation_encoding_roundtrip() {
        for idx in [0u16, 7, 964, 1027] {
            for (rot, dir) in [
                (0u8, Direction::South),
                (1, Direction::East),
                (2, Direction::North),
                (3, Direction::West),
            ] {
                let item = ftr_idx_2_item_no(idx, dir);
                assert_eq!(ftr_get_rotation(item), rot);
                assert_eq!(ftr_no_rot_2_idx(ftr_idx_2_no(item), rot), item);
                let (back_idx, back_dir) = ftr_item_no_2_idx_dir(item);
                assert_eq!(back_idx, idx);
                assert_eq!(back_dir, dir);
            }
        }
        assert_eq!(pc_ftr_get_rotation(0x0F05), 1);
        assert_eq!(pc_ftr_no_rot_2_idx(0x3C1, 2), 0x0F06);
    }

    #[test]
    fn footprint_tables_match_source() {
        // aMR_poccess_table: B90 {0,-16}, B180 {0,-1}, B270 {0,+16}, B0 {0,+1}.
        assert_eq!(ShapeType::TypeB90.unit_count(), 2);
        assert_eq!(ShapeType::TypeB90.unit_offsets()[1], (0, -1));
        assert_eq!(ShapeType::TypeB180.unit_offsets()[1], (-1, 0));
        assert_eq!(ShapeType::TypeB270.unit_offsets()[1], (0, 1));
        assert_eq!(ShapeType::TypeB0.unit_offsets()[1], (1, 0));
        assert_eq!(ShapeType::TypeA.unit_count(), 1);
        assert_eq!(ShapeType::TypeC.unit_count(), 4);
        assert_eq!(
            ShapeType::TypeC.unit_offsets(),
            [(0, 0), (1, 0), (0, 1), (1, 1)]
        );
        // Rotation cycles the four 1x2 orientations.
        let mut s = ShapeType::TypeB90;
        for _ in 0..4 {
            s = s.rotate_cw();
        }
        assert_eq!(s, ShapeType::TypeB90);
        assert_eq!(ShapeType::TypeB90.rotate_ccw(), ShapeType::TypeB0);
        assert_eq!(ShapeType::TypeA.rotate_cw(), ShapeType::TypeA);
        assert_eq!(ShapeType::TypeC.rotate_ccw(), ShapeType::TypeC);
    }

    #[test]
    fn facing_and_special_offsets() {
        // player_angle + 180deg: 45-135 -> EAST, 135-225 -> NORTH, 225-315 -> WEST.
        assert_eq!(player_facing_to_angle_idx(0x0000), Direction::North); // +180 = 0x8000
        assert_eq!(player_facing_to_angle_idx(0x8000), Direction::South); // +180 wraps to 0
        assert_eq!(player_facing_to_angle_idx(0xC000), Direction::East); // +180 = 0x4000
        assert_eq!(player_facing_to_angle_idx(0x4000), Direction::West); // +180 = 0xC000
        assert_eq!(special_orientation_offset(special_furniture::STEGO_HEAD), 1);
        assert_eq!(special_orientation_offset(special_furniture::BALLOON_COMMON0), 1);
        assert_eq!(special_orientation_offset(special_furniture::BALLOON_COMMON7), 1);
        assert_eq!(special_orientation_offset(special_furniture::FROG), 2);
        assert_eq!(special_orientation_offset(0), 0);
    }

    #[test]
    fn save_angle_quantization() {
        assert_eq!(save_angle_quantize(0.9, 0.1), Direction::East);
        assert_eq!(save_angle_quantize(-0.9, 0.1), Direction::West);
        assert_eq!(save_angle_quantize(0.1, 0.9), Direction::South);
        assert_eq!(save_angle_quantize(0.1, -0.9), Direction::North);
        // At/below threshold falls through.
        assert_eq!(save_angle_quantize(0.8, 0.6), Direction::South);
        assert_eq!(pc_ftr_save_angle(0.9, 0.0), Direction::East as u8);
    }

    #[test]
    fn fg_serialization_uses_reservation_cells() {
        let mut fg = RoomLayer::default();
        // 2x2 at (4,4): primary gets the item, others RSV_FE1F.
        assert!(set_furniture_to_fg(
            &mut fg,
            100,
            ShapeType::TypeC,
            Direction::East,
            4,
            4,
            true
        ));
        assert_eq!(fg.get(4, 4), Some(ftr_no_rot_2_idx(100, 1)));
        assert_eq!(fg.get(5, 4), Some(RSV_FE1F));
        assert_eq!(fg.get(4, 5), Some(RSV_FE1F));
        assert_eq!(fg.get(5, 5), Some(RSV_FE1F));
        // Clearing writes EMPTY_NO everywhere.
        assert!(set_furniture_to_fg(
            &mut fg,
            100,
            ShapeType::TypeC,
            Direction::East,
            4,
            4,
            false
        ));
        assert_eq!(fg.get(4, 4), Some(EMPTY_NO));
        assert_eq!(fg.get(5, 5), Some(EMPTY_NO));
        // Deserialization skips RSV_FE1F cells.
        let mut fg2 = RoomLayer::default();
        set_furniture_to_fg(&mut fg2, 100, ShapeType::TypeC, Direction::East, 4, 4, true);
        let actors = make_furniture_actors_from_layers(&fg2, &RoomLayer::default());
        assert_eq!(actors.len(), 1);
        assert_eq!(actors[0], (100, Direction::East, layer::MAIN));
    }

    #[test]
    fn switch_bitfield_roundtrip() {
        let mut layer = RoomLayer::default();
        // Actor at (2,3) with switch on -> bit (2-1)+(3-1)*8 = 17.
        save_switch_data(&mut layer, &|x, z| {
            if x == 2 && z == 3 {
                Some(1)
            } else {
                None
            }
        });
        assert_eq!(layer.ftr_switch, 1u64 << 17);
        assert_eq!(restore_switch_bit(&layer, 2, 3), 1);
        assert_eq!(restore_switch_bit(&layer, 1, 1), 0);
        assert_eq!(RoomLayer::switch_bit_index(8, 8), Some(63));
        assert_eq!(RoomLayer::switch_bit_index(1, 1), Some(0));
        assert_eq!(RoomLayer::switch_bit_index(0, 1), None);
        assert_eq!(pc_switch_bit_index(8, 8), 63);
        assert_eq!(pc_switch_bit_index(9, 1), 0xFF);
    }

    #[test]
    fn placement_judge_paths() {
        let room = MyRoomRuntime::new(64);
        let fg0 = RoomLayer::default();
        let fg1 = RoomLayer::default();
        let occupied = [[false; UT_X_NUM]; UT_Z_NUM];
        let surf = |_: i32, _: i32| false;
        // Player at (4,4) facing north (angle 0 -> furniture faces north, i.e.
        // placed toward -z). First forward cell (4,4) is free.
        let r = judge_place_furniture(
            &room,
            (4, 4),
            0x0000,
            10,
            ShapeType::TypeA,
            SetType::Normal,
            false,
            &surf,
            &fg0,
            &fg1,
            &occupied,
            true,
            true,
            true,
        );
        match r {
            PlaceJudge::Success { x, z, rotation, layer, .. } => {
                assert_eq!((x, z), (4, 4));
                assert_eq!(rotation, Direction::North);
                assert_eq!(layer, layer::MAIN);
            }
            _ => panic!("expected success"),
        }
        // Not flat -> CantPlace.
        assert_eq!(
            judge_place_furniture(
                &room, (4, 4), 0, 10, ShapeType::TypeA, SetType::Normal, false, &surf,
                &fg0, &fg1, &occupied, false, true, true
            ),
            PlaceJudge::CantPlace
        );
        // Not owned -> OtherRoom.
        assert_eq!(
            judge_place_furniture(
                &room, (4, 4), 0, 10, ShapeType::TypeA, SetType::Normal, false, &surf,
                &fg0, &fg1, &occupied, true, false, true
            ),
            PlaceJudge::OtherRoom
        );
        // Full budget -> MaxFurniture.
        let full = MyRoomRuntime::new(0);
        assert_eq!(
            judge_place_furniture(
                &full, (4, 4), 0, 10, ShapeType::TypeA, SetType::Normal, false, &surf,
                &fg0, &fg1, &occupied, true, true, true
            ),
            PlaceJudge::MaxFurniture
        );
        // ON_SURFACE above a surface goes to layer 1.
        let mut fg0s = RoomLayer::default();
        fg0s.set(4, 4, ftr_no_rot_2_idx(50, 0));
        let surf_yes = |x: i32, z: i32| x == 4 && z == 4;
        let r2 = judge_place_furniture(
            &room,
            (4, 4),
            0x0000,
            60,
            ShapeType::TypeA,
            SetType::OnSurface,
            false,
            &surf_yes,
            &fg0s,
            &fg1,
            &occupied,
            true,
            true,
            true,
        );
        match r2 {
            PlaceJudge::Success { layer, .. } => assert_eq!(layer, layer::SECONDARY),
            _ => panic!("expected surface placement"),
        }
        // Occupied forward cells -> CantPlace (interior bounds also enforced).
        let mut occ = [[false; UT_X_NUM]; UT_Z_NUM];
        for z in 1..=8i32 {
            for x in 1..=8i32 {
                occ[z as usize][x as usize] = true;
            }
        }
        assert_eq!(
            judge_place_furniture(
                &room, (4, 4), 0, 10, ShapeType::TypeA, SetType::Normal, false, &surf,
                &fg0, &fg1, &occ, true, true, true
            ),
            PlaceJudge::CantPlace
        );
    }

    #[test]
    fn occupancy_and_reservations() {
        let mut room = MyRoomRuntime::new(64);
        assert!(room.occupy(ShapeType::TypeB0, 4, 4, 7));
        assert_eq!(room.occupancy[4][4], Some(7));
        assert_eq!(room.occupancy[4][5], Some(7));
        // Overlap rejected.
        assert!(!room.occupy(ShapeType::TypeA, 5, 4, 8));
        room.release(7);
        assert_eq!(room.occupancy[4][4], None);
        assert!(room.occupy(ShapeType::TypeA, 5, 4, 8));

        let mut rsv = [FurnitureReservation::default(); RSV_FTR_NUM];
        assert_eq!(reserve_furniture(&mut rsv, 100, 1, 0, 4, 4), Some(0));
        assert_eq!(reserve_furniture(&mut rsv, 101, 2, 0, 5, 5), Some(1));
        assert_eq!(reserve_furniture(&mut rsv, 102, 3, 0, 6, 6), Some(2));
        assert_eq!(reserve_furniture(&mut rsv, 103, 0, 0, 7, 7), None);
        assert_eq!(rsv[0].frames, RSV_FRAME_COUNT);
        assert_eq!(rsv[0].initial_frames, 46);
    }

    #[test]
    fn scene_max_and_misc_abi() {
        assert_eq!(scene_furniture_max(SceneKind::NpcHouse), 30);
        assert_eq!(scene_furniture_max(SceneKind::SmallPlayerRoom), 32);
        assert_eq!(scene_furniture_max(SceneKind::MediumPlayerRoom), 48);
        assert_eq!(scene_furniture_max(SceneKind::LargePlayerRoom), 64);
        assert_eq!(scene_furniture_max(SceneKind::UpperRoom), 64);
        assert_eq!(scene_furniture_max(SceneKind::Basement), 64);
        assert_eq!(scene_furniture_max(SceneKind::Cottage), 64);
        assert_eq!(scene_furniture_max(SceneKind::MuseumPainting), 20);
        assert_eq!(scene_furniture_max(SceneKind::MuseumFossil), 25);
        assert_eq!(scene_furniture_max(SceneKind::Shop), 10);
        assert_eq!(pc_scene_furniture_max(3), 64);
        assert_eq!(pc_scene_furniture_max(9), 10);
        assert_eq!(pc_furniture_storage_slots(), 3);
        assert_eq!(pc_ftr_is_storage(interaction::STORAGE_DRAWERS), 1);
        assert_eq!(pc_ftr_is_storage(interaction::TOGGLE), 0);
        assert_eq!(pc_furniture_weight(), 1);
        assert!(weight_possible(63, 64));
        assert!(!weight_possible(64, 64));
        assert_eq!(pc_judge_place_2nd_layer(1, 1), 1);
        assert_eq!(pc_judge_place_2nd_layer(1, 0), 0);
        assert_eq!(pc_judge_place_2nd_layer(0, 1), 0);
        assert_eq!(pc_room_bounds_ok(1, 1), 1);
        assert_eq!(pc_room_bounds_ok(8, 8), 1);
        assert_eq!(pc_room_bounds_ok(0, 8), 0);
        assert_eq!(pc_room_bounds_ok(9, 9), 0);
    }

    #[test]
    fn storage_roundtrip_to_fg() {
        let mut s1 = RoomLayer::default();
        let mut s2 = RoomLayer::default();
        let stored = [0x2101u16, 0x2202, EMPTY_NO];
        let n = keep_items_to_fg(&stored, &mut s1, &mut s2);
        assert_eq!(n, 2);
        assert_eq!(s1.get(0, 0), Some(0x2101));
        assert_eq!(s1.get(1, 0), Some(0x2202));
        // Full storage layers -> nothing more written.
        for z in 0..UT_Z_NUM as i32 {
            for x in 0..UT_X_NUM as i32 {
                s1.set(x, z, 0x1111);
                s2.set(x, z, 0x2222);
            }
        }
        let n2 = keep_items_to_fg(&stored, &mut s1, &mut s2);
        assert_eq!(n2, 0);
    }
}
