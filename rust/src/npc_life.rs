//! Villager life-cycle rules from `m_npc.c` not yet in Rust: best friend
//! and highest friendship, who moves out next, whether a transferring
//! villager may move in and which slot it takes, the "has appeared"
//! bitfield that stops repeat move-ins, and present-clothing swaps.
//! Talk/patience timers are already in `dialogue_topics.rs` /
//! `interaction.rs`; friendship clamping is in `npc.rs`.
//!
//! Source-verified against ac-decomp `src/game/m_npc.c` + `include/m_npc.h`
//! (primary) and ACGC-PC-Port (secondary: identical for these functions,
//! including the quirks below).
//!
//! ## Facts
//! - Best friend (`mNpc_GetAnimalMemoryBestFriend`): the highest friendship
//!   memory, which must be at least 80. On a tie, a memory with a letter
//!   beats one without; with equal letter state, the memory spoken to
//!   EARLIER wins (`lbRTC_IsOverTime(best, mem) == LESS` means mem is older).
//! - Highest friendship (`mNpc_GetHighestFriendshipIdx`): uses `<=`, so the
//!   LAST memory among equals wins; a 0-friendship memory still counts.
//! - Move-out choice (`mNpc_SetRemoveAnimalNo`): only with more than 5
//!   villagers and no pending choice (0xFF). Pass 1: a villager every
//!   player has met; pass 2: one "some player" has met; pass 3: any
//!   villager. Each pass picks uniformly with `RANDOM`.
//! - Move-in (`mNpc_GetRemoveAnimal`, foreign town side): rejected if the
//!   villager is uninitialised or itself moving, already lives here, was the
//!   last villager to leave, or is the current summer camper. Slot: the
//!   first free one (town grows); otherwise the goodbye villager, otherwise
//!   slot 0 (the replaced villager's house is destroyed and a goodbye letter
//!   sent). The new villager has no house yet (home 0xFF).
//! - Has-appeared table: one bit per NPC id (`npc_id & 0xFFF`); reading an
//!   index past the table returns TRUE. When every starter/move-in villager
//!   has appeared, the table is cleared and refilled with current villagers.
//! - Present clothing: once per day a villager with a gifted shirt
//!   (`present_cloth`, an ITEM1 cloth) whose memory says `has_present_cloth`
//!   puts it on (the first such memory flips to `wearing_present_cloth`).
//!
//! ## Quirks
//! - Pass 2 "met by some player" stops counting at the first player who has
//!   NOT met the villager, so it really means "met by the first existing
//!   player (in save order)".
//! - `mNpc_DecideRemoveAnimalNo` tests `ignored_idx >= 0 || ignored_idx < 15`
//!   (always true), so the count is always reduced by one. With
//!   `ignored_idx = -1` the last villager in slot order can never be picked.

pub const ANIMAL_NUM_MAX: usize = 15;
pub const ANIMAL_NUM_MIN: usize = 5;
pub const ANIMAL_MEMORY_NUM: usize = 7;
pub const BEST_FRIEND_MIN: i8 = 80;
pub const NO_REMOVE: u8 = 0xFF;

/// One `Anmmem_c` reduced to what these rules read.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Memory {
    /// Player this memory belongs to (None = free slot).
    pub player: Option<u8>,
    pub friendship: i8,
    pub letter_exists: bool,
    /// Last-talk time as a comparable timestamp.
    pub last_speak: u64,
    pub has_present_cloth: bool,
    pub wearing_present_cloth: bool,
}

/// `mNpc_GetHighestFriendshipIdx`.
pub fn highest_friendship(mems: &[Memory]) -> Option<usize> {
    let mut max = 0i8;
    let mut res = None;
    for (i, m) in mems.iter().enumerate() {
        if m.player.is_some() && max <= m.friendship {
            max = m.friendship;
            res = Some(i);
        }
    }
    res
}

