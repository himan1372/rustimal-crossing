//! Fishing and bug-catching mechanics.
//!
//! Verified against `src/actor/ac_set_ovl_gyoei.c` (fish spawn tables,
//! time periods, 24 terms, 5-day transition, field-rank modifiers,
//! weighted selection with retry), `include/ac_set_ovl_gyoei.h`
//! (spawn areas, time defines), `src/actor/ac_set_ovl_insect.c`
//! (insect spawn, nothing-spawns roll, habitat filtering),
//! `include/ac_set_ovl_insect.h` (14 spawn areas),
//! `include/ac_insect_h.h` (aINS_ACTOR_NUM=9),
//! `src/game/m_player_main_swing_net.c_inc` (net capture geometry:
//! 50/60 sweep, 15/21+radius, projection test),
//! `include/m_player.h` (mPlayer_NET_CATCH_TABLE_COUNT=8)
//! (USA Rev. 0 decomp / PC port).
//!
//! Architecture: fish and insects are separate pipelines. Fish spawn
//! via weighted selection with habitat-retry; insects filter habitats
//! first, then select (with an explicit no-spawn probability mass).
//! The net uses a player-side capture-request table evaluated in
//! insertion order during the swing -- never a nearest-insect search.

// ---- Shared: time terms ----

/// Fish time periods: TIME_0 = 21:00-03:59, TIME_1 = 04:00-08:59,
/// TIME_2 = 09:00-15:59, TIME_3 = 16:00-20:59.
/// Note: hour 21-23 matches no branch, so time_no keeps its initial
/// TIME_0 value (source initializes time_no = aSOG_TIME_0).
pub fn fish_time_no(hour: u8) -> u8 {
    if hour <= 3 {
        0
    } else if hour <= 8 {
        1
    } else if hour <= 15 {
        2
    } else if hour <= 20 {
        3
    } else {
        0
    }
}

/// 24 half-month fishing terms: (month-1)*2 + (day>15 ? 1 : 0).
pub fn fish_term(month: u8, day: u8) -> u8 {
    let mut term = (month.saturating_sub(1)) * 2;
    if day > 15 {
        term += 1;
    }
    term % 24
}

/// 5-day term transition rate: days_from_end 5..1 -> 5/6..1/6.
/// During transition, current-term weight * rate + next-term weight *
/// (1-rate) both feed the spawn table.
pub fn term_transition_rate(days_from_end: u8) -> f32 {
    match days_from_end {
        5 => 5.0 / 6.0,
        4 => 4.0 / 6.0,
        3 => 3.0 / 6.0,
        2 => 2.0 / 6.0,
        1 => 1.0 / 6.0,
        _ => 1.0,
    }
}

/// Field-rank spawn modifier (shared shape for fish and insects).
pub fn field_rank_rate(rank: u8) -> f32 {
    match rank {
        0 => 0.5,
        1 => 0.75,
        2 => 0.875,
        _ => 1.0,
    }
}

// ---- Fish spawn ----

/// Fish spawn areas.
pub mod fish_area {
    pub const POOL: u8 = 0;
    pub const WATERFALL: u8 = 1;
    pub const RIVER_MOUTH: u8 = 2;
    pub const OFFING: u8 = 3;
    pub const SEA: u8 = 4;
    pub const RIVER: u8 = 5;
    pub const POND: u8 = 6;
}

/// One fish spawn table entry: (species, area, weight).
pub type FishSpawnEntry = (u16, u8, f32);

/// Weighted fish selection with field-rank modifier and
/// without-replacement retry on habitat incompatibility.
/// weights: candidate (weight, area_ok). Returns selected index or None.
/// Mirrors aSOG_gyoei_get_idx_sub: total from unmodified weights, then
/// each candidate subtracts weight*env_rate from the running total.
pub fn fish_select(weights: &[(f32, bool)], env_rate: f32, roll: f32) -> Option<usize> {
    let total: f32 = weights.iter().map(|(w, _)| w).sum();
    if total <= 0.0 {
        return None;
    }
    let mut tried = vec![false; weights.len()];
    let mut selected = total * roll;
    loop {
        let mut now = total;
        let mut pick = None;
        for (i, (w, _)) in weights.iter().enumerate() {
            if tried[i] {
                continue;
            }
            now -= w * env_rate;
            if selected >= now {
                pick = Some(i);
                break;
            }
        }
        match pick {
            None => return None,
            Some(i) => {
                if weights[i].1 {
                    return Some(i);
                }
                tried[i] = true; // habitat incompatible: retry without it
                if tried.iter().all(|&t| t) {
                    return None;
                }
            }
        }
    }
}

