//! Event visitors: Katrina, Saharah, Wendell, Gracie, Crazy Redd —
//! pure data and kernels.
//!
//! Verified against `src/actor/npc/event/ac_ev_gypsy_move.c_inc`,
//! `src/actor/npc/event/ac_ev_carpetPeddler.c_inc`,
//! `src/actor/npc/event/ac_ev_artist_move.c_inc`,
//! `src/actor/npc/event/ac_ev_designer_talk.c_inc`,
//! `src/actor/npc/event/ac_ev_broker_move.c_inc`,
//! `src/actor/npc/event/ac_ev_miko_move.c_inc`,
//! `src/actor/ac_event_manager.c`, `src/bg_item/bg_item_common.c_inc`,
//! `src/game/m_mail_password_check.c`, `src/game/m_npc.c`,
//! `include/m_name_table.h`, `include/m_private.h`, `include/m_event.h`,
//! `include/ac_ev_gypsy.h`, `include/ac_ev_carpetPeddler.h`,
//! `include/ac_ev_designer.h`, `include/ac_ev_broker.h`
//! (GAFE01_00 Rev 0 USA decomp).
//! 10 of 12 brief claims checked verbatim; 2 HEADLINE CORRECTIONS.
//!
//! CORRECTION 1 — Katrina's fortune distribution: the brief claimed
//! Normal 10% / Money luck 50%. Tracing the transform
//! (`RANDOM(10)` values 0–3 forced to 4, then `−= 4`) gives index 0
//! (five roll values) = mPr_DESTINY_NORMAL → **Normal 50%**, every
//! other destiny 10%. The decomp's own comment confirms: "50% chance
//! of 'normal' luck, 10% chance for other types".
//!
//! CORRECTION 2 — Wendell's fruit path: the brief claimed fish AND
//! fruit go to the gift demo. The `next_act_idx` table shows only FISH
//! goes to DEMO_START_WAIT; FOOD goes to TALK_END_WAIT (the ordinary
//! talk-end path, same as OTHER).
//!
//! Architecture: the event scheduling, the event manager, all five
//! actors' state machines, the wallet/inventory/save mutations, the
//! item-selection tables, the tent actor, the handbill generation, and
//! the RNG all stay in C. This module holds only the source-proven
//! pure data and calculations with primitive arguments.

/// Visitor identities (`include/m_name_table.h`).
pub mod id {
    /// Wendell, the hungry artist.
    pub const ARTIST: u16 = 0xD000;
    /// Crazy Redd, the traveling tent merchant (primary identity).
    pub const BROKER: u16 = 0xD001;
    /// Gracie, the fashionista.
    pub const DESIGNER: u16 = 0xD002;
    /// Saharah, the carpet/wallpaper seller.
    pub const CARPETPEDDLER: u16 = 0xD006;
    /// Katrina, the fortune teller (primary identity).
    pub const GYPSY: u16 = 0xD00C;
    /// Redd alternate identities (same name entry 0x206).
    pub const EV_BROKER2: u16 = 0xD013;
    pub const EV_YOMISE: u16 = 0xD02C;
    pub const EV_YOMISE2: u16 = 0xD039;
    /// Katrina's New Year omikuji actor (same name entry 0x207).
    pub const EV_MIKO: u16 = 0xD03D;
}

/// NPC name indices (`m_npc.c`); Katrina is female, the rest male.
pub mod name_idx {
    pub const WENDELL: u16 = 0x209;
    pub const REDD: u16 = 0x206;
    pub const GRACIE: u16 = 0x20B;
    pub const SAHARAH: u16 = 0x208;
    pub const KATRINA: u16 = 0x207;
}

/// Special-event absolute values (`include/m_event.h`).
pub mod event {
    pub const ARTIST: u8 = 74;
    pub const BROKER_SALE: u8 = 75;
    pub const DESIGNER: u8 = 76;
    pub const GYPSY: u8 = 77;
    pub const SHOP_SALE: u8 = 78;
    pub const CARPET_PEDDLER: u8 = 79;
}

