//! GBA Island data kernels, verified against `m_island.c`
//! (GAFE01_00 Rev 0 USA decomp).
//!
//! Only the pure conversion/table/decision logic lives here. Stateful work
//! stays in C: the `Island_c` / `Island_agb_c` struct copies (`mISL_KeepIsland`,
//! `mISL_RestoreIsland`, `mISL_ChangeBG`), ARAM texture fetches
//! (`mISL_get_npc_tex/pal`, `mISL_get_earth_tex`), the island-block lookups
//! (`mFI_GetIslandBlockNumX`), and the full `mISL_gc_to_agb` / `mISL_agb_to_gc`
//! struct conversions (they need the exact C struct layouts, the save pointer,
//! `zelda_malloc`, and the weather state).
//!
//! ## Quirks preserved (documented here; the struct conversions stay in C)
//!
//! - `mISL_agb_to_gc_fllot_bit`: in the retail (non-`BUGFIXES`) build the
//!   restore is `gc->wall_original = (u32)gc->wall_original;` — a no-op
//!   self-assignment. The wall/floor "original" bits are therefore NEVER
//!   restored from AGB data. Any C reimplementation must keep the no-op.
//! - `mISL_gc_to_agb_memory`: `saved_town_tune` is NOT written to the AGB
//!   side (the `mISL_u64` call is `#ifdef BUGFIXES`'d out), but
//!   `mISL_agb_to_gc_memory` DOES restore it. So the town tune survives a
//!   GC->AGB->GC round trip only via whatever was already in the AGB slot.
//! - `mISL_get_npc_idx` returns 0 both when the NPC is not an islander and
//!   when it is the islander at table index 0 (Bliss). Callers cannot
//!   distinguish the two.
//! - `mISL_toHole`: a hole number outside [0, 25) is clamped to 0, and the
//!   deposit bit is cleared only when a pitfall was actually converted.

// ---------------------------------------------------------------------------
// Constants from m_name_table.h / m_field_make.h (verified values)
// ---------------------------------------------------------------------------

/// NPC_START + offsets for the 18 islanders, in `l_misl_agb_npc_table` order.
pub const ISLANDER_NPC_TABLE: [u16; 18] = [
    0xE0DA, // NPC_BLISS
    0xE0E0, // NPC_DOBIE
    0xE0D9, // NPC_OHARE
    0xE0DB, // NPC_DRIFT
    0xE0E7, // NPC_JUNE
    0xE0E4, // NPC_FAITH
    0xE0E1, // NPC_FLOSSIE
    0xE0DF, // NPC_FLASH
    0xE0D8, // NPC_MAELLE
    0xE0E2, // NPC_ANNALISE
    0xE0DC, // NPC_BUD
    0xE0DE, // NPC_ELINA
    0xE0DD, // NPC_BOOMER
    0xE0E9, // NPC_PIGLEG
    0xE0E5, // NPC_YODEL
    0xE0E3, // NPC_PLUCKY
    0xE0E6, // NPC_ROWAN
    0xE0EA, // NPC_ANKHA
];

/// `ITEM_NAME_GET_TYPE(n) == NAME_TYPE_NPC` gate used by `mISL_get_npc_idx`.
/// `ITEM_NAME_GET_TYPE(n) = ((n) & 0xF000) >> 12`; `NAME_TYPE_NPC` is enum
/// index 14.
pub fn pc_misl_is_npc_name(npc_id: u16) -> bool {
    ((npc_id & 0xF000) >> 12) == 14
}

/// `l_misl_count_max_table[32]` — per-action repeat thresholds.
pub const ACTION_COUNT_MAX_TABLE: [u8; 32] = [
    10, 4, 1, 10, 1, 5, 3, 10, 5, 1, 1, 1, 1, 1, 1, 1,
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
];

