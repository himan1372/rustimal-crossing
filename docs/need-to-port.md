# Need to Port — Animal Crossing GameCube Rust Rewrite

**Status as of 2026-10-11 (evening)**

- **Ported to Rust:** 125 modules in `rust/src/` (decomp-verified against GAFE01_00 Rev 0 USA)
- **Still in C:** everything below — grouped into game systems with plain-English names.
- This doc is updated as work lands: ✅ marks show what's done, and the trailing
  "Leftover pieces" section collects the small follow-ups each port batch uncovers.

A "system" here means a chunk of gameplay Philip would recognize. Filenames are
the decomp sources (`src/game/`, `src/actor/`, `src/actor/npc/`) that implement it.

Legend: ✅ = ported (Rust module named). Entries without ✅ still need porting.

---

## Special visitors (characters who aren't villagers)

✅ **Wisp the ghost** — appears at night, grants wishes. → `wisp.rs`
`ac_npc_majin.c`, `ac_npc_majin2.c`, `ac_npc_majin3.c`, `ac_npc_majin4.c`, `ac_npc_majin5.c`

✅ **Blanca the faceless cat** — draws a face on her, she returns wearing it. → `blanca.rs`
`ac_npc_mask_cat.c`, `ac_npc_mask_cat2.c`

✅ **Gulliver the seagull** — washes up on the beach, wake him for furniture. → `gulliver.rs`
(Event flags exist in `m_event.h`; his actor/beach logic is ported.)

✅ **Kapp'n's boat** — the boat ride to the island (songs, island trips). → `boat.rs`
`ac_boat.c`, `ac_boat_demo.c`

✅ **Train arrival/departure** — Rover intro, Porter at the station, train scenes. → `train.rs`
`m_train_control.c`, `ac_train0.c`, `ac_train1.c`, `ac_train_door.c`, `ac_train_window.c`, `ac_station.c`, `ac_npc_station_master.c`, `ac_npc_depart_master.c`, `ac_npc_depart_master_anime.c`, `ac_depart.c`, `ac_shasho.c`

✅ **Tortimer the mayor** — event-day speeches and gifts. → `tortimer.rs`
`m_soncho.c`, `ac_npc_soncho.c`

✅ **K.K. Slider** — Saturday night concerts, song requests, airchecks. → `totakeke.rs`
`ac_npc_totakeke.c`, `m_music_ovl.c`, `ac_radio.c`

✅ **Mr. Resetti** — the reset lecture. → `majin.rs`
`ac_reset_demo.c`

✅ **Post office clerks** — Pelly/Phyllis/Pete counter interactions. → `post_office.rs`
`ac_npc_post_girl.c`, `ac_npc_post_man.c`, `ac_post_office.c`

✅ **Police officers** — Copper/Booker at the police station. → `police.rs`
`ac_npc_police.c`, `ac_npc_police2.c`

✅ **Fortune teller, wallpaper sellers, fashionista** — Katrina, Saharah, Wendell, Gracie, Redd tent events. → `event_visitors.rs`
(The actors use Japanese identifiers; they were traced and ported.)

---

## Shops & services

✅ **Stalk market (turnips)** — Joan's Sunday turnip sales, Nook's daily prices, spoilage. → `stalk_market.rs`
`m_kabu_manager.c`

✅ **Police station lost-and-found** — lost items, pickup logic. → `lost_found.rs`
`m_police_box.c`, `ac_police_box.c`

✅ **ABD bank** — deposit/withdraw/balance machine. → `bank.rs`
`m_bank_ovl.c`

✅ **Catalog ordering** — Nook's catalog kiosk (ties into `special_delivery.rs` mail path, but the kiosk UI isn't ported). → `catalog.rs`
`m_catalog_ovl.c`

✅ **Able Sisters design patterns** — the design editor, pattern slots, Mabel/Sable. → `design.rs`
`m_needlework.c`, `m_needlework_ovl.c`, `m_cpedit_ovl.c`, `m_cporiginal_ovl.c`, `m_design_ovl.c`, `ac_needlework_indoor.c`, `ac_needlework_shop.c`, `ac_npc_needlework.c`, `ac_broker_design.c`

✅ **Town tune composer** — the melody board at the post office. → `melody.rs`
`m_melody.c`, `m_mscore_ovl.c`, `ac_mscore_control.c`

