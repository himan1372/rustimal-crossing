//! Town map overlay (m_map_ovl.c) pure kernels.
//!
//! Verified against GAFE01_00 Rev 0 decomp (`src/game/m_map_ovl.c`,
//! `include/m_map_ovl.h`, `include/m_map_ovl_h.h`). Only deterministic,
//! engine-free helpers are ported here: block-type -> map-visual mapping,
//! bridge substitution, background-kind filtering, house icon layer
//! conversion and placement, building-label precedence, and the cursor
//! selection/animation inputs.
//!
//! The following stay in C until Rust/C equivalence is proven:
//! allocation and overlay lifecycle (`mMP_map_ovl_construct`/
//! `mMP_map_ovl_destruct`), all save/field access (`Save_Get(bridge...)`,
//! `Save_Get(private_data)`, `Save_Get(animals)`, combination tables),
//! `mFI_Wpos2BlockNum`, collision-layer queries, NPC/player identity and
//! name conversion, private/animal free checks, input callbacks,
//! transitions, sound effects, font rendering, matrices, display lists,
//! textures, palettes, and the villager sort/cache assembly.

#![allow(non_snake_case)]

use std::os::raw::c_int;

/// Interior map width in acres (FG_BLOCK_X_NUM).
pub const MAP_BLOCKX: i32 = 5;
/// Interior map depth in acres (FG_BLOCK_Z_NUM).
pub const MAP_BLOCKZ: i32 = 6;
/// Interior map acres total (FG_BLOCK_TOTAL_NUM).
pub const MAP_TOTAL: i32 = 30;
/// Map overlay acre spacing (`mMP_BLOCK_SIZE_F`).
pub const MAP_BLOCK_SIZE_F: f32 = 22.0;
/// Cursor pulse animation frames (`mMP_CURSOR_FRAMES`).
pub const MAP_CURSOR_FRAMES: i32 = 18;
/// Resident pointers stored per map block
/// (`mMP_RESIDENTS_PER_BLOCK_MAX`).
pub const MAP_RESIDENTS_PER_BLOCK_MAX: i32 = 4;

/// Number of field block types (`mFM_BLOCK_TYPE_NUM`).
pub const MAP_BLOCK_TYPE_NUM: i32 = 108;
/// Number of distinct `kan_tizu_*` map textures in `l_map_texture`.
pub const MAP_TEXTURE_NUM: i32 = 69;
/// Palette index 0: `kan_tizu1_pal`.
pub const MAP_PAL_1: i32 = 0;
/// Palette index 1: `kan_tizu2_pal`.
pub const MAP_PAL_2: i32 = 1;

/// Label ids, in `enum label_type` order (`m_map_ovl.h`).
pub const LABEL_NPC: i32 = 0;
pub const LABEL_PLAYER: i32 = 1;
pub const LABEL_SHOP: i32 = 2;
pub const LABEL_POLICE: i32 = 3;
pub const LABEL_POST: i32 = 4;
pub const LABEL_SHRINE: i32 = 5;
pub const LABEL_STATION: i32 = 6;
pub const LABEL_JUNK: i32 = 7;
pub const LABEL_MUSEUM: i32 = 8;
pub const LABEL_NEEDLE: i32 = 9;
pub const LABEL_PORT: i32 = 10;
pub const LABEL_NUM: i32 = 11;

