# Animal Crossing PC Port — Developer Documentation

PC port of Animal Crossing GameCube built on top of a 99.52% complete C decompilation.

## Architecture Overview

The game's rendering has 3 tiers:

```
Game code (N64 display lists) → emu64 (DL interpreter) → GX (GameCube GPU API) → [OpenGL]
```

Within the graphics pipeline, we replace **only tier 3** (GX → OpenGL 3.3). The longer-term goal is a Rust rewrite advanced through source-verified, ABI-compatible slices. Unported emu64, scene logic, and game code remain in C/C++, with `#ifdef TARGET_PC` guards where platform differences require them.

The PC disc reader, DVD filesystem shim, ARAM shim, GBI runtime-pointer adapter, frame profiler, video interface/frame-pacing shim, PC matrix/vector replacements, villager-mail repeat check, and rewrite-owned procedural town planner are built from `pc/rust` as a `staticlib`. Existing C entry points retain their ABI. The town planner has its own new C ABI and is conditionally connected to PC save-table initialization through `pc/src/pc_town_adapter.c`; the legacy generator remains the per-town fallback and the non-PC implementation.

### Boot Chain

```
main() [pc_main.c]
  → pc_settings_load()          # load settings.ini
  → pc_platform_init()          # SDL2, GL 3.3, GLAD
  → pc_disc_init()              # find & open disc image (CISO/ISO/GCM)
  → pc_assets_init()            # extract DOL/REL from disc, load all ~2500 assets
  → pc_texture_pack_init()      # scan texture_pack/ for HD replacements
  → ac_entry()                  # game's main.c: sets HotStartEntry = &entry
  → boot_main()                 # boot.c: OSInit, DVD, archives
    → entry() → mainproc() → graph_proc()   # THE MAIN LOOP

graph_proc() loops over scenes via game_dlftbls[]:
  first_game → second_game → trademark → select (title demo)
  → player_select (table index 6) → play (gameplay)
  OR: --model-viewer → model_viewer_init (table index 10)

Each frame: graph_main()
  → game_main() → scene->exec()         # builds N64 display lists
  → graph_task_set00() → emu64_taskstart()  # processes DLs → GX → GL
  → VIWaitForRetrace() [pc/rust/src/vi.rs]  # SDL swap + event pump + frame pacing
```

## Runtime Asset Loading

The original decomp compiles ~16,400 binary `.inc` files directly into the executable. The PC port instead loads assets at runtime from a GameCube disc image, eliminating the need for the decomp's asset extraction pipeline.

### Pipeline

```
User provides disc image (.ciso/.iso/.gcm)
  → pc_disc_init() opens and parses GCM filesystem
  → pc_assets_init() extracts main.dol + foresta.rel.szs into memory
  → ~2500 assets loaded from DOL/REL data at their original ROM offsets
  → Byte-swap applied per asset (SWAP_NONE/SWAP_U16/SWAP_U32/SWAP_VTX)
  → Source files use lazy-load pattern for function-local static data
```

### Code Generation

`pc/tools/gen_runtime_assets.py` (632 lines) scans all `src/*.c` files for `#include "assets/*.inc"` patterns and:

1. **Transforms source files in-place**: replaces inline `#include` with sized-array declarations and lazy-load code under `#ifdef TARGET_PC`
2. **Generates `pc/src/pc_assets.c`** (~30K lines): central loader with asset table mapping ~2500 assets to their ROM offsets, byte-swap types, and source (DOL or REL)
3. **Generates `pc/include/pc_assets.h`**: public API (`pc_assets_init`, `pc_load_asset`)
4. **Copies `.bin` fallback files** to `pc/build32/bin/assets/` for non-disc-image builds

### Fallback Chain

1. **Primary**: Disc image in `rom/`, `orig/`, or current directory
2. **Secondary**: Pre-extracted DOL + REL files in `orig/GAFE01_00/`
3. **Tertiary**: Individual `.bin` files in `assets/`

### Disc Image Support

`pc/rust/src/lib.rs` handles CISO (block-mapped, 32KB headers), ISO, and GCM (raw) formats. It includes Yaz0 decompression for compressed REL files and parses the GCM File System Table. The Rust `dvd.rs` module implements the PC DVD filesystem shim: it registers paths, opens entries from the disc image or the extracted-file fallback, and provides the synchronous reads and callbacks expected by the PC port. It retains the Dolphin `DVD*` C ABI and uses the `DVDFileInfo` fields at offsets `0x18`, `0x30`, and `0x34` defined by `include/dolphin/dvd.h` (`0x3C` bytes total).

The former C implementation, `pc/src/pc_dvd.c`, remains in the source tree as a reference and is excluded from the active PC target; C and C++ call sites continue to use the Dolphin declarations in `include/dolphin/dvd.h`.

The image's rendering/presentation panel aligns with a documented runtime boundary here: `pc/rust/src/vi.rs` provides the PC-facing VI frame boundary (event polling, deferred GX drain, swap, pacing, and retrace count). Display-list production, scene rendering decisions, camera behavior, and game presentation logic remain in the C/C++ decompilation. The image's world, character, and gameplay panels are leads only; their detailed behavior must be traced in source before any future migration.

## Runtime Port Progress: Procedural Town Generation

### Source findings

The procedural-town infographic was checked against `include/m_field_make.h`, `src/game/m_random_field_ovl.c`, `src/game/m_random_field.c`, and `src/game/m_field_make.c`.

- **Verified dimensions:** the save layout has a 7 by 10 block grid. `FG_BLOCK_X_NUM` and `FG_BLOCK_Z_NUM` define a 5 by 6 playable-acre region (30 acres). Each foreground acre contains 16 by 16 actor/unit entries (`UT_X_NUM`, `UT_Z_NUM`, and `mFM_fg_c`); calling these graphical tiles would be imprecise.
- **Verified elevation selection:** `mRF_GetRandomStepMode` selects the three-step path when `RANDOM(100) < 15`. Two-step generation is the other path. `l_mRF_step3_blockss` contains 10 source-defined three-step base layouts.
- **Verified random source:** `RANDOM(n)` expands through `fqrand`; `src/static/libc64/qrand.c` updates the state with `state * 0x19660D + 0x3C6EF35F` (decimal increment 1,013,904,223). `src/system/sys_math.c:init_rnd` seeds it from `osGetCount()`, and `src/second_game.c` calls `init_rnd`. This supports the image's approximate multiplier/increment and timer-seed claims. The generator itself consumes this shared stream rather than owning a town-only seed.
- **Verified placement contract:** the base map fixes the station and player-house acres. `mRF_SetUniqueRailBlock` chooses one shop and one post office, one in each outer rail pair. `mRF_SetNeedleworkAndWharfBlock` puts the port at full-grid `(5,6)` and selects the tailor from a random index among the first three remaining `BEACH` blocks in full-grid row 6. The Rust plan maps the port to playable-acre `(4,5)` and currently chooses the tailor from `(0..2,5)` as a simpler rewrite rule; that exact coordinate restriction is a design choice, not a source guarantee. `mRF_SetUniqueFlatBlock` selects the shrine below the cliff on a preferred river side, the police box on the opposite side when possible, and the museum below the cliff on either side. Bridge, slope, pool, and beach-river placements are source functions in the same file. The generator rebuilds candidate layouts until required placement bits are present, selects concrete acre combinations from the supplied combination table, then copies generated heights into the save table.
- **Verified source boundary:** `mFM_InitFgCombiSaveData` calls the C generator and populates the original 7 by 10 save table and foreground arrays. The PC build now runs a Rust-plan adapter after that C generation and before save-table/foreground population; the legacy generator remains the fallback and remains the implementation on non-PC targets.
- **Verified resident setup and house sites:** `include/m_npc_personal_id.h` defines six look classes. `mNpc_DecideLivingNpcMax` selects one eligible `mNpc_GROW_STARTER` resident per look class from a shuffled roster; `ANIMAL_NUM_MAX` is 15. In `mNpc_MakeReservedListBeforeFieldct`, `mNpc_InitNpcData` gathers reserved foreground units by scanning the 5 by 6 playable acres and applying `mNT_IS_RESERVE`, with a bounded list of 60 sites. `mNpc_SetNpcHome` assigns distinct reserved sites to residents that do not already have saved homes. `mNpc_BuildHouseBeforeFieldct` requires an in-acre center unit with coordinates 1 through 14 and builds a 3 by 3 footprint: house at the center, signboard at offset (-1,+1), and the other seven cells marked unavailable; displaced items are handled through the source's mailbox/deposit path. The infographic's exact “~27 excluded” count is not established here.
- **Verified population growth limits:** `mNpc_CheckGrow` uses town field rank, a minimum elapsed interval of one day, and a check that the local player has talked to all current residents; `mNpc_CheckGrowFieldRank` uses per-rank probabilities from 40% through 100%. Forced removal runs only at capacity and waits at least 10 days. These lifecycle rules remain C; a moving-box depiction was not verified in this pass.
- **Verified environment scoring:** `src/game/m_field_assessment.c` counts trees, flowers, weeds, and trash outside the dump by acre. Tree bands are <=8, 9–11, 12–14, 15–17, and >=18, worth 0/1/2/1/0 points; flowers offset weed count, and three or more effective weeds or any outside-dump trash zero that acre. Town score adds perfect acres plus half of good acres, but five or more outside-dump trash zeroes it. The rank thresholds are 0, 2, 4, 7, 12, and 16. `mFAs_PERFECT_DAY_STREAK_MAX` is 15 and the header comments associate it with the golden-axe reward; a wishing-well visitor checklist is not established by this scoring code.
- **Verified rejection-sampling loop:** `mRF_MakeRandomField_ovl` builds a complete candidate town, accumulates placement bits, and repeats the whole attempt while `perfect_bit != (perfect_bit & bit)`, where `perfect_bit` has all 9 `mRF_BIT_*` bits set: `SLOPE_LEFT`, `SLOPE_RIGHT`, `BRIDGE_UPPER`, `BRIDGE_LOWER`, `SHRINE`, `POLICE`, `MUSEUM`, `POOL`, `NEEDLEWORK`. Invalid candidates are discarded, never backtracked acre-by-acre. This is the mechanism behind the occasional long black screen during town creation.
- **Verified generation phase order:** clear save acres → base landform (cliffs + river) → flat-place info → beach base → bridges/slopes → tailor/dock → wishing well/police/museum → shop/post office (no perfection bit; placed by internal retry) → lake → sea-block bridge fixup → height table → acre selection from the combination table → copy heights.
- **Verified lookup-table architecture:** `mRF_block_info[]` maps each of the 108 `mFM_BLOCK_TYPE_*` values to a bitmask of `mRF_BLOCKKIND_*` kinds (bit positions in `include/m_random_field_h.h`: `PLAYER`=1<<0 … `DOCK`=1<<30, `ISLAND_LEFT`=1<<31). Block properties live in this table, not in per-acre attached metadata. `mRF_gate_info2[108][4]` maps (block type, direction as NORTH/WEST/SOUTH/EAST) to gate kinds `GATE_NONE`, `GATE1_TYPE0/1`, `GATE2_TYPE0/1`, `GATE3_TYPE0`, with gate counts `{0,1,1,2,2,3}` from `mRF_GateType2GateCount` — gates are resolved later, never attached to blocks up front. `l_river_next_direct[]` maps the 7 river types (`mRF_RIVER0`–`mRF_RIVER6`) to exit directions SOUTH, EAST, WEST, EAST, SOUTH, WEST, SOUTH.
- **Verified rail placement detail:** `mRF_SetUniqueRailBlock` randomly orders shop vs post office, then replaces `TRACKS_DUMP` placeholders at block-grid x = 1+`RANDOM(2)` (left) and x = 4+`RANDOM(2)` (right) on row A — matching the Rust plan's left pair {0,1} / right pair {3,4} of the playable row.

