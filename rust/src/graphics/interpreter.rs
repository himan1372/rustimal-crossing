//! The `emu64` display-list interpreter (`emu64_taskstart`).
//!
//! Every `Gfx` is fetched, decoded, dispatched through the handler
//! table (`opcode - G_FIRST_CMD`, `NUM_COMMANDS = 64` entries), allowed
//! to mutate the renderer state, and followed by the next command.
//! Nested display lists use a real call stack; a `G_DL` whose target is
//! a native GX display list (`G_DL_GXDL`) is handed to the GX backend
//! instead of being interpreted.

use super::command::{self, dl_param, op, Gfx};
use super::segments::{DlStack, SegmentTable, DL_MAX_STACK_LEVEL};
use super::state::{Dirty, GbiState};
use super::texture::TextureConverter;
use super::vertex::{DecodedVtx, Vtx, VtxCache};
use std::collections::HashMap;

/// Memory backing the interpreter: resolves segmented addresses to
/// display-list data, vertex data, and opaque GX-display-list bytes.
pub trait GfxMemory {
    fn vertices(&self, flat_addr: u32, n: usize) -> Option<Vec<Vtx>>;
    fn bytes(&self, flat_addr: u32, n: usize) -> Option<Vec<u8>>;
}

/// A triangle emitted toward the GX backend.
#[derive(Clone, Copy, Debug)]
pub struct EmittedTri {
    pub v: [DecodedVtx; 3],
}

/// GX backend interface: the interpreter translates state into these calls.
pub trait GxBackend {
    fn emit_triangle(&mut self, tri: EmittedTri);
    fn emit_quad(&mut self, v: [DecodedVtx; 4]);
    fn emit_texrect(&mut self, ulx: u16, uly: u16, lrx: u16, lry: u16);
    fn gx_call_display_list(&mut self, bytes: &[u8]);
    fn cull_display_list(&mut self, v0: u8, vn: u8) -> bool;
}

/// Return address on the DL stack: which list and where to resume.
#[derive(Clone, Copy, Debug)]
pub struct DlFrame {
    pub list_addr: u32,
    pub pc: usize,
}

/// The interpreter: GBI state + DL stack + segments + vertex cache.
pub struct Emu64 {
    pub state: GbiState,
    pub segments: SegmentTable,
    pub vtx_cache: VtxCache,
    pub tex_conv: TextureConverter,
    /// Model-view matrix stack (retail keeps model_view_mtx_stack).
    pub mv_stack: Vec<[f32; 16]>,
    /// Projection matrix.
    pub proj: [f32; 16],
    /// Texture matrices.
    pub tex_mtx: [[f32; 16]; 2],
    /// Commands processed this task (diagnostic, like retail's counter).
    pub cmds_processed: u64,
    /// `disable_polygons`: skips GX display-list calls.
    pub disable_polygons: bool,
    /// G_MTX stack-overflow counter (retail's err_count++ path).
    pub mtx_overflow: u32,
    /// `FrameCansel`: aborts the task loop.
    pub frame_cancel: bool,
    /// Triangles emitted (kept when no backend is attached).
    pub emitted: Vec<EmittedTri>,
    /// Display-list return frames (retail's `DL_stack` raw pointers).
    dl_frames: Vec<DlFrame>,
}

impl Emu64 {
    pub fn new() -> Emu64 {
        Emu64 {
            state: GbiState::new(),
            segments: SegmentTable::new(),
            vtx_cache: VtxCache::new(),
            tex_conv: TextureConverter::new(),
            mv_stack: vec![identity()],
            proj: identity(),
            tex_mtx: [identity(), identity()],
            cmds_processed: 0,
            disable_polygons: false,
            mtx_overflow: 0,
            frame_cancel: false,
            emitted: Vec::new(),
            dl_frames: Vec::new(),
        }
    }

