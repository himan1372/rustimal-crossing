//! The Morning Aerobics NPC (`ac_taisou_npc0.c`).
//!
//! Verified against `src/actor/npc/ac_taisou_npc0.c`,
//! `ac_taisou_npc0_talk.c_inc`, `ac_taisou_npc0_anime.c_inc`,
//! `include/ac_taisou_npc0.h`, `include/audio.h`,
//! `src/game/m_event_map_npc_data.c_inc`,
//! `src/game/m_event_schedule.c_inc`, `src/actor/ac_event_manager.c`,
//! `include/ac_npc_anim_def.h`, `include/m_name_table.h`
//! (GAFE01_00 Rev 0 USA). All 10 brief claims PROVEN.
//!
//! Scope split: the MORNING_AEROBICS Jul 25–Aug 31 06:00 schedule
//! row already lives in the sibling `event_day` module; this
//! module adds the rumor/talk rows there. This module owns the
//! pure kernels: the failure-rate table and eligibility, the
//! action→animation table, all ten ratio tables, the delay math,
//! the dialogue formulas, the init-angle rule, the leader
//! selection, and the shrine layout. The radio counter,
//! RNG call order, NPC clip lifecycle, the leader/group globals,
//! animation init, demo/talk, Tortimer's radio-card inventory
//! mutations, and the event-manager handlers stay in C.
//!
//! Nuances carried: the failure `type` falls back to 0 when the
//! NPC id is more than 4 above TAISOU_0; the mistake is eligible
//! only for actions other than 0 and 9; the failure action uses
//! `1 + RANDOM(8)` with a do/while rejection of `next_action`
//! (the loop stays in C); the taisou6 ratio array is genuinely
//! unsorted (`{0.0, 0.5, 1.0, 1.5, 0.96875}`) — preserved as
//! written; the block-part comparison is against
//! `startRatio[idx][seq + 1]` (the NEXT segment boundary);
//! NPC 0 never gets the RANDOM(48) delay; the dialogue row is 0
//! while Morning Aerobics is active, 1 for the Sports Fair
//! variant; action == 10 fixes the villager message offset at +2
//! instead of calling RANDOM(2).

/// First aerobics NPC ID (`m_name_table.h`).
pub const SP_NPC_EV_TAISOU_0: u16 = 0xD04E;
/// Last aerobics NPC ID.
pub const SP_NPC_EV_TAISOU_4: u16 = 0xD052;
/// Tortimer's aerobics ID.
pub const SP_NPC_SONCHO_D078: u16 = 0xD078;
/// The aerobics radio prop actor ID.
pub const AEROBICS_RADIO: u16 = 0x582C;
/// Leader sentinel: `SP_NPC_EV_TAISOU_4 + 1`; registration keeps
/// the smallest NPC ID seen.
pub const LEADER_SENTINEL: u16 = 0xD053;

/// Exercise-failure probabilities per NPC type
/// (`chk_failure_rate`).
pub const FAILURE_RATE: [f32; 5] = [0.0, 0.05, 0.1, 0.15, 0.3];

/// Returns the failure rate for an NPC type index, with retail's
/// `> 4 → 0` fallback (out-of-range indices also yield 0 rather
/// than an out-of-bounds read).
#[no_mangle]
pub extern "C" fn pc_taisou_failure_rate(npc_type: i32) -> f32 {
    if npc_type < 0 || npc_type > 4 {
        0.0
    } else {
        FAILURE_RATE[npc_type as usize]
    }
}

/// The next exercise action: `(radio.measure + 1) % 10`
/// (`aTS0_ctrl_gymnastic_sub0`).
#[no_mangle]
pub extern "C" fn pc_taisou_next_action(measure: i32) -> i32 {
    (measure + 1) % 10
}

/// Returns true when a mistake may be selected: no failure actor
/// is currently designated, the next action is neither 0 nor 9,
/// and the sampled random float is below the type's rate. The
/// float sample stays in C.
#[no_mangle]
pub extern "C" fn pc_taisou_failure_eligible(
    next_action: i32,
    rand_float: f32,
    rate: f32,
    failure_active: bool,
) -> bool {
    !failure_active && next_action != 0 && next_action != 9 && rand_float < rate
}

/// The do/while rejection predicate for the failure action:
/// `1 + RANDOM(8)` is re-rolled while it equals `next_action`.
/// The loop and its RNG calls stay in C.
#[no_mangle]
pub extern "C" fn pc_taisou_failure_reject(failure_action: i32, next_action: i32) -> bool {
    failure_action == next_action
}

/// Action → base animation ID (`aTS0_set_animation`,
/// `ac_npc_anim_def.h` values). The C caller adds
/// `(anime_dir & 1)` for the variant and passes FALSE to the
/// animation init.
pub const ANIM_SEQ_NO: [i32; 13] = [
    81, // INTRO1_A
    83, // TAISOU1
    84, // TAISOU2
    87, // TAISOU4_A
    89, // TAISOU5_A
    91, // TAISOU6_A
    93, // TAISOU7
    85, // TAISOU3_A
    83, // TAISOU1
    5,  // WAIT1
    94, // KIDUKU1
    5,  // WAIT1
    105, // TIRED1
];

