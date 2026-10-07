//! Player tool families: axe, net, fishing rod.
//!
//! Verified against `m_player_main_swing_axe.c_inc`,
//! `m_player_main_reflect_axe.c_inc`, `m_player_main_air_axe.c_inc`,
//! `m_player_main_broken_axe.c_inc`, `m_player_main_ready_net.c_inc`,
//! `m_player_main_swing_net.c_inc`, `m_player_main_pull_net.c_inc`,
//! `m_player_main_cast_rod.c_inc`, `m_player_main_relax_rod.c_inc`,
//! `m_player_main_vib_rod.c_inc`, `m_player_main_fly_rod.c_inc`
//! (USA Rev. 0 decomp).
//!
//! Each family follows the same architecture as the scoop states:
//! request (with priority) → setup → animation-frame events → world
//! side effects → transition. The animation is the gameplay clock.

/// Axe frame timeline for SWING_AXE, verbatim.
/// Frame 10 → whoosh sound; frame 15 → the hit (effect, tree
/// resolution, sound, axe-damage bookkeeping); frame 16.5 → bee
/// attack status; frame >= 17 → shock if a bee was disturbed, else
/// WALK (priority 1) when the stick moves.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AxeSwingEvent {
    Whoosh,
    Hit,
    BeeAttackStatus,
    SettleOrWalk,
}

pub fn swing_axe_frame_event(frame: f32, bee_flag: bool, moving: bool) -> Option<AxeSwingEvent> {
    if frame == 10.0 {
        Some(AxeSwingEvent::Whoosh)
    } else if frame == 15.0 {
        Some(AxeSwingEvent::Hit)
    } else if frame == 16.5 {
        Some(AxeSwingEvent::BeeAttackStatus)
    } else if frame >= 17.0 {
        // The source branches on bee_flag vs movement here, but the
        // frame gate is the same; the caller decides shock vs walk.
        let _ = (bee_flag, moving);
        Some(AxeSwingEvent::SettleOrWalk)
    } else {
        None
    }
}

/// Axe hit-effect origin offset, verbatim: (-7, 20, 24) rotated by the
/// player's Y angle.
pub const AXE_HIT_OFFSET: (f32, f32, f32) = (-7.0, 20.0, 24.0);

/// Tree resolution at the frame-15 hit, verbatim:
/// `tree_cutcount_check_proc` counts down; when it reaches <= 0 the
/// tree becomes a stump (`bg_item_fg_sub(item, 0)`); otherwise the
/// tree shakes. Fruit drops unless it's a bee tree; a bee tree sets
/// `bee_counter = 5.0` instead.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TreeHitOutcome {
    /// (became_stump, dropped_fruit, bee_counter_set)
    Resolved(bool, bool, bool),
}

pub fn tree_hit_outcome(cutcount_remaining: i32, is_bee_tree: bool) -> (bool, bool, bool) {
    let became_stump = cutcount_remaining <= 0;
    let dropped_fruit = !is_bee_tree;
    let bee_counter_set = is_bee_tree;
    (became_stump, dropped_fruit, bee_counter_set)
}

/// Bee-tree disturbance counter set on the frame-15 hit.
pub const AXE_BEE_COUNTER: f32 = 5.0;

/// REFLECT_AXE settles at frame 30.5 (bee status) / 31 (walk);
/// AIR_AXE at 35.5 / 36. Same shape as SWING_AXE's tail.
pub fn reflect_axe_settle_frame(frame: f32) -> bool {
    frame == 30.5
}

pub fn air_axe_settle_frame(frame: f32) -> bool {
    frame == 35.5
}

/// Semantic player actions reported for tree chops.
pub mod semantic {
    pub const CHOP_TREE: u32 = 0;
    pub const CHOP_PALM_TREE: u32 = 1;
}

/// Net catch capsule, verbatim: from `net_top_col_pos` toward
/// `net_bot_col_pos`, length 50 (normal net) or 60 (gold net).
/// Insects register themselves into the catch request tables; the
/// swing then tests each candidate against the capsule.
pub fn net_catch_length(is_gold_net: bool) -> f32 {
    if is_gold_net {
        60.0
    } else {
        50.0
    }
}

