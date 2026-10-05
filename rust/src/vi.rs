//! PC Video Interface compatibility layer.
//!
//! The game continues to call the Dolphin VI C ABI. This module supplies the
//! PC frame boundary: event polling, pending GX work, buffer swap, frame
//! pacing, and retrace counting. Scene and display-list generation stay in C/C++.

use std::ffi::{c_char, c_void, CString};

const TIMER_GX_FLUSH: i32 = 2;
const TIMER_POLL_EVENTS: i32 = 10;
const TIMER_SWAP: i32 = 11;
const TIMER_PACE: i32 = 12;

#[no_mangle]
pub static mut g_frame_limiter: u32 = 60;
#[no_mangle]
pub static mut pc_frame_counter: u32 = 0;

static mut RETRACE_COUNT: u32 = 0;
static mut FRAME_START_TIME: u64 = 0;
static mut PERF_FREQ: u64 = 0;
static mut FPS_START: u64 = 0;
static mut FPS_COUNT: i32 = 0;
static mut PRE_RETRACE_CALLBACK: Option<extern "C" fn(u32)> = None;
static mut POST_RETRACE_CALLBACK: Option<extern "C" fn(u32)> = None;

extern "C" {
    static mut g_pc_running: i32;
    static mut g_pc_verbose: i32;
    static mut g_pc_window: *mut c_void;
    static mut g_pc_nes_active: i32;

    fn SDL_GetPerformanceCounter() -> u64;
    fn SDL_GetPerformanceFrequency() -> u64;
    fn SDL_Delay(milliseconds: u32);
    fn SDL_SetWindowTitle(window: *mut c_void, title: *const c_char);

    fn pc_platform_poll_events() -> i32;
    fn pc_platform_swap_buffers();
    fn pc_gx_draw_pending();
    fn pc_audio_get_buffer_fill() -> i32;
    fn pc_profiler_add_time_slow(timer: i32, start: u64);
    fn pc_profiler_end_frame(frame_ms: f64, audio_fill: i32);
}

fn profiling_enabled() -> bool {
    // This global is exported by the Rust profiler module under its C ABI name.
    unsafe { crate::profiler::g_pc_profile_enabled != 0 }
}

fn begin_profile_timer() -> u64 {
    if profiling_enabled() {
        // SAFETY: SDL is initialized before the VI frame loop starts.
        unsafe { SDL_GetPerformanceCounter() }
    } else {
        0
    }
}

fn add_profile_time(timer: i32, start: u64) {
    if profiling_enabled() {
        // SAFETY: This uses the existing pc_profiler_* C ABI.
        unsafe { pc_profiler_add_time_slow(timer, start) };
    }
}

#[no_mangle]
pub extern "C" fn VIInit() {
    let frame_limiter = unsafe { g_frame_limiter };
    if frame_limiter > 0 {
        let frame_period_us = ((1.0 / f64::from(frame_limiter)) * 1_000_000.0) as u32;
        println!("[VI] frame limit={frame_period_us}us ({frame_limiter} Hz)");
    } else {
        println!("[VI] frame limit=disabled");
    }
}

#[no_mangle]
pub extern "C" fn VIConfigure(_render_mode: *const c_void) {}

#[no_mangle]
pub extern "C" fn VISetNextFrameBuffer(_frame_buffer: *mut c_void) {}

#[no_mangle]
pub extern "C" fn VIFlush() {}

