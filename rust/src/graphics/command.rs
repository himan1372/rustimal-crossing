//! Gfx command representation: opcodes, word layout, display-list params.
//!
//! A `Gfx` is two 32-bit words. The opcode is the top byte of word 0.
//! Retail dispatches through `dl_func_tbl` indexed by
//! `opcode - G_FIRST_CMD`, with `NUM_COMMANDS = 64` handlers.

/// One display-list command: two 32-bit words.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Gfx {
    pub w0: u32,
    pub w1: u32,
}

impl Gfx {
    pub const fn new(w0: u32, w1: u32) -> Gfx {
        Gfx { w0, w1 }
    }
    /// Opcode: top byte of word 0.
    pub fn opcode(self) -> u8 {
        (self.w0 >> 24) as u8
    }
    /// DMA-style address field (word 1).
    pub fn addr(self) -> u32 {
        self.w1
    }
    /// Parameter field: bits 16-23 of word 0.
    pub fn param(self) -> u8 {
        ((self.w0 >> 16) & 0xFF) as u8
    }
    /// Length field: low 16 bits of word 0.
    pub fn len(self) -> u16 {
        (self.w0 & 0xFFFF) as u16
    }
}

/// Word view used by handlers that reinterpret the two words.
#[derive(Clone, Copy, Debug)]
pub struct GfxWords {
    pub w0: u32,
    pub w1: u32,
}

// --- Opcode space ------------------------------------------------------------

/// First opcode in the dispatch table (`G_FIRST_CMD = G_SETTEXEDGEALPHA`).
pub const G_FIRST_CMD: u8 = 0xCE; // G_SETTEXEDGEALPHA
/// Number of entries in `dl_func_tbl`.
pub const NUM_COMMANDS: usize = 64;

/// Core opcodes, retail F3DEX_GBI numbering (`include/PR/gbi.h`).
pub mod op {
    pub const G_NOOP: u8 = 0x00;
    pub const G_VTX: u8 = 0x01;
    pub const G_MODIFYVTX: u8 = 0x02;
    pub const G_CULLDL: u8 = 0x03;
    pub const G_BRANCH_Z: u8 = 0x04;
    pub const G_TRI1: u8 = 0x05;
    pub const G_TRI2: u8 = 0x06;
    pub const G_QUAD: u8 = 0x07;
    pub const G_LINE3D: u8 = 0x08;
    // AC custom packed-triangle/quad commands.
    pub const G_TRIN: u8 = 0x09;
    pub const G_TRIN_INDEPEND: u8 = 0x0A;
    pub const G_QUADN: u8 = 0x0B;
    pub const G_QUAD_INDEPEND: u8 = 0x0C;
    // AC custom combiner/material commands (in the dispatch range).
    pub const G_SETTEXEDGEALPHA: u8 = 0xCE;
    pub const G_SETCOMBINE_NOTEV: u8 = 0xCF;
    pub const G_SETTILE_DOLPHIN: u8 = 0xD2;
    pub const G_SETCOMBINE_TEV: u8 = 0xD0;
    pub const G_TEXTURE: u8 = 0xD7;
    pub const G_POPMTX: u8 = 0xD8;
    pub const G_GEOMETRYMODE: u8 = 0xD9;
    pub const G_MTX: u8 = 0xDA;
    pub const G_MOVEWORD: u8 = 0xDB;
    pub const G_MOVEMEM: u8 = 0xDC;
    pub const G_DL: u8 = 0xDE;
    pub const G_ENDDL: u8 = 0xDF;
    pub const G_LOAD_UCODE: u8 = 0xDD;
    pub const G_DMA_IO: u8 = 0xD6;
    pub const G_SPECIAL_1: u8 = 0xD5;
    pub const G_SPECIAL_2: u8 = 0xD4;
    pub const G_SPECIAL_3: u8 = 0xD3;    pub const G_SPNOOP: u8 = 0xE0;
    pub const G_SETOTHERMODE_L: u8 = 0xE2;
    pub const G_SETOTHERMODE_H: u8 = 0xE3;
    pub const G_TEXRECT: u8 = 0xE4;
    pub const G_TEXRECTFLIP: u8 = 0xE5;
    pub const G_RDPLOADSYNC: u8 = 0xE6;
    pub const G_RDPPIPESYNC: u8 = 0xE7;
    pub const G_RDPTILESYNC: u8 = 0xE8;
    pub const G_RDPFULLSYNC: u8 = 0xE9;
    pub const G_SETSCISSOR: u8 = 0xED;
    pub const G_SETPRIMDEPTH: u8 = 0xEE;
    pub const G_LOADTLUT: u8 = 0xF0;
    pub const G_RDPHALF_1: u8 = 0xE1;
    pub const G_RDPHALF_2: u8 = 0xF1;
    pub const G_SETTILE: u8 = 0xF5;
    pub const G_LOADTILE: u8 = 0xF4;
    pub const G_LOADBLOCK: u8 = 0xF3;
    pub const G_SETTIMG: u8 = 0xFD;
    pub const G_SETCOMBINE: u8 = 0xFC;
    pub const G_SETENVCOLOR: u8 = 0xFB;
    pub const G_SETPRIMCOLOR: u8 = 0xFA;
    pub const G_SETBLENDCOLOR: u8 = 0xF9;
    pub const G_SETFOGCOLOR: u8 = 0xF8;
    pub const G_SETFILLCOLOR: u8 = 0xF7;
}

