//! Column construction for the Rust rewrite.
//!
//! Source-verified (upstream `src/game/m_collision_bg.c`,
//! `src/game/m_collision_bg_column.c_inc`):
//!
//! Columns are a separate collision layer from terrain wall
//! vectors: the engine compiles recognized foreground objects in
//! the local terrain neighborhood into compact vertical-cylinder
//! primitives (`mCoBG_column_c`: X/Z position, ground Y, top
//! height, radius, attribute-wall flag, unit coords), then resolves
//! them *before* the terrain wall-vector solver.
//!
//! Verified details:
//!
//! * Column storage is capped at 16 (`mCoBG_column_c column[16]`),
//!   vs 128 wall vectors. `mCoBG_MakeColumnCollisionData` walks the
//!   unit neighborhood in row-major order and stops building after
//!   16 *examined* slots — not the nearest 16, and it ignores the
//!   `MakeOneColumnCollisionData` return value, so the count is
//!   examined slots (failed slots stay zeroed via `bzero`) —
//!   confirmed quirk, harmless in the normal path.
//! * The actor's own unit is excluded (`ut_x == ux && ut_z == uz`
//!   → no column).
//! * Object→cylinder recipes are hard-coded constants:
//!   hole (19, ground Y, atr_wall=TRUE, requires old_on_ground);
//!   small/med/large/full trees (19, +30/+40/+60/+80); stumps
//!   (+30, radius 10 for the four `*_STUMP001` IDs else 18);
//!   rock (19, +31.5); mailbox (15, +50); sign (19, +45);
//!   special signboard (10, +45); koinobori/flag (19, +160).
//! * Normal column collision: actor not already inside at the old
//!   position, `height >= now_y + 3.0`, `dist < range + radius` →
//!   radial X/Z push (`rev_vec.y = 0`), plus wall-contact
//!   registration; `0 < dist − check_dist < 2.7` → contact only.
//! * Attribute columns (holes): ignored when the actor was not
//!   previously on ground; otherwise the same radial test.
//! * Pipeline order in `mCoBG_WallCheck`: object columns → decal
//!   columns → terrain wall vectors (`mCoBG_GetWallReverse`).
//!
//! Unknown: the original design rationale for the specific
//! radius/height constants. The decal-circle register/clear
//! machinery and the separate line-vs-column sweep routine are
//! documented gaps, not ported here.

/// Column record (`mCoBG_column_c`).
#[derive(Clone, Copy, Debug, Default)]
pub struct Column {
    pub pos: [f32; 3],
    pub height: f32,
    pub radius: f32,
    pub atr_wall: bool,
    pub ux: i32,
    pub uz: i32,
}

/// Recognized foreground-object classes for column construction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ColumnItemKind {
    Hole,
    SmallTree,
    MedTree,
    LargeTree,
    FullTree,
    StumpNarrow, // TREE_STUMP001 etc.: radius 10
    StumpWide,   // other stumps: radius 18
    Rock,
    Mailbox,
    Sign,
    SpecialSignboard,
    Koinobori,
}

/// Maximum column storage.
pub const COLUMN_MAX: usize = 16;

/// Contact-only band, shared with the wall solver.
pub const COLUMN_CONTACT_BAND: f32 = 2.7;

/// Vertical participation margin.
pub const COLUMN_HEIGHT_MARGIN: f32 = 3.0;

