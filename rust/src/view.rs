//! View / projection state (`m_view.c`).
//!
//! Source-verified against ac-decomp `src/game/m_view.c` (primary) and the
//! ACGC-PC-Port copy (secondary: adds an eager `set_viewport` in
//! `initView` so submenu restore has a valid viewport before the first
//! `showPerspectiveView`; on GameCube that memory happened to be zeroed).
//!
//! ## Facts
//! - Logical screen is 320x240 (`SCREEN_WIDTH/HEIGHT`) rendered to a 2x
//!   framebuffer: `set_viewport` writes `vscale = (w*4, h*4)` and
//!   `vtrans = ((w + l)*4, (h + top)*4)` (N64 would use w*2), and a
//!   full-screen scissor is doubled to 640x480 (`setScissorX`).
//! - `initView` defaults: fovY 20 deg, near 200 outdoors (`field_type ==
//!   FG`) or 20 indoors, far 1600, scale 1, eye (0,0,-1), up +Y.
//!   Retail has a copy-paste bug setting `center.x` three times (y/z left
//!   as-is); `BUGFIXES` builds set x/y/z.
//! - `setLookAtView` nudges `eye.x += 0.1` when eye and center share X and
//!   Z (avoids a degenerate look-at with up = +Y).
//! - Update flags: LOOKAT 1, SCISSOR 2, PERSPECTIVE 4, ORTHOGRAPHIC 8;
//!   `showView(mask)` picks ortho when `((mask >> 4) | (mask & flag)) & 8`.
//! - Stretch effect (`do_stretch_view`): rotation/scale ease toward their
//!   targets by `step` per call; `step == 1` snaps and stops; the matrix is
//!   R * S * R^-1 with radians converted by 10430.378 (= 0x10000 / 2pi).

use crate::game_math::Xyz;

pub const SCREEN_WIDTH: i32 = 320;
pub const SCREEN_HEIGHT: i32 = 240;

pub const VIEW_UPDATE_NONE: u32 = 0;
pub const VIEW_UPDATE_LOOKAT: u32 = 1;
pub const VIEW_UPDATE_SCISSOR: u32 = 2;
pub const VIEW_UPDATE_PERSPECTIVE: u32 = 4;
pub const VIEW_UPDATE_ORTHOGRAPHIC: u32 = 8;

/// Radians -> s16 angle (`10430.378f`).
pub const RAD_TO_SHORT: f32 = 10430.378;

/// `rect`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub top: i32,
    pub bottom: i32,
    pub l: i32,
    pub r: i32,
}

/// N64 `Vp` (vscale, vtrans).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Viewport {
    pub vscale: [i16; 4],
    pub vtrans: [i16; 4],
}

/// `set_viewport`.
pub fn viewport_for(screen: &Rect) -> Viewport {
    let w = screen.r - screen.l;
    let h = screen.bottom - screen.top;
    Viewport {
        vscale: [(w * 4) as i16, (h * 4) as i16, 511, 0],
        vtrans: [((w + screen.l) * 4) as i16, ((h + screen.top) * 4) as i16, 511, 0],
    }
}

/// `setScissorX`: (left, top, right, bottom) actually emitted.
pub fn scissor_for(left: i32, top: i32, right: i32, bottom: i32) -> (i32, i32, i32, i32) {
    if left == 0 && top == 0 && right == SCREEN_WIDTH && bottom == SCREEN_HEIGHT {
        (left, top, right * 2, bottom * 2)
    } else {
        (left, top, right, bottom)
    }
}

/// Stretch state (`View::stretch`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stretch {
    pub target_rotate: Xyz,
    pub target_scale: Xyz,
    pub rotate: Xyz,
    pub scale: Xyz,
    pub step: f32,
}

impl Default for Stretch {
    /// `stretchViewInit`.
    fn default() -> Self {
        let one = Xyz::new(1.0, 1.0, 1.0);
        Self { target_rotate: Xyz::ZERO, target_scale: one, rotate: Xyz::ZERO, scale: one, step: 0.0 }
    }
}

impl Stretch {
    /// `do_stretch_view` state update; returns the s16 rotation angles to
    /// apply (R * S * R^-1) or `None` when inactive.
    pub fn advance(&mut self) -> Option<[i16; 3]> {
        let step = self.step;
        if step == 0.0 {
            return None;
        }
        if step == 1.0 {
            self.rotate = self.target_rotate;
            self.scale = self.target_scale;
            self.step = 0.0;
        } else {
            let lerp = |a: &mut f32, b: f32| *a += step * (b - *a);
            lerp(&mut self.rotate.x, self.target_rotate.x);
            lerp(&mut self.rotate.y, self.target_rotate.y);
            lerp(&mut self.rotate.z, self.target_rotate.z);
            lerp(&mut self.scale.x, self.target_scale.x);
            lerp(&mut self.scale.y, self.target_scale.y);
            lerp(&mut self.scale.z, self.target_scale.z);
        }
        Some([
            (self.rotate.x * RAD_TO_SHORT) as i16,
            (self.rotate.y * RAD_TO_SHORT) as i16,
            (self.rotate.z * RAD_TO_SHORT) as i16,
        ])
    }
}

