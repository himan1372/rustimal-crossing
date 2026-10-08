//! UKI (fishing float) actor: the coordinator between the player, the
//! float's process machine, and the fish actor.
//!
//! Ports `include/ac_uki.h`, `src/actor/ac_uki.c`, and
//! `src/actor/ac_uki_move.c_inc` (GAFE01_00 Rev. 0).
//!
//! Retail does NOT implement fishing as one state machine. UKI keeps four
//! independent pieces of state:
//! - `proc`: the float's own process (physics/visual trajectory),
//! - `status`: the player-visible status the rod animation syncs to,
//! - `gyo_command` / `gyo_status`: the fish-side handshake channel,
//! - `child_actor`: the linked fish actor.
//! The player only writes `command`; the fish only writes `gyo_command`
//! (plus `gyo_type` / `child_actor`). Collapsing these into one
//! `FishingState` enum loses retail behavior.
//!
//! Engine-dependent pieces (background collision queries, effects, sound,
//! vibration, the fish actor itself) are modeled as inputs (`StepEnv`,
//! `WaterEnv`) and outputs (`UkiEvent`) so the logic stays faithful and
//! testable without the engine.

use core::f32::consts::PI;

/// `sizeof(UKI_ACTOR) == 0x2C8` (`include/ac_uki.h`).
pub const UKI_ACTOR_SIZE: usize = 0x2C8;

/// Actor physics init (`aUKI_actor_ct`).
pub const MAX_VELOCITY_Y: f32 = -20.0;
pub const GRAVITY: f32 = 1.2;
pub const INIT_SCALE: f32 = 0.01;

/// UKI process ids (`aUKI_PROC_*`, `src/actor/ac_uki.c`).
pub mod proc {
    pub const CARRY: u8 = 0;
    pub const READY: u8 = 1;
    pub const AIR: u8 = 2;
    pub const CAST: u8 = 3;
    pub const WAIT: u8 = 4;
    pub const HIT: u8 = 5;
    pub const TOUCH: u8 = 6;
    pub const BITE: u8 = 7;
    pub const CATCH: u8 = 8;
    pub const GET: u8 = 9;
    pub const FORCE: u8 = 10;
    pub const NUM: u8 = 11;
}

/// Player-visible UKI statuses (`aUKI_STATUS_*`, `include/ac_uki.h`).
/// Independent from `proc`.
pub mod status {
    pub const STATUS_0: u8 = 0;
    pub const CARRY: u8 = 1;
    pub const READY: u8 = 2;
    pub const CAST: u8 = 3;
    pub const FLOAT: u8 = 4;
    pub const VIB: u8 = 5;
    pub const COMEBACK: u8 = 6;
    pub const CATCH: u8 = 7;
    pub const NUM: u8 = 8;
}

/// Player command values written to `uki->command` (`aUKI_set_value`).
/// Retail has no formal enum; the numbers are source-proven at the call
/// sites and the names are reconstruction.
pub mod command {
    pub const NONE: i32 = 0; // suspends the proc machine
    pub const CARRY: i32 = 1; // normal carry / return-to-carry
    pub const READY: i32 = 2; // prepare to cast
    pub const CAST: i32 = 3; // commit the cast
    pub const AIR: i32 = 4; // retract without casting
    pub const GET: i32 = 5; // put away / take the caught fish
    pub const REEL: i32 = 6; // reel / collect / vibration
    pub const FORCE_COMEBACK: i32 = 7; // forced comeback trajectory
    pub const FORCE: i32 = 8; // force / event cleanup
}

/// Fish-to-UKI command channel (`gyo_command`). Values 1 and 2 are written
/// by the fish actor; the rest is player/event control.
pub mod gyo_command {
    pub const NONE: i32 = 0;
    pub const ENGAGED: i32 = 1; // fish reached the float
    pub const BITTEN: i32 = 2; // fish bit
}

/// Fish-side status (`gyo_status`). Retail uses raw integers; the names are
/// reconstruction, the values and transitions are source-proven.
pub mod gyo_status {
    pub const IDLE: i32 = 0;
    pub const AVAILABLE: i32 = 1; // WAIT: fish may engage
    pub const TOUCH: i32 = 2; // fish touching / approaching
    pub const BITE: i32 = 3; // bite before the active reel
    pub const HOOKED: i32 = 4; // actively hooked / reeling (set by UKI)
    pub const CAUGHT_RETURNING: i32 = 5; // successful catch, flying back
    pub const CAUGHT: i32 = 6; // at the player, CATCH begins
    pub const GET: i32 = 7; // GET / held-fish state
    pub const FINALIZE: i32 = 8; // finalize, then back to carry
}

/// Fish sizes (`aGYO_SIZE_*`, `include/ac_gyoei.h`).
pub mod fish_size {
    pub const XXS: u8 = 0;
    pub const XS: u8 = 1;
    pub const S: u8 = 2;
    pub const M: u8 = 3;
    pub const L: u8 = 4;
    pub const XL: u8 = 5;
    pub const XXL: u8 = 6;
    pub const WHALE: u8 = 7;
}

/// Rod types (`aGYO_ROD_*`, `include/ac_gyoei.h`).
pub mod rod {
    pub const NORMAL: u8 = 0;
    pub const GOLDEN: u8 = 1;
}

/// 3D vector.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Xyz {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Xyz {
    pub const fn new(x: f32, y: f32, z: f32) -> Xyz {
        Xyz { x, y, z }
    }
}

/// Per-fish data (`aGYO_type_c`, `src/actor/ac_gyoei_type.c_inc`):
/// size, search area, bite-time category. 45 entries in `aGYO_TYPE_*` order.
#[derive(Clone, Copy, Debug)]
pub struct GyoeiType {
    pub size: u8,
    pub search_area: u8,
    pub bite_time: u8,
}

