---
name: asset-pipeline
description: Work with rustimal-crossing's runtime asset pipeline. Use when adding or converting binary assets, regenerating the asset table, debugging missing/corrupt assets at runtime, or understanding how GameCube disc data reaches the PC build.
---

# Asset Pipeline Skill (rustimal-crossing)

## Overview

The original decomp compiles ~16,400 binary `.inc` files directly into the executable.
The PC port does **not** do this. Instead, binary assets are loaded at runtime from
a GameCube disc image (or fallbacks), so the repo stays free of ROM-derived binaries.

Pipeline, end to end:

```
decomp src/**/*.c with #include "assets/*.inc"   (big-endian data as C arrays)
  → tools/gen_runtime_assets.py rewrites them + generates the loader
  → src/pc_assets.c  (generated asset table, ~30K lines — do NOT hand-edit)
  → include/pc_assets.h (public API)
  → at boot: pc_assets_init() extracts main.dol + foresta.rel.szs into memory
  → pc_load_asset() loads each asset from its original ROM offset, byte-swapped
```

**Path convention:** this repo's root IS the PC port's `pc/` subtree (on Philip's
Windows machine the repo is nested under `pc/`). `DOCUMENTATION.md` uses `pc/`-prefixed
paths for his layout; paths below are repo-root-relative.

## The generator: tools/gen_runtime_assets.py

Scans `src/` for `#include "assets/*.inc"` patterns and:

1. **Rewrites source files in-place**: replaces the inline `#include` with a sized-array
   declaration plus lazy-load code, all under `#ifdef TARGET_PC`. GameCube-target
   builds are untouched.
2. **Generates `src/pc_assets.c`**: central loader with the asset table mapping
   ~2,500 assets to ROM offsets, byte-swap types, and source (DOL or REL).
3. **Generates `include/pc_assets.h`**: the public API (see below).
4. **Copies `.bin` fallbacks** to `build32/bin/assets/` for builds without a disc image.

Usage — always preview before writing:

```bash
python tools/gen_runtime_assets.py --scan-only              # report stats only
python tools/gen_runtime_assets.py --dry-run                # show changes, write nothing
python tools/gen_runtime_assets.py                          # full run
python tools/gen_runtime_assets.py --fix-offsets --dry-run  # preview ROM-offset fixes
python tools/gen_runtime_assets.py --fix-offsets            # apply ROM-offset fixes
```

### Loading modes

1. **ROM-direct**: reads from `orig/GAFE01_00/sys/main.dol` + `orig/GAFE01_00/files/foresta.rel.szs`
   (symbols from `config/GAFE01_00/.../symbols.txt`).
2. **`.bin` fallback**: reads pre-extracted `.bin` files from `assets/`.

### Byte-swapping

GameCube ROM data is big-endian; the PC target is little-endian. Swap type is chosen
per asset from its element type:

| Element type | Swap | Constant |
|---|---|---|
| `u8` / `s8` / `char` | none | `SWAP_NONE` (0) |
| `u16` / `s16` | swap each u16 | `SWAP_U16` (1) |
| `Vtx` | swap s16/u16 fields (first 12 of 16 bytes per vertex) | `SWAP_VTX` (2) |
| `u32` / `s32` | swap each u32 | `SWAP_U32` (3) |

ROM sources: `SRC_REL` (0), `SRC_DOL` (1), `SRC_NONE` (2, `.bin` fallback only).

## Runtime API: include/pc_assets.h

```c
void pc_load_asset(const char* bin_path, void* dest, unsigned int size,
                   unsigned int rom_off, int rom_src, int swap_type);
void pc_bswap_asset_u16(void* data, unsigned int size);
void pc_bswap_asset_u32(void* data, unsigned int size);
void pc_bswap_asset_vtx(void* data, unsigned int size);
void pc_assets_pal_n64_to_gc(unsigned short* pal, int count);
/* Returns 1 when ROM data is found, 0 otherwise. */
int pc_assets_init(void);
```

### Fallback chain (in order)

1. Disc image (`.ciso`/`.iso`/`.gcm`) in `rom/`, `orig/`, or the current directory.
2. Pre-extracted DOL + REL in `orig/GAFE01_00/`.
3. Individual `.bin` files in `assets/`.

## Disc image support

The Rust side (`rust/src/lib.rs`, `dvd.rs` module) opens and parses the disc:

- Formats: CISO (block-mapped, 32KB headers), ISO, GCM (raw).
- Yaz0 decompression for compressed REL files; parses the GCM File System Table.
- Keeps the Dolphin `DVD*` C ABI (`include/dolphin/dvd.h`); `dvd.rs` is the PC DVD
  filesystem shim (path registration, synchronous reads, callbacks).
- The former C implementation `src/pc_dvd.c` remains in the tree **as reference only**
  and is excluded from the active PC target.

## Shader seed: tools/gen_shader_seed.py

The seed embeds known shader-config keys in the binary so a fresh install precompiles
them at boot instead of hitching mid-game:

```bash
python tools/gen_shader_seed.py build32/bin/shader_cache.bin
```

- Regenerate after a thorough play session (the game appends newly seen keys to
  `shader_cache.bin` next to the exe).
- Merges existing `include/pc_shader_seed.h` keys with new cache keys; drops keys
  whose key size went stale. Cache magic: `ACSV` (`0x41435356`).

## Regeneration workflow

1. `python tools/gen_runtime_assets.py --scan-only` — confirm the asset count looks right.
2. `python tools/gen_runtime_assets.py --dry-run` — review every source rewrite.
3. `python tools/gen_runtime_assets.py` — apply; `src/pc_assets.c` and
   `include/pc_assets.h` are regenerated.
4. Rebuild with `TARGET_PC` defined; `pc_assets_init()` must return 1 (ROM found).
5. If ROM offsets drifted (decomp rebase), use `--fix-offsets --dry-run` first.

## Verification checklist

- [ ] `--scan-only` reports the expected asset count (~2,500).
- [ ] `--dry-run` shows only intended source rewrites; no unrelated files touched.
- [ ] No `#include "assets/*.inc"` remains unconverted in `src/` (GameCube builds unaffected).
- [ ] `pc_assets_init()` returns 1 at boot (ROM data found, not silent fallback).
- [ ] Spot-check a `u16` and a `Vtx` asset against decomp values after byte-swap.
- [ ] Game boots to the title screen with no unexpected fallback-chain misses.
- [ ] `src/pc_assets.c` was regenerated by the tool, never hand-edited.

## Rules

- **Never hand-edit `src/pc_assets.c`** — it is generated. Fix the generator or the
  source `.c` files instead.
- **Never commit ROM-derived binaries** (`.bin` fallbacks, disc images, `shader_cache.bin`).
- New assets enter through the decomp `#include "assets/*.inc"` pattern, then the
  generator — not by hand-writing loader calls.
- Preserve existing C ABIs when touching the loader.
- No test runs without Philip's explicit authorization (standing project rule).

## Note on the backlog task file

`.claude-agents/backlog/document-asset-pipeline.md` mentions "Bevy 0.19 glTF/.glb
import conventions" and "Blender -> .glb". Those do **not** apply to this repo —
rustimal-crossing is the C/Rust PC port and uses no Bevy. Blender/.glb work belongs
to the `bevy-crossing` prototype. This skill documents the pipeline that actually
exists here.