/// Distinct `kan_tizu_*` texture names backing `MAP_TEXTURE_TABLE`,
/// in order of first appearance in `l_map_texture` (`m_map_ovl.c`).
pub const MAP_TEXTURE_NAMES: [&str; MAP_TEXTURE_NUM as usize] = [
    "kan_tizu_f_TA_tex_txt",    // 0
    "kan_tizu_tst1_TA_tex_txt", // 1
    "kan_tizu_t_TA_tex_txt",    // 2
    "kan_tizu_tr1_TA_tex_txt",  // 3
    "kan_tizu_fmh_TA_tex_txt",  // 4
    "kan_tizu_c1_TA_tex_txt",   // 5
    "kan_tizu_c2_TA_tex_txt",   // 6
    "kan_tizu_c3_TA_tex_txt",   // 7
    "kan_tizu_c4_TA_tex_txt",   // 8
    "kan_tizu_c5_TA_tex_txt",   // 9
    "kan_tizu_c6_TA_tex_txt",   // 10
    "kan_tizu_c7_TA_tex_txt",   // 11
    "kan_tizu_c1r1_TA_tex_txt", // 12
    "kan_tizu_c2r1_TA_tex_txt", // 13
    "kan_tizu_c3r1_TA_tex_txt", // 14
    "kan_tizu_c4r1_TA_tex_txt", // 15
    "kan_tizu_c5r1_TA_tex_txt", // 16
    "kan_tizu_c6r1_TA_tex_txt", // 17
    "kan_tizu_c7r1_TA_tex_txt", // 18
    "kan_tizu_c1r2_TA_tex_txt", // 19
    "kan_tizu_c2r2_TA_tex_txt", // 20
    "kan_tizu_c3r2_TA_tex_txt", // 21
    "kan_tizu_c4r2_TA_tex_txt", // 22
    "kan_tizu_c5r2_TA_tex_txt", // 23
    "kan_tizu_c6r3_TA_tex_txt", // 24
    "kan_tizu_c7r3_TA_tex_txt", // 25
    "kan_tizu_r1_TA_tex_txt",   // 26
    "kan_tizu_r2_TA_tex_txt",   // 27
    "kan_tizu_r4_TA_tex_txt",   // 28
    "kan_tizu_r5_TA_tex_txt",   // 29
    "kan_tizu_r6_TA_tex_txt",   // 30
    "kan_tizu_r7_TA_tex_txt",   // 31
    "kan_tizu_r1b_TA_tex_txt",  // 32
    "kan_tizu_r2b_TA_tex_txt",  // 33
    "kan_tizu_r4b_TA_tex_txt",  // 34
    "kan_tizu_r5b_TA_tex_txt",  // 35
    "kan_tizu_r6b_TA_tex_txt",  // 36
    "kan_tizu_r7b_TA_tex_txt",  // 37
    "kan_tizu_c1s_TA_tex_txt",  // 38
    "kan_tizu_c2s_TA_tex_txt",  // 39
    "kan_tizu_c3s_TA_tex_txt",  // 40
    "kan_tizu_c4s_TA_tex_txt",  // 41
    "kan_tizu_c5s_TA_tex_txt",  // 42
    "kan_tizu_c6s_TA_tex_txt",  // 43
    "kan_tizu_c7s_TA_tex_txt",  // 44
    "kan_tizu_m_TA_tex_txt",    // 45
    "kan_tizu_mr1_TA_tex_txt",  // 46
    "kan_tizu_fsh_TA_tex_txt",  // 47
    "kan_tizu_fpk_TA_tex_txt",  // 48
    "kan_tizu_fpo_TA_tex_txt",  // 49
    "kan_tizu_fko_TA_tex_txt",  // 50
    "kan_tizu_pr1_TA_tex_txt",  // 51
    "kan_tizu_pr2_TA_tex_txt",  // 52
    "kan_tizu_pr4_TA_tex_txt",  // 53
    "kan_tizu_pr5_TA_tex_txt",  // 54
    "kan_tizu_pr6_TA_tex_txt",  // 55
    "kan_tizu_pr7_TA_tex_txt",  // 56
    "kan_tizu_mr1b_TA_tex_txt", // 57
    "kan_tizu_fmu_TA_tex_txt",  // 58
    "kan_tizu_fta_TA_tex_txt",  // 59
    "kan_tizu_tr1b_TA_tex_txt", // 60
    "kan_tizu_c1r2b_TA_tex_txt", // 61
    "kan_tizu_c3r1b_TA_tex_txt", // 62
    "kan_tizu_c4r1b_TA_tex_txt", // 63
    "kan_tizu_c4r2b_TA_tex_txt", // 64
    "kan_tizu_c5r2b_TA_tex_txt", // 65
    "kan_tizu_c6r1b_TA_tex_txt", // 66
    "kan_tizu_c7r1b_TA_tex_txt", // 67
    "kan_tizu_mwf_TA_tex_txt",  // 68
];

