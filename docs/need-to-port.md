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

## Villagers (core AI — partly ported)

The everyday villager behavior engine. Only the special-event visitors are ported
(see above); the 15 regular villagers' daily life is still largely in C.

✅ **Villager AI & schedules** (claude, 2026-10-12: npc_walk.rs, npc_set_manager.rs, quest_firstjob.rs, npc_life.rs; island/HP-mail parts of m_npc.c and C wiring deferred) — daily routines, schedule tables, walking, favors.
✅ **Schedule tables & talk cooldown** (muse, 2026-10-12) — the six personality schedule tables (verbatim, strict `end_time > now_sec` first-match lookup), the saved/current/forced-timer manager step, and the talk-cooldown temper table (verbatim, looks-indexed quirk preserved). → `villager_ai.rs`
`m_npc.c`, `m_npc_schedule.c`, `m_npc_walk.c`, `ac_set_npc_manager.c`, `m_quest.c`

Still in C: per-villager schedule/walk records, the Common schedule-slot bookkeeping flows, RNG call order in dialogue selection, favor acceptance/completion/reward flows.

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

✅ **Mail & letter generation (non-NPC)** — the data model, slot ops, routing, post-office
delivery and archive (`mail.rs`); Mom's scheduled letters, pools A/B/C (`mother_mail.rs`);
NPC event/birthday/Christmas/goodbye/password mail (`npc_event_mail.rs`); museum donation
remail kinds + message table (`museum.rs`); catalog orders + lottery special delivery
(`special_delivery.rs`); NPC letter-reply scorer + handbill composer (`npc_reply.rs`).
Still in C: the mailbox overlay UI, the letter-writing UI, and the ROM text resources.
`m_mail.c`, `m_mailbox_ovl.c`, `m_mail_check.c`, `m_mail_check_ovl.c`, `m_mail_password_check.c`

✅ **Mortgage repayment** — Nook's house-debt payment kernels: money-item scan,
max_repay, digit adjust with clamp/reject, preview fields, and the wallet/pocket
consumption routine (wallet-first, then denominations 100→30000). → `repay.rs`
(muse, 2026-10-12). Submenu flow, sounds, drawing, and the give-from-submenu
request stay in C.
`m_repay_ovl.c`

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
  event-manager return trees; the controller/NPC lifecycles stay in C. `props.rs`
  does exist (claude; it was unregistered from lib.rs until 2026-10-12), but it only
  documents `ac_goza` and has no goza-specific kernel, so the goza mat is still C)
- ✅ First shrine visit: `ac_hatumode_control.c`, `ac_hatumode_npc0.c` → `hatsumode.rs`
- ✅ Morning aerobics: `ac_taisou_npc0.c` → `taisou.rs`
- ✅ Tug of war: `ac_tunahiki_control.c`, `ac_tunahiki_npc0.c`, `ac_tunahiki_npc1.c` → `tunahiki.rs`
- ✅ Fishing tourney: `ac_turi.c`, `ac_turi_npc0.c`, `ac_ev_angler.c`, `m_fishrecord.c` → `fishing_tourney.rs`
- ✅ Sports fair ball toss: `ac_tamaire_npc0.c`, `ac_tamaire_npc1.c`, `ac_tama.c` → `tamaire.rs`
- ✅ Field Day footrace: `ac_tokyoso_control.c`, `ac_tokyoso_npc0.c`, `ac_tokyoso_npc1.c` → `tokyoso.rs`
  (note: these files are the footrace, NOT summer festival stalls — the old label here was wrong)
