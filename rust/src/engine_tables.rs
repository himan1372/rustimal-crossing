//! Small engine pieces: actor profile table (`m_actor_dlftbls.c`), BG item
//! respawn (`m_bg_item.c`), clip table reset (`m_clip.c`), BG texture BSS
//! buffers (`m_bg_tex.c`), the pre-NMI reset screen (`m_prenmi.c`), the
//! insect/fish spawn scheduler (`ac_set_manager.c`), waterfalls
//! (`ac_fallS.c`, `ac_fallSESW.c`) and the empty stub actors
//! (`ac_boxManager.c`, `ac_boxMove.c`, `ac_boxTrick01.c`,
//! `ac_fieldm_draw.c`).
//!
//! Source-verified against ac-decomp (primary) and ACGC-PC-Port
//! (secondary). PC differences: `m_game_dlftbls.c` appends
//! `pc_model_viewer` (index 10, see `scene.rs`); `m_prenmi.c` counts its
//! timer in f32 60 fps frames; `ac_boxTrick01.c` forward-declares its
//! procs `static` (retail declares them `extern` then defines `static`).
//!
//! ## Facts
//! - `actor_dlftbls[246]`: index = `mAc_PROFILE_*` id (verified with the
//!   compiled enum: FIELD_DRAW 4, BOXMANAGER 14, UKI 111, SET_MANAGER 114,
//!   EFFECTBG 119, NUM 246). One slot (index 185) is `MAKE_ACTOR_NULL`.
//! - `clip_clear` NULLs every pointer in `Clip_c` except the last.
//! - BG items: `mBI_change_bg_item` deletes whichever of the four seasonal
//!   BG item actors exists (cherry, normal, winter, xmas, searched in that
//!   order), sets `bg_item_profile = time.bgitem_profile` and arms a
//!   respawn that `mBI_move` runs each frame until no BG item actor is
//!   found (then spawns the new profile once).
//! - `m_bg_tex.c` reserves 32 dummy texture/palette buffers in a fixed
//!   BSS order (sizes below), overwritten per acre with the real data.
//! - Pre-NMI: 30 frames; a white horizontal line shrinks from y=127 to
//!   y=112 while fading alpha from 250 to 10 (CRT switch-off), then
//!   `viBlack` and `GAME_GOTO_NEXT(second_game)`.
//! - Waterfalls only place a looping sound source (SE 12) 40 units above
//!   the actor; FG ids `WATERFALL_SOUTH` / `WATERFALL_EAST`.
//! - BoxManager/BoxMove/BoxTrick01/Fieldm_Draw have empty procs in the
//!   shipping game (leftovers of an N64-era puzzle/field system).

use std::ffi::c_int;

// ---------------------------------------------------------------------------
// Prenmi
// ---------------------------------------------------------------------------

/// `PRENMI_TIMER`.
pub const PRENMI_TIMER: i32 = 30;

/// Line alpha and Y for a pre-NMI timer value (`prenmi_draw`).
pub fn prenmi_line(timer: f32) -> (u8, f32) {
    let t = 1.0 - timer / PRENMI_TIMER as f32;
    let alpha = (t * -240.0 + 250.0) as u8;
    let y = t * -15.0 + 127.0;
    (alpha, y)
}

/// `prenmi_move`: true when the reset proceeds to `second_game`.
pub fn prenmi_done(reset_delay: bool, timer: f32) -> bool {
    reset_delay || timer <= 0.0
}

// ---------------------------------------------------------------------------
// BG item respawn
// ---------------------------------------------------------------------------

/// Search order of `mBI_search_bg_item` (profile names).
pub const BG_ITEM_PROFILES: [&str; 4] = ["BgCherryItem", "BgItem", "BgWinterItem", "BgXmasItem"];

/// `mBI` state: armed respawn of the seasonal BG item actor.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BgItemRespawn {
    pub armed: bool,
}

