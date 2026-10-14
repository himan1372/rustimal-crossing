//! Villager memory & identity bookkeeping — pure kernels.
//!
//! Decomp sources: `src/game/m_npc.c`, `src/game/m_private.c`,
//! `src/game/m_land.c`, `src/lb_rtc.c` (GAFE01_00 Rev 0 USA).
//!
//! This covers the portable half of the "Still in C" villager list:
//! per-villager memory-slot selection (`Anmmem_c` records), the friendship
//! arithmetic, the eviction policy, and the PersonalID/land-name identity
//! comparisons. What stays in C by design: RNG draws, RTC reads, save
//! mutation (the actual `mNpc_ClearAnimalMemory` writes, bells/item grants),
//! actor lookups (worn-cloth fallbacks), and the shop-item RNG loop.
//!
//! ## Quirks preserved
//!
//! - `mNpc_AddFriendship` has a documented `@BUG`: it dereferences `memory`
//!   to read `friendship` *before* the NULL check, so retail crashes on a
//!   NULL pointer and the check is dead code. Our kernel takes the value by
//!   value, so it is inherently NULL-safe; the clamp behavior is identical.
//! - `lbRTC_IsOverTime`'s doc comment claims it returns `LESS` when
//!   `t0 <= t1`, but the code returns `LESS` iff `t1 < t0` (equal times give
//!   `OVER`). We port the code's behavior, not the comment.
//! - `mPr_CheckCmpPlayerName` / `mLd_CheckCmpLandName` return FALSE when
//!   *either* name is blank — two blank names do NOT compare equal.
//! - `mNpc_GetOldAnimalMemoryIdx` seeds from the clear code (all `0xFF`,
//!   i.e. the maximum time), so a never-spoken slot is never picked as
//!   "oldest" unless every slot is clear; empty input returns 0.

/// `lbRTC_LESS` — returned by [`rtc_is_over_time`] when `t1` is strictly
/// earlier than `t0`.
pub const RTC_LESS: i32 = -1;
/// `lbRTC_OVER` — returned by [`rtc_is_over_time`] when `t1` is equal to or
/// later than `t0`.
pub const RTC_OVER: i32 = 1;

/// `CHAR_SPACE` — the blank name character (`include/m_font.h`).
pub const CHAR_SPACE: u8 = 32;
/// `LAND_NAME_SIZE` — 8 bytes (`include/m_land_h.h`).
pub const LAND_NAME_SIZE: usize = 8;
/// `PLAYER_NAME_LEN` — 8 bytes (`include/m_personal_id.h`).
pub const PLAYER_NAME_LEN: usize = 8;
/// `ANIMAL_MEMORY_NUM` — 7 memory slots per villager (`include/m_npc.h`).
pub const ANIMAL_MEMORY_NUM: usize = 7;
/// Null-land sentinel: `land_id == 0xFFFF` with a blank name.
pub const NULL_LAND_ID: u16 = 0xFFFF;
/// Friendship clamp bounds from `mNpc_AddFriendship`.
pub const FRIENDSHIP_MIN: i32 = 0;
pub const FRIENDSHIP_MAX: i32 = 127;

/// `lbRTC_time_c` — only the fields the kernels compare.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RtcTime {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

impl RtcTime {
    /// `mTM_rtcTime_clear_code` — all `0xFF`, the "never set" sentinel that
    /// also acts as the maximum time in oldest-slot selection.
    pub const CLEAR: RtcTime = RtcTime {
        year: 0xFFFF,
        month: 0xFF,
        day: 0xFF,
        hour: 0xFF,
        minute: 0xFF,
        second: 0xFF,
    };
}

/// `PersonalID_c` — player identity (land + player).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PersonalId {
    pub land_name: [u8; LAND_NAME_SIZE],
    pub land_id: u16,
    pub player_id: u16,
    pub player_name: [u8; PLAYER_NAME_LEN],
}

/// The fields of `Anmmem_c` (animal memory record) that the bookkeeping
/// kernels read. Save mutation itself stays in C.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AnimalMemorySlot {
    pub player_id: PersonalId,
    pub last_speak_time: RtcTime,
    pub friendship: i8,
    pub letter_exists: bool,
}

