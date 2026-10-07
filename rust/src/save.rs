//! Save & storage system for the Rust rewrite.
//!
//! Source-verified architecture (upstream `include/m_card.h`,
//! `src/game/m_card.c`, `include/m_common_data.h`, `src/game/m_flashrom.c`,
//! `pc/src/pc_save_bswap.c`):
//!
//! * One memory card holds one town. The town file is `DobutsunomoriP_MURA`,
//!   57 blocks (`mCD_LAND_SAVE_SIZE` = 0x72000); with the 64-byte GCI header
//!   the file is 467,008 bytes (0x72040).
//! * The MURA file is subdivided into card sub-entries (`l_mcd_file_table`):
//!   misc, main save, main backup, mail region, original(design) region and
//!   diary region. The backup copy is the game's recovery path.
//! * `SAVE_T_REGIONS` maps the headline `Save_t` regions (148,128 bytes,
//!   offsets from `m_common_data.h`). Sizes are derived as the span to the
//!   next field, so they include alignment padding.
//! * The `keep_*` regions (saved letters, saved designs, diaries) are
//!   separate card regions keyed by land ID -- this is why saved letters and
//!   patterns can survive an in-game town rebuild while the world state is
//!   reinitialized.
//! * Integrity: 16-bit additive checksum over big-endian u16 words
//!   (`mFRm_ReturnCheckSum`); the stored field is the two's complement fixup
//!   (`mFRm_GetFlatCheckSum`) so the whole region sums to zero.
//! * The GameCube format is big-endian; the PC port byte-swaps `Save_t` on
//!   load/save (`pc_save_bswap.c`). The checksum helpers here operate on the
//!   on-card (big-endian) byte order.
//! * Travel data is a separate 3-block file (`DobutsunomoriP_PL_<n>`,
//!   `mCD_foreigner_c`: checksum + player record + removed villager +
//!   copy-protect); NES data is a separate 1-block file (contemporary
//!   documentation; not traced in the decomp card code).
//!
//! Rewrite-owned choices: the region table is a curated subset of the full
//! `Save_t` map; the reset model (`ResetPreserved`) encodes the keep-region
//! preservation semantics rather than the full init sequence; the "3 items
//! per storage furniture" rule is a contemporary-guide claim, not yet traced
//! to a decomp struct, and is labeled as such.

use std::slice;

/// One memory-card block / sector.
#[allow(dead_code)] // Public save-system API for the rewrite and future adapters.
pub const SECTOR_SIZE: usize = 0x2000;
/// GCI file header (game ID, company, banner/icon metadata).
#[allow(dead_code)] // Public save-system API for the rewrite and future adapters.
pub const GCI_HEADER_LEN: usize = 64;
/// Offset of save data inside the MURA GCI: comment + banner + icon.
#[allow(dead_code)] // Public save-system API for the rewrite and future adapters.
pub const SAVE_DATA_OFFSET: usize = 0x1440;
#[allow(dead_code)] // Public save-system API for the rewrite and future adapters.
pub const CARD_COMMENT_LEN: usize = 64;
#[allow(dead_code)] // Public save-system API for the rewrite and future adapters.
pub const CARD_BANNER_LEN: usize = 0xE00;
#[allow(dead_code)] // Public save-system API for the rewrite and future adapters.
pub const CARD_ICON_LEN: usize = 0x600;

/// Main town file name (`l_mCD_land_file_name`).
#[allow(dead_code)] // Public save-system API for the rewrite and future adapters.
pub const MAIN_FILE_NAME: &str = "DobutsunomoriP_MURA";
/// Dummy/backup town file name (`l_mCD_land_file_name_dummy`).
#[allow(dead_code)] // Public save-system API for the rewrite and future adapters.
pub const MAIN_FILE_DUMMY_NAME: &str = "DobutsunomoriP_MURA_d";
/// Travel file name prefix (`l_mCD_player_file_name`; index appended).
#[allow(dead_code)] // Public save-system API for the rewrite and future adapters.
pub const TRAVEL_FILE_PREFIX: &str = "DobutsunomoriP_PL_";
/// Bonus/gift letter file name prefix (`l_mCD_present_file_name`).
#[allow(dead_code)] // Public save-system API for the rewrite and future adapters.
pub const PRESENT_FILE_PREFIX: &str = "DobutsunomoriP_Omake_";