/// Block type -> map-texture index, verbatim from `l_map_texture`
/// (`m_map_ovl.c:91-200`), compressed to indices into
/// [`MAP_TEXTURE_NAMES`].
pub const MAP_TEXTURE_TABLE: [u8; MAP_BLOCK_TYPE_NUM as usize] = [
     0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  1,
     2,  3,  4,  5,  6,  7,  8,  9, 10, 11, 12, 13,
    14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 19, 22,
    23, 24, 25,  0, 26, 27, 27, 28, 29, 30, 31, 32,
    33, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43,
    44,  0,  0, 45, 46, 47, 48, 49, 50, 51, 52, 52,
    53, 54, 55, 56,  0, 26,  5,  5,  5,  5, 57, 46,
    58, 59, 60, 61, 62, 63, 64, 65, 66, 67, 67, 67,
    67, 67, 67, 67, 68, 68, 68, 68, 68, 61, 64, 65,
];

/// Block type -> palette selector, verbatim from `l_map_pal`
/// (`m_map_ovl.c:202-311`). 1 = `kan_tizu2_pal`, 0 = `kan_tizu1_pal`.
pub const MAP_PAL_TABLE: [u8; MAP_BLOCK_TYPE_NUM as usize] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
    1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 1, 1, 1, 1, 1, 1, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1,
    1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 1, 1, 1, 1, 1, 0, 0, 0,
];

/// Block type -> bridge-substituted block type, verbatim from
/// `pluss_bridge` (`m_map_ovl.c:313-421`). `0xFF` (`mFM_BLOCK_TYPE_NONE`)
/// means no bridge replacement. Verified non-`0xFF` entries:
/// 13->86, 24->88, 25->89, 27->92, 28->93, 29->87, 32->90, 33->91,
/// 34->105, 35->106, 36->107, 40->47, 41->48, 42->49, 43->50, 44->51,
/// 45->52, 46->53, 64->82.
pub const MAP_PLUSS_BRIDGE: [u8; MAP_BLOCK_TYPE_NUM as usize] = {
    const N: u8 = 0xFF;
    [
          N,   N,   N,   N,   N,   N,   N,   N,   N,   N,   N,   N,
          N,  86,   N,   N,   N,   N,   N,   N,   N,   N,   N,   N,
         88,  89,   N,  92,  93,  87,   N,   N,  90,  91, 105, 106,
        107,   N,   N,   N,  47,  48,  49,  50,  51,  52,  53,   N,
          N,   N,   N,   N,   N,   N,   N,   N,   N,   N,   N,   N,
          N,   N,   N,   N,  82,   N,   N,   N,   N,   N,   N,   N,
          N,   N,   N,   N,   N,   N,   N,   N,   N,   N,   N,   N,
          N,   N,   N,   N,   N,   N,   N,   N,   N,   N,   N,   N,
          N,   N,   N,   N,   N,   N,   N,   N,   N,   N,   N,   N,
    ]
};

/// Cursor pulse brightness (green prim-color component), verbatim from
/// the table in `mMP_set_cursol_dl` (`m_map_ovl.c`).
pub const MAP_CURSOR_BRIGHTNESS: [u8; MAP_CURSOR_FRAMES as usize] = [
    0, 1, 2, 5, 10, 20, 50, 75, 90, 100, 90, 75, 50, 20, 10, 5, 2, 1,
];

/// Cursor pulse scale (X and Y), verbatim from the table in
/// `mMP_set_cursol_dl` (`m_map_ovl.c`).
pub const MAP_CURSOR_SCALE: [f32; MAP_CURSOR_FRAMES as usize] = [
    1.0, 1.015, 1.03, 1.04, 1.05, 1.06, 1.07, 1.08, 1.09, 1.1, 1.09, 1.08,
    1.07, 1.06, 1.05, 1.04, 1.03, 1.015,
];

/// House icon offsets within an acre, from `mMP_set_house_dl`
/// (`m_map_ovl.c`).
pub const MAP_HOUSE_OFFSET_X: [f32; 3] = [5.0, 13.0, 17.0];
/// House icon offsets within an acre, from `mMP_set_house_dl`
/// (`m_map_ovl.c`).
pub const MAP_HOUSE_OFFSET_Y: [f32; 3] = [-4.0, -11.0, -18.0];

/// Town-name colors from `land_color` in `mMP_set_dl` (`m_map_ovl.c`),
/// selected by `menu->data0`: 0 = magenta, 1 = blue.
pub const MAP_LAND_COLOR: [[u8; 4]; 2] = [
    [255, 0, 255, 255],
    [60, 60, 255, 255],
];

