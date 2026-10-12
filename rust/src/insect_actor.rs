//! Insect actors: the shared controller (`ac_insect.c`,
//! `_move`, `_clip`, `_draw`), the type→program
//! dispatch (`ac_insect_data.c_inc`), and the 14
//! behavior programs (`ac_ins_*.c`).
//!
//! Verified against GAFE01_00 Rev 0 USA (see
//! `insect-verification-report.md`; 12/12 sections
//! PROVEN, with a few brief corrections noted
//! below). The 14 `ac_ins_*.c` files are 14 behavior
//! programs shared by 41 insect types, not
//! one-species implementations: every insect shares
//! `aINS_position_move`; the per-type `mv_proc` is
//! the behavior program.
//!
//! This module ports the pure kernels: the shared
//! pool constants, the type→program table, the
//! catch-range rules, the stress-band and patience
//! math, the fade/alpha rules, and the despawn
//! predicate. The 14 per-family state machines,
//! movement/rendering/collision, the stress actor
//! iteration, lifetime wiring, catch registration,
//! and inventory handoff stay in C.
//!
//! Brief corrections carried here: butterfly/mosquito
//! spawn is background height − 30.0f (brief said
//! +30.0f); the chou habitat check rejects units
//! whose high id digit is 5 (0x5xxx = houses/shops),
//! not generic non-plantable items; the spider is
//! MINO-only — dango's SPIDER case is unreachable
//! dead code; the facing-gated catch group includes
//! the 29–31 beetles; the net-catch special case
//! lives in the cull check, not the registration
//! path; stress decay is dt-scaled but stress growth
//! is not; the 600.0f cull distance (a stale "15
//! units" comment in the decomp is wrong).

/// Pooled insect slots; `aINS_MAKE_NEW` searches
/// slots 0–7, `aINS_MAKE_EXIST` uses only slot 8.
pub const INSECT_ACTOR_NUM: i32 = 9;
/// Default lifetime in frames.
pub const INSECT_LIFETIME: i32 = 216000;
/// Stress radius multiplier: 3 * unit size.
pub const MAX_STRESS_DIST_MULT: f32 = 3.0;
/// Patience step.
pub const PATIENCE_STEP: f32 = 0.5;
/// s16 angle within 90 degrees.
pub const FACING_90_S16: i32 = 16384;
/// Normal-insect cull distance.
pub const CULL_DIST: f32 = 600.0;

/// Insect type ids (`ac_insect_h.h`; sequential
/// 0–40, SPIRIT = NUM = 40, NONE = 41).
pub const T_COMMON_BUTTERFLY: i32 = 0;
pub const T_YELLOW_BUTTERFLY: i32 = 1;
pub const T_TIGER_BUTTERFLY: i32 = 2;
pub const T_PURPLE_BUTTERFLY: i32 = 3;
pub const T_ROBUST_CICADA: i32 = 4;
pub const T_BEE: i32 = 8;
pub const T_LONGHORN_BEETLE: i32 = 23;
pub const T_COCKROACH: i32 = 28;
pub const T_SAW_STAG_BEETLE: i32 = 29;
pub const T_GIANT_BEETLE: i32 = 31;
pub const T_ANT: i32 = 38;
pub const T_MOSQUITO: i32 = 39;
pub const T_SPIRIT: i32 = 40;
pub const T_NUM: i32 = 41;

/// Behavior program ids.
pub const P_CHOU: i32 = 0;
pub const P_BATTA: i32 = 1;
pub const P_TONBO: i32 = 2;
pub const P_TENTOU: i32 = 3;
pub const P_HOTARU: i32 = 4;
pub const P_SEMI: i32 = 5;
pub const P_KABUTO: i32 = 6;
pub const P_GOKI: i32 = 7;
pub const P_HITODAMA: i32 = 8;
pub const P_AMENBO: i32 = 9;
pub const P_KA: i32 = 10;
pub const P_DANGO: i32 = 11;
pub const P_KERA: i32 = 12;
pub const P_MINO: i32 = 13;
pub const P_NUM: i32 = 14;

/// Type → behavior program (`aINS_program_type` in
/// `ac_insect_data.c_inc`), index = type 0–40.
pub const PROGRAM_FOR_TYPE: [i32; 41] = [
    P_CHOU, P_CHOU, P_CHOU, P_CHOU, // 0–3 butterflies
    P_SEMI, P_SEMI, P_SEMI, P_SEMI, P_SEMI, // 4–8 cicadas + bee
    P_TONBO, P_TONBO, P_TONBO, P_TONBO, // 9–12 dragonflies
    P_BATTA, P_BATTA, P_BATTA, P_BATTA, P_BATTA, P_BATTA, // 13–18 locusts/crickets
    P_KABUTO, P_KABUTO, P_KABUTO, P_KABUTO, P_KABUTO, // 19–23 beetles
    P_TENTOU, P_TENTOU, P_TENTOU, // 24–26 flower insects
    P_HOTARU, // 27 firefly
    P_GOKI, // 28 cockroach
    P_KABUTO, P_KABUTO, P_KABUTO, // 29–31 beetles
    P_TENTOU, // 32 snail
    P_KERA, // 33 mole cricket
    P_AMENBO, // 34 pond skater
    P_MINO, // 35 bagworm
    P_DANGO, // 36 pill bug
    P_MINO, // 37 spider
    P_DANGO, // 38 ant
    P_KA, // 39 mosquito
    P_HITODAMA, // 40 spirit
];