/// Main town save size: 57 blocks (`mCD_LAND_SAVE_SIZE`).
#[allow(dead_code)] // Public save-system API for the rewrite and future adapters.
pub const MAIN_SAVE_SIZE: usize = 0x72000;
#[allow(dead_code)] // Public save-system API for the rewrite and future adapters.
pub const MAIN_BLOCK_COUNT: usize = 57;
/// Full GCI length: 57 blocks plus the 64-byte GCI header.
#[allow(dead_code)] // Public save-system API for the rewrite and future adapters.
pub const GCI_TOTAL_SIZE: usize = 467008;
/// `sizeof(Save_t)` = 0x242A0.
#[allow(dead_code)] // Public save-system API for the rewrite and future adapters.
pub const SAVE_T_SIZE: usize = 148128;

/// Mail (saved-letter) card region size (`mCD_MAIL_SAVE_SIZE`).
#[allow(dead_code)] // Public save-system API for the rewrite and future adapters.
pub const MAIL_SAVE_SIZE: usize = 0xC000;
/// Saved-design card region size (`mCD_ORIGINAL_SAVE_SIZE`).
#[allow(dead_code)] // Public save-system API for the rewrite and future adapters.
pub const ORIGINAL_SAVE_SIZE: usize = 0xE000;
/// Diary card region size (`mCD_DIARY_SAVE_SIZE`).
#[allow(dead_code)] // Public save-system API for the rewrite and future adapters.
pub const DIARY_SAVE_SIZE: usize = 0xC000;
/// Bonus/gift letter file size (`mCD_PRESENT_SAVE_SIZE`).
#[allow(dead_code)] // Public save-system API for the rewrite and future adapters.
pub const PRESENT_SAVE_SIZE: usize = 0x2000;
/// Travel file size (`mCD_PLAYER_SAVE_SIZE`).
#[allow(dead_code)] // Public save-system API for the rewrite and future adapters.
pub const TRAVEL_SAVE_SIZE: usize = 0x6000;

/// Players per town (`PLAYER_NUM`).
#[allow(dead_code)] // Public save-system API for the rewrite and future adapters.
pub const PLAYER_NUM: usize = 4;
/// Travel slots (`FOREIGNER_NUM`).
#[allow(dead_code)] // Public save-system API for the rewrite and future adapters.
pub const FOREIGNER_NUM: usize = 1;
/// Pocket inventory slots (`mPr_POCKETS_SLOT_COUNT`).
#[allow(dead_code)] // Public save-system API for the rewrite and future adapters.
pub const POCKET_SLOTS: usize = 15;
/// Letters held in the inventory/mailbox (`mPr_INVENTORY_MAIL_COUNT`).
#[allow(dead_code)] // Public save-system API for the rewrite and future adapters.
pub const INVENTORY_MAIL_COUNT: usize = 10;
/// Saved-letter pages and letters per page (`mCD_KEEP_MAIL_*`).
#[allow(dead_code)] // Public save-system API for the rewrite and future adapters.
pub const KEEP_MAIL_PAGES: usize = 8;
#[allow(dead_code)] // Public save-system API for the rewrite and future adapters.
pub const KEEP_MAIL_PER_PAGE: usize = 20;
/// Town-wide saved letters: 8 pages x 20.
#[allow(dead_code)] // Public save-system API for the rewrite and future adapters.
pub const SAVED_LETTER_COUNT: usize = 160;
/// Saved-design pages and designs per page (`mCD_KEEP_ORIGINAL_*`).
#[allow(dead_code)] // Public save-system API for the rewrite and future adapters.
pub const KEEP_ORIGINAL_PAGES: usize = 8;
#[allow(dead_code)] // Public save-system API for the rewrite and future adapters.
pub const KEEP_ORIGINAL_PER_PAGE: usize = 12;
/// Town-wide saved patterns: 8 pages x 12.
#[allow(dead_code)] // Public save-system API for the rewrite and future adapters.
pub const SAVED_PATTERN_COUNT: usize = 96;
/// Personal pattern slots (`mPr_ORIGINAL_DESIGN_COUNT`).
#[allow(dead_code)] // Public save-system API for the rewrite and future adapters.
pub const PATTERNS_PER_PLAYER: usize = 8;
/// Diary months per player (`mCD_KEEP_DIARY_ENTRY_COUNT`).
#[allow(dead_code)] // Public save-system API for the rewrite and future adapters.
pub const DIARY_MONTHS: usize = 12;
/// Bonus/gift letters (`mCD_PRESENT_MAX`).
#[allow(dead_code)] // Public save-system API for the rewrite and future adapters.
pub const PRESENT_MAX: usize = 9;
/// Items per storage furniture unit. Contemporary-guide claim; not yet
/// traced to a decomp struct. House storage is per furniture unit, not a
/// shared global inventory.
#[allow(dead_code)] // Public save-system API for the rewrite and future adapters.
pub const FURNITURE_STORAGE_SLOTS: usize = 3;
/// NES save file: 1 block (contemporary documentation).
#[allow(dead_code)] // Public save-system API for the rewrite and future adapters.
pub const NES_BLOCK_COUNT: usize = 1;
/// Travel file: 3 blocks (contemporary documentation).
#[allow(dead_code)] // Public save-system API for the rewrite and future adapters.
pub const TRAVEL_BLOCK_COUNT: usize = 3;

