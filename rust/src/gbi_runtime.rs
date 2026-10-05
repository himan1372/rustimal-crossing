//! Packs native runtime pointers into the 32-bit display-list representation.

use std::ffi::{c_char, CStr};
use std::sync::Mutex;

const TOKEN_BASE: u32 = 0x02F0_0000;
const TOKEN_COUNT: usize = 8192;

struct TokenTable {
    values: [usize; TOKEN_COUNT],
    next: usize,
    warned: bool,
}

static TOKENS: Mutex<TokenTable> = Mutex::new(TokenTable {
    values: [0; TOKEN_COUNT],
    next: 0,
    warned: false,
});

fn c_text(value: *const c_char) -> String {
    if value.is_null() {
        return String::new();
    }
    // SAFETY: The C ABI supplies NUL-terminated source-location strings.
    unsafe { CStr::from_ptr(value).to_string_lossy().into_owned() }
}

#[no_mangle]
pub extern "C" fn pc_gbi_pack_runtime_ptr(
    address: usize,
    is_ptr: i32,
    expr: *const c_char,
    file: *const c_char,
    line: i32,
) -> u32 {
    if is_ptr == 0 {
        return address as u32;
    }
    if address & 1 == 0 {
        return (address | 1) as u32;
    }

    let mut table = TOKENS.lock().unwrap_or_else(|e| e.into_inner());
    let slot = table.next & (TOKEN_COUNT - 1);
    table.next = table.next.wrapping_add(1);
    table.values[slot] = address;
    if !table.warned {
        eprintln!(
            "[GBI] odd pointer alignment: {} at {}:{} = 0x{:08x}; using fallback token table",
            c_text(expr),
            c_text(file),
            line,
            address as u32
        );
        table.warned = true;
    }
    TOKEN_BASE + (slot as u32) * 2
}

#[no_mangle]
pub extern "C" fn pc_gbi_unpack_runtime_ptr(packed: u32) -> usize {
    let token = packed.wrapping_sub(TOKEN_BASE);
    if token >= (TOKEN_COUNT as u32) * 2 || token & 1 != 0 {
        return 0;
    }
    TOKENS.lock().unwrap_or_else(|e| e.into_inner()).values[(token / 2) as usize]
}
