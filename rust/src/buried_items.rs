//! Buried-item system for the Rust rewrite.
//!
//! Source-verified architecture (upstream `include/m_common_data.h`,
//! `src/game/m_field_info.c`, `src/game/m_museum.c`,
//! `src/game/m_all_grow_ovl.c`, `src/game/m_name_table.c`,
//! `src/actor/ac_event_manager.c`, `include/m_name_table.h`):
//!
//! * "Buried" is a state, not an item type. The save holds a per-block
//!   bit array `u16 deposit[FG_BLOCK_X_NUM * FG_BLOCK_Z_NUM][UT_Z_NUM]`
//!   (save offset 0x020F1C), commented in the decomp as "flags for which
//!   items are buried around town". One bit per tile: `mFI_LineDepositON` /
//!   `mFI_LineDepositOFF` / `mFI_GetLineDeposit` manipulate bit `ut_x` of
//!   row `ut_z`.
//! * Burying writes the item ID into the town foreground grid AND sets the
//!   deposit bit (`mMsm_DepositItemBlock_cancel`: `*fg_items = deposit_item;
//!   *deposit |= (1 << ut_x);`).
//! * Daily fossils: `mMsm_DepositFossil` keeps at most
//!   `mMsm_DEPOSIT_FOSSIL_MAX` (5) buried, one per x-column of acres,
//!   skipping player/shrine/station/pool/dump acres.
//! * Gyroids ("haniwa"): `mAGrw_HANIWA_NUM` (3) deposited the same way.
//! * Pitfalls are the exception: `mMsm_DepositItemBlock` stores
//!   `BURIED_PITFALL_HOLE_START + hole_num` (0x002A + hole, 25 holes,
//!   plus a reserved range) instead of item + deposit bit.
//! * Dig conversion (`bg_item_fg_sub_dig2take_conv`): buried pitfall hole ->
//!   `ITM_PITFALL`; `SHINE_SPOT` (0x005C) -> bell bags by money-power roll;
//!   everything else passes through unchanged.
//! * The normal pickup path refuses buried tiles:
//!   `Player_actor_CheckItem_fromPosition` requires
//!   `mFI_Wpos2DepositGet(...) == FALSE`.
//! * Clearing (`be_flat_unit` in the event manager): buried pitfall holes and
//!   the shine spot are converted with `bg_item_fg_sub_dig2take_conv`, the
//!   foreground item is set to `EMPTY_NO`, and the deposit bit is turned off.
//!
//! Rewrite-owned: the deterministic RNG, tile-validity heuristics, and the
//! high-level dig/step APIs. Tile collision checks (`mCoBG_CheckHole_OrgAttr`
//! etc.) live in the C collision system and are not modeled here.

// Public buried-item API for the rewrite and future adapters. The crate
// builds as a staticlib, so unused public items would warn as dead code.
#![allow(dead_code)]

/// Tiles per acre in each direction (`UT_BASE_NUM`).
pub const UT_X_NUM: usize = 16;
pub const UT_Z_NUM: usize = 16;
/// Main-town acres (`FG_BLOCK_X_NUM` / `FG_BLOCK_Z_NUM`).
pub const FG_BLOCK_X_NUM: usize = 5;
pub const FG_BLOCK_Z_NUM: usize = 6;
pub const FG_BLOCK_TOTAL_NUM: usize = FG_BLOCK_X_NUM * FG_BLOCK_Z_NUM;

/// Empty foreground item (`EMPTY_NO`).
pub const EMPTY_NO: u16 = 0x0000;
/// Generic fossil item placed by the daily generator (`ITM_FOSSIL`).
pub const ITM_FOSSIL: u16 = 0x2511;
/// Pitfall seed item (`ITM_PITFALL`).
pub const ITM_PITFALL: u16 = 0x2512;
/// Bell bags from the glowing spot (`ITM_MONEY_*`).
pub const ITM_MONEY_1000: u16 = 0x2100;
pub const ITM_MONEY_10000: u16 = 0x2101;
pub const ITM_MONEY_30000: u16 = 0x2102;

