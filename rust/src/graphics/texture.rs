//! N64 texture formats -> GX texture formats + texel conversion + cache.
//!
//! Retail converts N64/TMEM-layout texels into GameCube-tiled GX textures
//! through `texconv_tile()`, keyed by **source address** in a real cache
//! (`texture_cache_select`). TLUTs have their own conversion/cache path
//! (`tlutconv_new`). This module ports the conversions; the GX texture
//! *object* (the hardware-side struct) lives in `gx_backend`.

use super::command::{img_fmt, img_siz};
use std::collections::HashMap;

/// GX texture formats relevant to the conversion table.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GxTexFormat {
    I4,
    I8,
    IA4,
    IA8,
    C4,
    C8,
    RGB565,
    RGB5A3,
    RGBA8,
    CMPR,
    A8,   // GX_CTF_A8
    Z8,
    Z16,
    Z24X8,
    /// CI 16-bit special case: retail's table holds `0xA` here.
    Ci16Special,
    /// Unsupported by retail's table.
    Unsupported,
}

/// `emu64::fmtxtbl[8][4]`, verbatim. Indexed by [N64 fmt][bpp index
/// 0=4b, 1=8b, 2=16b, 3=32b].
pub const FMTXTBL: [[GxTexFormat; 4]; 8] = [
    // G_IM_FMT_RGBA
    [GxTexFormat::CMPR, GxTexFormat::Unsupported, GxTexFormat::RGB5A3, GxTexFormat::RGBA8],
    // G_IM_FMT_YUV
    [GxTexFormat::Unsupported; 4],
    // G_IM_FMT_CI
    [GxTexFormat::C4, GxTexFormat::C8, GxTexFormat::Ci16Special, GxTexFormat::Unsupported],
    // G_IM_FMT_IA
    [GxTexFormat::Unsupported, GxTexFormat::IA4, GxTexFormat::IA8, GxTexFormat::Unsupported],
    // G_IM_FMT_I
    [GxTexFormat::I4, GxTexFormat::I8, GxTexFormat::RGB565, GxTexFormat::Unsupported],
    // (row 5)
    [GxTexFormat::CMPR, GxTexFormat::A8, GxTexFormat::RGB5A3, GxTexFormat::Unsupported],
    // (row 6)
    [GxTexFormat::Unsupported, GxTexFormat::Z8, GxTexFormat::Z16, GxTexFormat::Z24X8],
    // (row 7)
    [GxTexFormat::Unsupported; 4],
];

/// `cvtN64ToDol()`: table lookup with the 0xFFFF -> GX_TF_I4 fallback.
pub fn cvt_n64_to_gx(fmt: u8, siz: u8) -> GxTexFormat {
    let bpp = match siz {
        img_siz::G_IM_SIZ_4B => 0,
        img_siz::G_IM_SIZ_8B => 1,
        img_siz::G_IM_SIZ_16B => 2,
        img_siz::G_IM_SIZ_32B => 3,
        _ => return GxTexFormat::Unsupported,
    };
    if (fmt as usize) >= FMTXTBL.len() {
        return GxTexFormat::I4; // 0xFFFF fallback
    }
    match FMTXTBL[fmt as usize][bpp] {
        GxTexFormat::Unsupported => GxTexFormat::I4, // retail's 0xFFFF -> GX_TF_I4
        f => f,
    }
}

/// `rgba5551_to_rgb5a3()`, verbatim: alpha bit 1 -> opaque form
/// (`0x8000 | (v >> 1)`), alpha bit 0 -> transparent 3-bit-alpha form.
pub fn rgba5551_to_rgb5a3(rgba5551: u16) -> u16 {
    if rgba5551 & 1 != 0 {
        0x8000 | (rgba5551 >> 1)
    } else {
        (((rgba5551 >> 4) & !0xFF) | ((rgba5551 >> 3) & 0xF0) | ((rgba5551 >> 2) & 0x0F)) as u16
    }
}

/// Block (tile) dimensions per texel size, verbatim `blk_tbl`:
/// 4b -> 8x8, 8b -> 8x4, 16b -> 4x4, 32b -> 4x4.
pub fn block_dims(siz: u8) -> (u32, u32) {
    match siz {
        img_siz::G_IM_SIZ_4B => (8, 8),
        img_siz::G_IM_SIZ_8B => (8, 4),
        _ => (4, 4),
    }
}

