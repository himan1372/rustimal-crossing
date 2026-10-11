//! April Fools dialogue controller — Layer A: pure data and helpers.
//!
//! Verified against `src/actor/ac_aprilfool_control.c` and
//! `include/m_name_table.h` (GAFE01_00 Rev 0 USA, via public decomp).
//! Every table below is copied from the C source, not derived.
//!
//! Layering (per the brief, kept): this module owns the NPC→index table,
//! the message-ID table, the per-player bitfield helpers, and the exact
//! talk-check / talk-set / get-message logic. The actor lifecycle
//! (construction, clip registration, destruction), the event-manager
//! activation (`aprilfool_start`), the save-area allocation, and all NPC
//! talk state machines stay in C.

/// Special-NPC actor names (`include/m_name_table.h`, `SP_NPC_START = 0xD000`).
pub mod npc_name {
    pub const POST_GIRL: u16 = 0xD003; // Pelly
    pub const SHOP_MASTER: u16 = 0xD008; // Tom Nook
    pub const POLICE: u16 = 0xD00D; // Copper
    pub const STATION_MASTER: u16 = 0xD00E; // Porter
    pub const POLICE2: u16 = 0xD010; // Booker
    pub const POST_MAN: u16 = 0xD011; // Pete
    pub const POST_GIRL2: u16 = 0xD012; // Phyllis
    pub const CURATOR: u16 = 0xD06D; // Blathers
    pub const NEEDLEWORK0: u16 = 0xD070; // Mable
    pub const NEEDLEWORK1: u16 = 0xD071; // Sable
    pub const SENDO: u16 = 0xD072; // Kapp'n
}

/// NPC table (`ac_aprilfool_control.c:61-71`). Position in this array IS the
/// bit position in each player's save mask. Do NOT reorder — it would
/// reinterpret existing saved bits.
const NPC_TABLE: [u16; 11] = [
    npc_name::STATION_MASTER, // 0: Porter
    npc_name::SHOP_MASTER,    // 1: Tom Nook
    npc_name::CURATOR,        // 2: Blathers
    npc_name::NEEDLEWORK0,    // 3: Mable
    npc_name::NEEDLEWORK1,    // 4: Sable
    npc_name::POLICE,         // 5: Copper
    npc_name::POLICE2,        // 6: Booker
    npc_name::POST_GIRL,      // 7: Pelly
    npc_name::POST_GIRL2,     // 8: Phyllis
    npc_name::POST_MAN,       // 9: Pete
    npc_name::SENDO,          // 10: Kapp'n
];

/// Message-ID table (`ac_aprilfool_control.c:122-123`). Explicit table, NOT
/// derived arithmetically — the order is not numeric.
const MSG_NUM_TABLE: [i32; 11] = [
    0x3BB5, 0x3BAC, 0x3BB0, 0x3BB1, 0x3BB2, 0x3BB3, //
    0x3BB4, 0x3BAE, 0x3BAF, 0x3BAD, 0x3BB6,
];

/// `aAPC_get_data_idx`: linear search, returns table index or -1.
pub fn npc_index(actor_name: u16) -> Option<usize> {
    NPC_TABLE.iter().position(|&n| n == actor_name)
}

/// `aAPC_GET_NPC_BIT(bitfield, data_idx)`: `(1 << data_idx) & bitfield`.
/// Returns true if the bit is set. Caller must ensure a valid index.
pub fn get_npc_bit(bitfield: u16, data_idx: usize) -> bool {
    debug_assert!(data_idx < 11);
    (bitfield & (1u16 << data_idx)) != 0
}

/// `aAPC_SET_NPC_BIT(bitfield, data_idx)`: `bitfield | (1 << data_idx)`.
/// Sets one bit, preserving all others. Caller must ensure a valid index.
pub fn set_npc_bit(bitfield: u16, data_idx: usize) -> u16 {
    debug_assert!(data_idx < 11);
    bitfield | (1u16 << data_idx)
}

