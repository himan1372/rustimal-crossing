# Rewiring

**What this means, plainly:** every Rust module in this repo is a finished, tested
replacement part sitting on a shelf. "Rewiring" is the job of opening up the
game's C code and plugging those parts in — replacing a C function call with a
call into the matching Rust function — then proving the game still behaves the
same. Two parts are plugged in today: the letter scorer (call-site pattern)
and five Wave 3 platform modules (build-system pattern, see below).
Everything else below is still on the shelf.

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
| `pc_make_move_bg_walls` (`PcMoveBgWall[4]`), `pc_move_bg_delta` | `mCoBG_SizeData2CollisionData`, `mCoBG_MoveActorWithMoveBg_OnMoveBg` | `m_collision_bg_move.c_inc` — registry stays in C for now |
| `pc_cardinal_edge_mask`, `pc_check_height_exactly` (+ `cardinal_edge_exists`, `adjust_slate_unit_for_cardinal`) | `mCoBG_GetUnitInfSearchData`, `mCoBG_CheckHeightExactly`, `mCoBG_UtInf2NormalSlateWallVector` | `m_collision_bg_wall.c_inc` |
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

### Wave 2 — functions needing game structs (status after 2026-10-08 audit)

Every claim below was re-verified against the USA Rev. 0 decomp; the
Rust-side corrections are in (commit `848b39c`, workbook row 249).
Standing rule established: **Rust never owns the retail RNG call** — C
does `r = RANDOM(n)` and passes the bounded value (or a callback) in.

**Wave 2A — safe to wire now** (clean scalar boundaries, verified verbatim):

| Rust kernel | C function | C call site idea |
|---|---|---|
| `pc_npc_house_goods` | `mQst_GetGoods_common` | `m_quest.c` — C keeps the `RANDOM(10)` |
| `pc_turn_mod` | `Player_actor_Movement_Walk` | `m_player_main_walk.c_inc` — `0.01f32` already bit-identical |
| `pc_wander_choice` | `aNPC_think_wander_decide_next` | `ac_npc_think_wander.c_inc` |
| `pc_friendship_mode` | `aNPC_chk_avoid_and_search` | `ac_npc_move.c_inc` — pass `*friendship + over_friendship` |
| `pc_house_next_loan` | `aNSC_set_talk_info_start_wait` | `ac_npc_shop_common.c` |
| `pc_shop_real_level` | `mSP_GetRealShopLevel` | `m_shop.c` |
| `pc_shop_plus_sales` | `mSP_PlusSales` | `m_shop.c` — C writes back `sales_sum` |
| `pc_game_dlftbls_count` | `game_dlftbls` users | count only (11, incl. the PC-only model viewer) |

**Wave 2B — wire with C-side state gathering** (kernels are correct; C
collects the state):

| Rust kernel | Notes |
|---|---|
| `pc_eligible_furniture_count` | replaces only the counting pass of `mNpc_DecideNpcFurniture`; C builds the 100-byte flag array, keeps the second scan |
| `pc_request_pick_carried` | now takes `&[u8]`, no allocation across FFI |
| `pc_npc_schedule_state` | correct value for `schedule->saved_type` only; C keeps forced/current/event overrides |
| `pc_npc_is_asleep` | answers the base schedule, not the actor's sleep state |
| `pc_npc_patience(talk_num, looks)` | takes personality — retail indexes `l_npc_temper` by looks (quirk preserved) |
| `pc_collision_neighborhood`, `pc_collision_pack` | sub-operations, not whole-function replacements |
| `pc_buried_line_get/set/clear` | the exact retail row-pointer boundary (`mFI_*`); the whole-array `pc_buried_get/set/clear` are rewrite-side convenience |

**Wave 2C — fixed in Rust, ready to wire** (were wrong, now corrected):

| Rust kernel | What was fixed |
|---|---|
| `pc_request_dispatch` | was a conventional weighted selector; now the verbatim 61-RNG-call shuffle (shared impl with `talk_topics`). **ABI changed** to `(probs, n, rng callback)` — C passes a 3-line `RANDOM` wrapper |
| `pc_letter_friendship_delta` | was `+3/+6/0/+3`; now the retail `+3 / -5-if-BAD / +3-if-present` |
| `pc_locomotion_core` | documented as a classification helper only, not a movement replacement |

**Wave 2D — do not wire yet** (boundary redesign needed):

- `pc_town_generate`, `pc_town_select_initial_villagers` — `TownPlan`
  is rewrite-owned, not retail `mFM_*` state; wiring now would replace
  the retail generator rather than shim a function.
- `pc_scene_table_index` — retail resolves via init-function-pointer
  comparison; the pointer scan stays in C.
- Full player-movement core — needs the `#[repr(C)]`
  `PcPlayerMoveState` bridge designed first.

### Wave 3 — engine plumbing (status 2026-10-08, commit `aeb52a7`)

Different pattern from Waves 1–2: no call-site changes. The Rust
modules already export the same ABI names as the PC C platform files,
so the "wiring" is CMake exclusions — the C versions stop compiling
and the Rust archive satisfies the existing C calls. Symbol coverage
verified 1:1 for all seven modules.

**Wired** (exclusions active in `CMakeLists.txt`; headers and C callers
unchanged):

| Rust module | Replaces | Notes |
|---|---|---|
| `aram.rs` | `src/pc_aram.c` | `ARFree` ABI fixed to `void` (decomp header's `u32` is an "Unused/inlined in P2" guess; no C callers) |
| `dvd.rs` + `lib.rs` (`pc_disc_*`) | `src/pc_dvd.c` + `src/pc_disc.c` | migrated as one cluster — `dvd.rs` calls the Rust `pc_disc_*` |
| `gbi_runtime.rs` | `src/pc_gbi_runtime.c` | token mechanism preserved |
| `profiler.rs` | `src/pc_profiler.c` | C hot-path inline wrappers keep calling the Rust `*_slow` exports |

**Staged** (exclusion lines written but commented out):

| Rust module | Replaces | What's missing |
|---|---|---|
| `mtx.rs` | `src/pc_mtx.c` | `f32.to_bits()` differential suite first (float op-order sensitivity) |
| `vi.rs` | `src/pc_vi.c` | wire last, after boot/game-loop validation on the Rust profiler; callback ABI already fixed to `void*` |

**Deliberately not replaced:** `pc_platform.c` (SDL/GX/audio stay C),
the profiler's C-owned emu64 counters, `g_pc_profile_*` header decls.

Also in `CMakeLists.txt`: the hard-coded Rust `.rs` DEPENDS list is now
a recursive `CONFIGURE_DEPENDS` glob over `rust/src/*.rs`, so the
archive rebuilds when any module changes.

### Already wired (do not touch)

| Rust module | C function(s) | Called from |
|---|---|---|
| `letter_score.rs` | `mMck_check_key_hit`, `mMck_check_key_hit_nes` | `m_mail_check.c`, `m_mail_check_ovl.c`, `m_npc.c` |
| Wave 3 (build-system) | `pc_aram.c`, `pc_dvd.c`, `pc_disc.c`, `pc_gbi_runtime.c`, `pc_profiler.c` exclusions | `CMakeLists.txt` — Rust archive satisfies the same ABI |

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
