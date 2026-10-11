//! Town props and small scenery actors: mailbox (`ac_mailbox`), island
//! flag (`ac_flag`), raffle drum + ball (`ac_garagara`), harvest festival
//! tables (`ac_htable`), signboards (`ac_sign`), notice/construction signs
//! (`ac_reserve`), nameplates (`ac_nameplate`), the dump (`ac_dump`), the
//! aerobics radio (`ac_radio`, furniture twin `ac_radio_test`), sports fair
//! baskets/balls/mats (`ac_kago`, `ac_tama`, `ac_goza`), the tug-of-war rope
//! (`ac_rope`), fortune stand (`ac_mikuji`), mural (`ac_mural`), moving test
//! platform (`ac_mbg`), Able Sisters' sewing machine (`ac_misin`) and the
//! empty `ac_dummy` / debug `ac_sample`.
//!
//! Source-verified against ac-decomp (primary) and ACGC-PC-Port
//! (secondary). Angles evaluated with mingw32 gcc.
//!
//! ## PC-port changes
//! - The mailbox ringing SE uses an accumulated 1 s period (retail:
//!   `game_frame % 60 == 0`); radio note puffs, raffle drum, sewing machine
//!   and flag curves run in fixed 60 Hz ticks.
//! - `ac_dump`: the acre-change despawn block was removed (borderless); the
//!   radio uses the "more than one acre away" rule.
//!
//! ## Facts
//! - Mailbox: flag up while the house mailbox has mail (SE 0x433 at frame
//!   15 going up, 0x434 down); the owner hears SE 0x43B once a second.
//!   Opening needs A, owner, facing within 6961 (38.24 deg) of the box's
//!   45-deg-rotated front, and the unit in front being this mailbox. With
//!   a Resetti visit pending and `reset_count < 3` (`< 4` on the Australian
//!   build) opening the mailbox triggers Resetti instead (`reset_type 5`).
//!   Bug: the destructor frees only the first of its two skeletons.
//! - Island flag: talking offers to change the design; lowering takes 40
//!   frames, raising 100 frames in two quadratic "pulls" (72.5 -> 110 by
//!   frame 55, then -> 132.5). The chosen design is copied into
//!   `island.flag_design`.
//! - Raffle drum: only built on fukubiki days. A ball pops out at
//!   animation frame 46 with random speed/angle and bounces inside a
//!   40x14 tray (gravity 0.15, max fall 1, bounce x0.4, wall x0.985,
//!   rolling x0.99).
//! - Harvest tables: Tom Nook's festival knife-and-fork is placed beside the
//!   table unless the player already carries one; the pickup probe looks
//!   up to three units ahead, accumulating the step each probe (quirk).
//! - Signboards: a new sign "boings" in (sine wobble, amplitude x0.4 and
//!   frequency +750 per half-cycle) until the wobble is < 0.0002; removal
//!   shrinks to zero. Debug register NMREG0 == 1000 turns every tree into a
//!   random sign (100: every other empty unit).
//! - Notice signs: message 11053 + index (indices 6-8 name the town fruit
//!   and Tom Nook); index 0x42 uses 11074. Nameplates 4969, dump 4970,
//!   flag 12390.
//! - Aerobics radio: note effect every 36 frames, offset (2, 0, -10).
//!   K.K. songs are not played by these actors: house radios go through
//!   `aMR_RadioCommonMove`; K.K.'s concerts are run by `ac_mikanbox`
//!   (see `kk_credits.rs`).
//! - Sewing machine: the cloth under the needle is one of your designs
//!   (1 in 3) or a random shop shirt; it cycles four corner targets.
//! - Mural: debug-register driven (TAKREG 10-12) grid of design tiles.
//! - Mbg: debug moving platform, +-60 units sine sway (2 deg/frame), spin
//!   47 (0x2F) per frame.

// ---------------------------------------------------------------------------
// Mailbox
// ---------------------------------------------------------------------------

/// `aMBX_PLAYER_OPEN_ANGLE` (gcc-evaluated).
pub const MAILBOX_OPEN_ANGLE: i32 = 6961;
/// Box facing per arrange index (`angle_table`).
pub const MAILBOX_ANGLES: [i16; 4] = [0x4000, 0, 0x4000, 0];

