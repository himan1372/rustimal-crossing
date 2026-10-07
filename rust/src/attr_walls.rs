//! Attribute/forbidden-wall lookup tables for the Rust rewrite.
//!
//! Source-verified (upstream `src/game/m_collision_bg.c`,
//! `src/game/m_collision_bg_wall.c_inc`,
//! `src/game/m_collision_bg_info.c_inc`, `include/m_collision_bg.h`):
//!
//! Collision units carry a 6-bit `unit_attribute` (0–63). Attributes
//! 27–62 are special terrain-edge descriptors that can synthesize
//! *explicit* wall geometry on top of ordinary height-difference walls.
//!
//! * `mCoBG_make_vector_table[8]` (`m_collision_bg.c:82`): eight
//!   reusable synthetic wall definitions — four cardinal, four
//!   diagonal (slate) — each with a short-angle normal angle, a
//!   2D normal, and a wall name.
//! * `mCoBG_forbid_vector_idx[36][2]` (`m_collision_bg.c:93`):
//!   maps attributes 27–62 to up to two vector IDs; `-1` = none.
//!   The two-wall entries (51–54 tunnels, 59–62 river-bank corners)
//!   encode a corner as two independent synthetic segments.
//! * Generation gate (`m_collision_bg_wall.c_inc:508`):
//!   `forbid_proc = (old_on_ground & attr_wall) & 1` selects
//!   between `mCoBG_MakeForbidAttrVector_DUMMY` (does nothing) and
//!   `mCoBG_MakeForbidAttrVector`. Forbidden vectors only appear
//!   when the actor was previously grounded AND the attribute-wall
//!   flag is on.
//! * Generated walls get `atr_wall = TRUE`, `regist_p = NULL`
//!   (no moving-background pointer), and their segment from
//!   `mCoBG_UnitNoName2StartEnd` — which uses the check-type
//!   padding table (`{5.0,10.0}` / `{1e-6,2e-6}`), so NORMAL and
//!   PLAYER get different segment extents. No wall-height bounds
//!   are assigned to attribute walls here.
//! * `attr_wall` also switches ordinary wall registration between
//!   the `..._AttributeOff` and `..._AttributeOn` normal/slate
//!   variants.
//! * The same forbid table is reused by
//!   `mCoBG_CheckAttribute_BallRolling`
//!   (`m_collision_bg_info.c_inc:858`), with each vector's normal
//!   angle flipped by +180°.
//!
//! Confirmed vs inferred: the eight-vector table, the 36×2 index
//! table, the 27–62 attribute comments (bridges/river/cliff/tunnel
//! family), the DUMMY gate, `atr_wall=TRUE`, and the ball-rolling
//! reuse are all confirmed from source. Interpreting 27–62 as a
//! "declarative geometry language" for terrain edges is the
//! brief's strongly supported model; the original asset-authoring
//! pipeline that produced these attributes is still unknown.

/// Wall-name constants (`include/m_collision_bg.h`).
pub mod wall_name {
    pub const UP: u8 = 0;
    pub const LEFT: u8 = 1;
    pub const DOWN: u8 = 2;
    pub const RIGHT: u8 = 3;
    pub const SLATE_UP: u8 = 4;
    pub const SLATE_DOWN: u8 = 5;
}

/// One synthetic wall definition (`mCoBG_forbid_vec_data_c`).
/// `norm_angle_deg` stands in for the short-angle value; the source
/// uses DEG2SHORT_ANGLE2 of the same degrees.
#[derive(Clone, Copy, Debug)]
pub struct ForbidVecData {
    pub norm_angle_deg: f32,
    pub norm: [f32; 2],
    pub wall_name: u8,
}

const SQRT2_2: f32 = 0.7071067811865476;

