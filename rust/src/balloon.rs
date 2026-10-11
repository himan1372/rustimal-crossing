//! Balloon presents (`m_fuusen.c`, `ac_fuusen.c`,
//! `ac_balloon.c`).
//!
//! Verified against `src/game/m_fuusen.c`,
//! `include/m_fuusen.h`, `include/m_common_data.h`,
//! `src/game/m_play.c`, `src/actor/ac_fuusen.c`,
//! `include/ac_fuusen.h`, `src/actor/ac_balloon.c`,
//! `include/ac_balloon.h`, `src/game/m_private.c`,
//! `include/m_name_table.h` (GAFE01_00 Rev 0 USA). 17/17
//! sections PROVEN with DIFFERS-level nuances (see below).
//!
//! Scope split: this module owns the pure kernels — the
//! spawn-minute predicate, the roll and probability math,
//! the wind-bucket and entry-point tables, the movement
//! and escape predicates, the tree thresholds, the
//! present-drop distance gate, the reward branch, and the
//! held-balloon hide predicate. Actor lifecycle, the
//! common-data accessors, all `fqrand()`/`RANDOM`/
//! `RANDOM_F` call order, foreground placement and clip
//! calls, save access, effects/audio dispatch, the
//! `anim_frame`/`dt` accumulators, and the held-balloon
//! skeleton/animation setup stay in C.
//!
//! Nuances carried: `escape_timer`/`timer` are f32, not
//! int; `fuwafuwa_cycle` is frame-rate scaled, not a
//! fixed +250; the present-drop check is
//! `escape_timer <= 1554`, not `==`; the look-up distance
//! is 40.0f × 16 = 640.0f; the NULL guard checks
//! `item_tree_fruit_drop_proc` while the call uses
//! `fruit_set_proc`; the LOOK_UP +0.25f lands in
//! `Balloon_move`; colors come from two 5-entry RGBA
//! tables (prim+env); a failed probability roll leaves
//! the state at CHK_SPAWN while PENDING_SPAWN retries
//! every frame; the probability increment has no clamp;
//! `ac_balloon.c` also has a second caller in
//! `m_player_main_release_creature.c_inc`.

/// Outdoor balloon actor profile id.
pub const PROFILE_FUUSEN: i32 = 178;
/// The dummy present item placed by the actor.
pub const ITM_PRESENT: i32 = 0x251C;
/// Rare furniture list used by the reward conversion.
pub const LISTTYPE_RARE: i32 = 2;

/// Spawn states.
pub const STATE_DEAD: i32 = 0;
pub const STATE_CHK_SPAWN: i32 = 1;
pub const STATE_SPAWNED: i32 = 2;
pub const STATE_LOOK_UP: i32 = 3;
pub const STATE_PENDING_SPAWN: i32 = 4;
pub const STATE_NUM: i32 = 5;
/// Initial spawn probability.
pub const INIT_SPAWN_PERCENT: f32 = 0.05;
/// LOOK_UP probability bonus, applied in Balloon_move.
pub const LOOK_UP_BONUS: f32 = 0.25;
/// Failure-increment constants.
pub const FAIL_INCREMENT_BASE: f32 = 0.025;
pub const GOODS_POWER_MAX: f32 = 50.0;
pub const FIELDRANK_SIX: f32 = 6.0;