/// What opening the mailbox does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MailboxOpen {
    None,
    /// Resetti hijacks (`reset_type = 5`).
    Resetti,
    /// Open the mailbox submenu.
    Open,
}

/// `aMBX_check_take_mail`. `player_angle_y` is the actor's
/// `player_angle_y`; `aus` selects the GAFU01 reset threshold.
#[allow(clippy::too_many_arguments)]
pub fn mailbox_try_open(
    box_rot_y: i16,
    player_angle_y: i16,
    a_pressed: bool,
    is_owner: bool,
    facing_this_box: bool,
    reset_pending: bool,
    reset_count: u8,
    house_owned: bool,
    submenu_ok: bool,
    aus: bool,
) -> MailboxOpen {
    let front = box_rot_y as i32 - 0x2000;
    let d = (player_angle_y as i32 - front) as i16 as i32;
    if !(d.abs() < MAILBOX_OPEN_ANGLE && a_pressed && is_owner && facing_this_box) {
        return MailboxOpen::None;
    }
    let limit = if aus { 4 } else { 3 };
    if reset_pending && reset_count < limit {
        MailboxOpen::Resetti
    } else if house_owned && submenu_ok {
        MailboxOpen::Open
    } else {
        MailboxOpen::None
    }
}

/// Flag animation target: (raise?, settle directly into the up-wait loop?).
pub fn mailbox_flag(mail_count: usize, currently_up_or_unset: bool) -> (bool, bool) {
    if mail_count != 0 {
        (true, currently_up_or_unset)
    } else {
        (false, false)
    }
}

// ---------------------------------------------------------------------------
// Island flag
// ---------------------------------------------------------------------------

pub const FLAG_TOP: f32 = 132.5;
pub const FLAG_BOTTOM: f32 = 72.5;
pub const FLAG_MIDDLE: f32 = 110.0;
pub const FLAG_UP_FRAMES: i32 = 100;
pub const FLAG_DOWN_FRAMES: i32 = 40;

/// `aFLAG_up` height for frame `t` (0..100).
pub fn flag_up_height(t: i32) -> f32 {
    let (s, m, e) = (FLAG_BOTTOM, FLAG_MIDDLE, FLAG_TOP);
    let (fe, f3, f2, f1) = (100.0f32, 70.0f32, 55.0f32, 15.0f32);
    let t = t as f32;
    if t <= f1 {
        s + (m - s) / (f1 * f2) * t * t
    } else if t <= f2 {
        m + (m - s) / (f2 * (f1 - f2)) * (t - f2) * (t - f2)
    } else if t <= f3 {
        m + (e - m) / ((f3 - f2) * (fe - f2)) * (t - f2) * (t - f2)
    } else {
        e + (e - m) / ((fe - f2) * ((f3 - f2) - (fe - f2))) * (t - fe) * (t - fe)
    }
}

/// `aFLAG_down` height for frame `t` (0..40).
pub fn flag_down_height(t: i32) -> f32 {
    let (s, e) = (FLAG_BOTTOM, FLAG_TOP);
    let (fe, fm) = (40.0f32, 16.0f32);
    let len = e - s;
    let t = t as f32;
    if t <= fm {
        e + t * (-(len / (fm * fe)) * t)
    } else {
        s + -(len / (fe * (fm - fe))) * (t - fe) * (t - fe)
    }
}

// ---------------------------------------------------------------------------
// Raffle ball
// ---------------------------------------------------------------------------

/// `aGRGR_ball_c` physics state.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RaffleBall {
    pub pos: [f32; 3],
    pub src: [f32; 3],
    pub vel_y: f32,
    pub vel_xz: f32,
    pub angle: i16,
    pub on_ground: bool,
    pub old_on_ground: bool,
    pub hit_wall: bool,
    pub hit_wall_angle: i16,
}

