//! Submenu framework (`m_submenu.c`, `m_submenu_ovl.c`): when a menu may
//! open, the WAIT -> PREWAIT -> LINKWAIT -> PLAY -> END lifecycle, which
//! sub-overlays each menu loads, slide-in/out motion, the menu return
//! chain, held-input repeat, and the per-open-mode pocket eligibility rules
//! (`mSM_check_open_inventory_itemlist`).
//!
//! Source-verified against ac-decomp 09ca8e8b (primary) and ACGC-PC-Port
//! (secondary). Item ranges/categories verified with mingw32 gcc over
//! `m_name_table.h`. Overlay linking, prerender, audio, player restore,
//! object-bank copies and drawing stay in C.
//!
//! ## Opening (`mSM_submenu_ctrl`)
//! - Only in WAIT, with no screen fade or wipe running.
//! - START or Y -> inventory; X -> map when the town-map flag is set, not
//!   in the island cottages and not standing in an ocean acre. Both need
//!   no reset, no START-refuse flag/timer, a player action that allows
//!   menus and no first-intro.
//! - Otherwise, a pending A press facing within 45 degrees of north at a
//!   message board opens the notice board, at a map board the map (param
//!   0; the X-button map uses param 1). This path **skips the first-intro
//!   check**.
//! - Any `menu_type` set by NPCs/objects (`mSM_open_submenu*` only records
//!   the type and four params) is picked up the same frame: PREWAIT +
//!   prerender init, regardless of the button gates.
//! - LINKWAIT fills the 15-entry selection buffer with (EMPTY_NO, slot 15)
//!   and sets `after_mode` 7. END waits 2 frames and refuses START for 1.
//!
//! ## Overlay framework
//! - `OVL_FLAGS`: which menus pull in the item drawer, tag, hand and gyroid
//!   portrait sub-overlays.
//! - `START_DATA`: each menu slides in from +-300 on one axis at speed 75
//!   (inventory from the right; card-mail inventory and needlework mode 2
//!   from the left).
//! - `move_menu`: every other frame the speed is multiplied (x2 out, x0.5
//!   in, clamped 1..75); position advances by half the speed per frame and
//!   is truncated to an integer.
//! - Directions: odd = moving in (-> PLAY), even = moving out (-> END).
//! - Return chain: a closing menu hands its predecessor to its successor if
//!   one was opened from it; otherwise control returns to the predecessor,
//!   and closing the first menu ends the submenu.
//! - Input repeat: held C-directions + triggers, plus the stick (> 0.5) as a
//!   C-direction by 90-degree sector. A new input fires at once, then
//!   suppresses 25 frames; a held one then fires every 4th frame (timer 3).
//!   PC also folds the D-pad in and ticks by dt.
//!
//! ## Eligibility (`mSM_check_item_for_*`)
//! Modes without a checker return 0xFFFF (bit 15 set too). "Usual
//! exclusions" = exercise cards 0x2523-0x252F and the knife and fork
//! 0x2530; spirits are 0x2D28-0x2D2C.
//! - Gyroid entrust: **empty slots allowed**; else normal, not money, no
//!   usual exclusions, not spirits.
//! - Quest: any non-empty item (any condition), no usual exclusions, not
//!   spirits.
//! - Sell / give: normal condition, no usual exclusions, not spirits;
//!   sell also excludes money.
//! - Take: as give, and param 1 = fish only, 2 = bugs only.
//! - Put in furniture: normal ITEM1 items except fish, bugs, turnips and the
//!   usual exclusions.
//! - Music player: normal ITEM1 music (MINIDISK) only.
//! - Shrine: quest-condition items that are overdue.
//! - Exchange: normal, not the signboard, no usual exclusions; a fish
//!   offered for a non-fish needs water within 120 units ahead (the check
//!   also stores `water_pos`).
//! - Curator: normal, no usual exclusions (spirits and money pass). PC
//!   PC_ENHANCEMENTS narrows it to donatable items and fossils (non-retail).
//!
//! ## Corrections to the brief
//! - Board/sign A-presses bypass the first-intro gate; NPC requests bypass
//!   all button gates.
//! - `wait_timer` is decremented in WAIT but nothing in `m_submenu.c` reads
//!   it.