impl BgItemRespawn {
    /// `mBI_change_bg_item`: returns true when an actor was deleted
    /// (and `bg_item_profile` must be updated).
    pub fn change(&mut self, bg_item_exists: bool) -> bool {
        if bg_item_exists {
            self.armed = true;
        }
        bg_item_exists
    }

    /// `mBI_move`: true when this frame should spawn the new actor.
    pub fn tick(&mut self, bg_item_exists: bool) -> bool {
        if self.armed && !bg_item_exists {
            self.armed = false;
            return true;
        }
        false
    }
}

// ---------------------------------------------------------------------------
// BG texture BSS
// ---------------------------------------------------------------------------

/// `m_bg_tex.c` dummy buffers in BSS order (name, bytes).
pub const BG_TEX_BUFFERS: [(&str, usize); 32] = [
    ("sprashC_tex", 512),
    ("sprashA_tex", 512),
    ("wave3_tex", 1024),
    ("wave2_tex", 2048),
    ("wave1_tex", 1024),
    ("sand_tex", 1024),
    ("beach_pal2", 32),
    ("beach2_tex2", 256),
    ("beach1_tex2", 1024),
    ("beach_tex", 2048),
    ("tunnel_tex", 2048),
    ("rail_pal", 32),
    ("bush_pal", 32),
    ("cliff_pal", 32),
    ("earth_pal", 32),
    ("station_pal", 32),
    ("bridge_2_pal", 32),
    ("bridge_1_pal", 32),
    ("tekkyo_tex", 2048),
    ("bridge_2_tex", 2048),
    ("bridge_1_tex", 2048),
    ("water_2_tex", 512),
    ("water_1_tex", 512),
    ("river_tex", 1024),
    ("stone_tex", 2048),
    ("station_tex", 2048),
    ("rail_tex", 2048),
    ("grass_tex", 512),
    ("bush_b_tex", 1024),
    ("bush_a_tex", 2048),
    ("cliff_tex", 2048),
    ("earth_tex", 2048),
];

// ---------------------------------------------------------------------------
// Set manager (insect / fish spawn scheduler)
// ---------------------------------------------------------------------------

/// `aSetMgr_WAIT_TIME`.
pub const SET_MGR_WAIT_TIME: i32 = 5;
/// `aSetMgr_OVERLAY_*`: 0 = insects (`aSOI_insect_set`), 1 = fish
/// (`aSOG_gyoei_set`).
pub const SET_MGR_OVERLAY_NUM: i32 = 2;

/// `aSetMgr_MOVE_*`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SetMgrProc {
    CheckSet = 0,
    CheckWait = 1,
    Set = 2,
}

/// What one `aSetMgr_move` frame asks of the C side.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SetMgrAction {
    None,
    /// Record the wade end block (`Get_WadeEndPos_proc`).
    RecordNextBlock,
    /// Run spawn overlay `type` (0 insects, 1 fish).
    RunOverlay(i32),
}

/// `SET_MANAGER` move state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SetManager {
    pub move_proc: SetMgrProc,
    pub next_move_proc: SetMgrProc,
    pub set_ovl_type: i32,
    pub wait_timer: i32,
}

impl Default for SetManager {
    /// `aSetMgr_ct`.
    fn default() -> Self {
        Self {
            move_proc: SetMgrProc::CheckSet,
            next_move_proc: SetMgrProc::CheckSet,
            set_ovl_type: 0,
            wait_timer: SET_MGR_WAIT_TIME,
        }
    }
}