impl RaffleBall {
    /// `aGRBL_make`; `r1..r3` are three `fqrand()` draws.
    pub fn spawn(src: [f32; 3], pos: [f32; 3], r1: f32, r2: f32, r3: f32) -> Self {
        Self {
            pos,
            src,
            vel_y: -0.15 - r1 * 0.1,
            vel_xz: 0.225 + r2 * 0.1,
            angle: (0x4000 + (r3 * 4096.0 - 2048.0) as i32) as i16,
            on_ground: false,
            old_on_ground: false,
            hit_wall: false,
            hit_wall_angle: 0,
        }
    }

    fn bg_check(&mut self) {
        let ground = self.src[1];
        self.old_on_ground = self.on_ground;
        self.on_ground = false;
        if self.pos[1] - 2.0 < ground {
            self.pos[1] = ground + 2.0;
            self.on_ground = true;
        }
        self.hit_wall = false;
        let (x_max, x_min) = (self.src[0] + 20.0, self.src[0] - 20.0);
        let (z_max, z_min) = (self.src[2] + 7.0, self.src[2] - 7.0);
        if x_max < self.pos[0] {
            self.pos[0] = x_max;
            self.hit_wall = true;
            self.hit_wall_angle = -0x4000;
        }
        if self.pos[0] < x_min {
            self.pos[0] = x_min;
            self.hit_wall = true;
            self.hit_wall_angle = 0x4000;
        }
        if z_max < self.pos[2] {
            self.pos[2] = z_max;
            self.hit_wall = true;
            self.hit_wall_angle = i16::MIN;
        }
        if self.pos[2] < z_min {
            self.pos[2] = z_min;
            self.hit_wall = true;
            self.hit_wall_angle = 0;
        }
    }

    fn integrate(&mut self) {
        self.vel_y = (self.vel_y - 0.15).max(-1.0);
        self.pos[1] += self.vel_y;
        self.pos[0] += self.vel_xz * crate::game_math::sin_s(self.angle);
        self.pos[2] += self.vel_xz * crate::game_math::cos_s(self.angle);
    }

    /// `aGRBL_KorokoroMove` (the first frame, `StartMove`, skips the
    /// bounce/friction block).
    pub fn step(&mut self, first: bool) {
        if !first {
            if self.hit_wall {
                let diff = (self.angle.wrapping_add(i16::MIN) as i32 - self.hit_wall_angle as i32) as i16;
                self.angle = self.angle.wrapping_sub(diff);
                self.vel_xz *= 0.985;
            }
            if self.on_ground {
                if self.old_on_ground {
                    self.vel_xz *= 0.99;
                } else {
                    self.vel_y *= -0.4;
                }
            }
        }
        self.integrate();
        self.bg_check();
    }
}

/// Frame of the drum animation at which the ball is released.
pub const RAFFLE_BALL_FRAME: i32 = 46;

// ---------------------------------------------------------------------------
// Harvest festival tables
// ---------------------------------------------------------------------------

/// `aHTBL_GetPlayerLookAtUnit` offsets per 22.5-degree sector.
pub const LOOK_AT_OFFSETS: [(i32, i32); 16] = [
    (0, 1), (1, 1), (1, 1), (1, 0), (1, 0), (1, -1), (1, -1), (0, -1),
    (0, -1), (-1, -1), (-1, -1), (-1, 0), (-1, 0), (-1, 1), (-1, 1), (0, 1),
];

/// The three probed units for the fork pickup, with the cumulative quirk
/// (each probe adds its step to the previous one; probes 1 and 2 rotate
/// the sector by +2 / -2).
pub fn fork_probe_units(ux: i32, uz: i32, angle_y: i16) -> [(i32, i32); 3] {
    const HIGH: [i32; 3] = [0, 2, -2];
    let base = ((angle_y as i32 >> 12) & 0xF) as usize;
    let (mut x, mut z) = (ux, uz);
    let mut out = [(0, 0); 3];
    for (i, h) in HIGH.iter().enumerate() {
        let idx = ((base as i32 + h) & 0xF) as usize;
        x += LOOK_AT_OFFSETS[idx].0;
        z += LOOK_AT_OFFSETS[idx].1;
        out[i] = (x, z);
    }
    out
}

/// Maximum pickup distance from the player (56 units).
pub const FORK_PICKUP_RADIUS: f32 = 56.0;

// ---------------------------------------------------------------------------
// Signboards
// ---------------------------------------------------------------------------