pub const GYOEI_TYPES: [GyoeiType; 45] = [
    GyoeiType { size: 1, search_area: 3, bite_time: 4 }, // CRUCIAN_CARP
    GyoeiType { size: 2, search_area: 3, bite_time: 2 }, // BROOK_TROUT
    GyoeiType { size: 4, search_area: 2, bite_time: 3 }, // CARP
    GyoeiType { size: 4, search_area: 2, bite_time: 3 }, // KOI
    GyoeiType { size: 4, search_area: 3, bite_time: 4 }, // CATFISH
    GyoeiType { size: 1, search_area: 3, bite_time: 3 }, // SMALL_BASS
    GyoeiType { size: 3, search_area: 2, bite_time: 2 }, // BASS
    GyoeiType { size: 4, search_area: 1, bite_time: 1 }, // LARGE_BASS
    GyoeiType { size: 1, search_area: 4, bite_time: 4 }, // BLUEGILL
    GyoeiType { size: 5, search_area: 2, bite_time: 4 }, // GIANT_CATFISH
    GyoeiType { size: 5, search_area: 2, bite_time: 1 }, // GIANT_SNAKEHEAD
    GyoeiType { size: 4, search_area: 2, bite_time: 3 }, // BARBEL_STEED
    GyoeiType { size: 3, search_area: 3, bite_time: 3 }, // DACE
    GyoeiType { size: 1, search_area: 3, bite_time: 3 }, // PALE_CHUB
    GyoeiType { size: 0, search_area: 2, bite_time: 1 }, // BITTERLING
    GyoeiType { size: 0, search_area: 3, bite_time: 3 }, // LOACH
    GyoeiType { size: 0, search_area: 3, bite_time: 3 }, // POND_SMELT
    GyoeiType { size: 1, search_area: 2, bite_time: 1 }, // SWEETFISH
    GyoeiType { size: 1, search_area: 1, bite_time: 1 }, // CHERRY_SALMON
    GyoeiType { size: 4, search_area: 1, bite_time: 1 }, // LARGE_CHAR
    GyoeiType { size: 3, search_area: 2, bite_time: 2 }, // RAINBOW_TROUT
    GyoeiType { size: 5, search_area: 1, bite_time: 1 }, // STRINGFISH
    GyoeiType { size: 4, search_area: 2, bite_time: 1 }, // SALMON
    GyoeiType { size: 0, search_area: 2, bite_time: 3 }, // GOLDFISH
    GyoeiType { size: 1, search_area: 4, bite_time: 3 }, // PIRANHA
    GyoeiType { size: 3, search_area: 3, bite_time: 2 }, // AROWANA
    GyoeiType { size: 2, search_area: 1, bite_time: 1 }, // EEL
    GyoeiType { size: 1, search_area: 2, bite_time: 4 }, // FRESHWATER_GOBY
    GyoeiType { size: 1, search_area: 2, bite_time: 2 }, // ANGELFISH
    GyoeiType { size: 0, search_area: 2, bite_time: 3 }, // GUPPY
    GyoeiType { size: 0, search_area: 2, bite_time: 3 }, // POPEYED_GOLDFISH
    GyoeiType { size: 5, search_area: 2, bite_time: 0 }, // COELACANTH
    GyoeiType { size: 1, search_area: 4, bite_time: 4 }, // CRAWFISH
    GyoeiType { size: 0, search_area: 4, bite_time: 3 }, // FROG
    GyoeiType { size: 0, search_area: 2, bite_time: 2 }, // KILLIFISH
    GyoeiType { size: 3, search_area: 2, bite_time: 4 }, // JELLYFISH
    GyoeiType { size: 4, search_area: 3, bite_time: 2 }, // SEA_BASS
    GyoeiType { size: 4, search_area: 2, bite_time: 1 }, // RED_SNAPPER
    GyoeiType { size: 4, search_area: 2, bite_time: 0 }, // BARRED_KNIFEJAW
    GyoeiType { size: 6, search_area: 1, bite_time: 1 }, // ARAPAIMA
    GyoeiType { size: 7, search_area: 1, bite_time: 1 }, // WHALE
    GyoeiType { size: 0, search_area: 1, bite_time: 3 }, // EMPTY_CAN
    GyoeiType { size: 3, search_area: 2, bite_time: 4 }, // BOOT
    GyoeiType { size: 4, search_area: 2, bite_time: 4 }, // OLD_TIRE
    GyoeiType { size: 4, search_area: 2, bite_time: 1 }, // SALMON2
];

/// UKI reel timer by fish size (`aUKI_set_proc_bite`: 26/39/39/39/52/65/78/78).
pub const REEL_TIMER: [i16; 8] = [26, 39, 39, 39, 52, 65, 78, 78];

/// Fish search angles by rod and search area
/// (`ac_gyo_kaseki.c`: normal 3/7/30/50/180, golden 7.5/15/40/60/180).
pub const SEARCH_ANGLE: [[f32; 5]; 2] = [
    [3.0, 7.0, 30.0, 50.0, 180.0],
    [7.5, 15.0, 40.0, 60.0, 180.0],
];

/// Fish search distances by search area (both rods).
pub const SEARCH_DIST: [f32; 5] = [40.0, 40.0, 40.0, 50.0, 60.0];

/// Fish-side bite persistence by rod and bite-time category, before the x2.
pub const BITE_TIME: [[f32; 5]; 2] = [
    [10.0, 11.0, 12.0, 15.0, 45.0],
    [11.0, 12.0, 13.0, 18.0, 60.0],
];

/// Fish touch radii by size.
pub const TOUCH_DISTANCE: [f32; 8] = [12.0, 13.0, 15.0, 15.0, 20.0, 25.0, 30.0, 30.0];

/// Fish touch counters by size.
pub const TOUCH_COUNT: [i16; 8] = [19, 19, 19, 19, 20, 22, 24, 24];

/// Fish retreat speeds by size.
pub const BACK_SPEED: [f32; 8] = [-0.38, -0.40, -0.42, -0.42, -0.45, -0.50, -0.70, -0.70];

/// Trash substitution by size (`gomi[]`): XXS/XS -> Empty Can (41),
/// S/M/L -> Boot (42), XL/XXL/WHALE -> Old Tire (43).
pub const TRASH_BY_SIZE: [u8; 8] = [41, 41, 42, 42, 42, 43, 43, 43];