/// Interior spawn region: x,z in 2..13 (12x12) of the 16x16 acre.
pub fn interior_unit_ok(x: u8, z: u8) -> bool {
    x >= 2 && x <= 13 && z >= 2 && z <= 13
}

/// Ocean fish need water_height - ground_height >= 20.0.
pub const OCEAN_DEPTH_MIN: f32 = 20.0;

/// Fish bite timing (doubled by aUKI_set_proc_bite for normal fish).
/// Index by fish size 0..7.
pub const BITE_TIMING: [u8; 8] = [26, 39, 39, 39, 52, 65, 78, 78];

/// Float (UKI) statuses.
pub mod uki {
    pub const CARRY: u8 = 1;
    pub const READY: u8 = 2;
    pub const CAST: u8 = 3;
    pub const FLOAT: u8 = 4;
    pub const VIB: u8 = 5;
    pub const COMEBACK: u8 = 6;
    pub const CATCH: u8 = 7;
}

/// Cast trajectory: 50-frame parabola, 40-frame cast timer.
pub const CAST_FRAMES: u8 = 50;
pub const CAST_TIMER: u8 = 40;

/// Fish type -> item: gyo_type 0..39 -> ITM_FISH00..39; extended
/// trash entries follow. Returns the item offset.
pub fn fish_item_offset(gyo_type: u8) -> u8 {
    gyo_type // 0..39 map directly; trash are extended types
}

// ---- Insect spawn ----

/// Insect spawn areas (14).
pub mod insect_area {
    pub const ON_TREE: u8 = 0;
    pub const ON_FLOWER: u8 = 1;
    pub const RAINING_ON_FLOWER: u8 = 2;
    pub const FLYING: u8 = 3;
    pub const ON_GROUND: u8 = 4;
    pub const IN_BUSH: u8 = 5;
    pub const FLYING_NEAR_WATER: u8 = 6;
    pub const ON_WATER: u8 = 7;
    pub const ON_CANDY: u8 = 8;
    pub const ON_TRASH: u8 = 9;
    pub const UNDER_ROCK: u8 = 10;
    pub const UNDERGROUND: u8 = 11;
    pub const FLYING_NEAR_FLOWERS_OR_AROUND: u8 = 12;
    pub const NOTHING: u8 = 13;
}

/// Insect actor pool: 9 total, 1 reserved -> 8 normal spawn slots.
pub const INSECT_ACTOR_NUM: usize = 9;
pub const INSECT_SPAWN_SLOTS: usize = 8;

/// Multi-birth insects: red dragonfly and firefly spawn 6 + rand(3).
pub fn insect_birth_count(is_multi: bool, rand3: u8) -> u8 {
    if is_multi {
        6 + (rand3 % 3)
    } else {
        1
    }
}

/// Insect selection with the explicit no-spawn mass: if total <= 100,
/// roll against 100 (not total), leaving 100-total as no-spawn chance.
/// Returns selected index or None (nothing spawns).
pub fn insect_select(weights: &[f32], roll: f32) -> Option<usize> {
    let total: f32 = weights.iter().sum();
    if total <= 0.0 {
        return None;
    }
    let basis = if total > 100.0 { total } else { 100.0 };
    let mut selected = basis * roll;
    for (i, w) in weights.iter().enumerate() {
        selected -= w;
        if selected < 0.0 {
            return Some(i);
        }
    }
    None // fell into the no-spawn mass (or rounding)
}

// ---- Net capture ----

/// Net capture request table slots.
pub const NET_CATCH_TABLE_COUNT: usize = 8;

/// Normal net: 50.0 sweep, 15.0 + radius envelope.
/// Golden net: 60.0 sweep, 21.0 + radius envelope.
pub fn net_sweep_len(golden: bool) -> f32 {
    if golden { 60.0 } else { 50.0 }
}
pub fn net_capture_len(golden: bool, radius: f32) -> f32 {
    if golden { 21.0 + radius } else { 15.0 + radius }
}

/// Capture window opens after animation frame 6.0.
pub const NET_CAPTURE_FRAME_MIN: f32 = 6.0;