/// Card sub-entry kinds in `l_mcd_file_table` order (`mCD_FILE_*`).
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(dead_code)] // Public save-system API for the rewrite and future adapters.
pub enum SaveFileKind {
    Misc = 0,
    Main = 1,
    MainBackup = 2,
    Mail = 3,
    Original = 4,
    Diary = 5,
    Present = 6,
    Player = 7,
}

#[allow(dead_code)] // Public save-system API for the rewrite and future adapters.
pub struct SaveFileEntry {
    pub kind: SaveFileKind,
    /// Size of the card file this entry lives in.
    pub file_size: usize,
    /// Size of this sub-entry.
    pub entry_size: usize,
    /// Whether the entry lives inside the main MURA file.
    pub in_main_file: bool,
}

/// Mirrors `l_mcd_file_table`: the MURA file holds the main save, its backup,
/// and the mail/original/diary regions; presents and travel are separate files.
#[allow(dead_code)] // Public save-system API for the rewrite and future adapters.
pub static SAVE_FILE_TABLE: [SaveFileEntry; 8] = [
    SaveFileEntry { kind: SaveFileKind::Misc,       file_size: MAIN_SAVE_SIZE,    entry_size: SAVE_T_SIZE,        in_main_file: true },
    SaveFileEntry { kind: SaveFileKind::Main,       file_size: MAIN_SAVE_SIZE,    entry_size: SAVE_T_SIZE,        in_main_file: true },
    SaveFileEntry { kind: SaveFileKind::MainBackup, file_size: MAIN_SAVE_SIZE,    entry_size: SAVE_T_SIZE,        in_main_file: true },
    SaveFileEntry { kind: SaveFileKind::Mail,       file_size: MAIN_SAVE_SIZE,    entry_size: MAIL_SAVE_SIZE,     in_main_file: true },
    SaveFileEntry { kind: SaveFileKind::Original,   file_size: MAIN_SAVE_SIZE,    entry_size: ORIGINAL_SAVE_SIZE, in_main_file: true },
    SaveFileEntry { kind: SaveFileKind::Diary,      file_size: MAIN_SAVE_SIZE,    entry_size: DIARY_SAVE_SIZE,    in_main_file: true },
    SaveFileEntry { kind: SaveFileKind::Present,    file_size: PRESENT_SAVE_SIZE, entry_size: PRESENT_SAVE_SIZE,  in_main_file: false },
    SaveFileEntry { kind: SaveFileKind::Player,     file_size: TRAVEL_SAVE_SIZE,  entry_size: TRAVEL_SAVE_SIZE,   in_main_file: false },
];

