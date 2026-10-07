//! `mCoBG_Wpos2Attribute()` — the effective gameplay-terrain interpreter.
//!
//! Verified against `m_collision_bg.c` (USA Rev. 0 decomp, lines 1415-1606).
//!
//! This is the engine's semantic projection of the raw 6-bit collision
//! `unit_attribute` into the terrain meaning gameplay should see at a
//! world XZ position. Its outputs are:
//!
//! - the effective attribute (stored into the actor's collision result by
//!   `mCoBG_GroundCheck()`), and
//! - `cant_dig`: an INDEPENDENT side-channel, set from the RAW attribute
//!   class (27-62 => TRUE), never derived from the returned attribute.
//!
//! Key architecture (source-proven):
//! - `pos.y` is discarded; this is an XZ cell query.
//! - Raw 10 (HOLE) -> FLOOR (non-FG) / GRASS2 (FG); cant_dig stays FALSE.
//! - Raw 63 (slope) -> recurse into the cardinal neighbor selected by the
//!   unit AREA; cant_dig is inherited from the neighbor (same pointer).
//! - Raw 25-26: dynamic wave evaluation, cant_dig stays FALSE.
//! - Raw 27-62: cant_dig = TRUE, then subfamilies:
//!   - 43-62 -> FLOOR / GRASS2 (FG)
//!   - 27-31 -> `mCoBG_woodb_water_info[attr-27][area]` (WOOD/RIVER_*)
//!   - 32-35 -> STONE
//!   - 36-38 -> dynamic wave evaluation
//!   - 39-42 -> `mCoBG_grass3_water_info[attr-39][area]`, with the
//!     field-type gate `(mapped <= GRASS3 && field != FG) ? FLOOR : mapped`
//! - Default: `(attr <= GRASS3 && field != FG) ? FLOOR : attr`.
//!
//! Because the Rust side has no collision-map access, the attr-63 neighbor
//! redirect is expressed as `redirect: Some(area)`; the caller (which owns
//! the map) resolves the neighbor's raw attribute and re-runs, threading
//! `cant_dig` through exactly like the source's shared pointer.

/// Raw attribute numbers used by this interpreter (from `m_collision_bg.h`).
pub mod attr {
    pub const GRASS2: u8 = 2;
    pub const GRASS3: u8 = 3;
    pub const STONE: u8 = 7;
    pub const FLOOR: u8 = 8;
    pub const HOLE: u8 = 10;
    pub const WAVE: u8 = 11;
    pub const WATER: u8 = 12;
    pub const RIVER_NE: u8 = 21;
    pub const SAND: u8 = 22;
    pub const WOOD: u8 = 23;
    pub const SEA: u8 = 24;
    pub const SLOPE: u8 = 63;
}

/// Unit-area order for the `[idx][area]` tables: N=0, W=1, S=2, E=3
/// (matches `mCoBG_AREA_*` and the existing `get_unit_area`).
pub mod area {
    pub const N: u8 = 0;
    pub const W: u8 = 1;
    pub const S: u8 = 2;
    pub const E: u8 = 3;
}

/// `mCoBG_woodb_water_info[6][4]` verbatim. Only rows 0-4 (attrs 27-31)
/// are ever indexed; row 5 is dead in the source and preserved as-is.
pub const WOODB_WATER_INFO: [[u8; 4]; 6] = [
    [15, 15, 23, 23], // 27: RIVER_NW, RIVER_NW, WOOD, WOOD
    [23, 17, 17, 23], // 28: WOOD, RIVER_SW, RIVER_SW, WOOD
    [23, 23, 19, 19], // 29: WOOD, WOOD, RIVER_SE, RIVER_SE
    [21, 23, 23, 21], // 30: RIVER_NE, WOOD, WOOD, RIVER_NE
    [23, 23, 23, 23], // 31: WOOD x4 (bridge center)
    [23, 23, 23, 23], // (spare row: never indexed by attrs 27-31)
];

