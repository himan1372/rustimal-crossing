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
#include <stddef.h> /* size_t */

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

/* ================= Wave 2 — struct-boundary kernels =================
 *
 * These take values extracted from C structs, never the structs
 * themselves. C owns all retail state, tables, and RNG; Rust returns a
 * pure result and C applies it. Deliberately NOT declared here:
 * pc_locomotion_core (wrong abstraction), pc_town_generate and
 * pc_town_select_initial_villagers (different town representation). */

/* ---- Item / quest: m_quest.c, m_npc.c ---- */

/* mQst_GetGoods_common furniture branch: nonzero when the RANDOM(10)
 * roll means "use the villager's own furniture". C generates RANDOM(10). */
u8 pc_npc_house_goods(u32 roll_0_9);

/* mNpc_DecideNpcFurniture eligible count. C classifies each of the 100
 * house-grid items with the retail predicates into flags (bit 0 =
 * furniture, bits 1..3 = excluded kind, 7 = no exclusion); C keeps
 * RANDOM(num) and the selection/storage. */
u32 pc_eligible_furniture_count(const u8* flags);

/* ---- Quest request selection: ac_quest_talk_normal_init.c ---- */

/* aQMgr_get_possession_ftr_cpt_wl_rnd: pocket index of the sel-th
 * eligible carried item, or -1. eligible[i] nonzero = pocket i eligible.
 * rng_value must already satisfy 0 <= rng_value < eligible_count
 * (C passes RANDOM(item_cnt) directly); out-of-range returns -1. */
int pc_request_pick_carried(const u16* pockets, const u8* eligible,
                            size_t count, u32 rng_value);

/* aQMgr_decide_idx_prob_table: 100-entry shuffle + pick with exactly 61
 * rng(100) calls in retail order. rng is C's callback, e.g.
 * `static u32 pc_rng(u32 n) { return (u32)RANDOM(n); }`. */
int pc_request_dispatch(const u8* probs, size_t count, u32 (*rng)(u32));

/* ---- Player: m_player*.c_inc ---- */

/* Turn coefficient from the controller movement percentage. */
f32 pc_turn_mod(f32 move_pr);

/* ---- NPC movement: m_npc.c ---- */

/* Wander thinker action choice. C passes looks/personality and a
 * bounded RANDOM(10) roll; C keeps the actor/schedule/destination. */
u8 pc_wander_choice(u8 personality, int roll);

/* aNPC_chk_avoid_and_search decision from extracted values. */
u8 pc_friendship_mode(int friendship, u8 player_same_block);

/* ---- NPC schedule: m_npc.c ---- */

/* Base/saved schedule state for a personality at a time. C assigns the
 * result to sched->saved_type; the live scheduler (forced type/timer,
 * current type, events) stays in C. */
u8 pc_npc_schedule_state(u8 looks, u32 seconds);

/* Base-schedule sleep predicate (helper only — not an actor sleep check). */
int pc_npc_is_asleep(u8 looks, u32 seconds);

/* ---- NPC interaction: m_npc.c ---- */

/* mNpc_GetOverImpatient: patience class from talk count + looks
 * (temper table indexed by looks). C keeps mNpc_CountTalkNum. */
u8 pc_npc_patience(u8 talk_num, u8 looks);

/* ---- Mail: m_npc.c ---- */

/* mNpc_Remail friendship delta: good/present -> +3/+6/-2/+1.
 * C owns Mail_c/Anmmem_c and applies mNpc_AddFriendship. */
int pc_letter_friendship_delta(u8 good, u8 present);

/* ---- Collision setup ---- */

/* Neighborhood dimension from range (3/5/7). */
u32 pc_collision_neighborhood(f32 range);

/* mCoBG_CollisionData_c 32-bit pack: bit0 slate, 1..5 center, 6..10
 * top_left, 11..15 bot_left, 16..20 bot_right, 21..25 top_right,
 * 26..31 unit_attribute. */
u32 pc_collision_pack(u8 slate, u8 center, u8 top_left, u8 bot_left,
                      u8 bot_right, u8 top_right, u8 attribute);

/* ---- Field deposit: m_field_info.c ---- */

/* mFI_GetLineDeposit / mFI_LineDepositON / mFI_LineDepositOFF.
 * C passes mFI_GetDepositP(bx, bz) + ut_z directly. */
