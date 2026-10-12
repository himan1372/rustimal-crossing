//! Nook's shop interiors, fitting rooms, umbrella displays, and the shop-level actor.
//!
//! Verified against `src/actor/ac_shop_design.c`,
//! `src/actor/ac_shop_manekin.c`, `src/actor/ac_shop_umbrella.c`,
//! `src/actor/ac_shop_indoor.c`, `src/actor/ac_shop_level.c`,
//! `src/actor/ac_shop_goods.c`, `include/ac_shop_goods.h`,
//! `include/m_name_table.h`, `include/m_shop.h` (GAFE01_00 Rev 0
//! USA). Two brief corrections carried: saplings and
//! Halloween-status flower bags map to `RSV_SHOP_HALLOWEEN`, not
//! `RSV_SHOP_PLANT`; and the carpet default at index 3 (DEPART_1F)
//! is `ITM_CARPET70`, not `ITM_CARPET68`.
//!
//! Scope split: the sibling `shop` module owns shop tiers, sales
//! sums, renewal scheduling, and the goods count. This module owns
//! the item→reserved-slot mapping, the sold-slot routing, the
//! mannequin/umbrella display state, the indoor room types and
//! defaults, the lottery drum positions, and the shop-building
//! field-ground replacement. Rendering, texture/DMA, collision
//! registration, the actor engine callbacks, and the save stay in C.

pub const UT_X_NUM: i32 = 16;
pub const UT_Z_NUM: i32 = 16;
pub const RSV_NO: u16 = 0xFFFF;
pub const EMPTY_NO: u16 = 0x0000;

/// The shop `tanuki_shop_status` values (`include/m_shop.h`).
pub mod shop_status {
    pub const NORMAL: u8 = 0;
    pub const EVENT: u8 = 1;
    pub const HALLOWEEN: u8 = 2;
    pub const FUKUBIKI: u8 = 3;
    pub const HUKUBUKURO_SALE: u8 = 4;
}

/// Reserved shop field-ground IDs (`include/m_name_table.h:3935-3966`).
pub mod rsv {
    pub const SHOP_PAPER: u16 = 0xFE00;
    pub const SHOP_CLOTH: u16 = 0xFE01;
    pub const SHOP_FTR: u16 = 0xFE02;
    pub const SHOP_CARPET: u16 = 0xFE03;
    pub const SHOP_WALL: u16 = 0xFE04;
    pub const SHOP_HALLOWEEN: u16 = 0xFE05;
    pub const SHOP_TOOL: u16 = 0xFE06;
    pub const SHOP_EVENT: u16 = 0xFE07;
    pub const SHOP_PLANT: u16 = 0xFE08;
    pub const SHOP_RARE: u16 = 0xFE09;
    pub const SHOP_UMBRELLA: u16 = 0xFE0A;
    pub const SHOP_PAINT: u16 = 0xFE0B;
    pub const SHOP_SIGNBOARD: u16 = 0xFE0C;
    pub const SHOP_FE0D: u16 = 0xFE0D;
    pub const SOLD_PAPER: u16 = 0xFE0E;
    pub const SOLD_CLOTH: u16 = 0xFE0F;
    pub const SOLD_FTR: u16 = 0xFE10;
    pub const SOLD_CARPET: u16 = 0xFE11;
    pub const SOLD_WALL: u16 = 0xFE12;
    pub const SOLD_PLANT: u16 = 0xFE13;
    pub const SOLD_TOOL: u16 = 0xFE14;
    pub const SOLD_RARE: u16 = 0xFE15;
    pub const SOLD_UMBRELLA: u16 = 0xFE16;
    pub const SOLD_PAINT: u16 = 0xFE17;
    pub const SOLD_SIGNBOARD: u16 = 0xFE18;
    pub const FE1F: u16 = 0xFE1F;
}

/// Shop-building item IDs (`include/m_name_table.h:3057`).
pub mod shop_bldg {
    pub const SHOP0: u16 = 0x5804;
    pub const SHOP3: u16 = 0x5807;
    pub const POST_OFFICE: u16 = 0x5808;
}