/// `mCoBG_make_vector_table[8]` — verbatim.
pub const MAKE_VECTOR_TABLE: [ForbidVecData; 8] = [
    ForbidVecData { norm_angle_deg: 0.0, norm: [0.0, 1.0], wall_name: wall_name::UP },
    ForbidVecData { norm_angle_deg: -90.0, norm: [-1.0, 0.0], wall_name: wall_name::RIGHT },
    ForbidVecData { norm_angle_deg: 90.0, norm: [1.0, 0.0], wall_name: wall_name::LEFT },
    ForbidVecData { norm_angle_deg: 180.0, norm: [0.0, -1.0], wall_name: wall_name::DOWN },
    ForbidVecData { norm_angle_deg: 45.0, norm: [SQRT2_2, SQRT2_2], wall_name: wall_name::SLATE_UP },
    ForbidVecData { norm_angle_deg: 135.0, norm: [SQRT2_2, -SQRT2_2], wall_name: wall_name::SLATE_DOWN },
    ForbidVecData { norm_angle_deg: 225.0, norm: [-SQRT2_2, -SQRT2_2], wall_name: wall_name::SLATE_UP },
    ForbidVecData { norm_angle_deg: 315.0, norm: [SQRT2_2, -SQRT2_2], wall_name: wall_name::SLATE_DOWN },
];

/// `mCoBG_forbid_vector_idx[36][2]` — verbatim. Index = attr − 27.
pub const FORBID_VECTOR_IDX: [[i16; 2]; 36] = [
    [4, -1], [5, -1], [6, -1], [7, -1], [-1, -1], [0, -1], [1, -1], [2, -1],
    [3, -1], [3, -1], [6, -1], [5, -1], [4, -1], [5, -1], [6, -1], [7, -1],
    [0, -1], [1, -1], [2, -1], [3, -1], [0, -1], [1, -1], [2, -1], [3, -1],
    [0, 2], [3, 2], [3, 1], [0, 1], [4, -1], [5, -1], [6, -1], [7, -1],
    [0, 2], [3, 2], [3, 1], [0, 1],
];

/// Decomp header comments for attributes 27–62
/// (`include/m_collision_bg.h:79`), in attribute order.
pub const ATTRIBUTE_NAMES: [&str; 36] = [
    "wood bridge nw", "wood bridge sw", "wood bridge se", "wood bridge ne",
    "wood bridge center", "stone bridge n", "stone bridge e", "stone bridge w",
    "stone bridge s", "wave_s", "wave_se", "wave_sw",
    "river bank nw", "river bank sw", "river bank se", "river bank ne",
    "grass 3 north (river)", "grass 3 east (river)", "grass 3 west (river)", "grass 3 south (river)",
    "grass 4 north (cliff)", "grass 4 east (cliff)", "grass 4 west (cliff)", "grass 4 south (cliff)",
    "grass 4 tunnel left upper", "grass 4 tunnel left lower", "grass 4 tunnel right lower", "grass 4 tunnel right upper",
    "grass 3 north west (cliff)", "grass 3 south west (cliff)", "grass 3 south east (cliff)", "grass 3 north east (cliff)",
    "grass 3 north west (river bank)", "grass 3 south west (river bank)", "grass 3 south east (river bank)", "grass 3 north east (river bank)",
];

pub const ATTRIBUTE_FIRST: u8 = 27;
pub const ATTRIBUTE_LAST: u8 = 62;

/// Attribute-wall attribute range check
/// (`mCoBG_MakeForbidAttrVector`).
pub fn is_forbid_attribute(attr: u8) -> bool {
    attr >= ATTRIBUTE_FIRST && attr <= ATTRIBUTE_LAST
}

/// The vector IDs an attribute expands to (`mCoBG_MakeForbidVectorData`).
/// Returns the indices into `MAKE_VECTOR_TABLE`; empty for attributes
/// outside 27–62 or the all-`{-1,-1}` entry (attr 31).
pub fn forbid_vectors(attr: u8) -> Vec<usize> {
    if !is_forbid_attribute(attr) {
        return Vec::new();
    }
    FORBID_VECTOR_IDX[(attr - ATTRIBUTE_FIRST) as usize]
        .iter()
        .filter(|&&i| i != -1)
        .map(|&i| i as usize)
        .collect()
}

/// True for two-wall (corner) attributes 51–54 and 59–62.
pub fn is_two_wall_attribute(attr: u8) -> bool {
    matches!(attr, 51..=54 | 59..=62)
}

/// Generation gate: `forbid_proc = (old_on_ground & attr_wall) & 1`.
/// true = run the real `mCoBG_MakeForbidAttrVector`; false = DUMMY.
pub fn forbid_generation_enabled(old_on_ground: bool, attr_wall: bool) -> bool {
    (old_on_ground as u8 & attr_wall as u8) & 1 == 1
}