/// Wobble state of one appearing sign (`exist_flag == 1`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SignWobble {
    pub amplitude: f32,
    pub phase: i16,
    pub freq: i16,
}

impl SignWobble {
    /// One frame: returns (scale_x, scale_y, settled).
    pub fn step(&mut self) -> (f32, f32, bool) {
        let prev = self.phase;
        self.phase = self.phase.wrapping_add(self.freq);
        let now = self.phase;
        let sy = self.amplitude * crate::game_math::sinf_table(now as f32 * 0.0000958738);
        let sx = 0.01 - sy;
        if (prev < 0 && now >= 0) || ((prev as i32) < -0x8000 && (now as i32) >= -0x8000) {
            self.amplitude *= 0.4;
            self.freq = self.freq.wrapping_add(750);
        }
        (sx, sy + 0.01, self.amplitude.abs() < 0.0002)
    }
}

/// `aSIGN_actor_BkandUtNum_formal`: normalise unit overflow into the
/// neighbouring block; true when the block is inside the map.
pub fn normalize_block_unit(bx: &mut i32, bz: &mut i32, ux: &mut i32, uz: &mut i32, bx_max: i32, bz_max: i32) -> bool {
    if *uz < 0 {
        *uz += 16;
        *bz -= 1;
    } else if *uz >= 16 {
        *uz -= 16;
        *bz += 1;
    }
    if *ux < 0 {
        *ux += 16;
        *bx -= 1;
    } else if *ux >= 16 {
        *ux -= 16;
        *bx += 1;
    }
    *bz >= 0 && *bz < bz_max && *bx >= 0 && *bx < bx_max
}

// ---------------------------------------------------------------------------
// Signs / plates / misc
// ---------------------------------------------------------------------------

pub const MSG_RESERVE_BASE: i32 = 11053;
pub const MSG_RESERVE_SPECIAL: i32 = 11074;
pub const MSG_NAMEPLATE: i32 = 4969;
pub const MSG_DUMP: i32 = 4970;
pub const MSG_FLAG: i32 = 12390;

/// Notice sign message and whether it needs the fruit/Tom Nook strings.
pub fn reserve_message(sign_idx: i32) -> (i32, bool) {
    if sign_idx == 0x42 {
        (MSG_RESERVE_SPECIAL, false)
    } else {
        (MSG_RESERVE_BASE + sign_idx, (6..=8).contains(&sign_idx))
    }
}

/// Talk gate shared by nameplate / dump / notice signs: player on the
/// sign's south side and within 45 deg.
pub fn sign_talk_gate(player_z: f32, sign_z: f32, player_angle_y: i16) -> bool {
    player_z >= sign_z && (player_angle_y as i32).abs() < 0x2000
}

/// Aerobics radio: note puff period and offset.
pub const RADIO_NOTE_PERIOD: i32 = 36;
pub const RADIO_NOTE_OFFSET: [f32; 3] = [2.0, 0.0, -10.0];

/// Sports fair variant (red 0 / white 1) for baskets and ball piles.
pub fn sportsfair_variant(npc_id: u16, base: u16) -> u16 {
    npc_id.wrapping_sub(base)
}
pub const SPORTSFAIR_BALLS_RED: u16 = 0x5834;
pub const SPORTSFAIR_BASKET_RED: u16 = 0x5836;

/// Sewing-machine cloth: 1 in 3 uses one of the player's designs.
pub fn sewing_uses_design(random3: i32) -> bool {
    random3 == 1
}
pub const SEWING_CLOTH_TARGETS: [[f32; 3]; 4] = [[-4.0, 0.0, 4.0], [-4.0, 0.0, -4.0], [4.0, 0.0, -4.0], [4.0, 0.0, 4.0]];

/// Debug moving platform (`ac_mbg`): x offset for a counter in degrees.
pub fn mbg_offset_x(counter_deg: f32) -> f32 {
    crate::game_math::sinf_table(counter_deg.to_radians()) * 60.0
}
pub const MBG_SPIN_STEP: i16 = 47;

// ---------------------------------------------------------------------------
// Post office e-Reader terminal (ac_pterminal)
// ---------------------------------------------------------------------------

