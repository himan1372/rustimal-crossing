//! Cliff/slate classification (`mCoBG_CheckCliffAttr`,
//! `mCoBG_Wpos2CheckSlateCol`) and the slate collision-triangle builder.
//!
//! Verified against `m_collision_bg_info.c_inc` (USA Rev. 0 decomp,
//! lines 81-117 and 1040-1098) and `m_collision_bg_line.c_inc`
//! (lines 119-275).
//!
//! CORRECTION to the brief: both symbols DO exist in the current source
//! (the brief searched the wrong files). They are small, exact, and now
//! ported verbatim.
//!
//! Architecture recap (source-proven):
//! - `slate_flag` (1 bit): physical collision shape — selects the slate
//!   ground-height path and the slate triangle builder.
//! - `unit_attribute == 63`: semantic topology proxy — resolved through
//!   the neighboring unit, never returned as a gameplay attribute.
//! - `CheckCliffAttr`: purely semantic — raw attrs 47-58 (cliff/tunnel).
//! - `Wpos2CheckSlateCol`: slate_flag OR (check_attr && attr in a fixed
//!   14-attribute set). Used by the snowman actor (check_attr=FALSE, so a
//!   pure slate_flag test) and, via CheckCliffAttr, by the talk camera.

/// `mCoBG_CheckCliffAttr` verbatim: TRUE for raw attrs 47-54 (grass4
/// cliff/tunnel) and 55-58 (grass3 cliff).
pub fn check_cliff_attr(attr: u32) -> bool {
    (47..=54).contains(&attr) || (55..=58).contains(&attr)
}

/// The 14 raw attributes that make `mCoBG_Wpos2CheckSlateCol` return TRUE
/// when `check_attr` is set (and slate_flag is clear):
/// wood-bridge pieces 27-30 (NOT the 31 center), wave_se/sw 37-38
/// (NOT wave_s 36), river banks 39-42, grass3 cliff 55-58.
pub const SLATE_COL_ATTRS: [u32; 14] = [
    27, 28, 29, 30, 37, 38, 39, 40, 41, 42, 55, 56, 57, 58,
];

/// `mCoBG_Wpos2CheckSlateCol` verbatim (minus the unit lookup, which the
/// caller supplies): slate_flag => TRUE; else if check_attr, TRUE iff
/// the raw attribute is in `SLATE_COL_ATTRS`.
pub fn wpos2check_slate_col(slate_flag: bool, attr: u32, check_attr: bool) -> bool {
    if slate_flag {
        return true;
    }
    if check_attr {
        return SLATE_COL_ATTRS.contains(&attr);
    }
    false
}

/// `mCoBG_WoodSoundEffect` verbatim (adjacent in the same file): TRUE
/// for WOOD (23) and wood-bridge attrs 27-31 (INCLUDING the 31 center,
/// unlike the slate-col set).
pub fn wood_sound_effect(attr: u32) -> bool {
    attr == 23 || (27..=31).contains(&attr)
}

/// Slate branch of `mCoBG_GetBgNorm_FromWpos` (info.c_inc:81): a slate
/// unit reports a straight-up normal (0, 100, 0) instead of the
/// geometric slope normal. The non-slate branch needs GetNormTriangle
/// (not yet ported); this kernel covers the slate case plus the
/// flat-terrain case the source shares with it.
pub fn slate_ground_normal(slate_flag: bool) -> (f32, f32, f32) {
    let _ = slate_flag;
    (0.0, 100.0, 0.0)
}