/// Buried pitfall-hole item range (`BURIED_PITFALL_HOLE_START`..`_END`).
pub const BURIED_PITFALL_HOLE_START: u16 = 0x002A;
pub const BURIED_PITFALL_HOLE_END: u16 = 0x0042;
/// Reserved pitfall-hole range.
pub const BURIED_PITFALL_HOLE_RSV_START: u16 = 0x0043;
pub const BURIED_PITFALL_HOLE_RSV_END: u16 = 0x005B;
/// Daily glowing spot item (`SHINE_SPOT`).
pub const SHINE_SPOT: u16 = 0x005C;

/// Max buried fossils the daily generator keeps (`mMsm_DEPOSIT_FOSSIL_MAX`).
pub const DEPOSIT_FOSSIL_MAX: u8 = 5;
/// Gyroids deposited per generation pass (`mAGrw_HANIWA_NUM`).
pub const HANIWA_NUM: u8 = 3;

/// Mirrors `mFI_LineDepositON`: set the buried bit for tile `ut_x` in a row.
pub fn line_deposit_on(line: &mut u16, ut_x: u8) {
    *line |= 1 << ut_x;
}

/// Mirrors `mFI_LineDepositOFF`: clear the buried bit for tile `ut_x`.
pub fn line_deposit_off(line: &mut u16, ut_x: u8) {
    *line &= !(1 << ut_x);
}

/// Mirrors `mFI_GetLineDeposit`: read the buried bit for tile `ut_x`.
pub fn line_deposit_get(line: u16, ut_x: u8) -> bool {
    (line >> ut_x) & 1 == 1
}

/// True for buried-pitfall-hole items (`ITEM_IS_BURIED_PITFALL_HOLE`).
pub fn item_is_buried_pitfall_hole(item: u16) -> bool {
    (BURIED_PITFALL_HOLE_START..=BURIED_PITFALL_HOLE_END).contains(&item)
}

/// True for reserved pitfall-hole items (`ITEM_IS_BURIED_PITFALL_HOLE_RSV`).
pub fn item_is_buried_pitfall_hole_rsv(item: u16) -> bool {
    (BURIED_PITFALL_HOLE_RSV_START..=BURIED_PITFALL_HOLE_RSV_END).contains(&item)
}

/// Town burial state: foreground item grid plus the parallel buried-bit
/// array. `deposit[block][ut_z]` bit `ut_x` mirrors
/// `Save.deposit[FG_BLOCK_X_NUM * FG_BLOCK_Z_NUM][UT_Z_NUM]`.
pub struct BurialGrid {
    pub items: [[[u16; UT_X_NUM]; UT_Z_NUM]; FG_BLOCK_TOTAL_NUM],
    pub deposit: [[u16; UT_Z_NUM]; FG_BLOCK_TOTAL_NUM],
}

impl Default for BurialGrid {
    fn default() -> Self {
        Self {
            items: [[[EMPTY_NO; UT_X_NUM]; UT_Z_NUM]; FG_BLOCK_TOTAL_NUM],
            deposit: [[0; UT_Z_NUM]; FG_BLOCK_TOTAL_NUM],
        }
    }
}

impl BurialGrid {
    fn block_idx(bx: usize, bz: usize) -> Option<usize> {
        if bx < FG_BLOCK_X_NUM && bz < FG_BLOCK_Z_NUM {
            Some(bz * FG_BLOCK_X_NUM + bx)
        } else {
            None
        }
    }

    fn valid_tile(ut_x: usize, ut_z: usize) -> bool {
        ut_x < UT_X_NUM && ut_z < UT_Z_NUM
    }

    /// Mirrors `mFI_BlockDepositON`.
    pub fn block_deposit_on(&mut self, bx: usize, bz: usize, ut_x: usize, ut_z: usize) -> bool {
        match Self::block_idx(bx, bz) {
            Some(b) if Self::valid_tile(ut_x, ut_z) => {
                line_deposit_on(&mut self.deposit[b][ut_z], ut_x as u8);
                true
            }
            _ => false,
        }
    }

