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

/// Behavior program: the kaseki program is selected for
/// SALMON2 (rewritten to SALMON, then falling through),
/// coelacanth, jellyfish, sea bass, red snapper,
/// barred knifejaw, and whale; everything else (incl.
/// plain SALMON) uses the test program.
#[no_mangle]
pub extern "C" fn pc_fish_behavior_program(fish_type: i32) -> i32 {
    // ac_gyoei_clip.c_inc: `case SALMON2: gyo_type = SALMON;` then FALLS
    // THROUGH into the KASEKI group, so the river-mouth salmon variant runs
    // the KASEKI program while plain SALMON takes `default` (TEST). The type
    // rewrite to SALMON is the caller's side effect; only the program is
    // returned here.
    match fish_type {
        T_SALMON2 | T_COELACANTH | T_JELLYFISH | T_SEA_BASS | T_RED_SNAPPER | T_BARRED_KNIFEJAW | T_WHALE => {
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
/// Fish action-state machines, bobber-sync protocol, and draw tables
/// (`ac_gyo_test.c`, `ac_gyo_kaseki.c`, `ac_gyoei_draw.c_inc`,
/// `ac_uki_move.c_inc`).
///
/// The two behavior programs share `aGYO_CTRL_ACTOR` but NOT a state
/// machine: standard fish have 7 actions, special fish have 11 (including
/// `WAIT_MONSTER`, whose callback is `none_proc1` but which has a real
/// initializer — preserve it, don't delete the state). `setupAction`
/// stores the ID, installs the callback from the parallel table, then
/// calls the initializer IMMEDIATELY — transition side effects are never
/// deferred a tick.
///
/// The bobber (`UKI_ACTOR`, 0x2C8 bytes — NOT 0x2B8, which is
/// `hamon_accum`'s offset) syncs via `gyo_command`/`gyo_status`/`gyo_type`
/// at offsets 0x288/0x28C/0x290. `aUKI_set_proc` runs the procedure
/// initializer BEFORE storing the new `proc`. `gyo_status = 8` is the
/// release/create handoff: the fish resets it to 0 BEFORE calling the
/// creation helper.
///
/// `aGYO_hosei_y` is a RENDER-ONLY vertical correction (subtracted in the
/// model matrix), never applied to the actor's world position or AI.
/// `aGYO_displayList` is 45 pointers to `(dl_a, dl_b, dl_c)` triples; the
/// renderer picks `[(int)(frame * 0.5f)]`, so `dl_c` is unreachable via
/// that expression. There is NO fish-specific `aGYO_overlay_entry_c`
/// (0x1C08) anywhere in the tree — that size belongs to the insect
/// overlay `aINS_overlay_entry_c`; the fish overlay scratch is
/// `aGYO_overlay_c` = 2 × 0x3C00 bytes.

/// Standard-fish actions (`aGTT_ACTION_*`).
pub mod gtt {
    pub const SWIM: u8 = 0;
    pub const WAIT: u8 = 1;
    pub const ESCAPE: u8 = 2;
    pub const NEAR: u8 = 3;
    pub const TOUCH: u8 = 4;
    pub const BITE: u8 = 5;
    pub const COMEBACK: u8 = 6;
    pub const NUM: u8 = 7;
}

/// Special-fish actions (`aGKK_ACTION_*`).
pub mod gkk {
    pub const SWIM: u8 = 0;
    pub const SWIM2: u8 = 1;
    pub const SWIM3: u8 = 2;
    pub const SWIM4: u8 = 3;
    pub const WAIT: u8 = 4;
    pub const WAIT_MONSTER: u8 = 5;
    pub const ESCAPE: u8 = 6;
    pub const NEAR: u8 = 7;
    pub const TOUCH: u8 = 8;
    pub const BITE: u8 = 9;
    pub const COMEBACK: u8 = 10;
    pub const NUM: u8 = 11;
}

/// Bobber `gyo_status` protocol values.
pub mod gyo_status {
    pub const NONE: i32 = 0;
    pub const TOUCHABLE: i32 = 1;
    pub const TOUCH_OK: i32 = 2;
    pub const BITE_ANIM: i32 = 3;
    pub const BITE_ACTIVE: i32 = 4;
    pub const COMEBACK: i32 = 5;
    pub const CATCH_ANIM: i32 = 6;
    pub const GET_HANDOFF: i32 = 7;
    pub const RELEASE_CREATE: i32 = 8;
}

/// `UKI_ACTOR` layout facts (`ac_uki.h`).
pub const UKI_ACTOR_SIZE: usize = 0x2C8;
pub const UKI_GYO_COMMAND_OFS: usize = 0x288;
pub const UKI_GYO_STATUS_OFS: usize = 0x28C;
pub const UKI_GYO_TYPE_OFS: usize = 0x290;

/// Animation pattern types (`aGYO_ANIME_PAT_TYPE_*`).
pub const ANIME_PAT_NONE: i32 = 0;
pub const ANIME_PAT_FAST: i32 = 1;
pub const ANIME_PAT_SLOW: i32 = 2;

/// Species animation-pattern table (`aGYO_anime_ptn[45]`).
pub const GYO_ANIME_PTN: [i32; 45] = [
    1, 2, 2, 2, 2, 1, 2, 2, 1, 2, 2, 2, 2, 1, 1,
    2, 1, 1, 1, 2, 2, 2, 2, 1, 1, 2, 2, 1, 1, 1,
    1, 2, 1, 1, 1, 2, 2, 2, 2, 2, 0, 0, 0, 0, 0,
];

/// Fast animation-frame sequence (`aGYO_frame_ptn_fast[8]`).
pub const GYO_FRAME_PTN_FAST: [i32; 8] = [1, 1, 0, 0, 2, 2, 0, 0];
/// Slow animation-frame sequence (`aGYO_frame_ptn_slow[16]`).
pub const GYO_FRAME_PTN_SLOW: [i32; 16] =
    [1, 1, 1, 1, 0, 0, 0, 0, 2, 2, 2, 2, 0, 0, 0, 0];

/// Species render Y-offset table (`aGYO_hosei_y[45]`): subtracted from the
/// model's world Y in the draw matrix ONLY — never touches physics/AI.
pub const GYO_HOSEI_Y: [f32; 45] = [
    0.0, 0.0, 2.2, 2.2, 2.2, 0.0, 1.5, 2.2, 0.0, 2.2, 2.2, 2.2, 1.5, 0.0, 0.0,
    0.0, 0.0, 0.0, 0.0, 2.2, 1.5, 2.2, 1.5, 0.0, 0.0, 2.2, 2.2, 0.0, 0.0, 0.0,
    0.0, 2.2, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
];

/// Bite-timer base table (`timer[]` in `aUKI_set_proc_bite`), indexed by
/// size class; trash uses a flat 26.
pub const GYO_BITE_TIMER: [i32; 8] = [26, 39, 39, 39, 52, 65, 78, 78];
/// Trash substitution table (`gomi[8]`), indexed by size class:
/// `{can, can, boot, boot, boot, tire, tire, tire}` as `aGYO_TYPE_*`
/// values (41/42/43).
pub const GYO_GOMI: [i32; 8] = [41, 41, 42, 42, 42, 43, 43, 43];

/// `aGYO_anime_frame`: increments the frame index BEFORE reading the
/// pattern; wraps to 0 at the sequence end (decrementing `*swork2` on wrap
/// when `gyo_flags & 0x200`); NONE forces index 0 and returns 0. Writes
/// back `*anim_frame`; optionally writes back `*swork2`. Returns the frame.
#[no_mangle]
pub extern "C" fn pc_gyo_anime_frame(
    anim_frame: *mut i32,
    gyo_type: i32,
    gyo_flags: u32,
    swork2: *mut i32,
) -> i32 {
    if anim_frame.is_null() || gyo_type < 0 || gyo_type as usize >= GYO_ANIME_PTN.len() {
        return 0;
    }
    let af = unsafe { &mut *anim_frame };
    let mut idx = *af + 1;
    let ret = match GYO_ANIME_PTN[gyo_type as usize] {
        ANIME_PAT_FAST => {
            if idx >= GYO_FRAME_PTN_FAST.len() as i32 {
                idx = 0;
                if gyo_flags & 0x200 != 0 && !swork2.is_null() {
                    unsafe { *swork2 -= 1 };
                }
            }
            GYO_FRAME_PTN_FAST[idx as usize]
        }
        ANIME_PAT_SLOW => {
            if idx >= GYO_FRAME_PTN_SLOW.len() as i32 {
                idx = 0;
                if gyo_flags & 0x200 != 0 && !swork2.is_null() {
                    unsafe { *swork2 -= 1 };
                }
            }
            GYO_FRAME_PTN_SLOW[idx as usize]
        }
        _ => {
            idx = 0;
            0
        }
    };
    *af = idx;
    ret
}

/// Display-list selector: `(int)(frame * 0.5f)`.
#[no_mangle]
pub extern "C" fn pc_gyo_dl_index(frame: i32) -> i32 {
    (frame as f32 * 0.5) as i32
}

/// `aUKI_set_proc_bite` timer: trash → 26, else
/// `(int)((f32)(int)timer[size] * 2.0f)`.
#[no_mangle]
pub extern "C" fn pc_gyo_bite_timer(trash: bool, size_class: i32) -> i32 {
    if trash {
        26
    } else if size_class >= 0 && (size_class as usize) < GYO_BITE_TIMER.len() {
        ((GYO_BITE_TIMER[size_class as usize] as f32) * 2.0) as i32
    } else {
        26
    }
}

/// Trash substitution pick: `gomi[size_type]` (`aGYO_TYPE_*` value).
#[no_mangle]
pub extern "C" fn pc_gyo_trash_pick(size_type: i32) -> i32 {
    if size_type >= 0 && (size_type as usize) < GYO_GOMI.len() {
        GYO_GOMI[size_type as usize]
    } else {
        41
    }
}

/// Special-fish WAIT expiry: chooses from `{SWIM, SWIM2, SWIM}` via
/// `RANDOM(3)` — `rng` is the raw `RANDOM(3.0)` value in [0, 3).
#[no_mangle]
pub extern "C" fn pc_gkk_wait_swim_choice(rng: f32) -> u8 {
    const CHOICE: [u8; 3] = [gkk::SWIM, gkk::SWIM2, gkk::SWIM];
    let i = (rng as i32).clamp(0, 2) as usize;
    CHOICE[i]
}

    #[test]
    fn action_state_ids() {
        // standard: 7 actions
        assert_eq!((gtt::SWIM, gtt::WAIT, gtt::ESCAPE, gtt::NEAR, gtt::TOUCH, gtt::BITE, gtt::COMEBACK, gtt::NUM), (0, 1, 2, 3, 4, 5, 6, 7));
        // special: 11 actions, WAIT_MONSTER has no-op callback but a real slot
        assert_eq!((gkk::SWIM, gkk::SWIM2, gkk::SWIM3, gkk::SWIM4, gkk::WAIT, gkk::WAIT_MONSTER, gkk::ESCAPE, gkk::NEAR, gkk::TOUCH, gkk::BITE, gkk::COMEBACK, gkk::NUM), (0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11));
        // bobber protocol + layout
        assert_eq!((gyo_status::NONE, gyo_status::TOUCHABLE, gyo_status::TOUCH_OK, gyo_status::BITE_ANIM, gyo_status::BITE_ACTIVE, gyo_status::COMEBACK, gyo_status::CATCH_ANIM, gyo_status::GET_HANDOFF, gyo_status::RELEASE_CREATE), (0, 1, 2, 3, 4, 5, 6, 7, 8));
        assert_eq!(UKI_ACTOR_SIZE, 0x2C8);
        assert_eq!((UKI_GYO_COMMAND_OFS, UKI_GYO_STATUS_OFS, UKI_GYO_TYPE_OFS), (0x288, 0x28C, 0x290));
    }

    #[test]
    fn draw_tables() {
        assert_eq!(GYO_HOSEI_Y.len(), 45);
        assert_eq!(GYO_HOSEI_Y[2], 2.2);
        assert_eq!(GYO_HOSEI_Y[6], 1.5);
        assert_eq!(GYO_HOSEI_Y[0], 0.0);
        assert_eq!(GYO_HOSEI_Y[44], 0.0);
        assert_eq!(GYO_ANIME_PTN.len(), 45);
        assert_eq!(GYO_ANIME_PTN[0], ANIME_PAT_FAST);
        assert_eq!(GYO_ANIME_PTN[1], ANIME_PAT_SLOW);
        assert_eq!(GYO_ANIME_PTN[40], ANIME_PAT_NONE);
        assert_eq!(GYO_FRAME_PTN_FAST, [1, 1, 0, 0, 2, 2, 0, 0]);
        assert_eq!(GYO_FRAME_PTN_SLOW, [1, 1, 1, 1, 0, 0, 0, 0, 2, 2, 2, 2, 0, 0, 0, 0]);
        // display-list selector
        assert_eq!(pc_gyo_dl_index(0), 0);
        assert_eq!(pc_gyo_dl_index(1), 0);
        assert_eq!(pc_gyo_dl_index(2), 1);
        assert_eq!(pc_gyo_dl_index(3), 1);
    }

    #[test]
    fn anime_frame_stepping() {
        // FAST (type 0): increments before read, wraps at 8.
        let mut af = 0i32;
        let mut sw = 10i32;
        assert_eq!(pc_gyo_anime_frame(&mut af, 0, 0, &mut sw), 1); // idx 1 -> 1
        assert_eq!(af, 1);
        assert_eq!(pc_gyo_anime_frame(&mut af, 0, 0, &mut sw), 0); // idx 2 -> 0
        af = 7;
        assert_eq!(pc_gyo_anime_frame(&mut af, 0, 0, &mut sw), 1); // wrap -> fast[0]
        assert_eq!(af, 0);
        assert_eq!(sw, 10); // no decrement without flag 0x200
        af = 7;
        assert_eq!(pc_gyo_anime_frame(&mut af, 0, 0x200, &mut sw), 1);
        assert_eq!(sw, 9); // wrap + flag -> swork2--
        // SLOW (type 1): wraps at 16.
        af = 15;
        assert_eq!(pc_gyo_anime_frame(&mut af, 1, 0, &mut sw), 1); // slow[0]
        assert_eq!(af, 0);
        // NONE (type 40): forced to 0, returns 0.
        af = 5;
        assert_eq!(pc_gyo_anime_frame(&mut af, 40, 0x200, &mut sw), 0);
        assert_eq!(af, 0);
        assert_eq!(sw, 9); // NONE does not touch swork2
    }

    #[test]
    fn bite_and_trash() {
        assert_eq!(pc_gyo_bite_timer(true, 3), 26);
        assert_eq!(pc_gyo_bite_timer(false, 0), 52); // (int)(26 * 2.0)
        assert_eq!(pc_gyo_bite_timer(false, 4), 104); // (int)(52 * 2.0)
        assert_eq!(pc_gyo_bite_timer(false, 7), 156);
        assert_eq!(pc_gyo_trash_pick(0), 41);
        assert_eq!(pc_gyo_trash_pick(1), 41);
        assert_eq!(pc_gyo_trash_pick(2), 42);
        assert_eq!(pc_gyo_trash_pick(5), 43);
        assert_eq!(pc_gyo_trash_pick(7), 43);
        // special WAIT choice: {SWIM, SWIM2, SWIM}
        assert_eq!(pc_gkk_wait_swim_choice(0.0), gkk::SWIM);
        assert_eq!(pc_gkk_wait_swim_choice(1.5), gkk::SWIM2);
        assert_eq!(pc_gkk_wait_swim_choice(2.9), gkk::SWIM);
    }

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
