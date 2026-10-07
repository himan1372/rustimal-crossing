//! Scene dispatcher for the Rust rewrite.
//!
//! Source-verified architecture (upstream `src/game/m_game_dlftbls.c`,
//! `src/graph.c`, `src/game.c`, `include/game.h`,
//! `include/m_game_dlftbls.h`, `src/first_game.c`,
//! `src/game/m_trademark.c`, `src/game/m_select.c`):
//!
//! * `game_dlftbls[]` ("Display List Function TaBLe") is the central scene
//!   table. Current decomp order: first_game (0), select (1), play (2),
//!   second_game (3), NULL (4, removed & unused), trademark (5),
//!   player_select (6), save_menu (7), famicom_emu (8), prenmi (9),
//!   pc_model_viewer (10, PC-port only).
//! * Each entry carries an init function pointer, a cleanup pointer, and
//!   `alloc_size` (`sizeof(GAME_<class>)`).
//! * The GAME struct holds `exec` at 0x4, `cleanup` at 0x8,
//!   `next_game_init` at 0xC, and `next_game_class_size` at 0x10.
//! * Transitions are function-pointer driven: `GAME_GOTO_NEXT` sets
//!   `doing = FALSE` and records the next init pointer plus the next
//!   scene's state size; `game_get_next_game_dlftbl` matches that pointer
//!   against the table to find the next entry.
//! * `graph_proc` runs the lifecycle: `malloc(alloc_size)` →
//!   `game_ct(init)` (sets `doing = TRUE`, clears next) → per-frame
//!   `graph_main` → `game_main` → `scene->exec()` while doing → resolve
//!   next entry → `game_dt` (cleanup) → `free`.
//! * Observed transitions: first_game → second_game; trademark → play;
//!   select → play. The per-frame `exec` model means menus, gameplay, the
//!   NES emulator, and debug scenes are peers, not nested loops.
//!
//! Rewrite-owned: the `Scene` trait and `SceneManager` below model this
//! architecture in Rust. Scene state sizes are per-build C values and are
//! not reproduced.

// Public scene/boot API for the rewrite and future adapters. The crate
// builds as a staticlib, so unused public items would warn as dead code.
#![allow(dead_code)]

/// Scene IDs in `game_dlftbls` table order. Index 4 is the removed/NULL
/// entry and has no scene.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SceneId {
    FirstGame = 0,
    Select = 1,
    Play = 2,
    SecondGame = 3,
    Trademark = 5,
    PlayerSelect = 6,
    SaveMenu = 7,
    FamicomEmu = 8,
    Prenmi = 9,
    /// PC-port only (`#ifdef TARGET_PC`).
    ModelViewer = 10,
}

/// Number of entries in `game_dlftbls` (including the NULL slot and the
/// PC-only model viewer).
pub const GAME_DLFTBLS_COUNT: usize = 11;

impl SceneId {
    /// Table index for this scene, or `None` for the removed slot.
    pub fn table_index(self) -> usize {
        self as usize
    }

    pub fn from_table_index(idx: usize) -> Option<SceneId> {
        match idx {
            0 => Some(SceneId::FirstGame),
            1 => Some(SceneId::Select),
            2 => Some(SceneId::Play),
            3 => Some(SceneId::SecondGame),
            5 => Some(SceneId::Trademark),
            6 => Some(SceneId::PlayerSelect),
            7 => Some(SceneId::SaveMenu),
            8 => Some(SceneId::FamicomEmu),
            9 => Some(SceneId::Prenmi),
            10 => Some(SceneId::ModelViewer),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            SceneId::FirstGame => "first_game",
            SceneId::Select => "select",
            SceneId::Play => "play",
            SceneId::SecondGame => "second_game",
            SceneId::Trademark => "trademark",
            SceneId::PlayerSelect => "player_select",
            SceneId::SaveMenu => "save_menu",
            SceneId::FamicomEmu => "famicom_emu",
            SceneId::Prenmi => "prenmi",
            SceneId::ModelViewer => "pc_model_viewer",
        }
    }
}