/// Item-ID ranges needed by the interior mapping.
pub mod itm {
    pub const PAPER_START: u16 = 0x2000;
    pub const PAPER_END: u16 = 0x2100;
    pub const TOOL_START: u16 = 0x2200;
    pub const UMBRELLA_START: u16 = 0x2204;
    pub const UMBRELLA_END: u16 = 0x222C;
    pub const RED_PAINT: u16 = 0x222D;
    pub const BROWN_PAINT: u16 = 0x2238;
    pub const TOOL_END: u16 = 0x225C;
    pub const CLOTH_START: u16 = 0x2400;
    pub const CLOTH_MANEKIN: u16 = 0x24FF;
    pub const CLOTH_END: u16 = 0x24FF;
    pub const SIGNBOARD: u16 = 0x251E;
    pub const CARPET_START: u16 = 0x2600;
    pub const CARPET_END: u16 = 0x2647;
    pub const WALL_START: u16 = 0x2700;
    pub const WALL_END: u16 = 0x2747;
    pub const FOOD_CANDY: u16 = 0x2806;
    pub const SAPLING: u16 = 0x2900;
    pub const CEDAR_SAPLING: u16 = 0x2901;
    pub const WHITE_PANSY_BAG: u16 = 0x2902;
    pub const YELLOW_TULIP_BAG: u16 = 0x290A;
    pub const DIARY00: u16 = 0x2B00;
    pub const DIARY15: u16 = 0x2B0F;
    pub const HUKUBUKURO_BAG: u16 = 0x2E00;

    /// `ITEM_IS_FTR`: furniture type = high nibble 1 or 3.
    pub fn is_ftr(n: u16) -> bool {
        let t = (n & 0xF000) >> 12;
        t == 1 || t == 3
    }

    /// Carpet defaults per room type; index 3 is ITM_CARPET70
    /// (the brief said ITM_CARPET68 — the decomp disagrees).
    pub const CARPET67: u16 = 0x2643;
    pub const CARPET68: u16 = 0x2644;
    pub const CARPET69: u16 = 0x2645;
    pub const CARPET70: u16 = 0x2646;
    pub const WALL67: u16 = 0x2743;
    pub const WALL68: u16 = 0x2744;
    pub const WALL69: u16 = 0x2745;
    pub const WALL70: u16 = 0x2746;
}

/// Item → reserved display-slot family
/// (`aSD_ItemName2ReservePointName`), verbatim including the
/// source-commented unreachable signboard check inside the tool
/// range (`/* @BUG - this is impossible to reach ... */`) and the
/// HALLOWEEN handling of saplings/Halloween-status flower bags.
pub fn item_to_slot(item: u16, status: u8) -> u16 {
    if status == shop_status::EVENT {
        return rsv::SHOP_EVENT;
    }
    if item == itm::SIGNBOARD {
        return rsv::SHOP_SIGNBOARD;
    }
    if itm::is_ftr(item) {
        return rsv::SHOP_FTR;
    }
    if item == rsv::SOLD_FTR {
        return rsv::SHOP_FTR;
    }
    if (item >= itm::PAPER_START && item <= itm::PAPER_END)
        || (item >= itm::DIARY00 && item <= itm::DIARY15)
    {
        return rsv::SHOP_PAPER;
    }
    if item == rsv::SOLD_PAPER {
        return rsv::SHOP_PAPER;
    }
    if item >= itm::CARPET_START && item <= itm::CARPET_END - 4 {
        return rsv::SHOP_CARPET;
    }
    if item == rsv::SOLD_CARPET {
        return rsv::SHOP_CARPET;
    }
    if item >= itm::WALL_START && item <= itm::WALL_END - 4 {
        return rsv::SHOP_WALL;
    }
    if item == rsv::SOLD_WALL {
        return rsv::SHOP_WALL;
    }
    if item >= itm::CLOTH_START && item <= itm::CLOTH_END {
        return rsv::SHOP_CLOTH;
    }
    if item == rsv::SOLD_CLOTH {
        return rsv::SHOP_CLOTH;
    }
    if item >= itm::TOOL_START && item <= itm::TOOL_END {
        if item >= itm::UMBRELLA_START && item <= itm::UMBRELLA_END - 1 {
            return rsv::SHOP_UMBRELLA;
        }
        if item >= itm::RED_PAINT && item <= itm::BROWN_PAINT {
            return rsv::SHOP_PAINT;
        }
        // @BUG (in the original source): unreachable — ITM_SIGNBOARD
        // is caught at the top of the function.
        if item == itm::SIGNBOARD {
            return rsv::SHOP_SIGNBOARD;
        }
        return rsv::SHOP_TOOL;
    }
    if item == rsv::SOLD_TOOL {
        return rsv::SHOP_TOOL;
    }
    if item == rsv::SOLD_UMBRELLA {
        return rsv::SHOP_UMBRELLA;
    }
    if item == rsv::SOLD_PAINT {
        return rsv::SHOP_PAINT;
    }
    if item == rsv::SOLD_SIGNBOARD {
        return rsv::SHOP_SIGNBOARD;
    }
    if (item >= itm::SAPLING && item <= itm::CEDAR_SAPLING)
        || (item >= itm::WHITE_PANSY_BAG
            && item <= itm::YELLOW_TULIP_BAG
            && status == shop_status::HALLOWEEN)
    {
        return rsv::SHOP_HALLOWEEN;
    }
    if (item >= itm::WHITE_PANSY_BAG && item <= itm::YELLOW_TULIP_BAG)
        || item == itm::FOOD_CANDY
    {
        return rsv::SHOP_PLANT;
    }
    EMPTY_NO
}