/// Build one column from a foreground item
/// (`mCoBG_MakeOneColumnCollisionData` core). `ground_y` is the
/// unit-center ground height; holes additionally require
/// `old_on_ground`. Returns None for unrecognized items.
pub fn make_one_column(
    kind: ColumnItemKind,
    ut_x: i32,
    ut_z: i32,
    ground_y: f32,
    old_on_ground: bool,
) -> Option<Column> {
    let mut col = Column {
        pos: [0.0, ground_y, 0.0],
        ux: ut_x,
        uz: ut_z,
        ..Default::default()
    };
    match kind {
        ColumnItemKind::Hole => {
            if !old_on_ground {
                return None;
            }
            col.radius = 19.0;
            col.height = ground_y;
            col.atr_wall = true;
        }
        ColumnItemKind::SmallTree => {
            col.radius = 19.0;
            col.height = ground_y + 30.0;
        }
        ColumnItemKind::MedTree => {
            col.radius = 19.0;
            col.height = ground_y + 40.0;
        }
        ColumnItemKind::LargeTree => {
            col.radius = 19.0;
            col.height = ground_y + 60.0;
        }
        ColumnItemKind::FullTree => {
            col.radius = 19.0;
            col.height = ground_y + 80.0;
        }
        ColumnItemKind::StumpNarrow => {
            col.radius = 10.0;
            col.height = ground_y + 30.0;
        }
        ColumnItemKind::StumpWide => {
            col.radius = 18.0;
            col.height = ground_y + 30.0;
        }
        ColumnItemKind::Rock => {
            col.radius = 19.0;
            col.height = ground_y + 31.5;
        }
        ColumnItemKind::Mailbox => {
            col.radius = 15.0;
            col.height = ground_y + 50.0;
        }
        ColumnItemKind::Sign => {
            col.radius = 19.0;
            col.height = ground_y + 45.0;
        }
        ColumnItemKind::SpecialSignboard => {
            col.radius = 10.0;
            col.height = ground_y + 45.0;
        }
        ColumnItemKind::Koinobori => {
            col.radius = 19.0;
            col.height = ground_y + 160.0;
        }
    }
    Some(col)
}

/// Build the column array (`mCoBG_MakeColumnCollisionData`): walk
/// the unit row-major order, build at most 16 *examined* slots.
/// Failed slots are returned as `None` (zeroed in the engine);
/// the actor's own unit is skipped before counting.
pub fn make_column_collision_data(
    units: &[(Option<ColumnItemKind>, i32, i32, f32)],
    actor_ux: i32,
    actor_uz: i32,
    old_on_ground: bool,
) -> Vec<Option<Column>> {
    let mut out = Vec::new();
    for &(kind, ux, uz, ground_y) in units {
        if out.len() >= COLUMN_MAX {
            break;
        }
        if ux == actor_ux && uz == actor_uz {
            continue; // actor's own unit: no column
        }
        let col = kind.and_then(|k| make_one_column(k, ux, uz, ground_y, old_on_ground));
        out.push(col);
    }
    out
}

/// Normal column collision test (`mCoBG_ColumnCheck_NormalWall`):
/// returns the X/Z push vector, or None for contact-only/ignore.
/// `was_inside` = actor was already inside at the old position;
/// `now_y` = ground_dist + pos.y.
pub fn column_check_normal(
    col: &Column,
    pos: [f32; 2],
    was_inside: bool,
    now_y: f32,
    range: f32,
) -> (Option<[f32; 2]>, bool) {
    if was_inside || col.height < now_y + COLUMN_HEIGHT_MARGIN {
        return (None, false);
    }
    let dx = pos[0] - col.pos[0];
    let dz = pos[1] - col.pos[2];
    let dist = (dx * dx + dz * dz).sqrt();
    let check_dist = range + col.radius;
    if dist < check_dist {
        let rev_dist = check_dist - dist;
        let inv = if dist == 0.0 { 1.0 } else { 1.0 / dist };
        (Some([dx * inv * rev_dist, dz * inv * rev_dist]), true)
    } else {
        let diff = dist - check_dist;
        (None, diff > 0.0 && diff < COLUMN_CONTACT_BAND)
    }
}

/// Attribute-column test (`mCoBG_ColumnCheck*_AttrWall`): ignored
/// unless the actor was previously on ground; otherwise the same
/// radial test without the height gate.
pub fn column_check_attr(
    col: &Column,
    pos: [f32; 2],
    was_inside: bool,
    old_on_ground: bool,
    range: f32,
) -> (Option<[f32; 2]>, bool) {
    if !old_on_ground || was_inside {
        return (None, false);
    }
    let dx = pos[0] - col.pos[0];
    let dz = pos[1] - col.pos[2];
    let dist = (dx * dx + dz * dz).sqrt();
    let check_dist = range + col.radius;
    if dist < check_dist {
        let rev_dist = check_dist - dist;
        let inv = if dist == 0.0 { 1.0 } else { 1.0 / dist };
        (Some([dx * inv * rev_dist, dz * inv * rev_dist]), true)
    } else {
        let diff = dist - check_dist;
        (None, diff > 0.0 && diff < COLUMN_CONTACT_BAND)
    }
}

