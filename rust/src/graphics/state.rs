//! Canonical GBI render state + dirty tracking.
//!
//! `emu64` keeps a shadow render state and only reprograms GX when
//! `dirty_flags` say something changed. The game-side vocabulary stays
//! N64-era (geometry mode, othermode, combiner, tiles); translation to
//! GX happens in the backend behind those dirty flags.

use super::command::img_fmt;
use super::texture::GxTexFormat;

/// Dirty-state categories, mirroring `emu64`'s `dirty_flags[]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Dirty {
    Projection,
    PrimColor,
    EnvColor,
    FillColor,
    Combine,
    OthermodeH,
    OthermodeL,
    GeometryMode,
    Texture,
    Texture1,
    Texture2,
    TextureMatrix,
    Lighting,
}

/// Set of dirty categories.
#[derive(Clone, Debug, Default)]
pub struct DirtySet {
    flags: Vec<Dirty>,
}

impl DirtySet {
    pub fn mark(&mut self, d: Dirty) {
        if !self.flags.contains(&d) {
            self.flags.push(d);
        }
    }
    pub fn clear(&mut self) {
        self.flags.clear();
    }
    pub fn is_dirty(&self, d: Dirty) -> bool {
        self.flags.contains(&d)
    }
    pub fn all(&self) -> &[Dirty] {
        &self.flags
    }
}

/// Other-mode (RDP) state.
#[derive(Clone, Copy, Debug, Default)]
pub struct Othermode {
    pub hi: u32,
    pub lo: u32,
}

impl Othermode {
    /// `G_CYC_2CYCLE` set in othermode high: two-cycle combiner.
    pub fn is_2cycle(self) -> bool {
        self.hi & 0x0030_0000 == 0x0030_0000 // G_CYC_2CYCLE
    }
}

/// One texture tile descriptor (N64 side + Dolphin extensions).
#[derive(Clone, Copy, Debug)]
pub struct TileState {
    pub fmt: u8,
    pub siz: u8,
    /// Dolphin extension: this tile was described by the `_DOLPHIN` form.
    pub is_dolphin: bool,
    /// Dolphin form: real width/height (wd+1, (ht+1)*4 for images).
    pub width: u16,
    pub height: u16,
    pub gx_format: GxTexFormat,
    pub wrap_s: u8,
    pub wrap_t: u8,
    pub tlut: u8,
    pub img_addr: u32,
}

impl Default for TileState {
    fn default() -> TileState {
        TileState {
            fmt: img_fmt::G_IM_FMT_RGBA,
            siz: 2,
            is_dolphin: false,
            width: 0,
            height: 0,
            gx_format: GxTexFormat::I4,
            wrap_s: 0,
            wrap_t: 0,
            tlut: 0,
            img_addr: 0,
        }
    }
}

/// GX wrap modes (from the Dolphin extension vocabulary).
pub mod wrap {
    pub const GX_CLAMP: u8 = 0;
    pub const GX_REPEAT: u8 = 1;
    pub const GX_MIRROR: u8 = 2;
}

/// TLUT slot state: up to 16 slots in this model.
pub const NUM_TLUTS: usize = 16;

#[derive(Clone, Copy, Debug, Default)]
pub struct TlutState {
    pub addr: u32,
    pub count: u16,
}

/// RGBA color register.
#[derive(Clone, Copy, Debug, Default)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

/// Fog state: colors plus the N64 fog position parameters.
#[derive(Clone, Copy, Debug, Default)]
pub struct FogState {
    pub color: Rgba,
    /// `fog_zmult` / `fog_zoffset` feed the GX fog state.
    pub zmult: i16,
    pub zoffset: i16,
}

/// The canonical game-side render state.
#[derive(Clone, Debug)]
pub struct GbiState {
    pub geometry_mode: u32,
    pub othermode: Othermode,
    /// N64 combiner mux words (raw, pre-translation).
    pub combine_l: u32,
    pub combine_h: u32,
    /// Which combiner path is active (auto/tev/manual/notev).
    pub combine_path: super::combine::CombinePath,
    pub prim_color: Rgba,
    pub prim_lod: u8,
    pub env_color: Rgba,
    pub blend_color: Rgba,
    pub fill_color: Rgba,
    pub fog: FogState,
    /// Current texture image (from G_SETTIMG / G_SETTIMG_DOLPHIN).
    pub tex_image_fmt: u8,
    pub tex_image_siz: u8,
    pub tex_image_w: u16,
    pub tex_image_h: u16,
    pub tex_image_addr: u32,
    pub tex_image_dolphin: bool,
    /// Per-tile state (retail tracks NUM_TILES worth of tile/TLUT state).
    pub tiles: [TileState; 8],
    pub tluts: [TlutState; NUM_TLUTS],
    /// Texture-generation enables.
    pub tex_gen: bool,
    pub tex_gen_linear: bool,
    /// Scissor in game logical coordinates (not GX register coords).
    pub scissor: (u16, u16, u16, u16),
    pub dirty: DirtySet,
}

impl GbiState {
    pub fn new() -> GbiState {
        GbiState {
            geometry_mode: 0,
            othermode: Othermode::default(),
            combine_l: 0,
            combine_h: 0,
            combine_path: super::combine::CombinePath::Auto,
            prim_color: Rgba::default(),
            prim_lod: 0,
            env_color: Rgba::default(),
            blend_color: Rgba::default(),
            fill_color: Rgba::default(),
            fog: FogState::default(),
            tex_image_fmt: img_fmt::G_IM_FMT_RGBA,
            tex_image_siz: 2,
            tex_image_w: 0,
            tex_image_h: 0,
            tex_image_addr: 0,
            tex_image_dolphin: false,
            tiles: [TileState::default(); 8],
            tluts: [TlutState::default(); NUM_TLUTS],
            tex_gen: false,
            tex_gen_linear: false,
            scissor: (0, 0, 640, 480),
            dirty: DirtySet::default(),
        }
    }

    pub fn set_geometry(&mut self, clear: u32, set: u32) {
        self.geometry_mode = (self.geometry_mode & !clear) | set;
        self.tex_gen = self.geometry_mode & super::command::geo::G_TEXTURE_GEN != 0;
        self.tex_gen_linear = self.geometry_mode & super::command::geo::G_TEXTURE_GEN_LINEAR != 0;
        self.dirty.mark(Dirty::GeometryMode);
    }
}

impl Default for GbiState {
    fn default() -> Self {
        Self::new()
    }
}