/// Sold-slot routing (`aSD_ReportGoodsSales`), verbatim. Each
/// branch is distinct: the sentinel for the goods-list sale report
/// and the field-ground write differ per category, and paper has a
/// bare `return TRUE` with neither.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaleRoute {
    /// SOLD_CLOTH to the sale report, change2naked_manekin_proc,
    /// FG write of RSV_NO, return FALSE.
    SoldCloth,
    /// SOLD_UMBRELLA, delete_umbrella_proc.
    SoldUmbrella,
    /// Rare furniture (item == save rare_item) → SOLD_RARE,
    /// other furniture → SOLD_FTR.
    SoldFurniture { rare: bool },
    SoldCarpet,
    SoldWall,
    /// SOLD_PLANT; grab bags report and write immediately with no
    /// TRUE/FALSE split.
    SoldPlant { grab_bag: bool },
    SoldPaint,
    SoldTool,
    SoldSignboard,
    /// Bare `return TRUE`: no sale report, no FG write.
    PaperDistinct,
    SoldDiary,
    /// Item does not route to any sold slot.
    NoSale,
}

/// Classify the sale route. `is_rare` = `item == Save_Get(shop).rare_item`
/// (save-owned; passed in).
pub fn sale_route(item: u16, is_rare: bool) -> SaleRoute {
    if item == itm::HUKUBUKURO_BAG {
        return SaleRoute::SoldPlant { grab_bag: true };
    }
    if (item >= itm::SAPLING && item <= itm::CEDAR_SAPLING)
        || (item >= itm::WHITE_PANSY_BAG && item <= itm::YELLOW_TULIP_BAG)
        || item == itm::FOOD_CANDY
    {
        return SaleRoute::SoldPlant { grab_bag: false };
    }
    if item >= itm::CLOTH_START && item <= itm::CLOTH_END {
        return SaleRoute::SoldCloth;
    }
    if itm::is_ftr(item) {
        return SaleRoute::SoldFurniture { rare: is_rare };
    }
    if item >= itm::CARPET_START && item <= itm::CARPET_END - 4 {
        return SaleRoute::SoldCarpet;
    }
    if item >= itm::WALL_START && item <= itm::WALL_END - 4 {
        return SaleRoute::SoldWall;
    }
    if item == itm::SIGNBOARD {
        return SaleRoute::SoldSignboard;
    }
    if item >= itm::TOOL_START && item <= itm::TOOL_END {
        if item >= itm::UMBRELLA_START && item <= itm::UMBRELLA_END - 1 {
            return SaleRoute::SoldUmbrella;
        } else if item >= itm::RED_PAINT && item <= itm::BROWN_PAINT {
            return SaleRoute::SoldPaint;
        } else if item == itm::SIGNBOARD {
            return SaleRoute::SoldSignboard;
        } else {
            return SaleRoute::SoldTool;
        }
    }
    if item >= itm::PAPER_START && item <= itm::PAPER_END {
        return SaleRoute::PaperDistinct;
    }
    if item >= itm::DIARY00 && item <= itm::DIARY15 {
        return SaleRoute::SoldDiary;
    }
    SaleRoute::NoSale
}

/// The sentinel for the goods-list sale report for a route.
pub fn sold_sentinel(route: SaleRoute) -> u16 {
    match route {
        SaleRoute::SoldCloth => rsv::SOLD_CLOTH,
        SaleRoute::SoldUmbrella => rsv::SOLD_UMBRELLA,
        SaleRoute::SoldFurniture { rare } => {
            if rare { rsv::SOLD_RARE } else { rsv::SOLD_FTR }
        }
        SaleRoute::SoldCarpet => rsv::SOLD_CARPET,
        SaleRoute::SoldWall => rsv::SOLD_WALL,
        SaleRoute::SoldPlant { .. } => rsv::SOLD_PLANT,
        SaleRoute::SoldPaint => rsv::SOLD_PAINT,
        SaleRoute::SoldTool => rsv::SOLD_TOOL,
        SaleRoute::SoldSignboard => rsv::SOLD_SIGNBOARD,
        SaleRoute::PaperDistinct => EMPTY_NO,
        SaleRoute::SoldDiary => rsv::SOLD_PAPER,
        SaleRoute::NoSale => EMPTY_NO,
    }
}

/// The field-ground write that follows the sale. Note the retail
/// quirks: sold clothing writes RSV_NO (the SOLD_CLOTH sentinel
/// goes only to the sale report), and paper writes nothing.
pub fn sold_fg_write(route: SaleRoute) -> Option<u16> {
    match route {
        SaleRoute::SoldCloth => Some(RSV_NO),
        SaleRoute::PaperDistinct => None,
        SaleRoute::NoSale => None,
        _ => Some(sold_sentinel(route)),
    }
}

