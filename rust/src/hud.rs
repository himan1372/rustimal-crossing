//! Field HUD widgets: the item-name bubble, camera hint and bell counter
//! (`m_watch_my_step.c`) and the corner clock (`m_banti.c`).
//!
//! Source-verified against ac-decomp (primary) and ACGC-PC-Port
//! (secondary). PC changes: timers are f32 60 fps frames; the bell counter
//! keeps a sub-Bell `money_accum` so dt-scaled `add_calc` does not stall;
//! **the camera-controls hint is only shown indoors** (`Camera2_InDoorCheck`)
//! because the PC port allows camera control everywhere.
//!
//! ## Item bubble ("watch my step")
//! Modes: 0 idle -> (item under the cursor and the normal camera not yet
//! settled) 1 -> 2 (2 frames each, the tail pops in) -> 3 grow (opacity
//! eases to 1 with `add_calc(1 - sqrt(0.5), 0.25, 0.15)`) -> 4 shrink once
//! the item is gone or the camera settles -> 0 below 0.01. The bubble sits
//! 30 units above the player, flipped left when the player is right of
//! x=200 on screen (or right of 120 while facing +X), flipped down above
//! y=68. Width follows the name: `scale = max(0, w*0.875 - 17.5) / 122.5`.
//! Shown outdoors (not during the intro), or indoors at home for
//! non-furniture items.
//!
//! ## Bell counter
//! Shown in shops (Nook's 4 tiers, Redd, post office, Able Sisters) while
//! no submenu is open, except during the part-time job and in Nook's
//! shops during the fukubiki (raffle). Money = wallet + bell bags in
//! pockets (100 / 1,000 / 10,000 / 30,000). The displayed value eases to
//! the real value (`add_calc(0.1, 10000, 1)`) with a coin loop SE that
//! stops after 300 frames or when caught up (then a register "ding").
//!
//! ## Clock ("banti")
//! Six flip-digit animations (month, day, hour tens/ones, minute
//! tens/ones) start only when all are idle, comparing the displayed time
//! with the RTC on the 12-hour clock (0 and 12 show "12"). Visibility
//! follows `mPlib_Get_address_able_display` debounced: switching to
//! READY_NET waits 50 frames; leaving a non-TRUE state 30 frames;
//! leaving TRUE 50 frames; TALKING switches immediately. Alpha eases to 1
//! (0.0425 max step) when allowed and no submenu, out otherwise, quickly
//! (0.1) during demos. The clock moves to the left side when the player is
//! in the bottom-right of the screen (x >= 180, y >= 150) for 10 frames,
//! swapping only while faded out.

use crate::game_math::add_calc;

// ---------------------------------------------------------------------------
// Item bubble
// ---------------------------------------------------------------------------

/// `S_watch_my_step` state that the logic owns.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ItemBubble {
    pub mode: u8,
    pub timer: i32,
    pub opacity: f32,
    pub trans_x: f32,
    pub trans_y: f32,
    pub pos_x: f32,
    pub pos_y: f32,
}

impl ItemBubble {
    /// Mode machine of `watch_my_step_move`. `has_item`: a window item
    /// exists; `camera_settled`: normal camera with `morph_counter > 20`.
    pub fn step(&mut self, has_item: bool, camera_settled: bool) {
        let k = 1.0 - 0.5f32.sqrt();
        match self.mode {
            0 => {
                if has_item && !camera_settled {
                    self.opacity = 0.0;
                    self.timer = 2;
                    self.mode = 1;
                }
            }
            1 | 2 => {
                let t = self.timer;
                self.timer -= 1;
                if t == 0 {
                    self.timer = 2;
                    self.mode += 1;
                }
            }
            3 => {
                add_calc(&mut self.opacity, 1.0, k, 0.25, 0.15);
                if !has_item || camera_settled {
                    self.mode = 4;
                }
            }
            4 => {
                add_calc(&mut self.opacity, 0.0, k, 0.1, 0.05);
                add_calc(&mut self.opacity, 0.0, k, 0.005, 0.005);
                if self.opacity < 0.01 {
                    self.mode = 0;
                }
            }
            _ => self.mode = 0,
        }
    }