/// `enum submenu_overlay`.
pub mod ovl {
    pub const NONE: u8 = 0;
    pub const INVENTORY: u8 = 1;
    pub const HBOARD: u8 = 2;
    pub const TIMEIN: u8 = 3;
    pub const LEDIT: u8 = 4;
    pub const MAP: u8 = 5;
    pub const NOTICE: u8 = 6;
    pub const REPAY: u8 = 7;
    pub const MSCORE: u8 = 8;
    pub const BIRTHDAY: u8 = 9;
    pub const EDITOR: u8 = 10;
    pub const MAILBOX: u8 = 11;
    pub const BOARD: u8 = 12;
    pub const ADDRESS: u8 = 13;
    pub const HANIWA: u8 = 14;
    pub const EDITENDCHK: u8 = 15;
    pub const WARNING: u8 = 16;
    pub const CPMAIL: u8 = 17;
    pub const CPWARNING: u8 = 18;
    pub const CPEDIT: u8 = 19;
    pub const CATALOG: u8 = 20;
    pub const MUSIC: u8 = 21;
    pub const BANK: u8 = 22;
    pub const NEEDLEWORK: u8 = 23;
    pub const CPORIGINAL: u8 = 24;
    pub const DESIGN: u8 = 25;
    pub const GBA: u8 = 26;
    pub const DIARY: u8 = 27;
    pub const CALENDAR: u8 = 28;
    pub const PASSWORDMAKE: u8 = 29;
    pub const PASSWORDCHK: u8 = 30;
    pub const NUM: usize = 31;
}

/// `mSM_IV_OPEN_*`.
pub mod iv_open {
    pub const NORMAL: u8 = 0;
    pub const MAILBOX: u8 = 1;
    pub const HANIWA_ENTRUST: u8 = 2;
    pub const HANIWA_TAKE: u8 = 3;
    pub const QUEST: u8 = 4;
    pub const SELL: u8 = 5;
    pub const GIVE: u8 = 6;
    pub const SEND_MAIL: u8 = 7;
    pub const TAKE: u8 = 8;
    pub const PUTIN_FTR: u8 = 9;
    pub const MINIDISK: u8 = 10;
    pub const SHRINE: u8 = 11;
    pub const OPEN_12: u8 = 12;
    pub const EXCHANGE: u8 = 13;
    pub const CPMAIL: u8 = 14;
    pub const CURATOR: u8 = 15;
    pub const PASSWORD: u8 = 16;
    pub const NUM: usize = 17;
}

/// `mSM_PROCESS_*`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Process {
    Wait,
    PreWait,
    LinkWait,
    Play,
    End,
}

pub const POCKETS: usize = 15;
pub const ALL_SLOTS: u16 = 0xFFFF;
pub const AFTER_MODE_DEFAULT: u8 = 7;
pub const END_WAIT_FRAMES: i32 = 2;
pub const END_START_REFUSE_FRAMES: u8 = 1;

/// Button-driven open request (`mSM_submenu_ctrl`).
#[derive(Clone, Copy, Debug, Default)]
pub struct OpenInputs {
    pub start_or_y: bool,
    /// X pressed, map flag set, not a cottage, not an ocean acre.
    pub map_ok: bool,
    pub reset: bool,
    pub start_refuse: bool,
    pub start_refuse_timer: u8,
    pub player_allows_menu: bool,
    pub first_intro: bool,
    /// A pressed while facing within 45 degrees of north.
    pub a_facing_north: bool,
    pub message_board_ahead: bool,
    pub map_board_ahead: bool,
}

