//! Scene table: 52 scene manifests + the Scene_ct interpreter model.
//!
//! Verified against `m_play.c` (Gameplay_Scene_Read / Gameplay_Scene_Init /
//! mPl_SceneNo2SoundRoomType), `m_scene.c` (Scene_ct / Scene_Proc_*),
//! `m_scene.h` (Scene_Word_u, macros), `m_scene_table.h` (scene enum),
//! and all 51 files under `src/data/scene/` (USA Rev. 0 decomp / PC port).
//!
//! Architecture (source-proven):
//!   scene ID (0..52) -> scene_word_data[idx] -> Scene_Word_u[] manifest
//!   -> Scene_ct() walks words until type==END, dispatching through the
//!   11-entry Scene_Proc table. Gameplay_Scene_Read() is a selector,
//!   not a parser: it installs the manifest pointer and calls
//!   Gameplay_Scene_Init().
//! Key corrections/notes:
//!   - SCENE_RANDOM_NPC_TEST (8) and SCENE_FIELD_TOOL (32) share the
//!     same manifest (field_tool_field_info); mapping is not 1:1.
//!   - mSc_DATA_MY_ROOM_CT() is never used in any decompiled manifest.
//!   - Scene_Proc_Sound is a stub in the decomp (sound params unresolved).
//!   - FIELD_CT packs (bg_disp_size<<16 | room_type<<8 | draw_type) into
//!     the generic param3 slot (big-endian union layout); the PC port
//!     unpacks it explicitly (TARGET_PC path in Scene_Proc_Field_ct).

/// Number of scene IDs.
pub const SCENE_NUM: usize = 52;

/// Scene word type bytes (mSc_SCENE_DATA_TYPE_*).
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

/// 8-byte scene word, generic view (Scene_Word_Data_Misc_c).
#[derive(Clone, Copy, Debug)]
pub struct SceneWord {
    pub ty: u8,
    pub p0: u8,
    pub p1: u8,
    pub p2: u8,
    pub param3: u32,
}

/// Item-type axis (mSc_ITEM_TYPE_*): BGITEM=0, DUMMY=1, BGPOLICEITEM=2,
/// BGPOSTITEM=3.
pub mod item_type {
    pub const BGITEM: u8 = 0;
    pub const DUMMY: u8 = 1;
    pub const BGPOLICEITEM: u8 = 2;
    pub const BGPOSTITEM: u8 = 3;
}

/// Room-type axis (mSc_ROOM_TYPE_*): OUTDOORS=0, MY_ROOM=1, NPC_ROOM=2,
/// MISC_ROOM=3.
pub mod room_type {
    pub const OUTDOORS: u8 = 0;
    pub const MY_ROOM: u8 = 1;
    pub const NPC_ROOM: u8 = 2;
    pub const MISC_ROOM: u8 = 3;
}

/// Draw-type axis (FIELD_DRAW_TYPE_*): OUTDOORS=0, INDOORS=1, TRAIN=2,
/// PLAYER_SELECT=3.
pub mod draw_type {
    pub const OUTDOORS: u8 = 0;
    pub const INDOORS: u8 = 1;
    pub const TRAIN: u8 = 2;
    pub const PLAYER_SELECT: u8 = 3;
}

/// Decoded FIELD_CT parameters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FieldCtParams {
    pub item_type: u8,
    pub bg_num: u8,
    pub bg_disp_size: u16,
    pub room_type: u8,
    pub draw_type: u8,
}

/// Unpack a FIELD_CT word using the TARGET_PC extraction logic:
/// item_type=p0, bg_num=p1, param3=(bg_disp_size<<16)|(room_type<<8)|draw_type.
pub fn fieldct_unpack(word: &SceneWord) -> FieldCtParams {
    debug_assert_eq!(word.ty, SceneWordType::FieldCt as u8);
    FieldCtParams {
        item_type: word.p0,
        bg_num: word.p1,
        bg_disp_size: ((word.param3 >> 16) & 0xFFFF) as u16,
        room_type: ((word.param3 >> 8) & 0xFF) as u8,
        draw_type: (word.param3 & 0xFF) as u8,
    }
}

