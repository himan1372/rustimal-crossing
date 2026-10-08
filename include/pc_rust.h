/*
 * pc_rust.h — C declarations for the Rust Wave-1 kernels.
 *
 * Wave 1 functions are stateless pure kernels embedded inside large retail
 * C translation units. Do NOT exclude those C files from the build (unlike
 * Wave 3). Instead, include this header in the affected TU and replace the
 * pure calculation internally with a pc_* call, keeping the original C
 * function and all state/tables on the C side.
 *
 * Boundary rule: C owns all state, pointers, tables, and RNG. Rust receives
 * scalars (or plain buffers) and returns a scalar result only. In
 * particular, RNG values are generated in C (RANDOM(n)) and passed in;
 * Rust never calls the retail RNG.
 *
 * Types u8/u16/u32/f32 come from the project's existing typedefs.
 */
#ifndef PC_RUST_H
#define PC_RUST_H

#include "pc_types.h"

#ifdef __cplusplus
extern "C" {
#endif

/* ---- Collision: m_collision_bg.c / m_collision_bg_wall.c_inc /
 * ----           m_collision_bg_column.c_inc                    */

/* mCoBG_MakeTab2MoveTail: pull-back bias. dst_xz is read AND written. */
void pc_make_tab_2_move_tail(f32* dst_xz, f32 src_x, f32 src_z);

/* mCoBG_UnitNoName2StartEnd: wall segment endpoints. Writes out_start[2],
 * out_end[2]. wall_name 0..5 valid; >= 6 leaves outputs unassigned. */
void pc_unit_no_name_2_start_end(f32 ux, f32 uz, u8 wall_name, u8 check_type,
                                 f32* out_start, f32* out_end);

/* mCoBG_MakeForbidVectorData index selection: writes up to 2 vector IDs
 * into out; returns the count. C keeps mCoBG_make_vector_table and calls
 * mCoBG_UnitNoName2StartEnd itself. */
u8 pc_forbid_vectors(u8 attr, u8* out);

/* mCoBG_MakeUnitVector gate: (old_on_ground & attr_wall) & 1. */
u8 pc_forbid_gate(u8 old_on_ground, u8 attr_wall);

/* mCoBG_JudgeWallFromVector threshold on a precomputed angle:
 * returns 1 when |angle_deg| > 89.5. The angle itself stays in C. */
int pc_judge_wall_from_vector(f32 angle_deg);

/* Distance push/contact classification: 0 = push (dist < range),
 * 1 = contact (|dist - range| < 2.7), 2 = ignore. Height checks and
 * side effects stay in C. */
int pc_distance_dispatch(f32 dist, f32 range);

/* mCoBG_ActorFearture2CheckRange: 3 / 5 / 7 from range. */
int pc_bg_neighborhood(f32 range);

/* Distance reverse: (range - dist) + 0.00001. */
f32 pc_bg_distance_reverse(f32 range, f32 dist);

/* mCoBG_RoomScopeCheck half-extent: class 0 -> 160, 1 -> 240, 2 -> 320. */
f32 pc_bg_room_scope(u8 class);

/* mCoBG_MakeOneColumnCollisionData recipe from a retail item ID:
 * writes radius, height (ground_y + retail offset), atr_wall flag.
 * Returns 1 when the item produces a column. This is the wireable
 * boundary; pc_column_recipe (kind-id based) is internal only. */
u8 pc_column_recipe_item(u16 item, f32 ground_y, u8 old_on_ground,
                         f32* out_radius, f32* out_height, u8* out_atr);

/* ---- Inventory: m_private.c ---- */

/* mPr_GetPossessionItemIdx: first pocket holding item, or -1. */
int pc_inventory_find(const u16* pockets, u16 item);

/* mPr_GetPossessionItemSum: count of pockets holding item. */
u32 pc_inventory_count(const u16* pockets, u16 item);

/* ---- Dialogue: ac_npc_talk.c_inc ---- */

/* aNPC_set_talk_info_talk_request_check: base + looks*3 + rng3.
 * rng3 = RANDOM(3) generated in C; island selects the 0x34AC base. */
u32 pc_topic_talk_check(u8 looks, u32 rng3, int island);

/* aNPC_force_talk_request gate: 0 = none, 1 = forced, 2 = spontaneous. */
int pc_topic_force_gate(int force_call_msg_no, int friendship,
                        int over_friendship, int search_for_player,
                        f32 force_call_timer, f32 dist_xz, f32 dist_y);

/* MSG_MAX constant bridge (0x3F91). Wire only where the Rust constant
 * should be authoritative; otherwise keep the C macro. */
u32 pc_msg_max(void);

/* ---- NPC house: m_quest.c / m_npc.c ---- */

/* m_quest.c: 0x0D8B + looks request-proc ID. */
u32 pc_request_proc_id(u8 looks);

/* mNpc_GetNpcFloorNo / mNpc_GetNpcWallNo adapter: C gathers the field
 * type, wall/floor IDs, and owner validity; returns (wall << 8) | floor,
 * or -1 when they don't apply. */
int pc_house_wall_floor(int is_npc_room_field, u16 wall_id, u16 floor_id,
                        int has_owner);

/* ---- Scene: m_scene.c ---- */

/* Scene_ct word-type tag from the first byte, or -1 for invalid. */
int pc_scene_word_type(u8 first_byte);

/* goto_other_scene arithmetic: next_scene_id + 1. */
int pc_door_next_scene(int next_scene_id);

#ifdef __cplusplus
}
#endif

#endif /* PC_RUST_H */
