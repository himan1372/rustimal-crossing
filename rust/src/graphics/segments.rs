//! Segmented addresses and the display-list call stack.
//!
//! Model data addresses are *segmented*: the top byte selects one of
//! `EMU64_NUM_SEGMENTS = 16` base addresses, the low 24 bits are the
//! offset within the segment. `gSPSegment()` installs segment bases at
//! runtime (e.g. the billboard matrix goes into segment 7; animated
//! texture frames live in segments 0x08-0x0D).

/// Number of segments (`EMU64_NUM_SEGMENTS`).
pub const NUM_SEGMENTS: usize = 16;
/// Display-list call-stack depth (`DL_MAX_STACK_LEVEL`).
pub const DL_MAX_STACK_LEVEL: usize = 18;

/// The 16-entry segment table: `segments[seg] = base address`.
#[derive(Clone, Debug)]
pub struct SegmentTable {
    pub segments: [u32; NUM_SEGMENTS],
}

impl SegmentTable {
    pub fn new() -> SegmentTable {
        SegmentTable { segments: [0; NUM_SEGMENTS] }
    }

    /// `gSPSegment(seg, base)`.
    pub fn set(&mut self, seg: u8, base: u32) {
        if (seg as usize) < NUM_SEGMENTS {
            self.segments[seg as usize] = base;
        }
    }

    /// `seg2k0`: resolve a segmented address to a flat address.
    /// Returns `None` for an invalid segment (retail's `segchk`
    /// warns/validates; the TARGET_PC build refuses NULL).
    pub fn resolve(&self, seg_addr: u32) -> Option<u32> {
        let seg = (seg_addr >> 24) as usize;
        if seg >= NUM_SEGMENTS {
            return None;
        }
        Some(self.segments[seg].wrapping_add(seg_addr & 0x00FF_FFFF))
    }

    /// Build a segmented address from a segment id and offset.
    pub fn make(seg: u8, offset: u32) -> u32 {
        ((seg as u32) << 24) | (offset & 0x00FF_FFFF)
    }
}

impl Default for SegmentTable {
    fn default() -> Self {
        Self::new()
    }
}

/// The display-list call stack.
///
/// `G_DL` with `G_DL_PUSH` pushes the return address (`gfx_p + 1`) and
/// jumps; `G_DL_NOPUSH` jumps without pushing; `G_ENDDL` pops or, when
/// the stack is empty, terminates the task. Overflow is counted, not
/// fatal (retail prints "*** DL stack overflow ***").
#[derive(Clone, Debug)]
pub struct DlStack {
    pub stack: [u32; DL_MAX_STACK_LEVEL],
    pub level: usize,
    pub overflow_count: u32,
}

impl DlStack {
    pub fn new() -> DlStack {
        DlStack { stack: [0; DL_MAX_STACK_LEVEL], level: 0, overflow_count: 0 }
    }

    /// Push a return address. Returns `false` on overflow.
    pub fn push(&mut self, ret: u32) -> bool {
        if self.level < DL_MAX_STACK_LEVEL {
            self.stack[self.level] = ret;
            self.level += 1;
            true
        } else {
            self.overflow_count += 1;
            false
        }
    }

    /// Pop a return address; `None` means the task ends.
    pub fn pop(&mut self) -> Option<u32> {
        if self.level > 0 {
            self.level -= 1;
            Some(self.stack[self.level])
        } else {
            None
        }
    }
}

impl Default for DlStack {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segment_resolve() {
        let mut t = SegmentTable::new();
        t.set(7, 0x8000_0000);
        assert_eq!(t.resolve(SegmentTable::make(7, 0x1234)), Some(0x8000_1234));
        assert_eq!(t.resolve(0xFF00_0000), None);
    }

    #[test]
    fn dl_stack_push_pop() {
        let mut s = DlStack::new();
        assert!(s.push(100));
        assert!(s.push(200));
        assert_eq!(s.pop(), Some(200));
        assert_eq!(s.pop(), Some(100));
        assert_eq!(s.pop(), None); // task ends
    }

    #[test]
    fn dl_stack_overflow_counted() {
        let mut s = DlStack::new();
        for i in 0..DL_MAX_STACK_LEVEL {
            assert!(s.push(i as u32));
        }
        assert!(!s.push(999));
        assert_eq!(s.overflow_count, 1);
    }
}
