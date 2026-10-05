//! Frame profiler for the PC renderer and its emulation layer.
//!
//! The C header keeps the hot-path timer wrappers and the public C ABI. SDL
//! supplies the same high-resolution counter on both supported PC platforms.

use std::fmt::Write as _;
use std::sync::Mutex;

const TIMER_COUNT: usize = 20;
const DIRTY_GROUP_COUNT: usize = 16;

#[no_mangle]
pub static mut g_pc_profile_enabled: i32 = 0;
#[no_mangle]
pub static mut g_pc_profile_interval: i32 = 120;

const TIMER_NAMES: [&str; TIMER_COUNT] = [
    "gx_begin",
    "dl_replay",
    "gx_flush",
    "buf_upload",
    "uniforms",
    "uniform_lookup",
    "tex_bind",
    "shader_switch",
    "gl_state",
    "draw_submit",
    "poll",
    "swap",
    "pace",
    "game_logic",
    "emu64_task",
    "texobj",
    "draw_finish",
    "audio_frame",
    "jw_frame",
    "efb_copy",
];

const DIRTY_NAMES: [&str; DIRTY_GROUP_COUNT] = [
    "proj",
    "modelview",
    "tev_colors",
    "tev_stages",
    "swap",
    "konst",
    "alpha",
    "lighting",
    "texgen",
    "textures",
    "indirect",
    "fog",
    "depth",
    "color_mask",
    "cull",
    "blend",
];

#[derive(Clone, Copy)]
struct ProfilerFrame {
    timers_ms: [f64; TIMER_COUNT],
    draws: i32,
    flushes: i32,
    shader_switches: i32,
    uniforms: i32,
    uniform_lookups: i32,
    texture_binds: i32,
    buffer_uploads: i32,
    buffer_upload_bytes: usize,
    state_changes: i32,
    dirty_groups: [i32; DIRTY_GROUP_COUNT],
    vertices: i32,
    indices: i32,
    emu64_cmds: i32,
    emu64_tris: i32,
    emu64_vtx_cmds: i32,
    emu64_dl_cmds: i32,
    cull_visible: i32,
    cull_rejected: i32,
    frame_ms: f64,
    audio_fill: i32,
}

impl ProfilerFrame {
    const fn zeroed() -> Self {
        Self {
            timers_ms: [0.0; TIMER_COUNT],
            draws: 0,
            flushes: 0,
            shader_switches: 0,
            uniforms: 0,
            uniform_lookups: 0,
            texture_binds: 0,
            buffer_uploads: 0,
            buffer_upload_bytes: 0,
            state_changes: 0,
            dirty_groups: [0; DIRTY_GROUP_COUNT],
            vertices: 0,
            indices: 0,
            emu64_cmds: 0,
            emu64_tris: 0,
            emu64_vtx_cmds: 0,
            emu64_dl_cmds: 0,
            cull_visible: 0,
            cull_rejected: 0,
            frame_ms: 0.0,
            audio_fill: 0,
        }
    }
}

struct ProfilerState {
    frame: ProfilerFrame,
    accum: ProfilerFrame,
    peak: ProfilerFrame,
    frames: i32,
    have_frame: bool,
    frame_marked: bool,
    frequency: u64,
}

impl ProfilerState {
    const fn new() -> Self {
        Self {
            frame: ProfilerFrame::zeroed(),
            accum: ProfilerFrame::zeroed(),
            peak: ProfilerFrame::zeroed(),
            frames: 0,
            have_frame: false,
            frame_marked: false,
            frequency: 0,
        }
    }
}

static STATE: Mutex<ProfilerState> = Mutex::new(ProfilerState::new());

unsafe extern "C" {
    fn SDL_GetPerformanceCounter() -> u64;
    fn SDL_GetPerformanceFrequency() -> u64;

    static mut pc_emu64_frame_cmds: i32;
    static mut pc_emu64_frame_tri_cmds: i32;
    static mut pc_emu64_frame_vtx_cmds: i32;
    static mut pc_emu64_frame_dl_cmds: i32;
    static mut pc_emu64_frame_cull_visible: i32;
    static mut pc_emu64_frame_cull_rejected: i32;
}

fn enabled() -> bool {
    // SAFETY: The C runtime owns this setting and updates it on the main thread.
    unsafe { g_pc_profile_enabled != 0 }
}

fn state() -> std::sync::MutexGuard<'static, ProfilerState> {
    STATE.lock().unwrap_or_else(|error| error.into_inner())
}