/// Condensed per-scene manifest: every decompiled *_info[] array decoded
/// to counts/params. Fields: (sound0, sound1, doors, arrange_ftr,
/// arrange_room_ct, ctrl_actors, actors, obj_banks, item_type, bg_num,
/// bg_disp_size, room_type, draw_type). arrange_ftr = 255 means absent.
#[rustfmt::skip]
pub const SCENE_MANIFESTS: [(u8, u8, u8, u8, bool, u8, u8, u8, u8, u8, u16, u8, u8); SCENE_NUM] = [
    (0,0, 0,255, false, 12,2,1, 0,4,0x5000, 0,0), //  0 test01
    (0,0, 0,255, false, 12,0,0, 0,4,0x5000, 0,0), //  1 test02
    (0,0, 0,255, false, 12,2,4, 0,4,0x5000, 0,0), //  2 test03
    (0,0, 0,255, false, 14,2,0, 0,4,0x5000, 0,0), //  3 water_test
    (0,0, 0,255, false,  0,0,0, 0,4,0x5000, 0,0), //  4 test_step01
    (0,0, 0,255, false, 12,0,0, 0,4,0x5000, 0,0), //  5 test04
    (0,0, 1, 30, false, 12,2,1, 1,1,0xA000, 2,1), //  6 npc_room01
    (0,0, 0,255, false, 17,2,2, 0,4,0x1C00, 0,0), //  7 test_fd_npc_land
    (0,0, 0,255, false,  9,1,1, 0,4,0x2800, 0,0), //  8 field_tool_field
    (0,0, 1,  6, false, 16,0,1, 1,1,0xA000, 3,1), //  9 shop01
    (0,0, 0,255, false, 11,0,0, 0,4,0x5000, 0,0), // 10 BG_TEST01
    (0,0, 0,255, false, 11,0,0, 0,4,0x7800, 0,0), // 11 BG_TEST01_XLU
    (0,0, 1,  6,  true, 11,0,0, 1,1,0xA000, 3,1), // 12 broker_shop
    (0,0, 1, 30,  true,  9,3,1, 1,1,0xA000, 3,1), // 13 fg_tool_in
    (0,0, 1,  6, false, 10,3,1, 3,1,0xA000, 3,1), // 14 post_office
    (0,0, 1,  8,  true,  6,0,0, 0,1,0x7800, 3,2), // 15 start_demo1
    (0,0, 1,  8,  true,  6,0,0, 0,1,0xA000, 3,2), // 16 start_demo2
    (0,0, 1,255, false, 10,2,2, 2,1,0xA000, 3,1), // 17 police_box
    (0,0, 1,255, false,  8,0,0, 0,1,0xA000, 3,1), // 18 buggy
    (0,0, 0,255,  true,  3,0,0, 0,1,0xA000, 3,3), // 19 player_select
    (0,1, 1, 30, false, 12,2,2, 1,1,0xA000, 1,1), // 20 player_room_s
    (0,1, 1, 32, false, 12,2,2, 1,1,0xA000, 1,1), // 21 player_room_m
    (0,1, 1, 48, false, 12,2,2, 1,1,0xA000, 1,1), // 22 player_room_l
    (0,0, 1,  6, false, 16,0,1, 1,1,0xA000, 3,1), // 23 shop02
    (0,0, 1,  6, false, 15,0,0, 1,1,0xA000, 3,1), // 24 shop03
    (0,0, 1,  6, false, 15,0,1, 1,1,0xA000, 3,1), // 25 shop04_1f
    (0,0, 0,255, false,  5,0,0, 0,4,0x5000, 0,0), // 26 test05
    (0,0, 0,255,  true,  3,0,0, 0,1,0xA000, 3,3), // 27 PLAYER_SELECT2
    (0,0, 0,255,  true,  3,0,0, 0,1,0xA000, 3,3), // 28 PLAYER_SELECT3
    (0,0, 1,  6, false, 16,0,1, 1,1,0xA000, 3,1), // 29 shop04_2f
    (0,0, 0,255, false, 12,0,0, 0,4,0x2800, 0,0), // 30 event_notification
    (0,0, 1,  3, false, 11,0,0, 1,1,0xA000, 3,1), // 31 kamakura
    (0,0, 0,255, false,  9,1,1, 0,4,0x2800, 0,0), // 32 field_tool_field (shared w/ 8)
    (0,0, 0,255, false, 10,1,1, 0,4,0x2000, 0,0), // 33 title_demo
    (0,0, 0,255,  true,  3,0,0, 0,1,0xA000, 3,3), // 34 PLAYER_SELECT4
    (0,1, 4,  1, false, 12,4,1, 1,1,0xA000, 1,1), // 35 museum_entrance
    (0,1, 1,  1, false, 11,1,1, 1,1,0xA000, 1,1), // 36 museum_picture
    (0,1, 1, 25, false, 12,0,1, 1,1,0xA000, 1,1), // 37 museum_fossil
    (0,1, 1,255, false, 11,4,2, 1,1,0xB000, 1,1), // 38 museum_insect
    (0,1, 1,  1, false, 11,2,1, 1,1,0xA000, 1,1), // 39 museum_fish
    (0,1, 2, 48, false, 12,2,2, 1,1,0xA000, 1,1), // 40 player_room_ll1
    (0,1, 1, 48, false, 12,2,2, 1,1,0xA000, 1,1), // 41 player_room_ll2
    (0,1, 1, 48, false, 12,0,1, 1,1,0xA000, 1,1), // 42 p_room_bm_s
    (0,1, 1, 48, false, 12,0,1, 1,1,0xA000, 1,1), // 43 p_room_bm_m
    (0,1, 1, 48, false, 12,0,1, 1,1,0xA000, 1,1), // 44 p_room_bm_l
    (0,1, 1, 48, false, 12,0,1, 1,1,0xA000, 1,1), // 45 p_room_bm_ll1
    (0,1, 1,  1, false, 12,3,1, 1,1,0xA000, 1,1), // 46 NEEDLEWORK
    (0,1, 1, 48, false, 12,2,2, 1,1,0xA000, 1,1), // 47 player_room_island
    (0,0, 1, 30, false, 12,2,1, 1,1,0xA000, 2,1), // 48 npc_room_island
    (0,0, 1,  8,  true,  6,0,0, 0,1,0x7800, 3,2), // 49 start_demo3
    (0,0, 1, 30, false, 12,0,0, 1,1,0xA000, 2,1), // 50 lighthouse
    (0,0, 1,  3, false, 11,0,0, 1,1,0xA000, 3,1), // 51 tent
];

