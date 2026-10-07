//! Attribute action-policy translation (`l_attribute_action_info[64]`).
//!
//! Verified against `m_collision_bg_info.c_inc` / `m_collision_bg.h`
//! (USA Rev. 0 decomp).
//!
//! The raw 6-bit `unit_attribute` (0-63) is translated by one table lookup
//! into an action-policy byte:
//!
//! - bits 0-2: plant-growth policy (0/2/4 = growth stages, 7 = kill)
//! - bit 3:    placement allowed (`mCoBG_ATR_PLACE`)
//! - bit 4:    NPC allowed (`mCoBG_ATR_NPC`)
//! - bits 5-7: unused (always zero in this table)
//!
//! This is a SECOND, independent layer from `mCoBG_Wpos2Attribute()`
//! (the contextual terrain translator): the action table is keyed by the
//! RAW collision attribute, never the effective one. It also never drives
//! collision geometry or column construction — only gameplay permissions.
//!
//! Correction to the brief: the river-bank asymmetry is at attribute 62
//! (grass 3 NE river bank: NPC + NO_PLACE + KILL = 0x17), not 61.

/// `l_attribute_action_info` (m_collision_bg_info.c_inc, verbatim):
/// raw 6-bit unit_attribute -> action-policy byte.
/// bits 0-2: plant policy; bit 3: placement; bit 4: NPC; bits 5-7: unused.
pub const ATTRIBUTE_ACTION_INFO: [u8; 64] = [
    0x1C, 0x1A, 0x18, 0x1F, 0x1C, 0x1A, 0x18, 0x1F, // 0-7
    0x1F, 0x1F, 0x18, 0x00, 0x07, 0x07, 0x07, 0x07, // 8-15
    0x07, 0x07, 0x07, 0x07, 0x07, 0x07, 0x1F, 0x1F, // 16-23
    0x07, 0x08, 0x08, 0x07, 0x07, 0x07, 0x07, 0x1F, // 24-31
    0x1F, 0x1F, 0x1F, 0x1F, 0x08, 0x00, 0x00, 0x07, // 32-39
    0x07, 0x07, 0x07, 0x18, 0x18, 0x18, 0x18, 0x1F, // 40-47
    0x1F, 0x1F, 0x1F, 0x1F, 0x1F, 0x1F, 0x1F, 0x07, // 48-55
    0x07, 0x07, 0x07, 0x18, 0x18, 0x18, 0x17, 0x18, // 56-63
];

/// Plant-policy values (`m_collision_bg.h`).
pub mod plant {
    pub const PLANT0: u8 = 0; // stay a sapling
    pub const PLANT1: u8 = 1; // (enum only - never emitted by the table)
    pub const PLANT2: u8 = 2; // grow to second stage
    pub const PLANT3: u8 = 3; // (enum only - never emitted by the table)
    pub const PLANT4: u8 = 4; // fully grow
    pub const KILL_PLANT: u8 = 7; // no growth; plants die
}

/// Policy-bit positions.
pub mod bit {
    pub const PLANT_MASK: u8 = 0x07;
    pub const PLACE: u8 = 1 << 3;
    pub const NPC: u8 = 1 << 4;
}

/// Raw attribute -> policy byte. Masks to 6 bits like the defensive
/// `attr & 0x3F` in `mCoBG_Attr2CheckPlaceNpc`.
pub fn attribute_action_info(attr: u32) -> u8 {
    ATTRIBUTE_ACTION_INFO[(attr & 0x3F) as usize]
}

/// `mCoBG_CheckPlace_OrgAttr` verbatim: bit 3 of the policy byte.
/// NOTE: no 0x3F mask here (unlike the NPC path) — the source indexes
/// directly, relying on the 6-bit collision field.
pub fn check_place_org_attr(org_attr: u32) -> bool {
    (ATTRIBUTE_ACTION_INFO[(org_attr & 0x3F) as usize] >> 3) & 1 == 1
}

/// `mCoBG_Attr2CheckPlaceNpc` verbatim: bit 4, with the `attr & 0x3F` mask.
pub fn attr2check_place_npc(attr: u32) -> bool {
    let info = attribute_action_info(attr);
    (info >> 4) & 1 == 1
}

/// `mCoBG_Attr2CheckPoorGround` verbatim (@unused/@fabricated in the
/// decomp — semantics clear, provenance less certain):
/// poor ground iff the plant policy is KILL_PLANT or PLANT0.
pub fn attr2check_poor_ground(attr: u32) -> bool {
    let plant = attribute_action_info(attr) & bit::PLANT_MASK;
    plant == plant::KILL_PLANT || plant == plant::PLANT0
}