/// Ball-rolling reuse (`mCoBG_CheckAttribute_BallRolling`): for each
/// emitted vector, the angle is the vector's normal angle + 180°.
/// `None` marks the unused slot (the source's `-1` sentinel).
pub fn ball_rolling_angles(attr: u8) -> [Option<f32>; 2] {
    let mut out = [None, None];
    for (slot, vid) in forbid_vectors(attr).into_iter().enumerate() {
        out[slot] = Some(MAKE_VECTOR_TABLE[vid].norm_angle_deg + 180.0);
    }
    out
}

/// An attribute wall as it enters the wall-vector list:
/// `atr_wall = TRUE`, no moving-BG registration, no height bounds
/// assigned by the generator.
#[derive(Clone, Copy, Debug)]
pub struct AttributeWallSpec {
    pub vector_id: usize,
    pub normal: [f32; 2],
    pub norm_angle_deg: f32,
    pub wall_name: u8,
    pub atr_wall: bool,
    pub has_moving_bg: bool,
}

/// Build the attribute-wall spec the generator registers
/// (`mCoBG_MakeForbidVectorData` core, before segment computation).
pub fn attribute_wall_spec(vector_id: usize) -> AttributeWallSpec {
    let v = MAKE_VECTOR_TABLE[vector_id];
    AttributeWallSpec {
        vector_id,
        normal: v.norm,
        norm_angle_deg: v.norm_angle_deg,
        wall_name: v.wall_name,
        atr_wall: true,
        has_moving_bg: false,
    }
}

/// Which ordinary-wall registration variant is selected by the
/// attribute-wall flag (the `..._AttributeOff` / `..._AttributeOn`
/// tables), for both normal and slate walls.
pub fn wall_registrar_variant(attr_wall: bool) -> &'static str {
    if attr_wall {
        "AttributeOn"
    } else {
        "AttributeOff"
    }
}

use crate::segment_map::{unit_no_name_2_start_end, CheckType, WallName};

const WALL_NAME_FOR_VECTOR: [WallName; 8] = [
    WallName::Up,
    WallName::Right,
    WallName::Left,
    WallName::Down,
    WallName::SlateUp,
    WallName::SlateDown,
    WallName::SlateUp,
    WallName::SlateDown,
];

/// Full forbidden-wall segment (`mCoBG_MakeForbidVectorData`):
/// vector ID -> wall_name -> segment via `mCoBG_UnitNoName2StartEnd`.
/// Returns (start, end) X/Z pairs in vector order for `attr`.
pub fn forbid_wall_segments(
    attr: u8,
    ux: f32,
    uz: f32,
    check_type: CheckType,
) -> Vec<([f32; 2], [f32; 2])> {
    forbid_vectors(attr)
        .into_iter()
        .map(|vid| {
            let (s, e) = unit_no_name_2_start_end(ux, uz, WALL_NAME_FOR_VECTOR[vid], check_type);
            (s, e)
        })
        .collect()
}
/// C ABI: write the vector IDs for `attr` into `out` (capacity ≥ 2);
/// returns the number of vectors (0–2).
#[no_mangle]
pub extern "C" fn pc_forbid_vectors(attr: u8, out: *mut u8) -> u8 {
    let v = forbid_vectors(attr);
    if !out.is_null() && v.len() > 0 {
        let dst = unsafe { core::slice::from_raw_parts_mut(out, v.len()) };
        for (i, id) in v.iter().enumerate() {
            dst[i] = *id as u8;
        }
    }
    v.len() as u8
}