pub const ITM_PITFALL: u16 = 0x2512;
pub const HOLE_START: u16 = 0x0011;
pub const HOLE_END: u16 = 0x0029;
pub const EMPTY_NO: u16 = 0x0000;
pub const BURIED_PITFALL_HOLE_START: u16 = 0x002A;
pub const FLAG_ITEM: u16 = 0x584E;
pub const BOAT_ITEM: u16 = 0x584F;
pub const COTTAGE_MY: u16 = 0x5850;
pub const COTTAGE_NPC: u16 = 0x5851;
pub const VILLAGER_SIGNBOARD: u16 = 0xA012;

/// Unit (tile) counts per block, `UT_X_NUM` / `UT_Z_NUM` (`UT_BASE_NUM` = 16).
pub const UT_X_NUM: usize = 16;
pub const UT_Z_NUM: usize = 16;

// ---------------------------------------------------------------------------
// Endian swaps (`mISL_int`, `mISL_short`, `mISL_u64`)
// ---------------------------------------------------------------------------

/// Byte-swap `n` u32s. `mISL_int` writes the swaps with explicit shifts;
/// `u32::swap_bytes` is bit-identical.
pub fn pc_misl_bswap32_slice(dst: &mut [u32], src: &[u32]) {
    let n = dst.len().min(src.len());
    for i in 0..n {
        dst[i] = src[i].swap_bytes();
    }
}

/// Byte-swap `n` u16s (`mISL_short`).
pub fn pc_misl_bswap16_slice(dst: &mut [u16], src: &[u16]) {
    let n = dst.len().min(src.len());
    for i in 0..n {
        dst[i] = src[i].swap_bytes();
    }
}

/// `mISL_u64`: splits each u64 into hi/lo u32, byte-swaps each half, and
/// reassembles with the halves exchanged — i.e. a full 8-byte reversal,
/// identical to `u64::swap_bytes`.
pub fn pc_misl_bswap64_slice(dst: &mut [u64], src: &[u64]) {
    let n = dst.len().min(src.len());
    for i in 0..n {
        dst[i] = src[i].swap_bytes();
    }
}

// ---------------------------------------------------------------------------
// Checksums (`mISL_ReturnCheckSum`, `mISL_GetFlatCheckSum`)
// ---------------------------------------------------------------------------

/// Byte-sum checksum over `data` (`mISL_ReturnCheckSum`). The C accumulator is
/// a signed int; wrapping is used so huge inputs behave like the retail build.
pub fn pc_misl_return_checksum(data: &[u8]) -> i32 {
    let mut checksum: i32 = 0;
    for &b in data {
        checksum = checksum.wrapping_add(b as i32);
    }
    checksum
}

/// Checksum adjustment (`mISL_GetFlatCheckSum`): computes the value that flattens
/// the running checksum while ignoring one section (the landinfo, which "can
/// change" per the call-site comment in `mISL_gc_to_agb`).
///
/// C: `diff = (u8)(checksum - now_checksum) - ignored; diff = ~diff; diff += 1;`
/// i.e. the two's-complement negation of the low byte of
/// `checksum - now_checksum - ignored_section_checksum`.
pub fn pc_misl_get_flat_checksum(data: &[u8], now_checksum: u8, ignored_section_checksum: i32) -> u8 {
    let checksum = pc_misl_return_checksum(data);
    let diff = (checksum
        .wrapping_sub(now_checksum as i32)
        .wrapping_sub(ignored_section_checksum)) as u8;
    diff.wrapping_neg()
}

// ---------------------------------------------------------------------------
// Islander NPC index (`mISL_get_npc_idx`)
// ---------------------------------------------------------------------------

/// Index of `npc_id` in the islander table, or 0 when it is not a villager-NPC
/// name or not an islander (quirk: index 0 / Bliss is indistinguishable from
/// "not found").
pub fn pc_misl_get_npc_idx(npc_id: u16) -> usize {
    let mut res = 0usize;
    if pc_misl_is_npc_name(npc_id) {
        for (i, &id) in ISLANDER_NPC_TABLE.iter().enumerate() {
            if id == npc_id {
                res = i;
                break;
            }
        }
    }
    res
}

