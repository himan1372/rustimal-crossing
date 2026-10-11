//! Able Sisters design patterns — record layout and editor kernels.
//!
//! Verified against `include/m_needlework.h`, `src/game/m_needlework.c`,
//! `src/game/m_design_ovl.c`, `include/m_design_ovl.h`,
//! `include/m_cporiginal_ovl.h`, `include/m_private.h`,
//! `include/m_common_data.h`, `src/actor/ac_needlework_indoor.c`,
//! `src/actor/npc/ac_npc_needlework_talk.c_inc`,
//! `include/ac_npc_needlework.h`, `src/actor/ac_flag.c`
//! (GAFE01_00 Rev 0 USA decomp). All 12 brief claims checked; no
//! behavioral corrections. The brief's open `flag_design_set`
//! question is now CLOSED: the field's only live consumer is the
//! island flagpole (set when a design is applied, tested at render);
//! it is dead state on player/shop records in retail — preserve the
//! byte, do not invent behavior for it.
//!
//! Deliberately NOT ported: the drawing tools' controller input, the
//! rendering, the submenu state machines, the NPC dialogue, the shop
//! actor's double-buffer handoff, the ARAM/ROM default loading, and
//! the save records themselves. `ac_broker_design.c` is a furniture
//! broker actor, NOT the pattern editor — it stays out of this
//! module entirely.

/// Record layout constants (`include/m_needlework.h`).
pub mod record {
    /// Design name length.
    pub const NAME_LEN: usize = 16;
    /// Canvas dimensions.
    pub const WIDTH: u8 = 32;
    pub const HEIGHT: u8 = 32;
    /// 2 pixels per byte → 512 bytes.
    pub const TEX_SIZE: usize = 512;
    /// Byte offsets within the record.
    pub const NAME_OFS: usize = 0x000;
    pub const PALETTE_OFS: usize = 0x010;
    pub const FLAG_OFS: usize = 0x011;
    pub const DESIGN_OFS: usize = 0x020;
    /// `sizeof(mNW_original_design_c)`.
    pub const SIZE: usize = 0x220;
    /// Player design slots (`mPr_ORIGINAL_DESIGN_COUNT`).
    pub const PLAYER_COUNT: usize = 8;
    /// Shop clothing slots 0–3, umbrella slots 4–7.
    pub const SHOP_COUNT: usize = 8;
    pub const SHOP_CLOTH_COUNT: usize = 4;
}

/// Tile-aware pixel-to-byte offset (`mDE_POS2TEXEL`), verbatim: the
/// canvas is addressed as a 4×4 grid of 8×8 tiles, and the final
/// `>> 1` converts the pixel index to a byte index (2 pixels/byte).
pub fn pos2texel(x: u8, y: u8) -> usize {
    let x = x as usize;
    let y = y as usize;
    (((x & 7) + (y & 7) * 8 + ((((x & 0x18) >> 3) + (((y & 0x18) >> 3) * 4)) * 0x40)) >> 1)
}

/// Read a 4-bit pixel. Odd x reads the LOW nibble, even x the high
/// nibble (`m_design_ovl.c:638-641`).
pub fn pixel_get(tex: &[u8; record::TEX_SIZE], x: u8, y: u8) -> u8 {
    let b = tex[pos2texel(x, y)];
    if x & 1 == 1 { b & 0x0F } else { (b >> 4) & 0x0F }
}

/// Write a 4-bit pixel, preserving the sibling nibble.
pub fn pixel_set(tex: &mut [u8; record::TEX_SIZE], x: u8, y: u8, color: u8) {
    let idx = pos2texel(x, y);
    let c = color & 0x0F;
    tex[idx] = if x & 1 == 1 {
        (tex[idx] & 0xF0) | c
    } else {
        (tex[idx] & 0x0F) | (c << 4)
    };
}

/// RGB5A3 → RGB8 for the editor palette (`mDE_pallet_RGB5A3_to_RGB24`),
/// verbatim integer math. Branches on bit 15: opaque a1r5g5b5 uses
/// `(ch * 255) / 31`; transparent a3r4g4b4 uses `ch * 17`. The alpha
/// channel itself is NOT converted — RGB only.
pub fn rgb5a3_to_rgb8(color: u16) -> (u8, u8, u8) {
    if color & 0x8000 != 0 {
        let r = (((color >> 10) & 0x1F) as u32 * 255 / 31) as u8;
        let g = (((color >> 5) & 0x1F) as u32 * 255 / 31) as u8;
        let b = (((color >> 0) & 0x1F) as u32 * 255 / 31) as u8;
        (r, g, b)
    } else {
        let r = (((color >> 8) & 0xF) * 17) as u8;
        let g = (((color >> 4) & 0xF) * 17) as u8;
        let b = (((color >> 0) & 0xF) * 17) as u8;
        (r, g, b)
    }
}