    /// `emu64_taskstart`: interpret the display list at `entry_addr`.
    /// `lists` maps flat addresses to display-list bodies (the emulator's
    /// RAM image of Gfx data).
    pub fn taskstart<M: GfxMemory, B: GxBackend>(
        &mut self,
        mem: &M,
        backend: &mut B,
        lists: &HashMap<u32, Vec<Gfx>>,
        entry_addr: u32,
    ) {
        let mut stack = DlStack::new();
        let mut list_addr = entry_addr;
        let mut pc: usize = 0;
        let mut end_dl = false;

        while !end_dl && !self.frame_cancel {
            let list = match lists.get(&list_addr) {
                Some(l) => l,
                None => break,
            };
            if pc >= list.len() {
                break;
            }
            let gfx = list[pc];
            let opcode = gfx.opcode();
            self.cmds_processed += 1;

            // Dispatch: dl_func_tbl[opcode - G_FIRST_CMD].
            let idx = opcode.wrapping_sub(command::G_FIRST_CMD) as usize;
            if idx < command::NUM_COMMANDS {
                self.dispatch(mem, backend, lists, gfx, &mut stack, &mut list_addr, &mut pc, &mut end_dl);
            } else {
                // Out-of-range opcode: retail logs and continues/aborts.
                pc += 1;
            }
        }
        let _ = DL_MAX_STACK_LEVEL;
    }

