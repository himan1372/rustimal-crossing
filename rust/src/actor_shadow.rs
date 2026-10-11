//! Actor drop shadows (`m_actor_shadow.c`).
//!
//! Source-verified against ac-decomp `src/game/m_actor_shadow.c`
//! (primary) and ACGC-PC-Port (secondary: only adds the missing `return`
//! in `mActorShadow_SetTexScroll`; retail falls off the end of a non-void
//! function and "returns" `two_tex_scroll`'s result by register luck).
//!
//! ## Per-shadow pipeline (`mAc_ActorShadowDraw_ShadowDrawFlagOn`)
//! 1. Kind: 0 outdoors (`field id & 0xF000 == 0`), 1 indoors.
//! 2. Position: forced `shadow_position` or `world.position`; ground Y from
//!    the shadow BG check (or the plain BG check when forced).
//! 3. Sun angle by clock: 4:00-20:00 sweeps 0..1 (`(t-14400)/57600`),
//!    0:00-4:00 is `0.5 + t/28800`, 20:00-24:00 `(t-72000)/28800`;
//!    `angle = 0x8000 - (s16(16384*f) + 0x6000)`. Alpha from
//!    `kankyo.shadow_alpha`.
//! 4. Ground tilt: average of the BG angles at (+-10, +-10) (`>> 2`).
//! 5. Height falloff: `d = |y - groundY|`, `lim = rate + 100`; above `lim`
//!    scale 0.6 alpha 0; else `k = (lim - d)/lim`, scale `0.6 + 0.4k`,
//!    alpha factor `k`.
//! 6. Radius: non-fish `max(|sx|,|sz|)/19 * 0.018` (z radius 0.01); fish
//!    (eel shadows) use separate x/z radii.
//! 7. Outdoor kind-0 shadows slide: if the ground 40 units "behind" along
//!    the sun angle is >20 lower, the texture scrolls to 28 (13/frame).
//! 8. Final alpha `= shadow_alpha * k * alpha_rate * 0.75` (x0.7 indoors),
//!    scale `= radius * falloff * size_rate`.
//! 9. Y offset per scene: 0.5 for scenes 19/27/28/34, 0.6 for 15/16/49,
//!    else 2.0.

/// Kind 0 = outdoor field, 1 = indoor.
pub fn shadow_kind(field_id: u16) -> u8 {
    (field_id & 0xF000 != 0) as u8
}

/// `mActorShadow_AdjustRate`.
pub fn adjust_rate(rate: f32) -> f32 {
    rate.clamp(0.0, 1.0)
}

/// `mActorShadow_GetTimeAngleY_TimeAlpha` angle part.
pub fn time_angle_y(now_sec: i32) -> i16 {
    let f = if (14400..72000).contains(&now_sec) {
        (now_sec - 14400) as f32 / 57600.0
    } else if now_sec < 14400 {
        0.5 + now_sec as f32 / 28800.0
    } else {
        (now_sec - 72000) as f32 / 28800.0
    };
    let a = (16384.0 * f) as i16;
    (0x8000i32 - (a as i32 + 0x6000)) as i16
}

/// `mActorShadow_GetGroundAverageAngle`: average of four sampled tilts.
pub fn ground_average_angle(samples: [[i16; 3]; 4]) -> [i16; 3] {
    let mut sum = [0i32; 3];
    for s in samples {
        for k in 0..3 {
            sum[k] += s[k] as i32;
        }
    }
    [(sum[0] >> 2) as i16, (sum[1] >> 2) as i16, (sum[2] >> 2) as i16]
}

/// `mActorShadow_GetDistanceRate`: (scale factor, alpha factor).
pub fn distance_rate(pos_y: f32, ground_y: f32, rate: f32) -> (f32, f32) {
    let d = (pos_y - ground_y).abs();
    let lim = rate + 100.0;
    if d > lim {
        return (0.6, 0.0);
    }
    let k = (lim - d) / lim;
    (k * 0.4 + 0.6, k)
}

/// `mActorShadow_RadiusScaleRate` (non-fish): (radius scale, z radius).
pub fn radius_not_fish(size_x: f32, size_z: f32) -> (f32, f32) {
    let big = size_x.abs().max(size_z.abs());
    ((big / 19.0) * 0.018, 0.01)
}

/// `mAc_DecideShadowRadius_IamFish`.
pub fn radius_fish(size_x: f32, size_z: f32) -> (f32, f32) {
    ((size_x / 19.0) * 0.018, (size_z / 19.0) * 0.018)
}

/// `mActorShadow_GetLastAlphaScale`: (scale, alpha byte).
pub fn last_alpha_scale(
    shadow_alpha: f32,
    alpha_k: f32,
    alpha_rate: f32,
    radius: f32,
    falloff: f32,
    size_rate: f32,
    indoor: bool,
) -> (f32, u8) {
    let mut a = shadow_alpha * alpha_k * alpha_rate * 0.75;
    if indoor {
        a *= 0.7;
    }
    (radius * falloff * size_rate, a as u8)
}

/// Shadow Y lift per scene (`mActorShadow_DrawActorShadow`).
pub fn scene_y_adjust(scene: i32) -> f32 {
    match scene {
        19 | 27 | 28 | 34 => 0.5,
        15 | 16 | 49 => 0.6,
        _ => 2.0,
    }
}

/// `mActorShadow_GetShadowTopPos_GetSlideS` decision: slide target.
pub fn slide_target(kind: u8, pos_y: f32, behind_ground_y: f32) -> i32 {
    if kind == 0 && pos_y - behind_ground_y > 20.0 {
        28
    } else {
        0
    }
}

/// `mActorShadow_SetTexScroll` step toward the slide target.
pub fn scroll_step(current: i32, target: i32) -> i32 {
    match current.cmp(&target) {
        std::cmp::Ordering::Less => 0xD,
        std::cmp::Ordering::Greater => -0xD,
        std::cmp::Ordering::Equal => 0,
    }
}

/// `mActorShadow_OperateScrollLimit`.
pub fn operate_scroll_limit(value: i32, step: i32, limit: i32) -> i32 {
    if step > 0 {
        (value + step).min(limit)
    } else if step < 0 {
        (value + step).max(limit)
    } else {
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sun_angle_sweep() {
        assert_eq!(time_angle_y(14400), 0x2000); // 4:00
        assert_eq!(time_angle_y(43200), 0x0000); // noon: 0x8000 - (0x2000 + 0x6000)
        assert_eq!(time_angle_y(0), 0x0000);     // midnight wraps to the same as noon
    }

    #[test]
    fn falloff_and_scroll() {
        assert_eq!(distance_rate(0.0, 0.0, 0.0), (1.0, 1.0));
        assert_eq!(distance_rate(200.0, 0.0, 0.0), (0.6, 0.0));
        assert_eq!(operate_scroll_limit(20, 13, 28), 28);
        assert_eq!(operate_scroll_limit(10, -13, 0), 0);
        assert_eq!(scroll_step(0, 28), 13);
        assert_eq!(scene_y_adjust(27), 0.5);
        assert_eq!(shadow_kind(0x0001), 0);
        assert_eq!(shadow_kind(0x6001), 1);
    }
}