/// Destiny values (`include/m_private.h`).
pub mod destiny {
    pub const NORMAL: u8 = 0;
    pub const POPULAR: u8 = 1;
    pub const UNPOPULAR: u8 = 2;
    pub const BAD_LUCK: u8 = 3;
    pub const MONEY_LUCK: u8 = 4;
    pub const GOODS_LUCK: u8 = 5;
}

/// April Fools: none of the five appear in the 11-entry
/// `npc_data_table` (`ac_aprilfool_control.c`) — confirmed negative.
pub const APRILFOOL_NONE: bool = true;

/// Katrina's destiny table, in roll-index order.
pub const FORTUNE_DESTINY: [u8; 6] = [
    destiny::NORMAL,
    destiny::POPULAR,
    destiny::UNPOPULAR,
    destiny::BAD_LUCK,
    destiny::MONEY_LUCK,
    destiny::GOODS_LUCK,
];

/// Katrina's fortune price.
pub const FORTUNE_PRICE: u32 = 50;

/// Katrina's fortune-index transform (`aEGPS_decide_result`), verbatim:
/// values 0–3 are forced to 4, then 4 is subtracted. Given a uniform
/// RANDOM(10) roll this yields index 0 (Normal) at 50% and every other
/// index at 10%. The RANDOM(10) call stays in C.
pub fn fortune_index(roll: u8) -> u8 {
    let mut selected = roll;
    if selected < 4 {
        selected = 4;
    }
    selected - 4
}

/// Destiny for a fortune roll.
pub fn fortune_destiny(roll: u8) -> u8 {
    FORTUNE_DESTINY[fortune_index(roll) as usize]
}

/// Result message: `0x974 + index`.
pub fn fortune_result_msg(roll: u8) -> u16 {
    0x0974 + fortune_index(roll) as u16
}

/// Saved-destiny message (`msg_no2`, 0x97C–0x980), indexed by
/// `destiny.type − 1`. Only reachable for non-normal destiny.
pub fn fortune_saved_msg(saved_destiny: u8) -> u16 {
    0x097C + (saved_destiny - 1) as u16
}

/// Katrina's talk opening (`aEGPS_set_talk_info`): fortune already
/// given → 0x982; saved destiny normal → 0x970; otherwise → 0x97B.
pub fn fortune_talk_msg(fortune_given: bool, saved_destiny: u8) -> u16 {
    if fortune_given {
        0x0982
    } else if saved_destiny == destiny::NORMAL {
        0x0970
    } else {
        0x097B
    }
}

/// Whether talk routes to the fresh-fortune path (CALL_IN) rather than
/// the saved-destiny path (DECIDE_RESULT2): true iff the saved
/// destiny is normal.
pub fn fortune_needs_new(saved_destiny: u8) -> bool {
    saved_destiny == destiny::NORMAL
}

/// Call-in menu messages: decline → 0x971, accept with enough Bells →
/// 0x973, accept without enough → 0x972. Acceptance is
/// `wallet >= FORTUNE_PRICE`.
pub mod fortune_callin {
    pub const DECLINE: u16 = 0x0971;
    pub const ACCEPT: u16 = 0x0973;
    pub const ACCEPT_POOR: u16 = 0x0972;
}

pub fn fortune_callin_msg(accept: bool, can_afford: bool) -> u16 {
    if !accept {
        fortune_callin::DECLINE
    } else if can_afford {
        fortune_callin::ACCEPT
    } else {
        fortune_callin::ACCEPT_POOR
    }
}

/// Fortune-string bases (`aEGPS_set_string`): adjective, noun, verb,
/// place. `str_no = base + RANDOM(32)`; the four rolls stay in C and
/// run in this order after the wallet is decremented.
pub mod fortune_str {
    pub const ADJECTIVE: u8 = 0;
    pub const NOUN: u8 = 1;
    pub const VERB: u8 = 2;
    pub const PLACE: u8 = 3;
}

pub const FORTUNE_STR_BASE: [u16; 4] = [0x1A4, 0x1C4, 0x184, 0x164];

pub fn fortune_string_no(str_type: u8, rand32: u8) -> u16 {
    FORTUNE_STR_BASE[(str_type % 4) as usize] + (rand32 % 32) as u16
}