/// Returns (menu, param0) to open, if any.
pub fn button_open(i: &OpenInputs) -> Option<(u8, i32)> {
    let gates = !i.reset && !i.start_refuse && i.start_refuse_timer == 0 && i.player_allows_menu;
    if (i.start_or_y || i.map_ok) && gates && !i.first_intro {
        return Some(if i.start_or_y { (ovl::INVENTORY, 0) } else { (ovl::MAP, 1) });
    }
    if i.a_facing_north && gates {
        if i.message_board_ahead {
            return Some((ovl::NOTICE, 0));
        }
        if i.map_board_ahead {
            return Some((ovl::MAP, 0));
        }
    }
    None
}

/// `ABS((s16)(rot_y + 0x8000)) < DEG2SHORT_ANGLE(45)` (= 8192).
pub fn facing_north(rot_y: i16) -> bool {
    (rot_y.wrapping_sub(i16::MIN) as i32).abs() < 8192
}

/// One `mSM_submenu_move` step for the process machine. `prerender_done`
/// = mode >= PRERENDER_DONE; `overlay_closed` = the overlay asked to end.
pub fn process_step(p: Process, menu_requested: bool, can_poll: bool, prerender_done: bool, overlay_closed: bool) -> Process {
    match p {
        Process::Wait if menu_requested && can_poll => Process::PreWait,
        Process::PreWait if prerender_done => Process::LinkWait,
        Process::LinkWait => Process::Play,
        Process::Play if overlay_closed => Process::End,
        Process::End => Process::Wait,
        _ => p,
    }
}

/// Sub-overlays (`flg_table`): (item drawer, tag, hand, gyroid portrait).
pub const OVL_FLAGS: [u8; ovl::NUM] = [0, 0xE, 0x10, 0, 0, 0, 0, 0, 0, 0, 0, 0xE, 0, 0, 0x1E, 0, 0, 0xE, 0, 0xC, 0xC, 0xE, 0, 0xC, 0, 0, 0, 0, 0, 0, 0];
pub const FLAG_USE_ITEM: u8 = 1 << 1;
pub const FLAG_USE_TAG: u8 = 1 << 2;
pub const FLAG_USE_HAND: u8 = 1 << 3;
pub const FLAG_USE_HANIWA_PORTRAIT: u8 = 1 << 4;

/// Slide-in start (pos_x, pos_y, speed_x, speed_y) per menu.
pub const START_DATA: [(f32, f32, f32, f32); ovl::NUM] = [
    (0.0, 0.0, 0.0, 0.0),
    (300.0, 0.0, 75.0, 0.0),
    (0.0, 300.0, 0.0, 75.0),
    (0.0, 300.0, 0.0, 75.0),
    (0.0, 300.0, 0.0, 75.0),
    (0.0, 300.0, 0.0, 75.0),
    (0.0, 300.0, 0.0, 75.0),
    (0.0, 300.0, 0.0, 75.0),
    (0.0, 300.0, 0.0, 75.0),
    (0.0, 300.0, 0.0, 75.0),
    (0.0, -300.0, 0.0, 75.0),
    (-300.0, 0.0, 75.0, 0.0),
    (0.0, 300.0, 0.0, 75.0),
    (0.0, 0.0, 0.0, 0.0),
    (-300.0, 0.0, 75.0, 0.0),
    (0.0, -300.0, 0.0, 75.0),
    (0.0, 0.0, 0.0, 0.0),
    (300.0, 0.0, 75.0, 0.0),
    (0.0, 0.0, 0.0, 0.0),
    (0.0, 300.0, 0.0, 75.0),
    (300.0, 0.0, 75.0, 0.0),
    (-300.0, 0.0, 75.0, 0.0),
    (0.0, 300.0, 0.0, 75.0),
    (300.0, 0.0, 75.0, 0.0),
    (300.0, 0.0, 75.0, 0.0),
    (0.0, 300.0, 0.0, 75.0),
    (-300.0, 0.0, 75.0, 0.0),
    (-300.0, 0.0, 75.0, 0.0),
    (0.0, 300.0, 0.0, 75.0),
    (0.0, 300.0, 0.0, 75.0),
    (0.0, 300.0, 0.0, 75.0),
];