/// `catch_ME_data` (`ac_insect_move.c_inc`), indexed
/// by type 0–40. Negative entries shrink the stress
/// radius.
pub const CATCH_ME_DATA: [f32; 41] = [
    0.0, 0.0, 0.0, 0.0, // 0–3 butterflies
    10.0, 10.0, 10.0, 0.0, 10.0, // 4–8 cicadas + bee
    0.0, 0.0, 0.0, 0.0, // 9–12 dragonflies
    0.0, // 13 long locust
    20.0, // 14 migratory locust
    -20.0, -20.0, -20.0, -20.0, // 15–18 crickets/grasshopper
    -20.0, -20.0, -20.0, 0.0, 0.0, // 19–23 beetles
    -20.0, -20.0, -20.0, // 24–26 ladybugs/mantis
    0.0, 0.0, // 27–28 firefly/cockroach
    0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, // 29–40 rest
];

/// Stress multiplier table.
pub const STRESS_TABLE: [f32; 5] = [0.3, 1.0, 2.0, 5.0, 10.0];

/// Type → behavior program. Out-of-range types map
/// to -1; the C dispatch indexes the table directly.
#[no_mangle]
pub extern "C" fn pc_insect_program_for_type(insect_type: i32) -> i32 {
    if insect_type < 0 || insect_type >= T_NUM {
        -1
    } else {
        PROGRAM_FOR_TYPE[insect_type as usize]
    }
}

/// Catch range (`aINS_get_catch_range`):
/// default 8.0; common/yellow butterflies 24.0
/// unconditionally; cicadas/bee and all 8 beetle
/// types get 24.0 only when the player's relative
/// angle is within 90 degrees (else 0.0); the
/// cockroach gets the same facing test only when
/// `flag == 4`. `d_angle_y` is
/// `ABS(world.angle.y − player_angle_y)` as an s16
/// (in i32 form).
#[no_mangle]
pub extern "C" fn pc_insect_catch_range(insect_type: i32, flag: i32, d_angle_y: i32) -> f32 {
    let facing = |d: i32| if d > FACING_90_S16 { 0.0 } else { 24.0 };
    match insect_type {
        T_COMMON_BUTTERFLY | T_YELLOW_BUTTERFLY => 24.0,
        4..=8 | 19..=23 | 29..=31 => facing(d_angle_y),
        T_COCKROACH => {
            if flag == 4 {
                facing(d_angle_y)
            } else {
                8.0
            }
        }
        _ => 8.0,
    }
}

/// Stress radius addition for a type.
#[no_mangle]
pub extern "C" fn pc_insect_catch_me_radius(insect_type: i32) -> f32 {
    if insect_type < 0 || insect_type >= T_NUM {
        0.0
    } else {
        CATCH_ME_DATA[insect_type as usize]
    }
}

/// Stress band index (`aINS_get_stress_sub`):
/// `((min_dist − unit) − max(0, dist − unit)) / 20`,
/// clamped to 0–4. The C source only clamps the top;
/// the bottom clamp is unreachable for valid inputs
/// and guards the table index.
#[no_mangle]
pub extern "C" fn pc_insect_stress_idx(min_dist: f32, dist: f32, unit: f32) -> i32 {
    let tmp0 = (dist - unit).max(0.0);
    let idx = (((min_dist - unit) - tmp0) / 20.0) as i32;
    idx.clamp(0, (STRESS_TABLE.len() - 1) as i32)
}

/// Stress from one nearby actor: the actor's
/// horizontal displacement × the band multiplier.
/// Only *moving* actors contribute; the caller takes
/// the max over the UNUSED/PLAYER/NPC/BG lists.
#[no_mangle]
pub extern "C" fn pc_insect_stress_calc(dx: f32, dz: f32, band: i32) -> f32 {
    let band = (band.clamp(0, (STRESS_TABLE.len() - 1) as i32)) as usize;
    (dx * dx + dz * dz).sqrt() * STRESS_TABLE[band]
}

/// Patience step (`aINS_calc_patience`): stress == 0
/// → decrease by `0.5 * dt`, clamped at 0; otherwise
/// increase by `stress * 0.5`, clamped at 100.
/// Note the dt asymmetry: decay is frame-rate scaled,
/// growth is not.
#[no_mangle]
pub extern "C" fn pc_insect_patience_step(patience: f32, stress: f32, dt: f32) -> f32 {
    if stress == 0.0 {
        (patience - PATIENCE_STEP * dt).max(0.0)
    } else {
        (patience + stress * PATIENCE_STEP).min(100.0)
    }
}

/// Fade alpha: most types lose 11 alpha per fade
/// frame from 255.
#[no_mangle]
pub extern "C" fn pc_insect_fade_alpha(fade_frames: i32) -> i32 {
    255 - fade_frames * 11
}

/// Fade done when alpha ≤ 0 → destroy.
#[no_mangle]
pub extern "C" fn pc_insect_fade_done(fade_frames: i32) -> bool {
    pc_insect_fade_alpha(fade_frames) <= 0
}

/// Firefly alpha path: `alpha2 = fade_frames * 11`.
#[no_mangle]
pub extern "C" fn pc_insect_firefly_alpha2(fade_frames: i32) -> i32 {
    fade_frames * 11
}

/// Firefly done when alpha2 ≥ 255 (sets
/// alpha0=alpha1=0 and destroys).
#[no_mangle]
pub extern "C" fn pc_insect_firefly_fade_done(fade_frames: i32) -> bool {
    pc_insect_firefly_alpha2(fade_frames) >= 255
}

