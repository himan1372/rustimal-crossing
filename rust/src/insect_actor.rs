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