pub const MAIL_HEADER_LEN: usize = 24;
pub const MAIL_BODY_LEN: usize = 192;
pub const MAIL_FOOTER_LEN: usize = 32;
pub const PAPER_UNIQUE_NUM: u8 = 64;
pub const NAME_TYPE_SPNPC: u16 = 13;
pub const NAME_TYPE_NPC: u16 = 14;

/// A letter card scanned through the GBA/e-Reader (`aPT_receive_buf`).
#[derive(Clone, Debug, PartialEq)]
pub struct ECardLetter {
    pub header: [u8; MAIL_HEADER_LEN],
    pub body: Vec<u8>,
    pub footer: [u8; MAIL_FOOTER_LEN],
    pub paper_type: u8,
    pub present: u16,
    pub npc_id: u16,
    pub header_back_start: u8,
    pub card_no: u16,
    /// Retail sets `mMl_TYPE_SPNPC_PASSWORD` only through the buggy
    /// `ITEM_NAME_GET_TYPE(npc_id == NAME_TYPE_SPNPC)`, which is always 0.
    pub password_type: bool,
}

/// `aPT_SendMail` parsing of the 256-byte buffer. `None` if the sender is
/// neither a special NPC nor a villager (the card is still marked used).
/// `bugfix` = the decomp's BUGFIXES branch.
pub fn parse_ecard_letter(buf: &[u8; 256], bugfix: bool) -> Option<ECardLetter> {
    let be = |o: usize| u16::from_be_bytes([buf[o], buf[o + 1]]);
    let body_ofs = MAIL_HEADER_LEN;
    let footer_ofs = body_ofs + MAIL_BODY_LEN;
    let paper_ofs = footer_ofs + MAIL_FOOTER_LEN;
    let npc_id = be(paper_ofs + 3);
    let kind = (npc_id & 0xF000) >> 12;
    if kind != NAME_TYPE_SPNPC && kind != NAME_TYPE_NPC {
        return None;
    }
    let mut header = [0; MAIL_HEADER_LEN];
    header.copy_from_slice(&buf[..MAIL_HEADER_LEN]);
    let mut footer = [0; MAIL_FOOTER_LEN];
    footer.copy_from_slice(&buf[footer_ofs..paper_ofs]);
    Some(ECardLetter {
        header,
        body: buf[body_ofs..footer_ofs].to_vec(),
        footer,
        paper_type: buf[paper_ofs] % PAPER_UNIQUE_NUM,
        present: be(paper_ofs + 1),
        npc_id,
        header_back_start: buf[paper_ofs + 5].min(MAIL_HEADER_LEN as u8 - 1),
        card_no: be(paper_ofs + 6),
        password_type: bugfix && kind == NAME_TYPE_SPNPC,
    })
}

/// Terminal messages: question 0x3DEC, send ok 0x3DF4, send failed 0x3DF1,
/// GBA not found 0x3DF3 (after 60 frames), received 0x3DF5.
pub const MSG_PT_SEND_OK: i32 = 0x3DF4;
pub const MSG_PT_SEND_FAIL: i32 = 0x3DF1;
pub const MSG_PT_NO_GBA: i32 = 0x3DF3;
pub const MSG_PT_RECEIVED: i32 = 0x3DF5;

// ---------------------------------------------------------------------------
// Title logo (ac_animal_logo)
// ---------------------------------------------------------------------------

/// Logo fly-in length (keyframes, 60 Hz / 50 Hz builds), played at 0.5.
pub fn logo_in_frames(pal50: bool) -> f32 {
    if pal50 {
        101.0
    } else {
        121.0
    }
}
pub const LOGO_BACK_FADEIN_RATE: i32 = 20;
pub const LOGO_BACK_FADEIN_MAX: i32 = 220;
pub const LOGO_COPYRIGHT_ALPHA_RATE: i32 = 63;