/// Escape timer values.
pub const ESCAPE_TIMER: f32 = 1554.0;
pub const BIRTH_ESCAPE_TIMER: f32 = 2000.0;
pub const WOOD_STOP_TIMER: f32 = 19554.0; // 18000 + 1554
/// Construction values.
pub const BIRTH_Y_OFFSET: f32 = 110.0;
pub const BIRTH_HEIGHT: f32 = 200.0;
pub const BIRTH_SCALE: f32 = 0.01;
pub const BIRTH_MOVE_TIMER: f32 = 10.0;
/// Balloon colors per type.
pub const TYPE_COUNT: i32 = 5;
/// Map-edge escape bounds.
pub const EDGE_X_MIN: f32 = 660.0;
pub const EDGE_X_MAX: f32 = 3820.0;
pub const EDGE_Z_MIN: f32 = 660.0;
pub const EDGE_Z_MAX: f32 = 4460.0;
/// Train-station escape rectangle.
pub const STATION_X_MIN: f32 = 2040.0;
pub const STATION_X_MAX: f32 = 2440.0;
pub const STATION_Z_MIN: f32 = 800.0;
pub const STATION_Z_MAX: f32 = 960.0;
/// Look-up distance gate: 40.0f × 16.
pub const LOOK_UP_DIST: f32 = 640.0;
/// Present deletion height above the ground reference.
pub const DELETE_HEIGHT: f32 = 500.0;
/// Vertical bobbing amplitude.
pub const BOB_AMPLITUDE: f32 = 10.0;
/// Wall-hit y_offset adjustment.
pub const WALL_Y_RAISE: f32 = 0.05;
pub const WALL_Y_LOWER: f32 = 0.005;
pub const Y_OFFSET_CAP: f32 = 300.0;
/// Screen-visibility early-return bounds.
pub const SCREEN_X_MIN: f32 = -40.0;
pub const SCREEN_X_MAX: f32 = 360.0;
pub const SCREEN_Y_MIN: f32 = -40.0;
pub const SCREEN_Y_MAX: f32 = 280.0;
/// Tree-interception squared thresholds.
pub const CEDAR_HORIZ2: f32 = 484.0;
pub const TREE_HORIZ2: f32 = 225.0;
pub const TREE_VERT2: f32 = 225.0;
/// Steering probes, in short-angle units.
pub const STEER_PROBES: [i32; 3] = [-2500, 0, 2500];
/// Steering probe distance.
pub const STEER_DIST: f32 = 80.0;
/// Held-balloon hide threshold above the player.
pub const HELD_HIDE_HEIGHT: f32 = 200.0;

/// 16 wind buckets → 8 perimeter indices.
pub const DATA_INDEX_DATA: [usize; 16] = [0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 0];
/// Eight perimeter entry points (x, z).
pub const BIRTH_POS: [[f32; 2]; 8] = [
    [1600.0, 500.0],
    [500.0, 500.0],
    [500.0, 1600.0],
    [500.0, 4620.0],
    [1600.0, 4620.0],
    [3980.0, 4620.0],
    [3980.0, 1600.0],
    [3980.0, 500.0],
];
/// Per-entry coordinate-variation magnitudes (x, z).
pub const BIRTH_POS_RANDOM: [[f32; 2]; 8] = [
    [1280.0, 0.0],
    [960.0, 960.0],
    [0.0, 1920.0],
    [960.0, -960.0],
    [1280.0, 0.0],
    [-960.0, -960.0],
    [0.0, 1920.0],
    [-960.0, 960.0],
];

/// Spawn-minute predicate: `(min % 5) == 3` and not the
/// minute already recorded.
#[no_mangle]
pub extern "C" fn pc_balloon_spawn_minute_ok(min: i32, last_min: i32) -> bool {
    min % 5 == 3 && min != last_min
}

/// Spawn roll: `rng < spawn_percent`. The `fqrand()`
/// sample stays in C.
#[no_mangle]
pub extern "C" fn pc_balloon_roll_ok(rng: f32, spawn_percent: f32) -> bool {
    rng < spawn_percent
}

/// Failure increment: `0.025 + goods_power_mod * 0.025 +
/// field_rank_mod * 0.025 + RANDOM_F(0.025)`, where the
/// mods are normalized by GOODS_POWER_MAX (50) and
/// FIELDRANK_SIX (6). The `RANDOM_F(0.025)` sample stays
/// in C; no clamp is applied.
#[no_mangle]
pub extern "C" fn pc_balloon_fail_increment(
    goods_power: f32,
    field_rank: f32,
    rand_f: f32,
) -> f32 {
    FAIL_INCREMENT_BASE
        + (goods_power / GOODS_POWER_MAX) * FAIL_INCREMENT_BASE
        + (field_rank / FIELDRANK_SIX) * FAIL_INCREMENT_BASE
        + rand_f
}