/// `mSM_set_new_start_data` x-override.
pub fn start_pos(menu: u8, param0: i32) -> (f32, f32) {
    let d = START_DATA[menu as usize];
    let left = (menu == ovl::INVENTORY && param0 == iv_open::CPMAIL as i32) || (menu == ovl::NEEDLEWORK && param0 == 2);
    (if left { -300.0 } else { d.0 }, d.1)
}

/// `mSM_MOVE_*`: odd = in (-> PLAY), even = out (-> END).
pub fn move_dir_opens(dir: i16) -> bool {
    dir & 1 != 0
}

/// `mSM_move_menu`. Returns true on arrival.
pub fn move_menu(pos: &mut f32, speed: &mut f32, flag: &mut bool, speed_mult: f32, p0: f32, p1: f32, p2: f32) -> bool {
    if !*flag {
        if p2 * (*pos - p0) >= 0.0 {
            *speed = (*speed * speed_mult).clamp(1.0, 75.0);
        }
        *flag = true;
    } else {
        *flag = false;
    }
    *pos += *speed * p2 * 0.5;
    if p2 * (*pos - p1) > 0.0 {
        *flag = false;
        *pos = p1;
        return true;
    }
    false
}

/// `mSM_move_Move` for direction `dir` (axis = dir >> 2, sign from bit 1).
/// Returns true on arrival; the position is truncated each frame.
pub fn move_step(pos: &mut [f32; 2], speed: &mut [f32; 2], flag: &mut bool, dir: i16) -> bool {
    const DATA: [(f32, f32, f32, f32); 2] = [(2.0, 0.0, 300.0, 1.0), (0.5, 120.0, 0.0, -1.0)];
    let idx = (dir >> 2) as usize;
    let d = DATA[(dir & 1) as usize];
    let s = if dir & 2 != 0 { -1.0 } else { 1.0 };
    let r = move_menu(&mut pos[idx], &mut speed[idx], flag, d.0, d.1 * s, d.2 * s, d.3 * s);
    pos[idx] = pos[idx] as i32 as f32;
    r
}

/// C-direction bits (`BUTTON_CRIGHT/CUP/CLEFT/CDOWN`).
pub const ADD_TRIGGER: [u32; 4] = [0x0001, 0x0008, 0x0002, 0x0004];

/// `mSM_make_trigger_data` repeat state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Repeat {
    pub last: u32,
    pub timer: i16,
}

impl Repeat {
    /// `held_c` = `getButton() & 0xF`, `trig` = new presses, stick as
    /// (magnitude, u16 angle). Returns the effective trigger this frame.
    pub fn step(&mut self, held_c: u32, trig: u32, stick: Option<(f32, u16)>) -> u32 {
        let mut t = (held_c & 0xF) | trig;
        if let Some((mag, ang)) = stick {
            if mag > 0.5 {
                // DEG2SHORT_ANGLE2(45) = 8192, (90) = 16384.
                t |= ADD_TRIGGER[(ang.wrapping_add(8192) / 16384) as usize];
            }
        }
        if t == self.last {
            if self.timer > 0 {
                self.timer -= 1;
                t = 0;
            } else {
                self.timer = 3;
            }
        } else {
            self.last = t;
            self.timer = 25;
        }
        t
    }
}

/// `mSM_tex_move`: background scroll.
pub fn tex_move(p: &mut [f32; 2]) {
    for v in p.iter_mut() {
        *v += 0.3535;
        while *v >= 1024.0 {
            *v -= 1024.0;
        }
    }
}

/// Menu chain links (`menu_info[].pre/next_menu_type`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Link {
    pub pre: u8,
    pub next: u8,
}

/// `mSM_return_func` outcome.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Return {
    /// Spliced out: `next` now returns to `pre`.
    Spliced,
    /// Control goes back to this menu.
    Back(u8),
    /// First menu closed: submenu END.
    EndSubmenu,
}

