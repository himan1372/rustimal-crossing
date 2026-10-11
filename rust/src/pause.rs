//! Debug pause helper (m_pause.c) pure kernels.
//!
//! Verified against the ACGC-PC-Port tree (GAFE01_00 Rev 0 USA
//! base). Despite the name, this is NOT a pause-menu UI — it is a
//! tiny debug-mode-gated input/state helper with no graphics,
//! options, or overlay state machine. It returns 1 when the
//! gameplay update may proceed and 0 when it stays gated; the
//! caller (`Game_play_move`) inverts the result.
//!
//! Tree-vs-brief note: the brief's `Pause_proc` (int timer,
//! `++timer > 8` folded into the return check) matches the retail
//! N64 form; the PC-port tree carries `f32 timer` with a separate
//! dt-accumulation block (`timer += dt_num_60fps_frames`, tested
//! against `8.0f`). This port follows the tree, since that is
//! what the C side being rewritten actually runs.
//!
//! The following stay in C: the `ZURUMODE2_ENABLED()` gate, the
//! `!Pause_proc` inversion and `pause` flag in `Game_play_move`,
//! `dt_num_60fps_frames` sourcing, the separate PC-port
//! `g_pc_paused` coarse pause, and pad polling.

#![allow(non_snake_case)]

use std::os::raw::c_int;

/// `pause_t` (`include/m_pause.h`): 8 bytes, `enabled` first.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct PauseState {
    pub enabled: c_int,
    pub timer: f32,
}

/// Button bits (`include/types.h`).
pub const PAUSE_BUTTON_R: u16 = 0x0010;
pub const PAUSE_BUTTON_DDOWN: u16 = 0x0400;
pub const PAUSE_BUTTON_Z: u16 = 0x2000;
/// Unpause timer threshold (strict `>`).
pub const PAUSE_TIMER_THRESHOLD: f32 = 8.0;

/// `Pause_ct` (`m_pause.c`): zero the state.
#[no_mangle]
pub extern "C" fn pc_pause_init(state: *mut PauseState) {
    if state.is_null() {
        return;
    }
    unsafe {
        (*state).timer = 0.0;
        (*state).enabled = 0;
    }
}

/// `Pause_proc` (`m_pause.c`): the debug pause gate. `now_buttons`
/// is the held-button bitfield (`pad->now`), `on_buttons` the
/// newly-pressed bitfield (`pad->on`), `dt_frames` the injected
/// `dt_num_60fps_frames`. Returns 1 when the gameplay update may
/// proceed (and resets the timer), 0 when it stays gated.
#[no_mangle]
pub extern "C" fn pc_pause_proc(
    state: *mut PauseState,
    now_buttons: u16,
    on_buttons: u16,
    dt_frames: f32,
) -> c_int {
    if state.is_null() {
        return 1;
    }
    let s = unsafe { &mut *state };
    let now = now_buttons;
    let on = on_buttons;

    // Toggle BEFORE the return check, so the new value applies now.
    if (now & PAUSE_BUTTON_R) != 0 && (on & PAUSE_BUTTON_DDOWN) != 0 {
        s.enabled = if s.enabled != 0 { 0 } else { 1 };
    }

    // PC-port dt accumulation (separate block in the tree).
    if (now & PAUSE_BUTTON_Z) != 0 && (now & PAUSE_BUTTON_R) != 0 && (on & PAUSE_BUTTON_R) == 0 {
        s.timer += dt_frames;
    }

    if s.enabled == 0
        || ((now & PAUSE_BUTTON_Z) != 0
            && ((on & PAUSE_BUTTON_R) != 0
                || ((now & PAUSE_BUTTON_R) != 0 && s.timer > PAUSE_TIMER_THRESHOLD)))
    {
        s.timer = 0.0;
        1
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proc(s: &mut PauseState, now: u16, on: u16, dt: f32) -> c_int {
        pc_pause_proc(s as *mut PauseState, now, on, dt)
    }

    #[test]
    fn init_zeroes() {
        let mut s = PauseState { enabled: 1, timer: 5.0 };
        pc_pause_init(&mut s as *mut PauseState);
        assert_eq!((s.enabled, s.timer), (0, 0.0));
    }

    #[test]
    fn disabled_proceeds_and_resets_timer() {
        let mut s = PauseState { enabled: 0, timer: 99.0 };
        assert_eq!(proc(&mut s, 0, 0, 1.0), 1);
        assert_eq!(s.timer, 0.0);
    }

    #[test]
    fn toggle_needs_r_held_and_ddown_new() {
        let mut s = PauseState::default();
        // D-down newly pressed without R held -> no toggle
        assert_eq!(proc(&mut s, 0, PAUSE_BUTTON_DDOWN, 1.0), 1);
        assert_eq!(s.enabled, 0);
        // R newly pressed (not just held) + D-down newly pressed still toggles:
        // the toggle only needs R HELD
        assert_eq!(proc(&mut s, PAUSE_BUTTON_R, PAUSE_BUTTON_DDOWN, 1.0), 0);
        assert_eq!(s.enabled, 1);
        // toggling on immediately gates (returns 0)
        // toggle back off
        assert_eq!(proc(&mut s, PAUSE_BUTTON_R, PAUSE_BUTTON_DDOWN, 1.0), 1);
        assert_eq!(s.enabled, 0);
    }

    #[test]
    fn z_plus_new_r_unpauses_immediately() {
        let mut s = PauseState { enabled: 1, timer: 0.0 };
        // Z held, R newly pressed -> return 1, timer reset
        assert_eq!(proc(&mut s, PAUSE_BUTTON_Z | PAUSE_BUTTON_R, PAUSE_BUTTON_R, 1.0), 1);
        assert_eq!(s.timer, 0.0);
    }

    #[test]
    fn zr_hold_uses_strict_timer_threshold() {
        let mut s = PauseState { enabled: 1, timer: 0.0 };
        // hold Z+R, R not newly pressed: accumulates dt, stays gated
        assert_eq!(proc(&mut s, PAUSE_BUTTON_Z | PAUSE_BUTTON_R, 0, 1.0), 0);
        assert_eq!(s.timer, 1.0);
        // exactly 8.0 does NOT unpause (strict >)
        s.timer = 8.0;
        assert_eq!(proc(&mut s, PAUSE_BUTTON_Z | PAUSE_BUTTON_R, 0, 0.0), 0);
        // 8.0 + epsilon does
        s.timer = 8.0;
        assert_eq!(proc(&mut s, PAUSE_BUTTON_Z | PAUSE_BUTTON_R, 0, 0.001), 1);
        assert_eq!(s.timer, 0.0); // reset on the return-1 path
    }

    #[test]
    fn return_zero_leaves_timer() {
        let mut s = PauseState { enabled: 1, timer: 3.0 };
        // Z held alone: no accumulation (needs R held too), no unpause
        assert_eq!(proc(&mut s, PAUSE_BUTTON_Z, 0, 1.0), 0);
        assert_eq!(s.timer, 3.0);
    }

    #[test]
    fn toggle_on_with_unpause_combo_returns_one() {
        // R held + D-down newly pressed toggles 0->1, but Z held +
        // R newly pressed in the same call still returns 1
        let mut s = PauseState::default();
        let r = proc(
            &mut s,
            PAUSE_BUTTON_R | PAUSE_BUTTON_Z | PAUSE_BUTTON_DDOWN,
            PAUSE_BUTTON_R | PAUSE_BUTTON_DDOWN,
            1.0,
        );
        assert_eq!(s.enabled, 1);
        assert_eq!(r, 1);
    }
}