/// `aAL_game_start_wait` "PRESS START" pulse: returns (timer, opacity).
/// Rises over 50 frames per half-wave, falls over 22.
pub fn press_start_pulse(timer: i16) -> (i16, f32) {
    let step = (32768.0 / if timer > 0 { 50.0f32 } else { 22.0 }) as i16;
    let t = timer.wrapping_add(step);
    let o = (127.5 * crate::game_math::sin_s(t) + 127.5).clamp(0.0, 255.0);
    (t, o)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ecard_and_logo() {
        let mut buf = [0u8; 256];
        buf[248] = 70;
        buf[249..251].copy_from_slice(&0x1234u16.to_be_bytes());
        buf[251..253].copy_from_slice(&0xD06Bu16.to_be_bytes());
        buf[253] = 99;
        buf[254..256].copy_from_slice(&7u16.to_be_bytes());
        let l = parse_ecard_letter(&buf, false).unwrap();
        assert_eq!((l.paper_type, l.present, l.header_back_start, l.card_no), (6, 0x1234, 23, 7));
        assert!(!l.password_type);
        assert!(parse_ecard_letter(&buf, true).unwrap().password_type);
        buf[251] = 0x10;
        assert!(parse_ecard_letter(&buf, false).is_none());
        let (t, o) = press_start_pulse(0);
        assert_eq!(t, 1489);
        assert!(o > 127.5);
    }

    #[test]
    fn mailbox() {
        // Box facing 0x4000 -> front at 0x2000; player straight in front.
        assert_eq!(mailbox_try_open(0x4000, 0x2000, true, true, true, false, 0, true, true, false), MailboxOpen::Open);
        assert_eq!(mailbox_try_open(0x4000, 0x2000, true, true, true, true, 2, true, true, false), MailboxOpen::Resetti);
        assert_eq!(mailbox_try_open(0x4000, 0x2000, true, true, true, true, 3, true, true, false), MailboxOpen::Open);
        assert_eq!(mailbox_try_open(0x4000, 0x2000, true, true, true, true, 3, true, true, true), MailboxOpen::Resetti);
        assert_eq!(mailbox_try_open(0x4000, 0x6000, true, true, true, false, 0, true, true, false), MailboxOpen::None);
        assert_eq!(mailbox_flag(2, false), (true, false));
    }

    #[test]
    fn flag_curves_are_continuous() {
        assert_eq!(flag_up_height(0), FLAG_BOTTOM);
        let a = flag_up_height(15);
        let b = FLAG_MIDDLE + (FLAG_MIDDLE - FLAG_BOTTOM) / (55.0 * (15.0 - 55.0)) * 1600.0;
        assert!((a - b).abs() < 1e-3);
        assert!((flag_up_height(55) - FLAG_MIDDLE).abs() < 1e-4);
        assert!((flag_up_height(100) - FLAG_TOP).abs() < 1e-4);
        assert_eq!(flag_down_height(0), FLAG_TOP);
        assert!((flag_down_height(40) - FLAG_BOTTOM).abs() < 1e-4);
    }

    #[test]
    fn raffle_ball_stays_in_tray() {
        let mut b = RaffleBall::spawn([0.0, 0.0, 0.0], [10.0, 10.0, 0.0], 0.5, 0.5, 0.5);
        b.step(true);
        for _ in 0..600 {
            b.step(false);
            assert!(b.pos[0] <= 20.0 && b.pos[0] >= -20.0 && b.pos[2].abs() <= 7.0 && b.pos[1] >= 2.0 - 1e-4);
        }
    }

    #[test]
    fn fork_probes_and_signs() {
        // Facing +Z (angle 0): one step north, then sector +2 (diagonal), then sector -2.
        assert_eq!(fork_probe_units(5, 5, 0), [(5, 6), (6, 7), (5, 8)]);
        let (mut bx, mut bz, mut ux, mut uz) = (1, 1, -1, 16);
        assert!(normalize_block_unit(&mut bx, &mut bz, &mut ux, &mut uz, 5, 6));
        assert_eq!((bx, bz, ux, uz), (0, 2, 15, 0));
        let mut w = SignWobble { amplitude: 0.01, phase: 0, freq: 2000 };
        let mut frames = 0;
        while !w.step().2 {
            frames += 1;
            assert!(frames < 10000);
        }
        assert_eq!(reserve_message(7), (11060, true));
        assert_eq!(reserve_message(0x42), (11074, false));
    }
}