/// One row of the scene table: the init-function name and the scene.
/// (`alloc_size` is a per-build C value and is intentionally not modeled.)
pub struct SceneEntry {
    pub id: SceneId,
    pub init_fn: &'static str,
}

/// The scene table in decomp order, mirroring `game_dlftbls[]`.
pub static SCENE_TABLE: [SceneEntry; 10] = [
    SceneEntry { id: SceneId::FirstGame, init_fn: "first_game_init" },
    SceneEntry { id: SceneId::Select, init_fn: "select_init" },
    SceneEntry { id: SceneId::Play, init_fn: "play_init" },
    SceneEntry { id: SceneId::SecondGame, init_fn: "second_game_init" },
    SceneEntry { id: SceneId::Trademark, init_fn: "trademark_init" },
    SceneEntry { id: SceneId::PlayerSelect, init_fn: "player_select_init" },
    SceneEntry { id: SceneId::SaveMenu, init_fn: "save_menu_init" },
    SceneEntry { id: SceneId::FamicomEmu, init_fn: "famicom_emu_init" },
    SceneEntry { id: SceneId::Prenmi, init_fn: "prenmi_init" },
    SceneEntry { id: SceneId::ModelViewer, init_fn: "pc_model_viewer_init" },
];

/// What a scene's per-frame `exec` can request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SceneRequest {
    /// Keep running this scene (`doing` stays true).
    Continue,
    /// `GAME_GOTO_NEXT`: stop this scene and initialize `SceneId` next.
    Goto(SceneId),
    /// Shut the dispatcher down (`dlftbl = NULL` ends `graph_proc`).
    Shutdown,
}

/// A scene: `init` runs once on entry, `exec` runs every frame,
/// `cleanup` runs on exit. Mirrors the init/exec/cleanup split of the
/// `Game_dlftbl` + `GAME` structs.
///
/// The source contract: `init` is expected to install the scene's `exec`
/// (and optionally `cleanup`) on the GAME object — e.g. `play_init`
/// sets `game->exec = play_main`. `first_game` is the exception: it does
/// init-time work and requests the next scene without installing an exec.
pub trait Scene {
    fn init(&mut self);
    fn exec(&mut self) -> SceneRequest;
    fn cleanup(&mut self);
}

/// The dispatcher: tracks the current scene, the `doing` flag, and the
/// pending next-init pointer, mirroring the `GAME` struct fields at
/// 0x4/0x8/0xC.
pub struct SceneManager {
    current: Option<SceneId>,
    doing: bool,
    next_init: Option<SceneId>,
    frame: u64,
}

impl SceneManager {
    /// Boot: the dispatcher starts at `game_dlftbls[0]` (`first_game`).
    pub fn new() -> Self {
        Self { current: Some(SceneId::FirstGame), doing: true, next_init: None, frame: 0 }
    }

    pub fn current(&self) -> Option<SceneId> {
        self.current
    }

    pub fn is_doing(&self) -> bool {
        self.doing
    }

    pub fn frame(&self) -> u64 {
        self.frame
    }

    /// Run one frame of the current scene. Mirrors the
    /// `while (game_is_doing(game))` body of `graph_proc`.
    pub fn run_frame<S: Scene>(&mut self, scene: &mut S) -> SceneRequest {
        if !self.doing {
            return SceneRequest::Continue;
        }
        self.frame += 1;
        match scene.exec() {
            SceneRequest::Continue => SceneRequest::Continue,
            SceneRequest::Goto(next) => {
                // GAME_GOTO_NEXT: doing = FALSE, record next init.
                self.doing = false;
                self.next_init = Some(next);
                SceneRequest::Goto(next)
            }
            SceneRequest::Shutdown => {
                self.doing = false;
                self.next_init = None;
                SceneRequest::Shutdown
            }
        }
    }

