//! Scene lighting bookkeeping (`m_lights.c`).
//!
//! Source-verified against ac-decomp `src/game/m_lights.c` and
//! `include/m_lights.h` (primary); ACGC-PC-Port identical.
//!
//! ## Model
//! - `Lights`: type 0 = point, 1 = directional ("diffuse"), 2 = point with a
//!   visible glow sprite (`drawGlow`). Types 0 and 2 light identically.
//! - `Global_light`: ambient (default 80,80,80), fog colour/near/far
//!   (default black, 996..1600) and a doubly linked list of registered
//!   lights drawn from a fixed 32-slot pool (`light_list_buf`).
//! - Per draw, `LightsN` collects at most **7** diffuse lights plus the
//!   ambient (`LightsN_new_diffuse` drops the 8th onwards silently).
//! - `LightsN_list_check(pos != NULL)` converts point lights into
//!   directional ones relative to the object: inside `radius`, colour is
//!   scaled by `1 - (d/r)^2` and the direction is the light->object
//!   vector scaled to length 120 (or x120 when closer than 1 unit).
//!   With `pos == NULL` point lights become real N64 point lights with
//!   `kc = 8`, `kl = -1` (0xFF), `kq = clamp(4500000 / r^2, 20, 255)`.
//! - Glow sprites (`Light_list_point_draw`) scale by `r^2 * 2.6e-6` at
//!   alpha 50.
//!
//! ## Quirk preserved
//! `Light_list_buf_delete` computes the next search index as
//! `(light - lights) / sizeof(Light_list)`: the pointer difference is
//! already an element count, so the extra division by 12 lands the cursor
//! near slot 0. Harmless (the allocator scans for a free slot) but it
//! changes which slot is reused next; reproduced in `LightPool::delete`.

use crate::game_math::Xyz;

pub const LIGHT_TYPE_POINT: u8 = 0;
pub const LIGHT_TYPE_DIFFUSE: u8 = 1;
pub const LIGHT_TYPE_POINT_GLOW: u8 = 2;
pub const MAX_DIFFUSE: usize = 7;
pub const POOL_SIZE: usize = 32;
/// `sizeof(Light_list)` on GameCube (3 pointers).
pub const LIGHT_LIST_SIZEOF: usize = 0xC;

/// Defaults from `Global_light_ct`.
pub const DEFAULT_AMBIENT: [u8; 3] = [80, 80, 80];
pub const DEFAULT_FOG_COLOR: [u8; 3] = [0, 0, 0];
pub const DEFAULT_FOG_NEAR: i16 = 996;
pub const DEFAULT_FOG_FAR: i16 = 1600;

/// `LightPoint`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LightPoint {
    pub x: i16,
    pub y: i16,
    pub z: i16,
    pub color: [u8; 3],
    pub draw_glow: bool,
    pub radius: i16,
}

/// `LightDiffuse`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LightDiffuse {
    pub x: i8,
    pub y: i8,
    pub z: i8,
    pub color: [u8; 3],
}

/// `Lights`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Light {
    Point { glow: bool, p: LightPoint },
    Diffuse(LightDiffuse),
}

/// One computed per-object light (`Light_new`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LightOut {
    /// Directional: colour + direction (s8-ish range).
    Dir { color: [u8; 3], dir: [i32; 3] },
    /// N64 point light.
    Point { color: [u8; 3], pos: [i16; 3], kc: u8, kl: u8, kq: u8 },
}

/// `LightsN`: ambient + up to 7 lights.
#[derive(Clone, Debug, PartialEq)]
pub struct LightsN {
    pub ambient: [u8; 3],
    pub lights: Vec<LightOut>,
}

impl LightsN {
    /// `new_LightsN` / `Global_light_read`.
    pub fn new(ambient: [u8; 3]) -> Self {
        Self { ambient, lights: Vec::with_capacity(MAX_DIFFUSE) }
    }

    fn push(&mut self, l: LightOut) {
        if self.lights.len() < MAX_DIFFUSE {
            self.lights.push(l);
        }
    }

    /// `LightsN__point_proc`: point light seen from `obj`.
    pub fn add_point_relative(&mut self, p: &LightPoint, obj: Xyz) {
        if p.radius <= 0 {
            return;
        }
        let dx = p.x as f32 - obj.x;
        let dy = p.y as f32 - obj.y;
        let dz = p.z as f32 - obj.z;
        let d2 = dx * dx + dy * dy + dz * dz;
        let r = p.radius as f32;
        if d2 >= r * r || self.lights.len() >= MAX_DIFFUSE {
            return;
        }
        let d = d2.sqrt();
        let k = d / r;
        let k = 1.0 - k * k;
        let color = [
            (p.color[0] as f32 * k) as u8,
            (p.color[1] as f32 * k) as u8,
            (p.color[2] as f32 * k) as u8,
        ];
        let s = if d < 1.0 { 120.0 } else { 120.0 / d };
        self.push(LightOut::Dir { color, dir: [(dx * s) as i32, (dy * s) as i32, (dz * s) as i32] });
    }