/// `mCoBG_grass3_water_info[4][4]` verbatim (attrs 39-42).
pub const GRASS3_WATER_INFO: [[u8; 4]; 4] = [
    [15, 15, 2, 2], // 39: RIVER_NW, RIVER_NW, GRASS2, GRASS2
    [2, 17, 17, 2], // 40: GRASS2, RIVER_SW, RIVER_SW, GRASS2
    [2, 2, 19, 19], // 41: GRASS2, GRASS2, RIVER_SE, RIVER_SE
    [21, 2, 2, 21], // 42: RIVER_NE, GRASS2, GRASS2, RIVER_NE
];

/// `F32_IS_ZERO(v)`: `fabsf(v) < 0.008` (types.h:146).
#[inline]
pub fn f32_is_zero(v: f32) -> bool {
    v.abs() < 0.008
}

/// Projection of `target` onto the infinite line through `p0`→`p1`
/// (`mCoBG_GetCrossLineAndPerpendicular`, verbatim math).
/// Returns `None` when the segment is degenerate (source: cross = 0, FALSE).
pub fn cross_line_and_perpendicular(
    p0: (f32, f32),
    p1: (f32, f32),
    target: (f32, f32),
) -> Option<(f32, f32)> {
    let vx = p1.0 - p0.0;
    let vy = p1.1 - p0.1;
    let len = vx * vx + vy * vy;
    if len != 0.0 {
        let t = (-vx * (p0.0 - target.0) + -vy * (p0.1 - target.1)) / len;
        Some((p0.0 + t * vx, p0.1 + t * vy))
    } else {
        None
    }
}

/// `mCoBG_CheckWaveAtrDetail` verbatim: classify a unit-local point
/// against a wave boundary segment, given the current wave phase
/// (`wave_cos` = `mCoBG_WaveCos()`). Returns SEA / WAVE / SAND.
pub fn check_wave_atr_detail(
    point: (f32, f32),
    low: (f32, f32),
    high: (f32, f32),
    wave_cos: f32,
) -> u8 {
    let rate = (1.0 + wave_cos) * 0.5;
    if let Some(cross) = cross_line_and_perpendicular(high, low, point) {
        let dx = high.0 - low.0;
        let dz = high.1 - low.1;
        let dist = (dx * dx + dz * dz).sqrt();
        let dpx = cross.0 - low.0;
        let dpz = cross.1 - low.1;
        let dist_point = (dpx * dpx + dpz * dpz).sqrt();
        let pos_rate = if !f32_is_zero(dist) {
            1.1 * (dist_point / dist)
        } else {
            -1.0
        };
        if pos_rate <= 0.0 {
            return attr::SEA;
        }
        if pos_rate >= 1.1 {
            return attr::SAND;
        }
        if pos_rate <= rate {
            return attr::WAVE;
        }
        return attr::SAND;
    }
    attr::SEA
}

/// Wave-template boundary segments `(low, high)` in unit-local coords,
/// half unit = 20.0 (`mCoBG_GetWaveDynamicAttr` verbatim).
fn wave_segment(orig_attr: u8) -> Option<((f32, f32), (f32, f32))> {
    const H: f32 = 20.0; // mFI_UT_WORLDSIZE_HALF_X/Z_F
    match orig_attr {
        36 => Some(((0.0, H), (0.0, -H))),     // wave_s
        37 => Some(((0.0, 0.0), (-H, -H))),    // wave_se
        38 => Some(((0.0, 0.0), (H, H))),      // wave_sw
        25 => Some(((H, H), (0.0, 0.0))),      // wave_se2
        26 => Some(((-H, H), (0.0, 0.0))),     // wave_sw2
        _ => None,
    }
}

/// `mCoBG_GetWaveDynamicAttr` verbatim (minus the `pos` plumbing):
/// evaluate a wave-template raw attribute at a unit-local point.
pub fn get_wave_dynamic_attr(orig_attr: u8, local: (f32, f32), wave_cos: f32) -> u8 {
    match wave_segment(orig_attr) {
        Some((low, high)) => check_wave_atr_detail(local, low, high, wave_cos),
        None => attr::SEA,
    }
}