/// GBA↔GC texture conversion (`mNW_AGB_to_GC_texture`), verbatim:
/// swaps the two 4-bit nibbles of each byte. The reverse calls the
/// same routine — nibble-swapping is self-inverse.
pub fn nibble_swap(b: u8) -> u8 {
    ((b & 0xF0) >> 4) | ((b & 0xF) << 4)
}

/// Palette selection (`mNW_PaletteIdx2Palette`): the index is masked
/// to 0–15 (`palette_idx & 15`). The 16 palettes are source tables in
/// `m_needlework.c`, not a generated hue wheel.
pub fn palette_mask(idx: u8) -> u8 {
    idx & 15
}

/// Blank pattern initialization (`mNW_InitOriginalData`), verbatim:
/// the name is the full 16-char literal `"blank           "`
/// (blank + 11 spaces), the texture is filled with 0xFF, palette 0.
pub mod blank {
    pub const NAME: &[u8; 16] = b"blank           ";
    pub const TEX_FILL: u8 = 0xFF;
    pub const PALETTE: u8 = 0;
}

/// Default palette-index tables. Player slots
/// (`mNW_InitMyOriginalPallet`): the first four textures come from
/// ARAM resource 27 and names from ROM strings 0x6DF+i; slots 4–7
/// are blank-initialized. Shop slots
/// (`mNW_InitNeedleworkPelatteNo`): ARAM resource 28, ROM strings
/// 0x6E7+i. The actual texture/name loading stays in C.
pub mod defaults {
    pub const PLAYER_PALETTES: [u8; 8] = [0, 8, 7, 7, 0, 0, 0, 0];
    pub const SHOP_PALETTES: [u8; 8] = [7, 1, 10, 3, 6, 0, 6, 7];
    pub const PLAYER_TEX_DEFAULTS: usize = 4;
}

/// Resolve a visible list position to the underlying record index
/// through the display-order map. The map starts as the identity
/// sequence; after reordering, the visible position and the record
/// index differ — never treat a selection index as a direct array
/// index.
pub fn resolve_slot(order_table: &[u8; record::PLAYER_COUNT], visible: usize) -> u8 {
    order_table[visible & 7]
}

/// The paid design service (`aNNW_DESIGN_PRICE`).
pub const DESIGN_PRICE: u32 = 350;

/// Sable's visit-day counter (`aNNW_day_day`), verbatim: the day
/// count increments only for non-foreign players on a new RTC date,
/// capped at `aNNW_TALK_DAYS_MAX` = 10. Repeated conversations on
/// the same date do not increment. (The last-date bookkeeping stays
/// in C; this is the day-count rule.)
pub mod visit {
    pub const DAYS_MAX: u8 = 10;

    pub fn next_days(days: u8, is_foreign: bool, date_changed: bool) -> u8 {
        if !is_foreign && date_changed && days < DAYS_MAX {
            days + 1
        } else {
            days
        }
    }
}

/// Shop slot split: indices 0–3 are the clothing/mannequin set,
/// 4–7 the umbrella set (`mNW_CLOTH_DESIGN_NUM` = 4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShopSlot {
    Cloth(u8),
    Umbrella(u8),
}

pub fn shop_slot(idx: u8) -> ShopSlot {
    let i = idx & 7;
    if i < record::SHOP_CLOTH_COUNT as u8 {
        ShopSlot::Cloth(i)
    } else {
        ShopSlot::Umbrella(i - record::SHOP_CLOTH_COUNT as u8)
    }
}

/// Memory-card pattern folders: 8 folders × 12 patterns
/// (`m_cporiginal_ovl.h`). Separate from the 8 player slots.
pub mod card {
    pub const PAGE_NUM: usize = 8;
    pub const ORIGINAL_NUM: usize = 12;
    pub const FOLDER_NAME_LEN: usize = 12;
}

/// Editor modes and tools (`include/m_design_ovl.h`; note the
/// header's spelling `mDE_MODE_PALLET`).
pub mod editor {
    pub const MODE_MAIN: u8 = 0;
    pub const MODE_PALLET: u8 = 1;
    pub const MODE_GRID: u8 = 2;
    pub const MODE_TOOL: u8 = 3;
    pub const TOOL_PEN: u8 = 0;
    pub const TOOL_NURI: u8 = 1;
    pub const TOOL_WAKU: u8 = 2;
    pub const TOOL_MARK: u8 = 3;
    pub const TOOL_UNDO: u8 = 4;
}

