//! Tool target resolvers: shovel/axe/net/rod decision logic.
//!
//! Verified against `m_player_lib.c` (mPlib_Check_scoop_after),
//! `m_field_info.c` (mFI_GetDigStatus, golden shovel),
//! `m_player_main_reflect_scoop.c_inc` (frame 13),
//! `m_player_main_putin_scoop.c_inc` (burial + golden demo),
//! `m_player_common.c_inc` (axe resolver + durability),
//! `m_player_main_swing_net.c_inc` (capture geometry),
//! `m_player_main_ready_rod.c_inc` (5-point water validation),
//! `m_player_main_swing_axe.c_inc` (damage reset)
//! (USA Rev. 0 decomp / PC port).
//!
//! Architecture: each tool has its own target resolver feeding a
//! main-index state; animation frames are the commit points.
//! Shovel: mPlib_Check_scoop_after -> DIG/FILL/GET/REFLECT/AIR.
//! Axe: Check_axe_after -> TREE/REFLECT/NONE. Net: sweep volume.
//! Rod: 5-point water patch -> bobber actor coupling.

/// The eight neighbor-unit offsets searched by the shovel resolver
/// (player's own unit omitted).
pub const SCOOP_NEIGHBORS: [(i32, i32); 8] = [
    (-1, -1), (0, -1), (1, -1),
    (-1, 0),           (1, 0),
    (-1, 1),  (0, 1),  (1, 1),
];

/// Diagonal neighbor indices (NW, NE, SW, SE) needing wall checks.
pub const SCOOP_DIAGONALS: [usize; 4] = [0, 2, 5, 7];

/// Diagonal distance cutoff: SQ(63.245553f).
pub const SCOOP_DIAG_DIST_SQ: f32 = 63.245553 * 63.245553;

/// Vertical compatibility threshold: +/-63.245552.
pub const SCOOP_VERT_THRESHOLD: f32 = 63.245552;

/// NPC exclusion radius for shovel hits: SQ(39.0f).
pub const SCOOP_NPC_RADIUS_SQ: f32 = 39.0 * 39.0;

/// Dig statuses (mFI_DIGSTATUS_*).
pub mod dig_status {
    pub const MISS: u8 = 0;
    pub const CANCEL: u8 = 1;
    pub const FILLIN: u8 = 2;
    pub const DIG: u8 = 3;
    pub const PUT_ITEM: u8 = 4;
    pub const GET_ITEM: u8 = 5;
    pub const NUM: u8 = 6;
}

/// Golden-shovel buried-money roll: RANDOM(10) == 1.
pub const GOLD_SHOVEL_ROLL: u32 = 10;
/// Item awarded on a won golden-shovel roll (ITM_MONEY_100).
pub const GOLD_SHOVEL_MONEY_ITEM: u16 = 0x2103;

/// Golden-shovel area gate: a new roll is only possible when the dig
/// position differs from the previous golden dig by more than half a
/// field-unit in X or Z (mFI_CheckDigDiffPosArea).
pub fn dig_differs_from_last(x: f32, z: f32, last_x: f32, last_z: f32, half_x: f32, half_z: f32) -> bool {
    x > last_x + half_x || x < last_x - half_x || z > last_z + half_z || z < last_z - half_z
}

/// Golden-shovel DIG injection: on a DIG result with the golden shovel,
/// when the area gate passes and the 1/10 roll hits, the status becomes
/// GET_ITEM with 100 Bells.
pub fn gold_shovel_dig_result(area_gate: bool, roll: u32) -> Option<u16> {
    if area_gate && roll == 1 {
        Some(GOLD_SHOVEL_MONEY_ITEM)
    } else {
        None
    }
}

/// REFLECT_SCOOP contact frame.
pub const REFLECT_SCOOP_CONTACT_FRAME: f32 = 13.0;
/// REFLECT_SCOOP reverse speed set at frame 13.
pub const REFLECT_SCOOP_REVERSE_SPEED: f32 = 4.8;

/// Shovel strike effect position: 37 units forward + 2-unit lateral
/// adjustment from the player (ac_pos.x += 37*sin + 2*cos, etc.).
pub fn reflect_scoop_effect_offset(sin: f32, cos: f32) -> (f32, f32) {
    (37.0 * sin + 2.0 * cos, 37.0 * cos - 2.0 * sin)
}

