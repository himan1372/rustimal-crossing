# Need to Port — Animal Crossing GameCube Rust Rewrite

**Status as of 2026-10-11 (evening)**

- **Ported to Rust:** 169 modules in `rust/src/` (plus `lib.rs`; decomp-verified against GAFE01_00 Rev 0 USA)
- **Still in C:** everything below — grouped into game systems with plain-English names.
- 2026-10-11 (claude): marked birthdays, event NPC placement, holiday scenery, buildings & scenery,
  household radio, calendar overlay and the engine-level batch as ported.
- 2026-10-11 (muse): module count corrected 140 → 167 (includes support/infrastructure
  modules the entries below don't name individually); refreshed the suggested port order.
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
- ✅ Groundhog Day: `ac_groundhog_control.c`, `ac_groundhog_npc0.c` → `groundhog.rs`
  (time terms, clock predicates, message-index math, animation timing, spawn gates,
  event-manager return trees; the controller/NPC lifecycles stay in C; there is no
  `props.rs` anywhere in the repo — the brief's goza-mat claim was unfounded)
- ✅ First shrine visit: `ac_hatumode_control.c`, `ac_hatumode_npc0.c` → `hatsumode.rs`
- ✅ Morning aerobics: `ac_taisou_npc0.c` → `taisou.rs`
- ✅ Tug of war: `ac_tunahiki_control.c`, `ac_tunahiki_npc0.c`, `ac_tunahiki_npc1.c` → `tunahiki.rs`
- ✅ Fishing tourney: `ac_turi.c`, `ac_turi_npc0.c`, `ac_ev_angler.c`, `m_fishrecord.c` → `fishing_tourney.rs`
- ✅ Sports fair ball toss: `ac_tamaire_npc0.c`, `ac_tamaire_npc1.c`, `ac_tama.c` → `tamaire.rs`
- ✅ Field Day footrace: `ac_tokyoso_control.c`, `ac_tokyoso_npc0.c`, `ac_tokyoso_npc1.c` → `tokyoso.rs`
  (note: these files are the footrace, NOT summer festival stalls — the old label here was wrong)
- ✅ Summer festival stalls (actual): `ac_yatai.c`, `ac_ev_yomise.c` → `yomise.rs` (muse)
  (stall *prop* kernels are in `holiday_scenery.rs`; the vendor purchase kernels — price/message table,
  stock init, nonempty count, choice→stock resolution, page scan, next-action selection — are in `yomise.rs`;
  SP_NPC_EV_YOMISE, SP_NPC_EV_YOMISE2, FIREWORKS_STALL0, FIREWORKS_STALL1;
  save-area/RNG/inventory/Bells/demo/NPC-lifecycle/message/event-manager pieces stay in C)
- ✅ April Fools: `ac_aprilfool_control.c` → `aprilfool.rs` (NPC→index table, message-ID table, per-player bitfields, talk-check/talk-set/get-message logic; actor lifecycle, event-manager activation, save-area allocation, and NPC talk state machines stay in C)
- ✅ Birthdays: `m_birthday_ovl.c`, `ac_birth_control.c` → `birthday_ovl.rs`, `birth_control.rs` (claude)
- ✅ Event NPC placement: `m_event_map_npc.c`, `ac_go_home_npc.c` → `event_map_npc.rs`, `go_home_npc.rs` (claude)

✅ **Holiday scenery actors** — carp streamers, festival stalls, shrine, lotus. → `holiday_scenery.rs` (claude)
`ac_koinobori.c`, `ac_yatai.c`, `ac_shrine.c`, `ac_lotus.c`, `ac_tukimi.c`
(`ac_yatai.c` is the stall *prop* — its kernels are ported here; the `ac_ev_yomise.c` stall NPCs are still C, see the leftover section below.)

---

## Leftover pieces from the recent event ports (still in C)

Small, bounded follow-ups identified during the 2026-10-11 event-port batch. Each is a
candidate for a future pure-kernel port once its boundary is verified.

- **Summer festival stalls (actual)** — vendor purchase kernels now in `yomise.rs`
  (price/message table, 8-entry stock init from injected RNG kind, nonempty counting,
  choice→stock resolution with the choice-3 sentinel, page scan with fallback literals,
  next-talk-action selection; exact purchase side-effect order documented). Still in C:
  talk scripts/message resources, inventory/Bells mutation, the mDemo_* handoff protocol,
  NPC lifecycle, event scheduling/placement/manager callbacks, and the stall prop's
  bg-offset/collision pieces.
- **Ball-toss scoring** — RESOLVED: no numeric team score exists anywhere in the decomp
  (exhaustive trace — no save area, no counters, no dialogue/flag changes on a made basket;
  aTMN1_flag is a turn flag, max 3). The `ef_tamaire.c` effect kernels are now in `tamaire.rs`
  (launch velocity, ground-outcome classifier, DUMMY_TURI attraction expiry, `powf(0.5f,dt)`
  bounce damping, 0x155 loud-bounce gate, basket variant). Sounds, RNG, trig tables,
  ground queries, and the effect lifecycle stay in C.
- **Fishing record database** — RESOLVED: pure record kernels are in `fishrecord.rs`
  (formulas, class selection, strict winner compare, discarded-draw passthrough,
  sim loop continue/step-count/hour, sort-swap decision, oldest-date recycle,
  first-fit free slot, same-date search, mail-template index, save-validation
  predicate, NPC land-name bytes). Still in C: 5-slot insert/cleanup/recycle
  against save storage, `mEv_fishRecord_local` orchestration, holder1/2
  (slot-zero behavior), `last_holder`, `mFR_fishmail`, mail construction/delivery
  with delete-on-success, the notice-scheduler call site. Verified 12/13; one
  DIFFERS correction: the sort is OLDEST-first (not newest-first), which explains
  the slot-zero winner anomaly. Future: `mEv_fishday` contest-date arithmetic.
- **Fishing tourney host & save areas** — RESOLVED: Chip's host kernels are now in
  `fishing_tourney.rs` (talk-init tree, initial messages, entry check/set with the
  foreigner-vs-local split, float→int→s16 position conversion, June-vs-fall event
  selection, 10 talk-action constants). Still in C: area allocation/zeroing/lifetime,
  NPC lifecycle, demo/talk framework, inventory/Bells mutation, RNG, message resources.
  Open: the `flag` field's purpose (zero references); the area-clearing lifecycle
  (generic 5-slot recycling only).
- **Tug-of-war rope actor & orchestration** — RESOLVED: rope kernels are in
  `tunahiki.rs` (there is no `props.rs` anywhere in the repo): three-unit
  collision-offset X positions, vertex fact + 60-entry flag table + deformed
  vertex X, `fdtug_start`/`stop`/`in` return trees, event/area/place/state
  constants. Still in C: rope actor lifecycle + draw + display lists, controller
  save-area access, NPC schedules + clip + creation/deletion, fdtug
  placement/fades/status, all animation RNG + effects/audio. Corrections: the
  rope's attr 100 IS `mCoBG_ATTRIBUTE_NONE` (never writes the attribute); the
  talk-freeze is ANY-single-flag (`(flag & TALK_ALL) == 0`); the tug sound is
  member-gated, not team-gated.
- **Morning aerobics radio & group systems** — RESOLVED: pure kernels are in
  `taisou.rs` (radio availability predicate, raw-counter → measure/progress
  conversion, tempo/48, leader-cull predicate, regist drain/reset, failure-type
  index, radio-time check, season-limit conjunction, card-ID derivations, the
  full Tortimer card-progression decision tree + messages + talk-proc selection).
  Still in C: audio group lookup, the four group globals, NPC clip, per-NPC
  `Radio_c`, ratio tables/state machine, RNG + arbitration, animation/effects,
  Tortimer's inventory mutations, `radiocard` save data, calendar storage,
  demo/talk/handover. Corrections: `Radio_c` is padded (0x000/0x004/0x008, not
  the brief's 0x000/0x001/0x005); `FTR_START(FTR_RADIO_TEST)` is 0x1FCC
  (radio.rs's 1011 is the furniture table index).
- **Footrace controller** — RESOLVED: controller kernels are in `tokyoso.rs`
  (action enum, `_00==3`→WAIT2 override, set-start predicate, `_01` set/clear
  with the `(2+idx)&3`/`idx&3` asymmetry, kansen leave predicates, dash/stumble
  rolls, kokeru-done, late-race predicate, goal-think mapping, NPC0 birth reset,
  NPC1 birth/self-delete phases, timer/event/slot/mask constants). `aTKC_FLAG_15`
  is PROVEN unused (declaration only, zero `0x8000` literals). Still in C:
  controller lifecycle + setupAction, save-area writes, NPC clip, animation
  init, effect creation, all RNG calls, the BUGFIXES branch, the double
  setupAction call order. Corrections: event constant is 15 (0xF), not 0xE;
  `_00=2` exists (NPC0 birth sets it, NPC1 birth requires it); NPC0's destructor
  ASSIGNS flags (wipes bits); NPC1's `_00==3` self-delete is in its move callback.
- **Hatsumode open question** — RESOLVED 2026-10-11: `root_table`/`proc_idx_table`
  really do have only 4 entries (`{ 5, 4, 9, 3 }` / `{ 4, 27, 27, 20 }`), index 4 really is
  reachable (HATUMODE_4 = 0xD05C − 0xD058 = 4, signed arithmetic), and retail PROVABLY
  performs a 1-byte out-of-bounds read on each table in `aHN0_ready2` on every normal
  run — the sole read site guards only the save area (and retries), never the index.
  `pc_hatsumode_ready2` keeps its "reports invalid" behavior (OOB values unknowable
  without the binary; UB in Rust too). Only the OOB bytes' *contents* remain unconfirmed
  (no retail binary here).
- **Groundhog Day** — RESOLVED: pure controller/NPC kernels are in `groundhog.rs`
  (time-term buckets, TIME_CHECK predicate, initial-action selection, message-index
  computation with the term-decrement order, discrete frame-speed values, event-idx
  tables, Tortimer's think tree, `groundhog_start`/`stop`/`in` return trees).
  Still in C: controller lifecycle + scene/fade, common-area allocation, NPC clip,
  the MAJIN actor + retire handshake, Tortimer's speech machine, BGM/camera, all
  spawning, event-manager callbacks. Corrections: `RANDOM(3.0f)` ∈ {0,1,2} so the
  frame speed takes only {0.4, 0.65, 0.9}; Tortimer's think DEFAULT is 4, not 0;
  the NPC reads the term from the controller clip; `groundhog_in` re-creates the
  controller when the common word == 1. The brief's `props.rs`/goza-mat premise
  was unfounded — no such file exists in the repo.
- ✅ **Birthdays** — `m_birthday_ovl.c`, `ac_birth_control.c` → `birthday_ovl.rs`, `birth_control.rs` (claude; April Fools control kernels are in `aprilfool.rs`).
- ✅ **Event NPC placement** — `m_event_map_npc.c`, `ac_go_home_npc.c` → `event_map_npc.rs`, `go_home_npc.rs` (claude).
- **Bulletin-board follow-ups** — RESOLVED 2026-10-11: batch trigger gates on
  title-demo-none + foreground field, then the nonzero auto_nwrite_year guard; the
  cursor/checkpoint update runs AFTER the write decision EVEN WHEN NO POSTS WERE
  WRITTEN (do not move it inside the write branch). `mNtc_TO_MONTHDAY` proven
  verbatim; handbill formulas proven (standard `0x1A4 + id`, fishing-day
  `0x242 + ((candidate.day / 7) & 3)`); post timestamp sec/min=0, hour=6 proven;
  `mNtc_notice_write_num` uses field-wise `lbRTC_IsEqualTime(..., CHECK_ALL)`,
  not a raw byte compare; `mNtc_notice_write` shifts posts down one when full;
  `mNtc_ClearPage` assigns the clear-code struct and fills the message with
  CHAR_SPACE; `sChk_board_save_check` rejects message bytes 127/128
  (control code / message tag). House message: NO save-time validator or
  normalizer exists anywhere in the tree (settled NOT FOUND) — editor
  constrains new input only; do not add a Rust sanitizer. All pure kernels
  ported into `notice_overlay.rs` (+8 exports).
- **NES follow-ups** — RESOLVED 2026-10-11: tag-stream format proven
  (NESTAG_CMD_SIZE=3, NESTAG_SIZE=4 — the brief conflated them; traversal
  formula unaffected); VEQ/VNE skip the next tag; HSC default len =
  tag_len-2; update_highscore HSC offset word packs a flag bit
  (ofs=&0x7FF, bit=>>15&1); Baseball `GNM_TAG(6)` for 8-byte "BASEBALL"
  and Mahjong `GNM_TAG(6)` for 7-byte "MAHJONG" are source-marked @BUG —
  do NOT correct. `special_zelda()` proven verbatim (markers + 3-slot
  checksum, conditional writeback only, no wholesale slot clear); it runs
  only in process1's load path (mode 1, SPE=1, first_play==FALSE).
  Acquisition pools proven with NO differences (00/01/04/06 shop-A,
  02/03/05/13 lottery, 07/09/10/11/12 homepage, 08/14 island, 15-18
  special-present). 20th table entry proven: `FTR_KOB_DISKSYSTEM8` →
  Excitebike texture/palette, game 14, AGB 0xFF; reference table @unused.
  scene.rs `FamicomEmu` `cleanup_fn: None` gap CONFIRMED and FIXED to
  `Some("famicom_emu_cleanup")`. 8 new pure exports in `famicom.rs`.
- **Bee follow-ups** — CALC_EASE short-angle wraparound; appear rotation.z increment
  and scale easing; position integration ownership; force-catch/table-catch request
  events; `disappear_timer > 1.0` fallback in CAUGHT; ant scale chase; the exact
  honeycomb actor callbacks and their update ordering.
- **Fish follow-ups** — the full test/kaseki state machines and their complete transition
  tables; exact UKI_ACTOR status synchronization around the catch animation
  (`aGTT_comeback`/`aGKK_comeback`); the species-indexed `aGYO_hosei_y[]` and
  animation-pattern tables; `aGYO_overlay_entry_c` (0x1C08) model tables.
- **Insect follow-ups** — per-family transition tables for the 14 behavior programs;
  exact net-hit/inventory handoff; per-family work-slot and flag semantics;
  how the Rust spawn tables pass type/position/release metadata into the C controller;
  `aINS_overlay_entry_c` (0x1C08) model tables; chou flower-search offset tables
  and exact RNG call order.
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
- **Town-map follow-ups** — `mMP_house_pos_list` contents live in a binary `.inc`
  asset absent from the decomp (matching algorithm ported, contents injected);
  the PC-port dt-accumulating `cursor_frame` f32 vs the N64 `++`/`%=` form;
  `mMP_make_max_no_table` acre-loop bounds (starts at field index (0,1), scan
  count); the full resident-cache assembly (`mPr_CheckPrivate`, the
  first-introduction condition, `mNpc_CheckFreeAnimalInfo`, name conversion,
  the swap-and-restart villager sort); the `add_calc(0.7, 19.0, 1.8)` cursor
  easing; exit-door vs player-position selection in `mMP_set_init_data`.
- **Diary follow-ups** — the card-load repair path (`mFRm_ReturnCheckSum`
  gate) for a corrupt diary checksum; the `chase_f()` scroll easing and
  distance-scaled speeds; the read→write scroll's quadratic `_28`
  animation to exactly -100.0f; `mDI_Play_write`'s editor-state copy
  (`_22 → _48`, `_24 → _46`) and `_18 = min(lines, _24)` clamp; the
  `mDI_set_writing_body` end-of-text `i != 30` special case and
  trailing-newline `line_len--` rules; the full 12-entry `month_tex_table`
  texture references.
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

✅ **Insect actors** — the 14 individual bug actors. → `insect_actor.rs` (shared
controller kernels: 41-type program dispatch, catch range, stress band, patience,
fade/alpha, despawn predicate, shadow sizes; the 14 family state machines stay in C)

✅ **Fish actors** — shadows, release behavior. → `fish_actor.rs` (gyoei controller
kernels: size/search/bite tables, behavior dispatch, release math, kage alpha/ripple,
flee predicate; test/kaseki state machines stay in C)

✅ **Fish & bug records** — fishing-contest record kernels. → `fishrecord.rs`
`m_fishrecord.c` (five dated size records, NPC rival generation, winner simulation inputs; NOT the museum encyclopedia — the collection tracker is in `museum_display.rs`; persistent record slots, RTC scheduling, and reward mail stay in C)

✅ **Bees** — the bee-tree shake behavior. → `bee_ant.rs` (tree-shake decision
helpers: eligibility, retry window, shock/status frames, force-catch; audit-fixed
type ids, status values, s16 target frame, 8.9f speed; ghog hive visual stays in C)

---

## Big-ticket systems

✅ **NES games** — the Famicom emulator and unlockable NES titles. → `famicom.rs`
(furniture/metadata tables, save-header validation, exit wrapper, retail-form
fbdemo transition math; the 6502 core and wrapper stay in C)

**GBA Island** — link-cable island, minigames, islanders.
`m_island.c`, `m_gba_ovl.c`, `ac_airplane.c`

✅ **Bulletin boards** — town board and house board posts. → `town_board.rs`
(15-slot record kernels: first-empty scan, full-board shift, page clear, init
handbills, auto-post time) + `house_board.rs` (128-byte message line check)

✅ **Town map** — the map overlay UI.
`m_map_ovl.c` → `town_map.rs` (108-entry texture/palette tables,
bridge substitution, layer conversion, house-pos match, building-label
precedence, cursor selection/animation kernels). Interior is 5x6 = 30 acres.

✅ **Diary** — the player diary.
`m_diary.c`, `m_diary_ovl.c` → `diary.rs` (992-byte entries, backward
length scan, space init, entry indexing, width-192 wrap scan,
line-check state machine, scroll limits, input-repeat timing,
month adjustments, confirmation branch).

**Name editor** — the character-entry screen, including the NG-word filter (this is where 'phili' got rejected; fix options already offered to Philip). ✅ → `name_editor.rs`
`m_editor_ovl.c`, `m_editEndChk_ovl.c`, `m_ledit_ovl.c`

✅ **Passwords** — secret codes (make/check). → `password.rs`
`m_passwordChk_ovl.c`, `m_passwordMake_ovl.c` (21-byte payload → 28-char codec, payload unpack/checksum, line editor, make/check init, mail-body extraction; stateful shop/reward/inventory/RNG pieces stay in C)

✅ **Notices & handbills UI** — notice overlays. → `notice_overlay.rs`
`m_notice.c`, `m_notice_ovl.c` (navigation, stick classification, texture mapping, line/page helpers, 43-entry scheduler, sorting, treasure gates; the handbill *composer* is in `npc_reply.rs`, the ROM loader and stateful scheduling stay in C)

✅ **Address book** — the contacts UI. → `address_book.rs`
`m_address_ovl.c` (player/NPC list rebuild on open, page bitfields, recipient index resolution, B behavior; save/identity/editor/display-list integration stays in C)

✅ **Pause menu** — pause overlay. → `pause.rs`
`m_pause.c` (NOTE: this is a debug input/update gate, not a visual pause menu — Z+R held + R-newly-pressed accumulates a timer past 8.0f, then R+D-down toggles; `Pause_proc` returns 1 to proceed / 0 to gate; game-loop wiring, pad polling, and dt sourcing stay in C)

✅ **Selection/choice UI** — generic list and yes/no prompts. → `choice.rs`
`m_choice.c` (option admission, clamped cursor, dt-based auto-repeat, B-to-last shortcut, determination commit; `m_select.c` is a separate debug scene selector — only its index wraparound is ported)

✅ **Calendar overlay** — the calendar UI. → `calendar.rs` (claude)
`m_calendar_ovl.c`, `m_calendar.c` (save bookkeeping, 36-month grid, fixed/floating events, icons, page flips, day cursor, diary hand-off; drawing stays in C)

---

## ✅ Buildings & scenery actors

Town structures and decorative actors, ported by claude: → `buildings.rs` (shops, lighthouse +
switch, cottages, tent, statues, windmill, structure table), `props.rs` (signs, mailbox, flag,
raffle drum, harvest tables, e-Reader terminal, title logo, sports-fair props, misc) and
`kk_credits.rs` (`ac_mikanbox`: K.K. show roll, song pick, cameo timer). Logic/data only;
models and display lists stay in C.
`ac_conveni.c` (convenience store), `ac_cottage.c`, `ac_tent.c`, `ac_toudai.c` (lighthouse), `ac_lighthouse_switch.c`, `ac_windmill.c`, `ac_douzou.c` (statues), `ac_mikanbox.c` (orange box), `ac_mikuji.c` (fortune), `ac_nameplate.c`, `ac_sign.c`, `ac_mailbox.c`, `ac_structure.c`, `ac_mural.c`, `ac_kago.c` (palanquin), `ac_dump.c` (dump), `ac_tama.c`, `ac_rope.c`, `ac_ball.c`, `ac_goza.c`, `ac_garagara.c`, `ac_misin.c`, `ac_mbg.c`, `ac_pterminal.c`, `ac_reserve.c`, `ac_sample.c`, `ac_super.c`, `ac_dummy.c`, `ac_flag.c`, `ac_htable.c`, `ac_animal_logo.c`, `ac_animal_logo_misc.c`

✅ **Household actors** — radio, etc. → `radio.rs` (claude)
`ac_radio.c` plus the aerobics radio furniture and the stereo K.K. disc playback / room BGM
(`ac_my_room` `aMR_MiniDiskCommon*`; songs are `BGM_MD0 + n`)

---

## Engine-level (ported only in part, or intentionally C-side)

These are partially covered (`frame_loops.rs`, `scene.rs`, `graphics.rs`). The logic/data of the files
below was ported by claude (2026-10-11); display-list emission and the per-mode camera procs stay in C.
- ✅ Camera: `m_camera2.c`, `m_view.c` → `camera.rs`, `view.rs` (per-mode camera state machines stay in C)
- ✅ Lighting: `m_lights.c` → `lights.rs`
- ✅ Controller/rumble: `m_controller.c`, `m_vibctl.c` → `controller.rs`
- ✅ Actor shadows: `m_actor_shadow.c` → `actor_shadow.rs`
- ✅ Actor/game tables: `m_actor_dlftbls.c`, `m_game_dlftbls.c` → `engine_tables.rs` (game table already in `scene.rs`)
- ✅ BG items/textures: `m_bg_item.c`, `m_bg_tex.c`, `m_land.c` → `engine_tables.rs`, `game_string.rs` (`m_bg_tex` is BSS layout only)
- ✅ Clipping: `m_clip.c` → `engine_tables.rs` (clip table reset)
- ✅ String/font core: `m_string.c` → `game_string.rs` (font tables are ported in `npc_reply.rs`)
- ✅ Intro demo: `ac_intro_demo.c`, `ac_present_demo.c`, `ac_ride_off_demo.c` → `demo_actors.rs`
- ✅ Field drawing: `ac_field_draw.c`, `ac_fieldm_draw.c`, `ac_effectbg.c` → `field_effects.rs`, `engine_tables.rs`
- ✅ Misc (→ `hud.rs`, `game_string.rs`, `engine_tables.rs`, `menu_overlays.rs`, `room_scoring.rs`, `hand_over_item.rs`, `field_effects.rs`; `m_tag_ovl` UI stays C apart from its bell-bag rule): `m_watch_my_step.c`, `m_eappli.c`, `m_prenmi.c`, `m_banti.c`, `m_hand_ovl.c`, `m_huusui_room.c`, `m_huusui_room_ovl.c`, `m_mark_room.c`, `m_mark_room_ovl.c`, `m_tag_ovl.c`, `m_timeIn_ovl.c`, `m_warning_ovl.c`, `m_cpmail_ovl.c`, `m_cpwarning_ovl.c`, `ac_handOverItem.c`, `ac_boxManager.c`, `ac_boxMove.c`, `ac_boxTrick01.c`, `ac_buggy.c`, `ac_fallS.c`, `ac_fallSESW.c`, `ac_set_manager.c`

---

## Explicitly NOT porting (excluded)

- Debug builds: `m_debug.c`, `m_debug_display.c`, `m_debug_hayakawa.c`, `m_debug_mode.c`, `m_item_debug.c`
- System/boot: `main.c`, `game.c`, `graph.c`, `irqmgr.c`, `padmgr.c`, `executor.c`, `gamealloc.c`, `gfxalloc.c`, `m_malloc.c`
- Region variants: everything under `src/AUS/`
- Player select / save menu shells: `player_select.c`, `save_menu.c`, `first_game.c`, `second_game.c`, `zurumode.c`, `s_cpak.c`, `lb_reki.c`, `lb_rtc.c`, `c_keyframe.c`, `PreRender.c`, `THA_GA.c`, `TwoHeadArena.c`, `ev_cherry_manager.c`

---

## Suggested port order (opinionated)

Philip asked "what's next" before — if he asks again, the high-value gameplay systems still in C are:
1. **Bulletin-board follow-ups** — RESOLVED 2026-10-11 (batch trigger, board
   writes, save-check, house-message validation all proven; pure kernels ported)
2. **Hatsumode open question** — RESOLVED 2026-10-11 (retail OOB read at index 4 proven)
3. **GBA Island** — needs link-cable hardware behavior; defer
   (`m_island.c`, `m_gba_ovl.c`, `ac_airplane.c`)

Already done from the old list: Snowman ✅, April Fools ✅, Birthdays ✅, Passwords ✅, NES games ✅, Summer festival stalls ✅.

- **Password follow-ups** — the stateful check-overlay pre-fill from
  `menu_info->data2` (not spaces); the POPULAR `hit_rate = 4` checksum-bit quirk;
  the MAGAZINE `npc_type`→`hit_rate_index` bit fold; the full shop reward flow
  (RNG calls/order, inventory mutation, item checks, identity lookups) stays C;
  the password-make Famicom title list is external data.
- **Notice follow-ups** — the handbill ROM loader stays in C; stateful scheduling
  (RTC/save writes), fishing mail, RNG calls, treasure placement, and quest
  mutation stay in C; the brief omitted schedule entries 0x18–0x1A.
- **Address book follow-ups** — save writes, identity lookups, the name editor,
  and display-list construction stay in C; the address list is rebuilt on open,
  not persisted.
- **Pause follow-ups** — the C-owned debug gate and game-loop wiring
  (`ZURUMODE2_ENABLED()`, pad polling, dt sourcing) stay in C; `m_pause.c` is a
  debug input/update gate, NOT a visual pause menu; `Game_play_IS_PAUSED` is
  dead in the checked tree.
- **Choice follow-ups** — message-window embedding, font width measurement,
  ROM/ARAM string loading, control-code substitution, the `mMsg` control-code
  dispatch, sounds, graphics/display lists, matrix setup, and easing stay in C;
  all of `m_select.c`'s scene launching stays in C; the
  `SELECT_COURSE_NUM_VISIBLE` 14 vs draw-loop 15 discrepancy is preserved as-is.
