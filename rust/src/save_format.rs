//! Animal Crossing (GAFE01_00 USA Rev. 0) land-file format, integrity, and
//! save/load selection logic.
//!
//! Source-verified against the retail decomp (`src/game/m_card.c`,
//! `src/game/m_flashrom.c`, `include/m_card.h`, `include/m_flashrom.h`,
//! `include/m_common_data.h`, `include/m_land.h`, `include/m_time.h`,
//! `src/game/m_time.c`, `src/game/m_lib.c`):
//!
//! * The town file on a GameCube memory card is `DobutsunomoriP_MURA`,
//!   `mCD_LAND_SAVE_SIZE` = 0x72000 bytes of game data. A `.gci` export adds
//!   the 0x40-byte directory entry in front (0x72040 total).
//! * Layout of the 0x72000-byte image: 0x26000 auxiliary region, then the
//!   primary `Save` slot (0x26000), then the backup `Save` slot (0x26000).
//! * `Save` is the sector-aligned union: `Save_t` (0x242A0) padded to
//!   0x26000. The checksum covers the whole 0x26000 slot, padding included.
//! * Integrity is a 16-bit additive checksum over big-endian u16 words
//!   (`mFRm_ReturnCheckSum`); the stored field is the two's-complement
//!   fixup (`mFRm_GetFlatCheckSum`) so a valid region sums to zero. This is
//!   NOT a CRC.
//! * Load order: card A main, card A backup, card B main, card B backup.
//!   Each candidate is validated as: GAFE/land identity, then checksum,
//!   then version (5 or 6 accepted; the writer emits 6). Loading the backup
//!   sets the "outdated" error info.
//! * Before saving, `mCD_check_broken_land` validates both copies; if
//!   exactly one is good it is rewritten over the broken one.
//! * Writes go through a 0x2000-sector compare: a sector is rewritten only
//!   when it differs from what's on the card (`mem_cmp` returns TRUE when
//!   buffers are *equal*, so `== 0` means "different, write it").
//! * `copy_protect` (Save_t+0x1A, range 1..=0xFFF0) is a card-association
//!   value, entirely separate from the checksum.
//!
//! The GameCube CARD layer itself (CARDReadAsync/CARDWriteAsync, the async
//! state machine, the noLand protect-code handshake) is hardware I/O and
//! stays behind the engine boundary; this module models every pure decision
//! the retail code makes on byte buffers. Endianness: the on-card format is
//! big-endian; all multi-byte fields are read/written big-endian here.
//!
//! Deviations and open items (see DOCUMENTATION.md): the exact 0x40-byte
//! GCI directory entry of a retail-exported file can't be verified without
//! an actual USA retail GCI; the banner/icon/comment bytes come from ROM
//! resources the decomp excludes, so `others` construction takes them from
//! a provider.

use super::save::{checksum_fixup, checksum_valid};

/// Size of the live town struct (`sizeof(Save_t)`).
pub const SAVE_T_SIZE: usize = 0x242A0;
/// Size of one on-card save slot (`sizeof(Save)`: Save_t padded to a sector
/// multiple).
pub const SAVE_SLOT_SIZE: usize = 0x26000;
/// Size of the whole town file (`mCD_LAND_SAVE_SIZE`).
pub const LAND_FILE_SIZE: usize = 0x72000;
/// GameCube memory-card sector size used for compare-before-write.
pub const SECTOR_SIZE: usize = 0x2000;
/// GCI directory-entry prefix length.
pub const GCI_PREFIX_LEN: usize = 0x40;

/// Offsets of the three regions inside the 0x72000-byte image.
pub const MISC_OFS: usize = 0x00000;
pub const MAIN_OFS: usize = 0x26000;
pub const BACKUP_OFS: usize = 0x4C000;

/// Save identity (`mFRm_SAVE_ID` = 'GAFE', big-endian).
pub const SAVE_ID: u32 = 0x4741_4645;
/// Version the writer emits (`mFRm_VERSION`).
pub const SAVE_VERSION: i32 = 6;
/// Older version the loader still accepts.
pub const SAVE_VERSION_OLD: i32 = 5;

/// `mFRm_chk_t` field offsets inside a save slot.
pub mod chk {
    pub const VERSION: usize = 0x00; // s32 BE
    pub const CODE: usize = 0x04; // u32 BE
    pub const LAND_ID: usize = 0x08; // u16 BE
    pub const TIME: usize = 0x0A; // lbRTC_time_c, 8 bytes
    pub const CHECKSUM: usize = 0x12; // u16 BE
    pub const SIZE: usize = 0x14;
}

