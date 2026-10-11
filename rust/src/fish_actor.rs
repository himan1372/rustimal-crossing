//! Fish actors: the swimming-fish controller
//! (`ac_gyoei.c` + `_move`/`_draw`/`_clip`/`_type`/`_model`),
//! the water-shadow/disappearance visual
//! (`ac_gyo_kage.c`), and the release actor
//! (`ac_gyo_release.c`), with the two behavior
//! programs (`ac_gyo_test.c`, `ac_gyo_kaseki.c`).
//!
//! Verified against GAFE01_00 Rev 0 USA (see
//! `fish-verification-report.md`; 11/11 sections
//! PROVEN or DIFFERS-with-correction).
//!
//! Architecture: swimming fish are *embedded
//! controller slots* (two `aGYO_CTRL_ACTOR` slots in
//! the `GYOEI_ACTOR`), not standalone actors per
//! fish. The kage actor is a separate short-lived
//! water-surface visual (not the ordinary ground
//! shadow). The release actor is a standalone actor
//! created on catch, linked to the float through
//! `UKI_ACTOR.child_actor`.
//!
//! This module ports the pure kernels: the size,
//! search, and bite tables, the shadow-scale table,
//! the behavior dispatch, the release velocity /
//! gravity / revert / shrink / shadow math, the
//! kage alpha and ripple selection, the flee
//! predicate, and the search/bite rod tables. Slot
//! and actor allocation, clip callbacks, catch
//! registration, float linkage, collision/background
//! APIs, rendering, effects/sound, and the full
//! test/kaseki state machines stay in C.
//!
//! Brief corrections carried here: `sizeof` values
//! are GYOEI_ACTOR 0x634, aGYO_CTRL_ACTOR 0x254,
//! GYO_RELEASE_ACTOR 0x1C0, `exist[4]` at 0x61C,
//! `segment_type[4]` at 0x620; the shadow-scale
//! table is confirmed in kage/release/test/kaseki
//! but NOT in `ac_gyoei.c` itself; the regular and
//! gold rod search-area tables are identical;
//! the kage `draw_frame` wraps modularly and the
//! ripple gate is a one-shot, not an equality;
//! release shrink uses `powf(rate, dt)`;
//! `aGYR_empty_area` returns 0 when no segment is
//! free; gyo_flags bit 0x10 is kaseki-only
//! (`uki->gyo_status == 7`); the whale gets no flee
//! ripple; the 6-entry shadow-correction arrays have
//! a verbatim `@BUG` comment — do NOT extend them.

/// Fish type count (40 normal + extended values).
pub const FISH_TYPE_NUM: i32 = 45;
/// Item id base.
pub const ITM_FISH_START: i32 = 0x2300;
/// Special types (index = enum order).
pub const T_SALMON: i32 = 22;
pub const T_COELACANTH: i32 = 31;
pub const T_JELLYFISH: i32 = 35;
pub const T_SEA_BASS: i32 = 36;
pub const T_RED_SNAPPER: i32 = 37;
pub const T_BARRED_KNIFEJAW: i32 = 38;
pub const T_WHALE: i32 = 40;
pub const T_SALMON2: i32 = 44;

/// Size classes.
pub const S_XXS: i32 = 0;
pub const S_XS: i32 = 1;
pub const S_S: i32 = 2;
pub const S_M: i32 = 3;
pub const S_L: i32 = 4;
pub const S_XL: i32 = 5;
pub const S_XXL: i32 = 6;
pub const S_WHALE: i32 = 7;
pub const SIZE_NUM: i32 = 8;

/// Behavior programs.
pub const B_TEST: i32 = 0;
pub const B_KASEKI: i32 = 1;

/// Shadow-scale table (confirmed in kage, release,
/// test, and kaseki; not in `ac_gyoei.c` itself).
pub const SHADOW_SCALE: [f32; 8] = [0.3, 0.4, 0.5, 0.5, 0.6, 0.8, 1.2, 10.0];

/// Size class per fish type (`gyoei_type[]` in
/// `ac_gyoei_type.c_inc`), index 0–44.
pub const FISH_SIZE: [i32; 45] = [
    1, 2, 4, 4, 4, 1, 3, 4, 1, 5, // 0–9
    5, 4, 3, 1, 0, 0, 0, 1, 1, 4, // 10–19
    3, 5, 4, 0, 1, 3, 2, 1, 1, 0, // 20–29
    0, 5, 1, 0, 0, 3, 4, 4, 4, 6, // 30–39
    7, 0, 3, 4, 4, // 40–44
];