/// Post-success probability: `(1.0 − current) * 0.1`.
#[no_mangle]
pub extern "C" fn pc_balloon_post_success_percent(current: f32) -> f32 {
    (1.0 - current) * 0.1
}

/// Maps a wind angle to a perimeter index:
/// `data_index_data[(((s16)(angle & 0xF000)) >> 12) & 0xF]`.
#[no_mangle]
pub extern "C" fn pc_balloon_wind_bucket(angle_y: i32) -> usize {
    let bucket = (((angle_y & 0xF000) as i16) as i32 >> 12) & 0xF;
    DATA_INDEX_DATA[bucket as usize]
}

/// Entry point (`aFSN_birth_init`): the perimeter base
/// plus the table-driven variation. When both axes are
/// available the axis follows `game_frame & 1`; when
/// only Z is available Z is used; otherwise X. The
/// `fqrand()` sample stays in C (`rand_sample`).
/// Variation = sample × |offset|, added or subtracted
/// per the offset's sign. Writes (x, z) to `out[0..2]`;
/// returns 1 on success. The ground-height query and the
/// y = ground + 200 assignment stay in C.
#[no_mangle]
pub extern "C" fn pc_balloon_entry_point(
    wind_idx: usize,
    game_frame: i32,
    rand_sample: f32,
    out: *mut f32,
) -> u8 {
    if out.is_null() || wind_idx >= 8 {
        return 0;
    }
    let base = BIRTH_POS[wind_idx];
    let rnd = BIRTH_POS_RANDOM[wind_idx];
    let randomize_z = if rnd[0] != 0.0 && rnd[1] != 0.0 {
        game_frame & 1 != 0
    } else if rnd[1] != 0.0 {
        true
    } else {
        false
    };
    let mut x = base[0];
    let mut z = base[1];
    if !randomize_z {
        if rnd[0] != 0.0 {
            let v = rand_sample * rnd[0].abs();
            if rnd[0] > 0.0 {
                x += v;
            } else {
                x -= v;
            }
        }
    } else if rnd[1] != 0.0 {
        let v = rand_sample * rnd[1].abs();
        if rnd[1] > 0.0 {
            z += v;
        } else {
            z -= v;
        }
    }
    let out = unsafe { std::slice::from_raw_parts_mut(out, 2) };
    out[0] = x;
    out[1] = z;
    1
}

/// Balloon type index: `game_frame % 5`.
#[no_mangle]
pub extern "C" fn pc_balloon_type_idx(game_frame: i32) -> i32 {
    game_frame % TYPE_COUNT
}

/// Movement speed: `wind_power * 0.5 + 1.0`.
#[no_mangle]
pub extern "C" fn pc_balloon_speed(wind_power: f32) -> f32 {
    wind_power * 0.5 + 1.0
}

/// Escape bounds: map edges OR the train-station
/// rectangle. The timer expiry gate stays in C.
#[no_mangle]
pub extern "C" fn pc_balloon_escape_bounds(x: f32, z: f32) -> bool {
    x <= EDGE_X_MIN
        || x >= EDGE_X_MAX
        || z <= EDGE_Z_MIN
        || z >= EDGE_Z_MAX
        || (x >= STATION_X_MIN && x <= STATION_X_MAX && z >= STATION_Z_MIN && z <= STATION_Z_MAX)
}

/// Tree-interception thresholds: squared horizontal
/// distance < 484 for cedar, < 225 otherwise, and
/// squared vertical distance < 225. Inputs are the
/// squared distances; the foreground lookup stays in C.
#[no_mangle]
pub extern "C" fn pc_balloon_tree_thresholds(is_cedar: bool, horiz2: f32, vert2: f32) -> bool {
    let h = if is_cedar { CEDAR_HORIZ2 } else { TREE_HORIZ2 };
    horiz2 < h && vert2 < TREE_VERT2
}