fn ticks_to_ms(state: &mut ProfilerState, ticks: u64) -> f64 {
    if state.frequency == 0 {
        // SAFETY: SDL is initialized before the profiler's frame loop begins.
        state.frequency = unsafe { SDL_GetPerformanceFrequency() };
    }
    ticks as f64 * 1000.0 / state.frequency as f64
}

fn accumulate_frame(state: &mut ProfilerState) {
    let frame = state.frame;
    for index in 0..TIMER_COUNT {
        state.accum.timers_ms[index] += frame.timers_ms[index];
    }
    state.accum.draws += frame.draws;
    state.accum.flushes += frame.flushes;
    state.accum.shader_switches += frame.shader_switches;
    state.accum.uniforms += frame.uniforms;
    state.accum.uniform_lookups += frame.uniform_lookups;
    state.accum.texture_binds += frame.texture_binds;
    state.accum.buffer_uploads += frame.buffer_uploads;
    state.accum.buffer_upload_bytes += frame.buffer_upload_bytes;
    state.accum.state_changes += frame.state_changes;
    for index in 0..DIRTY_GROUP_COUNT {
        state.accum.dirty_groups[index] += frame.dirty_groups[index];
    }
    state.accum.vertices += frame.vertices;
    state.accum.indices += frame.indices;
    state.accum.emu64_cmds += frame.emu64_cmds;
    state.accum.emu64_tris += frame.emu64_tris;
    state.accum.emu64_vtx_cmds += frame.emu64_vtx_cmds;
    state.accum.emu64_dl_cmds += frame.emu64_dl_cmds;
    state.accum.cull_visible += frame.cull_visible;
    state.accum.cull_rejected += frame.cull_rejected;
    state.accum.frame_ms += frame.frame_ms;
    state.accum.audio_fill += frame.audio_fill;

    if frame.frame_ms > state.peak.frame_ms {
        state.peak = frame;
    }
}

fn print_report(state: &ProfilerState) {
    let count = state.frames as f64;
    let average_frame = state.accum.frame_ms / count;
    let fps = if average_frame > 0.0 {
        1000.0 / average_frame
    } else {
        0.0
    };
    let submit_ms = state.accum.timers_ms[2] / count;
    let poll_ms = state.accum.timers_ms[10] / count;
    let swap_ms = state.accum.timers_ms[11] / count;
    let pace_ms = state.accum.timers_ms[12] / count;
    let non_submit_ms = (average_frame - submit_ms - poll_ms - swap_ms - pace_ms).max(0.0);

    println!(
        "[PROFILE] frames={} avg={:.3}ms {:.1}fps peak={:.3}ms draws={:.1} flushes={:.1} verts={:.0} idx={:.0}",
        state.frames,
        average_frame,
        fps,
        state.peak.frame_ms,
        state.accum.draws as f64 / count,
        state.accum.flushes as f64 / count,
        state.accum.vertices as f64 / count,
        state.accum.indices as f64 / count
    );
    println!(
        "[PROFILE] cpu_other={:.3}ms gx_flush={:.3}ms poll={:.3}ms swap={:.3}ms pace={:.3}ms audio_fill={:.0}",
        non_submit_ms,
        submit_ms,
        poll_ms,
        swap_ms,
        pace_ms,
        state.accum.audio_fill as f64 / count
    );

    let mut report = String::from("[PROFILE] timers");
    for (index, name) in TIMER_NAMES.iter().enumerate() {
        let _ = write!(
            report,
            " {name}={:.3}",
            state.accum.timers_ms[index] / count
        );
    }
    println!("{report}");

    println!(
        "[PROFILE] gl calls/state per frame: uniforms={:.1} lookups={:.1} tex_binds={:.1} buf_uploads={:.1} {:.1}KB shader_switch={:.1} state={:.1} cmds={:.1} tris={:.1} vtxcmd={:.1} dl={:.1} cull={:.1}/{:.1}",
        state.accum.uniforms as f64 / count,
        state.accum.uniform_lookups as f64 / count,
        state.accum.texture_binds as f64 / count,
        state.accum.buffer_uploads as f64 / count,
        state.accum.buffer_upload_bytes as f64 / count / 1024.0,
        state.accum.shader_switches as f64 / count,
        state.accum.state_changes as f64 / count,
        state.accum.emu64_cmds as f64 / count,
        state.accum.emu64_tris as f64 / count,
        state.accum.emu64_vtx_cmds as f64 / count,
        state.accum.emu64_dl_cmds as f64 / count,
        state.accum.cull_visible as f64 / count,
        state.accum.cull_rejected as f64 / count
    );

    let mut dirty_report = String::from("[PROFILE] dirty groups per frame:");
    for (index, name) in DIRTY_NAMES.iter().enumerate() {
        let average = state.accum.dirty_groups[index] as f64 / count;
        if average > 0.0 {
            let _ = write!(dirty_report, " {name}={average:.1}");
        }
    }
    println!("{dirty_report}");
}

