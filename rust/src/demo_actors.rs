//! Scripted demo controllers: intro (`ac_intro_demo.c`), present delivery
//! (`ac_present_demo.c`) and train arrival (`ac_ride_off_demo.c`).
//!
//! Source-verified against ac-decomp `src/actor/ac_{intro,present,
//! ride_off}_demo{.c,_move.c_inc}` and their headers (primary);
//! ACGC-PC-Port identical. All three are `ACTOR_PART_CONTROL` actors that
//! install themselves as `demo_clip` (`demo_clip2` for presents) and run a
//! table of action procs; the step functions below return what the C side
//! must do instead of calling game APIs.
//!
//! ## Train arrival (shared by intro and ride-off)
//! first_set (train_coming_flag = 3, player in "standing in train" demo,
//! player placed at (1970, 760) = block (3,1) + (1.25, 3) units) ->
//! train_birth_wait (camera to (2180, 820); wait for TRAIN1, set its
//! arg0) -> ride_train (wait for train action 5) -> ride_off_player
//! (station master speaks; unit (2180, 820) becomes stone) ->
//! walk_one_unit (walk to (2220, 840) at 2.5). The ride-off demo ends
//! there; the intro continues.
//! Naming quirk: ride-off writes its "Y" start constant (760) into Z.
//!
//! ## Intro (first day)
//! go_out_of_station (z >= 970) -> birth_rcn_guide (Tom Nook variant by
//! shop level; spawned at unit (8, 15), or for a house restart at
//! houses (6,5)/(9,5)/(6,12)/(9,12) offset (+-10, 8)) -> first_talk_end ->
//! walk_after_rcn_guide (follow at the guide's speed, 0.6x inside 80
//! units) -> decide_house (acre changes disabled) -> retire_rcn_guide
//! (house claimed, cloth/cassette/carpet/wallpaper catalogued, loan set
//! to `mPlayer_DEBT0`, first job started) -> first_job (until done).
//! Re-entry: not first intro -> first_job; indoors -> in_house (idle);
//! coming back out of a player house -> house-restart branch.
//! Naming quirk: a house whose owner ID is *null* is passed as
//! `aID_STATE_TAKEN_HOUSE`, an owned one as `FREE_HOUSE` (decomp names
//! appear swapped relative to the check).
//!
//! ## Present demo (`aPRD_setup_present`)
//! Priority: (1) today is the player's birthday and a villager is
//! assigned -> NES furniture `FTR_FAMICOM_COMMON02`; (2) a lighthouse
//! quest state for Tortimer (Jan/Feb contributed -> his present, started
//! -> no present); (3) all fish caught and no golden rod trophy -> golden
//! rod; (4) all insects and no golden net -> golden net; (5) fallback
//! `SONCHO_VACATION1_STARTED` with no present.

use std::ffi::c_int;

// ---------------------------------------------------------------------------
// Shared train arrival
// ---------------------------------------------------------------------------

pub const PLAYER_START_X: f32 = 640.0 * 3.0 + 1.25 * 40.0; // 1970
pub const PLAYER_START_Z: f32 = 640.0 + 3.0 * 40.0; // 760
pub const OFF_UT_X: f32 = 640.0 * 3.0 + 7.0 * 40.0 + 20.0; // 2220
pub const OFF_UT_Z: f32 = 640.0 + 5.0 * 40.0; // 840
pub const OUT_OF_STATION_Z: f32 = 640.0 + 8.25 * 40.0; // 970
pub const TRAIN_CAMERA_POS: (f32, f32) = (2180.0, 820.0);
pub const TRAIN_STOPPED_ACTION: i32 = 5;
pub const WALK_SPEED: f32 = 2.5;

/// Inputs the train scripts poll each frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct TrainInputs {
    pub train_found: bool,
    pub train_action: i32,
    pub master_speaking: bool,
    pub player_z: f32,
    pub master_speed: f32,
}

// ---------------------------------------------------------------------------
// Ride-off demo
// ---------------------------------------------------------------------------

/// `aROD` actions (names mirror the C procs).
#[allow(clippy::enum_variant_names)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RideOff {
    FirstSet,
    TrainBirthWait,
    RideTrain,
    RideOffPlayer { heard: bool },
    WalkOneUnit,
    Done,
}