/// `aAPC_talk_chk_proc` (pure core).
///
/// C semantics, preserved exactly: returns `false` ONLY when the player is
/// local, the NPC is tracked, and its bit is clear — i.e. `false` means
/// "April Fools dialogue NOT yet consumed, use the special branch".
/// Callers test `!talk_chk(...)` to enter the April Fools path.
/// Returns `true` for foreigners, untracked NPCs, or already-consumed bits.
pub fn talk_chk(bitfield: u16, actor_name: u16, is_foreigner: bool) -> bool {
    if is_foreigner {
        return true;
    }
    match npc_index(actor_name) {
        None => true,
        Some(idx) => get_npc_bit(bitfield, idx),
    }
}

/// `aAPC_talk_set_proc` (pure core): ORs the NPC's bit into the mask.
/// No-op for untracked NPCs. Never clears other bits.
pub fn talk_set(bitfield: u16, actor_name: u16) -> u16 {
    match npc_index(actor_name) {
        None => bitfield,
        Some(idx) => set_npc_bit(bitfield, idx),
    }
}

/// `aAPC_get_msg_num_proc` (pure core).
///
/// Returns `(message_id, updated_mask)`. If `update_talked`, the NPC's bit
/// is set in the returned mask before lookup. Returns `(-1, bitfield)`
/// unchanged for untracked NPCs.
pub fn get_msg_num(bitfield: u16, actor_name: u16, update_talked: bool) -> (i32, u16) {
    match npc_index(actor_name) {
        None => (-1, bitfield),
        Some(idx) => {
            let mask = if update_talked {
                set_npc_bit(bitfield, idx)
            } else {
                bitfield
            };
            (MSG_NUM_TABLE[idx], mask)
        }
    }
}

// ---- C ABI ----
// All pc_-prefixed: the C originals stay compiled; these are staged kernels.

/// NPC actor name → table index, or -1 if untracked.
#[no_mangle]
pub extern "C" fn pc_aprilfool_npc_index(actor_name: u16) -> i32 {
    npc_index(actor_name).map(|i| i as i32).unwrap_or(-1)
}

/// Exact bit-test: 1 if the bit is set, 0 if clear, -1 if index invalid.
#[no_mangle]
pub extern "C" fn pc_aprilfool_talk_seen(bitfield: u16, data_idx: i32) -> i32 {
    if data_idx < 0 || data_idx >= 11 {
        return -1;
    }
    if get_npc_bit(bitfield, data_idx as usize) {
        1
    } else {
        0
    }
}

/// Set one bit, preserving others. Returns the mask unchanged if index invalid.
#[no_mangle]
pub extern "C" fn pc_aprilfool_mark_seen(bitfield: u16, data_idx: i32) -> u16 {
    if data_idx < 0 || data_idx >= 11 {
        return bitfield;
    }
    set_npc_bit(bitfield, data_idx as usize)
}

/// Message ID for a table index, or -1 if invalid.
#[no_mangle]
pub extern "C" fn pc_aprilfool_message_id(data_idx: i32) -> i32 {
    if data_idx < 0 || data_idx >= 11 {
        return -1;
    }
    MSG_NUM_TABLE[data_idx as usize]
}

/// Combined lookup + optional mark. Returns the message ID (-1 if untracked);
/// writes the updated mask to `out_mask` (may be null to skip).
#[no_mangle]
pub extern "C" fn pc_aprilfool_update_and_message(
    actor_name: u16,
    bitfield: u16,
    update_talked: i32,
    out_mask: *mut u16,
) -> i32 {
    let (msg_id, new_mask) = get_msg_num(bitfield, actor_name, update_talked != 0);
    if !out_mask.is_null() {
        // SAFETY: null-checked above; caller provides a valid u16 slot.
        unsafe {
            *out_mask = new_mask;
        }
    }
    msg_id
}