/// Slate triangle Y-equalization from `mCoBG_GetAreaPolygon`
/// (line.c_inc:119). Inputs are the world-space Y offsets the source
/// compares (`corner * 10 + base_height`); returns the three vertex Y
/// values (v0, v1, v2) for the given area after slate equalization.
///
/// Vertex layout per area (source):
/// - N: v0=leftUp, v1=center, v2=rightUp
/// - W: v0=leftUp, v1=leftDown, v2=center
/// - S: v0=center, v1=leftDown, v2=rightDown
/// - E: v0=center, v1=rightDown, v2=rightUp
///
/// Equalization (USA Rev. 0, unfixed):
/// - N: leftUp<rightUp -> all center; leftUp>rightUp -> all rightUp
/// - W: leftUp>leftDown -> v0=v1 (twice — the @BUG: the source notes
///   this should be v2=v1; the BUGFIX build differs); leftUp<leftDown
///   -> v1=v0, v2=v0
/// - S: leftDown<rightDown -> v0=v1, v2=v1; leftDown>rightDown -> v0=v2, v1=v2
/// - E: rightUp<rightDown -> v0=v2, v1=v2; rightUp>rightDown -> v0=v1, v2=v1
pub fn slate_area_polygon_y(
    area: u8, // 0=N, 1=W, 2=S, 3=E
    left_up: f32,
    left_down: f32,
    right_down: f32,
    right_up: f32,
    center: f32,
) -> (f32, f32, f32) {
    match area {
        0 => {
            if left_up < right_up {
                (center, center, center)
            } else if left_up > right_up {
                (right_up, right_up, right_up)
            } else {
                (left_up, center, right_up)
            }
        }
        1 => {
            if left_up > left_down {
                // @BUG preserved: the second assignment is `v0->y = v1->y`
                // again (should be v2->y = v1->y per the source comment).
                // Net effect: v0 = v1 = leftDown, v2 stays center.
                (left_down, left_down, center)
            } else if left_up < left_down {
                (left_up, left_up, left_up)
            } else {
                (left_up, left_down, center)
            }
        }
        2 => {
            if left_down < right_down {
                (left_down, left_down, left_down)
            } else if left_down > right_down {
                (right_down, right_down, right_down)
            } else {
                (center, left_down, right_down)
            }
        }
        _ => {
            // E (source `default`)
            if right_up < right_down {
                (right_up, right_up, right_up)
            } else if right_up > right_down {
                (right_down, right_down, right_down)
            } else {
                (center, right_down, right_up)
            }
        }
    }
}

// ---- C ABI ----

/// C ABI: `mCoBG_CheckCliffAttr`; 1 = cliff attribute (47-58).
#[no_mangle]
pub extern "C" fn pc_check_cliff_attr(attr: u32) -> u8 {
    check_cliff_attr(attr) as u8
}

/// C ABI: `mCoBG_Wpos2CheckSlateCol`; 1 = slate collision at this unit.
#[no_mangle]
pub extern "C" fn pc_wpos2check_slate_col(slate_flag: u8, attr: u32, check_attr: u8) -> u8 {
    wpos2check_slate_col(slate_flag != 0, attr, check_attr != 0) as u8
}

/// C ABI: `mCoBG_WoodSoundEffect`; 1 = wood footstep sound.
#[no_mangle]
pub extern "C" fn pc_wood_sound_effect(attr: u32) -> u8 {
    wood_sound_effect(attr) as u8
}