    /// Resolve the pending transition, mirroring
    /// `game_get_next_game_dlftbl`: match the next-init pointer against
    /// the table. Returns the next scene, or `None` to shut down.
    pub fn advance(&mut self) -> Option<SceneId> {
        let next = self.next_init?;
        // Table lookup by init identity, as in the C dispatcher.
        let found = SCENE_TABLE.iter().find(|e| e.id == next).map(|e| e.id);
        self.current = found;
        self.next_init = None;
        self.doing = found.is_some();
        self.frame = 0;
        found
    }
}

impl Default for SceneManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Boot chain stages. The `Pc*` stages are the PC-port wrapper
/// (`pc_main.c`); the rest follows the original game's structure
/// (`ac_entry` → `boot_main` → `entry` → `mainproc` → `graph_proc`).
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BootStage {
    PcMain,
    PcSettingsLoad,
    PcPlatformInit,
    PcDiscInit,
    PcAssetsInit,
    PcTexturePackInit,
    AcEntry,
    BootMain,
    Entry,
    MainProc,
    GraphProc,
}

/// The boot chain in order. The PC-port stages are marked; the original
/// game owns everything from `AcEntry` on.
pub static BOOT_CHAIN: [(BootStage, bool); 11] = [
    (BootStage::PcMain, true),
    (BootStage::PcSettingsLoad, true),
    (BootStage::PcPlatformInit, true),
    (BootStage::PcDiscInit, true),
    (BootStage::PcAssetsInit, true),
    (BootStage::PcTexturePackInit, true),
    (BootStage::AcEntry, false),
    (BootStage::BootMain, false),
    (BootStage::Entry, false),
    (BootStage::MainProc, false),
    (BootStage::GraphProc, false),
];

/// C ABI: table index for a scene id, or -1 for the removed slot/unknown.
/// Mirrors `game_get_next_game_dlftbl`'s index mapping.
#[no_mangle]
pub extern "C" fn pc_scene_table_index(scene_id: u8) -> i32 {
    match SceneId::from_table_index(scene_id as usize) {
        Some(id) => id.table_index() as i32,
        None => -1,
    }
}

/// C ABI: number of `game_dlftbls` entries.
#[no_mangle]
pub extern "C" fn pc_game_dlftbls_count() -> u32 {
    GAME_DLFTBLS_COUNT as u32
}

/// Table index of the removed/NULL entry (`DLFTBL_NULL()`, "removed &
/// unused _GAME entry"). The dispatcher never maps an init pointer here.
pub const SCENE_TABLE_NULL_INDEX: usize = 4;

/// Verified per-scene exec/cleanup names. `first_game` is special: its
/// init does ROM/save setup and immediately requests `second_game`
/// without ever installing an exec (`first_game.c`).
pub struct SceneExec {
    pub id: SceneId,
    pub exec_fn: Option<&'static str>,
    pub cleanup_fn: Option<&'static str>,
}

pub static SCENE_EXEC_TABLE: [SceneExec; 10] = [
    SceneExec { id: SceneId::FirstGame, exec_fn: None, cleanup_fn: Some("first_game_cleanup") },
    SceneExec { id: SceneId::Select, exec_fn: Some("select_main"), cleanup_fn: None },
    SceneExec { id: SceneId::Play, exec_fn: Some("play_main"), cleanup_fn: Some("play_cleanup") },
    SceneExec { id: SceneId::SecondGame, exec_fn: Some("second_game_main"), cleanup_fn: None },
    SceneExec { id: SceneId::Trademark, exec_fn: Some("trademark_main"), cleanup_fn: None },
    SceneExec { id: SceneId::PlayerSelect, exec_fn: Some("player_select_main"), cleanup_fn: None },
    SceneExec { id: SceneId::SaveMenu, exec_fn: Some("save_menu_main"), cleanup_fn: None },
    SceneExec { id: SceneId::FamicomEmu, exec_fn: Some("famicom_emu_main"), cleanup_fn: None },
    SceneExec { id: SceneId::Prenmi, exec_fn: Some("prenmi_main"), cleanup_fn: None },
    SceneExec { id: SceneId::ModelViewer, exec_fn: None, cleanup_fn: None },
];