/// SWING_NET outcome routing, verbatim:
/// check_type 2 → PULL_NET (priority 26) with hit sound + vibration;
/// check_type 0 → STOP_NET (priority 26) with NPC UZAI marking.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NetSwingOutcome {
    Pull,
    Stop,
    Continue,
}

pub fn net_swing_outcome(check_type: i32, end_flag: bool, hit: bool) -> NetSwingOutcome {
    if end_flag || hit {
        if check_type == 2 {
            NetSwingOutcome::Pull
        } else if check_type == 0 {
            NetSwingOutcome::Stop
        } else {
            NetSwingOutcome::Continue
        }
    } else {
        NetSwingOutcome::Continue
    }
}

/// Request priorities used inside the net/rod families (verified at
/// the call sites).
pub mod tool_priority {
    pub const SWING_NET: i32 = 22;
    pub const PULL_NET: i32 = 26;
    pub const STOP_NET: i32 = 26;
    pub const VIB_ROD: i32 = 26;
    pub const COLLECT_ROD: i32 = 26;
    pub const FLY_ROD: i32 = 27;
}

/// PULL_NET catch demo message: 0xA2C base, replaced by the insect's
/// own message when one was caught.
pub const PULL_NET_MSG_BASE: i32 = 0xA2C;

/// Rod bite chain, verbatim:
/// RELAX_ROD case 5 → VIB_ROD (priority 26);
/// RELAX_ROD case 6 → COLLECT_ROD (priority 26);
/// VIB_ROD with nonzero item status → FLY_ROD (priority 27).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RodBiteOutcome {
    Vibrate,
    Collect,
    Nothing,
}

pub fn relax_rod_case(case: i32) -> RodBiteOutcome {
    match case {
        5 => RodBiteOutcome::Vibrate,
        6 => RodBiteOutcome::Collect,
        _ => RodBiteOutcome::Nothing,
    }
}

pub fn vib_rod_hook(status: i32) -> bool {
    status != 0
}

/// CAST_ROD: the cast stroke sound fires at frame 20.
pub fn cast_rod_frame_event(frame: f32) -> bool {
    frame == 20.0
}

// ---- C ABI ----

/// C ABI: SWING_AXE frame event. 0 = none, 1 = Whoosh, 2 = Hit,
/// 3 = BeeAttackStatus, 4 = SettleOrWalk.
#[no_mangle]
pub extern "C" fn pc_swing_axe_frame_event(frame: f32) -> u8 {
    match swing_axe_frame_event(frame, false, false) {
        None => 0,
        Some(AxeSwingEvent::Whoosh) => 1,
        Some(AxeSwingEvent::Hit) => 2,
        Some(AxeSwingEvent::BeeAttackStatus) => 3,
        Some(AxeSwingEvent::SettleOrWalk) => 4,
    }
}

/// C ABI: tree-hit outcome. Writes (became_stump, dropped_fruit,
/// bee_counter_set) as bytes to out[3]; returns 1.
#[no_mangle]
pub extern "C" fn pc_tree_hit_outcome(cutcount_remaining: i32, is_bee_tree: u8, out: *mut u8) -> u8 {
    if out.is_null() {
        return 0;
    }
    let (stump, fruit, bee) = tree_hit_outcome(cutcount_remaining, is_bee_tree != 0);
    unsafe {
        *out.add(0) = stump as u8;
        *out.add(1) = fruit as u8;
        *out.add(2) = bee as u8;
    }
    1
}

/// C ABI: net catch capsule length.
#[no_mangle]
pub extern "C" fn pc_net_catch_length(is_gold_net: u8) -> f32 {
    net_catch_length(is_gold_net != 0)
}

/// C ABI: SWING_NET outcome. 0 = Continue, 1 = Pull, 2 = Stop.
#[no_mangle]
pub extern "C" fn pc_net_swing_outcome(check_type: i32, end_flag: u8, hit: u8) -> u8 {
    match net_swing_outcome(check_type, end_flag != 0, hit != 0) {
        NetSwingOutcome::Continue => 0,
        NetSwingOutcome::Pull => 1,
        NetSwingOutcome::Stop => 2,
    }
}