/// Despawn predicate: released & invisible & not the
/// net-catch target → destroy; normal insects
/// destroyed when > 600.0 from the player in a
/// different acre (and not the net-catch target);
/// the destruct flag also skips destruction while
/// the insect is the net-catch target.
#[no_mangle]
pub extern "C" fn pc_insect_despawn_check(
    released: bool,
    visible: bool,
    net_target: bool,
    dist_to_player: f32,
    same_acre: bool,
    destruct_flag: bool,
) -> bool {
    if released && !visible && !net_target {
        return true;
    }
    if destruct_flag && !net_target {
        return true;
    }
    dist_to_player > CULL_DIST && !same_acre && !net_target
}

/// Per-type shadow size from the shared setup
/// function: 8.0 for tiger/purple butterflies,
/// cicadas, bee, dragonflies; 4.0 for
/// bagworm/pill bug/spider/mosquito; 2.0 for ant;
/// 6.0 default.
#[no_mangle]
pub extern "C" fn pc_insect_shadow_size(insect_type: i32) -> f32 {
    match insect_type {
        T_TIGER_BUTTERFLY | T_PURPLE_BUTTERFLY => 8.0,
        4..=12 => 8.0,
        35..=37 | T_MOSQUITO => 4.0,
        T_ANT => 2.0,
        _ => 6.0,
    }
}

// ---------------------------------------------------------------------------
// Follow-ups resolved (claude, 2026-10-12)
// Sources: ac-decomp ac_insect.c / _clip / _move, all 14 ac_ins_*.c,
// ac_set_ovl_insect.c, m_player_main_notice_net.c_inc / pull_net,
// m_player_lib.c, plus the US symbol map. The state-machine table is
// generated by tools/gen_insect_fsm.py.
//
// - Overlay: `aINS_overlay` (.bss 0x5418 = 3 x `aINS_overlay_entry_c`
//   0x1C08) is written only by `aINS_actor_ct` (`_1C00 = PROGRAM_NUM`,
//   `_1C04 = 0`) and never read. It is vestigial N64 code-overlay
//   bookkeeping (a 0x1C00-byte slot per loaded behaviour program, tagged
//   with the program id), not model tables.
// - Spawn -> controller: `aSOI_ins_make_sub` fills `aINS_Init_c`
//   {type, unit centre (unit corner + half a unit), extra_data, game} and
//   calls `make_insect_proc(&init, FALSE)` = `aINS_MAKE_NEW` (slots 0-7).
//   Ants go to the deferred `make_ant_proc` instead. `extra_data` becomes
//   the actor's `actor_specific`: 0 normal, 1 released by the player
//   (`aINS_make_actor` -> `MAKE_EXIST`, slot 8). For cockroaches only:
//   4 on a tree, 6 on a spoiled turnip (0x2F03); goki's init branches on it.
// - Net handoff: every frame each insect without `bit_1` registers
//   (label = actor, position, range) with
//   `Set_Item_net_catch_request_table_proc`. The player publishes the
//   winning label; the matching program switches to its escape/held action
//   and every despawn path skips the labelled actor. The player then puts
//   `insect->item` (catch type 0), `ITM_INSECT08` (bee swarm) or
//   `ITM_INSECT38` (ant) in the pockets.
// ---------------------------------------------------------------------------

/// `aINS_overlay_entry_c` and the controller's overlay block (dead data).
pub const OVERLAY_ENTRY_SIZE: usize = 0x1C08;
pub const OVERLAY_ENTRY_DATA: usize = 0x1C00;
pub const OVERLAY_ENTRIES: usize = 3;
pub const OVERLAY_BSS_SIZE: usize = OVERLAY_ENTRIES * OVERLAY_ENTRY_SIZE;

/// `aINS_Init_c` spawn-parameter block size (header).
pub const INIT_STRUCT_SIZE: usize = 0x18;

/// `aINS_INSECT_ACTOR` / controller sizes (header).
pub const INSECT_ACTOR_SIZE: usize = 0x288;
pub const INSECT_CTRL_SIZE: usize = 0x183C;

/// `aINS_INIT_*` / `aSOI_SPAWN_*` values carried in `extra_data`.
pub const INIT_NORMAL: i32 = 0;
pub const INIT_RELEASE: i32 = 1;
pub const SPAWN_GOKI_TREE: i32 = 4;
pub const SPAWN_GOKI_ITEM: i32 = 6;
pub const ITM_KABU_SPOILED: u16 = 0x2F03;

/// `extra_data` the spawner gives an insect (`aSOI_ins_set_insect_data`).
pub fn spawn_extra_data(insect_type: i32, unit_is_tree: bool, unit_item: u16) -> i32 {
    if insect_type == T_COCKROACH {
        if unit_is_tree {
            return SPAWN_GOKI_TREE;
        }
        if unit_item == ITM_KABU_SPOILED {
            return SPAWN_GOKI_ITEM;
        }
    }
    INIT_NORMAL
}

/// Spawn position: the unit's world corner plus half a unit on x/z.
pub fn spawn_position(unit_corner_x: f32, unit_corner_z: f32) -> (f32, f32) {
    (unit_corner_x + 20.0, unit_corner_z + 20.0)
}

/// `aINS_searchRegistSpace`: NEW uses the first free of slots 0-7; EXIST
/// (released insects) uses only slot 8.
pub fn regist_space(exist: &[bool; 9], make_exist: bool) -> Option<usize> {
    if make_exist {
        (!exist[8]).then_some(8)
    } else {
        exist[..8].iter().position(|e| !e)
    }
}

/// Item ids (`ITM_INSECT_START` + n, gcc-verified header values).
pub const ITM_INSECT_START: u16 = 0x2D00;
pub const ITM_SPIRIT0: u16 = 0x2D28;
pub const ITM_SPIRIT4: u16 = 0x2D2C;
pub const ITM_INSECT_BEE: u16 = 0x2D08;
pub const ITM_INSECT_ANT: u16 = 0x2D26;
pub const T_SPIDER: i32 = 37;
pub const T_PILL_BUG: i32 = 36;