/// Inputs to one `mCoBG_Wpos2Attribute` step. The caller supplies the
/// raw attribute and the position-derived values the source computes
/// internally (`Wpos2UnitInfo`, `GetUnitArea`, `Pos2UnitPos`,
/// `Common_Get(field_type)`).
pub struct Wpos2AttributeInputs {
    /// Raw 6-bit `unit_attribute` of the containing unit.
    pub raw_attr: u8,
    /// Unit area of the query point: 0=N, 1=W, 2=S, 3=E.
    pub area: u8,
    /// Unit-local (x, z) of the query point (for wave evaluation).
    pub local_xz: (f32, f32),
    /// `mCoBG_Pos2UnitPos` success; FALSE -> wave attrs return SAND.
    pub local_valid: bool,
    /// `Common_Get(field_type) == mFI_FIELDTYPE2_FG`.
    pub is_fg_field: bool,
    /// `mCoBG_WaveCos()`: the current global wave phase.
    pub wave_cos: f32,
}

/// Output of one step. For raw attr 63, `redirect` carries the area
/// whose cardinal neighbor must be queried; the caller re-runs with
/// the neighbor's raw attribute and uses THAT step's `cant_dig`
/// (exactly the source's shared-pointer threading).
pub struct Wpos2AttributeOut {
    pub attr: u8,
    pub cant_dig: bool,
    pub redirect: Option<u8>,
}

/// One `mCoBG_Wpos2Attribute()` evaluation step, verbatim branch order.
pub fn wpos2attribute_step(inp: &Wpos2AttributeInputs) -> Wpos2AttributeOut {
    let raw = inp.raw_attr;
    let out = |attr: u8, cant_dig: bool| Wpos2AttributeOut {
        attr,
        cant_dig,
        redirect: None,
    };

    // HOLE -> FLOOR / GRASS2; cant_dig stays FALSE.
    if raw == attr::HOLE {
        return out(
            if inp.is_fg_field {
                attr::GRASS2
            } else {
                attr::FLOOR
            },
            false,
        );
    }

    // Slope: redirect to the area's cardinal neighbor (recursive).
    if raw == attr::SLOPE {
        return Wpos2AttributeOut {
            attr: 0,
            cant_dig: false,
            redirect: Some(inp.area),
        };
    }

    // Wave templates 25-26: dynamic, cant_dig stays FALSE.
    if (25..=26).contains(&raw) {
        let a = if inp.local_valid {
            get_wave_dynamic_attr(raw, inp.local_xz, inp.wave_cos)
        } else {
            attr::SAND
        };
        return out(a, false);
    }

    // The big special-terrain family: cant_dig = TRUE.
    if (27..=62).contains(&raw) {
        // 43-62 -> FLOOR / GRASS2.
        if (43..=62).contains(&raw) {
            return out(
                if inp.is_fg_field {
                    attr::GRASS2
                } else {
                    attr::FLOOR
                },
                true,
            );
        }
        // 27-31: wood bridges, area-dependent.
        if (27..=31).contains(&raw) {
            let idx = (raw - 27) as usize;
            let a = WOODB_WATER_INFO[idx][(inp.area & 3) as usize];
            return out(a, true);
        }
        // 32-35: stone bridges -> STONE.
        if (32..=35).contains(&raw) {
            return out(attr::STONE, true);
        }
        // 36-38: dynamic waves (undiggable).
        if (36..=38).contains(&raw) {
            let a = if inp.local_valid {
                get_wave_dynamic_attr(raw, inp.local_xz, inp.wave_cos)
            } else {
                attr::SAND
            };
            return out(a, true);
        }
        // 39-42: river banks, area-dependent with field-type gate.
        if (39..=42).contains(&raw) {
            let idx = (raw - 39) as usize;
            let mapped = GRASS3_WATER_INFO[idx][(inp.area & 3) as usize];
            let a = if (mapped <= attr::GRASS3 && !inp.is_fg_field) == false {
                mapped
            } else {
                attr::FLOOR
            };
            return out(a, true);
        }
    }

    // Default: (attr <= GRASS3 && non-FG) ? FLOOR : attr.
    let a = if (raw <= attr::GRASS3 && !inp.is_fg_field) == false {
        raw
    } else {
        attr::FLOOR
    };
    out(a, false)
}