/// The N64 TMEM bank/word swap: `ofs ^ ((block >> 1) & 4)`.
/// `blk_siz` is the block size in bytes for the texel size.
pub fn tmem_swizzle(ofs: u32, blk_siz: u32) -> u32 {
    let block = ofs / blk_siz;
    ofs ^ ((block >> 1) & 4)
}

/// Rearrange 8-bit IA texels (intensity/alpha nibble swap into the
/// GameCube IA representation).
pub fn ia8_rearrange(src: &[u8], dst: &mut [u8]) {
    for (d, &s) in dst.iter_mut().zip(src.iter()) {
        // N64 IA8 is (I:4, A:4) nibbles; GX IA8 wants (A:4, I:4).
        *d = (s << 4) | (s >> 4);
    }
}

/// Convert one tile of N64-layout texels into GX-tiled output.
///
/// `src` is the N64 source (TMEM-swizzled indexing applied by the
/// caller via [`tmem_swizzle`]); `dst` receives GX-tiled texels.
/// `width`/`height` are in texels. Returns the bytes written.
pub fn texconv_tile(src: &[u8], dst: &mut [u8], width: u32, height: u32, fmt: u8, siz: u8) -> usize {
    let (blk_w, blk_h) = block_dims(siz);
    let mut out = 0usize;
    let texel_bytes: usize = match siz {
        img_siz::G_IM_SIZ_4B => 1, // nibbles packed; handled per-u32 by retail
        img_siz::G_IM_SIZ_8B => 1,
        img_siz::G_IM_SIZ_16B => 2,
        _ => 4,
    };
    let row_stride = width as usize * texel_bytes;

    let mut by = 0;
    while by < height {
        let mut bx = 0;
        while bx < width {
            for y in 0..blk_h {
                let yy = by + y;
                if yy >= height {
                    break;
                }
                for x in 0..blk_w {
                    let xx = bx + x;
                    if xx >= width {
                        break;
                    }
                    let src_ofs = (yy as usize) * row_stride + (xx as usize) * texel_bytes;
                    if src_ofs + texel_bytes > src.len() || out + texel_bytes > dst.len() {
                        continue;
                    }
                    match (fmt, siz) {
                        (img_fmt::G_IM_FMT_RGBA, img_siz::G_IM_SIZ_16B) => {
                            let v = u16::from_be_bytes([src[src_ofs], src[src_ofs + 1]]);
                            let c = rgba5551_to_rgb5a3(v);
                            dst[out..out + 2].copy_from_slice(&c.to_be_bytes());
                        }
                        (img_fmt::G_IM_FMT_IA, img_siz::G_IM_SIZ_8B) => {
                            dst[out] = (src[src_ofs] << 4) | (src[src_ofs] >> 4);
                        }
                        _ => {
                            dst[out..out + texel_bytes].copy_from_slice(&src[src_ofs..src_ofs + texel_bytes]);
                        }
                    }
                    out += texel_bytes;
                }
            }
            bx += blk_w;
        }
        by += blk_h;
    }
    out
}

/// Convert an RGBA5551 TLUT to RGB5A3 (retail `tlutconv` path).
pub fn tlutconv_rgba5551(tlut: &[u16], dst: &mut [u16]) {
    for (d, &s) in dst.iter_mut().zip(tlut.iter()) {
        *d = rgba5551_to_rgb5a3(s);
    }
}

/// TLUT formats retail accepts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TlutFormat {
    Rgba16,
    Ia16,
}

/// `tlutconv_new`: convert a TLUT, or reject it. Retail explicitly
/// rejects one unsupported IA16 condition (`err_count++`, returns
/// nullptr); this port returns `None` for IA16 instead of inventing
/// conversion bytes.
pub fn tlut_convert(tlut: &[u16], fmt: TlutFormat) -> Option<Vec<u16>> {
    match fmt {
        TlutFormat::Rgba16 => {
            let mut out = vec![0u16; tlut.len()];
            tlutconv_rgba5551(tlut, &mut out);
            Some(out)
        }
        TlutFormat::Ia16 => None, // retail's explicit rejection
    }
}

/// A cache entry: converted bytes for one source address.
#[derive(Clone, Debug)]
pub struct CacheEntry {
    pub src_addr: u32,
    pub converted: Vec<u8>,
    pub gx_format: GxTexFormat,
}

