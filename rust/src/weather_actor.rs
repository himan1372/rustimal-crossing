//! The weather controller (`ac_weather.c` + `ac_weather_{fine,rain,snow,
//! sakura,leaf}.c`) and the start-of-day weather decisions
//! (`m_kankyo_weather.c_inc` `mEnv_DecideWeather_*`, `mEnv_NowWeather`).
//! The daily probability table, roll, event override and save packing are
//! in `weather_season.rs`; this module is what happens around them.
//!
//! Source-verified against ac-decomp 09ca8e8b (primary, USA) and
//! ACGC-PC-Port (secondary: particle motion and spawn throttles scale by
//! `dt_num_60fps_frames`; logic otherwise unchanged). Drawing, sound
//! playback and collision stay in C; these are the decision kernels.
//!
//! ## Start of day
//! - First start: roll, event override, then real-work rain -> CLEAR/NONE;
//!   store packed in the save, clear the weather renew flag.
//! - Normal start with the renew flag: same roll; if the new weather is
//!   CLEAR/SAKURA and the saved (yesterday's) one was RAIN/SNOW, a rainbow
//!   is reserved for today and a gyroid is ordered. Without the flag the
//!   saved weather is restored (real-work rain still forced clear).
//! - Every normal start re-rolls the island: `fqrand() < TAKREG50 * 0.1 +
//!   0.8` (retail register 0 -> 80%) gives CLEAR/NONE, otherwise RAIN/HEAVY.
//! - The same roll also runs live: when the renew flag trips while playing
//!   (`aWeather_ChangeWeatherTime0`), outside the start demos, the new
//!   weather is saved and requested as a transition (not on the island).
//!
//! ## Controller
//! - Title demos 1-5 force (SAKURA, LIGHT), (RAIN, NORMAL), (CLEAR, NONE)
//!   x2, (SNOW, LIGHT); on the island the island weather; else the save.
//!   USA starts the aim level from common `weather_intensity` (AUS: save).
//! - Snow and cherry blossoms pre-simulate 40 frames on construction (with
//!   the controller 50 units lower) so the sky isn't empty.
//! - A change request (accepted only while lighting isn't mid-transition)
//!   sets aim level 0; the level steps one every 180 frames; once it's 0,
//!   outdoors waits for every particle to expire before switching type,
//!   then restarts at level 1 and climbs to the new aim. Indoors switches
//!   immediately.
//! - LEAVES (K.K. Slider's concert only) is stored in common as CLEAR.
//! - Pool: 100 slots; allocation takes the lowest free slot; a slot whose
//!   timer isn't -100 counts down and frees at <= 0.
//!
//! ## Particles (per call of `make`, only when level != 0)
//! - Rain: 1/2/3 drops for LIGHT/NORMAL/HEAVY; x in [-130,130), z in
//!   [-200,160) around the camera, 190 above ground, fall speed
//!   -11.5 + RANDOM_F(-2.5) per frame, life 1000. After **10 frames** the
//!   drop is freed and a splash (life 8) is made at ground height below it:
//!   the splash is time-based, not a ground hit.
//! - Snow: one flake on frames where `frame_counter & 7 == 0` (LIGHT),
//!   `& 3 == 0` (NORMAL), else odd frames; life 280; the fall speed draw
//!   happens before the free-slot check (consumes RNG even when full).
//! - Cherry blossoms: `& 7` at LIGHT, else `& 3`; leaves: `& 15` at LIGHT,
//!   else `& 7`. Both drift +0.45 x per frame, life 280.
//! - Flakes/petals wrap in a 200 x 380 box around the camera.
//! - Thunder: only June-August in RAIN at HEAVY. With `t = timer % 1000`,
//!   a flash at `t == t2` and `t == t2 + 20`, thunder at `t2 + 65`, then
//!   `t2 = 100 + RANDOM_F(500)` (initial t2 = 30).
//! - Rain ambience: sys-lev 7/8/9 by level, or 0x12/0x13/0x14 with an
//!   umbrella open; it is re-evaluated when the umbrella state flips.
//!
//! ## Corrections to the brief
//! - Rain splashes are timed (10 frames), not triggered near the ground.
//! - Every frame the controller burns two RNG draws (`wind::
//!   burn_wind_direct_draws`), which matters for RNG reproducibility.