/// C ABI: radius/height recipe for an item kind id (0-11 as in
/// `ColumnItemKind` order); writes radius, height, atr_wall flag.
/// Returns 1 when recognized.
///
/// NOTE: this is an *internal* lookup helper, NOT a retail boundary.
/// Retail `mCoBG_MakeOneColumnCollisionData` classifies by actual item ID,
/// which this ABI cannot represent -- use `pc_column_recipe_item` for
/// wiring. Do not call this from C expecting retail behavior.
/// Retail item-ID sets for column collision (`mCoBG_MakeOneColumnCollisionData`,
/// m_collision_bg_column.c_inc). Values resolved from m_name_table.h; the
/// IS_ITEM_*_TREE predicates are verbatim membership lists (the S0/S1/S2 macros
/// even repeat three entries, deduped here).
fn is_small_tree(item: u16) -> bool {
    matches!(item, 0x0801 | 0x0806 | 0x080E | 0x0816 | 0x081E | 0x0826 | 0x082E | 0x0833 | 0x0838 | 0x0850 | 0x0855 | 0x085E | 0x0864)
}
fn is_med_tree(item: u16) -> bool {
    matches!(item, 0x0802 | 0x0807 | 0x080F | 0x0817 | 0x081F | 0x0827 | 0x082F | 0x0834 | 0x0839 | 0x0851 | 0x0856 | 0x085F | 0x0865)
}
fn is_large_tree(item: u16) -> bool {
    matches!(item, 0x0803 | 0x0808 | 0x0810 | 0x0818 | 0x0820 | 0x0828 | 0x0830 | 0x0835 | 0x083A | 0x0852 | 0x0857 | 0x0860 | 0x0866)
}
fn is_full_tree(item: u16) -> bool {
    // Retail: IS_ITEM_FULL_TREE(item) || item == RSV_TREE.
    item == 0xFE1A || matches!(item, 0x005E | 0x005F | 0x0060 | 0x0061 | 0x0069 | 0x0078 | 0x0079 | 0x007A | 0x007F | 0x0080 | 0x0081 | 0x0082 | 0x0804 | 0x0809 | 0x080A | 0x080B | 0x080C | 0x0811 | 0x0812 | 0x0813 | 0x0814 | 0x0819 | 0x081A | 0x081B | 0x081C | 0x0821 | 0x0822 | 0x0823 | 0x0824 | 0x0829 | 0x082A | 0x082B | 0x082C | 0x0831 | 0x0836 | 0x083B | 0x0853 | 0x0858 | 0x0859 | 0x085A | 0x085B | 0x0861 | 0x0867 | 0x0868)
}
fn is_tree_stump(item: u16) -> bool {
    // Retail: four inclusive ranges (the macro has a missing-parens quirk
    // but && binds tighter than ||, so the ranges are as written).
    (0x0001..=0x0004).contains(&item)
        || (0x0070..=0x0073).contains(&item)
        || (0x0074..=0x0077).contains(&item)
        || (0x007B..=0x007E).contains(&item)
}
fn is_narrow_stump(item: u16) -> bool {
    // Retail: radius 10 only for the *001 of each stump family.
    matches!(item, 0x0001 | 0x0070 | 0x0074 | 0x007B)
}
fn is_rock(item: u16) -> bool {
    (0x0063..=0x0067).contains(&item)
        || (0x006A..=0x006E).contains(&item)
        || item == 0x006F
}
fn is_hole(item: u16) -> bool {
    (0x0011..=0x0029).contains(&item) || item == 0x005D || item == 0xFE19
}
fn is_mailbox(item: u16) -> bool {
    (0xF001..=0xF004).contains(&item)
}
fn is_sign(item: u16) -> bool {
    // Retail: item == DUMMY_RESERVE || ITEM_IS_SIGNBOARD(item).
    item == 0xF102 || (0x0900..=0x0920).contains(&item)
}