/// Search-area class per fish type.
pub const FISH_SEARCH: [i32; 45] = [
    3, 3, 2, 2, 3, 3, 2, 1, 4, 2, // 0–9
    2, 2, 3, 3, 2, 3, 3, 2, 1, 1, // 10–19
    2, 1, 2, 2, 4, 3, 2, 2, 2, 2, // 20–29
    2, 4, 4, 2, 2, 3, 2, 2, 1, 1, // 30–39
    1, 2, 2, 2, 2, // 40–44
];

/// Bite-time class per fish type.
pub const FISH_BITE: [i32; 45] = [
    4, 2, 3, 3, 4, 3, 2, 1, 4, 4, // 0–9
    1, 3, 3, 3, 1, 3, 3, 1, 1, 1, // 10–19
    2, 1, 1, 3, 3, 2, 1, 4, 2, 3, // 20–29
    3, 0, 4, 3, 2, 4, 2, 1, 0, 1, // 30–39
    1, 3, 4, 4, 1, // 40–44
];

/// Rod-indexed search areas (regular and gold rods
/// are identical — verified).
pub const SEARCH_AREA: [[f32; 5]; 2] = [
    [40.0, 40.0, 40.0, 50.0, 60.0],
    [40.0, 40.0, 40.0, 50.0, 60.0],
];
/// Rod-indexed bite times.
pub const BITE_TIME: [[f32; 5]; 2] = [
    [10.0, 11.0, 12.0, 15.0, 45.0],
    [11.0, 12.0, 13.0, 18.0, 60.0],
];

/// Kage scale factor: `shadow_scale[size] * 0.02f`.
pub const KAGE_SCALE_MUL: f32 = 0.02;
/// Kage delete timer.
pub const KAGE_DELETE_TIMER: i32 = 100;
/// Kage draw frame.
pub const KAGE_DRAW_FRAME: i32 = 38;

/// Release shrink thresholds.
pub const RELEASE_SHRINK_AFTER: i32 = 140;
pub const RELEASE_DELETE_AFTER: i32 = 160;
pub const RELEASE_SHRINK_LIMIT: f32 = 0.006;

/// Shadow-size corrections (6 entries — the source
/// comments that XXL/whale entries were never added;
/// do NOT extend).
pub const HOSEI_X: [f32; 6] = [0.0, 0.0, 0.0, 0.0, 0.0, 1.5];
pub const HOSEI_Z: [f32; 6] = [3.0, 2.5, 2.5, 2.5, 2.5, 7.0];
/// Eel corrections.
pub const EEL_CORRECTION_X: f32 = 9.0;
pub const EEL_CORRECTION_Z: f32 = 3.5;

/// Size class of a fish type.
#[no_mangle]
pub extern "C" fn pc_fish_size_class(fish_type: i32) -> i32 {
    if fish_type < 0 || fish_type >= FISH_TYPE_NUM {
        -1
    } else {
        FISH_SIZE[fish_type as usize]
    }
}

/// Search-area class of a fish type.
#[no_mangle]
pub extern "C" fn pc_fish_search_class(fish_type: i32) -> i32 {
    if fish_type < 0 || fish_type >= FISH_TYPE_NUM {
        -1
    } else {
        FISH_SEARCH[fish_type as usize]
    }
}

/// Bite-time class of a fish type.
#[no_mangle]
pub extern "C" fn pc_fish_bite_class(fish_type: i32) -> i32 {
    if fish_type < 0 || fish_type >= FISH_TYPE_NUM {
        -1
    } else {
        FISH_BITE[fish_type as usize]
    }
}

/// Shadow scale for a size class.
#[no_mangle]
pub extern "C" fn pc_fish_shadow_scale(size_type: i32) -> f32 {
    if size_type < 0 || size_type >= SIZE_NUM {
        0.0
    } else {
        SHADOW_SCALE[size_type as usize]
    }
}

/// Behavior program: SALMON2 is normalized to SALMON
/// first; the kaseki program is selected for
/// coelacanth, jellyfish, sea bass, red snapper,
/// barred knifejaw, and whale; everything else uses
/// the test program.
#[no_mangle]
pub extern "C" fn pc_fish_behavior_program(fish_type: i32) -> i32 {
    let fish_type = if fish_type == T_SALMON2 {
        T_SALMON
    } else {
        fish_type
    };
    match fish_type {
        T_COELACANTH | T_JELLYFISH | T_SEA_BASS | T_RED_SNAPPER | T_BARRED_KNIFEJAW | T_WHALE => {
            B_KASEKI
        }
        _ => B_TEST,
    }
}