/// Headline `Save_t` regions: (name, byte offset, span including padding).
/// Offsets from `include/m_common_data.h`; spans derived to the next field.
#[allow(dead_code)] // Public save-system API for the rewrite and future adapters.
pub static SAVE_T_REGIONS: &[(&str, usize, usize)] = &[
    ("save_check", 0x000000, 0x000014),
    ("scene_no", 0x000014, 0x000004),
    ("now_npc_max", 0x000018, 0x000001),
    ("copy_protect", 0x00001A, 0x000002),
    ("private_data", 0x000020, 0x009100),
    ("land_info", 0x009120, 0x00000C),
    ("noticeboard", 0x00912C, 0x000BB8),
    ("homes", 0x009CE8, 0x009AC0),
    ("fg", 0x0137A8, 0x003C00),
    ("combi_table", 0x0173A8, 0x000090),
    ("animals", 0x017438, 0x008EF8),
    ("shop", 0x020340, 0x000140),
    ("kabu_price_schedule", 0x020480, 0x000018),
    ("fruit", 0x020688, 0x000002),
    ("post_office", 0x020694, 0x00083C),
    ("police_box", 0x020ED0, 0x000028),
    ("melody", 0x020F08, 0x000008),
    ("config", 0x020F10, 0x000004),
    ("renew_time", 0x020F14, 0x000004),
    ("station_type", 0x020F18, 0x000001),
    ("weather", 0x020F19, 0x000001),
    ("save_exist", 0x020F1A, 0x000001),
    ("deposit", 0x020F1C, 0x0003C0),
    ("museum_display", 0x0213A8, 0x00003F),
    ("bridge", 0x0213F0, 0x000010),
    ("needlework", 0x021400, 0x001100),
    ("time_delta", 0x022528, 0x000018),
    ("island", 0x022540, 0x001900),
    ("fishRecord", 0x023E68, 0x0000B8),
    ("good_field", 0x024178, 0x00000C),
    ("bg_tex_idx", 0x024184, 0x000001),
    ("town_day", 0x02418A, 0x000001),
    ("travel_hard_time", 0x024198, 0x000008),
];

/// Look up a `Save_t` region by name.
#[allow(dead_code)] // Public save-system API for the rewrite and future adapters.
pub fn save_t_region(name: &str) -> Option<(usize, usize)> {
    SAVE_T_REGIONS
        .iter()
        .find(|(region, _, _)| *region == name)
        .map(|(_, offset, span)| (*offset, *span))
}

/// 16-bit additive checksum over big-endian u16 words.
/// Mirrors `mFRm_ReturnCheckSum` (odd lengths sum to zero, as in the source).
#[allow(dead_code)] // Public save-system API for the rewrite and future adapters.
pub fn checksum_sum(data: &[u8]) -> u16 {
    if data.len() & 1 != 0 {
        return 0;
    }
    let mut sum: u16 = 0;
    for chunk in data.chunks_exact(2) {
        sum = sum.wrapping_add(u16::from_be_bytes([chunk[0], chunk[1]]));
    }
    sum
}

/// Checksum field value to store so the region verifies.
/// Mirrors `mFRm_GetFlatCheckSum`: two's complement of the sum excluding the
/// field's current value. `current` is the field value already present in
/// `data`; pass 0 for a fresh region.
#[allow(dead_code)] // Public save-system API for the rewrite and future adapters.
pub fn checksum_fixup(data: &[u8], current: u16) -> u16 {
    let sum = checksum_sum(data).wrapping_sub(current);
    (!sum).wrapping_add(1)
}

/// True when the region's big-endian u16 words sum to zero.
/// Mirrors the load-time `mFRm_ReturnCheckSum(...) == 0` validation.
#[allow(dead_code)] // Public save-system API for the rewrite and future adapters.
pub fn checksum_valid(data: &[u8]) -> bool {
    checksum_sum(data) == 0
}

/// Memory-card blocks needed for `bytes` of payload.
#[allow(dead_code)] // Public save-system API for the rewrite and future adapters.
pub fn blocks_for_bytes(bytes: usize) -> usize {
    bytes.div_ceil(SECTOR_SIZE)
}

