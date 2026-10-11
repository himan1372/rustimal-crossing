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


/// Simulation loop constants (`mEv_fishRecord_local`, m_fishrecord.c).
/// The half-hour candidate loop runs while the cursor is strictly
/// before 17:50:00; afterwards the caller's timestamp is forced to
/// 18:00:00 unconditionally (even when the NPC loses).
pub const SIM_END_H: i32 = 17;
pub const SIM_END_M: i32 = 50;
pub const SIM_FINAL_H: i32 = 18;
pub const SIM_FINAL_M: i32 = 0;

/// NPC identity land-name bytes (`mEv_fishRecord_local`): the fixed
/// untranslated Japanese town name (source comment: "Untranslated JP,
/// \u30af\u30cb\u30c3\u30c1"). Preserve the bytes; do not transliterate.
pub const NPC_LAND_NAME: [u8; 5] = [0x98, 0xA6, 0x8F, 0xA1, 0x20];

/// Prize-mail template base and paper (`mFR_GetFishPresentMail`).
pub const MAIL_TEMPLATE_BASE: i32 = 0x23E;
pub const MAIL_PAPER_DEEP_SEA: i32 = 15;

#[no_mangle]
pub extern "C" fn pc_fr_npc_beats_player(best_npc: i32, player: i32) -> i32 {
    // Strict comparison: a tie does NOT beat the player.
    (best_npc > player) as i32
}

/// The discarded `fqrand(); /* unused call to fqrand..? */` at the
/// start of `mEv_fishRecord_local` (m_fishrecord.c:163). The draw is
/// an observable RNG-state change even though its value feeds nothing.
/// Identity passthrough: the caller must produce one fqrand() sample
/// here (before the timestamp copy) and pass it through; the return
/// value is deliberately unused.
#[no_mangle]
pub extern "C" fn pc_fr_discarded_draw(draw: f32) -> f32 {
    draw
}

/// `mEv_fishRecord_local` loop condition: nonzero while the simulation
/// cursor is strictly before 17:50:00. (The C code calls
/// `lbRTC_IsOverTime(&endpoint, &cursor) == lbRTC_LESS` — the args are
/// inverted, i.e. cursor < endpoint.)
#[no_mangle]
pub extern "C" fn pc_fr_sim_continue(cursor_h: i32, cursor_m: i32) -> i32 {
    ((cursor_h * 60 + cursor_m) < (SIM_END_H * 60 + SIM_END_M)) as i32
}

/// Number of candidate generations from a start time in minutes
/// (30-minute steps while the cursor is before 17:50). 06:00 -> 24
/// (06:00 through 17:30).
#[no_mangle]
pub extern "C" fn pc_fr_sim_step_count(start_minutes: i32) -> i32 {
    let end = SIM_END_H * 60 + SIM_END_M;
    if start_minutes >= end {
        0
    } else {
        (end - 1 - start_minutes) / 30 + 1
    }
}

/// The hour used for the candidate at iteration `step` (0-based) of the
/// simulation: `mFR_make_NpcRecord(record_set_time->hour)` is called
/// before the cursor advances 30 minutes.
#[no_mangle]
pub extern "C" fn pc_fr_sim_hour(start_minutes: i32, step: i32) -> i32 {
    (start_minutes + step * 30) / 60
}

/// Chronological date comparison (newer-first ordering is decided by
/// the caller): nonzero when (y1,m1,d1) is strictly later than
/// (y2,m2,d2). The C code compares the packed `ymd_u.raw`, which is
/// chronological, so lexicographic (y,m,d) is equivalent.
#[no_mangle]
pub extern "C" fn pc_fr_date_after(
    y1: i32, m1: i32, d1: i32,
    y2: i32, m2: i32, d2: i32,
) -> i32 {
    ((y1, m1, d1) > (y2, m2, d2)) as i32
}

/// `mFR_sort_record` swap decision: the C condition is
/// `records[j].size != 0 && (records[j].size == 0 || date(i) > date(j))`;
/// the middle arm is redundant (verbatim quirk, kept as documentation).
/// Ascending selection sort — the OLDEST records end up at index 0
/// (this is why holder1's slot-zero winner read works).
#[no_mangle]
pub extern "C" fn pc_fr_sort_swap(
    j_size: i32,
    i_y: i32, i_m: i32, i_d: i32,
    j_y: i32, j_m: i32, j_d: i32,
) -> i32 {
    (j_size != 0 && (i_y, i_m, i_d) > (j_y, j_m, j_d)) as i32
}

/// `mFR_recycle_record`: index of the oldest-dated slot. Replacement
/// uses strict-OVER only, so ties keep the lower slot (slot 0 wins
/// ties). Dates arrive as five (y,m,d) triples.
#[no_mangle]
pub extern "C" fn pc_fr_recycle_index(
    years: *const i32,
    months: *const i32,
    days: *const i32,
) -> i32 {
    unsafe {
        let mut best = 0i32;
        for i in 1..5i32 {
            let bi = best as usize;
            let ii = i as usize;
            let b = (*years.add(bi), *months.add(bi), *days.add(bi));
            let c = (*years.add(ii), *months.add(ii), *days.add(ii));
            if c < b {
                best = i;
            }
        }
        best
    }
}