/// Release initial velocity toward the water target.
/// When `dist > unit`: `speed_xz = (dist/60)/0.5`,
/// `speed_y = max(0, (dy+450)/60)`; otherwise
/// `speed_xz = 2.5`, `speed_y = ABS(dist)*0.18`.
/// Writes into `out[2]` = (speed_xz, speed_y).
#[no_mangle]
pub extern "C" fn pc_fish_release_velocity(dist: f32, dy: f32, unit: f32, out: *mut f32) {
    let (speed_xz, speed_y) = if dist > unit {
        ((dist / 60.0) / 0.5, ((dy + 450.0) / 60.0).max(0.0))
    } else {
        (2.5, dist.abs() * 0.18)
    };
    if !out.is_null() {
        unsafe {
            *out = speed_xz;
            *out.add(1) = speed_y;
        }
    }
}

/// Gravity step: subtract 0.25 per update, scaled by
/// dt (`vy − 0.25*dt`).
#[no_mangle]
pub extern "C" fn pc_fish_release_gravity(vy: f32, dt: f32) -> f32 {
    vy - 0.25 * dt
}

/// Water-contact revert state update. On first water
/// entry the flag is set and the timer armed to 3.0;
/// the timer decrements each update. The caller
/// restores X/Z when the return is true:
/// flag set && timer expired && no longer in water.
#[no_mangle]
pub extern "C" fn pc_fish_release_revert(
    in_water: bool,
    flag: bool,
    timer: f32,
    dt: f32,
    out: *mut f32,
) -> bool {
    let (flag, timer) = if in_water && !flag {
        (true, 3.0)
    } else {
        (flag, (timer - dt).max(0.0))
    };
    let revert = flag && timer <= 0.0 && !in_water;
    if !out.is_null() {
        unsafe {
            *out = if flag { 1.0 } else { 0.0 };
            *out.add(1) = timer;
        }
    }
    revert
}

/// Release shrink math. After `exist_timer > 140`:
/// rate = (scale > 0.006 ? 0.89 : 0.98)^dt; scale
/// and shadow_scale are multiplied by it. After
/// `exist_timer > 160` the actor is deleted. Writes
/// new (scale, shadow_scale) into `out[2]`; returns
/// true when the actor should be deleted.
#[no_mangle]
pub extern "C" fn pc_fish_release_shrink(
    scale: f32,
    shadow_scale: f32,
    exist_timer: i32,
    dt: f32,
    out: *mut f32,
) -> bool {
    let rate = if scale > RELEASE_SHRINK_LIMIT {
        0.89f32
    } else {
        0.98f32
    };
    let (scale, shadow_scale) = if exist_timer > RELEASE_SHRINK_AFTER {
        let r = rate.powf(dt);
        (scale * r, shadow_scale * r)
    } else {
        (scale, shadow_scale)
    };
    if !out.is_null() {
        unsafe {
            *out = scale;
            *out.add(1) = shadow_scale;
        }
    }
    exist_timer > RELEASE_DELETE_AFTER
}

/// Release shadow dimensions:
/// `ofs_y = height < 0 ? 0 : 10 + (50 − height)*0.2`;
/// eel uses corrections {9.0, 3.5}, other size
/// classes use the 6-entry hoseix/hoseiz arrays
/// (indexes ≥ 6 fall back to the last entry in C;
/// do not extend the arrays).
/// Writes (shadow_size_x, shadow_size_z) into
/// `out[2]`.
#[no_mangle]
pub extern "C" fn pc_fish_release_shadow(
    size_type: i32,
    is_eel: bool,
    height: f32,
    shadow_scale: f32,
    out: *mut f32,
) {
    let ofs_y = if height < 0.0 {
        0.0
    } else {
        10.0 + (50.0 - height) * 0.2
    };
    let (cx, cz) = if is_eel {
        (EEL_CORRECTION_X, EEL_CORRECTION_Z)
    } else {
        let i = (size_type.clamp(0, 5)) as usize;
        (HOSEI_X[i], HOSEI_Z[i])
    };
    let s = pc_fish_shadow_scale(size_type);
    if !out.is_null() {
        unsafe {
            *out = s * ofs_y * shadow_scale + cx;
            *out.add(1) = s * ofs_y * shadow_scale - cz;
        }
    }
}

/// Kage alpha: `((int)(delete_timer*0.5) − 10)*6`,
/// clamped at zero.
#[no_mangle]
pub extern "C" fn pc_fish_kage_alpha(delete_timer: f32) -> i32 {
    ((((delete_timer * 0.5) as i32) - 10) * 6).max(0)
}