/// Mannequins (`ac_shop_manekin.c`): the count includes clothing
/// items AND the RSV_SHOP_SOLD_CLOTH sentinel (which becomes a
/// naked mannequin, preserving the display position).
pub mod mannequin {
    use super::*;

    pub fn is_mannequin_spot(item: u16) -> bool {
        (item >= itm::CLOTH_START && item <= itm::CLOTH_END)
            || item == rsv::SOLD_CLOTH
    }

    /// Clothing → texture index: `item - ITM_CLOTH_START`.
    pub fn cloth_idx(item: u16) -> u16 {
        item.wrapping_sub(itm::CLOTH_START)
    }

    /// The naked mannequin's index: `ITM_CLOTH_MANEKIN - ITM_CLOTH_START`.
    pub const NAKED_IDX: u16 = itm::CLOTH_MANEKIN - itm::CLOTH_START; // 255

    /// Buffer sizes: `(count + 1) * mNW_DESIGN_TEX_SIZE` and
    /// `(count + 1) * mNW_PALETTE_SIZE` (the +1 is the naked mannequin).
    pub fn bank_bytes(count: usize, unit_size: usize) -> usize {
        (count + 1) * unit_size
    }

    /// Coordinate normalization (verbatim: C remainder, then
    /// first-match search in C).
    pub fn normalize(ux: i32, uz: i32) -> (i32, i32) {
        (ux % UT_X_NUM, uz % UT_Z_NUM)
    }

    /// Find the first mannequin record at the normalized unit
    /// (retail `break`s after the first match).
    pub fn find_first(records: &[(i32, i32)], ux: i32, uz: i32) -> Option<usize> {
        let (nx, nz) = normalize(ux, uz);
        records.iter().position(|&r| r == (nx, nz))
    }
}

/// Umbrellas (`ac_shop_umbrella.c`).
pub mod umbrella {
    use super::*;

    pub const STATE_WAIT: u8 = 0;
    pub const STATE_DELETE: u8 = 1;
    pub const STATE_NONE: u8 = 2;

    /// Model index: `item - ITM_UMBRELLA_START` (32-entry opaque and
    /// translucent tables).
    pub fn umbrella_idx(item: u16) -> u16 {
        item.wrapping_sub(itm::UMBRELLA_START)
    }

    /// One DELETE tick: scale decreases by 0.001f until zero,
    /// then the state goes to NONE.
    pub fn delete_step(scale: f32) -> f32 {
        (scale - 0.001).max(0.0)
    }

    /// The retail draw loop reads `umbrella->scale` (the FIRST
    /// record) for every umbrella — preserved verbatim (`@BUG` in
    /// the source). The movement state machine still updates each
    /// record individually.
    pub fn retail_draw_scale(scales: &[f32]) -> f32 {
        scales.first().copied().unwrap_or(0.0)
    }
}

/// Indoor room types (`ac_shop_indoor.c`): five types; DEPART_1F
/// uses shop1 wall/floor with shop4 lottery models.
pub mod indoor {
    pub const TYPE_ZAKKA: u8 = 0;
    pub const TYPE_CONVENI: u8 = 1;
    pub const TYPE_SUPER: u8 = 2;
    pub const TYPE_DEPART_1F: u8 = 3;
    pub const TYPE_DEPART_2F: u8 = 4;
    pub const TYPE_NUM: u8 = 5;

    /// Default wall items per room type (index = room type).
    pub const WALL_DEFAULTS: [u16; 5] = [
        super::itm::WALL67,
        super::itm::WALL68,
        super::itm::WALL69,
        super::itm::WALL68,
        super::itm::WALL70,
    ];

    /// Default carpet items per room type (index = room type).
    /// Index 3 is ITM_CARPET70 (verified; the brief said 68).
    pub const CARPET_DEFAULTS: [u16; 5] = [
        super::itm::CARPET67,
        super::itm::CARPET68,
        super::itm::CARPET69,
        super::itm::CARPET70,
        super::itm::CARPET70,
    ];

    /// Texture-bank alternation: the actor switches to the OTHER
    /// bank (0 <-> 1) before copying the new texture.
    pub fn alternate_bank(idx: u8) -> u8 {
        idx ^ 1
    }
}

/// Lottery drum positions (`garagara_set_table`), indexed by shop
/// index 0-3 (SCENE_SHOP0/CONVENI/SUPER/DEPART). The 5th entry is
/// never reached (DEPART_2 keeps actor_specific = -1).
pub mod lottery {
    pub const DRUM_POS: [(f32, f32, f32); 4] = [
        (160.0, 60.0, 190.0),
        (240.0, 60.0, 190.0),
        (240.0, 60.0, 350.0),
        (240.0, 60.0, 350.0),
    ];

