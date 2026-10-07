//! Deterministic town-layout generation for the Rust game rewrite.
//!
//! The exported record is a rewrite-owned format. It intentionally does not
//! alias the decompilation's bitfields or save structures.
//!
//! Source-verified architecture (upstream `m_random_field.c` /
//! `m_random_field_ovl.c`, confirmed against the 2026 town-generation
//! research):
//!
//! * The town is 5 x 6 acres of 16 x 16 units. It is NOT a noise heightmap:
//!   it is constrained assembly of authored terrain blocks whose properties
//!   live in lookup tables (`BLOCK_KIND_TABLE`, mirroring `mRF_block_info`),
//!   not in per-acre attached metadata.
//! * Generation runs in phases: base landform (cliffs + river) -> beach ->
//!   bridges/slopes -> tailor/dock -> wishing well/police/museum ->
//!   shop/post office -> lake -> height table -> acre selection.
//! * Gates are resolved later from `(block type, direction)` via
//!   `GATE_TABLE` (`mRF_gate_info2`), never attached to blocks up front.
//! * Rejection sampling: a complete candidate town is built, then the
//!   `PERFECT_*` placement bits are checked (`is_valid_town`). An invalid
//!   candidate is discarded and generation restarts from the RNG stream.
//!   Repeated rejection is what causes the occasional long black screen
//!   during town creation on real hardware.
//! * Three-step (three-tier) towns are chosen 15% of the time
//!   (`mRF_GetRandomStepMode`: `GetRandom(100) < 15`).
//!
//! Rewrite-owned simplifications (documented, not source claims):
//! the cliff/river tracer uses boundary signatures instead of the source's
//! block-chain lookup tables and authored step-3 templates; grass patterns
//! are semantic selectors; no acre artwork or authored byte tables are
//! reproduced.

use std::slice;

pub const ACRE_WIDTH: usize = 5;
pub const ACRE_DEPTH: usize = 6;
pub const ACRE_COUNT: usize = ACRE_WIDTH * ACRE_DEPTH;
pub const UNITS_PER_ACRE: usize = 16 * 16;
pub const INITIAL_STARTERS: usize = 6;
pub const LOOK_CLASS_COUNT: usize = 6;
pub const MAX_TOWN_VILLAGERS: usize = 15;
const MAX_VILLAGER_CANDIDATES: usize = 4096;
const UNITS_PER_SIDE: usize = 16;
const HOUSE_RADIUS: usize = 1;

const NORTH: u8 = 1;
const EAST: u8 = 2;
const SOUTH: u8 = 4;
const WEST: u8 = 8;
const INFRA_BRIDGE: u8 = 1;
const INFRA_POND: u8 = 2;
const INFRA_SLOPE: u8 = 4;

// ---------------------------------------------------------------------------
// Source-verified generator tables.
// ---------------------------------------------------------------------------
// The tables below mirror the lookup-table architecture of the decompiled
// generator (`src/game/m_random_field.c`, `src/game/m_random_field_ovl.c`,
// `include/m_random_field.h`, `include/m_random_field_h.h`,
// `include/m_field_make.h` in the upstream decompilation):
//
// * `BLOCK_KIND_TABLE` mirrors `mRF_block_info[]`: every block type maps to a
//   bitmask of `BK_*` kinds. Block properties live in this table, not in
//   per-acre attached metadata.
// * `GATE_TABLE` mirrors `mRF_gate_info2[][]`: gates are NOT attached to
//   acres up front. They are resolved later from (block type, direction) via
//   `gate_type_for`, exactly as the 2026 research describes.
// * `RIVER_NEXT_DIRECTION` mirrors `l_river_next_direct[]` (7 river types).
// * `PERFECT_*` mirrors the `mRF_BIT_*` validation bits. The source accepts
//   a town only when `perfect_bit == (perfect_bit & bit)` after a complete
//   generation attempt; otherwise the whole town is discarded and generation
//   restarts (`mRF_MakeRandomField_ovl`).
//
// Block-kind bit positions (from `m_random_field_h.h`):
#[allow(dead_code)] // Public lookup API for the generator; used by downstream adapters.
pub const BK_NONE: u32 = 0;
#[allow(dead_code)] // Public lookup API for the generator; used by downstream adapters.
pub const BK_PLAYER: u32 = 1 << 0;
#[allow(dead_code)] // Public lookup API for the generator; used by downstream adapters.
pub const BK_SHOP: u32 = 1 << 1;
#[allow(dead_code)] // Public lookup API for the generator; used by downstream adapters.
pub const BK_SHRINE: u32 = 1 << 2;
#[allow(dead_code)] // Public lookup API for the generator; used by downstream adapters.
pub const BK_POLICE: u32 = 1 << 3;
#[allow(dead_code)] // Public lookup API for the generator; used by downstream adapters.
pub const BK_POSTOFFICE: u32 = 1 << 4;
#[allow(dead_code)] // Public lookup API for the generator; used by downstream adapters.
pub const BK_STATION: u32 = 1 << 5;
#[allow(dead_code)] // Public lookup API for the generator; used by downstream adapters.
pub const BK_CLIFF: u32 = 1 << 6;
#[allow(dead_code)] // Public lookup API for the generator; used by downstream adapters.
pub const BK_RIVER: u32 = 1 << 7;
#[allow(dead_code)] // Public lookup API for the generator; used by downstream adapters.
pub const BK_WATERFALL: u32 = 1 << 8;
#[allow(dead_code)] // Public lookup API for the generator; used by downstream adapters.
pub const BK_BRIDGE: u32 = 1 << 9;
#[allow(dead_code)] // Public lookup API for the generator; used by downstream adapters.
pub const BK_RAILROAD: u32 = 1 << 10;
#[allow(dead_code)] // Public lookup API for the generator; used by downstream adapters.
pub const BK_MARINE: u32 = 1 << 11;
#[allow(dead_code)] // Public lookup API for the generator; used by downstream adapters.
pub const BK_BORDER: u32 = 1 << 12;
#[allow(dead_code)] // Public lookup API for the generator; used by downstream adapters.
pub const BK_TUNNEL: u32 = 1 << 13;
#[allow(dead_code)] // Public lookup API for the generator; used by downstream adapters.
pub const BK_SLOPE: u32 = 1 << 14;
#[allow(dead_code)] // Public lookup API for the generator; used by downstream adapters.
pub const BK_POOL: u32 = 1 << 15;
#[allow(dead_code)] // Public lookup API for the generator; used by downstream adapters.
pub const BK_DUMP: u32 = 1 << 16;
#[allow(dead_code)] // Public lookup API for the generator; used by downstream adapters.
pub const BK_MUSEUM: u32 = 1 << 17;
/// Decomp name `mRF_BLOCKKIND_18`; its exact purpose is not documented.
#[allow(dead_code)] // Public lookup API for the generator; used by downstream adapters.
pub const BK_18: u32 = 1 << 18;
#[allow(dead_code)] // Public lookup API for the generator; used by downstream adapters.
pub const BK_TAILORS: u32 = 1 << 19;
#[allow(dead_code)] // Public lookup API for the generator; used by downstream adapters.
pub const BK_OCEAN: u32 = 1 << 20;
#[allow(dead_code)] // Public lookup API for the generator; used by downstream adapters.
pub const BK_ISLAND: u32 = 1 << 21;
#[allow(dead_code)] // Public lookup API for the generator; used by downstream adapters.
pub const BK_OFFING: u32 = 1 << 22;
#[allow(dead_code)] // Public lookup API for the generator; used by downstream adapters.
pub const BK_RIVER0: u32 = 1 << 23;
#[allow(dead_code)] // Public lookup API for the generator; used by downstream adapters.
pub const BK_RIVER1: u32 = 1 << 24;
#[allow(dead_code)] // Public lookup API for the generator; used by downstream adapters.
pub const BK_RIVER2: u32 = 1 << 25;
#[allow(dead_code)] // Public lookup API for the generator; used by downstream adapters.
pub const BK_RIVER3: u32 = 1 << 26;
#[allow(dead_code)] // Public lookup API for the generator; used by downstream adapters.
pub const BK_RIVER4: u32 = 1 << 27;
#[allow(dead_code)] // Public lookup API for the generator; used by downstream adapters.
pub const BK_RIVER5: u32 = 1 << 28;
#[allow(dead_code)] // Public lookup API for the generator; used by downstream adapters.
pub const BK_RIVER6: u32 = 1 << 29;
#[allow(dead_code)] // Public lookup API for the generator; used by downstream adapters.
pub const BK_DOCK: u32 = 1 << 30;
#[allow(dead_code)] // Public lookup API for the generator; used by downstream adapters.
pub const BK_ISLAND_LEFT: u32 = 1 << 31;