pub fn return_func(links: &mut [Link; ovl::NUM], closing: u8) -> Return {
    let l = links[closing as usize];
    let r = if l.next != ovl::NONE {
        links[l.next as usize].pre = l.pre;
        links[l.pre as usize].next = l.next;
        Return::Spliced
    } else if l.pre == ovl::NONE {
        Return::EndSubmenu
    } else {
        links[l.pre as usize].next = ovl::NONE;
        Return::Back(l.pre)
    };
    links[closing as usize] = Link::default();
    r
}

// ---------------------------------------------------------------------------
// Eligibility
// ---------------------------------------------------------------------------

pub const COND_NORMAL: u8 = 0;
pub const COND_PRESENT: u8 = 1;
pub const COND_QUEST: u8 = 2;

/// gcc-verified ids/categories.
pub mod ids {
    pub const EMPTY: u16 = 0x0000;
    pub const CARD00: u16 = 0x2523;
    pub const CARD12: u16 = 0x252F;
    pub const KNIFE_AND_FORK: u16 = 0x2530;
    pub const SPIRIT0: u16 = 0x2D28;
    pub const SPIRIT4: u16 = 0x2D2C;
    pub const SIGNBOARD: u16 = 0x251E;
    pub const TYPE_ITEM1: u16 = 2;
    pub const CAT_MONEY: u16 = 1;
    pub const CAT_FISH: u16 = 3;
    pub const CAT_MINIDISK: u16 = 10;
    pub const CAT_INSECT: u16 = 13;
    pub const CAT_KABU: u16 = 15;
}

pub fn item_type(i: u16) -> u16 {
    (i & 0xF000) >> 12
}
pub fn item_cat(i: u16) -> u16 {
    (i & 0x0F00) >> 8
}
fn is_item1_cat(i: u16, cat: u16) -> bool {
    item_type(i) == ids::TYPE_ITEM1 && item_cat(i) == cat
}
fn usual_excluded(i: u16) -> bool {
    (ids::CARD00..=ids::CARD12).contains(&i) || i == ids::KNIFE_AND_FORK
}
fn spirit(i: u16) -> bool {
    (ids::SPIRIT0..=ids::SPIRIT4).contains(&i)
}

/// Per-slot inputs for the checkers.
#[derive(Clone, Copy, Debug, Default)]
pub struct Slot {
    pub item: u16,
    pub cond: u8,
    /// Shrine: `mQst_CheckLimitbyPossessionIdx`.
    pub overdue: bool,
}

/// `mSM_check_item_for_*`. `water_ahead` answers the exchange water search.
pub fn slot_eligible(mode: u8, s: &Slot, param: i32, water_ahead: &mut impl FnMut() -> bool) -> bool {
    use ids::*;
    let i = s.item;
    let normal = s.cond == COND_NORMAL;
    match mode {
        iv_open::HANIWA_ENTRUST => i == EMPTY || (normal && !is_item1_cat(i, CAT_MONEY) && !usual_excluded(i) && !spirit(i)),
        iv_open::QUEST => i != EMPTY && !usual_excluded(i) && !spirit(i),
        iv_open::SELL => i != EMPTY && normal && !is_item1_cat(i, CAT_MONEY) && !usual_excluded(i) && !spirit(i),
        iv_open::GIVE => i != EMPTY && normal && !usual_excluded(i) && !spirit(i),
        iv_open::TAKE => {
            let cat = item_cat(i);
            i != EMPTY
                && normal
                && !usual_excluded(i)
                && !spirit(i)
                && (param == 0 || (item_type(i) == TYPE_ITEM1 && ((cat == CAT_FISH && param == 1) || (cat == CAT_INSECT && param == 2))))
        }
        iv_open::PUTIN_FTR => {
            let cat = item_cat(i);
            i != EMPTY && normal && item_type(i) == TYPE_ITEM1 && cat != CAT_FISH && cat != CAT_KABU && cat != CAT_INSECT && !usual_excluded(i)
        }
        iv_open::MINIDISK => normal && is_item1_cat(i, CAT_MINIDISK),
        iv_open::SHRINE => s.cond == COND_QUEST && s.overdue,
        iv_open::EXCHANGE => {
            if i == EMPTY || !normal || i == SIGNBOARD || usual_excluded(i) {
                return false;
            }
            let exchange_id = param as u16;
            if is_item1_cat(i, CAT_FISH) && !is_item1_cat(exchange_id, CAT_FISH) {
                return water_ahead();
            }
            true
        }
        iv_open::CURATOR => i != EMPTY && normal && !usual_excluded(i),
        _ => true,
    }
}

