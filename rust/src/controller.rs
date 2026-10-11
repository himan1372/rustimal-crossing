//! Controller stick processing (`m_controller.c`) and rumble programs
//! (`m_vibctl.c`).
//!
//! Source-verified against ac-decomp `src/game/m_controller.c`,
//! `src/game/m_vibctl.c`, `include/m_controller.h`, `include/m_vibctl.h`
//! (primary) and ACGC-PC-Port (secondary; both files byte-identical).
//!
//! ## Stick (`mCon_calc`)
//! Only pad 0 is read. Raw stick magnitude `t = |(x, y)|`:
//! - `t <= STICK_MIN` (9.899495 = 7*sqrt(2)): everything zero, the angle
//!   keeps its previous value.
//! - `t > STICK_MAX` (61): the vector is clamped to radius 61 along
//!   `atans_table(x, y)`.
//! - `move_p*` = `x / 61` (no dead-zone shift), `adjusted_p*` =
//!   `(x -+ min) / (61 - min)` (dead zone removed), both saturating at +-1.
//!
//! `chkButton`/`getButton`/`chkTrigger`/`getTrigger`/`getJoystick_*` all
//! return "nothing pressed" during the title demo.
//!
//! ## Rumble (`mVibctl`)
//! Up to 4 concurrent elements, each an attack/sustain/release envelope
//! over one of 16 on/off programs (`mVW_data`). Every pad-manager callback:
//! each element advances one frame (`entry_frame`), computes
//! `frame_intensity = step1 * step0` scaled by the envelope ramp, and for
//! RUMBLE steps accumulates intensity, firing the motor when it crosses 1.0
//! (a PWM-like duty cycle). STOP / STOP_HARD steps pass through. Finished
//! elements are removed (shifting the rest down), then the strongest
//! element's command drives the motor unless a force-stop flag is set
//! (bit 0: options menu "vibration off", bit 2: title demo in a field,
//! bit 3: soft reset).
//! - `step1 = total_frames * 0.01` (a strength percentage),
//!   `step0 = 1` for `step < 41`, else `1 / (step - 40)` (falloff); entries
//!   with `step >= 640` are rejected.

use crate::game_math::{atans_table, check_percent_abs, cos_s, sin_s};
use std::ffi::c_int;

pub const STICK_MIN: f32 = 9.899495;
pub const STICK_MAX: f32 = 61.0;
pub const STICK_UNCORRECTED_SCALE: f32 = 1.0 / STICK_MAX;
pub const STICK_CORRECTED_SCALE: f32 = 1.0 / (STICK_MAX - STICK_MIN);

/// `MCON` (`sizeof == 0x38`).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Mcon {
    pub move_px: f32,
    pub move_py: f32,
    pub move_pr: f32,
    pub move_angle: i16,
    pub last_move_px: f32,
    pub last_move_py: f32,
    pub last_move_pr: f32,
    pub last_move_angle: i16,
    pub adjusted_px: f32,
    pub adjusted_py: f32,
    pub adjusted_pr: f32,
    pub last_adjusted_px: f32,
    pub last_adjusted_py: f32,
    pub last_adjusted_pr: f32,
}