/// PUTIN_SCOOP burial effect frames: 18 normally, 25 for FILL_UP_I1.
pub const PUTIN_BURY_EFFECT_FRAME: f32 = 18.0;
pub const PUTIN_BURY_EFFECT_FRAME_FILL_UP: f32 = 25.0;

/// Axe durability: damage per hit.
pub const AXE_DAMAGE_NORMAL: i32 = 1;
pub const AXE_DAMAGE_REFLECTED: i32 = 3;
/// Damage threshold that advances the axe one wear stage (and resets
/// the counter): AXE -> USE_1 -> ... -> USE_7 -> EMPTY_NO.
pub const AXE_DAMAGE_STAGE_THRESHOLD: i32 = 9;
/// Wear stages: index = current item (0=AXE .. 7=USE_7); value = next.
pub const AXE_WEAR_NEXT: [u8; 8] = [1, 2, 3, 4, 5, 6, 7, 8]; // 8 = broken (EMPTY_NO)

/// Apply axe damage; returns (new_wear_stage, damage_counter_reset).
/// wear_stage: 0=AXE .. 7=USE_7, 8=broken.
pub fn axe_apply_damage(wear_stage: u8, damage: i32, reflected: bool) -> (u8, i32) {
    let damage = damage + if reflected { AXE_DAMAGE_REFLECTED } else { AXE_DAMAGE_NORMAL };
    if damage >= AXE_DAMAGE_STAGE_THRESHOLD {
        let next = if wear_stage < 8 { AXE_WEAR_NEXT[wear_stage as usize] } else { 8 };
        (next, 0)
    } else {
        (wear_stage, damage)
    }
}

/// Net capture evaluation frame (CatchSomethingCheck_common frame).
pub const NET_CAPTURE_FRAME: f32 = 6.0;
/// Net sweep lengths: normal 50, gold 60.
pub const NET_SWEEP_LEN: f32 = 50.0;
pub const NET_SWEEP_LEN_GOLD: f32 = 60.0;
/// Net radial tolerance base: 15 normal, 21 gold (plus target radius).
pub const NET_RADIAL_BASE: f32 = 15.0;
pub const NET_RADIAL_BASE_GOLD: f32 = 21.0;

/// Net sweep length for a gold flag.
pub fn net_sweep_length(gold: bool) -> f32 {
    if gold { NET_SWEEP_LEN_GOLD } else { NET_SWEEP_LEN }
}
/// Net radial tolerance for a gold flag and target radius.
pub fn net_radial_tol(gold: bool, rad_req: f32) -> f32 {
    if gold { NET_RADIAL_BASE_GOLD + rad_req } else { NET_RADIAL_BASE + rad_req }
}

/// Rod cast validation: ready-rod frame, forward projection, sample
/// offsets, water height limit.
pub const ROD_VALIDATE_FRAME: f32 = 10.0;
pub const ROD_CAST_DISTANCE: f32 = 100.0;
pub const ROD_SAMPLE_OFFSETS: [(f32, f32); 5] = [
    (0.0, 0.0), (-10.0, 10.0), (10.0, 10.0), (-10.0, -10.0), (10.0, -10.0),
];
pub const ROD_WATER_HEIGHT_LIMIT: f32 = 60.0;

/// One rod water-sample check: valid water attribute, no movable
/// background collision, surface less than 60 above the player.
pub fn rod_sample_ok(water_attr: bool, move_bg: bool, surface_above_player: f32) -> bool {
    water_attr && !move_bg && surface_above_player < ROD_WATER_HEIGHT_LIMIT
}

// ---- C ABI ----

/// C ABI: golden-shovel DIG injection. Returns the item id, or 0 when
/// no injection.
#[no_mangle]
pub extern "C" fn pc_gold_shovel_dig(area_gate: u8, roll: u32) -> u16 {
    gold_shovel_dig_result(area_gate != 0, roll).unwrap_or(0)
}

/// C ABI: axe damage application. Returns packed (wear_stage << 16) |
/// damage_counter.
#[no_mangle]
pub extern "C" fn pc_axe_apply_damage(wear_stage: u8, damage: i32, reflected: u8) -> u32 {
    let (w, d) = axe_apply_damage(wear_stage, damage, reflected != 0);
    ((w as u32) << 16) | (d as u32 & 0xFFFF)
}