/// Copy-protection field offset inside `Save_t` (`Save_t + 0x1A`).
///
/// Note: `m_common_data.h` names a second `copy_protect` at 0x028596, which
/// cannot compile as plain C (duplicate member) and is a decomp artifact;
/// the card-association check only ever reads the first 0x200 bytes of each
/// copy (`mCD_check_copyProtect`), so the operative field is the 0x1A one.
pub const COPY_PROTECT_OFS: usize = 0x1A;
/// `mLd_CHECK_LAND_ID`: `(id & 0xFF00) == mLd_BITMASK` with
/// `mLd_BITMASK = 0x3000`.
pub const LAND_ID_BITMASK: u16 = 0x3000;

/// Auxiliary region: `MemcardHeader_c` (comment 64 + banner 0xE00 + icon
/// 0x600) then a 0x20 pad, then the three keep blocks.
pub mod aux {
    pub const HEADER_LEN: usize = 0x1440;
    pub const MAIL_OFS: usize = 0x1440;
    pub const MAIL_SIZE: usize = 0xBAC0;
    pub const ORIGINAL_OFS: usize = 0xCF00;
    /// Keep-design block size. NOTE: the decomp's visible
    /// `mCD_keep_original_c` fields only sum to 0xCC68 (32-aligned 0xCC80),
    /// contradicting the struct's own `_CC80` "force size to 0xCCA0"
    /// comment — the decomp struct is missing 0x38 bytes somewhere. The
    /// 0xCCA0 value is kept because the struct's stated intent and an
    /// independent PC-port round-trip log both agree on 0xCCA0; treat the
    /// decomp field arithmetic, not the size, as suspect.
    pub const ORIGINAL_SIZE: usize = 0xCCA0;
    pub const DIARY_OFS: usize = 0x19BA0;
    pub const DIARY_SIZE: usize = 0xBA20;
    /// Checksum field offset inside each keep block (first u16, big-endian).
    pub const CHECKSUM_OFS: usize = 0;
}

/// Card filenames.
pub mod file {
    pub const LAND: &str = "DobutsunomoriP_MURA";
    pub const LAND_DUMMY: &str = "DobutsunomoriP_MURA_d";
    /// `CARD_ATTR_NO_MOVE | CARD_ATTR_NO_COPY`, applied to the dummy file at
    /// creation and to the land file at set-permission time.
    pub const ATTR_NO_MOVE: u8 = 0x01;
    pub const ATTR_NO_COPY: u8 = 0x02;
}

// ---------------------------------------------------------------------------
// Big-endian field access
// ---------------------------------------------------------------------------

fn be_u16(b: &[u8], ofs: usize) -> u16 {
    u16::from_be_bytes([b[ofs], b[ofs + 1]])
}

fn be_u32(b: &[u8], ofs: usize) -> u32 {
    u32::from_be_bytes([b[ofs], b[ofs + 1], b[ofs + 2], b[ofs + 3]])
}

fn be_i32(b: &[u8], ofs: usize) -> i32 {
    i32::from_be_bytes([b[ofs], b[ofs + 1], b[ofs + 2], b[ofs + 3]])
}

fn put_be_u16(b: &mut [u8], ofs: usize, v: u16) {
    b[ofs..ofs + 2].copy_from_slice(&v.to_be_bytes());
}

fn put_be_u32(b: &mut [u8], ofs: usize, v: u32) {
    b[ofs..ofs + 4].copy_from_slice(&v.to_be_bytes());
}

fn put_be_i32(b: &mut [u8], ofs: usize, v: i32) {
    b[ofs..ofs + 4].copy_from_slice(&v.to_be_bytes());
}

// ---------------------------------------------------------------------------
// Save-check header
// ---------------------------------------------------------------------------

/// Parsed `mFRm_chk_t` (first 0x14 bytes of a save slot).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SaveCheck {
    pub version: i32,
    pub code: u32,
    pub land_id: u16,
    pub time: [u8; 8],
    pub checksum: u16,
}