/// C ABI: the generation gate (`forbid_proc`).
#[no_mangle]
pub extern "C" fn pc_forbid_gate(old_on_ground: u8, attr_wall: u8) -> u8 {
    forbid_generation_enabled(old_on_ground != 0, attr_wall != 0) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tables_have_expected_shapes() {
        assert_eq!(MAKE_VECTOR_TABLE.len(), 8);
        assert_eq!(FORBID_VECTOR_IDX.len(), 36);
        assert_eq!(ATTRIBUTE_NAMES.len(), 36);
        // Cardinal then slate ordering.
        assert_eq!(MAKE_VECTOR_TABLE[0].wall_name, wall_name::UP);
        assert_eq!(MAKE_VECTOR_TABLE[4].wall_name, wall_name::SLATE_UP);
        assert_eq!(MAKE_VECTOR_TABLE[5].wall_name, wall_name::SLATE_DOWN);
    }

    #[test]
    fn bridge_river_cliff_mappings() {
        assert_eq!(forbid_vectors(27), vec![4]); // wood bridge nw -> diagonal
        assert_eq!(forbid_vectors(32), vec![0]); // stone bridge n -> UP
        assert_eq!(forbid_vectors(35), vec![3]); // stone bridge s -> DOWN
        assert!(forbid_vectors(31).is_empty()); // wood bridge center -> none
        assert_eq!(forbid_vectors(36), vec![3]); // wave_s -> DOWN
        assert_eq!(forbid_vectors(43), vec![0]); // grass 3 north (river)
        assert_eq!(forbid_vectors(47), vec![0]); // grass 4 north (cliff)
    }

    #[test]
    fn two_wall_corners() {
        assert_eq!(forbid_vectors(51), vec![0, 2]);
        assert_eq!(forbid_vectors(52), vec![3, 2]);
        assert_eq!(forbid_vectors(53), vec![3, 1]);
        assert_eq!(forbid_vectors(54), vec![0, 1]);
        assert_eq!(forbid_vectors(59), vec![0, 2]);
        assert_eq!(forbid_vectors(62), vec![0, 1]);
        for a in [51, 52, 53, 54, 59, 60, 61, 62] {
            assert!(is_two_wall_attribute(a));
        }
        assert!(!is_two_wall_attribute(47));
    }

    #[test]
    fn range_and_names() {
        assert!(is_forbid_attribute(27));
        assert!(is_forbid_attribute(62));
        assert!(!is_forbid_attribute(26));
        assert!(!is_forbid_attribute(63));
        assert!(forbid_vectors(63).is_empty());
        assert_eq!(ATTRIBUTE_NAMES[0], "wood bridge nw");
        assert_eq!(ATTRIBUTE_NAMES[35], "grass 3 north east (river bank)");
    }

    #[test]
    fn generation_gate() {
        assert!(forbid_generation_enabled(true, true));
        assert!(!forbid_generation_enabled(false, true));
        assert!(!forbid_generation_enabled(true, false));
        assert!(!forbid_generation_enabled(false, false));
    }

    #[test]
    fn ball_rolling_reuses_table() {
        let a = ball_rolling_angles(32);
        assert_eq!(a, [Some(180.0), None]); // 0° + 180°
        let a = ball_rolling_angles(51);
        assert_eq!(a, [Some(180.0), Some(270.0)]); // 0°+180, 90°+180
        assert_eq!(ball_rolling_angles(31), [None, None]);
    }

    #[test]
    fn spec_and_registrar() {
        let s = attribute_wall_spec(4);
        assert!(s.atr_wall && !s.has_moving_bg);
        assert_eq!(s.wall_name, wall_name::SLATE_UP);
        assert_eq!(wall_registrar_variant(true), "AttributeOn");
        assert_eq!(wall_registrar_variant(false), "AttributeOff");
    }

    #[test]
    fn forbid_segments_use_real_mapping() {
        use crate::segment_map::CheckType;
        // Attr 32 (stone bridge n) -> vector 0 (UP) at unit (1,1).
        let segs = forbid_wall_segments(32, 1.0, 1.0, CheckType::Normal);
        assert_eq!(segs.len(), 1);
        let (s, e) = segs[0];
        assert!((s[0] - 35.0).abs() < 1e-4 && (s[1] - 40.0).abs() < 1e-4);
        assert!((e[0] - 85.0).abs() < 1e-4 && (e[1] - 40.0).abs() < 1e-4);
        // Attr 51 (tunnel) -> two segments.
        assert_eq!(forbid_wall_segments(51, 0.0, 0.0, CheckType::Normal).len(), 2);
        assert!(forbid_wall_segments(31, 0.0, 0.0, CheckType::Normal).is_empty());
    }
}
