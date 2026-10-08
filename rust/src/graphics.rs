//! GameCube/N64-hybrid graphics runtime (emu64 / GBI interpreter).
//!
//! Retail Animal Crossing does not build GX primitives directly. The game
//! keeps a large N64/libultra-derived graphics command representation
//! (`Gfx` display lists), interprets it at runtime through the `emu64`
//! taskstart loop, and translates the resulting state into GameCube
//! GX/TEV/texture operations. Conceptually:
//!
//! ```text
//! Game Gfx* (N64-style command data)
//!       │
//!       ▼
//! emu64 taskstart loop  (this module: interpreter.rs)
//!       │
//!       ├─ GBI state + dirty flags        (state.rs)
//!       ├─ display-list stack (18 deep)   (segments.rs)
//!       ├─ segmented address resolution   (segments.rs)
//!       ├─ texture conversion + cache     (texture.rs)
//!       ├─ combiner -> TEV translation    (combine.rs)
//!       └─ vertex decode + matrix sharing (vertex.rs)
//!       │
//!       ▼
//! GX backend  (gx_backend.rs: a trait the engine implements)
//!       │
//!       ▼
//! EFB -> XFB -> VI  (frame.rs + existing vi.rs)
//! ```
//!
//! The game-side representation stays canonical N64-era state; the GX
//! backend translates it. That mirrors retail, where `emu64` keeps a
//! shadow render state and only reprograms GX through dirty tracking.
//!
//! What stays engine-side: actual GX FIFO emission, EFB/RAM emulation,
//! the Dolphin GX library reconstruction, ROM model/texture assets.
//! This module ports the *game-facing* semantics: the command language,
//! the state machine, the texture-format conversions, and the frame
//! organization.

pub mod command;
pub mod combine;
pub mod frame;
pub mod gx_backend;
pub mod interpreter;
pub mod segments;
pub mod state;
pub mod texture;
pub mod vertex;

pub use command::{Gfx, GfxWords};
pub use interpreter::Emu64;
pub use segments::{DlStack, SegmentTable};
pub use state::{GbiState, Othermode};
pub use texture::{TextureCache, TextureConverter};
pub use vertex::Vtx;

// ---------------------------------------------------------------------------
// C ABI exports
// ---------------------------------------------------------------------------

/// Create an interpreter instance. Returns an opaque handle.
#[no_mangle]
pub extern "C" fn pc_gbi_emu64_create() -> *mut interpreter::Emu64 {
    Box::into_raw(Box::new(interpreter::Emu64::new()))
}

/// Destroy an interpreter instance.
#[no_mangle]
pub extern "C" fn pc_gbi_emu64_destroy(emu: *mut interpreter::Emu64) {
    if !emu.is_null() {
        // SAFETY: handle came from pc_gbi_emu64_create.
        unsafe { drop(Box::from_raw(emu)) };
    }
}

/// Install a segment base (`gSPSegment`).
#[no_mangle]
pub extern "C" fn pc_gbi_set_segment(emu: *mut interpreter::Emu64, seg: u8, base: u32) {
    if !emu.is_null() {
        // SAFETY: handle came from pc_gbi_emu64_create.
        unsafe { (*emu).segments.set(seg, base) };
    }
}

/// N64 RGBA5551 -> GX RGB5A3 conversion.
#[no_mangle]
pub extern "C" fn pc_gbi_rgba5551_to_rgb5a3(v: u16) -> u16 {
    texture::rgba5551_to_rgb5a3(v)
}

/// N64 (fmt, siz) -> GX format index into FMTXTBL order; returns the
/// table's format discriminant, or 255 for the I4 fallback.
#[no_mangle]
pub extern "C" fn pc_gbi_n64_to_gx_format(fmt: u8, siz: u8) -> u8 {
    texture::cvt_n64_to_gx(fmt, siz) as u8
}

/// Texture block (tile) dimensions for a texel size: packs w in the low
/// 16 bits and h in the high 16 bits.
#[no_mangle]
pub extern "C" fn pc_gbi_block_dims(siz: u8) -> u32 {
    let (w, h) = texture::block_dims(siz);
    w | (h << 16)
}

/// N64 TMEM bank/word swizzle.
#[no_mangle]
pub extern "C" fn pc_gbi_tmem_swizzle(ofs: u32, blk_siz: u32) -> u32 {
    texture::tmem_swizzle(ofs, blk_siz)
}

/// GX display-list alignment checks: nonzero = valid.
#[no_mangle]
pub extern "C" fn pc_gbi_gx_dl_valid(addr: usize, size: usize) -> u8 {
    (gx_backend::gx_dl_addr_valid(addr) && gx_backend::gx_dl_size_valid(size)) as u8
}

/// Commands processed by the interpreter (diagnostic counter).
#[no_mangle]
pub extern "C" fn pc_gbi_cmds_processed(emu: *const interpreter::Emu64) -> u64 {
    if emu.is_null() {
        return 0;
    }
    // SAFETY: handle came from pc_gbi_emu64_create.
    unsafe { (*emu).cmds_processed }
}