/// Plant-policy result: `Ok(stage)` with stage in {0, 2, 4}, or
/// `Err(())` for "cannot plant here" (the source's -1).
///
/// `mCoBG_Attribute2CheckPlant` verbatim, minus the field-type gate and
/// the attribute-63 neighbor redirect (those need field/collision access;
/// see docs):
/// - field must be foreground (caller checks `mFI_FIELDTYPE2_FG`)
/// - attr 63: re-run on the +Z neighbor's raw attribute (caller handles)
/// - plant == KILL_PLANT -> Err; else Ok(plant)
pub fn attribute2check_plant(attr: u32) -> Result<u8, ()> {
    let plant = attribute_action_info(attr) & bit::PLANT_MASK;
    if plant == plant::KILL_PLANT {
        Err(())
    } else {
        Ok(plant)
    }
}

/// `mCoBG_Change2PoorAttr` mapping (rewrite.c_inc:159): fertile terrain
/// degrades to its poor variant. Returns the new attribute, or `None`
/// when unchanged. GRASS0/1 -> GRASS2, SOIL0/1 -> SOIL2.
pub fn change2poor_attr(attr: u8) -> Option<u8> {
    // Attribute numbers from m_collision_bg.h: GRASS0=0, GRASS1=1,
    // GRASS2=2, SOIL0=4, SOIL1=5, SOIL2=6.
    match attr {
        0 | 1 => Some(2),
        4 | 5 => Some(6),
        _ => None,
    }
}

// ---- C ABI ----

/// C ABI: placement permission for a raw attribute; 1 = allowed.
#[no_mangle]
pub extern "C" fn pc_check_place_attr(attr: u32) -> u8 {
    check_place_org_attr(attr) as u8
}

/// C ABI: NPC permission for a raw attribute; 1 = allowed.
#[no_mangle]
pub extern "C" fn pc_check_npc_attr(attr: u32) -> u8 {
    attr2check_place_npc(attr) as u8
}

/// C ABI: plant policy; returns 0/2/4, or 0xFF for "cannot plant" (-1).
#[no_mangle]
pub extern "C" fn pc_check_plant_attr(attr: u32) -> u8 {
    match attribute2check_plant(attr) {
        Ok(stage) => stage,
        Err(()) => 0xFF,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_spot_checks() {
        // Brief's examples, verified against the generated table.
        assert_eq!(attribute_action_info(0), 0x1C); // NPC|PLACE|PLANT4
        assert_eq!(attribute_action_info(3), 0x1F); // NPC|PLACE|KILL
        assert_eq!(attribute_action_info(63), 0x18); // NPC|PLACE|PLANT0
        // The river-bank asymmetry is at 62, not 61.
        assert_eq!(attribute_action_info(62), 0x17); // NPC|NO_PLACE|KILL
        assert_eq!(attribute_action_info(61), 0x18);
        // Wood bridge center (31) vs directional pieces (27-30).
        assert_eq!(attribute_action_info(31), 0x1F); // NPC|PLACE|KILL
        assert_eq!(attribute_action_info(27), 0x07); // NO_NPC|NO_PLACE|KILL
        // Wave (11) keeps saplings where water (12) kills.
        assert_eq!(attribute_action_info(11) & bit::PLANT_MASK, plant::PLANT0);
        assert_eq!(attribute_action_info(12) & bit::PLANT_MASK, plant::KILL_PLANT);
        // Masking: attr & 0x3F.
        assert_eq!(attribute_action_info(64), attribute_action_info(0));
        // Only PLANT0/PLANT2/PLANT4/KILL ever emitted.
        for b in ATTRIBUTE_ACTION_INFO {
            let p = b & bit::PLANT_MASK;
            assert!(p == 0 || p == 2 || p == 4 || p == 7, "attr plant bits = {}", p);
            assert_eq!(b & 0xE0, 0);
        }
    }

    #[test]
    fn consumer_semantics() {
        assert!(check_place_org_attr(0));
        assert!(!check_place_org_attr(12));
        assert!(!check_place_org_attr(62)); // the asymmetric river bank
        assert!(attr2check_place_npc(0));
        assert!(!attr2check_place_npc(12));
        assert!(attr2check_place_npc(62)); // NPC yes, PLACE no
        assert_eq!(attribute2check_plant(0), Ok(4));
        assert_eq!(attribute2check_plant(1), Ok(2));
        assert_eq!(attribute2check_plant(2), Ok(0));
        assert_eq!(attribute2check_plant(3), Err(()));
        assert!(attr2check_poor_ground(3)); // KILL -> poor
        assert!(attr2check_poor_ground(2)); // PLANT0 -> poor
        assert!(!attr2check_poor_ground(0)); // PLANT4 -> not poor
        assert_eq!(change2poor_attr(0), Some(2));
        assert_eq!(change2poor_attr(5), Some(6));
        assert_eq!(change2poor_attr(2), None);
        // C ABI.
        assert_eq!(pc_check_place_attr(62), 0);
        assert_eq!(pc_check_npc_attr(62), 1);
        assert_eq!(pc_check_plant_attr(0), 4);
        assert_eq!(pc_check_plant_attr(3), 0xFF);
    }
}
