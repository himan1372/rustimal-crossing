//! Actor-relative wall-hit direction classification.
//!
//! Verified against `m_collision_bg.c` (USA Rev. 0 decomp).
//!
//! Correction to the brief: `mCoBG_Check45Angle` is NOT a 4-way
//! FRONT/RIGHT/LEFT/BACK classifier — it is a boolean predicate that
//! reports whether two short-angles are within 45 degrees of each
//! other on the circle. The FRONT/RIGHT/LEFT/BACK classification is the
//! CALLER's else-if chain (`mCoBG_SearchColOwnPart`), which probes
//! rotated copies of the wall normal angle:
//!
//! - FRONT: `Check45Angle(wall + (180° - 1 tick), actor)`
//! - RIGHT: `Check45Angle(wall + (-90°), actor)`
//! - LEFT:  `Check45Angle(wall + (+90°), actor)`
//! - BACK:  `Check45Angle(wall, actor)`
//!
//! So FRONT structurally means "the actor faces within 45° of the
//! wall-normal's opposite" (priority: FRONT > RIGHT > LEFT > BACK).
//! Each probe reuses the same 45° window; the `-1` tick on the 180°
//! rotation is verbatim.

/// Hit-flag bits (`m_collision_bg.h`).
pub mod hit_flag {
    pub const DIDNT_HIT: u32 = 0;
    pub const HIT_WALL: u32 = 1 << 0;
    pub const FRONT: u32 = 1 << 1;
    pub const RIGHT: u32 = 1 << 2;
    pub const LEFT: u32 = 1 << 3;
    pub const BACK: u32 = 1 << 4;
}

/// 45° in short-angle ticks: `DEG2SHORT_ANGLE(45.0f)` = 0x2000.
pub const ANGLE_45_TICKS: i32 = 8192;
/// Wraparound threshold: `(u16)(DEG2SHORT_ANGLE(-45.0f) - 1)` = 0xDFFF.
/// Catches raw differences whose circular distance is small, e.g.
/// d = -60000 -> circular -60000 + 65536 = 5536 ticks.
/// NOTE the verbatim off-by-one: accepts circular distance up to 8193
/// ticks, not 8192.
pub const ANGLE_WRAP_TICKS: i32 = 57343;
/// `DEG2SHORT_ANGLE2(180.0f) - 1` = 32767 (the FRONT probe rotation).
pub const ANGLE_180_M1_TICKS: i16 = 32767;
/// `DEG2SHORT_ANGLE2(90.0f)` = 16384 (the LEFT/RIGHT probe rotations).
pub const ANGLE_90_TICKS: i16 = 16384;

/// `mCoBG_Check45Angle` verbatim: TRUE when the two short-angles are
/// within a 45° window on the circle (either directly or via wraparound).
pub fn check_45_angle(angle0: i16, angle1: i16) -> bool {
    let d = (angle1 as i32 - angle0 as i32).abs();
    d <= ANGLE_45_TICKS || d >= ANGLE_WRAP_TICKS
}

/// Direction sectors from `mCoBG_SearchColOwnPart`, in probe priority order.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HitDir {
    Front,
    Right,
    Left,
    Back,
}

/// `mCoBG_SearchColOwnPart` direction selection (the flag writes are left
/// to the caller): first matching sector in FRONT > RIGHT > LEFT > BACK
/// order. Wrapping addition reproduces the source's implicit int->s16
/// conversion of the rotated angle.
pub fn search_col_own_part(wall_angle: i16, actor_angle: i16) -> Option<HitDir> {
    if check_45_angle(wall_angle.wrapping_add(ANGLE_180_M1_TICKS), actor_angle) {
        Some(HitDir::Front)
    } else if check_45_angle(wall_angle.wrapping_add(-ANGLE_90_TICKS), actor_angle) {
        Some(HitDir::Right)
    } else if check_45_angle(wall_angle.wrapping_add(ANGLE_90_TICKS), actor_angle) {
        Some(HitDir::Left)
    } else if check_45_angle(wall_angle, actor_angle) {
        Some(HitDir::Back)
    } else {
        None
    }
}

/// Two-wall opposing check from `mCoBG_MakePartDirectHitWallFlag`:
/// `dangle` (u16 wall-angle difference) within ±3 ticks of 180°.
pub fn walls_opposing(dangle: u16) -> bool {
    dangle > 32768 - 3 && dangle < 32768 + 3
}