/// Returns the base animation ID for an exercise action (0–12),
/// or -1 outside the table. The variant bit is separate.
#[no_mangle]
pub extern "C" fn pc_taisou_anim_id(action: i32) -> i32 {
    if action < 0 || action >= ANIM_SEQ_NO.len() as i32 {
        return -1;
    }
    ANIM_SEQ_NO[action as usize]
}

/// Returns the animation variant bit: `anime_dir & 1`.
#[no_mangle]
pub extern "C" fn pc_taisou_anim_variant(anime_dir: i32) -> i32 {
    anime_dir & 1
}

/// Segment start-ratio tables, in segment order 0–9
/// (`aTS0_startRatio`). Lengths vary: 3,5,3,5,3,5,5,5,5,2.
/// Segment 6 (taisou6) is unsorted as written — do not sort it.
pub const START_RATIO_0: [f32; 3] = [0.0, 0.5, 0.93581255];
pub const START_RATIO_1: [f32; 5] = [0.0, 0.25, 0.5, 0.75, 0.96875];
pub const START_RATIO_2: [f32; 3] = [0.0, 0.5, 0.96875];
pub const START_RATIO_3: [f32; 5] = [0.0, 0.25, 0.5, 0.75, 0.96875];
pub const START_RATIO_4: [f32; 3] = [0.0, 0.5, 0.96875];
pub const START_RATIO_5: [f32; 5] = [0.0, 0.25, 0.5, 0.75, 0.96875];
pub const START_RATIO_6: [f32; 5] = [0.0, 0.5, 1.0, 1.5, 0.96875];
pub const START_RATIO_7: [f32; 5] = [0.0, 0.25, 0.5, 0.75, 0.96875];
pub const START_RATIO_8: [f32; 5] = [0.0, 0.25, 0.5, 0.75, 0.96875];
pub const START_RATIO_9: [f32; 2] = [0.0, 0.96875];
/// Measure-end ratios per segment (`aTS0_endRatio`).
pub const END_RATIO: [f32; 10] = [
    0.93581255, 0.96875, 0.96875, 0.96875, 0.96875, 0.96875, 0.96875, 0.96875, 0.96875,
    0.96875,
];
/// Animation morph ratios per segment (`aTS0_morphRatio`).
pub const MORPH_RATIO: [f32; 10] = [
    0.030187488, 0.03125, 0.03125, 0.03125, 0.03125, 0.03125, 0.03125, 0.03125, 0.03125,
    0.03125,
];
/// Ratio spans per segment (`aTS0_ratioLength`).
pub const RATIO_LENGTH: [[f32; 2]; 10] = [
    [0.5, 0.5],
    [0.25, 0.25],
    [0.5, 0.5],
    [0.25, 0.25],
    [0.5, 0.5],
    [0.25, 0.25],
    [0.5, 0.5],
    [0.25, 0.25],
    [0.25, 0.25],
    [1.0, 1.0],
];

fn start_ratio(seg: usize, idx: usize) -> Option<f32> {
    match seg {
        0 => START_RATIO_0.get(idx),
        1 => START_RATIO_1.get(idx),
        2 => START_RATIO_2.get(idx),
        3 => START_RATIO_3.get(idx),
        4 => START_RATIO_4.get(idx),
        5 => START_RATIO_5.get(idx),
        6 => START_RATIO_6.get(idx),
        7 => START_RATIO_7.get(idx),
        8 => START_RATIO_8.get(idx),
        9 => START_RATIO_9.get(idx),
        _ => None,
    }
    .copied()
}

/// Looks up `startRatio[seg][idx]`; returns -1.0 when out of
/// range.
#[no_mangle]
pub extern "C" fn pc_taisou_start_ratio(seg: i32, idx: i32) -> f32 {
    if seg < 0 || idx < 0 {
        return -1.0;
    }
    start_ratio(seg as usize, idx as usize).unwrap_or(-1.0)
}

/// The `aTS0_set_current` frame-sync math:
/// `start + ((measure_progress - now) - startRatio[seg][anime_dir])
/// * (end - start) / ratioLength[seg][anime_dir & 1]`.
/// Returns -1.0 when the table lookup is out of range.
#[no_mangle]
pub extern "C" fn pc_taisou_sync_frame(
    start: f32,
    end: f32,
    measure_progress: f32,
    now: f32,
    seg: i32,
    anime_dir: i32,
) -> f32 {
    let sr = if seg < 0 || anime_dir < 0 {
        None
    } else {
        start_ratio(seg as usize, anime_dir as usize)
    };
    let rl = if seg < 0 || seg >= RATIO_LENGTH.len() as i32 {
        None
    } else {
        Some(RATIO_LENGTH[seg as usize][(anime_dir & 1) as usize])
    };
    match (sr, rl) {
        (Some(sr), Some(rl)) if rl != 0.0 => {
            start + ((measure_progress - now) - sr) * (end - start) / rl
        }
        _ => -1.0,
    }
}

/// Returns true when the NPC gets the random timing offset:
/// every participant except SP_NPC_EV_TAISOU_0
/// (`aTS0_setup_delay_cnt`; the RANDOM(48) call stays in C).
#[no_mangle]
pub extern "C" fn pc_taisou_has_delay(npc_id: u16) -> bool {
    npc_id != SP_NPC_EV_TAISOU_0
}