    #[allow(clippy::too_many_arguments)]
    fn dispatch<M: GfxMemory, B: GxBackend>(
        &mut self,
        mem: &M,
        backend: &mut B,
        lists: &HashMap<u32, Vec<Gfx>>,
        gfx: Gfx,
        stack: &mut DlStack,
        list_addr: &mut u32,
        pc: &mut usize,
        end_dl: &mut bool,
    ) {
        match gfx.opcode() {
            op::G_DL => {
                let target = match self.segments.resolve(gfx.addr()) {
                    Some(a) => a,
                    None => {
                        *pc += 1;
                        return;
                    }
                };
                match gfx.param() {
                    dl_param::G_DL_PUSH => {
                        // Push the return frame (overflow is counted
                        // inside, like retail's "*** DL stack overflow ***").
                        self.push_frame(stack, *list_addr, *pc + 1);
                        *list_addr = target;
                        *pc = 0;
                        return;
                    }
                    dl_param::G_DL_NOPUSH => {
                        *list_addr = target;
                        *pc = 0;
                        return;
                    }
                    _ => {
                        // G_DL_GXDL (or unknown): native GX display list.
                        if !self.disable_polygons {
                            if let Some(bytes) = mem.bytes(target, gfx.len() as usize) {
                                backend.gx_call_display_list(&bytes);
                            } else {
                                backend.gx_call_display_list(&[]);
                            }
                        }
                    }
                }
                *pc += 1;
            }
            op::G_ENDDL => {
                if let Some(frame) = self.pop_frame(stack) {
                    *list_addr = frame.list_addr;
                    *pc = frame.pc;
                } else {
                    *end_dl = true;
                }
            }
            op::G_VTX => {
                self.h_vtx(mem, gfx);
                *pc += 1;
            }
            op::G_TRI1 => {
                self.h_tri(mem, backend, gfx, 1);
                *pc += 1;
            }
            op::G_TRI2 => {
                self.h_tri(mem, backend, gfx, 2);
                *pc += 1;
            }
            op::G_QUAD => {
                self.h_quad(mem, backend, gfx);
                *pc += 1;
            }
            op::G_TRIN | op::G_TRIN_INDEPEND => {
                *pc = self.h_trin(mem, backend, lists, *list_addr, *pc, gfx);
            }
            op::G_MTX => {
                self.h_mtx(gfx);
                *pc += 1;
            }
            op::G_POPMTX => {
                self.mv_stack.pop();
                if self.mv_stack.is_empty() {
                    self.mv_stack.push(identity());
                }
                *pc += 1;
            }
            op::G_GEOMETRYMODE => {
                let clear = ((gfx.w0 >> 0) & 0x00FF_FFFF) as u32;
                let set = (gfx.w1 & 0x00FF_FFFF) as u32;
                // Retail: G_GEOMETRYMODE packs clear/set; the actual
                // handler distinguishes via the param field.
                let _ = clear;
                self.state.set_geometry(0, set);
                *pc += 1;
            }
            op::G_SETOTHERMODE_H => {
                self.state.othermode.hi = gfx.w1;
                self.state.dirty.mark(Dirty::OthermodeH);
                *pc += 1;
            }
            op::G_SETOTHERMODE_L => {
                self.state.othermode.lo = gfx.w1;
                self.state.dirty.mark(Dirty::OthermodeL);
                *pc += 1;
            }
            op::G_SETCOMBINE => {
                self.state.combine_l = gfx.w0;
                self.state.combine_h = gfx.w1;
                self.state.combine_path = super::combine::CombinePath::Auto;
                self.state.dirty.mark(Dirty::Combine);
                *pc += 1;
            }
            op::G_SETCOMBINE_TEV => {
                self.state.combine_path = super::combine::CombinePath::Preconverted;
                self.state.dirty.mark(Dirty::Combine);
                *pc += 1;
            }
            op::G_SETCOMBINE_NOTEV => {
                self.state.combine_path = super::combine::CombinePath::NotEv;
                self.state.dirty.mark(Dirty::Combine);
                *pc += 1;
            }
            op::G_SETPRIMCOLOR => {
                self.state.prim_color = rgba(gfx.w1);
                self.state.prim_lod = ((gfx.w0 >> 8) & 0xFF) as u8;
                self.state.dirty.mark(Dirty::PrimColor);
                *pc += 1;
            }
            op::G_SETENVCOLOR => {
                self.state.env_color = rgba(gfx.w1);
                self.state.dirty.mark(Dirty::EnvColor);
                *pc += 1;
            }
            op::G_SETFOGCOLOR => {
                self.state.fog.color = rgba(gfx.w1);
                *pc += 1;
            }
            op::G_SETFILLCOLOR => {
                self.state.fill_color = rgba(gfx.w1);
                self.state.dirty.mark(Dirty::FillColor);
                *pc += 1;
            }
            op::G_SETBLENDCOLOR => {
                self.state.blend_color = rgba(gfx.w1);
                *pc += 1;
            }
            op::G_SETTIMG => self.h_settimg(gfx, pc),
            op::G_SETTILE => self.h_settile(gfx, pc),
            op::G_LOADTLUT => {
                let tile = ((gfx.w0 >> 16) & 0x7) as usize % 8;
                self.state.tluts[tile % super::state::NUM_TLUTS].addr = gfx.addr();
                self.state.tluts[tile % super::state::NUM_TLUTS].count = (gfx.len() >> 6) as u16;
                self.state.dirty.mark(Dirty::Texture);
                *pc += 1;
            }
            op::G_LOADBLOCK | op::G_LOADTILE => {
                // TMEM load: marks texture dirty; conversion is lazy.
                self.state.dirty.mark(Dirty::Texture);
                *pc += 1;
            }
            op::G_TEXTURE => {
                // on/off + texel density; affects tile state only.
                self.state.dirty.mark(Dirty::Texture);
                *pc += 1;
            }
            op::G_TEXRECT => {
                // Gtexrect2 is 192 bits = 3 Gfx words: the handler does
                // gfx_p += 2 on top of the interpreter's gfx_p++.
                let (ulx, uly, lrx, lry) = texrect_coords(gfx);
                backend.emit_texrect(ulx, uly, lrx, lry);
                *pc += 3;
            }
            op::G_TEXRECTFLIP => {
                // Retail maps 0xE5 to dl_G_NOOP.
                *pc += 1;
            }
            op::G_SETTILE_DOLPHIN => self.h_settile_dolphin(gfx, pc),
            op::G_CULLDL => {
                let v0 = ((gfx.w0 >> 16) & 0xFF) as u8;
                let vn = (gfx.w1 & 0xFFFF) as u8;
                if backend.cull_display_list(v0, vn) {
                    *end_dl = true;
                } else {
                    *pc += 1;
                }
            }
            op::G_SETSCISSOR => {
                let (x0, y0, x1, y1) = (
                    ((gfx.w0 >> 12) & 0xFFF) as u16,
                    ((gfx.w0 >> 0) & 0xFFF) as u16,
                    ((gfx.w1 >> 12) & 0xFFF) as u16,
                    ((gfx.w1 >> 0) & 0xFFF) as u16,
                );
                self.state.scissor = (x0, y0, x1, y1);
                *pc += 1;
            }
            op::G_SETTEXEDGEALPHA => {
                *pc += 1;
            }
            // Handlers retail implements that this port models as
            // state-neutral no-ops (documented here so the 64-entry
            // table shape stays visible):
            // - G_RDPHALF_1/2, G_RDPLOADSYNC/RDPPIPESYNC/RDPTILESYNC/
            //   RDPFULLSYNC: RDP syncs, no GBI state effect.
            // - G_MOVEWORD/G_MOVEMEM: RSP DMEM moves, not interpreted.
            // - G_LOAD_UCODE: microcode swap, not applicable.
            // - G_MODIFYVTX/G_BRANCH_Z/G_LINE3D: not used by AC assets.
            // - G_QUADN (custom packed quads): variable-length like
            //   TRIN; parse shape reserved for a follow-up.
            // - G_SPECIAL_1/2/3: AC special modes; reserved.
            // - G_SETPRIMDEPTH/G_SETCONVERT/G_SETKEYR/G_SETKEYGB/
            //   G_FILLRECT/G_SETZIMG/G_SETCIMG: RDP-side, no GBI state.
            op::G_RDPHALF_1
            | op::G_RDPHALF_2
            | op::G_RDPLOADSYNC
            | op::G_RDPPIPESYNC
            | op::G_RDPTILESYNC
            | op::G_RDPFULLSYNC
            | op::G_MOVEWORD
            | op::G_MOVEMEM
            | op::G_MODIFYVTX
            | op::G_BRANCH_Z
            | op::G_LINE3D
            | op::G_SETPRIMDEPTH
            | op::G_SPECIAL_1
            | op::G_SPECIAL_2
            | op::G_SPECIAL_3 => {
                *pc += 1;
            }
            _ => {
                // Unimplemented/dead opcodes: advance like retail's
                // null-handler slots.
                *pc += 1;
            }
        }
        let _ = lists;
    }