/// Two-wall close-angle check: `dangle < DEG2SHORT_ANGLE2(67.5f)` = 12288.
pub fn walls_close_angle(dangle: u16) -> bool {
    dangle < 12288
}

// ---- C ABI ----

/// C ABI: the 45° predicate; 1 = within the window.
#[no_mangle]
pub extern "C" fn pc_check_45_angle(angle0: i16, angle1: i16) -> u8 {
    check_45_angle(angle0, angle1) as u8
}

/// C ABI: direction sector; 0 = none, 1 = front, 2 = right, 3 = left, 4 = back.
#[no_mangle]
pub extern "C" fn pc_hit_wall_dir(wall_angle: i16, actor_angle: i16) -> u8 {
    match search_col_own_part(wall_angle, actor_angle) {
        None => 0,
        Some(HitDir::Front) => 1,
        Some(HitDir::Right) => 2,
        Some(HitDir::Left) => 3,
        Some(HitDir::Back) => 4,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn predicate_windows() {
        // Direct window: |d| <= 8192.
        assert!(check_45_angle(0, 8192));
        assert!(check_45_angle(0, -8192));
        assert!(!check_45_angle(0, 8193));
        assert!(!check_45_angle(0, -8193));
        // Wraparound window: |d| >= 57343 (d = 65535 here).
        assert!(check_45_angle(-32768, 32767));
        assert!(!check_45_angle(0, 32767));
        // Genuine wraparound: raw d = -60000, circular = 5536 ticks.
        assert!(check_45_angle(30000, -30000));
        // Raw d = -40000, circular = 25536 ticks -> outside.
        assert!(!check_45_angle(20000, -20000));
        // Verbatim off-by-one: raw d = -57343 (abs exactly 57343)
        // still accepted; circularly that is 8193 ticks.
        assert!(check_45_angle(24576, -32767));
    }

    #[test]
    fn direction_classification() {
        // Wall normal 0 (north wall). Actor facing ~180° (32767-ish)
        // -> FRONT (faces the reversed normal).
        assert_eq!(search_col_own_part(0, 32767), Some(HitDir::Front));
        // Actor facing the normal itself -> BACK.
        assert_eq!(search_col_own_part(0, 0), Some(HitDir::Back));
        // Actor facing wall-90° -> RIGHT; wall+90° -> LEFT.
        assert_eq!(search_col_own_part(0, -16384), Some(HitDir::Right));
        assert_eq!(search_col_own_part(0, 16384), Some(HitDir::Left));
        // Priority: exactly on the FRONT/RIGHT boundary goes FRONT.
        // wall=0, actor=8192+? -> probe: front needs |a-32767|<=8192;
        // craft actor = 32767-8192 = 24575: front hit, right probe
        // |24575-(-16384)| = 40959 -> miss. Use boundary overlap instead:
        // actor facing such that both front and right windows catch it is
        // impossible here (windows are 90° apart), so check a 45°-exact tie
        // on one probe: actor = 32767 - 8192 is inside FRONT only.
        assert_eq!(search_col_own_part(0, 24575), Some(HitDir::Front));
        // The four 45° probes tile the circle completely (the wraparound
        // clause closes the 1-tick seam), so every actor angle lands in
        // some sector: 10000 is 6384 ticks from the LEFT probe center.
        assert_eq!(search_col_own_part(0, 10000), Some(HitDir::Left));
        // Seam tick: circularly 8193 from the FRONT probe center, caught
        // by the verbatim wraparound off-by-one.
        assert_eq!(search_col_own_part(0, -24577), Some(HitDir::Front));
        // C ABI codes.
        assert_eq!(pc_hit_wall_dir(0, 32767), 1);
        assert_eq!(pc_hit_wall_dir(0, 0), 4);
        assert_eq!(pc_hit_wall_dir(0, 10000), 3);
    }

    #[test]
    fn two_wall_checks() {
        assert!(walls_opposing(32768));
        assert!(walls_opposing(32766));
        assert!(!walls_opposing(32765));
        assert!(!walls_opposing(32771));
        assert!(walls_close_angle(12287));
        assert!(!walls_close_angle(12288));
    }
}