/// One `aTS0_check_over_bgm_block_part` delay step, purely: when
/// the NPC has a delay and `delay_cnt != 0`, decrements the
/// counter, saves `ratio * 0.2` as the new current ratio, and
/// scales the ratio by 0.8; otherwise subtracts the saved
/// current ratio. Writes (new_current_ratio, adjusted_ratio) to
/// `out[0..2]` and returns the new delay count.
#[no_mangle]
pub extern "C" fn pc_taisou_delay_step(
    ratio: f32,
    delay_cnt: i32,
    current_ratio: f32,
    has_delay: bool,
    out: *mut f32,
) -> i32 {
    let (new_delay, new_cur, adjusted) = if has_delay && delay_cnt != 0 {
        (delay_cnt - 1, ratio * 0.2, ratio * 0.8)
    } else {
        (delay_cnt, current_ratio, ratio - current_ratio)
    };
    if !out.is_null() {
        let out = unsafe { std::slice::from_raw_parts_mut(out, 2) };
        out[0] = new_cur;
        out[1] = adjusted;
    }
    new_delay
}

/// Tests whether the adjusted ratio passed the next segment
/// boundary: `ratio > startRatio[seg][seq + 1]`
/// (`aTS0_check_over_bgm_block_part` returns FALSE while
/// `ratio <=` that entry). Returns false on out-of-range
/// lookups.
#[no_mangle]
pub extern "C" fn pc_taisou_over_block(adjusted: f32, seg: i32, seq: i32) -> bool {
    if seg < 0 || seq < 0 {
        return false;
    }
    match start_ratio(seg as usize, (seq + 1) as usize) {
        Some(boundary) => adjusted > boundary,
        None => false,
    }
}

/// Dialogue bases per looks index: row 0 while Morning Aerobics
/// is active, row 1 for the Sports Fair variant.
pub const MSG_BASE: [[i32; 6]; 2] = [
    [6716, 6725, 6701, 6746, 6761, 6776],
    [6806, 6815, 6791, 6836, 6851, 6862],
];
/// Copper's message bases per event row (0x2665 / 0x2668).
pub const COPPER_BASE: [i32; 2] = [9829, 9832];

/// Selects the dialogue row: 0 while Morning Aerobics is active,
/// 1 otherwise.
#[no_mangle]
pub extern "C" fn pc_taisou_msg_row(aerobics_active: bool) -> i32 {
    if aerobics_active { 0 } else { 1 }
}

/// NPC type for dialogue: `npc_id - SP_NPC_EV_TAISOU_0`.
#[no_mangle]
pub extern "C" fn pc_taisou_npc_type(npc_id: u16) -> i32 {
    npc_id.wrapping_sub(SP_NPC_EV_TAISOU_0) as i32
}

/// Villager exercise message: `base + type*3 + rand2`, except
/// action == 10 uses the fixed offset +2 instead of RANDOM(2).
/// The random values are supplied by the C caller.
#[no_mangle]
pub extern "C" fn pc_taisou_villager_msg(
    row: i32,
    looks: i32,
    npc_type: i32,
    rand2: i32,
    action: i32,
) -> i32 {
    let r = (row & 1) as usize;
    let l = looks.rem_euclid(6) as usize;
    let base = MSG_BASE[r][l] + npc_type * 3;
    if action == 10 {
        base + 2
    } else {
        base + rand2
    }
}

/// Copper's message: `COPPER_BASE[row] + rand3`.
#[no_mangle]
pub extern "C" fn pc_taisou_copper_msg(row: i32, rand3: i32) -> i32 {
    COPPER_BASE[(row & 1) as usize] + rand3
}

/// Schedule-init facing: NPC 0 faces 180° (32768 short-angle
/// units); everyone else faces 0°.
#[no_mangle]
pub extern "C" fn pc_taisou_init_angle(npc_id: u16) -> i32 {
    if npc_id == SP_NPC_EV_TAISOU_0 {
        32768
    } else {
        0
    }
}

/// Leader selection: the smallest NPC ID wins; the sentinel
/// starts above the range (`aTS0_leader_no`).
#[no_mangle]
pub extern "C" fn pc_taisou_leader_update(leader: u16, npc_id: u16) -> u16 {
    leader.min(npc_id)
}

/// Aerobics shrine layout: seven slots of (field-unit X, Z):
/// TAISOU_1..4, TAISOU_0, the radio, Tortimer.
pub const SPAWN_LAYOUT: [(i32, i32); 7] = [
    (4, 10),
    (6, 10),
    (8, 10),
    (10, 10),
    (7, 12),
    (5, 13),
    (9, 8),
];
/// NPC slot mask `0b0011111` in the data record (4 NPC bits).
pub const SPAWN_NPC_MASK: u32 = 0b0011111;
pub const SPAWN_SLOT_COUNT: usize = 7;