    pub fn drum_pos(shop_idx: u8) -> Option<(f32, f32, f32)> {
        DRUM_POS.get(shop_idx as usize).copied()
    }
}

/// Shop-level building replacement (`ac_shop_level.c`).
pub mod level {
    use super::*;

    /// `aSL_RewriteShopFg` scans for an item in [SHOP0, SHOP3+1]
    /// inclusive. The upper bound includes the post office
    /// (`/* @BUG - shouldn't this just be <= SHOP3? ... */`) —
    /// preserved verbatim.
    pub fn is_shop_building(item: u16) -> bool {
        item >= shop_bldg::SHOP0 && item <= shop_bldg::SHOP3 + 1
    }

    /// `aSL_GroundClean` per-unit effects: `mPB_keep_item` with the
    /// existing item, deposit flag OFF, then FG write of RSV_NO.
    /// The unit iteration and the lost-and-found/deposit side
    /// effects stay in C; this is the per-unit constant.
    pub const GROUND_CLEAN_FG_WRITE: u16 = RSV_NO;

    /// The three `aSL_JudgeRenewShop` date exclusions, verbatim.
    /// Date arithmetic (bargain-day packing, month lengths, the
    /// 4th-Thursday-of-November lookup) stays in C; the C side
    /// passes the concrete values.
    pub fn renewal_blocked(
        now: (u8, u8),         // (month, day)
        one_day: (u8, u8),     // renewal_one_day
        two_days: (u8, u8),    // renewal_two_days
        bargain: Option<(u8, u8)>, // (month, day) from mEv_get_bargain_day, None if 0
        max_days: [(u8, u8); 3],   // (day, month-max-day) for now, one_day, two_days
        nov_fourth_thursday_plus1: u8, // Black Friday day-of-month
    ) -> bool {
        // No upgrading shop if a Nook sale event happens within 2 days.
        if let Some((bm, bd)) = bargain {
            if (now.0, now.1) == (bm, bd)
                || (one_day.0, one_day.1) == (bm, bd)
                || (two_days.0, two_days.1) == (bm, bd)
            {
                return true;
            }
        }
        // No upgrading shop if lottery day (last day of month) is within 2 days.
        if max_days.iter().any(|&(day, max)| day == max) {
            return true;
        }
        // Don't upgrade shop on sale day (Black Friday = 4th Thursday of November + 1).
        const NOVEMBER: u8 = 11;
        for &(m, d) in &[now, one_day, two_days] {
            if m == NOVEMBER && d == nov_fourth_thursday_plus1 {
                return true;
            }
        }
        false
    }
}

/// Thrown-item display pool (`include/ac_shop_goods.h:12`).
pub mod goods {
    /// `aSG_SINGLE_DRAW_NUM` — transient drop records.
    pub const SINGLE_DRAW_NUM: usize = 9;
}

// ---- C ABI ----
// All pc_-prefixed: the C originals stay compiled; these are staged kernels.

#[no_mangle]
pub extern "C" fn pc_si_item_to_slot(item: u16, shop_status: u8) -> u16 {
    item_to_slot(item, shop_status)
}

#[no_mangle]
pub extern "C" fn pc_si_sold_route(item: u16, is_rare: u8) -> u8 {
    match sale_route(item, is_rare != 0) {
        SaleRoute::SoldCloth => 0,
        SaleRoute::SoldUmbrella => 1,
        SaleRoute::SoldFurniture { rare } => {
            if rare { 2 } else { 3 }
        }
        SaleRoute::SoldCarpet => 4,
        SaleRoute::SoldWall => 5,
        SaleRoute::SoldPlant { grab_bag } => {
            if grab_bag { 6 } else { 7 }
        }
        SaleRoute::SoldPaint => 8,
        SaleRoute::SoldTool => 9,
        SaleRoute::SoldSignboard => 10,
        SaleRoute::PaperDistinct => 11,
        SaleRoute::SoldDiary => 12,
        SaleRoute::NoSale => 13,
    }
}

#[no_mangle]
pub extern "C" fn pc_si_sold_sentinel(route: u8) -> u16 {
    let route = match route {
        0 => SaleRoute::SoldCloth,
        1 => SaleRoute::SoldUmbrella,
        2 => SaleRoute::SoldFurniture { rare: true },
        3 => SaleRoute::SoldFurniture { rare: false },
        4 => SaleRoute::SoldCarpet,
        5 => SaleRoute::SoldWall,
        6 => SaleRoute::SoldPlant { grab_bag: true },
        7 => SaleRoute::SoldPlant { grab_bag: false },
        8 => SaleRoute::SoldPaint,
        9 => SaleRoute::SoldTool,
        10 => SaleRoute::SoldSignboard,
        11 => SaleRoute::PaperDistinct,
        12 => SaleRoute::SoldDiary,
        _ => SaleRoute::NoSale,
    };
    sold_sentinel(route)
}

