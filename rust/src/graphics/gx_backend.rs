//! GX backend models: display-list alignment, texture objects, TEV,
//! projection compression, EFB->XFB copy.
//!
//! This is the *translation target* of the interpreter, not a GX
//! implementation: actual FIFO emission and Flipper emulation stay
//! engine-side. The models here preserve the retail ABI details the
//! interpreter depends on.

/// GX display lists require 32-byte-aligned address AND size
/// (`GXBeginDisplayList` checks both).
pub const GX_DL_ALIGN: usize = 32;

pub fn gx_dl_addr_valid(addr: usize) -> bool {
    addr % GX_DL_ALIGN == 0
}

pub fn gx_dl_size_valid(size: usize) -> bool {
    size % GX_DL_ALIGN == 0 && size > 0
}

/// A recorded GX display-list call (`GXCallDisplayList` emits hardware
/// command 0x40 with address + byte count).
#[derive(Clone, Debug)]
pub struct GxDisplayListCall {
    pub addr: u32,
    pub byte_count: u32,
    pub addr_aligned: bool,
    pub size_aligned: bool,
}

impl GxDisplayListCall {
    pub fn new(addr: u32, byte_count: u32) -> GxDisplayListCall {
        GxDisplayListCall {
            addr,
            byte_count,
            addr_aligned: gx_dl_addr_valid(addr as usize),
            size_aligned: gx_dl_size_valid(byte_count as usize),
        }
    }
}

/// GX texture object model (retail `GXTexObj` fields: mode0/mode1,
/// image0/image3, format, tlutName, loadCnt, ...).
#[derive(Clone, Copy, Debug, Default)]
pub struct GxTexObj {
    /// Width/height stored as (w-1, h-1); image address as (addr >> 5).
    pub width_minus_1: u16,
    pub height_minus_1: u16,
    pub image_addr_srl5: u32,
    pub format: u8,
    pub tlut_name: u8,
    pub wrap_s: u8,
    pub wrap_t: u8,
    pub mag_filter: u8,
    pub min_filter: u8,
}

/// `GXInitTexObj` validation: width/height <= 1024, image 32-byte aligned.
pub fn gx_tex_obj_valid(width: u32, height: u32, image_addr: usize) -> bool {
    width <= 1024 && height <= 1024 && image_addr % 32 == 0
}

/// GX tiled-storage tile sizes per format (`GXGetTexBufferSize`):
/// I4/CMPR 8x8, I8/IA4 8x4, IA8/RGB565 4x4, RGBA8 4x4 with 64-byte tiles.
pub fn gx_tile_dims(format: super::texture::GxTexFormat) -> (u32, u32, u32) {
    use super::texture::GxTexFormat::*;
    match format {
        I4 | CMPR => (8, 8, 32),
        I8 | IA4 | C4 | C8 => (8, 4, 32),
        IA8 | RGB565 => (4, 4, 32),
        RGBA8 => (4, 4, 64),
        _ => (4, 4, 32),
    }
}

/// `GXSetProjection` sends only six matrix values (projMtx[0..5]) to XF
/// registers 32-37, plus the projection type at register 38.
#[derive(Clone, Copy, Debug, Default)]
pub struct GxProjectionPacket {
    pub m: [f32; 6],
    pub proj_type: u8,
}

pub fn gx_projection_packet(mtx: &[f32; 16], proj_type: u8) -> GxProjectionPacket {
    GxProjectionPacket {
        m: [mtx[0], mtx[1], mtx[2], mtx[3], mtx[4], mtx[5]],
        proj_type,
    }
}

/// Matrix-upload register bases (retail GX register layout):
/// position matrices at `id*4 + 0xB0000`, normals at `id*3 + 0x400`,
/// texture matrices around `0x500`.
pub mod xf_reg {
    pub const POS_BASE: u32 = 0xB_0000;
    pub const NRM_BASE: u32 = 0x400;
    pub const TEX_BASE: u32 = 0x500;

    pub fn pos_reg(id: u8) -> u32 {
        POS_BASE + (id as u32) * 4
    }
    pub fn nrm_reg(id: u8) -> u32 {
        NRM_BASE + (id as u32) * 3
    }
}

/// EFB -> XFB copy parameters (`GXCopyDisp`).
#[derive(Clone, Copy, Debug)]
pub struct EfbCopy {
    pub src_w: u32,
    pub src_h: u32,
    /// Y scale, clamping, AA filter, vertical filter, gamma, field mode.
    pub y_scale: f32,
    pub clamp_top: bool,
    pub clamp_bottom: bool,
    pub field_mode: super::frame::XfbMode,
}

/// EFB -> texture copy (`copy_efb_to_texture` builds a native GX display
/// list: `GXSetTexCopySrc`, `GXSetTexCopyDst`, `GXSetCopyFilter`,
/// `GXCopyTex`). Retail captures `SCREEN_WIDTH*2 x SCREEN_HEIGHT*2` as
/// `GX_TF_RGB565` for bump/wipe textures.
#[derive(Clone, Copy, Debug)]
pub struct EfbTexCopy {
    pub src_x: u16,
    pub src_y: u16,
    pub src_w: u16,
    pub src_h: u16,
    pub dst_w: u16,
    pub dst_h: u16,
    pub format: super::texture::GxTexFormat,
}

impl EfbTexCopy {
    pub fn wipe_capture() -> EfbTexCopy {
        EfbTexCopy {
            src_x: 0,
            src_y: 0,
            src_w: (super::frame::SCREEN_WIDTH * 2) as u16,
            src_h: (super::frame::SCREEN_HEIGHT * 2) as u16,
            dst_w: (super::frame::SCREEN_WIDTH * 2) as u16,
            dst_h: (super::frame::SCREEN_HEIGHT * 2) as u16,
            format: super::texture::GxTexFormat::RGB565,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dl_alignment() {
        assert!(gx_dl_addr_valid(0x8000_0020));
        assert!(!gx_dl_addr_valid(0x8000_0001));
        assert!(gx_dl_size_valid(64));
        assert!(!gx_dl_size_valid(60));
        let c = GxDisplayListCall::new(0x20, 64);
        assert!(c.addr_aligned && c.size_aligned);
    }

    #[test]
    fn tex_obj_validation() {
        assert!(gx_tex_obj_valid(32, 32, 0x8000_0020));
        assert!(!gx_tex_obj_valid(2048, 32, 0x8000_0020));
        assert!(!gx_tex_obj_valid(32, 32, 0x8000_0001));
    }

    #[test]
    fn projection_packet_takes_six() {
        let m = [1.0f32; 16];
        let p = gx_projection_packet(&m, 1);
        assert_eq!(p.m.len(), 6);
        assert_eq!(xf_reg::pos_reg(2), 0xB_0000 + 8);
    }
}