/// Texture/TLUT conversion cache, keyed by **source address**
/// (retail's `texture_cache_select`). On a hit the converted bytes are
/// reused; on a miss the caller converts and inserts.
///
/// Dynamic textures (player designs, animated frames, framebuffer
/// captures) that reuse or modify a source address need explicit
/// invalidation — the cache does not hash texel contents.
#[derive(Clone, Debug, Default)]
pub struct TextureCache {
    entries: HashMap<u32, CacheEntry>,
}

impl TextureCache {
    pub fn new() -> TextureCache {
        TextureCache { entries: HashMap::new() }
    }

    /// Look up a converted texture by source address.
    pub fn select(&self, src_addr: u32) -> Option<&CacheEntry> {
        self.entries.get(&src_addr)
    }

    /// Insert a freshly converted texture (retail does `DCStoreRange`
    /// here; on native hardware that cache writeback is required).
    pub fn insert(&mut self, entry: CacheEntry) {
        self.entries.insert(entry.src_addr, entry);
    }

    /// Invalidate one source address (dynamic-texture path).
    pub fn invalidate(&mut self, src_addr: u32) {
        self.entries.remove(&src_addr);
    }

    pub fn clear(&self) -> usize {
        self.entries.len()
    }
}

/// High-level converter: cache-aware N64 -> GX texture conversion.
pub struct TextureConverter {
    pub cache: TextureCache,
}

impl TextureConverter {
    pub fn new() -> TextureConverter {
        TextureConverter { cache: TextureCache::new() }
    }

    /// `texconv_tile_new`: convert (or fetch from cache) one texture.
    /// `src_addr` is the identity key.
    pub fn convert(
        &mut self,
        src: &[u8],
        src_addr: u32,
        width: u32,
        height: u32,
        fmt: u8,
        siz: u8,
    ) -> &[u8] {
        if !self.cache.entries.contains_key(&src_addr) {
            let gx = cvt_n64_to_gx(fmt, siz);
            let texel_bytes = match siz {
                img_siz::G_IM_SIZ_4B => 1,
                img_siz::G_IM_SIZ_8B => 1,
                img_siz::G_IM_SIZ_16B => 2,
                _ => 4,
            };
            let mut converted = vec![0u8; (width as usize) * (height as usize) * texel_bytes];
            texconv_tile(src, &mut converted, width, height, fmt, siz);
            // Retail: DCStoreRange(converted_addr, len) here.
            self.cache.insert(CacheEntry { src_addr, converted, gx_format: gx });
        }
        &self.cache.entries[&src_addr].converted
    }
}

impl Default for TextureConverter {
    fn default() -> Self {
        Self::new()
    }
}

/// Player original-design texture: 32x32 4-bit = 512 bytes, 32-byte
/// aligned (`mNW_original_tex_c`).
pub const DESIGN_TEX_SIZE: usize = 512;
pub const DESIGN_TEX_W: u32 = 32;
pub const DESIGN_TEX_H: u32 = 32;

/// 32x32 CI4-like design texture with palette.
#[derive(Clone, Debug)]
pub struct DesignTexture {
    /// 512 bytes, 4 bits per texel.
    pub data: [u8; DESIGN_TEX_SIZE],
    /// 16-color palette (RGB5A3 after conversion).
    pub palette: [u16; 16],
}

impl DesignTexture {
    pub fn new() -> DesignTexture {
        DesignTexture { data: [0; DESIGN_TEX_SIZE], palette: [0; 16] }
    }

    /// Read one 4-bit texel.
    pub fn texel(&self, x: u32, y: u32) -> u8 {
        if x >= DESIGN_TEX_W || y >= DESIGN_TEX_H {
            return 0;
        }
        let i = (y * DESIGN_TEX_W + x) as usize;
        let b = self.data[i / 2];
        if i % 2 == 0 {
            (b >> 4) & 0xF
        } else {
            b & 0xF
        }
    }

    /// Write one 4-bit texel.
    pub fn set_texel(&mut self, x: u32, y: u32, v: u8) {
        if x >= DESIGN_TEX_W || y >= DESIGN_TEX_H {
            return;
        }
        let i = (y * DESIGN_TEX_W + x) as usize;
        let b = &mut self.data[i / 2];
        if i % 2 == 0 {
            *b = (*b & 0x0F) | ((v & 0xF) << 4);
        } else {
            *b = (*b & 0xF0) | (v & 0xF);
        }
    }
}