    // -- handler helpers ------------------------------------------------------

    /// Extra frame stack for (list_addr, pc) pairs (the DL_stack in
    /// retail stores raw pointers; this is the equivalent).
    fn push_frame(&mut self, stack: &mut DlStack, list_addr: u32, pc: usize) {
        if stack.push(0) {
            self.dl_frames.push(DlFrame { list_addr, pc });
        }
        // On overflow the jump still happens but the return address is
        // lost, exactly like retail.
    }
    fn pop_frame(&mut self, stack: &mut DlStack) -> Option<DlFrame> {
        stack.pop()?;
        self.dl_frames.pop()
    }

    fn h_vtx<M: GfxMemory>(&mut self, mem: &M, gfx: Gfx) {
        // Retail: n = bits 12-19 (raw, no +1); vn = bits 1-7 = v0+n in
        // N64-index units; v0 = (vn >> 1) - n.
        // E.g. gsSPVertex(v,16,0) -> 16 verts loaded at slot 0.
        let n = ((gfx.w0 >> 12) & 0xFF) as usize;
        let vn = ((gfx.w0 >> 1) & 0x7F) as usize;
        let v0 = (vn >> 1).saturating_sub(n);
        let addr = match self.segments.resolve(gfx.addr()) {
            Some(a) => a,
            None => return,
        };
        if let Some(src) = mem.vertices(addr, n) {
            let tex_gen = self.state.tex_gen;
            self.vtx_cache.load(v0, n, &src, tex_gen, false);
        }
    }