int pc_buried_line_get(const u16* line, u8 ut_x);
void pc_buried_line_set(u16* line, u8 ut_x);
void pc_buried_line_clear(u16* line, u8 ut_x);

/* ---- House: ac_npc_shop_common.c ---- */

/* Next mortgage from size + basement flag. C applies size->loan and
 * the message/save mutations. */
u32 pc_house_next_loan(u8 size, int basement_ordered);

/* ---- Shop: m_shop.c ---- */

/* mSP_GetRealShopLevel. disable_visitor_req mirrors the PC port's
 * g_pc_settings toggle (Nookington's without a foreign visitor). */
u8 pc_shop_real_level(u32 sales_sum, int visitor_flag,
                      int disable_visitor_req);

/* mSP_PlusSales: clamped new sales sum. C writes it back to the save. */
u32 pc_shop_plus_sales(u32 sales_sum, u8 tier, u32 sum);

/* ---- Scene ---- */

/* game_get_next_game_dlftbl index validation: C resolves the table
 * pointer via the retail function-pointer scan first; Rust classifies
 * the static index. */
int pc_scene_table_index(u8 table_index);

/* game_dlftbls[] entry count (11 with TARGET_PC model viewer). Low value — ARRAY_COUNT suffices. */
u32 pc_game_dlftbls_count(void);

/* ---- Scene: m_scene.c ---- */

/* Scene_ct word-type tag from the first byte, or -1 for invalid. */
int pc_scene_word_type(u8 first_byte);

/* goto_other_scene arithmetic: next_scene_id + 1. */
int pc_door_next_scene(int next_scene_id);

/* ---- Fishing (uki): src/actor/ac_uki_move.c_inc, ac_gyo_kaseki.c, ac_gyo_test.c ----
 * All five are verbatim table lookups; C keeps every RNG roll, every actor
 * mutation, and every side effect. (pc_uki_proc_count / pc_uki_status_count
 * exist in Rust but have no C call sites, so they stay undeclared.) */

/* BITE proc reel timer: trash gyo types -> 26, else REEL_TIMER[size] * 2.
 * Takes the raw s16 gyo_type; out-of-range degrades safely. */
s16 pc_uki_reel_timer(u8 gyo_type);

/* Fish->item table index for aUKI_get_fish_type, or -1 when the C guard
 * (0 <= gyo_type < aGYO_TYPE_EXTENDED_NUM) would have rejected it. */
s8 pc_uki_fish_item(u8 gyo_type);

/* Trash substitution on bite commit (gomi[size_type]); the 1/20 roll stays
 * in C. Returns the replacement gyo type (41/42/43). */
u8 pc_uki_trash_for_size(u8 size_type);

/* Fish bite persistence in frames: aGYO_bite_time[rod][category] * 2.
 * golden is 0/1 (from aGKK_get_uki_type()/aGYO_get_uki_type() == *_GOLDEN). */
int pc_uki_bite_frames(u8 golden, u8 category);

/* Fish approach half-angle in degrees: aGYO_search_angle[rod][area].
 * C still applies DEG2SHORT_ANGLE2 and the search-distance table. */
f32 pc_uki_search_angle(u8 golden, u8 area);


/* ---- Mail family (Wave A): leaflet.rs / mail.rs / mother_mail.rs /
 * ---- npc_event_mail.rs / npc_reply.rs / special_delivery.rs /
 * ---- villager_mail.rs                                        */

/* leaflet.rs — post-office leaflet broadcast (m_post_office.c,
 * ac_npc_post_man_move.c_inc, ac_event_manager.c, m_event.c, m_shop.c).
 * C keeps the singleton state, mailboxes, and all side effects. */

/* House pending bit for a leaflet recipient mask (0 = pending). */
int pc_leaflet_house_pending(s16 flags, int house_no);

/* Event-leaflet type filter: which broadcast channel a mail type uses. */
int pc_leaflet_event_filter(u8 mail_type);

/* Shop-sale leaflet template selection by shop level and category. */
int pc_leaflet_shop_sale_template(int shop_level, int category);

/* Broker-sale leaflet template from a pre-rolled RNG value (roll stays in C). */
int pc_leaflet_broker_template(int rng3);

/* Rare-chirashi leaflet template by shop level and rare type. */
int pc_leaflet_rare_chirashi_template(int shop_level, int rare_type);

/* Postman delivery check: normal mail pending + leaflet/event flags. */
int pc_leaflet_postman_check(int normal_pending, s16 leaflet_flags,
                             s16 event_flags, int house_no, int is_arbeit);