/// `flag_design_set` semantics (CLOSED by a full use-site search):
/// the byte at record offset 0x011 is a "custom design applied"
/// marker whose ONLY live consumer is the island flagpole
/// (`ac_flag.c`): set when a design is applied to the flag, tested
/// at render to choose the custom vs default texture/palette. It is
/// never written or read for player/shop records in retail — dead
/// state there. Whole-struct copies (`mNW_CopyOriginalTextureClass`
/// is a `bcopy`) propagate it verbatim — preserve that, do not
/// "fix" it.
pub mod flag {
    /// Mark a record as having a custom design applied.
    pub fn mark_applied(flag: &mut u8) {
        *flag = 1;
    }

    /// Test whether a custom design was applied.
    pub fn is_applied(flag: u8) -> bool {
        flag != 0
    }
}

// ---- C ABI ----
// All pc_-prefixed: the C originals stay compiled; these are staged kernels.

#[no_mangle]
pub extern "C" fn pc_ds_pos2texel(x: u8, y: u8) -> u16 {
    pos2texel(x, y) as u16
}

#[no_mangle]
pub extern "C" fn pc_ds_pixel_get(tex: *const u8, x: u8, y: u8) -> u8 {
    if tex.is_null() || x >= record::WIDTH || y >= record::HEIGHT {
        return 0;
    }
    let tex = unsafe { &*(tex as *const [u8; record::TEX_SIZE]) };
    pixel_get(tex, x, y)
}

#[no_mangle]
pub extern "C" fn pc_ds_pixel_set(tex: *mut u8, x: u8, y: u8, color: u8) {
    if tex.is_null() || x >= record::WIDTH || y >= record::HEIGHT {
        return;
    }
    let tex = unsafe { &mut *(tex as *mut [u8; record::TEX_SIZE]) };
    pixel_set(tex, x, y, color);
}

#[no_mangle]
pub extern "C" fn pc_ds_nibble_swap(b: u8) -> u8 {
    nibble_swap(b)
}

/// Packed 0x00RRGGBB.
#[no_mangle]
pub extern "C" fn pc_ds_rgb5a3_to_rgb8(color: u16) -> u32 {
    let (r, g, b) = rgb5a3_to_rgb8(color);
    ((r as u32) << 16) | ((g as u32) << 8) | b as u32
}

#[no_mangle]
pub extern "C" fn pc_ds_palette_mask(idx: u8) -> u8 {
    palette_mask(idx)
}

#[no_mangle]
pub extern "C" fn pc_ds_visit_next_days(days: u8, is_foreign: u8, date_changed: u8) -> u8 {
    visit::next_days(days, is_foreign != 0, date_changed != 0)
}

/// 0 = clothing slot, 1 = umbrella slot.
#[no_mangle]
pub extern "C" fn pc_ds_shop_slot_kind(idx: u8) -> u8 {
    match shop_slot(idx) {
        ShopSlot::Cloth(_) => 0,
        ShopSlot::Umbrella(_) => 1,
    }
}

/// The within-kind slot index (idx & 3).
#[no_mangle]
pub extern "C" fn pc_ds_shop_slot_idx(idx: u8) -> u8 {
    (idx & 7) & 3
}