    fn h_tri<M: GfxMemory, B: GxBackend>(&mut self, _mem: &M, backend: &mut B, gfx: Gfx, count: u8) {
        for i in 0..count {
            let (a, b, c) = if i == 0 {
                (
                    ((gfx.w0 >> 16) & 0xFF) as u8,
                    ((gfx.w0 >> 8) & 0xFF) as u8,
                    (gfx.w0 & 0xFF) as u8,
                )
            } else {
                (
                    ((gfx.w1 >> 16) & 0xFF) as u8,
                    ((gfx.w1 >> 8) & 0xFF) as u8,
                    (gfx.w1 & 0xFF) as u8,
                )
            };
            // Retail divides N64 indices by 2 into the vertex array.
            if let (Some(&va), Some(&vb), Some(&vc)) =
                (self.vtx_cache.get(a), self.vtx_cache.get(b), self.vtx_cache.get(c))
            {
                backend.emit_triangle(EmittedTri { v: [va, vb, vc] });
            }
        }
    }

    fn h_quad<M: GfxMemory, B: GxBackend>(&mut self, _mem: &M, backend: &mut B, gfx: Gfx) {
        let idx = [
            ((gfx.w1 >> 24) & 0xFF) as u8,
            ((gfx.w1 >> 16) & 0xFF) as u8,
            ((gfx.w1 >> 8) & 0xFF) as u8,
            (gfx.w1 & 0xFF) as u8,
        ];
        let mut v = [DecodedVtx::default(); 4];
        for (i, &ix) in idx.iter().enumerate() {
            match self.vtx_cache.get(ix) {
                Some(&d) => v[i] = d,
                None => return,
            }
        }
        backend.emit_quad(v);
    }

    /// `dl_G_TRIN`: packed triangles. Returns the new pc.
    /// 5-bit: first pass 3 faces, subsequent passes 4 faces per word.
    /// 7-bit: `is_7bit` unpacking; faces per word differ per pass.
    /// `n_faces = ((w0 >> 17) & 0x7F) + 1`.
    fn h_trin<M: GfxMemory, B: GxBackend>(
        &mut self,
        _mem: &M,
        backend: &mut B,
        lists: &HashMap<u32, Vec<Gfx>>,
        list_addr: u32,
        pc: usize,
        first: Gfx,
    ) -> usize {
        let _is_7bit_first = (first.w1 & 1) == 1; // mode is per-word; see loop
        let mut n_faces = (((first.w0 >> 17) & 0x7F) + 1) as usize;
        let mut cur = pc;
        let mut first_pass = true;
        let list = match lists.get(&list_addr) {
            Some(l) => l,
            None => return pc + 1,
        };
        while n_faces > 0 {
            let g = match list.get(cur) {
                Some(&g) => g,
                None => break,
            };
            // Retail tests the 5b/7b bit per word, not once for the run.
            let word_7bit = (g.w1 & 1) == 1;
            let faces_this_word = if word_7bit { 3 } else if first_pass { 3 } else { 4 };
            for f in 0..faces_this_word {
                if n_faces == 0 {
                    break;
                }
                let (a, b, c) = if word_7bit {
                    unpack_7b(g, f)
                } else {
                    unpack_5b(g, f)
                };
                // TRIN indices are NOT halved (unlike TRI1/TRI2/QUAD).
                if let (Some(&va), Some(&vb), Some(&vc)) =
                    (self.vtx_cache.get_raw(a), self.vtx_cache.get_raw(b), self.vtx_cache.get_raw(c))
                {
                    backend.emit_triangle(EmittedTri { v: [va, vb, vc] });
                }
                n_faces -= 1;
            }
            cur += 1;
            first_pass = false;
        }
        cur
    }