impl SetManager {
    /// `aSetMgr_move`: `wade_start` / `wade_end` are
    /// `mFI_CheckPlayerWade(START/END)`.
    pub fn step(&mut self, wade_start: bool, wade_end: bool) -> SetMgrAction {
        let (done, action) = match self.move_proc {
            SetMgrProc::CheckSet => {
                if wade_start {
                    self.next_move_proc = SetMgrProc::CheckWait;
                    (true, SetMgrAction::RecordNextBlock)
                } else {
                    (false, SetMgrAction::None)
                }
            }
            SetMgrProc::CheckWait => {
                let mut done = false;
                if self.next_move_proc == SetMgrProc::CheckWait {
                    let t = if self.wait_timer == 0 {
                        0
                    } else {
                        self.wait_timer -= 1;
                        self.wait_timer
                    };
                    if t == 0 {
                        self.next_move_proc = SetMgrProc::Set;
                        self.wait_timer = SET_MGR_WAIT_TIME;
                        done = true;
                    }
                }
                (done, SetMgrAction::None)
            }
            SetMgrProc::Set => {
                let ty = if (0..SET_MGR_OVERLAY_NUM).contains(&self.set_ovl_type) { self.set_ovl_type } else { 0 };
                self.set_ovl_type += 1;
                let mut done = false;
                if self.set_ovl_type >= SET_MGR_OVERLAY_NUM || wade_end {
                    self.next_move_proc = SetMgrProc::CheckSet;
                    self.set_ovl_type = 0;
                    done = true;
                }
                (done, SetMgrAction::RunOverlay(ty))
            }
        };
        if done {
            self.move_proc = self.next_move_proc;
        }
        action
    }
}

// ---------------------------------------------------------------------------
// Waterfalls
// ---------------------------------------------------------------------------

/// Sound source lift and SE for `aFLS/aFLEW_actor_move`.
pub const WATERFALL_SE_LIFT: f32 = 40.0;
pub const WATERFALL_SE: u16 = 12;

// ---------------------------------------------------------------------------
// Actor profile table
// ---------------------------------------------------------------------------

/// Profile name for an `mAc_PROFILE_*` id (`actor_dlftbls`).
pub fn actor_profile_name(id: usize) -> Option<&'static str> {
    ACTOR_PROFILE_NAMES.get(id).copied().flatten()
}

/// Number of profiles (`actor_dlftbls_num`).
#[no_mangle]
pub extern "C" fn pc_actor_dlftbls_num() -> c_int {
    ACTOR_PROFILE_NAMES.len() as c_int
}

