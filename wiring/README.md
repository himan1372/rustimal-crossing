# wiring/ — Wave 1 C plugs (USE_RUST)

Edited decomp C files with the Wave 1 Rust plugs applied. Each plug is
wrapped in `#ifdef USE_RUST` / `#else` / `#endif`:

- **Without `USE_RUST` defined** (default): the original C code runs,
  byte-for-byte identical to the decomp. Safe to copy these files in
  and build — nothing changes.
- **With `-DUSE_RUST`**: the pure calculation inside each function is
  replaced by a call into the Rust staticlib (declared in
  `pc/include/pc_rust.h`, which is already on the compiler's include
  path). All C state, tables, structs, and RNG stay in C.

## Layout

Mirrors the decomp tree. On your Windows machine the repo lives at
`ACGC-PC-Port/pc/`, so copy like this from the repo root:

```
cp wiring/src/game/m_collision_bg.c         ../src/game/m_collision_bg.c
cp wiring/src/game/m_collision_bg_wall.c_inc ../src/game/m_collision_bg_wall.c_inc
cp wiring/src/game/m_collision_bg_column.c_inc ../src/game/m_collision_bg_column.c_inc
cp wiring/src/game/m_private.c              ../src/game/m_private.c
cp wiring/src/game/m_quest.c                ../src/game/m_quest.c
cp wiring/src/game/m_scene.c                ../src/game/m_scene.c
cp wiring/src/actor/npc/ac_npc_talk.c_inc    ../src/actor/npc/ac_npc_talk.c_inc
```

(`../src/` = `ACGC-PC-Port/src/`, the `DECOMP_ROOT` the CMake build
globs.)

To enable the plugs, add `-DUSE_RUST` to the C compile flags
(e.g. `add_compile_definitions(USE_RUST)` in CMakeLists.txt, or per
test). Test one file at a time: enable, rebuild, run the scenario,
compare behavior, then move on.

## What's plugged (all green Wave 1)

| File | Plugs |
|---|---|
| `src/game/m_collision_bg.c` | `pc_make_tab_2_move_tail`, `pc_judge_wall_from_vector`, `pc_distance_dispatch`, `pc_bg_distance_reverse`, `pc_bg_neighborhood`, `pc_bg_room_scope` |
| `src/game/m_collision_bg_wall.c_inc` | `pc_unit_no_name_2_start_end`, `pc_forbid_vectors`, `pc_forbid_gate` |
| `src/game/m_collision_bg_column.c_inc` | `pc_column_recipe_item` |
| `src/game/m_private.c` | `pc_inventory_find`, `pc_inventory_count` |
| `src/game/m_quest.c` | `pc_request_proc_id` |
| `src/game/m_scene.c` | `pc_scene_word_type`, `pc_door_next_scene` |
| `src/actor/npc/ac_npc_talk.c_inc` | `pc_topic_talk_check`, `pc_topic_force_gate` |

## Provenance

Based on the upstream decomp snapshot in `acgc-upstream/` (2026-10-06).
If your local `ACGC-PC-Port/src/` differs, merge the `#ifdef USE_RUST`
blocks by hand — each one is marked with a `Wave 1 plug` comment.