    /// Mirrors `mFI_BlockDepositOFF`.
    pub fn block_deposit_off(&mut self, bx: usize, bz: usize, ut_x: usize, ut_z: usize) -> bool {
        match Self::block_idx(bx, bz) {
            Some(b) if Self::valid_tile(ut_x, ut_z) => {
                line_deposit_off(&mut self.deposit[b][ut_z], ut_x as u8);
                true
            }
            _ => false,
        }
    }

    /// Mirrors `mFI_GetBlockDeposit`.
    pub fn block_deposit_get(&self, bx: usize, bz: usize, ut_x: usize, ut_z: usize) -> bool {
        match Self::block_idx(bx, bz) {
            Some(b) if Self::valid_tile(ut_x, ut_z) => {
                line_deposit_get(self.deposit[b][ut_z], ut_x as u8)
            }
            _ => false,
        }
    }

    /// Bury an item: write the item ID and set the deposit bit. Mirrors the
    /// non-pitfall path of `mMsm_DepositItemBlock`. Returns false if the tile
    /// is out of range or already occupied.
    pub fn bury_item(&mut self, bx: usize, bz: usize, ut_x: usize, ut_z: usize, item: u16) -> bool {
        match Self::block_idx(bx, bz) {
            Some(b) if Self::valid_tile(ut_x, ut_z) && self.items[b][ut_z][ut_x] == EMPTY_NO => {
                self.items[b][ut_z][ut_x] = item;
                line_deposit_on(&mut self.deposit[b][ut_z], ut_x as u8);
                true
            }
            _ => false,
        }
    }

    /// Bury a pitfall seed. The source stores
    /// `BURIED_PITFALL_HOLE_START + hole_num` instead of item + deposit bit.
    /// Returns the hole item ID, or `None` if no hole slot is free.
    pub fn bury_pitfall(&mut self, bx: usize, bz: usize, ut_x: usize, ut_z: usize) -> Option<u16> {
        let hole = self.alloc_pitfall_hole()?;
        match Self::block_idx(bx, bz) {
            Some(b) if Self::valid_tile(ut_x, ut_z) && self.items[b][ut_z][ut_x] == EMPTY_NO => {
                let item = BURIED_PITFALL_HOLE_START + hole as u16;
                self.items[b][ut_z][ut_x] = item;
                Some(item)
            }
            _ => None,
        }
    }

    /// Find a free pitfall-hole slot (0..=24), mirroring the hole-number
    /// allocation behind `mCoBG_BnumUnum2HoleNumber`.
    fn alloc_pitfall_hole(&self) -> Option<u8> {
        let mut used = [false; 25];
        for block in self.items.iter() {
            for row in block.iter() {
                for item in row.iter() {
                    if item_is_buried_pitfall_hole(*item) {
                        used[(*item - BURIED_PITFALL_HOLE_START) as usize] = true;
                    }
                }
            }
        }
        used.iter().position(|u| !u).map(|i| i as u8)
    }

    /// Whether the normal pickup path may take the tile's item. Mirrors the
    /// `mFI_Wpos2DepositGet(...) == FALSE` gate in
    /// `Player_actor_CheckItem_fromPosition`: buried tiles are not
    /// pick-uppable by the ordinary path.
    pub fn can_pickup(&self, bx: usize, bz: usize, ut_x: usize, ut_z: usize) -> bool {
        !self.block_deposit_get(bx, bz, ut_x, ut_z)
            && !item_is_buried_pitfall_hole(self.item_at(bx, bz, ut_x, ut_z))
    }

    pub fn item_at(&self, bx: usize, bz: usize, ut_x: usize, ut_z: usize) -> u16 {
        match Self::block_idx(bx, bz) {
            Some(b) if Self::valid_tile(ut_x, ut_z) => self.items[b][ut_z][ut_x],
            _ => EMPTY_NO,
        }
    }