// ---------------------------------------------------------------------------
// Generated by tools/gen_actor_profiles.py from src/game/m_actor_dlftbls.c
// ---------------------------------------------------------------------------
/// `actor_dlftbls` profile names by `mAc_PROFILE_*` id (None = `MAKE_ACTOR_NULL`).
pub const ACTOR_PROFILE_NAMES: [Option<&str>; 246] = [
    Some("Player"), Some("BgItem"), Some("Sample"), Some("Fieldm_Draw"), Some("Field_Draw"), Some("Airplane"),
    Some("Room_Sunshine"), Some("Lamp_Light"), Some("Ev_Angler"), Some("Ball"), Some("Haniwa"), Some("My_Room"),
    Some("Mbg"), Some("T_Tama"), Some("BoxManager"), Some("BoxMove"), Some("BoxTrick01"), Some("Arrange_Room"),
    Some("Arrange_Furniture"), Some("TrainDoor"), Some("T_Keitai"), Some("Halloween_Npc"), Some("Ev_Pumpkin"), Some("Ride_Off_Demo"),
    Some("Npc_Mamedanuki"), Some("Hanabi_Npc0"), Some("Hanabi_Npc1"), Some("Snowman"), Some("Npc_Post_Girl"), Some("Npc_Engineer"),
    Some("Npc_Majin3"), Some("Npc_Sleep_Obaba"), Some("Npc"), Some("Effect_Control"), Some("Npc2"), Some("Kamakura_Npc0"),
    Some("Npc_Post_Man"), Some("Shop_Design"), Some("Quest_Manager"), Some("MailBox"), Some("House"), Some("Shop_Level"),
    Some("Shop"), Some("MyHouse"), Some("Ev_Artist"), Some("Ev_Broker"), Some("Ev_Designer"), Some("T_Umbrella"),
    Some("Npc_Shop_Master"), Some("Birth_Control"), Some("Shop_Manekin"), Some("Shop_Indoor"), Some("Event_Manager"), Some("Shop_Goods"),
    Some("BrShop"), Some("Weather"), Some("Post_Office"), Some("Npc_Guide"), Some("Npc_Guide2"), Some("Insect"),
    Some("Station"), Some("Ev_CarpetPeddler"), Some("Ev_KabuPeddler"), Some("Reserve"), Some("HandOverItem"), Some("Npc_Conv_Master"),
    Some("Npc_Super_Master"), Some("Npc_Depart_Master"), Some("Tools"), Some("Structure"), Some("Ev_Gypsy"), Some("Npc_Police"),
    Some("Train0"), Some("Train1"), Some("Npc_Station_Master"), Some("Ev_Santa"), Some("Npc_Police2"), Some("Police_Box"),
    Some("BgPoliceItem"), Some("BgCherryItem"), Some("BgWinterItem"), Some("BgXmasItem"), Some("BgPostItem"), Some("FallS"),
    Some("FallSESW"), Some("Ev_Broker2"), Some("Broker_Design"), Some("T_Utiwa"), Some("Psnowman"), Some("My_Indoor"),
    Some("Npc_Rcn_Guide"), Some("Intro_Demo"), Some("Shrine"), Some("Buggy"), Some("T_Hanabi"), Some("Conveni"),
    Some("Super"), Some("Depart"), Some("Hanami_Npc0"), Some("S_Car"), Some("Hanami_Npc1"), Some("Npc_P_Sel"),
    Some("Npc_P_Sel2"), Some("Npc_Rcn_Guide2"), Some("Train_Window"), Some("Npc_Majin4"), Some("Kamakura"), Some("Gyoei"),
    Some("Npc_Majin"), Some("T_NpcSao"), Some("Ev_Soncho"), Some("Uki"), Some("Npc_Majin2"), Some("Normal_Npc"),
    Some("Set_Manager"), Some("Set_Npc_Manager"), Some("Npc_Shop_Mastersp"), Some("Room_Sunshine_Posthouse"), Some("Room_Sunshine_Police"), Some("Effectbg"),
    Some("Ev_Cherry_Manager"), Some("Ev_Yomise"), Some("Tokyoso_Npc0"), Some("Tokyoso_Npc1"), Some("Goza"), Some("Radio"),
    Some("Yatai"), Some("Tokyoso_Control"), Some("Shop_Umbrella"), Some("Gyo_Release"), Some("Tukimi"), Some("Kamakura_Indoor"),
    Some("Ev_Miko"), Some("Gyo_Kage"), Some("Mikuji"), Some("House_Goki"), Some("T_Cracker"), Some("Npc_Needlework"),
    Some("T_Pistol"), Some("T_Flag"), Some("T_Tumbler"), Some("Tukimi_Npc0"), Some("Tukimi_Npc1"), Some("Needlework_Shop"),
    Some("Countdown_Npc0"), Some("Countdown_Npc1"), Some("Turi_Npc0"), Some("Taisou_Npc0"), Some("Count"), Some("Garagara"),
    Some("Tamaire_Npc0"), Some("Tamaire_Npc1"), Some("Hatumode_Npc0"), Some("Npc_Totakeke"), Some("Count02"), Some("Hatumode_Control"),
    Some("Tama"), Some("Kago"), Some("Turi"), Some("House_Clock"), Some("Tunahiki_Control"), Some("Tunahiki_Npc0"),
    Some("Tunahiki_Npc1"), Some("Koinobori"), Some("Bee"), Some("Nameplate"), Some("Dump"), Some("Rope"),
    Some("Ev_Dozaemon"), Some("Windmill"), Some("Lotus"), Some("Animal_Logo"), Some("Mikanbox"), Some("Douzou"),
    Some("Npc_Rtc"), Some("Toudai"), Some("Npc_Restart"), Some("Npc_Majin5"), Some("Fuusen"), Some("Ev_Dokutu"),
    Some("Dummy"), Some("Npc_Curator"), Some("Museum"), Some("Ev_Ghost"), Some("Museum_Picture"), None,
    Some("Museum_Fish"), Some("Museum_Insect"), Some("Bridge_A"), Some("Room_Sunshine_Museum"), Some("Needlework_Indoor"), Some("Room_Sunshine_Minsect"),
    Some("Museum_Fossil"), Some("Mural"), Some("Ac_Sign"), Some("Ant"), Some("Npc_Sendo"), Some("Flag"),
    Some("Present_Demo"), Some("Present_Npc"), Some("Boat"), Some("Boat_Demo"), Some("Cottage_My"), Some("Cottage_Npc"),
    Some("Ev_Soncho2"), Some("Npc_Mask_Cat"), Some("Npc_Mask_Cat2"), Some("Reset_Demo"), Some("Go_Home_Npc"), Some("Misin"),
    Some("Lighthouse_Switch"), Some("Museum_Indoor"), Some("T_Anrium1"), Some("T_Biscus1"), Some("T_Biscus2"), Some("T_Hasu1"),
    Some("T_Hat1"), Some("T_Zinnia1"), Some("T_Biscus3"), Some("T_Biscus4"), Some("T_Zinnia2"), Some("T_Hat2"),
    Some("T_Hat3"), Some("T_Rei1"), Some("T_Rei2"), Some("Npc_Shasho"), Some("T_Bag1"), Some("T_Bag2"),
    Some("Ev_Castaway"), Some("T_Cobra1"), Some("Balloon"), Some("Npc_Soncho"), Some("Aprilfool_Control"), Some("Ev_Majin"),
    Some("Harvest_Npc0"), Some("Harvest_Npc1"), Some("Groundhog_Control"), Some("Ev_Speech_Soncho"), Some("Groundhog_Npc0"), Some("Ev_Turkey"),
    Some("Ghog"), Some("Htable"), Some("Npc_Hem"), Some("Tent"), Some("Pterminal"), Some("Mscore_Control"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_ids() {
        assert_eq!(ACTOR_PROFILE_NAMES.len(), 246);
        assert_eq!(actor_profile_name(4), Some("Field_Draw"));
        assert_eq!(actor_profile_name(14), Some("BoxManager"));
        assert_eq!(actor_profile_name(111), Some("Uki"));
        assert_eq!(actor_profile_name(114), Some("Set_Manager"));
        assert_eq!(actor_profile_name(119), Some("Effectbg"));
        assert_eq!(ACTOR_PROFILE_NAMES.iter().filter(|n| n.is_none()).count(), 1);
        assert_eq!(actor_profile_name(185), None);
    }

    #[test]
    fn prenmi_line_endpoints() {
        assert_eq!(prenmi_line(30.0), (250, 127.0));
        assert_eq!(prenmi_line(0.0), (10, 112.0));
    }

    #[test]
    fn set_manager_cycle() {
        let mut m = SetManager::default();
        assert_eq!(m.step(false, false), SetMgrAction::None);
        assert_eq!(m.step(true, false), SetMgrAction::RecordNextBlock);
        for _ in 0..4 {
            m.step(false, false);
            assert_eq!(m.move_proc, SetMgrProc::CheckWait);
        }
        m.step(false, false);
        assert_eq!(m.move_proc, SetMgrProc::Set);
        assert_eq!(m.step(false, false), SetMgrAction::RunOverlay(0));
        assert_eq!(m.step(false, false), SetMgrAction::RunOverlay(1));
        assert_eq!(m.move_proc, SetMgrProc::CheckSet);
    }

    #[test]
    fn bg_item_respawn() {
        let mut r = BgItemRespawn::default();
        assert!(r.change(true));
        assert!(!r.tick(true));
        assert!(r.tick(false));
        assert!(!r.tick(false));
    }
}