/// The generic per-frame wrapper every scene runs inside, mirroring
/// `game_main()` in `game.c`: draw setup, clock update, the scene exec,
/// background music, first-move bookkeeping, then the frame counter.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GameMainPhase {
    DrawFirst = 0,
    Time = 1,
    SceneExec = 2,
    Bgm = 3,
    MoveFirst = 4,
    FrameCounter = 5,
}

pub const GAME_MAIN_PHASES: [GameMainPhase; 6] = [
    GameMainPhase::DrawFirst,
    GameMainPhase::Time,
    GameMainPhase::SceneExec,
    GameMainPhase::Bgm,
    GameMainPhase::MoveFirst,
    GameMainPhase::FrameCounter,
];

/// The two halves of `play_main()`: simulation, then rendering.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlayExecPhase {
    Move = 0,
    Draw = 1,
}

/// Visual transitions are a separate mechanism from scene transitions
/// (`Game_play_fbdemo_wipe_*` in `m_play.c`). A wipe/fade can hide a
/// scene change, but the scene lifetime is controlled by the GAME state
/// machine, not the wipe.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VisualTransition {
    None = 0,
    Wipe = 1,
    Fade = 2,
}

/// Level-2 "scene" data: records processed by `Scene_ct()` inside
/// `play_init()` via `Gameplay_Scene_Read()`. This is a data-driven
/// world/room initializer (player, actors, doors, field, rooms,
/// furniture, sound) — NOT the top-level `game_dlftbls` scene system.
/// Mirrors `mSc_SCENE_DATA_TYPE_*`.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SceneDataKind {
    Player = 0,
    CtrlActor = 1,
    Actor = 2,
    ObjectExchangeBank = 3,
    DoorData = 4,
    FieldCt = 5,
    MyRoomCt = 6,
    ArrangeRoomCt = 7,
    ArrangeFurnitureCt = 8,
    Sound = 9,
}

impl SceneManager {
    /// Request the gameplay scene (`game_goto_next_game_play`).
    pub fn goto_play(&mut self) {
        self.doing = false;
        self.next_init = Some(SceneId::Play);
    }

    /// Request the NES/Famicom scene
    /// (`game_goto_next_game_famicom_emu`).
    pub fn goto_famicom_emu(&mut self) {
        self.doing = false;
        self.next_init = Some(SceneId::FamicomEmu);
    }