/// C ABI: slate triangle vertex Y values; writes v0/v1/v2.
#[no_mangle]
pub unsafe extern "C" fn pc_slate_area_polygon_y(
    area: u8,
    left_up: f32,
    left_down: f32,
    right_down: f32,
    right_up: f32,
    center: f32,
    out_v0: *mut f32,
    out_v1: *mut f32,
    out_v2: *mut f32,
) {
    let (v0, v1, v2) = slate_area_polygon_y(area, left_up, left_down, right_down, right_up, center);
    if !out_v0.is_null() {
        unsafe { *out_v0 = v0 };
    }
    if !out_v1.is_null() {
        unsafe { *out_v1 = v1 };
    }
    if !out_v2.is_null() {
        unsafe { *out_v2 = v2 };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cliff_and_slate_col() {
        // CheckCliffAttr: 47-58 exactly.
        for a in 47..=58 {
            assert!(check_cliff_attr(a), "attr {}", a);
        }
        assert!(!check_cliff_attr(46));
        assert!(!check_cliff_attr(59));
        assert!(!check_cliff_attr(63));
        // Wpos2CheckSlateCol: slate_flag dominates.
        assert!(wpos2check_slate_col(true, 0, false));
        assert!(wpos2check_slate_col(true, 0, true));
        assert!(!wpos2check_slate_col(false, 0, false));
        assert!(!wpos2check_slate_col(false, 0, true));
        // The 14-attribute set.
        for a in SLATE_COL_ATTRS {
            assert!(wpos2check_slate_col(false, a, true), "attr {}", a);
        }
        // Exclusions: bridge center 31, wave_s 36, cliff 47-54, slope 63.
        for a in [31u32, 36, 47, 54, 63] {
            assert!(!wpos2check_slate_col(false, a, true), "attr {}", a);
        }
        // Wood sound: WOOD + 27-31 (center included here).
        assert!(wood_sound_effect(23));
        assert!(wood_sound_effect(31));
        assert!(!wood_sound_effect(32));
        // Slate normal is straight up.
        assert_eq!(slate_ground_normal(true), (0.0, 100.0, 0.0));
        // C ABI.
        assert_eq!(pc_check_cliff_attr(50), 1);
        assert_eq!(pc_check_cliff_attr(46), 0);
        assert_eq!(pc_wpos2check_slate_col(0, 27, 1), 1);
        assert_eq!(pc_wpos2check_slate_col(0, 31, 1), 0);
        assert_eq!(pc_wood_sound_effect(31), 1);
    }

    #[test]
    fn slate_polygon_equalization() {
        // N: leftUp < rightUp -> all collapse to center.
        assert_eq!(slate_area_polygon_y(0, 10.0, 0.0, 0.0, 30.0, 20.0), (20.0, 20.0, 20.0));
        // N: leftUp > rightUp -> all collapse to rightUp.
        assert_eq!(slate_area_polygon_y(0, 30.0, 0.0, 0.0, 10.0, 20.0), (10.0, 10.0, 10.0));
        // N: equal -> untouched.
        assert_eq!(slate_area_polygon_y(0, 10.0, 0.0, 0.0, 10.0, 20.0), (10.0, 20.0, 10.0));
        // W: leftUp > leftDown -> @BUG: v0=v1=leftDown, v2 stays center.
        assert_eq!(slate_area_polygon_y(1, 30.0, 10.0, 0.0, 0.0, 20.0), (10.0, 10.0, 20.0));
        // W: leftUp < leftDown -> v1=v2=v0.
        assert_eq!(slate_area_polygon_y(1, 10.0, 30.0, 0.0, 0.0, 20.0), (10.0, 10.0, 10.0));
        // S: leftDown < rightDown -> all leftDown.
        assert_eq!(slate_area_polygon_y(2, 0.0, 10.0, 30.0, 0.0, 20.0), (10.0, 10.0, 10.0));
        // S: leftDown > rightDown -> all rightDown.
        assert_eq!(slate_area_polygon_y(2, 0.0, 30.0, 10.0, 0.0, 20.0), (10.0, 10.0, 10.0));
        // E: rightUp < rightDown -> all rightUp.
        assert_eq!(slate_area_polygon_y(3, 0.0, 0.0, 30.0, 10.0, 20.0), (10.0, 10.0, 10.0));
        // E: rightUp > rightDown -> all rightDown.
        assert_eq!(slate_area_polygon_y(3, 0.0, 0.0, 10.0, 30.0, 20.0), (10.0, 10.0, 10.0));
        // C ABI writes.
        let (mut a, mut b, mut c) = (0.0f32, 0.0f32, 0.0f32);
        unsafe { pc_slate_area_polygon_y(0, 10.0, 0.0, 0.0, 30.0, 20.0, &mut a, &mut b, &mut c) };
        assert_eq!((a, b, c), (20.0, 20.0, 20.0));
    }
}