### Rust rewrite implementation

`pc/rust/src/town_gen.rs` adds a clean-room, deterministic first town-planning subsystem for the Rust rewrite. It uses the source-backed 5 by 6 acre/16 by 16 unit dimensions and facility placement constraints, and generates semantic data only: elevation tiers, reciprocal river-edge connections ending at the southern beach, cliff/waterfall edge flags, facility roles, bridge, slope and pond markers, grass-pattern selectors, decoration cell kinds, and caller-supplied resident IDs with house-center slots. Resident placement reserves a complete, non-overlapping 3 by 3 unit footprint for each resident. The center is `House`, the signboard marker is one unit southwest at offset (-1,+1), and the remaining seven units are `HouseReserved`; the saved `house_units` value is the center's linear acre-unit index. A separate selector chooses one already-eligible resident from each of the six source look classes for initial town setup. The maximum resident slots match the source cap of 15. The module also exposes a source-based field assessment over generated vegetation, with outside-dump trash counts supplied by the caller. Its seedable 32-bit RNG ports the repository's verified `qrand` LCG recurrence and float conversion. It does not reproduce the original town generator's exact RNG call order or its source-authored acre tables.

The three-tier probability is set to the source-observed 15%. The module's explicit seed input, simplified river routing, cliff geometry, bridge/pond/slope markers, grass-pattern selection, decoration probabilities, and house-site selection are **rewrite design choices**. The footprint shape and sign offset mirror `mNpc_BuildHouseBeforeFieldct`, but the Rust planner synthesizes candidate sites on grass acres with no facility, river, bridge, or slope marker; it permits sites on acres with cliff edges because the footprint is contained within that acre's 16 by 16 grid. It does not consume the source field's authored `RSV` reserve tokens, emulate their ordering, or reproduce the C home-selection RNG sequence. Although its LCG matches the shared `qrand` algorithm, its outputs will differ from the original because it has its own seed input and call sequence, and it does not use the authored acre variants. The starting-roster helper and field-scoring thresholds are source-based, but callers supply eligible resident IDs and outside-dump trash counts. Version-specific roster exclusions, resident growth/departure timing, and visitor eligibility remain separate work. No game art or copyrighted assets are included.

The `pc_town_generate` API and matching layout are declared in `pc/include/pc_town_gen.h`. `pc/src/pc_town_adapter.c` bridges a generated `TownPlan` into the existing `mFM_combination_c` table, and `src/game/m_field_make.c:mFM_InitFgCombiSaveData` calls it on PC after the source generator. The adapter now takes semantic facility locations from the Rust plan and maps them to the corresponding existing block types: station, shop, post office, player house, shrine/wishing well, police box, museum, needlework/tailor, and port/dock. It maps the Rust-owned rail row's facility roles and river connector while retaining non-semantic track decoration from the legacy row; the Rust river must still join the C-selected `TRACKS_RIVER` location. The other playable acres map through existing `data_combi_table` block types and receive the planned elevation tiers. Before mapping, the adapter checks the one-of-each facility counts and their source-backed fixed/eligible regions. It only commits if every mapped acre has an authored block type, terrain/facility constraints hold, and exactly two bridge combinations are represented. Unsupported river/waterfall shapes, conflicts, or missing authored combination types reject the whole Rust plan and leave the C-generated map intact. Bridge markers are constrained to flat river acres because the authored table has no cliff/waterfall bridge combinations. The seed comes from `osGetCount()` and does not advance the shared `RANDOM` stream.

This is a gameplay connection, not a full replacement of legacy field behavior. Existing C foreground arrays and moving-actor/save initialization still run from the adapted combination table. Semantic building positions—including the port/dock—now come from Rust instead of being copied back from the C plan. Rust now creates resident home-footprint semantics and center coordinates in `TownPlan`, but these values are not yet projected into the game's foreground actor/save arrays; live NPC home assignment and house construction remain in C and continue to use saved home data and source-authored reserve sites. Generated river/cliff layouts that cannot be represented by authored block types fall back to the C map. In particular, waterfalls currently adapt only when the legacy combination table has a supported straight horizontal-cliff form. Facility region constraints follow source structures, but randomized locations and the full authored acre-selection process are not bit-exact ports. The adapter adds a PC-only internal function and does not change an existing C ABI or affect non-PC generation.

**October 2026 source-fidelity upgrade.** `town_gen.rs` now ports the generator's lookup tables verbatim: `BLOCK_KIND_TABLE` (all 108 block types → `BK_*` bitmask, bit positions from `m_random_field_h.h`), `GATE_TABLE` (all 108×4 block-type/direction gate entries), `GATE_COUNT_TABLE`, and `RIVER_NEXT_DIRECTION` (7 river types), with accessors `block_kind`, `gate_type_for`, `gate_count_for`, and `river_next_direction`. Generation was restructured into the source's phase order with a real rejection sampler: `generate_candidate` builds one complete town and collects the 9 `PERFECT_*` bits (slopes left/right, bridges upper/lower with the northernmost crossing as "upper", shrine, police, museum, pool, needlework), and `generate` retries from a single deterministic RNG stream until `is_valid_town` passes (bounded by `MAX_GENERATION_ATTEMPTS` = 4096), mirroring `while (perfect_bit != (perfect_bit & bit))`. A roster that cannot cover the requested villager count fails fast instead of burning attempts. Cliff/river tracing still uses the rewrite's boundary-signature model rather than the source's block-chain lookup tables and 10 authored step-3 templates, and no authored acre byte tables are reproduced. `cargo check --lib` passes with no new warnings (one pre-existing `dvd.rs` warning remains). A test-run fidelity fix: the source rolls the 15% three-tier choice once *outside* the retry loop, so `generate` now draws `three_tiers` once and reuses it across rejected candidates instead of re-rolling per attempt. Full test suite (`cargo test --lib`): **54/54 pass**, run 2026-10-07 after Philip authorized tests.

### Standalone x86 prototype

`pc/town_prototype` is an asset-free Rust console app that imports `pc/rust/src/town_gen.rs` directly. It prints acre roles and resident home coordinates, then writes a standalone WebGL HTML view driven by the generated plan. The view renders the 80 by 96 unit surface, tiered acre shelves, cliff faces, seeded curved river channels, waterfall drops, bridge markers, resident houses/signs, facility buildings, vegetation, and the southern ocean boundary. Camera orbit/zoom/pan and layer toggles work in Edge without a local server or network access. It accepts `--seed` and `--villagers` (1 through 15); IDs are sample values. The CLI retries up to 64 consecutive seeds only when its requested plan is unplaceable and reports the seed actually rendered. Home candidates may use cliff-edge acres, while each 3 by 3 footprint remains inside one grass acre and excludes facilities, rivers, bridges, and slopes. Build with `pc/town_prototype/build_x86.bat` (defaults to `C:\msys64`; override via `MSYS2_ROOT`), then run `pc/town_prototype/run.bat --seed 305419896 --villagers 6`. This uses the existing `i686-pc-windows-gnu` Rust target and outputs `outputs/ac_town_prototype.exe` and `outputs/ac_town_preview.html`, independently of the game build and without game assets. The x86 executable built successfully using `C:\msys64\mingw32\bin`; the executable ran and its HTML preview rendered in Edge. No tests were run. See `pc/town_prototype/README.md` for controls. This is a visualization of rewrite-owned semantic data, not original town geometry or live C gameplay.

### Validation

The Rust module has isolated unit checks for deterministic output, fixed facility regions, reciprocal river edges and a southern outlet, resident roster selection, distinct resident homes with complete non-overlapping 3 by 3 footprints and source sign offsets, field-assessment rules, C record sizes, and invalid inputs. `pc/tests/pc_town_adapter_checks.c` adds a standalone C check target that exercises the actual adapter against a synthetic table containing every authored block type. It checks Rust facility-to-block placement, elevation transfer, river-to-rail alignment, bridge count, out-of-bounds preservation, and all-or-nothing behavior for rejected layouts. In an MSYS2 MINGW32 build directory, build it with `mingw32-make pc_town_adapter_checks` and run it with `ctest -R pc_town_adapter_checks --output-on-failure`. Neither the Rust nor C checks have been run, per the current instruction. The 32-bit CMake/MSYS2 build is still required to validate C compilation and linker integration on this checkout. Do not infer bit-exact original town reproduction from the module tests or adapter checks.

## Runtime Port Progress: Save & Storage System

### Source findings

The save/storage infographic was checked against `include/m_card.h`, `src/game/m_card.c`, `include/m_common_data.h`, `src/game/m_flashrom.c`, `include/m_private.h`, `include/m_personal_id.h`, and `pc/src/pc_save_bswap.c`.