/// Block-type indices named by the generator, in `mFM_BLOCK_TYPE_*` order.
#[allow(dead_code)] // Public lookup API for the generator; used by downstream adapters.
pub const BLOCK_TYPE_TRACKS_STATION: u8 = 11;
#[allow(dead_code)] // Public lookup API for the generator; used by downstream adapters.
pub const BLOCK_TYPE_TRACKS_DUMP: u8 = 12;
#[allow(dead_code)] // Public lookup API for the generator; used by downstream adapters.
pub const BLOCK_TYPE_PLAYER_HOUSE: u8 = 14;
#[allow(dead_code)] // Public lookup API for the generator; used by downstream adapters.
pub const BLOCK_TYPE_FLAT: u8 = 39;
#[allow(dead_code)] // Public lookup API for the generator; used by downstream adapters.
pub const BLOCK_TYPE_BEACH: u8 = 63;
#[allow(dead_code)] // Public lookup API for the generator; used by downstream adapters.
pub const BLOCK_TYPE_TRACKS_SHOP: u8 = 65;
#[allow(dead_code)] // Public lookup API for the generator; used by downstream adapters.
pub const BLOCK_TYPE_SHRINE: u8 = 66;
#[allow(dead_code)] // Public lookup API for the generator; used by downstream adapters.
pub const BLOCK_TYPE_TRACKS_POST_OFFICE: u8 = 67;
#[allow(dead_code)] // Public lookup API for the generator; used by downstream adapters.
pub const BLOCK_TYPE_POLICE_BOX: u8 = 68;
#[allow(dead_code)] // Public lookup API for the generator; used by downstream adapters.
pub const BLOCK_TYPE_MUSEUM: u8 = 84;
#[allow(dead_code)] // Public lookup API for the generator; used by downstream adapters.
pub const BLOCK_TYPE_NEEDLEWORK: u8 = 85;
#[allow(dead_code)] // Public lookup API for the generator; used by downstream adapters.
pub const BLOCK_TYPE_PORT: u8 = 100;
#[allow(dead_code)] // Public lookup API for the generator; used by downstream adapters.
pub const BLOCK_TYPE_NUM: usize = 108;

/// Perfection/validation bits, mirroring the source `mRF_BIT_*` order.
#[allow(dead_code)] // Public lookup API for the generator; used by downstream adapters.
pub const PERFECT_SLOPE_LEFT: u16 = 1 << 0;
#[allow(dead_code)] // Public lookup API for the generator; used by downstream adapters.
pub const PERFECT_SLOPE_RIGHT: u16 = 1 << 1;
#[allow(dead_code)] // Public lookup API for the generator; used by downstream adapters.
pub const PERFECT_BRIDGE_UPPER: u16 = 1 << 2;
#[allow(dead_code)] // Public lookup API for the generator; used by downstream adapters.
pub const PERFECT_BRIDGE_LOWER: u16 = 1 << 3;
#[allow(dead_code)] // Public lookup API for the generator; used by downstream adapters.
pub const PERFECT_SHRINE: u16 = 1 << 4;
#[allow(dead_code)] // Public lookup API for the generator; used by downstream adapters.
pub const PERFECT_POLICE: u16 = 1 << 5;
#[allow(dead_code)] // Public lookup API for the generator; used by downstream adapters.
pub const PERFECT_MUSEUM: u16 = 1 << 6;
#[allow(dead_code)] // Public lookup API for the generator; used by downstream adapters.
pub const PERFECT_POOL: u16 = 1 << 7;
#[allow(dead_code)] // Public lookup API for the generator; used by downstream adapters.
pub const PERFECT_NEEDLEWORK: u16 = 1 << 8;
#[allow(dead_code)] // Public lookup API for the generator; used by downstream adapters.
pub const PERFECT_ALL: u16 = 0x1FF;
/// Safety cap on full-town regeneration attempts per `generate` call.
#[allow(dead_code)] // Public lookup API for the generator; used by downstream adapters.
pub const MAX_GENERATION_ATTEMPTS: u32 = 4096;

/// The seven source river types (`mRF_RIVER0`..`mRF_RIVER6`).
#[allow(dead_code)] // Public lookup API for the generator; used by downstream adapters.
pub const RIVER_TYPE_NUM: usize = 7;
/// River exit direction per river type, mirroring `l_river_next_direct[]`.
/// Entries use the `NORTH`/`EAST`/`SOUTH`/`WEST` edge constants above.
#[allow(dead_code)] // Public lookup API for the generator; used by downstream adapters.
pub const RIVER_NEXT_DIRECTION: [u8; RIVER_TYPE_NUM] =
    [SOUTH, EAST, WEST, EAST, SOUTH, WEST, SOUTH];

/// Next river direction for a river type; mirrors `mRF_RiverIdx2NextDirect`
/// (out-of-range input yields `SOUTH`, as in the source).
#[allow(dead_code)] // Public lookup API for the generator; used by downstream adapters.
pub fn river_next_direction(river_idx: u8) -> u8 {
    if (river_idx as usize) < RIVER_TYPE_NUM {
        RIVER_NEXT_DIRECTION[river_idx as usize]
    } else {
        SOUTH
    }
}

/// Gate kinds (`mRF_GATE_*`): gates are resolved from a lookup table later in
/// generation, never attached to the acre/block up front.
#[allow(dead_code)] // Public lookup API for the generator; used by downstream adapters.
pub const GATE_NONE: u8 = 0;
#[allow(dead_code)] // Public lookup API for the generator; used by downstream adapters.
pub const GATE1_TYPE0: u8 = 1;
#[allow(dead_code)] // Public lookup API for the generator; used by downstream adapters.
pub const GATE1_TYPE1: u8 = 2;
#[allow(dead_code)] // Public lookup API for the generator; used by downstream adapters.
pub const GATE2_TYPE0: u8 = 3;
#[allow(dead_code)] // Public lookup API for the generator; used by downstream adapters.
pub const GATE2_TYPE1: u8 = 4;
#[allow(dead_code)] // Public lookup API for the generator; used by downstream adapters.
pub const GATE3_TYPE0: u8 = 5;
#[allow(dead_code)] // Public lookup API for the generator; used by downstream adapters.
pub const GATE_KIND_NUM: usize = 6;
/// Gate count per gate kind, mirroring `mRF_GateType2GateCount`.
#[allow(dead_code)] // Public lookup API for the generator; used by downstream adapters.
pub const GATE_COUNT_TABLE: [u8; GATE_KIND_NUM] = [0, 1, 1, 2, 2, 3];
/// Direction indices into `GATE_TABLE` rows, matching the source table order.
#[allow(dead_code)] // Public lookup API for the generator; used by downstream adapters.
pub const DIR_NORTH: u8 = 0;
#[allow(dead_code)] // Public lookup API for the generator; used by downstream adapters.
pub const DIR_WEST: u8 = 1;
#[allow(dead_code)] // Public lookup API for the generator; used by downstream adapters.
pub const DIR_SOUTH: u8 = 2;
#[allow(dead_code)] // Public lookup API for the generator; used by downstream adapters.
pub const DIR_EAST: u8 = 3;

/// Per-block-type kind bitmask, mirroring `mRF_block_info[]`.
/// Index with a `BLOCK_TYPE_*` constant.
#[allow(dead_code)] // Public lookup API for the generator; used by downstream adapters.
pub static BLOCK_KIND_TABLE: [u32; BLOCK_TYPE_NUM] = [
    BK_BORDER,
    BK_BORDER | BK_RIVER0,
    BK_BORDER,
    BK_BORDER,
    BK_BORDER,
    BK_BORDER,
    BK_BORDER,
    BK_BORDER,
    BK_BORDER,
    BK_RAILROAD | BK_TUNNEL,
    BK_RAILROAD | BK_TUNNEL,
    BK_STATION | BK_RAILROAD,
    BK_RAILROAD | BK_DUMP,
    BK_RIVER | BK_RAILROAD | BK_18 | BK_RIVER0,
    BK_PLAYER,
    BK_CLIFF,
    BK_CLIFF,
    BK_CLIFF,
    BK_CLIFF,
    BK_CLIFF,
    BK_CLIFF,
    BK_CLIFF,
    BK_CLIFF | BK_RIVER | BK_WATERFALL | BK_RIVER0,
    BK_CLIFF | BK_RIVER | BK_WATERFALL | BK_RIVER0,
    BK_CLIFF | BK_RIVER | BK_18 | BK_RIVER0,
    BK_CLIFF | BK_RIVER | BK_18 | BK_RIVER0,
    BK_CLIFF | BK_RIVER | BK_WATERFALL | BK_RIVER0,
    BK_CLIFF | BK_RIVER | BK_18 | BK_RIVER0,
    BK_CLIFF | BK_RIVER | BK_18 | BK_RIVER0,
    BK_CLIFF | BK_RIVER | BK_18 | BK_RIVER1,
    BK_CLIFF | BK_RIVER | BK_WATERFALL | BK_RIVER1,
    BK_CLIFF | BK_RIVER | BK_WATERFALL | BK_RIVER1,
    BK_CLIFF | BK_RIVER | BK_18 | BK_RIVER1,
    BK_CLIFF | BK_RIVER | BK_18 | BK_RIVER1,
    BK_CLIFF | BK_RIVER | BK_RIVER2,
    BK_CLIFF | BK_RIVER | BK_RIVER2,
    BK_CLIFF | BK_RIVER | BK_RIVER2,
    BK_CLIFF | BK_RIVER | BK_WATERFALL | BK_RIVER2,
    BK_CLIFF | BK_RIVER | BK_WATERFALL | BK_RIVER2,
    BK_NONE,
    BK_RIVER | BK_18 | BK_RIVER0,
    BK_RIVER | BK_18 | BK_RIVER1,
    BK_RIVER | BK_18 | BK_RIVER2,
    BK_RIVER | BK_18 | BK_RIVER3,
    BK_RIVER | BK_18 | BK_RIVER4,
    BK_RIVER | BK_18 | BK_RIVER5,
    BK_RIVER | BK_18 | BK_RIVER6,
    BK_RIVER | BK_BRIDGE | BK_RIVER0,
    BK_RIVER | BK_BRIDGE | BK_RIVER1,
    BK_RIVER | BK_BRIDGE | BK_RIVER2,
    BK_RIVER | BK_BRIDGE | BK_RIVER3,
    BK_RIVER | BK_BRIDGE | BK_RIVER4,
    BK_RIVER | BK_BRIDGE | BK_RIVER5,
    BK_RIVER | BK_BRIDGE | BK_RIVER6,
    BK_CLIFF | BK_SLOPE,
    BK_CLIFF | BK_SLOPE,
    BK_CLIFF | BK_SLOPE,
    BK_CLIFF | BK_SLOPE,
    BK_CLIFF | BK_SLOPE,
    BK_CLIFF | BK_SLOPE,
    BK_CLIFF | BK_SLOPE,
    BK_CLIFF | BK_BORDER,
    BK_CLIFF | BK_BORDER,
    BK_MARINE,
    BK_RIVER | BK_MARINE | BK_18 | BK_RIVER0,
    BK_SHOP | BK_RAILROAD,
    BK_SHRINE,
    BK_POSTOFFICE | BK_RAILROAD,
    BK_POLICE,
    BK_RIVER | BK_POOL | BK_RIVER0,
    BK_RIVER | BK_POOL | BK_RIVER1,
    BK_RIVER | BK_POOL | BK_RIVER2,
    BK_RIVER | BK_POOL | BK_RIVER3,
    BK_RIVER | BK_POOL | BK_RIVER4,
    BK_RIVER | BK_POOL | BK_RIVER5,
    BK_RIVER | BK_POOL | BK_RIVER6,
    BK_BORDER,
    BK_RIVER | BK_BORDER | BK_RIVER0,
    BK_CLIFF | BK_BORDER,
    BK_RAILROAD | BK_BORDER,
    BK_MARINE | BK_BORDER,
    BK_MARINE | BK_BORDER,
    BK_RIVER | BK_BRIDGE | BK_MARINE | BK_RIVER0,
    BK_NONE,
    BK_MUSEUM,
    BK_MARINE | BK_TAILORS,
    BK_RIVER | BK_BRIDGE | BK_RAILROAD | BK_RIVER0,
    BK_CLIFF | BK_RIVER | BK_BRIDGE | BK_RIVER1,
    BK_CLIFF | BK_RIVER | BK_BRIDGE | BK_RIVER0,
    BK_CLIFF | BK_RIVER | BK_BRIDGE | BK_RIVER0,
    BK_CLIFF | BK_RIVER | BK_BRIDGE | BK_RIVER1,
    BK_CLIFF | BK_RIVER | BK_BRIDGE | BK_RIVER1,
    BK_CLIFF | BK_RIVER | BK_BRIDGE | BK_RIVER0,
    BK_CLIFF | BK_RIVER | BK_BRIDGE | BK_RIVER0,
    BK_MARINE | BK_OCEAN | BK_OFFING,
    BK_MARINE | BK_OCEAN | BK_OFFING,
    BK_MARINE | BK_OCEAN | BK_OFFING,
    BK_MARINE | BK_OCEAN | BK_OFFING,
    BK_MARINE | BK_OCEAN | BK_ISLAND | BK_ISLAND_LEFT,
    BK_MARINE | BK_OCEAN | BK_ISLAND,
    BK_MARINE | BK_DOCK,
    BK_MARINE | BK_OCEAN | BK_OFFING,
    BK_MARINE | BK_OCEAN | BK_OFFING,
    BK_MARINE | BK_OCEAN | BK_OFFING,
    BK_MARINE | BK_OCEAN | BK_OFFING,
    BK_CLIFF | BK_RIVER | BK_BRIDGE | BK_RIVER1,
    BK_CLIFF | BK_RIVER | BK_BRIDGE | BK_RIVER1,
    BK_CLIFF | BK_RIVER | BK_BRIDGE | BK_RIVER1
];