    /// Placement from the player's screen position (`mode < 4` only).
    pub fn place(&mut self, screen_x: f32, screen_y: f32, player_rot_y: i16) {
        self.trans_x = if screen_x > 200.0 || (screen_x > 120.0 && player_rot_y >= 0) { -1.0 } else { 1.0 };
        self.trans_y = if screen_y < 68.0 { -1.0 } else { 1.0 };
        self.pos_x = screen_x - (self.trans_x * -40.0 + 160.0);
        self.pos_y = screen_y - (self.trans_y * 42.0 + 120.0);
    }
}

/// Bubble width scale from the item-name pixel width.
pub fn bubble_scale(name_width: f32) -> f32 {
    (name_width * 0.875 - 17.5).max(0.0) / 122.5
}

// ---------------------------------------------------------------------------
// Bell counter
// ---------------------------------------------------------------------------

/// Scenes where the counter shows (`place_chk`); the first five are
/// Nook's shops (affected by the raffle exclusion).
pub const BELL_COUNTER_SCENES: [i32; 8] = [9, 23, 24, 25, 29, 12, 14, 46];

/// Bell bag item -> amount (`mTG_itemNo_to_amount` / `get_all_money`).
pub fn bell_bag_amount(item: u16) -> u32 {
    match item {
        0x2100 => 1000,
        0x2101 => 10000,
        0x2102 => 30000,
        0x2103 => 100,
        _ => 0,
    }
}

/// `get_all_money`: wallet + bell bags (normal condition) in pockets.
pub fn all_money(wallet: u32, pockets: impl IntoIterator<Item = u16>) -> u32 {
    pockets.into_iter().fold(wallet, |m, it| m + bell_bag_amount(it))
}

/// Whether the counter is active in this scene.
pub fn bell_counter_enabled(scene: i32, first_job: bool, raffle_running: bool) -> bool {
    match BELL_COUNTER_SCENES.iter().position(|&s| s == scene) {
        None => false,
        Some(i) => !(first_job || (i <= 4 && raffle_running)),
    }
}

/// One frame of the displayed-money ease (`add_calc(0.1, 10000, 1)`).
pub fn ease_money(shown: u32, actual: u32) -> u32 {
    let mut m = shown as f32;
    add_calc(&mut m, actual as f32, 0.1, 10000.0, 1.0);
    m as u32
}

// ---------------------------------------------------------------------------
// Clock (banti)
// ---------------------------------------------------------------------------

/// `mPlayer_ADDRESSABLE_*`.
pub const ADDRESSABLE_TRUE: i32 = 0;
pub const ADDRESSABLE_FALSE_MOVEMENT: i32 = 1;
pub const ADDRESSABLE_FALSE_TALKING: i32 = 2;
pub const ADDRESSABLE_FALSE_READY_NET: i32 = 3;

/// 12-hour clock digit value used by the flip check.
pub fn clock_hour(hour: u8) -> u8 {
    match hour {
        0 | 12 => 12,
        h if h > 11 => h - 12,
        h => h,
    }
}

/// Which digits flip between two times: [month, day, hour10, hour1, min10, min1].
pub fn clock_flips(shown: (u8, u8, u8, u8), now: (u8, u8, u8, u8)) -> [bool; 6] {
    let (sm, sd, sh, smin) = shown;
    let (nm, nd, nh, nmin) = now;
    let (a, b) = (clock_hour(sh), clock_hour(nh));
    [sm != nm, sd != nd, a / 10 != b / 10, a % 10 != b % 10, smin / 10 != nmin / 10, smin % 10 != nmin % 10]
}

/// Debounce state of `banti_move`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ClockVisibility {
    pub addressable: i32,
    pub timer: i32,
    pub alpha: f32,
}

