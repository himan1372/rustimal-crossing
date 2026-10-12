//! Test-only stand-ins for the C/SDL symbols the crate imports.
//!
//! The library is a staticlib linked into the C game, so symbols such as
//! `g_pc_verbose`, `SDL_GetPerformanceCounter` or `pc_platform_swap_buffers`
//! come from `pc_main.c`, SDL and the graphics/audio layers. A
//! `cargo test` binary links the crate alone, so on Windows (MSVC and
//! mingw32 alike) those references are unresolved and the test binary fails
//! to link. Linux toolchains garbage-collect the unused exports, which is
//! why the suite links there.
//!
//! These definitions exist only under `cfg(test)` and are never part of the
//! game build. They return inert values: not running, not verbose, no
//! window, zero counters.

#![allow(non_upper_case_globals)]

use std::ffi::{c_char, c_void};

#[no_mangle]
pub static mut g_pc_running: i32 = 0;
#[no_mangle]
pub static mut g_pc_verbose: i32 = 0;
#[no_mangle]
pub static mut g_pc_window: *mut c_void = std::ptr::null_mut();
#[no_mangle]
pub static mut g_pc_nes_active: i32 = 0;

#[no_mangle]
pub static mut pc_emu64_frame_cmds: i32 = 0;
#[no_mangle]
pub static mut pc_emu64_frame_tri_cmds: i32 = 0;
#[no_mangle]
pub static mut pc_emu64_frame_vtx_cmds: i32 = 0;
#[no_mangle]
pub static mut pc_emu64_frame_dl_cmds: i32 = 0;
#[no_mangle]
pub static mut pc_emu64_frame_cull_visible: i32 = 0;
#[no_mangle]
pub static mut pc_emu64_frame_cull_rejected: i32 = 0;

#[no_mangle]
pub extern "C" fn SDL_GetPerformanceCounter() -> u64 {
    0
}
#[no_mangle]
pub extern "C" fn SDL_GetPerformanceFrequency() -> u64 {
    1_000_000
}
#[no_mangle]
pub extern "C" fn SDL_Delay(_milliseconds: u32) {}
#[no_mangle]
pub extern "C" fn SDL_SetWindowTitle(_window: *mut c_void, _title: *const c_char) {}

#[no_mangle]
pub extern "C" fn pc_platform_poll_events() -> i32 {
    0
}
#[no_mangle]
pub extern "C" fn pc_platform_swap_buffers() {}
#[no_mangle]
pub extern "C" fn pc_gx_draw_pending() {}
#[no_mangle]
pub extern "C" fn pc_audio_get_buffer_fill() -> i32 {
    0
}