/// One `aROD_actor_move` frame.
pub fn ride_off_step(state: RideOff, i: &TrainInputs) -> RideOff {
    match state {
        RideOff::FirstSet => RideOff::TrainBirthWait,
        RideOff::TrainBirthWait if i.train_found => RideOff::RideTrain,
        RideOff::RideTrain if i.train_action == TRAIN_STOPPED_ACTION => RideOff::RideOffPlayer { heard: false },
        RideOff::RideOffPlayer { heard: false } if i.master_speaking => RideOff::RideOffPlayer { heard: true },
        RideOff::RideOffPlayer { heard: true } if !i.master_speaking => RideOff::WalkOneUnit,
        RideOff::WalkOneUnit if i.player_z >= OFF_UT_Z && crate::game_math::f32_is_zero(i.master_speed) => {
            RideOff::Done
        }
        s => s,
    }
}

// ---------------------------------------------------------------------------
// Intro demo
// ---------------------------------------------------------------------------

/// `aID_ACT_*`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntroAct {
    FirstSet = 0,
    TrainBirthWait,
    RideTrain,
    RideOffPlayer,
    WalkOneUnit,
    GoOutOfStation,
    BirthRcnGuide,
    FirstTalkEndWait,
    WalkAfterRcnGuide,
    InHouse,
    DecideHouse,
    RetireRcnGuideWait,
    FirstJob,
}

/// Where `aID_actor_ct` starts.
pub fn intro_start(first_intro: bool, outdoors: bool, came_from_player_room: bool) -> (IntroAct, Option<IntroAct>) {
    if !first_intro {
        (IntroAct::FirstJob, None)
    } else if !outdoors {
        (IntroAct::InHouse, None)
    } else if came_from_player_room {
        // House restart: guide appears at the house, then decide_house.
        (IntroAct::BirthRcnGuide, Some(IntroAct::DecideHouse))
    } else {
        (IntroAct::FirstSet, None)
    }
}

/// Raccoon guide spawn for `aID_birth_rcn_guide`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GuideSpawn {
    pub ux: i32,
    pub uz: i32,
    pub ofs_x: f32,
    pub ofs_z: f32,
    pub param: i16,
    pub data: i32,
    pub kind: i32,
}

pub const ID_STATE_INITIAL: i16 = 0;
pub const ID_STATE_TAKEN_HOUSE: i16 = 1;
pub const ID_STATE_FREE_HOUSE: i16 = 2;

/// `aID_birth_rcn_guide` placement. `house_owner_null` is
/// `mPr_NullCheckPersonalID(&homes[house].ownerID)`.
pub fn guide_spawn(house_restart: Option<(usize, bool)>) -> GuideSpawn {
    const UX: [i32; 4] = [6, 9, 6, 9];
    const UZ: [i32; 4] = [5, 5, 12, 12];
    const OFS_X: [f32; 4] = [10.0, -10.0, 10.0, -10.0];
    match house_restart {
        Some((h, owner_null)) => {
            let h = h & 3;
            GuideSpawn {
                ux: UX[h],
                uz: UZ[h],
                ofs_x: OFS_X[h],
                ofs_z: 8.0,
                param: if owner_null { ID_STATE_TAKEN_HOUSE } else { ID_STATE_FREE_HOUSE },
                data: 3,
                kind: 2,
            }
        }
        None => GuideSpawn { ux: 8, uz: 15, ofs_x: 0.0, ofs_z: 0.0, param: ID_STATE_INITIAL, data: 3, kind: 1 },
    }
}

/// Guide NPC per shop level (`rcn_name[mSP_GetShopLevel()]`).
pub const RCN_GUIDE_NAMES: [u16; 4] = [0xD014, 0xD015, 0xD016, 0xD017]; // SP_NPC_RCN_GUIDE{,_1,_2,_3}

/// `aID_walk_after_rcn_guide` follow speed.
pub fn follow_rate(guide_speed: f32, player_distance_xz: f32) -> f32 {
    const RATE: [f32; 2] = [1.0, 0.6];
    RATE[(player_distance_xz < 80.0) as usize] * guide_speed
}

// ---------------------------------------------------------------------------
// Present demo
// ---------------------------------------------------------------------------

/// `aPRD_TYPE_*`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresentType {
    Birthday = 0,
    GoldenRod,
    GoldenNet,
    SonchoVacation0Contributed,
    SonchoVacation1Contributed,
    SonchoVacation0Started,
    SonchoVacation1Started,
}

/// `mSC_LIGHTHOUSE_EVENT_*` as consumed here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LighthouseEvent {
    None,
    JanContributed,
    FebContributed,
    JanStarted,
    FebStarted,
}