✅ **Museum building exhibits** — fish/insect/picture rooms and the fossil hall actors. → `museum_display.rs`
(the fossil *assessment* kernels are in `museum.rs`; the building, tanks, and exhibit actors are now ported too)
`ac_museum.c`, `ac_museum_fish.c`, `ac_museum_fossil.c`, `ac_museum_indoor.c`, `ac_museum_insect.c`, `ac_museum_picture.c`

✅ **Shop interiors & fitting rooms** — Nook's store rooms, mannequins, umbrella stand, shop levels. → `shop_interior.rs`
(the shop *logic* is in `shop.rs`; the interior actors are now ported too)
`ac_shop_indoor.c`, `ac_shop_manekin.c`, `ac_shop_umbrella.c`, `ac_shop_level.c`, `ac_shop_goods.c`, `ac_shop_design.c`

✅ **Player house interiors** — rooms, clock, furniture arranging. → `house_interior.rs`
(the furniture placement judge, switch bitfields, and haniwa steps are in `furniture.rs`; `ac_arrange_ftr.c` is an empty stub with nothing to port)
`ac_my_house.c`, `ac_my_indoor.c`, `ac_my_room.c`, `ac_house_clock.c`, `ac_arrange_ftr.c`, `ac_arrange_room.c`

---

## Holidays & events

**Event-day NPCs and controllers** — each holiday has dedicated actor files. The pure
kernels are ported (Rust module named); actor lifecycles, RNG call order, save areas,
effects, and event orchestration stay in C until differential tests pass. The event
*scheduler* rows live in `event_day.rs`.
- ✅ New Year countdown: `ac_countdown.c`, `ac_countdown_npc0.c`, `ac_countdown_npc1.c`, `ac_count02.c` → `countdown.rs`
- ✅ Halloween: `ac_halloween_npc.c` → `halloween.rs`
- ✅ Fireworks: `ac_hanabi_npc0.c`, `ac_hanabi_npc1.c` → `fireworks.rs`
- ✅ Cherry blossom festival: `ac_hanami_npc0.c`, `ac_hanami_npc1.c` → `cherry_blossom.rs`
- ✅ Harvest Festival: `ac_harvest_npc0.c`, `ac_harvest_npc1.c` → `harvest.rs`
- ✅ Moon viewing: `ac_tukimi.c`, `ac_tukimi_npc0.c`, `ac_tukimi_npc1.c` → `moon_viewing.rs`
- Groundhog Day: `ac_groundhog_control.c`, `ac_groundhog_npc0.c`, `ac_goza.c`
- ✅ First shrine visit: `ac_hatumode_control.c`, `ac_hatumode_npc0.c` → `hatsumode.rs`
- ✅ Morning aerobics: `ac_taisou_npc0.c` → `taisou.rs`
- ✅ Tug of war: `ac_tunahiki_control.c`, `ac_tunahiki_npc0.c`, `ac_tunahiki_npc1.c` → `tunahiki.rs`
- ✅ Fishing tourney: `ac_turi.c`, `ac_turi_npc0.c`, `ac_ev_angler.c`, `m_fishrecord.c` → `fishing_tourney.rs`
- ✅ Sports fair ball toss: `ac_tamaire_npc0.c`, `ac_tamaire_npc1.c`, `ac_tama.c` → `tamaire.rs`
- ✅ Field Day footrace: `ac_tokyoso_control.c`, `ac_tokyoso_npc0.c`, `ac_tokyoso_npc1.c` → `tokyoso.rs`
  (note: these files are the footrace, NOT summer festival stalls — the old label here was wrong)
- Summer festival stalls (actual): `ac_yatai.c`, `ac_ev_yomise.c`
  (SP_NPC_EV_YOMISE, SP_NPC_EV_YOMISE2, FIREWORKS_STALL0, FIREWORKS_STALL1)
- April Fools: `ac_aprilfool_control.c`
- Birthdays: `m_birthday_ovl.c`, `ac_birth_control.c`
- Event NPC placement: `m_event_map_npc.c`, `ac_go_home_npc.c`

**Holiday scenery actors** — carp streamers, festival stalls, shrine, lotus.
`ac_koinobori.c`, `ac_yatai.c`, `ac_shrine.c`, `ac_lotus.c`, `ac_tukimi.c`
(`ac_yatai.c` is the actual stall prop actor — still in C; see the leftover section below.)

---

## Leftover pieces from the recent event ports (still in C)

Small, bounded follow-ups identified during the 2026-10-11 event-port batch. Each is a
candidate for a future pure-kernel port once its boundary is verified.