/// C ABI: RELAX_ROD case routing. 0 = Nothing, 1 = Vibrate, 2 = Collect.
#[no_mangle]
pub extern "C" fn pc_relax_rod_case(case: i32) -> u8 {
    match relax_rod_case(case) {
        RodBiteOutcome::Nothing => 0,
        RodBiteOutcome::Vibrate => 1,
        RodBiteOutcome::Collect => 2,
    }
}

/// C ABI: VIB_ROD hook decision.
#[no_mangle]
pub extern "C" fn pc_vib_rod_hook(status: i32) -> u8 {
    vib_rod_hook(status) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn axe_family() {
        assert_eq!(swing_axe_frame_event(10.0, false, false), Some(AxeSwingEvent::Whoosh));
        assert_eq!(swing_axe_frame_event(15.0, false, false), Some(AxeSwingEvent::Hit));
        assert_eq!(swing_axe_frame_event(16.5, false, false), Some(AxeSwingEvent::BeeAttackStatus));
        assert_eq!(swing_axe_frame_event(17.0, true, false), Some(AxeSwingEvent::SettleOrWalk));
        assert_eq!(swing_axe_frame_event(14.0, false, false), None);
        assert_eq!(AXE_HIT_OFFSET, (-7.0, 20.0, 24.0));
        // Tree resolution: cutcount exhausted -> stump.
        assert_eq!(tree_hit_outcome(0, false), (true, true, false));
        assert_eq!(tree_hit_outcome(2, false), (false, true, false));
        // Bee tree: no fruit, bee counter set.
        assert_eq!(tree_hit_outcome(2, true), (false, false, true));
        assert_eq!(AXE_BEE_COUNTER, 5.0);
        assert!(reflect_axe_settle_frame(30.5));
        assert!(!reflect_axe_settle_frame(30.0));
        assert!(air_axe_settle_frame(35.5));
        assert!(!air_axe_settle_frame(35.0));
        // C ABI.
        assert_eq!(pc_swing_axe_frame_event(15.0), 2);
        assert_eq!(pc_swing_axe_frame_event(14.0), 0);
        let mut out = [0u8; 3];
        assert_eq!(pc_tree_hit_outcome(0, 0, out.as_mut_ptr()), 1);
        assert_eq!(out, [1, 1, 0]);
        assert_eq!(pc_tree_hit_outcome(3, 1, out.as_mut_ptr()), 1);
        assert_eq!(out, [0, 0, 1]);
    }

    #[test]
    fn net_family() {
        assert_eq!(net_catch_length(false), 50.0);
        assert_eq!(net_catch_length(true), 60.0);
        assert_eq!(net_swing_outcome(2, true, true), NetSwingOutcome::Pull);
        assert_eq!(net_swing_outcome(0, true, false), NetSwingOutcome::Stop);
        assert_eq!(net_swing_outcome(2, false, false), NetSwingOutcome::Continue);
        assert_eq!(net_swing_outcome(1, true, false), NetSwingOutcome::Continue);
        assert_eq!(PULL_NET_MSG_BASE, 0xA2C);
        // C ABI.
        assert_eq!(pc_net_catch_length(0), 50.0);
        assert_eq!(pc_net_catch_length(1), 60.0);
        assert_eq!(pc_net_swing_outcome(2, 1, 1), 1);
        assert_eq!(pc_net_swing_outcome(0, 1, 0), 2);
        assert_eq!(pc_net_swing_outcome(2, 0, 0), 0);
    }

    #[test]
    fn rod_family() {
        assert_eq!(relax_rod_case(5), RodBiteOutcome::Vibrate);
        assert_eq!(relax_rod_case(6), RodBiteOutcome::Collect);
        assert_eq!(relax_rod_case(4), RodBiteOutcome::Nothing);
        assert!(vib_rod_hook(1));
        assert!(!vib_rod_hook(0));
        assert!(cast_rod_frame_event(20.0));
        assert!(!cast_rod_frame_event(19.0));
        // C ABI.
        assert_eq!(pc_relax_rod_case(5), 1);
        assert_eq!(pc_relax_rod_case(6), 2);
        assert_eq!(pc_relax_rod_case(0), 0);
        assert_eq!(pc_vib_rod_hook(3), 1);
        assert_eq!(pc_vib_rod_hook(0), 0);
    }
}