/// `mNpc_GetAnimalMemoryBestFriend` (+ `mNpc_SelectBestFriend`).
pub fn best_friend(mems: &[Memory]) -> Option<usize> {
    let mut best: Option<usize> = None;
    let mut best_f = 0i8;
    let mut res = None;
    for (i, m) in mems.iter().enumerate() {
        if m.player.is_none() {
            continue;
        }
        let take = if m.friendship > best_f {
            best_f = m.friendship;
            true
        } else if m.friendship == best_f {
            match best {
                Some(b) => {
                    let b = &mems[b];
                    if b.letter_exists == m.letter_exists {
                        m.last_speak < b.last_speak
                    } else {
                        m.letter_exists
                    }
                }
                None => true,
            }
        } else {
            false
        };
        if take {
            best = Some(i);
            res = Some(i);
        }
    }
    res.filter(|&i| mems[i].friendship >= BEST_FRIEND_MIN)
}

/// A villager as the move-out rule sees it: which players it remembers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Resident {
    pub exists: bool,
    /// Bit p set = has a memory of player p.
    pub knows: u8,
}

/// `mNpc_DecideRemoveAnimalNo_Friend`. `players` = existing player slots
/// in save order.
fn remove_by_friend(animals: &[Resident; ANIMAL_NUM_MAX], ignored: i32, players: &[u8], met_all: bool, random: &mut impl FnMut(i32) -> i32) -> Option<usize> {
    if players.is_empty() {
        return None;
    }
    let mut cands = Vec::new();
    for (i, a) in animals.iter().enumerate() {
        if !a.exists || i as i32 == ignored {
            continue;
        }
        let met = players.iter().take_while(|&&p| (a.knows >> p) & 1 == 1).count();
        if (met_all && met == players.len()) || (!met_all && met > 0) {
            cands.push(i);
        }
    }
    if cands.is_empty() {
        return None;
    }
    cands.get(random(cands.len() as i32) as usize).copied()
}

/// `mNpc_DecideRemoveAnimalNo` (count always reduced by one).
fn remove_any(animals: &[Resident; ANIMAL_NUM_MAX], ignored: i32, now_npc_max: i32, random: &mut impl FnMut(i32) -> i32) -> Option<usize> {
    let n = now_npc_max - 1;
    if n <= 0 || n > ANIMAL_NUM_MAX as i32 {
        return None;
    }
    let mut sel = random(n);
    for (i, a) in animals.iter().enumerate() {
        if a.exists {
            if sel <= 0 && ignored != i as i32 {
                return Some(i);
            }
            sel -= 1;
        }
    }
    None
}

/// `mNpc_SetRemoveAnimalNo`: returns the new `remove_animal_no`.
pub fn choose_move_out(current: u8, animals: &[Resident; ANIMAL_NUM_MAX], ignored: i32, players: &[u8], now_npc_max: u8, random: &mut impl FnMut(i32) -> i32) -> u8 {
    if now_npc_max as usize <= ANIMAL_NUM_MIN || current != NO_REMOVE {
        return current;
    }
    remove_by_friend(animals, ignored, players, true, random)
        .or_else(|| remove_by_friend(animals, ignored, players, false, random))
        .or_else(|| remove_any(animals, ignored, now_npc_max as i32, random))
        .map_or(current, |i| i as u8)
}

/// `mNpc_GetRemoveAnimal` eligibility of an incoming villager.
pub fn may_move_in(incoming_valid: bool, incoming_removing: bool, incoming_id: u16, town_ids: &[u16], last_removed: u16, summer_camper: Option<u16>) -> bool {
    incoming_valid && !incoming_removing && !town_ids.contains(&incoming_id) && incoming_id != last_removed && summer_camper != Some(incoming_id)
}

/// Where an accepted villager goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoveInSlot {
    /// Free slot; `now_npc_max` grows.
    Free(usize),
    /// Replaces a resident (house destroyed, goodbye letter sent).
    Replace(usize),
}

pub fn move_in_slot(free: Option<usize>, goodbye: Option<usize>) -> MoveInSlot {
    match free {
        Some(i) => MoveInSlot::Free(i),
        None => MoveInSlot::Replace(goodbye.unwrap_or(0)),
    }
}

/// `npc_used_tbl`: one bit per NPC index.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HaveAppeared(pub Vec<u8>);