/// `mCon_calc`.
pub fn mcon_calc(m: &mut Mcon, mut stick_x: f32, mut stick_y: f32) {
    m.last_move_px = m.move_px;
    m.last_move_py = m.move_py;
    m.last_move_pr = m.move_pr;
    m.last_move_angle = m.move_angle;
    m.last_adjusted_px = m.adjusted_px;
    m.last_adjusted_py = m.adjusted_py;
    m.last_adjusted_pr = m.adjusted_pr;

    let mut t = (stick_x * stick_x + stick_y * stick_y).sqrt();
    if t <= STICK_MIN {
        m.move_px = 0.0;
        m.move_py = 0.0;
        m.move_pr = 0.0;
        m.adjusted_px = 0.0;
        m.adjusted_py = 0.0;
        m.adjusted_pr = 0.0;
        return;
    }
    let angle = atans_table(stick_x, stick_y);
    if t > STICK_MAX {
        stick_x = cos_s(angle) * STICK_MAX;
        stick_y = sin_s(angle) * STICK_MAX;
        t = STICK_MAX;
    }
    m.move_angle = angle;
    m.move_px = check_percent_abs(stick_x, STICK_MIN, STICK_MAX, STICK_UNCORRECTED_SCALE, false);
    m.move_py = check_percent_abs(stick_y, STICK_MIN, STICK_MAX, STICK_UNCORRECTED_SCALE, false);
    m.move_pr = check_percent_abs(t, STICK_MIN, STICK_MAX, STICK_UNCORRECTED_SCALE, false);
    m.adjusted_px = check_percent_abs(stick_x, STICK_MIN, STICK_MAX, STICK_CORRECTED_SCALE, true);
    m.adjusted_py = check_percent_abs(stick_y, STICK_MIN, STICK_MAX, STICK_CORRECTED_SCALE, true);
    m.adjusted_pr = check_percent_abs(t, STICK_MIN, STICK_MAX, STICK_CORRECTED_SCALE, true);
}

/// `chkButton` / `chkTrigger`: all bits of `mask` held (FALSE in title demo).
pub fn chk_buttons(mask: u16, buttons: u16, title_demo: bool) -> bool {
    !title_demo && (mask & buttons) == mask
}

// ---------------------------------------------------------------------------
// Rumble
// ---------------------------------------------------------------------------

/// `PAD_MOTOR_*`.
pub const MOTOR_STOP: u8 = 0;
pub const MOTOR_RUMBLE: u8 = 1;
pub const MOTOR_STOP_HARD: u8 = 2;

const S: u8 = MOTOR_STOP;
const R: u8 = MOTOR_RUMBLE;
const H: u8 = MOTOR_STOP_HARD;

/// `mVibctl_VIB_PROG_*` programs (`mVW_data`), in enum order:
/// NON, FFF, F, MF, MP, P, FUNBARI, ANAHORI (dig), ANAUME (fill hole),
/// IMPACT, KI_GA_TAORERU (tree falls), KI_WO_YUSURU (shake tree),
/// KORONDA (trip), SURPRISE, DUMMY_B, SAMPLE.
pub const VIB_PROGRAMS: [&[u8]; 16] = [
    &[S],
    &[R],
    &[R, S],
    &[R, S, S],
    &[R, S, S, S],
    &[R, S, S, S, S],
    &[R, R, R, R, R, H, H, H],
    &[R, R, R, S, S, S, S, S],
    &[
        R, R, R, R, R, R, S, S, S, S, R, H, S, S, S, S, R, H, S, S, S, S, R, H, S, S, S, S, R, H, S, S, S, S, R, H,
        S, S, S, S, R, H, S, S, S, S, R, H, S, S, S, S, R, H, S, R, R, R, R, H,
    ],
    &[R, R, R, R, R, R, H, H, H],
    &[
        R, R, R, R, R, R, H, H, H, S, S, S, S, S, S, S, S, S, S, S, R, S, S, S, S, R, S, S, S, S, R, S, S, S, S, R,
    ],
    &[R, R, R, R, R, H, H, H, S, S, S, S, S],
    &[R, R, R, R, R, R, H, H, H, S, S, R, S, S],
    &[R, R, R, R, H, H, S],
    &[R],
    &[R, R, R, R, R, H, H, H, S, S, S, S, S],
];

pub const ELEM_NUM: usize = 4;
pub const ENTRY_ATTACK: usize = 0;
pub const ENTRY_SUSTAIN: usize = 1;
pub const ENTRY_RELEASE: usize = 2;
pub const ENTRY_END: usize = 3;

pub const FORCE_STOP0: i32 = 1;
pub const FORCE_STOP1: i32 = 2;
pub const FORCE_STOP2: i32 = 4;
pub const FORCE_STOP3: i32 = 8;
pub const FORCE_STOP_ALL: i32 = 15;