/// Writes the (x, z) field-unit position of a spawn slot to
/// `out[0..2]`; returns 1 on success. Slots are 0-based.
#[no_mangle]
pub extern "C" fn pc_taisou_spawn_pos(slot: u8, out: *mut i32) -> u8 {
    if out.is_null() {
        return 0;
    }
    let s = slot as usize;
    if s >= SPAWN_LAYOUT.len() {
        return 0;
    }
    let (px, pz) = SPAWN_LAYOUT[s];
    let out = unsafe { std::slice::from_raw_parts_mut(out, 2) };
    out[0] = px;
    out[1] = pz;
    1
}


/// Radio-counter constants (`Na_GetRadioCounter`,
/// `src/static/jaudio_NES/game/radio.c`).
pub const RADIO_SEQ_AEROBICS: i32 = 0xDA;
pub const RADIO_COUNTER_FIRST: i32 = 744;
pub const RADIO_COUNTER_MEASURE: i32 = 768;
pub const RADIO_MEASURE9_PROGRESS: f32 = 216.0;
pub const AUDIO_TATUMS_PER_BEAT: u16 = 48;

/// Availability predicate for `sAdos_GetRadioCounter`: nonzero iff the
/// current BGM group is enabled and its sequence ID is 0xDA. On failure
/// the C code returns -1 WITHOUT touching `Radio_c` (stale values may
/// remain); the group lookup itself stays in C.
#[no_mangle]
pub extern "C" fn pc_taisou_radio_available(enabled: i32, seq_id: i32) -> i32 {
    (enabled != 0 && seq_id == RADIO_SEQ_AEROBICS) as i32
}

/// Raw-counter → (measure, progress) conversion (`Na_GetRadioCounter`).
/// The first measure spans raw 0..744; each later measure spans 768
/// units; measure 9 uses a 216-unit progress denominator (its progress
/// can exceed 1.0). All divisions are the C originals'.
#[no_mangle]
pub extern "C" fn pc_taisou_radio_convert(
    raw_counter: i32,
    measure_out: *mut i8,
    progress_out: *mut f32,
) {
    let (measure, progress) = if raw_counter < RADIO_COUNTER_FIRST {
        (0i8, raw_counter as f32 / RADIO_COUNTER_MEASURE as f32)
    } else {
        let mut counter = raw_counter - RADIO_COUNTER_FIRST;
        let mut measure = counter / RADIO_COUNTER_MEASURE;
        counter -= measure * RADIO_COUNTER_MEASURE;
        measure += 1;
        let progress = if measure == 9 {
            counter as f32 / RADIO_MEASURE9_PROGRESS
        } else {
            counter as f32 / RADIO_COUNTER_MEASURE as f32
        };
        (measure as i8, progress)
    };
    unsafe {
        if !measure_out.is_null() {
            *measure_out = measure;
        }
        if !progress_out.is_null() {
            *progress_out = progress;
        }
    }
}

/// Tempo write (`Na_GetRadioCounter`): the BGM group's u16 tempo
/// divided by `AUDIO_TATUMS_PER_BEAT` (48), integer division.
#[no_mangle]
pub extern "C" fn pc_taisou_radio_tempo(tempo: u16) -> u16 {
    tempo / AUDIO_TATUMS_PER_BEAT
}

/// Leader-cull predicate (`aTS0_actor_move`): when the leader is NOT
/// marked `ACTOR_STATE_NO_CULL` and its block differs from the player's
/// current block, the group deletion flag is set and the leader deletes
/// itself (other actors observe the flag on their own updates).
#[no_mangle]
pub extern "C" fn pc_taisou_leader_cull(
    leader_no_cull: i32,
    leader_block: i32,
    player_block: i32,
) -> i32 {
    (leader_no_cull == 0 && leader_block != player_block) as i32
}

/// Registration drain (`aTS0_actor_dt`): every destructor decrements
/// the count; at zero-or-below it clamps to zero and signals the
/// shared deletion flag reset via `*reset_delete`.
#[no_mangle]
pub extern "C" fn pc_taisou_regist_drain(regist_sum: i32, reset_delete: *mut i32) -> i32 {
    let new_sum = regist_sum - 1;
    let (clamped, reset) = if new_sum <= 0 { (0, 1) } else { (new_sum, 0) };
    unsafe {
        if !reset_delete.is_null() {
            *reset_delete = reset;
        }
    }
    clamped
}

/// Failure-rate table index (`ac_taisou_npc0.c`): `npc_id - 0xD04E`,
/// forced to 0 when above 4 (covers Tortimer 0xD078 and any other
/// out-of-range ID).
#[no_mangle]
pub extern "C" fn pc_taisou_failure_type(npc_id: u16) -> i32 {
    let t = npc_id.wrapping_sub(SP_NPC_EV_TAISOU_0);
    if t > 4 {
        0
    } else {
        t as i32
    }
}