impl HaveAppeared {
    pub fn new(bytes: usize) -> Self {
        Self(vec![0; bytes])
    }

    /// `mNpc_SetHaveAppeared` (`idx` = `npc_id & 0xFFF`).
    pub fn set(&mut self, idx: usize) {
        if let Some(b) = self.0.get_mut(idx / 8) {
            *b |= 1 << (idx & 7);
        }
    }

    /// `mNpc_GetHaveAppeared_idx` (out of range reads as appeared).
    pub fn get(&self, idx: usize) -> bool {
        self.0.get(idx / 8).is_none_or(|b| (b >> (idx & 7)) & 1 == 1)
    }

    /// `mNpc_ResetHaveAppeared`: `grow[i]` = starter/move-in eligible.
    /// Returns true if the table was reset to `residents`.
    pub fn reset_if_exhausted(&mut self, grow_eligible: &[bool], residents: &[usize]) -> bool {
        if grow_eligible.iter().enumerate().any(|(i, &g)| g && !self.get(i)) {
            return false;
        }
        self.0.iter_mut().for_each(|b| *b = 0);
        for &r in residents {
            self.set(r);
        }
        true
    }
}

/// `mNpc_GetPresentClothMemoryIdx` (does not check the memory is in use).
pub fn present_cloth_memory(mems: &[Memory]) -> Option<usize> {
    mems.iter().position(|m| m.wearing_present_cloth)
}

/// `mNpc_GetPresentClothMemoryIdx_rnd`.
pub fn present_cloth_memory_rnd(mems: &[Memory], random: impl FnOnce(i32) -> i32) -> Option<usize> {
    let c: Vec<usize> = mems.iter().enumerate().filter(|(_, m)| m.player.is_some() && m.wearing_present_cloth).map(|(i, _)| i).collect();
    if c.is_empty() {
        return None;
    }
    c.get(random(c.len() as i32) as usize).copied()
}