/// Building block-kind flags in the ordered `if/else if` chain of
/// `mMP_set_field_data` (`m_map_ovl.c`), with the label each assigns.
const BUILDING_CHAIN: [(u32, i32); 9] = [
    (1 << 1, LABEL_SHOP),    // mBK_SHOP
    (1 << 3, LABEL_POLICE),  // mBK_POLICE
    (1 << 4, LABEL_POST),    // mBK_POSTOFFICE
    (1 << 2, LABEL_SHRINE),  // mBK_SHRINE
    (1 << 5, LABEL_STATION), // mBK_STATION
    (1 << 16, LABEL_JUNK),   // mBK_DUMP
    (1 << 17, LABEL_MUSEUM), // mBK_MUSEUM
    (1 << 19, LABEL_NEEDLE), // mBK_TAILORS
    (1 << 30, LABEL_PORT),   // mBK_DOCK
];

/// `mMP_check_bg_kind` (`m_map_ovl.c:632`): returns 1 when the block
/// kind may contribute a map texture, 0 when rejected. Rejects kinds
/// 0-8 (`BORDER_CLIFF_CORNER_TOP_RIGHT` and below), the two border-cliff
/// transitions (61, 62) and the two tunnels (9, 10).
#[no_mangle]
pub extern "C" fn pc_map_check_bg_kind(kind: c_int) -> c_int {
    let ok = !(kind <= 8 || kind == 61 || kind == 62 || kind == 9 || kind == 10);
    ok as c_int
}

/// `l_map_texture` lookup: returns the map-texture index for a field
/// block type, or -1 when `type_id` is out of range.
#[no_mangle]
pub extern "C" fn pc_map_tex_idx(type_id: c_int) -> c_int {
    if (0..MAP_BLOCK_TYPE_NUM).contains(&type_id) {
        MAP_TEXTURE_TABLE[type_id as usize] as c_int
    } else {
        -1
    }
}

/// `l_map_pal` lookup: returns the palette selector (0 or 1) for a
/// field block type, or -1 when `type_id` is out of range.
#[no_mangle]
pub extern "C" fn pc_map_pal_idx(type_id: c_int) -> c_int {
    if (0..MAP_BLOCK_TYPE_NUM).contains(&type_id) {
        MAP_PAL_TABLE[type_id as usize] as c_int
    } else {
        -1
    }
}

/// Bridge substitution from `mMP_make_max_no_table` (`m_map_ovl.c:643`):
/// returns `pluss_bridge[type_id]` when the saved bridge exists at the
/// current acre (`bridge_bx`, `bridge_bz`) and the table entry is valid,
/// otherwise `type_id`. Returns -1 when `type_id` is out of range.
#[no_mangle]
pub extern "C" fn pc_map_bridge_type(
    type_id: c_int,
    bridge_exists: c_int,
    bridge_bx: c_int,
    bridge_bz: c_int,
    bx: c_int,
    bz: c_int,
) -> c_int {
    if !(0..MAP_BLOCK_TYPE_NUM).contains(&type_id) {
        return -1;
    }
    let sub = MAP_PLUSS_BRIDGE[type_id as usize];
    if bridge_exists != 0 && bridge_bx == bx && bridge_bz == bz && sub != 0xFF {
        sub as c_int
    } else {
        type_id
    }
}

/// `mMP_check_layer` (`m_map_ovl.c`): converts the collision layer
/// (`mCoBG_Height2GetLayer` result: 0/1/2, anything else = default)
/// into the map icon layer. `field_step3` is the result of
/// `mRF_CheckFieldStep3`. Subtracts one when the field is not step 3
/// and clamps negative results to zero.
#[no_mangle]
pub extern "C" fn pc_map_check_layer(collision_layer: c_int, field_step3: c_int) -> c_int {
    let mut layer = match collision_layer {
        0 => 2,
        1 => 1,
        2 => 0,
        _ => 1,
    };
    if field_step3 == 0 {
        layer -= 1;
    }
    layer.max(0)
}