/// Port of `lbRTC_IsOverTime` (`src/lb_rtc.c`).
///
/// Returns [`RTC_LESS`] iff `t1` is strictly earlier than `t0`
/// (lexicographic year → month → day → hour → minute → second),
/// otherwise [`RTC_OVER`]. The nested retail code is exactly this
/// comparison unrolled; equal times return `OVER` (the source doc comment
/// claiming `LESS` for `t0 <= t1` is wrong — the code is authoritative).
pub fn rtc_is_over_time(t0: &RtcTime, t1: &RtcTime) -> i32 {
    let a = (t0.year, t0.month, t0.day, t0.hour, t0.minute, t0.second);
    let b = (t1.year, t1.month, t1.day, t1.hour, t1.minute, t1.second);
    if b < a {
        RTC_LESS
    } else {
        RTC_OVER
    }
}

/// Port of `mPr_NullCheckPlayerName` — a name is null when every byte is
/// `CHAR_SPACE`.
pub fn null_check_player_name(name: &[u8; PLAYER_NAME_LEN]) -> bool {
    name.iter().all(|&c| c == CHAR_SPACE)
}

/// Port of `mLd_NullCheckLandName` — same all-spaces rule for land names.
pub fn null_check_land_name(name: &[u8; LAND_NAME_SIZE]) -> bool {
    name.iter().all(|&c| c == CHAR_SPACE)
}

/// Port of `mPr_NullCheckPersonalID` — null PID is `land_id == 0xFFFF` with a
/// blank player name.
pub fn null_check_personal_id(pid: &PersonalId) -> bool {
    pid.land_id == NULL_LAND_ID && null_check_player_name(&pid.player_name)
}

/// Port of `mPr_CheckCmpPlayerName` — both names non-blank and byte-equal.
/// Note: two blank names do NOT compare equal.
pub fn check_cmp_player_name(a: &[u8; PLAYER_NAME_LEN], b: &[u8; PLAYER_NAME_LEN]) -> bool {
    !null_check_player_name(a) && !null_check_player_name(b) && a == b
}

/// Port of `mLd_CheckCmpLandName` — both names non-blank and byte-equal.
pub fn check_cmp_land_name(a: &[u8; LAND_NAME_SIZE], b: &[u8; LAND_NAME_SIZE]) -> bool {
    !null_check_land_name(a) && !null_check_land_name(b) && a == b
}

/// Port of `mLd_CheckCmpLand` — land id plus land name both match.
pub fn check_cmp_land(
    name_a: &[u8; LAND_NAME_SIZE],
    id_a: u16,
    name_b: &[u8; LAND_NAME_SIZE],
    id_b: u16,
) -> bool {
    id_a == id_b && check_cmp_land_name(name_a, name_b)
}

/// Port of `mPr_CheckCmpPersonalID` — full identity equality: land id,
/// player id, land name, and player name.
pub fn check_cmp_personal_id(a: &PersonalId, b: &PersonalId) -> bool {
    a.land_id == b.land_id
        && a.player_id == b.player_id
        && check_cmp_land_name(&a.land_name, &b.land_name)
        && check_cmp_player_name(&a.player_name, &b.player_name)
}

/// Port of `mNpc_AddFriendship` — `friendship + amount` clamped to
/// `[0, 127]`.
///
/// Retail quirk: the C code dereferences `memory` *before* its NULL check
/// (documented `@BUG`), so a NULL pointer crashes retail. This kernel takes
/// the value by value and is inherently safe; the arithmetic is identical.
pub fn add_friendship(current: i8, amount: i32) -> i8 {
    let v = current as i32 + amount;
    v.clamp(FRIENDSHIP_MIN, FRIENDSHIP_MAX) as i8
}

/// Port of `mNpc_CheckFreeAnimalMemory` — a slot is free when its
/// `memory_player_id` is a null PID.
pub fn check_free_animal_memory(slot: &AnimalMemorySlot) -> bool {
    null_check_personal_id(&slot.player_id)
}

/// Port of `mNpc_GetFreeAnimalMemoryIdx` — index of the first free slot, or
/// `None` (retail `-1`).
pub fn get_free_animal_memory_idx(slots: &[AnimalMemorySlot]) -> Option<usize> {
    slots.iter().position(check_free_animal_memory)
}

/// Port of `mNpc_GetAnimalMemoryIdx` — index of the slot belonging to `pid`,
/// or `None` (retail `-1`).
pub fn get_animal_memory_idx(pid: &PersonalId, slots: &[AnimalMemorySlot]) -> Option<usize> {
    slots.iter().position(|s| check_cmp_personal_id(&s.player_id, pid))
}

