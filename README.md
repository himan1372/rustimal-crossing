# Rustimal Crossing

A Rust rewrite of Animal Crossing (GameCube) game logic, wired into the ACGC-PC-Port as optional replacements for the original C code.

## Getting It Working

### What You Need

1. **ACGC-PC-Port** — the PC port of the Animal Crossing GameCube decomp. This repo does **not** include it (see note below).
2. **This repo** (`rustimal-crossing`)
3. **MSYS2 with MinGW32** — the build toolchain:
   - `mingw-w64-i686-gcc`
   - `cmake`
   - `make`
   - `SDL2`
   
   Install in the MINGW32 shell:
   ```
   pacman -S mingw-w64-i686-gcc cmake make mingw-w64-i686-SDL2
   ```
4. **Rust** — with the `i686-pc-windows-gnu` target:
   ```
   rustup target add i686-pc-windows-gnu
   ```

> **Note:** The ACGC-PC-Port contains decompiled Nintendo game code and is not included here for legal reasons. You'll need to obtain it separately.

### Where to Put Everything

Clone this repo **inside** the ACGC-PC-Port directory, in a folder called `pc/`:

```
ACGC-PC-Port/
├── src/            ← PC port source (decomp C files)
├── pc/             ← this repo goes here
│   ├── rust/
│   ├── wiring/
│   ├── include/
│   └── ...
└── build_pc.sh
```

### Wiring It Up

The `wiring/` directory contains modified C files with `#ifdef USE_RUST` blocks. Copy them over the port's `src/`:

```bash
# From the ACGC-PC-Port directory:
cp -r pc/wiring/* src/
```

Each wired file keeps the original C code as default. The Rust replacement only activates when you build with `-DUSE_RUST`.

### Applying Bug Fixes

The `fixes/` directory contains pure C bug fixes for the PC port (no Rust). Copy each file to its matching location under `src/`:

```bash
# From the ACGC-PC-Port directory:
cp pc/fixes/m_trademark.c src/game/m_trademark.c
cp pc/fixes/m_player_main_pickup.c_inc src/game/m_player_main_pickup.c_inc
```

Current fixes:
- `m_trademark.c` → `src/game/m_trademark.c` — title demo was showing the player's save data instead of random villagers
- `m_player_main_pickup.c_inc` → `src/game/m_player_main_pickup.c_inc` — removed the cash register ching when picking up money

### Compiling

Build from the ACGC-PC-Port directory:

```bash
./build_pc.sh
```

To enable the Rust replacements, make sure `USE_RUST` is defined. Check `build_pc.sh` — it should pass `-DUSE_RUST` to the compiler. If not, add it to the CFLAGS.

To build **without** Rust (pure original C behavior), build without `-DUSE_RUST`.

### Testing

After building, run the game and playtest the systems you've enabled. Test one wave at a time — don't enable everything at once.

---

## What This Is

This repo contains Rust reimplementations of Animal Crossing: GameCube game systems, verified against the decompiled source. Each module is a "replacement part" — finished, tested Rust code that can optionally replace the original C at runtime via `#ifdef USE_RUST` plugs.

The architecture is **kernel + shim**:
- **Rust** implements the pure logic (math, data selection, state machines)
- **C** keeps all game state and calls into Rust through a C ABI boundary

See `rewiring.md` for the full pattern.

## Wave Status

### Wave 1 — Core Systems ✅
- Collision (background, walls, columns)
- Private/player data (pockets, inventory)
- Quest system
- Scene management
- NPC talk

### Wave 2 — NPC Behavior ✅
- **Green:** Turn snapping, wander AI, friendship, shop levels, mail, request dispatch
- **Yellow:** Furniture eligibility, quest item selection, NPC schedules, patience, buried items

### Wave 3 — Platform ✅
- ARAM, DVD/disc, GBI runtime, profiler, MTX, VI

All waves playtest-validated. See `DOCUMENTATION.md` for per-module details.

## Repo Layout

```
rust/           ← Rust modules (the rewrite)
wiring/         ← C files with USE_RUST plugs (copy to ../src/)
fixes/          ← PC-port C bug fixes, no Rust involved (copy to ../src/)
include/        ← pc_rust.h and other headers
src/            ← PC platform files (pc_audio.c, pc_gx.c, etc.)
town_prototype/ ← Standalone town-gen prototype
tools/          ← Build and utility scripts
tests/          ← Rust test suite
```

## Contributing

New modules should:
1. Be verified against the decomp source
2. Keep the existing C ABI (don't break the C side)
3. Include a `DOCUMENTATION.md` section
4. Have Rust unit tests (`cargo test`)
5. Get a wiring file in `wiring/` mirroring the decomp path

## License

Original Rust code in this repo is yours to use. The wiring files are derived from the Animal Crossing decomp and carry its legal status — don't redistribute Nintendo's code.