/// Tortimer's radio-card constants (`m_soncho.c` / `m_soncho.h`).
pub const RADIO_CARD00: i32 = 0x2523;
pub const RADIO_CARD_COUNT: i32 = 13;
/// `FTR_START(FTR_RADIO_TEST)` — the aerobics radio furniture item.
/// (radio.rs's 1011 is the furniture *table index*, not this item ID.)
pub const RADIO_FURNITURE_ITEM: i32 = 0x1FCC;
pub const MORNING_AEROBICS_EVENT: i32 = 103;
/// `mSCR_TALK_*` (0-9).
pub const TALK_PICKUP_ALL: i32 = 0;
pub const TALK_INSPECTION: i32 = 1;
pub const TALK_NEXT_INSPECTION: i32 = 2;
pub const TALK_NEWCARD: i32 = 3;
pub const TALK_BEFORE_GIVE: i32 = 4;
pub const TALK_GIVE: i32 = 5;
pub const TALK_NEXT: i32 = 6;
pub const TALK_9: i32 = 9;
/// `mSCR_ACTION_*` (0-6).
pub const ACTION_TOO_LATE: i32 = 0;
pub const ACTION_DELETE_OLD_CARD: i32 = 1;
pub const ACTION_NEW_CARD: i32 = 2;
pub const ACTION_LOST_CARD: i32 = 3;
pub const ACTION_STAMP_CARD: i32 = 4;
pub const ACTION_FINISH_CARD: i32 = 5;
pub const ACTION_LOST_CARD_AND_FINISH: i32 = 6;
/// Radio-card dialogue messages.
pub const MSG_CARD_FOREIGNER: i32 = 0x343B;
pub const MSG_CARD_SAME_DAY: i32 = 0x3433;
pub const MSG_CARD_TOO_LATE: i32 = 0x3437;
pub const MSG_CARD_DELETE_OLD: i32 = 0x343C;
pub const MSG_CARD_NEW: i32 = 0x3422;
pub const MSG_CARD_NEW_MARKED: i32 = 0x3423;
pub const MSG_CARD_LOST: i32 = 0x342E;
pub const MSG_CARD_LOST_MARKED: i32 = 0x342F;
pub const MSG_CARD_STAMP: i32 = 0x3427;
pub const MSG_CARD_FINISH: i32 = 0x342A;
pub const MSG_CARD_LOST_FINISH_FOLLOWUP: i32 = 0x342D;
/// Radio-time states (`mSC_RADIO_TIME_*`).
pub const RADIO_TIME_LESS: i32 = 0;
pub const RADIO_TIME_SAME_DAY: i32 = 1;
pub const RADIO_TIME_OVER: i32 = 2;

/// `mSC_Radio_time_check`: a year mismatch returns LESS (0) — it is
/// NOT an automatic "new year, new card". Otherwise `equal_date` is
/// the `lbRTC_IsEqualDate` verdict (-1 LESS, 0 EQUAL, 1 OVER;
/// anything else defaults to LESS).
#[no_mangle]
pub extern "C" fn pc_taisou_radio_time_check(
    now_year: i32,
    last_year: i32,
    equal_date: i32,
) -> i32 {
    if now_year != last_year {
        RADIO_TIME_LESS
    } else {
        match equal_date {
            -1 => RADIO_TIME_LESS,
            0 => RADIO_TIME_SAME_DAY,
            1 => RADIO_TIME_OVER,
            _ => RADIO_TIME_LESS,
        }
    }
}

/// `mSC_Radio_limit_check`: the verbatim conjunction
/// `month >= 8 && day >= 19` → FALSE (0). Returns nonzero while card
/// issuance is still allowed. Note the quirk: September 1 passes
/// (day 1 < 19), because both conditions must hold.
#[no_mangle]
pub extern "C" fn pc_taisou_limit_check(month: i32, day: i32) -> i32 {
    ((month >= 8 && day >= 19) == false) as i32
}

/// Card-ID derivations (`m_soncho.c`):
/// - lost-card recovery increments `days` BEFORE deriving the ID;
/// - stamping derives from the current `days`;
/// - the 1-card OVER branch derives `days = card - CARD00 + 1` and the
///   C caller writes it back to `radiocard->days`.
#[no_mangle]
pub extern "C" fn pc_taisou_card_lost_id(days: i32) -> i32 {
    RADIO_CARD00 + days + 1
}

#[no_mangle]
pub extern "C" fn pc_taisou_card_stamp_id(days: i32) -> i32 {
    RADIO_CARD00 + days
}

#[no_mangle]
pub extern "C" fn pc_taisou_card_derive_days(card: i32) -> i32 {
    card - RADIO_CARD00 + 1
}

/// `mSC_Radio_Set_Talk_Proc` action selection (pure part). Returns the
/// `mSCR_ACTION_*` value, or -1 for the foreigner branch and -2 for
/// the same-day branch (those set only the message/talk state, no
/// action). `limit_ok` is `mSC_Radio_limit_check()`; `days` is
/// `radiocard->days`; `card` is the selected card ID (1-card branch).
/// Inventory/calendar/RNG mutations stay in C.
#[no_mangle]
pub extern "C" fn pc_taisou_radio_action(
    foreigner: i32,
    time_check: i32,
    num_cards: i32,
    days: i32,
    limit_ok: i32,
    card: i32,
) -> i32 {
    if foreigner != 0 {
        return -1;
    }
    if time_check == RADIO_TIME_SAME_DAY {
        return -2;
    }
    let mut action = ACTION_TOO_LATE;
    if num_cards <= 0 {
        if !(limit_ok == 0 && (days == RADIO_CARD_COUNT || time_check == RADIO_TIME_LESS)) {
            match time_check {
                x if x == RADIO_TIME_LESS => action = ACTION_NEW_CARD,
                x if x == RADIO_TIME_OVER => {
                    action = if days == RADIO_CARD_COUNT - 1 {
                        ACTION_LOST_CARD_AND_FINISH
                    } else if days < RADIO_CARD_COUNT {
                        ACTION_LOST_CARD
                    } else {
                        ACTION_NEW_CARD
                    };
                }
                _ => {}
            }
        }
    } else if num_cards > 1 {
        action = ACTION_DELETE_OLD_CARD;
    } else if time_check == RADIO_TIME_LESS {
        action = ACTION_DELETE_OLD_CARD;
    } else if time_check == RADIO_TIME_OVER {
        let derived = pc_taisou_card_derive_days(card);
        action = if derived >= RADIO_CARD_COUNT {
            ACTION_FINISH_CARD
        } else {
            ACTION_STAMP_CARD
        };
    }
    // NOTE: the 1-card SAME_DAY arm is unreachable in C (the outer
    // else-if already handled SAME_DAY); it would leave TOO_LATE here.
    action
}