- **Verified file layout:** the town file is `DobutsunomoriP_MURA` (`l_mCD_land_file_name`), `mCD_LAND_SAVE_SIZE` = 0x72000 = 57 blocks; with the 64-byte GCI header the file is 467,008 bytes (0x72040). Sibling names: `DobutsunomoriP_MURA_d` (dummy/backup), `DobutsunomoriP_PL_` (travel, index appended), `DobutsunomoriP_Omake_` (bonus/gift letters).
- **Verified sub-entry table:** `l_mcd_file_table` (`mCD_FILE_*` order) subdivides the MURA file into misc, main save, main backup (`SAVE_MAIN_BAK`), mail region (0xC000), original/design region (0xE000), and diary region (0xC000); presents (0x2000) and travel/player (0x6000) are separate files. `sizeof(Save_t)` = 148,128 bytes (0x242A0), sector-aligned by the `Save` union.
- **Verified record counts:** `PLAYER_NUM` = 4, `FOREIGNER_NUM` = 1, `mPr_POCKETS_SLOT_COUNT` = 15, `mPr_INVENTORY_MAIL_COUNT` = 10, `mPr_ORIGINAL_DESIGN_COUNT` = 8 personal patterns. `keep_mail` = 8 pages x 20 = 160 town-wide saved letters; `keep_original` = 8 x 12 = 96 town-wide saved patterns; `keep_diary` = 4 players x 12 months. Each keep region carries a u16 checksum and a land ID. Bonus letters: `mCD_PRESENT_MAX` = 9.
- **Verified checksum:** `mFRm_ReturnCheckSum` adds native u16 words (odd lengths sum to zero); `mFRm_GetFlatCheckSum` stores the two's complement fixup so the region sums to zero; loads validate `ReturnCheckSum(...) == 0` alongside land ID checks.
- **Verified endianness:** the GameCube format is big-endian; `pc_save_bswap.c` byte-swaps `Save_t` (including u8 bitfield repacking) on load/save.
- **Verified reset semantics:** the `keep_*` regions are separate land-ID-keyed card regions, which is why saved letters/patterns/diaries can survive an in-game town rebuild while `Save_t` world state is reinitialized. Travel uses `mCD_foreigner_c` (checksum + player record + removed villager + copy-protect).
- **Not decomp-traced:** the "3 items per storage furniture unit" rule (contemporary guides only); NES 1-block and travel 3-block sizes (contemporary documentation); e+ expanded file layout (out of scope for the USA decomp).

### Rust rewrite implementation

`rust/src/save.rs` ports the storage architecture: physical constants (sector size, GCI header, `SAVE_DATA_OFFSET` = 0x1440), file names and sizes, the 8-entry `SAVE_FILE_TABLE` mirroring `l_mcd_file_table`, all record counts above, a 33-region `SAVE_T_REGIONS` map with decomp offsets (spans derived to the next field, including padding), the big-endian checksum trio (`checksum_sum` / `checksum_fixup` / `checksum_valid`), block/GCI math helpers, and the `ResetPreserved` town-rebuild model. C ABI exports: `pc_save_checksum`, `pc_save_checksum_fixup`, `pc_save_checksum_valid`. `cargo check --lib` passes with no new warnings. Full test suite (`cargo test --lib`): **54/54 pass**, run 2026-10-07 after Philip authorized tests; the suite's checksum-roundtrip test caught a test-only setup bug (checksum field must be zeroed before fixup), fixed. No C callers are rewired yet; full Windows game link unverified.

### Runtime Port Progress: Conversation & NPC Behavior

### Source findings

The conversation/NPC infographic was checked against `include/m_npc_personal_id.h`, `include/m_npc.h`, `src/game/m_npc_schedule.c`, `include/m_npc_schedule_h.h`, `src/game/m_msg_main.c_inc`, and `include/m_msg_data.h`.

- **Verified six personalities:** `mNpc_LOOKS_*` in `mNpc_LOOKS_*` order: `GIRL` (normal), `KO_GIRL` (peppy), `BOY` (lazy), `SPORT_MAN` (jock), `GRIM_MAN` (cranky), `NANIWA_LADY` (snooty). The Japanese bo/fu/ge/ha/ko/ta labels do not appear in these decomp identifiers.
- **Verified per-personality schedules:** `mNPS_schedule[mNpc_LOOKS_NUM]` holds one schedule table per personality, ported verbatim: normal sleeps 21:00–05:00, peppy 23:30–07:00, lazy 22:00–08:00, jock 01:00–05:30, cranky 05:00–10:00, snooty 02:30–09:00. Schedule states are `FIELD` (same acre as home), `IN_HOUSE`, `SLEEP`, `STAND`, `WANDER`, `WALK_WANDER`, `SPECIAL`.
- **Verified mood fields:** `mNpc_MOOD_0`–`mNpc_MOOD_8` (9 moods) plus `Animal_c.mood`/`mood_time` ("feel"/"feel_tim"). The index-to-meaning mapping is not established from the decomp.
- **Verified message engine:** `mMsg_ChangeMsgData` loads one of `MSG_MAX` (0x3F91) messages into the window object, resets the cursor, and sets a 20.0s timer (`mMsg_SetTimer`); `mMsg_LoadMsgData` backs it. Selection (NPC AI) and rendering (`m_msg`) are separate layers.
- **Verified catchphrase:** mutable per-villager state, `ANIMAL_CATCHPHRASE_LEN` = 10.
- **Verified friendship clamp:** `mNpc_AddFriendship` clamps to 0..=127 (established during the letter-scoring work).

### Rust rewrite implementation

`rust/src/npc.rs` ports the NPC/conversation architecture: the six personalities, the six schedule tables verbatim with `schedule_state_at`/`is_asleep` lookups, opaque 9-value moods, 10-char catchphrases, the message-window state machine (`MsgWindow::load` mirrors the cursor-reset + 20.0s timer), message script ops (text, pause, wait-input, substitution variables), dialogue-category selection from personality + context, a conversation tree (talk/favor/give/trade/bye) with answer resolution feeding mood/friendship deltas, and `NpcActor` runtime state with the source 0..=127 friendship clamp. C ABI exports: `pc_npc_schedule_state`, `pc_npc_is_asleep`. Dialogue categories, script ops, and answer effects are rewrite-owned models of the documented architecture; no authored message text or IDs are reproduced. `cargo check --lib` passes with no new warnings. Full test suite (`cargo test --lib`): **54/54 pass**, run 2026-10-07 after Philip authorized tests, including 6 new NPC tests (schedule sleep windows, transitions, catchphrase truncation, message-window stepping, friendship clamp, sleep refusal). No C callers are rewired yet; full Windows game link unverified.

### Runtime Port Progress: Buried Items

### Source findings

The buried-item brief was traced through `include/m_common_data.h`, `src/game/m_field_info.c`, `src/game/m_museum.c`, `src/game/m_all_grow_ovl.c`, `src/game/m_name_table.c`, and `src/actor/ac_event_manager.c`. The brief's core claim is confirmed: "buried" is a state attached to a town item, not a separate object class.

- **Verified buried-bit storage:** `u16 deposit[FG_BLOCK_X_NUM * FG_BLOCK_Z_NUM][UT_Z_NUM]` at save offset 0x020F1C, commented "flags for which items are buried around town". One bit per tile: `mFI_LineDepositON`/`mFI_LineDepositOFF`/`mFI_GetLineDeposit` set/clear/read bit `ut_x` of row `ut_z`; block-level wrappers `mFI_BlockDepositON/OFF`, `mFI_GetBlockDeposit`, `mFI_BkUtNum2DepositON/OFF/GET`, `mFI_Wpos2DepositON/OFF/GET`.
- **Verified geometry:** 16×16 tiles per acre (`UT_BASE_NUM`), 5×6 main acres (`FG_BLOCK_X_NUM`/`FG_BLOCK_Z_NUM`) = 80×96 town tiles. This corrects the third-party "80×80" claim flagged in the brief.
- **Verified bury operation:** `mMsm_DepositItemBlock_cancel` writes `*fg_items = deposit_item; *deposit |= (1 << ut_x);` — item ID plus buried bit together.
- **Verified daily fossils:** `mMsm_DepositFossil` keeps at most `mMsm_DEPOSIT_FOSSIL_MAX` (5) buried fossils, one per x-column of acres (`mMsm_RecordDepositFossil` sets bit `1 << (block_x + 1)`; `mMsm_GetDepositBlockNum` counts them), skipping player/shrine/station/pool/dump acres, placed with `ITM_FOSSIL` (0x2511).
- **Verified gyroids:** `mAGrw_HANIWA_NUM` (3) deposited via the same `mMsm_DepositItemBlock_cancel` path.
- **Pitfall correction:** pitfalls do NOT use item + deposit bit. `mMsm_DepositItemBlock` stores `BURIED_PITFALL_HOLE_START + hole_num` (0x002A–0x0042, 25 holes, plus reserved 0x0043–0x005B) with `ITEM_IS_BURIED_PITFALL_HOLE` / `_RSV` predicates. This refines the brief's model.
- **Verified dig conversion:** `bg_item_fg_sub_dig2take_conv` maps buried pitfall holes to `ITM_PITFALL` (0x2512) and `SHINE_SPOT` (0x005C) to bell bags by money-power roll (30k if `rng <= 2*money_power/40` or money-luck destiny, 10k if `rng <= 12*money_power/40`, else 1k); everything else passes through. Bell IDs 0x2100/0x2101/0x2102.
- **Verified interaction gate:** `Player_actor_CheckItem_fromPosition` requires `mFI_Wpos2DepositGet(...) == FALSE` — buried tiles can't be picked up by the ordinary path.
- **Verified clear sequence:** `be_flat_unit` converts with `bg_item_fg_sub_dig2take_conv`, sets the foreground item to `EMPTY_NO` (0x0000), and calls `mFI_Wpos2DepositOFF`.
- **Verified glowing-spot placement:** `mAGrw_SetShineGroundBlock` picks a random flat tile per player (`TOTAL_PLAYER_NUM`) with no item where a hole can be dug.

### Rust rewrite implementation

`rust/src/buried_items.rs` ports the burial architecture: geometry constants, verified item IDs, line/block deposit bit operations, `BurialGrid` (foreground item grid + parallel deposit array), `bury_item` (item + deposit bit), `bury_pitfall` (hole-item allocation 0x002A+, no deposit bit), `dig_up` (conversion + clear sequence), `dig2take_conv` (exact bell-roll logic), `can_pickup` (deposit gate), `is_pitfall_trap`, fossil daily-count and deposit-record bit logic, and a rewrite-owned `can_bury_item` validity heuristic (labeled as such). C ABI exports: `pc_buried_get`, `pc_buried_set`, `pc_buried_clear`. `cargo check --lib` passes with no new warnings. Unit tests were not run, per the standing instruction. No C callers are rewired yet; full Windows game link unverified.

### Runtime Port Progress: House & Nook Shop

### Source findings

The house/shop brief was traced through `include/m_player.h`, `include/m_home_h.h`, `include/m_private.h`, `include/m_shop.h`, `src/game/m_shop.c`, `src/actor/npc/ac_npc_shop_common.c`, `src/actor/ac_shop_design.c`, and `src/actor/ac_intro_demo_move.c_inc`.