impl Default for DesignTexture {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fmtxtbl_spot_checks() {
        assert_eq!(cvt_n64_to_gx(img_fmt::G_IM_FMT_RGBA, img_siz::G_IM_SIZ_4B), GxTexFormat::CMPR);
        assert_eq!(cvt_n64_to_gx(img_fmt::G_IM_FMT_RGBA, img_siz::G_IM_SIZ_16B), GxTexFormat::RGB5A3);
        assert_eq!(cvt_n64_to_gx(img_fmt::G_IM_FMT_RGBA, img_siz::G_IM_SIZ_32B), GxTexFormat::RGBA8);
        assert_eq!(cvt_n64_to_gx(img_fmt::G_IM_FMT_CI, img_siz::G_IM_SIZ_4B), GxTexFormat::C4);
        assert_eq!(cvt_n64_to_gx(img_fmt::G_IM_FMT_CI, img_siz::G_IM_SIZ_8B), GxTexFormat::C8);
        assert_eq!(cvt_n64_to_gx(img_fmt::G_IM_FMT_IA, img_siz::G_IM_SIZ_8B), GxTexFormat::IA4);
        assert_eq!(cvt_n64_to_gx(img_fmt::G_IM_FMT_IA, img_siz::G_IM_SIZ_16B), GxTexFormat::IA8);
        assert_eq!(cvt_n64_to_gx(img_fmt::G_IM_FMT_I, img_siz::G_IM_SIZ_4B), GxTexFormat::I4);
        assert_eq!(cvt_n64_to_gx(img_fmt::G_IM_FMT_I, img_siz::G_IM_SIZ_8B), GxTexFormat::I8);
        // Unsupported -> GX_TF_I4 fallback.
        assert_eq!(cvt_n64_to_gx(img_fmt::G_IM_FMT_RGBA, img_siz::G_IM_SIZ_8B), GxTexFormat::I4);
        assert_eq!(cvt_n64_to_gx(img_fmt::G_IM_FMT_YUV, img_siz::G_IM_SIZ_16B), GxTexFormat::I4);
    }

    #[test]
    fn rgba5551_conversion() {
        // Opaque: 0x8000 | (v >> 1).
        assert_eq!(rgba5551_to_rgb5a3(0xFFFF), 0xFFFF);
        assert_eq!(rgba5551_to_rgb5a3(0x0001), 0x8000);
        // Transparent: the (v>>4 & ~0xFF)|(v>>3 & 0xF0)|(v>>2 & 0x0F) form.
        assert_eq!(rgba5551_to_rgb5a3(0x0000), 0x0000);
        assert_eq!(rgba5551_to_rgb5a3(0xFFFE), 0x1FFE);
    }

    #[test]
    fn swizzle_and_blocks() {
        assert_eq!(block_dims(img_siz::G_IM_SIZ_4B), (8, 8));
        assert_eq!(block_dims(img_siz::G_IM_SIZ_8B), (8, 4));
        assert_eq!(block_dims(img_siz::G_IM_SIZ_16B), (4, 4));
        assert_eq!(tmem_swizzle(0, 8), 0);
        assert_eq!(tmem_swizzle(16, 8), 20); // block 2 -> ofs ^ 4
    }

    #[test]
    fn cache_hit_avoids_reconvert() {
        let mut c = TextureConverter::new();
        let src = vec![0xABu8; 64];
        let a = c.convert(&src, 0x8000_1000, 8, 8, img_fmt::G_IM_FMT_I, img_siz::G_IM_SIZ_4B).as_ptr();
        let b = c.convert(&src, 0x8000_1000, 8, 8, img_fmt::G_IM_FMT_I, img_siz::G_IM_SIZ_4B).as_ptr();
        assert_eq!(a, b);
        c.cache.invalidate(0x8000_1000);
        assert!(c.cache.select(0x8000_1000).is_none());
    }

    #[test]
    fn tlut_ia16_rejected() {
        assert!(tlut_convert(&[0xFFFF; 16], TlutFormat::Rgba16).is_some());
        assert!(tlut_convert(&[0xFFFF; 16], TlutFormat::Ia16).is_none());
    }

    #[test]
    fn design_texel_roundtrip() {
        let mut d = DesignTexture::new();
        d.set_texel(3, 5, 0xB);
        assert_eq!(d.texel(3, 5), 0xB);
        assert_eq!(d.texel(4, 5), 0x0);
    }
}
