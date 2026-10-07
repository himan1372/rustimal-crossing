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

## The kernel + shim architecture

Do NOT port whole stateful C functions. Port the pure mathematical /
data-selection **kernel**, and keep a deliberately boring **C-side shim**:

```
                 C GAME (keeps all state)
                   │
          ┌────────▼─────────┐
          │   C ADAPTER/SHIM │
          │ gather args → call Rust → apply result
          └────────┬─────────┘
                   │  C ABI boundary (raw pointers, #[repr(C)] types)
          ┌────────▼─────────┐
          │  RUST KERNEL     │
          │  safe, pure, no game state, no allocation
          └──────────────────┘
```

Rules:
- The unsafe FFI boundary is a few lines: validate pointers, convert to
  Rust types, call the safe kernel, write results back. The algorithm
  itself is 100% safe Rust.
- `#[repr(C)]` on every struct crossing the boundary (`PcVec2`,
  `PcSegment`, `PcForbidVector`, `PcHouseSurface`, `PcUnitCoord`).
- Raw pointers at the ABI, never `&[T]` across FFI.
- Keep `f32` as `f32` — never widen to `f64` mid-formula.
- Reproduce the original's edge behavior exactly (e.g.
  `make_tab_2_move_tail` divides with no zero check, producing NaN
  biases for zero input — the port does the same, it does not "fix" it).
- Keep the original C function behind a `USE_RUST` compile flag as a
  one-function rollback switch until the plug is proven.

## Differential testing

For every migrated function, compare against the C implementation by
**IEEE-754 bit pattern** (`to_bits()`), not epsilon:

```rust
assert_eq!(
    (kernel[0].to_bits(), kernel[1].to_bits()),
    (oracle[0].to_bits(), oracle[1].to_bits()),
);
```

A tiny float difference can flip branches like `dist < range`. Test with
random inputs, edge cases (zero, negatives, huge values), and real captured
game inputs. `wall_priority.rs` already contains the template
(`move_tail_differential_bits`).

## Wave 1 — reorganized into kernels

### Wave 1A — pure numerical kernels ★★★★★

| Rust kernel | C function | C call site |
|---|---|---|
| `pc_msg_max` | message capacity constant | `m_msg_main.c_inc` |
| `make_tab_2_move_tail` → `pc_make_tab_2_move_tail` | `mCoBG_MakeTab2MoveTail` | `m_collision_bg.c` |
| `segment_for_wall` → `pc_unit_no_name_2_start_end` | `mCoBG_UnitNoName2StartEnd` | `m_collision_bg_wall.c_inc` |
| `pc_inventory_find`, `pc_inventory_count` | pocket scan | `m_private.c` |
| `pc_judge_wall_from_vector` | `mCoBG_JudgeWallFromVector` | `m_collision_bg.c` — note: depends on the atan backend; port `mCoBG_Get2VectorAngleF` faithfully, don't swap in `atan2f` |

### Wave 1B — pure lookup / mapping kernels ★★★★☆

| Rust kernel | C function | C call site |
|---|---|---|
| `forbid_vector_kernel` → `pc_forbid_vectors`, `forbid_proc` → `pc_forbid_proc` | `mCoBG_MakeForbidVectorData` + gate | `m_collision_bg_wall.c_inc`, `mCoBG_MakeUnitVector` |
| `priority_order` (+ `pc_wall_priority` fixed-buffer ABI — to add) | `mCoBG_GetWallPriority` | `m_collision_bg.c` — preserve the `<=` merge and tie reconstruction |
| `pc_scene_word_type` | scene-word tag decode | `m_scene.c` — decode only, not `Scene_Proc` execution |
| `pc_column_recipe` | item → radius/height recipe | `mCoBG_MakeOneColumnCollisionData` — C keeps ground-height lookup and the `check_proc` callback |

### Wave 1C — simple data operations ★★★★☆

| Rust kernel | C function | C call site |
|---|---|---|
| `pc_bg_neighborhood_coords` (fixed 49-elem buffer, no `Vec` over FFI) | `mCoBG_MakeSizeUnitInfo` coordinate core | `m_collision_bg.c` |
| `pc_bg_room_scope` | room-size lookup | room-scope check |
| `pc_door_next_scene` (arithmetic only) | `goto_other_scene` ID math | `m_scene.c` — C keeps the fade/wipe/scene mutation |
| `house_surface_lookup` → `pc_house_wall_floor` | `mNpc_GetNpcFloorNo/WallNo` core | `m_npc.c` — C keeps `Common_Get`/`Save_Get` lookups |

### Wave 1D — extracted stateless pieces ★★★☆☆

| Rust kernel | C function | C call site |
|---|---|---|
| `talk_count_allowed` → `pc_talk_count_allowed`, `talk_patience` → `pc_talk_patience` | talk-gate comparisons | `m_npc.c` — C keeps `l_npc_talk_info` state |
| `pc_bg_distance_reverse` | distance formula only | split further before plugging: point-line distance → penetration → height gate → dispatch |

### Postponed (not Wave 1)

- `pc_distance_dispatch` — port the individual kernels first (point-line
  distance, penetration, height gate), then the dispatch.
- `pc_request_proc_id` — exact request-procedure table/caller not yet pinned.
- `pc_topic_talk_check` — full talk selection is stateful; the gates above
  are the Wave 1 piece.
- Full `mCoBG_GetWallReverse` solver — much later; Wave 1 builds its pieces.
- Full scene interpreter / house-scene setup — C keeps the mutations.

## Full inventory — everything still on the shelf

### Wave 2 — functions needing game structs

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
| `endpoint_circle.rs` | `pc_cross_circle_line` | `mCoBG_GetCrossCircleAndLine2Dvector` callers |
| `decal_circles.rs` | (registration is gameplay-driven; add ABI when a C caller is chosen) | dig/scoop actions |

### Wave 3 — engine plumbing (build-system level, do last)

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

1. Pick one kernel from Wave 1A.
2. Write the safe Rust kernel + thin `#[no_mangle] unsafe extern "C"` shim
   (raw pointers, `#[repr(C)]` types, `f32` discipline).
3. Write differential bit-pattern tests against the C source formula.
4. Add the Rust staticlib to the C link (Windows MSYS2/MinGW step).
5. Add the C adapter with a `USE_RUST` fallback flag; default it on for
   Wave 1A, off for riskier ones.
6. Rebuild, run the game scenario that exercises it, compare behavior.
7. Mark the row done here and in the workbook.

## no_std note

The Wave 1 kernels use no allocation (`[T; 49]` buffers, `[Option<_>; 2]`
arrays, no `Vec` over FFI). Keep it that way — the long-term target is a
bare-metal-friendly `no_std` core, and every allocation at the boundary
is a future porting cost.

## What is needed from Philip

- **Nothing for the analysis and prep:** kernel extraction and differential
  tests are done from the decomp alone.
- **Your Windows machine for the actual plugs:** each rewiring ends with
  "rebuild and run." That needs your MSYS2/MinGW tree
  (`C:\Users\phili\game c\shmanimal crossing\ACGC-PC-Port`) and your game
  disc image. That part can't be done remotely.
- **One decision when we start:** fallback flag default-on or default-off
  per wave. Wave 1A is safe enough for default-on.