    fn h_mtx(&mut self, gfx: Gfx) {
        // `type` is bits 0-7 of w0 (Gmtx.type). Retail discipline:
        // push when (type & G_MTX_PUSH) == G_MTX_NOPUSH (bit clear),
        // then LOAD-vs-MUL; pop is the separate G_POPMTX command.
        // The matrix itself is N64 s16.16 at the segmented address;
        // conversion to f32 happens in the GX backend (the engine feeds
        // matrices through set_matrix below).
        let typ = (gfx.w0 & 0xFF) as u8;
        let is_proj = typ & command::mtx_param::G_MTX_PROJECTION != 0;
        if is_proj {
            self.state.dirty.mark(Dirty::Projection);
        } else {
            if typ & command::mtx_param::G_MTX_PUSH == command::mtx_param::G_MTX_NOPUSH {
                if self.mv_stack.len() < 32 {
                    let top = *self.mv_stack.last().unwrap_or(&identity());
                    self.mv_stack.push(top);
                } else {
                    // Retail: "gsSPMatrix StackOverflow." + err_count++.
                    self.mtx_overflow += 1;
                }
            }
            // LOAD-vs-MUL and the s16.16 -> f32 conversion are applied
            // by the backend when it consumes set_matrix().
        }
    }

    /// Engine-side matrix upload (the GX backend feeds real matrices).
    pub fn set_matrix(&mut self, m: [f32; 16], projection: bool) {
        if projection {
            self.proj = m;
            self.state.dirty.mark(Dirty::Projection);
        } else if let Some(top) = self.mv_stack.last_mut() {
            *top = m;
        }
    }

    fn h_settimg(&mut self, gfx: Gfx, pc: &mut usize) {
        // Gsetimg2: wd:10 ht:8 isDolphin:1 siz:2 fmt:3 (TARGET_PC layout).
        let w0 = gfx.w0;
        let is_dolphin = (w0 >> 18) & 1 != 0;
        let (fmt, siz, w, h) = if is_dolphin {
            // _SHIFTL(fmt, 21, 3) | _SHIFTL(siz, 19, 2) | _SHIFTL(1, 18, 1)
            let fmt = ((w0 >> 21) & 0x7) as u8;
            let siz = ((w0 >> 19) & 0x3) as u8;
            let wd = ((w0 >> 0) & 0x3FF) as u16;
            let ht = ((w0 >> 10) & 0xFF) as u16;
            (fmt, siz, wd + 1, (ht + 1) * 4)
        } else {
            let fmt = ((w0 >> 21) & 0x7) as u8;
            let siz = ((w0 >> 19) & 0x3) as u8;
            let wd = (w0 & 0xFFF) as u16;
            (fmt, siz, wd + 1, 0)
        };
        self.state.tex_image_fmt = fmt;
        self.state.tex_image_siz = siz;
        self.state.tex_image_w = w;
        self.state.tex_image_h = h;
        self.state.tex_image_addr = gfx.w1;
        self.state.tex_image_dolphin = is_dolphin;
        self.state.dirty.mark(Dirty::Texture);
        *pc += 1;
    }

    fn h_settile(&mut self, gfx: Gfx, pc: &mut usize) {
        // Plain dl_G_SETTILE: stores the tile and clears the
        // per-tile Dolphin flag (Dolphin tiles use opcode 0xD2).
        let tile = ((gfx.w0 >> 16) & 0x7) as usize;
        if tile < self.state.tiles.len() {
            let t = &mut self.state.tiles[tile];
            t.is_dolphin = false;
            t.img_addr = self.state.tex_image_addr;
            self.state.dirty.mark(Dirty::Texture);
        }
        *pc += 1;
    }

    /// `dl_G_SETTILE_DOLPHIN` (opcode 0xD2): w0 packs dol_fmt:4 (bits
    /// 20-23), tile:3 (16-18), tlut:4 (12-15), wrap_s:2 (10-11),
    /// wrap_t:2 (8-9), shift_s:4 (4-7), shift_t:4 (0-3).
    fn h_settile_dolphin(&mut self, gfx: Gfx, pc: &mut usize) {
        let w0 = gfx.w0;
        let tile = ((w0 >> 16) & 0x7) as usize;
        if tile < self.state.tiles.len() {
            let t = &mut self.state.tiles[tile];
            t.is_dolphin = true;
            t.fmt = self.state.tex_image_fmt;
            t.siz = self.state.tex_image_siz;
            t.width = self.state.tex_image_w;
            t.height = self.state.tex_image_h;
            t.gx_format = super::texture::cvt_n64_to_gx(t.fmt, t.siz);
            t.wrap_s = ((w0 >> 10) & 0x3) as u8;
            t.wrap_t = ((w0 >> 8) & 0x3) as u8;
            t.tlut = ((w0 >> 12) & 0xF) as u8;
            t.img_addr = self.state.tex_image_addr;
            self.state.dirty.mark(Dirty::Texture);
        }
        *pc += 1;
    }
}