/// Screen-visibility gate: returns false when the actor
/// is outside [-40, 360] × [-40, 280] and the movement
/// callback should return early.
#[no_mangle]
pub extern "C" fn pc_balloon_screen_visible(sx: f32, sy: f32) -> bool {
    sx >= SCREEN_X_MIN && sx <= SCREEN_X_MAX && sy >= SCREEN_Y_MIN && sy <= SCREEN_Y_MAX
}

/// Wall-hit y_offset update: +0.05 capped at 300 on a
/// wall hit, −0.005 floored at 110 otherwise.
#[no_mangle]
pub extern "C" fn pc_balloon_wall_y_offset(hit_wall: bool, y_offset: f32) -> f32 {
    if hit_wall {
        (y_offset + WALL_Y_RAISE).min(Y_OFFSET_CAP)
    } else {
        (y_offset - WALL_Y_LOWER).max(BIRTH_Y_OFFSET)
    }
}

/// Look-up distance gate: horizontal distance < 640.0.
/// The `escape_timer <= 1554` gate stays in C.
#[no_mangle]
pub extern "C" fn pc_balloon_look_up_dist_ok(dx: f32, dz: f32) -> bool {
    (dx * dx + dz * dz).sqrt() < LOOK_UP_DIST
}

/// Deletion gate: y > ground + 500 after placement.
#[no_mangle]
pub extern "C" fn pc_balloon_delete_ok(y: f32, ground_y: f32) -> bool {
    y > ground_y + DELETE_HEIGHT
}

/// Reward branch (`mPr_DummyPresentToTruePresent`): the
/// RANDOM(5) outcome != 0 attempts furniture; the fruit
/// fallback fires when that fails OR when the outcome is
/// 0. Returns true when the furniture selection should
/// be attempted; the list selection, `mFI_GetOtherFruit()`,
/// and the possession-item conversion stay in C.
#[no_mangle]
pub extern "C" fn pc_present_reward_branch(rand5: i32, furniture_ok: bool) -> bool {
    rand5 != 0 && furniture_ok
}

/// Held-balloon hide predicate: `balloon_y − player_y >
/// 200.0`. The skeleton/animation and position copy stay
/// in C.
#[no_mangle]
pub extern "C" fn pc_player_balloon_hide_ok(balloon_y: f32, player_y: f32) -> bool {
    balloon_y - player_y > HELD_HIDE_HEIGHT
}