impl SaveCheck {
    pub fn parse(slot: &[u8]) -> SaveCheck {
        let mut time = [0u8; 8];
        time.copy_from_slice(&slot[chk::TIME..chk::TIME + 8]);
        SaveCheck {
            version: be_i32(slot, chk::VERSION),
            code: be_u32(slot, chk::CODE),
            land_id: be_u16(slot, chk::LAND_ID),
            time,
            checksum: be_u16(slot, chk::CHECKSUM),
        }
    }

    /// `mFRm_CheckSaveData_ID`: code == 'GAFE'.
    pub fn id_ok(&self) -> bool {
        self.code == SAVE_ID
    }

    /// `mLd_CHECK_LAND_ID`.
    pub fn land_id_ok(&self) -> bool {
        self.land_id & 0xFF00 == LAND_ID_BITMASK
    }

    /// `mFRm_CheckSaveData_common`: identity check the loader runs before
    /// the checksum. `land_id` is the candidate's own `land_info.id`
    /// (`save->land_info.id`), which the loader passes back in.
    pub fn common_ok(&self, land_id: u16) -> bool {
        self.id_ok() && self.land_id_ok() && self.land_id == land_id
    }

    /// Versions the loader accepts (`mCD_LoadLand`).
    pub fn version_ok(&self) -> bool {
        self.version == SAVE_VERSION || self.version == SAVE_VERSION_OLD
    }
}

/// Full retail candidate validation in retail order: identity, then
/// checksum over the whole 0x26000 slot, then version (`mCD_LoadLand`).
pub fn candidate_valid(slot: &[u8; SAVE_SLOT_SIZE], land_id: u16) -> bool {
    let hdr = SaveCheck::parse(slot);
    if !hdr.common_ok(land_id) {
        return false;
    }
    if !checksum_valid(slot) {
        return false;
    }
    hdr.version_ok()
}

/// `mFRm_ClearSaveCheckData`: code = -1, land_id = 0xFFFF, time = RTC clear
/// code (all 0xFF), checksum = 0. Version is deliberately untouched (the
/// buffer is zeroed separately in practice).
pub fn clear_save_check(slot: &mut [u8; SAVE_SLOT_SIZE]) {
    put_be_u32(slot, chk::CODE, 0xFFFF_FFFF);
    put_be_u16(slot, chk::LAND_ID, 0xFFFF);
    slot[chk::TIME..chk::TIME + 8].fill(0xFF);
    put_be_u16(slot, chk::CHECKSUM, 0);
}

/// Metadata stamped at save time (`mFRm_SetSaveCheckData` + caller).
pub struct SaveMeta {
    pub version: i32,
    pub land_id: u16,
    /// 8-byte `lbRTC_time_c` (big-endian wire bytes).
    pub rtc_time: [u8; 8],
    pub copy_protect: u16,
}

/// Build a serialized save slot exactly like `mCD_SaveHome_bg_set_data`:
/// zero the 0x26000 buffer, copy the live `Save_t` (0x242A0), stamp
/// version/code/land_id/RTC time, then store the flat checksum over the whole
/// slot.
///
/// Retail applies the save-time mutations (save_exist, copy_protect,
/// travel_hard_time, reset-code clearing, money-stone shine, Wisp removal)
/// to the *live* save via `Save_Set`/`Common_Set` *before* the `bcopy`;
/// only version/save-check/checksum are set post-copy. `meta.copy_protect`
/// is written here for convenience — the resulting bytes are identical to
/// retail's pre-copy application. Any other live-save mutations must
/// already be reflected in `live` by the engine.
pub fn build_save_slot(live: &[u8; SAVE_T_SIZE], meta: &SaveMeta) -> [u8; SAVE_SLOT_SIZE] {
    let mut slot = [0u8; SAVE_SLOT_SIZE];
    slot[..SAVE_T_SIZE].copy_from_slice(live);
    put_be_i32(&mut slot, chk::VERSION, meta.version);
    put_be_u32(&mut slot, chk::CODE, SAVE_ID);
    put_be_u16(&mut slot, chk::LAND_ID, meta.land_id);
    slot[chk::TIME..chk::TIME + 8].copy_from_slice(&meta.rtc_time);
    put_be_u16(&mut slot, COPY_PROTECT_OFS, meta.copy_protect);
    let current = be_u16(&slot, chk::CHECKSUM);
    let fixup = checksum_fixup(&slot, current);
    put_be_u16(&mut slot, chk::CHECKSUM, fixup);
    slot
}