- **Verified mortgage values:** `mPlayer_DEBT0` 17400 (buy house), `DEBT1` 148000 (medium), `DEBT2` 398000 (large), `DEBT3` 49800 (basement), `DEBT4` 798000 (upper). The Nook dialogue side mirrors them as `aNSC_LOAN_MEDIUM/LARGE/UPPER/STATUE(0)/BASEMENT`. The mortgage lives in `Private_c.inventory.loan` (`m_private.h:204`); the intro sets `loan = mPlayer_DEBT0` directly (17400 outstanding, confirming the brief's 19800 − 1000 − 1400 breakdown).
- **Verified house sizes:** `mHm_HOMESIZE_SMALL/MEDIUM/LARGE/UPPER/STATUE` with the decomp's own comments (medium = paid off first debt, large = second debt excluding basement, upper = third debt & basement, statue = final debt). No separate basement size — it is a flag.
- **Verified size state:** `home_size_info_s` packs `size:3`, `next_size:3`, `statue_rank:2` (0=gold, 1=silver, 2=bronze, 3=jade), `renew:1`, `statue_ordered:1`, `basement_ordered:1`, plus the upgrade order date.
- **Verified expansion rule:** `aNSC_set_talk_info_start_wait` — when construction finishes (`renew`), the new loan is assigned for the house just built: basement orders get 49800, otherwise `rehouse_loan[size-1]` = {148000, 398000, 798000, 0}. The house only changes after the existing debt reaches zero. Statue rank = town statue count capped at 3 (`Save_Get(num_statues)`).
- **Verified shop thresholds (cumulative):** `mSP_COMBINI_SUM` 25000, `mSP_SUPER_SUM` 90000, `mSP_DSUPER_SUM` 240000 (`m_shop.h`). The old guides' 65,000/150,000 are the incremental deltas, as the brief suspected.
- **Verified sales counter:** `Shop_c.sales_sum` (u32 at save offset 0x128, "current money towards upgrading shop"). `mSP_PlusSales` adds and **clamps to the current tier's threshold** — the brief's "excess is discarded" quirk, now source code.
- **Verified transaction accounting:** selling calls `mSP_PlusSales(money / 2)` (`ac_npc_shop_common.c:2220`) — half of Nook's payout; catalog orders call `mSP_PlusSales(price)` (`ac_npc_shop_common.c:2358`) and furniture orders too (`ac_shop_design.c:368`) — full price.
- **Verified tier state machine:** `mSP_GetRealShopLevel` derives the tier from the counter; Nookington's additionally requires `visitor_flag` ("set when a foreign player enters Nook's shop"). The PC port already has a `disable_shop_visitor_req` toggle for it. `mSP_RenewShopLevel` syncs the saved (displayed) level; `shop_info.upgrading_today` marks the remodeling day.
- **Verified tool lockout:** `mSP_SelectTool` — shovel always; net at 3000, rod at 8000, axe at 12000, but the lockout applies **only in Nook's Cranny**; higher tiers unlock all four.
- **Verified persistent stock:** `Shop_c.items[mSP_GOODS_COUNT]` (39 slots — confirms the save-editor finding), `rare_item`, `lottery_items[3]`, `shop_info` bitfields, `exchange_time`, `renewal_time`, `visitor_flag`.

### Rust rewrite implementation

`rust/src/house.rs` ports the house FSM: the five `mPlayer_DEBT*` values, `HouseSize`, `HomeSizeInfo`, per-player `House` with `pay`/`order_expansion`/`order_basement`/`complete_construction` (the renew-branch loan assignment)/`order_statue`/`complete_statue`, and C ABI `pc_house_next_loan`. `rust/src/shop.rs` ports the shop: tiers, cumulative thresholds, `ShopState` with `plus_sales` (exact clamp logic), `record_purchase`/`record_sale` (half payout)/`record_catalog_order`, `real_level` (with the visitor-requirement toggle), `renew_level`, `set_new_visitor`, `tool_slots` (Cranny-only lockout), the 39-slot stock, and the guide-derived per-tier category slot table (labeled as such). C ABI: `pc_shop_real_level`, `pc_shop_plus_sales`. `cargo check --lib` passes with no new warnings. Unit tests were not run, per the standing instruction. No C callers are rewired; full Windows game link unverified.

### Runtime Port Progress: Menu & Init (Scene System)

### Source findings

The menu/init brief was traced through `src/game/m_game_dlftbls.c`, `src/graph.c`, `src/game.c`, `include/game.h`, `include/m_game_dlftbls.h`, `src/first_game.c`, `src/game/m_trademark.c`, and `src/game/m_select.c`. The brief's core claim is confirmed: there is no single main-menu function — the game is a scene dispatcher.

- **Verified scene table:** `DLFTBL_GAME game_dlftbls[]` ("Display List Function TaBLe"): first_game (0), select (1), play (2), second_game (3), NULL (4, "removed & unused"), trademark (5), player_select (6), save_menu (7), famicom_emu (8), prenmi (9), pc_model_viewer (10, `#ifdef TARGET_PC`). Each entry holds an init function pointer, a cleanup pointer, and `alloc_size` (`sizeof(GAME_<class>)`).
- **Verified GAME struct:** `exec` at 0x0, `cleanup` at 0x8, `next_game_init` at 0xC, `next_game_class_size` at 0x10 — confirming the brief's offset-0xC observation. `frame_counter` at 0xA0.
- **Verified transition mechanism:** `GAME_GOTO_NEXT` sets `doing = FALSE` and records `next_game_init` + the next scene's state size; `game_get_next_game_dlftbl` matches that init pointer against the table (`ARE_INIT_PROCS_EQUAL` comparisons) to find the next entry.
- **Verified lifecycle:** `graph_proc` starts at `game_dlftbls[0]`, then loops: `malloc(alloc_size)` → `game_ct(init)` (sets `doing = TRUE`, clears next) → per-frame `graph_main` → `game_main` → `scene->exec()` while doing → resolve next entry → `game_dt` (cleanup) → `free`.
- **Verified transitions:** first_game → second_game (`first_game.c:16`); trademark → play (`m_trademark.c:183`); select → play (`m_select.c:25`).
- **Doc correction:** the earlier "player_select (scene 19)" note was wrong; the current decomp places player_select at table index 6 (fixed above).

### Rust rewrite implementation

`rust/src/scene.rs` ports the dispatcher: `SceneId` with verified table indices (index 4 maps to nothing), `SCENE_TABLE` with init-function names, the `Scene` trait (init/exec/cleanup), `SceneRequest` (Continue/Goto/Shutdown), and `SceneManager` modeling the `doing` flag, the next-init pointer, and table-based transition resolution. Also models the boot chain (`BOOT_CHAIN`), marking the six PC-port wrapper stages vs the original game's `ac_entry` → `boot_main` → `entry` → `mainproc` → `graph_proc`. C ABI: `pc_scene_table_index`, `pc_game_dlftbls_count`. Scene state sizes are per-build C values and are not reproduced. `cargo check --lib` passes with no new warnings. Unit tests were not run, per the standing instruction. No C callers are rewired; full Windows game link unverified.

### Runtime Port Progress: Per-Scene Init/Exec Bodies

### Source findings

The brief's deep-dive was verified function-by-function against the decomp:

- **Verified exec installation:** `play_init` sets `game->exec = play_main; game->cleanup = play_cleanup;` (`m_play.c:466-467`). Every other scene's init installs its own exec: `second_game_main`, `trademark_main`, `select_main`, `player_select_main` (`player_select.c:228`), `save_menu_main`, `famicom_emu_main`, `prenmi_main`. **Exception:** `first_game_init` never installs an exec — it does ROM/save setup and immediately calls `GAME_GOTO_NEXT(game, second_game, SECOND)` during init, so its frame loop never runs.
- **Verified `game_main` wrapper** (`game.c:138`): frame-rate bookkeeping → `game_draw_first` → `mTM_time` → `this->exec(this)` (between GAME_EXEC markers) → `mBGM_main` → `game_move_first` → `frame_counter++`.
- **Verified `play_main` structure** (`m_play.c:858`): controller/debug setup → `Game_play_move(game)` → `Game_play_draw(play)` → overlay/debug drawing, with doing-point instrumentation markers.
- **Verified Pre-NMI path:** `graph_main`'s reset check does `GAME_GOTO_NEXT(game, prenmi, PRENMI)` when the reset status is `IRQ_RESET_PRENMI` and the scene hasn't disabled it (`graph.c:363`).
- **Verified wipe/fade separation:** `Game_play_fbdemo_wipe_*` functions are a visual system destroyed separately from the scene — wipes hide transitions but don't control scene lifetime.
- **Verified level-2 scene data:** `mSc_SCENE_DATA_TYPE_*` — player, ctrl actor, actor, object-exchange bank, door data, field, my room, arrange room, arrange furniture, sound — processed by `Scene_ct()` inside `play_init()`. This is a world/room initializer, not the top-level scene system.
- DLFTBL entry 4 confirmed as `DLFTBL_NULL()` ("removed & unused"); the dispatcher skips it.

### Rust rewrite implementation

Extended `rust/src/scene.rs`: `SCENE_EXEC_TABLE` with the verified per-scene exec/cleanup names (first_game has no exec), `SCENE_TABLE_NULL_INDEX = 4`, `GAME_MAIN_PHASES` (the six `game_main` wrapper phases), `PlayExecPhase` (Move/Draw), `VisualTransition` (wipe/fade as separate from scene transitions), `SceneDataKind` (the 10 level-2 scene-data types), and `SceneManager::goto_play/goto_famicom_emu/goto_prenmi` transition helpers mirroring the C helpers. The `Scene` trait docs now record the init-installs-exec contract and the first_game exception. **Build fix:** an earlier edit had accidentally replaced `mod scene;` with `mod behavior;` in `lib.rs`, silently dropping the scene module from the build (its tests never ran, its C ABI exports never existed); restored. `cargo test --lib`: 94/94 pass. No C callers are rewired; full Windows game link unverified.

### Runtime Port Progress: Villager Movement

### Source findings

The brief's "no global pathfinder" claim was verified function-by-function against the decomp:

- **Verified wander destination** (`ac_npc_think_wander.c_inc:20`): `angle = RANDOM(360°)`, `dst = center + (sin(angle), cos(angle)) * radius` — a continuous world-space point, filtered by foreground (empty/item/furniture only), `mCoBG_Wpos2CheckNpc`, and the movement-range check.
- **Verified movement struct** (`ac_npc.h`): `dst_pos` (goal) vs `avoid_pos` (steering target), range center/radius/type, `mv_angl`, speed with max/acceleration/deceleration.
- **Verified range types:** block, circle, square (square is later-revision only, `#if VERSION >= VER_GAFU01_00`).
- **Verified avoidance probes** (`ac_npc_think.c_inc:283`): `add_angl` table is literally {±22.5°, ±45°, ±90°}, probed two unit-widths ahead; recursion `n → n+1` on failure; fallback is a 180° turn. Badly-stuck fallback is a random ±112.5° turn (`turn_angl_table`).
- **Verified decide tables** with the source's own probability comments: normal 40/30/30 wait/walk/run, peppy 70/20/10, lazy 60/20/20, jock 30/20/50, cranky 40/30/30, snooty 50/40/10. Roll is `RANDOM(10)` against the borders. Fatigued/sleepy villagers always wait.
- **Verified friendship movement** (`ac_npc_move.c_inc:507`): same block + friendship < 0 → AVOID; > 128 → SEARCH.
- **Verified clap check** (`ac_npc_think.c_inc`): normal feel + player catching fish/bug + within 3 units + facing within 67.5°.
- **Verified go-home:** `house + (20, 60)` requested as WALK.
- **Verified walk dispatch:** move / avoid-move / search-move / to-point-move.

### Rust rewrite implementation

`rust/src/movement.rs` ports the steering system: `MoveRangeType`, `FriendshipMode`, `WalkProc`, `WanderChoice`, `DECIDE_BORDERS` verbatim with `decide_wander`, `AVOID_PROBE_ANGLES`/`TURN_BACKWARD_ANGLES`/`GO_HOME_OFFSET` constants, `friendship_mode`, `Movement` (dst/avoid split, `set_dst`/`set_avoid`/`restore_dst`, acceleration-based `step`, circular `wander_destination`, `probe_avoid`, circle containment), and `check_clap`. C ABI: `pc_wander_choice`, `pc_friendship_mode`. `cargo test --lib`: 101/101 pass. No C callers are rewired; full Windows game link unverified.

### Runtime Port Progress: Player Movement

### Source findings

The brief's pipeline was verified against the player sources:

- **Verified controller layer** (`m_player_controller.c_inc:156`): getters for `move_pR`, `move_angle`/`last_move_angle`, `adjusted_pR`/`last_adjusted_pR` read from `gamePT->mcon` (title demo uses its own copy). The movement code never sees raw stick values.
- **Verified turn smoothing** (`m_player_main_walk.c_inc:131`): `movePR >= 1.0 → mod = 0.5`; `<= 0.05 → mod = 0.01`; else `0.01 + 0.5157895 * (movePR - 0.05)`; then `add_calc_short_angle2(&target, angle, CALC_EASE(mod), 2500, 50)` where `CALC_EASE(x) = 1 - sqrt(1 - x)` (`ac_museum_insect_priv.h:19`). Stronger stick = faster turning.
- **Verified state reuse:** `Player_actor_Movement_Run` is literally `Player_actor_Movement_Walk` (`m_player_main_run.c_inc:57`); `Player_actor_Movement_Dash` is literally `Player_actor_Movement_Run` (`m_player_main_dash.c_inc:99`). DASH → RUN → WALK share one steering core.
- **Verified dash speed:** `movePR = (7.5 * movePR) / over_norm` when the dash button is held (`mPlib_CheckButtonOnly_forDush`, B/L/R).
- **Verified animation coupling:** `sp = 0.6 * sqrt((speed * over_norm) / 7.5)`; near one wall, `sp *= sqrt(|sin(wall_angle − facing)|)` clamped to 0.22 minimum.
- **Verified braking:** `Player_actor_Movement_Base_Braking` with amount `0.32625001` (`m_player_common.c_inc:1494`).
- **Verified dash terrain sampling:** the 12 offsets verbatim (0/±20 lateral at 0, 28.28, 56.57, 84.85 forward), each checked with `mCoBG_GetBgNorm_FromWpos`.
- Player vs villager: same background-collision world, but the player steers from the stick (no destination) while villagers steer toward AI destinations.

### Rust rewrite implementation

`rust/src/player_move.rs` ports the locomotion core: `ControllerMove` (the `mcon` fields), `turn_mod`/`calc_ease`/`smooth_turn_toward` (shortest-arc facing), `LocomotionState` with the `movement_core` hierarchy (Dash→Run→Walk), `anim_speed`/`anim_speed_near_wall`, `BRAKE_AMOUNT`, `DASH_SAMPLE_OFFSETS` verbatim, and `PlayerMovement` (`step_core`, `brake`). C ABI: `pc_turn_mod`, `pc_locomotion_core`. `cargo check --lib` clean. Unit tests were written but NOT run, per the standing instruction. No C callers are rewired; full Windows game link unverified.

### Runtime Port Progress: Collision

### Source findings

The brief's two-system model was verified against the headers and sources:

- **Verified 4-byte unit record** (`m_collision_bg.h:161`): bitfields slate:1, center:5, top_left:5, bot_left:5, bot_right:5, top_right:5, attribute:6 — five height samples plus a terrain attribute per unit.
- **Verified query bounds:** `mCoBG_UNIT_VEC_INFO_MAX` = 128 wall vectors per query, `mCoBG_WALL_COL_NUM` = 2 stored wall contacts, `mCoBG_MOVE_REGIST_MAX` = 64 moving-background registrations.
- **Verified wall kinds** (`m_collision_bg.c:753`): normal, attribute, move.
- **Verified slate walls** (`m_collision_bg.c:87`): diagonal normals at 45°/135° from diagonal height comparison (`mCoBG_WALL_SLATE_UP/DOWN`).
- **Verified hit flags** (`mCoBG_HIT_WALL`, `_FRONT/_RIGHT/_LEFT/_BACK`) and result fields (on_ground, hit_wall_count, is_in_water, is_on_move_bg_obj, `rev_pos`).
- **Verified object colliders** (`m_collision_obj.h`): joint sphere / pipe / triangle types, 50-collider table (`Cl_COLLIDER_NUM`), groups PLAYER/GROUP_2/GROUP_3, masses immovable/heavy/normal.
- The engine reconstructs local geometry from the unit records (triangles, wall segments with top/bottom/normal) and tests the previous→current movement segment against it — swept-style, producing a `rev_pos` correction along the wall normal. Object overlaps produce `collision_vec` separation split by mass.

### Rust rewrite implementation

`rust/src/collision.rs` ports both systems: `CollisionData` with exact bit packing/unpacking and flat detection, `UnitArea`, `WallKind`, `SlateDir` with diagonal-comparison detection, `WallSeg` (signed distance, normal-based correction), directional hit flags, `BgResult`, the neighborhood-size rule (3/5/7 by range), plane-equation ground height, ground Y correction, `ColliderType`/groups/`Mass` with mass-split separation rules, and sphere-overlap depth. C ABI: `pc_collision_neighborhood`, `pc_collision_pack`. `cargo check --lib` clean. Unit tests were written but NOT run, per the standing instruction. No C callers are rewired; full Windows game link unverified.

### Runtime Port Progress: Inventory Scan (Impulse Buying)

### Source findings

The brief's inventory-query model was verified against `m_private.c`/`m_private.h`/`m_npc.c`:

- **Verified possession primitive** (`m_private.c:314`): `mPr_GetPossessionItemIdx` is a linear scan over `priv->inventory.pockets` (`mPr_POCKETS_SLOT_COUNT` = 15), returning the first matching slot or -1. Deterministic, no RNG, stops at first match.
- **Verified condition-aware variant** (`m_private.c:335`): `mPr_GetPossessionItemIdxWithCond` also requires the 2-bit condition to match; conditions are packed via `mPr_GET_ITEM_COND` = `(conds >> (slot << 1)) & 3`.
- **Verified count variants:** `mPr_GetPossessionItemSum` counts matches ("how many" vs "where").
- **Verified free-slot reuse** (`m_private.c:651`): finding an empty pocket is `mPr_GetPossessionItemIdx(priv, EMPTY_NO)` — the same primitive.
- **Verified NPC furniture filter** (`m_npc.c:3066`): `mNpc_CheckSelectFurniture` excludes clothing, umbrellas, insects, fish, gyroids, identified fossils, and NES games; `mNpc_DecideNpcFurniture` counts eligible house furniture and picks randomly into `reward_furniture`.
- "Impulse buying" is community terminology, not a decomp function. The confirmed architecture is two-stage: NPC logic picks a candidate (mechanism untraced), then the possession primitive checks the pockets. The exact candidate-selection algorithm (random pocket vs random item vs favorite-first) is not yet proven.

### Rust rewrite implementation

`rust/src/inventory.rs` ports the query layer: `Inventory` (15 pockets + packed conditions), `find_item`/`find_item_with_cond`/`count_item`/`count_item_with_cond`/`find_free_slot`/`put`, `item_cond`/`set_item_cond` with the exact shift math, `ExcludedFurniture` + `selectable_furniture`, `CandidateStrategy` (FavoriteFirst/RandomCarried, marked rewrite-owned/untraced), and `resolve_candidate`. C ABI: `pc_inventory_find`, `pc_inventory_count`. `cargo check --lib` clean. Unit tests were written but NOT run, per the standing instruction. No C callers are rewired; full Windows game link unverified.

### Runtime Port Progress: Villager Behavior Engine

### Source findings

The brief's AI-architecture claims were traced through `include/ac_npc.h`, `src/actor/npc/ac_npc2_action.c_inc`, `src/actor/npc/ac_npc2_think.c_inc`, `src/game/m_npc.c`, and `include/m_quest.h`. The "selection, not generation" model is confirmed at the code level:

- **Verified action arbiter:** `aNPC_set_request_act` records a requested action only when its priority is >= the pending priority (`ac_npc2_action.c_inc:352`). The action proc then dispatches through a per-action function table (`aNPC_act_proc`).
- **Verified action vocabulary:** `aNPC_ACT_*` — wait, walk, run, turn, chase insect, chase fish, greeting, talk, into/leave house, umbrella open/close, play music (ensou), react to tool, clap, get, change cloth, pitfall, revive, special. Requests carry a kind (DEFAULT/AVOID/SEARCH/TO_POINT), a target (player/any NPC/target NPC/ball/insect/fish), six argument words, and a separate head-tracking request.
- **Verified move-out selection:** `mNpc_SetRemoveAnimalNo` prefers a villager ALL players have met, then one SOME player has met, then falls back to uniform random. The popular "ignore them and they leave" claim is contradicted by the source — met villagers are *preferred* for removal.
- **Verified move-in validation:** a transferring villager is rejected when already in town, when it was the most recently removed, or when it is the current summer camper; its record (identity, personality, catchphrase, memories) travels with it, which is why moved villagers remember their former town.
- **Verified letter bonuses:** `mQst_LETTER_SCORE_BONUS` = 3 (good score), `mQst_LETTER_PRESENT_BONUS` = 6 (present attached).
- Mood remains an opaque index in the decomp (`npc.rs` `Mood`); the Normal/Happy/Angry/Sad presentation names come from contemporary player documentation, not source identifiers.

### Rust rewrite implementation

`rust/src/behavior.rs` ports the engine: `BehaviorAction` (23 actions in decomp order), `ActionKind`, `ActionTarget`, `ActionRequest` with the priority-wins rule, head tracking, `VisibleMood` with interaction gating (angry/sad can refuse talk), `WorldEvent` → `reaction_for` (fish/bug caught → clap, pitfall → trapped state, tool use → react), the `FavorState` machine (None/Offered/Accepted/InProgress/Completed/Rewarded with NPC-chained deliveries), `BehaviorStage` pipeline ordering (schedule → world events → mood → activity → movement → interaction), `VillagerBehavior` runtime state, `select_move_out` with the met-preference order, `transfer_allowed` validation, `TransferRecord`, and letter→friendship via the `m_quest.h` bonuses clamped to the 0..=127 GameCube friendship range (not New Horizons' 0-255). C ABI: `pc_letter_friendship_delta`. `cargo check --lib` passes with no new warnings. Unit tests were not run, per the standing instruction. No C callers are rewired; full Windows game link unverified.

### Runtime Port Progress: Villager Interest & Interaction

### Source findings

The brief's "no formal hobby system" warning was honored and its open questions were traced through `include/m_npc.h`, `src/game/m_npc.c`, and `include/m_quest.h`:

- **Verified feel (mood) enum:** `mNpc_FEEL_*` — Normal, Happy, Angry, Sad, Sleepy, Pitfall, plus two "uzai" (pestering) feels, 9 total (`mNpc_FEEL_ALL_NUM`). This corrects the earlier `npc.rs` note that claimed the decomp does not name the moods.
- **Verified talk-frequency mechanic:** `m_npc.c` has per-villager talk info (timer, talk_num, quest_request flag, unlock/reset timers) and a per-feel temper table `l_npc_temper` with verbatim values: Normal (4000, 12, 15), Happy (3000, 10, 13), Angry (4000, 12, 15), Sad (4000, 10, 13), Sleepy (5000, 9, 12), Pitfall (5000, 9, 12) as (unlock_timer, over_impatient_num, talk_num_max). `mNpc_GetOverImpatient` returns MILDLY_ANNOYED past the impatient threshold and ANNOYED (refuse to talk) past the max. Happy villagers lose patience faster than normal ones.
- **Verified quest system:** `mQst_QUEST_TYPE_*` — Delivery (kinds: normal/foreign/removed/lost, with sender + recipient IDs = the chained-favor machinery), Errand (chain/first-job types; first-job quests include deliver furniture, send letter, deliver carpet/axe, post notice, introductions), Contest (fruit, ball, snowman, flower, fish, insect, letter).
- Inventory inspection ("impulse buying") remains player-documented but untraced to a source function; individual item-preference structures also remain untraced. Both are marked as such in the code.

### Rust rewrite implementation

`rust/src/interaction.rs` ports the interaction layer: `Feel` (named moods), `TEMPER_TABLE` verbatim, `TalkInfo` with `count_talk`/`patience`/`over_impatient`, `Patience`, `QuestType`/`DeliveryKind`/`ContestKind`/`ErrandType`, the five `InterestLayer`s (personality, individual, current desire, opportunistic, environmental — no hobby field), `InteractionKind`, and `InteractionContext` with the selection hierarchy (annoyed → refused; pending request → request; held item → item trade; else conversation). C ABI: `pc_npc_patience`. Also corrected the `npc.rs` mood comment to reference the feel enum. `cargo check --lib` passes with no new warnings. Unit tests were not run, per the standing instruction. No C callers are rewired; full Windows game link unverified.

### Runtime Port Progress: Video Interface

The image shows a game/rendering path ending in a platform presentation step. Repository code supports a narrower fact: the PC `VIWaitForRetrace` implementation polls SDL events, drains pending GX work, swaps the window, applies frame pacing, records profiler data, and advances the PC frame counter. Those operations now live in `pc/rust/src/vi.rs`; calls still enter through the Dolphin VI API in `include/dolphin/vi.h`. This does not move scene logic, display-list generation, GX rendering, or gameplay into Rust.

`pc/src/pc_vi.c` remains as a source reference and is excluded from the active PC target. `VIConfigurePan` remains in `pc/src/pc_stubs.c`; it was not part of this migration. The image's world simulation, character AI, save details, and proposed clean-room architecture are not established by this change and need separate source tracing before design or migration.

### Runtime Port Progress: Villager Mail Check

The infographic's seven letter-scoring checks are present in `src/game/m_mail_check_ovl.c`; `mNpc_CheckNormalMail_nes` applies the source thresholds (`>=100` good, `<50` bad, and the middle band remains the neutral rank). The infographic's abbreviated weights need care: the code has separate punctuation/capitalization, trigram, repetition, whitespace, run-on, and block-spacing checks, with behavior defined by the source loops.

This increment ports the bounded repeated-character scan `mNpc_CheckNormalMail_sub` from `src/game/m_npc.c` to `pc/rust/src/villager_mail.rs`, retaining its C ABI. It counts non-space bytes in the fixed `MAIL_BODY_LEN` buffer and flags ordinary-character and selected punctuation/control-code runs at different thresholds. `mNpc_CheckNormalMail_length` uses this result, and `mQst_GetMailRank` in `src/game/m_quest.c` uses that length/rank for the letter-contest bonus. The C definition remains for non-PC builds. The source's friendship clamp is `0..=127` in `mNpc_AddFriendship`; the infographic's `0..255` scale is contradicted by that implementation.

The US trigram table comment in `m_mail_check_ovl.c` confirms missing `0x7F` terminators in the original table data and describes a scan continuing into adjacent bytes until a terminator or match. The PC `BUGFIXES` path supplies table terminators. The image's numeric comparison to another region and its claim about the number of accepted trigrams are not established by repository code. The personality demographics, conversation pipeline summary, and move-in/move-out description remain leads pending their own source traces.

### Runtime Port Progress: Letter Scoring Engine

This increment ports the full letter scoring system to `pc/rust/src/letter_score.rs` (with generated trigram data in `pc/rust/src/letter_score_tables.rs`), building on the earlier `villager_mail.rs` repeat-check port. All algorithms were verified line-by-line against the decompilation (`src/game/m_mail_check_ovl.c`, `src/game/m_npc.c`, `src/game/m_quest.c`) before porting.

The normal-letter scorer (`mMck_check_key_hit_nes`) implements the seven checks: A (+20 terminal punctuation when the body is under 192 bytes; +/-10 per separator with a capital within 3 characters after it, needing more than 3 characters remaining), B (+3 per valid word-start trigram), C (+20/-10 first-character capitalization), D (-50 triple alpha repeat, raw body, no space stripping), E (+20/-20 at a 20% space ratio), F (-150 for 75+ characters after a `.`/`?`/`!` separator; spaces count, separator-free letters never trigger), G (-20 per complete 32-character window without a space). Thresholds `>= 100` (positive reply), `50-99` (no reply), `< 50` (negative reply); friendship moves `-2/+1/+3/+6` (bad/good x present), clamped `0..=127`.

The quest ranker (`mQst_GetMailRank` via `mNpc_CheckNormalMail_length`) computes rank `0-11` = length tier (`17`/`49` non-space chars) + trigram bonus (`0`/`3`, needs `>= 30%` hit rate or the lenient default) + present bonus (`6`). The existing `mNpc_CheckNormalMail_sub` port is reused for the run-on/character-count piece.

Trigram tables: the 776 intended pairs were extracted from `str_a_table..str_z_table` (per-table counts `57/49/44/.../1` match the published research exactly). Two modes are implemented: `TrigramMode::Intended` (proper terminators, matching the port's `BUGFIXES` build) and `TrigramMode::NtscU`, which reproduces the missing-`0x7F` bug by scanning tables consecutively (verified: `"aab"` scores `0` intended vs `+3` NTSC-U). The post-Z RAM tail is modeled via `TRIGRAM_RAM_EXTRA` (758 byte pairs reconstructed from Hunter R.'s `trigrams-bugged.txt` `~~~` section using the decomp's `m_font.h` charset; 21 obscure entries omitted as documented). With the tail, per-letter effective counts match Hunter's published table (A=1000 ... Z=780, total 23,670) to within the 21 omitted pairs (verified: `"A! "` scores `0` intended vs `+3` NTSC-U via the tail's `('!',' ')` pair). The C ABI (`mMck_check_key_hit_nes`, `mMck_check_key_hit`, `mQst_GetMailRank`) defaults to `Intended`, consistent with the port's `BUGFIXES` build.

Source findings that correct earlier research prose: check D does not strip spaces; the run-on rule needs 4 ordinary / 9 symbol repeats (the counter resets to 0, so "3+"/"up to 7" summaries are simplifications — C and the Rust port agree); check F only fires after a separator; friendship clamps at `0..=127`, contradicting the infographics' `0..255`. An empty body would read one byte before the buffer in C (undefined behavior); the Rust port scores it as 0 instead. Unit tests cover each check, both trigram modes, quest extremes, and the `"!!!!!"` case; `cargo check --lib` is clean. Tests were written but not run (standing rule); the i686 Windows build runs on the MSYS2 machine. Workbook rows 31-34 and new Image Research Leads rows record all findings.

**ABI audit (2026-10-07).** Three findings against the decomp headers:
1. **Fixed:** `mMck_check_key_hit`'s Rust parameters were reversed vs the C declaration (`m_mail_check_ovl.h:29`: `int mMck_check_key_hit(int* len, u8* str)`). The Rust export now takes `(len: *mut i32, str_: *const u8)` in C order. Had the old order ever interposed, every C caller would have passed the word-count pointer as the body.
2. **Fixed:** `mQst_GetMailRank`'s second parameter is now `u16`, matching the C `mActor_name_t` (was `i32`; benign on i686 cdecl but wrong).
3. **Resolved 2026-10-07 — the letter engine is now live-wired:** `m_mail_check_ovl.c` is excluded from `GAME_C_SOURCES` in `CMakeLists.txt` (following the file's own precedent: "exclude the individual files to avoid duplicate symbol definitions"). Research before the change: the file defines only the two scoring globals (trigram tables are static); its only callers are `m_mail_check.c:6` (`mMC_get_mail_hit_rate` → `mMck_check_key_hit`) and `m_npc.c:1238` (`mMck_check_key_hit_nes`), both of which now bind to the Rust exports. The game build defines `BUGFIXES` globally and the Rust exports use `TrigramMode::Intended`, which matches the BUGFIXES table terminators — behavior is preserved. `mQst_GetMailRank` remains `static` in `m_quest.c`, so its Rust export still can't intercept that call site; wiring it needs a C-side patch (not done). Full Windows game link still needs verification on the MSYS2 machine.

### Runtime Port Progress: Real-Time/Calendar Engine

This increment ports the GameCube time stack to `pc/rust/src/game_time.rs`, verified against the decompilation before porting. The architecture has four layers:

**OS time** (`src/static/dolphin/os/OSTime.c`): 64-bit `OSTime` ticks at 40.5 MHz (bus clock / 4), epoch 2000-01-01 (`GC_UNIX_EPOCH_DIFF = 946684800`). `ticks_to_calendar_time` / `calendar_time_to_ticks` port the Gregorian conversion exactly, including `BIAS = 0xB2575`, `wday = (days + 6) % 7`, and the standard leap-year rule.

**Game RTC** (`src/lb_rtc.c`): `RtcTime` (1-based month, 0=Sunday weekday), `rtc_week` (weekday via days since 1901-01-01), `get_days_by_month`, `weekly_day` (nth-weekday, e.g. 4th Thursday of November; `LAST_WEEKDAY_OF_MONTH` for the last one), and `interval_days`. The interval calculation faithfully reproduces the original's simplified century-blind leap counting — this is a real quirk, not a bug to fix. Time add/subtract helpers handle month/year rollover.

**Game clock** (`src/game/m_time.c`): the key architectural finding is `GameClock { time_delta }` — the in-game Set Clock never touches hardware; `lbRTC_SetTime` stores `time_delta = desired_ticks - hard_ticks` and `lbRTC_GetTime` returns `hard_ticks + time_delta` (confirmed in `lb_rtc.c:148-165,243-260`). Seasons use the real 18-term `mTM_calender` table (not just four seasons), `term_index` reproduces `mTM_get_termIdx` exactly, `FIELD_RENEW_HOUR = 6` is the daily reset, `renewal_needed` compares save vs current ymd, and `clamp_year` enforces 2001-2030 (GC) / 2100 (PC). `game_day_ymd` maps pre-06:00 times to the previous game day.

**Astronomical** (`src/lb_reki.c`): `vernal_equinox_day` / `autumnal_equinox_day` use the original float formulas from the Japanese astronomy reference; `harvest_moon_day` uses the GameCube precomputed table (2002-2030). Note: the PC port corrected 17 Harvest Moon dates (e.g. 2026: Sep 26 GC vs Sep 25 corrected); the Rust port keeps the GC table for faithfulness.

**Event primitives**: `DatePredicate` (Fixed, NthWeekday, Weekly, HarvestMoon, VernalEquinox, AutumnalEquinox) and `EventWindow` (date range + daily time window, e.g. Joan Sundays 6:00-12:00) mirror how `m_event.c` builds schedules.

15 unit tests pass (authorized run): epoch/weekday/roundtrips, leap years, nth-weekday spot checks (Thanksgiving 2026, etc.), season terms, 6 AM boundary, renewal, year clamps, delta-based Set Clock, equinoxes, event predicates. Workbook rows 36+ record the findings.

## File Reference

### PC Port Layer (what we wrote)

#### Core

| File | Purpose |
|------|---------|
| `pc/src/pc_main.c` | Entry point, SDL2/GL init, CLI flags, DPI scaling |
| `pc/src/pc_gx.c` | GX → OpenGL: all GX API functions, vertex submission, state, draw dispatch, dirty-flag uniform system |
| `pc/src/pc_gx_tev.c` | TEV shader: GLSL program loading, uniform upload |
| `pc/src/pc_gx_texture.c` | 10 GC texture format decoders, 2048-entry cache with FNV-1a |
| `pc/src/pc_os.c` | Dolphin OS: memory arena, timers, calendar time, message queues, thread stubs |
| `pc/rust/src/dvd.rs` | Rust PC DVD filesystem shim; preserves the Dolphin `DVD*` C ABI and disc/extracted-file lookup behavior |
| `pc/rust/src/vi.rs` | Rust PC Video Interface shim; preserves VI APIs, frame counter globals, event/swap boundary, pacing and retrace counting |
| `pc/rust/src/villager_mail.rs` | Rust port of the fixed-size villager-mail repeat check; preserves `mNpc_CheckNormalMail_sub` C ABI |
| `pc/rust/src/letter_score.rs` | Rust letter scoring engine: normal 7-check scorer, quest 0-11 ranker, dual trigram modes; C ABI (`mMck_check_key_hit_nes`, `mMck_check_key_hit`, `mQst_GetMailRank`) |
| `pc/rust/src/game_time.rs` | Rust real-time/calendar engine: OSTime<->calendar, game RTC, Set-Clock delta, 18-term seasons, 6 AM renewal, equinoxes/harvest moon, event predicates |
| `pc/rust/src/letter_score_tables.rs` | Generated: 776 intended trigram pairs extracted from the decomp's `str_a_table..str_z_table` |
| `pc/rust/src/aram.rs` | Rust 16 MiB ARAM buffer, bump allocator, DMA and synchronous ARQ compatibility |
| `pc/rust/src/gbi_runtime.rs` | Rust GBI runtime pointer pack/unpack shim used by N64 display-list macros |
| `pc/rust/src/profiler.rs` | Rust frame profiler using SDL's cross-platform performance counter and the existing `pc_profiler_*` C ABI |
| `pc/rust/src/mtx.rs` | Rust matrix/vector and libultra fixed-point helpers used by the C game and renderer |
| `pc/src/pc_misc.c` | HW register arrays, EXI/SI/PPC stubs, malloc wrappers, trig |

#### Asset Loading

| File | Purpose |
|------|---------|
| `pc/rust/src/lib.rs` | Rust GC disc image I/O (CISO/ISO/GCM), FST parsing, Yaz0 decompression; C ABI |
| `pc/rust/Cargo.toml` | Rust static library manifest for the PC runtime layer |
| `pc/src/pc_assets.c` | Auto-generated: ROM extraction, asset table, per-file loaders, byte-swap |
| `pc/tools/gen_runtime_assets.py` | Source scanner: transforms .inc includes to runtime loads, generates pc_assets.c |

#### I/O and Storage

| File | Purpose |
|------|---------|
| `pc/src/pc_card.c` | Memory card API → local file save/load |
| `pc/src/pc_m_card.c` | Memory card manager: GCI save/load, village generation, ARAM data blocks |
| `pc/src/pc_save_bswap.c` | GCI save file bidirectional LE↔BE byte-swap (Dolphin-compatible) |
| `pc/src/pc_pad.c` | Keyboard + SDL2 gamepad input (GC button format) |
| `pc/src/pc_audio.c` | SDL2 audio: 32kHz s16 stereo, dedicated producer thread + SPSC ring buffer |
| `pc/rust/src/aram.rs` | 16 MiB ARAM buffer, bump allocator, DMA and synchronous ARQ compatibility |

#### Enhancements

| File | Purpose |
|------|---------|
| `pc/src/pc_settings.c` | Runtime `settings.ini` parser/writer (resolution up to 4K, fullscreen, vsync, MSAA) |
| `pc/src/pc_texture_pack.c` | Dolphin-compatible HD texture pack loader (XXHash64 matching, DDS, preloading) |
| `pc/src/pc_model_viewer.c` | Debug model viewer: 75 building/structure models, orbit camera |

#### Support

| File | Purpose |
|------|---------|
| `pc/src/pc_stubs.c` | Remaining link stubs (GBA, famicom, libultra, threads) |
| `pc/src/pc_stubs_cpp.cpp` | JSystem C++ vtable stubs |
| `pc/src/pc_fontdata.c` | Embedded font (byte-swapped for LE) |
| `pc/shaders/default.vert` | GLSL vertex shader (runtime-loaded, required) |
| `pc/shaders/default.frag` | GLSL fragment shader (runtime-loaded, uniform-driven TEV stages with bias/scale/clamp/swap) |

#### Headers

| File | Purpose |
|------|---------|
| `pc/include/pc_platform.h` | Platform config, 32-bit guard, SDL2/GL includes, crash API, widescreen defs |
| `pc/include/pc_gx_internal.h` | PCGXState, PCGXVertex, PCGXTevStage, indirect texture structs |
| `pc/include/pc_save_bswap.h` | GCI save byte-swap API |
| `pc/include/pc_model_viewer.h` | Model viewer struct and init/cleanup |
| `pc/include/pc_bswap.h` | `pc_bswap16/32/64` macros + array swap helpers |
| `pc/include/pc_settings.h` | Settings struct and load/save/apply API |
| `pc/include/pc_texture_pack.h` | Texture pack init/lookup/shutdown API |
| `pc/include/pc_disc.h` | Disc image I/O and FST lookup API |
| `pc/include/pc_assets.h` | Asset loader init and per-asset load API |
| `pc/include/pc_types.h` | Platform type definitions |
| `pc/include/pc_diag.h` | Diagnostic output macros (PC_DIAG) |

### Critical Decomp Modifications

These are the most-modified files from the upstream decompilation:

| File | Why |
|------|-----|
| `src/static/libforest/emu64/emu64.c` | Texture cache routing, TEXEL1, vertex colors, fog guard, per-stage texture binding, widescreen NOOPTag handling |
| `include/libforest/gbi_extensions.h` | 30 GBI bitfield structs reversed for LE x86 |
| `src/static/libforest/emu64/emu64_utility.c` | seg2k0 proximity heuristic, N64Mtx byte-swap |
| `src/static/boot.c` | Arena init, REL skip, actable endian swap |
| `src/graph.c` | Frame loop diagnostics, model viewer routing |
| `src/padmgr.c` | GC→N64 button conversion, once-per-frame guard |
| `src/game/m_play.c` | Scene transition diagnostics, fog BG fix, widescreen stretch markers |
| `src/game/m_player_lib.c` | Player palette byte-swap from ARAM |
| `src/game/m_field_make.c` | FG data u16 byte-swap (3 swap sites) |
| `src/game/m_room_type.c` | Room wall/floor palette u16 byte-swap |
| `src/game/m_scene.c` | Scene_Word_u endianness fix |
| `src/sys_matrix.c` | Matrix_MtxtoMtxF endian swap, suMtxMakeTS/SRT/SRT_ZXY fixes |
| `src/game/m_npc.c` | Title demo animal slot cleanup: clear before write, skip sentinel entries |
| `src/game/m_trademark.c` | Clear npclist before demo repopulation, sentinel entry for demo_npc_list |
| `src/actor/npc/ac_npc_think_wander.c_inc` | Clamp `looks` before indexing decide_boarder[] (latent OOB bug) |
| `src/game.c` | Frame timing and game exec dispatch |
| `src/jaudio_NES/na_combo.c` | Melody sequence u16 offset byte-swap |

About ~100 decomp files total are modified. Most changes are small `#ifdef TARGET_PC` blocks for byte-swapping or platform adaptation.

## Rendering Pipeline

### Vertex Submission

Deferred commit model. A position call commits the *previous* vertex. Auto-flush via `pc_gx_flush_if_begin_complete()` when expected vertex count is reached (handles missing GXEnd). Explicit GXEnd calls added at end of dl_G_TRIN/dl_G_QUADN/dl_G_TRI2 to prevent batches from flushing after viewport changes.

VAO attribute pointers and quad-to-triangle EBO are set up once at init, not per draw.

### SHARED vs NONSHARED Vertices

- **SHARED** (GX_PNMTX0, slot 0): pre-transformed at load time for seamless character joints
- **NONSHARED** (GX_PNMTX1, slot 1): transformed by GX matrix each frame

Do NOT force all vertices to NONSHARED — it breaks character joint seams.

### TEV Pipeline

Up to 3 stages, KONST colors, swap tables, per-stage texture binding. Single GLSL program with uniform-driven stages. Shaders loaded from `pc/shaders/` at runtime (required — no embedded fallback).

Per-stage uniforms:
- **Bias**: ADDHALF (+0.5), SUBHALF (-0.5) applied after TEV blend
- **Scale**: SCALE_2 (x2), SCALE_4 (x4), DIVIDE_2 (x0.5) applied after bias
- **Clamp**: per-channel clamp to [0,1] at output register write
- **Output register**: stages can write to PREV, REG0, REG1, or REG2
- **Swap tables**: 4 configurable tables (ivec4 channel remap), per-stage selection for texture and rasterizer colors

### Texture Cache

2048-entry cache keyed by (ptr, w, h, fmt, tlut_name, content_hash). ~100% hit rate at steady state. 10 GC texture formats decoded: I4, I8, IA4, IA8, RGB565, RGB5A3, RGBA8, CI4, CI8, CI14x2, CMPR (S3TC).

Stale GL texture IDs are cleaned up on cache eviction to prevent GPU resource leaks.

### Uniform Dirty-Flag System

`pc_gx.c` uses per-uniform dirty flags to skip redundant `glUniform*` calls. Flags are set when GX state changes and cleared after upload. Reduces GL call overhead by ~12%.

### Widescreen (3-state system)

Controlled by `g_pc_widescreen_stretch`:
- **0 (hor+)**: full-window viewport, FOV-corrected projection. Default, resets each frame.
- **1 (stretch)**: full-window, no correction. For fullscreen transitions/inventory backgrounds.
- **2 (pillarbox)**: centered 4:3 viewport with black bars. For inventory UI alignment.

m_play.c inserts NOOPTag markers in POLY_OPA display lists to toggle between states. emu64 reads these during DL processing. Frustum culling bounds are widened for hor+ to prevent side-of-screen popping.

## Endianness

All ROM/ARAM data is big-endian. Multi-byte fields must be byte-swapped after loading.

Pattern: `#ifdef TARGET_PC` byte-swap block right after `_JW_GetResourceAram` call.

Known swap sites:
- RARC archives (JKRAramArchive.cpp)
- FG data: 3 sites in m_field_make.c
- Messages: mMsg_Get_BodyParam
- Player palettes: m_player_lib.c
- Room wall/floor palettes: m_room_type.c
- N64Mtx s16 pairs: emu64_utility.c
- Scene_Word_u: m_scene.c
- Billboard matrices: sys_matrix.c (Matrix_MtxtoMtxF, suMtxMakeTS/SRT/SRT_ZXY)
- NPC clothing: ac_npc_cloth.c_inc (both DMA paths)
- Raw binary actables: 6 files swapped once at boot via mFM_InitActableEndian()
- Melody sequences: na_combo.c (u16 offsets)
- TLUT palettes: clock face, furniture, museum items (fd629a59)
- ADSR phase bitfield: stereo pan/reverb flags (d6e4b1ae)

**Cannot centralize**: ARAM data has mixed layouts (u8 textures, u16 palettes, u32 offsets). A bulk swap at the `_JW_GetResourceAram` layer would corrupt byte-level data.

**EFB-copied textures** are generated in little-endian format on PC, unlike ROM-sourced textures which are big-endian. Endianness fixes to texture decoders must account for both paths.

## Audio

jaudio_NES engine compiled and linked (59 source files, ~23K lines). SDL2 backend at 32kHz s16 stereo. rspsim software DSP processes ADPCM/RESAMP/ENVMIX.

All effects enabled: reverb, comb filter, Haas effect, Dolby surround.

### Threaded Architecture

Audio production runs on a dedicated SDL thread, matching the GC's `neosproc` thread model:

- **Game thread**: `Na_GameFrame()` queues audio commands via thread-safe message queues (SDL_mutex-protected `Z_osSendMesg`/`Z_osRecvMesg`)
- **Audio producer thread**: Loops calling `pc_audio_process_frame()` → `CreateAudioTask` → `RspStart2` (rspsim), writes samples into SPSC ring buffer (32768 samples = ~512ms)
- **SDL callback thread**: Reads from ring buffer → speakers

This decoupling prevents OS thread preemption of the game thread from causing audio dropouts. Frame pacing uses timer-based 60fps with spin-wait (no longer tied to audio buffer fill level).

### Known audio issue

Subtle bass distortion in specific rooms (museum dinosaur room). Present since early audio implementation. Root cause likely in A_CMD_UNK3 implementation accuracy (reverse-engineered from table data, no original microcode reference).

## Save System

GCI format only (64-byte CARDDir header + 0x72000 raw data). Bidirectional LE↔BE byte-swap for all ~300+ multi-byte fields.

- Save file: `save/DobutsunomoriP_MURA.gci`
- Also scans for Dolphin naming format (`8P-GAFE-...`)
- Backup rotation: up to 3 `.bak` files on each save
- Recovery: tries temp file, then backups if main save is missing
- Compatible with Dolphin emulator (can import/export saves)

## Enhancement Features

Compiled under `PC_ENHANCEMENTS` define (enabled by default in CMakeLists.txt).

### Settings (`settings.ini`)

```ini
[Graphics]
window_width = 1280
window_height = 720
fullscreen = 0          # 0=windowed, 1=fullscreen, 2=borderless
vsync = 0
msaa = 4                # 0/2/4/8
```

Auto-generated with defaults on first run. Resolution presets up to 4K supported. Custom resolutions can be set in the .ini file. DPI-aware on Windows (respects system scaling).

### HD Texture Packs

Drop Dolphin-compatible HD texture packs into `texture_pack/` directory. Uses XXHash64 for matching (identical algorithm to Dolphin). Supports DDS files with BC7, BC1, BC3, or uncompressed RGBA.

Filename format: `tex1_{W}x{H}_{hash}[_{tlut_hash}]_{fmt}.dds`

Wildcard palette support: `tex1_WxH_DATAHASH_$_FMT.dds` matches any palette variant.

### 4x MSAA

Anti-aliasing via multisampled framebuffer. Configurable in `settings.ini` (0/2/4/8 samples).

## Input

Keyboard mapping:
- **WASD** = analog stick
- **Arrow keys** = C-stick
- **Space** = A, **LShift** = B, **Enter** = Start
- **IJKL** = D-pad
- **Q/E** = L/R triggers, **Z** = Z trigger
- **F3** = toggle frame limiter
- **ESC** = quit

SDL2 gamepad with hotplug, analog sticks (deadzone 500), triggers, D-pad, and rumble.

PADRead returns GC button format. Conversion to N64 format happens in `padmgr_UpdatePC()`.

## Fault Handling

The PC port does not install a VEH/signal crash recovery handler. Faults are allowed to propagate to the OS/debugger.
Actor profile validation remains in `m_actor.c` to skip NULL/invalid profiles before dispatch.

## Build System

32-bit MinGW GCC 15.x (i686) + CMake + SDL2 2.30.10 + GLAD2 (GL 3.3 Core).

**Must compile as 32-bit** — decomp code casts pointers to u32 everywhere.

### Quick Start

```bash
# 1. Place disc image in pc/build32/bin/rom/
# 2. Build (from MSYS2 MINGW32 shell):
./build_pc.sh

# 3. Run:
pc/build32/bin/AnimalCrossing.exe --verbose
```

`build_pc.sh` handles CMake configuration and build in one step.

### Cross-Compilation

| Toolchain | File | Target |
|-----------|------|--------|
| Linux i686 | `pc/cmake/Toolchain-linux32.cmake` | Native Linux 32-bit |
| MinGW from Linux | `pc/cmake/Toolchain-mingw32.cmake` | Windows 32-bit cross-compile |

### CLI Flags

| Flag | Effect |
|------|--------|
| `--verbose` / `-v` | Enable diagnostic output |
| `--no-framelimit` | Disable the frame limiter |
| `--model-viewer [N]` | Launch model viewer (optional start index) |
| `--time HOUR` | Override in-game hour (0-23) |
| `--help` / `-h` | Show help |

## Platform Support

| Platform | Status |
|----------|--------|
| Windows (MinGW i686) | Primary target, fully tested |
| Linux (i686) | Compiles and links, mmap arena |

Linux support uses POSIX equivalents: `mmap()` instead of `VirtualAlloc()`, `mkdir()` guards for directory creation.

## Common Pitfalls

- **32-bit required**: 64-bit builds crash in JKRHeap (pointer→u32 casts).
- **`__attribute__((weak))`** doesn't work on MinGW/PE. Use regular definitions.
- **libc64/malloc.c** is excluded — it redefines system malloc and crashes the CRT.
- **NDEBUG must always be defined**: decomp asserts have side effects. Without NDEBUG, assert macros run and cause texture corruption.
- **Optimization must be -O0**: any optimization (-O1+) exposes UB in decomp code (infinite spawn loops, crashes).
- **windows.h macros**: always `#undef near` / `#undef far` after including.
- **GC address space**: emu64 uses 0x80000000-0x83000000 range. Guard with TARGET_PC.
- **glClear respects write masks**: must set glDepthMask(GL_TRUE) + glColorMask(all TRUE) before glClear.
- **seg2k0 collision**: PC heap pointers can collide with N64 segment addresses. Fixed with proximity heuristic + VirtualAlloc/mmap arena at >=0x10000000.
- **`#included .c` files**: emu64_utility.c, emu64_print.cpp, jsyswrapper_ext.cpp, jsyswrapper_main.cpp, ac_animal_logo_misc.c, m_item_debug.c, ac_npc_shop_common.c — these are compiled as part of their parent file, not standalone.
- **Title demo OOB**: `demo_npc_list` has 14 valid entries but `mNpc_SetAnimalTitleDemo` loops 15 times. On GC, the garbage 15th read was benign; on PC it produced invalid NPC `looks` → OOB crash in wander logic. Fixed with sentinel entry, slot clearing, and looks clamp.
- **EFB-copied textures are LE**: ROM textures are BE, but EFB copies are generated in LE on PC. Texture decoder endianness fixes must not break EFB copies.