/// `mVibInfo_elem_entry_c`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct VibEntry {
    pub kind: usize,
    pub frames: i32,
    pub step: f32,
}

/// `mVibElem_c`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct VibElem {
    pub entries: [VibEntry; 3],
    pub step0: f32,
    pub step1: f32,
    pub now_entry: usize,
    pub state_idx: usize,
    pub frame_intensity: f32,
    pub entry_frame: i32,
    pub now_intensity: f32,
    pub command: u8,
}

impl VibElem {
    /// `mVibElem_move`.
    fn step(&mut self) {
        let entry = self.entries[self.now_entry];
        let prog = VIB_PROGRAMS[entry.kind.min(15)];
        self.entry_frame += 1;
        self.frame_intensity = self.step1 * self.step0;
        if self.now_entry == ENTRY_ATTACK {
            self.frame_intensity *= self.entry_frame as f32 * entry.step;
        } else if self.now_entry == ENTRY_RELEASE {
            self.frame_intensity *= entry.step * (entry.frames - self.entry_frame) as f32;
        }
        let cmd = prog[self.state_idx];
        if cmd == MOTOR_RUMBLE {
            self.now_intensity += self.frame_intensity;
            if self.now_intensity >= 1.0 {
                self.command = MOTOR_RUMBLE;
                self.now_intensity -= 1.0;
            } else {
                self.command = MOTOR_STOP;
            }
        } else {
            self.command = cmd;
        }
        self.state_idx += 1;
        if self.state_idx >= prog.len() {
            self.state_idx = 0;
        }
        if self.entry_frame >= entry.frames {
            self.now_entry += 1;
            self.entry_frame = 0;
            self.state_idx = 0;
        }
    }
}

/// Side effects a rumble tick asks the pad manager for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VibTick {
    /// `Some(true)` = `padmgr_force_stop_ON`, `Some(false)` = `_OFF`.
    pub force_stop_change: Option<bool>,
    /// `padmgr_RumbleSet(PAD0, cmd)` unless force-stopped.
    pub motor: Option<u8>,
}

/// `mVibInfo_c`.
#[derive(Clone, Copy, Debug, Default)]
pub struct VibInfo {
    pub elements: [VibElem; ELEM_NUM],
    pub num_elements: usize,
    pub target: Option<usize>,
    pub force_stop: i32,
    pub last_force_stop: i32,
}

impl VibInfo {
    /// `mVibInfo_elem_entry` (`mVibctl_entry`).
    #[allow(clippy::too_many_arguments)]
    pub fn entry(
        &mut self,
        total_frames: i32,
        attack_type: usize,
        sustain_type: usize,
        release_type: usize,
        attack_frames: i32,
        sustain_frames: i32,
        release_frames: i32,
        step: f32,
    ) {
        if !((attack_frames != 0 || sustain_frames != 0 || release_frames != 0)
            && step < 640.0
            && self.num_elements < ELEM_NUM)
        {
            return;
        }
        let ramp = |frames: i32| if frames > 0 { 1.0 / frames as f32 } else { -1.0 };
        let mut e = VibElem {
            entries: [
                VibEntry { kind: attack_type, frames: attack_frames, step: ramp(attack_frames) },
                VibEntry { kind: sustain_type, frames: sustain_frames, step: ramp(sustain_frames) },
                VibEntry { kind: release_type, frames: release_frames, step: ramp(release_frames) },
            ],
            ..VibElem::default()
        };
        if attack_frames == 0 {
            e.now_entry = if sustain_frames == 0 { ENTRY_RELEASE } else { ENTRY_SUSTAIN };
        }
        e.step1 = total_frames as f32 * 0.01;
        e.step0 = if step < 41.0 { 1.0 } else { 1.0 / (step - 40.0) };
        self.elements[self.num_elements] = e;
        self.num_elements += 1;
    }