/// `insect->item` set by each program's init: `ITM_INSECT_START + type`
/// for every real type, `ITM_SPIRIT0` for the spirit. QUIRK: dango's
/// `case SPIDER: break;` would leave EMPTY_NO, but spiders only ever run
/// the MINO program (which gives `ITM_INSECT37`), so it is unreachable.
pub fn insect_item(insect_type: i32, program: i32) -> u16 {
    if insect_type == T_SPIRIT {
        ITM_SPIRIT0
    } else if insect_type == T_SPIDER && program == P_DANGO {
        0
    } else {
        ITM_INSECT_START + insect_type as u16
    }
}

/// Pocket item for a finished net catch (`Player_actor_setup_main_Notice_net`).
pub fn net_catch_item(catch_type: i8, insect_item: u16, insect_idx: i32) -> u16 {
    if catch_type == 0 {
        insect_item
    } else if insect_idx == T_BEE {
        ITM_INSECT_BEE
    } else {
        ITM_INSECT_ANT
    }
}

/// `mPlib_Get_space_putin_item_forHITODAMA`: a caught spirit (SPIRITn
/// counts n+1) stacks onto the first NORMAL pocket holding an item in
/// [SPIRIT0, SPIRIT0 + (5 - count)) (half-open, `mPr_GetPossessionItemIdxKindWithCond`,
/// so the stack never exceeds SPIRIT4); otherwise
/// it takes the first free slot. Returns (slot, item written).
pub fn spirit_putin(item: u16, pockets: &[(u16, bool)]) -> Option<(usize, u16)> {
    if (ITM_SPIRIT0..=ITM_SPIRIT4).contains(&item) {
        let count = 1 + (item - ITM_SPIRIT0);
        let end = ITM_SPIRIT0 + (5 - count);
        if let Some(i) = pockets.iter().position(|&(p, normal)| normal && (ITM_SPIRIT0..end).contains(&p)) {
            return Some((i, pockets[i].0 + count));
        }
    }
    pockets.iter().position(|&(p, _)| p == 0).map(|i| (i, item))
}

/// After a catch: the collection bit is set for everything but the spirit;
/// ants and mosquitoes get the arrow (`eEC_EFFECT_YAJIRUSHI`) effect.
pub fn catch_sets_collect_bit(insect_idx: i32) -> bool {
    insect_idx != T_SPIRIT
}
pub fn catch_shows_arrow(insect_idx: i32) -> bool {
    insect_idx == T_ANT || insect_idx == T_MOSQUITO
}
/// Catch fanfare: 0x4B if already collected, else 0x28.
pub fn catch_fanfare(already_collected: bool) -> u16 {
    if already_collected {
        0x4B
    } else {
        0x28
    }
}

/// Shared `insect_flags` / `flag` semantics (controller + all programs).
pub mod flags {
    /// Delete at the next cull check (unless held in the net).
    pub const DESTRUCT: u8 = 1 << 0;
    /// No catch request is registered (escaping / not catchable).
    pub const NO_CATCH: u8 = 1 << 1;
    /// Fleeing: wall check off, lifetime auto-escape suppressed.
    pub const FLEEING: u8 = 1 << 2;
    /// Lifetime (216000 frames) ran out.
    pub const LIFE_OVER: u8 = 1 << 3;
    /// Unique wall check enabled (cleared by batta, dango, kera, mino, tentou).
    pub const WALL_CHECK: u8 = 1 << 4;

    /// `aINS_set_catch_range`: register a catch request this frame.
    pub fn registers_catch(f: u8) -> bool {
        f & NO_CATCH == 0
    }
    /// `aINS_BGcheck`: `mCoBG_UniqueWallCheck` runs.
    pub fn wall_check(f: u8) -> bool {
        f & FLEEING == 0 && f & WALL_CHECK != 0
    }
    /// Programs' `actor_move`: lifetime expiry forces LET_ESCAPE.
    pub fn lifetime_escape(f: u8) -> bool {
        f & LIFE_OVER != 0 && f & FLEEING == 0
    }
    /// Goki is catchable at the special range only while `flag == 4`.
    pub const GOKI_CATCHABLE_FLAG: i32 = 4;
}

/// Per-family meaning of the generic work slots (decomp macro names where
/// they exist).
pub const WORK_SLOTS: [(&str, &str); 14] = [
    ("chou", "s32_work0 loop corner 0-3, s32_work1/2 target unit x/z, s32_work3 corner latch; f32_work0/1 orbit centre x/z, f32_work2/3 timers; flag += 0x800 wing phase"),
    ("batta", "s32_work0 jump-spread index 0-15, s32_work1 net-startled latch; f32_work2/3 block centre x/z; flag 2-frame settle counter"),
    ("tonbo", "f32_work0 set height, f32_work1 reserve height, f32_work2/3 block centre x/z; s32_work0 stop state, s32_work2 rest anime timer"),
    ("tentou", "s32_work0/3 spawn unit x/z, s32_work1 target angle, s32_work2 turn delay"),
    ("hotaru", "s32_work0 target angle, s32_work2 float angle, s32_work3 wall hit; f32_work0/1 target x/z, f32_work2/3 centre x/z; flag light on"),
    ("semi", "s32_work0 is caught; timer cry timer"),
    ("kabuto", "s32_work0-2 chase-angle timers, s32_work3 rot y"),
    ("goki", "s32_work0 target angle, s32_work1 change-wait timer, s32_work2 move timer, s32_work3 item pointer; flag bits 1/2/4 (4 = catchable)"),
    ("hitodama", "s32_work1 move angle add, s32_work2 move timer, s32_work3 angle; f32_work0/1 max x/z"),
    ("amenbo", "s32_work3 hit wall; f32_work0 scale, f32_work1 scale rate"),
    ("ka", "s32_work0 angle, s32_work1 attack timer; f32_work0 turn speed, f32_work1 speed"),
    ("dango", "f32_work2/3 block centre x/z"),
    ("kera", "s32_work0 avoid timer, s32_work1/2 unit x/z, s32_work3 scale toggle; f32_work0 dirt timer, f32_work2/3 block centre x/z"),
    ("mino", "s32_work0 current shake, s32_work1/2 tree unit x/z, s32_work3 target shake; f32_work0 anim time, f32_work1/2 shake pos x/y, f32_work3 base z"),
];

