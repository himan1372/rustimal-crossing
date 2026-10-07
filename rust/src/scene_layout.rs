//! Per-scene state layouts (scene description language) for the Rust rewrite.
//!
//! Source-verified (upstream `include/m_scene.h`,
//! `include/m_scene_table.h`, `src/game/m_scene.c`,
//! `src/game/m_play.c`):
//!
//! Gameplay scenes are declarative records, not monolithic objects.
//! A scene is a `Scene_Word_u[]` array of tagged records; `Scene_ct`
//! (`m_scene.c:322`) walks the array and dispatches each record by
//! its type byte through a static handler table, stopping at END.
//! Records with `type >= mSc_SCENE_DATA_TYPE_NUM` are skipped.
//!
//! Record types (`m_scene.h:96`): 0 PLAYER_PTR, 1 CTRL_ACTOR_PTR,
//! 2 ACTOR_PTR, 3 OBJECT_EXCHANGE_BANK_PTR, 4 DOOR_DATA_PTR,
//! 5 FIELD_CT, 6 MY_ROOM_CT, 7 ARRANGE_ROOM_CT,
//! 8 ARRANGE_FURNITURE_CT, 9 SOUND, 10 END.
//!
//! * FIELD_CT carries item_type, bg_num, bg_disp_size, room_type,
//!   draw_type; its handler calls `mFM_SetFieldInitData` and sets
//!   game_started=FALSE, in_initial_block=TRUE, sunlight_flag=TRUE.
//!   NOTE: the PC port (`TARGET_PC`) repacks FIELD_CT from
//!   `misc.param3` on little-endian — the struct layout is
//!   big-endian-ordered. This rewrite stores decoded fields directly.
//! * Room types (`m_scene.h:39`): OUTDOORS, MY_ROOM, NPC_ROOM,
//!   MISC_ROOM.
//! * MY_ROOM_CT / ARRANGE_ROOM_CT / ARRANGE_FURNITURE_CT activate
//!   room-resource systems (`mScn_ObtainMyRoomBank`,
//!   `mScn_ObtainCarpetBank`, `arrange_ftr_num`).
//! * `Door_data_c` (`m_scene.h:47`): next_scene_id,
//!   exit_orientation, exit_type, extra_data, exit_position,
//!   door_actor_name, wipe_type — the explicit state bridge between
//!   scenes.
//! * `goto_other_scene` (`m_scene.c:512`): saves the door record,
//!   `next_scene_id = door_data->next_scene_id + 1`,
//!   `play->next_scene_no`, `restore_fgdata_all(play)`; a
//!   WIPE_TYPE_NORMAL door becomes WIPE_TYPE_FADE_BLACK.
//!
//! The gameplay scene table (`m_scene_table.h`) holds ~54 scenes
//! (SCENE_FG, SCENE_NPC_HOUSE, SCENE_MY_ROOM_*, SCENE_SHOP0,
//! museum rooms, ...). Individual `*_info[]` scene arrays exist as
//! data but their contents were not recovered in the reviewed headers.

/// Record type tags (`mSc_SCENE_DATA_TYPE_*`).
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SceneWordType {
    PlayerPtr = 0,
    CtrlActorPtr = 1,
    ActorPtr = 2,
    ObjectExchangeBankPtr = 3,
    DoorDataPtr = 4,
    FieldCt = 5,
    MyRoomCt = 6,
    ArrangeRoomCt = 7,
    ArrangeFurnitureCt = 8,
    Sound = 9,
    End = 10,
}

impl SceneWordType {
    pub fn from_u8(t: u8) -> Option<Self> {
        match t {
            0 => Some(Self::PlayerPtr),
            1 => Some(Self::CtrlActorPtr),
            2 => Some(Self::ActorPtr),
            3 => Some(Self::ObjectExchangeBankPtr),
            4 => Some(Self::DoorDataPtr),
            5 => Some(Self::FieldCt),
            6 => Some(Self::MyRoomCt),
            7 => Some(Self::ArrangeRoomCt),
            8 => Some(Self::ArrangeFurnitureCt),
            9 => Some(Self::Sound),
            10 => Some(Self::End),
            _ => None,
        }
    }
}