    /// System reset path: `graph_main`'s reset check requests the
    /// Pre-NMI scene through the same transition mechanism whenever the
    /// reset status is `IRQ_RESET_PRENMI` and the scene has not disabled
    /// it (`disable_prenmi`).
    pub fn goto_prenmi(&mut self) {
        self.doing = false;
        self.next_init = Some(SceneId::Prenmi);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct ScriptedScene {
        frames_before_goto: u64,
        goto: SceneId,
        frames: u64,
        cleaned: bool,
    }

    impl Scene for ScriptedScene {
        fn init(&mut self) {
            self.frames = 0;
        }
        fn exec(&mut self) -> SceneRequest {
            self.frames += 1;
            if self.frames >= self.frames_before_goto {
                SceneRequest::Goto(self.goto)
            } else {
                SceneRequest::Continue
            }
        }
        fn cleanup(&mut self) {
            self.cleaned = true;
        }
    }

    #[test]
    fn table_indices_match_decomp() {
        assert_eq!(SceneId::FirstGame.table_index(), 0);
        assert_eq!(SceneId::Select.table_index(), 1);
        assert_eq!(SceneId::Play.table_index(), 2);
        assert_eq!(SceneId::SecondGame.table_index(), 3);
        assert_eq!(SceneId::Trademark.table_index(), 5);
        assert_eq!(SceneId::PlayerSelect.table_index(), 6);
        assert_eq!(SceneId::SaveMenu.table_index(), 7);
        assert_eq!(SceneId::FamicomEmu.table_index(), 8);
        assert_eq!(SceneId::Prenmi.table_index(), 9);
        assert_eq!(SceneId::ModelViewer.table_index(), 10);
        // Index 4 is the removed/NULL entry.
        assert_eq!(SceneId::from_table_index(4), None);
        assert_eq!(GAME_DLFTBLS_COUNT, 11);
    }

    #[test]
    fn boot_starts_at_first_game() {
        let mgr = SceneManager::new();
        assert_eq!(mgr.current(), Some(SceneId::FirstGame));
        assert!(mgr.is_doing());
    }

    #[test]
    fn goto_next_resolves_through_table() {
        let mut mgr = SceneManager::new();
        let mut scene = ScriptedScene { frames_before_goto: 3, goto: SceneId::SecondGame, frames: 0, cleaned: false };
        scene.init();
        // first_game -> second_game, as in the decomp.
        assert_eq!(mgr.run_frame(&mut scene), SceneRequest::Continue);
        assert_eq!(mgr.run_frame(&mut scene), SceneRequest::Continue);
        assert_eq!(mgr.run_frame(&mut scene), SceneRequest::Goto(SceneId::SecondGame));
        assert!(!mgr.is_doing());
        scene.cleanup();
        assert!(scene.cleaned);
        assert_eq!(mgr.advance(), Some(SceneId::SecondGame));
        assert!(mgr.is_doing());
        assert_eq!(mgr.frame(), 0);
    }

    #[test]
    fn shutdown_ends_dispatcher() {
        let mut mgr = SceneManager::new();
        struct Quit;
        impl Scene for Quit {
            fn init(&mut self) {}
            fn exec(&mut self) -> SceneRequest {
                SceneRequest::Shutdown
            }
            fn cleanup(&mut self) {}
        }
        let mut q = Quit;
        assert_eq!(mgr.run_frame(&mut q), SceneRequest::Shutdown);
        assert_eq!(mgr.advance(), None);
    }

    #[test]
    fn boot_chain_marks_pc_stages() {
        let pc_stages = BOOT_CHAIN.iter().filter(|(_, pc)| *pc).count();
        assert_eq!(pc_stages, 6);
        assert_eq!(BOOT_CHAIN.first().unwrap().0, BootStage::PcMain);
        assert_eq!(BOOT_CHAIN.last().unwrap().0, BootStage::GraphProc);
    }

    #[test]
    fn per_scene_exec_names_match_source() {
        let play = SCENE_EXEC_TABLE.iter().find(|e| e.id == SceneId::Play).unwrap();
        assert_eq!(play.exec_fn, Some("play_main"));
        assert_eq!(play.cleanup_fn, Some("play_cleanup"));
        // first_game never installs an exec: init transitions directly.
        let first = SCENE_EXEC_TABLE.iter().find(|e| e.id == SceneId::FirstGame).unwrap();
        assert_eq!(first.exec_fn, None);
        // The NULL slot has no entry.
        assert_eq!(SCENE_TABLE_NULL_INDEX, 4);
        assert_eq!(SceneId::from_table_index(SCENE_TABLE_NULL_INDEX), None);
    }

    #[test]
    fn game_main_wraps_scene_exec() {
        assert_eq!(GAME_MAIN_PHASES.len(), 6);
        assert_eq!(GAME_MAIN_PHASES[2], GameMainPhase::SceneExec);
        assert_eq!(GAME_MAIN_PHASES[5], GameMainPhase::FrameCounter);
    }

    #[test]
    fn scene_data_kinds_cover_world_init() {
        assert_eq!(SceneDataKind::Player as u8, 0);
        assert_eq!(SceneDataKind::Sound as u8, 9);
    }

    #[test]
    fn transition_helpers_request_scenes() {
        let mut mgr = SceneManager::new();
        mgr.goto_play();
        assert_eq!(mgr.advance(), Some(SceneId::Play));
        let mut mgr2 = SceneManager::new();
        mgr2.goto_famicom_emu();
        assert_eq!(mgr2.advance(), Some(SceneId::FamicomEmu));
        let mut mgr3 = SceneManager::new();
        mgr3.goto_prenmi();
        assert_eq!(mgr3.advance(), Some(SceneId::Prenmi));
    }
}