/// Kind bitmask for a block type; mirrors `mRF_Type2BlockInfo`.
/// Out-of-range types yield `BK_NONE`.
#[allow(dead_code)] // Public lookup API for the generator; used by downstream adapters.
pub fn block_kind(block_type: u8) -> u32 {
    if (block_type as usize) < BLOCK_TYPE_NUM {
        BLOCK_KIND_TABLE[block_type as usize]
    } else {
        BK_NONE
    }
}

/// Gate lookup per (block type, direction), mirroring `mRF_gate_info2[][]`.
/// Each row is `[NORTH, WEST, SOUTH, EAST]`.
#[allow(dead_code)] // Public lookup API for the generator; used by downstream adapters.
pub static GATE_TABLE: [[u8; 4]; BLOCK_TYPE_NUM] = [
    [GATE_NONE, GATE_NONE, GATE_NONE, GATE_NONE],
    [GATE_NONE, GATE_NONE, GATE_NONE, GATE_NONE],
    [GATE_NONE, GATE_NONE, GATE_NONE, GATE_NONE],
    [GATE_NONE, GATE_NONE, GATE_NONE, GATE_NONE],
    [GATE_NONE, GATE_NONE, GATE_NONE, GATE_NONE],
    [GATE_NONE, GATE_NONE, GATE_NONE, GATE_NONE],
    [GATE_NONE, GATE_NONE, GATE_NONE, GATE_NONE],
    [GATE_NONE, GATE_NONE, GATE_NONE, GATE_NONE],
    [GATE_NONE, GATE_NONE, GATE_NONE, GATE_NONE],
    [GATE_NONE, GATE_NONE, GATE_NONE, GATE_NONE],
    [GATE_NONE, GATE_NONE, GATE_NONE, GATE_NONE],
    [GATE_NONE, GATE1_TYPE1, GATE1_TYPE0, GATE1_TYPE1],
    [GATE_NONE, GATE1_TYPE1, GATE1_TYPE0, GATE1_TYPE1],
    [GATE_NONE, GATE1_TYPE1, GATE2_TYPE1, GATE1_TYPE1],
    [GATE1_TYPE0, GATE1_TYPE0, GATE1_TYPE0, GATE1_TYPE0],
    [GATE1_TYPE0, GATE2_TYPE0, GATE1_TYPE0, GATE2_TYPE0],
    [GATE2_TYPE0, GATE2_TYPE0, GATE1_TYPE0, GATE1_TYPE0],
    [GATE2_TYPE0, GATE1_TYPE0, GATE2_TYPE0, GATE1_TYPE0],
    [GATE1_TYPE0, GATE1_TYPE0, GATE2_TYPE0, GATE2_TYPE0],
    [GATE1_TYPE0, GATE2_TYPE0, GATE2_TYPE0, GATE1_TYPE0],
    [GATE2_TYPE0, GATE1_TYPE0, GATE2_TYPE0, GATE1_TYPE0],
    [GATE2_TYPE0, GATE1_TYPE0, GATE1_TYPE0, GATE2_TYPE0],
    [GATE2_TYPE1, GATE2_TYPE0, GATE2_TYPE1, GATE2_TYPE0],
    [GATE3_TYPE0, GATE2_TYPE0, GATE2_TYPE1, GATE1_TYPE0],
    [GATE3_TYPE0, GATE1_TYPE0, GATE3_TYPE0, GATE1_TYPE0],
    [GATE2_TYPE1, GATE1_TYPE0, GATE3_TYPE0, GATE2_TYPE0],
    [GATE2_TYPE1, GATE2_TYPE0, GATE3_TYPE0, GATE1_TYPE0],
    [GATE3_TYPE0, GATE1_TYPE0, GATE3_TYPE0, GATE1_TYPE0],
    [GATE3_TYPE0, GATE1_TYPE0, GATE2_TYPE1, GATE2_TYPE0],
    [GATE1_TYPE0, GATE3_TYPE0, GATE1_TYPE0, GATE3_TYPE0],
    [GATE2_TYPE0, GATE3_TYPE0, GATE1_TYPE0, GATE2_TYPE1],
    [GATE2_TYPE0, GATE2_TYPE1, GATE2_TYPE0, GATE2_TYPE1],
    [GATE1_TYPE0, GATE2_TYPE1, GATE2_TYPE0, GATE3_TYPE0],
    [GATE1_TYPE0, GATE3_TYPE0, GATE2_TYPE0, GATE2_TYPE1],
    [GATE1_TYPE0, GATE3_TYPE0, GATE1_TYPE0, GATE3_TYPE0],
    [GATE1_TYPE0, GATE2_TYPE1, GATE2_TYPE0, GATE3_TYPE0],
    [GATE1_TYPE0, GATE3_TYPE0, GATE2_TYPE0, GATE2_TYPE1],
    [GATE2_TYPE0, GATE2_TYPE1, GATE2_TYPE0, GATE2_TYPE1],
    [GATE2_TYPE0, GATE2_TYPE1, GATE1_TYPE0, GATE3_TYPE0],
    [GATE1_TYPE0, GATE1_TYPE0, GATE1_TYPE0, GATE1_TYPE0],
    [GATE2_TYPE1, GATE1_TYPE0, GATE2_TYPE1, GATE1_TYPE0],
    [GATE1_TYPE0, GATE2_TYPE1, GATE1_TYPE0, GATE2_TYPE1],
    [GATE1_TYPE0, GATE2_TYPE1, GATE1_TYPE0, GATE2_TYPE1],
    [GATE2_TYPE1, GATE1_TYPE0, GATE1_TYPE0, GATE2_TYPE1],
    [GATE1_TYPE0, GATE2_TYPE1, GATE2_TYPE1, GATE1_TYPE0],
    [GATE2_TYPE1, GATE2_TYPE1, GATE1_TYPE0, GATE1_TYPE0],
    [GATE1_TYPE0, GATE1_TYPE0, GATE2_TYPE1, GATE2_TYPE1],
    [GATE2_TYPE1, GATE1_TYPE0, GATE2_TYPE1, GATE1_TYPE0],
    [GATE1_TYPE0, GATE2_TYPE1, GATE1_TYPE0, GATE2_TYPE1],
    [GATE1_TYPE0, GATE2_TYPE1, GATE1_TYPE0, GATE2_TYPE1],
    [GATE2_TYPE1, GATE1_TYPE0, GATE1_TYPE0, GATE2_TYPE1],
    [GATE1_TYPE0, GATE2_TYPE1, GATE2_TYPE1, GATE1_TYPE0],
    [GATE2_TYPE1, GATE2_TYPE1, GATE1_TYPE0, GATE1_TYPE0],
    [GATE1_TYPE0, GATE1_TYPE0, GATE2_TYPE1, GATE2_TYPE1],
    [GATE1_TYPE0, GATE2_TYPE0, GATE1_TYPE0, GATE2_TYPE0],
    [GATE2_TYPE0, GATE2_TYPE0, GATE1_TYPE0, GATE1_TYPE0],
    [GATE2_TYPE0, GATE1_TYPE0, GATE2_TYPE0, GATE1_TYPE0],
    [GATE1_TYPE0, GATE1_TYPE0, GATE2_TYPE0, GATE2_TYPE0],
    [GATE1_TYPE0, GATE2_TYPE0, GATE2_TYPE0, GATE1_TYPE0],
    [GATE2_TYPE0, GATE1_TYPE0, GATE2_TYPE0, GATE1_TYPE0],
    [GATE2_TYPE0, GATE1_TYPE0, GATE1_TYPE0, GATE2_TYPE0],
    [GATE_NONE, GATE_NONE, GATE_NONE, GATE_NONE],
    [GATE_NONE, GATE_NONE, GATE_NONE, GATE_NONE],
    [GATE1_TYPE0, GATE1_TYPE0, GATE_NONE, GATE1_TYPE0],
    [GATE2_TYPE1, GATE1_TYPE0, GATE_NONE, GATE1_TYPE0],
    [GATE_NONE, GATE1_TYPE1, GATE1_TYPE0, GATE1_TYPE1],
    [GATE1_TYPE0, GATE1_TYPE0, GATE1_TYPE0, GATE1_TYPE0],
    [GATE_NONE, GATE1_TYPE1, GATE1_TYPE0, GATE1_TYPE1],
    [GATE1_TYPE0, GATE1_TYPE0, GATE1_TYPE0, GATE1_TYPE0],
    [GATE2_TYPE1, GATE1_TYPE0, GATE2_TYPE1, GATE1_TYPE0],
    [GATE1_TYPE0, GATE2_TYPE1, GATE1_TYPE0, GATE2_TYPE1],
    [GATE1_TYPE0, GATE2_TYPE1, GATE1_TYPE0, GATE2_TYPE1],
    [GATE2_TYPE1, GATE1_TYPE0, GATE1_TYPE0, GATE2_TYPE1],
    [GATE1_TYPE0, GATE2_TYPE1, GATE2_TYPE1, GATE1_TYPE0],
    [GATE2_TYPE1, GATE2_TYPE1, GATE1_TYPE0, GATE1_TYPE0],
    [GATE1_TYPE0, GATE1_TYPE0, GATE2_TYPE1, GATE2_TYPE1],
    [GATE_NONE, GATE_NONE, GATE_NONE, GATE_NONE],
    [GATE_NONE, GATE_NONE, GATE_NONE, GATE_NONE],
    [GATE_NONE, GATE_NONE, GATE_NONE, GATE_NONE],
    [GATE_NONE, GATE_NONE, GATE_NONE, GATE_NONE],
    [GATE_NONE, GATE_NONE, GATE_NONE, GATE_NONE],
    [GATE_NONE, GATE_NONE, GATE_NONE, GATE_NONE],
    [GATE2_TYPE1, GATE1_TYPE0, GATE_NONE, GATE1_TYPE0],
    [GATE_NONE, GATE_NONE, GATE_NONE, GATE_NONE],
    [GATE_NONE, GATE_NONE, GATE_NONE, GATE_NONE],
    [GATE1_TYPE0, GATE1_TYPE0, GATE_NONE, GATE1_TYPE0],
    [GATE_NONE, GATE1_TYPE1, GATE2_TYPE1, GATE1_TYPE1],
    [GATE1_TYPE0, GATE3_TYPE0, GATE1_TYPE0, GATE3_TYPE0],
    [GATE3_TYPE0, GATE1_TYPE0, GATE3_TYPE0, GATE1_TYPE0],
    [GATE2_TYPE1, GATE1_TYPE0, GATE3_TYPE0, GATE2_TYPE0],
    [GATE1_TYPE0, GATE2_TYPE1, GATE2_TYPE0, GATE3_TYPE0],
    [GATE1_TYPE0, GATE3_TYPE0, GATE2_TYPE0, GATE2_TYPE1],
    [GATE3_TYPE0, GATE1_TYPE0, GATE3_TYPE0, GATE1_TYPE0],
    [GATE3_TYPE0, GATE1_TYPE0, GATE2_TYPE1, GATE2_TYPE0],
    [GATE_NONE, GATE_NONE, GATE_NONE, GATE_NONE],
    [GATE_NONE, GATE_NONE, GATE_NONE, GATE_NONE],
    [GATE_NONE, GATE_NONE, GATE_NONE, GATE_NONE],
    [GATE_NONE, GATE_NONE, GATE_NONE, GATE_NONE],
    [GATE_NONE, GATE_NONE, GATE_NONE, GATE_NONE],
    [GATE_NONE, GATE_NONE, GATE_NONE, GATE_NONE],
    [GATE1_TYPE0, GATE1_TYPE0, GATE1_TYPE0, GATE_NONE],
    [GATE_NONE, GATE_NONE, GATE_NONE, GATE_NONE],
    [GATE_NONE, GATE_NONE, GATE_NONE, GATE_NONE],
    [GATE_NONE, GATE_NONE, GATE_NONE, GATE_NONE],
    [GATE_NONE, GATE_NONE, GATE_NONE, GATE_NONE],
    [GATE1_TYPE0, GATE3_TYPE0, GATE1_TYPE0, GATE3_TYPE0],
    [GATE1_TYPE0, GATE2_TYPE1, GATE2_TYPE0, GATE3_TYPE0],
    [GATE1_TYPE0, GATE3_TYPE0, GATE2_TYPE0, GATE2_TYPE1],
];