impl ClockVisibility {
    /// Debounce + fade for one frame. `demo`: demo/title/disabled;
    /// `submenu_idle`: `process_status == WAIT`.
    pub fn step(&mut self, now: i32, demo: bool, submenu_idle: bool) {
        if self.addressable != now {
            self.timer += 1;
            let update = if now == ADDRESSABLE_FALSE_READY_NET {
                self.timer > 50
            } else if self.addressable != ADDRESSABLE_TRUE {
                self.timer > 30 || now == ADDRESSABLE_FALSE_TALKING
            } else {
                self.timer > 50 || now == ADDRESSABLE_FALSE_TALKING
            };
            if update {
                self.timer = 0;
                self.addressable = now;
            }
        } else {
            self.timer = 0;
        }
        let k = 1.0 - 0.8f32.sqrt();
        if demo {
            self.addressable = ADDRESSABLE_FALSE_MOVEMENT;
            add_calc(&mut self.alpha, 0.0, k, 0.1, 0.005);
        } else if self.addressable == ADDRESSABLE_TRUE && submenu_idle {
            add_calc(&mut self.alpha, 1.0, k, 0.0425, 0.0005);
        } else {
            add_calc(&mut self.alpha, 0.0, k, 0.0425, 0.0005);
        }
    }
}

/// `banti_chk_disp_left` state.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ClockSide {
    pub move_left: bool,
    pub move_timer: f32,
    pub disp_left: bool,
    pub disabled: bool,
}

impl ClockSide {
    pub fn step(&mut self, screen_x: f32, screen_y: f32, alpha: f32) {
        let want_left = screen_x >= 180.0 && screen_y >= 150.0;
        if self.move_left != want_left {
            self.move_left = want_left;
            self.move_timer = 0.0;
        }
        self.move_timer += 1.0;
        if self.move_timer > 10.0 {
            self.move_timer = 10.0;
            if alpha < 0.01 {
                self.disp_left = want_left;
                self.disabled = false;
            } else if self.disp_left != want_left {
                self.disabled = true;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bubble_cycle() {
        let mut b = ItemBubble::default();
        b.step(true, false);
        assert_eq!(b.mode, 1);
        for _ in 0..6 {
            b.step(true, false);
        }
        assert_eq!(b.mode, 3);
        b.step(true, false);
        assert!(b.opacity > 0.0);
        b.step(false, false);
        assert_eq!(b.mode, 4);
        for _ in 0..200 {
            b.step(false, false);
        }
        assert_eq!(b.mode, 0);
        b.place(250.0, 50.0, 0);
        assert_eq!((b.trans_x, b.trans_y), (-1.0, -1.0));
    }

    #[test]
    fn bells() {
        assert_eq!(all_money(500, [0x2100, 0x2103, 0x1000]), 1600);
        assert!(bell_counter_enabled(46, false, true));
        assert!(!bell_counter_enabled(9, false, true));
        assert!(!bell_counter_enabled(1, false, false));
        assert_eq!(ease_money(0, 1000), 100);
    }

    #[test]
    fn clock() {
        assert_eq!(clock_hour(0), 12);
        assert_eq!(clock_hour(13), 1);
        assert_eq!(clock_flips((1, 1, 9, 59), (1, 1, 10, 0)), [false, false, true, true, true, true]);
        let mut v = ClockVisibility::default();
        v.step(ADDRESSABLE_FALSE_TALKING, false, true);
        assert_eq!(v.addressable, ADDRESSABLE_FALSE_TALKING); // immediate
        for _ in 0..30 {
            v.step(ADDRESSABLE_TRUE, false, true);
        }
        assert_eq!(v.addressable, ADDRESSABLE_FALSE_TALKING);
        v.step(ADDRESSABLE_TRUE, false, true);
        assert_eq!(v.addressable, ADDRESSABLE_TRUE);
    }
}
