//! Frame organization: display-list streams, material presets, video modes.
//!
//! `DisplayList_initialize()` establishes separate command streams the
//! game appends to all frame; they are assembled into the frame later:
//!
//! ```text
//! GRAPH
//!  ├── polygon opaque      (POLY_OPA)
//!  ├── polygon translucent (POLY_XLU)
//!  ├── overlay             (OVERLAY)
//!  ├── work                (WORK)
//!  ├── font                (FONT)
//!  ├── shadow              (SHADOW)
//!  ├── lighting            (LIGHT)
//!  ├── background opaque   (BG_OPA)
//!  └── background translucent (BG_XLU)
//! ```
//!
//! `m_rcp.c`'s `z_gsCPModeSet_Data[15][6]` are prebuilt rendering-state
//! display lists (the game's material-state library): objects emit one
//! preset then only their object-specific state.

use super::command::{op, Gfx};

/// The nine display-list streams (`include/graph.h`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Stream {
    PolyOpa,
    PolyXlu,
    Overlay,
    Work,
    Font,
    Shadow,
    Light,
    BgOpa,
    BgXlu,
}

impl Stream {
    pub const ALL: [Stream; 9] = [
        Stream::PolyOpa,
        Stream::PolyXlu,
        Stream::Overlay,
        Stream::Work,
        Stream::Font,
        Stream::Shadow,
        Stream::Light,
        Stream::BgOpa,
        Stream::BgXlu,
    ];
}

/// One stream's command buffer for the current frame.
#[derive(Clone, Debug, Default)]
pub struct StreamBuffer {
    pub stream: Option<Stream>,
    pub cmds: Vec<Gfx>,
}

impl StreamBuffer {
    pub fn push(&mut self, g: Gfx) {
        self.cmds.push(g);
    }
}

/// The frame's nine streams.
#[derive(Clone, Debug)]
pub struct Frame {
    pub streams: [StreamBuffer; 9],
}

impl Frame {
    pub fn new() -> Frame {
        let mut streams: [StreamBuffer; 9] = Default::default();
        for (i, s) in Stream::ALL.iter().enumerate() {
            streams[i].stream = Some(*s);
        }
        Frame { streams }
    }

    pub fn stream(&mut self, s: Stream) -> &mut StreamBuffer {
        &mut self.streams[s as usize]
    }

    /// Total commands across all streams.
    pub fn total_cmds(&self) -> usize {
        self.streams.iter().map(|s| s.cmds.len()).sum()
    }
}

impl Default for Frame {
    fn default() -> Self {
        Self::new()
    }
}

// --- Material presets (z_gsCPModeSet_Data concept) ----------------------------

/// Material preset ids. Retail's `z_gsCPModeSet_Data` is
/// `static Gfx[15][6]` (m_rcp.c): 15 material presets, each a 6-command
/// state-establishing display list (textured-opaque,
/// textured-translucent, fogged-opaque, texture-edge-alpha, ...).
/// This port models the preset vocabulary; the full 15 rows of retail
/// command data are ROM-side content.
pub const CP_MODE_SET_ROWS: usize = 15;
pub const CP_MODE_SET_COLS: usize = 6;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaterialPreset {
    TexturedOpaque = 0,
    TexturedTranslucent = 1,
    FoggedOpaque = 2,
    TextureEdgeAlpha = 3,
    UntexturedOpaque = 4,
    UntexturedTranslucent = 5,
}