/// Dialogue message for the selected action (`mSC_Radio_Set_Talk_Proc`).
/// `calendar_marked` selects the 0x3423/0x342F variants.
#[no_mangle]
pub extern "C" fn pc_taisou_radio_msg(action: i32, calendar_marked: i32) -> i32 {
    match action {
        -1 => MSG_CARD_FOREIGNER,
        -2 => MSG_CARD_SAME_DAY,
        x if x == ACTION_TOO_LATE => MSG_CARD_TOO_LATE,
        x if x == ACTION_DELETE_OLD_CARD => MSG_CARD_DELETE_OLD,
        x if x == ACTION_NEW_CARD => {
            if calendar_marked != 0 {
                MSG_CARD_NEW_MARKED
            } else {
                MSG_CARD_NEW
            }
        }
        x if x == ACTION_LOST_CARD => {
            if calendar_marked != 0 {
                MSG_CARD_LOST_MARKED
            } else {
                MSG_CARD_LOST
            }
        }
        x if x == ACTION_STAMP_CARD => MSG_CARD_STAMP,
        x if x == ACTION_FINISH_CARD => MSG_CARD_FINISH,
        x if x == ACTION_LOST_CARD_AND_FINISH => {
            if calendar_marked != 0 {
                MSG_CARD_LOST_MARKED
            } else {
                MSG_CARD_LOST
            }
        }
        _ => MSG_CARD_TOO_LATE,
    }
}