/// EV_MIKO (Katrina's New Year omikuji actor): its own 4-entry destiny
/// table, `RANDOM(4)`, message `0x1911 + omikuji`. Grants
/// ITM_FORTUNE_SLIP and a mail letter (in C).
pub const MIKO_DESTINY: [u8; 4] = [
    destiny::NORMAL,
    destiny::BAD_LUCK,
    destiny::MONEY_LUCK,
    destiny::GOODS_LUCK,
];

pub fn miko_destiny(omikuji: u8) -> u8 {
    MIKO_DESTINY[(omikuji % 4) as usize]
}

pub fn miko_msg(omikuji: u8) -> u16 {
    0x1911 + (omikuji % 4) as u16
}

/// Saharah's price constants (`ac_ev_carpetPeddler.h`).
pub mod saharah {
    pub const PRICE_START: u32 = 3000;
    pub const PRICE_MULT_CAP: u8 = 4;
    /// Message base (`m_msg_data.h`).
    pub const MSG_START: u16 = 0x048A;
}

/// Saharah's price (`aECPD_1st_check_init`), RESOLVED from the decomp:
/// `price = 3000 × 2^used`, where `used` is the arabian event-save
/// count, capped at 4 → max 48,000. Each completed trade-in doubles
/// the next price.
pub fn saharah_price(used: u8) -> u32 {
    let used = used.min(saharah::PRICE_MULT_CAP);
    saharah::PRICE_START << used
}

/// The used-count increment after a completed trade-in, capped at 4.
pub fn saharah_next_used(used: u8) -> u8 {
    (used + 1).min(saharah::PRICE_MULT_CAP)
}

/// Saharah's second-menu messages (`aECPD_2nd_check`): the possession
/// check precedes the money check, and the polarity is trade-in — the
/// player must POSSESS a carpet to proceed. No carpet → START+4;
/// carpet but no money → START+3; both → START+5 (GRAD_MESSAGE).
pub fn saharah_check_msg(has_carpet: bool, has_money: bool) -> u16 {
    let base = saharah::MSG_START;
    if !has_carpet {
        base + 4
    } else if !has_money {
        base + 3
    } else {
        base + 5
    }
}

/// Item-name encoding constants (`m_name_table.h`, `m_room_type.h`).
pub mod item_enc {
    pub const NAME_TYPE_ITEM1: u8 = 2;
    pub const ITEM1_NO_START: u16 = 0x2000;
    pub const ITEM1_CAT_TOOL: u8 = 2;
    pub const ITEM1_CAT_FISH: u8 = 3;
    pub const ITEM1_CAT_FRUIT: u8 = 8;
}

/// Wendell's item classes (`ac_ev_artist.c`).
pub mod wendell_class {
    pub const FISH: u8 = 0;
    pub const FOOD: u8 = 1;
    pub const TOOL: u8 = 2;
    pub const OTHER: u8 = 3;
}

/// Wendell's classification (`aEART_item_check`), verbatim: the switch
/// only runs when `ITEM_NAME_GET_TYPE(item) == NAME_TYPE_ITEM1`;
/// non-ITEM1 items stay OTHER. The bit-masking of type/category stays
/// in C — pass the decoded integers.
pub fn wendell_classify(name_type: u8, category: u8) -> u8 {
    if name_type != item_enc::NAME_TYPE_ITEM1 {
        return wendell_class::OTHER;
    }
    match category {
        item_enc::ITEM1_CAT_FISH => wendell_class::FISH,
        item_enc::ITEM1_CAT_FRUIT => wendell_class::FOOD,
        item_enc::ITEM1_CAT_TOOL => wendell_class::TOOL,
        _ => wendell_class::OTHER,
    }
}

/// Wendell's message table, indexed: default/empty → 0, FISH → 1,
/// FOOD → 2, TOOL → 3, OTHER → 4.
pub const WENDELL_MSG: [u16; 5] = [0x02F6, 0x02F9, 0x02F8, 0x02FC, 0x02F7];

pub fn wendell_msg(class: u8) -> u16 {
    WENDELL_MSG[(class.min(4)) as usize]
}