#[no_mangle]
pub extern "C" fn VIWaitForRetrace() {
    let mut perf_freq = unsafe { PERF_FREQ };
    if perf_freq == 0 {
        // SAFETY: SDL is initialized before the VI frame loop starts.
        perf_freq = unsafe { SDL_GetPerformanceFrequency() };
        unsafe { PERF_FREQ = perf_freq };
    }

    // SAFETY: SDL's performance counter is process-wide and monotonic.
    let vi_enter = unsafe { SDL_GetPerformanceCounter() };
    let frame_start_time = unsafe { FRAME_START_TIME };
    let frame_ms = if frame_start_time != 0 {
        vi_enter.wrapping_sub(frame_start_time) as f64 * 1000.0 / perf_freq as f64
    } else {
        0.0
    };

    let t_before_poll = begin_profile_timer();
    // SAFETY: Event polling runs on the same thread that owns SDL's window.
    if unsafe { pc_platform_poll_events() } == 0 {
        unsafe { g_pc_running = 0 };
        return;
    }
    add_profile_time(TIMER_POLL_EVENTS, t_before_poll);

    // Attribute the last deferred GX batch to flush time, as the C shim does.
    let t_drain = begin_profile_timer();
    // SAFETY: This is the existing renderer entry point, called on the game thread.
    unsafe { pc_gx_draw_pending() };
    add_profile_time(TIMER_GX_FLUSH, t_drain);

    // SAFETY: The PC platform owns SDL's window and performs the GL swap.
    let t_before_swap = unsafe { SDL_GetPerformanceCounter() };
    let t_before_swap_prof = begin_profile_timer();
    unsafe { pc_platform_swap_buffers() };
    add_profile_time(TIMER_SWAP, t_before_swap_prof);
    let t_after_swap = unsafe { SDL_GetPerformanceCounter() };

    let t_before_pace = unsafe { SDL_GetPerformanceCounter() };
    let t_before_pace_prof = begin_profile_timer();
    let nes_active = unsafe { g_pc_nes_active != 0 };
    let frame_limiter = unsafe { g_frame_limiter };
    let pace_us = if nes_active {
        16_667
    } else if frame_limiter > 0 {
        ((1.0 / f64::from(frame_limiter)) * 1_000_000.0) as i32
    } else {
        0
    };

    if (nes_active || frame_limiter > 0) && frame_start_time != 0 {
        let mut now = unsafe { SDL_GetPerformanceCounter() };
        let mut elapsed_us = now.wrapping_sub(frame_start_time).wrapping_mul(1_000_000) / perf_freq;
        while elapsed_us < pace_us as u64 {
            let remain_us = pace_us as u64 - elapsed_us;
            if remain_us > 2_000 {
                // SAFETY: SDL_Delay is the platform-neutral coarse part of frame pacing.
                unsafe { SDL_Delay(1) };
            }
            now = unsafe { SDL_GetPerformanceCounter() };
            elapsed_us = now.wrapping_sub(frame_start_time).wrapping_mul(1_000_000) / perf_freq;
        }
    }
    add_profile_time(TIMER_PACE, t_before_pace_prof);
    let t_after_pace = unsafe { SDL_GetPerformanceCounter() };
    let profile_frame_ms = if frame_start_time != 0 {
        t_after_pace.wrapping_sub(frame_start_time) as f64 * 1000.0 / perf_freq as f64
    } else {
        frame_ms
    };

    if frame_ms > 20.0 && unsafe { g_pc_verbose != 0 } {
        let swap_ms = t_after_swap.wrapping_sub(t_before_swap) as f64 * 1000.0 / perf_freq as f64;
        let pace_ms = t_after_pace.wrapping_sub(t_before_pace) as f64 * 1000.0 / perf_freq as f64;
        let work_ms = vi_enter.wrapping_sub(frame_start_time) as f64 * 1000.0 / perf_freq as f64;
        let audio_fill = unsafe { pc_audio_get_buffer_fill() };
        let frame = unsafe { pc_frame_counter };
        println!(
            "[STUTTER] frame {frame}: total={frame_ms:.1}ms work={:.1}ms swap={swap_ms:.1}ms pace={pace_ms:.1}ms audio_fill={audio_fill}",
            work_ms - swap_ms - pace_ms
        );
    }

    let audio_fill = unsafe { pc_audio_get_buffer_fill() };
    // SAFETY: This preserves the existing profiler's per-frame C ABI.
    unsafe { pc_profiler_end_frame(profile_frame_ms, audio_fill) };

    let mut fps_start = unsafe { FPS_START };
    let mut fps_count = unsafe { FPS_COUNT };
    if fps_start == 0 {
        fps_start = unsafe { SDL_GetPerformanceCounter() };
    }
    fps_count += 1;
    if fps_count >= 60 {
        let now = unsafe { SDL_GetPerformanceCounter() };
        let elapsed_seconds = now.wrapping_sub(fps_start) as f64 / perf_freq as f64;
        let fps = f64::from(fps_count) / elapsed_seconds;
        let title = CString::new(format!("Animal Crossing - {fps:.1} FPS"));
        if let Ok(title) = title {
            // SAFETY: SDL owns the window; CString keeps the title valid for this call.
            unsafe { SDL_SetWindowTitle(g_pc_window, title.as_ptr()) };
        }
        fps_start = unsafe { SDL_GetPerformanceCounter() };
        fps_count = 0;
    }
    unsafe {
        FPS_START = fps_start;
        FPS_COUNT = fps_count;
        FRAME_START_TIME = SDL_GetPerformanceCounter();
        RETRACE_COUNT = RETRACE_COUNT.wrapping_add(1);
        pc_frame_counter = pc_frame_counter.wrapping_add(1);
    }
}

#[no_mangle]
pub extern "C" fn VIGetRetraceCount() -> u32 {
    unsafe { RETRACE_COUNT }
}

#[no_mangle]
pub extern "C" fn VISetBlack(_black: i32) {}

#[no_mangle]
pub extern "C" fn VIGetTvFormat() -> u32 {
    0 // VI_NTSC
}

#[no_mangle]
pub extern "C" fn VIGetDTVStatus() -> u32 {
    0
}

#[no_mangle]
pub extern "C" fn VISetPreRetraceCallback(
    callback: Option<extern "C" fn(u32)>,
) -> Option<extern "C" fn(u32)> {
    // SAFETY: The legacy shim stores this callback on the game thread only.
    unsafe { std::ptr::replace(std::ptr::addr_of_mut!(PRE_RETRACE_CALLBACK), callback) }
}

#[no_mangle]
pub extern "C" fn VISetPostRetraceCallback(
    callback: Option<extern "C" fn(u32)>,
) -> Option<extern "C" fn(u32)> {
    // SAFETY: The legacy shim stores this callback on the game thread only.
    unsafe { std::ptr::replace(std::ptr::addr_of_mut!(POST_RETRACE_CALLBACK), callback) }
}

#[no_mangle]
pub extern "C" fn VIGetCurrentLine() -> u32 {
    0
}

#[no_mangle]
pub extern "C" fn VISetNextXFB(_frame_buffer: *mut c_void) {}
