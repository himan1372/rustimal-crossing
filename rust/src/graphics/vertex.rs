//! Vertex decoding and matrix sharing.
//!
//! `dl_G_VTX()` reads N64-style `Vtx` records (s16 positions, packed
//! normals, texture coords, colors, flags) into an internal `Vertex`.
//! Positions become f32; normals are decoded and, under
//! `G_TEXTURE_GEN`, normalized. Each vertex carries a shared/nonshared
//! matrix flag: texture-gen mode *forces* shared (`MTX_SHARED`),
//! otherwise the vertex's own flag bit decides.

use super::command::mtx_share;

/// Retail N64-style vertex record.
#[derive(Clone, Copy, Debug, Default)]
pub struct Vtx {
    pub x: i16,
    pub y: i16,
    pub z: i16,
    /// Packed normal (xyz in the high bytes of the N64 Vtx `n` field).
    pub nx: i8,
    pub ny: i8,
    pub nz: i8,
    pub flag: u8,
    pub s: i16,
    pub t: i16,
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

/// Decoded internal vertex (what `emu64` hands to GX emission).
#[derive(Clone, Copy, Debug, Default)]
pub struct DecodedVtx {
    pub pos: [f32; 3],
    pub normal: [f32; 3],
    pub st: [f32; 2],
    pub color: [u8; 4],
    /// `MTX_SHARED` (0) or `MTX_NONSHARED` (1).
    pub mtx_flag: u8,
    /// Which GX position matrix this vertex uses.
    pub gx_pnmtx: u8,
}

/// Decode one `Vtx` into the internal representation.
///
/// - `tex_gen`: `G_TEXTURE_GEN` is active in geometry mode.
/// - `force_vtx_flag_copy`: retail's `FORCE_VTX_FLAG_COPY` debug flag.
pub fn decode_vtx(v: &Vtx, tex_gen: bool, force_vtx_flag_copy: bool) -> DecodedVtx {
    let mut normal = [
        v.nx as f32 / 127.0,
        v.ny as f32 / 127.0,
        v.nz as f32 / 127.0,
    ];
    if tex_gen {
        // Retail normalizes under texture-gen (alternate 1/120, 1/128
        // scalings exist behind debug flags; AC/e+ uses normalization).
        let len = (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
        if len > 0.0 {
            normal[0] /= len;
            normal[1] /= len;
            normal[2] /= len;
        }
    }

    // Matrix-sharing flag: texture-gen forces shared; otherwise the
    // vertex's flag bit decides.
    let mtx_flag = if !force_vtx_flag_copy && tex_gen {
        mtx_share::MTX_SHARED
    } else {
        v.flag & mtx_share::MTX_NONSHARED
    };
    let gx_pnmtx = if mtx_flag == mtx_share::MTX_SHARED {
        mtx_share::SHARED_MTX // GX_PNMTX0
    } else {
        mtx_share::NONSHARED_MTX // GX_PNMTX1
    };

    DecodedVtx {
        pos: [v.x as f32, v.y as f32, v.z as f32],
        normal,
        st: [v.s as f32, v.t as f32],
        color: [v.r, v.g, v.b, v.a],
        mtx_flag,
        gx_pnmtx,
    }
}

/// The vertex cache: `G_VTX` loads `n` vertices at `v0`.
/// Retail's TRI1 divides N64 indices by 2 into this array.
#[derive(Clone, Debug)]
pub struct VtxCache {
    pub verts: Vec<DecodedVtx>,
}

impl VtxCache {
    pub fn new() -> VtxCache {
        VtxCache { verts: Vec::new() }
    }

    /// Load `n` vertices starting at slot `v0` (grows the cache).
    pub fn load(&mut self, v0: usize, n: usize, src: &[Vtx], tex_gen: bool, force_flag_copy: bool) {
        let need = v0 + n;
        if self.verts.len() < need {
            self.verts.resize(need, DecodedVtx::default());
        }
        for (i, v) in src.iter().take(n).enumerate() {
            self.verts[v0 + i] = decode_vtx(v, tex_gen, force_flag_copy);
        }
    }

    /// TRI1/TRI2/QUAD indexing: N64 index / 2.
    pub fn get(&self, n64_index: u8) -> Option<&DecodedVtx> {
        self.verts.get((n64_index / 2) as usize)
    }

    /// TRIN indexing: retail does NOT halve packed-triangle indices
    /// (`set_position` indexes `vertices[vtx]` directly).
    pub fn get_raw(&self, index: u8) -> Option<&DecodedVtx> {
        self.verts.get(index as usize)
    }
}

impl Default for VtxCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tex_gen_forces_shared() {
        let v = Vtx { flag: 1, nx: 100, ny: 0, nz: 0, ..Default::default() };
        let d = decode_vtx(&v, true, false);
        assert_eq!(d.mtx_flag, mtx_share::MTX_SHARED);
        assert_eq!(d.gx_pnmtx, mtx_share::SHARED_MTX);
        let d2 = decode_vtx(&v, false, false);
        assert_eq!(d2.mtx_flag, mtx_share::MTX_NONSHARED);
    }

    #[test]
    fn normal_normalized_under_texgen() {
        let v = Vtx { nx: 100, ny: 100, nz: 0, ..Default::default() };
        let d = decode_vtx(&v, true, false);
        let len = (d.normal[0] * d.normal[0] + d.normal[1] * d.normal[1]).sqrt();
        assert!((len - 1.0).abs() < 1e-4);
    }

    #[test]
    fn tri1_index_halved() {
        let mut c = VtxCache::new();
        let src = [Vtx { x: 1, ..Default::default() }, Vtx { x: 2, ..Default::default() }];
        c.load(0, 2, &src, false, false);
        assert_eq!(c.get(2).unwrap().pos[0], 2.0); // N64 index 2 -> slot 1
    }
}
