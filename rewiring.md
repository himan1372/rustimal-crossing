# Rewiring

**What this means, plainly:** every Rust module in this repo is a finished, tested
replacement part sitting on a shelf. "Rewiring" is the job of opening up the
game's C code and plugging those parts in — replacing a C function call with a
call into the matching Rust function — then proving the game still behaves the
same. Only one part is plugged in today (the letter scorer). Everything below
is still on the shelf.

## The one working example

`rust/src/letter_score.rs` → `mMck_check_key_hit` / `mMck_check_key_hit_nes`
is called from the C mail-check code (`src/game/m_mail_check.c`,
`m_mail_check_ovl.c`, `m_npc.c`). That is the pattern to repeat: find the C
call site, swap the call, rebuild, verify behavior.

## Inventory — every system that needs a C call

Status for all rows below: **Rust done, C call not made.** Ordered by
suggested integration wave (easiest/safest first).

### Wave 1 — Stateless pure functions (no game state, lowest risk)

These take numbers in and return numbers. They cannot corrupt game state,
so they are the safest first plugs.

| Rust module | C function(s) | C call site idea |
|---|---|---|
| `wall_priority.rs` | `pc_make_tab_2_move_tail` | `mCoBG_MakeTab2MoveTail` in `m_collision_bg.c` |
| `segment_map.rs` | `pc_unit_no_name_2_start_end` | `mCoBG_UnitNoName2StartEnd` in `m_collision_bg_wall.c_inc` |
| `attr_walls.rs` | `pc_forbid_vectors`, `pc_forbid_gate` | `mCoBG_MakeForbidVectorData` / `mCoBG_MakeUnitVector` gate |
| `columns.rs` | `pc_column_recipe` | `mCoBG_MakeOneColumnCollisionData` item chain |
| `wall_solver.rs` | `pc_judge_wall_from_vector`, `pc_distance_dispatch` | `mCoBG_JudgeWallFromVector`, distance dispatch in `m_collision_bg.c` |
| `bg_check.rs` | `pc_bg_neighborhood`, `pc_bg_distance_reverse`, `pc_bg_room_scope` | neighborhood/range helpers in `m_collision_bg.c` |
| `inventory.rs` | `pc_inventory_find`, `pc_inventory_count` | `mPr_GetPossessionItemIdx` in `m_private.c` |
| `dialogue_topics.rs` | `pc_topic_talk_check`, `pc_msg_max` | talk-gate checks in `ac_npc_talk.c_inc` / `m_npc.c` |
| `house_scene.rs` | `pc_request_proc_id`, `pc_house_wall_floor` | house-scene setup in `m_npc.c` |
| `scene_layout.rs` | `pc_scene_word_type`, `pc_door_next_scene` | scene interpreter in `m_scene.c` |

### Wave 2 — Functions needing game structs (translate C structs at the boundary)

| Rust module | C function(s) | C call site idea |
|---|---|---|
| `item_prefs.rs` | `pc_npc_house_goods`, `pc_eligible_furniture_count` | furniture selection in `m_npc.c` |
| `request_selector.rs` | `pc_request_pick_carried`, `pc_request_dispatch` | quest-talk init in `ac_quest_talk_normal_init.c` |
| `player_move.rs` | `pc_turn_mod`, `pc_locomotion_core` | player movement in `m_player*.c_inc` |
| `movement.rs` | `pc_wander_choice`, `pc_friendship_mode` | NPC wander/friendship in `m_npc.c` |
| `npc.rs` | `pc_npc_schedule_state`, `pc_npc_is_asleep` | NPC scheduling |
| `interaction.rs` | `pc_npc_patience` | NPC interaction |
| `behavior.rs` | `pc_letter_friendship_delta` | letter friendship effects |
| `collision.rs` | `pc_collision_neighborhood`, `pc_collision_pack` | collision packing |
| `buried_items.rs` | `pc_buried_get/set/clear` | buried-item field state |
| `house.rs` | `pc_house_next_loan` | house loan progression |
| `shop.rs` | `pc_shop_real_level`, `pc_shop_plus_sales` | shop state |
| `scene.rs` | `pc_scene_table_index`, `pc_game_dlftbls_count` | scene tables |
| `town_gen.rs` | `pc_town_generate`, `pc_town_select_initial_villagers` | town generation (`m_random_field.c`) |
| `villager_mail.rs` | (check exports) | villager mail checks |
| `save.rs` | (check exports) | save handling |

### Wave 3 — Engine plumbing (build-system level, do last)

| Rust module | C function(s) | Notes |
|---|---|---|
| `aram.rs` | `pc_aram_get_base` | ARAM access; platform-sensitive |
| `dvd.rs` | (check exports) | disc access; platform-sensitive |
| `gbi_runtime.rs` | `pc_gbi_pack_runtime_ptr`, `pc_gbi_unpack_runtime_ptr` | graphics runtime pointers |
| `mtx.rs` | (check exports) | matrix math |
| `vi.rs` | (check exports) | video interface |
| `profiler.rs` | (check exports) | profiling only |
| `lib.rs` | `pc_disc_init/is_open/extract_dol/extract_rel/shutdown` | disc image handling |

### Already wired (do not touch)

| Rust module | C function(s) | Called from |
|---|---|---|
| `letter_score.rs` | `mMck_check_key_hit`, `mMck_check_key_hit_nes` | `m_mail_check.c`, `m_mail_check_ovl.c`, `m_npc.c` |

## How a single rewiring goes

1. Pick one function from Wave 1.
2. Find its C counterpart and call site in the decomp.
3. Add the Rust staticlib to the C link (Windows MSYS2/MinGW step).
4. Swap the call; keep the C function as a fallback behind a flag if nervous.
5. Rebuild, run the game scenario that exercises it, compare behavior.
6. Mark the row done here and in the workbook.

## What is needed from Philip

- **Nothing for the analysis and prep:** the call-site mapping above can be
  done from the decomp alone.
- **Your Windows machine for the actual plugs:** each rewiring ends with
  "rebuild and run." That needs your MSYS2/MinGW tree
  (`C:\Users\phili\game c\shmanimal crossing\ACGC-PC-Port`) and your game
  disc image. I cannot do that part from here.
- **One decision when we start:** whether to keep each C original behind a
  compile flag as a fallback, or swap outright. Flag is safer; outright is
  cleaner. Wave 1 is safe enough to swap outright.