/// Gate type for a block type and direction; mirrors
/// `mRF_BlockTypeDirect2GateType`. Out-of-range input yields `GATE_NONE`.
#[allow(dead_code)] // Public lookup API for the generator; used by downstream adapters.
pub fn gate_type_for(block_type: u8, direction: u8) -> u8 {
    if (block_type as usize) < BLOCK_TYPE_NUM && (direction as usize) < 4 {
        GATE_TABLE[block_type as usize][direction as usize]
    } else {
        GATE_NONE
    }
}

/// Gate count for a gate kind; mirrors the `gate_count_table` lookup.
/// Out-of-range input yields 0.
#[allow(dead_code)] // Public lookup API for the generator; used by downstream adapters.
pub fn gate_count_for(gate_type: u8) -> u8 {
    if (gate_type as usize) < GATE_KIND_NUM {
        GATE_COUNT_TABLE[gate_type as usize]
    } else {
        0
    }
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Feature {
    #[default]
    None = 0,
    Station = 1,
    Shop = 2,
    PostOffice = 3,
    PlayerHouse = 4,
    WishingWell = 5,
    PoliceStation = 6,
    Museum = 7,
    Tailor = 8,
    Dock = 9,
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CellKind {
    #[default]
    Grass = 0,
    Flower = 1,
    Tree = 2,
    Rock = 3,
    Weed = 4,
    Litter = 5,
    /// Center unit of a resident home's 3 by 3 footprint.
    House = 6,
    /// Signboard at the home's southwest unit, relative offset (-1, +1).
    HouseSign = 7,
    /// One of the home's seven other reserved footprint units.
    HouseReserved = 8,
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum GroundKind {
    #[default]
    Grass = 0,
    Beach = 1,
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum GrassPattern {
    #[default]
    Triangles = 0,
    Circles = 1,
    Squares = 2,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct TownAcre {
    /// Facility role, or `Feature::None` for an ordinary acre.
    pub feature: u8,
    /// Rewrite ground class. The last playable row borders the southern beach.
    pub ground_kind: u8,
    /// Semantic grass variant selector; no texture or copyrighted art is stored.
    pub grass_pattern: u8,
    /// Relative elevation tier, from 1 (lowest) through 3 (highest).
    pub elevation: u8,
    /// Connections from this acre to adjacent river acres: N=1, E=2, S=4, W=8.
    pub river_edges: u8,
    /// Cliff edges using the same N/E/S/W bit assignments.
    pub cliff_edges: u8,
    /// Marks a river crossing where this acre steps down to a lower tier.
    pub waterfall_edges: u8,
    /// Rewrite-owned flags for procedural bridges and other crossings.
    pub infrastructure: u8,
    /// Semantic data per 16 by 16 foreground unit; no art data is stored.
    /// Resident homes reserve complete 3 by 3 areas in this grid.
    pub cells: [u8; UNITS_PER_ACRE],
}

impl Default for TownAcre {
    fn default() -> Self {
        Self {
            feature: Feature::None as u8,
            ground_kind: GroundKind::Grass as u8,
            grass_pattern: GrassPattern::Triangles as u8,
            elevation: 1,
            river_edges: 0,
            cliff_edges: 0,
            waterfall_edges: 0,
            infrastructure: 0,
            cells: [CellKind::Grass as u8; UNITS_PER_ACRE],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct TownPlan {
    pub seed: u32,
    pub acre_count: u32,
    pub villager_count: u8,
    pub elevation_tier_count: u8,
    pub reserved: [u8; 2],
    pub villagers: [u16; MAX_TOWN_VILLAGERS],
    pub house_acres: [u8; MAX_TOWN_VILLAGERS],
    /// Linear unit index of each resident home's center (`z * 16 + x`).
    pub house_units: [u16; MAX_TOWN_VILLAGERS],
    pub acres: [TownAcre; ACRE_COUNT],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VillagerCandidate {
    pub id: u16,
    /// Source `mNpc_LOOKS_*` value, 0 through 5.
    pub look: u8,
    pub reserved: u8,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TownAssessment {
    pub field_rank: u8,
    pub score: u8,
    pub perfect_acres: u8,
    pub good_acres: u8,
    pub bad_acres: u8,
    pub reserved: u8,
    pub tree_count: u16,
    pub flower_count: u16,
    pub weed_count: u16,
    pub trash_outside_dump: u16,
}

#[derive(Clone, Copy)]
struct Rng(u32);

impl Rng {
    fn new(seed: u32) -> Self {
        Self(seed)
    }

    fn fraction(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(0x0019_660D).wrapping_add(0x3C6E_F35F);
        f32::from_bits((self.0 >> 9) | 0x3F80_0000) - 1.0
    }

    fn below(&mut self, bound: usize) -> usize {
        if bound == 0 {
            0
        } else {
            (self.fraction() * bound as f32) as usize
        }
    }

    fn chance(&mut self, percent: usize) -> bool {
        self.below(100) < percent
    }
}

fn acre_index(x: usize, z: usize) -> usize {
    z * ACRE_WIDTH + x
}

fn set_feature(acres: &mut [TownAcre; ACRE_COUNT], x: usize, z: usize, feature: Feature) {
    acres[acre_index(x, z)].feature = feature as u8;
}

fn mark_edge(acres: &mut [TownAcre; ACRE_COUNT], x: usize, z: usize, edge: u8) {
    let (nx, nz, opposite) = match edge {
        NORTH if z > 0 => (x, z - 1, SOUTH),
        EAST if x + 1 < ACRE_WIDTH => (x + 1, z, WEST),
        SOUTH if z + 1 < ACRE_DEPTH => (x, z + 1, NORTH),
        WEST if x > 0 => (x - 1, z, EAST),
        _ => return,
    };
    acres[acre_index(x, z)].river_edges |= edge;
    acres[acre_index(nx, nz)].river_edges |= opposite;
}

fn set_cliff_boundary(acres: &mut [TownAcre; ACRE_COUNT], z: usize) {
    if z + 1 >= ACRE_DEPTH {
        return;
    }
    for x in 0..ACRE_WIDTH {
        acres[acre_index(x, z)].cliff_edges |= SOUTH;
        acres[acre_index(x, z + 1)].cliff_edges |= NORTH;
    }
}

fn place_river(acres: &mut [TownAcre; ACRE_COUNT], rng: &mut Rng) -> bool {
    // The route moves south to the coast and may shift by one acre per row.
    // Every turn is represented by reciprocal acre-edge connections.
    let starts = [1usize, 3usize];
    for _ in 0..128 {
        let mut x = starts[rng.below(starts.len())];
        let mut row_x = [0usize; ACRE_DEPTH];
        row_x[0] = x;
        let mut viable = true;

        for z in 1..ACRE_DEPTH {
            let mut candidates = [0usize; 3];
            let mut count = 0;
            for nx in [x.saturating_sub(1), x, (x + 1).min(ACRE_WIDTH - 1)] {
                if nx.abs_diff(x) > 1 || (z == 1 && nx == 2) || (z == ACRE_DEPTH - 1 && nx == 4) {
                    continue;
                }
                let lo = nx.min(x);
                let hi = nx.max(x);
                let crosses_reserved =
                    (lo..=hi).any(|cx| acres[acre_index(cx, z)].feature != Feature::None as u8);
                if !crosses_reserved && !candidates[..count].contains(&nx) {
                    candidates[count] = nx;
                    count += 1;
                }
            }
            if count == 0 {
                viable = false;
                break;
            }
            x = candidates[rng.below(count)];
            row_x[z] = x;
        }
        if !viable {
            continue;
        }

        x = row_x[0];
        for z in 0..ACRE_DEPTH {
            let target_x = row_x[z];
            let direction = if target_x >= x { EAST } else { WEST };
            while x != target_x {
                mark_edge(acres, x, z, direction);
                if direction == EAST {
                    x += 1;
                } else {
                    x -= 1;
                }
            }
            if z + 1 < ACRE_DEPTH {
                mark_edge(acres, x, z, SOUTH);
            } else {
                acres[acre_index(x, z)].river_edges |= SOUTH; // southern ocean outlet
            }
        }
        return true;
    }
    false
}

/// Place the wishing well, police station, and museum on flat acres below the
/// highest tier. Mirrors `mRF_SetUniqueFlatBlock`: the well goes on one side
/// of the river, the police box prefers the opposite side, and the museum may
/// sit on either side. Returns the `PERFECT_*` bits earned so the rejection
/// sampler can discard candidates with missing placements.
fn place_lower_facilities(
    acres: &mut [TownAcre; ACRE_COUNT],
    highest_tier: u8,
    rng: &mut Rng,
) -> u16 {
    let candidates = |acres: &[TownAcre; ACRE_COUNT], side: Option<bool>| -> Vec<usize> {
        (0..ACRE_COUNT)
            .filter(|&idx| {
                let acre = &acres[idx];
                let x = idx % ACRE_WIDTH;
                let z = idx / ACRE_WIDTH;
                let river_x = (0..ACRE_WIDTH)
                    .find(|&river_x| acres[acre_index(river_x, z)].river_edges != 0)
                    .unwrap_or(ACRE_WIDTH / 2);
                let is_left = x < river_x;
                acre.feature == Feature::None as u8
                    && acre.river_edges == 0
                    && acre.cliff_edges == 0
                    && acre.waterfall_edges == 0
                    && acre.infrastructure & (INFRA_BRIDGE | INFRA_SLOPE) == 0
                    && !acre.cells.contains(&(CellKind::House as u8))
                    && z > 0
                    && acre.elevation < highest_tier
                    && side.is_none_or(|want_left| want_left == is_left)
            })
            .collect()
    };

    let well_left = rng.chance(50);
    let mut bits = 0u16;
    let mut well_choices = candidates(acres, Some(well_left));
    if well_choices.is_empty() {
        well_choices = candidates(acres, None);
    }
    if well_choices.is_empty() {
        return bits;
    }
    let well = well_choices.swap_remove(rng.below(well_choices.len()));
    acres[well].feature = Feature::WishingWell as u8;
    bits |= PERFECT_SHRINE;

    let mut police_choices = candidates(acres, Some(!well_left));
    if police_choices.is_empty() {
        police_choices = candidates(acres, None);
    }
    if police_choices.is_empty() {
        return bits;
    }
    let police = police_choices[rng.below(police_choices.len())];
    acres[police].feature = Feature::PoliceStation as u8;
    bits |= PERFECT_POLICE;

    let museum_choices = candidates(acres, None);
    if museum_choices.is_empty() {
        return bits;
    }
    let museum = museum_choices[rng.below(museum_choices.len())];
    acres[museum].feature = Feature::Museum as u8;
    bits |= PERFECT_MUSEUM;
    bits
}

fn reserve_house_footprint(acre: &mut TownAcre, center: usize) -> bool {
    let center_x = center % UNITS_PER_SIDE;
    let center_z = center / UNITS_PER_SIDE;
    if center_x < HOUSE_RADIUS
        || center_x + HOUSE_RADIUS >= UNITS_PER_SIDE
        || center_z < HOUSE_RADIUS
        || center_z + HOUSE_RADIUS >= UNITS_PER_SIDE
    {
        return false;
    }

    for z in center_z - HOUSE_RADIUS..=center_z + HOUSE_RADIUS {
        for x in center_x - HOUSE_RADIUS..=center_x + HOUSE_RADIUS {
            if acre.cells[z * UNITS_PER_SIDE + x] != CellKind::Grass as u8 {
                return false;
            }
        }
    }

    for z in center_z - HOUSE_RADIUS..=center_z + HOUSE_RADIUS {
        for x in center_x - HOUSE_RADIUS..=center_x + HOUSE_RADIUS {
            let kind = if x == center_x && z == center_z {
                CellKind::House
            } else if x + 1 == center_x && z == center_z + 1 {
                CellKind::HouseSign
            } else {
                CellKind::HouseReserved
            };
            acre.cells[z * UNITS_PER_SIDE + x] = kind as u8;
        }
    }
    true
}

fn place_residents(plan: &mut TownPlan, pool: &[u16], count: usize, rng: &mut Rng) -> bool {
    if count > MAX_TOWN_VILLAGERS || count > pool.len() {
        return false;
    }
    let mut selected = pool.to_vec();
    selected.sort_unstable();
    selected.dedup();
    if count > selected.len() {
        return false;
    }
    for i in (1..selected.len()).rev() {
        let j = rng.below(i + 1);
        selected.swap(i, j);
    }
    let homes: Vec<(usize, usize)> = (0..ACRE_COUNT)
        .filter(|&idx| {
            let acre = &plan.acres[idx];
            acre.feature == Feature::None as u8
                && acre.ground_kind == GroundKind::Grass as u8
                && acre.river_edges == 0
                && acre.infrastructure & (INFRA_BRIDGE | INFRA_SLOPE) == 0
                && idx / ACRE_WIDTH > 0
        })
        .flat_map(|acre| {
            (1..UNITS_PER_SIDE - 1).flat_map(move |z| {
                (1..UNITS_PER_SIDE - 1)
                    .map(move |x| (acre, z * UNITS_PER_SIDE + x))
            })
        })
        .collect();
    if homes.len() < count {
        return false;
    }
    let mut chosen_homes = homes;
    for i in (1..chosen_homes.len()).rev() {
        let j = rng.below(i + 1);
        chosen_homes.swap(i, j);
    }
    plan.villager_count = count as u8;
    let mut placed = 0;
    for index in 0..count {
        let mut home_site = None;
        while let Some((home, unit)) = chosen_homes.pop() {
            if reserve_house_footprint(&mut plan.acres[home], unit) {
                home_site = Some((home, unit));
                break;
            }
        }
        let Some((home, unit)) = home_site else {
            return false;
        };
        let villager = selected[index];
        plan.villagers[index] = villager;
        plan.house_acres[index] = home as u8;
        plan.house_units[index] = unit as u16;
        placed += 1;
    }
    placed == count
}

fn decorate(plan: &mut TownPlan, rng: &mut Rng) {
    for acre in &mut plan.acres {
        for cell in &mut acre.cells {
            if *cell == CellKind::House as u8
                || *cell == CellKind::HouseSign as u8
                || *cell == CellKind::HouseReserved as u8
            {
                continue;
            }
            *cell = if rng.chance(3) {
                CellKind::Tree as u8
            } else if rng.chance(4) {
                CellKind::Flower as u8
            } else if rng.chance(2) {
                CellKind::Weed as u8
            } else if rng.chance(1) {
                CellKind::Rock as u8
            } else if rng.chance(1) {
                CellKind::Litter as u8
            } else {
                CellKind::Grass as u8
            };
        }
    }
}

/// One complete generation attempt: a town plus the `PERFECT_*` placement
/// bits it earned. Mirrors the source loop body in `mRF_MakeRandomField_ovl`,
/// which builds the whole town before checking validity.
struct Candidate {
    plan: TownPlan,
    bits: u16,
    structural_ok: bool,
}

/// The source validity predicate: `while (perfect_bit != (perfect_bit & bit))`.
/// A candidate town is accepted only when every placement bit is set.
pub fn is_valid_town(bits: u16) -> bool {
    bits & PERFECT_ALL == PERFECT_ALL
}

fn generate_candidate(seed: u32, rng: &mut Rng, pool: &[u16], villager_count: usize) -> Candidate {
    let mut bits: u16 = 0;
    // mRF_GetRandomStepMode: three-step towns are chosen 15% of the time.
    let three_tiers = rng.chance(15);
    let mut plan = TownPlan {
        seed,
        acre_count: ACRE_COUNT as u32,
        villager_count: 0,
        elevation_tier_count: if three_tiers { 3 } else { 2 },
        reserved: [0; 2],
        villagers: [u16::MAX; MAX_TOWN_VILLAGERS],
        house_acres: [u8::MAX; MAX_TOWN_VILLAGERS],
        house_units: [u16::MAX; MAX_TOWN_VILLAGERS],
        acres: [TownAcre::default(); ACRE_COUNT],
    };
    let invalid = |plan: TownPlan, bits: u16| Candidate {
        plan,
        bits,
        structural_ok: false,
    };

    // -- Phase 1: base landform. The source fixes the station, player house,
    // and dump placeholders from its base block table, then traces cliff and
    // river block chains (`mRF_MakeBaseLandform`); three-step towns copy one
    // of ten authored templates. The rewrite keeps its boundary-signature
    // cliff/river model here instead of the authored template bytes.
    set_feature(&mut plan.acres, 2, 0, Feature::Station);
    set_feature(&mut plan.acres, 2, 1, Feature::PlayerHouse);
    set_feature(&mut plan.acres, 4, 5, Feature::Dock);
    // The tailor sits on the beach row; placing it before the river trace
    // keeps the river from crossing its acre, as in the previous layout.
    let tailor_x = rng.below(3);
    set_feature(&mut plan.acres, tailor_x, 5, Feature::Tailor);
    if !place_river(&mut plan.acres, rng) {
        return invalid(plan, bits);
    }

    // -- Phase 2: beach base and grass layer (`mRF_SetMarinBlock`). Grass is
    // a separate persistent layer from terrain topology, randomized here.
    for z in 0..ACRE_DEPTH {
        for x in 0..ACRE_WIDTH {
            let acre = &mut plan.acres[acre_index(x, z)];
            if z == ACRE_DEPTH - 1 {
                acre.ground_kind = GroundKind::Beach as u8;
            }
            acre.grass_pattern = match rng.below(3) {
                1 => GrassPattern::Circles as u8,
                2 => GrassPattern::Squares as u8,
                _ => GrassPattern::Triangles as u8,
            };
        }
    }

    // -- Phase 3: elevation tiers and waterfalls. --
    let first_cliff = 1 + rng.below(2);
    let second_cliff = (first_cliff + 1).min(ACRE_DEPTH - 2);
    set_cliff_boundary(&mut plan.acres, first_cliff);
    if three_tiers {
        set_cliff_boundary(&mut plan.acres, second_cliff);
    }
    for x in 0..ACRE_WIDTH {
        for z in 0..ACRE_DEPTH {
            let elevation = if three_tiers {
                if z <= first_cliff {
                    3
                } else if z <= second_cliff {
                    2
                } else {
                    1
                }
            } else if z <= first_cliff {
                2
            } else {
                1
            };
            plan.acres[acre_index(x, z)].elevation = elevation;
        }
    }
    for idx in 0..ACRE_COUNT {
        let x = idx % ACRE_WIDTH;
        let z = idx / ACRE_WIDTH;
        let edges = plan.acres[idx].river_edges;
        if edges & SOUTH != 0 && z + 1 < ACRE_DEPTH {
            let below = &plan.acres[acre_index(x, z + 1)];
            if plan.acres[idx].elevation > below.elevation {
                plan.acres[idx].waterfall_edges |= SOUTH;
            }
        }
    }

    // -- Phase 4: bridges and slopes (`mRF_SetBridgeAndSlopeBlock`). One
    // slope per river side; two bridge crossings, upper (north) and lower.
    for (want_left, bit) in [(true, PERFECT_SLOPE_LEFT), (false, PERFECT_SLOPE_RIGHT)] {
        let mut candidates: Vec<usize> = (0..ACRE_COUNT)
            .filter(|&idx| {
                let x = idx % ACRE_WIDTH;
                let z = idx / ACRE_WIDTH;
                let river_x = (0..ACRE_WIDTH)
                    .find(|&river_x| plan.acres[acre_index(river_x, z)].river_edges != 0)
                    .unwrap_or(ACRE_WIDTH / 2);
                plan.acres[idx].cliff_edges & SOUTH != 0
                    && plan.acres[idx].feature == Feature::None as u8
                    && plan.acres[idx].infrastructure & INFRA_SLOPE == 0
                    && (x < river_x) == want_left
            })
            .collect();
        if candidates.is_empty() {
            candidates = (0..ACRE_COUNT)
                .filter(|&idx| {
                    plan.acres[idx].cliff_edges & SOUTH != 0
                        && plan.acres[idx].feature == Feature::None as u8
                        && plan.acres[idx].infrastructure & INFRA_SLOPE == 0
                })
                .collect();
        }
        if !candidates.is_empty() {
            let selected = candidates.swap_remove(rng.below(candidates.len()));
            plan.acres[selected].infrastructure |= INFRA_SLOPE;
            bits |= bit;
        }
    }

    // -- Phase 5: tailor and dock (`mRF_SetNeedleworkAndWharfBlock`). The
    // tailor was fixed on the beach row in phase 1; the dock too. Mark the
    // needlework placement complete here, where the source sets it.
    bits |= PERFECT_NEEDLEWORK;

    // -- Phase 6: wishing well, police station, museum
    // (`mRF_SetUniqueFlatBlock`). --
    bits |= place_lower_facilities(&mut plan.acres, plan.elevation_tier_count, rng);

    // -- Phase 7: shop and post office (`mRF_SetUniqueRailBlock`). The source
    // swaps shop/post office order randomly and pins them to the rail row at
    // x in {1,2} / {4,5} of its block grid; this placement carries no
    // perfection bit in the source either.
    let left_x = rng.below(2);
    let right_x = 3 + rng.below(2);
    if rng.below(2) == 0 {
        set_feature(&mut plan.acres, left_x, 0, Feature::Shop);
        set_feature(&mut plan.acres, right_x, 0, Feature::PostOffice);
    } else {
        set_feature(&mut plan.acres, left_x, 0, Feature::PostOffice);
        set_feature(&mut plan.acres, right_x, 0, Feature::Shop);
    }

    // -- Phase 8: lake/pond (`mRF_SetPoolBlock`) and the sea bridge fixup
    // (`mRF_SetSeaBlockWithBridgeRiver`). Bridge sites stay on lower, flat
    // river acres; the northernmost crossing is the "upper" bridge.
    let river_acres: Vec<usize> = plan
        .acres
        .iter()
        .enumerate()
        .filter_map(|(idx, acre)| (acre.river_edges != 0).then_some(idx))
        .collect();
    let mut bridge_sites: Vec<usize> = river_acres
        .iter()
        .copied()
        .filter(|&idx| {
            let acre = &plan.acres[idx];
            idx / ACRE_WIDTH > 0
                && acre.feature == Feature::None as u8
                && acre.cliff_edges == 0
                && acre.waterfall_edges == 0
        })
        .collect();
    if bridge_sites.len() >= 2 {
        for i in (1..bridge_sites.len()).rev() {
            let j = rng.below(i + 1);
            bridge_sites.swap(i, j);
        }
        bridge_sites[..2].sort_by_key(|&idx| idx / ACRE_WIDTH);
        plan.acres[bridge_sites[0]].infrastructure |= INFRA_BRIDGE;
        bits |= PERFECT_BRIDGE_UPPER;
        plan.acres[bridge_sites[1]].infrastructure |= INFRA_BRIDGE;
        bits |= PERFECT_BRIDGE_LOWER;
    }
    let pond_idx = river_acres[rng.below(river_acres.len())];
    plan.acres[pond_idx].infrastructure |= INFRA_POND;
    bits |= PERFECT_POOL;

    // -- Phase 9: residents and decoration (rewrite-owned finalization). --
    if !place_residents(&mut plan, pool, villager_count, rng) {
        return invalid(plan, bits);
    }
    decorate(&mut plan, rng);
    Candidate {
        plan,
        bits,
        structural_ok: true,
    }
}

/// Generate one rewrite-owned town plan from a caller-provided seed and roster.
/// The roster is expected to contain only villagers eligible for this town.
///
/// This is rejection sampling, mirroring `mRF_MakeRandomField_ovl`: each call
/// builds complete candidate towns and discards the invalid ones, drawing
/// from a single RNG stream so the result stays deterministic per seed.
pub fn generate(seed: u32, pool: &[u16], villager_count: usize) -> Option<TownPlan> {
    if villager_count > MAX_TOWN_VILLAGERS || villager_count > pool.len() {
        return None;
    }
    // A roster that cannot cover the count even in principle can never
    // succeed; fail fast instead of burning every attempt.
    {
        let mut unique = pool.to_vec();
        unique.sort_unstable();
        unique.dedup();
        if villager_count > unique.len() {
            return None;
        }
    }
    let mut rng = Rng::new(seed);
    for _ in 0..MAX_GENERATION_ATTEMPTS {
        let candidate = generate_candidate(seed, &mut rng, pool, villager_count);
        if candidate.structural_ok && is_valid_town(candidate.bits) {
            return Some(candidate.plan);
        }
    }
    None
}

/// Select one source-eligible initial villager from each of the six look
/// classes. The candidate list must already exclude non-starter entries.
pub fn select_initial_villagers(
    seed: u32,
    candidates: &[VillagerCandidate],
) -> Option<[u16; INITIAL_STARTERS]> {
    if candidates.len() > MAX_VILLAGER_CANDIDATES {
        return None;
    }
    let mut rng = Rng::new(seed);
    let mut selected = [u16::MAX; INITIAL_STARTERS];
    for look in 0..LOOK_CLASS_COUNT {
        let mut eligible: Vec<u16> = candidates
            .iter()
            .filter(|candidate| {
                candidate.look as usize == look && !selected[..look].contains(&candidate.id)
            })
            .map(|candidate| candidate.id)
            .collect();
        eligible.sort_unstable();
        eligible.dedup();
        if eligible.is_empty() {
            return None;
        }
        selected[look] = eligible[rng.below(eligible.len())];
    }
    Some(selected)
}

/// Evaluate semantic vegetation using the source field-assessment tree bands,
/// flower-offset weed rule, per-acre litter penalty, and rank thresholds.
/// Trash counts must exclude items in the dump, matching the source contract.
pub fn assess(plan: &TownPlan, trash_outside_dump: &[u16; ACRE_COUNT]) -> TownAssessment {
    const TREE_LIMITS: [usize; 5] = [8, 11, 14, 17, 255];
    const TREE_POINTS: [u8; 5] = [0, 1, 2, 1, 0];
    const RANK_LIMITS: [u8; 7] = [0, 2, 4, 7, 12, 16, 255];

    let mut result = TownAssessment::default();
    let mut trash_total = 0u16;
    for (idx, acre) in plan.acres.iter().enumerate() {
        let trees = acre
            .cells
            .iter()
            .filter(|&&c| c == CellKind::Tree as u8)
            .count();
        let flowers = acre
            .cells
            .iter()
            .filter(|&&c| c == CellKind::Flower as u8)
            .count();
        let weeds = acre
            .cells
            .iter()
            .filter(|&&c| c == CellKind::Weed as u8)
            .count();
        let trash = trash_outside_dump[idx];
        let effective_weeds = weeds as isize - flowers as isize;
        result.tree_count += trees as u16;
        result.flower_count += flowers as u16;
        result.weed_count += weeds as u16;
        trash_total = trash_total.saturating_add(trash);

        let acre_points = if trash != 0 || effective_weeds >= 3 {
            0
        } else {
            let band = TREE_LIMITS
                .iter()
                .position(|&limit| trees <= limit)
                .unwrap_or(4);
            TREE_POINTS[band]
        };
        match acre_points {
            2 => result.perfect_acres += 1,
            1 => result.good_acres += 1,
            _ => result.bad_acres += 1,
        }
    }

    result.trash_outside_dump = trash_total;
    result.score = if trash_total >= 5 {
        0
    } else {
        result.perfect_acres + result.good_acres / 2
    };
    result.field_rank = RANK_LIMITS
        .iter()
        .position(|&limit| result.score <= limit)
        .unwrap_or(RANK_LIMITS.len() - 1) as u8;
    result
}

/// C ABI for the Rust rewrite's generator.
///
/// `candidate_ids` must point to `candidate_count` identifiers and `out_plan`
/// must point to one writable `TownPlan`. IDs should already satisfy the
/// caller's version-specific resident eligibility rules.
#[no_mangle]
pub unsafe extern "C" fn pc_town_generate(
    seed: u32,
    candidate_ids: *const u16,
    candidate_count: u32,
    villager_count: u8,
    out_plan: *mut TownPlan,
) -> i32 {
    if out_plan.is_null()
        || candidate_count as usize > MAX_VILLAGER_CANDIDATES
        || (candidate_count != 0 && candidate_ids.is_null())
    {
        return 0;
    }
    // SAFETY: The C ABI requires a readable candidate array of the stated
    // bounded length and a writable output record.
    let pool = if candidate_count == 0 {
        &[]
    } else {
        unsafe { slice::from_raw_parts(candidate_ids, candidate_count as usize) }
    };
    let Some(plan) = generate(seed, pool, villager_count as usize) else {
        return 0;
    };
    unsafe { out_plan.write(plan) };
    1
}

/// Select one eligible candidate for each of the six initial look classes.
/// Returns 0 if a class has no eligible candidate or an input pointer is null.
#[no_mangle]
pub unsafe extern "C" fn pc_town_select_initial_villagers(
    seed: u32,
    candidates: *const VillagerCandidate,
    candidate_count: u32,
    out_ids: *mut u16,
) -> i32 {
    if candidates.is_null()
        || out_ids.is_null()
        || candidate_count == 0
        || candidate_count as usize > MAX_VILLAGER_CANDIDATES
    {
        return 0;
    }
    // SAFETY: The ABI requires a readable bounded candidate array and six
    // writable u16 output entries.
    let candidates = unsafe { slice::from_raw_parts(candidates, candidate_count as usize) };
    let Some(selected) = select_initial_villagers(seed, candidates) else {
        return 0;
    };
    unsafe { std::ptr::copy_nonoverlapping(selected.as_ptr(), out_ids, INITIAL_STARTERS) };
    1
}

/// Assess generated vegetation using caller-provided per-acre trash counts
/// outside the dump. Returns 1 on success and 0 for any null pointer.
#[no_mangle]
pub unsafe extern "C" fn pc_town_assess(
    plan: *const TownPlan,
    trash_by_acre: *const u16,
    out_assessment: *mut TownAssessment,
) -> i32 {
    if plan.is_null() || trash_by_acre.is_null() || out_assessment.is_null() {
        return 0;
    }
    // SAFETY: The ABI requires one readable town plan, 30 trash counts, and
    // one writable assessment record.
    let plan = unsafe { &*plan };
    let trash = unsafe { slice::from_raw_parts(trash_by_acre, ACRE_COUNT) };
    let trash: &[u16; ACRE_COUNT] = match trash.try_into() {
        Ok(counts) => counts,
        Err(_) => return 0,
    };
    unsafe { out_assessment.write(assess(plan, trash)) };
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    const POOL: [u16; 12] = [10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21];

    #[test]
    fn seeded_generation_is_repeatable() {
        let a = generate(0x1234_5678, &POOL, 6).unwrap();
        let b = generate(0x1234_5678, &POOL, 6).unwrap();
        assert_eq!(a.seed, b.seed);
        assert_eq!(a.villagers, b.villagers);
        assert_eq!(a.house_acres, b.house_acres);
        assert_eq!(a.house_units, b.house_units);
        assert_eq!(a.acres[0].cells, b.acres[0].cells);
        assert_eq!(a.acres[29].river_edges, b.acres[29].river_edges);
    }

    #[test]
    fn output_uses_original_playable_acre_dimensions_and_fixed_facilities() {
        let plan = generate(1, &POOL, 6).unwrap();
        assert_eq!(plan.acre_count as usize, ACRE_COUNT);
        assert_eq!(plan.acres[acre_index(2, 0)].feature, Feature::Station as u8);
        let left_rail = [
            plan.acres[acre_index(0, 0)].feature,
            plan.acres[acre_index(1, 0)].feature,
        ];
        let right_rail = [
            plan.acres[acre_index(3, 0)].feature,
            plan.acres[acre_index(4, 0)].feature,
        ];
        assert_eq!(
            left_rail
                .iter()
                .filter(|&&f| f == Feature::Shop as u8)
                .count()
                + right_rail
                    .iter()
                    .filter(|&&f| f == Feature::Shop as u8)
                    .count(),
            1
        );
        assert_eq!(
            left_rail
                .iter()
                .filter(|&&f| f == Feature::PostOffice as u8)
                .count()
                + right_rail
                    .iter()
                    .filter(|&&f| f == Feature::PostOffice as u8)
                    .count(),
            1
        );
        assert!(left_rail
            .iter()
            .any(|&f| f == Feature::Shop as u8 || f == Feature::PostOffice as u8));
        assert!(right_rail
            .iter()
            .any(|&f| f == Feature::Shop as u8 || f == Feature::PostOffice as u8));
        assert_eq!(
            plan.acres[acre_index(2, 1)].feature,
            Feature::PlayerHouse as u8
        );
        assert_eq!(plan.acres[acre_index(4, 5)].feature, Feature::Dock as u8);
        assert!(plan.acres[25..]
            .iter()
            .all(|acre| acre.ground_kind == GroundKind::Beach as u8));
        assert!(plan
            .acres
            .iter()
            .all(|acre| acre.cells.len() == UNITS_PER_ACRE));
    }

    #[test]
    fn river_connections_are_reciprocal_and_reach_southern_ocean() {
        for seed in 0..1024 {
            let plan = generate(seed, &POOL, 6).unwrap();
            let river = &plan.acres;
            let outlets = river
                .iter()
                .enumerate()
                .filter(|(idx, acre)| {
                    idx / ACRE_WIDTH == ACRE_DEPTH - 1 && acre.river_edges & SOUTH != 0
                })
                .count();
            assert_eq!(outlets, 1);
            for z in 0..ACRE_DEPTH {
                for x in 0..ACRE_WIDTH {
                    let acre = &river[acre_index(x, z)];
                    if acre.river_edges & EAST != 0 && x + 1 < ACRE_WIDTH {
                        assert_ne!(river[acre_index(x + 1, z)].river_edges & WEST, 0);
                    }
                    if acre.river_edges & SOUTH != 0 && z + 1 < ACRE_DEPTH {
                        assert_ne!(river[acre_index(x, z + 1)].river_edges & NORTH, 0);
                    }
                    assert_eq!(acre.waterfall_edges & !acre.river_edges, 0);
                }
            }
        }
    }

    #[test]
    fn villager_homes_are_distinct_and_outside_facilities_and_river() {
        for seed in 0..1024 {
            let plan = generate(seed, &POOL, 6).unwrap();
            let homes = &plan.house_acres[..6];
            let mut reserved_units = Vec::new();
            for i in 0..homes.len() {
                for j in 0..i {
                    assert_ne!(
                        (homes[j], plan.house_units[j]),
                        (homes[i], plan.house_units[i])
                    );
                }
                let acre = &plan.acres[homes[i] as usize];
                assert_eq!(acre.feature, Feature::None as u8);
                assert_eq!(acre.ground_kind, GroundKind::Grass as u8);
                assert_eq!(acre.river_edges, 0);
                assert!((1..=3).contains(&acre.elevation));
                assert_eq!(acre.infrastructure & (INFRA_BRIDGE | INFRA_SLOPE), 0);

                let center = plan.house_units[i] as usize;
                let center_x = center % UNITS_PER_SIDE;
                let center_z = center / UNITS_PER_SIDE;
                assert!((1..UNITS_PER_SIDE - 1).contains(&center_x));
                assert!((1..UNITS_PER_SIDE - 1).contains(&center_z));
                for z in center_z - HOUSE_RADIUS..=center_z + HOUSE_RADIUS {
                    for x in center_x - HOUSE_RADIUS..=center_x + HOUSE_RADIUS {
                        let unit = z * UNITS_PER_SIDE + x;
                        assert!(!reserved_units.contains(&(homes[i], unit)));
                        reserved_units.push((homes[i], unit));
                        let expected = if x == center_x && z == center_z {
                            CellKind::House
                        } else if x + 1 == center_x && z == center_z + 1 {
                            CellKind::HouseSign
                        } else {
                            CellKind::HouseReserved
                        };
                        assert_eq!(acre.cells[unit], expected as u8);
                    }
                }
            }
        }
    }

    #[test]
    fn invalid_resident_counts_are_rejected() {
        assert!(generate(1, &POOL, MAX_TOWN_VILLAGERS + 1).is_none());
        assert!(generate(1, &POOL[..3], 4).is_none());
        assert!(generate(1, &[7, 7, 8], 3).is_none());
    }

    #[test]
    fn initial_roster_selects_one_unique_candidate_per_look_class() {
        let candidates = [
            VillagerCandidate {
                id: 11,
                look: 0,
                reserved: 0,
            },
            VillagerCandidate {
                id: 12,
                look: 0,
                reserved: 0,
            },
            VillagerCandidate {
                id: 21,
                look: 1,
                reserved: 0,
            },
            VillagerCandidate {
                id: 31,
                look: 2,
                reserved: 0,
            },
            VillagerCandidate {
                id: 41,
                look: 3,
                reserved: 0,
            },
            VillagerCandidate {
                id: 51,
                look: 4,
                reserved: 0,
            },
            VillagerCandidate {
                id: 61,
                look: 5,
                reserved: 0,
            },
        ];
        let selected = select_initial_villagers(42, &candidates).unwrap();
        assert!(selected.contains(&11) || selected.contains(&12));
        assert!(selected.contains(&21));
        assert!(selected.contains(&31));
        assert!(selected.contains(&41));
        assert!(selected.contains(&51));
        assert!(selected.contains(&61));
        assert_eq!(
            selected
                .iter()
                .copied()
                .collect::<std::collections::HashSet<_>>()
                .len(),
            INITIAL_STARTERS
        );
        assert!(select_initial_villagers(42, &candidates[..5]).is_none());
    }

    #[test]
    fn exported_records_match_the_c_header_layout() {
        assert_eq!(std::mem::size_of::<TownAcre>(), 264);
        assert_eq!(std::mem::size_of::<TownPlan>(), 8008);
        assert_eq!(std::mem::size_of::<TownAssessment>(), 14);
        assert_eq!(std::mem::size_of::<VillagerCandidate>(), 4);
    }

    #[test]
    fn random_state_uses_the_source_lcg_constants() {
        let mut rng = Rng::new(0);
        let fraction = rng.fraction();
        assert_eq!(rng.0, 0x3C6E_F35F);
        assert_eq!(
            fraction,
            f32::from_bits(0x3F80_0000 | (0x3C6E_F35F >> 9)) - 1.0
        );
    }

    #[test]
    fn generated_layouts_have_required_facilities_and_crossings() {
        let mut three_tier_count = 0;
        for seed in 0u32..2048 {
            let varied_seed = seed.wrapping_mul(0x9E37_79B9);
            let plan = generate(varied_seed, &POOL, 6).unwrap();
            three_tier_count += (plan.elevation_tier_count == 3) as usize;
            for required in [
                Feature::WishingWell,
                Feature::PoliceStation,
                Feature::Museum,
                Feature::Tailor,
                Feature::Dock,
            ] {
                assert_eq!(
                    plan.acres
                        .iter()
                        .filter(|a| a.feature == required as u8)
                        .count(),
                    1
                );
            }
            assert_eq!(
                plan.acres
                    .iter()
                    .filter(|a| a.infrastructure & INFRA_POND != 0)
                    .count(),
                1
            );
            assert_eq!(
                plan.acres
                    .iter()
                    .filter(|a| a.infrastructure & INFRA_BRIDGE != 0)
                    .count(),
                2
            );
            for (idx, acre) in plan.acres.iter().enumerate() {
                if acre.infrastructure & INFRA_BRIDGE != 0 {
                    assert!(idx / ACRE_WIDTH > 0);
                    assert_ne!(acre.river_edges, 0);
                    assert_eq!(acre.cliff_edges, 0);
                    assert_eq!(acre.waterfall_edges, 0);
                    assert_eq!(acre.feature, Feature::None as u8);
                }
            }
            assert_eq!(
                plan.acres
                    .iter()
                    .filter(|a| a.infrastructure & INFRA_SLOPE != 0)
                    .count(),
                2
            );
            for z in 0..ACRE_DEPTH - 1 {
                for x in 0..ACRE_WIDTH {
                    let acre = &plan.acres[acre_index(x, z)];
                    let below = &plan.acres[acre_index(x, z + 1)];
                    if acre.river_edges & SOUTH != 0 && acre.elevation > below.elevation {
                        assert_ne!(acre.waterfall_edges & SOUTH, 0);
                    }
                    if acre.waterfall_edges & SOUTH != 0 {
                        assert!(acre.river_edges & SOUTH != 0);
                        assert!(acre.elevation > below.elevation);
                    }
                }
            }
        }
        assert!(
            (200..400).contains(&three_tier_count),
            "three-tier seeds: {three_tier_count}"
        );
    }

    #[test]
    fn field_assessment_matches_tree_weed_flower_and_trash_rules() {
        let mut plan = generate(2, &POOL, 6).unwrap();
        let no_trash = [0u16; ACRE_COUNT];
        for acre in &mut plan.acres {
            acre.cells.fill(CellKind::Grass as u8);
            acre.cells[..12].fill(CellKind::Tree as u8);
        }
        let perfect = assess(&plan, &no_trash);
        assert_eq!(perfect.perfect_acres, 30);
        assert_eq!(perfect.score, 30);
        assert_eq!(perfect.field_rank, 6);

        plan.acres[0].cells[12..16].fill(CellKind::Weed as u8);
        plan.acres[0].cells[16..18].fill(CellKind::Flower as u8);
        assert_eq!(assess(&plan, &no_trash).perfect_acres, 30);
        plan.acres[0].cells[16..18].fill(CellKind::Grass as u8);
        assert_eq!(assess(&plan, &no_trash).bad_acres, 1);

        let mut trash = [0u16; ACRE_COUNT];
        trash[..5].fill(1);
        let dusty = assess(&plan, &trash);
        assert_eq!(dusty.score, 0);
        assert_eq!(dusty.field_rank, 0);
        assert_eq!(dusty.bad_acres, 5);
    }
}
