# Wave 1 wiring runbook — Philip's-machine steps

All 16 green Wave 1 plugs are in `wiring/` (7 edited C files). Each plug
is behind `#ifdef USE_RUST`: without the flag the original C runs, with
`-DUSE_RUST` the Rust kernel takes over. You will copy the files in,
prove the build is clean WITHOUT the flag, then enable it and playtest.

Do it on your Windows machine in MSYS2 bash. The repo is at
`C:\Users\phili\game c\shmanimal crossing\ACGC-PC-Port\pc`.

## Step 1 — pull

```bash
cd "/c/Users/phili/game c/shmanimal crossing/ACGC-PC-Port/pc"
git pull
ls wiring/
```

You should see `README.md`, `RUNBOOK.md`, and `src/`. If `git pull`
says "already up to date" and `wiring/` is missing, stop and tell me.

## Step 2 — back up the originals

```bash
mkdir -p ../src_backup_wave1
cp ../src/game/m_collision_bg.c ../src_backup_wave1/
cp ../src/game/m_collision_bg_wall.c_inc ../src_backup_wave1/
cp ../src/game/m_collision_bg_column.c_inc ../src_backup_wave1/
cp ../src/game/m_private.c ../src_backup_wave1/
cp ../src/game/m_quest.c ../src_backup_wave1/
cp ../src/game/m_scene.c ../src_backup_wave1/
cp ../src/actor/npc/ac_npc_talk.c_inc ../src_backup_wave1/
```

This is your undo button. If anything goes wrong later, copying these
back restores the originals exactly.

## Step 3 — copy the wired files in

From the repo root (`pc/`):

```bash
cp wiring/src/game/m_collision_bg.c ../src/game/m_collision_bg.c
cp wiring/src/game/m_collision_bg_wall.c_inc ../src/game/m_collision_bg_wall.c_inc
cp wiring/src/game/m_collision_bg_column.c_inc ../src/game/m_collision_bg_column.c_inc
cp wiring/src/game/m_private.c ../src/game/m_private.c
cp wiring/src/game/m_quest.c ../src/game/m_quest.c
cp wiring/src/game/m_scene.c ../src/game/m_scene.c
cp wiring/src/actor/npc/ac_npc_talk.c_inc ../src/actor/npc/ac_npc_talk.c_inc
```

## Step 4 — rebuild WITHOUT the flag (sanity check)

Rebuild as usual. It should compile clean — the `#else` branches are
the original code, untouched. Boot the game briefly to confirm it runs
like before. If this build fails, stop and paste me the error: the copy
itself is the problem, not the Rust code.

## Step 5 — enable USE_RUST

Open `pc/CMakeLists.txt` in a text editor. At the very top you'll see:

```cmake
cmake_minimum_required(VERSION 3.16)
project(ac_pc C CXX)
```

Add one line right after it:

```cmake
add_compile_definitions(USE_RUST)
```

Save, re-run your cmake configure, and rebuild. (The line must come
before any `add_executable`/`add_library`; right after `project(...)`
is the safe spot.)

## Step 6 — playtest

Boot the game and exercise what the plugs touch:

1. **Walk into walls, trees, rocks, signposts** — collision feels
   exactly the same, no walking through things, no getting stuck.
2. **Open your pockets** — items show correctly.
3. **Talk to villagers** — conversations start normally, including the
   "do you want to talk?" prompt villagers give on their own.
4. **Go through a door** (your house, a villager's house, the shop) —
   scene changes work.

Play for ~10 minutes. You're comparing against how the game felt in
Step 4: anything different is a suspect.

## Step 7 — if something breaks

1. Note exactly what you were doing (e.g. "walked into a tree on the
   north cliff and fell through").
2. Tell me. The plugs are per-file, so we can narrow it down fast.
3. To get back to a working game immediately: copy the backups back
   (`cp ../src_backup_wave1/*` to the matching `../src/` spots),
   remove the `add_compile_definitions(USE_RUST)` line, rebuild.

## After

Tell me it works (or what broke). Next I prepare the Wave 2 green
plugs the same way. Keep `../src_backup_wave1/` until Wave 1 is fully
signed off.