/// Inputs to `aPRD_setup_present`.
#[derive(Clone, Copy, Debug)]
pub struct PresentInputs {
    pub is_birthday: bool,
    pub birthday_npc_set: bool,
    pub lighthouse: LighthouseEvent,
    pub lighthouse_present: u16,
    pub all_fish: bool,
    pub has_golden_rod: bool,
    pub all_insects: bool,
    pub has_golden_net: bool,
}

pub const ITM_GOLDEN_ROD: u16 = 0x223C;
pub const ITM_GOLDEN_NET: u16 = 0x2239;
/// `FTR_START(FTR_FAMICOM_COMMON02)`.
pub const FTR_FAMICOM_COMMON02_ITEM: u16 = 0x1DB0; // verified with gcc

/// `aPRD_setup_present`: (type, present item).
pub fn choose_present(i: &PresentInputs) -> (PresentType, u16) {
    if i.is_birthday && i.birthday_npc_set {
        return (PresentType::Birthday, FTR_FAMICOM_COMMON02_ITEM);
    }
    match i.lighthouse {
        LighthouseEvent::JanContributed => return (PresentType::SonchoVacation0Contributed, i.lighthouse_present),
        LighthouseEvent::FebContributed => return (PresentType::SonchoVacation1Contributed, i.lighthouse_present),
        LighthouseEvent::JanStarted => return (PresentType::SonchoVacation0Started, 0),
        LighthouseEvent::FebStarted => return (PresentType::SonchoVacation1Started, 0),
        LighthouseEvent::None => {}
    }
    if i.all_fish && !i.has_golden_rod {
        (PresentType::GoldenRod, ITM_GOLDEN_ROD)
    } else if i.all_insects && !i.has_golden_net {
        (PresentType::GoldenNet, ITM_GOLDEN_NET)
    } else {
        (PresentType::SonchoVacation1Started, 0)
    }
}

#[no_mangle]
pub extern "C" fn pc_present_demo_follow_rate(guide_speed: f32, dist: f32) -> f32 {
    follow_rate(guide_speed, dist)
}

#[no_mangle]
pub extern "C" fn pc_intro_demo_start(first_intro: c_int, outdoors: c_int, from_player_room: c_int) -> c_int {
    intro_start(first_intro != 0, outdoors != 0, from_player_room != 0).0 as c_int
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constants_match_header_math() {
        assert_eq!(PLAYER_START_X, 1970.0);
        assert_eq!(PLAYER_START_Z, 760.0);
        assert_eq!(OFF_UT_X, 2220.0);
        assert_eq!(OFF_UT_Z, 840.0);
        assert_eq!(OUT_OF_STATION_Z, 970.0);
    }

    #[test]
    fn ride_off_sequence() {
        let mut s = RideOff::FirstSet;
        let mut i = TrainInputs::default();
        s = ride_off_step(s, &i);
        assert_eq!(s, RideOff::TrainBirthWait);
        i.train_found = true;
        s = ride_off_step(s, &i);
        i.train_action = 5;
        s = ride_off_step(s, &i);
        i.master_speaking = true;
        s = ride_off_step(s, &i);
        i.master_speaking = false;
        s = ride_off_step(s, &i);
        assert_eq!(s, RideOff::WalkOneUnit);
        i.player_z = 840.0;
        assert_eq!(ride_off_step(s, &i), RideOff::Done);
    }

    #[test]
    fn present_priority() {
        let base = PresentInputs {
            is_birthday: false,
            birthday_npc_set: false,
            lighthouse: LighthouseEvent::None,
            lighthouse_present: 0,
            all_fish: true,
            has_golden_rod: false,
            all_insects: true,
            has_golden_net: false,
        };
        assert_eq!(choose_present(&base).0, PresentType::GoldenRod);
        let p = PresentInputs { has_golden_rod: true, ..base };
        assert_eq!(choose_present(&p).0, PresentType::GoldenNet);
        let p = PresentInputs { is_birthday: true, birthday_npc_set: true, ..base };
        assert_eq!(choose_present(&p), (PresentType::Birthday, FTR_FAMICOM_COMMON02_ITEM));
        let p = PresentInputs { all_fish: false, all_insects: false, ..base };
        assert_eq!(choose_present(&p), (PresentType::SonchoVacation1Started, 0));
    }

    #[test]
    fn intro_entry_and_guide() {
        assert_eq!(intro_start(false, true, false).0, IntroAct::FirstJob);
        assert_eq!(intro_start(true, false, false).0, IntroAct::InHouse);
        assert_eq!(intro_start(true, true, true), (IntroAct::BirthRcnGuide, Some(IntroAct::DecideHouse)));
        let g = guide_spawn(Some((3, true)));
        assert_eq!((g.ux, g.uz, g.ofs_x, g.param), (9, 12, -10.0, ID_STATE_TAKEN_HOUSE));
        assert_eq!(follow_rate(2.0, 50.0), 1.2);
    }
}