/// C ABI: net sweep length / radial tolerance. mode 0 = sweep, 1 = radial.
#[no_mangle]
pub extern "C" fn pc_net_geometry(gold: u8, rad_req: f32, mode: u8) -> f32 {
    match mode {
        1 => net_radial_tol(gold != 0, rad_req),
        _ => net_sweep_length(gold != 0),
    }
}

/// C ABI: rod water-sample check.
#[no_mangle]
pub extern "C" fn pc_rod_sample_ok(water_attr: u8, move_bg: u8, surface_above: f32) -> u8 {
    rod_sample_ok(water_attr != 0, move_bg != 0, surface_above) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scoop_resolver_constants() {
        assert_eq!(SCOOP_NEIGHBORS.len(), 8);
        assert!(!SCOOP_NEIGHBORS.contains(&(0, 0)));
        assert_eq!(SCOOP_DIAGONALS, [0, 2, 5, 7]);
        assert!((SCOOP_DIAG_DIST_SQ - 4000.0).abs() < 0.01);
        assert_eq!(SCOOP_VERT_THRESHOLD, 63.245552);
        assert_eq!(SCOOP_NPC_RADIUS_SQ, 39.0 * 39.0);
        assert_eq!(dig_status::NUM, 6);
        assert_eq!(dig_status::DIG, 3);
        // Area gate: half-unit threshold.
        assert!(dig_differs_from_last(100.0, 0.0, 0.0, 0.0, 40.0, 40.0));
        assert!(!dig_differs_from_last(10.0, 0.0, 0.0, 0.0, 40.0, 40.0));
        // Golden shovel injection.
        assert_eq!(gold_shovel_dig_result(true, 1), Some(0x2103));
        assert_eq!(gold_shovel_dig_result(true, 2), None);
        assert_eq!(gold_shovel_dig_result(false, 1), None);
        assert_eq!(pc_gold_shovel_dig(1, 1), 0x2103);
        assert_eq!(pc_gold_shovel_dig(1, 5), 0);
        // Frame-13 effect offset: facing +Z (sin=0, cos=1) -> (2, 37).
        let (dx, dz) = reflect_scoop_effect_offset(0.0, 1.0);
        assert_eq!((dx, dz), (2.0, 37.0));
        assert_eq!(REFLECT_SCOOP_CONTACT_FRAME, 13.0);
        assert_eq!(REFLECT_SCOOP_REVERSE_SPEED, 4.8);
    }

    #[test]
    fn axe_durability() {
        // Fresh axe, normal hit x8: still stage 0, counter 8.
        let (w, d) = axe_apply_damage(0, 0, false);
        assert_eq!((w, d), (0, 1));
        let (w, d) = axe_apply_damage(0, 8, false);
        assert_eq!((w, d), (1, 0)); // 9 damage -> next wear stage, reset
        // Reflected hit: +3.
        let (w, d) = axe_apply_damage(1, 6, true);
        assert_eq!((w, d), (2, 0));
        let (w, d) = axe_apply_damage(1, 5, true);
        assert_eq!((w, d), (1, 8));
        // Final stage breaks.
        let (w, d) = axe_apply_damage(7, 8, false);
        assert_eq!((w, d), (8, 0));
        // C ABI packing.
        assert_eq!(pc_axe_apply_damage(0, 8, 0), (1 << 16) | 0);
    }

    #[test]
    fn net_and_rod() {
        assert_eq!(net_sweep_length(false), 50.0);
        assert_eq!(net_sweep_length(true), 60.0);
        assert_eq!(net_radial_tol(false, 5.0), 20.0);
        assert_eq!(net_radial_tol(true, 5.0), 26.0);
        assert_eq!(NET_CAPTURE_FRAME, 6.0);
        assert_eq!(pc_net_geometry(1, 5.0, 1), 26.0);
        assert_eq!(ROD_SAMPLE_OFFSETS.len(), 5);
        assert!(rod_sample_ok(true, false, 59.9));
        assert!(!rod_sample_ok(true, false, 60.0));
        assert!(!rod_sample_ok(false, false, 0.0));
        assert!(!rod_sample_ok(true, true, 0.0));
        assert_eq!(pc_rod_sample_ok(1, 0, 10.0), 1);
        assert_eq!(ROD_CAST_DISTANCE, 100.0);
    }
}