/// Geometric capture test: project the candidate position onto the
/// net sweep axis (top->bottom), then check the perpendicular distance
/// against the capture envelope. Returns true on capture.
/// top/bot: sweep axis endpoints; pos: candidate; radius: its range.
pub fn net_capture_test(
    top: (f32, f32, f32),
    bot: (f32, f32, f32),
    pos: (f32, f32, f32),
    radius: f32,
    golden: bool,
) -> bool {
    let axis = (bot.0 - top.0, bot.1 - top.1, bot.2 - top.2);
    let axis_len_sq = axis.0 * axis.0 + axis.1 * axis.1 + axis.2 * axis.2;
    if axis_len_sq <= 0.0 {
        return false;
    }
    // Projection of (pos - top) onto axis.
    let rel = (pos.0 - top.0, pos.1 - top.1, pos.2 - top.2);
    let t = (rel.0 * axis.0 + rel.1 * axis.1 + rel.2 * axis.2) / axis_len_sq;
    let closest = (top.0 + axis.0 * t, top.1 + axis.1 * t, top.2 + axis.2 * t);
    let dx = pos.0 - closest.0;
    let dy = pos.1 - closest.1;
    let dz = pos.2 - closest.2;
    let dist_sq = dx * dx + dy * dy + dz * dz;
    let len = net_capture_len(golden, radius);
    // Candidate must also be within the sweep segment (t in 0..1
    // extended by the envelope along the axis is approximated by the
    // radial test; the source clamps via the projection).
    let _ = net_sweep_len(golden);
    t >= 0.0 && t <= 1.0 && dist_sq <= len * len
}

/// Default insect catch radius; butterflies get 24.
pub const CATCH_RADIUS_DEFAULT: f32 = 8.0;
pub const CATCH_RADIUS_BUTTERFLY: f32 = 24.0;

/// Evaluate capture requests in insertion order; first geometric hit
/// wins (not nearest). force: optional forced-capture candidate index.
pub fn net_capture_pick(
    force: Option<usize>,
    requests: &[(f32, f32, f32, f32)], // (x,y,z,radius) in insertion order
    top: (f32, f32, f32),
    bot: (f32, f32, f32),
    golden: bool,
) -> Option<usize> {
    if let Some(f) = force {
        if f < requests.len() {
            return Some(f);
        }
    }
    for (i, r) in requests.iter().enumerate() {
        if net_capture_test(top, bot, (r.0, r.1, r.2), r.3, golden) {
            return Some(i);
        }
    }
    None
}

// ---- C ABI ----

/// C ABI: fish time period for an hour (0..3).
#[no_mangle]
pub extern "C" fn pc_fish_time_no(hour: u8) -> u8 {
    fish_time_no(hour)
}

/// C ABI: half-month term for month/day.
#[no_mangle]
pub extern "C" fn pc_fish_term(month: u8, day: u8) -> u8 {
    fish_term(month, day)
}

/// C ABI: field-rank spawn modifier * 1000.
#[no_mangle]
pub extern "C" fn pc_field_rank_rate_milli(rank: u8) -> u32 {
    (field_rank_rate(rank) * 1000.0) as u32
}

/// C ABI: 1 if (x,z) is in the 12x12 interior.
#[no_mangle]
pub extern "C" fn pc_interior_unit_ok(x: u8, z: u8) -> u8 {
    interior_unit_ok(x, z) as u8
}

/// C ABI: net sweep length * 10.
#[no_mangle]
pub extern "C" fn pc_net_sweep_len(golden: u8) -> u32 {
    (net_sweep_len(golden != 0) * 10.0) as u32
}

/// C ABI: net capture envelope * 10.
#[no_mangle]
pub extern "C" fn pc_net_capture_len(golden: u8, radius: f32) -> u32 {
    (net_capture_len(golden != 0, radius) * 10.0) as u32
}