/// Movement-timer gate: `Actor_position_moveF` runs when
/// `timer <= 0.0`; otherwise the timer decrements.
#[no_mangle]
pub extern "C" fn pc_balloon_move_timer_ok(timer: f32) -> bool {
    timer <= 0.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawn() {
        // Qualifying minutes: 03, 08, ..., 58.
        for m in [3, 8, 13, 18, 23, 28, 33, 38, 43, 48, 53, 58] {
            assert!(pc_balloon_spawn_minute_ok(m, -1), "{m}");
        }
        assert!(!pc_balloon_spawn_minute_ok(4, -1));
        assert!(!pc_balloon_spawn_minute_ok(0, -1));
        assert!(!pc_balloon_spawn_minute_ok(3, 3)); // already recorded
        assert!(pc_balloon_roll_ok(0.04, 0.05));
        assert!(!pc_balloon_roll_ok(0.05, 0.05)); // strict
        assert_eq!(
            pc_balloon_fail_increment(25.0, 3.0, 0.01),
            0.025 + 0.5 * 0.025 + 0.5 * 0.025 + 0.01
        );
        let p = pc_balloon_post_success_percent(0.05);
        assert!((p - 0.095).abs() < 1e-6, "{p}");
        assert_eq!(pc_balloon_post_success_percent(0.0), 0.1);
    }

    #[test]
    fn entry() {
        // 16 buckets -> 8 indices.
        let buckets: Vec<usize> = (0..16).map(|i| pc_balloon_wind_bucket((i as i32) << 12)).collect();
        assert_eq!(buckets, DATA_INDEX_DATA);
        // Entry 1: both axes available; game_frame & 1 chooses.
        let mut out = [0.0f32; 2];
        assert_eq!(pc_balloon_entry_point(1, 0, 0.5, out.as_mut_ptr()), 1);
        assert_eq!(out, [500.0 + 480.0, 500.0]); // X: 0.5 * 960
        let mut out = [0.0f32; 2];
        assert_eq!(pc_balloon_entry_point(1, 1, 0.5, out.as_mut_ptr()), 1);
        assert_eq!(out, [500.0, 500.0 + 480.0]); // Z: 0.5 * 960
        // Entry 0: only X available.
        let mut out = [0.0f32; 2];
        assert_eq!(pc_balloon_entry_point(0, 1, 0.25, out.as_mut_ptr()), 1);
        assert_eq!(out, [1600.0 + 320.0, 500.0]); // 0.25 * 1280
        // Entry 5: negative offsets subtract.
        let mut out = [0.0f32; 2];
        assert_eq!(pc_balloon_entry_point(5, 0, 1.0, out.as_mut_ptr()), 1);
        assert_eq!(out, [3980.0 - 960.0, 4620.0]);
        assert_eq!(pc_balloon_type_idx(7), 2);
        assert_eq!(pc_balloon_entry_point(8, 0, 0.0, out.as_mut_ptr()), 0);
    }

    #[test]
    fn movement_and_bounds() {
        assert_eq!(pc_balloon_speed(2.0), 2.0);
        assert!(pc_balloon_escape_bounds(600.0, 2000.0));
        assert!(pc_balloon_escape_bounds(3900.0, 2000.0));
        assert!(pc_balloon_escape_bounds(2000.0, 600.0));
        assert!(pc_balloon_escape_bounds(2000.0, 4500.0));
        assert!(pc_balloon_escape_bounds(2200.0, 880.0)); // station
        assert!(!pc_balloon_escape_bounds(2000.0, 2000.0));
        assert!(!pc_balloon_escape_bounds(2040.0, 700.0)); // station x, wrong z
        assert!(pc_balloon_tree_thresholds(false, 224.0, 224.0));
        assert!(!pc_balloon_tree_thresholds(false, 225.0, 224.0));
        assert!(pc_balloon_tree_thresholds(true, 483.0, 224.0));
        assert!(!pc_balloon_tree_thresholds(true, 484.0, 224.0));
        assert!(pc_balloon_tree_thresholds(true, 224.0, 224.0));
        assert!(pc_balloon_screen_visible(0.0, 0.0));
        assert!(!pc_balloon_screen_visible(361.0, 0.0));
        assert!(!pc_balloon_screen_visible(0.0, -41.0));
        assert_eq!(pc_balloon_wall_y_offset(true, 110.0), 110.05);
        assert_eq!(pc_balloon_wall_y_offset(true, 299.99), 300.0);
        assert_eq!(pc_balloon_wall_y_offset(false, 110.0), 110.0);
        assert_eq!(pc_balloon_wall_y_offset(false, 120.0), 119.995);
        assert!(pc_balloon_move_timer_ok(0.0));
        assert!(!pc_balloon_move_timer_ok(0.5));
    }

    #[test]
    fn drop_and_reward() {
        // 640.0 strict.
        assert!(pc_balloon_look_up_dist_ok(400.0, 300.0)); // 500 < 640
        assert!(!pc_balloon_look_up_dist_ok(640.0, 0.0));
        assert!(pc_balloon_delete_ok(601.0, 100.0));
        assert!(!pc_balloon_delete_ok(600.0, 100.0));
        // Reward: rand5 != 0 attempts furniture; failure or 0 -> fruit.
        assert!(pc_present_reward_branch(3, true));
        assert!(!pc_present_reward_branch(0, true));
        assert!(!pc_present_reward_branch(3, false));
        assert!(pc_player_balloon_hide_ok(500.0, 100.0));
        assert!(!pc_player_balloon_hide_ok(300.0, 100.0));
    }
}