// ---------------------------------------------------------------------------
// Foreground item substitution (`mISL_gc_to_agb_fg2` per-item body)
// ---------------------------------------------------------------------------

/// Maps one GC foreground item id to its AGB-side equivalent. Verbatim order
/// from the decomp: cottage/flag/boat/special ids first, then the dug-hole
/// range, then the villager signboard; anything else passes through unchanged.
pub fn pc_misl_fg2_item(item: u16) -> u16 {
    if item == 0xF123 {
        COTTAGE_MY
    } else if item == 0xF124 {
        COTTAGE_NPC
    } else if item == 0xF122 {
        FLAG_ITEM
    } else if item == 0xF128 {
        BOAT_ITEM
    } else if item >= HOLE_START && item <= HOLE_END {
        EMPTY_NO
    } else if item == 0xF11F {
        VILLAGER_SIGNBOARD
    } else {
        item
    }
}

// ---------------------------------------------------------------------------
// Player action counters (`mISL_SetPlayerAction` loop body)
// ---------------------------------------------------------------------------

/// Applies one `mISL_SetPlayerAction` step: for each of the 32 action bits set
/// in `action`, the per-bit counter increments; any counter reaching its
/// `ACTION_COUNT_MAX_TABLE` threshold sets the corresponding bit in `check`
/// and clamps back to the threshold. Note the threshold check runs for every
/// bit on every call, not only for bits set in `action` (verbatim).
pub fn pc_misl_player_action_step(count_table: &mut [u8; 32], check: &mut u32, action: u32) {
    for i in 0..32 {
        if ((action >> i) & 1) != 0 {
            count_table[i] = count_table[i].wrapping_add(1);
        }
        if ACTION_COUNT_MAX_TABLE[i] <= count_table[i] {
            *check |= 1u32 << i;
            count_table[i] = ACTION_COUNT_MAX_TABLE[i];
        }
    }
}

// ---------------------------------------------------------------------------
// Pitfall -> buried-hole conversion (`mISL_toHole`)
// ---------------------------------------------------------------------------