/// `mNpc_ChangePresentCloth` for one villager: flips the first memory with
/// `has_present_cloth` to wearing; returns whether to put on the cloth.
pub fn change_present_cloth(mems: &mut [Memory], present_is_cloth: bool) -> bool {
    if !present_is_cloth {
        return false;
    }
    if let Some(m) = mems.iter_mut().find(|m| m.player.is_some() && m.has_present_cloth) {
        m.wearing_present_cloth = true;
        m.has_present_cloth = false;
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mem(p: u8, f: i8, letter: bool, t: u64) -> Memory {
        Memory { player: Some(p), friendship: f, letter_exists: letter, last_speak: t, ..Default::default() }
    }

    #[test]
    fn friendship_picks() {
        let ms = [mem(0, 90, false, 5), mem(1, 90, false, 3), mem(2, 50, true, 1)];
        assert_eq!(best_friend(&ms), Some(1));
        let ms2 = [mem(0, 90, false, 5), mem(1, 90, true, 9)];
        assert_eq!(best_friend(&ms2), Some(1));
        assert_eq!(best_friend(&[mem(0, 79, false, 0)]), None);
        assert_eq!(highest_friendship(&[mem(0, 10, false, 0), mem(1, 10, false, 0)]), Some(1));
        assert_eq!(highest_friendship(&[Memory::default()]), None);
    }

    #[test]
    fn move_out_choice() {
        let mut a = [Resident::default(); ANIMAL_NUM_MAX];
        for r in a.iter_mut().take(6) {
            r.exists = true;
        }
        a[2].knows = 0b11;
        a[4].knows = 0b10;
        let mut r0 = |_: i32| 0;
        assert_eq!(choose_move_out(NO_REMOVE, &a, -1, &[0, 1], 6, &mut r0), 2);
        a[2].knows = 0;
        // Player 1 met a[4] but player 0 did not: pass 2 skips it (quirk).
        assert_eq!(choose_move_out(NO_REMOVE, &a, -1, &[0, 1], 6, &mut r0), 0);
        assert_eq!(choose_move_out(NO_REMOVE, &a, -1, &[0, 1], 5, &mut r0), NO_REMOVE);
        assert_eq!(choose_move_out(3, &a, -1, &[0, 1], 6, &mut r0), 3);
        // Count reduced by one: the 6th villager (slot 5) is unreachable.
        assert_eq!(remove_any(&a, -1, 6, &mut |n| n - 1), Some(4));
    }

    #[test]
    fn move_in_and_appeared() {
        assert!(may_move_in(true, false, 0xE010, &[0xE001], 0xE002, None));
        assert!(!may_move_in(true, false, 0xE002, &[0xE001], 0xE002, None));
        assert!(!may_move_in(true, false, 0xE010, &[0xE001], 0, Some(0xE010)));
        assert_eq!(move_in_slot(None, None), MoveInSlot::Replace(0));
        let mut h = HaveAppeared::new(2);
        h.set(3);
        assert!(h.get(3) && !h.get(4) && h.get(99));
        assert!(!h.reset_if_exhausted(&[true, true, false, true], &[0]));
        h.set(0);
        h.set(1);
        assert!(h.reset_if_exhausted(&[true, true, false, true], &[7]));
        assert!(h.get(7) && !h.get(3));
    }

    #[test]
    fn present_cloth() {
        let mut ms = [mem(0, 1, false, 0), Memory { has_present_cloth: true, ..mem(1, 1, false, 0) }];
        assert!(change_present_cloth(&mut ms, true));
        assert!(ms[1].wearing_present_cloth && !ms[1].has_present_cloth);
        assert_eq!(present_cloth_memory(&ms), Some(1));
        assert_eq!(present_cloth_memory_rnd(&ms, |_| 0), Some(1));
        assert!(!change_present_cloth(&mut ms, false));
    }

    #[test]
    fn best_friend_tiebreaks() {
        // Same friendship, no letters: the EARLIER speaker wins.
        let ms = [mem(0, 90, false, 3), mem(1, 90, false, 5)];
        assert_eq!(best_friend(&ms), Some(0));
        // A letter beats no letter even when spoken to later.
        let ms = [mem(0, 90, false, 1), mem(1, 90, true, 99)];
        assert_eq!(best_friend(&ms), Some(1));
        // Below the 80 minimum: no best friend.
        assert_eq!(best_friend(&[mem(0, 79, true, 0)]), None);
        // Free slots are skipped entirely.
        let mut ms = [Memory::default(), mem(0, 95, false, 0)];
        assert_eq!(best_friend(&ms), Some(1));
        // Highest friendship uses <= : the LAST among equals wins.
        let ms = [mem(0, 42, false, 0), mem(1, 42, false, 0), mem(2, 42, false, 0)];
        assert_eq!(highest_friendship(&ms), Some(2));
    }

    #[test]
    fn move_out_gates() {
        // Constants.
        assert_eq!(ANIMAL_NUM_MAX, 15);
        assert_eq!(ANIMAL_NUM_MIN, 5);
        assert_eq!(ANIMAL_MEMORY_NUM, 7);
        assert_eq!(BEST_FRIEND_MIN, 80);
        assert_eq!(NO_REMOVE, 0xFF);
        // No players -> no candidate (avoids div-by-zero in the pick).
        let a = [Resident::default(); ANIMAL_NUM_MAX];
        assert_eq!(choose_move_out(NO_REMOVE, &a, -1, &[], 6, &mut |_| 0), NO_REMOVE);
        // Move-in rejections.
        assert!(!may_move_in(false, false, 0xE010, &[], 0, None)); // uninitialised
        assert!(!may_move_in(true, true, 0xE010, &[], 0, None)); // itself moving
        assert!(!may_move_in(true, false, 0xE010, &[0xE010], 0, None)); // already here
        assert!(may_move_in(true, false, 0xE010, &[0xE001], 0xE002, Some(0xE011)));
        // Move-in slot prefers a free slot over replacing.
        assert_eq!(move_in_slot(Some(7), Some(3)), MoveInSlot::Free(7));
        assert_eq!(move_in_slot(None, Some(3)), MoveInSlot::Replace(3));
    }

}
