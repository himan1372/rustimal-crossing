# Wire remaining C callers to Rust exports
- [x] Inventory rust/src/ modules whose C ABI exports have no C callers rewired yet (2026-10-10, muse — table below).
- [ ] For each module: add the C plug in wiring/ behind #ifdef USE_RUST — follow the established pattern in wave3-runbook.md (C-side state gathering, helper-only functions where needed).
- [ ] Hand Philip copy commands plus one-at-a-time playtest steps; wait for his reports before moving on.
- [ ] Do NOT touch ecology modules — parked (Philip said "dont do it just yet").
Rules: keep C/C++ decomp logic as-is, preserve existing C ABIs, no test runs without authorization.

## Inventory (2026-10-10)

Checked every `#[no_mangle]` export in rust/src/ against the USE_RUST call
sites in wiring/ and the "Already wired" tables in rewiring.md.

### Already wired — do not touch
- letter_score.rs (call-site pattern in C mail code)
- Wave 3 (build-system exclusions, all 7 on Rust): aram, dvd+disc, gbi_runtime, profiler, mtx, vi
- Wave 1/2 kernels with plugs in wiring/: pc_make_tab_2_move_tail,
  pc_unit_no_name_2_start_end, pc_inventory_find/count, pc_judge_wall_from_vector,
  pc_distance_dispatch, pc_bg_neighborhood, pc_bg_distance_reverse, pc_bg_room_scope,
  pc_forbid_vectors/gate, pc_scene_word_type, pc_column_recipe_item, pc_door_next_scene,
  pc_house_next_loan, pc_topic_force_gate, pc_topic_talk_check, pc_npc_house_goods,
  pc_turn_mod, pc_wander_choice, pc_friendship_mode, pc_shop_real_level, pc_shop_plus_sales,
  pc_request_proc_id, pc_eligible_furniture_count, pc_request_pick_carried,
  pc_npc_schedule_state, pc_npc_patience, pc_buried_line_get/set/clear,
  pc_request_dispatch, pc_letter_friendship_delta
  (plus C-side helpers already in the plugs: password/check family, mail-wait,
  present, addd, rng_100, msg_win, summercamp, island-ftr)
- uki.rs — wired 2026-10-10 (this task): pc_uki_reel_timer, pc_uki_fish_item,
  pc_uki_trash_for_size, pc_uki_bite_frames, pc_uki_search_angle
  (pc_uki_proc_count/status_count: no C call sites, left undeclared)

### Do NOT wire (redesign or parked)
- ecology.rs — PARKED per Philip. species.rs (fish/insect tables) is
  ecology-adjacent: treat as parked until he says otherwise.
- town_gen.rs: pc_town_generate, pc_town_select_initial_villagers — rewrite-owned
  state (Wave 2D). pc_town_assess needs assessment, not a plug.
- player_move.rs: pc_locomotion_core — wrong abstraction (Wave 2D).
- scene.rs: pc_scene_table_index — pointer scan stays in C (Wave 2D).
- buried_items.rs: pc_buried_get/set/clear — wrong boundary, use line ops (Wave 2D).
- columns.rs: pc_column_recipe — internal-only, not retail-wireable.
- title_demo.rs — Rust owns DEMO_STATE (Mutex static); the C struct's demono
  would desync. Needs pure-kernel exports (demono_next/titledemo_index/
  demo_button_ok already exist as pure fns) before any plug. Do NOT wire as-is.
- dialogue_topics.rs: pc_msg_max — low priority by design.

### Defer until open issues resolve
- audio.rs — open audio bugs (Bug 2 dig sound, Bug 3 museum BGM deferred).
- player_action.rs, player_tools.rs, tool_resolvers.rs — Bug 2 area.
- Collision family (attr_walls pc_forbid_proc, bridge_acre, endpoint_circle,
  column_sweep, collision_temporal, wall_hit_dir, slate_classify, wpos2attribute,
  terrain_walls, bg_check pc_bg_neighborhood_coords, move_bg, wall_priority) —
  acre-boundary bug unresolved; collision plugs wait.
- field_gen.rs, placement.rs, step3_data.rs, template_select.rs, albumin.rs,
  albumin_geometry.rs — acre/town-gen area; same acre-bug deferral.
- save.rs, save_format.rs — save-corruption risk; wire late, with extra care.
- graphics.rs — black-screen failure mode; wire last.

### Wire next (verified pure, clear C sites) — suggested order
1. Mail family — letter_score precedent, clear sites:
   leaflet.rs (7), mail.rs (5), mother_mail.rs (4), npc_event_mail.rs (5),
   npc_reply.rs (5), special_delivery.rs (7), villager_mail.rs (1)
   — DONE 2026-10-15 (muse, commit 0d169b73, claim wire-wave-a-mail-family).
   8 new plug files + m_private/m_npc/m_shop extended + 34 decls +
   villager_mail pc_ rename. Deliberately unwired: pc_leaflet_next_delivery
   (same C site as pc_next_delivery, alternate packing), pc_mck_key_hit
   (subsumed by pc_npc_letter_rank whole-function plug), pc_npc_receive_friendship
   (site already wired by Wave 2 pc_letter_friendship_delta), non-post-girl
   mPO_get_keep_mail_sum sites.