/// Talk-procedure selection for the action (`mSCR_Talk_Proc`
/// dispatcher indices). `inv_full` is nonzero when no empty
/// normal-condition inventory slot exists.
#[no_mangle]
pub extern "C" fn pc_taisou_radio_talk_proc(action: i32, inv_full: i32) -> i32 {
    match action {
        -1 => TALK_9,
        -2 => TALK_NEXT,
        x if x == ACTION_TOO_LATE => TALK_NEXT,
        x if x == ACTION_DELETE_OLD_CARD => TALK_PICKUP_ALL,
        x if x == ACTION_NEW_CARD => {
            if inv_full != 0 {
                TALK_NEXT
            } else {
                TALK_NEXT_INSPECTION
            }
        }
        x if x == ACTION_LOST_CARD => {
            if inv_full != 0 {
                TALK_NEXT
            } else {
                TALK_NEXT_INSPECTION
            }
        }
        x if x == ACTION_STAMP_CARD => TALK_INSPECTION,
        x if x == ACTION_FINISH_CARD => TALK_INSPECTION,
        x if x == ACTION_LOST_CARD_AND_FINISH => {
            if inv_full != 0 {
                TALK_NEXT
            } else {
                TALK_NEXT_INSPECTION
            }
        }
        _ => TALK_NEXT,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failure_arithmetic() {
        assert_eq!(pc_taisou_failure_rate(0), 0.0);
        assert_eq!(pc_taisou_failure_rate(4), 0.3);
        assert_eq!(pc_taisou_failure_rate(5), 0.0);
        assert_eq!(pc_taisou_failure_rate(-1), 0.0);
        assert_eq!(pc_taisou_next_action(3), 4);
        assert_eq!(pc_taisou_next_action(9), 0);
        // Eligible: no failure active, action not 0/9, float below rate.
        assert!(pc_taisou_failure_eligible(5, 0.04, 0.05, false));
        assert!(!pc_taisou_failure_eligible(5, 0.06, 0.05, false));
        assert!(!pc_taisou_failure_eligible(0, 0.0, 0.3, false));
        assert!(!pc_taisou_failure_eligible(9, 0.0, 0.3, false));
        assert!(!pc_taisou_failure_eligible(5, 0.0, 0.3, true));
        assert!(pc_taisou_failure_reject(5, 5));
        assert!(!pc_taisou_failure_reject(4, 5));
    }

    #[test]
    fn animation_table() {
        assert_eq!(pc_taisou_anim_id(0), 81);
        assert_eq!(pc_taisou_anim_id(7), 85);
        assert_eq!(pc_taisou_anim_id(12), 105);
        assert_eq!(pc_taisou_anim_id(13), -1);
        assert_eq!(pc_taisou_anim_variant(3), 1);
        assert_eq!(pc_taisou_anim_variant(4), 0);
    }

    #[test]
    fn ratio_tables() {
        assert_eq!(pc_taisou_start_ratio(0, 2), 0.93581255);
        // Segment 6 keeps its unsorted order.
        assert_eq!(pc_taisou_start_ratio(6, 2), 1.0);
        assert_eq!(pc_taisou_start_ratio(6, 3), 1.5);
        assert_eq!(pc_taisou_start_ratio(6, 4), 0.96875);
        assert_eq!(pc_taisou_start_ratio(9, 1), 0.96875);
        assert_eq!(pc_taisou_start_ratio(10, 0), -1.0);
        assert_eq!(pc_taisou_start_ratio(0, 3), -1.0);
        assert_eq!(END_RATIO[0], 0.93581255);
        assert_eq!(MORPH_RATIO[0], 0.030187488);
        assert_eq!(RATIO_LENGTH[9], [1.0, 1.0]);
        // Sync frame: start + ((prog - now) - sr) * (end - start) / rl.
        let f = pc_taisou_sync_frame(0.0, 100.0, 0.5, 0.0, 1, 1);
        assert!((f - (0.5 - 0.25) * 100.0 / 0.25).abs() < 1e-4);
        assert_eq!(pc_taisou_sync_frame(0.0, 100.0, 0.5, 0.0, 10, 0), -1.0);
    }

    #[test]
    fn delay_math() {
        assert!(pc_taisou_has_delay(0xD04F));
        assert!(!pc_taisou_has_delay(0xD04E));
        let mut out = [0.0f32; 2];
        // With delay: decrement, save ratio*0.2, scale by 0.8.
        let d = pc_taisou_delay_step(1.0, 10, 0.0, true, out.as_mut_ptr());
        assert_eq!(d, 9);
        assert_eq!(out, [0.2, 0.8]);
        // Without delay: subtract saved ratio.
        let d = pc_taisou_delay_step(1.0, 0, 0.3, true, out.as_mut_ptr());
        assert_eq!(d, 0);
        assert_eq!(out, [0.3, 0.7]);
        // Over-block: adjusted > startRatio[seg][seq+1].
        assert!(pc_taisou_over_block(0.6, 1, 1)); // > 0.5
        assert!(!pc_taisou_over_block(0.5, 1, 1));
        assert!(!pc_taisou_over_block(0.6, 1, 4)); // seq+1 OOB
    }

    #[test]
    fn dialogue() {
        assert_eq!(pc_taisou_msg_row(true), 0);
        assert_eq!(pc_taisou_msg_row(false), 1);
        assert_eq!(pc_taisou_npc_type(0xD050), 2);
        // Villager: base + type*3 + rand2; action 10 fixes +2.
        assert_eq!(pc_taisou_villager_msg(0, 0, 2, 1, 5), 6716 + 6 + 1);
        assert_eq!(pc_taisou_villager_msg(0, 0, 2, 1, 10), 6716 + 6 + 2);
        assert_eq!(pc_taisou_villager_msg(1, 0, 2, 1, 5), 6806 + 6 + 1);
        assert_eq!(pc_taisou_copper_msg(0, 2), 9829 + 2);
        assert_eq!(pc_taisou_copper_msg(1, 2), 9832 + 2);
        assert_eq!(pc_taisou_init_angle(0xD04E), 32768);
        assert_eq!(pc_taisou_init_angle(0xD04F), 0);
        assert_eq!(pc_taisou_leader_update(0xD053, 0xD050), 0xD050);
        assert_eq!(pc_taisou_leader_update(0xD04E, 0xD050), 0xD04E);
    }

    #[test]
    fn spawn_layout() {
        let mut out = [0i32; 2];
        assert_eq!(pc_taisou_spawn_pos(0, out.as_mut_ptr()), 1);
        assert_eq!(out, [4, 10]);
        assert_eq!(pc_taisou_spawn_pos(6, out.as_mut_ptr()), 1);
        assert_eq!(out, [9, 8]);
        assert_eq!(pc_taisou_spawn_pos(7, out.as_mut_ptr()), 0);
    }

    #[test]
    fn radio_counter_kernels() {
        // Availability: enabled + 0xDA.
        assert_eq!(pc_taisou_radio_available(1, 0xDA), 1);
        assert_eq!(pc_taisou_radio_available(0, 0xDA), 0);
        assert_eq!(pc_taisou_radio_available(1, 0xD9), 0);
        // Conversion: first measure, boundary, later measures, measure 9.
        let mut m: i8 = -1;
        let mut p: f32 = -1.0;
        pc_taisou_radio_convert(0, &mut m, &mut p);
        assert_eq!((m, p), (0, 0.0));
        pc_taisou_radio_convert(743, &mut m, &mut p);
        assert_eq!(m, 0);
        assert!((p - 743.0 / 768.0).abs() < 1e-6);
        pc_taisou_radio_convert(744, &mut m, &mut p);
        assert_eq!((m, p), (1, 0.0));
        pc_taisou_radio_convert(744 + 768, &mut m, &mut p);
        assert_eq!((m, p), (2, 0.0));
        // Measure 9 starts at 744 + 8*768 and uses the 216 denominator.
        pc_taisou_radio_convert(744 + 8 * 768, &mut m, &mut p);
        assert_eq!((m, p), (9, 0.0));
        pc_taisou_radio_convert(744 + 8 * 768 + 108, &mut m, &mut p);
        assert_eq!(m, 9);
        assert!((p - 0.5).abs() < 1e-6);
        // Tempo: integer division by 48.
        assert_eq!(pc_taisou_radio_tempo(96), 2);
        assert_eq!(pc_taisou_radio_tempo(100), 2);
        // Leader cull: not-NO_CULL + block mismatch.
        assert_eq!(pc_taisou_leader_cull(0, 3, 5), 1);
        assert_eq!(pc_taisou_leader_cull(1, 3, 5), 0);
        assert_eq!(pc_taisou_leader_cull(0, 5, 5), 0);
        // Regist drain: clamp + reset signal.
        let mut r = 0;
        assert_eq!(pc_taisou_regist_drain(3, &mut r), 2);
        assert_eq!(r, 0);
        assert_eq!(pc_taisou_regist_drain(1, &mut r), 0);
        assert_eq!(r, 1);
        // Failure type: idx, clamped above 4 (Tortimer 0xD078 -> 0).
        assert_eq!(pc_taisou_failure_type(0xD04E), 0);
        assert_eq!(pc_taisou_failure_type(0xD052), 4);
        assert_eq!(pc_taisou_failure_type(0xD078), 0);
    }

    #[test]
    fn radio_card_kernels() {
        // Time check: year mismatch -> LESS; else the IsEqualDate map.
        assert_eq!(pc_taisou_radio_time_check(2026, 2025, 1), 0);
        assert_eq!(pc_taisou_radio_time_check(2026, 2026, -1), 0);
        assert_eq!(pc_taisou_radio_time_check(2026, 2026, 0), 1);
        assert_eq!(pc_taisou_radio_time_check(2026, 2026, 1), 2);
        // Limit: month>=8 && day>=19 -> 0 (past the season).
        assert_eq!(pc_taisou_limit_check(8, 19), 0);
        assert_eq!(pc_taisou_limit_check(9, 1), 1); // conjunction quirk
        assert_eq!(pc_taisou_limit_check(7, 30), 1);
        // Card IDs: CARD00 = 0x2523; lost increments days first.
        assert_eq!(pc_taisou_card_lost_id(5), 0x2523 + 6);
        assert_eq!(pc_taisou_card_stamp_id(5), 0x2523 + 5);
        assert_eq!(pc_taisou_card_derive_days(0x2523 + 4), 5);
        // Action tree.
        assert_eq!(pc_taisou_radio_action(1, 2, 0, 0, 1, 0), -1); // foreigner
        assert_eq!(pc_taisou_radio_action(0, 1, 0, 0, 1, 0), -2); // same day
        assert_eq!(pc_taisou_radio_action(0, 0, 0, 0, 1, 0), 2); // no cards, LESS -> NEW
        assert_eq!(pc_taisou_radio_action(0, 0, 0, 0, 0, 0), 0); // no cards, LESS, past limit -> TOO_LATE
        assert_eq!(pc_taisou_radio_action(0, 2, 0, 12, 1, 0), 6); // days==12 OVER -> LOST_AND_FINISH
        assert_eq!(pc_taisou_radio_action(0, 2, 0, 5, 1, 0), 3); // OVER -> LOST
        assert_eq!(pc_taisou_radio_action(0, 2, 0, 13, 1, 0), 2); // days==13 OVER -> NEW
        assert_eq!(pc_taisou_radio_action(0, 0, 2, 0, 1, 0), 1); // >1 card -> DELETE
        assert_eq!(pc_taisou_radio_action(0, 0, 1, 0, 1, 0), 1); // 1 card LESS -> DELETE
        assert_eq!(pc_taisou_radio_action(0, 2, 1, 0, 1, 0x2523 + 12), 5); // derive 13 -> FINISH
        assert_eq!(pc_taisou_radio_action(0, 2, 1, 0, 1, 0x2523 + 4), 4); // derive 6 -> STAMP
        // Messages.
        assert_eq!(pc_taisou_radio_msg(-1, 0), 0x343B);
        assert_eq!(pc_taisou_radio_msg(-2, 0), 0x3433);
        assert_eq!(pc_taisou_radio_msg(2, 0), 0x3422);
        assert_eq!(pc_taisou_radio_msg(2, 1), 0x3423);
        assert_eq!(pc_taisou_radio_msg(3, 1), 0x342F);
        assert_eq!(pc_taisou_radio_msg(4, 0), 0x3427);
        assert_eq!(pc_taisou_radio_msg(5, 0), 0x342A);
        // Talk procedures.
        assert_eq!(pc_taisou_radio_talk_proc(-1, 0), 9);
        assert_eq!(pc_taisou_radio_talk_proc(0, 0), 6);
        assert_eq!(pc_taisou_radio_talk_proc(1, 0), 0);
        assert_eq!(pc_taisou_radio_talk_proc(2, 1), 6);
        assert_eq!(pc_taisou_radio_talk_proc(2, 0), 2);
        assert_eq!(pc_taisou_radio_talk_proc(4, 0), 1);
    }
}
