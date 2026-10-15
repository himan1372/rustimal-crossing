//! Town wind: today's power range, the 10-minute target changes, and the
//! gust/lull state machine (`m_kankyo_weather.c_inc` `mEnv_InitWind`,
//! `mEnv_ChangeWind`, `mEnv_WindMove`, `mEnv_GetWindPowerF*`,
//! `mEnv_DecideWindDirect`). The date table and range selection already
//! live in `weather_season.rs` (`wind_term`, `WIND_PERCENTS`,
//! `WIND_POWER_RANGES`); this module is the per-frame part.
//!
//! Source-verified against ac-decomp 09ca8e8b (primary) and ACGC-PC-Port
//! (secondary: timers become f32 and tick by `dt_num_60fps_frames`; logic
//! unchanged). RNG is injected: `random_f(x)` = `RANDOM_F(x)`,
//! `fqrand()` in [0,1). The draw order below matches the C exactly.
//!
//! ## Facts
//! - Init: a random direction `RANDOM_F(65536)` (any angle) and power
//!   `min + fqrand() * (max - min)`; Koinobori day forces 135 degrees
//!   (0x6000) and power 1.0. State STEADY, change timer 36000 frames.
//! - Every 36000 frames (10 minutes) in STEADY: target angle = current +
//!   `RANDOM_F(0x1555) - 0x1555/2` (a 30-degree window), target power from
//!   `fqrand()` inside today's range, both reached linearly over the next
//!   36000 frames.
//! - Gusts: when `gust_timer <= 0` in STEADY, draw `(int)(fqrand()*100) & 1`,
//!   then `rng1`, then `rng2`. FAST if the bit is 1 or today's minimum is 0,
//!   else SLOW. FAST aims at `max + 0.3 + rng1 * 0.1`; SLOW at
//!   `min - 0.3 - rng1 * 0.1`. Today's minimum is only ever 0, 0.4 or 0.6,
//!   and min 0 always gusts FAST, so lulls land in (0.0, 0.1] or (0.2, 0.3].
//!   Duration `200 + rng2 * 100` frames, approached with
//!   `add_calc(.., 0.05, 0.05, 0.0001)`.
//! - REDUCING eases back to the pre-gust power with
//!   `add_calc(.., 0.032, 0.032, 0.000005)`; when it lands exactly, the next
//!   gust is `fqrand() * 450 + 150` frames away.
//! - Power reads: `power()` clamps to 0..1; `power_windmill()` is unclamped
//!   (gusts up to 1.4 on gusty days show on windmills).
//!
//! ## Quirks
//! - On the frame a gust starts, the 10-minute change check is skipped (the
//!   `else` branch); the change timer still counts down.
//! - The weather actor calls `mEnv_DecideWindDirect(0x3000, 0x3000)` every
//!   frame and discards the result: two `RANDOM_F` draws per frame of pure
//!   RNG advance (see `burn_wind_direct_draws`).

use crate::game_math::add_calc;

pub const WIND_CHANGE_RATE: f32 = 36000.0;
pub const KOINOBORI_ANGLE: f32 = 24576.0; // DEG2SHORT_ANGLE(135) = 0x6000
/// `DEG2SHORT_ANGLE(30.0f)` = (s16)(30 * 182.04445) = 5461 = 0x1555.
pub const THIRTY_DEG: f32 = 5461.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindState {
    Steady,
    Fast,
    Slow,
    Reducing,
}

/// `mEnv_wind_info` (timers are f32 to cover the PC dt variant).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Wind {
    pub state: WindState,
    pub angle: f32,
    pub target_angle: f32,
    pub angle_step: f32,
    pub power: f32,
    pub target_power: f32,
    pub power_step: f32,
    pub range_min: f32,
    pub range_max: f32,
    pub change_timer: f32,
    pub gust_timer: f32,
    pub power_timer: f32,
    pub last_power: f32,
    pub gust_power: f32,
}