/// Field-ground write for a sale; EMPTY_NO means "no write" (paper).
#[no_mangle]
pub extern "C" fn pc_si_sold_fg_write(route: u8) -> u16 {
    let route = match route {
        0 => SaleRoute::SoldCloth,
        1 => SaleRoute::SoldUmbrella,
        2 => SaleRoute::SoldFurniture { rare: true },
        3 => SaleRoute::SoldFurniture { rare: false },
        4 => SaleRoute::SoldCarpet,
        5 => SaleRoute::SoldWall,
        6 => SaleRoute::SoldPlant { grab_bag: true },
        7 => SaleRoute::SoldPlant { grab_bag: false },
        8 => SaleRoute::SoldPaint,
        9 => SaleRoute::SoldTool,
        10 => SaleRoute::SoldSignboard,
        11 => SaleRoute::PaperDistinct,
        12 => SaleRoute::SoldDiary,
        _ => SaleRoute::NoSale,
    };
    sold_fg_write(route).unwrap_or(EMPTY_NO)
}

#[no_mangle]
pub extern "C" fn pc_si_is_mannequin_spot(item: u16) -> u8 {
    mannequin::is_mannequin_spot(item) as u8
}

#[no_mangle]
pub extern "C" fn pc_si_umbrella_delete_step(scale: f32) -> f32 {
    umbrella::delete_step(scale)
}

/// Retail draw-scale accessor: reads the FIRST record's scale
/// (verbatim `@BUG` in the source).
#[no_mangle]
pub extern "C" fn pc_si_umbrella_draw_scale(scales: *const f32, count: usize) -> f32 {
    if scales.is_null() || count == 0 {
        return 0.0;
    }
    let scales = unsafe { std::slice::from_raw_parts(scales, count) };
    umbrella::retail_draw_scale(scales)
}

#[no_mangle]
pub extern "C" fn pc_si_alternate_bank(idx: u8) -> u8 {
    indoor::alternate_bank(idx)
}

#[no_mangle]
pub extern "C" fn pc_si_drum_pos(shop_idx: u8, out: *mut f32) -> u8 {
    match lottery::drum_pos(shop_idx) {
        Some((x, y, z)) if !out.is_null() => {
            let out = unsafe { std::slice::from_raw_parts_mut(out, 3) };
            out[0] = x;
            out[1] = y;
            out[2] = z;
            1
        }
        _ => 0,
    }
}

#[no_mangle]
pub extern "C" fn pc_si_is_shop_building(item: u16) -> u8 {
    level::is_shop_building(item) as u8
}