    fn delete(&mut self, idx: usize) {
        for i in idx..self.num_elements - 1 {
            self.elements[i] = self.elements[i + 1];
        }
        self.elements[self.num_elements - 1] = VibElem::default();
        self.num_elements -= 1;
    }

    /// `mVibInfo_move` (one pad-manager callback). `vibration_disabled` is
    /// `Save_Get(config.vibration_disabled)`.
    pub fn tick(&mut self, vibration_disabled: bool) -> VibTick {
        if vibration_disabled {
            self.force_stop |= FORCE_STOP0;
        } else {
            self.force_stop &= !FORCE_STOP0;
        }
        let force_stop_change = if self.force_stop != 0 && self.last_force_stop == 0 {
            Some(true)
        } else if self.force_stop == 0 && self.last_force_stop != 0 {
            Some(false)
        } else {
            None
        };
        self.last_force_stop = self.force_stop;

        for i in 0..self.num_elements {
            self.elements[i].step();
        }
        for i in (0..self.num_elements).rev() {
            if self.elements[i].now_entry >= ENTRY_END {
                self.delete(i);
            }
        }
        let mut max = -10000.0f32;
        self.target = None;
        for i in 0..self.num_elements {
            if self.elements[i].frame_intensity > max {
                max = self.elements[i].frame_intensity;
                self.target = Some(i);
            }
        }
        let motor = (self.force_stop == 0).then(|| self.target.map_or(MOTOR_STOP, |t| self.elements[t].command));
        VibTick { force_stop_change, motor }
    }
}

// ---------------------------------------------------------------------------
// C ABI
// ---------------------------------------------------------------------------

/// `mCon_calc` on a C `MCON`.
#[no_mangle]
pub extern "C" fn pc_mcon_calc(m: *mut Mcon, stick_x: f32, stick_y: f32) {
    if let Some(m) = unsafe { m.as_mut() } {
        mcon_calc(m, stick_x, stick_y);
    }
}

/// Length of a rumble program (0 for an invalid id).
#[no_mangle]
pub extern "C" fn pc_vibctl_program_len(prog: c_int) -> c_int {
    usize::try_from(prog).ok().and_then(|p| VIB_PROGRAMS.get(p)).map_or(0, |p| p.len() as c_int)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn program_lengths_match_c_arrays() {
        let lens: Vec<usize> = VIB_PROGRAMS.iter().map(|p| p.len()).collect();
        assert_eq!(lens, vec![1, 1, 2, 3, 4, 5, 8, 8, 60, 9, 36, 13, 14, 7, 1, 13]);
    }

    #[test]
    fn stick_dead_zone_and_clamp() {
        let mut m = Mcon::default();
        mcon_calc(&mut m, 5.0, 5.0);
        assert_eq!(m.move_pr, 0.0);
        mcon_calc(&mut m, 100.0, 0.0);
        assert_eq!(m.move_px, 1.0);
        assert_eq!(m.adjusted_pr, 1.0);
        assert_eq!(m.move_angle, 0);
        mcon_calc(&mut m, 0.0, 30.0);
        assert_eq!(m.move_angle, 0x4000);
        assert!((m.move_py - 30.0 / 61.0).abs() < 1e-6);
    }

    #[test]
    fn rumble_envelope_runs_and_ends() {
        let mut v = VibInfo::default();
        v.entry(100, 1, 1, 1, 2, 3, 2, 0.0);
        let mut frames = 0;
        while v.num_elements > 0 && frames < 100 {
            let t = v.tick(false);
            assert!(t.motor.is_some());
            frames += 1;
        }
        assert_eq!(frames, 7);
        // Rejected: no frames, or step too large.
        v.entry(100, 1, 1, 1, 0, 0, 0, 0.0);
        v.entry(100, 1, 1, 1, 1, 1, 1, 640.0);
        assert_eq!(v.num_elements, 0);
        let t = v.tick(true);
        assert_eq!(t.force_stop_change, Some(true));
        assert_eq!(t.motor, None);
    }
}