#[no_mangle]
pub extern "C" fn pc_profiler_begin_frame() {
    if !enabled() {
        return;
    }

    let mut state = state();
    if state.frame_marked {
        accumulate_frame(&mut state);
        state.frames += 1;
        state.frame_marked = false;

        // SAFETY: The profiling interval is configured by the C runtime.
        if state.frames >= unsafe { g_pc_profile_interval } {
            print_report(&state);
            state.accum = ProfilerFrame::zeroed();
            state.peak = ProfilerFrame::zeroed();
            state.frames = 0;
        }
    }
    state.frame = ProfilerFrame::zeroed();
    state.have_frame = true;
}

#[no_mangle]
pub extern "C" fn pc_profiler_add_time_slow(timer: i32, start: u64) {
    if start == 0 || !(0..TIMER_COUNT as i32).contains(&timer) {
        return;
    }
    let mut state = state();
    if !state.have_frame {
        return;
    }
    // SAFETY: SDL's counter is process-wide and uses the matching performance frequency.
    let now = unsafe { SDL_GetPerformanceCounter() };
    let elapsed = now.wrapping_sub(start);
    let elapsed_ms = ticks_to_ms(&mut state, elapsed);
    state.frame.timers_ms[timer as usize] += elapsed_ms;
}

#[no_mangle]
pub extern "C" fn pc_profiler_add_count_draw_slow(vertices: i32, indices: i32) {
    let mut state = state();
    if state.have_frame {
        state.frame.draws += 1;
        state.frame.vertices += vertices;
        state.frame.indices += indices;
    }
}

#[no_mangle]
pub extern "C" fn pc_profiler_add_count_flush_slow() {
    let mut state = state();
    if state.have_frame {
        state.frame.flushes += 1;
    }
}

#[no_mangle]
pub extern "C" fn pc_profiler_add_count_shader_switch_slow() {
    let mut state = state();
    if state.have_frame {
        state.frame.shader_switches += 1;
    }
}

#[no_mangle]
pub extern "C" fn pc_profiler_add_count_uniform_slow() {
    let mut state = state();
    if state.have_frame {
        state.frame.uniforms += 1;
    }
}

#[no_mangle]
pub extern "C" fn pc_profiler_add_count_uniform_lookup_slow() {
    let mut state = state();
    if state.have_frame {
        state.frame.uniform_lookups += 1;
    }
}

#[no_mangle]
pub extern "C" fn pc_profiler_add_count_texture_bind_slow() {
    let mut state = state();
    if state.have_frame {
        state.frame.texture_binds += 1;
    }
}

#[no_mangle]
pub extern "C" fn pc_profiler_add_count_buffer_upload_slow(bytes: usize) {
    let mut state = state();
    if state.have_frame {
        state.frame.buffer_uploads += 1;
        state.frame.buffer_upload_bytes += bytes;
    }
}

#[no_mangle]
pub extern "C" fn pc_profiler_add_count_state_change_slow() {
    let mut state = state();
    if state.have_frame {
        state.frame.state_changes += 1;
    }
}

#[no_mangle]
pub extern "C" fn pc_profiler_add_dirty_mask_slow(dirty: u32) {
    let mut state = state();
    if !state.have_frame {
        return;
    }
    for index in 0..DIRTY_GROUP_COUNT {
        if dirty & (1 << index) != 0 {
            state.frame.dirty_groups[index] += 1;
        }
    }
}

/// Ends the visible frame while keeping its profiler record open for post-VI work.
#[no_mangle]
pub extern "C" fn pc_profiler_end_frame(frame_ms: f64, audio_fill: i32) {
    if !enabled() {
        return;
    }

    let mut state = state();
    if !state.have_frame {
        return;
    }

    // SAFETY: These counters are owned and updated by the C emulation runtime.
    unsafe {
        state.frame.frame_ms = frame_ms;
        state.frame.audio_fill = audio_fill;
        state.frame.emu64_cmds = pc_emu64_frame_cmds;
        state.frame.emu64_tris = pc_emu64_frame_tri_cmds;
        state.frame.emu64_vtx_cmds = pc_emu64_frame_vtx_cmds;
        state.frame.emu64_dl_cmds = pc_emu64_frame_dl_cmds;
        state.frame.cull_visible = pc_emu64_frame_cull_visible;
        state.frame.cull_rejected = pc_emu64_frame_cull_rejected;
    }
    state.frame_marked = true;
}