// ---- Attribute-63 neighbor resolution (`mCoBG_SearchAttribute`) ----

/// `mCoBG_unit_offset[8]` verbatim (m_collision_bg.c:103): (x, z) unit
/// offsets for the 8 direct values. Unit size = 40 (`mFI_UNIT_BASE_SIZE`).
/// Order: N, W, S, E, NW, NE, SE, SW. NOTE the source quirk: with north =
/// -z, the table's index-5 ("NE") entry points to (-40, +40) and index-7
/// ("SW") to (+40, -40) — the two are transposed vs geometric intuition.
/// Preserved verbatim; behavior, not names, is what matters.
pub const UNIT_OFFSETS: [(f32, f32); 8] = [
    (0.0, -40.0),   // N
    (-40.0, 0.0),   // W
    (0.0, 40.0),    // S
    (40.0, 0.0),    // E
    (-40.0, -40.0), // NW
    (-40.0, 40.0),  // "NE" (source table; geometrically SW)
    (40.0, 40.0),   // SE
    (40.0, -40.0),  // "SW" (source table; geometrically NE)
];

/// `mCoBG_PlussDirectOffset` verbatim (XZ part): adds the unit offset
/// for `direct`. Returns `None` (no write, per the source guard) when
/// `direct` is out of range.
pub fn pluss_direct_offset(x: f32, z: f32, direct: u8) -> Option<(f32, f32)> {
    UNIT_OFFSETS
        .get(direct as usize)
        .map(|(ox, oz)| (x + ox, z + oz))
}

/// `mCoBG_SearchAttribute` core: resolve the attr-63 redirect.
/// `wpos.y` is forced to 0 (source does this redundantly — Wpos2Attribute
/// does it again), then the position moves exactly one unit in the
/// cardinal `area` direction (0=N..3=E). Local position within the unit
/// is PRESERVED (no snapping to the neighbor center); only the four
/// cardinal areas are valid redirect sources — never diagonal.
/// Returns the neighbor query position, or `None` for an invalid area.
pub fn search_attribute_redirect(x: f32, z: f32, area: u8) -> Option<(f32, f32)> {
    if area > 3 {
        return None;
    }
    pluss_direct_offset(x, z, area)
}

// ---- Slate ground height (`mCoBG_GetBGHeight_Normal_SlateGround`) ----

/// Slate orientation from corner samples, verbatim:
/// `top_left != bot_right` -> SLATE_UP, else SLATE_DOWN.
/// (This is a different, single-comparison test from the wall-building
/// slate-detail search.)
pub fn slate_ground_orientation(top_left: u32, bot_right: u32) -> u8 {
    if top_left != bot_right {
        0 // WALL_SLATE_UP
    } else {
        1 // WALL_SLATE_DOWN
    }
}

/// Area -> corner-sample mapping of `mCoBG_GetAreaYSlatingUnit`, verbatim,
/// including the fallthrough: an invalid area under SLATE_UP falls into
/// the SLATE_DOWN area switch (source `// fallthrough`).
/// Returns the raw corner value (caller multiplies by 10 and adds base height).
pub fn area_y_slating_unit(
    top_left: u32,
    bot_left: u32,
    bot_right: u32,
    top_right: u32,
    slate_up: bool,
    area: u8,
) -> u32 {
    let up = |a: u8| match a {
        2 | 3 => bot_right, // AREA_S | AREA_E
        0 | 1 => top_left,  // AREA_N | AREA_W
        _ => 0,             // invalid -> fallthrough to DOWN switch
    };
    let down = |a: u8| match a {
        0 | 3 => top_right, // AREA_N | AREA_E
        1 | 2 => bot_left,  // AREA_W | AREA_S
        _ => 0,
    };
    if slate_up {
        let v = up(area);
        if area > 3 {
            down(area) // the source fallthrough
        } else {
            v
        }
    } else {
        down(area)
    }
}