/// Behaviour-program state machine (see `PROGRAM_FSM`).
#[derive(Clone, Copy, Debug)]
pub struct ProgramFsm {
    pub family: &'static str,
    pub actions: &'static [&'static str],
    pub transitions: &'static [&'static [u8]],
    pub initial: &'static [u8],
    pub forced: &'static [u8],
}

impl ProgramFsm {
    pub fn action(&self, name: &str) -> Option<usize> {
        self.actions.iter().position(|a| *a == name)
    }
    pub fn can_go(&self, from: &str, to: &str) -> bool {
        match (self.action(from), self.action(to)) {
            (Some(f), Some(t)) => self.transitions[f].contains(&(t as u8)) || self.forced.contains(&(t as u8)),
            _ => false,
        }
    }
}

/// `aICH_loop_move_ctrl`: the four orbit corners (offsets from the flower
/// target) visited in order, the sign checks for "corner passed", and the
/// steering step (0x800 within 15 units, else 0x400).
pub const CHOU_LOOP_PT: [(f32, f32); 4] = [(10.0, 10.0), (10.0, -10.0), (-10.0, 10.0), (-10.0, -10.0)];
pub const CHOU_LOOP_CHK: [(f32, f32); 4] = [(-1.0, -1.0), (1.0, 1.0), (1.0, -1.0), (-1.0, 1.0)];
pub fn chou_steer_step(dist: f32) -> i16 {
    if dist < 15.0 {
        0x800
    } else {
        0x400
    }
}

/// `aICH_flower_search_sub` offset tables (type 1 = near, type 2 = far).
pub const CHOU_ADD_NEAR: [i32; 3] = [-1, 0, 1];
pub const CHOU_ADD_FAR: [i32; 3] = [-2, 0, 2];

/// One flower-search candidate, consuming `RANDOM(3)` for x then for z
/// (each `RANDOM(n)` is exactly one `fqrand()`); an axis outside 1..=14
/// keeps its old value.
pub fn chou_search_candidate(cur: (i32, i32), far: bool, rx: i32, rz: i32) -> (i32, i32) {
    let t = if far { CHOU_ADD_FAR } else { CHOU_ADD_NEAR };
    let mut x = cur.0 + t[rx as usize];
    let mut z = cur.1 + t[rz as usize];
    if !(1..=14).contains(&x) {
        x = cur.0;
    }
    if !(1..=14).contains(&z) {
        z = cur.1;
    }
    (x, z)
}

/// `aICH_flower_search` gate. RNG order: ONE `fqrand()` is always drawn
/// first (the `||` short-circuits after it). On a flower unit a roll below
/// 0.5 keeps the current target; purple butterflies never search. A search
/// then draws RANDOM(3) x, RANDOM(3) z (near). The candidate must be
/// walkable (`mFI_CheckFGNpcOn`); if it is but its ground is 20+ units above
/// the butterfly, it retries ONCE with the far table (two more RANDOM(3)
/// draws, still walkability-checked, no further retry).
pub fn chou_should_search(roll: f32, on_flower_unit: bool, purple: bool) -> bool {
    (roll >= 0.5 || !on_flower_unit) && !purple
}

/// RNG draws consumed by one `aICH_flower_search` call.
pub fn chou_search_rng_draws(searched: bool, near_walkable_but_too_high: bool) -> u32 {
    1 + if searched { 2 + if near_walkable_but_too_high { 2 } else { 0 } } else { 0 }
}