/// `mFR_get_free_record`: first slot with size == 0, in array order;
/// -1 when the table is full.
#[no_mangle]
pub extern "C" fn pc_fr_free_slot(sizes: *const i32) -> i32 {
    unsafe {
        for i in 0..5i32 {
            if *sizes.add(i as usize) == 0 {
                return i;
            }
        }
        -1
    }
}

/// Same-date record search (used by `mEv_fishRecord_set` on the
/// renewal date and by `mEv_fishRecord_holder` on y/m/d): first index
/// with an exact year/month/day match, else -1.
#[no_mangle]
pub extern "C" fn pc_fr_find_same_date(
    y: i32, m: i32, d: i32,
    years: *const i32,
    months: *const i32,
    days: *const i32,
) -> i32 {
    unsafe {
        for i in 0..5i32 {
            let ii = i as usize;
            if *years.add(ii) == y && *months.add(ii) == m && *days.add(ii) == d {
                return i;
            }
        }
        -1
    }
}

/// Prize-mail template (`mFR_GetFishPresentMail`):
/// `0x23E + (((record->time.day - 1) / 7) & 3)` — the week of the
/// month selects the contest mail template.
#[no_mangle]
pub extern "C" fn pc_fr_mail_template(day: i32) -> i32 {
    MAIL_TEMPLATE_BASE + (((day - 1) / 7) & 3)
}

/// Save-validation predicate (`sCck_CheckSaveData_fishRecord_NSW`):
/// nonzero when the record passes — negative sizes fail; a populated
/// record with an invalid date fails; cleared dates are fine for empty
/// slots. `date_ok` is the save checker's date verdict.
#[no_mangle]
pub extern "C" fn pc_fr_save_valid(size: i32, date_ok: i32) -> i32 {
    (size >= 0 && (size == 0 || date_ok != 0)) as i32
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

    #[test]
    fn sim_loop_semantics() {
        // Continues while strictly before 17:50.
        assert_eq!(pc_fr_sim_continue(6, 0), 1);
        assert_eq!(pc_fr_sim_continue(17, 30), 1);
        assert_eq!(pc_fr_sim_continue(17, 49), 1);
        assert_eq!(pc_fr_sim_continue(17, 50), 0);
        assert_eq!(pc_fr_sim_continue(18, 0), 0);
        // 06:00 -> 24 candidates (06:00..17:30).
        assert_eq!(pc_fr_sim_step_count(6 * 60), 24);
        assert_eq!(pc_fr_sim_step_count(17 * 60 + 30), 1);
        assert_eq!(pc_fr_sim_step_count(17 * 60 + 50), 0);
        // Candidate hours: first and last of a 06:00 run.
        assert_eq!(pc_fr_sim_hour(6 * 60, 0), 6);
        assert_eq!(pc_fr_sim_hour(6 * 60, 23), 17);
        // Strict winner: tie loses.
        assert_eq!(pc_fr_npc_beats_player(40, 40), 0);
        assert_eq!(pc_fr_npc_beats_player(41, 40), 1);
        assert_eq!(pc_fr_npc_beats_player(39, 40), 0);
        // Discarded draw passes through (value unused by design).
        assert_eq!(pc_fr_discarded_draw(0.7), 0.7);
    }

    #[test]
    fn table_kernels() {
        // Recycle: oldest wins; ties keep the lower slot.
        let y = [2026, 2026, 2025, 2026, 2026];
        let m = [6, 6, 6, 6, 6];
        let d = [10, 10, 1, 11, 9];
        assert_eq!(pc_fr_recycle_index(y.as_ptr(), m.as_ptr(), d.as_ptr()), 2);
        // Free slot: first fit.
        let sizes = [5, 0, 3, 0, 1];
        assert_eq!(pc_fr_free_slot(sizes.as_ptr()), 1);
        let full = [1, 2, 3, 4, 5];
        assert_eq!(pc_fr_free_slot(full.as_ptr()), -1);
        // Same-date search.
        assert_eq!(pc_fr_find_same_date(2026, 6, 10, y.as_ptr(), m.as_ptr(), d.as_ptr()), 0);
        assert_eq!(pc_fr_find_same_date(2026, 6, 12, y.as_ptr(), m.as_ptr(), d.as_ptr()), -1);
        // Sort swap: ascending (oldest to index 0); empty j never swaps.
        assert_eq!(pc_fr_sort_swap(3, 2026, 6, 10, 2026, 6, 9), 1);
        assert_eq!(pc_fr_sort_swap(3, 2026, 6, 9, 2026, 6, 10), 0);
        assert_eq!(pc_fr_sort_swap(0, 2026, 6, 10, 2026, 6, 9), 0);
        // Mail template weeks.
        assert_eq!(pc_fr_mail_template(1), 0x23E);
        assert_eq!(pc_fr_mail_template(7), 0x23E);
        assert_eq!(pc_fr_mail_template(8), 0x23F);
        assert_eq!(pc_fr_mail_template(15), 0x240);
        assert_eq!(pc_fr_mail_template(29), 0x23E); // wraps: (28/7)&3 = 0
        // Save validation.
        assert_eq!(pc_fr_save_valid(-1, 1), 0);
        assert_eq!(pc_fr_save_valid(0, 0), 1);
        assert_eq!(pc_fr_save_valid(42, 0), 0);
        assert_eq!(pc_fr_save_valid(42, 1), 1);
        // NPC land-name bytes are the fixed untranslated-JP sequence.
        assert_eq!(NPC_LAND_NAME, [0x98, 0xA6, 0x8F, 0xA1, 0x20]);
    }
}