/// Modes with a checker (`check_process[]` non-NULL).
pub fn has_checker(mode: u8) -> bool {
    matches!(
        mode,
        iv_open::HANIWA_ENTRUST
            | iv_open::QUEST
            | iv_open::SELL
            | iv_open::GIVE
            | iv_open::TAKE
            | iv_open::PUTIN_FTR
            | iv_open::MINIDISK
            | iv_open::SHRINE
            | iv_open::EXCHANGE
            | iv_open::CURATOR
    )
}

/// `mSM_check_open_inventory_itemlist`.
pub fn open_itemlist_mask(mode: u8, slots: &[Slot; POCKETS], param: i32, water_ahead: &mut impl FnMut() -> bool) -> u16 {
    if !has_checker(mode) {
        return ALL_SLOTS;
    }
    let mut m = 0u16;
    for (i, s) in slots.iter().enumerate() {
        if slot_eligible(mode, s, param, water_ahead) {
            m |= 1 << i;
        }
    }
    m
}

/// `mSM_COLLECT_INSECT/FISH_GET` bit index in `furniture_collected_bitfield`.
pub fn insect_collect_bit(idx: u32) -> u32 {
    0x2F2 + idx
}
pub fn fish_collect_bit(idx: u32) -> u32 {
    0x31A + idx
}
pub const COLLECT_NUM: u32 = 40;