    /// `LightsN__P_point_proc`: true point light.
    pub fn add_point_absolute(&mut self, p: &LightPoint) {
        if p.radius <= 0 {
            return;
        }
        let r = p.radius as f32;
        let kq = (4_500_000.0 / (r * r)).clamp(20.0, 255.0);
        self.push(LightOut::Point { color: p.color, pos: [p.x, p.y, p.z], kc: 8, kl: 0xFF, kq: kq as u8 });
    }

    /// `LightsN__diffuse_proc`.
    pub fn add_diffuse(&mut self, d: &LightDiffuse) {
        self.push(LightOut::Dir { color: d.color, dir: [d.x as i32, d.y as i32, d.z as i32] });
    }

    /// `LightsN_list_check` over the registered lights (front of list first).
    pub fn list_check<'a>(&mut self, lights: impl IntoIterator<Item = &'a Light>, obj: Option<Xyz>) {
        for l in lights {
            match (l, obj) {
                (Light::Point { p, .. }, Some(o)) => self.add_point_relative(p, o),
                (Light::Point { p, .. }, None) => self.add_point_absolute(p),
                (Light::Diffuse(d), _) => self.add_diffuse(d),
            }
        }
    }
}

/// Glow sprite scale for a type-2 light (`Light_list_point_draw`).
pub fn glow_scale(radius: i16) -> f32 {
    (radius as f32 * radius as f32) * 2.6e-6
}

/// The 32-slot `Light_buffer` allocator.
#[derive(Clone, Debug)]
pub struct LightPool {
    pub used: [bool; POOL_SIZE],
    pub current: usize,
    pub idx: usize,
}

impl Default for LightPool {
    fn default() -> Self {
        Self { used: [false; POOL_SIZE], current: 0, idx: 0 }
    }
}

impl LightPool {
    /// `Light_list_buf_new`: returns the slot or `None` when full.
    pub fn alloc(&mut self) -> Option<usize> {
        if self.current >= POOL_SIZE {
            return None;
        }
        while self.used[self.idx] {
            self.idx += 1;
            if self.idx >= POOL_SIZE {
                self.idx = 0;
            }
        }
        self.used[self.idx] = true;
        self.current += 1;
        Some(self.idx)
    }

    /// `Light_list_buf_delete` (with the double-division cursor quirk).
    pub fn delete(&mut self, slot: usize) {
        if slot < POOL_SIZE && self.used[slot] {
            self.current -= 1;
            self.used[slot] = false;
            self.idx = slot / LIGHT_LIST_SIZEOF;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_relative_falloff() {
        let p = LightPoint { x: 100, y: 0, z: 0, color: [200, 100, 50], draw_glow: false, radius: 200 };
        let mut n = LightsN::new(DEFAULT_AMBIENT);
        n.add_point_relative(&p, Xyz::ZERO);
        // d = 100, r = 200 -> k = 0.75; dir = 100 * 1.2.
        assert_eq!(n.lights[0], LightOut::Dir { color: [150, 75, 37], dir: [120, 0, 0] });
        n.add_point_relative(&p, Xyz::new(400.0, 0.0, 0.0)); // out of range
        assert_eq!(n.lights.len(), 1);
    }

    #[test]
    fn point_absolute_kq_clamped() {
        let mut n = LightsN::new(DEFAULT_AMBIENT);
        n.add_point_absolute(&LightPoint { radius: 100, ..Default::default() });
        n.add_point_absolute(&LightPoint { radius: 1000, ..Default::default() });
        assert!(matches!(n.lights[0], LightOut::Point { kq: 255, .. }));
        assert!(matches!(n.lights[1], LightOut::Point { kq: 20, .. }));
    }

    #[test]
    fn seven_light_cap() {
        let mut n = LightsN::new(DEFAULT_AMBIENT);
        for _ in 0..10 {
            n.add_diffuse(&LightDiffuse::default());
        }
        assert_eq!(n.lights.len(), MAX_DIFFUSE);
    }

    #[test]
    fn pool_wraps_and_delete_cursor_quirk() {
        let mut pool = LightPool::default();
        for i in 0..POOL_SIZE {
            assert_eq!(pool.alloc(), Some(i));
        }
        assert_eq!(pool.alloc(), None);
        pool.delete(25);
        assert_eq!(pool.idx, 2); // 25 / 12
        assert_eq!(pool.alloc(), Some(25));
    }
}