/// Wendell's next-action markers (Rust-side; the C action IDs stay in
/// the actor code). CORRECTED: only FISH goes to the gift demo;
/// FOOD takes the ordinary talk-end path.
pub mod wendell_next {
    pub const TALK_END_WAIT: u8 = 0;
    pub const DEMO_START_WAIT: u8 = 1;
    pub const REFUSE_DEMO_START_WAIT: u8 = 2;
}

pub fn wendell_next_action(class: u8) -> u8 {
    match class {
        wendell_class::FISH => wendell_next::DEMO_START_WAIT,
        wendell_class::TOOL => wendell_next::REFUSE_DEMO_START_WAIT,
        _ => wendell_next::TALK_END_WAIT,
    }
}

/// Gracie's dialogue tables (`aEDSN_get_msg_no`), verbatim — the
/// duplicates are retail behavior, preserved. Selected by
/// `wear_flag` (0/1) and the caller-supplied index (0–26).
pub const GRACIE_MSG_0: [u16; 27] = [
    0x0721, 0x0722, 0x0723, 0x0724, 0x0725, 0x0726, 0x0727, 0x0728,
    0x0729, 0x072A, 0x072B, 0x072C, 0x072D, 0x072E, 0x072F, 0x0730,
    0x0731, 0x0732, 0x0733, 0x0747, 0x0748, 0x0749, 0x074A, 0x074B,
    0x072B, 0x072C, 0x074C,
];

pub const GRACIE_MSG_1: [u16; 27] = [
    0x0734, 0x0735, 0x0736, 0x0745, 0x0737, 0x0738, 0x0739, 0x0746,
    0x073A, 0x073B, 0x073C, 0x073D, 0x073E, 0x073F, 0x0740, 0x0741,
    0x0742, 0x0743, 0x0744, 0x0747, 0x0748, 0x0749, 0x074A, 0x074B,
    0x073C, 0x073D, 0x074C,
];

pub fn gracie_msg(wear_flag: u8, idx: u8) -> u16 {
    let i = (idx % 27) as usize;
    if wear_flag == 0 {
        GRACIE_MSG_0[i]
    } else {
        GRACIE_MSG_1[i]
    }
}

/// Gracie's reward-result dialogue indices: results {0, 1, 2} →
/// message-table indices {12, 14, 16}.
pub const GRACIE_RESULT_MSG_IDX: [u8; 3] = [12, 14, 16];

pub fn gracie_result_msg_idx(result: u8) -> u8 {
    GRACIE_RESULT_MSG_IDX[(result % 3) as usize]
}

/// Gracie's reward list: result 0 selects from the EVENT clothing
/// list, result 1 from the COMMON clothing list, result 2 changes the
/// talk status instead. The item-list selection itself stays in C.
pub fn gracie_reward_is_event_list(result: u8) -> bool {
    result == 0
}

/// Redd's handbill templates (`aEvMgr_actor_regist_broker_handbill`).
pub const REDD_HANDBILL: [u16; 3] = [0x031, 0x032, 0x033];

pub fn redd_handbill(rand3: u8) -> u16 {
    REDD_HANDBILL[(rand3 % 3) as usize]
}

/// Redd's tent-liveness rule (`aEBRK_check_tent`), verbatim: if no tent
/// is cached, search for one; otherwise Redd deletes himself when the
/// cached actor is not the tent profile OR both its movement and draw
/// callbacks are NULL.
pub fn redd_tent_dead(tent_found: bool, is_brshop: bool, mv_null: bool, dw_null: bool) -> bool {
    tent_found && (!is_brshop || (mv_null && dw_null))
}

// ---- C ABI ----
// All pc_-prefixed: the C originals stay compiled; these are staged kernels.

#[no_mangle]
pub extern "C" fn pc_ev_fortune_index(roll: u8) -> u8 {
    fortune_index(roll)
}

#[no_mangle]
pub extern "C" fn pc_ev_fortune_result_msg(roll: u8) -> u16 {
    fortune_result_msg(roll)
}

#[no_mangle]
pub extern "C" fn pc_ev_fortune_saved_msg(saved_destiny: u8) -> u16 {
    fortune_saved_msg(saved_destiny)
}

#[no_mangle]
pub extern "C" fn pc_ev_fortune_talk_msg(fortune_given: u8, saved_destiny: u8) -> u16 {
    fortune_talk_msg(fortune_given != 0, saved_destiny)
}