fn identity() -> [f32; 16] {
    let mut m = [0.0f32; 16];
    m[0] = 1.0;
    m[5] = 1.0;
    m[10] = 1.0;
    m[15] = 1.0;
    m
}

fn rgba(w1: u32) -> super::state::Rgba {
    super::state::Rgba {
        r: ((w1 >> 24) & 0xFF) as u8,
        g: ((w1 >> 16) & 0xFF) as u8,
        b: ((w1 >> 8) & 0xFF) as u8,
        a: (w1 & 0xFF) as u8,
    }
}

/// `Gtexrect2` layout: ulx = w0>>12, uly = w0&0xFFF,
/// lrx = w1>>12, lry = w1&0xFFF.
fn texrect_coords(gfx: Gfx) -> (u16, u16, u16, u16) {
    (
        ((gfx.w0 >> 12) & 0xFFF) as u16,
        ((gfx.w0 >> 0) & 0xFFF) as u16,
        ((gfx.w1 >> 12) & 0xFFF) as u16,
        ((gfx.w1 >> 0) & 0xFFF) as u16,
    )
}

/// 5-bit packed-triangle extraction, verbatim from gbi_extensions.h.
fn unpack_5b(g: Gfx, face: usize) -> (u8, u8, u8) {
    let w0 = g.w0;
    let w1 = g.w1;
    match face {
        0 => (
            ((w1 >> 4) & 0x1F) as u8,
            ((w1 >> 9) & 0x1F) as u8,
            ((w1 >> 14) & 0x1F) as u8,
        ),
        1 => (
            ((w1 >> 19) & 0x1F) as u8,
            ((w1 >> 24) & 0x1F) as u8,
            ((((w1 >> 29) & 7) | ((w0 & 3) << 3)) & 0x1F) as u8,
        ),
        2 => (
            ((w0 >> 2) & 0x1F) as u8,
            ((w0 >> 7) & 0x1F) as u8,
            ((w0 >> 12) & 0x1F) as u8,
        ),
        _ => (
            ((w0 >> 17) & 0x1F) as u8,
            ((w0 >> 22) & 0x1F) as u8,
            ((w0 >> 27) & 0x1F) as u8,
        ),
    }
}

/// 7-bit packed-triangle extraction, verbatim from gbi_extensions.h.
fn unpack_7b(g: Gfx, face: usize) -> (u8, u8, u8) {
    let w0 = g.w0;
    let w1 = g.w1;
    match face {
        0 => (
            ((w1 >> 1) & 0x7F) as u8,
            ((w1 >> 8) & 0x7F) as u8,
            ((w1 >> 15) & 0x7F) as u8,
        ),
        1 => (
            ((w1 >> 22) & 0x7F) as u8,
            ((((w1 >> 29) & 7) | ((w0 & 0xF) << 3)) & 0x7F) as u8,
            ((w0 >> 4) & 0x7F) as u8,
        ),
        _ => (
            ((w0 >> 11) & 0x7F) as u8,
            ((w0 >> 18) & 0x7F) as u8,
            ((w0 >> 25) & 0x7F) as u8,
        ),
    }
}

impl Default for Emu64 {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graphics::command::op;

    struct TestMem;
    impl GfxMemory for TestMem {
        fn vertices(&self, _a: u32, _n: usize) -> Option<Vec<Vtx>> {
            None
        }
        fn bytes(&self, _a: u32, _n: usize) -> Option<Vec<u8>> {
            None
        }
    }