/// Behaviour-program state machines (index = `P_*`), extracted from
/// `ac_ins_*.c` by tools/gen_insect_fsm.py: per action, the actions its act
/// proc, init proc and one level of helpers can switch to (constant and
/// computed `setupAction` arguments), the actions `actor_init` can start
/// in, and those `actor_move` forces (net catch / lifetime escape).
pub const PROGRAM_FSM: [ProgramFsm; 14] = [
    // QUIRK (muse, 2026-10-12): FLY -> LANDING is missing from the generated
    // table's regex view but real: `aICH_fly` calls `aICH_rest_check`, which on
    // pansies (FLOWER_PANSIES0) calls setupAction(insect,
    // aINS_INSECT_TYPE_PURPLE_BUTTERFLY) — the TYPE id 3 numerically equals
    // aICH_ACTION_LANDING. tools/gen_insect_fsm.py now resolves type-constant
    // action arguments by ordinal, so this edge is generated, not hand-kept.
    // (Purple butterflies never run rest_check, so they never flower-search;
    // tiger butterflies only run it while patience <= 90.)
    ProgramFsm { family: "chou", actions: &["AVOID", "LET_ESCAPE", "FLY", "LANDING", "HOVER", "REST"],
        transitions: &[&[2], &[], &[0, 3], &[0, 4], &[0, 2, 5], &[0, 2, 5]],
        initial: &[2, 1], forced: &[1] },
    ProgramFsm { family: "batta", actions: &["AVOID", "LET_ESCAPE", "CHANGE_DIRECTION", "WAIT", "JUMP", "DROWN"],
        transitions: &[&[2, 5], &[5], &[3], &[0, 2, 5, 4], &[2], &[]],
        initial: &[2, 1], forced: &[1] },
    ProgramFsm { family: "tonbo", actions: &["AVOID", "LET_ESCAPE", "FLY", "ONIYANMA_FLY", "WAIT", "TOUCH_WATER", "TOUCH_WATER_REVERSE", "HOVER_WAIT_ON_WATER", "FLY_ON_NOTICE", "REST_ON_NOTICE"],
        transitions: &[&[1, 5, 4], &[], &[8, 5, 4], &[1], &[2, 1], &[6], &[7], &[5, 4], &[2, 9], &[2]],
        initial: &[2, 1, 3], forced: &[1] },
    ProgramFsm { family: "tentou", actions: &["AVOID", "AVOID_MAIMAI", "LET_ESCAPE", "LET_ESCAPE_MAIMAI", "MOVE", "WAIT"],
        transitions: &[&[], &[], &[], &[], &[0, 1, 5], &[0, 1, 4]],
        initial: &[2, 3, 5], forced: &[2, 3] },
    ProgramFsm { family: "hotaru", actions: &["AVOID", "LET_ESCAPE", "FLY"],
        transitions: &[&[], &[], &[]],
        initial: &[2, 1], forced: &[1] },
    ProgramFsm { family: "semi", actions: &["AVOID", "LET_ESCAPE", "WAIT"],
        transitions: &[&[], &[], &[0]],
        initial: &[0, 1, 2], forced: &[1] },
    ProgramFsm { family: "kabuto", actions: &["AVOID", "LET_ESCAPE", "WAIT"],
        transitions: &[&[], &[], &[0]],
        initial: &[0, 1, 2], forced: &[1] },
    ProgramFsm { family: "goki", actions: &["AVOID", "LET_ESCAPE", "WAIT_ON_FLOWER", "MOVE_ON_FLOWER", "WAIT_ON_TREE", "MOVE_ON_TREE", "WAIT_ON_ITEM", "MOVE_ON_ITEM"],
        transitions: &[&[], &[], &[0, 3], &[0, 2], &[0], &[0], &[0], &[0]],
        initial: &[1, 2, 6, 4], forced: &[1] },
    ProgramFsm { family: "hitodama", actions: &["AVOID", "LET_ESCAPE", "FLY"],
        transitions: &[&[], &[], &[]],
        initial: &[0, 2], forced: &[1] },
    ProgramFsm { family: "amenbo", actions: &["WAIT", "LET_ESCAPE", "MOVE", "REST"],
        transitions: &[&[], &[], &[3], &[2]],
        initial: &[1, 2], forced: &[1] },
    ProgramFsm { family: "ka", actions: &["AVOID", "LET_ESCAPE", "FLY", "SEARCH", "ATTACK_WAIT", "ATTACK"],
        transitions: &[&[], &[], &[0, 3], &[4, 0, 2], &[5, 0, 2, 3], &[]],
        initial: &[2, 1], forced: &[1] },
    ProgramFsm { family: "dango", actions: &["AVOID", "LET_ESCAPE", "STOP", "HIDE", "APPEAR", "DIVE", "DROWN", "RETIRE"],
        transitions: &[&[5, 7, 2], &[5], &[0, 5], &[4], &[2], &[6], &[], &[5]],
        initial: &[3, 1], forced: &[1] },
    ProgramFsm { family: "kera", actions: &["AVOID", "LET_ESCAPE", "HIDE", "APPEAR", "DIVE", "DROWN", "DUG"],
        transitions: &[&[4, 6, 1], &[4], &[], &[], &[], &[], &[]],
        initial: &[2, 1], forced: &[1] },
    ProgramFsm { family: "mino", actions: &["AVOID", "LET_ESCAPE", "HIDE", "APPEAR", "APPEAR_STOP", "WAIT", "DISAPPEAR", "DIVE", "DROWN", "FALL"],
        transitions: &[&[], &[7], &[3, 9], &[4, 9], &[9, 5], &[6, 9], &[9, 2], &[], &[], &[]],
        initial: &[2, 1], forced: &[1] },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispatch_table() {
        assert_eq!(pc_insect_program_for_type(0), P_CHOU);
        assert_eq!(pc_insect_program_for_type(3), P_CHOU);
        assert_eq!(pc_insect_program_for_type(8), P_SEMI);
        assert_eq!(pc_insect_program_for_type(12), P_TONBO);
        assert_eq!(pc_insect_program_for_type(18), P_BATTA);
        assert_eq!(pc_insect_program_for_type(23), P_KABUTO);
        assert_eq!(pc_insect_program_for_type(31), P_KABUTO);
        assert_eq!(pc_insect_program_for_type(26), P_TENTOU);
        assert_eq!(pc_insect_program_for_type(27), P_HOTARU);
        assert_eq!(pc_insect_program_for_type(28), P_GOKI);
        assert_eq!(pc_insect_program_for_type(33), P_KERA);
        assert_eq!(pc_insect_program_for_type(34), P_AMENBO);
        assert_eq!(pc_insect_program_for_type(35), P_MINO);
        assert_eq!(pc_insect_program_for_type(37), P_MINO);
        assert_eq!(pc_insect_program_for_type(36), P_DANGO);
        assert_eq!(pc_insect_program_for_type(38), P_DANGO);
        assert_eq!(pc_insect_program_for_type(39), P_KA);
        assert_eq!(pc_insect_program_for_type(40), P_HITODAMA);
        assert_eq!(pc_insect_program_for_type(41), -1);
        // Full-table length and all programs used.
        assert_eq!(PROGRAM_FOR_TYPE.len(), 41);
        for p in 0..P_NUM {
            assert!(PROGRAM_FOR_TYPE.contains(&p), "program {p} unused");
        }
    }

    #[test]
    fn catch_ranges() {
        assert_eq!(pc_insect_catch_range(0, 0, 20000), 24.0);
        assert_eq!(pc_insect_catch_range(1, 0, 20000), 24.0);
        assert_eq!(pc_insect_catch_range(2, 0, 0), 8.0); // tiger butterfly default
        assert_eq!(pc_insect_catch_range(4, 0, 1000), 24.0); // facing
        assert_eq!(pc_insect_catch_range(4, 0, 16384), 24.0); // boundary inclusive
        assert_eq!(pc_insect_catch_range(4, 0, 16385), 0.0);
        assert_eq!(pc_insect_catch_range(31, 0, 0), 24.0); // giant beetle gated too
        assert_eq!(pc_insect_catch_range(28, 4, 0), 24.0); // cockroach flag==4 gated
        assert_eq!(pc_insect_catch_range(28, 4, 20000), 0.0);
        assert_eq!(pc_insect_catch_range(28, 3, 0), 8.0); // flag!=4 → default
        assert_eq!(pc_insect_catch_range(27, 0, 0), 8.0);
        assert_eq!(pc_insect_catch_range(40, 0, 0), 8.0);
    }

    #[test]
    fn stress_and_patience() {
        // idx = ((min_dist − unit) − max(0, dist − unit)) / 20
        assert_eq!(pc_insect_stress_idx(120.0, 40.0, 40.0), 4); // (80−0)/20 = 4
        assert_eq!(pc_insect_stress_idx(120.0, 120.0, 40.0), 0); // (80−80)/20 = 0
        assert_eq!(pc_insect_stress_idx(200.0, 40.0, 40.0), 4); // clamped
        assert_eq!(pc_insect_stress_idx(30.0, 40.0, 40.0), 0); // clamped bottom
        assert!((pc_insect_stress_calc(3.0, 4.0, 2) - 5.0 * 2.0).abs() < 1e-5);
        assert!((pc_insect_stress_calc(0.0, 0.0, 0) - 0.0).abs() < 1e-5);
        // Patience: decay dt-scaled, growth not.
        assert!((pc_insect_patience_step(50.0, 0.0, 1.0) - 49.5).abs() < 1e-5);
        assert!((pc_insect_patience_step(50.0, 0.0, 2.0) - 49.0).abs() < 1e-5);
        assert_eq!(pc_insect_patience_step(0.2, 0.0, 1.0), 0.0);
        assert!((pc_insect_patience_step(50.0, 10.0, 2.0) - 55.0).abs() < 1e-5);
        assert_eq!(pc_insect_patience_step(99.0, 10.0, 1.0), 100.0);
        // Catch-ME radii.
        assert_eq!(pc_insect_catch_me_radius(4), 10.0);
        assert_eq!(pc_insect_catch_me_radius(14), 20.0);
        assert_eq!(pc_insect_catch_me_radius(15), -20.0);
        assert_eq!(pc_insect_catch_me_radius(27), 0.0);
        assert_eq!(pc_insect_catch_me_radius(99), 0.0);
    }

    #[test]
    fn fade_and_despawn() {
        assert_eq!(pc_insect_fade_alpha(0), 255);
        assert_eq!(pc_insect_fade_alpha(1), 244);
        assert!(!pc_insect_fade_done(23)); // 255−253=2
        assert!(pc_insect_fade_done(24)); // 255−264=−9
        assert_eq!(pc_insect_firefly_alpha2(1), 11);
        assert!(!pc_insect_firefly_fade_done(23)); // 253 < 255
        assert!(pc_insect_firefly_fade_done(24)); // 264 ≥ 255
        assert!(pc_insect_despawn_check(true, false, false, 0.0, true, false));
        assert!(!pc_insect_despawn_check(true, false, true, 0.0, true, false)); // net target
        assert!(!pc_insect_despawn_check(true, true, false, 0.0, true, false)); // visible
        assert!(pc_insect_despawn_check(false, true, false, 700.0, false, false));
        assert!(!pc_insect_despawn_check(false, true, false, 700.0, true, false)); // same acre
        assert!(!pc_insect_despawn_check(false, true, true, 700.0, false, false)); // net target
        assert!(pc_insect_despawn_check(false, true, false, 0.0, true, true)); // destruct flag
        assert!(!pc_insect_despawn_check(false, true, false, 0.0, true, false));
        // Shadow sizes.
        assert_eq!(pc_insect_shadow_size(2), 8.0);
        assert_eq!(pc_insect_shadow_size(8), 8.0);
        assert_eq!(pc_insect_shadow_size(12), 8.0);
        assert_eq!(pc_insect_shadow_size(35), 4.0);
        assert_eq!(pc_insect_shadow_size(37), 4.0);
        assert_eq!(pc_insect_shadow_size(39), 4.0);
        assert_eq!(pc_insect_shadow_size(38), 2.0);
        assert_eq!(pc_insect_shadow_size(0), 6.0);
        assert_eq!(pc_insect_shadow_size(28), 6.0);
    }
}