use crate::weather_season::intensity as wi;

/// `mEnv_WEATHER_*` incl. LEAVES (4), which `weather_season.rs` omits.
pub mod wt {
    pub use crate::weather_season::weather::*;
    pub const LEAVES: u8 = 4;
}

pub const PRIV_NUM: usize = 100;
pub const PRIV_TIMER_FOREVER: i16 = -100;
pub const LEVEL_STEP_FRAMES: i16 = 180;
pub const SNOW_PRESIM_FRAMES: u32 = 40;

/// `mEnv_NowWeather` / `DemoWeatherTbl`: title demo 1..=5.
pub const DEMO_WEATHER: [(u8, u8); 5] =
    [(wt::SAKURA, wi::LIGHT), (wt::RAIN, wi::NORMAL), (wt::CLEAR, wi::NONE), (wt::CLEAR, wi::NONE), (wt::SNOW, wi::LIGHT)];

/// Real-work (`mEv_CheckRealArbeit`) rain override.
pub fn real_work_override(w: (u8, u8), real_work: bool) -> (u8, u8) {
    if real_work && w.0 == wt::RAIN {
        (wt::CLEAR, wi::NONE)
    } else {
        w
    }
}

/// `mEnv_DecideWeather_GameStart` after the roll: event override, then the
/// real-work fix.
pub fn decide_game_start(rolled: (u8, u8), event: Option<(u8, u8)>, real_work: bool) -> (u8, u8) {
    real_work_override(event.unwrap_or(rolled), real_work)
}

/// `mEnv_PreRainNowFine_Init` trigger: rain/snow yesterday, clear/sakura now.
pub fn rainbow_after(prev_saved_type: u8, new_type: u8) -> bool {
    (new_type == wt::CLEAR || new_type == wt::SAKURA) && (prev_saved_type == wt::SNOW || prev_saved_type == wt::RAIN)
}

/// Island weather for a normal start (`takreg50` = debug register, 0 on
/// retail).
pub fn island_weather(fqrand: f32, takreg50: i16) -> (u8, u8) {
    if fqrand < takreg50 as f32 * 0.1 + 0.8 {
        (wt::CLEAR, wi::NONE)
    } else {
        (wt::RAIN, wi::HEAVY)
    }
}

/// `aWeather_weatherinfo_CommonSet`: LEAVES and above are stored as CLEAR.
pub fn common_weather_type(t: u8) -> u8 {
    if t >= wt::LEAVES {
        wt::CLEAR
    } else {
        t
    }
}

/// One `aWeather_Priv` slot (the bookkeeping part).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Priv {
    pub used: bool,
    pub status: u8,
    pub timer: i16,
    pub kind: u8,
}

/// Particle pool plus the level/transition state of `WEATHER_ACTOR`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WeatherCtl {
    pub privs: [Priv; PRIV_NUM],
    pub current_status: u8,
    pub next_status: u8,
    pub current_level: i16,
    pub aim_level: i16,
    pub next_level: i16,
    pub counter: i16,
    pub request_change: bool,
}

impl WeatherCtl {
    pub fn new(status: u8, level: i16, aim: i16) -> Self {
        Self {
            privs: [Priv::default(); PRIV_NUM],
            current_status: status,
            next_status: status,
            current_level: level,
            aim_level: aim,
            next_level: 0,
            counter: 0,
            request_change: false,
        }
    }

    /// `aWeather_GetWeatherPrvNum`: lowest free slot.
    pub fn free_slot(&self) -> Option<usize> {
        self.privs.iter().position(|p| !p.used)
    }

    /// `aWeather_GetWeatherPrv`.
    pub fn alloc(&mut self, id: usize, status: u8, timer: i16) -> bool {
        match self.privs.get_mut(id) {
            Some(p) if !p.used => {
                *p = Priv { used: true, status, timer, kind: 0 };
                true
            }
            _ => false,
        }
    }

    /// `aWeather_CountWeatherPrivate`.
    pub fn live(&self) -> usize {
        self.privs.iter().filter(|p| p.used).count()
    }