/// Room-type discriminator (`mSc_ROOM_TYPE_*`).
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RoomType {
    Outdoors = 0,
    MyRoom = 1,
    NpcRoom = 2,
    MiscRoom = 3,
}

/// Door transition record (`Door_data_c`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DoorData {
    pub next_scene_id: i32,
    pub exit_orientation: u8,
    pub exit_type: u8,
    pub extra_data: u16,
    pub exit_position: [i16; 3],
    pub door_actor_name: u16,
    pub wipe_type: u8,
}

/// One decoded scene word.
#[derive(Clone, Debug)]
pub enum SceneWord {
    PlayerPtr { num_actors: u8 },
    CtrlActorPtr { num_ctrl_actors: u8 },
    ActorPtr { num_actors: u8 },
    ObjectBank { num_banks: u8 },
    DoorData { num_doors: u8, doors: Vec<DoorData> },
    FieldCt {
        item_type: u8,
        bg_num: u8,
        bg_disp_size: u16,
        room_type: RoomType,
        draw_type: u8,
    },
    MyRoomCt,
    ArrangeRoomCt,
    ArrangeFurnitureCt { arrange_ftr_num: u8 },
    Sound { param0: u8, param1: u8, param2: u8, param3: u32 },
    End,
}

impl SceneWord {
    pub fn word_type(&self) -> SceneWordType {
        match self {
            Self::PlayerPtr { .. } => SceneWordType::PlayerPtr,
            Self::CtrlActorPtr { .. } => SceneWordType::CtrlActorPtr,
            Self::ActorPtr { .. } => SceneWordType::ActorPtr,
            Self::ObjectBank { .. } => SceneWordType::ObjectExchangeBankPtr,
            Self::DoorData { .. } => SceneWordType::DoorDataPtr,
            Self::FieldCt { .. } => SceneWordType::FieldCt,
            Self::MyRoomCt => SceneWordType::MyRoomCt,
            Self::ArrangeRoomCt => SceneWordType::ArrangeRoomCt,
            Self::ArrangeFurnitureCt { .. } => SceneWordType::ArrangeFurnitureCt,
            Self::Sound { .. } => SceneWordType::Sound,
            Self::End => SceneWordType::End,
        }
    }
}

/// The field-initialization state a FIELD_CT record produces
/// (`Scene_Proc_Field_ct`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FieldInit {
    pub bg_num: u8,
    pub bg_disp_size: u16,
    pub room_type: RoomType,
    pub draw_type: u8,
    pub item_type: u8,
    /// Always FALSE after a FIELD_CT record.
    pub game_started: bool,
    /// Always TRUE after a FIELD_CT record.
    pub in_initial_block: bool,
    /// Always TRUE after a FIELD_CT record.
    pub sunlight_flag: bool,
}

impl FieldInit {
    pub fn from_word(w: &SceneWord) -> Option<Self> {
        if let SceneWord::FieldCt { item_type, bg_num, bg_disp_size, room_type, draw_type } = w {
            Some(Self {
                bg_num: *bg_num,
                bg_disp_size: *bg_disp_size,
                room_type: *room_type,
                draw_type: *draw_type,
                item_type: *item_type,
                game_started: false,
                in_initial_block: true,
                sunlight_flag: true,
            })
        } else {
            None
        }
    }
}

/// Interpret a scene description (`Scene_ct`): walk words until END,
/// calling the handler for each known record type.
pub fn interpret_scene(
    words: &[SceneWord],
    handle: &mut dyn FnMut(&SceneWord),
) {
    for w in words {
        if w.word_type() == SceneWordType::End {
            break;
        }
        // Mirrors `if (type < mSc_SCENE_DATA_TYPE_NUM)`; every
        // decodable word is a known type, so all are dispatched.
        handle(w);
    }
}