/// House-position lookup from `mMP_set_house_data` (`m_map_ovl.c`):
/// `entries` points to 3 `(x, z)` pairs (`mMP_house_pos_list`). Returns
/// the index of the first entry whose x equals `ut_x` and whose z
/// equals `ut_z - 1`; falls back to entry 0 when none match.
#[no_mangle]
pub extern "C" fn pc_map_house_pos_match(ut_x: c_int, ut_z: c_int, entries: *const c_int) -> c_int {
    if entries.is_null() {
        return 0;
    }
    let entries = unsafe { std::slice::from_raw_parts(entries, 6) };
    for i in 0..3 {
        if entries[2 * i] == ut_x && entries[2 * i + 1] == ut_z - 1 {
            return i as c_int;
        }
    }
    0
}

/// Building-label precedence from the ordered `if/else if` chain in
/// `mMP_set_field_data` (`m_map_ovl.c`). Returns the label id for the
/// first matching block-kind flag, or -1 when none match.
#[no_mangle]
pub extern "C" fn pc_map_building_label(kind_flags: u32) -> c_int {
    for (flag, label) in BUILDING_CHAIN {
        if kind_flags & flag != 0 {
            return label;
        }
    }
    -1
}

/// Trigger bit constants for [`pc_map_cursor_step`].
pub const MAP_TRIG_A: u32 = 0x0001;
pub const MAP_TRIG_B: u32 = 0x0002;
pub const MAP_TRIG_START: u32 = 0x0004;
pub const MAP_TRIG_X: u32 = 0x0008;
pub const MAP_TRIG_CLEFT: u32 = 0x0010;
pub const MAP_TRIG_CRIGHT: u32 = 0x0020;
pub const MAP_TRIG_CUP: u32 = 0x0040;
pub const MAP_TRIG_CDOWN: u32 = 0x0080;

/// Cursor step result codes.
pub const MAP_CURSOR_EXIT: i32 = 0;
pub const MAP_CURSOR_MOVED: i32 = 1;
pub const MAP_CURSOR_STAYED: i32 = 2;

/// Pure selection/bounds kernel of `mMP_move_Play` (`m_map_ovl.c`).
///
/// `trigger` is the C-button/face-button trigger mask (`overlay->
/// menu_control.trigger`), `data0` is `menu->data0`. `out` receives
/// `[new_sel_bx, new_sel_bz, target_dx, target_dz]`.
///
/// Order mirrors the source else-if chain: A/B/Start exit first
/// (X exits only when `data0 == 1`), then C-left (bx > 0), C-down
/// (bz < 5), C-up (bz > 0), C-right (bx < 4), each shifting the target
/// by exactly 22.0 in the relevant direction. Returns
/// [`MAP_CURSOR_EXIT`], [`MAP_CURSOR_MOVED`] or [`MAP_CURSOR_STAYED`].
#[no_mangle]
pub extern "C" fn pc_map_cursor_step(
    sel_bx: c_int,
    sel_bz: c_int,
    trigger: u32,
    data0: c_int,
    out: *mut c_int,
) -> c_int {
    let mut bx = sel_bx;
    let mut bz = sel_bz;
    let mut dx: c_int = 0;
    let mut dz: c_int = 0;
    let result: c_int;

    if trigger & (MAP_TRIG_A | MAP_TRIG_B | MAP_TRIG_START) != 0
        || (trigger & MAP_TRIG_X != 0 && data0 == 1)
    {
        result = MAP_CURSOR_EXIT;
    } else if trigger & MAP_TRIG_CLEFT != 0 && bx > 0 {
        bx -= 1;
        dx = -22;
        result = MAP_CURSOR_MOVED;
    } else if trigger & MAP_TRIG_CDOWN != 0 && bz < MAP_BLOCKZ - 1 {
        bz += 1;
        dz = -22;
        result = MAP_CURSOR_MOVED;
    } else if trigger & MAP_TRIG_CUP != 0 && bz > 0 {
        bz -= 1;
        dz = 22;
        result = MAP_CURSOR_MOVED;
    } else if trigger & MAP_TRIG_CRIGHT != 0 && bx < MAP_BLOCKX - 1 {
        bx += 1;
        dx = 22;
        result = MAP_CURSOR_MOVED;
    } else {
        result = MAP_CURSOR_STAYED;
    }

    if !out.is_null() {
        unsafe {
            *out.add(0) = bx;
            *out.add(1) = bz;
            *out.add(2) = dx;
            *out.add(3) = dz;
        }
    }
    result
}