/// BITE angular wobble (degrees) and drift speed by fish size.
pub const BITE_ANGLE_DEG: [f32; 8] = [10.1513671875, 9.4647216796875, 8.778076171875, 8.778076171875,
    8.0914306640625, 7.40478515625, 7.03125, 7.03125];
pub const BITE_SPEED: [f32; 8] = [0.1, 0.2, 0.3, 0.3, 0.4, 0.6, 0.8, 0.8];

/// Fish type -> inventory item (`aUKI_get_fish_type` table): 0..39 map to
/// themselves, 40 -> 39 (duplicate), 41 -> Empty Can, 42 -> Boot,
/// 43 -> Old Tire, 44 -> 22 (FISH22). Returns the item index.
pub fn fish_item_index(gyo_type: u8) -> Option<u8> {
    match gyo_type {
        0..=39 => Some(gyo_type),
        40 => Some(39),
        41 => Some(41),
        42 => Some(42),
        43 => Some(43),
        44 => Some(22),
        _ => None,
    }
}

/// Trash types are gyo types 41..=43 (`aGYO_IS_FISH_TRASH`).
pub fn is_trash_gyo_type(gyo_type: u8) -> bool {
    (41..=43).contains(&gyo_type)
}

/// UKI reel timer for a bite: trash always 26, otherwise
/// `REEL_TIMER[size] * 2`.
pub fn reel_timer_for(gyo_type: u8) -> i16 {
    if is_trash_gyo_type(gyo_type) {
        return 26;
    }
    let size = GYOEI_TYPES.get(gyo_type as usize).map(|t| t.size).unwrap_or(3);
    REEL_TIMER[size as usize] * 2
}

/// Fish-side bite persistence in frames: `aGYO_bite_time[rod][category] * 2`.
pub fn fish_bite_frames(golden_rod: bool, bite_time_category: u8) -> i32 {
    let rod = if golden_rod { 1 } else { 0 };
    let cat = (bite_time_category as usize).min(4);
    (BITE_TIME[rod][cat] * 2.0) as i32
}

/// Trash substitution on bite commit (`gomi[size_type]`).
pub fn trash_substitute(size: u8) -> u8 {
    TRASH_BY_SIZE[(size as usize).min(7)]
}

/// Observable side effects the engine must perform.
#[derive(Clone, Debug, PartialEq)]
pub enum UkiEvent {
    /// Splash effect (`aUKI_effect_sibuki`).
    Splash { arg: i16 },
    /// Simple vibration entry (`mVibctl_simple_entry(50, ...)`).
    Vibration,
    /// Sound trigger by retail name.
    Sound(&'static str),
    /// Ripple ring effect (`aUKI_effect_hamon`).
    Ripple { arg: i16 },
    /// Touch vibration proc.
    TouchVib,
    /// Bite vibration proc.
    BiteVib,
}

/// Per-frame inputs the engine supplies to `step`.
#[derive(Clone, Copy, Debug)]
pub struct StepEnv {
    /// Background collision: the bobber is in water (CAST -> WAIT gate).
    pub in_water: bool,
    /// Water flow vector (x, z) for drift.
    pub flow: (f32, f32),
    /// Horizontal distance to the player.
    pub dist_to_player: f32,
    /// Player facing in radians (used by READY's parabola and drift-home).
    pub player_facing: f32,
    /// The bobber sits on WAVE/SAND (coast behavior).
    pub on_coast: bool,
    /// Waterfall terrain under the bobber.
    pub on_waterfall: bool,
    /// Current water surface height.
    pub water_height: f32,
}

impl Default for StepEnv {
    fn default() -> Self {
        StepEnv {
            in_water: false,
            flow: (0.0, 0.0),
            dist_to_player: 0.0,
            player_facing: 0.0,
            on_coast: false,
            on_waterfall: false,
            water_height: 0.0,
        }
    }
}

/// The UKI actor state (retail fields, engine-independent subset).
#[derive(Clone, Debug)]
pub struct UkiActor {
    pub proc: u8,
    pub status: u8,
    pub command: i32,
    pub gyo_command: i32,
    pub gyo_status: i32,
    pub gyo_type: i32,
    pub gyo_scale: f32,
    /// Linked fish actor id (`child_actor`); None == NULL.
    pub child_actor: Option<u32>,
    pub frame_timer: i16,
    pub cast_timer: i16,
    pub touch_timer: i16,
    pub parabola_vec: [Xyz; 2],
    pub parabola_acc: [Xyz; 2],
    /// world.position.
    pub pos: Xyz,
    /// uki_pos: the fish's logical position (== pos except during HIT).
    pub uki_pos: Xyz,
    /// gyo_pos: written by the fish actor each frame.
    pub gyo_pos: Xyz,
    pub right_hand_pos: Xyz,
    pub left_hand_pos: Xyz,
    pub cast_goal_point: Xyz,
    pub rod_top_position: Xyz,
    pub position_speed: Xyz,
    pub angle_speed_y: i32,
    /// Actor movement speed (from the ACTOR base).
    pub speed: f32,
    pub max_velocity_y: f32,
    pub gravity: f32,
    /// world.angle.y in radians.
    pub angle_y: f32,
    pub touched_flag: bool,
    pub hit_water_flag: bool,
    pub coast_flag: bool,
    pub color: [i32; 3],
    /// Vibration / ripple accumulators (`hamon_accum`, `touch_vib_accum`).
    pub hamon_accum: f32,
    pub touch_vib_accum: f32,
    /// Bite touch-vibration frame counter (models the 10-frame period).
    pub touch_vib_frames: u8,
}

impl Default for UkiActor {
    /// `aUKI_actor_ct` initial state.
    fn default() -> Self {
        UkiActor {
            proc: proc::CARRY,
            status: status::CARRY,
            command: command::NONE,
            gyo_command: gyo_command::NONE,
            gyo_status: gyo_status::IDLE,
            gyo_type: -1,
            gyo_scale: 1.0,
            child_actor: None,
            frame_timer: 2,
            cast_timer: 0,
            touch_timer: 0,
            parabola_vec: [Xyz::default(); 2],
            parabola_acc: [Xyz::default(); 2],
            pos: Xyz::default(),
            uki_pos: Xyz::default(),
            gyo_pos: Xyz::default(),
            right_hand_pos: Xyz::default(),
            left_hand_pos: Xyz::default(),
            cast_goal_point: Xyz::default(),
            rod_top_position: Xyz::default(),
            position_speed: Xyz::default(),
            angle_speed_y: 0,
            speed: 0.0,
            max_velocity_y: MAX_VELOCITY_Y,
            gravity: GRAVITY,
            angle_y: 0.0,
            touched_flag: false,
            hit_water_flag: false,
            coast_flag: false,
            color: [255, 255, 255],
            hamon_accum: 0.0,
            touch_vib_accum: 0.0,
            touch_vib_frames: 0,
        }
    }
}

/// `aUKI_timer_step`: decrement if nonzero, return the value.
pub fn timer_step(timer: &mut i16) -> i16 {
    if *timer != 0 {
        *timer -= 1;
    }
    *timer
}

fn chase_f(current: &mut f32, target: f32, step: f32) {
    if *current < target {
        *current = (*current + step).min(target);
    } else if *current > target {
        *current = (*current - step).max(target);
    }
}

/// Parabola setup (`aUKI_parabola_init`), verbatim math.
/// type 0: linear vec in `vec[0]`.
/// type 1: accelerated vec in `vec`, landing on world.position.
/// type 2: accelerated acc in `acc`, landing on uki_pos.
pub fn parabola_init(p0: &Xyz, p1: &Xyz, timer: i16, ptype: u8) -> ([Xyz; 2], [Xyz; 2]) {
    let mut vec = [Xyz::default(); 2];
    let mut acc = [Xyz::default(); 2];
    let f = timer as f32;
    if f == 0.0 {
        return (vec, acc);
    }
    let step = f * 0.5;
    let dx = p1.x - p0.x;
    let dy = p1.y - p0.y;
    let dz = p1.z - p0.z;
    let y_param = if dy > 100.0 { 12.0 } else { 4.5 };
    match ptype {
        0 => {
            vec[0] = Xyz::new(dx / f, dy / f, dz / f);
        }
        1 => {
            vec[1].y = (2.0 * (y_param * f - dy)) / (f * f);
            vec[0].x = dx / step;
            vec[0].y = y_param - vec[1].y;
            vec[0].z = dz / step;
            vec[1].x = vec[0].x / f;
            vec[1].z = vec[0].z / f;
        }
        2 => {
            acc[1].y = (2.0 * (y_param * f - dy)) / (f * f);
            acc[0].x = dx / step;
            acc[0].y = y_param - acc[1].y;
            acc[0].z = dz / step;
            acc[1].x = acc[0].x / f;
            acc[1].z = acc[0].z / f;
        }
        _ => {}
    }
    (vec, acc)
}

/// Advance one frame: `position += vec; vec -= acc`.
fn parabola_advance(pos: &mut Xyz, v: &mut Xyz, a: &Xyz) {
    pos.x += v.x;
    pos.y += v.y;
    pos.z += v.z;
    v.x -= a.x;
    v.y -= a.y;
    v.z -= a.z;
}

impl UkiActor {
    /// `aUKI_set_value`: the player writes hand data + command every frame.
    pub fn set_value(&mut self, pos: Xyz, pos_speed: Xyz, angle_speed_y: i32, command: i32) {
        self.right_hand_pos = pos;
        self.position_speed = pos_speed;
        self.angle_speed_y = angle_speed_y;
        self.command = command;
    }