    struct TestBackend {
        tris: usize,
        gxdl: usize,
        texrects: usize,
    }
    impl GxBackend for TestBackend {
        fn emit_triangle(&mut self, _t: EmittedTri) {
            self.tris += 1;
        }
        fn emit_quad(&mut self, _v: [DecodedVtx; 4]) {}
        fn emit_texrect(&mut self, _a: u16, _b: u16, _c: u16, _d: u16) {
            self.texrects += 1;
        }
        fn gx_call_display_list(&mut self, _b: &[u8]) {
            self.gxdl += 1;
        }
        fn cull_display_list(&mut self, _v0: u8, _vn: u8) -> bool {
            false
        }
    }

    fn gfx(opcode: u8, param: u8, len: u16, addr: u32) -> Gfx {
        Gfx::new(((opcode as u32) << 24) | ((param as u32) << 16) | (len as u32), addr)
    }

    #[test]
    fn dl_push_nopush_enddl() {
        let mut emu = Emu64::new();
        let mut be = TestBackend { tris: 0, gxdl: 0, texrects: 0 };
        emu.segments.set(1, 0x1000);
        // list at 0x1000: color, DL_PUSH to 0x2000, end
        // list at 0x2000: color, ENDDL
        let mut lists = HashMap::new();
        lists.insert(
            0x1000,
            vec![
                gfx(op::G_SETENVCOLOR, 0, 0, 0x1122_3344),
                gfx(op::G_DL, dl_param::G_DL_PUSH, 0, (1 << 24) | 0x2000), // seg 1 + 0x2000
                gfx(op::G_ENDDL, 0, 0, 0),
            ],
        );
        lists.insert(
            0x2000,
            vec![gfx(op::G_SETPRIMCOLOR, 0, 0, 0xAABB_CCDD), gfx(op::G_ENDDL, 0, 0, 0)],
        );
        emu.taskstart(&TestMem, &mut be, &lists, 0x1000);
        assert_eq!(emu.state.env_color.r, 0x11);
        assert_eq!(emu.state.prim_color.r, 0xAA);
        assert!(emu.cmds_processed >= 5);
    }

    #[test]
    fn gxdl_passthrough() {
        let mut emu = Emu64::new();
        let mut be = TestBackend { tris: 0, gxdl: 0, texrects: 0 };
        emu.segments.set(1, 0x5000);
        let mut lists = HashMap::new();
        lists.insert(
            0x5000,
            vec![
                gfx(op::G_DL, dl_param::G_DL_GXDL, 64, (1 << 24) | 0x6000),
                gfx(op::G_ENDDL, 0, 0, 0),
            ],
        );
        emu.taskstart(&TestMem, &mut be, &lists, 0x5000);
        assert_eq!(be.gxdl, 1);
    }

    #[test]
    fn unpack_5b_first_face() {
        // v0=1, v1=2, v2=3 in the first face slots.
        let g = Gfx::new(0, (3 << 14) | (2 << 9) | (1 << 4));
        assert_eq!(unpack_5b(g, 0), (1, 2, 3));
    }

    #[test]
    fn trin_variable_length() {
        let mut emu = Emu64::new();
        let mut be = TestBackend { tris: 0, gxdl: 0, texrects: 0 };
        // Load 4 verts via cache directly.
        let src = [
            Vtx { x: 0, ..Default::default() },
            Vtx { x: 1, ..Default::default() },
            Vtx { x: 2, ..Default::default() },
            Vtx { x: 3, ..Default::default() },
        ];
        emu.vtx_cache.load(0, 4, &src, false, false);
        // G_TRIN, 5-bit, n_faces = 1: one packed word follows the header.
        // TRIN indices are NOT halved: raw slots 0,1,2.
        let header = Gfx::new(((op::G_TRIN as u32) << 24) | (0 << 17), 0);
        let data = Gfx::new(0, (0 << 4) | (1 << 9) | (2 << 14)); // v0=0,v1=1,v2=2
        let mut lists = HashMap::new();
        lists.insert(0x7000, vec![header, data, Gfx::new((op::G_ENDDL as u32) << 24, 0)]);
        emu.taskstart(&TestMem, &mut be, &lists, 0x7000);
        assert_eq!(be.tris, 1);
    }
}