/// Port of `mNpc_GetOldAnimalMemoryIdx` — index of the slot with the oldest
/// (least recent) `last_speak_time`. Seeded from the clear code, so cleared
/// slots never win unless all are clear. Empty input returns 0, like retail.
pub fn get_old_animal_memory_idx(slots: &[AnimalMemorySlot]) -> usize {
    let mut oldest = RtcTime::CLEAR;
    let mut res = 0usize;
    for (i, slot) in slots.iter().enumerate() {
        // LESS iff slot time < oldest, i.e. slot is older.
        if rtc_is_over_time(&oldest, &slot.last_speak_time) == RTC_LESS {
            res = i;
            oldest = slot.last_speak_time;
        }
    }
    res
}

/// Port of `mNpc_GetOldPlayerAnimalMemoryIdx` — index of a slot that belongs
/// to this town but whose player no longer exists (deleted player), or
/// `None` (retail `-1`). `players` is the town's current player list
/// (retail reads `Save_Get(private_data)`; null-PID entries are skipped).
pub fn get_old_player_animal_memory_idx(
    slots: &[AnimalMemorySlot],
    town_land_name: &[u8; LAND_NAME_SIZE],
    town_land_id: u16,
    players: &[PersonalId],
) -> Option<usize> {
    slots.iter().position(|slot| {
        if check_free_animal_memory(slot) {
            return false;
        }
        if !check_cmp_land(
            &slot.player_id.land_name,
            slot.player_id.land_id,
            town_land_name,
            town_land_id,
        ) {
            return false;
        }
        // Slot belongs to this town — evictable only if no current player
        // matches it.
        !players.iter().any(|p| {
            !null_check_personal_id(p) && check_cmp_personal_id(p, &slot.player_id)
        })
    })
}