    /// The machine only runs while `command != 0` (`aUKI_actor_move`).
    pub fn is_active(&self) -> bool {
        self.command != command::NONE
    }

    /// Per-frame hand offset applied by `aUKI_actor_move` when active:
    /// right_hand += rotated(+3, -7), y += 6.
    pub fn apply_hand_offset(&mut self, player_facing: f32) {
        let s = player_facing.sin();
        let c = player_facing.cos();
        self.right_hand_pos.x += 3.0 * s + -7.0 * c;
        self.right_hand_pos.y += 6.0;
        self.right_hand_pos.z += 3.0 * c - -7.0 * s;
    }

    fn clear_spd(&mut self) {
        self.position_speed = Xyz::default();
        self.speed = 0.0;
    }

    /// Process initializer dispatch (`aUKI_set_proc`).
    pub fn set_proc(&mut self, proc: u8, arg: i16, events: &mut Vec<UkiEvent>) {
        match proc {
            proc::CARRY => {
                self.position_speed.y = 0.0;
                // ct() sets 2 at construction; set_proc_carry sets 4 thereafter.
                self.frame_timer = 4;
                self.status = status::CARRY;
            }
            proc::READY => {
                // 20-frame linear parabola to 30 units behind the player.
                self.frame_timer = 32;
                let mut target = self.pos;
                target.x -= 30.0 * self.angle_y.sin();
                target.z -= 30.0 * self.angle_y.cos();
                let (vec, _) = parabola_init(&self.pos, &target, 20, 0);
                self.parabola_vec = vec;
                self.status = status::READY;
            }
            proc::AIR => {
                self.frame_timer = 14;
                let (vec, _) = parabola_init(&self.pos, &self.uki_pos, self.frame_timer, 0);
                self.parabola_vec = vec;
                self.status = status::CARRY;
            }
            proc::CAST => {
                self.frame_timer = 50;
                self.cast_timer = 40;
                let (vec, _) = parabola_init(&self.pos, &self.cast_goal_point, self.frame_timer, 1);
                self.parabola_vec = vec;
                self.status = status::CAST;
            }
            proc::WAIT => {
                self.frame_timer = 12;
                self.gyo_status = gyo_status::AVAILABLE;
                self.gyo_type = -1;
                self.child_actor = None;
            }
            proc::HIT => {
                self.frame_timer = 52;
                let (vec, _) =
                    parabola_init(&self.pos, &self.right_hand_pos, self.frame_timer, 1);
                self.parabola_vec = vec;
                self.status = status::COMEBACK;
                if self.coast_flag {
                    self.coast_flag = false;
                } else {
                    events.push(UkiEvent::Splash { arg });
                }
                events.push(UkiEvent::Sound("NA_SE_10C"));
            }
            proc::TOUCH => {
                self.clear_spd();
                self.frame_timer = 12;
                self.gyo_status = gyo_status::TOUCH;
            }
            proc::BITE => {
                self.clear_spd();
                events.push(UkiEvent::Splash { arg: 0 });
                self.frame_timer = reel_timer_for(self.gyo_type.max(0) as u8);
                self.gyo_status = gyo_status::BITE;
            }
            proc::CATCH => {
                self.frame_timer = 20;
                self.gyo_status = gyo_status::CAUGHT;
                self.status = status::CATCH;
            }
            proc::GET => {
                self.gyo_status = gyo_status::GET;
            }
            _ => {}
        }
        self.proc = proc;
    }