/// Cursor animation tables: given `cursor_frame`, writes the pulse
/// brightness (green prim-color component) and scale from the
/// 18-entry tables into `out` (`[brightness, scale]`). The frame
/// wraps modulo 18 like the source's `cursor_frame %= 18`.
#[no_mangle]
pub extern "C" fn pc_map_cursor_pulse(frame: c_int, out: *mut c_int) {
    let i = frame.rem_euclid(MAP_CURSOR_FRAMES) as usize;
    if !out.is_null() {
        unsafe {
            *out.add(0) = MAP_CURSOR_BRIGHTNESS[i] as c_int;
            *out.add(1) = (MAP_CURSOR_SCALE[i] * 1000.0) as c_int;
        }
    }
}

/// Initial cursor state from `mMP_set_init_data` (`m_map_ovl.c`):
/// `out` receives `[x, z, frame]` where `x = bx * 22.0`,
/// `z = -bz * 22.0` and `frame = 0`.
#[no_mangle]
pub extern "C" fn pc_map_cursor_init(bx: c_int, bz: c_int, out: *mut f32) {
    if !out.is_null() {
        unsafe {
            *out.add(0) = bx as f32 * MAP_BLOCK_SIZE_F;
            *out.add(1) = -(bz as f32) * MAP_BLOCK_SIZE_F;
            *out.add(2) = 0.0;
        }
    }
}

/// Window-frame selection from `mMP_set_win_dl` (`m_map_ovl.c`):
/// returns 4 (`kan_win_waku4T`) for `label_cnt == 4`, 3 (`waku3T`)
/// for 3, 1 (`wakuT`) for 1, 0 (`waku1T`) for 0, and 2 (`waku2T`)
/// otherwise.
#[no_mangle]
pub extern "C" fn pc_map_win_frame(label_cnt: c_int) -> c_int {
    match label_cnt {
        4 => 4,
        3 => 3,
        1 => 1,
        0 => 0,
        _ => 2,
    }
}

/// `mMP_get_label_cnt` (`m_map_ovl.c`): returns the true resident
/// count for NPC and player labels, 1 for building labels.
#[no_mangle]
pub extern "C" fn pc_map_label_top_cnt(label_no: c_int, label_cnt: c_int) -> c_int {
    if label_no == LABEL_NPC || label_no == LABEL_PLAYER {
        label_cnt
    } else {
        1
    }
}

/// House-icon offset from `mMP_set_house_dl` (`m_map_ovl.c`): `out`
/// receives `[offset_x, offset_y]` selected by `house_idx % 3` and
/// `house_idx / 3`.
#[no_mangle]
pub extern "C" fn pc_map_house_offset(house_idx: c_int, out: *mut f32) {
    if !out.is_null() {
        unsafe {
            *out.add(0) = MAP_HOUSE_OFFSET_X[(house_idx.rem_euclid(3)) as usize];
            *out.add(1) = MAP_HOUSE_OFFSET_Y[(house_idx.div_euclid(3).min(2)) as usize];
        }
    }
}