- ✅ Summer festival stalls (actual): `ac_yatai.c`, `ac_ev_yomise.c` → `yomise.rs` (muse)
  (stall *prop* kernels are in `holiday_scenery.rs` (claude); the vendor purchase kernels — price/message table,
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

Small, bounded follow-ups identified during the 2026-10-11 event-port batch.

### 🆕 New items (added 2026-10-11/12 — all resolved)

- ✅ **lbRTC_IsEqualDate little-endian type-pun bug** — RESOLVED 2026-10-12 (claude).
  Proven with mingw32: the packed `int` compare reverses every cross-month/cross-year
  ordering (Jan 31 compared OVER Feb 1); equality is unaffected. 9 of the 14 callers use
  the ordering (m_all_grow_ovl growth gate, m_event ×3, m_fishrecord ×3, m_soncho ×2,
  lbRTC's signed interval). Fix: `fixes/lb_rtc_IsEqualDate_little_endian.patch`
  (`#ifdef TARGET_PC`, explicit `year<<16|month<<8|day`), applied to the local
  ACGC-PC-Port checkout (uncommitted, since that repo is upstream). Compiles with the real
  build flags. The Rust model is `go_home_npc.rs` `pc_lbrtc_is_equal_date`.
- ✅ **Hatsumode OOB byte contents** — RESOLVED 2026-10-12 (claude), proven from the
  byte-matching decomp's US symbol map (no disc needed): `root_table[4]` = `proc_idx_table[0]`
  = **4**; `proc_idx_table[4]` = the top byte of `proc_table[0]`'s pointer = **0x80**
  (foresta.rel is head-allocated above the DOL); then `dt_tbl[0x80]` reads .data
  0x36CDC = `aHC_position_data` NEEDLEWORK `pos.z` (0.0f) + the high byte of the
  HOUSE_CLOCK profile id = **five zero bytes**: think NONE, init `none_proc1`, no talk.
  The fifth worshipper idles, so the OOB is benign. `hatsumode.rs` now returns these values.
- ✅ **20 test-expectation failures** — RESOLVED 2026-10-12 (claude, authorized run):
  **970/970 pass**. 16 were wrong test expectations (bank, catalog, cockroach, countdown
  digit mask, design, event_visitors, hatsumode, kamakura, lost_found, name_editor
  fixture, shop_interior ×2, snowman, stalk_market ×4 incl. f32 boundary effects,
  town_board). 4 were REAL implementation bugs, now fixed: countdown + fireworks
  snap-back used swapped axes (atan2(dz,dx) with sin/cos); fish_actor SALMON2 must fall
  through to KASEKI; the NG-word scan missed the C's extra start offset. Windows test
  linking needed test-only C/SDL stubs (`rust/src/test_c_stubs.rs`); run with
  `cargo test --lib --target i686-pc-windows-gnu`.

### ✅ Done (all of the recent event-port leftovers)

- ✅ **Summer festival stalls (actual)** — vendor purchase kernels in `yomise.rs` (price/message table, 8-entry stock init, choice→stock resolution, page scan, next-talk-action). Still in C: talk scripts, inventory/Bells mutation, mDemo handoff, NPC lifecycle, scheduling/placement, stall prop bg-offset/collision.
- ✅ **Ball-toss scoring** — no numeric team score exists anywhere in the decomp (aTMN1_flag is a turn flag); effect kernels in `tamaire.rs`.
- ✅ **Fishing record database** — pure record kernels in `fishrecord.rs` (sort is oldest-first, not newest-first).
- ✅ **Fishing tourney host & save areas** — Chip's host kernels in `fishing_tourney.rs` (entry records-and-ENDS, foreigner split).
- ✅ **Tug-of-war rope actor & orchestration** — rope kernels in `tunahiki.rs` (collision offsets, vertex deformation, fdtug trees).
- ✅ **Morning aerobics radio & group systems** — pure kernels in `taisou.rs` (radio availability, group drain, card checks, Tortimer decision tree).
- ✅ **Footrace controller** — controller kernels in `tokyoso.rs` (event constant 15/0xF; double setupAction preserved).
- ✅ **Hatsumode open question** — OOB read at index 4 proven (muse); byte values proven from the matching symbol map (claude, 2026-10-12): root 4, think 0x80, `dt_tbl[0x80]` = five zero bytes → benign idle; `pc_hatsumode_ready2` returns them.
- ✅ **Groundhog Day** — pure controller/NPC kernels in `groundhog.rs` (module count 168→169).
- ✅ **Birthdays** — `birthday_ovl.rs`, `birth_control.rs` (claude).
- ✅ **Event NPC placement** — `event_map_npc.rs`, `go_home_npc.rs` (claude).
- ✅ **Bulletin-board follow-ups** — batch trigger, board-write, save-check, and house-message kernels in `notice_overlay.rs` (8 new exports).
- ✅ **NES follow-ups** — nesinfo parser kernels, `special_zelda()` repair, acquisition pools in `famicom.rs` (8 new exports); `scene.rs` FamicomEmu `cleanup_fn` gap fixed.
- ✅ **Bee follow-ups** — easing/APPEAR/catch-request/CAUGHT/ant-scale kernels in `bee_ant.rs` (13 new exports; `angle2` does NOT wrap — brief corrected).
- ✅ **Fish follow-ups** — action-state tables (standard 7, special 11), bobber-sync protocol, draw tables in `fish_actor.rs` (5 new exports + data tables; UKI_ACTOR is 0x2C8 not 0x2B8; 0x1C08 is insect-only, not fish).
- ✅ **Gyroid follow-ups** — RESOLVED 2026-10-12 (claude) → `haniwa.rs`. HANIWA_START/END
  verified with gcc (0x15B0/0x17AB; name-table HANIWA_NUM is 127, the Rust `HANIWA_NUM` = 3
  is `mAGrw_HANIWA_NUM`). `FTR_NO_2_IDX(f) = f << 2`, so the random select only yields
  south-facing ids 0x15B0..0x17A8. DOREWO survives until the selected tag points elsewhere;
  the shown message lags one frame. CORRECTIONS: `mHW_MSG_MOU` IS used (literal 6 when the
  pockets are full), and the stored-slot turnip spoil is `mAGrw_SpoilAllPossession`, not
  SetDigItem. Buy flow: bags are spent smallest-first and change goes back to the wallet.
  QUIRK: the last candidate block is reused for the remaining gyroids.
- ✅ **Cockroach follow-ups** — RESOLVED 2026-10-12 (claude) → `cockroach.rs`. Growth runs
  at player-data load (`m_start_data_init.c`), not on house entry. Spawning moves roaches
  saved→visible; leaving adds the visible ones back; a stomp only decrements visible, so
  YES, stomping permanently lowers the saved count. `check_danna_kill_proc` =
  `aMR_CheckDannaKill`, registered in `aMR_SetClip`; BUG: it only tests FTR0 ids
  (0x1000..=0x2000), so 0x3000+ furniture never kills. QUIRK: an existing infestation adds
  the full day gap.
- ✅ **Balloon follow-ups** — RESOLVED 2026-10-12 (claude) → `balloon.rs`. A failed unit
  lookup keeps count 0, so the present is never dropped and the balloon never deleted. The
  little shake is one-sided (+500 / 0). The present draws unless escaping + dropped + timer
  moved (PC uses `<=`). The present resolves on pickup: 80% rare furniture, else
  `mFI_GetOtherFruit` (50% next-fruit bias; oranges only in peach towns). Stuck in a tree:
  18000 frames.
- ✅ **Town-map follow-ups** — RESOLVED 2026-10-12 (claude) → `town_map.rs`. The house-pos
  lookup rules are exact (0x03B8 terminator, 3 entries, `ut_z - 1`; a missing block falls
  back to the FIRST row's entry); the contents are still the absent asset. The max-no
  scan starts at (1,1) and covers 5×6. Retail vs PC cursor counters. Player slots skip the
  intro player and fill with "free". The villager sort restarts after each swap.
  `add_calc(0.7,19,1.8)` easing. Indoors with a pending door → exit door.
- ✅ **Diary follow-ups** — RESOLVED 2026-10-12 (claude) → `diary.rs`. Checksum = u16 word
  sum == 0. BUG: on the next-land load path the original-design block is copied into
  `l_keepDiary`, so a corrupt diary becomes design bytes. Speeds: 16/3, 16/7, ×2 / ×0.5
  scaling capped at 8 (read) / 2 (write). Panel `-0.6n²` overshoots to -101.4 for one frame.
  Exit line = min(lines, `_24`). Layout limited to 31 lines; trailing-newline and `i != 30`
  rules; empty-entry `text[-1]` read; 12 month textures listed.
- ✅ **Mushroom follow-ups** — RESOLVED 2026-10-12 (claude) → `mushroom.rs`. BUG: the
  player-acre exclusion removes the player's whole row AND column. The `possible_cols`
  refill is dead code (never decremented). Rows are picked uniformly; the "bruh" break ends
  the scan. Clears count quarter-hours in either direction (future stamps clear too).
  Startup uses acre (0,0) (nothing spared) with no season check. Column-0 trees wrap
  neighbour offsets.
- ✅ **Igloo follow-ups** — RESOLVED 2026-10-12 (claude) → `kamakura.rs`. PC `anim_frame`
  wraps at 1024; the steam accumulator is a shared function `static`. The chase factor is
  0.019532442×dt. The camper is 1 random resident (already in `event_map_npc.rs`). The mochi
  table is unused (1400-frame loop, recorded). Rumor event 2, window Jan 2 → Feb 23 20:00,
  told only in winter.
- ✅ **Snowman follow-ups** — RESOLVED 2026-10-12 (claude) → `snowman.rs`. Reward ids
  verified with gcc. `move_dist` lives in common-area part 0 (ball A) / 1, and is kept only
  for an untouched ball. The FG probe's NULL deref is modelled as a reject. Arc
  `(8t − t²/7.5)·25` (peak 3000 at t=30), 5 dust puffs −90°..90°. `snowman_start` blocks on
  SNOWMAN0/3/6 + born check; new balls appear from 06:00.

### ⏳ Open (unclaimed)

- ✅ **Insect follow-ups** — RESOLVED 2026-10-12 (claude) → `insect_actor.rs`. All 14
  behaviour-program state machines are generated by `tools/gen_insect_fsm.py` (`PROGRAM_FSM`:
  actions, transitions incl. computed `setupAction` args, initial and forced LET_ESCAPE).
  Net handoff: per-frame catch requests (label = actor), the player picks the label, and the
  pocket item is `insect->item` (= ITM_INSECT_START + type; bee → INSECT08, ant → INSECT38).
  Spirits stack into [SPIRIT0, SPIRIT0+5−count). The flag bits are now named (DESTRUCT,
  NO_CATCH, FLEEING, LIFE_OVER, WALL_CHECK) and every family's work slots are documented.
  Spawn metadata: unit centre, MAKE_NEW; extra_data 0 / 1 released / 4 goki-tree / 6
  goki-turnip. `aINS_overlay_entry_c` (0x1C08) is dead N64 overlay bookkeeping, NOT model
  tables. chou flower search: ±1 then ±2 retry, units 1..14, and RNG order 1 fqrand +
  2 (+2) RANDOM(3).

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

🔄 **Weather system** (claimed by claude 2026-10-12, `port-nature-weather-assessment`) — rain/snow/leaf/sakura weather selection and the weather actors.
`m_kankyo.c`, `m_kankyo_weather.c_inc`, `ac_weather.c`, `ac_weather_fine.c`,
`ac_weather_leaf.c`, `ac_weather_rain.c`, `ac_weather_sakura.c`

🔄 **Field assessment (perfect town)** (claimed by claude 2026-10-12, `port-nature-weather-assessment`) — the town-beauty rating logic.
`m_field_assessment.c`

🔄 **Grass wear** (claimed by claude 2026-10-12, `port-nature-weather-assessment`) — grass deterioration under the player's feet.
`m_player_main_remove_grass.c_inc`

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
- ✅ Camera: `m_camera2.c`, `m_view.c` → `camera.rs`, `view.rs` (per-mode camera state machines stay in C) (claude)
- ✅ Lighting: `m_lights.c` → `lights.rs` (claude)
- ✅ Controller/rumble: `m_controller.c`, `m_vibctl.c` → `controller.rs` (claude)
- ✅ Actor shadows: `m_actor_shadow.c` → `actor_shadow.rs` (claude)
- ✅ Actor/game tables: `m_actor_dlftbls.c`, `m_game_dlftbls.c` → `engine_tables.rs` (game table already in `scene.rs`) (claude)
- ✅ BG items/textures: `m_bg_item.c`, `m_bg_tex.c`, `m_land.c` → `engine_tables.rs`, `game_string.rs` (`m_bg_tex` is BSS layout only) (claude)
- ✅ Clipping: `m_clip.c` → `engine_tables.rs` (clip table reset) (claude)
- ✅ String/font core: `m_string.c` → `game_string.rs` (font tables are ported in `npc_reply.rs`) (claude)
- ✅ Intro demo: `ac_intro_demo.c`, `ac_present_demo.c`, `ac_ride_off_demo.c` → `demo_actors.rs` (claude)
- ✅ Field drawing: `ac_field_draw.c`, `ac_fieldm_draw.c`, `ac_effectbg.c` → `field_effects.rs`, `engine_tables.rs` (claude)
- ✅ Misc (→ `hud.rs`, `game_string.rs`, `engine_tables.rs`, `menu_overlays.rs`, `room_scoring.rs`, `hand_over_item.rs`, `field_effects.rs`; `m_tag_ovl` UI stays C apart from its bell-bag rule): `m_watch_my_step.c`, `m_eappli.c`, `m_prenmi.c`, `m_banti.c`, `m_hand_ovl.c`, `m_huusui_room.c`, `m_huusui_room_ovl.c`, `m_mark_room.c`, `m_mark_room_ovl.c`, `m_tag_ovl.c`, `m_timeIn_ovl.c`, `m_warning_ovl.c`, `m_cpmail_ovl.c`, `m_cpwarning_ovl.c`, `ac_handOverItem.c`, `ac_boxManager.c`, `ac_boxMove.c`, `ac_boxTrick01.c`, `ac_buggy.c`, `ac_fallS.c`, `ac_fallSESW.c`, `ac_set_manager.c` (claude)
- **Inventory UI** — the pockets/inventory management overlay. `m_inventory_ovl.c`
- **Submenu framework** — the generic submenu system backing many overlays. `m_submenu.c`, `m_submenu_ovl.c`
- **Save/flash system** — intentionally C-side (hardware save writes). `m_flashrom.c`

---

## Explicitly NOT porting (excluded)

- Debug builds: `m_debug.c`, `m_debug_display.c`, `m_debug_hayakawa.c`, `m_debug_mode.c`, `m_item_debug.c`
- System/boot: `main.c`, `game.c`, `graph.c`, `irqmgr.c`, `padmgr.c`, `executor.c`, `gamealloc.c`, `gfxalloc.c`, `m_malloc.c`
- Region variants: everything under `src/AUS/`
- Player select / save menu shells: `player_select.c`, `save_menu.c`, `first_game.c`, `second_game.c`, `zurumode.c`, `s_cpak.c`, `lb_reki.c`, `lb_rtc.c`, `c_keyframe.c`, `PreRender.c`, `THA_GA.c`, `TwoHeadArena.c`, `ev_cherry_manager.c`

---

## Suggested port order (opinionated)

Philip asked "what's next" before — if he asks again, the current state is in
"Leftover pieces" above (all resolved) plus the newly listed open systems:
villager core AI, the mail/letter system, mortgage repayment, weather,
field assessment, grass wear, the inventory UI, and the submenu framework.
Beyond that:
1. **GBA Island** — needs link-cable hardware behavior; defer
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
