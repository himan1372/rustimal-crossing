//! Step-3 pre-authored town bodies (10 x 70 block types).
//!
//! Extracted programmatically from l_mRF_step3_blocks* in
//! src/game/m_random_field_ovl.c (USA Rev. 0 decomp / PC port).
//! Each body is a complete 7x10 semantic acre layout, copied
//! wholesale into both cliff_blocks and river_blocks by
//! mRF_MakeBaseLandformStep3 (one mRF_GetRandom(10) call).
//! Values are mFM_BLOCK_TYPE_* ids.

/// Number of step-3 bodies.
pub const STEP3_COUNT: usize = 10;
/// Blocks per body (7x10).
pub const STEP3_LEN: usize = 70;

/// Step-3 body "3".
pub const STEP3_3: [u8; 70] = [
      5,   1,   0,   0,   0,   0,   8,
      9,  13,  12,  11,  12,  12,  10,
      2,  43,  44,  14,  18,  15,  62,
     61,  15,  22,  15,  16,  18,  62,
     61,  15,  26,  39,  39,  17,   4,
      2,  39,  28,  15,  15,  16,   4,
     80,  39,  40,  39,  39,  39,  81,
    101, 101, 101, 101, 101, 101, 101,
     83,  83,  83, 102,  98,  99, 102,
     83,  83,  83, 103, 103, 103, 103,
];

/// Step-3 body "7".
pub const STEP3_7: [u8; 70] = [
      5,   1,   0,   0,   0,   0,   8,
      9,  13,  12,  11,  12,  12,  10,
      2,  43,  44,  14,  39,  18,  62,
     61,  15,  22,  15,  15,  16,   4,
      2,  39,  40,  18,  15,  15,  62,
     61,  15,  22,  16,  39,  39,   4,
     80,  39,  40,  39,  39,  39,  81,
    101, 101, 101, 101, 101, 101, 101,
     83,  83,  83, 102,  98,  99, 102,
     83,  83,  83, 103, 103, 103, 103,
];

/// Step-3 body "7R".
pub const STEP3_7R: [u8; 70] = [
      5,   0,   0,   0,   0,   1,   8,
      9,  12,  12,  11,  12,  13,  10,
     61,  19,  39,  14,  46,  45,   4,
      2,  21,  15,  15,  22,  15,  62,
     61,  15,  15,  19,  40,  39,   4,
      2,  39,  39,  21,  22,  15,  62,
     80,  39,  39,  39,  40,  39,  81,
    101, 101, 101, 101, 101, 101, 101,
     83,  83,  83, 102,  98,  99, 102,
     83,  83,  83, 103, 103, 103, 103,
];

/// Step-3 body "8".
pub const STEP3_8: [u8; 70] = [
      5,   0,   0,   0,   0,   1,   8,
      9,  12,  12,  11,  12,  13,  10,
     61,  15,  19,  14,  46,  45,   4,
      2,  39,  21,  15,  26,  18,  62,
     61,  15,  15,  19,  28,  16,   4,
      2,  39,  39,  21,  22,  15,  62,
     80,  39,  39,  39,  40,  39,  81,
    101, 101, 101, 101, 101, 101, 101,
     83,  83,  83, 102,  98,  99, 102,
     83,  83,  83, 103, 103, 103, 103,
];

/// Step-3 body "B".
pub const STEP3_B: [u8; 70] = [
      5,   0,   0,   0,   1,   0,   8,
      9,  12,  12,  11,  13,  12,  10,
     61,  15,  19,  14,  40,  18,  62,
      2,  39,  21,  15,  22,  16,   4,
     61,  15,  19,  46,  45,  39,   4,
      2,  39,  21,  22,  15,  15,  62,
     80,  39,  39,  40,  39,  39,  81,
    101, 101, 101, 101, 101, 101, 101,
     83,  83,  83, 102,  98,  99, 102,
     83,  83,  83, 103, 103, 103, 103,
];

/// Step-3 body "BR".
pub const STEP3_BR: [u8; 70] = [
      5,   0,   1,   0,   0,   0,   8,
      9,  12,  13,  11,  12,  12,  10,
     61,  19,  40,  14,  18,  15,  62,
      2,  21,  22,  15,  16,  39,   4,
      2,  39,  43,  44,  18,  15,  62,
     61,  15,  15,  22,  16,  39,   4,
     80,  39,  39,  40,  39,  39,  81,
    101, 101, 101, 101, 101, 101, 101,
     83,  83,  83, 102,  98,  99, 102,
     83,  83,  83, 103, 103, 103, 103,
];

/// Step-3 body "E".
pub const STEP3_E: [u8; 70] = [
      5,   0,   1,   0,   0,   0,   8,
      9,  12,  13,  11,  12,  12,  10,
     61,  19,  40,  14,  18,  15,  62,
      2,  21,  22,  15,  16,  39,   4,
      2,  39,  43,  44,  18,  15,  62,
     61,  15,  15,  22,  16,  39,   4,
     80,  39,  39,  40,  39,  39,  81,
    101, 101, 101, 101, 101, 101, 101,
     83,  83,  83, 102,  98,  99, 102,
     83,  83,  83, 103, 103, 103, 103,
];

