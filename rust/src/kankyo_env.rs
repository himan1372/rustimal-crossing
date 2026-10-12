//! Environment-lighting decisions from `m_kankyo.c`: the 8 time-of-day
//! light periods and blend factor, which colour table each weather uses,
//! the weather lighting cross-fade, the rainbow, and the indoor window-light
//! target. The authored `BaseLight` colour tables, `mEnv_CalcSetLight`,
//! fog and point lights stay in C (graphics state).
//!
//! Source-verified against ac-decomp 09ca8e8b `src/game/m_kankyo.c` +
//! `include/m_kankyo.h` (primary) and ACGC-PC-Port (secondary: rainbow
//! fades and the light-switch animation scale by dt; a PC_ENHANCEMENTS fog
//! reset for the C-stick camera).
//!
//! ## Facts
//! - Light periods start at 0, 4, 6, 8, 12, 16, 18, 20 h (end 24 h).
//!   `mEnv_GetNowTerm` returns the first `i` with `start[i+1] >= now`, so an
//!   exact boundary second still belongs to the earlier period. The blend
//!   toward the next period is `get_percent(start[i+1], start[i], now)`.
//! - Colour tables by weather: CLEAR and SAKURA use "fine", RAIN and SNOW
//!   use "rain" (separate insect-museum variants). Weather ids are masked
//!   with `& 3`, so LEAVES (4) uses the fine tables.
//! - Lighting cross-fade: `rate += 1/600` per frame (AUS 1/250) to 1.0;
//!   the switch only completes once the weather actor finishes changing.
//!   A weather request is only accepted while no cross-fade is running.
//! - Rainbow: reserved by a rain/snow -> fine day change. Shown only on that
//!   date between 09:00 and 15:00, and only in **summer**; fades in over
//!   1800 frames (30 s), out over 108000 (30 min); the reservation is
//!   dropped on any other date. A visible rainbow vanishes after 15:30.
//! - Window light indoors: target 0 near midnight and noon, otherwise
//!   0.22 + 0.78 * (1 - lamp%) in rooms with a light switch, otherwise 1.0
//!   between 05:00 and 18:00 (villager lights off), else 0.
//!
//! ## Quirk
//! - The "near midnight/noon" test casts seconds to **s16**, so it also
//!   matches 05:45:45-05:49:43 and 18:10:17-18:14:15 (gcc-verified over
//!   all 86400 seconds): window light briefly drops at those times too.

pub const TERM_NUM: usize = 8;
const H: i32 = 3600;
pub const LIGHT_TERM_START: [i32; TERM_NUM + 1] = [0, 4 * H, 6 * H, 8 * H, 12 * H, 16 * H, 18 * H, 20 * H, 24 * H];
pub const RAINBOW_START: i32 = 9 * H;
pub const RAINBOW_END: i32 = 15 * H;
pub const RAINBOW_BYE: i32 = 15 * H + 30 * 60;
pub const NPC_LIGHTS_OFF: i32 = 5 * H;
pub const NPC_LIGHTS_ON: i32 = 18 * H;
pub const SECONDS_IN_DAY: i32 = 86400;
pub const SECONDS_IN_HALFDAY: i32 = 43200;
pub const CHANGE_WEATHER_ENV_RATE_USA: f32 = 1.0 / 600.0;
pub const CHANGE_WEATHER_ENV_RATE_AUS: f32 = 1.0 / 250.0;
pub const RAINBOW_FADE_IN: f32 = 1.0 / 1800.0;
pub const RAINBOW_FADE_OUT: f32 = 1.0 / 108000.0;

/// `mEnv_GetNowTerm`.
pub fn light_term(now_sec: i32) -> usize {
    (0..TERM_NUM).find(|&i| LIGHT_TERM_START[i + 1] >= now_sec).unwrap_or(0)
}

/// `get_percent(max, min, x)` (m_lib.c).
pub fn get_percent(max: i32, min: i32, x: i32) -> f32 {
    if x < min {
        0.0
    } else if x < max {
        let d = (max - min) as f32;
        if d != 0.0 {
            ((x - min) as f32 / d).min(1.0)
        } else {
            1.0
        }
    } else {
        1.0
    }
}

/// (term, next term, blend) for the outdoor colour lerp.
pub fn light_blend(now_sec: i32) -> (usize, usize, f32) {
    let t = light_term(now_sec);
    (t, (t + 1) % TERM_NUM, get_percent(LIGHT_TERM_START[t + 1], LIGHT_TERM_START[t], now_sec))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LightTable {
    Fine,
    Rain,
}

/// `l_mEnv_normal_kcolor_data[weather & 3]`.
pub fn light_table(weather: u8) -> LightTable {
    match weather & 3 {
        1 | 2 => LightTable::Rain,
        _ => LightTable::Fine,
    }
}

/// `l_mEnv_electric_light` weather cross-fade state.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EnvFade {
    pub now: u8,
    pub next: u8,
    pub rate: f32,
}