/* Next leaflet delivery hour slot. */
int pc_leaflet_next_delivery(u8 hour);

/* mail.rs — mailbox and post-office queue kernels (m_mail.c, m_post_office.c). */

/* Font-state: mailbox slot unused marker from a font byte. */
u8 pc_mail_unused(u8 font_byte);

/* Font-state: mark-read transformation of a font byte. */
u8 pc_mail_mark_read(u8 font_byte);

/* Next delivery time slot (09:00/17:00 schedule) for an hour. */
u16 pc_next_delivery(u8 hour);

/* Post-office queue full check from player/npc mail counts. */
u8 pc_po_full(s16 players, s16 npcs);

/* Last used mailbox slot index from a used-flags array. */
size_t pc_mailbox_last_idx(const u8* used, size_t n);

/* mother_mail.rs — mother's mail scheduler (m_private.c).
 * C keeps the per-player collection state and all RNG rolls. */

/* Fixed-date (Pool A) letter paper selection. */
int pc_mother_mail_paper(u8 month, u8 day, u8 bday_month, u8 bday_day);

/* Pool B no-repeat check: 1 when the normal letter was already sent. */
int pc_mother_mail_normal_not_send(const u8* num);

/* Pool C monthly no-repeat check. */
int pc_mother_mail_monthly_not_send(const u8* num, u8 month);

/* Pool C monthly letter number for a month and event number. */
int pc_mother_mail_monthly_no(u8 month, u8 event_no);

/* npc_event_mail.rs — NPC event/famicom/popular/magazine/goodbye mail
 * number selection. C keeps looks/tables and the send itself. */
u16 pc_npcmail_event_no(u8 looks, u8 typ);
u16 pc_npcmail_famicom_no(u8 looks, u8 valid);
u16 pc_npcmail_popular_no(u8 looks, u8 target, u16 npc_code);
u16 pc_npcmail_magazine_no(u8 looks, u8 verdict);
u16 pc_npcmail_goodbye_no(u8 looks, u8 roll);

/* npc_reply.rs — villager letter-reply kernels. C keeps the letter body
 * buffer, friendship state, and all RNG rolls. */
int pc_mck_key_hit(const u8* body);
int pc_npc_letter_rank(const u8* body);
int pc_npc_bad_msg_no(int foreign, int looks, int roll3);
int pc_npc_good_base_no(int foreign, int looks);
int pc_npc_receive_friendship(int rank, int has_present);

/* special_delivery.rs — catalog orders and lottery (m_shop.c / post office).
 * C keeps shop level, tickets, and all RNG rolls. */
u8 pc_catalog_order_num(void);
u8 pc_lottery_item_count(void);
u8 pc_lottery_req_tickets(void);
u8 pc_ticket_month(u16 item);
u8 pc_ticket_count(u16 item);
u8 pc_lottery_roll(u8 rng_100);
u16 pc_catalog_mail_template(u8 shop_level);

/* villager_mail.rs — bounded repeated-character check for villager mail
 * bodies. Renamed from mNpc_CheckNormalMail_sub to avoid a link collision
 * with the retail TU (title_demo precedent). */
int pc_npc_check_normal_mail_sub(int* char_num, const u8* body);


/* ---- Wave B leftovers: bg_check.rs / move_bg.rs / terrain_walls.rs /
 * ---- dialogue_topics.rs / house.rs                            */
/* NOTE: bg_check / move_bg / terrain_walls sit near the open acre-boundary
 * bug. These exports are pure kernels (coordinate generation, wall
 * classification); the acre-edge wall logic itself stays in C. */

/* bg_check.rs — fixed-buffer neighborhood coordinates
 * (mCoBG_MakeSizeUnitInfo coordinate core). PcUnitCoord is repr(C)
 * { int x; int z }; the caller provides capacity >= 49. Returns count. */
typedef struct { int x; int z; } pc_unit_coord_t;
u8 pc_bg_neighborhood_coords(int center_x, int center_z, int size,
                             pc_unit_coord_t* coords);

/* move_bg.rs — move-bg wall construction (pure geometry kernel).
 * PcMoveBgWall is repr(C) { f32 start_x, start_z, end_x, end_z, normal_x,
 * normal_z; s16 normal_angle; u8 wall_name; f32 bottom, top }. */