    /// Timer half of `aWeather_MoveWeatherPrv` (after the profile move).
    pub fn tick_timers(&mut self) {
        for p in self.privs.iter_mut().filter(|p| p.used) {
            if p.timer != PRIV_TIMER_FOREVER {
                p.timer -= 1;
                if p.timer <= 0 {
                    p.used = false;
                }
            }
        }
    }

    /// `aWeather_RequestChangeWeather`; `env_ok` = the lighting accepted
    /// (`mEnv_ReqeustChangeWeatherEnviroment`).
    pub fn request(&mut self, status: u8, level: i16, env_ok: bool) {
        if !env_ok {
            return;
        }
        if status != self.current_status {
            self.next_status = status;
            self.next_level = level;
            self.request_change = true;
            self.aim_level = 0;
        } else {
            self.aim_level = level;
        }
    }

    /// `aWeather_RenewWeatherLevel`: true when the level stepped (the
    /// ambience is re-evaluated then).
    pub fn renew_level(&mut self) -> bool {
        if self.current_level != self.aim_level {
            self.counter += 1;
            if self.counter >= LEVEL_STEP_FRAMES {
                self.counter = 0;
                if self.aim_level < self.current_level {
                    self.current_level -= 1;
                } else {
                    self.current_level += 1;
                }
                return true;
            }
        }
        false
    }

    /// `aWeather_ChangeWeather`. Returns the new common weather type when
    /// the switch happened.
    pub fn try_switch(&mut self, outdoors: bool) -> Option<u8> {
        if self.request_change && self.current_level == 0 && (!outdoors || self.live() == 0) {
            self.current_status = self.next_status;
            self.current_level = 1;
            self.aim_level = self.next_level;
            self.request_change = false;
            return Some(common_weather_type(self.current_status));
        }
        None
    }
}

/// `aWeatherRain_DecideMakeRainCount`.
pub fn rain_drops(level: i16) -> u32 {
    match level {
        1 => 1,
        2 => 2,
        _ => 3,
    }
}

/// Spawn throttles by frame counter (retail; PC uses a dt accumulator).
pub fn snow_spawns(level: i16, frame: u32) -> bool {
    match level {
        1 => frame & 7 == 0,
        2 => frame & 3 == 0,
        _ => frame & 1 != 0,
    }
}
pub fn sakura_spawns(level: i16, frame: u32) -> bool {
    if level == 1 {
        frame & 7 == 0
    } else {
        frame & 3 == 0
    }
}
pub fn leaf_spawns(level: i16, frame: u32) -> bool {
    if level == 1 {
        frame & 15 == 0
    } else {
        frame & 7 == 0
    }
}

/// Rain drop spawn (offsets and speed from three `RANDOM_F` draws in order:
/// x, z, speed).
pub fn rain_drop(random_f: &mut impl FnMut(f32) -> f32) -> (f32, f32, f32) {
    let x = -130.0 + random_f(260.0);
    let z = -200.0 + random_f(360.0);
    let vy = -9.5 + random_f(-2.5) - 2.0;
    (x, z, vy)
}
pub const RAIN_LIFE: i16 = 1000;
pub const RAIN_SPLASH_AFTER: i16 = 10;
pub const SPLASH_LIFE: i16 = 8;
pub const RAIN_SPAWN_HEIGHT: f32 = 190.0;
pub const FLAKE_LIFE: i16 = 280;

/// Thunder schedule (`aWeather_MakeKaminari`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Thunder {
    pub timer: i16,
    pub t2: i16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThunderEvent {
    None,
    Flash,
    Boom,
}

impl Default for Thunder {
    fn default() -> Self {
        Self { timer: 0, t2: 30 }
    }
}