/// Converts buried pitfalls back to hole items after an AGB->GC transfer.
/// `fg` holds `UT_Z_NUM * UT_X_NUM` item ids in row-major order; `deposit`
/// holds one u16 row mask per row (bit `ut_x` set = buried here).
/// `hole_number(bx, bz, ut_x, ut_z)` stands in for `mCoBG_BnumUnum2HoleNumber`
/// (a background-collision lookup that stays in C); out-of-range results
/// (< 0 or >= 25) are clamped to 0, verbatim.
pub fn pc_misl_to_hole(
    fg: &mut [u16],
    deposit: &mut [u16],
    bx: i32,
    bz: i32,
    hole_number: fn(i32, i32, i32, i32) -> i32,
) {
    debug_assert_eq!(fg.len(), UT_Z_NUM * UT_X_NUM);
    debug_assert_eq!(deposit.len(), UT_Z_NUM);
    for ut_z in 0..UT_Z_NUM {
        for ut_x in 0..UT_X_NUM {
            let idx = ut_z * UT_X_NUM + ut_x;
            if fg[idx] == ITM_PITFALL && ((deposit[ut_z] >> ut_x) & 1) != 0 {
                let mut hole_no = hole_number(bx, bz, ut_x as i32, ut_z as i32);
                if hole_no < 0 || hole_no >= 25 {
                    hole_no = 0;
                }
                fg[idx] = BURIED_PITFALL_HOLE_START + hole_no as u16;
                deposit[ut_z] &= !(1u16 << ut_x);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bswap32_matches_c_shift_expression() {
        // C: ((v & 0xFF) << 24) | ((v & 0xFF00) << 8) | ((v >> 8) & 0xFF00) | ((v >> 24) & 0xFF)
        let src = [0x01020304u32, 0xAABBCCDDu32, 0x00000000u32, 0xFFFFFFFFu32];
        let mut dst = [0u32; 4];
        pc_misl_bswap32_slice(&mut dst, &src);
        assert_eq!(dst, [0x04030201, 0xDDCCBBAA, 0x00000000, 0xFFFFFFFF]);
    }

    #[test]
    fn bswap16_reverses_bytes() {
        let src = [0x1234u16, 0x00FFu16];
        let mut dst = [0u16; 2];
        pc_misl_bswap16_slice(&mut dst, &src);
        assert_eq!(dst, [0x3412, 0xFF00]);
    }

    #[test]
    fn bswap64_reverses_all_eight_bytes() {
        // C splits hi/lo, byte-swaps each, and exchanges the halves.
        let src = [0x0102030405060708u64];
        let mut dst = [0u64; 1];
        pc_misl_bswap64_slice(&mut dst, &src);
        assert_eq!(dst, [0x0807060504030201]);
    }

    #[test]
    fn return_checksum_sums_bytes() {
        assert_eq!(pc_misl_return_checksum(&[1, 2, 3, 250]), 256);
        assert_eq!(pc_misl_return_checksum(&[]), 0);
    }

    #[test]
    fn flat_checksum_negates_low_byte() {
        // diff = -((sum - now - ignored) as u8)
        let data = [10u8, 20, 30]; // sum = 60
        // (60 - 0 - 0) as u8 = 60 -> -60 mod 256 = 196
        assert_eq!(pc_misl_get_flat_checksum(&data, 0, 0), 196);
        // with now_checksum: (60 - 60 - 0) = 0 -> 0
        assert_eq!(pc_misl_get_flat_checksum(&data, 60, 0), 0);
        // ignored section: (60 - 0 - 10) = 50 -> -50 mod 256 = 206
        assert_eq!(pc_misl_get_flat_checksum(&data, 0, 10), 206);
        // wrapping: sum 60, now 200 -> (60-200) as u8 = 116 -> -116 mod 256 = 140
        assert_eq!(pc_misl_get_flat_checksum(&data, 200, 0), 140);
    }

    #[test]
    fn npc_idx_table_lookup() {
        assert_eq!(pc_misl_get_npc_idx(0xE0DA), 0); // BLISS at index 0
        assert_eq!(pc_misl_get_npc_idx(0xE0EA), 17); // ANKHA last
        assert_eq!(pc_misl_get_npc_idx(0xE0D8), 8); // MAELLE
        // non-NPC name -> 0
        assert_eq!(pc_misl_get_npc_idx(0x2512), 0);
        // NPC-range name that is not an islander -> 0
        assert_eq!(pc_misl_get_npc_idx(0xE0E8), 0); // CHERI, not an islander
    }

    #[test]
    fn is_npc_name_gate() {
        assert!(pc_misl_is_npc_name(0xE0DA));
        assert!(!pc_misl_is_npc_name(0xD000)); // high nibble D
        assert!(!pc_misl_is_npc_name(0x5850));
    }

    #[test]
    fn fg2_item_substitution() {
        assert_eq!(pc_misl_fg2_item(0xF123), COTTAGE_MY);
        assert_eq!(pc_misl_fg2_item(0xF124), COTTAGE_NPC);
        assert_eq!(pc_misl_fg2_item(0xF122), FLAG_ITEM);
        assert_eq!(pc_misl_fg2_item(0xF128), BOAT_ITEM);
        assert_eq!(pc_misl_fg2_item(HOLE_START), EMPTY_NO);
        assert_eq!(pc_misl_fg2_item(HOLE_END), EMPTY_NO);
        assert_eq!(pc_misl_fg2_item(0xF11F), VILLAGER_SIGNBOARD);
        // pass-through
        assert_eq!(pc_misl_fg2_item(0x1234), 0x1234);
        assert_eq!(pc_misl_fg2_item(0x0010), 0x0010); // just below HOLE_START
        assert_eq!(pc_misl_fg2_item(0x002A), 0x002A); // just above HOLE_END
    }

    #[test]
    fn player_action_threshold_sets_bit_and_clamps() {
        let mut counts = [0u8; 32];
        let mut check = 0u32;
        // bit 2 has threshold 1: a single set saturates it
        pc_misl_player_action_step(&mut counts, &mut check, 1 << 2);
        assert_eq!(check & (1 << 2), 1 << 2);
        assert_eq!(counts[2], 1);
        // further sets keep it clamped at the threshold
        pc_misl_player_action_step(&mut counts, &mut check, 1 << 2);
        assert_eq!(counts[2], 1);
        // bit 0 has threshold 10: nine sets do not trip it
        for _ in 0..9 {
            pc_misl_player_action_step(&mut counts, &mut check, 1 << 0);
        }
        assert_eq!(check & 1, 0);
        assert_eq!(counts[0], 9);
        pc_misl_player_action_step(&mut counts, &mut check, 1 << 0);
        assert_eq!(check & 1, 1);
        assert_eq!(counts[0], 10);
    }

    #[test]
    fn player_action_threshold_applies_without_new_bits() {
        // Verbatim: the threshold arm runs even when the action bit is clear.
        let mut counts = [0u8; 32];
        counts[5] = ACTION_COUNT_MAX_TABLE[5]; // pre-saturated
        let mut check = 0u32;
        pc_misl_player_action_step(&mut counts, &mut check, 0);
        assert_eq!(check & (1 << 5), 1 << 5);
    }

    #[test]
    fn to_hole_converts_pitfall_and_clears_bit() {
        let mut fg = [0u16; UT_Z_NUM * UT_X_NUM];
        let mut deposit = [0u16; UT_Z_NUM];
        fg[2 * UT_X_NUM + 3] = ITM_PITFALL;
        deposit[2] = 1 << 3;
        fg[5 * UT_X_NUM + 7] = ITM_PITFALL; // no deposit bit -> untouched
        pc_misl_to_hole(&mut fg, &mut deposit, 4, 11, |_bx, _bz, _x, _z| 7);
        assert_eq!(fg[2 * UT_X_NUM + 3], BURIED_PITFALL_HOLE_START + 7);
        assert_eq!(deposit[2] & (1 << 3), 0);
        assert_eq!(fg[5 * UT_X_NUM + 7], ITM_PITFALL);
    }

    #[test]
    fn to_hole_clamps_bad_hole_number_to_zero() {
        let mut fg = [0u16; UT_Z_NUM * UT_X_NUM];
        let mut deposit = [0u16; UT_Z_NUM];
        fg[0] = ITM_PITFALL;
        deposit[0] = 1;
        pc_misl_to_hole(&mut fg, &mut deposit, 4, 11, |_bx, _bz, _x, _z| 42);
        assert_eq!(fg[0], BURIED_PITFALL_HOLE_START);
        fg[0] = ITM_PITFALL;
        deposit[0] = 1;
        pc_misl_to_hole(&mut fg, &mut deposit, 4, 11, |_bx, _bz, _x, _z| -3);
        assert_eq!(fg[0], BURIED_PITFALL_HOLE_START);
    }

    #[test]
    fn to_hole_forwards_coordinates() {
        let mut fg = [0u16; UT_Z_NUM * UT_X_NUM];
        let mut deposit = [0u16; UT_Z_NUM];
        fg[1 * UT_X_NUM + 2] = ITM_PITFALL;
        deposit[1] = 1 << 2;
        // encode the received coordinates into the returned hole number
        // (a capturing closure can't coerce to a fn pointer)
        pc_misl_to_hole(&mut fg, &mut deposit, 9, 10, |bx, bz, x, z| {
            (bx - 9) + (bz - 10) + (x - 2) + (z - 1)
        });
        assert_eq!(fg[1 * UT_X_NUM + 2], BURIED_PITFALL_HOLE_START);
    }
}