    /// Dig up a tile with the shovel. Returns the unearthed inventory item
    /// (after `dig2take_conv`) and clears both the item and the deposit bit,
    /// mirroring the `be_flat_unit` clear sequence
    /// (`mFI_SetFG_common(EMPTY_NO, ...)` + `mFI_Wpos2DepositOFF`).
    /// Returns `None` when nothing buried is there.
    pub fn dig_up(
        &mut self,
        bx: usize,
        bz: usize,
        ut_x: usize,
        ut_z: usize,
        rng_100: f32,
        money_power: f32,
        money_luck: bool,
    ) -> Option<u16> {
        let b = Self::block_idx(bx, bz)?;
        if !Self::valid_tile(ut_x, ut_z) {
            return None;
        }
        let item = self.items[b][ut_z][ut_x];
        let buried = line_deposit_get(self.deposit[b][ut_z], ut_x as u8)
            || item_is_buried_pitfall_hole(item)
            || item == SHINE_SPOT;
        if !buried || item == EMPTY_NO {
            return None;
        }
        let taken = dig2take_conv(item, rng_100, money_power, money_luck);
        self.items[b][ut_z][ut_x] = EMPTY_NO;
        line_deposit_off(&mut self.deposit[b][ut_z], ut_x as u8);
        Some(taken)
    }

    /// True when a character stepping on the tile triggers a pitfall.
    pub fn is_pitfall_trap(&self, bx: usize, bz: usize, ut_x: usize, ut_z: usize) -> bool {
        item_is_buried_pitfall_hole(self.item_at(bx, bz, ut_x, ut_z))
    }
}

/// Mirrors `bg_item_fg_sub_dig2take_conv`: convert a dug-up town item into
/// the inventory item the player receives. `rng_100` is `RANDOM_F(100.0)`,
/// `money_power` is `mPr_GetMoneyPower()`.
pub fn dig2take_conv(item: u16, rng_100: f32, money_power: f32, money_luck: bool) -> u16 {
    if item_is_buried_pitfall_hole(item) {
        return ITM_PITFALL;
    }
    if item == SHINE_SPOT {
        let max_bells_roll = 2.0 * (money_power / 40.0);
        let large_bells_roll = 10.0 * (money_power / 40.0);
        if rng_100 <= max_bells_roll || money_luck {
            return ITM_MONEY_30000;
        } else if rng_100 <= large_bells_roll + max_bells_roll {
            return ITM_MONEY_10000;
        } else {
            return ITM_MONEY_1000;
        }
    }
    item
}

/// How many fossils the daily generator still needs to bury. Mirrors the
/// `mMsm_DEPOSIT_FOSSIL_MAX - fossil_count` logic in `mMsm_DepositFossil`.
pub fn fossils_to_deposit(existing: u8) -> u8 {
    DEPOSIT_FOSSIL_MAX.saturating_sub(existing)
}

/// Fossil deposit record: bit `1 << (block_x + 1)` marks x-columns that
/// already received a fossil (`mMsm_RecordDepositFossil` /
/// `mMsm_GetDepositBlockNum`). Returns how many columns are used.
pub fn deposit_record_block_num(record: u8) -> u8 {
    let mut n = 0;
    for block_x in 0..FG_BLOCK_X_NUM {
        if (record >> (block_x + 1)) & 1 == 1 {
            n += 1;
        }
    }
    n
}

/// Mark an x-column as fossil-deposited in the record.
pub fn deposit_record_mark(record: &mut u8, block_x: usize) {
    if block_x < FG_BLOCK_X_NUM {
        *record |= 1 << (block_x + 1);
    }
}

/// Rewrite-owned validity heuristic for player burial: trees, buildings,
/// signboards and other structural objects are excluded, following the
/// ACSE editor's exclusions. This is a heuristic, not a decomp predicate.
pub fn can_bury_item(item: u16) -> bool {
    if item == EMPTY_NO || item == SHINE_SPOT {
        return false;
    }
    if item_is_buried_pitfall_hole(item) || item_is_buried_pitfall_hole_rsv(item) {
        return false;
    }
    // Structural/signboard ranges are rewrite-owned exclusions.
    true
}