/// Full `aAPC_talk_chk_proc` logic: 1 = TRUE (normal path), 0 = FALSE (use
/// April Fools dialogue). `is_foreigner`: 1 if foreign player, else 0.
#[no_mangle]
pub extern "C" fn pc_aprilfool_talk_chk(bitfield: u16, actor_name: u16, is_foreigner: i32) -> i32 {
    if talk_chk(bitfield, actor_name, is_foreigner != 0) {
        1
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_11_npc_mappings() {
        let cases: [(u16, usize); 11] = [
            (npc_name::STATION_MASTER, 0),
            (npc_name::SHOP_MASTER, 1),
            (npc_name::CURATOR, 2),
            (npc_name::NEEDLEWORK0, 3),
            (npc_name::NEEDLEWORK1, 4),
            (npc_name::POLICE, 5),
            (npc_name::POLICE2, 6),
            (npc_name::POST_GIRL, 7),
            (npc_name::POST_GIRL2, 8),
            (npc_name::POST_MAN, 9),
            (npc_name::SENDO, 10),
        ];
        for (name, idx) in cases {
            assert_eq!(npc_index(name), Some(idx), "name {:#06X}", name);
        }
        assert_eq!(npc_index(0xD000), None);
        assert_eq!(npc_index(0xFFFF), None);
    }

    #[test]
    fn message_ids_match_table() {
        let expected: [i32; 11] = [
            0x3BB5, 0x3BAC, 0x3BB0, 0x3BB1, 0x3BB2, 0x3BB3, //
            0x3BB4, 0x3BAE, 0x3BAF, 0x3BAD, 0x3BB6,
        ];
        for (i, &msg) in expected.iter().enumerate() {
            assert_eq!(MSG_NUM_TABLE[i], msg, "index {}", i);
        }
    }

    #[test]
    fn bit_ops_independent() {
        // Highest tracked bit (10) alone.
        let m = set_npc_bit(0, 10);
        assert_eq!(m, 1u16 << 10);
        assert!(get_npc_bit(m, 10));
        assert!(!get_npc_bit(m, 9));
        // Setting one bit preserves others.
        let m2 = set_npc_bit(0b101, 1);
        assert_eq!(m2, 0b111);
        // Setting an already-set bit is idempotent.
        assert_eq!(set_npc_bit(m2, 1), m2);
    }

    #[test]
    fn talk_chk_semantics() {
        // Tracked, bit clear, local → FALSE (0): use April Fools dialogue.
        assert!(!talk_chk(0, npc_name::SHOP_MASTER, false));
        // Tracked, bit set, local → TRUE: normal path.
        assert!(talk_chk(1u16 << 1, npc_name::SHOP_MASTER, false));
        // Untracked NPC → TRUE regardless of mask.
        assert!(talk_chk(0, 0xD000, false));
        // Foreign player → TRUE even with bit clear.
        assert!(talk_chk(0, npc_name::SHOP_MASTER, true));
    }

    #[test]
    fn get_msg_num_update_flag() {
        // update_talked = true: returns msg AND sets the bit.
        let (msg, mask) = get_msg_num(0, npc_name::CURATOR, true);
        assert_eq!(msg, 0x3BB0);
        assert_eq!(mask, 1u16 << 2);
        // update_talked = false: returns msg, mask untouched.
        let (msg2, mask2) = get_msg_num(0, npc_name::CURATOR, false);
        assert_eq!(msg2, 0x3BB0);
        assert_eq!(mask2, 0);
        // Untracked: -1, mask unchanged.
        let (msg3, mask3) = get_msg_num(0xFF, 0xD000, true);
        assert_eq!(msg3, -1);
        assert_eq!(mask3, 0xFF);
    }

    #[test]
    fn c_abi_edge_cases() {
        assert_eq!(pc_aprilfool_npc_index(npc_name::SENDO), 10);
        assert_eq!(pc_aprilfool_npc_index(0x1234), -1);
        assert_eq!(pc_aprilfool_talk_seen(0b1000, 3), 1);
        assert_eq!(pc_aprilfool_talk_seen(0b1000, 2), 0);
        assert_eq!(pc_aprilfool_talk_seen(0, 11), -1);
        assert_eq!(pc_aprilfool_talk_seen(0, -1), -1);
        assert_eq!(pc_aprilfool_mark_seen(0b101, 1), 0b111);
        assert_eq!(pc_aprilfool_mark_seen(0b101, 99), 0b101);
        assert_eq!(pc_aprilfool_message_id(10), 0x3BB6);
        assert_eq!(pc_aprilfool_message_id(11), -1);
        assert_eq!(pc_aprilfool_talk_chk(0, npc_name::POLICE, 0), 0);
        assert_eq!(pc_aprilfool_talk_chk(1u16 << 5, npc_name::POLICE, 0), 1);
    }
}