/// `mCD_get_land_copyProtect`: `RANDOM(0xFFF0)` then `code++`, i.e. the
/// range 1..=0xFFF0. `f` is `fqrand()` in [0, 1).
pub fn new_copy_protect(f: f32) -> u16 {
    ((f * 0xFFF0 as f32) as u16).wrapping_add(1)
}

/// Card-association check (`mCD_check_copyProtect`): the stored value must
/// equal the runtime value. Independent of the checksum.
pub fn copy_protect_ok(stored: u16, runtime: u16) -> bool {
    stored == runtime
}

// ---------------------------------------------------------------------------
// Load selection: main vs backup, card A vs card B
// ---------------------------------------------------------------------------

/// Which on-card copy validated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveSlot {
    Main,
    Backup,
}

/// Result of scanning one card's two copies in retail order
/// (`mCD_LoadLand`): main first, then backup. `outdated` is true when the
/// backup was the copy that validated (retail raises
/// `mCD_ERROR_OUTDATED`, "loaded backup").
pub fn select_slot(main: &[u8; SAVE_SLOT_SIZE], backup: &[u8; SAVE_SLOT_SIZE], land_id: u16) -> Option<(SaveSlot, bool)> {
    if candidate_valid(main, land_id) {
        return Some((SaveSlot::Main, false));
    }
    if candidate_valid(backup, land_id) {
        return Some((SaveSlot::Backup, true));
    }
    None
}

// ---------------------------------------------------------------------------
// Broken-land check and repair
// ---------------------------------------------------------------------------

/// Outcome of `mCD_check_broken_land`: each copy is classified good/bad;
/// repair is requested only when exactly one copy is good and the other is
/// bad (both bad, or both good, means no repair).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BrokenLand {
    pub ok: Option<SaveSlot>,
    pub broken: Option<SaveSlot>,
}

impl BrokenLand {
    pub fn needs_repair(&self) -> bool {
        self.ok.is_some() && self.broken.is_some()
    }
}

pub fn check_broken_land(
    main: &[u8; SAVE_SLOT_SIZE],
    backup: &[u8; SAVE_SLOT_SIZE],
    land_id: u16,
) -> BrokenLand {
    let mut ok = None;
    let mut broken = None;
    for (slot, bytes) in [(SaveSlot::Main, main), (SaveSlot::Backup, backup)] {
        // Retail order: checksum first, then the identity check.
        if checksum_valid(bytes) && SaveCheck::parse(bytes).common_ok(land_id) {
            ok = Some(slot);
        } else {
            broken = Some(slot);
        }
    }
    BrokenLand { ok, broken }
}

/// `mCD_repair_land`: rewrite the broken copy from the surviving good one.
/// Returns the repaired image, or `None` when there is nothing to repair.
pub fn repair_land(image: &[u8; LAND_FILE_SIZE], land_id: u16) -> Option<Box<[u8; LAND_FILE_SIZE]>> {
    let main: &[u8; SAVE_SLOT_SIZE] = image[MAIN_OFS..MAIN_OFS + SAVE_SLOT_SIZE].try_into().ok()?;
    let backup: &[u8; SAVE_SLOT_SIZE] = image[BACKUP_OFS..BACKUP_OFS + SAVE_SLOT_SIZE].try_into().ok()?;
    let st = check_broken_land(main, backup, land_id);
    if !st.needs_repair() {
        return None;
    }
    // Boxed: a 0x72000-byte image does not belong on the caller's stack.
    let mut out = Box::new(*image);
    match (st.ok, st.broken) {
        (Some(SaveSlot::Main), Some(SaveSlot::Backup)) => {
            out[BACKUP_OFS..BACKUP_OFS + SAVE_SLOT_SIZE].copy_from_slice(main)
        }
        (Some(SaveSlot::Backup), Some(SaveSlot::Main)) => {
            out[MAIN_OFS..MAIN_OFS + SAVE_SLOT_SIZE].copy_from_slice(backup)
        }
        _ => return None,
    }
    Some(out)
}

// ---------------------------------------------------------------------------
// Sector-compare write plan (mCD_write_comp_bg core decision)
// ---------------------------------------------------------------------------