2. Wave 1C leftovers: bg_check pc_bg_neighborhood_coords, move_bg (2),
   terrain_walls (pc_cardinal_edge_mask, pc_check_height_exactly),
   house_scene pc_house_wall_floor, dialogue_topics pc_talk_count_allowed /
   pc_talk_patience_raw, scene pc_game_dlftbls_count, house pc_order_date_passed,
   npc pc_npc_is_asleep (helper-only), wall_priority (needs Rust fixed-buffer
   ABI export added first)
   — DONE 2026-10-16 (muse, commits 6449a9c8 + 6092d87d, claim wire-wave-b-leftovers).
   Wired 10: pc_bg_neighborhood_coords, pc_make_move_bg_walls, pc_move_bg_delta,
   pc_cardinal_edge_mask, pc_check_height_exactly, pc_talk_count_allowed,
   pc_order_date_passed, pc_house_wall_floor, pc_game_dlftbls_count (+ decl for
   pc_talk_patience_raw). Rust bugfixes in dialogue_topics.rs (8/8 tests): timer
   polarity, patience discriminants -> C enum. Deliberately unwired:
   pc_talk_patience_raw (dup of Wave 2B pc_npc_patience plug), pc_npc_is_asleep /
   pc_collision_pack (no C site), pc_collision_neighborhood (Wave 1 already wired it).
   wall_priority deferred (needs new Rust export). CAUTION: collision plugs in the
   acre-boundary-bug area — playtest acre transitions.
3. Dialogue core: npc_ai.rs (5), force_call.rs (3), talk_topics.rs (3),
   talk_request.rs (2)
   — PARTIAL 2026-10-16 (cosmo, commit 807baad02dad, claim wire-wave-c-dialogue-core).
   Wired (3): pc_talk_request_dispatch -> aNPC_talk_request_event_npc,
   pc_normal_talk_request_gate -> aNPC_normal_talk_request,
   pc_force_call_req_proc -> aNPC_force_call_req_proc (all in
   wiring/src/actor/npc/ac_npc_talk.c_inc + 3 pc_rust.h decls).
   BLOCKED (8, decomp files not in wiring/): pc_schedule_step,
   pc_schedule_global_override (need m_npc_schedule.c), pc_uzai_trigger,
   pc_talk_patience, pc_auto_greeting_msg_no (need ac_npc_think*.c_inc /
   ac_npc_act_greeting.c_inc), pc_select_talk_kind, pc_fj_hint_msg_no,
   pc_weather_time_msg_no (need ac_quest_talk_normal_init.c).
   SKIPPED (2, overlap with earlier waves): pc_force_talk_request overlaps
   Wave 1 pc_topic_force_gate plug in aNPC_force_talk_request (needs Philip's
   call); pc_wander_decide_next overlaps Wave 2 pc_wander_choice plug in
   aNPC_think_wander_decide_next (skipped).
   — PART 2 2026-10-16 (cosmo): +4 wired using ACreTeam/ac-decomp sources
   (Philip's link). m_npc_schedule.c: pc_schedule_step, pc_schedule_global_override.
   ac_quest_talk_normal_init.c: pc_fj_hint_msg_no, pc_weather_time_msg_no.
   Wave C now 7/13. Still pending: pc_select_talk_kind (branch analysis),
   pc_uzai_trigger, pc_talk_patience, pc_auto_greeting_msg_no (C sources TBD).
4. Quest: quest.rs (7), quest_gen.rs (15)
5. Furniture: furniture.rs (10), fg_data.rs (2)
6. Misc low-risk: bee_ant.rs (6), weather_season.rs (5), frame_loops.rs (3),
   villager_home.rs (5), collision.rs pc_collision_neighborhood/pack (sub-ops)

### uki module — plugs delivered 2026-10-10 (awaiting Philip's copy + playtest)
- wiring/src/actor/ac_uki_move.c_inc: aUKI_set_proc_bite reel timer ->
  pc_uki_reel_timer; aUKI_get_fish_type table index -> pc_uki_fish_item
- wiring/src/actor/ac_gyo_kaseki.c + ac_gyo_test.c (both fish variants are
  live actors): approach half-angle -> pc_uki_search_angle; bite_init frames ->
  pc_uki_bite_frames; gomi trash substitution -> pc_uki_trash_for_size
  (the 1/20 roll stays in C; Rust never owns the RNG)
- include/pc_rust.h: declarations for the 5 wired exports
- Plug logic syntax-checked standalone in both USE_RUST and vanilla modes
  (gcc -fsyntax-only -Wall clean); full-tree build needs his MSYS2 machine.