/// Kage ripple argument by size class: 0/1 → 2,
/// 3 → 1, 2/4/5 → 0, 6/7 → none (−1).
#[no_mangle]
pub extern "C" fn pc_fish_kage_ripple(size_class: i32) -> i32 {
    match size_class {
        0 | 1 => 2,
        3 => 1,
        2 | 4 | 5 => 0,
        _ => -1,
    }
}

/// Flee predicate: player dashing within 110.0,
/// tool action within 150.0, or the escape flag.
#[no_mangle]
pub extern "C" fn pc_fish_flee(dist: f32, dashing: bool, tool_hit: bool, escape_flag: bool) -> bool {
    (dist < 110.0 && dashing) || (dist < 150.0 && tool_hit) || escape_flag
}

/// Search check: within the rod's search area for
/// the search class and |dy| ≤ 10.
#[no_mangle]
pub extern "C" fn pc_fish_search_ok(dist: f32, rod: i32, search_class: i32, dy: f32) -> bool {
    let rod = rod.clamp(0, 1) as usize;
    let sc = search_class.clamp(0, 4) as usize;
    dist <= SEARCH_AREA[rod][sc] && dy.abs() <= 10.0
}

/// Bite time: `bite_time[rod][bite_class] * 2.0`.
#[no_mangle]
pub extern "C" fn pc_fish_bite_time(rod: i32, bite_class: i32) -> f32 {
    let rod = rod.clamp(0, 1) as usize;
    let bc = bite_class.clamp(0, 4) as usize;
    BITE_TIME[rod][bc] * 2.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tables() {
        // Representative brief entries.
        assert_eq!(pc_fish_size_class(0), S_XS); // crucian carp
        assert_eq!(pc_fish_size_class(2), S_L); // carp
        assert_eq!(pc_fish_size_class(8), S_XS); // bluegill
        assert_eq!(pc_fish_size_class(9), S_XL); // giant catfish
        assert_eq!(pc_fish_size_class(14), S_XXS); // bitterling
        assert_eq!(pc_fish_size_class(21), S_XL); // stringfish
        assert_eq!(pc_fish_size_class(31), S_XL); // coelacanth
        assert_eq!(pc_fish_size_class(39), S_XXL); // arapaima
        assert_eq!(pc_fish_size_class(40), S_WHALE); // whale
        assert_eq!(pc_fish_size_class(41), S_XXS); // empty can
        assert_eq!(pc_fish_size_class(44), S_L); // SALMON2
        assert_eq!(pc_fish_size_class(45), -1);
        assert_eq!(pc_fish_search_class(0), 3);
        assert_eq!(pc_fish_bite_class(0), 4);
        assert_eq!(pc_fish_bite_class(31), 0);
        assert!((pc_fish_shadow_scale(0) - 0.3).abs() < 1e-6);
        assert!((pc_fish_shadow_scale(7) - 10.0).abs() < 1e-6);
        assert_eq!(pc_fish_shadow_scale(8), 0.0);
        // Spot checks against the source table.
        assert_eq!(FISH_SIZE.len(), 45);
        assert_eq!(FISH_SEARCH.len(), 45);
        assert_eq!(FISH_BITE.len(), 45);
    }

    #[test]
    fn dispatch() {
        assert_eq!(pc_fish_behavior_program(31), B_KASEKI); // coelacanth
        assert_eq!(pc_fish_behavior_program(35), B_KASEKI); // jellyfish
        assert_eq!(pc_fish_behavior_program(36), B_KASEKI); // sea bass
        assert_eq!(pc_fish_behavior_program(37), B_KASEKI); // red snapper
        assert_eq!(pc_fish_behavior_program(38), B_KASEKI); // barred knifejaw
        assert_eq!(pc_fish_behavior_program(40), B_KASEKI); // whale
        assert_eq!(pc_fish_behavior_program(44), B_KASEKI); // SALMON2→SALMON fallthrough
        assert_eq!(pc_fish_behavior_program(0), B_TEST);
        assert_eq!(pc_fish_behavior_program(22), B_TEST); // salmon
        assert_eq!(pc_fish_behavior_program(39), B_TEST); // arapaima
    }

    #[test]
    fn release_math() {
        let mut out = [0.0f32; 2];
        pc_fish_release_velocity(120.0, 100.0, 40.0, out.as_mut_ptr());
        assert!((out[0] - (120.0 / 60.0) / 0.5).abs() < 1e-5);
        assert!((out[1] - (100.0 + 450.0) / 60.0).abs() < 1e-5);
        pc_fish_release_velocity(120.0, -500.0, 40.0, out.as_mut_ptr());
        assert_eq!(out[1], 0.0); // clamped
        pc_fish_release_velocity(30.0, 0.0, 40.0, out.as_mut_ptr());
        assert_eq!(out[0], 2.5);
        assert!((out[1] - 30.0 * 0.18).abs() < 1e-5);
        assert!((pc_fish_release_gravity(3.0, 1.0) - 2.75).abs() < 1e-5);
        // Revert rule.
        let mut st = [0.0f32; 2];
        assert!(!pc_fish_release_revert(true, false, 0.0, 1.0, st.as_mut_ptr())); // entry arms
        assert_eq!(st[0], 1.0);
        assert_eq!(st[1], 3.0);
        assert!(!pc_fish_release_revert(false, true, 3.0, 1.0, st.as_mut_ptr())); // timer running
        assert_eq!(st[1], 2.0);
        assert!(pc_fish_release_revert(false, true, 0.5, 1.0, st.as_mut_ptr())); // expired → revert
        // Shrink math.
        let mut sh = [0.0f32; 2];
        assert!(!pc_fish_release_shrink(0.01, 1.0, 141, 1.0, sh.as_mut_ptr()));
        assert!((sh[0] - 0.01 * 0.89).abs() < 1e-6);
        assert!(!pc_fish_release_shrink(0.006, 1.0, 141, 1.0, sh.as_mut_ptr()));
        assert!((sh[0] - 0.006 * 0.98).abs() < 1e-6);
        assert!(pc_fish_release_shrink(0.01, 1.0, 161, 1.0, sh.as_mut_ptr()));
        assert!(!pc_fish_release_shrink(0.01, 1.0, 140, 1.0, sh.as_mut_ptr()));
        assert_eq!(sh[0], 0.01); // no shrink before 141
        assert_eq!(sh[1], 1.0);
        // Shadow dimensions.
        let mut sd = [0.0f32; 2];
        pc_fish_release_shadow(4, false, 30.0, 1.0, sd.as_mut_ptr());
        let ofs = 10.0 + (50.0 - 30.0) * 0.2; // 14.0
        assert!((sd[0] - (0.6 * ofs * 1.0 + 0.0)).abs() < 1e-5);
        assert!((sd[1] - (0.6 * ofs * 1.0 - 2.5)).abs() < 1e-5);
        pc_fish_release_shadow(4, true, 30.0, 1.0, sd.as_mut_ptr()); // eel
        assert!((sd[0] - (0.6 * ofs + 9.0)).abs() < 1e-5);
        assert!((sd[1] - (0.6 * ofs - 3.5)).abs() < 1e-5);
        pc_fish_release_shadow(7, false, 30.0, 1.0, sd.as_mut_ptr()); // whale: clamp, not extend
        assert!((sd[0] - (10.0 * ofs + 1.5)).abs() < 1e-5);
        assert!((sd[1] - (10.0 * ofs - 7.0)).abs() < 1e-5);
        pc_fish_release_shadow(4, false, -5.0, 1.0, sd.as_mut_ptr()); // below ground
        assert!((sd[0] - 0.0).abs() < 1e-5);
    }

    #[test]
    fn kage_and_misc() {
        assert_eq!(pc_fish_kage_alpha(100.0), ((50 - 10) * 6)); // 240
        assert_eq!(pc_fish_kage_alpha(20.0), 0); // (10−10)*6
        assert_eq!(pc_fish_kage_alpha(0.0), 0); // clamped
        assert_eq!(pc_fish_kage_alpha(-4.0), 0); // clamped
        assert_eq!(pc_fish_kage_ripple(0), 2);
        assert_eq!(pc_fish_kage_ripple(1), 2);
        assert_eq!(pc_fish_kage_ripple(3), 1);
        assert_eq!(pc_fish_kage_ripple(2), 0);
        assert_eq!(pc_fish_kage_ripple(4), 0);
        assert_eq!(pc_fish_kage_ripple(5), 0);
        assert_eq!(pc_fish_kage_ripple(6), -1);
        assert_eq!(pc_fish_kage_ripple(7), -1);
        assert!(pc_fish_flee(100.0, true, false, false));
        assert!(!pc_fish_flee(100.0, false, false, false));
        assert!(pc_fish_flee(140.0, false, true, false));
        assert!(!pc_fish_flee(150.0, false, true, false));
        assert!(pc_fish_flee(500.0, false, false, true));
        assert!(pc_fish_search_ok(35.0, 0, 0, 5.0));
        assert!(!pc_fish_search_ok(45.0, 0, 0, 5.0));
        assert!(!pc_fish_search_ok(35.0, 0, 0, 11.0));
        assert!((pc_fish_bite_time(0, 0) - 20.0).abs() < 1e-5);
        assert!((pc_fish_bite_time(1, 4) - 120.0).abs() < 1e-5);
    }
}