/// `mCoBG_GetBGHeight_Normal_SlateGround` verbatim (minus the angle
/// zeroing, which the source performs on the caller's `s_xyz*`):
/// orientation from `top_left != bot_right`, area from the caller,
/// `corner * 10 + base_height`.
pub fn slate_ground_height(
    top_left: u32,
    bot_left: u32,
    bot_right: u32,
    top_right: u32,
    area: u8,
    base_height: f32,
) -> f32 {
    let up = slate_ground_orientation(top_left, bot_right) == 0;
    area_y_slating_unit(top_left, bot_left, bot_right, top_right, up, area) as f32 * 10.0
        + base_height
}

// ---- C ABI ----

/// C ABI: `mCoBG_PlussDirectOffset` XZ part. Returns 1 and writes the
/// offset position via out_x/out_z; returns 0 for an invalid direction
/// (source writes nothing in that case).
#[no_mangle]
pub unsafe extern "C" fn pc_pluss_direct_offset(
    x: f32,
    z: f32,
    direct: u8,
    out_x: *mut f32,
    out_z: *mut f32,
) -> u8 {
    match pluss_direct_offset(x, z, direct) {
        Some((nx, nz)) => {
            if !out_x.is_null() {
                unsafe { *out_x = nx };
            }
            if !out_z.is_null() {
                unsafe { *out_z = nz };
            }
            1
        }
        None => 0,
    }
}

/// C ABI: attr-63 redirect target. Returns 1 and writes the neighbor
/// query position; 0 for an invalid area (> 3).
#[no_mangle]
pub unsafe extern "C" fn pc_search_attribute_redirect(
    x: f32,
    z: f32,
    area: u8,
    out_x: *mut f32,
    out_z: *mut f32,
) -> u8 {
    match search_attribute_redirect(x, z, area) {
        Some((nx, nz)) => {
            if !out_x.is_null() {
                unsafe { *out_x = nx };
            }
            if !out_z.is_null() {
                unsafe { *out_z = nz };
            }
            1
        }
        None => 0,
    }
}

/// C ABI: `mCoBG_GetBGHeight_Normal_SlateGround` (minus angle zeroing):
/// corner samples + area + base height -> ground Y.
#[no_mangle]
pub extern "C" fn pc_slate_ground_height(
    top_left: u32,
    bot_left: u32,
    bot_right: u32,
    top_right: u32,
    area: u8,
    base_height: f32,
) -> f32 {
    slate_ground_height(top_left, bot_left, bot_right, top_right, area, base_height)
}

/// C ABI: one Wpos2Attribute step.