#[cfg(test)]
mod followup_tests {
    use super::*;

    #[test]
    fn spawn_and_slots() {
        assert_eq!(spawn_extra_data(T_COCKROACH, true, 0), SPAWN_GOKI_TREE);
        assert_eq!(spawn_extra_data(T_COCKROACH, false, ITM_KABU_SPOILED), SPAWN_GOKI_ITEM);
        assert_eq!(spawn_extra_data(T_BEE, true, 0), INIT_NORMAL);
        let mut ex = [false; 9];
        assert_eq!(regist_space(&ex, false), Some(0));
        assert_eq!(regist_space(&ex, true), Some(8));
        ex[..8].iter_mut().for_each(|e| *e = true);
        assert_eq!(regist_space(&ex, false), None);
        assert_eq!(OVERLAY_BSS_SIZE, 0x5418);
    }

    #[test]
    fn items_and_handoff() {
        assert_eq!(insect_item(T_COMMON_BUTTERFLY, P_CHOU), 0x2D00);
        assert_eq!(insect_item(T_MOSQUITO, P_KA), 0x2D27);
        assert_eq!(insect_item(T_SPIRIT, P_HITODAMA), ITM_SPIRIT0);
        assert_eq!(insect_item(T_SPIDER, P_MINO), 0x2D25);
        assert_eq!(insect_item(T_SPIDER, P_DANGO), 0);
        assert_eq!(net_catch_item(0, 0x2D05, 5), 0x2D05);
        assert_eq!(net_catch_item(1, 0, T_BEE), ITM_INSECT_BEE);
        assert_eq!(net_catch_item(1, 0, T_ANT), ITM_INSECT_ANT);
        // A caught spirit (count 1) stacks onto SPIRIT0..SPIRIT3, not SPIRIT4.
        let pockets = [(0x2D2Cu16, true), (0x2D2A, true), (0, true)];
        assert_eq!(spirit_putin(ITM_SPIRIT0, &pockets), Some((1, 0x2D2B)));
        let full = [(0x2D2Cu16, true), (0, true)];
        assert_eq!(spirit_putin(ITM_SPIRIT0, &full), Some((1, ITM_SPIRIT0)));
        assert!(!catch_sets_collect_bit(T_SPIRIT) && catch_shows_arrow(T_ANT));
        assert_eq!(catch_fanfare(true), 0x4B);
    }