typedef struct {
    f32 start_x; f32 start_z; f32 end_x; f32 end_z;
    f32 normal_x; f32 normal_z; s16 normal_angle; u8 wall_name;
    f32 bottom; f32 top;
} pc_move_bg_wall_t;
u8 pc_make_move_bg_walls(f32 right, f32 left, f32 up, f32 down,
                         f32 pos_x, f32 pos_y, f32 pos_z, s16 angle_y,
                         const f32* base_ofs, f32 scale, f32 height,
                         u8 check_type, pc_move_bg_wall_t* out);
/* Move-bg frame delta: writes 3-element delta to out_delta. */
void pc_move_bg_delta(f32 cur_x, f32 cur_y, f32 cur_z,
                      f32 last_x, f32 last_y, f32 last_z, f32* out_delta);

/* terrain_walls.rs — cardinal edge mask + exact height check. */
u8 pc_cardinal_edge_mask(u8 size, u8 index);
u8 pc_check_height_exactly(f32 start_top, f32 start_btm, f32 end_top, f32 end_btm,
                           f32 start_x, f32 start_z, f32 end_x, f32 end_z,
                           u8 wall_name, f32 pos_y, f32 point_x, f32 point_z,
                           u8 is_move_bg, f32* out_top, f32* out_bot);

/* dialogue_topics.rs — talk gating kernels. */
u8 pc_talk_count_allowed(u8 talk_num, u8 talk_num_max, u16 timer);
u8 pc_talk_patience_raw(u8 talk_num, u8 over_impatient_num, u8 talk_num_max);

/* house.rs — catalog order date comparison. */
u8 pc_order_date_passed(u16 oy, u8 om, u8 od, u16 ty, u8 tm, u8 td);

/* talk_request.rs — talk-request dispatch + normal-talk gate (Wave C dialogue core).
 * C keeps all demo/submenu state and performs the actions; Rust decides. */
u8 pc_talk_request_dispatch(u8 demo_speak_active, u8 demo_speech_active,
                            u8 demo_talk_active, u8 demo_listenable,
                            u8 submenu_wait, u8 submenu_wait_timer_zero,
                            u8 has_talk_request_proc);
u8 pc_normal_talk_request_gate(u8 clip_proc_present, u8 clip_proc_result);

/* force_call.rs — force-call request availability gate (Wave C).
 * mDemo_CAN_ACTOR_TALK decomposes to !SPEAK && !TALK per the Rust port. */
u8 pc_force_call_req_proc(u8 force_call_flag, u8 talk_condition_none,
                          u8 in_speak_demo, u8 in_talk_demo);

/* npc_ai.rs — schedule manager step + global override (Wave C).
 * out[0]=saved_type, out[1]=current_type, out[2]=new forced_timer. */
void pc_schedule_step(u8 looks, u32 now_sec, u32 forced_type,
                      i32 forced_timer, i32 forced_ticks, u32* out);
u8 pc_schedule_global_override(u8 first_job, u8 halloween, u32* out_type);

/* talk_topics.rs — first-job hint + KI weather/time message (Wave C). */
i32 pc_fj_hint_msg_no(i32 hint_type, u8 looks);
i32 pc_weather_time_msg_no(i32 base, i32 time_kind, i32 weather, i32 rand2);


/* ---- Wave E furniture: furniture.rs / fg_data.rs ---- */
/* NOTE: furniture is referenced from the pending save system, but these
 * are read-only catalog kernels (rotation, bounds, placement, counts).
 * They do not touch save data. */

/* furniture.rs — furniture catalog kernels. */
u8 pc_ftr_get_rotation(u16 item);
u16 pc_ftr_no_rot_2_idx(u16 ftr_no, u8 rot);
u8 pc_ftr_save_angle(f32 sin_v, f32 cos_v);
size_t pc_furniture_storage_slots(void);
size_t pc_scene_furniture_max(u8 kind);
u8 pc_room_bounds_ok(int x, int z);
u8 pc_switch_bit_index(int x, int z);
u8 pc_judge_place_2nd_layer(u8 below_is_surface, u8 layer1_empty);
u8 pc_ftr_is_storage(u16 interaction_type);
u32 pc_furniture_weight(void);

/* fg_data.rs — foreground data parsing kernels. */
u32 pc_fg_parse_count(const u8* data, size_t len);
u8 pc_fg_sign_count(const u8* record);

#ifdef __cplusplus
}
#endif

#endif /* PC_RUST_H */