- **Summer festival stalls (actual)** — the real stall behavior: `ac_yatai.c` (stall prop
  actor), `ac_ev_yomise.c` (SP_NPC_EV_YOMISE, SP_NPC_EV_YOMISE2 stall NPCs),
  FIREWORKS_STALL0/1. Talk scripts, inventory/item handoff, and scheduling not yet traced.
- **Ball-toss scoring** — `eEC_EFFECT_TAMAIRE` trajectory and basket collision after the
  throw effect; how baskets are counted and whether a numeric team score exists.
- **Fishing record database** — `m_fishrecord.c` insertion/recycling (5-slot DB),
  `mEv_fishRecord_local` NPC simulation, `mFR_fishmail` prize mail. RNG order (including
  the extra unused `fqrand()`) must be preserved.
- **Fishing tourney host & save areas** — `ac_ev_angler.c` talk flow, entry flags, the
  `aEANG_event_data_c` / `aEANG_event_common_c` ownership, Chip's position persistence.
- **Tug-of-war rope actor & orchestration** — rope prop actor, NPC clip, the
  `fdtug_start`/`fdtug_stop`/`fdtug_in` event-manager handlers.
- **Morning aerobics radio & group systems** — radio counter (`sAdos_GetRadioCounter`),
  leader/group globals, Tortimer's radio-card inventory/calendar mutations.
- **Footrace controller** — the `_00` phase state machine (`aTKC_set`/`aTKC_go`), the
  shared save area (slot 8), the `_01` bitfield and `aTKC_FLAG_15` semantics (unresolved),
  stumble/dash effects.
- **Hatsumode open question** — `root_table`/`proc_idx_table` have 4 entries but 5 NPCs
  can spawn (possible retail out-of-bounds read at index 4); needs assembly-level
  confirmation before porting.
- **Groundhog Day** — `ac_groundhog_control.c`, `ac_groundhog_npc0.c`, `ac_goza.c` (not started).
- **April Fools / Birthdays** — `ac_aprilfool_control.c`; `m_birthday_ovl.c`,
  `ac_birth_control.c` (not started).
- **Event NPC placement** — `m_event_map_npc.c`, `ac_go_home_npc.c` (not started).
- **Gyroid follow-ups** — HANIWA_START/END numerics (verify against the build);
  `FTR_NO_2_IDX` mapping for `mSP_RandomHaniwaSelect`; the `mHW_MSG_DOREWO`
  interruption special case; `mAGrw_SetDigItem` running `mAGrw_SpoilKabu` over the
  stored haniwa slots; inventory-transfer semantics (`m_tag_ovl`/`m_hand_ovl`/
  `m_inventory_ovl`).
- **Cockroach follow-ups** — the house-entry → infestation-update → actor-spawn →
  stomp/death → saved-count-update call chain (does stomping decrement the saved
  count?); `aMR_CLIP->check_danna_kill_proc()` implementation and registration;
  the `mCkRh_DecideNowGokiFamilyCount` call site on house entry.
- **Balloon follow-ups** — the NULL-guard/call mismatch in the present drop (`item_tree_fruit_drop_proc` guard vs `fruit_set_proc` call); the little-shake double-set comment; the draw callback's present-skip condition; the rare-list selection internals and `mFI_GetOtherFruit()`; the `anim_frame`/`dt` accumulators.
- **Mushroom follow-ups** — the column-refill logic when `possible_cols` hits 0;
  the per-row candidate bitfields and the `mMsr_SetBlockMashroom` scan order
  (incl. the "bruh" early break); `lbRTC_IsOverTime` future-timestamp handling;
  the startup cleanup's (0, 0) player acre and missing event-status check.
- **Igloo follow-ups** — the `anim_frame` accumulator and the `graph_dt_period_elapsed`
  steam cadence; the villager-selection policy inside `mEvMN_GetNpcJointEv`; the
  unused `aKI_mochi_anime_data` table; the frame-rate-independent chase factor;
  the `mEv_EVENT_RUMOR_KAMAKURA` rumor row.
- **Snowman follow-ups** — `@BUG` NULL dereference in the foreground-search during
  the head-jump assembly; reward furniture IDs computed from the X-macro expansion
  (verify against the build before wiring); the `move_dist` common-area accessor;
  the event-manager birth orchestration (`snowman_start`, `mEv_snowman_born_check`);
  assembly dust effects and the parabolic head-jump arc.

---

## Nature & seasonal

✅ **Snowman building** — snowballs, snowman moods and rewards. → `snowman.rs`
`m_snowman.c`, `ac_snowman.c`, `ac_psnowman.c` (growth step, proportion score, message arithmetic, reward-index selection, record packing, slot/ID mapping, melt step, lifespan check, push/combine gates; snowman-season row 62 in `event_day.rs`)