    /// Force-command check (`aUKI_force_command`): command 8 -> FORCE,
    /// command 7 -> forced COMEBACK trajectory. Returns true if taken.
    fn force_command(&mut self, events: &mut Vec<UkiEvent>) -> bool {
        match self.command {
            command::FORCE => {
                self.set_proc(proc::FORCE, 0, events);
                true
            }
            command::FORCE_COMEBACK => {
                self.clear_spd();
                self.frame_timer = 52;
                let (vec, _) =
                    parabola_init(&self.pos, &self.right_hand_pos, self.frame_timer, 1);
                self.parabola_vec = vec;
                self.status = status::COMEBACK;
                events.push(UkiEvent::Splash { arg: 1 });
                events.push(UkiEvent::Sound("NA_SE_10C"));
                self.set_proc(proc::FORCE, 0, events);
                true
            }
            _ => false,
        }
    }

    /// Water drift (`aUKI_movement`): flow-angle drift near the player,
    /// homing drift beyond 130 units, ripple past 127.
    fn water_drift(&mut self, env: &StepEnv, events: &mut Vec<UkiEvent>) {
        let flow_angle = env.flow.1.atan2(env.flow.0);
        if env.dist_to_player < 130.0 || env.on_waterfall {
            if self.gyo_command != gyo_command::ENGAGED {
                if self.touch_timer != 5 {
                    self.touch_timer = 0;
                }
                chase_f(&mut self.speed, 0.45, 0.1);
            } else {
                chase_f(&mut self.speed, 0.225, 0.1);
            }
            self.position_speed.x = self.speed * flow_angle.sin();
            self.position_speed.z = self.speed * flow_angle.cos();
        } else {
            if self.gyo_command != gyo_command::ENGAGED && self.touch_timer != 5 {
                self.touch_timer = 0;
            }
            self.speed = 0.8;
            self.position_speed.x = self.speed * env.player_facing.sin();
            self.position_speed.z = self.speed * env.player_facing.cos();
        }
        if env.dist_to_player > 127.0 {
            self.hamon_accum += 1.0;
            if self.hamon_accum >= 10.0 {
                self.hamon_accum = 0.0;
                events.push(UkiEvent::Ripple { arg: 4 });
            }
        }
        self.pos.x += self.position_speed.x;
        self.pos.z += self.position_speed.z;
    }

    /// Coast/wave handling (`aUKI_coast_wave`).
    fn coast_wave(&mut self, env: &StepEnv, events: &mut Vec<UkiEvent>) {
        if env.on_coast {
            self.clear_spd();
            self.coast_flag = true;
        } else {
            self.water_drift(env, events);
        }
    }

    /// Vertical water physics (`aUKI_set_spd_relations_in_water`):
    /// the touch_timer state machine. Sets max_velocity_y/gravity.
    fn water_state(&mut self, env: &StepEnv, events: &mut Vec<UkiEvent>) {
        let wh = env.water_height;
        if self.gyo_command == gyo_command::BITTEN {
            if self.gyo_status == gyo_status::HOOKED {
                let height = wh + 7.5;
                if self.touch_timer != 6 {
                    vib_calc(self, height, 2.0, 1.0);
                    if self.pos.y >= height {
                        self.touch_timer = 6;
                    }
                } else {
                    vib_calc(self, height, 0.3, 0.1);
                }
            } else {
                let height = wh - 7.5;
                if self.touch_timer != 5 {
                    vib_calc(self, height, 1.5, 0.5);
                    if self.pos.y < wh - 7.5 {
                        self.touch_timer = 5;
                    }
                } else {
                    vib_calc(self, height, 0.3, 0.1);
                }
            }
        } else if self.gyo_command == gyo_command::ENGAGED {
            if self.touched_flag {
                self.touched_flag = false;
                self.touch_timer = 2;
                events.push(UkiEvent::Ripple { arg: 2 });
                events.push(UkiEvent::TouchVib);
            }
            match self.touch_timer {
                4 => vib_calc(self, wh, 0.3, -0.050000005),
                0 | 2 => {
                    let h = wh - 1.7;
                    vib_calc(self, h, 0.9, 1.0);
                    if self.pos.y < h {
                        self.touch_timer = 3;
                    }
                }
                3 => {
                    vib_calc(self, wh, 0.9, 1.0);
                    if self.pos.y >= wh {
                        self.touch_timer = 4;
                    }
                }
                _ => {}
            }
        } else if env.on_waterfall {
            if self.pos.y < wh {
                if self.touch_timer == 5 {
                    vib_calc(self, wh, 0.9, 1.0);
                } else {
                    vib_calc(self, wh, 0.3, -0.050000005);
                }
            } else {
                self.max_velocity_y = -3.0;
                self.gravity = 3.0;
            }
            match self.proc {
                proc::WAIT | proc::TOUCH | proc::BITE => {
                    self.gyo_status = gyo_status::AVAILABLE;
                    self.position_speed.y = 0.0;
                    self.gyo_command = gyo_command::NONE;
                }
                _ => {}
            }
        } else if self.touch_timer == 5 {
            vib_calc(self, wh, 0.9, 1.0);
            if self.pos.y >= wh {
                events.push(UkiEvent::Ripple { arg: 1 });
                self.touch_timer = 0;
            }
        } else {
            vib_calc(self, wh, 0.3, -0.050000005);
        }
    }