#[cfg(test)]
mod tests_wave4 {
    use super::*;

    #[test]
    fn train_stall_cases() {
        // RideTrain waits for the exact stopped action.
        let mut i = TrainInputs::default();
        i.train_action = 4;
        assert_eq!(ride_off_step(RideOff::RideTrain, &i), RideOff::RideTrain);
        i.train_action = 6;
        assert_eq!(ride_off_step(RideOff::RideTrain, &i), RideOff::RideTrain);
        // WalkOneUnit needs z >= 840 AND the master stopped.
        let i = TrainInputs { player_z: 839.9, master_speed: 0.0, ..Default::default() };
        assert_eq!(ride_off_step(RideOff::WalkOneUnit, &i), RideOff::WalkOneUnit);
        let i = TrainInputs { player_z: 840.0, master_speed: 1.0, ..Default::default() };
        assert_eq!(ride_off_step(RideOff::WalkOneUnit, &i), RideOff::WalkOneUnit);
        let i = TrainInputs { player_z: 900.0, master_speed: 0.007, ..Default::default() };
        assert_eq!(ride_off_step(RideOff::WalkOneUnit, &i), RideOff::Done); // |0.007| < 0.008
        // FirstSet advances unconditionally; TrainBirthWait stalls without the train.
        assert_eq!(ride_off_step(RideOff::FirstSet, &TrainInputs::default()), RideOff::TrainBirthWait);
        assert_eq!(ride_off_step(RideOff::TrainBirthWait, &TrainInputs::default()), RideOff::TrainBirthWait);
        // Heard flag latches: speaking again after heard stays latched.
        let heard = RideOff::RideOffPlayer { heard: true };
        let i = TrainInputs { master_speaking: true, ..Default::default() };
        assert_eq!(ride_off_step(heard, &i), heard);
    }

    #[test]
    fn present_lighthouse_and_guide() {
        let base = PresentInputs {
            is_birthday: false,
            birthday_npc_set: false,
            lighthouse: LighthouseEvent::None,
            lighthouse_present: 0x1DB1,
            all_fish: true,
            has_golden_rod: false,
            all_insects: true,
            has_golden_net: false,
        };
        // Lighthouse states outrank the golden tools.
        let p = PresentInputs { lighthouse: LighthouseEvent::JanContributed, ..base };
        assert_eq!(choose_present(&p), (PresentType::SonchoVacation0Contributed, 0x1DB1));
        let p = PresentInputs { lighthouse: LighthouseEvent::FebContributed, ..base };
        assert_eq!(choose_present(&p), (PresentType::SonchoVacation1Contributed, 0x1DB1));
        let p = PresentInputs { lighthouse: LighthouseEvent::JanStarted, ..base };
        assert_eq!(choose_present(&p), (PresentType::SonchoVacation0Started, 0));
        let p = PresentInputs { lighthouse: LighthouseEvent::FebStarted, ..base };
        assert_eq!(choose_present(&p), (PresentType::SonchoVacation1Started, 0));
        // Birthday without an assigned villager falls through.
        let p = PresentInputs { is_birthday: true, birthday_npc_set: false, ..base };
        assert_eq!(choose_present(&p).0, PresentType::GoldenRod);
        // Default guide spawn (station): kind 1, data 3, initial state.
        let g = guide_spawn(None);
        assert_eq!((g.ux, g.uz, g.ofs_x, g.ofs_z), (8, 15, 0.0, 0.0));
        assert_eq!((g.param, g.data, g.kind), (ID_STATE_INITIAL, 3, 1));
        // House-restart wrap: index masked to 4 houses, owned house -> FREE_HOUSE.
        let g = guide_spawn(Some((9, false)));
        assert_eq!((g.ux, g.uz, g.ofs_x, g.param), (9, 5, -10.0, ID_STATE_FREE_HOUSE));
        // Follow rate boundary: exactly 80 units is the far rate.
        assert_eq!(follow_rate(2.0, 80.0), 2.0);
        assert_eq!(follow_rate(2.0, 79.9), 1.2);
        assert_eq!(RCN_GUIDE_NAMES.len(), 4);
        assert_eq!(TRAIN_CAMERA_POS, (2180.0, 820.0));
        assert_eq!(TRAIN_STOPPED_ACTION, 5);
    }
}
