//! Fishing-contest records — Layer A: pure data and helpers.
//!
//! Verified against `src/game/m_fishrecord.c` and `include/m_fishrecord.h`
//! (GAFE01_00 Rev 0 USA, via public decomp).
//!
//! Scope: this is the fishing-CONTEST system (five dated size records, NPC
//! rival generation, winner simulation inputs). It is NOT the museum
//! collection tracker — that's `museum_display::collection`. The persistent
//! record slots, RTC scheduling, and reward mail stay in C.
//!
//! RNG note: the C code calls `fqrand()` / `RANDOM(n)` inline. The helpers
//! below take the sampled values as parameters so tests can reproduce the
//! exact sequence without a global RNG.

/// Fish size classes (`m_fishrecord.h` enum).
pub mod size_class {
    pub const SMALL: i32 = 0;
    pub const MEDIUM: i32 = 1;
    pub const LARGE: i32 = 2;
    pub const NUM: i32 = 3;
}

/// Number of contest record slots (`mFR_RECORD_NUM`).
pub const RECORD_NUM: usize = 5;

/// `mFR_fish_rndsize`: integer fish size in inches from a size class and one
/// `fqrand()` sample in [0, 1). The C code truncates the f32 division toward
/// zero for the positive values produced here.
///
/// - LARGE:  `(50.0 + 20.0 * rng * rng) / 2.54`
/// - MEDIUM: `(30.0 + 20.0 * rng) / 2.54`
/// - SMALL:  `(30.0 - 20.0 * rng * rng) / 2.54` (also the default arm)
pub fn fish_rndsize(fish_size: i32, rng_sample: f32) -> i32 {
    let inches = match fish_size {
        size_class::LARGE => (50.0 + 20.0 * rng_sample * rng_sample) / 2.54,
        size_class::MEDIUM => (30.0 + 20.0 * rng_sample) / 2.54,
        _ => (30.0 - 20.0 * rng_sample * rng_sample) / 2.54,
    };
    inches as i32
}

/// `mFR_make_NpcRecord` class selection (pure part).
///
/// - hour < 9: always SMALL.
/// - 9 <= hour < 15: `rand2` is `RANDOM(2)` → SMALL or MEDIUM.
/// - hour >= 15: `rand3` is `RANDOM(3)` → SMALL, MEDIUM, or LARGE.
/// Returns the size class; the caller then feeds it to `fish_rndsize` with
/// a fresh `fqrand()` sample (separate RNG call — order matters).
pub fn npc_size_class(hour: u8, rand2: i32, rand3: i32) -> i32 {
    if hour < 9 {
        size_class::SMALL
    } else if hour < 15 {
        rand2 // 0 = SMALL, 1 = MEDIUM
    } else {
        rand3 // 0 = SMALL, 1 = MEDIUM, 2 = LARGE
    }
}

// ---- C ABI ----
// All pc_-prefixed: the C originals stay compiled; these are staged kernels.

#[no_mangle]
pub extern "C" fn pc_fr_fish_rndsize(fish_size: i32, rng_sample: f32) -> i32 {
    fish_rndsize(fish_size, rng_sample)
}

#[no_mangle]
pub extern "C" fn pc_fr_npc_size_class(hour: u8, rand2: i32, rand3: i32) -> i32 {
    npc_size_class(hour, rand2, rand3)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rndsize_formulas() {
        // rng = 0.0: SMALL = 30/2.54, MEDIUM = 30/2.54, LARGE = 50/2.54.
        assert_eq!(fish_rndsize(size_class::SMALL, 0.0), (30.0f32 / 2.54) as i32);
        assert_eq!(fish_rndsize(size_class::MEDIUM, 0.0), (30.0f32 / 2.54) as i32);
        assert_eq!(fish_rndsize(size_class::LARGE, 0.0), (50.0f32 / 2.54) as i32);
        // rng = 1.0: SMALL = 10/2.54, MEDIUM = 50/2.54, LARGE = 70/2.54.
        assert_eq!(fish_rndsize(size_class::SMALL, 1.0), (10.0f32 / 2.54) as i32);
        assert_eq!(fish_rndsize(size_class::MEDIUM, 1.0), (50.0f32 / 2.54) as i32);
        assert_eq!(fish_rndsize(size_class::LARGE, 1.0), (70.0f32 / 2.54) as i32);
        // Unknown class falls into the SMALL (default) arm.
        assert_eq!(fish_rndsize(99, 0.5), fish_rndsize(size_class::SMALL, 0.5));
        // Truncation, not rounding: 30/2.54 = 11.81... → 11.
        assert_eq!(fish_rndsize(size_class::SMALL, 0.0), 11);
    }

    #[test]
    fn npc_class_by_hour() {
        // Before 09:00: always SMALL regardless of rand inputs.
        assert_eq!(npc_size_class(0, 1, 2), size_class::SMALL);
        assert_eq!(npc_size_class(8, 1, 2), size_class::SMALL);
        // 09:00–14:59: uses RANDOM(2).
        assert_eq!(npc_size_class(9, 0, 2), size_class::SMALL);
        assert_eq!(npc_size_class(14, 1, 0), size_class::MEDIUM);
        // 15:00+: uses RANDOM(3).
        assert_eq!(npc_size_class(15, 0, 0), size_class::SMALL);
        assert_eq!(npc_size_class(15, 0, 1), size_class::MEDIUM);
        assert_eq!(npc_size_class(23, 0, 2), size_class::LARGE);
    }

    #[test]
    fn c_abi_passthrough() {
        assert_eq!(pc_fr_fish_rndsize(0, 0.0), 11);
        assert_eq!(pc_fr_npc_size_class(10, 1, 0), 1);
    }
}