✅ **Igloo** — the winter igloo interior and camper. → `kamakura.rs`
`ac_kamakura.c`, `ac_kamakura_indoor.c`, `ac_kamakura_npc0.c` (entry check, exit position, lighting target, candle phase, sin-point-half interpolation, greeting selection, furniture trade-list branch; kamakura schedule row 3 in `event_day.rs`)

✅ **Mushrooms** — fall mushroom spawning. → `mushroom.rs`
`m_mushroom.c`, `m_mushroom.h` (target count, time conversions, rounding, tree eligibility, offsets, exclusions, cleanup eligibility, clear-all/share distribution; mushroom-season row 48 in `event_day.rs`). NOTE: no mushroom furniture mechanism exists in the decomp — the system only ever places ITM_FOOD_MUSHROOM.

✅ **Balloon presents** — floating balloon gifts. → `balloon.rs`
`m_fuusen.c`, `ac_fuusen.c`, `ac_balloon.c` (spawn minute/roll/increment math, wind bucket + entry-point tables, movement/escape/tree predicates, present-drop gates, reward branch, held-balloon hide predicate)

✅ **Cockroaches** — infestation when away, stomping. → `cockroach.rs`
`m_cockroach.c`, `ac_house_goki.c` (growth/clamp, visible count, house index, stomp/proximity predicates, wait/move decision, jump-angle bug, timers/fades)

✅ **Gyroids** — digging them up, the gyroid household items. → `haniwa.rs`
`m_haniwa_ovl.c`, `m_haniwaPortrait_ovl.c`, `ac_haniwa.c`, `m_all_grow.c`, `m_all_grow_ovl.c` (item range, storage messages, house idx, bell bags, dialogue category, dig-direct predicate)

**Insect actors** — the 14 individual bug actors (spawn *tables* are ported in `insect_tables.rs`/`species.rs`; the per-bug behavior actors are not).
`ac_insect.c`, `ac_ins_amenbo.c`, `ac_ins_batta.c`, `ac_ins_chou.c`, `ac_ins_dango.c`, `ac_ins_goki.c`, `ac_ins_hitodama.c`, `ac_ins_hotaru.c`, `ac_ins_ka.c`, `ac_ins_kabuto.c`, `ac_ins_kera.c`, `ac_ins_mino.c`, `ac_ins_semi.c`, `ac_ins_tentou.c`, `ac_ins_tonbo.c`

**Fish actors** — shadows, release behavior (the *float* is ported in `uki.rs`; these are the fish themselves).
`ac_gyo_kage.c`, `ac_gyoei.c`, `ac_gyo_release.c`

**Fish & bug records** — the encyclopedia/collection tracking.
`m_fishrecord.c`

**Bees** — the bee-tree shake behavior (spawn side is in `bee_ant.rs`; hive/attack actors partially).
`ac_ghog.c` (check against `bee_ant.rs` before porting)

---

## Big-ticket systems

**NES games** — the Famicom emulator and unlockable NES titles.
`famicom_emu.c`, `m_fbdemo.c`, `m_fbdemo_fade.c`, `m_fbdemo_triforce.c`, `m_fbdemo_wipe1.c`

**GBA Island** — link-cable island, minigames, islanders.
`m_island.c`, `m_gba_ovl.c`, `ac_airplane.c`

**Bulletin boards** — town board and house board posts.
`m_board_ovl.c`, `m_hboard_ovl.c`

**Town map** — the map overlay UI.
`m_map_ovl.c`

**Diary** — the player diary.
`m_diary.c`, `m_diary_ovl.c`

**Name editor** — the character-entry screen, including the NG-word filter (this is where 'phili' got rejected; fix options already offered to Philip).
`m_editor_ovl.c`, `m_editEndChk_ovl.c`, `m_ledit_ovl.c`

**Passwords** — secret codes (make/check).
`m_passwordChk_ovl.c`, `m_passwordMake_ovl.c`

**Notices & handbills UI** — notice overlays (the handbill *composer* is ported in `npc_reply.rs`).
`m_notice.c`, `m_notice_ovl.c`

**Address book** — the contacts UI.
`m_address_ovl.c`

**Pause menu** — pause overlay.
`m_pause.c`

**Selection/choice UI** — generic list and yes/no prompts.
`m_select.c`, `m_choice.c`