/// GCI file length for a `blocks`-block card file (payload + GCI header).
#[allow(dead_code)] // Public save-system API for the rewrite and future adapters.
pub fn gci_len_for_blocks(blocks: usize) -> usize {
    blocks * SECTOR_SIZE + GCI_HEADER_LEN
}

/// Card regions preserved across an in-game town rebuild. The `keep_*`
/// regions (mail, designs, diaries) are separate land-ID-keyed card regions;
/// the world state in `Save_t` is reinitialized. Deleting the memory-card
/// entry instead removes everything.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(dead_code)] // Public save-system API for the rewrite and future adapters.
pub enum ResetPreserved {
    SavedLetters = 0,
    SavedDesigns = 1,
    Diaries = 2,
}

/// Which card regions an in-game town rebuild preserves.
#[allow(dead_code)] // Public save-system API for the rewrite and future adapters.
pub fn reset_preserved_regions() -> &'static [ResetPreserved] {
    &[
        ResetPreserved::SavedLetters,
        ResetPreserved::SavedDesigns,
        ResetPreserved::Diaries,
    ]
}

/// C ABI: additive checksum over big-endian u16 words. Returns 0 for null,
/// empty, or odd-length input, matching the source behavior.
#[no_mangle]
pub unsafe extern "C" fn pc_save_checksum(data: *const u8, len: usize) -> u16 {
    if data.is_null() || len == 0 {
        return 0;
    }
    // SAFETY: The C ABI requires a readable buffer of the stated length.
    let bytes = unsafe { slice::from_raw_parts(data, len) };
    checksum_sum(bytes)
}

/// C ABI: checksum field value to store. `current` is the field value already
/// present in the buffer (0 for a fresh region).
#[no_mangle]
pub unsafe extern "C" fn pc_save_checksum_fixup(data: *const u8, len: usize, current: u16) -> u16 {
    if data.is_null() || len == 0 {
        return 0;
    }
    // SAFETY: The C ABI requires a readable buffer of the stated length.
    let bytes = unsafe { slice::from_raw_parts(data, len) };
    checksum_fixup(bytes, current)
}

/// C ABI: 1 when the region verifies (words sum to zero), 0 otherwise.
#[no_mangle]
pub unsafe extern "C" fn pc_save_checksum_valid(data: *const u8, len: usize) -> i32 {
    if data.is_null() || len == 0 {
        return 0;
    }
    // SAFETY: The C ABI requires a readable buffer of the stated length.
    let bytes = unsafe { slice::from_raw_parts(data, len) };
    i32::from(checksum_valid(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_math_matches_documented_sizes() {
        assert_eq!(blocks_for_bytes(MAIN_SAVE_SIZE), MAIN_BLOCK_COUNT);
        assert_eq!(gci_len_for_blocks(MAIN_BLOCK_COUNT), GCI_TOTAL_SIZE);
        assert_eq!(SAVE_DATA_OFFSET, CARD_COMMENT_LEN + CARD_BANNER_LEN + CARD_ICON_LEN);
        assert_eq!(SAVED_LETTER_COUNT, KEEP_MAIL_PAGES * KEEP_MAIL_PER_PAGE);
        assert_eq!(SAVED_PATTERN_COUNT, KEEP_ORIGINAL_PAGES * KEEP_ORIGINAL_PER_PAGE);
    }

    #[test]
    fn checksum_roundtrip() {
        let mut region = vec![0x11u8; 64];
        // region[0..2] is the checksum field
        let fixup = checksum_fixup(&region, 0);
        region[0] = (fixup >> 8) as u8;
        region[1] = fixup as u8;
        assert!(checksum_valid(&region));
        region[10] ^= 0xFF;
        assert!(!checksum_valid(&region));
    }

    #[test]
    fn save_t_region_lookup() {
        let (off, _) = save_t_region("private_data").unwrap();
        assert_eq!(off, 0x20);
        let (off, _) = save_t_region("time_delta").unwrap();
        assert_eq!(off, 0x22528);
        assert!(save_t_region("nope").is_none());
    }
}