impl Thunder {
    /// One frame. `active` = June-August, RAIN, HEAVY (and not in the
    /// basement or start demos); `flash_ok` = the scene allows the flash.
    pub fn step(&mut self, month: u8, status: u8, level: i16, flash_ok: bool, random_f: impl FnOnce(f32) -> f32) -> ThunderEvent {
        if !((6..=8).contains(&month) && status == wt::RAIN && level == 3) {
            return ThunderEvent::None;
        }
        let t = self.timer % 1000;
        let mut ev = ThunderEvent::None;
        if (t == self.t2 || t == self.t2 + 20) && flash_ok {
            ev = ThunderEvent::Flash;
        }
        if t == self.t2 + 65 {
            ev = ThunderEvent::Boom;
            self.t2 = (100.0 + random_f(500.0)) as i16;
        }
        self.timer = self.timer.wrapping_add(1);
        ev
    }
}

/// Rain ambience id (`aWeather_ChangeEnvSE`).
pub fn rain_ambience(level: i16, umbrella: bool) -> Option<u8> {
    let base = match level {
        1 => 7,
        2 => 8,
        3 => 9,
        _ => return None,
    };
    Some(if umbrella { base + 11 } else { base })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_decisions() {
        assert_eq!(decide_game_start((wt::RAIN, wi::HEAVY), None, true), (wt::CLEAR, wi::NONE));
        assert_eq!(decide_game_start((wt::RAIN, wi::LIGHT), Some((wt::SNOW, wi::LIGHT)), true), (wt::SNOW, wi::LIGHT));
        assert!(rainbow_after(wt::RAIN, wt::SAKURA) && !rainbow_after(wt::CLEAR, wt::CLEAR));
        assert_eq!(island_weather(0.79, 0), (wt::CLEAR, wi::NONE));
        assert_eq!(island_weather(0.8, 0), (wt::RAIN, wi::HEAVY));
        assert_eq!(common_weather_type(wt::LEAVES), wt::CLEAR);
        assert_eq!(DEMO_WEATHER[4], (wt::SNOW, wi::LIGHT));
    }

    #[test]
    fn transitions_and_pool() {
        let mut c = WeatherCtl::new(wt::RAIN, 2, 2);
        assert_eq!(c.free_slot(), Some(0));
        assert!(c.alloc(0, 1, 2) && !c.alloc(0, 1, 2));
        c.request(wt::CLEAR, 0, true);
        assert_eq!((c.aim_level, c.request_change), (0, true));
        for _ in 0..360 {
            c.renew_level();
        }
        assert_eq!(c.current_level, 0);
        assert_eq!(c.try_switch(true), None); // a drop is still alive
        c.tick_timers();
        c.tick_timers();
        assert_eq!(c.live(), 0);
        assert_eq!(c.try_switch(true), Some(wt::CLEAR));
        assert_eq!((c.current_level, c.request_change), (1, false));
        let mut forever = WeatherCtl::new(0, 0, 0);
        forever.alloc(3, 1, PRIV_TIMER_FOREVER);
        forever.tick_timers();
        assert_eq!(forever.live(), 1);
        c.request(wt::RAIN, 3, false);
        assert!(!c.request_change);
    }

    #[test]
    fn particles_thunder_sound() {
        assert_eq!([1, 2, 3].map(rain_drops), [1, 2, 3]);
        assert!(snow_spawns(1, 8) && !snow_spawns(1, 4) && snow_spawns(2, 4) && snow_spawns(3, 5) && !snow_spawns(3, 4));
        assert!(leaf_spawns(1, 16) && !leaf_spawns(1, 8) && sakura_spawns(2, 4));
        let mut q = vec![0.5f32, 0.5, 1.0].into_iter();
        let (x, z, vy) = rain_drop(&mut |m| q.next().unwrap() * m);
        assert_eq!((x, z, vy), (0.0, -20.0, -14.0));
        let mut th = Thunder::default();
        let mut evs = vec![];
        for _ in 0..100 {
            evs.push(th.step(7, wt::RAIN, 3, true, |_| 0.0));
        }
        assert_eq!(evs[30], ThunderEvent::Flash);
        assert_eq!(evs[50], ThunderEvent::Flash);
        assert_eq!(evs[95], ThunderEvent::Boom);
        assert_eq!(th.t2, 100);
        assert_eq!(Thunder::default().step(5, wt::RAIN, 3, true, |_| 0.0), ThunderEvent::None);
        assert_eq!(rain_ambience(2, true), Some(0x13));
        assert_eq!(rain_ambience(1, false), Some(7));
    }
}