    #[test]
    fn flag_semantics() {
        use super::flags::*;
        assert!(registers_catch(WALL_CHECK) && !registers_catch(NO_CATCH));
        assert!(wall_check(WALL_CHECK) && !wall_check(WALL_CHECK | FLEEING));
        assert!(lifetime_escape(LIFE_OVER) && !lifetime_escape(LIFE_OVER | FLEEING));
    }

    #[test]
    fn fsm_tables() {
        assert_eq!(PROGRAM_FSM.len(), P_NUM as usize);
        let order = ["chou", "batta", "tonbo", "tentou", "hotaru", "semi", "kabuto", "goki", "hitodama", "amenbo", "ka", "dango", "kera", "mino"];
        for (i, f) in PROGRAM_FSM.iter().enumerate() {
            assert_eq!(f.family, order[i]);
            assert_eq!(f.transitions.len(), f.actions.len());
            assert!(f.forced.iter().all(|&a| f.actions[a as usize].starts_with("LET_ESCAPE")));
        }
        let chou = &PROGRAM_FSM[P_CHOU as usize];
        assert!(chou.can_go("HOVER", "REST") && chou.can_go("FLY", "AVOID") && !chou.can_go("AVOID", "HOVER"));
        let batta = &PROGRAM_FSM[P_BATTA as usize];
        assert!(batta.can_go("WAIT", "JUMP") && batta.can_go("JUMP", "CHANGE_DIRECTION"));
        let kera = &PROGRAM_FSM[P_KERA as usize];
        assert!(kera.can_go("AVOID", "DUG"));
    }

    #[test]
    fn abi_sizes() {
        // Header sizes (include/ac_insect_h.h): the init block passed to the
        // controller, the pooled actor, the controller, and one overlay entry.
        assert_eq!(INIT_STRUCT_SIZE, 0x18);
        assert_eq!(INSECT_ACTOR_SIZE, 0x288);
        assert_eq!(INSECT_CTRL_SIZE, 0x183C);
        assert_eq!(OVERLAY_ENTRY_SIZE, 0x1C08);
        assert_eq!(OVERLAY_ENTRY_DATA, 0x1C00);
        assert_eq!(OVERLAY_BSS_SIZE, 3 * 0x1C08);
        // Controller BSS holds the 9 pooled actors past the ACTOR header.
        assert!(INSECT_CTRL_SIZE > 9 * INSECT_ACTOR_SIZE);
    }

    #[test]
    fn chou_pansy_quirk() {
        // FLY -> LANDING via the pansy rest_check (type id 3 = action id 3).
        let chou = &PROGRAM_FSM[P_CHOU as usize];
        assert!(chou.can_go("FLY", "LANDING"));
        assert!(chou.can_go("LANDING", "HOVER"));
        assert!(!chou.can_go("FLY", "REST"));
        // Purple butterflies never flower-search (chou_should_search gate).
        assert!(!chou_should_search(0.9, false, true));
    }

    #[test]
    fn chou_search() {
        assert_eq!(chou_search_candidate((7, 7), false, 0, 2), (6, 8));
        assert_eq!(chou_search_candidate((1, 14), true, 0, 2), (1, 14));
        assert!(chou_should_search(0.6, true, false) && !chou_should_search(0.4, true, false));
        assert!(chou_should_search(0.1, false, false) && !chou_should_search(0.9, false, true));
        assert_eq!(chou_search_rng_draws(false, false), 1);
        assert_eq!(chou_search_rng_draws(true, true), 5);
        assert_eq!(chou_steer_step(10.0), 0x800);
    }
}