/// Scene-transition request (`goto_other_scene`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SceneTransition {
    /// `door_data->next_scene_id + 1`.
    pub next_scene_no: i32,
    /// Resolved wipe type (NORMAL doors become FADE_BLACK).
    pub wipe_type: u8,
    /// Whether foreground data must be restored.
    pub restore_fgdata: bool,
}

/// Normal wipe-type value used by doors.
pub const WIPE_TYPE_NORMAL: u8 = 0;
/// Fade-to-black wipe the engine substitutes for normal doors.
pub const WIPE_TYPE_FADE_BLACK: u8 = 1;

/// Build the transition for a door (`goto_other_scene` core).
pub fn goto_other_scene(door: &DoorData) -> SceneTransition {
    let wipe_type = if door.wipe_type == WIPE_TYPE_NORMAL {
        WIPE_TYPE_FADE_BLACK
    } else {
        door.wipe_type
    };
    SceneTransition {
        next_scene_no: door.next_scene_id + 1,
        wipe_type,
        restore_fgdata: true,
    }
}

/// C ABI: record-type tag of a raw scene word's first byte, or -1.
#[no_mangle]
pub extern "C" fn pc_scene_word_type(first_byte: u8) -> i32 {
    SceneWordType::from_u8(first_byte).map(|t| t as i32).unwrap_or(-1)
}

/// C ABI: next scene number for a door's next_scene_id.
#[no_mangle]
pub extern "C" fn pc_door_next_scene(next_scene_id: i32) -> i32 {
    next_scene_id + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn word_type_tags_match_source() {
        assert_eq!(SceneWordType::from_u8(0), Some(SceneWordType::PlayerPtr));
        assert_eq!(SceneWordType::from_u8(5), Some(SceneWordType::FieldCt));
        assert_eq!(SceneWordType::from_u8(10), Some(SceneWordType::End));
        assert_eq!(SceneWordType::from_u8(11), None);
        assert_eq!(SceneWordType::from_u8(255), None);
    }

    #[test]
    fn interpreter_stops_at_end() {
        let words = vec![
            SceneWord::ActorPtr { num_actors: 3 },
            SceneWord::FieldCt {
                item_type: 1, bg_num: 2, bg_disp_size: 100,
                room_type: RoomType::NpcRoom, draw_type: 0,
            },
            SceneWord::End,
            SceneWord::ActorPtr { num_actors: 9 },
        ];
        let mut seen = Vec::new();
        interpret_scene(&words, &mut |w| seen.push(w.word_type()));
        assert_eq!(seen, vec![SceneWordType::ActorPtr, SceneWordType::FieldCt]);
    }

    #[test]
    fn field_ct_produces_init_flags() {
        let w = SceneWord::FieldCt {
            item_type: 1, bg_num: 4, bg_disp_size: 200,
            room_type: RoomType::MyRoom, draw_type: 2,
        };
        let init = FieldInit::from_word(&w).unwrap();
        assert_eq!((init.bg_num, init.bg_disp_size), (4, 200));
        assert_eq!(init.room_type, RoomType::MyRoom);
        assert!(!init.game_started && init.in_initial_block && init.sunlight_flag);
        assert!(FieldInit::from_word(&SceneWord::MyRoomCt).is_none());
    }

    #[test]
    fn door_transition() {
        let door = DoorData {
            next_scene_id: 7, exit_orientation: 2, exit_type: 0,
            extra_data: 0, exit_position: [0, 0, 0],
            door_actor_name: 0, wipe_type: WIPE_TYPE_NORMAL,
        };
        let t = goto_other_scene(&door);
        assert_eq!(t.next_scene_no, 8);
        assert_eq!(t.wipe_type, WIPE_TYPE_FADE_BLACK);
        assert!(t.restore_fgdata);
        let custom = DoorData { wipe_type: 5, ..door };
        assert_eq!(goto_other_scene(&custom).wipe_type, 5);
    }

    #[test]
    fn room_type_values() {
        assert_eq!(RoomType::Outdoors as u8, 0);
        assert_eq!(RoomType::MiscRoom as u8, 3);
    }
}