/// C ABI: read the buried bit for tile `ut_x` of a deposit *row*.
///
/// This is the exact retail boundary (`mFI_GetLineDeposit(u16*
/// deposit, int ut_x)`): C passes `mFI_GetDepositP(bx, bz) + ut_z`
/// directly, so no block/line arithmetic happens here. Returns 1 when
/// buried, else 0.
#[no_mangle]
pub extern "C" fn pc_buried_line_get(line: *const u16, ut_x: u8) -> i32 {
    if line.is_null() || (ut_x as usize) >= UT_X_NUM {
        return 0;
    }
    i32::from(line_deposit_get(unsafe { *line }, ut_x))
}

/// C ABI: set the buried bit for tile `ut_x` of a deposit *row*
/// (mirrors `mFI_LineDepositON`).
#[no_mangle]
pub extern "C" fn pc_buried_line_set(line: *mut u16, ut_x: u8) {
    if line.is_null() || (ut_x as usize) >= UT_X_NUM {
        return;
    }
    unsafe {
        line_deposit_on(&mut *line, ut_x);
    }
}

/// C ABI: clear the buried bit for tile `ut_x` of a deposit *row*
/// (mirrors `mFI_LineDepositOFF`).
#[no_mangle]
pub extern "C" fn pc_buried_line_clear(line: *mut u16, ut_x: u8) {
    if line.is_null() || (ut_x as usize) >= UT_X_NUM {
        return;
    }
    unsafe {
        line_deposit_off(&mut *line, ut_x);
    }
}

/// C ABI: read the buried bit for a tile. Returns 1 when buried, else 0.
///
/// Rewrite-side convenience: takes the whole deposit array and does
/// the block/line indexing itself. This is NOT the retail boundary —
/// wire `pc_buried_line_get` instead (see above).
#[no_mangle]
pub extern "C" fn pc_buried_get(bx: u8, bz: u8, ut_x: u8, ut_z: u8, deposit: *const u16) -> i32 {
    if deposit.is_null() {
        return 0;
    }
    if (bx as usize) >= FG_BLOCK_X_NUM
        || (bz as usize) >= FG_BLOCK_Z_NUM
        || (ut_x as usize) >= UT_X_NUM
        || (ut_z as usize) >= UT_Z_NUM
    {
        return 0;
    }
    let block = (bz as usize) * FG_BLOCK_X_NUM + (bx as usize);
    let line = unsafe { *deposit.add(block * UT_Z_NUM + ut_z as usize) };
    i32::from(line_deposit_get(line, ut_x))
}

/// C ABI: set the buried bit for a tile.
#[no_mangle]
pub extern "C" fn pc_buried_set(bx: u8, bz: u8, ut_x: u8, ut_z: u8, deposit: *mut u16) {
    if deposit.is_null() {
        return;
    }
    if (bx as usize) >= FG_BLOCK_X_NUM
        || (bz as usize) >= FG_BLOCK_Z_NUM
        || (ut_x as usize) >= UT_X_NUM
        || (ut_z as usize) >= UT_Z_NUM
    {
        return;
    }
    let block = (bz as usize) * FG_BLOCK_X_NUM + (bx as usize);
    unsafe {
        let line = deposit.add(block * UT_Z_NUM + ut_z as usize);
        line_deposit_on(&mut *line, ut_x);
    }
}