#[no_mangle]
pub extern "C" fn pc_ev_fortune_string_no(str_type: u8, rand32: u8) -> u16 {
    fortune_string_no(str_type, rand32)
}

#[no_mangle]
pub extern "C" fn pc_ev_miko_destiny(omikuji: u8) -> u8 {
    miko_destiny(omikuji)
}

#[no_mangle]
pub extern "C" fn pc_ev_miko_msg(omikuji: u8) -> u16 {
    miko_msg(omikuji)
}

#[no_mangle]
pub extern "C" fn pc_ev_saharah_price(used: u8) -> u32 {
    saharah_price(used)
}

#[no_mangle]
pub extern "C" fn pc_ev_saharah_check_msg(has_carpet: u8, has_money: u8) -> u16 {
    saharah_check_msg(has_carpet != 0, has_money != 0)
}

#[no_mangle]
pub extern "C" fn pc_ev_wendell_classify(name_type: u8, category: u8) -> u8 {
    wendell_classify(name_type, category)
}

#[no_mangle]
pub extern "C" fn pc_ev_wendell_msg(class: u8) -> u16 {
    wendell_msg(class)
}

#[no_mangle]
pub extern "C" fn pc_ev_wendell_next_action(class: u8) -> u8 {
    wendell_next_action(class)
}

#[no_mangle]
pub extern "C" fn pc_ev_gracie_msg(wear_flag: u8, idx: u8) -> u16 {
    gracie_msg(wear_flag, idx)
}

#[no_mangle]
pub extern "C" fn pc_ev_gracie_result_msg_idx(result: u8) -> u8 {
    gracie_result_msg_idx(result)
}