/// Classify a retail item ID into a `ColumnItemKind`, in the exact branch
/// order of `mCoBG_MakeOneColumnCollisionData`. The hole branch is gated on
/// `old_on_ground` exactly as in retail; every other branch is item-only.
/// Returns `None` for items that produce no column.
pub fn column_kind_for_item(item: u16, old_on_ground: bool) -> Option<ColumnItemKind> {
    if old_on_ground && is_hole(item) {
        return Some(ColumnItemKind::Hole);
    }
    if is_small_tree(item) {
        return Some(ColumnItemKind::SmallTree);
    }
    if is_med_tree(item) {
        return Some(ColumnItemKind::MedTree);
    }
    if is_large_tree(item) {
        return Some(ColumnItemKind::LargeTree);
    }
    if is_full_tree(item) {
        return Some(ColumnItemKind::FullTree);
    }
    if is_tree_stump(item) {
        return Some(if is_narrow_stump(item) {
            ColumnItemKind::StumpNarrow
        } else {
            ColumnItemKind::StumpWide
        });
    }
    if is_rock(item) {
        return Some(ColumnItemKind::Rock);
    }
    if is_mailbox(item) {
        return Some(ColumnItemKind::Mailbox);
    }
    if is_sign(item) {
        return Some(ColumnItemKind::Sign);
    }
    if item == 0xFE30 {
        // RSV_SIGNBOARD (m_name_table.h).
        return Some(ColumnItemKind::SpecialSignboard);
    }
    if item == 0xF114 || item == 0xF122 {
        // DUMMY_KOINOBORI / DUMMY_FLAG (m_name_table.h).
        return Some(ColumnItemKind::Koinobori);
    }
    None
}

/// C ABI: retail-compatible column recipe. Takes the actual
/// `mActor_name_t` item ID (not a Rust kind id), classifies it exactly as
/// `mCoBG_MakeOneColumnCollisionData` does, and writes radius, height
/// (`ground_y` + retail offset), and the atr_wall flag. Returns 1 when the
/// item produces a column, 0 otherwise. This is the Wave 1 wireable
/// boundary for `mCoBG_MakeOneColumnCollisionData`.
#[no_mangle]
pub extern "C" fn pc_column_recipe_item(
    item: u16,
    ground_y: f32,
    old_on_ground: u8,
    out_radius: *mut f32,
    out_height: *mut f32,
    out_atr: *mut u8,
) -> u8 {
    let kind = match column_kind_for_item(item, old_on_ground != 0) {
        Some(k) => k,
        None => return 0,
    };
    let col = match make_one_column(kind, 0, 0, ground_y, old_on_ground != 0) {
        Some(c) => c,
        None => return 0,
    };
    if !out_radius.is_null() {
        unsafe { *out_radius = col.radius };
    }
    if !out_height.is_null() {
        unsafe { *out_height = col.height };
    }
    if !out_atr.is_null() {
        unsafe { *out_atr = col.atr_wall as u8 };
    }
    1
}