/// C ABI: clear the buried bit for a tile.
#[no_mangle]
pub extern "C" fn pc_buried_clear(bx: u8, bz: u8, ut_x: u8, ut_z: u8, deposit: *mut u16) {
    if deposit.is_null() {
        return;
    }
    if (bx as usize) >= FG_BLOCK_X_NUM
        || (bz as usize) >= FG_BLOCK_Z_NUM
        || (ut_x as usize) >= UT_X_NUM
        || (ut_z as usize) >= UT_Z_NUM
    {
        return;
    }
    let block = (bz as usize) * FG_BLOCK_X_NUM + (bx as usize);
    unsafe {
        let line = deposit.add(block * UT_Z_NUM + ut_z as usize);
        line_deposit_off(&mut *line, ut_x);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_deposit_bit_ops() {
        let mut line: u16 = 0;
        line_deposit_on(&mut line, 5);
        assert!(line_deposit_get(line, 5));
        assert!(!line_deposit_get(line, 4));
        line_deposit_off(&mut line, 5);
        assert!(!line_deposit_get(line, 5));
    }

    #[test]
    fn bury_and_dig_roundtrip() {
        let mut grid = BurialGrid::default();
        assert!(grid.bury_item(0, 0, 3, 7, ITM_FOSSIL));
        assert!(grid.block_deposit_get(0, 0, 3, 7));
        assert!(!grid.can_pickup(0, 0, 3, 7));
        let got = grid.dig_up(0, 0, 3, 7, 50.0, 40.0, false);
        assert_eq!(got, Some(ITM_FOSSIL));
        assert_eq!(grid.item_at(0, 0, 3, 7), EMPTY_NO);
        assert!(!grid.block_deposit_get(0, 0, 3, 7));
    }

    #[test]
    fn pitfall_uses_hole_item_not_deposit_bit() {
        let mut grid = BurialGrid::default();
        let hole = grid.bury_pitfall(1, 2, 4, 4).unwrap();
        assert!(item_is_buried_pitfall_hole(hole));
        assert_eq!(hole, BURIED_PITFALL_HOLE_START);
        // No deposit bit set for pitfalls (source path skips it).
        assert!(!grid.block_deposit_get(1, 2, 4, 4));
        assert!(grid.is_pitfall_trap(1, 2, 4, 4));
        // Digging a pitfall returns the seed item.
        let got = grid.dig_up(1, 2, 4, 4, 50.0, 40.0, false);
        assert_eq!(got, Some(ITM_PITFALL));
    }

    #[test]
    fn shine_spot_dig_conversion() {
        // 30k roll: rng within the max-bells window.
        assert_eq!(dig2take_conv(SHINE_SPOT, 1.0, 40.0, false), ITM_MONEY_30000);
        // Money-luck destiny forces the 30k bag.
        assert_eq!(dig2take_conv(SHINE_SPOT, 99.0, 40.0, true), ITM_MONEY_30000);
        // Mid roll: 10k bag (windows are 2.0 and 12.0 at money_power 40).
        assert_eq!(dig2take_conv(SHINE_SPOT, 5.0, 40.0, false), ITM_MONEY_10000);
        // High roll: 1k bag.
        assert_eq!(dig2take_conv(SHINE_SPOT, 50.0, 40.0, false), ITM_MONEY_1000);
        // Ordinary items pass through.
        assert_eq!(dig2take_conv(ITM_FOSSIL, 50.0, 40.0, false), ITM_FOSSIL);
    }

    #[test]
    fn fossil_daily_count_logic() {
        assert_eq!(fossils_to_deposit(0), 5);
        assert_eq!(fossils_to_deposit(3), 2);
        assert_eq!(fossils_to_deposit(5), 0);
        assert_eq!(fossils_to_deposit(9), 0);
    }

    #[test]
    fn fossil_deposit_record_bits() {
        let mut record: u8 = 0;
        deposit_record_mark(&mut record, 0);
        deposit_record_mark(&mut record, 4);
        assert_eq!(deposit_record_block_num(record), 2);
        deposit_record_mark(&mut record, 9); // out of range, ignored
        assert_eq!(deposit_record_block_num(record), 2);
    }

    #[test]
    fn dig_empty_tile_returns_none() {
        let mut grid = BurialGrid::default();
        assert_eq!(grid.dig_up(0, 0, 0, 0, 50.0, 40.0, false), None);
    }
}