/// Injected random sources.
pub trait WindRng {
    fn random_f(&mut self, max: f32) -> f32;
    fn fqrand(&mut self) -> f32;
}

impl Wind {
    /// `mEnv_InitWind` with today's range (`mEnv_DecideTodayWindPowerRange`).
    pub fn init(range: (f32, f32), koinobori: bool, rng: &mut impl WindRng) -> Self {
        let (min, max) = range;
        let (ta, tp) = if koinobori {
            (KOINOBORI_ANGLE, 1.0)
        } else {
            let a = rng.random_f(65536.0);
            let t = rng.fqrand();
            (a, min + t * (max - min))
        };
        Self {
            state: WindState::Steady,
            angle: ta,
            target_angle: ta,
            angle_step: 0.0,
            power: tp,
            target_power: tp,
            power_step: 0.0,
            range_min: min,
            range_max: max,
            change_timer: WIND_CHANGE_RATE,
            gust_timer: 0.0,
            power_timer: 0.0,
            last_power: 0.0,
            gust_power: 0.0,
        }
    }

    /// `mEnv_ChangeWind`.
    pub fn change(&mut self, koinobori: bool, rng: &mut impl WindRng) {
        if koinobori {
            self.target_angle = KOINOBORI_ANGLE;
            self.angle = KOINOBORI_ANGLE;
            self.target_power = 1.0;
            self.power = 1.0;
            self.angle_step = 0.0;
            self.power_step = 0.0;
        } else {
            let adj = rng.random_f(THIRTY_DEG) - THIRTY_DEG / 2.0;
            let w = rng.fqrand();
            self.target_angle = self.angle + adj;
            self.angle_step = adj / WIND_CHANGE_RATE;
            self.target_power = self.range_min + w * (self.range_max - self.range_min);
            self.power_step = (self.target_power - self.power) / WIND_CHANGE_RATE;
        }
    }

    fn approach(value: &mut f32, target: f32, step: &mut f32) {
        if (target > *value && *step < 0.0) || (target < *value && *step > 0.0) {
            *step = -*step;
        }
        if target != *value {
            *value += *step;
            if (*step > 0.0 && *value >= target) || (*step < 0.0 && *value <= target) {
                *step = 0.0;
            }
        }
    }

    /// `mEnv_WindMove`. `dt` = 1.0 on retail, `dt_num_60fps_frames` on PC.
    pub fn step(&mut self, koinobori: bool, dt: f32, rng: &mut impl WindRng) {
        match self.state {
            WindState::Steady => {
                Self::approach(&mut self.angle, self.target_angle, &mut self.angle_step);
                Self::approach(&mut self.power, self.target_power, &mut self.power_step);
                if self.gust_timer <= 0.0 {
                    let bit = ((rng.fqrand() * 100.0) as i32) & 1;
                    let rng1 = rng.fqrand();
                    let rng2 = rng.fqrand();
                    self.state = if bit == 1 || self.range_min == 0.0 { WindState::Fast } else { WindState::Slow };
                    self.last_power = self.power;
                    self.power_timer = 200.0 + rng2 * 100.0;
                    self.gust_power = if self.state == WindState::Fast {
                        self.range_max + 0.3 + rng1 * ((self.range_max + 0.4) - (self.range_max + 0.3))
                    } else {
                        self.range_min - 0.3 + rng1 * ((self.range_min - 0.4) - (self.range_min - 0.3))
                    };
                } else {
                    if self.gust_timer > 0.0 {
                        self.gust_timer -= dt;
                    }
                    if self.change_timer <= 0.0 {
                        self.change(koinobori, rng);
                        self.change_timer = WIND_CHANGE_RATE;
                    }
                }
            }
            WindState::Fast | WindState::Slow => {
                add_calc(&mut self.power, self.gust_power, 0.05, 0.05, 0.0001);
                if self.power_timer <= 0.0 {
                    self.state = WindState::Reducing;
                } else {
                    self.power_timer -= dt;
                }
            }
            WindState::Reducing => {
                add_calc(&mut self.power, self.last_power, 0.032, 0.032, 0.000005);
                if self.power == self.last_power {
                    let r = rng.fqrand();
                    self.gust_timer = r * 450.0 + 150.0;
                    self.state = WindState::Steady;
                }
            }
        }
        if self.change_timer > 0.0 {
            self.change_timer -= dt;
        }
    }