**Calendar overlay** — the calendar UI.
`m_calendar_ovl.c`

---

## Buildings & scenery actors (not yet ported)

Town structures and decorative actors. None of these have Rust modules.
`ac_conveni.c` (convenience store), `ac_cottage.c`, `ac_tent.c`, `ac_toudai.c` (lighthouse), `ac_lighthouse_switch.c`, `ac_windmill.c`, `ac_douzou.c` (statues), `ac_mikanbox.c` (orange box), `ac_mikuji.c` (fortune), `ac_nameplate.c`, `ac_sign.c`, `ac_mailbox.c`, `ac_structure.c`, `ac_mural.c`, `ac_kago.c` (palanquin), `ac_dump.c` (dump), `ac_tama.c`, `ac_rope.c`, `ac_ball.c`, `ac_goza.c`, `ac_garagara.c`, `ac_misin.c`, `ac_mbg.c`, `ac_pterminal.c`, `ac_reserve.c`, `ac_sample.c`, `ac_super.c`, `ac_dummy.c`, `ac_flag.c`, `ac_htable.c`, `ac_animal_logo.c`, `ac_animal_logo_misc.c`

**Household actors** — radio, etc.
`ac_radio.c` (also plays K.K. songs — see K.K. Slider above)

---

## Engine-level (ported only in part, or intentionally C-side)

These are partially covered (`frame_loops.rs`, `scene.rs`, `graphics.rs`) but the files below have no direct Rust equivalent. Most should stay in C unless there's a reason.
- Camera: `m_camera2.c`, `m_view.c`
- Lighting: `m_lights.c`
- Controller/rumble: `m_controller.c`, `m_vibctl.c`
- Actor shadows: `m_actor_shadow.c`
- Actor/game tables: `m_actor_dlftbls.c`, `m_game_dlftbls.c`
- BG items/textures: `m_bg_item.c`, `m_bg_tex.c`, `m_land.c`
- Clipping: `m_clip.c`
- String/font core: `m_string.c` (font tables are ported in `npc_reply.rs`)
- Intro demo: `ac_intro_demo.c`, `ac_present_demo.c`, `ac_ride_off_demo.c`
- Field drawing: `ac_field_draw.c`, `ac_fieldm_draw.c`, `ac_effectbg.c`
- Misc: `m_watch_my_step.c`, `m_eappli.c`, `m_prenmi.c`, `m_banti.c`, `m_hand_ovl.c`, `m_huusui_room.c`, `m_huusui_room_ovl.c`, `m_mark_room.c`, `m_mark_room_ovl.c`, `m_tag_ovl.c`, `m_timeIn_ovl.c`, `m_warning_ovl.c`, `m_cpmail_ovl.c`, `m_cpwarning_ovl.c`, `ac_handOverItem.c`, `ac_boxManager.c`, `ac_boxMove.c`, `ac_boxTrick01.c`, `ac_buggy.c`, `ac_fallS.c`, `ac_fallSESW.c`, `ac_set_manager.c`

---

## Explicitly NOT porting (excluded)

- Debug builds: `m_debug.c`, `m_debug_display.c`, `m_debug_hayakawa.c`, `m_debug_mode.c`, `m_item_debug.c`
- System/boot: `main.c`, `game.c`, `graph.c`, `irqmgr.c`, `padmgr.c`, `executor.c`, `gamealloc.c`, `gfxalloc.c`, `m_malloc.c`
- Region variants: everything under `src/AUS/`
- Player select / save menu shells: `player_select.c`, `save_menu.c`, `first_game.c`, `second_game.c`, `zurumode.c`, `s_cpak.c`, `lb_reki.c`, `lb_rtc.c`, `c_keyframe.c`, `PreRender.c`, `THA_GA.c`, `TwoHeadArena.c`, `ev_cherry_manager.c`

---

## Suggested port order (opinionated)

Philip asked "what's next" before — if he asks again, the high-value gameplay systems still in C are:
1. **Snowman** — seasonal, well-bounded (`m_snowman.c`)
2. **Summer festival stalls** — `ac_yatai.c` + `ac_ev_yomise.c` (real stall behavior; the footrace files were a mislabel)
3. **Groundhog Day / April Fools / Birthdays** — small bounded event sets
4. **Leftover event pieces** — ball-toss scoring, fishing record DB, footrace controller (see section above)
5. **Bulletin boards / town map / diary** — small UI overlays
6. **NES games** — huge win if feasible, but the emulator core is the hard part
7. **GBA Island** — needs link-cable hardware behavior; defer