/// Port of the *selection* half of `mNpc_ForceGetFreeAnimalMemoryIdx` —
/// which memory slot to evict when a villager needs a new player memory.
///
/// Eviction order (exactly retail's):
/// 1. First free slot.
/// 2. Otherwise the "cheapest" foreign (out-of-town) memory: no-letter
///    beats has-letter; then lower friendship wins; then the oldest
///    (least recently spoken to) wins.
/// 3. Otherwise a this-town memory of a deleted player.
/// 4. Otherwise slot 0 ("all else failed").
///
/// Retail then clears the chosen slot (`mNpc_ClearAnimalMemory` or the
/// island variant) — that save mutation stays in C; this kernel returns the
/// chosen index.
pub fn force_get_free_animal_memory_idx(
    slots: &[AnimalMemorySlot],
    town_land_name: &[u8; LAND_NAME_SIZE],
    town_land_id: u16,
    players: &[PersonalId],
) -> usize {
    if let Some(idx) = get_free_animal_memory_idx(slots) {
        return idx;
    }

    let mut chosen: Option<usize> = None;
    for (i, slot) in slots.iter().enumerate() {
        let is_this_land = check_cmp_land(
            &slot.player_id.land_name,
            slot.player_id.land_id,
            town_land_name,
            town_land_id,
        );
        if is_this_land {
            continue;
        }
        match chosen {
            None => chosen = Some(i),
            Some(c) => {
                let cand = &slots[c];
                if cand.letter_exists == slot.letter_exists {
                    if cand.friendship > slot.friendship {
                        chosen = Some(i);
                    } else if cand.friendship == slot.friendship {
                        // LESS iff slot is older than the candidate.
                        if rtc_is_over_time(&cand.last_speak_time, &slot.last_speak_time)
                            == RTC_LESS
                        {
                            chosen = Some(i);
                        }
                    }
                } else if !slot.letter_exists {
                    chosen = Some(i);
                }
            }
        }
    }

    if let Some(idx) = chosen {
        return idx;
    }

    get_old_player_animal_memory_idx(slots, town_land_name, town_land_id, players)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blank_pid() -> PersonalId {
        PersonalId {
            land_name: [CHAR_SPACE; LAND_NAME_SIZE],
            land_id: NULL_LAND_ID,
            player_id: 0,
            player_name: [CHAR_SPACE; PLAYER_NAME_LEN],
        }
    }

    fn pid(land: &[u8; 8], land_id: u16, player_id: u16, name: &[u8; 8]) -> PersonalId {
        PersonalId {
            land_name: *land,
            land_id,
            player_id,
            player_name: *name,
        }
    }

    fn slot(pid: PersonalId, year: u16, friendship: i8, letter: bool) -> AnimalMemorySlot {
        AnimalMemorySlot {
            player_id: pid,
            last_speak_time: RtcTime {
                year,
                month: 1,
                day: 1,
                hour: 0,
                minute: 0,
                second: 0,
            },
            friendship,
            letter_exists: letter,
        }
    }

    #[test]
    fn rtc_equal_times_give_over_not_less() {
        // Documents the doc-comment bug: equal times return OVER.
        let t = RtcTime {
            year: 2002,
            month: 5,
            day: 1,
            hour: 12,
            minute: 30,
            second: 0,
        };
        assert_eq!(rtc_is_over_time(&t, &t), RTC_OVER);
    }

    #[test]
    fn rtc_earlier_t1_gives_less_at_each_field() {
        let base = RtcTime {
            year: 2002,
            month: 5,
            day: 10,
            hour: 12,
            minute: 30,
            second: 45,
        };
        let earlier_year = RtcTime { year: 2001, ..base };
        let earlier_month = RtcTime { month: 4, ..base };
        let earlier_day = RtcTime { day: 9, ..base };
        let earlier_hour = RtcTime { hour: 11, ..base };
        let earlier_min = RtcTime { minute: 29, ..base };
        let earlier_sec = RtcTime { second: 44, ..base };
        for e in [
            earlier_year,
            earlier_month,
            earlier_day,
            earlier_hour,
            earlier_min,
            earlier_sec,
        ] {
            assert_eq!(rtc_is_over_time(&base, &e), RTC_LESS);
            assert_eq!(rtc_is_over_time(&e, &base), RTC_OVER);
        }
    }

    #[test]
    fn null_checks() {
        assert!(null_check_player_name(&[CHAR_SPACE; PLAYER_NAME_LEN]));
        assert!(!null_check_player_name(b"phil    "));
        assert!(null_check_land_name(&[CHAR_SPACE; LAND_NAME_SIZE]));
        assert!(null_check_personal_id(&blank_pid()));
        let mut p = blank_pid();
        p.land_id = 1;
        assert!(!null_check_personal_id(&p));
    }

    #[test]
    fn blank_names_never_compare_equal() {
        let blank = [CHAR_SPACE; PLAYER_NAME_LEN];
        assert!(!check_cmp_player_name(&blank, &blank));
        assert!(!check_cmp_land_name(
            &[CHAR_SPACE; LAND_NAME_SIZE],
            &[CHAR_SPACE; LAND_NAME_SIZE]
        ));
        assert!(check_cmp_player_name(b"phil    ", b"phil    "));
        assert!(!check_cmp_player_name(b"phil    ", b"bob     "));
    }

    #[test]
    fn personal_id_equality_needs_all_fields() {
        let land = *b"townland";
        let a = pid(&land, 7, 1, b"phil    ");
        assert!(check_cmp_personal_id(&a, &a));
        let diff_player = pid(&land, 7, 2, b"phil    ");
        assert!(!check_cmp_personal_id(&a, &diff_player));
        let diff_land = pid(&land, 8, 1, b"phil    ");
        assert!(!check_cmp_personal_id(&a, &diff_land));
        let diff_name = pid(&land, 7, 1, b"bob     ");
        assert!(!check_cmp_personal_id(&a, &diff_name));
    }

    #[test]
    fn friendship_clamps() {
        assert_eq!(add_friendship(100, 50), 127);
        assert_eq!(add_friendship(10, -50), 0);
        assert_eq!(add_friendship(60, 5), 65);
        assert_eq!(add_friendship(127, 1), 127);
        assert_eq!(add_friendship(0, -1), 0);
        // Negative friendship values clamp up (retail s8 can hold them).
        assert_eq!(add_friendship(-5, 0), 0);
    }

    #[test]
    fn free_slot_selection() {
        let land = *b"townland";
        let used = slot(pid(&land, 7, 1, b"phil    "), 2002, 10, false);
        let free = slot(blank_pid(), 2002, 0, false);
        let slots = [used, free, used];
        assert!(!check_free_animal_memory(&used));
        assert!(check_free_animal_memory(&free));
        assert_eq!(get_free_animal_memory_idx(&slots), Some(1));
        assert_eq!(get_free_animal_memory_idx(&[used, used]), None);
        assert_eq!(
            get_animal_memory_idx(&pid(&land, 7, 1, b"phil    "), &slots),
            Some(0)
        );
        assert_eq!(
            get_animal_memory_idx(&pid(&land, 7, 9, b"nobody  "), &slots),
            None
        );
    }

    #[test]
    fn oldest_slot_is_least_recent() {
        let land = *b"townland";
        let mk = |y| slot(pid(&land, 7, 1, b"phil    "), y, 10, false);
        let slots = [mk(2003), mk(2001), mk(2002)];
        assert_eq!(get_old_animal_memory_idx(&slots), 1);
        // Cleared (never-spoken) slots never win while a real time exists.
        let mut with_clear = slots;
        with_clear[0].last_speak_time = RtcTime::CLEAR;
        assert_eq!(get_old_animal_memory_idx(&with_clear), 1);
        // Empty input returns 0 like retail.
        assert_eq!(get_old_animal_memory_idx(&[]), 0);
    }

    #[test]
    fn force_evict_prefers_free_then_foreign() {
        let town = *b"townland";
        let away = *b"far away";
        let mk = |land: &[u8; 8], lid, y, f, l| slot(pid(land, lid, 1, b"phil    "), y, f, l);
        // Free slot wins outright.
        let slots = [mk(&town, 7, 2002, 50, false), mk(&away, 9, 2001, 1, false)];
        let mut with_free = slots;
        with_free[1] = slot(blank_pid(), 2002, 0, false);
        assert_eq!(
            force_get_free_animal_memory_idx(&with_free, &town, 7, &[]),
            1
        );
        // No free slot: foreign memory evicted over this-town memory.
        let slots2 = [mk(&town, 7, 2001, 1, false), mk(&away, 9, 2003, 100, false)];
        assert_eq!(
            force_get_free_animal_memory_idx(&slots2, &town, 7, &[]),
            1
        );
    }

    #[test]
    fn force_evict_foreign_tiebreaks() {
        let town = *b"townland";
        let away = *b"far away";
        let mk = |y, f, l| slot(pid(&away, 9, 1, b"phil    "), y, f, l);
        // Lower friendship evicted first.
        let slots = [mk(2002, 100, false), mk(2002, 5, false)];
        assert_eq!(force_get_free_animal_memory_idx(&slots, &town, 7, &[]), 1);
        // Equal friendship: older (least recently spoken) evicted.
        let slots = [mk(2003, 50, false), mk(2001, 50, false)];
        assert_eq!(force_get_free_animal_memory_idx(&slots, &town, 7, &[]), 1);
        // No-letter beats has-letter regardless of friendship.
        let slots = [mk(2001, 1, true), mk(2003, 127, false)];
        assert_eq!(force_get_free_animal_memory_idx(&slots, &town, 7, &[]), 1);
        // First foreign candidate sticks when the challenger has a letter
        // and the candidate doesn't (challenger can't displace).
        let slots = [mk(2003, 127, false), mk(2001, 1, true)];
        assert_eq!(force_get_free_animal_memory_idx(&slots, &town, 7, &[]), 0);
    }

    #[test]
    fn force_evict_falls_back_to_deleted_player_then_zero() {
        let town = *b"townland";
        let live = pid(&town, 7, 1, b"phil    ");
        let gone = pid(&town, 7, 2, b"bob     ");
        let s_live = slot(live, 2002, 50, false);
        let s_gone = slot(gone, 2001, 60, false);
        // Deleted player's slot is reclaimed.
        let slots = [s_live, s_gone];
        assert_eq!(
            force_get_free_animal_memory_idx(&slots, &town, 7, &[live]),
            1
        );
        // Nobody deleted and nothing foreign: slot 0.
        assert_eq!(
            force_get_free_animal_memory_idx(&slots, &town, 7, &[live, gone]),
            0
        );
    }

    #[test]
    fn old_player_idx_skips_live_and_foreign() {
        let town = *b"townland";
        let away = *b"far away";
        let live = pid(&town, 7, 1, b"phil    ");
        let gone = pid(&town, 7, 2, b"bob     ");
        let foreign = pid(&away, 9, 3, b"zed     ");
        let slots = [
            slot(live, 2002, 50, false),
            slot(foreign, 2001, 60, false),
            slot(gone, 2001, 60, false),
        ];
        assert_eq!(
            get_old_player_animal_memory_idx(&slots, &town, 7, &[live]),
            Some(2)
        );
        assert_eq!(
            get_old_player_animal_memory_idx(&slots, &town, 7, &[live, gone]),
            None
        );
    }
}