    /// `mEnv_GetWindPowerF`.
    pub fn power(&self) -> f32 {
        self.power.clamp(0.0, 1.0)
    }

    /// `mEnv_GetWindPowerF_Windmill` (unclamped).
    pub fn power_windmill(&self) -> f32 {
        self.power
    }

    /// `mEnv_GetWindAngleS`: the float angle truncated to a short angle.
    pub fn angle_s(&self) -> i16 {
        self.angle as i32 as i16
    }
}

/// `mEnv_DecideTodayWindPowerRange` with `roll` = `RANDOM(100)` and the
/// (calm, normal, gusty) percents for today.
pub fn today_range(roll: i32, percents: (u8, u8, u8)) -> (f32, f32) {
    let (calm, normal, _) = (percents.0 as i32, percents.1 as i32, percents.2);
    if roll < calm {
        (0.0, 0.4)
    } else if roll < calm + normal {
        (0.4, 0.6)
    } else {
        (0.6, 1.0)
    }
}

/// `aWeather_CheckWeatherTimer` -> `mEnv_DecideWindDirect(0x3000, 0x3000)`:
/// draws `RANDOM_F(x)` then `RANDOM_F(z)` and throws the result away.
pub fn burn_wind_direct_draws(rng: &mut impl WindRng) {
    let _ = rng.random_f(12288.0);
    let _ = rng.random_f(12288.0);
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Seq {
        f: Vec<f32>,
        i: usize,
        calls: Vec<&'static str>,
    }
    impl WindRng for Seq {
        fn random_f(&mut self, max: f32) -> f32 {
            self.calls.push("random_f");
            let v = self.f[self.i % self.f.len()] * max;
            self.i += 1;
            v
        }
        fn fqrand(&mut self) -> f32 {
            self.calls.push("fqrand");
            let v = self.f[self.i % self.f.len()];
            self.i += 1;
            v
        }
    }

    #[test]
    fn init_change_and_koinobori() {
        let mut r = Seq { f: vec![0.5], i: 0, calls: vec![] };
        let w = Wind::init((0.4, 0.6), false, &mut r);
        assert_eq!((w.angle, w.power, w.change_timer), (32768.0, 0.5, 36000.0));
        assert_eq!(r.calls, ["random_f", "fqrand"]);
        let k = Wind::init((0.0, 0.4), true, &mut r);
        assert_eq!((k.angle, k.power), (24576.0, 1.0));
        assert_eq!(k.angle_s(), 0x6000);
        let mut w2 = w;
        w2.change(false, &mut Seq { f: vec![1.0], i: 0, calls: vec![] });
        assert_eq!(w2.target_angle, 32768.0 + THIRTY_DEG / 2.0);
        assert_eq!(today_range(9, (10, 80, 10)), (0.0, 0.4));
        assert_eq!(today_range(89, (10, 80, 10)), (0.4, 0.6));
        assert_eq!(today_range(90, (10, 80, 10)), (0.6, 1.0));
    }

    #[test]
    fn gusts_and_lulls() {
        let mut r = Seq { f: vec![0.0], i: 0, calls: vec![] };
        let mut w = Wind::init((0.0, 0.4), false, &mut r);
        r.calls.clear();
        w.step(false, 1.0, &mut r);
        // min == 0 always gusts FAST; three fqrand draws in order.
        assert_eq!(w.state, WindState::Fast);
        assert_eq!(r.calls, ["fqrand", "fqrand", "fqrand"]);
        assert!((w.gust_power - 0.7).abs() < 1e-6);
        let mut slow = Wind::init((0.4, 0.6), false, &mut Seq { f: vec![0.0], i: 0, calls: vec![] });
        slow.step(false, 1.0, &mut Seq { f: vec![0.0, 1.0, 0.0], i: 0, calls: vec![] });
        assert_eq!(slow.state, WindState::Slow);
        // min 0.4, rng1 = 1.0: lull floor 0.0 (never negative in practice).
        assert!(slow.gust_power.abs() < 1e-6);
        slow.power = -0.05;
        assert_eq!(slow.power(), 0.0);
        assert_eq!(slow.power_windmill(), -0.05);
        let mut z = Seq { f: vec![0.25], i: 0, calls: vec![] };
        burn_wind_direct_draws(&mut z);
        assert_eq!(z.calls.len(), 2);
    }
}