/// `G_DL` parameter values.
pub mod dl_param {
    pub const G_DL_PUSH: u8 = 0;
    pub const G_DL_NOPUSH: u8 = 1;
    /// Target is a real GameCube GX display list, not Gfx data.
    pub const G_DL_GXDL: u8 = 2;
}

// --- Texture image formats/sizes (N64 vocabulary) ----------------------------

pub mod img_fmt {
    pub const G_IM_FMT_RGBA: u8 = 0;
    pub const G_IM_FMT_YUV: u8 = 1;
    pub const G_IM_FMT_CI: u8 = 2;
    pub const G_IM_FMT_IA: u8 = 3;
    pub const G_IM_FMT_I: u8 = 4;
}

pub mod img_siz {
    pub const G_IM_SIZ_4B: u8 = 0;
    pub const G_IM_SIZ_8B: u8 = 1;
    pub const G_IM_SIZ_16B: u8 = 2;
    pub const G_IM_SIZ_32B: u8 = 3;
}

// --- Geometry-mode bits (N64 vocabulary, AC extensions) -----------------------

pub mod geo {
    pub const G_ZBUFFER: u32 = 0x0000_0001;
    pub const G_SHADE: u32 = 0x0000_0004;
    pub const G_CULL_FRONT: u32 = 0x0000_0200;
    pub const G_CULL_BACK: u32 = 0x0000_0400;
    pub const G_FOG: u32 = 0x0001_0000;
    pub const G_LIGHTING: u32 = 0x0002_0000;
    pub const G_TEXTURE_GEN: u32 = 0x0004_0000;
    pub const G_TEXTURE_GEN_LINEAR: u32 = 0x0008_0000;
    pub const G_SHADING_SMOOTH: u32 = 0x0020_0000;
    // Animal Crossing extensions (gbi_extensions.h).
    pub const G_LIGHTING_POSITIONAL: u32 = 0x40_0000;
    pub const G_DECAL_LEQUAL: u32 = 0x00;
    pub const G_DECAL_GEQUAL: u32 = 0x10;
    pub const G_DECAL_EQUAL: u32 = 0x20;
    pub const G_DECAL_ALWAYS: u32 = 0x30;
    pub const G_DECAL_SPECIAL: u32 = 0x40;
    pub const G_DECAL_ALL: u32 = G_DECAL_ALWAYS | G_DECAL_SPECIAL; // 0x70
}

// --- Matrix command params ----------------------------------------------------

pub mod mtx_param {
    /// `type` is bits 0-7 of w0 (`Gmtx.type`).
    /// Push happens when `(type & G_MTX_PUSH) == G_MTX_NOPUSH`,
    /// i.e. when the PUSH bit is CLEAR (retail's inverted naming).
    pub const G_MTX_PUSH: u8 = 0x01;
    pub const G_MTX_NOPUSH: u8 = 0x00;
    pub const G_MTX_LOAD: u8 = 0x02;
    pub const G_MTX_MUL: u8 = 0x00; // bit clear = multiply
    pub const G_MTX_MODELVIEW: u8 = 0x00;
    pub const G_MTX_PROJECTION: u8 = 0x04;
    pub const G_MTX_TEXTURE: u8 = 0x08;
}

/// Shared vs non-shared matrix vertex flags.
pub mod mtx_share {
    pub const MTX_SHARED: u8 = 0;
    pub const MTX_NONSHARED: u8 = 1;
    /// `SHARED_MTX = GX_PNMTX0`, `NONSHARED_MTX = GX_PNMTX1`.
    pub const SHARED_MTX: u8 = 0;
    pub const NONSHARED_MTX: u8 = 1;
}

/// Animated-texture segment ids (0x08-0x0D).
pub mod anime_seg {
    pub const ANIME_1_TXT_SEG: u8 = 0x08;
    pub const ANIME_6_TXT_SEG: u8 = 0x0D;
}