/// C ABI: multi-birth count.
#[no_mangle]
pub extern "C" fn pc_insect_birth_count(is_multi: u8, rand3: u8) -> u8 {
    insect_birth_count(is_multi != 0, rand3)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_and_terms() {
        assert_eq!(fish_time_no(0), 0);
        assert_eq!(fish_time_no(3), 0);
        assert_eq!(fish_time_no(4), 1);
        assert_eq!(fish_time_no(8), 1);
        assert_eq!(fish_time_no(9), 2);
        assert_eq!(fish_time_no(15), 2);
        assert_eq!(fish_time_no(16), 3);
        assert_eq!(fish_time_no(20), 3);
        assert_eq!(fish_time_no(21), 0); // no branch matches; keeps init TIME_0
        assert_eq!(fish_time_no(23), 0);
    }

    #[test]
    fn terms_and_rates() {
        assert_eq!(fish_term(1, 1), 0);
        assert_eq!(fish_term(1, 16), 1);
        assert_eq!(fish_term(12, 31), 23);
        assert_eq!(term_transition_rate(5), 5.0 / 6.0);
        assert_eq!(term_transition_rate(1), 1.0 / 6.0);
        assert_eq!(field_rank_rate(0), 0.5);
        assert_eq!(field_rank_rate(2), 0.875);
        assert_eq!(field_rank_rate(5), 1.0);
    }

    #[test]
    fn fish_selection() {
        // Two candidates, equal weight, env 1.0.
        let w = [(10.0, true), (10.0, true)];
        assert_eq!(fish_select(&w, 1.0, 0.0), Some(0));
        assert_eq!(fish_select(&w, 1.0, 0.99), Some(1));
        // Habitat-incompatible first candidate is skipped via retry.
        let w2 = [(10.0, false), (10.0, true)];
        assert_eq!(fish_select(&w2, 1.0, 0.0), Some(1));
        // All incompatible -> None.
        let w3 = [(10.0, false)];
        assert_eq!(fish_select(&w3, 1.0, 0.5), None);
        // Interior.
        assert!(interior_unit_ok(2, 2));
        assert!(interior_unit_ok(13, 13));
        assert!(!interior_unit_ok(1, 5));
        assert!(!interior_unit_ok(14, 14));
        // Bite timing.
        assert_eq!(BITE_TIMING[0], 26);
        assert_eq!(BITE_TIMING[4], 52);
        assert_eq!(CAST_FRAMES, 50);
    }

    #[test]
    fn insect_selection() {
        // Total 60 <= 100: roll 0.7 -> 70 > 60 -> no spawn.
        let w = [30.0, 30.0];
        assert_eq!(insect_select(&w, 0.5), Some(0));
        assert_eq!(insect_select(&w, 0.7), None); // no-spawn mass
        // Total > 100: normalized.
        let w2 = [60.0, 60.0];
        assert_eq!(insect_select(&w2, 0.9), Some(1));
        // Birth counts.
        assert_eq!(insect_birth_count(false, 2), 1);
        assert_eq!(insect_birth_count(true, 0), 6);
        assert_eq!(insect_birth_count(true, 2), 8);
        assert_eq!(INSECT_SPAWN_SLOTS, 8);
    }

    #[test]
    fn net_geometry() {
        assert_eq!(net_sweep_len(false), 50.0);
        assert_eq!(net_sweep_len(true), 60.0);
        assert_eq!(net_capture_len(false, 8.0), 23.0);
        assert_eq!(net_capture_len(true, 8.0), 29.0);
        // Candidate on the axis -> capture.
        let top = (0.0, 0.0, 0.0);
        let bot = (0.0, -50.0, 0.0);
        assert!(net_capture_test(top, bot, (0.0, -25.0, 0.0), 8.0, false));
        // Far off-axis -> no capture.
        assert!(!net_capture_test(top, bot, (0.0, -25.0, 100.0), 8.0, false));
        // Insertion order: first hit wins, not nearest.
        let reqs = [
            (0.0, -25.0, 5.0, 8.0),  // hits
            (0.0, -25.0, 1.0, 8.0),  // also hits, nearer
        ];
        assert_eq!(net_capture_pick(None, &reqs, top, bot, false), Some(0));
        // Force takes precedence.
        assert_eq!(net_capture_pick(Some(1), &reqs, top, bot, false), Some(1));
        assert_eq!(NET_CATCH_TABLE_COUNT, 8);
        assert_eq!(NET_CAPTURE_FRAME_MIN, 6.0);
        // C ABI.
        assert_eq!(pc_fish_time_no(10), 2);
        assert_eq!(pc_fish_term(6, 20), 11);
        assert_eq!(pc_field_rank_rate_milli(0), 500);
        assert_eq!(pc_interior_unit_ok(5, 5), 1);
        assert_eq!(pc_net_sweep_len(1), 600);
        assert_eq!(pc_insect_birth_count(1, 2), 8);
    }
}