/// C ABI: one Wpos2Attribute step.
/// Returns the effective attribute in the low byte and cant_dig in bit 8;
/// returns 0x1_00 | area in bits 8..15 when a slope redirect is needed
/// (0xFF in the low byte marks the redirect).
#[no_mangle]
pub extern "C" fn pc_wpos2attribute_step(
    raw_attr: u8,
    area: u8,
    local_x: f32,
    local_z: f32,
    local_valid: u8,
    is_fg_field: u8,
    wave_cos: f32,
) -> u32 {
    let inp = Wpos2AttributeInputs {
        raw_attr,
        area,
        local_xz: (local_x, local_z),
        local_valid: local_valid != 0,
        is_fg_field: is_fg_field != 0,
        wave_cos,
    };
    let o = wpos2attribute_step(&inp);
    match o.redirect {
        Some(a) => 0xFF | ((a as u32) << 8) | (1u32 << 16),
        None => (o.attr as u32) | ((o.cant_dig as u32) << 8),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inp(raw: u8) -> Wpos2AttributeInputs {
        Wpos2AttributeInputs {
            raw_attr: raw,
            area: area::N,
            local_xz: (0.0, 0.0),
            local_valid: true,
            is_fg_field: true,
            wave_cos: 0.0,
        }
    }

    #[test]
    fn step_branches() {
        // Hole.
        let mut i = inp(10);
        assert_eq!(wpos2attribute_step(&i).attr, attr::GRASS2);
        i.is_fg_field = false;
        let o = wpos2attribute_step(&i);
        assert_eq!(o.attr, attr::FLOOR);
        assert!(!o.cant_dig);
        // Slope redirect.
        let o = wpos2attribute_step(&inp(63));
        assert_eq!(o.redirect, Some(area::N));
        assert!(!o.cant_dig);
        // Ordinary attrs pass through on FG.
        let o = wpos2attribute_step(&inp(0));
        assert_eq!((o.attr, o.cant_dig), (0, false));
        let o = wpos2attribute_step(&inp(12));
        assert_eq!((o.attr, o.cant_dig), (12, false));
        // Ordinary attrs collapse to FLOOR on non-FG when <= GRASS3.
        let mut i = inp(2);
        i.is_fg_field = false;
        assert_eq!(wpos2attribute_step(&i).attr, attr::FLOOR);
        let mut i = inp(12);
        i.is_fg_field = false;
        assert_eq!(wpos2attribute_step(&i).attr, 12);
        // 27-31: wood bridge area table.
        let mut i = inp(27);
        i.area = area::N;
        let o = wpos2attribute_step(&i);
        assert_eq!((o.attr, o.cant_dig), (15, true)); // RIVER_NW
        i.area = area::S;
        assert_eq!(wpos2attribute_step(&i).attr, attr::WOOD);
        // 31 (bridge center) is all wood.
        let mut i = inp(31);
        i.area = area::E;
        assert_eq!(wpos2attribute_step(&i).attr, attr::WOOD);
        // 32-35 -> STONE, undiggable.
        let o = wpos2attribute_step(&inp(33));
        assert_eq!((o.attr, o.cant_dig), (attr::STONE, true));
        // 43-62 -> GRASS2 on FG, FLOOR off FG, always cant_dig.
        let o = wpos2attribute_step(&inp(47));
        assert_eq!((o.attr, o.cant_dig), (attr::GRASS2, true));
        let mut i = inp(47);
        i.is_fg_field = false;
        let o = wpos2attribute_step(&i);
        assert_eq!((o.attr, o.cant_dig), (attr::FLOOR, true));
        // 39-42: river bank gate. GRASS2 entries collapse on non-FG.
        let mut i = inp(39);
        i.area = area::N; // RIVER_NW > GRASS3 -> passes even off FG
        i.is_fg_field = false;
        assert_eq!(wpos2attribute_step(&i).attr, 15);
        i.area = area::S; // GRASS2 <= GRASS3, non-FG -> FLOOR
        assert_eq!(wpos2attribute_step(&i).attr, attr::FLOOR);
        i.is_fg_field = true;
        assert_eq!(wpos2attribute_step(&i).attr, attr::GRASS2);
        // Wave templates: invalid local pos -> SAND.
        let mut i = inp(25);
        i.local_valid = false;
        let o = wpos2attribute_step(&i);
        assert_eq!((o.attr, o.cant_dig), (attr::SAND, false));
        let mut i = inp(36);
        i.local_valid = false;
        let o = wpos2attribute_step(&i);
        assert_eq!((o.attr, o.cant_dig), (attr::SAND, true));
    }

    #[test]
    fn wave_classifier_math() {
        // Attr 36 (wave_s): segment low=(0,+20), high=(0,-20).
        // Point projecting exactly onto low -> pos_rate 0 -> SEA.
        assert_eq!(get_wave_dynamic_attr(36, (7.3, 20.0), 0.0), attr::SEA);
        // Point at the sand end -> SAND.
        assert_eq!(get_wave_dynamic_attr(36, (0.0, -25.0), 0.0), attr::SAND);
        // Midpoint with wave_cos=0 (rate=0.5): pos_rate=0.55 > rate -> SAND.
        assert_eq!(get_wave_dynamic_attr(36, (0.0, 0.0), 0.0), attr::SAND);
        // Same midpoint with wave_cos=1 (rate=1.0): pos_rate=0.55 <= 1 -> WAVE.
        assert_eq!(get_wave_dynamic_attr(36, (0.0, 0.0), 1.0), attr::WAVE);
        // Unknown attr -> SEA.
        assert_eq!(get_wave_dynamic_attr(99, (0.0, 0.0), 0.0), attr::SEA);
        // Degenerate projection -> SEA.
        assert_eq!(check_wave_atr_detail((1.0, 1.0), (0.0, 0.0), (0.0, 0.0), 0.0), attr::SEA);
        // C ABI: redirect marker.
        assert_eq!(pc_wpos2attribute_step(63, 2, 0.0, 0.0, 1, 1, 0.0) & 0xFF, 0xFF);
        assert_eq!((pc_wpos2attribute_step(63, 2, 0.0, 0.0, 1, 1, 0.0) >> 8) & 0xFF, 2);
        // C ABI: attr + cant_dig bit.
        assert_eq!(pc_wpos2attribute_step(33, 0, 0.0, 0.0, 1, 1, 0.0), 7 | (1 << 8));
    }

    #[test]
    fn search_attribute_and_slate_ground() {
        // PlussDirectOffset: cardinal + verbatim diagonal quirk.
        assert_eq!(pluss_direct_offset(100.0, 100.0, 0), Some((100.0, 60.0))); // N
        assert_eq!(pluss_direct_offset(100.0, 100.0, 1), Some((60.0, 100.0))); // W
        assert_eq!(pluss_direct_offset(100.0, 100.0, 2), Some((100.0, 140.0))); // S
        assert_eq!(pluss_direct_offset(100.0, 100.0, 3), Some((140.0, 100.0))); // E
        assert_eq!(pluss_direct_offset(100.0, 100.0, 5), Some((60.0, 140.0))); // "NE" per source table
        assert_eq!(pluss_direct_offset(100.0, 100.0, 8), None); // out of range: no write
        // SearchAttribute redirect: cardinal only, local offset preserved.
        assert_eq!(search_attribute_redirect(12.0, 5.0, area::W), Some((-28.0, 5.0)));
        assert_eq!(search_attribute_redirect(12.0, 5.0, area::N), Some((12.0, -35.0)));
        assert_eq!(search_attribute_redirect(12.0, 5.0, 4), None);
        // SlateGround: top_left != bot_right -> SLATE_UP.
        // UP: S/E -> bot_right, N/W -> top_left.
        let h = slate_ground_height(10, 11, 20, 12, area::S, 100.0);
        assert_eq!(h, 20.0 * 10.0 + 100.0);
        let h = slate_ground_height(10, 11, 20, 12, area::N, 100.0);
        assert_eq!(h, 10.0 * 10.0 + 100.0);
        // top_left == bot_right -> SLATE_DOWN: N/E -> top_right, W/S -> bot_left.
        let h = slate_ground_height(10, 11, 10, 12, area::E, 50.0);
        assert_eq!(h, 12.0 * 10.0 + 50.0);
        let h = slate_ground_height(10, 11, 10, 12, area::W, 50.0);
        assert_eq!(h, 11.0 * 10.0 + 50.0);
        // Invalid area under SLATE_UP falls through to the DOWN switch.
        assert_eq!(area_y_slating_unit(10, 11, 20, 12, true, 9), 0);
        // C ABI.
        let (mut ox, mut oz) = (0.0f32, 0.0f32);
        assert_eq!(
            unsafe { pc_pluss_direct_offset(100.0, 100.0, 2, &mut ox, &mut oz) },
            1
        );
        assert_eq!((ox, oz), (100.0, 140.0));
        assert_eq!(
            unsafe { pc_pluss_direct_offset(100.0, 100.0, 9, &mut ox, &mut oz) },
            0
        );
        assert_eq!(
            unsafe { pc_search_attribute_redirect(12.0, 5.0, 1, &mut ox, &mut oz) },
            1
        );
        assert_eq!((ox, oz), (-28.0, 5.0));
        assert_eq!(pc_slate_ground_height(10, 11, 20, 12, area::S, 100.0), 300.0);
    }
}