/// Sound room types from mPl_SceneNo2SoundRoomType (scene id -> 0..3).
/// 1: MY_ROOM_S. 2: NPC_HOUSE, SHOP0, BROKER_SHOP, POST_OFFICE, BUGGY,
/// MY_ROOM_M, KAMAKURA, MY_ROOM_LL2, TENT. 3: MY_ROOM_L, CONVENI, SUPER,
/// DEPART, DEPART_2, MY_ROOM_LL1, COTTAGE_MY, POLICE_BOX. 0: everything else.
pub const SOUND_ROOM_TYPE: [u8; SCENE_NUM] = [
    0,0,0,0,0,0,
    2,0,0,2,0,0,
    2,0,2,0,0,3,
    2,0,1,2,3,3,
    3,3,0,0,0,3,
    0,2,0,0,0,0,
    0,0,0,0,3,2,
    0,0,0,0,0,3,
    0,0,0,2,
];

/// scene_data_status layout: 0x14 bytes per scene x 52 = 0x410 bytes.
/// Semantics unresolved in the decomp (only unk13=0 writes on scene read).
pub const SCENE_STATUS_SIZE: usize = 0x14;
pub const SCENE_STATUS_TOTAL: usize = SCENE_STATUS_SIZE * SCENE_NUM;

/// Object exchange arena: mSc_ARENA_SIZE = 0xA000, 32-byte aligned.
pub const EXCHANGE_ARENA_SIZE: usize = 0xA000;

// ---- C ABI ----

/// C ABI: unpack a FIELD_CT word's parameters. Fills out[6] =
/// {item_type, bg_num, bg_disp_size_hi, bg_disp_size_lo, room_type, draw_type}.
#[no_mangle]
pub extern "C" fn pc_fieldct_unpack(ty: u8, p0: u8, p1: u8, param3: u32, out: *mut u8) {
    if out.is_null() {
        return;
    }
    let w = SceneWord { ty, p0, p1, p2: 0, param3 };
    let f = fieldct_unpack(&w);
    unsafe {
        *out.add(0) = f.item_type;
        *out.add(1) = f.bg_num;
        *out.add(2) = (f.bg_disp_size >> 8) as u8;
        *out.add(3) = (f.bg_disp_size & 0xFF) as u8;
        *out.add(4) = f.room_type;
        *out.add(5) = f.draw_type;
    }
}