impl EnvFade {
    /// `mEnv_ReqeustChangeWeatherEnviroment`.
    pub fn request(&mut self, now: u8, next: u8) -> bool {
        if self.now != self.next {
            return false;
        }
        if light_table(now) != light_table(next) {
            self.rate = 0.0;
            self.now = now & 3;
            self.next = next & 3;
        }
        true
    }

    /// `mEnv_ManageChangeWeatherEnvRate`.
    pub fn step(&mut self, step: f32, actor_changing: bool, common_weather: u8) {
        if self.now != self.next {
            self.rate += step;
            if self.rate >= 1.0 {
                self.rate = 1.0;
                if !actor_changing {
                    self.now = self.next;
                    self.rate = 0.0;
                }
            }
        } else {
            self.rate = 0.0;
            if !actor_changing {
                self.now = common_weather;
                self.next = common_weather;
            }
        }
    }
}

/// Rainbow inputs for one frame.
#[derive(Clone, Copy, Debug)]
pub struct RainbowDay {
    pub reserved: bool,
    pub is_reserved_date: bool,
    pub now_sec: i32,
    pub summer: bool,
    pub outdoors: bool,
}

/// `mEnv_rainbow_power_calc`: (new opacity, reservation still held).
pub fn rainbow_step(opacity: f32, d: RainbowDay, dt: f32) -> (f32, bool) {
    let showing = d.reserved && d.outdoors && d.is_reserved_date && (RAINBOW_START..RAINBOW_END).contains(&d.now_sec) && d.summer;
    if showing {
        let o = (opacity + RAINBOW_FADE_IN * dt).min(1.0);
        (o, o < 1.0)
    } else {
        ((opacity - RAINBOW_FADE_OUT * dt).max(0.0), d.reserved)
    }
}

/// `mEnv_MakeWindowLightAlpha` target. `lamp` = Some(point-light %) in a
/// room with a light switch.
pub fn window_light_target(now_sec: i32, lamp: Option<f32>) -> f32 {
    let near = |v: i32| (v as i16 as i32).abs() < 120;
    if near(now_sec) || near(now_sec - SECONDS_IN_DAY) || near(now_sec - SECONDS_IN_HALFDAY) {
        0.0
    } else if let Some(p) = lamp {
        (1.0 - p) * 0.78 + 0.22
    } else if (NPC_LIGHTS_OFF..NPC_LIGHTS_ON).contains(&now_sec) {
        1.0
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terms() {
        assert_eq!(light_term(0), 0);
        assert_eq!(light_term(4 * H), 0);
        assert_eq!(light_term(4 * H + 1), 1);
        assert_eq!(light_term(23 * H), 7);
        let (t, n, p) = light_blend(10 * H);
        assert_eq!((t, n, p), (3, 4, 0.5));
        assert_eq!(light_blend(22 * H).1, 0);
        assert_eq!([0, 1, 2, 3, 4].map(light_table), [LightTable::Fine, LightTable::Rain, LightTable::Rain, LightTable::Fine, LightTable::Fine]);
    }

    #[test]
    fn fade_and_rainbow() {
        let mut f = EnvFade { now: 0, next: 0, rate: 0.0 };
        assert!(f.request(0, 1));
        assert_eq!((f.now, f.next), (0, 1));
        assert!(!f.request(1, 0));
        // f32 accumulation: 600 x (1/600) = 0.9999991, so the fade (like
        // the C) only reaches 1.0 on frame 601.
        for _ in 0..600 {
            f.step(CHANGE_WEATHER_ENV_RATE_USA, true, 1);
        }
        assert!(f.rate < 1.0);
        f.step(CHANGE_WEATHER_ENV_RATE_USA, true, 1);
        assert_eq!((f.now, f.rate), (0, 1.0));
        f.step(CHANGE_WEATHER_ENV_RATE_USA, false, 1);
        assert_eq!((f.now, f.rate), (1, 0.0));
        let day = RainbowDay { reserved: true, is_reserved_date: true, now_sec: 10 * H, summer: true, outdoors: true };
        let (o, held) = rainbow_step(0.0, day, 1.0);
        assert!(o > 0.0 && held);
        assert_eq!(rainbow_step(0.9999999, day, 1.0), (1.0, false));
        let winter = RainbowDay { summer: false, ..day };
        assert_eq!(rainbow_step(0.0, winter, 1.0), (0.0, true));
    }

    #[test]
    fn window_light_quirk() {
        assert_eq!(window_light_target(30, None), 0.0);
        assert_eq!(window_light_target(12 * H + 60, None), 0.0);
        assert_eq!(window_light_target(10 * H, None), 1.0);
        assert_eq!(window_light_target(20 * H, None), 0.0);
        // s16 wrap: 65536 s = 18:12:16 and 86400 - 65536 = 20864 s = 05:47:44.
        assert_eq!(window_light_target(20864, None), 0.0);
        assert_eq!(window_light_target(20864 + 200, None), 1.0);
        assert_eq!(window_light_target(65536 + 10, Some(0.0)), 0.0);
        assert!((window_light_target(10 * H, Some(0.5)) - 0.61).abs() < 1e-6);
    }
}