#[no_mangle]
pub extern "C" fn pc_si_renewal_blocked(
    now_month: u8,
    now_day: u8,
    one_month: u8,
    one_day: u8,
    two_month: u8,
    two_day: u8,
    bargain_valid: u8,
    bargain_month: u8,
    bargain_day: u8,
    now_max_day: u8,
    one_max_day: u8,
    two_max_day: u8,
    black_friday_day: u8,
) -> u8 {
    level::renewal_blocked(
        (now_month, now_day),
        (one_month, one_day),
        (two_month, two_day),
        if bargain_valid != 0 {
            Some((bargain_month, bargain_day))
        } else {
            None
        },
        [
            (now_day, now_max_day),
            (one_day, one_max_day),
            (two_day, two_max_day),
        ],
        black_friday_day,
    ) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slot_mapping() {
        // event status overrides everything
        assert_eq!(item_to_slot(0x1234, shop_status::EVENT), rsv::SHOP_EVENT);
        assert_eq!(item_to_slot(itm::SIGNBOARD, shop_status::NORMAL), rsv::SHOP_SIGNBOARD);
        // furniture (high nibble 1 or 3)
        assert_eq!(item_to_slot(0x1234, shop_status::NORMAL), rsv::SHOP_FTR);
        assert_eq!(item_to_slot(0x3456, shop_status::NORMAL), rsv::SHOP_FTR);
        // 0x2234 is a paint (ITM_RED_PAINT 0x222D..ITM_BROWN_PAINT 0x2238, gcc).
        assert_eq!(item_to_slot(0x2234, shop_status::NORMAL), rsv::SHOP_PAINT);
        assert_eq!(item_to_slot(0x2240, shop_status::NORMAL), rsv::SHOP_TOOL);
        assert_eq!(item_to_slot(0x2204, shop_status::NORMAL), rsv::SHOP_UMBRELLA);
        // paper and diaries
        assert_eq!(item_to_slot(0x2050, shop_status::NORMAL), rsv::SHOP_PAPER);
        assert_eq!(item_to_slot(0x2B05, shop_status::NORMAL), rsv::SHOP_PAPER);
        // carpet/wall (ranges end at *_END - 4)
        assert_eq!(item_to_slot(0x2630, shop_status::NORMAL), rsv::SHOP_CARPET);
        // ac_shop_design.c: `item <= ITM_CARPET_END - 4` = 0x2643 (CARPET67).
        assert_eq!(item_to_slot(0x2643, shop_status::NORMAL), rsv::SHOP_CARPET);
        assert_ne!(item_to_slot(0x2644, shop_status::NORMAL), rsv::SHOP_CARPET); // CARPET68 is past the range
        assert_eq!(item_to_slot(0x2730, shop_status::NORMAL), rsv::SHOP_WALL);
        // clothing
        assert_eq!(item_to_slot(0x2450, shop_status::NORMAL), rsv::SHOP_CLOTH);
        // umbrellas (nested in tool range, ITM_UMBRELLA_END - 1)
        assert_eq!(item_to_slot(0x2204, shop_status::NORMAL), rsv::SHOP_UMBRELLA);
        assert_eq!(item_to_slot(0x222B, shop_status::NORMAL), rsv::SHOP_UMBRELLA);
        // paint
        assert_eq!(item_to_slot(0x2230, shop_status::NORMAL), rsv::SHOP_PAINT);
        // plain tools
        assert_eq!(item_to_slot(0x2200, shop_status::NORMAL), rsv::SHOP_TOOL);
        // the unreachable nested signboard check is preserved but unreachable
        assert_eq!(item_to_slot(itm::SIGNBOARD, shop_status::NORMAL), rsv::SHOP_SIGNBOARD);
        // saplings -> HALLOWEEN (the brief's DIFFERS correction)
        assert_eq!(item_to_slot(0x2900, shop_status::NORMAL), rsv::SHOP_HALLOWEEN);
        assert_eq!(item_to_slot(0x2901, shop_status::NORMAL), rsv::SHOP_HALLOWEEN);
        // flower bags: HALLOWEEN only during Halloween status
        assert_eq!(
            item_to_slot(0x2905, shop_status::HALLOWEEN),
            rsv::SHOP_HALLOWEEN
        );
        assert_eq!(item_to_slot(0x2905, shop_status::NORMAL), rsv::SHOP_PLANT);
        // candy -> PLANT
        assert_eq!(item_to_slot(itm::FOOD_CANDY, shop_status::NORMAL), rsv::SHOP_PLANT);
        // sold round-trips
        assert_eq!(item_to_slot(rsv::SOLD_CLOTH, shop_status::NORMAL), rsv::SHOP_CLOTH);
        assert_eq!(item_to_slot(rsv::SOLD_UMBRELLA, shop_status::NORMAL), rsv::SHOP_UMBRELLA);
        assert_eq!(item_to_slot(rsv::SOLD_PAINT, shop_status::NORMAL), rsv::SHOP_PAINT);
        // unknown -> EMPTY_NO
        assert_eq!(item_to_slot(0x0001, shop_status::NORMAL), EMPTY_NO);
        assert_eq!(item_to_slot(0x5804, shop_status::NORMAL), EMPTY_NO);
    }

    #[test]
    fn sold_routing() {
        assert_eq!(sale_route(itm::HUKUBUKURO_BAG, false), SaleRoute::SoldPlant { grab_bag: true });
        assert_eq!(sale_route(0x2900, false), SaleRoute::SoldPlant { grab_bag: false });
        assert_eq!(sale_route(itm::FOOD_CANDY, false), SaleRoute::SoldPlant { grab_bag: false });
        assert_eq!(sale_route(0x2450, false), SaleRoute::SoldCloth);
        assert_eq!(
            sale_route(0x1234, true),
            SaleRoute::SoldFurniture { rare: true }
        );
        assert_eq!(
            sale_route(0x1234, false),
            SaleRoute::SoldFurniture { rare: false }
        );
        assert_eq!(sale_route(0x2630, false), SaleRoute::SoldCarpet);
        assert_eq!(sale_route(0x2730, false), SaleRoute::SoldWall);
        assert_eq!(sale_route(itm::SIGNBOARD, false), SaleRoute::SoldSignboard);
        assert_eq!(sale_route(0x2204, false), SaleRoute::SoldUmbrella);
        assert_eq!(sale_route(0x2230, false), SaleRoute::SoldPaint);
        assert_eq!(sale_route(0x2200, false), SaleRoute::SoldTool);
        assert_eq!(sale_route(0x2050, false), SaleRoute::PaperDistinct);
        assert_eq!(sale_route(0x2B05, false), SaleRoute::SoldDiary);
        assert_eq!(sale_route(0x0001, false), SaleRoute::NoSale);
        // sentinels
        assert_eq!(sold_sentinel(SaleRoute::SoldFurniture { rare: true }), rsv::SOLD_RARE);
        assert_eq!(sold_sentinel(SaleRoute::SoldFurniture { rare: false }), rsv::SOLD_FTR);
        assert_eq!(sold_sentinel(SaleRoute::SoldDiary), rsv::SOLD_PAPER);
        assert_eq!(sold_sentinel(SaleRoute::PaperDistinct), EMPTY_NO);
        // FG writes: sold clothing -> RSV_NO, paper -> none
        assert_eq!(sold_fg_write(SaleRoute::SoldCloth), Some(RSV_NO));
        assert_eq!(sold_fg_write(SaleRoute::PaperDistinct), None);
        assert_eq!(sold_fg_write(SaleRoute::NoSale), None);
        assert_eq!(sold_fg_write(SaleRoute::SoldPlant { grab_bag: true }), Some(rsv::SOLD_PLANT));
    }

    #[test]
    fn mannequins() {
        assert!(mannequin::is_mannequin_spot(0x2450));
        assert!(mannequin::is_mannequin_spot(rsv::SOLD_CLOTH));
        assert!(!mannequin::is_mannequin_spot(0x2200));
        assert_eq!(mannequin::cloth_idx(0x2450), 0x50);
        assert_eq!(mannequin::NAKED_IDX, 255);
        assert_eq!(mannequin::bank_bytes(3, 1024), 4 * 1024);
        assert_eq!(mannequin::normalize(20, 34), (4, 2));
        let records = [(4, 2), (4, 2), (5, 5)];
        // first match wins
        assert_eq!(mannequin::find_first(&records, 20, 34), Some(0));
        assert_eq!(mannequin::find_first(&records, 5, 5), Some(2));
        assert_eq!(mannequin::find_first(&records, 0, 0), None);
    }

    #[test]
    fn umbrellas() {
        assert_eq!(umbrella::STATE_WAIT, 0);
        assert_eq!(umbrella::STATE_DELETE, 1);
        assert_eq!(umbrella::STATE_NONE, 2);
        assert_eq!(umbrella::umbrella_idx(0x2204), 0);
        assert_eq!(umbrella::delete_step(0.01), 0.009);
        assert_eq!(umbrella::delete_step(0.0005), 0.0);
        // the retail draw bug: every umbrella uses the first record's scale
        assert_eq!(umbrella::retail_draw_scale(&[0.005, 0.01, 0.008]), 0.005);
    }

    #[test]
    fn indoor_defaults() {
        assert_eq!(indoor::TYPE_NUM, 5);
        assert_eq!(
            indoor::WALL_DEFAULTS,
            [itm::WALL67, itm::WALL68, itm::WALL69, itm::WALL68, itm::WALL70]
        );
        // carpet index 3 is ITM_CARPET70 (verified against the decomp)
        assert_eq!(indoor::CARPET_DEFAULTS[3], itm::CARPET70);
        assert_eq!(indoor::alternate_bank(0), 1);
        assert_eq!(indoor::alternate_bank(1), 0);
    }

    #[test]
    fn lottery_drums() {
        assert_eq!(lottery::drum_pos(0), Some((160.0, 60.0, 190.0)));
        assert_eq!(lottery::drum_pos(3), Some((240.0, 60.0, 350.0)));
        assert_eq!(lottery::drum_pos(4), None);
    }

    #[test]
    fn shop_building_scan() {
        assert!(level::is_shop_building(0x5804));
        assert!(level::is_shop_building(0x5807));
        // the @BUG upper bound includes the post office
        assert!(level::is_shop_building(0x5808));
        assert!(!level::is_shop_building(0x5809));
        assert!(!level::is_shop_building(0x5803));
    }

    #[test]
    fn renewal_exclusions() {
        // Nook sale within 2 days
        assert!(level::renewal_blocked(
            (3, 5),
            (3, 6),
            (3, 7),
            Some((3, 6)),
            [(5, 31), (6, 30), (7, 31)],
            27
        ));
        // lottery day (last day of month) within 2 days
        assert!(level::renewal_blocked(
            (3, 30),
            (3, 31),
            (4, 1),
            None,
            [(30, 31), (31, 31), (1, 30)],
            27
        ));
        // Black Friday
        assert!(level::renewal_blocked(
            (11, 27),
            (11, 28),
            (11, 29),
            None,
            [(27, 30), (28, 30), (29, 30)],
            27
        ));
        // nothing blocks
        assert!(!level::renewal_blocked(
            (6, 10),
            (6, 11),
            (6, 12),
            Some((6, 20)),
            [(10, 30), (11, 30), (12, 30)],
            26
        ));
    }

    #[test]
    fn goods_pool() {
        assert_eq!(goods::SINGLE_DRAW_NUM, 9);
    }
}