/// The logic-side `View`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    pub screen: Rect,
    pub fov_y: f32,
    pub near: f32,
    pub far: f32,
    pub scale: f32,
    pub eye: Xyz,
    pub center: Xyz,
    pub up: Xyz,
    pub flag: u32,
    pub stretch: Stretch,
}

impl View {
    /// `initView`. `outdoors` is `field_type == mFI_FIELDTYPE2_FG`;
    /// `bugfixes` selects the corrected center reset.
    pub fn init(outdoors: bool, prev_center: Xyz, bugfixes: bool) -> Self {
        let center = if bugfixes { Xyz::ZERO } else { Xyz::new(0.0, prev_center.y, prev_center.z) };
        Self {
            screen: Rect { top: 0, bottom: SCREEN_HEIGHT, l: 0, r: SCREEN_WIDTH },
            fov_y: 20.0,
            near: if outdoors { 200.0 } else { 20.0 },
            far: 1600.0,
            scale: 1.0,
            eye: Xyz::new(0.0, 0.0, -1.0),
            center,
            up: Xyz::new(0.0, 1.0, 0.0),
            flag: VIEW_UPDATE_LOOKAT | VIEW_UPDATE_SCISSOR | VIEW_UPDATE_PERSPECTIVE,
            stretch: Stretch::default(),
        }
    }

    /// `setLookAtView` (returns the possibly nudged eye like the C, which
    /// writes it back through the pointer).
    pub fn set_look_at(&mut self, mut eye: Xyz, center: Xyz, up: Xyz) -> Xyz {
        if eye.x == center.x && eye.z == center.z {
            eye.x += 0.1;
        }
        self.eye = eye;
        self.center = center;
        self.up = up;
        self.flag |= VIEW_UPDATE_LOOKAT;
        eye
    }

    /// `setPerspectiveView`.
    pub fn set_perspective(&mut self, fov_y: f32, near: f32, far: f32) {
        self.fov_y = fov_y;
        self.near = near;
        self.far = far;
        self.flag |= VIEW_UPDATE_PERSPECTIVE;
    }

    /// `setScaleView`.
    pub fn set_scale(&mut self, scale: f32) {
        self.scale = scale;
        self.flag |= VIEW_UPDATE_PERSPECTIVE;
    }

    /// Aspect ratio passed to `guPerspective`.
    pub fn aspect(&self) -> f32 {
        (self.screen.r - self.screen.l) as f32 / (self.screen.bottom - self.screen.top) as f32
    }

    /// `showView` projection choice.
    pub fn uses_ortho(&self, flag_mask: u32) -> bool {
        ((flag_mask >> 4) | (flag_mask & self.flag)) & VIEW_UPDATE_ORTHOGRAPHIC != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn viewport_is_double_scaled() {
        let vp = viewport_for(&Rect { top: 0, bottom: 240, l: 0, r: 320 });
        assert_eq!(vp.vscale, [1280, 960, 511, 0]);
        assert_eq!(vp.vtrans, [1280, 960, 511, 0]);
        assert_eq!(scissor_for(0, 0, 320, 240), (0, 0, 640, 480));
        assert_eq!(scissor_for(10, 0, 320, 240), (10, 0, 320, 240));
    }

    #[test]
    fn init_and_lookat() {
        let mut v = View::init(false, Xyz::new(5.0, 6.0, 7.0), false);
        assert_eq!(v.near, 20.0);
        assert_eq!(v.center, Xyz::new(0.0, 6.0, 7.0)); // retail copy-paste bug
        let eye = v.set_look_at(Xyz::new(1.0, 50.0, 2.0), Xyz::new(1.0, 0.0, 2.0), Xyz::new(0.0, 1.0, 0.0));
        assert_eq!(eye.x, 1.1);
        assert!(!v.uses_ortho(0));
        assert!(v.uses_ortho(VIEW_UPDATE_ORTHOGRAPHIC << 4));
    }

    #[test]
    fn stretch_snaps_at_one() {
        let mut s = Stretch { target_scale: Xyz::new(2.0, 2.0, 2.0), step: 0.5, ..Stretch::default() };
        s.advance();
        assert_eq!(s.scale.x, 1.5);
        s.step = 1.0;
        s.advance();
        assert_eq!(s.scale.x, 2.0);
        assert_eq!(s.advance(), None);
    }
}