    /// Fish actor writes: engaged the float (`gyo_command = 1`).
    pub fn fish_engage(&mut self, fish_id: u32, fish_type: i32, fish_angle_y: f32) {
        self.gyo_command = gyo_command::ENGAGED;
        self.gyo_type = fish_type;
        self.child_actor = Some(fish_id);
        self.angle_y = fish_angle_y;
    }

    /// Fish actor writes: bit (`gyo_command = 2`).
    pub fn fish_bite(&mut self) {
        self.gyo_command = gyo_command::BITTEN;
    }

    /// One logic tick of the proc machine. Returns engine side effects.
    /// Does nothing while `command == 0`.
    pub fn step(&mut self, env: &StepEnv, events: &mut Vec<UkiEvent>) {
        if !self.is_active() {
            return;
        }
        match self.proc {
            proc::CARRY => {
                self.pos = self.right_hand_pos;
                self.uki_pos = self.right_hand_pos;
                if self.command == command::READY {
                    if timer_step(&mut self.frame_timer) == 0 {
                        self.set_proc(proc::READY, 0, events);
                    }
                }
            }
            proc::READY => {
                if timer_step(&mut self.frame_timer) == 0 {
                    match self.command {
                        command::CAST => self.set_proc(proc::CAST, 0, events),
                        command::AIR => self.set_proc(proc::AIR, 0, events),
                        _ => {}
                    }
                } else {
                    let mut v = self.parabola_vec[0];
                    parabola_advance(&mut self.pos, &mut v, &self.parabola_vec[1]);
                    self.parabola_vec[0] = v;
                }
            }
            proc::AIR => {
                let mut v = self.parabola_vec[0];
                parabola_advance(&mut self.pos, &mut v, &self.parabola_vec[1]);
                self.parabola_vec[0] = v;
                if timer_step(&mut self.frame_timer) == 0 {
                    self.clear_spd();
                    self.set_proc(proc::CARRY, 0, events);
                }
            }
            proc::CAST => {
                let mut v = self.parabola_vec[0];
                let mut a = self.parabola_vec[1];
                parabola_advance(&mut self.pos, &mut v, &a);
                let _ = &mut a;
                self.parabola_vec[0] = v;
                if env.in_water {
                    self.hit_water_flag = true;
                    events.push(UkiEvent::Splash { arg: 3 });
                    events.push(UkiEvent::Vibration);
                    self.status = status::FLOAT;
                    events.push(UkiEvent::Sound("NA_SE_10B"));
                    self.set_proc(proc::WAIT, 0, events);
                }
            }
            proc::WAIT => {
                self.hit_water_flag = false;
                self.uki_pos = self.pos;
                chase_f(&mut self.position_speed.y, self.max_velocity_y, self.gravity);
                if !self.force_command(events) {
                    if timer_step(&mut self.cast_timer) == 0
                        && self.gyo_command == gyo_command::ENGAGED
                    {
                        self.set_proc(proc::TOUCH, 0, events);
                    } else if self.command == command::REEL {
                        self.clear_spd();
                        if timer_step(&mut self.frame_timer) == 0 {
                            // Failed reel: HIT with gyo_status != 5.
                            self.set_proc(proc::HIT, 1, events);
                        }
                    } else {
                        self.coast_wave(env, events);
                    }
                }
                self.water_state(env, events);
            }
            proc::HIT => {
                if timer_step(&mut self.frame_timer) == 0 {
                    if self.gyo_status == gyo_status::CAUGHT_RETURNING {
                        self.set_proc(proc::CATCH, 0, events);
                    } else {
                        self.gyo_command = gyo_command::NONE;
                        self.gyo_status = gyo_status::IDLE;
                        self.set_proc(proc::CARRY, 0, events);
                    }
                } else {
                    let mut v = self.parabola_vec[0];
                    parabola_advance(&mut self.pos, &mut v, &self.parabola_vec[1]);
                    self.parabola_vec[0] = v;
                    let mut a = self.parabola_acc[0];
                    parabola_advance(&mut self.uki_pos, &mut a, &self.parabola_acc[1]);
                    self.parabola_acc[0] = a;
                }
            }
            proc::TOUCH => {
                chase_f(&mut self.position_speed.y, self.max_velocity_y, self.gravity);
                if !self.force_command(events) {
                    if self.gyo_command == gyo_command::BITTEN {
                        events.push(UkiEvent::TouchVib);
                        self.set_proc(proc::BITE, 0, events);
                    } else if self.command == command::REEL {
                        self.clear_spd();
                        if timer_step(&mut self.frame_timer) == 0 {
                            self.set_proc(proc::HIT, 1, events);
                        }
                    } else {
                        // Drift; position integration is engine-side.
                        self.pos.x += self.position_speed.x;
                        self.pos.z += self.position_speed.z;
                    }
                }
                self.water_state(env, events);
            }
            proc::BITE => {
                chase_f(&mut self.position_speed.y, self.max_velocity_y, self.gravity);
                if !self.force_command(events) {
                    if self.gyo_command == gyo_command::BITTEN {
                        if self.command == command::REEL {
                            if timer_step(&mut self.frame_timer) == 0 {
                                self.clear_spd();
                                self.set_proc(proc::HIT, 3, events);
                                // Second trajectory: uki_pos -> left hand.
                                let (.., acc) = parabola_init(
                                    &self.uki_pos,
                                    &self.left_hand_pos,
                                    self.frame_timer,
                                    2,
                                );
                                self.parabola_acc = acc;
                                self.gyo_status = gyo_status::CAUGHT_RETURNING;
                            } else {
                                // Active reel: size-dependent wobble.
                                let size = GYOEI_TYPES
                                    .get(self.gyo_type.max(0) as usize)
                                    .map(|t| t.size)
                                    .unwrap_or(3) as usize;
                                self.angle_y += (BITE_ANGLE_DEG[size] * 0.5) * PI / 180.0;
                                self.speed = BITE_SPEED[size];
                                self.position_speed.x = self.speed * self.angle_y.sin();
                                self.position_speed.z = self.speed * self.angle_y.cos();
                                self.pos.x += self.position_speed.x;
                                self.pos.z += self.position_speed.z;
                                self.uki_pos = self.gyo_pos;
                                self.gyo_status = gyo_status::HOOKED;
                                if self.status != status::VIB {
                                    events.push(UkiEvent::BiteVib);
                                }
                                self.status = status::VIB;
                            }
                        } else {
                            self.touch_vib_frames = self.touch_vib_frames.wrapping_add(1);
                            if self.touch_vib_frames >= 10 {
                                self.touch_vib_frames = 0;
                                events.push(UkiEvent::TouchVib);
                            }
                        }
                    } else {
                        // Fish backed off.
                        self.set_proc(proc::WAIT, 0, events);
                    }
                }
                self.water_state(env, events);
            }
            proc::CATCH => {
                self.pos = self.right_hand_pos;
                self.uki_pos = self.left_hand_pos;
                match self.command {
                    command::CARRY => {
                        self.gyo_command = gyo_command::NONE;
                        self.gyo_status = gyo_status::FINALIZE;
                        self.set_proc(proc::CARRY, 0, events);
                    }
                    command::GET => self.set_proc(proc::GET, 0, events),
                    _ => {}
                }
            }
            proc::GET => {
                self.pos = self.right_hand_pos;
                self.uki_pos = self.left_hand_pos;
                if self.command == command::CARRY {
                    self.gyo_command = gyo_command::NONE;
                    self.gyo_status = gyo_status::IDLE;
                    self.set_proc(proc::CARRY, 0, events);
                }
            }
            proc::FORCE => {
                if self.status == status::COMEBACK {
                    if timer_step(&mut self.frame_timer) == 0 {
                        self.set_proc(proc::CARRY, 0, events);
                    } else {
                        let mut v = self.parabola_vec[0];
                        parabola_advance(&mut self.pos, &mut v, &self.parabola_vec[1]);
                        self.parabola_vec[0] = v;
                    }
                } else {
                    self.force_command(events);
                }
            }
            _ => {}
        }
    }
}

/// `aUKI_vib_calc`: pick vertical physics for a target height.
fn vib_calc(uki: &mut UkiActor, height: f32, max_speed_y: f32, gravity: f32) {
    if uki.pos.y < height {
        uki.max_velocity_y = max_speed_y;
    } else {
        uki.max_velocity_y = -max_speed_y;
    }
    uki.gravity = gravity;
}

/// Draw model selection (`ac_uki_draw.c_inc`): the equipped rod picks the
/// float model; anything that is not the normal rod gets the golden float.
pub fn float_model_golden(equipped_item_is_normal_rod: bool) -> bool {
    !equipped_item_is_normal_rod
}

// ---- C ABI ----

#[no_mangle]
pub extern "C" fn pc_uki_proc_count() -> u8 {
    proc::NUM
}

#[no_mangle]
pub extern "C" fn pc_uki_status_count() -> u8 {
    status::NUM
}

#[no_mangle]
pub extern "C" fn pc_uki_reel_timer(gyo_type: u8) -> i16 {
    reel_timer_for(gyo_type)
}

#[no_mangle]
pub extern "C" fn pc_uki_fish_item(gyo_type: u8) -> i8 {
    fish_item_index(gyo_type).map(|v| v as i8).unwrap_or(-1)
}

#[no_mangle]
pub extern "C" fn pc_uki_trash_for_size(size: u8) -> u8 {
    trash_substitute(size)
}

#[no_mangle]
pub extern "C" fn pc_uki_bite_frames(golden: u8, category: u8) -> i32 {
    fish_bite_frames(golden != 0, category)
}

#[no_mangle]
pub extern "C" fn pc_uki_search_angle(golden: u8, area: u8) -> f32 {
    let r = if golden != 0 { 1 } else { 0 };
    SEARCH_ANGLE[r][(area as usize).min(4)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_state() {
        let u = UkiActor::default();
        assert_eq!(u.proc, proc::CARRY);
        assert_eq!(u.status, status::CARRY);
        assert_eq!(u.frame_timer, 2);
        assert_eq!(u.cast_timer, 0);
        assert_eq!(u.gyo_type, -1);
        assert_eq!(u.gyo_command, gyo_command::NONE);
        assert_eq!(u.gyo_status, gyo_status::IDLE);
        assert!(u.child_actor.is_none());
        assert!(!u.is_active()); // command == 0 suspends the machine
        assert_eq!(UKI_ACTOR_SIZE, 0x2C8);
    }

    #[test]
    fn carry_ready_cast_wait() {
        let mut u = UkiActor::default();
        let mut ev = Vec::new();
        u.command = command::READY;
        u.right_hand_pos = Xyz::new(10.0, 5.0, 10.0);
        let env = StepEnv::default();
        // CARRY: follows the hand; frame_timer 2 -> READY after 2 ticks.
        u.step(&env, &mut ev);
        assert_eq!(u.pos, Xyz::new(10.0, 5.0, 10.0));
        assert_eq!(u.proc, proc::CARRY);
        u.step(&env, &mut ev);
        assert_eq!(u.proc, proc::READY);
        assert_eq!(u.status, status::READY);
        assert_eq!(u.frame_timer, 32);
        // READY: 32 ticks -> CAST on command 3.
        u.command = command::CAST;
        for _ in 0..32 {
            u.step(&env, &mut ev);
        }
        assert_eq!(u.proc, proc::CAST);
        assert_eq!(u.frame_timer, 50);
        assert_eq!(u.cast_timer, 40);
        assert_eq!(u.status, status::CAST);
        // CAST advances the parabola without touching frame_timer...
        let ft = u.frame_timer;
        u.step(&env, &mut ev);
        assert_eq!(u.frame_timer, ft);
        assert_eq!(u.proc, proc::CAST);
        // ...until water collision.
        let mut wet = StepEnv::default();
        wet.in_water = true;
        u.step(&wet, &mut ev);
        assert_eq!(u.proc, proc::WAIT);
        assert_eq!(u.status, status::FLOAT);
        assert!(u.hit_water_flag);
        assert_eq!(u.gyo_status, gyo_status::AVAILABLE);
        assert!(ev.contains(&UkiEvent::Sound("NA_SE_10B")));
    }

    #[test]
    fn wait_touch_bite_catch_get_carry() {
        let mut u = UkiActor::default();
        let mut ev = Vec::new();
        let env = StepEnv::default();
        // Jump to WAIT like a finished cast.
        u.command = command::CARRY;
        u.set_proc(proc::WAIT, 0, &mut ev);
        u.cast_timer = 40; // as set by a real CAST
        assert_eq!(u.gyo_status, gyo_status::AVAILABLE);
        // cast_timer gates the fish: 40 ticks; not reeling yet.
        for _ in 0..40 {
            u.step(&env, &mut ev);
            assert_eq!(u.proc, proc::WAIT);
        }
        assert_eq!(u.cast_timer, 0);
        // Fish engages.
        u.fish_engage(7, 22, 1.0); // salmon
        assert_eq!(u.gyo_type, 22);
        assert_eq!(u.child_actor, Some(7));
        u.step(&env, &mut ev);
        assert_eq!(u.proc, proc::TOUCH);
        assert_eq!(u.gyo_status, gyo_status::TOUCH);
        // Fish bites.
        u.fish_bite();
        u.step(&env, &mut ev);
        assert_eq!(u.proc, proc::BITE);
        assert_eq!(u.gyo_status, gyo_status::BITE);
        assert_eq!(u.frame_timer, reel_timer_for(22)); // salmon: L -> 52*2
        // Active reel.
        u.command = command::REEL;
        u.step(&env, &mut ev);
        assert_eq!(u.status, status::VIB);
        assert_eq!(u.gyo_status, gyo_status::HOOKED);
        assert!(ev.contains(&UkiEvent::BiteVib));
        // Reel to completion.
        for _ in 0..200 {
            if u.proc == proc::HIT {
                break;
            }
            u.step(&env, &mut ev);
        }
        assert_eq!(u.proc, proc::HIT);
        assert_eq!(u.gyo_status, gyo_status::CAUGHT_RETURNING);
        assert_eq!(u.status, status::COMEBACK);
        for _ in 0..60 {
            u.step(&env, &mut ev);
        }
        assert_eq!(u.proc, proc::CATCH);
        assert_eq!(u.gyo_status, gyo_status::CAUGHT);
        // Take the fish, then back to carry.
        u.command = command::GET;
        u.step(&env, &mut ev);
        assert_eq!(u.proc, proc::GET);
        assert_eq!(u.gyo_status, gyo_status::GET);
        u.command = command::CARRY;
        u.step(&env, &mut ev);
        assert_eq!(u.proc, proc::CARRY);
        assert_eq!(u.gyo_status, gyo_status::IDLE);
        assert_eq!(u.gyo_command, gyo_command::NONE);
    }

    #[test]
    fn failed_reel_returns_to_carry() {
        let mut u = UkiActor::default();
        let mut ev = Vec::new();
        let env = StepEnv::default();
        u.command = command::REEL;
        u.set_proc(proc::WAIT, 0, &mut ev);
        // No fish: 12-frame retraction -> HIT with gyo_status != 5.
        for _ in 0..60 {
            if u.proc == proc::HIT {
                break;
            }
            u.step(&env, &mut ev);
        }
        assert_eq!(u.proc, proc::HIT);
        assert_ne!(u.gyo_status, gyo_status::CAUGHT_RETURNING);
        for _ in 0..60 {
            u.step(&env, &mut ev);
        }
        assert_eq!(u.proc, proc::CARRY);
        assert_eq!(u.gyo_status, gyo_status::IDLE);
    }

    #[test]
    fn parabola_math() {
        let p0 = Xyz::new(0.0, 0.0, 0.0);
        let p1 = Xyz::new(60.0, 0.0, 0.0);
        let (vec, _acc) = parabola_init(&p0, &p1, 50, 1);
        // step = 25 -> vec.x = 60/25 = 2.4; y_param 4.5.
        assert!((vec[0].x - 2.4).abs() < 1e-4);
        assert!((vec[1].y - (2.0 * 4.5 * 50.0) / 2500.0).abs() < 1e-4);
        // Type 0 is linear.
        let (v0, _) = parabola_init(&p0, &p1, 20, 0);
        assert!((v0[0].x - 3.0).abs() < 1e-4);
        assert!((v0[0].y - 0.0).abs() < 1e-4);
        // High arcs use y_param 12.
        let high = Xyz::new(0.0, 150.0, 0.0);
        let (vh, _) = parabola_init(&p0, &high, 50, 1);
        assert!((vh[1].y - (2.0 * (12.0 * 50.0 - 150.0)) / 2500.0).abs() < 1e-4);
    }

    #[test]
    fn fish_tables() {
        // Reel timers: salmon (L) -> 104, trash -> 26.
        assert_eq!(reel_timer_for(22), 104);
        assert_eq!(reel_timer_for(41), 26);
        assert_eq!(reel_timer_for(0), 78); // crucian carp XS -> 39*2
        // Item conversion incl. the duplicate 39 and FISH22.
        assert_eq!(fish_item_index(0), Some(0));
        assert_eq!(fish_item_index(39), Some(39));
        assert_eq!(fish_item_index(40), Some(39));
        assert_eq!(fish_item_index(41), Some(41));
        assert_eq!(fish_item_index(44), Some(22));
        assert_eq!(fish_item_index(45), None);
        // Trash substitution by size.
        assert_eq!(trash_substitute(0), 41);
        assert_eq!(trash_substitute(3), 42);
        assert_eq!(trash_substitute(7), 43);
        // Golden rod tables.
        assert_eq!(pc_uki_search_angle(0, 0), 3.0);
        assert_eq!(pc_uki_search_angle(1, 0), 7.5);
        assert_eq!(fish_bite_frames(false, 0), 20);
        assert_eq!(fish_bite_frames(true, 4), 120);
        // Touch tables.
        assert_eq!(TOUCH_DISTANCE[0], 12.0);
        assert_eq!(TOUCH_COUNT[7], 24);
        assert_eq!(BACK_SPEED[5], -0.50);
        assert_eq!(SEARCH_DIST[3], 50.0);
        // C ABI spot checks.
        assert_eq!(pc_uki_proc_count(), 11);
        assert_eq!(pc_uki_status_count(), 8);
    }
}