/// Town-name color from `mMP_set_dl` (`m_map_ovl.c`): `out` receives
/// `[r, g, b, a]` from the two-entry `land_color` table selected by
/// `data0` (0 = magenta, 1 = blue).
#[no_mangle]
pub extern "C" fn pc_map_land_color(data0: c_int, out: *mut u8) {
    let col = if data0 == 0 { MAP_LAND_COLOR[0] } else { MAP_LAND_COLOR[1] };
    if !out.is_null() {
        unsafe {
            *out.add(0) = col[0];
            *out.add(1) = col[1];
            *out.add(2) = col[2];
            *out.add(3) = col[3];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bg_kind_rejects_borders_transitions_tunnels() {
        for k in [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 61, 62] {
            assert_eq!(pc_map_check_bg_kind(k), 0, "kind {k}");
        }
        for k in [11, 13, 39, 40, 63, 83, 100, 107] {
            assert_eq!(pc_map_check_bg_kind(k), 1, "kind {k}");
        }
    }

    #[test]
    fn texture_and_palette_tables() {
        assert_eq!(MAP_TEXTURE_TABLE.len(), 108);
        assert_eq!(MAP_PAL_TABLE.len(), 108);
        // spot checks against l_map_texture / l_map_pal
        assert_eq!(pc_map_tex_idx(0), 0); // BORDER_CLIFF_TOP -> f_TA
        assert_eq!(pc_map_tex_idx(11), 1); // TRACKS_STATION -> tst1
        assert_eq!(pc_map_tex_idx(39), 0); // FLAT -> f_TA
        assert_eq!(pc_map_tex_idx(40), 26); // RIVER_SOUTH -> r1
        assert_eq!(pc_map_tex_idx(100), 68); // PORT -> mwf
        assert_eq!(pc_map_tex_idx(108), -1);
        for k in [11, 12, 13, 14, 63, 64, 65, 66, 67, 68, 82, 83, 84, 85, 86, 100, 101, 102, 103, 104] {
            assert_eq!(pc_map_pal_idx(k), 1, "kind {k}");
        }
        assert_eq!(pc_map_pal_idx(39), 0);
        assert_eq!(pc_map_pal_idx(40), 0);
        assert_eq!(pc_map_pal_idx(108), -1);
    }

    #[test]
    fn bridge_substitution() {
        // substitute only when the bridge exists at the exact acre
        assert_eq!(pc_map_bridge_type(13, 1, 2, 3, 2, 3), 86);
        assert_eq!(pc_map_bridge_type(13, 1, 2, 3, 2, 4), 13);
        assert_eq!(pc_map_bridge_type(13, 0, 2, 3, 2, 3), 13);
        assert_eq!(pc_map_bridge_type(40, 1, 0, 0, 0, 0), 47);
        assert_eq!(pc_map_bridge_type(39, 1, 0, 0, 0, 0), 39); // FLAT has 0xFF entry
        assert_eq!(pc_map_bridge_type(24, 1, 1, 1, 1, 1), 88);
        assert_eq!(pc_map_bridge_type(27, 1, 1, 1, 1, 1), 92);
        assert_eq!(pc_map_bridge_type(64, 1, 1, 1, 1, 1), 82);
        assert_eq!(pc_map_bridge_type(200, 1, 0, 0, 0, 0), -1);
    }

    #[test]
    fn layer_conversion() {
        assert_eq!(pc_map_check_layer(0, 1), 2);
        assert_eq!(pc_map_check_layer(1, 1), 1);
        assert_eq!(pc_map_check_layer(2, 1), 0);
        assert_eq!(pc_map_check_layer(7, 1), 1);
        assert_eq!(pc_map_check_layer(0, 0), 1);
        assert_eq!(pc_map_check_layer(2, 0), 0); // -1 clamped to 0
        assert_eq!(pc_map_check_layer(1, 0), 0);
    }

    #[test]
    fn house_pos_match_uses_ut_z_minus_1() {
        let entries = [5, 2, 13, 9, 17, 16];
        assert_eq!(pc_map_house_pos_match(5, 3, entries.as_ptr()), 0); // z 2 == 3-1
        assert_eq!(pc_map_house_pos_match(13, 10, entries.as_ptr()), 1);
        assert_eq!(pc_map_house_pos_match(17, 17, entries.as_ptr()), 2);
        assert_eq!(pc_map_house_pos_match(0, 0, entries.as_ptr()), 0); // fallback
    }

    #[test]
    fn building_precedence_is_ordered() {
        assert_eq!(pc_map_building_label(1 << 1), LABEL_SHOP);
        assert_eq!(pc_map_building_label(1 << 19), LABEL_NEEDLE);
        assert_eq!(pc_map_building_label(1 << 30), LABEL_PORT);
        // shop wins over police when both flags are set
        assert_eq!(pc_map_building_label((1 << 1) | (1 << 3)), LABEL_SHOP);
        assert_eq!(pc_map_building_label(0), -1);
    }

    #[test]
    fn cursor_step_bounds_and_exit() {
        let mut out = [0; 4];
        // exit on A/B/Start
        assert_eq!(pc_map_cursor_step(2, 3, MAP_TRIG_A, 0, out.as_mut_ptr()), MAP_CURSOR_EXIT);
        assert_eq!(pc_map_cursor_step(2, 3, MAP_TRIG_X, 1, out.as_mut_ptr()), MAP_CURSOR_EXIT);
        // X does nothing when data0 != 1
        assert_eq!(pc_map_cursor_step(2, 3, MAP_TRIG_X, 0, out.as_mut_ptr()), MAP_CURSOR_STAYED);
        // left from bx=0 stays
        assert_eq!(pc_map_cursor_step(0, 3, MAP_TRIG_CLEFT, 0, out.as_mut_ptr()), MAP_CURSOR_STAYED);
        // left from bx=1 moves
        assert_eq!(pc_map_cursor_step(1, 3, MAP_TRIG_CLEFT, 0, out.as_mut_ptr()), MAP_CURSOR_MOVED);
        assert_eq!(out, [0, 3, -22, 0]);
        // down from bz=5 stays
        assert_eq!(pc_map_cursor_step(2, 5, MAP_TRIG_CDOWN, 0, out.as_mut_ptr()), MAP_CURSOR_STAYED);
        assert_eq!(pc_map_cursor_step(2, 4, MAP_TRIG_CDOWN, 0, out.as_mut_ptr()), MAP_CURSOR_MOVED);
        assert_eq!(out, [2, 5, 0, -22]);
        // up
        assert_eq!(pc_map_cursor_step(2, 0, MAP_TRIG_CUP, 0, out.as_mut_ptr()), MAP_CURSOR_STAYED);
        assert_eq!(pc_map_cursor_step(2, 1, MAP_TRIG_CUP, 0, out.as_mut_ptr()), MAP_CURSOR_MOVED);
        assert_eq!(out, [2, 0, 0, 22]);
        // right from bx=4 stays
        assert_eq!(pc_map_cursor_step(4, 3, MAP_TRIG_CRIGHT, 0, out.as_mut_ptr()), MAP_CURSOR_STAYED);
        assert_eq!(pc_map_cursor_step(3, 3, MAP_TRIG_CRIGHT, 0, out.as_mut_ptr()), MAP_CURSOR_MOVED);
        assert_eq!(out, [4, 3, 22, 0]);
    }

    #[test]
    fn cursor_pulse_tables() {
        let mut out = [0; 2];
        pc_map_cursor_pulse(0, out.as_mut_ptr());
        assert_eq!(out[0], 0);
        assert_eq!(out[1], 1000);
        pc_map_cursor_pulse(9, out.as_mut_ptr());
        assert_eq!(out[0], 100);
        assert_eq!(out[1], 1100);
        pc_map_cursor_pulse(18, out.as_mut_ptr()); // wraps
        assert_eq!(out[0], 0);
        pc_map_cursor_pulse(17, out.as_mut_ptr());
        assert_eq!(out[0], 1);
        assert_eq!(out[1], 1015);
    }

    #[test]
    fn cursor_init_math() {
        let mut out = [0.0f32; 3];
        pc_map_cursor_init(2, 3, out.as_mut_ptr());
        assert_eq!(out, [44.0, -66.0, 0.0]);
    }

    #[test]
    fn win_frame_selection() {
        assert_eq!(pc_map_win_frame(0), 0);
        assert_eq!(pc_map_win_frame(1), 1);
        assert_eq!(pc_map_win_frame(2), 2);
        assert_eq!(pc_map_win_frame(3), 3);
        assert_eq!(pc_map_win_frame(4), 4);
    }

    #[test]
    fn label_top_cnt() {
        assert_eq!(pc_map_label_top_cnt(LABEL_NPC, 3), 3);
        assert_eq!(pc_map_label_top_cnt(LABEL_PLAYER, 4), 4);
        assert_eq!(pc_map_label_top_cnt(LABEL_SHOP, 2), 1);
        assert_eq!(pc_map_label_top_cnt(LABEL_MUSEUM, 2), 1);
    }

    #[test]
    fn house_offsets() {
        let mut out = [0.0f32; 2];
        pc_map_house_offset(0, out.as_mut_ptr());
        assert_eq!(out, [5.0, -4.0]);
        pc_map_house_offset(4, out.as_mut_ptr());
        assert_eq!(out, [13.0, -11.0]);
        pc_map_house_offset(8, out.as_mut_ptr());
        assert_eq!(out, [17.0, -18.0]);
    }

    #[test]
    fn land_color() {
        let mut out = [0u8; 4];
        pc_map_land_color(0, out.as_mut_ptr());
        assert_eq!(out, [255, 0, 255, 255]);
        pc_map_land_color(1, out.as_mut_ptr());
        assert_eq!(out, [60, 60, 255, 255]);
    }
}