#[no_mangle]
pub extern "C" fn pc_ds_resolve_slot(order_table: *const u8, visible: u8) -> u8 {
    if order_table.is_null() {
        return visible & 7;
    }
    let t = unsafe { &*(order_table as *const [u8; record::PLAYER_COUNT]) };
    resolve_slot(t, visible as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout() {
        assert_eq!(record::NAME_LEN, 16);
        assert_eq!(record::TEX_SIZE, 512);
        assert_eq!(record::SIZE, 0x220);
        assert_eq!(record::DESIGN_OFS, 0x020);
        assert_eq!(record::PLAYER_COUNT, 8);
        assert_eq!(blank::NAME.len(), 16);
        assert_eq!(blank::NAME, b"blank           ");
        assert_eq!(defaults::PLAYER_PALETTES, [0, 8, 7, 7, 0, 0, 0, 0]);
        assert_eq!(defaults::SHOP_PALETTES, [7, 1, 10, 3, 6, 0, 6, 7]);
        assert_eq!(DESIGN_PRICE, 350);
    }

    #[test]
    fn texel_mapping() {
        // origin and tile boundaries: 4x4 grid of 8x8 tiles
        assert_eq!(pos2texel(0, 0), 0);
        assert_eq!(pos2texel(1, 0), 0); // same byte, low nibble
        assert_eq!(pos2texel(2, 0), 1);
        assert_eq!(pos2texel(7, 0), 3);
        assert_eq!(pos2texel(0, 1), 4);
        assert_eq!(pos2texel(8, 0), 32); // next tile over
        assert_eq!(pos2texel(0, 8), 256); // next tile down
        assert_eq!(pos2texel(31, 31), 511); // last byte
        // every pixel maps inside the buffer, and the mapping is a bijection
        let mut seen = [false; 512];
        for y in 0..32 {
            for x in 0..32 {
                let i = pos2texel(x, y);
                assert!(i < 512);
                assert!(!seen[i] || (x & 1) == 1, "byte {i} reused by even x");
                seen[i] = true;
            }
        }
    }

    #[test]
    fn pixels_roundtrip() {
        let mut tex = [0u8; 512];
        pixel_set(&mut tex, 0, 0, 0xA);
        pixel_set(&mut tex, 1, 0, 0x5);
        assert_eq!(pixel_get(&tex, 0, 0), 0xA);
        assert_eq!(pixel_get(&tex, 1, 0), 0x5);
        // sibling nibble preserved
        pixel_set(&mut tex, 2, 3, 0xF);
        assert_eq!(pixel_get(&tex, 3, 3), 0);
        assert_eq!(pixel_get(&tex, 2, 3), 0xF);
        // tile-boundary pixels don't alias
        pixel_set(&mut tex, 7, 7, 1);
        pixel_set(&mut tex, 8, 8, 2);
        assert_eq!(pixel_get(&tex, 7, 7), 1);
        assert_eq!(pixel_get(&tex, 8, 8), 2);
    }

    #[test]
    fn color_conversion() {
        // opaque: (31*255)/31 = 255 per channel at max
        assert_eq!(rgb5a3_to_rgb8(0xFFFF), (255, 255, 255));
        assert_eq!(rgb5a3_to_rgb8(0x8000), (0, 0, 0));
        // opaque red: r=31 -> 255
        assert_eq!(rgb5a3_to_rgb8(0xFC00), (255, 0, 0));
        // transparent: 4-bit * 17
        assert_eq!(rgb5a3_to_rgb8(0x0FFF), (255, 255, 255));
        assert_eq!(rgb5a3_to_rgb8(0x0000), (0, 0, 0));
        assert_eq!(rgb5a3_to_rgb8(0x0F00), (255, 0, 0));
    }

    #[test]
    fn swap_is_self_inverse() {
        for b in [0x00u8, 0xFF, 0xA5, 0x5A, 0x12] {
            assert_eq!(nibble_swap(nibble_swap(b)), b);
        }
        assert_eq!(nibble_swap(0xA5), 0x5A);
        assert_eq!(palette_mask(0x13), 3);
        assert_eq!(palette_mask(15), 15);
    }

    #[test]
    fn order_and_visits() {
        let mut table = [0u8, 1, 2, 3, 4, 5, 6, 7];
        assert_eq!(resolve_slot(&table, 3), 3);
        table = [7, 6, 5, 4, 3, 2, 1, 0];
        assert_eq!(resolve_slot(&table, 0), 7);
        assert_eq!(resolve_slot(&table, 7), 0);
        // sable counter: new date increments, same date doesn't, cap at 10
        assert_eq!(visit::next_days(0, false, true), 1);
        assert_eq!(visit::next_days(3, false, false), 3);
        assert_eq!(visit::next_days(3, true, true), 3);
        assert_eq!(visit::next_days(10, false, true), 10);
        assert_eq!(visit::DAYS_MAX, 10);
    }

    #[test]
    fn shop_slots_and_flag() {
        assert_eq!(shop_slot(0), ShopSlot::Cloth(0));
        assert_eq!(shop_slot(3), ShopSlot::Cloth(3));
        assert_eq!(shop_slot(4), ShopSlot::Umbrella(0));
        assert_eq!(shop_slot(7), ShopSlot::Umbrella(3));
        assert_eq!(shop_slot(0x87), ShopSlot::Umbrella(3)); // idx & 7
        let mut f = 0u8;
        assert!(!flag::is_applied(f));
        flag::mark_applied(&mut f);
        assert!(flag::is_applied(f));
        assert_eq!(card::PAGE_NUM, 8);
        assert_eq!(card::ORIGINAL_NUM, 12);
        assert_eq!(editor::MODE_PALLET, 1);
        assert_eq!(editor::TOOL_UNDO, 4);
    }
}