/// Build the preset's state-establishing commands (a small model of the
/// retail `z_gsCPModeSet_Data` rows: geometry mode + render mode +
/// combine words).
pub fn material_preset_cmds(preset: MaterialPreset) -> Vec<Gfx> {
    use super::command::geo;
    let mut v = Vec::new();
    let geo_bits: u32 = match preset {
        MaterialPreset::TexturedOpaque => {
            geo::G_ZBUFFER | geo::G_SHADE | geo::G_CULL_BACK | geo::G_SHADING_SMOOTH
        }
        MaterialPreset::TexturedTranslucent => geo::G_ZBUFFER | geo::G_SHADE | geo::G_SHADING_SMOOTH,
        MaterialPreset::FoggedOpaque => {
            geo::G_ZBUFFER | geo::G_SHADE | geo::G_CULL_BACK | geo::G_FOG | geo::G_SHADING_SMOOTH
        }
        MaterialPreset::TextureEdgeAlpha => geo::G_ZBUFFER | geo::G_SHADE,
        MaterialPreset::UntexturedOpaque => {
            geo::G_ZBUFFER | geo::G_SHADE | geo::G_CULL_BACK | geo::G_SHADING_SMOOTH
        }
        MaterialPreset::UntexturedTranslucent => geo::G_ZBUFFER | geo::G_SHADE,
    };
    // G_GEOMETRYMODE: w1 carries the set bits (simplified packing).
    v.push(Gfx::new(((op::G_GEOMETRYMODE as u32) << 24) | (geo_bits & 0xFF), geo_bits >> 8));
    v
}

// --- Video/render modes (jsyswrap.cpp customized modes) ----------------------

/// The game's standard logical framebuffer.
pub const SCREEN_WIDTH: u32 = 640;
pub const SCREEN_HEIGHT: u32 = 480;

/// XFB width in the customized NTSC modes.
pub const XFB_WIDTH: u32 = 660;

/// VI origin in the customized modes.
pub const VI_X_ORIGIN: u32 = 30;
pub const VI_Y_ORIGIN: u32 = 0;

/// XFB field mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum XfbMode {
    /// `VI_XFBMODE_SF`: progressive / DS modes.
    SingleField,
    /// `VI_XFBMODE_DF`: interlaced.
    DoubleField,
}

/// A customized render mode (subset of `GXRenderModeObj` fields the
/// game actually varies).
#[derive(Clone, Copy, Debug)]
pub struct RenderMode {
    pub name: &'static str,
    pub fb_width: u32,
    pub efb_height: u32,
    pub xfb_width: u32,
    pub xfb_height: u32,
    pub xfb_mode: XfbMode,
    /// 7-element vertical filter.
    pub vfilter: [u8; 7],
}

impl RenderMode {
    /// Standard NTSC 480 interlaced (`customized_GXNtsc480IntDf`).
    pub const NTSC_480_INT_DF: RenderMode = RenderMode {
        name: "NTSC480IntDf",
        fb_width: 640,
        efb_height: 480,
        xfb_width: 640,
        xfb_height: 480,
        xfb_mode: XfbMode::DoubleField,
        vfilter: [8, 8, 10, 12, 10, 8, 8],
    };
    /// NTSC 480 progressive (`customized_GXNtsc480Prog`).
    pub const NTSC_480_PROG: RenderMode = RenderMode {
        name: "NTSC480Prog",
        fb_width: 640,
        efb_height: 480,
        xfb_width: 640,
        xfb_height: 480,
        xfb_mode: XfbMode::SingleField,
        vfilter: [0, 0, 21, 22, 21, 0, 0],
    };
    /// NTSC 240 DS (`customized_GXNtsc240Ds`).
    pub const NTSC_240_DS: RenderMode = RenderMode {
        name: "NTSC240Ds",
        fb_width: 640,
        efb_height: 240,
        xfb_width: 640,
        xfb_height: 240,
        xfb_mode: XfbMode::SingleField,
        vfilter: [0, 0, 21, 22, 21, 0, 0],
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nine_streams() {
        let mut f = Frame::new();
        assert_eq!(f.streams.len(), 9);
        f.stream(Stream::PolyOpa).push(Gfx::new(0, 0));
        assert_eq!(f.total_cmds(), 1);
    }

    #[test]
    fn render_mode_constants() {
        assert_eq!(RenderMode::NTSC_480_INT_DF.vfilter, [8, 8, 10, 12, 10, 8, 8]);
        assert_eq!(RenderMode::NTSC_480_PROG.xfb_mode, XfbMode::SingleField);
        assert_eq!(SCREEN_WIDTH, 640);
        assert_eq!(SCREEN_HEIGHT, 480);
    }

    #[test]
    fn material_preset_builds() {
        let cmds = material_preset_cmds(MaterialPreset::FoggedOpaque);
        assert!(!cmds.is_empty());
        assert_eq!(cmds[0].opcode(), op::G_GEOMETRYMODE);
    }
}