/// C ABI: manifest summary for a scene id. Fills out[8] =
/// {doors, arrange_ftr(255=absent), arrange_room_ct, ctrl_actors,
///  actors, obj_banks, sound0, sound1}. Returns 0 on bad id.
#[no_mangle]
pub extern "C" fn pc_scene_manifest(idx: usize, out: *mut u8) -> u8 {
    if out.is_null() || idx >= SCENE_NUM {
        return 0;
    }
    let m = SCENE_MANIFESTS[idx];
    unsafe {
        *out.add(0) = m.2;
        *out.add(1) = m.3;
        *out.add(2) = m.4 as u8;
        *out.add(3) = m.5;
        *out.add(4) = m.6;
        *out.add(5) = m.7;
        *out.add(6) = m.0;
        *out.add(7) = m.1;
    }
    1
}

/// C ABI: sound room type for a scene id (mPl_SceneNo2SoundRoomType).
/// Returns 255 on bad id.
#[no_mangle]
pub extern "C" fn pc_scene_sound_room_type(idx: usize) -> u8 {
    if idx >= SCENE_NUM {
        return 255;
    }
    SOUND_ROOM_TYPE[idx]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_table_shape() {
        assert_eq!(SCENE_NUM, 52);
        assert_eq!(SCENE_MANIFESTS.len(), 52);
        assert_eq!(SOUND_ROOM_TYPE.len(), 52);
        // Shared manifest: 8 and 32 identical.
        assert_eq!(SCENE_MANIFESTS[8], SCENE_MANIFESTS[32]);
        // Word type values.
        assert_eq!(SceneWordType::FieldCt as u8, 5);
        assert_eq!(SceneWordType::End as u8, 10);
    }

    #[test]
    fn fieldct_unpack_pc_logic() {
        // npc_room01 FIELDCT(DUMMY, 1, 0xA000, NPC_ROOM, INDOORS)
        let packed = (0xA000u32 << 16) | (2u32 << 8) | 1u32;
        let w = SceneWord { ty: 5, p0: 1, p1: 1, p2: 0, param3: packed };
        let f = fieldct_unpack(&w);
        assert_eq!(f, FieldCtParams { item_type: 1, bg_num: 1, bg_disp_size: 0xA000, room_type: 2, draw_type: 1 });
        // Outdoor test scene: (BGITEM, 4, 0x5000, OUTDOORS, OUTDOORS)
        let w2 = SceneWord { ty: 5, p0: 0, p1: 4, p2: 0, param3: (0x5000u32 << 16) };
        let f2 = fieldct_unpack(&w2);
        assert_eq!(f2.bg_disp_size, 0x5000);
        assert_eq!(f2.room_type, 0);
    }

    #[test]
    fn manifest_spot_checks() {
        // 6 npc_room01
        let m = SCENE_MANIFESTS[6];
        assert_eq!((m.2, m.3, m.5, m.6, m.7), (1, 30, 12, 2, 1));
        // 35 museum_entrance: 4 doors
        assert_eq!(SCENE_MANIFESTS[35].2, 4);
        // 38 museum_insect: 0xB000 disp
        assert_eq!(SCENE_MANIFESTS[38].10, 0xB000);
        // 20 player_room_s: SOUND(0,1), 30 ftr
        assert_eq!((SCENE_MANIFESTS[20].0, SCENE_MANIFESTS[20].1, SCENE_MANIFESTS[20].3), (0, 1, 30));
        // 49 start_demo3: TRAIN draw type
        assert_eq!(SCENE_MANIFESTS[49].12, draw_type::TRAIN);
        // Sound room types
        assert_eq!(pc_scene_sound_room_type(20), 1);
        assert_eq!(pc_scene_sound_room_type(6), 2);
        assert_eq!(pc_scene_sound_room_type(22), 3);
        assert_eq!(pc_scene_sound_room_type(17), 3); // POLICE_BOX -> 3
        assert_eq!(pc_scene_sound_room_type(0), 0);
        assert_eq!(pc_scene_sound_room_type(99), 255);
        // C ABI
        let mut out = [0u8; 8];
        assert_eq!(pc_scene_manifest(6, out.as_mut_ptr()), 1);
        assert_eq!(&out[..], &[1, 30, 0, 12, 2, 1, 0, 0]);
        assert_eq!(pc_scene_manifest(99, out.as_mut_ptr()), 0);
    }
}