/// Indices of the 0x2000-byte sectors that differ between the on-card
/// content and the new data. Retail reads each existing sector and rewrites
/// it only when it differs (`mem_cmp(...) == 0` → different → write);
/// identical sectors are skipped, which the decomp attributes to card
/// lifetime. `offset`/`len` bound the region being written, mirroring the
/// `offset`/`data_len` parameters.
pub fn sector_write_plan(old: &[u8], new: &[u8], offset: usize, len: usize) -> Vec<usize> {
    let mut sectors = Vec::new();
    let mut ofs = 0usize;
    while ofs < len {
        let chunk = (len - ofs).min(SECTOR_SIZE);
        let o = &old[offset + ofs..offset + ofs + chunk];
        let n = &new[ofs..ofs + chunk];
        if o != n {
            sectors.push(ofs / SECTOR_SIZE);
        }
        ofs += chunk;
    }
    sectors
}

// ---------------------------------------------------------------------------
// Auxiliary region
// ---------------------------------------------------------------------------

/// The three keep blocks in writer/reader order (mail, original, diary),
/// as `(name, offset, size)` inside the 0x26000 misc region.
pub fn others_regions() -> [(&'static str, usize, usize); 3] {
    [
        ("mail", aux::MAIL_OFS, aux::MAIL_SIZE),
        ("original", aux::ORIGINAL_OFS, aux::ORIGINAL_SIZE),
        ("diary", aux::DIARY_OFS, aux::DIARY_SIZE),
    ]
}

/// Validate one keep block: its flat checksum (field at offset 0) must sum
/// the whole block to zero.
pub fn aux_block_valid(block: &[u8]) -> bool {
    checksum_valid(block)
}

/// Stamp a keep block's checksum field (`mFRm_GetFlatCheckSum` over the
/// block with its current field value).
pub fn aux_set_checksum(block: &mut [u8]) {
    let current = be_u16(block, aux::CHECKSUM_OFS);
    let fixup = checksum_fixup(block, current);
    put_be_u16(block, aux::CHECKSUM_OFS, fixup);
}

/// `mCD_load_set_others_common`: forgiving per-block validation. A block
/// with a bad checksum is simply not loaded; the town is unaffected.
/// `diary` mirrors the `diary_flag` parameter (FALSE on the noLand path).
/// Returns `(mail_ok, original_ok, diary_ok)`.
pub fn load_others(image: &[u8; LAND_FILE_SIZE], diary: bool) -> (bool, bool, bool) {
    let mail = &image[aux::MAIL_OFS..aux::MAIL_OFS + aux::MAIL_SIZE];
    let original = &image[aux::ORIGINAL_OFS..aux::ORIGINAL_OFS + aux::ORIGINAL_SIZE];
    let diary_ok = if diary {
        let d = &image[aux::DIARY_OFS..aux::DIARY_OFS + aux::DIARY_SIZE];
        aux_block_valid(d)
    } else {
        false
    };
    (aux_block_valid(mail), aux_block_valid(original), diary_ok)
}

/// Assemble the 0x26000 misc region in writer order
/// (`mCD_SaveHome_bg_set_others`): comment/banner/icon header bytes come
/// from the provider (they are ROM resources in retail), then the three
/// keep blocks verbatim (checksums must already be stamped by the caller).
pub fn build_others(header: &[u8; aux::HEADER_LEN], mail: &[u8], original: &[u8], diary: &[u8]) -> [u8; SAVE_SLOT_SIZE] {
    let mut out = [0u8; SAVE_SLOT_SIZE];
    out[..aux::HEADER_LEN].copy_from_slice(header);
    out[aux::MAIL_OFS..aux::MAIL_OFS + mail.len().min(aux::MAIL_SIZE)]
        .copy_from_slice(&mail[..mail.len().min(aux::MAIL_SIZE)]);
    out[aux::ORIGINAL_OFS..aux::ORIGINAL_OFS + original.len().min(aux::ORIGINAL_SIZE)]
        .copy_from_slice(&original[..original.len().min(aux::ORIGINAL_SIZE)]);
    out[aux::DIARY_OFS..aux::DIARY_OFS + diary.len().min(aux::DIARY_SIZE)]
        .copy_from_slice(&diary[..diary.len().min(aux::DIARY_SIZE)]);
    out
}

/// Assemble a full 0x72000 land image: misc region + main + backup.
/// The normal save writes the *same* slot to both copies; use
/// `repair_land` afterwards only when recovering a broken copy.
pub fn build_land_image(
    misc: &[u8; SAVE_SLOT_SIZE],
    main: &[u8; SAVE_SLOT_SIZE],
    backup: &[u8; SAVE_SLOT_SIZE],
) -> Box<[u8; LAND_FILE_SIZE]> {
    // Boxed: a 0x72000-byte image does not belong on the caller's stack.
    let mut out = Box::new([0u8; LAND_FILE_SIZE]);
    out[MISC_OFS..MISC_OFS + SAVE_SLOT_SIZE].copy_from_slice(misc);
    out[MAIN_OFS..MAIN_OFS + SAVE_SLOT_SIZE].copy_from_slice(main);
    out[BACKUP_OFS..BACKUP_OFS + SAVE_SLOT_SIZE].copy_from_slice(backup);
    out
}

// ---------------------------------------------------------------------------
// C ABI
// ---------------------------------------------------------------------------

/// Validate one 0x26000 save slot. Returns 1 when the retail candidate
/// check (identity, checksum, version) passes for `land_id`.
#[no_mangle]
pub unsafe extern "C" fn pc_savef_slot_valid(slot: *const u8, land_id: u16) -> i32 {
    if slot.is_null() {
        return 0;
    }
    let bytes: &[u8; SAVE_SLOT_SIZE] = &*(slot as *const [u8; SAVE_SLOT_SIZE]);
    i32::from(candidate_valid(bytes, land_id))
}

/// Pick the winning slot of a main/backup pair: 0 = main, 1 = backup,
/// -1 = neither valid.
#[no_mangle]
pub unsafe extern "C" fn pc_savef_select_slot(
    main: *const u8,
    backup: *const u8,
    land_id: u16,
) -> i32 {
    if main.is_null() || backup.is_null() {
        return -1;
    }
    let m: &[u8; SAVE_SLOT_SIZE] = &*(main as *const [u8; SAVE_SLOT_SIZE]);
    let b: &[u8; SAVE_SLOT_SIZE] = &*(backup as *const [u8; SAVE_SLOT_SIZE]);
    match select_slot(m, b, land_id) {
        Some((SaveSlot::Main, _)) => 0,
        Some((SaveSlot::Backup, _)) => 1,
        None => -1,
    }
}

/// Repair decision for a main/backup pair: 0 = nothing to do, 1 = main is
/// good / backup broken, 2 = backup is good / main broken.
#[no_mangle]
pub unsafe extern "C" fn pc_savef_broken_land(
    main: *const u8,
    backup: *const u8,
    land_id: u16,
) -> i32 {
    if main.is_null() || backup.is_null() {
        return 0;
    }
    let m: &[u8; SAVE_SLOT_SIZE] = &*(main as *const [u8; SAVE_SLOT_SIZE]);
    let b: &[u8; SAVE_SLOT_SIZE] = &*(backup as *const [u8; SAVE_SLOT_SIZE]);
    match check_broken_land(m, b, land_id) {
        BrokenLand { ok: Some(SaveSlot::Main), broken: Some(SaveSlot::Backup) } => 1,
        BrokenLand { ok: Some(SaveSlot::Backup), broken: Some(SaveSlot::Main) } => 2,
        _ => 0,
    }
}

/// Validate one auxiliary keep block (flat checksum over the whole block).
#[no_mangle]
pub unsafe extern "C" fn pc_savef_aux_valid(block: *const u8, len: usize) -> i32 {
    if block.is_null() || len == 0 {
        return 0;
    }
    let bytes = std::slice::from_raw_parts(block, len);
    i32::from(aux_block_valid(bytes))
}

/// Stamp the checksum field (offset 0) of one auxiliary keep block.
#[no_mangle]
pub unsafe extern "C" fn pc_savef_aux_stamp(block: *mut u8, len: usize) {
    if block.is_null() || len < 2 {
        return;
    }
    let bytes = std::slice::from_raw_parts_mut(block, len);
    aux_set_checksum(bytes);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn land_id() -> u16 {
        0x3001
    }

    fn live_save_t() -> [u8; SAVE_T_SIZE] {
        let mut live = [0xABu8; SAVE_T_SIZE];
        // land_info.id lives deep in Save_t; the candidate check reads the
        // header's land_id against the caller-supplied id, so the live
        // image content here is arbitrary.
        live
    }

    fn meta() -> SaveMeta {
        SaveMeta {
            version: SAVE_VERSION,
            land_id: land_id(),
            rtc_time: [0x07, 0xE2, 0x0A, 0x08, 0x10, 0x20, 0x30, 0x00],
            copy_protect: 0x1234,
        }
    }

    fn good_slot() -> [u8; SAVE_SLOT_SIZE] {
        build_save_slot(&live_save_t(), &meta())
    }

    #[test]
    fn layout_math() {
        assert_eq!(MAIN_OFS, MISC_OFS + SAVE_SLOT_SIZE);
        assert_eq!(BACKUP_OFS, MAIN_OFS + SAVE_SLOT_SIZE);
        assert_eq!(BACKUP_OFS + SAVE_SLOT_SIZE, LAND_FILE_SIZE);
        assert_eq!(SAVE_SLOT_SIZE - SAVE_T_SIZE, 0x1D60);
        assert_eq!(aux::ORIGINAL_OFS, aux::MAIL_OFS + aux::MAIL_SIZE);
        assert_eq!(aux::DIARY_OFS, aux::ORIGINAL_OFS + aux::ORIGINAL_SIZE);
        assert_eq!(aux::DIARY_OFS + aux::DIARY_SIZE, 0x255C0);
    }

    #[test]
    fn built_slot_validates() {
        let slot = good_slot();
        assert!(candidate_valid(&slot, land_id()));
        // Whole 0x26000 slot sums to zero, padding included.
        assert!(checksum_valid(&slot));
        let hdr = SaveCheck::parse(&slot);
        assert_eq!(hdr.version, 6);
        assert_eq!(hdr.code, SAVE_ID);
        assert_eq!(hdr.land_id, land_id());
        assert_eq!(hdr.time, meta().rtc_time);
        assert_eq!(be_u16(&slot, COPY_PROTECT_OFS), 0x1234);
    }

    #[test]
    fn checksum_covers_padding() {
        let mut slot = good_slot();
        // Corrupt a byte in the padding region past Save_t.
        slot[SAVE_T_SIZE + 100] ^= 0xFF;
        assert!(!candidate_valid(&slot, land_id()));
    }

    #[test]
    fn version_5_loads_but_7_does_not() {
        let live = live_save_t();
        let mut m5 = meta();
        m5.version = 5;
        let s5 = build_save_slot(&live, &m5);
        assert!(candidate_valid(&s5, land_id()));
        let mut m7 = meta();
        m7.version = 7;
        let s7 = build_save_slot(&live, &m7);
        assert!(!candidate_valid(&s7, land_id()));
    }

    #[test]
    fn identity_checked_before_checksum() {
        let mut slot = good_slot();
        // Break the identity but keep a valid checksum: recompute the
        // checksum after zeroing the code field.
        put_be_u32(&mut slot, chk::CODE, 0);
        let current = be_u16(&slot, chk::CHECKSUM);
        let fixup = checksum_fixup(&slot, current);
        put_be_u16(&mut slot, chk::CHECKSUM, fixup);
        assert!(checksum_valid(&slot));
        assert!(!candidate_valid(&slot, land_id()));
    }

    #[test]
    fn select_prefers_main_and_flags_backup() {
        let good = good_slot();
        let mut bad = good;
        bad[0x100] ^= 0x01;
        assert_eq!(select_slot(&good, &bad, land_id()), Some((SaveSlot::Main, false)));
        assert_eq!(select_slot(&bad, &good, land_id()), Some((SaveSlot::Backup, true)));
        assert_eq!(select_slot(&bad, &bad, land_id()), None);
    }

    #[test]
    fn broken_land_repair_matrix() {
        let good = good_slot();
        let mut bad = good;
        bad[0x200] ^= 0x02;

        let st = check_broken_land(&good, &bad, land_id());
        assert!(st.needs_repair());
        assert_eq!(st.ok, Some(SaveSlot::Main));

        let st = check_broken_land(&bad, &good, land_id());
        assert!(st.needs_repair());
        assert_eq!(st.ok, Some(SaveSlot::Backup));

        // Both good: no repair.
        assert!(!check_broken_land(&good, &good, land_id()).needs_repair());
        // Both bad: no repair (nothing to recover from).
        assert!(!check_broken_land(&bad, &bad, land_id()).needs_repair());
    }

    #[test]
    fn repair_copies_good_over_broken() {
        let good = Box::new(good_slot());
        let mut bad = good.clone();
        bad[0x300] ^= 0x04;
        let misc = Box::new([0u8; SAVE_SLOT_SIZE]);
        let image = build_land_image(&misc, &good, &bad);
        let repaired = repair_land(&image, land_id()).expect("should repair");
        let back: &[u8; SAVE_SLOT_SIZE] = repaired[BACKUP_OFS..BACKUP_OFS + SAVE_SLOT_SIZE]
            .try_into()
            .unwrap();
        assert_eq!(back, &good[..]);
        // Misc region untouched.
        assert_eq!(&repaired[..SAVE_SLOT_SIZE], &misc[..]);
    }

    #[test]
    fn sector_write_plan_skips_identical() {
        let old = vec![0x11u8; 0x6000];
        let mut new = old.clone();
        new[0x2000] ^= 0xFF; // only sector 1 differs
        let plan = sector_write_plan(&old, &new, 0, 0x6000);
        assert_eq!(plan, vec![1]);
        let plan2 = sector_write_plan(&old, &old, 0, 0x6000);
        assert!(plan2.is_empty());
    }

    #[test]
    fn aux_blocks_roundtrip() {
        let mut mail = vec![0x22u8; aux::MAIL_SIZE];
        aux_set_checksum(&mut mail);
        assert!(aux_block_valid(&mail));
        mail[100] ^= 0x01;
        assert!(!aux_block_valid(&mail));

        // Forgiving load: bad blocks are reported, town unaffected.
        let mut image = [0u8; LAND_FILE_SIZE];
        let mut orig = vec![0x33u8; aux::ORIGINAL_SIZE];
        aux_set_checksum(&mut orig);
        image[aux::MAIL_OFS..aux::MAIL_OFS + aux::MAIL_SIZE].copy_from_slice(&mail);
        image[aux::ORIGINAL_OFS..aux::ORIGINAL_OFS + aux::ORIGINAL_SIZE].copy_from_slice(&orig);
        let (m, o, d) = load_others(&image, true);
        assert!(!m);
        assert!(o);
        // Zeroed diary sums to zero, so retail treats it as valid.
        assert!(d);
    }

    #[test]
    fn zeroed_diary_sums_to_zero() {
        // A zeroed block sums to zero, so retail's `== 0` test passes and
        // the block would be "loaded" (a no-op copy of zeros).
        let diary = vec![0u8; aux::DIARY_SIZE];
        assert!(aux_block_valid(&diary));
    }

    #[test]
    fn clear_save_check_values() {
        let mut slot = good_slot();
        clear_save_check(&mut slot);
        let hdr = SaveCheck::parse(&slot);
        assert_eq!(hdr.code, 0xFFFF_FFFF);
        assert_eq!(hdr.land_id, 0xFFFF);
        assert_eq!(hdr.time, [0xFF; 8]);
        assert_eq!(hdr.checksum, 0);
        assert_eq!(hdr.version, 6); // untouched
        assert!(!hdr.id_ok());
    }

    #[test]
    fn land_id_bitmask() {
        assert!(SaveCheck { version: 6, code: SAVE_ID, land_id: 0x3001, time: [0; 8], checksum: 0 }.land_id_ok());
        assert!(!SaveCheck { version: 6, code: SAVE_ID, land_id: 0x2001, time: [0; 8], checksum: 0 }.land_id_ok());
    }

    #[test]
    fn copy_protect_range() {
        assert_eq!(new_copy_protect(0.0), 1);
        assert!(new_copy_protect(0.99999) <= 0xFFF0);
        assert!(new_copy_protect(0.99999) >= 1);
        assert!(copy_protect_ok(0x1234, 0x1234));
        assert!(!copy_protect_ok(0x1234, 0x1235));
    }

    #[test]
    fn build_land_image_layout() {
        let misc = Box::new([0x11u8; SAVE_SLOT_SIZE]);
        let main = Box::new(good_slot());
        let backup = Box::new(good_slot());
        let image = build_land_image(&misc, &main, &backup);
        assert_eq!(image.len(), LAND_FILE_SIZE);
        assert_eq!(&image[MISC_OFS..MISC_OFS + SAVE_SLOT_SIZE], &misc[..]);
        assert_eq!(&image[MAIN_OFS..MAIN_OFS + SAVE_SLOT_SIZE], &main[..]);
        assert_eq!(&image[BACKUP_OFS..BACKUP_OFS + SAVE_SLOT_SIZE], &backup[..]);
    }
}