#[cfg(test)]
mod wave4_tests {
    use super::*;

    struct Seq2 {
        f: Vec<f32>,
        i: usize,
    }
    impl WindRng for Seq2 {
        fn random_f(&mut self, max: f32) -> f32 {
            let v = self.f[self.i % self.f.len()] * max;
            self.i += 1;
            v
        }
        fn fqrand(&mut self) -> f32 {
            let v = self.f[self.i % self.f.len()];
            self.i += 1;
            v
        }
    }

    #[test]
    fn change_timer_path() {
        // Steady with gust_timer > 0 and change_timer <= 0: change() runs,
        // the 10-minute timer resets (then ticks down one frame).
        let mut r = Seq2 { f: vec![0.5], i: 0 };
        let mut w = Wind::init((0.4, 0.6), false, &mut r);
        w.gust_timer = 100.0;
        w.change_timer = 0.0;
        w.step(false, 1.0, &mut r);
        assert_eq!(w.change_timer, WIND_CHANGE_RATE - 1.0);
        assert_eq!(w.gust_timer, 99.0);
        // New targets stay inside today's range and the 30-degree window.
        assert!((0.4..=0.6).contains(&w.target_power));
        assert!((w.target_angle - w.angle).abs() <= THIRTY_DEG / 2.0 + 1.0);
    }

    #[test]
    fn gust_lifecycle_fast() {
        // FAST gust: power_timer expiry -> Reducing -> converged -> Steady.
        let mut r = Seq2 { f: vec![0.0], i: 0 };
        let mut w = Wind::init((0.0, 0.4), false, &mut r);
        w.step(false, 1.0, &mut r);
        assert_eq!(w.state, WindState::Fast);
        assert!((w.power_timer - 200.0).abs() < 1e-6);
        w.power_timer = 0.0;
        w.step(false, 1.0, &mut r);
        assert_eq!(w.state, WindState::Reducing);
        w.power = w.last_power;
        w.step(false, 1.0, &mut Seq2 { f: vec![0.0], i: 0 });
        assert_eq!(w.state, WindState::Steady);
        assert!((w.gust_timer - 150.0).abs() < 1e-6); // fqrand=0 -> 0*450+150
    }

    #[test]
    fn today_range_and_angle_edges() {
        // Class boundaries: roll == calm -> normal; roll == calm+normal -> gusty.
        assert_eq!(today_range(10, (10, 80, 10)), (0.4, 0.6));
        assert_eq!(today_range(90, (10, 80, 10)), (0.6, 1.0));
        assert_eq!(today_range(0, (0, 70, 30)), (0.4, 0.6));
        assert_eq!(today_range(99, (0, 70, 30)), (0.6, 1.0));
        // angle_s truncates toward zero like the C cast.
        let mut r = Seq2 { f: vec![], i: 0 };
        let mut w = Wind::init((0.0, 0.4), true, &mut r);
        w.angle = -100.7;
        assert_eq!(w.angle_s(), -100);
        // power() clamps; the windmill read does not.
        w.power = 1.4;
        assert_eq!(w.power(), 1.0);
        assert_eq!(w.power_windmill(), 1.4);
        assert_eq!(KOINOBORI_ANGLE, 24576.0);
        assert_eq!(THIRTY_DEG, 5461.0);
    }
}