#[no_mangle]
pub extern "C" fn pc_column_recipe(
    kind_id: u8,
    ground_y: f32,
    old_on_ground: u8,
    out_radius: *mut f32,
    out_height: *mut f32,
    out_atr: *mut u8,
) -> u8 {
    let kinds = [
        ColumnItemKind::Hole,
        ColumnItemKind::SmallTree,
        ColumnItemKind::MedTree,
        ColumnItemKind::LargeTree,
        ColumnItemKind::FullTree,
        ColumnItemKind::StumpNarrow,
        ColumnItemKind::StumpWide,
        ColumnItemKind::Rock,
        ColumnItemKind::Mailbox,
        ColumnItemKind::Sign,
        ColumnItemKind::SpecialSignboard,
        ColumnItemKind::Koinobori,
    ];
    let kind = match kinds.get(kind_id as usize) {
        Some(k) => *k,
        None => return 0,
    };
    let col = match make_one_column(kind, 0, 0, ground_y, old_on_ground != 0) {
        Some(c) => c,
        None => return 0,
    };
    if !out_radius.is_null() {
        unsafe { *out_radius = col.radius };
    }
    if !out_height.is_null() {
        unsafe { *out_height = col.height };
    }
    if !out_atr.is_null() {
        unsafe { *out_atr = col.atr_wall as u8 };
    }
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn item_id_classification() {
        // Spot checks against m_name_table.h values.
        assert_eq!(column_kind_for_item(0x0801, true), Some(ColumnItemKind::SmallTree)); // TREE_S0
        assert_eq!(column_kind_for_item(0x0807, true), Some(ColumnItemKind::MedTree)); // TREE_APPLE_S1
        assert_eq!(column_kind_for_item(0x0803, true), Some(ColumnItemKind::LargeTree)); // TREE_S2
        assert_eq!(column_kind_for_item(0x0804, true), Some(ColumnItemKind::FullTree)); // TREE
        assert_eq!(column_kind_for_item(0xFE1A, true), Some(ColumnItemKind::FullTree)); // RSV_TREE
        assert_eq!(column_kind_for_item(0x0001, true), Some(ColumnItemKind::StumpNarrow)); // TREE_STUMP001
        assert_eq!(column_kind_for_item(0x0070, true), Some(ColumnItemKind::StumpNarrow)); // TREE_PALM_STUMP001
        assert_eq!(column_kind_for_item(0x0002, true), Some(ColumnItemKind::StumpWide)); // TREE_STUMP002
        assert_eq!(column_kind_for_item(0x0063, true), Some(ColumnItemKind::Rock)); // ROCK_A
        assert_eq!(column_kind_for_item(0x006F, true), Some(ColumnItemKind::Rock)); // MONEY_FLOWER_SEED
        assert_eq!(column_kind_for_item(0x0011, true), Some(ColumnItemKind::Hole)); // HOLE_START
        assert_eq!(column_kind_for_item(0x005D, true), Some(ColumnItemKind::Hole)); // HOLE_SHINE
        assert_eq!(column_kind_for_item(0xFE19, true), Some(ColumnItemKind::Hole)); // RSV_HOLE
        assert!(column_kind_for_item(0x0011, false).is_none()); // hole needs old_on_ground
        assert_eq!(column_kind_for_item(0xF001, true), Some(ColumnItemKind::Mailbox)); // DUMMY_MAILBOX0
        assert_eq!(column_kind_for_item(0xF102, true), Some(ColumnItemKind::Sign)); // DUMMY_RESERVE
        assert_eq!(column_kind_for_item(0x0900, true), Some(ColumnItemKind::Sign)); // SIGNBOARD_START
        assert_eq!(column_kind_for_item(0xFE30, true), Some(ColumnItemKind::SpecialSignboard)); // RSV_SIGNBOARD
        assert_eq!(column_kind_for_item(0xF114, true), Some(ColumnItemKind::Koinobori)); // DUMMY_KOINOBORI
        assert_eq!(column_kind_for_item(0xF122, true), Some(ColumnItemKind::Koinobori)); // DUMMY_FLAG
        assert!(column_kind_for_item(0x0000, true).is_none()); // EMPTY_NO: no column
        assert!(column_kind_for_item(0xFFFF, true).is_none());
    }

    #[test]
    fn item_recipes() {
        let t = |k| make_one_column(k, 1, 2, 100.0, true).unwrap();
        assert_eq!((t(ColumnItemKind::SmallTree).radius, t(ColumnItemKind::SmallTree).height), (19.0, 130.0));
        assert_eq!(t(ColumnItemKind::MedTree).height, 140.0);
        assert_eq!(t(ColumnItemKind::LargeTree).height, 160.0);
        assert_eq!(t(ColumnItemKind::FullTree).height, 180.0);
        assert_eq!((t(ColumnItemKind::StumpNarrow).radius, t(ColumnItemKind::StumpNarrow).height), (10.0, 130.0));
        assert_eq!((t(ColumnItemKind::StumpWide).radius, t(ColumnItemKind::StumpWide).height), (18.0, 130.0));
        assert_eq!(t(ColumnItemKind::Rock).height, 131.5);
        assert_eq!((t(ColumnItemKind::Mailbox).radius, t(ColumnItemKind::Mailbox).height), (15.0, 150.0));
        assert_eq!(t(ColumnItemKind::Sign).height, 145.0);
        assert_eq!((t(ColumnItemKind::SpecialSignboard).radius, t(ColumnItemKind::SpecialSignboard).height), (10.0, 145.0));
        assert_eq!(t(ColumnItemKind::Koinobori).height, 260.0);
        let h = t(ColumnItemKind::Hole);
        assert!(h.atr_wall && (h.height - 100.0).abs() < 1e-6);
        assert!(make_one_column(ColumnItemKind::Hole, 1, 2, 100.0, false).is_none());
    }

    #[test]
    fn sixteen_slot_limit_and_own_unit() {
        let mut units = Vec::new();
        for i in 0..25 {
            units.push((Some(ColumnItemKind::SmallTree), i, 0, 0.0));
        }
        let cols = make_column_collision_data(&units, 99, 99, true);
        assert_eq!(cols.len(), COLUMN_MAX);
        let cols = make_column_collision_data(&units, 5, 0, true);
        assert_eq!(cols.len(), COLUMN_MAX); // own unit skipped before counting
        // Unrecognized slot counts as an examined slot.
        let units = [
            (None, 0, 0, 0.0),
            (Some(ColumnItemKind::Rock), 1, 0, 0.0),
        ];
        let cols = make_column_collision_data(&units, 99, 99, true);
        assert_eq!(cols.len(), 2);
        assert!(cols[0].is_none());
        assert!(cols[1].is_some());
    }

    #[test]
    fn normal_collision_math() {
        let col = make_one_column(ColumnItemKind::Rock, 0, 0, 0.0, true).unwrap();
        // Actor 10 units from center, range 5 -> overlap of (5+19)-10=14.
        let (push, contact) = column_check_normal(&col, [10.0, 0.0], false, 0.0, 5.0);
        let p = push.unwrap();
        assert!(contact && (p[0] - 14.0).abs() < 1e-4 && p[1].abs() < 1e-4);
        // Height gate: column top below now_y + 3.
        let (push, _) = column_check_normal(&col, [10.0, 0.0], false, 100.0, 5.0);
        assert!(push.is_none());
        // Already inside at old position: ignore.
        let (push, _) = column_check_normal(&col, [10.0, 0.0], true, 0.0, 5.0);
        assert!(push.is_none());
        // Near miss within 2.7: contact only.
        let (push, contact) = column_check_normal(&col, [26.0, 0.0], false, 0.0, 5.0);
        assert!(push.is_none() && contact);
        let (push, contact) = column_check_normal(&col, [30.0, 0.0], false, 0.0, 5.0);
        assert!(push.is_none() && !contact);
    }

    #[test]
    fn attr_column_gate() {
        let col = make_one_column(ColumnItemKind::Hole, 0, 0, 0.0, true).unwrap();
        let (push, _) = column_check_attr(&col, [10.0, 0.0], false, false, 5.0);
        assert!(push.is_none());
        let (push, contact) = column_check_attr(&col, [10.0, 0.0], false, true, 5.0);
        assert!(push.is_some() && contact);
    }

    #[test]
    fn recipe_abi_shape() {
        let mut r = 0.0f32;
        let mut h = 0.0f32;
        let mut a = 0u8;
        assert_eq!(pc_column_recipe(7, 50.0, 1, &mut r, &mut h, &mut a), 1);
        assert_eq!((r, h, a), (19.0, 81.5, 0));
        assert_eq!(pc_column_recipe(0, 50.0, 0, &mut r, &mut h, &mut a), 0);
        assert_eq!(pc_column_recipe(99, 50.0, 1, &mut r, &mut h, &mut a), 0);
    }
}