#[no_mangle]
pub extern "C" fn pc_ev_redd_handbill(rand3: u8) -> u16 {
    redd_handbill(rand3)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identities() {
        assert_eq!(id::ARTIST, 0xD000);
        assert_eq!(id::BROKER, 0xD001);
        assert_eq!(id::DESIGNER, 0xD002);
        assert_eq!(id::CARPETPEDDLER, 0xD006);
        assert_eq!(id::GYPSY, 0xD00C);
        assert_eq!(id::EV_BROKER2, 0xD013);
        assert_eq!(id::EV_YOMISE, 0xD02C);
        assert_eq!(id::EV_YOMISE2, 0xD039);
        assert_eq!(id::EV_MIKO, 0xD03D);
        assert_eq!(name_idx::WENDELL, 0x209);
        assert_eq!(name_idx::REDD, 0x206);
        assert_eq!(name_idx::GRACIE, 0x20B);
        assert_eq!(name_idx::SAHARAH, 0x208);
        assert_eq!(name_idx::KATRINA, 0x207);
        assert_eq!(event::ARTIST, 74);
        assert_eq!(event::CARPET_PEDDLER, 79);
    }

    #[test]
    fn fortune() {
        // rolls 0-3 -> index 0 (Normal); 5->1, 6->2, 7->3, 8->4, 9->5
        for r in 0..4u8 {
            assert_eq!(fortune_index(r), 0);
            assert_eq!(fortune_destiny(r), destiny::NORMAL);
        }
        assert_eq!(fortune_index(4), 0);
        assert_eq!(fortune_index(5), 1);
        assert_eq!(fortune_index(9), 5);
        assert_eq!(fortune_destiny(9), destiny::GOODS_LUCK);
        assert_eq!(fortune_result_msg(0), 0x0974);
        assert_eq!(fortune_result_msg(9), 0x0979);
        assert_eq!(fortune_saved_msg(1), 0x097C);
        assert_eq!(fortune_saved_msg(5), 0x0980);
        assert_eq!(fortune_talk_msg(true, 0), 0x0982);
        assert_eq!(fortune_talk_msg(false, 0), 0x0970);
        assert_eq!(fortune_talk_msg(false, 3), 0x097B);
        assert!(fortune_needs_new(destiny::NORMAL));
        assert!(!fortune_needs_new(destiny::MONEY_LUCK));
        assert_eq!(fortune_callin_msg(false, true), 0x0971);
        assert_eq!(fortune_callin_msg(true, true), 0x0973);
        assert_eq!(fortune_callin_msg(true, false), 0x0972);
        assert_eq!(FORTUNE_PRICE, 50);
        assert_eq!(fortune_string_no(0, 0), 0x1A4);
        assert_eq!(fortune_string_no(3, 31), 0x164 + 31);
        assert_eq!(miko_destiny(0), destiny::NORMAL);
        assert_eq!(miko_destiny(3), destiny::GOODS_LUCK);
        assert_eq!(miko_msg(2), 0x1913);
    }

    #[test]
    fn saharah() {
        assert_eq!(saharah_price(0), 3000);
        assert_eq!(saharah_price(1), 6000);
        assert_eq!(saharah_price(2), 12000);
        assert_eq!(saharah_price(3), 24000);
        assert_eq!(saharah_price(4), 48000);
        assert_eq!(saharah_price(9), 48000); // capped
        assert_eq!(saharah_next_used(3), 4);
        assert_eq!(saharah_next_used(4), 4);
        assert_eq!(saharah_check_msg(false, false), 0x048A + 4);
        assert_eq!(saharah_check_msg(true, false), 0x048A + 3);
        assert_eq!(saharah_check_msg(true, true), 0x048A + 5);
    }

    #[test]
    fn wendell() {
        assert_eq!(wendell_classify(2, 3), wendell_class::FISH);
        assert_eq!(wendell_classify(2, 8), wendell_class::FOOD);
        assert_eq!(wendell_classify(2, 2), wendell_class::TOOL);
        assert_eq!(wendell_classify(2, 9), wendell_class::OTHER);
        assert_eq!(wendell_classify(1, 3), wendell_class::OTHER); // not ITEM1
        assert_eq!(wendell_msg(0), 0x02F6);
        assert_eq!(wendell_msg(1), 0x02F9);
        assert_eq!(wendell_msg(2), 0x02F8);
        assert_eq!(wendell_msg(3), 0x02FC);
        assert_eq!(wendell_msg(4), 0x02F7);
        // corrected: only FISH -> gift demo; FOOD -> talk end
        assert_eq!(wendell_next_action(wendell_class::FISH), wendell_next::DEMO_START_WAIT);
        assert_eq!(wendell_next_action(wendell_class::FOOD), wendell_next::TALK_END_WAIT);
        assert_eq!(wendell_next_action(wendell_class::TOOL), wendell_next::REFUSE_DEMO_START_WAIT);
        assert_eq!(wendell_next_action(wendell_class::OTHER), wendell_next::TALK_END_WAIT);
    }

    #[test]
    fn gracie() {
        assert_eq!(GRACIE_MSG_0.len(), 27);
        assert_eq!(GRACIE_MSG_1.len(), 27);
        assert_eq!(gracie_msg(0, 0), 0x0721);
        assert_eq!(gracie_msg(1, 0), 0x0734);
        // duplicates preserved
        assert_eq!(gracie_msg(0, 10), 0x072B);
        assert_eq!(gracie_msg(0, 24), 0x072B);
        assert_eq!(gracie_msg(1, 11), 0x073C);
        assert_eq!(gracie_msg(1, 24), 0x073C);
        assert_eq!(gracie_msg(0, 26), 0x074C);
        assert_eq!(gracie_msg(1, 26), 0x074C);
        assert_eq!(gracie_result_msg_idx(0), 12);
        assert_eq!(gracie_result_msg_idx(1), 14);
        assert_eq!(gracie_result_msg_idx(2), 16);
        assert!(gracie_reward_is_event_list(0));
        assert!(!gracie_reward_is_event_list(1));
    }

    #[test]
    fn redd() {
        assert_eq!(redd_handbill(0), 0x031);
        assert_eq!(redd_handbill(2), 0x033);
        assert!(!redd_tent_dead(false, true, false, false)); // not found -> search
        assert!(redd_tent_dead(true, false, false, false)); // wrong profile
        assert!(redd_tent_dead(true, true, true, true)); // both procs null
        assert!(!redd_tent_dead(true, true, false, true)); // has a proc
        assert!(APRILFOOL_NONE);
    }
}