/// Step-3 body "ER".
pub const STEP3_ER: [u8; 70] = [
      5,   0,   0,   0,   1,   0,   8,
      9,  12,  12,  11,  13,  12,  10,
     61,  15,  19,  14,  40,  18,  62,
      2,  39,  21,  15,  22,  16,   4,
     61,  15,  19,  46,  45,  39,   4,
      2,  39,  21,  22,  15,  15,  62,
     80,  39,  39,  40,  39,  39,  81,
    101, 101, 101, 101, 101, 101, 101,
     83,  83,  83, 102,  98,  99, 102,
     83,  83,  83, 103, 103, 103, 103,
];

/// Step-3 body "F".
pub const STEP3_F: [u8; 70] = [
      5,   0,   1,   0,   0,   0,   8,
      9,  12,  13,  11,  12,  12,  10,
      2,  39,  40,  14,  18,  15,  62,
     61,  15,  22,  15,  16,  39,   4,
      2,  39,  43,  41,  44,  39,   4,
     61,  15,  15,  15,  22,  15,  62,
     80,  39,  39,  39,  40,  39,  81,
    101, 101, 101, 101, 101, 101, 101,
     83,  83,  83, 102,  98,  99, 102,
     83,  83,  83, 103, 103, 103, 103,
];

/// Step-3 body "FR".
pub const STEP3_FR: [u8; 70] = [
      5,   0,   0,   0,   1,   0,   8,
      9,  12,  12,  11,  13,  12,  10,
     61,  15,  19,  14,  40,  39,   4,
      2,  39,  21,  15,  22,  15,  62,
      2,  39,  46,  42,  45,  39,   4,
     61,  15,  22,  15,  15,  15,  62,
     80,  39,  40,  39,  39,  39,  81,
    101, 101, 101, 101, 101, 101, 101,
     83,  83,  83, 102,  98,  99, 102,
     83,  83,  83, 103, 103, 103, 103,
];

/// Selection table (l_mRF_step3_blockss), index 0..9.
pub const STEP3_BLOCKS: [&[u8; 70]; 10] = [
    &STEP3_3,
    &STEP3_7,
    &STEP3_7R,
    &STEP3_8,
    &STEP3_B,
    &STEP3_BR,
    &STEP3_E,
    &STEP3_ER,
    &STEP3_F,
    &STEP3_FR,
];

/// Step-3 base landform (mRF_MakeBaseLandformStep3): one RNG call picks a
/// body; the body is copied into BOTH cliff_blocks and river_blocks.
/// `rand10` must emulate mRF_GetRandom(10). Returns the selected index.
pub fn make_base_landform_step3(
    cliff_blocks: &mut [u8; 70],
    river_blocks: &mut [u8; 70],
    rand10: impl FnOnce(usize) -> usize,
) -> usize {
    let idx = rand10(STEP3_COUNT) % STEP3_COUNT;
    let src = STEP3_BLOCKS[idx];
    cliff_blocks.copy_from_slice(src);
    river_blocks.copy_from_slice(src);
    idx
}

// ---- C ABI ----

/// C ABI: step-3 body selection. Copies the chosen body into both
/// buffers (each 70 bytes). r10 = pre-rolled mRF_GetRandom(10) result.
/// Returns the body index used.
#[no_mangle]
pub extern "C" fn pc_step3_select(
    cliff_ptr: *mut u8,
    river_ptr: *mut u8,
    r10: usize,
) -> usize {
    if cliff_ptr.is_null() || river_ptr.is_null() {
        return usize::MAX;
    }
    let idx = r10 % STEP3_COUNT;
    let src = STEP3_BLOCKS[idx];
    unsafe {
        std::ptr::copy_nonoverlapping(src.as_ptr(), cliff_ptr, 70);
        std::ptr::copy_nonoverlapping(src.as_ptr(), river_ptr, 70);
    }
    idx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn step3_bodies() {
        assert_eq!(STEP3_COUNT, 10);
        assert_eq!(STEP3_LEN, 70);
        assert_eq!(STEP3_BLOCKS.len(), 10);
        // All bodies are 70 entries; player house at (3,2) in body 3.
        assert_eq!(STEP3_3[2 * 7 + 3], 14); // PLAYER_HOUSE = 14
        // Row 7 is all SEA_EXCEPTIONAL (101).
        assert!(STEP3_3[7 * 7..8 * 7].iter().all(|&b| b == 101));
        // Selection copies into both buffers; single RNG call.
        let mut cliff = [0u8; 70];
        let mut river = [0u8; 70];
        let mut calls = 0;
        let idx = make_base_landform_step3(&mut cliff, &mut river, |_| {
            calls += 1;
            4
        });
        assert_eq!(calls, 1);
        assert_eq!(idx, 4);
        assert_eq!(cliff, *STEP3_BLOCKS[4]);
        assert_eq!(river, *STEP3_BLOCKS[4]);
        // Bodies are distinct objects (R variants are separate literals).
        assert_ne!(STEP3_7, STEP3_7R);
        // C ABI.
        let mut c2 = [0u8; 70];
        let mut r2 = [0u8; 70];
        assert_eq!(pc_step3_select(c2.as_mut_ptr(), r2.as_mut_ptr(), 9), 9);
        assert_eq!(c2, *STEP3_BLOCKS[9]);
        assert_eq!(r2, *STEP3_BLOCKS[9]);
    }
}