/// `mSM_CHECK_LAST_*_GET`: this catch completes the 40-entry page.
pub fn is_last_collect(collected: u32, this_collected: bool) -> bool {
    collected == COLLECT_NUM - 1 && !this_collected
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opening_rules() {
        let base = OpenInputs { player_allows_menu: true, ..Default::default() };
        assert_eq!(button_open(&OpenInputs { start_or_y: true, ..base }), Some((ovl::INVENTORY, 0)));
        assert_eq!(button_open(&OpenInputs { map_ok: true, ..base }), Some((ovl::MAP, 1)));
        assert_eq!(button_open(&OpenInputs { start_or_y: true, first_intro: true, ..base }), None);
        let board = OpenInputs { a_facing_north: true, message_board_ahead: true, first_intro: true, ..base };
        assert_eq!(button_open(&board), Some((ovl::NOTICE, 0)));
        assert_eq!(button_open(&OpenInputs { start_refuse_timer: 1, ..board }), None);
        assert!(facing_north(i16::MIN) && facing_north(i16::MIN + 8191) && !facing_north(0));
        assert_eq!(process_step(Process::Wait, true, true, false, false), Process::PreWait);
        assert_eq!(process_step(Process::PreWait, true, true, false, false), Process::PreWait);
        assert_eq!(process_step(Process::End, false, true, false, false), Process::Wait);
    }

    #[test]
    fn framework_tables_and_motion() {
        assert_eq!(OVL_FLAGS[ovl::INVENTORY as usize], FLAG_USE_ITEM | FLAG_USE_TAG | FLAG_USE_HAND);
        assert_eq!(OVL_FLAGS[ovl::HANIWA as usize] & FLAG_USE_HANIWA_PORTRAIT, FLAG_USE_HANIWA_PORTRAIT);
        assert_eq!(start_pos(ovl::INVENTORY, 0), (300.0, 0.0));
        assert_eq!(start_pos(ovl::INVENTORY, iv_open::CPMAIL as i32), (-300.0, 0.0));
        let (mut pos, mut sp, mut fl) = ([300.0f32, 0.0], [75.0f32, 0.0], false);
        // MOVE_IN_RIGHT (1): from 300 toward 0.
        let mut frames = 0;
        while !move_step(&mut pos, &mut sp, &mut fl, 1) {
            frames += 1;
            assert!(frames < 200);
        }
        assert_eq!(pos[0], 0.0);
        assert!(move_dir_opens(1) && !move_dir_opens(0));
        let mut links = [Link::default(); ovl::NUM];
        links[ovl::INVENTORY as usize] = Link { pre: ovl::NONE, next: ovl::BOARD };
        links[ovl::BOARD as usize] = Link { pre: ovl::INVENTORY, next: ovl::NONE };
        assert_eq!(return_func(&mut links, ovl::BOARD), Return::Back(ovl::INVENTORY));
        assert_eq!(return_func(&mut links, ovl::INVENTORY), Return::EndSubmenu);
    }

    #[test]
    fn input_repeat() {
        let mut r = Repeat::default();
        let mut fired = vec![];
        for f in 0..40 {
            if r.step(0x1, 0, None) != 0 {
                fired.push(f);
            }
        }
        assert_eq!(fired, [0, 26, 30, 34, 38]);
        let mut r2 = Repeat::default();
        assert_eq!(r2.step(0, 0, Some((0.9, 16384))), 0x0008);
        assert_eq!(Repeat::default().step(0, 0, Some((0.9, 65000))), 0x0001);
        let mut t = [1023.9f32, 0.0];
        tex_move(&mut t);
        assert!(t[0] < 1.0);
    }

    #[test]
    fn eligibility() {
        let mut no_water = || false;
        let s = |item, cond| Slot { item, cond, overdue: false };
        assert!(slot_eligible(iv_open::HANIWA_ENTRUST, &s(0, 0), 0, &mut no_water));
        assert!(!slot_eligible(iv_open::SELL, &s(0x2100, 0), 0, &mut no_water)); // money
        assert!(slot_eligible(iv_open::QUEST, &s(0x2400, COND_PRESENT), 0, &mut no_water));
        assert!(!slot_eligible(iv_open::GIVE, &s(0x2D28, 0), 0, &mut no_water));
        assert!(slot_eligible(iv_open::CURATOR, &s(0x2D28, 0), 0, &mut no_water));
        assert!(slot_eligible(iv_open::TAKE, &s(0x2300, 0), 1, &mut no_water));
        assert!(!slot_eligible(iv_open::TAKE, &s(0x2D00, 0), 1, &mut no_water));
        assert!(!slot_eligible(iv_open::PUTIN_FTR, &s(0x2F00, 0), 0, &mut no_water)); // kabu
        assert!(!slot_eligible(iv_open::PUTIN_FTR, &s(0x1000, 0), 0, &mut no_water)); // furniture type
        assert!(slot_eligible(iv_open::MINIDISK, &s(0x2A00, 0), 0, &mut no_water));
        assert!(!slot_eligible(iv_open::EXCHANGE, &s(0x2300, 0), 0x2200, &mut no_water));
        assert!(slot_eligible(iv_open::EXCHANGE, &s(0x2300, 0), 0x2301, &mut no_water));
        assert!(!slot_eligible(iv_open::EXCHANGE, &s(0x251E, 0), 0, &mut no_water));
        let slots = [s(0x2400, 0); POCKETS];
        assert_eq!(open_itemlist_mask(iv_open::NORMAL, &slots, 0, &mut no_water), 0xFFFF);
        assert_eq!(open_itemlist_mask(iv_open::GIVE, &slots, 0, &mut no_water), 0x7FFF);
        assert_eq!((insect_collect_bit(0), fish_collect_bit(39)), (0x2F2, 0x341));
        assert!(is_last_collect(39, false) && !is_last_collect(39, true));
    }
}
