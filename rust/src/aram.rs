//! PC implementation of Dolphin's offset-based 16 MiB ARAM interface.

use std::ffi::c_void;
use std::ptr;
use std::sync::Mutex;

const ARAM_SIZE: usize = 16 * 1024 * 1024;
const DMA_ZERO_LIMIT: usize = 0x10_0000;

struct Aram {
    bytes: Box<[u8]>,
    alloc_ptr: u32,
}

static ARAM: Mutex<Option<Aram>> = Mutex::new(None);

fn lock_aram() -> std::sync::MutexGuard<'static, Option<Aram>> {
    ARAM.lock().unwrap_or_else(|e| e.into_inner())
}

#[no_mangle]
pub unsafe extern "C" fn ARInit(_stack_idx_addr: *mut u32, _length: u32) -> u32 {
    let mut aram = lock_aram();
    if aram.is_none() {
        let mut bytes = Vec::new();
        if bytes.try_reserve_exact(ARAM_SIZE).is_err() {
            return 0;
        }
        bytes.resize(ARAM_SIZE, 0);
        *aram = Some(Aram {
            bytes: bytes.into_boxed_slice(),
            alloc_ptr: 0,
        });
    }
    0 // ARAM addresses are offsets from the always-zero base.
}

#[no_mangle]
pub extern "C" fn pc_aram_get_base() -> *mut u8 {
    let mut aram = lock_aram();
    aram.as_mut()
        .map_or(ptr::null_mut(), |state| state.bytes.as_mut_ptr())
}

#[no_mangle]
pub extern "C" fn ARGetBaseAddress() -> u32 {
    0
}

#[no_mangle]
pub extern "C" fn ARGetSize() -> u32 {
    ARAM_SIZE as u32
}

#[no_mangle]
pub extern "C" fn ARAlloc(size: u32) -> u32 {
    let mut aram = lock_aram();
    let Some(state) = aram.as_mut() else {
        return 0;
    };
    let Some(aligned) = size.checked_add(31).map(|n| n & !31) else {
        return 0;
    };
    let Some(end) = state.alloc_ptr.checked_add(aligned) else {
        return 0;
    };
    if end as usize > ARAM_SIZE {
        eprintln!(
            "[PC/ARAM] Out of ARAM! Requested {}, used {}/{}",
            size, state.alloc_ptr, ARAM_SIZE
        );
        return 0;
    }
    let address = state.alloc_ptr;
    state.alloc_ptr = end;
    address
}

/// Matches the PC layer (`void ARFree(u32* addr)` in `pc_aram.c`),
/// which is the ABI Wave 3 replaces. Note: the decomp's Dolphin header
/// guesses `u32 ARFree(u32*)` but flags it "Unused/inlined in P2"
/// (no out-of-line retail implementation to check against); the PC
/// layer and the official SDK use `void`, so `void` it is.
#[no_mangle]
pub extern "C" fn ARFree(_addr: *mut u32) {
    // The original PC layer uses a bump allocator; free is a no-op.
}

#[no_mangle]
pub unsafe extern "C" fn ARStartDMA(kind: u32, mram_addr: u32, mut aram_addr: u32, length: u32) {
    let mut guard = lock_aram();
    let Some(state) = guard.as_mut() else { return };

    let base = state.bytes.as_mut_ptr() as usize as u32;
    let base_end = base.wrapping_add(ARAM_SIZE as u32);
    if aram_addr >= base && aram_addr < base_end {
        aram_addr = aram_addr.wrapping_sub(base);
    }

    let length_usize = length as usize;
    let aram_start = aram_addr as usize;
    if length_usize > ARAM_SIZE || aram_start > ARAM_SIZE - length_usize {
        if kind == 1 && mram_addr != 0 && length_usize > 0 && length_usize <= DMA_ZERO_LIMIT {
            // SAFETY: The legacy API supplies a writable 32-bit process address.
            unsafe { ptr::write_bytes(mram_addr as usize as *mut u8, 0, length_usize) };
        }
        return;
    }

    let aram_ptr = state.bytes.as_mut_ptr().wrapping_add(aram_start);
    let mram_ptr = mram_addr as usize as *mut u8;
    if kind == 0 {
        // SAFETY: The legacy API provides a readable MRAM address and validated ARAM range.
        unsafe { ptr::copy(mram_ptr, aram_ptr, length_usize) };
    } else {
        // SAFETY: The legacy API provides a writable MRAM address and validated ARAM range.
        unsafe { ptr::copy(aram_ptr, mram_ptr, length_usize) };
    }
}

#[no_mangle]
pub extern "C" fn ARGetInternalSize() -> u32 {
    ARAM_SIZE as u32
}

#[no_mangle]
pub extern "C" fn ARCheckInit() -> i32 {
    lock_aram().is_some() as i32
}

#[no_mangle]
pub extern "C" fn ARQInit() {}

#[no_mangle]
pub unsafe extern "C" fn ARQPostRequest(
    request: *mut c_void,
    _owner: u32,
    kind: u32,
    _priority: u32,
    source: u32,
    dest: u32,
    length: u32,
    callback: Option<unsafe extern "C" fn(u32)>,
) {
    if kind == 0 {
        // SAFETY: Preserve Dolphin's ARQ direction and address order.
        unsafe { ARStartDMA(kind, source, dest, length) };
    } else {
        // SAFETY: For ARAM-to-MRAM, ARQ's source/dest order is opposite ARStartDMA.
        unsafe { ARStartDMA(kind, dest, source, length) };
    }
    if let Some(callback) = callback {
        // SAFETY: Callback follows the ARQCallback ABI; request is passed as its 32-bit id.
        unsafe { callback(request as usize as u32) };
    }
}

#[no_mangle]
pub extern "C" fn ARQFlushQueue() {}
