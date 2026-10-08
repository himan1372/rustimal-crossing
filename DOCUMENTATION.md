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

`rust/src/player_move.rs` ports the locomotion core: `ControllerMove` (the `mcon` fields), `turn_mod`/`calc_ease`/`smooth_turn_toward` (shortest-arc facing), `LocomotionState` with the `movement_core` hierarchy (Dash→Run→Walk), `anim_speed`/`anim_speed_near_wall`, `BRAKE_AMOUNT`, `DASH_SAMPLE_OFFSETS` verbatim, and `PlayerMovement` (`step_core`, `brake`). C ABI: `pc_turn_mod`, `pc_locomotion_core`. `cargo check --lib` clean. Unit tests: 130/130 pass in the authorized `cargo test --lib` run on 2026-10-07. No C callers are rewired; full Windows game link unverified.

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

`rust/src/collision.rs` ports both systems: `CollisionData` with exact bit packing/unpacking and flat detection, `UnitArea`, `WallKind`, `SlateDir` with diagonal-comparison detection, `WallSeg` (signed distance, normal-based correction), directional hit flags, `BgResult`, the neighborhood-size rule (3/5/7 by range), plane-equation ground height, ground Y correction, `ColliderType`/groups/`Mass` with mass-split separation rules, and sphere-overlap depth. C ABI: `pc_collision_neighborhood`, `pc_collision_pack`. `cargo check --lib` clean. Unit tests: 130/130 pass in the authorized `cargo test --lib` run on 2026-10-07. No C callers are rewired; full Windows game link unverified.

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

`rust/src/inventory.rs` ports the query layer: `Inventory` (15 pockets + packed conditions), `find_item`/`find_item_with_cond`/`count_item`/`count_item_with_cond`/`find_free_slot`/`put`, `item_cond`/`set_item_cond` with the exact shift math, `ExcludedFurniture` + `selectable_furniture`, `CandidateStrategy` (FavoriteFirst/RandomCarried, marked rewrite-owned/untraced), and `resolve_candidate`. C ABI: `pc_inventory_find`, `pc_inventory_count`. `cargo check --lib` clean. Unit tests: 130/130 pass in the authorized `cargo test --lib` run on 2026-10-07. No C callers are rewired; full Windows game link unverified.

### Runtime Port Progress: Item Preference Structures

### Source findings

The brief's architecture was verified against `m_npc.h`/`m_npc.c`/`m_quest.c`:

- **Verified: no static favorite-item list.** `Animal_c` carries identity, memories, friendship, house info, clothing, mood, relations, quest state — no `favorite_items[]` field. Modern-style style/color/series favorites are NOT in the GameCube decomp.
- **Verified NPC definition data:** `npc_def_list[]` supplies cloth, umbrella, catchphrase string index via `mNpc_SetDefAnimalInfo` (`m_npc.c:2266`); personality comes from `npc_looks_table[npc_id & 0xFFF]`, while the house template is indexed by NPC ID directly, not by personality.
- **Verified house template** (`m_npc.c:2825`): `npc_house_list[npc_id & 0xFFF]` gives type, palette, wall_id, floor_id, main_layer_id, secondary_layer_id.
- **Verified NPC-associated furniture chain:** `mNpc_DecideNpcFurniture` scans 10x10 of the main furniture layer, filters eligible furniture, counts, picks `RANDOM(num)`; result stored as `reward_furniture`, retrieved with `mNpc_GetNpcFurniture`.
- **Verified 1/10 house-furniture branch** (`m_quest.c:931-938`, with the source comment): `RANDOM(10)`; roll 0 uses the villager's house furniture for furniture "goods", else `mSP_SelectRandomItem_New`.
- **Verified Islander structures:** `Anm_bestFtr_c { u32 check; u16 have_bitfield; }` inside `memuni_u` (`m_npc.h:166`); `mNpc_Island_Ftr_c { u16 set_ftr_bitfield; trade_list[4]; item_list[16]; }` (`m_npc.c:5204`); `mNpc_SetIslandRoomFtr` ORs each memory's `have_bitfield` into `set_ftr_bitfield`; `mNpc_GetIslandFtrIdx` normalizes variants via `aMR_CorrespondFurniture` / `aMR_GetFurnitureUnit`.
- The "favorite furniture" lists players observed are best reconstructed as emergent from actual house contents — strong inference, not proven. Whether villager request dialogue uses `mNpc_GetNpcFurniture()` directly, and the exact request-selection caller, remain untraced.

### Rust rewrite implementation

`rust/src/item_prefs.rs` ports the verified structures: `NpcDefData` (cloth/umbrella/catchphrase), `NpcHouseData` (type/palette/wall/floor/layer IDs), `select_reward_furniture` (10x10 scan, eligible filter, RNG-index pick), `GoodsSource` + the verbatim 1/10 rule (`goods_source_for_furniture`), `AnmBestFtr`, `IslandFtr` (16 slots, 4 trade entries, bitfield merge, normalized slot lookup), and C ABI exports `pc_npc_house_goods`, `pc_eligible_furniture_count`. `cargo check --lib` clean. Unit tests: 130/130 pass in the authorized `cargo test --lib` run on 2026-10-07. No C callers are rewired; full Windows game link unverified.

### Runtime Port Progress: Request-Selection Caller

### Source findings

The brief's open question — the junction between house furniture and inventory scans — was resolved in `src/actor/ac_quest_talk_normal_init.c` (the quest talk manager for normal villagers):

- **Verified junction function:** `aQMgr_decide_msg_check_possession(check_proc, base_msg, item_idx, msg_count, cancel_item)` — calls a possession-check proc; on a hit, picks a random message variant `base_msg + mQst_GetRandom(msg_count)` from a per-personality table (`l_ki_ftr[looks]`, `l_trade_ftr[looks]`, six personalities) and records the pocket index; returns -1 when the player lacks the item, failing that dialogue option.
- **Verified "impulse buying" selector:** `aQMgr_get_possession_ftr_cpt_wl_rnd` — counts eligible carried furniture (FTR0/FTR1 foreground types plus carpets and walls, NORMAL condition, excluding the cancel item) with the Sum query variants, then `sel_idx = RANDOM(item_cnt)` and walks the pockets to take the sel_idx-th eligible item. Selection is uniform over *eligible carried items* — not over pockets, not first-match. `aQMgr_get_possession_item_rnd` does the same for insects/fish.
- **Verified deterministic variants:** `aQMgr_get_possession_ftr_cpt_wl` (first match: FTR0 → FTR1 → carpet → wall) feeds the message-decide paths; the `_rnd` variants feed trade-offer construction (`aQMgr_order_decide_trade_N` → `aQMgr_order_decide_trade_common`).
- **Verified trade-offer assembly:** the villager wants the player's randomly chosen carried furniture (`trade_items[0]`); category goods come from `mQst_GetGoods_common` (carrying the 1/10 house-furniture branch); the offered item is a random category good, or a pitfall seed in pitfall mode (`aQMgr_SEL_ITEM_MODE_PITFALL`).
- **Verified probability tables:** trade message set `{25, 25, 25, 25}` (`l_trade_prob`); normal-talk set `{49, 17, 17, 17}` (`l_normal_3_prob`).
- The brief's downgrade was correct: `mQst_GetGoods_common` is a quest goods/reward generator, not the dialogue request selector — and `m_npc.c` does not call the possession scanner. The true junction lives one layer up in the quest-talk actors.

### Rust rewrite implementation

`rust/src/request_selector.rs` ports the verified machinery: `pick_random_eligible` (count + `rng % count` + walk, returning pocket idx and item), `pick_first_eligible`, `decide_msg_check_possession` (message binding), `decide_idx_prob_table` (cumulative-weight dispatch) with the verbatim `TRADE_PROBS`/`NORMAL_3_PROBS` tables, `TradeOffer` + `build_trade_offer` (wanted item, category goods with `GoodsSource` per slot via the 1/10 rule, random/pitfall offered item). C ABI: `pc_request_pick_carried`, `pc_request_dispatch`. `cargo check --lib` clean. Unit tests: 130/130 pass in the authorized `cargo test --lib` run on 2026-10-07. No C callers are rewired; full Windows game link unverified.

### Runtime Port Progress: Per-Scene NPC House State Layouts

### Source findings

The brief's three-domain model was verified against `m_npc.h`/`m_npc.c`/`m_quest.c`:

- **Verified `mNpc_NpcList_c`** (`m_npc.h:268`): name, field_name, house_position, position, appear_flag, conversation_flags, quest_info, house_data, reward_furniture — the runtime per-NPC/per-house state object.
- **Verified `mNpc_NpcHouseData_c`** (`m_npc.h:252`): type, palette, wall_id, floor_id, main_layer_id, secondary_layer_id.
- **Verified dual actor links** (`m_npc.c:2852`): `mNpc_SetNpcinfo` sets `npc->npc_info.animal` (persistent `Animal_c`) and `npc->npc_info.list` (runtime `mNpc_NpcList_c`) from one `npc_info_idx`; `-ANIMAL_NUM_MAX` selects the island fallback.
- **Verified scene dispatch** (`m_npc.c:2925`): `SCENE_NPC_HOUSE`/`SCENE_KAMAKURA`/`SCENE_COTTAGE_NPC` route into `mNpc_AddNpc_inNpcRoom` / `...Island`.
- **Verified owner resolution** (`m_npc.c:2876`): `mNpc_AddNpc_inNpcRoom` reads `house_owner_name`, resolves it via `mNpc_SearchAnimalinfo`, and places the move actor at unit (4, 7) — skipping reserved/empty/joint-event owners.
- **Verified room wall/floor** (`mNpc_RenewalNpcRoom`): for an `mFI_FIELD_NPCROOM0` field with a valid owner, wall/floor come from `npclist->house_data.wall_id`/`floor_id`.
- **Verified scan shape** (`m_npc.c:3083`): `data_idx = main_layer_id - fg_base_id` clamped at 0, `fg_items = fg_data_table[data_idx]->items[0]`, two-pass 10x10 scan with `fg_items += UT_X_NUM - 10` stride, `num = RANDOM(num)`, second pass returns the num-th eligible item.
- **Verified request-dispatch call** (`m_quest.c:980`): `(*Common_Get(clip).npc_clip->force_call_req_proc)(npc_actor, 0x0D8B + looks)` — request-procedure ID from base plus looks category. The function behind the pointer remains untraced.

### Rust rewrite implementation

`rust/src/house_scene.rs` ports the verified structures: `NpcListEntry` (full `mNpc_NpcList_c` layout; conversation/quest fields opaque), `NpcActorLinks` + `resolve_npc_links` (normal and island branches), `HouseSceneKind`, `resolve_house_owner` (owner → NPC index + (4,7) placement, with the reserved/empty/joint-event guards), `renewal_npc_room` (wall/floor from owner house data), `scan_house_furniture` (verbatim two-pass strided scan), `request_proc_id` (`0x0D8B + looks`), and `force_call_req_proc` modeled as a caller-supplied callback since the implementation is untraced. C ABI: `pc_request_proc_id`, `pc_house_wall_floor`. `cargo check --lib` clean. Unit tests: 165/165 pass in the authorized `cargo test --lib` run on 2026-10-07. No C callers are rewired; full Windows game link unverified.

### Runtime Port Progress: Per-Scene State Layouts (Scene Description Language)

### Source findings

The brief's scene-description model was verified against `m_scene.h`/`m_scene_table.h`/`m_scene.c`/`m_play.c`:

- **Verified record-type enum** (`m_scene.h:96`): 0 PLAYER_PTR, 1 CTRL_ACTOR_PTR, 2 ACTOR_PTR, 3 OBJECT_EXCHANGE_BANK_PTR, 4 DOOR_DATA_PTR, 5 FIELD_CT, 6 MY_ROOM_CT, 7 ARRANGE_ROOM_CT, 8 ARRANGE_FURNITURE_CT, 9 SOUND, 10 END.
- **Verified `Scene_Word_u` union** (`m_scene.h`): tagged record structs sharing the type byte; misc records carry param0–param3.
- **Verified dispatcher** (`m_scene.c:322`): `Scene_ct` walks the array with a static `Scene_Proc[]` table, breaks at END, skips `type >= mSc_SCENE_DATA_TYPE_NUM`.
- **Verified `Door_data_c`** (`m_scene.h:47`): next_scene_id, exit_orientation, exit_type, extra_data, exit_position, door_actor_name, wipe_type.
- **Verified FIELD_CT handler** (`m_scene.c:470`): `mFM_SetFieldInitData(bg_num, bg_disp_size)`, game_started=FALSE, in_initial_block=TRUE, sunlight_flag=TRUE; MY_ROOM_CT/ARRANGE_ROOM_CT/ARRANGE_FURNITURE_CT activate room-resource systems.
- **Verified room types** (`m_scene.h:39`): OUTDOORS, MY_ROOM, NPC_ROOM, MISC_ROOM.
- **Verified transition** (`m_scene.c:512`): `goto_other_scene` saves the door record, `next_scene_id = door_data->next_scene_id + 1`, `play->next_scene_no`, `restore_fgdata_all(play)`; WIPE_TYPE_NORMAL doors become WIPE_TYPE_FADE_BLACK.
- **Verified scene count:** ~54 gameplay scenes in `m_scene_table.h` (brief said 50; actual table has 54 + SCENE_NUM).
- **Porting caveat found in the decomp itself:** the PC port repacks FIELD_CT from `misc.param3` on little-endian (`TARGET_PC` branch) — the struct layout is big-endian-ordered, which matters for this x86 rewrite.
- Not recovered: `Gameplay_Scene_Read` internals and the contents of individual `*_info[]` scene arrays (declared but not exposed in the reviewed headers).

### Rust rewrite implementation

`rust/src/scene_layout.rs` ports the verified system: `SceneWordType` (verbatim tags), `SceneWord` (decoded records), `RoomType`, `DoorData`, `interpret_scene` (walk-until-END dispatcher), `FieldInit` (FIELD_CT output with the verbatim flag values), `goto_other_scene`/`SceneTransition` (the +1 scene rule and wipe substitution), and the little-endian FIELD_CT caveat documented. C ABI: `pc_scene_word_type`, `pc_door_next_scene`. `cargo check --lib` clean. Unit tests: 165/165 pass in the authorized `cargo test --lib` run on 2026-10-07. No C callers are rewired; full Windows game link unverified.

### Runtime Port Progress: Full Background-Check Sequence

### Source findings

The brief's pipeline was verified against `m_collision_bg.c`/`m_collision_bg.h`, including the exact call order at `m_collision_bg.c:1899` (`mCoBG_BgCheckControll_RemoveDirectedUnitColumn`):

1. `mCoBG_MoveActorWithMoveBg` — platform carry before anything else
2. `mCoBG_InitRevpos`, current + old center positions
3. `mCoBG_MakeActorInf` — old ground/water state, speeds, 3/5/7 neighborhood
4. `mCoBG_WallCheck` — columns first, then wall vectors
5. `mCoBG_GroundCheck` — terrain + water + jump flag
6. `mCoBG_MoveBgGroundCheck` — moving-platform support
7. `mCoBG_CarryOutReverse` — apply when `rev_type == 0`, else hold
8. `mCoBG_GiveRevposToActor`
9. `mCoBG_RoomScopeCheck` — scene-dependent room bounds

Verified formulas and limits:

- Neighborhood: `range <= 40 → 3`, `<= 80 → 5`, else `7` (`m_collision_bg.c:1806`).
- `mCoBG_UNIT_VEC_INFO_MAX = 128`, `mCoBG_MOVE_REGIST_MAX = 64`, `mCoBG_WALL_COL_NUM = 2`, 5 on/side contacts.
- Distance reverse: `(range - dist) + 0.00001f` (three occurrences).
- Ground adjust: `ground_y >= foot_y` → snap + grounded + y-speed 0; descending snap when previously grounded and `|ground_y - foot_y| <= xz_speed` (`m_collision_bg.c:409`).
- Water: river `20.0 + GetBgY`, sea `20.0`.
- Wave: `rate = (1.0 + wave_cos) * 0.5`.
- Room scope: MY_ROOM_S → 160, MY_ROOM_M/LL2 → 240, MY_ROOM_L/LL1 → 320.
- Plane height: `dot / -norm->y` from the triangle normal.

### Rust rewrite implementation

`rust/src/bg_check.rs` ports the verified sequence: `BgStage` (recovered call order), `BgCheckType` (player vs actor wall ordering), `BgActorInfo` (old/new ground state, speeds), `neighborhood_size`, `distance_reverse`, `adjust_actor_y` (both the snap-up and descending-snap branches), `water_y_river`/`water_y_sea`, `wave_rate`, `RoomSizeClass`/`room_scope_extent`, `carry_out_reverse`, and the source limits as constants. C ABI: `pc_bg_neighborhood`, `pc_bg_distance_reverse`, `pc_bg_room_scope`. `cargo check --lib` clean. Unit tests: 165/165 pass in the authorized `cargo test --lib` run on 2026-10-07. The inner wall solver's geometric internals (crossing tests, player prioritization, attribute tables) remain future work. No C callers are rewired; full Windows game link unverified.

### Runtime Port Progress: Inner Wall Solver Geometry

### Source findings

The brief's solver model was verified against `src/game/m_collision_bg.c`:

- **Verified wall record** (`m_collision_bg.c:27`): start/end XZ, vertical bounds (start_top/btm, end_top/btm), normal, normal_angle, wall_name, regist_p, atr_wall.
- **Verified kind dispatch** (`m_collision_bg.c:816`): `cross_rev_proc[] = { Normal, Attribute, Normal }` — moving walls reuse the normal crossing routine.
- **Verified vector gate** (`m_collision_bg.c:533`): `mCoBG_JudgeWallFromVector` returns TRUE when `|angle| > 89.5°` under the engine's angle convention.
- **Verified height tests:** `mCoBG_RoughCheckWallHeight` uses `bot_y + 3.0f`; `mCoBG_GetWallHeight` interpolates top/bottom along the wall.
- **Verified crossing corrections:** normal → `rev_dist = range + dist + 0.00001f`, `reverse = normal * rev_dist`; attribute → `reverse = cross - actor_end` (line-line intersection).
- **Verified distance dispatch:** `dist < range` → push `(range - dist) + 0.00001f`; `|dist - range| < 2.7f` → register contact only (two occurrences: lines 844, 961).
- **Verified normal-actor order** (`m_collision_bg.c:1181`): `spd > range * 0.5` → crossing pass; then static distance pass (`regist_p == NULL`); then moving distance pass; `actor_end += rev` after every wall.
- **Verified player order** (`m_collision_bg.c:1140`): distance pass (as NORMAL) → `mCoBG_GetWallPriority` (merge-sort by squared midpoint distance from actor_start) → distance pass in priority order (as PLAYER) → crossing pass. The brief's summary omitted the first unordered distance pass; the code has it.
- **Verified final:** `rev_pos = actor_end - original_end`; start is preserved while end is corrected iteratively.
- **Verified padding:** `mCoBG_tab_data = { {5.0f, 10.0f}, {0.000001f, 0.000002f} }` expands wall segments beyond tile edges.

### Rust rewrite implementation

`rust/src/wall_solver.rs` ports the verified dispatch: `WallSeg2` (full record), `WallKind2`, `judge_wall_from_vector` (89.5° gate with the convention caveat), `rough_check_wall_height`, `wall_height_at` (interpolation), `cross_reverse_normal` / `cross_reverse_attribute`, `distance_dispatch` (push/contact/ignore with the 2.7 tolerance), `distance_push`, `wall_priority` (midpoint sort), and `solve_walls` implementing the exact player vs normal-actor orderings with iterative `actor_end` correction and `rev_pos` reconstruction. C ABI: `pc_judge_wall_from_vector`, `pc_distance_dispatch`. `cargo check --lib` clean. Unit tests: 165/165 pass in the authorized `cargo test --lib` run on 2026-10-07. Endpoint-circle geometry and the 0.1f neighbor-suppression test remain future work. No C callers are rewired; full Windows game link unverified.

### Runtime Port Progress: Player Wall-Priority Sort

**64-wall analysis (2026-10-07, resolved):** the original's `u64` used mask
is UB for wall index ≥ 64 while the array holds 128. Player path analysis:
`mCoBG_BgCheckControll` calls the player check with range 18.0f
(`m_player_common.c_inc:2579`) → 3×3 neighborhood. Wall budget:
9 slate attempts + 12 normal attempts (edge de-duplication bitmask in
`l_make33_coldata`) + 18 forbid walls = 39 terrain max, plus up to 48
circle-defence walls (8 surrounding columns × ordered adjacent pairs × 2),
so the theoretical max is ~87 — ≥ 64 is reachable in pathological
arrangements, though normal gameplay sees < 20. On x86-64 the shift wraps
mod 64, aliasing wall 64 to bit 0 (silent priority-table corruption, not a
crash). The Rust port deliberately uses `u128`: bit-identical below 64
walls, correct above. The original UB is documented, not reproduced.

### Attribute/Forbidden-Wall Tables

### Source findings

The brief's forbidden-wall model was verified against `src/game/m_collision_bg.c`, `src/game/m_collision_bg_wall.c_inc`, `src/game/m_collision_bg_info.c_inc`, and `include/m_collision_bg.h`:

- **Verified eight-vector table** (`m_collision_bg.c:82`): `mCoBG_make_vector_table[8]` — 0°/UP, −90°/RIGHT, 90°/LEFT, 180°/DOWN, plus 45°/135°/225°/315° diagonal slate entries with `SQRT_OF_2_DIV_2` normals.
- **Verified index table** (`m_collision_bg.c:93`): `mCoBG_forbid_vector_idx[36][2]` verbatim, mapping attributes 27–62 to up to two vector IDs (`-1` = none). Two-wall corner attributes: 51–54 (tunnels) and 59–62 (river-bank corners), e.g. 51 = UP+LEFT. Attribute 31 (wood bridge center) maps to none.
- **Verified attribute comments** (`include/m_collision_bg.h:79`): the 27–62 family is wood bridge (27–31), stone bridge (32–35), wave (36–38), river bank (39–42), grass/river (43–46), grass/cliff (47–50), tunnel (51–54), diagonal cliff (55–58), diagonal river bank (59–62); 63 is a separate slate/slope representation, not part of the table.
- **Verified generation gate** (`m_collision_bg_wall.c_inc:508`): `forbid_proc = (old_on_ground & attr_wall) & 1` selects `mCoBG_MakeForbidAttrVector` vs the DUMMY no-op — forbidden vectors appear only when the actor was grounded AND the attribute-wall flag is set.
- **Verified wall record:** generated walls get `atr_wall = TRUE`, `regist_p = NULL`, normal/angle/name from the vector table, segment from `mCoBG_UnitNoName2StartEnd` with the check-type padding table, and no wall-height bounds.
- **Verified ball-rolling reuse** (`m_collision_bg_info.c_inc:858`): `mCoBG_CheckAttribute_BallRolling` reads the same forbid table, flipping each emitted normal angle by +180°.
- `attr_wall` also switches ordinary normal/slate wall registration between the `AttributeOff`/`AttributeOn` variants (modeled, internals not yet ported).

### Rust rewrite implementation

`rust/src/attr_walls.rs` ports the tables verbatim: `MAKE_VECTOR_TABLE`, `FORBID_VECTOR_IDX`, the decomp header comments as `ATTRIBUTE_NAMES`, the 27–62 range gate, `forbid_vectors` (0–2 vector IDs), `is_two_wall_attribute`, the `(old_on_ground & attr_wall) & 1` gate as `forbid_generation_enabled`, the ball-rolling +180° reuse as `ball_rolling_angles`, the `AttributeWallSpec` record (`atr_wall = TRUE`, no moving-BG pointer), and `wall_registrar_variant`. These feed into the existing solver: attribute walls enter `wall_solver.rs` as `WallSeg2` with `atr_wall = true`, where the dispatch tables already route them to the attribute solver. C ABI: `pc_forbid_vectors`, `pc_forbid_gate`. `cargo check --lib` clean. Unit tests: 198/198 pass in the authorized `cargo test --lib` run on 2026-10-07 (run #6). Gaps: exact `mCoBG_UnitNoName2StartEnd` segment-orientation mapping, the AttributeOn normal/slate registrar internals, the bridge/water special case in `RegistNormalWallVector_AttributeOff`, and the parallel `l_attribute_action_info` / water-translation tables — all marked future work. No C callers are rewired; full Windows game link unverified.

### Runtime Port Progress: Column Construction

### Column Construction

### Source findings

The brief's column-system model was verified against `src/game/m_collision_bg.c` and `src/game/m_collision_bg_column.c_inc`:

- **Verified struct** (`m_collision_bg.c:10`): `mCoBG_column_c` = X/Y/Z position, top height, radius, `s16 atr_wall`, unit coords `ux`/`uz` — a vertical cylinder expressed as an X/Z circle with bottom/top Y.
- **Verified 16-slot limit and counting quirk** (`m_collision_bg_column.c_inc:290`): `mCoBG_MakeColumnCollisionData` walks the neighborhood row-major, builds while `*col_count_p < 16`, and calls `mCoBG_MakeOneColumnCollisionData` *without* checking its return — the count is examined slots; failed slots stay zeroed (`bzero`'d in the normal path). Not the nearest 16.
- **Verified own-unit exclusion:** `ut_info->ut_x == ux && ut_info->ut_z == uz` → FALSE, no column.
- **Verified item recipes** (radius/height pairs): hole (19, ground Y, `atr_wall=TRUE`, requires `old_on_ground`); small/med/large/full tree (19, +30/+40/+60/+80); stumps (+30, radius 10 for the four `*_STUMP001` IDs, else 18); rock (19, +31.5); mailbox (15, +50); sign (19, +45); `RSV_SIGNBOARD` (10, +45); koinobori/flag (19, +160). Non-hole branches explicitly set `atr_wall = FALSE`; X/Z at unit center, Y from `mCoBG_GetBgY_OnlyCenter_FromWpos2`.
- **Verified normal collision** (`mCoBG_ColumnCheck_NormalWall`): skip if the actor was already inside at the old position or `height < now_y + 3.0`; `dist < range + radius` → radial X/Z push with `rev_vec.y = 0` and wall-contact registration; `0 < dist − check_dist < 2.7` → contact only.
- **Verified attribute columns:** ignored unless `old_on_ground`; then the same radial test without the height gate.
- **Verified pipeline order** (`m_collision_bg.c:1253/1261`): object columns → decal columns → terrain wall vectors (`mCoBG_GetWallReverse`).

### Rust rewrite implementation

`rust/src/columns.rs` ports the system: `Column` record, `ColumnItemKind` with the exact hard-coded recipes, `make_one_column`, `make_column_collision_data` (16-slot examined-count quirk and own-unit exclusion faithfully modeled, failed slots as `None`), `column_check_normal` / `column_check_attr` (height gate, radial push, 2.7 contact band, old-on-ground gate), and the C ABI `pc_column_recipe`. `cargo check --lib` clean. Unit tests: 198/198 pass in the authorized `cargo test --lib` run on 2026-10-07 (run #6). Gaps: the decal-circle register/clear machinery, the separate line-vs-column sweep routine, and column-derived ground height — all marked future work. No C callers are rewired; full Windows game link unverified.

### Runtime Port Progress: Segment-Orientation Mapping

### Source findings

The brief's two-orientation model was verified against `src/game/m_collision_bg_wall.c_inc`, `src/game/m_collision_bg_math.c_inc`, `src/game/m_collision_bg.c`, and the headers:

- **Verified segment placement** (`m_collision_bg_wall.c_inc:1`): `mCoBG_UnitNoName2StartEnd` — UP: `(ux*U−t0, uz*V)` → `+X` extended by `t1`; DOWN: same line with reversed endpoint order; LEFT/RIGHT: vertical `+Z` segments; SLATE_UP: `+X/−Z` diagonal; SLATE_DOWN: `+X/+Z` diagonal. All verbatim.
- **Verified padding table** (`m_collision_bg.c:58`): `mCoBG_tab_data = {{5.0, 10.0}, {0.000001, 0.000002}}` — NORMAL walls extend ~5/10 units past the tile; PLAYER walls get essentially exact boundaries. Unit world size 40 (`mFI_UNIT_BASE_SIZE`).
- **Verified normal selection** (`mCoBG_SearchWallFlag`): axis walls pick normals from neighboring corner-height comparisons, normal toward the higher side — UP → (0,±1)/0°/180°, DOWN → (0,∓1)/180°/0°, LEFT → (±1,0)/±90°, RIGHT → (∓1,0)/∓90°, all four branches read directly. Slate walls: SLATE_UP compares leftUp vs rightDown → (±√½,±√½)/45°/−135°; SLATE_DOWN compares leftDown vs rightUp → (±√½,∓√½)/135°/−45°.
- **Verified front test** (`m_collision_bg_math.c_inc:251`): `mCoBG_GetPointInfoFrontLine` = `n·point − n·start ≥ 0` — front/back comes from the stored normal, never segment direction.
- `wall_name` drives height interpolation (X for UP/DOWN, Z for LEFT/RIGHT, projected for slate), already modeled in `wall_solver.rs`.

### Rust rewrite implementation

`rust/src/segment_map.rs` ports the mapping: `WallName`, `CheckType`, `TAB_DATA`, `unit_no_name_2_start_end` (verbatim), `point_info_front_line`, `search_wall_flag` (all four cardinal branches), `slate_normal`, `interp_axis`. `attr_walls.rs` gained `forbid_wall_segments`, closing the previously flagged gap — attribute walls now get real segments through the actual mapping. C ABI: `pc_unit_no_name_2_start_end`. `cargo check --lib` clean. Unit tests: 198/198 pass in the authorized `cargo test --lib` run on 2026-10-07 (run #6). Gaps: Nintendo's original terminology for the wall_name/normal distinction (not in the decomp); `mCoBG_Check45Angle` front/left/right/back classification not yet ported. No C callers are rewired; full Windows game link unverified.

### Runtime Port Progress: Decal-Circle Machinery

### Source findings

The brief conflated two mechanisms; the decomp source separates them, and this port implements both:

**1. Circle-defence walls** (`mCoBG_MakeCircleDefenceWall`, `m_collision_bg_wall.c_inc:599`) — does NOT populate `mCoBG_decal_circle`. For each ordered pair of distinct columns whose unit offset (dx,dz) matches one of eight `defence_wall_info` entries, it appends TWO wall vectors from col0's position to col1's position — one per normal/angle/wall_name pair in the entry (opposite-facing normals), both `atr_wall = TRUE`, `regist_p = NULL`. Gated on `attr_wall && old_on_ground`; capped at 128 wall vectors. The table (verbatim): (±1,0) → (0,+1)/0° and (0,−1)/180° both WALL_UP; (0,±1) → (+1,0)/90° and (−1,0)/−90° both WALL_RIGHT; (±1,±1) → 135°/−45° diagonals both WALL_SLATE_DOWN; (±1,∓1) → −135°/45° diagonals both WALL_SLATE_UP. These bridge the gaps between adjacent object columns.

**2. Decal circles** (`m_collision_bg_column.c_inc:1`): `mCoBG_regist_circle_info[3]` registration records drive live `mCoBG_column_c` records fed as the second `mCoBG_ColumnWallCheck` pass (`m_collision_bg.c:1261`). Radius interpolates linearly (`mCoBG_CalcAdjust`) from start to end over the timer, then the slot deactivates; columns get `height = pos.y`, `atr_wall = TRUE`. The "why decal" answer: the registrars are the player dig/scoop actions — `mCoBG_RegistDecalCircle(pos, 0.0f, 19.0f, 12)` — the dug hole's decal gets a matching temporary collision circle growing 0→19 over 12 frames. Initialized at scene start, ticked per frame (`m_play.c:435/539`).

**Original-game bugs documented, not reproduced** (decomp-annotated): `RegistDecalCircle` clears `sizeof(whole array)` instead of one record (clobbering into the decal-circle data) and doesn't stop after the first free slot; `InitDecalCircle` clears 3× too much memory. The Rust port implements the intended behavior.

### Runtime Port Progress: Endpoint-Circle Intersection Math

### Source findings

The brief's correction was verified verbatim against `src/game/m_collision_bg.c` and `src/game/m_collision_bg_math.c_inc`:

- **Verified the key correction** (`m_collision_bg.c:908`): the endpoint solver intersects a line through the wall ENDPOINT parallel to the wall NORMAL with the actor's circle — `point = unit_vec->start`, `vec = unit_vec->normal`. The wall segment tangent (`end − start`) is never passed. The segment-based sibling `mCoBG_GetCrossCircleAndLine2D` is decomp-marked @unused/@fabricated.
- **Verified quadratic** (`m_collision_bg_math.c_inc:295`): `A = vx²+vz²` (no normalization), `B = 2(v·point − v·center)`, `C = |point−center|² − r²`, `R = B²−4AC` accepted when `R >= 0` (tangent counts), `root = ABS(sqrtf(R))` (verbatim redundancy), `A != 0` guard, `t = (−B ± root)/2A`.
- **Verified selection** (`m_collision_bg.c:858`): `mCoBG_GetSpecialDistanceReverse` picks the first intersection on the NON-front side; `reverse = edge − cross` — exactly parallel to the wall normal (`−tN`).
- **Verified gate sequence** (`m_collision_bg.c:894`): front(end) && front(start) → normal distance `dist < range` → start-in-circle else end-in-circle → `CheckDistSPCheck` suppression → intersection → height gate `(old_ground_y − 5) + 3 ≤ height.top` → reverse + wall-info registration.
- **Verified helpers:** `GetDistPointAndLine2D_Norm` = `|n·p − n·start|` with no division; `JudgePointInCircle` is squared, no sqrt; the XYZ wrapper extracts X/Z, runs 2-D math, writes X/Z back.

### Rust rewrite implementation

`rust/src/endpoint_circle.rs`: `judge_point_in_circle`, `dist_point_and_line_2d_norm`, `cross_circle_and_line_2dvector` (verbatim quadratic), `get_special_distance_reverse`, and `endpoint_circle_collision` implementing the full gate sequence (reusing `segment_map::point_info_front_line` and the `CheckDistSPCheck` test from `wall_priority.rs`). C ABI: `pc_cross_circle_line`. `cargo check --lib` clean. Unit tests were written but NOT run, per the standing instruction. This closes the endpoint-circle gap flagged since the wall-solver work. No C callers are rewired; full Windows game link unverified.

### Runtime Port Progress: Wave 1 Rewiring Prep

### Source findings

Per the brief's reclassification, Wave 1 was reorganized from "stateless functions" into pure **kernels** with C **shims**:

- **Wave 1A — pure numerical kernels:** `pc_msg_max`, `make_tab_2_move_tail`, `segment_for_wall`, `pc_inventory_find`/`pc_inventory_count`, `pc_judge_wall_from_vector` (with the faithful atan backend noted as a dependency).
- **Wave 1B — pure lookup/mapping:** `forbid_vector_kernel`, `forbid_proc` (explicit `old_on_ground, attr_wall, attribute` gate), `priority_order`, `pc_scene_word_type`, `pc_column_recipe` (C keeps ground-height lookup and the `check_proc` callback).
- **Wave 1C — simple data ops:** `pc_bg_neighborhood_coords` (fixed 49-elem buffer, no `Vec` over FFI), `pc_bg_room_scope`, `pc_door_next_scene` arithmetic, `house_surface_lookup`.
- **Wave 1D — extracted stateless pieces:** `talk_count_allowed`/`talk_patience` (C keeps `l_npc_talk_info` state), `pc_bg_distance_reverse` (to be split further before plugging).
- Postponed: `pc_distance_dispatch` (kernels first), `pc_request_proc_id` (table not pinned), `pc_topic_talk_check` (stateful; gates are the Wave 1 piece), full `GetWallReverse`.

### Rust rewrite implementation

- `wall_priority.rs`: `make_tab_2_move_tail` is now value-returning with the **faithful zero-division behavior** (no denominator clamp — zero input yields NaN biases exactly like C, instead of the previous 1e-9 clamp); `#[repr(C)] PcVec2` + `make_tab_2_move_tail_v` kernel; the C ABI is a thin unsafe shim. Added the differential bit-pattern test template (`move_tail_differential_bits`) comparing the kernel against an independent C transcription via `to_bits()`.
- `segment_map.rs`: `#[repr(C)] PcSegment` + `segment_for_wall` kernel; ABI is a shim.
- `attr_walls.rs`: `#[repr(C)] PcForbidVector` + `forbid_vector_kernel`; new `forbid_proc`/`pc_forbid_proc` combining gate + 27–62 range (old `pc_forbid_gate` kept for compatibility).
- `dialogue_topics.rs`: new `talk_count_allowed`, `talk_patience`/`TalkPatience`, `talk_patience_for_feeling` + C ABIs `pc_talk_count_allowed`, `pc_talk_patience`.
- `house_scene.rs`: `#[repr(C)] PcHouseSurface` + `house_surface_lookup` kernel; `pc_house_wall_floor` now routes through it.
- `bg_check.rs`: `#[repr(C)] PcUnitCoord` + `neighborhood_coords` (fixed `[T; 49]`, row-major) + `pc_bg_neighborhood_coords` ABI writing into the caller's buffer.
- `rewiring.md` rewritten: kernel+shim architecture, `USE_RUST` fallback pattern, differential-testing guidance (`to_bits()`, not epsilon), `f32` discipline, no_std note, and the 1A/1B/1C/1D tables with postponed items.
- `cargo check --lib` clean. Unit tests were written but NOT run, per the standing instruction.

### Runtime Port Progress: Terrain-Wall Generation + Bridge/Water Special Case

### Source findings (all verified against the local decomp)

- **Terrain-wall pipeline** (`m_collision_bg.c`, `m_collision_bg_wall.c_inc`): every background check builds a 3×3/5×5/7×7 neighborhood, then `mCoBG_MakeUnitVector` generates slate walls, cardinal terrain walls (with bridge/water modifications), and attribute/forbid walls into `unit_vec[128]`; moving-BG walls, columns, and circle-defence walls follow.
- **Terrain record** (`m_collision_bg.h:162`): 32-bit bitfield — 1-bit slate_flag, five 5-bit height samples (center, top_left, bot_left, bot_right, top_right), 6-bit unit_attribute. World offset = `value * 10.0 + base_height`.
- **Walls are edges between units**: `mCoBG_UtInf2NormalWallVector` takes two `UnitInfo`s; `mCoBG_SearchWallFlag` picks the normal from which side is higher (UP: higher neighbor → (0,+1)/0°, else (0,−1)/180°; LEFT: (1,0)/90° vs (−1,0)/−90°; DOWN: (0,−1)/180° vs (0,1)/0°; RIGHT: (−1,0)/−90° vs (1,0)/90°). No wall when all compared heights are equal. `mCoBG_JudgeTopAndSet` assigns top/btm per endpoint.
- **Slate detail** (`mCoBG_SearchSlateDetail`): `bot_right != top_left` → SLATE_UP; `top_right != bot_left` → SLATE_DOWN; else SLATE_UP. Slate normals: SLATE_UP → ±(√½,√½)/45°/−135°; SLATE_DOWN → (√½,−√½)/135° vs (−√½,√½)/−45°.
- **Bridge attributes** (`m_collision_bg.h:79-87`): 27=wood NW, 28=wood SW, 29=wood SE, 30=wood NE, 31=wood center, 32=stone N, 33=stone E, 34=stone W, 35=stone S.
- **old_in_water bridge special case** (`mCoBG_RegistNormalWallVector_AttributeOff`, verbatim): WOOD↔bridge(27–35) → bridge side's heights flattened to its minimum corner height with slate disabled, then the ordinary generator runs; bridge↔bridge and bridge↔non-wood → NO normal wall generated; ordinary↔ordinary → normal.
- **Slate suppression**: `mCoBG_RegistSlatingWallVector_AttributeOff_Slate_OldInWater` returns early for attributes 27–35 when old_in_water.
- **Dock/island rule** (`mCoBG_MakeUnitVector`): DOCK or ISLAND block kind forcibly clears old_in_water before generation.
- **Water-search masks** (`mCoBG_bridge_search_water`, verbatim): `{3, 6, 12, 9, 240, 1, 8, 2, 4}` for attrs 27–35, bits indexing DIRECT (0=N,1=W,2=S,3=E,4=NW,5=NE,6=SE,7=SW) — matches each bridge piece's geometry.
- **Quarter table** (`mCoBG_woodb_water_info`, verbatim): 27→{RIVER_NW,RIVER_NW,WOOD,WOOD}, 28→{WOOD,RIVER_SW,RIVER_SW,WOOD}, 29→{WOOD,WOOD,RIVER_SE,RIVER_SE}, 30→{RIVER_NE,WOOD,WOOD,RIVER_NE}, 31→all WOOD, 32→all WOOD.
- **Ground-check bridge code** (`m_collision_bg.c:1730`): when old_in_water and attr 27–35, searches the masked neighbor cells for water attributes and adopts the neighboring water's attribute/height.
- **Decomp-flagged @BUG reproduced**: the WALL_UP branch of `mCoBG_UtInf2NormalWallVector` never assigns `wall_name` (the other three branches do) — the port leaves the slot untouched for UP walls, exactly like the original.
- **Inference (not source-confirmed)**: the *purpose* of the flatten/suppress logic (walking from water onto a bridge without hitting a phantom cliff wall) is the brief's interpretation; the source has no developer comment.

### Rust rewrite implementation

`rust/src/terrain_walls.rs`: `TerrainUnit` (decoded world offsets + slate + attribute), `decode_height`, `WallKind` (NormalTerrain/Attribute/MovingBackground), `WallBounds`, `TerrainWall` (geometry separate from normal, like the original), `search_slate_detail`, `judge_top_and_set`, `search_wall_flag` (all four directions verbatim), `terrain_wall_policy` → `WallPolicy::{Normal, FlattenFirst, FlattenSecond, Suppress}`, `flatten_bridge_unit`, `slate_wall_suppressed`, `apply_block_water_rule`, `bridge_search_water_mask`, `bridge_quarter_attribute`, `slate_wall_geometry`, `cardinal_wall_from_units` (reuses `segment_map::unit_no_name_2_start_end`; reproduces the UP wall_name @BUG). C ABI: `pc_terrain_wall_policy`, `pc_bridge_search_water_mask`, `pc_bridge_quarter_attribute`, `pc_search_slate_detail`, `pc_search_wall_flag`. `cargo check --lib` clean. Unit tests were written but NOT run, per the standing instruction. Gaps: the full `mCoBG_MakeUnitVector` loop (neighborhood acquisition, `make_info` bit patterns, moving-BG walls), `mCoBG_SearchWaterAttributeFrom4Area`, and no C callers rewired; full Windows game link unverified.

### Runtime Port Progress: Moving-Background Walls

### Source findings (all verified against the local decomp)

- **Registration** (`m_collision_bg_move.c_inc`, `m_collision_bg.h:290`): `mCoBG_bg_regist_c` holds wpos/last_wpos pointers, angle_y, contact, bg_size, base_ofs, height, attribute, active_dist, scale_percent. Global manager: 64 fixed slots, no compaction on removal (indices are slot IDs); `mCoBG_RegistMoveBg` takes the first free slot; `CrossOffMoveBg` zeroes + NULLs + decrements count; boats use 2 dedicated slots (`l_mCoBG_boat_move_bg_data[2]`, size {20,20,40,40}, height 30, SAND attribute, active_dist 120).
- **Size presets** (verbatim): A={20,20,20,20}, B_0={60,20,20,20}, B_180={20,60,20,20}, B_270={20,20,20,60}, B_90={20,20,60,20}, C={40,40,40,40}; fields are right/left/up/down asymmetric extents. `type == FTR_TYPE_NUM` uses a caller-supplied size.
- **Wall generation** (`mCoBG_SizeData2CollisionData`, verbatim): exactly 4 walls - UP: (-left*r-t0,-up*r)->(+right*r+t0,-up*r), normal (0,-1), 180 deg; LEFT: (-left*r,+down*r+t0)->(-left*r,-up*r-t0), normal (-1,0), -90 deg; DOWN: (+right*r+t0,+down*r)->(-left*r-t0,+down*r), normal (0,1), 0 deg; RIGHT: (+right*r,-up*r-t0)->(+right*r,+down*r+t0), normal (1,0), 90 deg. Epsilon t0 comes from `mCoBG_tab_data[check_type]` (NORMAL: 5, PLAYER: 1e-6) - not universal. Bounds are flat (pos.y ... pos.y+height) at both endpoints. `regist_p = regist`, `atr_wall = FALSE`.
- **Rotation**: RotateY sign convention x'=x*cos+z*sin, z'=-x*sin+z*cos; geometry rotates only when |angle| >= 0.05 deg, but `normal_angle += angleY` always runs.
- **Activation**: walls generated only when 2D distance < active_dist; ground test uses square broad phase (|dx|<dist && |dz|<dist) then the footprint test.
- **Ground test** (`mCoBG_GetMoveBgHeight`): footprint built from scaled size + base_ofs, rotated whenever rad != 0 (no 0.05 threshold here), translated to wpos; inside iff both `RangeCheckLinePoint` slab tests pass; top = wpos.y + height; LAST matching registration wins.
- **Actor carry** (`mCoBG_MoveActorWithMoveBg_OnMoveBg`): position += wpos - last_wpos - translation only; rotation/scale do not carry the actor.
- **Contacts**: side contact = (actor_id, angle), on contact = actor_id only; both capped at 5, no dedup.
- **Solver integration**: moving walls reuse the normal-wall distance/crossing solvers; for non-player actors they are processed in a separate pass after static walls; `regist_p != NULL` is the wall-kind test (MOVE vs ATTRIBUTE vs NORMAL).

### Rust rewrite implementation

`rust/src/move_bg.rs`: `MoveBgSize` + 6 verbatim presets + boat constants, `MoveBgTransform` (current pos, short-angle Y, optional base offset, scale, height), `short_to_rad`/`deg_to_short`, `rotate_y` (verbatim sign convention), `make_move_bg_walls` (4 walls verbatim, threshold + always-accumulate angle), `range_check_line_point`, `judge_move_bg_ground_check`, `move_bg_footprint_height`, `move_bg_delta` (translation-only carry), `MoveBgContact` + `set_side_contact`/`set_on_contact` (5-cap, no dedup), `register_slot` (first free of 64). C ABI: `pc_make_move_bg_walls` (writes `PcMoveBgWall[4]`), `pc_move_bg_delta`. `cargo check --lib` clean. Unit tests were written but NOT run, per the standing instruction. Gaps: the 64-slot registry itself stays in C for now (per the kernel-first strategy); no C callers rewired; full Windows game link unverified.

### Runtime Port Progress: Bridge Policy

### Source findings (all verified against the local decomp)

- **Bridge = terrain state, not an object**: attributes 27-35 live in the terrain unit's 6-bit attribute field; the bridge is not part of the moving-background system.
- **Two policy layers** sharing `old_in_water` (copied from the previous frame's `result.is_in_water`): the wall-generation policy (what walls exist) and the ground/water policy (whether the actor stays water-classified).
- **Wall policy formalization** (`mCoBG_RegistNormalWallVector_AttributeOff`): !old_in_water -> normal; WOOD<->bridge(27-35) -> flatten bridge side to min corner height, slate off, ordinary generation; bridge<->bridge -> suppress; bridge<->non-wood -> suppress; non-bridge<->non-bridge -> normal. The special case requires the non-bridge side to be specifically WOOD.
- **Slope policy**: `mCoBG_RegistSlatingWallVector_AttributeOff_Slate_OldInWater` returns early for 27-35 when old_in_water — no slope wall on bridges in the water state.
- **Attribute lookup split** (`mCoBG_Wpos2Attribute`, m_collision_bg.c:1561): 27-31 -> area-dependent `mCoBG_woodb_water_info` (RIVER_*/WOOD per quarter); 32-35 -> STONE. The wall policy treats 27-35 uniformly, but attribute lookup does NOT.
- **Unit areas** (`mCoBG_GetUnitArea`, verbatim): triangle test on local (x,z) -> AREA_N=0/W=1/S=2/E=3.
- **Water table** (`mCoBG_unit_attribute_water_info`, verbatim, 64 entries): water/river attrs map to themselves; wood-bridge corners 27-30 and river banks 39-42 map to their river corners; everything else -> GRASS0.
- **Ground/water search** (m_collision_bg.c:1730): gated on `attribute_wall == FALSE && old_in_water && 27 <= attr <= 35`; scans directions 0..8 masked by `bridge_search_water`; each neighbor's RAW attribute goes through the water table; FIRST water/river result wins (direction order significant); result unit_attribute = selected water attr; water height computed at the actor's CURRENT position, not the neighbor's.
- **Feedback loop**: `result.is_in_water` -> next frame's `old_in_water` -> bridge policies -> new `is_in_water`.
- **Inference**: the machinery's purpose (water<->bridge transitions without phantom cliff walls, preserving water behavior around bridges) is interpretation, not a source comment.
- **CORRECTION during this work**: the attribute constants in the first terrain_walls commit were wrong (WATER=8/WOOD=19/SAND=18); the actual enum is GRASS0=0, WATER=12, RIVER_NE=21, SAND=22, WOOD=23, SEA=24. Fixed in this commit along with the woodb table values. The earlier tests passed only because they used the wrong constants consistently - caught by re-verifying against the header enum.

### Rust rewrite implementation

### Runtime Port Progress: AttributeWall_Special Exact Differences


### Runtime Port Progress: Line-vs-Column Sweep

### Runtime Port Progress: Placement Passes (Beach/Bridge/Slope/Buildings/Pond)

### Source findings (all verified against the local decomp)

- The generator is a constraint-satisfaction loop: 9 acceptance bits (SLOPE_LEFT, SLOPE_RIGHT, BRIDGE_UPPER, BRIDGE_LOWER, SHRINE, POLICE, MUSEUM, POOL, NEEDLEWORK), regenerated until all set. "Placement" is monotonic acre-type replacement, not coordinate placement.
- 7x10 grid, 5x6 playable interior. Player house at (3,2) in l_base_blocks is a hard obstacle during cliff/river tracing.
- Beach (mRF_SetMarinBlock): z=6, x=1..5: FLAT->BEACH, RIVER_SOUTH->BEACH_RIVER; (0,6)/(6,6) -> BORDER_CLIFF_OCEAN_LEFT/RIGHT.
- Bridges (mRF_SetBridgeBlock): 7 waterfall crossing types anchor the split; upper bridge mandatory (random river before crossing), lower bridge only if after_cross != 0 && stepmode==TWO && (RANDOM(10)&1); offset RIVER_SOUTH_BRIDGE - RIVER_SOUTH = 7 preserves direction. Beach-mouth fallback: BEACH_RIVER -> BEACH_RIVER_BRIDGE if BRIDGE_LOWER unset.
- Slopes (mRF_SetSlopeBlock): scan for BORDER_CLIFF_LEFT_TRANSITION, follow cliff contour, split LEFT/RIGHT at RIVER_CLIFF_ANY crossings; one random slope per side; replacement SLOPE_HORIZONTAL + cliff idx.
- Shrine/Police/Museum (mRF_SetUniqueFlatBlock): shrine prefers random side below cliff, police prefers opposite side first, museum takes either side; sequential and destructive.
- Shop/Post (mRF_SetUniqueRailBlock): TRACKS_SHOP/TRACKS_POST_OFFICE at z=1, bx = 1+RANDOM(2) and 4+RANDOM(2), requiring TRACKS_DUMP cells; left/right assignment randomized.
- Needlework/wharf: (5,6) must be BEACH -> PORT else fail; needlework picks the RANDOM(3)-th BEACH cell scanning x=1..5.
- Pond (mRF_SetPoolBlock): pure river types only (40..46, excludes composites/bridges), random one -> POOL_SOUTH + offset (29) preserves direction.
- Full pass order from mRF_MakeRandomField_ovl: base landform -> flat info -> beach -> bridges+slopes -> needlework/wharf -> unique flat -> unique rail -> pool -> beach fallback -> heights -> SelectBlock -> copy heights.
- Step-mode selection: mRF_GetRandom(100) < 15 (15% three-step).
- Correction vs brief: pass order matched source exactly; brief's "34..36" group bound was corrected in the albumin module.

### Rust rewrite implementation

`rust/src/placement.rs`: grid dims, block ids (script-extracted), bridge/pool offsets, acceptance bits, marin_cell, bridge_variant, lower_bridge_ok, slope_variant, pool_variant, is_pure_river, wharf/rail constants, is_step_three. C ABI: pc_marin_cell, pc_bridge_variant, pc_lower_bridge_ok, pc_slope_variant, pc_pool_variant. `cargo check --lib` clean. Unit tests were written but NOT run, per the standing instruction.

### Gaps

- mRF_MakeFlatPlaceInfomation classifications and mRF_FlatBlock2Unique selection not yet ported. Full grid-array passes (bridges/slopes) remain source-side; Rust has the per-cell resolvers. mRF_SelectBlock and data_combi resolution not yet ported.

### Runtime Port Progress: Collision Registration Internals (Scene_Read, Pools, Dispatch)

### Source findings (all verified against the local decomp)

- Gameplay_Scene_Read/Init perform no collision registration; CollisionCheck_ct runs once in play_init; CollisionCheck_dt is empty.
- Two registration pools: collider_table (OC) and mco_work.colliders[10] (OCC). CollisionCheck_clear empties both; object state untouched.
- Frame order (m_play.c): CollisionCheck_OC (OCC pass at its end) -> CollisionCheck_clear -> Actor_info_call_actor.
- TWO similarly-named clear tables (easy trap): OCClearFunctionTable (setOC: JntSph/Pipe/Tris OCClear) vs OCCClearFunctionTable (setOCC: NULL/NULL/Tris OCCClear; setOCC rejects non-Tris first).
- ClObj_OCClear (OC family): clears COLLIDED, collided_actor, PLAYER_WAS_HIT. ClObj_OCCClear (OCC family): clears collided_actor and flags1 &= ~0x04 (TRIS_HIT; source names the constant ~ClObj_FLAG_DONT_UPDATE_POS, a flags0-family name applied to flags1). CORRECTION (2026-10-07): the earlier "TRIS_HIT never cleared" analysis was wrong -- see the TRIS_HIT correction section.
- ClObjTrisElem_OCClear clears element FLAG_HIT; ClObjTrisElem_OCCClear zeroes attribute.t. (Brief conflated these names.)
- Dispatch: OC = JntSph/Pipe x JntSph/Pipe; OCC = Tris x JntSph/Pipe. setOCC_HitInfo sets TRIS_HIT + collided_actor + hit position (no COLLIDED).
- ClObj_set4 overwrites flags0/flags1/type wholesale.

### Correction (2026-10-07): TRIS_HIT is a one-frame latch, not a persistent flag

Retracts the earlier "TRIS_HIT anomaly" (commits 515579a, 03ecf54). The anomaly came from reading the constant NAME instead of its numeric effect:

- ClObj_OCCClear (m_collision_obj.c:698): `col->collision_flags1 &= ~ClObj_FLAG_DONT_UPDATE_POS;`
- ClObj_FLAG_DONT_UPDATE_POS == 0x04, but that name belongs to the flags0 family. The operation targets collision_flags1.
- Numerically: `collision_flags1 &= ~0x04` -> clears flags1 bit 2 == ClObj_FLAG2_TRIS_HIT.
- So every setOCC clears TRIS_HIT, collided_actor, and the element hit position. The intended invariant (net check: TRIS_HIT==0 => don't consume collided_actor) holds.
- Nothing in the current source ever clears the actual flags0 DONT_UPDATE_POS bit (only read at m_collision_obj.c:379).
- Player side confirms the latch: axe/net triangles registered via setOCC from Player_actor_SetPosition_OBJtoLine_forItem (m_player_common.c_inc:2133); TRIS_HIT consumed by Player_actor_Check_OBJtoLine_forItem_axe/net (lines 2055/2062) during Actor_info_call_actor, which runs AFTER CollisionCheck_OC and BEFORE the next registration.
- `~ClObj_FLAG_DONT_UPDATE_POS` appears exactly once in src/ (the OCCClear line), consistent with a flag-refactor naming slip rather than a second semantic use.
- Rust fix: occ_clear_flags0 -> occ_clear_flags1 (clears TRIS_HIT); tris_hit_survives_setocc -> tris_hit_cleared_by_setocc; C ABI pc_occ_clear_flags0 -> pc_occ_clear_flags1. No C callers existed.
- Remaining assembly question (deferred per Philip): confirm retail ClObj_OCCClear stores byte col+0x09 &= 0xFB (vs col+0x08).
- CORRECTIONS vs brief: (1) DONT_UPDATE_POS is cleared by ClObj_OCCClear, NOT ClObj_OCClear; (2) attribute.t zeroing is ClObjTrisElem_OCCClear, not OCClear.

### Rust rewrite implementation

Extended `rust/src/collision_temporal.rs`: pool constants, oc_dispatch/occ_dispatch tables, set4_flags, frame_order, clarified the two-table trap in docs. C ABI: pc_oc_dispatch, pc_occ_dispatch. `cargo check --lib` clean. Unit tests were written but NOT run, per the standing instruction.

### Gaps

- Retail assembly check of ClObj_OCCClear's flags1 &= ~0x04 (byte col+0x09) still needs static.dol (deferred per Philip).

### Runtime Port Progress: Albumin Physical Pipeline (keep_h, Bitfield, Height Layers)

### Source findings (all verified against the local decomp)

- Complete logical-to-physical pipeline: cliff topology + river topology -> mRF_RiverAlbuminCliff (3x7 lookup, 17 valid) -> combined block type -> mRF_SelectBlock -> data_combi_table -> bg_id -> data_bgd -> mFM_SetBG -> mFM_BgUtDataSet copies collision[16][16] into bg_info with NO transformation (m_field_make.c:121-135).
- keep_h semantics (m_field_make.c:128): `keep[0] = data[0].data.center` -- the saved/base height map initializes directly from collision center heights, not from a separate authored map.
- mCoBG_Collision_u is 32 bits (m_collision_bg.h:163): slate_flag:1 (bit 31), center:5, top_left:5, bot_left:5, bot_right:5, top_right:5, unit_attribute:6 (bits 5..0). Heights are 5-bit (max 31).
- Two height layers, do not conflate: (A) acre/base height from mRF_MakeBaseHeightTable (procedural cliff-topology level); (B) unit collision height from data_bgd (physical surface inside the acre). world_y = unit_height * 10.0 + acre_base_height.
- All 17 BG_TYPE_GRD_S_C*_R*_1 names confirmed present in data_combi.c; corner rivers (river4-7) map to river_no_album_data -- albumin is specifically the straight-river/cliff composite family.
- Note: the literal 17x256 collision words were already extracted from bg_data.c in the PC-port decomp (commit bb70ca1); no separate asset dump was needed for our repo.

### Rust rewrite implementation

Extended `rust/src/albumin_geometry.rs`: keep_h_init, pack_col_unit/unpack_col_unit (exact bitfield), documented the two height layers. `cargo check --lib` clean. Unit tests were written but NOT run, per the standing instruction.

### Gaps

- None. The physical pipeline is fully modeled.

### Runtime Port Progress: Quest System (Request Flow, Completion, Rewards)

### Source findings (all verified against the local decomp)

- Architecture: m_quest.c owns persistent quest state + helpers; ac_quest_manager.c owns quest definitions (aQMgr_set_data_c), the 35-slot runtime registration table (rebuilt every check cycle from persistent records in Private_c/Animal_c), and periodic checks; ac_quest_talk_init.c owns the request/completion/reward transaction.
- mQst_base_c is 12 bytes: quest_type:2, quest_kind:6, time_limit_enabled:1, progress:4, give_reward:1, unused:2, + 10-byte RTC time_limit. Delivery 0x28, errand 0x58, contest 0x28 bytes.
- Generic completion: progress == 0 (delivery/ordinary errands). Contest kinds use dedicated checks; fish/insect are CATEGORY-based (ITEM1_CAT_FISH/INSECT), not exact requested-item matches.
- Reward tables (source-verbatim): delivery NORMAL 40/0/0/0/0/30/30 (pay 200), FOREIGN 40/0/0/10/10/40 (1000), REMOVE 20/0/0/20/20/40 (1000), LOST 40/0/40/10/10 (0); contest fruit 0/0/0/30/30/40 (500), soccer 40/0/0/30/30, snowman/flower 60/0/0/20/20, fish/insect/letter 80/0/0/10/10. Selection via 100-slot prob_tbl, RANDOM(100).
- Errand rewards use aQMgr_actor_get_errand_reward (NOT l_set_errand_data): tiers by (used_num-1) clamped 0..3, pays {0,500,750,1000}. CORRECTION vs brief: used_num=1 pays 0, not 500.
- Timeout: mQst_CheckLimitOver -- absolute RTC deadline, 28-day hard cap, then kind tables (delivery 2/2/2/2, errand 2/2/2 then 0, contest 1/1/1/3/3/3/2) plus +3 (letter +2) for contests at progress 0.
- give_reward=TRUE when the reward can't be delivered (full pockets); quest persists and the next conversation retries the same reward.
- Money scaling: base * (scale*(100+rate))/10000, scale = 100 +/- up to 10, rate = money_power/100 clamped 700.
- Friendship: reject -3, normal reward +3, failure paths -5/-2/-1 by message id.
- New-quest attempt roll: 75% (mQst_GetRandom(4) != 0).

### Rust rewrite implementation

New `rust/src/quest.rs`: QuestBase, type/kind/reward enums, REGIST_NUM=35, all reward tables, prob_table_select, timeout tables + limit_days, contest_complete, scaled_pay, friendship constants, quest_attempt_roll. C ABI: pc_quest_complete, pc_quest_free, pc_reward_select, pc_quest_base_pay, pc_quest_limit_days, pc_contest_complete, pc_scaled_pay. `cargo check --lib` clean. Unit tests were written but NOT run, per the standing instruction.

### Gaps

- Request-generation tables (first-job vs normal type/kind selection), recipient-selection modes, entrusted-item pocket handling, and letter-quest specifics are traced in the brief but not yet ported -- natural next steps.

### Runtime Port Progress: Fishing and Bug-Catching (Ecology)

### Source findings (all verified against the local decomp)

- Fish time: 4 periods (0: 21-03:59, 1: 04-08:59, 2: 09-15:59, 3: 16-20:59); hours 21-23 match no branch and keep init TIME_0. 24 half-month terms ((month-1)*2 + (day>15)). 5-day term transition with rates 5/6..1/6 blending current/next term weights.
- Field-rank modifier: 0.5/0.75/0.875/1.0 (shared shape, different application per system).
- Fish: 7 spawn areas (pool/waterfall/river_mouth/offing/sea/river/pond); weighted selection subtracts weight*env_rate per candidate; habitat-incompatible picks retry without replacement. Spawn on 12x12 interior (2..13) water units; ocean fish need 20.0 depth; whale uses 5..10.
- Fish bite timing by size: {26,39,39,39,52,65,78,78}, doubled for normal fish. UKI float states CARRY/READY/CAST/FLOAT/VIB/COMEBACK/CATCH; 50-frame cast, 40-frame timer; gyo_type 0..39 -> ITM_FISH00..39 + trash.
- Insects: 14 spawn areas; 9 actor slots (8 normal spawn); the no-spawn roll (total<=100 -> roll against 100, leaving explicit no-spawn mass); habitat filtering BEFORE selection (unlike fish); multi-birth red dragonfly/firefly 6+rand(3).
- Net: player-side request table (8 slots, insertion order, force requests first); capture window after frame 6.0; golden net 60.0 sweep / 21.0+radius vs normal 50.0 / 15.0+radius; projection-onto-axis geometric test; first hit wins, never nearest.
- Do NOT: normalize insect weights, use one spawn helper for both, make net capture nearest-insect, or make golden net cosmetic.

### Rust rewrite implementation

New `rust/src/ecology.rs`: time terms, transition rates, field-rank rates, fish/insect spawn areas, fish_select (retry), insect_select (no-spawn mass), interior checks, bite timing, net geometry + capture test + insertion-order pick, birth counts. C ABI: pc_fish_time_no, pc_fish_term, pc_field_rank_rate_milli, pc_interior_unit_ok, pc_net_sweep_len, pc_net_capture_len, pc_insect_birth_count. `cargo check --lib` clean. Unit tests were written but NOT run, per the standing instruction.

### Gaps

- Full seasonal spawn tables (fish/insect species lists) are ported (`species.rs`); bee/ant special actors and letter-quest mail integration are traced in the brief but not yet ported.

### Runtime Port Progress: Shop/House Progression Fidelity Fixes

### Source findings (all verified against the local decomp)

House (m_home.c, ac_npc_shop_common.c):
- Three phases, not one: (A) order -> next_size+=1 (or basement_ordered=TRUE) + palette + order date, size/renew UNTOUCHED; (B) mHm_CheckRehouseOrder on a later calendar date (any y/m/d component differs) -> size=next_size (or has_basement=TRUE, or next_size=STATUE) + renew=TRUE; (C) Nook sees renew -> assigns loan (basement: 49800 + pad_1=1; else rehouse_loan[size-1]), clears renew.
- Basement orderable from MEDIUM or LARGE (old Rust required LARGE only).
- pad_1 is a basement-completion progression flag (set in the Nook renewal branch; gates the UPPER dialogue offer).
- Statue: ordered at loan==0/size==UPPER/next_size==UPPER (rank = town count capped 3); next_size=STATUE on a later date; Nook clears statue_ordered.
- Old Rust bugs fixed: order_expansion no longer sets size/renew immediately; order_basement works from MEDIUM; complete_construction split into check_rehouse_order + nook_process_renewal; pad_1/has_basement/ordered_outlook_pal added.

Shop (m_shop.c, ac_shop_level.c):
- plus_sales now uses wrapping_add (retail: plain u32 +=, then clamp) instead of saturating_add.
- disable_visitor_req labeled as a PC enhancement, not retail.
- Renovation scheduler added: schedule_renewal (+2 days at opening time, blocked by bargain-day collision on today/tomorrow/+2), renewal_due, complete_renewal. renew_level retained as the final primitive.

### Rust rewrite implementation

Rewrote `rust/src/house.rs` (three-phase API: order_expansion/order_basement/check_rehouse_order/nook_process_renewal/order_statue/nook_process_statue_built + pc_order_date_passed). Extended `rust/src/shop.rs` (wrapping_add, send_upgrade_notice, schedule_renewal/renewal_due/complete_renewal, date helpers). `cargo check --lib` clean. Unit tests were written but NOT run, per the standing instruction.

### Gaps

- The Nook dialogue/choice functions selecting LARGE vs BASEMENT vs UPPER still need a full trace for a complete Rust-ready progression function.

### Runtime Port Progress: Mail System (Beyond Letter Scoring)

### Source findings (all verified against the local decomp)

- Mail_c is a fixed 298-byte (0x12A) struct: header (recipient/sender PersonalID + name type), present (EMPTY_NO = none), content (font/state, mail_type, paper_type, 24-byte header, 192-byte body, 32-byte footer). Not a high-level Letter struct.
- Three stores: house mailbox 10 slots, player inventory mail 10 slots, Post Office transit queue 5 slots. Memory-card archive: 8 pages x 20 = 160 letters.
- Font byte is the lifecycle state: RECV/SEND/RECV_READ/RECV_PLAYER_PRESENT/RECV_PLAYER_PRESENT_READ; 0xFF = unused (mMl_clear_mail sets -1).
- Routing: player mail -> Post Office queue; automatic mail -> try recipient mailbox, fall back to PO queue. Queue full at 5 (players + NPCs).
- Delivery twice daily: <09:00 -> 09:00, <17:00 -> 17:00, else 09:00 next day. Delivery is atomic with mailbox capacity: full mailbox -> mail REMAINS queued. Queue slot cleared only after successful copy.
- PO keeps mail_recipient_flags (house bitmask) and keep_mail_sum_players/npcs.
- Reading transitions RECV -> RECV_READ (present variants likewise); present attach allowed for SEND/already-present mail; deletion rejects mail with presents.
- Name types: PLAYER/NPC/MUSEUM/CLEAR. 12 mail types (MAIL/XMAS/leaflets/MOTHER/OMIKUJI/HRA/SHOP/SNOWMAN/FISHING_CONTEST/POSTOFFICE/SPNPC_PASSWORD).
- NPC mail -> friendship (+3 sent, -5 bad rank, +3 with present); hooks first-job letter quests and the letter contest.

### Rust rewrite implementation

New `rust/src/mail.rs`: fixed-size Mail model, font state machine, slot ops (clear/unused/copy/find_free/count), Mailbox/InventoryMail/PostOfficeQueue types, PostOffice (receipt/delivery/recipient flags), delivery scheduler, auto routing, mailbox->inventory transfer, NPC friendship constants. C ABI: pc_mail_unused, pc_mail_mark_read, pc_next_delivery, pc_po_full. `cargo check --lib` clean. Unit tests were written but NOT run, per the standing instruction.

### Gaps

- Catalog-order/lottery special delivery, leaflet broadcast, mother-mail scheduler, and NPC reply generation are traced in the brief but not yet ported.

### Authorized Test Run (2026-10-07, commit e742d2a)

Philip said "Run the tests." `cargo test --lib`: **296/296 passed**, 0 failed. (294 prior + 2 new: template_select, step3_data; plus the audit-fix assertions in the albumin test.) This also runtime-verifies the audit fix. Authorization consumed.

### Code Audit (2026-10-07, self-directed)

Philip asked for an audit of earlier work for bugs/errors. Static audit (no test run - standing rule):

**Constant audit**: all block-type ids in albumin.rs, placement.rs, albumin_geometry.rs, template_select.rs, step3_data.rs re-verified programmatically against m_field_make.h - all correct. (NONE=255 fix from the previous commit confirmed in place.)

**Bug found and fixed**: `decide_albumin_cell` (albumin.rs) was missing the BORDER_CLIFF_RIVER (1) / TRACKS_RIVER (13) passthrough from mRF_DecideRiverAlbuminCliff. Fixed + test cases added.

**Verified clean**: albumin group gates + index math (no underflow: gates guarantee 0..6); decide merge semantics; marin_cell edge handling; bridge/slope/pool offsets; judge_flat_block source-faithful branches; needlework ordinal pick; base-height column order; albumin asset table order (22..38) and waterfall set (matches WATERFALL-attribute units); collision data integrity (17x256 units, attrs < 64, heights <= 16); template_select RNG-fidelity (double GetRandom on the -1 path preserved); select_traversal Z-major/X-minor; step3 dual-copy; lib.rs wiring; DOCUMENTATION.md consistency (no stale gaps).

**Warnings**: only dead-code warnings on C-ABI library functions - not bugs.

**Not covered by this audit**: runtime test verification (needs Philip's authorization); retail assembly/objdiff checks (needs static.dol from his ISO).

### Runtime Port Progress: Step-3 Template Bodies

### Source findings (all verified against the local decomp)

- Exactly ten l_mRF_step3_blocks* arrays (3, 7, 7R, 8, B, BR, E, ER, F, FR), each 70 entries, gathered in l_mRF_step3_blockss[10]. Selection = one mRF_GetRandom(10); body copied into BOTH cliff_blocks and river_blocks. No weighting, rotation, reflection, or anti-reuse between bodies; bodies are immutable source data.
- Bodies are complete 7x10 semantic layouts (borders, railroad, station, player house, cliffs, waterfalls, rivers, flats, ocean, sea-exceptional, islands), not just cliff templates.
- R variants are separate literal arrays, not runtime mirrors (brief's claim confirmed by array comparison).
- Body selection re-runs on every perfect-bit retry; stepmode (RANDOM(100) < 15) is chosen once before the loop.
- CORRECTION (own work): mFM_BLOCK_TYPE_NONE = 255 (explicit in header), not 109. My earlier positional enum extraction hit the wrong enum block; all other values (15/40/22/63/100/...) were correct. Fixed NONE in albumin.rs and placement.rs. step3_data.rs was extracted with the correct map.

### Rust rewrite implementation

`rust/src/step3_data.rs`: all ten bodies as numeric [u8; 70] (extracted programmatically, spot-checked), STEP3_BLOCKS selection table, make_base_landform_step3 (single-RNG-call, dual copy), C ABI pc_step3_select. `cargo check --lib` clean. Unit tests were written but NOT run, per the standing instruction.

### Gaps

- None on the bodies themselves.

### Runtime Port Progress: Template Anti-Reuse (SelectBlock)

### Source findings (all verified against the local decomp)

- Anti-reuse operates on concrete data_combi_table indices, not semantic types. l_use_data[70] (s16) reset to -1 at each mRF_SelectBlock; mRF_SearchAlreadyUse is exact index equality over all 70 slots.
- mRF_TypeCombCount / mRF_IndexInType2BlockNo: reuse=FALSE excludes used indices; reuse=TRUE ignores them. Uniform selection via mRF_GetRandom(count). Exhaustion -> reuse pool. Verified function bodies verbatim.
- Per-type template counts extracted programmatically from data_combi.c (368 entries, 92 NONE): brief's table matched exactly (FLAT 10, RIVER_SOUTH 4, PORT 3, POOL_* 1 each, etc.). SEA_EXCEPTIONAL has 0 entries (BG-name path).
- SEA_EXCEPTIONAL bypasses anti-reuse entirely: no l_use_data update, BG-name matching via mRF_GetExceptionalSeaBgDownBgName + mRF_BgName2RandomConbiNo. Retail bug confirmed: mRF_GetRandom(0) (always first match) with the @BUG comment; PC port BUGFIXES uses count. Both paths modeled via retail_bug flag.
- Defensive fallback BLOCK_COMBI_GRD_S_F_7 = 161 confirmed from m_combi_type.h.
- Traversal is Z-major/X-minor; selection happens after the perfect-bit loop (no town regen on template collision).
- One deviation risk noted: my select_block_type falls through to the reuse pool when IndexInType returns -1, matching the source's duplicated block.

### Rust rewrite implementation

`rust/src/template_select.rs`: use_data_reset, search_already_use, type_comb_count, index_in_type_2_block_no, select_block_type (with rand_n closure for RNG fidelity), bg_name_2_random_combi_no (retail_bug flag), select_traversal. C ABI: pc_type_comb_count, pc_search_already_use. `cargo check --lib` clean. Unit tests were written but NOT run, per the standing instruction.

### Gaps

- mRF_GetExceptionalSeaBgDownBgName table (27x2) not yet ported. Full SelectBlock grid loop remains source-side.

### Authorized Test Run (2026-10-07, commit d2c3957)

Philip said "Run the tests." `cargo test --lib`: **294/294 passed**, 0 failed. (281 from the previous run + 13 new: albumin table, placement passes, albumin geometry trace, placement deep dive.) Authorization consumed.

### Runtime Port Progress: Placement Passes, Deep Dive (Flat Info, Unique Buildings, Heights)

### Source findings (all verified against the local decomp)

- Index math: mRF_D2ToD1(bx, bz) = bz * 7 + bx. Placement scans cover z=0..7 (56 cells) - correction vs the brief's "42 blocks" claim.
- mRF_MakeFlatPlaceInfomation: initializes all cells to BOTH/BOTH, then per-column top-down scan (ABOVE until CLIFF_ANY hit -> BELOW) and per-row left-right scan (LEFT until RIVER or RIVER_CLIFF_ANY -> RIGHT). Computed BEFORE beach/bridge/slope mutation and never recomputed - the pass order is load-bearing.
- mRF_JudgeFlatBlock: source-faithful predicate ported. Subtle: a BOTH request is not a pure wildcard - when cliff_height == BOTH the source requires the info array to literally equal BOTH (true only on unclassified border cells). Live calls always use BELOW.
- mRF_FlatBlock2Unique: count qualifying -> RANDOM(num) -> rewrite n-th in scan order (sequential, destructive).
- Shrine: side0 = RANDOM(100)&1, side1 = side0^1, tries side0 then side1, BELOW cliff. Police: tries side1 (opposite) first, then side0. Museum: BOTH sides, BELOW.
- Shop/Post: RANDOM(1000)&1 decides which goes to x=1..2 vs x=4..5 slots on z=1; each waits for a TRACKS_DUMP cell.
- Needlework: RANDOM(3)-th BEACH cell in x=1..5 scan order (ordinal, not x = r+1) - encoded as needlework_pick.
- Dock has no success bit: (5,6) must be BEACH -> PORT, else the whole needlework/wharf pass returns 0 and NEEDLEWORK is never set.
- mRF_GetBlockBase: per X column scanned z=9->0 from FIELD_STEP1 (=0), incrementing after cliff-horizontal/top-right/top-left bits or border transitions.
- mRF_SelectBlock unique-avoidance: l_use_data + mRF_SearchAlreadyUse prefers unused combinations, reuses when exhausted.
- mRF_MakePerfectBit: builds 0x1FF from mRF_BIT_NUM.

### Rust rewrite implementation

Extended `rust/src/placement.rs`: d2_to_d1, PLACEMENT_CELL_COUNT, side/cheight enums, classify_river_row, classify_cliff_col, judge_flat_block (source-faithful), rewrite_flat_idx, shrine_sides, shop_first, needlework_pick, base_height_column, PASS_ORDER (12 passes). C ABI: pc_d2_to_d1, pc_shrine_sides, pc_needlework_pick. `cargo check --lib` clean. Unit tests were written but NOT run, per the standing instruction.

### Gaps

- Full grid-array passes (bridge/slope/building selection loops) remain source-side; Rust has the per-cell/per-row resolvers. mRF_SelectBlock combination resolution not yet ported.

### Runtime Port Progress: Albumin Physical Geometry Table

### Source findings

- Reconstructed all 17 albumin outputs as exact physical geometry: per-asset 16x16 ASCII height maps generated from the extracted collision data, plus a summary table (river direction, cliff shape, waterfall or not, BG asset, water height, sloped-unit count).
- Geometric rules distilled: waterfalls = river CROSSES the cliff contour (water drops 12/16 -> 0 across WATERFALL-attribute units); non-waterfalls = river ALONGSIDE the cliff on one terrace level. South rivers run at 0/12, east/west at 12. Cliff faces are 1-2 unit wide sloped bands (9-20 units). Water channels are ~3 units wide. East/west families are mirrors.
- Full reference: `albumin_geometry.md` in the repo root.

### Rust rewrite implementation

- `rust/src/albumin_geometry.rs`: added ASSET_RIVER_DIR and ASSET_CLIFF_IDX tables (asset index -> river direction / cliff shape).
- `cargo check --lib` clean. Unit tests were written but NOT run, per the standing instruction.

### Gaps

- None. The symbolic 7x7 table is now a physical geometry table.

### Runtime Port Progress: Albumin Physical Collision Data

### Source findings

- src/data/field/bg/acre/bg_data.c contains the complete data_bgd array with real 16x16 mCoBG_Collision_u collision data for all 17 albumin BG assets. No disc extraction needed.
- Unit layout (mCoBG_CollisionData_c): slate_flag:1, center:5, top_left:5, bot_left:5, bot_right:5, top_right:5, unit_attribute:6 - 32 bits total.
- Height levels observed: 16 = cliff-top ground, 12 = mid/river-on-terrace, 4 = low ground, 0 = water. Attributes: GRASS0/GRASS2/SOIL/BUSH for land, RIVER_S/SE/SW/E + numbered bank/cliff variants (39-62) for water edges, WATERFALL where the drop is.
- ASCII height maps confirm the physical reading: e.g. GRD_S_C1_R1_1 shows high ground north, a south-flowing river channel, a cliff-face band (height 12) where the river crosses the cliff, low ground south - the literal waterfall-over-cliff geometry. GRD_S_C3_R1_1 shows the cliff face running vertically with the river alongside.
- Attribute enum: 64 values (0-24 named, 25-63 numbered with comments: wood/stone bridges, waves, river banks, grass cliff variants, slate).

### Rust rewrite implementation

- `rust/src/albumin_collision_data.rs`: full 17x256 collision tables extracted programmatically (source-verified spot checks), attribute name table, ColUnit type. ~35KB of real geometry data.
- `rust/src/albumin_geometry.rs`: added albumin_unit / albumin_center_height / albumin_unit_world_y accessors. C ABI: pc_albumin_center_height, pc_albumin_unit_world_y.
- `cargo check --lib` clean. Unit tests were written but NOT run, per the standing instruction.

### Gaps

- None on the collision data itself. Remaining: wiring these tables into the Rust runtime terrain-height queries used by gameplay.

### Runtime Port Progress: Albumin Outputs to BG Geometry/Collision/Height

### Source findings (all verified against the local decomp)

- All 17 albumin block types have entries in data_combi_table (src/data/combi/data_combi.c), extracted programmatically. Primary BG assets: south row GRD_S_C1..C7_R1_1, east row GRD_S_C1..C5_R2_1, west row GRD_S_C1/C4/C5/C6/C7_R3_1. Variant counts: mostly 2-3 (WATERFALL_STRAIGHT_CLIFF_HORIZONTAL and both C1_R2/C1_R3 have 3; WATERFALL_WEST_CLIFF_VERTICAL_LEFT has only 1).
- The brief's BG/FG table matched the extraction exactly.
- Chain: albumin type -> mRF_SelectBlock (random pick among type's variants) -> combination_type -> data_combi_table -> bg_id -> sorted data_bgd -> mFM_SetBG -> mFM_BgUtDataSet copies collision[16][16] and keep_h into bg_info.
- Per-unit geometry (mCoBG): 4-corner heights + center + attribute + slate_flag; mCoBG_GetUnitArea partitions each unit into 4 triangles; world Y = corner * 10.0 + acre base height. Flat units use center*10+base.
- Acre base height is separate: mFM_combination_c = combination_type:14 + height:2, from mRF_GetBlockBase (cliff types increment per-column height); copied into save data.
- Waterfall family: 7 of 17 outputs (22,23,26,30,31,37,38); the rest are RIVER_* non-waterfall hybrids.
- CORRECTION (same day): the numeric 16x16 collision values ARE in the decomp source after all - src/data/field/bg/acre/bg_data.c carries full mCoBG_Collision_u arrays for every BG asset including all 17 GRD_S_C*_R* types. The "disc extraction" gap was wrong; see the Albumin Physical Collision Data section below.

### Rust rewrite implementation

`rust/src/albumin_geometry.rs`: ALBUMIN_ASSETS (17 entries with primary BG/FG + variant counts), albumin_asset lookup, is_waterfall_output, 16x16 grid constants, COLLISION_HEIGHT_SCALE=10.0, combi bit layout (14+2), world_y(corner, base_height). C ABI: pc_albumin_asset_idx, pc_is_waterfall_output, pc_albumin_variants. `cargo check --lib` clean. Unit tests were written but NOT run, per the standing instruction.

### Gaps

- Numeric collision arrays for the 17 GRD_S_C*_R* assets require game-disc data extraction (out of scope for the decomp source).

### Runtime Port Progress: River-Cliff Albumin Tables

### Source findings (all verified against the local decomp)

- mRF_RiverAlbuminCliff(cliff_type, river_type): group-gated (CLIFF 15-21, RIVER 40-46), indices river-40 / cliff-15 into river_cliff_album_data[7][7]. Three real rows (south/east/west) + four all-NONE rows (corner rivers). 17 valid cells: south 7, east 5, west 5.
- Exact rows: south -> 22..28 (WATERFALL_STRAIGHT_CLIFF_HORIZONTAL .. RIVER_STRAIGHT_CLIFF_BOTTOM_LEFT_CORNER); east -> 29,30,31,32,33,NONE,NONE; west -> 34,NONE,NONE,35,36,37,38.
- Block group ranges (blockGroup): RIVER_CLIFF_ANY 22..38, RIVER_CLIFF_1 22..28, RIVER_CLIFF_2 29..33, RIVER_CLIFF_3 34..38 (brief said 34..36; source says 38). Enum values extracted programmatically from m_field_make.h.
- mRF_DecideRiverAlbuminCliff: valid albumin -> cliff_blocks = combined type; no albumin but river present -> cliff_blocks = river block (ordinary river); no river -> unchanged. So cliff_blocks becomes the merged landform map.
- River-trace legality: mRF_TraceRiverPart2/Part1 call mRF_RiverAlbuminCliff when the next acre has a cliff; incompatible -> return FALSE, failing the river-generation attempt (rejection, not correction).
- Step-3 towns bypass albumin (pre-authored templates).
- Combined types keep cliff-direction bits in mRF_GetSystemBlockInfo (height calc) and participate in RIVER_CLIFF_ANY checks downstream.

### Rust rewrite implementation

`rust/src/albumin.rs`: bt/group constants, ALBUMIN 3x7 table, river_albumin_cliff (group gates + index math), decide_albumin_cell (merge semantics), albumin_valid_count. C ABI: pc_river_albumin_cliff, pc_decide_albumin_cell. `cargo check --lib` clean. Unit tests were written but NOT run, per the standing instruction.

### Gaps

- Step-3 template bodies and the full mRF_SelectBlock combination-resolution pass not yet ported. BG geometry/collision per albumin output (the suggested next target) not yet traced.

### Runtime Port Progress: Collision Temporal Lifecycle

### Source findings (all verified against the local decomp)

- Frame order (m_play.c): CollisionCheck_OC (OCC runs at its end) -> CollisionCheck_clear -> Actor_info_call_actor -> draw/registration. CollisionCheck_clear only NULLs the registration containers (collider_table, mco_work.colliders) and zeroes the counts; it does NOT touch object state.
- Two registration pools: OC (collider_table via setOC) and OCC (mco_work via setOCC, cap 10, TRIS-only).
- Two parallel clear families (naming corrected vs. brief): setOC -> OCClearFunctionTable (JntSph/Pipe/Tris OCClear -> ClObj_OCClear: flags0 &= ~COLLIDED, collided_actor=NULL, flags1 &= ~PLAYER_WAS_HIT; elements &= ~HIT). setOCC -> OCCClearFunctionTable (ClObjTris_OCCClear -> ClObj_OCCClear: collided_actor=NULL, flags1 &= ~0x04 == ~TRIS_HIT; element attribute.t zeroed). CORRECTION (2026-10-07): the source writes the constant as ~ClObj_FLAG_DONT_UPDATE_POS, a flags0-family NAME applied to flags1; numerically it clears flags1 bit 2 = TRIS_HIT. The earlier "TRIS_HIT anomaly/persistent hit" analysis was wrong -- see the TRIS_HIT correction section below.
- Flag values (m_collision_obj.h): flags0 COLLIDED=0x02, DONT_UPDATE_POS=0x04; flags1 PLAYER_WAS_HIT=0x01, OCC_CHECK=0x02, TRIS_HIT=0x04; element HIT=0x02.
- ANOMALY RETRACTED (2026-10-07): the earlier analysis claimed TRIS_HIT was never cleared because no `~ClObj_FLAG2_TRIS_HIT` exists in the source. That was a misreading of a misleading constant NAME. ClObj_OCCClear does `collision_flags1 &= ~ClObj_FLAG_DONT_UPDATE_POS`; ClObj_FLAG_DONT_UPDATE_POS == 0x04, and the operation targets flags1, so numerically it is `flags1 &= ~0x04`, clearing flags1 bit 2 == TRIS_HIT. TRIS_HIT is a one-frame result latch, not a persistent flag. Nothing ever clears the actual flags0 DONT_UPDATE_POS bit. See the TRIS_HIT correction section below.
- Temporal pipeline: player draw path registers axe/net triangles via setOCC (Player_actor_SetPosition_OBJtoLine_forItem); the OCC result is consumed one frame later in player movement (Player_actor_Check_OBJtoLine_forItem_*). Body pipes register via setOC in movement. So tool geometry is one frame ahead of semantic consumption.
- Multiple OCC hits: no break in the outer actor loop; later hits overwrite collided_actor, so the final payload is the last qualifying collider in table order (registration order = actor traversal order).
- Actor Status_c (damage/effects/collision_vec) is cleared separately at the end of each actor's update (CollisionCheck_Status_Clear) -- different reset point from ClObj flags.

### Rust rewrite implementation

`rust/src/collision_temporal.rs`: flag0/flag1/elem_flag constants, OCC_WORK_CAP, oc_clear_*/occ_clear_flags1 (both clear families), tris_hit_cleared_by_setocc, frame_phase order, TOOL_TRIANGLE_PIPELINE_FRAMES, occ_hit_info/setocc_after_hit lifecycle simulation, pool/dispatch/frame_order/set4_flags. C ABI: pc_oc_clear, pc_occ_clear_flags1 (replaced pc_occ_clear_flags0), pc_tris_hit, pc_oc_dispatch, pc_occ_dispatch. `cargo check --lib` clean. Unit tests were written but NOT run, per the standing instruction.

### Gaps

- Retail assembly verification of ClObj_OCCClear's `flags1 &= ~0x04` against static.dol (deferred per Philip's instruction; would confirm the mask lands on byte +0x09).

### Runtime Port Progress: Full Trace Loops

### Source findings (all verified against the local decomp)

- graph_proc: outer game loop (construct/run/destroy/next game via dlftbl) + inner frame loop while game_is_doing(). Per iteration: dt from OS ticks, SECONDS_TO_FRAMES, capped at 4.0 60-Hz frames, PC speedhack multiplies; graph->dt/dt_num_60fps_frames/dt_total_60fps_frames feed simulation (not 1 frame = 1 tick).
- graph_main: setup_double_buffer -> game_get_controller -> game_main -> draw_finish -> task_set00 (emu64) -> audio -> reset_check. Game logic precedes display-list submission.
- game_main: game_draw_first -> mTM_time -> exec (play_main) -> mBGM_main -> game_move_first -> frame_counter++.
- Game_play_move: submenu ctrl -> (WAIT: mDemo_Main, mEv_run) -> demo stock clear -> object-exchange DMA -> submenu move -> (WAIT: game_frame++, CollisionCheck_OC -> CollisionCheck_clear -> Actor_info_call_actor -> decal/msg) -> fade/camera/kankyo/wind/footsteps.
- Key ordering: CollisionCheck_OC runs BEFORE Actor_info_call_actor (collision staged, not per-actor); CollisionCheck_clear also precedes the actor loop.
- Actor_info_call_actor: ACTOR_PART_NUM partitions x linked lists; per actor: ct_proc (DMA-gated construction) / DMA-fail delete / no-mv_proc (delete or Actor_dt) / normal (last_world_position, player-relative metrics, ACTOR_STATE_24 clear, culling check incl. ACTOR_PART_NPC always moves, mv_proc, CollisionCheck_Status_Clear).
- CollisionCheck_OC: pairwise col1p/col2p=col1p+1 over collider_table, group/flag/owner checks, oc_collision_function[type1][type2] dispatch; then CollisionCheck_OCC (mco_work.colliders x table, occ_collision_function, cap 10 registrations).
- Axe/net: real ClObj triangles. Axe: start +31 Y, 35 units forward at +/-8.0255126953125 deg; hit test is just TRIS_HIT flag check (work done in the collision pass).
- Line trace (mCoBG_LineCheck_RemoveFg): 3x3 unit neighborhood ground trace (4 polygons/unit, plane then triangle), moving-BG quad loop, FG column wall trace (iterative XZ circle + Y reconstruction) and ground trace, reverse vectors accumulated (wall, wall-column, ground, ground-column) and summed; water = 19-21 band crossing OR endpoint below water height (UNDERWATER).
- NPC route trace (aNPC_trace_route): avoid_direction is the route-node cursor; FALSE means movement action completed, not failure; final node sets destination.
- Four trace meanings kept separate: game/frame, actor, collision, terrain line, NPC route.

### Rust rewrite implementation

`rust/src/frame_loops.rs`: DT_MAX_FRAMES + clamp_dt_frames, graph_phase/game_phase/play_move_phase order enums, actor_branch, OCC_WORK_CAP=10, axe triangle constants, LINE_TRACE_UNITS/AREAS, water band + water_crossing, reverse_slot order, npc_route_step. C ABI: pc_clamp_dt_frames, pc_water_crossing, pc_npc_route_step. `cargo check --lib` clean. Unit tests were written but NOT run, per the standing instruction.

### Gaps

- The suggested next layer (which registrations survive frames; when TRIS_HIT/collided_actor/OCC table clear) not yet traced. Column trace math and moving-BG quad generation not ported.

### Runtime Port Progress: Weather/Seasons + Tool Target Resolvers

### Source findings (all verified against the local decomp)

Weather/seasons:
- 18 calendar terms (mTM_calender, m_time.c): end-date table with season + bgitem_profile + bgitem_bank per term; lookup returns first term with month<end_month || (month==end_month && day<=end_day). Island climate bypasses the table and forces term 7 (summer).
- mTM_set_season_com writes four fields: season, term_idx, bgitem_profile, bgitem_bank.
- 20 weather terms (mEnv_GetWeatherChangeStep) with a 20-entry probability table; each entry sums to 10; roll is RANDOM_F(10) walked cumulatively. Outcomes: clear/rain-light/rain-heavy(Thunder)/snow-light/snow-heavy(snow)/sakura-light/sakura-heavy.
- @BUG confirmed in source: without BUGFIXES the sakura probability reads (weather>>8)&0xF (the snow field) instead of (weather>>0)&0xF; the port documents the intended fix. Port defaults to original behavior.
- Event weather override (mEv_GetEventWeather): WEATHER_CLEAR -> CLEAR, WEATHER_SNOW -> SNOW, WEATHER_SPORTS_FAIR -> CLEAR, else -1; then first-job rain is cleared to CLEAR/NONE. Weather saved as one byte: intensity | (weather<<4).
- Wind: 5 terms (end dates), (calm/normal/gusty) percents per term, power ranges calm 0-0.4 / normal 0.4-0.6 / gusty 0.6-1.0. Koinobori event forces wind angle 135 deg (0x6000) and power 1.0 in both mEnv_ChangeWind and mEnv_InitWind.
- Daily renewal boundary: mTM_FIELD_RENEW_HOUR = 6.

Tool resolvers:
- mPlib_Check_scoop_after: 8-neighbor unit search (player's unit omitted), angle-first selection with 180/360 wrap, diagonal wall rejection (both cardinal neighbors walled) + SQ(63.245553) distance cutoff with cardinal fallback, missing unit -> AIR_SCOOP, +/-63.245552 vertical threshold, NPC exclusion within SQ(39) -> AIR_SCOOP (10-Bell rock exception on GAFU+), snowman/snowball/ball stored as reflect actors.
- mFI_GetDigStatus: dig check table {MISS,CANCEL,FILLIN,DIG,PUT_ITEM,GET_ITEM}; golden shovel only affects DIG: area gate (differs by >half unit from static old_pos) + RANDOM(10)==1 -> GET_ITEM with ITM_MONEY_100 (0x2103). old_pos updates on every DIG regardless of shovel.
- PUTIN_SCOOP: player_drop_entry_proc runs at state entry (logic before animation); burial effect at frame 18 (25 for FILL_UP_I1); golden flag -> DEMO_GET_GOLDEN_ITEM(SHOVEL) at completion instead of normal fill completion.
- REFLECT_SCOOP frame 13: speed 4.8 + 180deg reversal, UZAI set, sound/vibration, insect notify; strike effect at 37 units forward + 2 lateral from player pos.
- Axe durability (Player_actor_GetitemNo_forDamageAxe): +1 normal / +3 reflected; at >=9 damage the item advances one wear stage (AXE->USE_1->...->USE_7->EMPTY_NO) and the counter resets (verified: damage reset in swing_axe frame 15). Golden tools bypass.
- Net: capture evaluated after frame 6 (CatchSomethingCheck_common 6.0f); sweep 50/60, radial tolerance 15/21 + target radius for normal/gold.
- Rod: ready-rod frame >=10 projects 100 units forward; 5 samples (center + 4x +/-10); each needs water attribute, no movable-BG collision, surface <60 above player; else AIR_ROD.

### Rust rewrite implementation

`rust/src/weather_season.rs`: CALENDAR_TERMS (18), term_idx, season constants, WEATHER_TERMS/TABLE (20), weather_term, weather_roll (with bugfix_sakura flag), event_weather_override, pack/unpack save byte, FIELD_RENEW_HOUR, WIND_TERMS/PERCENTS/POWER_RANGES, wind_term, Koinobori constants. C ABI: pc_term_idx, pc_weather_term, pc_weather_roll, pc_event_weather, pc_wind_term.
`rust/src/tool_resolvers.rs`: SCOOP_NEIGHBORS/DIAGONALS, distance/vertical/NPC constants, dig_status enum, gold-shovel area gate + injection, PUTIN/REFLECT frame constants, reflect_scoop_effect_offset, axe durability (axe_apply_damage), net geometry (net_sweep_length, net_radial_tol), rod validation (rod_sample_ok). C ABI: pc_gold_shovel_dig, pc_axe_apply_damage, pc_net_geometry, pc_rod_sample_ok. `cargo check --lib` clean. Unit tests were written but NOT run, per the standing instruction.

### Gaps

- Event schedule compiler (event_schedule_data[] symbolic dates, decode_date, event_today[] bitmasks) not yet ported -- the event-scheduling half of the brief. NPC schedule event overrides (first-job/Halloween -> FIELD) already in npc_ai.rs.
- Weather actor (ac_weather) particle/state machine not ported.
- Axe ±75deg candidate filtering and the net sweep line-collision not ported (geometry helpers live in C).

### Runtime Port Progress: Scene Table + Scene_ct Interpreter

### Source findings (all verified against the local decomp)

- The scene system: 52 scene IDs (m_scene_table.h) -> scene_word_data[52] pointer table in Gameplay_Scene_Read (m_play.c) -> per-scene Scene_Word_u[] manifest -> Scene_ct() walks 8-byte words until type==END, dispatching through an 11-entry Scene_Proc table. Gameplay_Scene_Read() is a selector/activation layer, not a parser: it installs scene_data_status[idx], scene_id, current_scene_data, then calls Gameplay_Scene_Init().
- Scene word types: PLAYER_PTR=0, CTRL_ACTOR_PTR=1, ACTOR_PTR=2, OBJECT_EXCHANGE_BANK_PTR=3, DOOR_DATA_PTR=4, FIELD_CT=5, MY_ROOM_CT=6, ARRANGE_ROOM_CT=7, ARRANGE_FURNITURE_CT=8, SOUND=9, END=10.
- All 52 manifests extracted programmatically from src/data/scene/ (51 files). SCENE_RANDOM_NPC_TEST (8) and SCENE_FIELD_TOOL (32) share field_tool_field_info -- the ID->manifest mapping is not 1:1.
- FIELD_CT packs (bg_disp_size<<16 | room_type<<8 | draw_type) into the generic param3 slot (big-endian union layout); the PC port unpacks it explicitly (TARGET_PC in Scene_Proc_Field_ct). Verified against m_scene.h macro + m_scene.c.
- mSc_DATA_MY_ROOM_CT() is never used in any decompiled manifest; ARRANGE_ROOM_CT appears in 9 scenes (broker, fg_tool_in, 4x player_select, 3x start_demo) and triggers mScn_ObtainCarpetBank.
- Scene_Proc_Sound is a stub in the decomp -- sound params (0,0) vs (0,1) unresolved. The separate mPl_SceneNo2SoundRoomType switch gives: 1 = MY_ROOM_S; 2 = NPC_HOUSE, SHOP0, BROKER_SHOP, POST_OFFICE, BUGGY, MY_ROOM_M, KAMAKURA, MY_ROOM_LL2, TENT; 3 = MY_ROOM_L, CONVENI, SUPER, DEPART, DEPART_2, MY_ROOM_LL1, COTTAGE_MY, POLICE_BOX; 0 = everything else.
- Gameplay_Scene_Init sequence: zero player/actor/bank counts -> mSc_data_bank_ct (0xA000-byte 32-aligned exchange arena) -> global light, door info, common reset -> Scene_ct -> mSc_decide_exchange_bank.
- Manifest pointers are installed by reference (no copying); scene data lives in static storage.
- Notable manifest facts: museum_entrance has 4 doors; museum_insect uses bg_disp_size 0xB000; player rooms S/M/L/LL use ARRANGE_FTR 30/32/48; train scenes use FIELD_DRAW_TYPE_TRAIN; room type (MY_ROOM/NPC_ROOM/MISC_ROOM/OUTDOORS) and draw type (INDOORS/TRAIN/PLAYER_SELECT/OUTDOORS) are independent axes.
- Verification method: the full 52-entry manifest table and the sound-room-type table were both checked programmatically against source (script extraction), not hand-transcribed. Caught and fixed: lighthouse obj_banks, POLICE_BOX/CONVENI/SUPER/DEPART/DEPART_2/LIGHTHOUSE/TENT sound types.

### Rust rewrite implementation

`rust/src/scene_table.rs`: SCENE_NUM=52, SceneWordType enum, SceneWord, item/room/draw type constants, FieldCtParams + fieldct_unpack (TARGET_PC logic), SCENE_MANIFESTS (all 52 decoded), SOUND_ROOM_TYPE, SCENE_STATUS_SIZE/TOTAL, EXCHANGE_ARENA_SIZE. C ABI: pc_fieldct_unpack, pc_scene_manifest, pc_scene_sound_room_type. `cargo check --lib` clean. Unit tests were written but NOT run, per the standing instruction.

### Gaps

- Scene_Proc_Sound body (stubbed in decomp); scene_data_status 0x14x52 semantics; the referenced data arrays (SHOP01_ctrl_actor_data, object banks, door data, player data) are the next level down.

### Runtime Port Progress: FG Template Data + SIGN Distribution Groundwork

### Source findings (all verified against the local decomp)

- KEY FINDING: the per-template SIGN00-SIGN20 locations are NOT in the decomp source. They live in `fgdata.bin`, a binary asset inside `forest_1st.arc` on the game disc (loaded via `RESOURCE_FGDATA` in jsyswrap.cpp, mounted from the user's game files at runtime). The decomp carries only the structs + the combination table.
- Binary format (from `mFM_fg_data_c`, m_field_make.h): per record `fg_id: u16 BE` + `items[16][16]: u16 BE` (row-major z,x) + `haniwa_step: [u8;4]` = 518 bytes/record. `mActor_name_t` is u16; the PC port byte-swaps u16s after load (`mFM_ByteSwapFGData`), so the file is big-endian.
- SIGN range: STRUCTURE_START=0x5800, SIGN00=0x5810 (22544) .. SIGN20=0x5824 (22564) (m_name_table.h).
- `data_combi.c` statistics (derived by script, not hand-counted): 368 combinations, 267 distinct FG types. Per-type variant counts: FLAT 10, BEACH 10, RIVER_SOUTH/EAST/WEST 4, CLIFF_HORIZONTAL 5, border/ocean types mostly share FG_TYPE_EMPTY. FLAT's 10 FG variants (GRD_S_F_1_2F, GRD_S_F_2..GRD_S_F_10) are the prime house-lot candidates.
- OCEAN_5 enum value corrected to 97 during verification (hand-count was wrong; script-extracted from the header).
- To finish the distribution: extract fgdata.bin from forest_1st.arc (game disc), parse with the provided parser, join SIGN locations with the combination table.

### Rust rewrite implementation

`rust/src/fg_data.rs`: FG_RECORD_SIZE (518), SIGN_FIRST/LAST, FgRecord, parse_fg_records (big-endian), sign_locations, COMBI_COUNTS table, house_lot_candidates, build_fg_index. C ABI: pc_fg_parse_count, pc_fg_sign_count. `cargo check --lib` clean. Unit tests were written but NOT run, per the standing instruction.

### Gaps

- Actual SIGN locations per template require fgdata.bin from the game disc (not in the repo; copyrighted asset, can't be vendored). The parser + join logic is ready for Philip to run on his Windows machine where the disc files exist.

### Runtime Port Progress: Villager Selection + House Lots

### Source findings (all verified against the local decomp)

- Init order is decisive (m_start_data_init.c): `mFM_InitFgCombiSaveData` (field gen) runs at line 205, `mNpc_InitNpcAllInfo` at 225, then `mNpc_Grow` and `mNpc_InitNpcData`. The river/cliffs/pond/acre layout exist before any villager exists — villagers never carve terrain.
- `Anmhome_c` (m_npc.h): type_unused + block_x/block_z/ut_x/ut_z. Reservation markers SIGN00-SIGN20 (21 IDs), `mNT_IS_RESERVE` (m_name_table.h).
- `mNpc_MakeReservedListBeforeFieldct` scans all 7680 FG cells (5x6 acres x 16x16) for reservation markers. `mNpc_SetNpcHome` shuffles the lot-index table with 30 swaps (villager order NOT shuffled), assigns lots to homeless villagers one-to-one, with the verbatim `ut_z + 1` offset (marker is one unit south of the stored house coordinate).
- `mNpc_BuildHouseBeforeFieldct`: 3x3 footprint (`ut_d` table verbatim), cell pattern house/signboard/RSV_NO x7, requires one-unit acre-edge clearance, only touches the FG item layer.
- Initial villagers (`mNpc_DecideLivingNpcMax`): shuffle all NPC definitions, accept STARTER-permission candidates with uncovered looks categories — coverage-constrained, not pure random. Called with count = mNpc_LOOKS_NUM (6).
- Natural growth: field-rank table {40,50,60,70,80,90,100} with `RANDOM(100) < prob`; gates are population<15, >=1 day elapsed, player from this town, talked to all villagers. `mNpc_GetMinLooks` picks the least-populated looks category with eligible unseen NPCs (ties -> bitfield); candidates need not-present + not-appeared + STARTER/MOVE_IN, then uniform choice.

### Rust rewrite implementation

`rust/src/villager_home.rs`: HomeInfo, reservation constants, collect_reserved_lots (source scan order), make_rand_table + LOT_SHUFFLE_SWAPS, assign_homes (with ut_z+1), HOUSE_FOOTPRINT + cells, house_footprint_in_bounds, decide_living_npc_max, GROW_PROB + check_grow_field_rank + check_grow, min_looks_bitfield, grow_candidate_eligible. C ABI: pc_is_reserve_marker, pc_house_footprint_in_bounds, pc_check_grow_field_rank, pc_check_grow, pc_min_looks_bitfield. `cargo check --lib` clean. Unit tests were written but NOT run, per the standing instruction.

### Gaps

- The have-appeared table lifecycle, summer-camp selection, mFM_GetReseveName river-set logic, and house destruction/restoration not yet ported. FG template SIGN data (data_combi.c) still undecoded — the next layer per the brief. No C callers rewired.

### Runtime Port Progress: Field Generator Core (m_random_field_ovl)

### Source findings (all verified against the local decomp)

- `mRF_GetRandomStepMode` = `mRF_GetRandom(100) < 15`: 15% three-level, 85% two-level. `mRF_MakePerfectBit` sets all 9 feature bits (SLOPE_LEFT/RIGHT, BRIDGE_UPPER/LOWER, SHRINE, POLICE, MUSEUM, POOL, NEEDLEWORK) = 0x1FF; generation repeats until `perfect_bit == (perfect_bit & bit)` — rejection sampling, not best-effort.
- Cliff tracer tables verbatim: 7 shape classes with `l_cliff_next_direct` = {EAST,NORTH,NORTH,EAST,SOUTH,SOUTH,EAST}; successor tables (horizontal -> {horizontal, bottom-right, top-left}; vertical-right -> {vertical-right, top-right}; vertical-left -> {vertical-left, bottom-left}); start tables A/B/C with row->table mapping {0,1}->A, 2->B, 3->C.
- River tracer tables verbatim: start X in {1,2,4,5}; successor shapes per river type; `l_river_next_direct` = {SOUTH,EAST,WEST,EAST,SOUTH,WEST,SOUTH}.
- `l_base_blocks` 7x10 outer frame ported verbatim (railroad row, player house at (3,2), sea/ocean/island rows).
- `mRF_GetSystemBlockInfo` cliff-shape bit mapping ported verbatim; `mRF_GetBlockBase` ported verbatim: per-column scan z=9..0 from STEP1, height++ after HORIZONTAL/TOP_RIGHT/TOP_LEFT shapes or border cliff transitions.
- Conversions verbatim: slope = SLOPE_HORIZONTAL + (cliff - CLIFF_HORIZONTAL); pool = POOL_SOUTH + (river - RIVER_SOUTH); bridge = RIVER_SOUTH_BRIDGE + (river - RIVER_SOUTH).
- `mRF_CheckFieldStep3` = top-left acre height == 3. Ten fixed step-3 templates exist (selection is uniform, not traced).
- Source bug documented: `mRF_BgName2RandomConbiNo` has `@BUG - this always selects the first entry instead of a random one` (`mRF_GetRandom(0)` in bug-compatible builds; `mRF_GetRandom(count)` under BUGFIXES).

### Rust rewrite implementation

`rust/src/field_gen.rs`: block-type constants (exact C values), step_mode, feat bits + perfect_bit + generation_accepted, CLIFF_NEXT_DIRECT, CLIFF_NEXT_SHAPES, cliff_start_table, RIVER_START_X, RIVER_NEXT_SHAPES/DIRECT, BASE_BLOCKS, block_cliff_shape_bits, acre_height_table, slope/pool/bridge conversions, is_field_step3, buggy_template_selection. C ABI: pc_field_step_mode, pc_field_generation_accepted, pc_cliff_next_direct, pc_river_next_direct, pc_acre_height_table, pc_slope_for_cliff, pc_pool_for_river, pc_bridge_for_river. `cargo check --lib` clean. Unit tests were written but NOT run, per the standing instruction.

### Gaps

- The full cliff/river trace loops, river-cliff albumin combination tables, beach/dock/bridge/slope/building/pond placement passes, template selection + anti-reuse, and the 10 step-3 template bodies are not yet ported. No C callers rewired. The existing town_gen.rs keeps its own higher-level model; these are the source-verbatim tables it was missing.

### Runtime Port Progress: Player Tool Families (Axe, Net, Rod)

### Source findings (all verified against the local decomp)

- Axe family. SWING_AXE: frame 10 whoosh sound; frame 15 = the hit — effect at offset (-7,20,24) rotated by player angle, tree resolution via tree_cutcount_check_proc (cutcount<=0 -> stump via bg_item_fg_sub, else shake; fruit drops unless bee tree; bee tree sets bee_counter=5.0), AXE_CUT sound, axe-damage bookkeeping; frame 16.5 -> bee attack status; frame >=17 -> shock if bee disturbed else WALK (priority 1). REFLECT_AXE settles at 30.5/31, AIR_AXE at 35.5/36 (same tail shape). Semantic actions CHOP_TREE / CHOP_PALM_TREE reported via mISL. BROKEN_AXE has separate reflect/swing request variants.
- Net family. READY_NET / READY_WALK_NET -> SWING_NET at priority 22. Catch test is a capsule from net_top_col to net_bot_col, length 50 (normal) / 60 (gold net); insects self-register into catch request tables. SWING_NET outcome: check_type 2 -> PULL_NET (priority 26, hit sound + vibration); check_type 0 -> STOP_NET (priority 26, NPC UZAI marking). PULL_NET runs the catch demo (base msg 0xA2C, insect-specific otherwise).
- Rod family. CAST_ROD: stroke sound at frame 20. RELAX_ROD case 5 -> VIB_ROD (priority 26, the bite); case 6 -> COLLECT_ROD (priority 26). VIB_ROD with nonzero item status -> FLY_ROD (priority 27, the hook).

### Rust rewrite implementation

`rust/src/player_tools.rs`: swing_axe_frame_event, AXE_HIT_OFFSET, tree_hit_outcome, AXE_BEE_COUNTER, reflect/air settle frames, net_catch_length, net_swing_outcome, tool_priority constants, PULL_NET_MSG_BASE, relax_rod_case, vib_rod_hook, cast_rod_frame_event. C ABI: pc_swing_axe_frame_event, pc_tree_hit_outcome, pc_net_catch_length, pc_net_swing_outcome, pc_relax_rod_case, pc_vib_rod_hook. `cargo check --lib` clean. Unit tests were written but NOT run, per the standing instruction.

### Gaps

- Axe-damage accumulation -> BROKEN_AXE transition thresholds, REFLECT_AXE's own frame-15 resolution details, insect catch-table registration, COLLECT_ROD/FLY_ROD bodies, NOTICE variants, and the remaining main-index states not yet ported. No C callers rewired.

### Runtime Port Progress: Player Action State Machines (Scoop, Wade, Pitfall)

### Source findings (all verified against the local decomp)

- The master `mPlayer_INDEX_*` enum (include/m_player.h) has 121 entries (0-120). No SWIM and no general CLIMB exist; water traversal is WADE/WADE_SNOWBALL, climbing is CLIMBUP_PITFALL only. The full enum is ported in exact C order so indices line up.
- Request/priority arbitration (m_player_common.c_inc): systems call Player_actor_request_main_index; the request wins only when `priority - requested_main_index_priority > 0` (plus two cancel/reset gates that need game state). 45 priority levels (PRIORITY_0..44).
- DIG_SCOOP frame events (verbatim): 14/15/16 -> DIG_HOLE args 0/1/2, 22 -> DIG_SCOOP (arg 0, or 3 for GET_SCOOP); tree-stump variant (DIG_KABU1) single event at frame 42. World hole registration at mod+20 (decal circle radius 19, arg 12).
- GET_SCOOP: inventory transaction happens in setup (before the animation); item scale timeline <=21 -> 0, 21-27 -> 0.0016666666*(frame-21), >=27 -> 0.01; continuation protocol 0x3F -> PUTAWAY_SCOOP, 0x40 -> PUTIN_SCOOP (both priority 21).
- FILL_SCOOP (verbatim): hole removal at 18+mod; impacts at 13/19/25 (+mod) args 3/4/5; final DIG_SCOOP effect at 40+mod.
- `Player_actor_Check_DigScoop`: TRUE for DIG/REFLECT/GET/FILL/PUTIN_SCOOP — AIR and PUTAWAY are NOT members (matches the brief).
- WADE: end pos 18.00001f along dir, 36-frame accel/brake curve (1.1999999/34.8), camera request (arg 9, 36.0f), then requests WALK to the end pos. WADE_SNOWBALL is the acre-boundary snowball variant.
- CLIMBUP_PITFALL: setup calls pit_exit_proc (pit removed before the animation); umbrella held -> DERU2 else DERU1 (verbatim selector); movement is animation-driven (cKF_SkeletonInfo_R_AnimationMove_base).

### Rust rewrite implementation

`rust/src/player_action.rs`: full `index` enum (121 entries, C order), `check_request_main_priority`/`request_admissible`, dig/fill/get frame-event functions, `check_dig_scoop`, wade constants + `wade_finished`, `climbup_pitfall_anim`. C ABI: pc_player_request_priority_delta, pc_dig_scoop_frame_event, pc_fill_scoop_frame_event, pc_get_scoop_continuation, pc_check_dig_scoop, pc_wade_finished, pc_climbup_pitfall_anim. `cargo check --lib` clean. Unit tests were written but NOT run, per the standing instruction.

### Gaps

- REFLECT_SCOOP frame-13 object resolution, PUTIN_SCOOP burial/golden-shovel demo, the scoop-target decision (mPlib_Check_scoop_after), and all non-scoop action states (axe/net/rod families etc.) not yet ported. No C callers rewired.

### Runtime Port Progress: NPC AI (Schedules, Reactions, Talk Throttle)

### Source findings (all verified against the local decomp)

- Six hard-coded daily schedules in m_npc_schedule.c, indexed by looks (girl/ko_girl/boy/sport_man/grim_man/naniwa_lady). All six tables ported verbatim (girl 05:00 sleep ... 18:30 field 21:00 house; sport-man crosses midnight; grim-man/naniwa start field at 04:00/01:30).
- `mNPS_schedule_manager_sub`: walks the table until `end_time > now_sec`, sets saved_type; forced_timer>0 -> current_type=forced_type with the timer decremented by forced_ticks, else current=Saved. Global override: First Job or Halloween active forces ALL town animals to FIELD (manager_sub0).
- Schedule change gate: NPC's step must match, desired type != current_type, and talk_condition == NONE — the scheduler never yanks an NPC out of conversation.
- FIELD steps: LEAVE_HOUSE/WANDER/IN_BLOCK/PITFALL; appear_flag=1 skips to IN_BLOCK. Sleep think runs with only ENTRANCE|OBSTACLE|FATIGUE interrupts and mNpc_FEEL_SLEEPY; an interrupted sleeper forces itself to FIELD (or IN_HOUSE in its home block) for 7200 frames (~2 min).
- Interrupt order (verified in aNPC_think_chk_interrupt_proc): talk start, pitfall, hands, uzai->REACT_TOOL, entrance, then the moving chain (clap, fatigue, collision turn, obstacle, ball, insect/fish), friendship LAST. Umbrella control runs unconditionally before the chain (not a checked priority member).
- Wander decisions (aNPC_think_wander_decide_next): verbatim border tables per looks ({3,6} girl, {6,8} ko-girl, {5,7} boy, {2,4} sport-man, {3,6} grim-man, {4,8} naniwa/special); fatigue OR SLEEPY feel -> WAIT; WALK_WANDER think uses rng>5; RUN downgraded to WALK when forced into FIELD (current==FIELD != saved).
- Uzai (annoyance): max_uzai_cross={600,240}, max_uzai_tool={3,1}, indexed by cross==1; triggers REACT_TOOL -> can escalate to force_call_req_proc complaint.
- Talk throttle (m_npc.c): temperament table {unlock_timer, over_impatient_num, talk_num_max} is keyed by LOOKS, not mood (callers pass animal->id.looks; the header's `feel` param name and the FEEL comments are misleading). NORMAL looks: unlock 4000, impatient 12, max 15. TalkEndMove sets a 1000-frame timer and counts talk_num; >=impatient -> unlock timers; >=max -> refuses (ANNOYED).
- NPC-NPC greeting: relation +8 both ways and both set HAPPY (ac_npc_act_greeting.c_inc) — the mood feedback loop is source-proven. (Bonus: same-sex NPCs copy catchphrases on greeting.)

### Brief corrections made during verification

1. The temperament table's "Mood" column is wrong — it is per-looks (personality). The over-impatient/max-talks columns were also swapped: the struct order is {unlock_timer, over_impatient_num, talk_num_max}.
2. Umbrella control is not priority #2 in the interrupt chain; it runs unconditionally before the checked chain.
3. The WALK_WANDER think mode has its own rng>5 branch, not the border tables.

### Rust rewrite implementation

`rust/src/npc_ai.rs`: SCHEDULE_TABLES (verbatim), sched enum, schedule_manager_sub, global_schedule_override, check_chg_schedule, field_step enum, sleep_force_schedule_type, Interrupt order, WANDER_BORDERS + wander_decide_next, uzai_trigger, TALK_TEMPER + TalkThrottle (talk_end/patience), GREETING_RELATION_BUMP. C ABI: pc_schedule_step, pc_schedule_global_override, pc_wander_decide_next, pc_uzai_trigger, pc_talk_patience. `cargo check --lib` clean. Unit tests were written but NOT run, per the standing instruction.

### Gaps

- Individual check bodies not yet ported (check_ball/check_insect/clap/entrance geometry, uzai step accumulation, mood-indexed animation tables, schedule-field/go-home/sleep think bodies). No C callers rewired.

### Runtime Port Progress: Force-Call State Machine

### Source findings (all verified against the local decomp)

- `aNPC_force_call_req_proc` (ac_npc_talk.c_inc:1): a generic NPC-clip callback — three hard gates (`force_call_flag == NONE`, `talk_condition == NONE`, `mDemo_CAN_ACTOR_TALK` = not in a SPEAK/TALK demo). On success: flag = REQUEST, `force_call_msg_no` = caller-supplied message. The caller owns message selection (e.g. m_quest.c's soccer contest passes `0x0D8B + looks`).
- Force-talk path (`aNPC_force_talk_request`, ac_npc_talk.c_inc:597): stored `force_call_msg_no != -1` → SPEAK demo installing it; else the friendship path needs ALL of: friendship pointer known, effective friendship > 0x80 (128), action = SEARCH, act_obj = PLAYER, timer <= 0, XZ < 80, |Y| < 60.
- Friendship chain: `aNPC_chk_avoid_and_search` (ac_npc_move.c_inc:505) requires player/NPC in the SAME block, then friendship < 0 → AVOID, > 128 → SEARCH. `aNPC_love_player` (ac_npc_think.c_inc:559) raises REQUEST only when player sex != NPC looks-sex, flag == NONE, timer <= 0 (msg_no left -1); approach pace RUN > 3 units, WALK > 1.5 units (unit = 40).
- Automatic greeting (`aNPC_set_talk_info_talk_request_check`): mainland `0x075F + looks*3 + RANDOM(3)`, island `0x34AC + looks*3 + RANDOM(3)` — a separate message family from the quest-manager taxonomy.
- Lifecycle: install callback sets msg+camera then clears them and moves flag to SET; on SET with SPEAK demo active → `setup_talk_start`, flag = START; talk end → `force_call_timer = 300` (≈5 s cooldown), flag = NONE. REQUEST with failed force-talk falls back to normal talk.
- So the villager conversation universe has (at least) three lanes: player-initiated quest-manager talk (topic taxonomy), NPC-initiated force calls (this module), event/special-NPC talk.

### Rust rewrite implementation

`rust/src/force_call.rs`: `force_call`/`friendship` enums, `demo_can_actor_talk`, `force_call_req_proc`, `chk_avoid_and_search`, `love_player_request_gate`, `love_player_pace`, `force_talk_request` (+ `ForceTalkPath`), `auto_greeting_msg_no`, `set_talk_info_force_call`, `talk_end_force_call_reset`. C ABI: `pc_force_call_req_proc`, `pc_force_talk_request`, `pc_auto_greeting_msg_no`. `cargo check --lib` clean. Unit tests were written but NOT run, per the standing instruction.

### Gaps

- Whether SCENE_NPC_HOUSE adds any house-specific trigger beyond this generic friendship path is still unproven (the brief's suggested next step); no C callers rewired.

### Runtime Port Progress: Talk Topic Taxonomy

### Source findings (all verified against the local decomp)

- `aQMgr_talk_normal_select_talk` (ac_quest_talk_normal_init.c:2145): top-level dispatch — first-job hint has absolute priority (`0x0841 + hint_type + looks*10`, hint_type = hint_count & 0x7F); else HAPPY mood → KI tree, other moods → normal/game tree. Ends with `aQMgr_change_NG_msg` + `mMsg_SET_CONTINUE_MSG_NUM` + `mMsg_SET_FORCENEXT`.
- Probability tables verbatim: KI {40,30,10,10,10}, normal-1 {70,30}, normal-2 {15,35,35,15}, trade {25,25,25,25}, normal-3 {49,17,17,17}, game {40,60}. The chooser (`aQMgr_decide_idx_prob_table`) builds a 100-entry table from the weights, shuffles 30x, picks one — weights are exact.
- All 19 message base tables verified verbatim against the source (KI x5, letter, memory, trade x4, normal-3 x4, game hint, remove_yes, ev_special, ev_cal). Every leaf is `table[looks] + variant`; looks 0-5 = personality.
- Leaf formulas verified: weather/time `base + time_kind*6 + weather*2 + RANDOM(2)` (SAKURA folded to CLEAR); normal-3 weather `base + weather*5 + RANDOM(5|4) + ofs` with the player-man-kind adjustment (msg_cnt=4, ofs=1); season `base + add_table[month-1]` (add_table = {10,11,0..9} verbatim); letter `base + mail_selection_type` (strategy id, not a random range); memory `base + idx*2 + (letter? 0:1)` with the +8 no-memory/high-friendship fallback.
- Verified fallbacks: KI leaf -1 → KI normal; normal-2 leaf -1 → normal-3. KI free-item+money gate: empty pocket AND wallet >= 3000.
- Source @BUGs noted: `ret_msg` uninitialized in `aQMgr_decide_normal_2_msg_no`, `aQMgr_decide_msg_trade`, `aQMgr_decide_msg_normal_3_msg_no`, `aQMgr_decide_normal_msg_no` — the port initializes to -1 (matches observed fallback behavior, avoids UB).

### Rust rewrite implementation

`rust/src/talk_topics.rs`: verbatim prob/base tables, category enums, `decide_idx_prob_table` (RNG via caller closure), `select_talk_kind`, `fj_hint_msg_no`, `weather_time_msg_no`, `normal3_weather_msg_no`, `normal3_season_msg_no`, `letter_msg_no`, `memory_msg_no`, `memory_fallback_msg_no`, `ki_fallback`, `ki_free_item_money_ok`. C ABI: `pc_select_talk_kind`, `pc_fj_hint_msg_no`, `pc_weather_time_msg_no`. `cargo check --lib` clean. Unit tests were written but NOT run, per the standing instruction.

### Gaps

- Deeper fallback edges (memory→trade, trade→normal-3, event→special→game-hint), the full game/event subtree (removal/special-event/calendar-rumor validity), island taxonomy (`ac_quest_talk_island.c`), greeting layer (`ac_quest_talk_greeting.c`), and the message-database semantics behind each ID range are not yet ported; no C callers rewired.

### Runtime Port Progress: Talk-Request Driver

### Source findings (all verified against the local decomp)

- CORRECTION to the brief: the symbol family EXISTS. `talk_request_proc` is not one function but a per-NPC strategy hook: `typedef void (*aNPC_TALK_REQUEST_PROC)(ACTOR*, GAME*)` (ac_npc.h:254), defaulted from `aNPC_ct_data_c.talk_request_proc` (ac_npc_ct.c_inc:315) and swappable at runtime.
- The driver is `aNPC_talk_request_event_npc` (ac_npc_talk.c_inc): a SPEAK/SPEECH/TALK demo active and NOT listenable -> `aNPC_setup_talk_start` directly; else if submenu idle (WAIT, timer 0) -> call the NPC's hook, or `mDemo_Request(mDemo_TYPE_TALK, actorx, NULL)` when no hook is installed; otherwise nothing.
- Concrete hook behaviors: `aCD0_norm_talk_request` -> `mDemo_Request(TYPE_TALK, ..., set_norm_talk_info)` with message `msg_base[looks] + RANDOM(3)` (+17 for NEW_YEAR/AFTER_10_SEC terms, else +term*4); `aCD0_force_talk_request` -> `mDemo_Request(TYPE_SPEAK, ...)`; quest-manager clip (`aNPC_normal_talk_request`) -> clip's bool proc gates talk start; `none_proc1` installed to mean "no request".
- Session lifecycle: `aNPC_setup_talk_start` (palActor = player, face player iff turn == NORMAL, talk_condition = START, save demo flags); `aNPC_setup_talk_end` (palActor = NULL, ignore timer = 600 when >= 0, talk_condition = NONE, force-call cleared, feel = 0xFF, demo flags restored).
- The brief's "driver, not dialogue database" framing was correct; the actual message flow is owned by the demo system (`mDemo_Request`), which this layer only triggers.

### Rust rewrite implementation

`rust/src/talk_request.rs`: `TalkRequestAction::{Wait, SetupTalkStart, InvokeProc, RequestTalkDemo}`, `TalkRequestInputs`, `talk_request_dispatch` (verbatim dispatch), `normal_talk_request_gate`, `countdown_norm_msg_no`, `TalkSession::{begin, end}`. C ABI: `pc_talk_request_dispatch`, `pc_normal_talk_request_gate`. `cargo check --lib` clean. Unit tests were written but NOT run, per the standing instruction.

### Gaps

- The demo system (`mDemo_Request` internals, message flow, choice handling) is not yet researched/ported; no C callers rewired.

### Runtime Port Progress: Cliff/Slate Classification

### Source findings (all verified against the local decomp)

- CORRECTION to the brief: `mCoBG_CheckCliffAttr` and `mCoBG_Wpos2CheckSlateCol` DO exist verbatim in the current source — in `m_collision_bg_info.c_inc` (USA: lines 1040/1088; AUS tree identical). The brief searched the wrong files.
- `mCoBG_CheckCliffAttr(attr)` (verbatim): TRUE iff attr in 47-54 (grass4 cliff/tunnel) or 55-58 (grass3 cliff). Purely semantic; used by the talk camera (`Camera2_TalkCheckCliffLRRange`, m_camera2.c:1124).
- `mCoBG_Wpos2CheckSlateCol(pos, check_attr)` (verbatim): TRUE if `slate_flag`; else if check_attr, TRUE iff attr in {27,28,29,30, 37,38, 39,40,41,42, 55,56,57,58} — wood-bridge pieces (NOT the 31 center), wave_se/sw (NOT wave_s 36), river banks, grass3 cliff. Used by the snowman actor with check_attr=FALSE (ac_snowman.c:679), i.e. as a pure slate_flag test blocking snowman rolling on slate units.
- `mCoBG_WoodSoundEffect` (adjacent, same file): TRUE for WOOD (23) and 27-31 INCLUDING the 31 center — ported as a bonus.
- `mCoBG_GetAreaPolygon` slate branch (line.c_inc:119): builds the area triangle then equalizes vertex Y values toward the lower side per area. SOURCE BUG preserved (USA Rev. 0, unfixed): in the AREA_W case with leftUp > leftDown, the source assigns `v0->y = v1->y` twice (comment: "this should be v2->y = v1->y"); the BUGFIX build differs. Net unfixed effect: v0 = v1 = leftDown, v2 stays center.
- `mCoBG_GetBgNorm_FromWpos` (info.c_inc:81): slate_flag == 1 reports a straight-up normal (0, 100, 0); flat non-slate terrain reports the same; only uneven non-slate terrain gets a real triangle normal.

### Rust rewrite implementation

`rust/src/slate_classify.rs`: `check_cliff_attr`, `wpos2check_slate_col` (+ `SLATE_COL_ATTRS` table), `wood_sound_effect`, `slate_ground_normal`, `slate_area_polygon_y` (verbatim equalization incl. the AREA_W @BUG). C ABI: `pc_check_cliff_attr`, `pc_wpos2check_slate_col`, `pc_wood_sound_effect`, `pc_slate_area_polygon_y`. `cargo check --lib` clean. Unit tests were written but NOT run, per the standing instruction.

### Gaps

- Full `GetAreaPolygon` non-slate branch and `GetNormTriangle` not yet ported; no C callers rewired.

### Runtime Port Progress: SearchAttribute + Slate Ground Height

### Source findings (all verified against the local decomp)

- `mCoBG_SearchAttribute` (m_collision_bg.c:495): three lines — `wpos.y = 0`, `PlussDirectOffset(next, wpos, direct)`, `return Wpos2Attribute(next, cant_dig)`. No tables, no slope math; the "search" is one cardinal neighbor step. Already modeled as `redirect: Some(area)` in `wpos2attribute_step`; now also ported as an explicit kernel.
- `mCoBG_PlussDirectOffset` (m_collision_bg.c:114): adds `mCoBG_unit_offset[direct]`; guard `direct in 0..8` else no write. Offset table verbatim: N=(0,-40), W=(-40,0), S=(0,+40), E=(+40,0), then the diagonals. SOURCE QUIRK: with north = -z, the index-5 ("NE") entry points to (-40,+40) and index-7 ("SW") to (+40,-40) — transposed vs geometric intuition. Preserved verbatim (the earlier `direct_offset` port in terrain_walls.rs already had it right). The attr-63 path only uses indices 0-3 (cardinal), so the quirk never affects slope resolution.
- `mCoBG_GetBGHeight_Normal_SlateGround` (m_collision_bg.c:~1347): orientation from the SINGLE comparison `top_left != bot_right` -> SLATE_UP else SLATE_DOWN (different from the wall-building slate-detail search); `GetAreaYSlatingUnit` area->corner mapping with the SLATE_UP-invalid-area fallthrough into the DOWN switch; `corner * 10 + base_height`; zeroes the caller's angle. Dispatch: `get_bg_y_normal_proc[slate_flag]` selects slate vs normal ground (m_collision_bg.c:1375).
- Confirmed the brief's key separations: `slate_flag` (physical collision shape) vs `unit_attribute == 63` (semantic topology proxy) are independent bitfields; GroundCheck's bridge-water branch requires attr 27-35, so raw 63 never enters it; the redirect preserves local position (no center snapping), so a 63->bridge->river/wood composition resolves at the same relative point.

### Rust rewrite implementation

Added to `rust/src/wpos2attribute.rs`: `UNIT_OFFSETS` (verbatim, quirk documented), `pluss_direct_offset`, `search_attribute_redirect` (cardinal-only, `None` for area > 3), `slate_ground_orientation`, `area_y_slating_unit` (with the UP fallthrough), `slate_ground_height`. C ABI: `pc_pluss_direct_offset`, `pc_search_attribute_redirect`, `pc_slate_ground_height`. `cargo check --lib` clean. Unit tests were written but NOT run, per the standing instruction.

### Gaps

- Full `GroundCheck` orchestration (water_y selection, `AdjustActorY`, `MakeJumpFlag`) not yet ported; no C callers rewired.

### Runtime Port Progress: Wpos2Attribute Effective-Terrain Interpreter

### Source findings (all verified against the local decomp)

- `mCoBG_Wpos2Attribute` (m_collision_bg.c:1517): effective gameplay-terrain interpreter over the raw 6-bit `unit_attribute`. Outputs the effective attribute (stored into the actor's collision result by GroundCheck) plus the INDEPENDENT `cant_dig` side-channel.
- Branch order (verbatim): HOLE -> FLOOR/GRASS2 (cant_dig stays FALSE); 63 -> `mCoBG_SearchAttribute` (area-selected cardinal neighbor, recursive, cant_dig inherited via shared pointer); 25-26 -> dynamic wave, cant_dig FALSE; 27-62 -> cant_dig TRUE then 43-62 -> FLOOR/GRASS2, 27-31 -> `woodb_water_info[attr-27][area]`, 32-35 -> STONE, 36-38 -> dynamic wave, 39-42 -> `grass3_water_info[attr-39][area]` with the `(mapped <= GRASS3 && non-FG) ? FLOOR : mapped` gate; default `(attr <= GRASS3 && non-FG) ? FLOOR : attr`.
- Wave classifier (`CheckWaveAtrDetail`, m_collision_bg.c:1415): projects the unit-local point onto the template segment, `pos_rate = 1.1 * dist_point/dist`; <= 0 -> SEA, >= 1.1 -> SAND, <= wave-phase rate -> WAVE, else SAND; degenerate segment -> SEA. `F32_IS_ZERO` = `fabsf(v) < 0.008` (types.h:146), same as the column sweep. Wave phase is a global (`mCoBG_wave_cos`, set by `WaveCos2BgCheck`).
- Wave templates verbatim: 36: (0,+20)->(0,-20); 37: (0,0)->(-20,-20); 38: (0,0)->(+20,+20); 25: (+20,+20)->(0,0); 26: (-20,+20)->(0,0).
- `woodb_water_info` has 6 rows but only rows 0-4 (attrs 27-31) are ever indexed; row 5 is dead in the source, preserved verbatim.
- Confirmed the brief's key architectural point: cant_dig is a raw-attribute-class property (27-62), never derived from the returned effective attribute; a tile can report GRASS2 while being undiggable.

### Rust rewrite implementation

`rust/src/wpos2attribute.rs`: `attr`/`area` constants, verbatim `WOODB_WATER_INFO`/`GRASS3_WATER_INFO` tables, `f32_is_zero`, `cross_line_and_perpendicular`, `check_wave_atr_detail`, `get_wave_dynamic_attr`, `wpos2attribute_step` (verbatim branch order). The attr-63 redirect is expressed as `redirect: Some(area)` in `Wpos2AttributeOut` — the caller (which owns the map) resolves the neighbor and re-runs, threading `cant_dig` like the source's shared pointer. C ABI: `pc_wpos2attribute_step` (low byte = effective attr or 0xFF redirect marker, bit 8 = cant_dig, bits 8-15 = redirect area, bit 16 = redirect flag). `cargo check --lib` clean. Unit tests were written but NOT run, per the standing instruction.

### Gaps

- Full `GroundCheck` integration (water_y selection, `AdjustActorY`) not yet ported; `mCoBG_CheckWaterAttribute`/`_OutOfSea` trivial classifiers noted but not yet ported; no C callers rewired.

### Runtime Port Progress: Water-Translation Deep Dive

### Source findings (verified against the local decomp, USA Rev. 0)

- `mCoBG_unit_attribute_water_info[64]` (m_collision_bg.c:1609) already ported verbatim in commit ab41f9a; re-verified entry-by-entry against the source this round — the port matches exactly, including the 55-58 = GRASS0 region (the brief's caution about miscounting high-number entries was warranted).
- GroundCheck water-branch structure (m_collision_bg.c:1728-1768):
  - Bridge path (attr 63 or 27-62): search runs only when `!attribute_wall && old_in_water && attr in 27-35`. First water neighbor (in mask direction order) wins: `result.unit_attribute = next_unit_attr`, `water_flag = TRUE`, `water_y = GetWaterHeight_File(...)`.
  - Correction to the earlier port: when the search RUNS but finds no water, the source does NOT fall back to `Wpos2Attribute` — `result.unit_attribute` is left unassigned (stale). The `Wpos2Attribute` fallback happens only when the search is SKIPPED (gated). `bridge_water_search` now returns a 3-state enum `BridgeWaterSearch::{Skipped, NoWater, Found(u8)}`; C ABI `pc_bridge_water_search` returns 0xFF / 0xFE / the attribute.
  - Non-bridge path: `attr in WATER..RIVER_NE` -> water_flag, `water_y = 20 + GetBgY_AngleS_FromWpos(...)`; `attr == SEA` -> water_flag, `water_y = 20.0`; else `result.unit_attribute = Wpos2Attribute(...)`.
- The brief's "dead SEA/37/38 branch" is a GAFU01 (Australian) version delta, not our target: USA GAFE01_00 has NO such branch in the bridge-search loop — the loop tests only `WATER..RIVER_NE`. No dead code to preserve.
- Architecture (source-proven, three independent layers on the raw 6-bit attribute): `l_attribute_action_info[64]` (NPC/place/plant permissions) vs `mCoBG_Wpos2Attribute()` (contextual terrain interpretation, with hole/slope/wave/bridge/bank rewriting + cant_dig) vs `mCoBG_unit_attribute_water_info[64]` (bridge water-connectivity: what a NEIGHBORING unit means for the old_in_water continuity test).
- `mCoBG_SearchWaterAttributeFrom4Area` (m_collision_bg.c:1680): pure raw-attribute lookup — no field-type check, no wave dynamics, no area geometry; the "4Area" refers to the caller's 8-direction neighbor search, not an internal triangle lookup.

### Gaps

- Water height (`GetWaterHeight_File`) and `AdjustActorY` water branch not yet ported; `woodb_water_info[][4]` (Wpos2Attribute's bridge-area table) not yet ported; no C callers rewired. Unit tests updated but NOT run, per the standing instruction.

### Runtime Port Progress: Attribute Action-Policy Table

### Source findings (all verified against the local decomp)

- `l_attribute_action_info[64]` (bg_info.c_inc:188, generated verbatim): raw 6-bit `unit_attribute` -> action-policy byte — bits 0-2 plant policy, bit 3 placement (`ATR_PLACE`), bit 4 NPC (`ATR_NPC`), bits 5-7 unused.
- Consumers: `mCoBG_CheckPlace_OrgAttr` (bit 3, NO 0x3F mask — indexes directly); `mCoBG_Attr2CheckPlaceNpc` (bit 4, WITH `attr & 0x3F` mask); `mCoBG_Attribute2CheckPlant` (FG-field gate; attr 63 redirects to the +Z neighbor's raw attribute and recurses; KILL_PLANT -> -1, else the stage); `mCoBG_Attr2CheckPoorGround` (@unused/@fabricated: poor iff plant in {KILL, PLANT0}).
- Only PLANT0/PLANT2/PLANT4/KILL_PLANT are ever emitted (PLANT1/PLANT3 exist in the enum only).
- Correction to the brief: the river-bank asymmetry is attribute **62** (grass 3 NE river bank: NPC + NO_PLACE + KILL = 0x17), not 61 — caught by generating the table from source.
- `mCoBG_Change2PoorAttr` (rewrite.c_inc:159): GRASS0/1->GRASS2, SOIL0/1->SOIL2 — moves terrain into the PLANT0 policy class.
- Architecture: the action table is keyed by the RAW collision attribute, independent of `mCoBG_Wpos2Attribute()` (contextual translator); it never drives collision geometry or column construction.

### Rust rewrite implementation

`rust/src/attribute_action.rs`: `ATTRIBUTE_ACTION_INFO` (verbatim, generated from source), `plant`/`bit` constants, `attribute_action_info` (with 0x3F mask), `check_place_org_attr`, `attr2check_place_npc`, `attr2check_poor_ground`, `attribute2check_plant` (field gate + attr-63 redirect left to the caller, which needs field/collision access), `change2poor_attr`. C ABI: `pc_check_place_attr`, `pc_check_npc_attr`, `pc_check_plant_attr` (0xFF = -1). `cargo check --lib` clean. Unit tests were written but NOT run, per the standing instruction.

### Runtime Port Progress: Directed-Unit Suppression + KeepH Height Family

### Source findings (all verified against the local decomp)

- **Directed-unit suppression lifecycle** (`mCoBG_BgCheckControll_RemoveDirectedUnitColumn`, m_collision_bg.c:1899): the caller passes (ux,uz); `mCoBG_MakeActorInf` stores them in `l_ActorInf._68/_6C`; BOTH the wall-column builder (`MakeColumnCollisionData`, line 1242) and the ground query (`GetBGHeight_NormalColumn`, line 1695) honor the exclusion; at the end the fields reset to (-1,-1). The ordinary `mCoBG_BgCheckControll` passes (-1,-1): no suppression. Terminology correction (per the brief): it is NOT "the center unit is always ignored" — it is an arbitrary directed unit per background check, which only becomes center-unit suppression when the caller passes the actor's center unit.
- **Column base uses KeepH, not the collision center**: `mCoBG_GetBgY_OnlyCenter_FromWpos2` (bg_info.c_inc:73) = `UtKeepH*10 + BaseHeight - ground_dist`. So `column.pos.y` (and hence `column.height`) is anchored to the KeepH center height. `mCoBG_GetLayer` uses the same KeepH height (thresholds 100/220, 3-step towns).
- **AddColumn helper** (`mCoBG_Wpos2BgUtCenterHeight_AddColumn`, bg_info.c_inc:59): column top when the unit builds one, else `collision->data.center*10 + BaseHeight`.
- **Height-gap detector**: `mCoBG_GetBgHeightGapBetweenNowDefault` = AddColumn − FromWpos2; `mCoBG_ExistHeightGap_KeepAndNow` = `(int)gap != 0` (truncation — ±0.5 gaps read as no gap).
- **Generic three-way query** (`mCoBG_GetBgY_AngleS_FromWpos`, bg_info.c_inc:21): max(normal, column, move-BG) minus ground_dist, but normal wins ties (`>=`) — DIFFERENT from the actor path's strict `>`; no directed-unit suppression here. Callers of the directed API remain untraced (open item).

### Rust rewrite implementation

`rust/src/column_sweep.rs` additions: `DirectedSuppression` (store/honor/reset lifecycle), `bg_y_only_center_from_wpos2`, `wpos2bg_ut_center_height_add_column`, `bg_height_gap_between_now_default`, `exist_height_gap_keep_and_now`, `layer` constants + `height2get_layer`, `bg_y_angles_from_wpos_select`. `cargo check --lib` clean. Unit tests were written but NOT run, per the standing instruction.

### Runtime Port Progress: Column-Derived Ground Height

### Source findings (all verified against the local decomp)

- `mCoBG_GetBGHeight_Column` (column.c_inc:444): builds ONE column from the unit's foreground object (`MakeOneColumnCollisionData` with `old_on_ground=FALSE`, so holes are rejected here), tests the query XZ against the footprint (`mCoBG_JudgePointInCircle_Xyz`: XZ only, `dx²+dz² <= r²` — exact body recovered, resolving the brief's open item), returns `col.height` or the 0.0 "no column" sentinel. The query Y is irrelevant.
- `mCoBG_GetBGHeight_NormalColumn` (m_collision_bg.c:1689): ground = max(normal terrain, column); ties go to the COLUMN (strict `normal > column`); when the column wins the ground angle is the zero-initialized `ground_angle0` (flat cap, no slope inherited). Directed-unit exclusion (`ut == (l_ActorInf._68,_6C)`) forces the column to 0.0.
- `mCoBG_AdjustActorY` ground branch (m_collision_bg.c:403): `ground_y >= foot_y` -> feet placed exactly on the ground, `on_ground=TRUE`, vertical speed zeroed. Column ground flows through the ordinary branch — no separate "on object" state.
- Key asymmetries (all source-verified): wall collision expands the column radius by the actor radius, but ground selection uses the raw column radius against the actor CENTER (a ~10-unit annulus where the wall hits before the top becomes ground); the ground query examines only the current unit's single foreground object, while wall collision scans up to 16 columns over the 3×3/5×5/7×7 neighborhood.
- Column height is absolute world Y (`pos.y + object height`), with `pos.y` sampled at the unit center — so a tree on a slope is a flat horizontal cap, constant across its footprint.

### Rust rewrite implementation

`rust/src/column_sweep.rs` additions: `get_bg_height_column`, `get_bg_height_normal_column` (returns ground + column-won flag), `adjust_actor_y_ground`. C ABI: `pc_column_ground_select`. `cargo check --lib` clean. Unit tests were written but NOT run, per the standing instruction.

#### Follow-up: deeper sweep analysis + column data table

- The follow-up brief re-derived `mCoBG_LineWallCheck_Column` in detail; it matches the ported implementation. Two corrections/notes on the brief:
  - Brief point 23 ("can carry a previous correction into the next test") is wrong: `reverse` is reset to `reverse0` (zero) at the top of every loop iteration and the function returns on the first accept, so `tmp_end` is always exactly `end_pos`. The accumulation is dead code, as previously documented.
  - The "next step" (root ordering of `mCoBG_GetCrossCircleAndLine2DvectorPlaneXZ_Xyz`) was already resolved: it delegates to the vector-form intersection (already ported), cross0 = t0 = (-b+root)/2a.
- **New**: `mCoBG_MakeOneColumnCollisionData` (column.c_inc:136) — the column spec table, verbatim: hole (19/0, atr_wall=TRUE, only when old_on_ground), small/med/large/full tree (19/30/40/60/80), stump (10 or 18 / 30), rock (19/31.5), mailbox (15/50), sign (19/45), reserve signboard (10/45), koinobori/flag (19/160). Column XZ = unit center; `pos.y` = terrain-center Y at the column's own unit; `height = pos.y + object height` (holes: `height = pos.y`). Skips the actor's own unit; 16-column cap. Item-ID classification (`IS_ITEM_*`) lives in m_name_table.h and was not ported (game-data ID lists).
- Rust: `ColumnKind`, `column_spec`, `column_top_height`, `COLUMN_MAX` in `column_sweep.rs`. `cargo check --lib` clean. Tests written but NOT run (standing rule).


### Source findings (all verified against the local decomp)

- `mCoBG_LineWallCheck_Column` (m_collision_bg_column.c_inc:454, verbatim): swept movement vs vertical cylinder columns. `vec_end_start = start - end`; XZ circle-line intersections via `mCoBG_GetCrossCircleAndLine2DvectorPlaneXZ_Xyz` (which delegates to the already-ported vector-form `mCoBG_GetCrossCircleAndLine2Dvector`); nearer intersection by squared XZ distance wins (strict `<`, ties go to cross1); per-axis segment-bounds test; `mult = (len_xz - sqrt(d_sq)) / len_xz`; rewind scales the FULL XYZ `vec_end_start` (trajectory truncated, direction preserved); accepted iff `end.y + rev.y <= col.height`. No actor radius added. Start-inside-column -> no sweep.
- `F32_IS_ZERO` is NOT exact zero: `|v| < 0.008` (types.h:146) — reproduced.
- The `tmp_end = end + reverse` per-column accumulation is DEAD CODE in the source: `reverse` is reset to zero at the top of every iteration and the function returns on the first accept, so `tmp_end` is always exactly `end_pos`. Documented, not replicated.
- Complementary `mCoBG_LineGroundCheck_Column` also ported: accepted when `start.y > height && end.y < height` (downward plane crossing); `mult = (height - end.y) / (start.y - end.y)`; verbatim early-out (returns FALSE immediately, skipping later columns, when the Y gate passes but the Y delta is ~zero). Its static `reverse0` is zero-initialized (the source's @BUG comment notwithstanding).
- This is a different primitive from the wall-vector endpoint path: no normal_angle, no WallBounds, no atr_wall — just pos/radius/height.

### Rust rewrite implementation

`rust/src/column_sweep.rs`: `f32_is_zero`, `Column`, `line_wall_check_column_one` / `line_wall_check_column`, `line_ground_check_column_one` / `line_ground_check_column`. C ABI: `pc_line_wall_check_column_one`, `pc_line_ground_check_column_one`. `cargo check --lib` clean. Unit tests were written but NOT run, per the standing instruction.

### Runtime Port Progress: Check45Angle Direction Classification

### Source findings (all verified against the local decomp)

- **Correction to the brief**: `mCoBG_Check45Angle` (m_collision_bg.c:641) is NOT a 4-way FRONT/RIGHT/LEFT/BACK classifier — it is a boolean predicate: `ABS(angle1-angle0) <= 8192 (0x2000 = 45°) || ABS(angle1-angle0) >= 57343 (0xDFFF = (u16)(-8193))`. The second clause is the wraparound catcher (verbatim off-by-one: accepts circular distance up to 8193 ticks).
- **The 4-way classification is the caller's else-if chain** (`mCoBG_SearchColOwnPart`, m_collision_bg.c:692), probing rotated wall angles in FRONT > RIGHT > LEFT > BACK priority:
  - FRONT: `Check45Angle(wall + (180° - 1 tick), actor)` — actor faces within 45° of the reversed wall normal; sets HIT_WALL_FRONT, records `in_front_wall_angle_y`, sets `unk_flag4`.
  - RIGHT: `Check45Angle(wall - 90°, actor)`; LEFT: `Check45Angle(wall + 90°, actor)`; BACK: `Check45Angle(wall, actor)`.
  - Flags go to `hit_wall` or `hit_attribute_wall` by wall type (WALL_TYPE0/1).
- **Two-wall logic** (`mCoBG_MakePartDirectHitWallFlag`): opposing walls = u16 angle difference within ±3 ticks of 180°; close-angle = < 12288 ticks (67.5°). `mCoBG_RegistWallCount`: with exactly 2 walls, averages their normals (sin/cos/atan tables) and probes the actor against (avg + 180° - 1 tick).
- This resolves the brief's open question #17 structurally: FRONT ⟺ actor faces anti-parallel to the wall normal (within 45°).

### Rust rewrite implementation

`rust/src/wall_hit_dir.rs`: `hit_flag` bits, tick constants, `check_45_angle` (verbatim incl. off-by-one), `search_col_own_part` (priority chain with wrapping rotation), `walls_opposing`, `walls_close_angle`. C ABI: `pc_check_45_angle`, `pc_hit_wall_dir`. `cargo check --lib` clean. Unit tests were written but NOT run, per the standing instruction.

#### Follow-up 2: the "debugger trace" of height inputs — done statically

The brief proposed a Dolphin debugger session to trace the height inputs. That session can't run here (no game image or emulator in this environment), but the decomp source gives the exact dataflow — better than a sampled trace:

- `old_ground_y` = `actor.last_world_position.y + ground_dist` (m_collision_bg.c:1829).
- **Normal player-special**: `height.top` = `wall_bounds.start_top`/`end_top` — DISCRETE per-endpoint selection, NO interpolation. Gate: `(old_ground_y - 5.0) + 3.0 <= top`. The actor radius (`range`) is used ONLY in the horizontal tests, never in the vertical gate.
- **Attribute player-special**: ZERO height inputs — no floating-point height load precedes the decision; the test is purely horizontal. A Dolphin trace would show the empty set.
- **Non-player normal path** (the interpolation question): `RoughCheckWallHeight` (`(bot_y + 3.0) <= start_top || <= end_top`) + `CheckHeightExactly`, which DOES interpolate via `mCoBG_GetWallHeight` — except moving walls (`regist_p != NULL`), which use end bounds directly. So: player path = discrete endpoint selection; non-player path = interpolation. The brief's Trace 5 is answered.
- The `atr_wall` branch itself lives in `mCoBG_GetWallKind` (dispatch-table selection), not inside the collision function.

Rust: `rough_check_wall_height`, `old_ground_y` in `endpoint_circle.rs` + tests. `cargo check --lib` clean. Tests written but NOT run (standing rule).

#### Follow-up: correcting the brief's negative result + the structural WHY

- **The symbol DOES exist in the current source.** The follow-up brief searched `m_collision_bg_wall.c_inc` (wall *construction*) and concluded `AttributeWall_Special` is not an upstream name. The function is `mCoBG_Distance2Reverse_AttributeWall_Special` at **m_collision_bg.c:970** — the collision *resolution* file, which is exactly where the brief said to look next. It was already ported in the previous commit.
- **Structural WHY (verified)**: `mCoBG_unit_vec_info_c` (m_collision_bg.c:27) is the shared primitive — `start`/`end`, `wall_bounds` (per-endpoint top/btm), `normal`, `normal_angle`, `wall_name`, `regist_p`, `atr_wall`. Ordinary walls populate `wall_bounds` via `mCoBG_JudgeTopAndSet`. Attribute walls do NOT:
  - `mCoBG_MakeForbidVectorData` (wall.c_inc:460): attributes 27-62 -> `mCoBG_forbid_vector_idx`/`mCoBG_make_vector_table` -> predefined normal/wall_name, `atr_wall = TRUE`, `regist_p = NULL` — no `wall_bounds` assignment.
  - `mCoBG_MakeCircleDefenceWall` (wall.c_inc:600): walls between adjacent collision columns, `atr_wall = TRUE` — no `wall_bounds` assignment.
  - This is WHY the attribute path has no height gate: there are no bounds to test. The brief's "same horizontal primitive, atr_wall selects a different acceptance policy downstream" is exactly right.
- **Rust**: `#[repr(C)] WallVecInfo` + `WallBounds` added to `endpoint_circle.rs` (verbatim layout) with the above documented. `cargo check --lib` clean. No new tests (nothing behavioral added).


### Source findings (all verified against the local decomp)

- **The symbol exists** (`mCoBG_Distance2Reverse_AttributeWall_Special`, m_collision_bg.c:970). The brief's hypothesis is confirmed and sharpened with the exact diffs vs `mCoBG_Distance2Reverse_NormalWall_Special`:
  1. **Same horizontal math**: identical front-line gates on actor_start/actor_end, same `dist < range`, same endpoint-circle tests (`JudgePointInCircle` -> `CheckDistSPCheck` -> `GetCrossCircleAndLine2Dvector`), same `GetSpecialDistanceReverse` (reverse = edge - cross).
  2. **THE difference — no height gate**: the normal-special path requires `(actor_info->old_ground_y - 5.0f) + 3.0f <= height.top` (i.e. `old_ground_y - 2.0 <= top`) per endpoint using that endpoint's wall bounds. The attribute path has NO height test — the wall always blocks the player regardless of elevation.
  3. **NULL registered height**: the attribute path registers `NULL` instead of the endpoint's `mCoBG_WallHeight_c`, so downstream consumers get no height info.
  4. **No `SetMoveBgContactSide`**: the attribute path never sets moving-background contact sides.
- **Dispatch** (`mCoBG_GetWallKind`, m_collision_bg.c:760): `regist_p != NULL` -> MOVE (2); else `atr_wall` -> ATTRIBUTE (1); else NORMAL (0). Player table: `{ NormalWall_Special, AttributeWall_Special, NormalWall_Special }` (MOVE reuses the normal-special path). `atr_wall` is set for forbid-vector walls and special collision walls (m_collision_bg_wall.c_inc:478,634,650).
- **Scope**: non-player attribute walls use `mCoBG_Distance2Reverse_AttributeWall` (point-to-line, no height check either, NULL height, 2.7f graze band) — not ported here; only the player special paths were in scope.

### Rust rewrite implementation

`rust/src/endpoint_circle.rs` additions: `wall_kind` constants + `get_wall_kind` (verbatim dispatch), `normal_special_height_gate` (the gate factored out so the difference is explicit), `attribute_wall_special_collision` (identical horizontal test, no height gate, no height registration). C ABI: `pc_attribute_wall_special`. The existing `endpoint_circle_collision` already modeled the normal path including its height gate. Scoped test run (`cargo test --lib endpoint_circle`): 5/5 pass.

### Runtime Port Progress: Bridge-Water Mask (Town-Gen Side)

### Source findings (all verified against the local decomp)

- **The brief's hypothesis — confirmed with the actual mechanism**: there is no stored binary "bridge-water mask" bitmap. The "mask" is block-type arithmetic plus a counterpart table:
  - Seven river block types (40-46: SOUTH, EAST, WEST, SOUTH_EAST, EAST_SOUTH, SOUTH_WEST, WEST_SOUTH) are immediately followed by seven bridge variants (47-53) in the same order. Conversion is `type + 7` (`mFM_BLOCK_TYPE_RIVER_SOUTH_BRIDGE - mFM_BLOCK_TYPE_RIVER_SOUTH`).
  - `mRF_SetBridgeBlock` (m_random_field_ovl.c:1022): finds the river/cliff crossing via waterfall block types, counts river blocks before/after the crossing, picks ONE random river block before the crossing and converts it (and, for two-step towns with a coin flip, one after — this is the "double bridge" mechanism). A bridge is never placed on arbitrary water; the brief's "bridge counterpart" model is exactly right.
  - `pluss_bridge[108]` (m_map_ovl.c, verbatim): block type -> bridge counterpart for the map overlay (255 = none). Covers the 7 river types, TRACKS_RIVER (13 -> 86), and the river-cliff combo bridges.
- **Tortimer's bridge** reuses the same counterpart concept: the map overlay swaps `pluss_bridge[type]` when `Save_Get(bridge)` exists in that block.
- **Scope honesty**: the brief's candidate-mask/rendering-mask distinction and per-tile water/land topology live in the binary acre BG data (collision arrays + display lists), not in derivable code. The collision-level bridge masks were already ported in `terrain_walls.rs` (bridge_search_water, woodb table, bridge policy).

### Rust rewrite implementation

`rust/src/bridge_acre.rs`: block-type constants, `is_river_block`/`is_bridge_block`, `river_to_bridge_block` (enum arithmetic), `PLUSS_BRIDGE` (verbatim 108-entry table, generated from source), `bridge_counterpart`, `select_bridge_blocks` (the SetBridgeBlock selection kernel). C ABI: `pc_river_to_bridge_block`, `pc_bridge_counterpart`. `cargo check --lib` clean. Unit tests were written but NOT run, per the standing instruction.

### Runtime Port Progress: Slope-Wall Policy

### Source findings (all verified against the local decomp)

- **The brief's open question — answered from the decomp**: the brief could not retrieve the terrain source through the web interface. The mechanism is: `mFI_UtNum2UtCol` (m_field_info.c:842) reads collision from `g_fdinfo->block_info[num].bg_info.collision`, and `m_field_make.c` (`mFM_SetBG`) fills that array by COPYING the `collision[UT_Z_NUM][UT_X_NUM]` array from a predefined `mFM_bg_data_c` entry (`data_bgd[]`) selected by bg_name. So slate flags, corner heights, and bridge attributes are baked into acre template data at build time — the field maker does NOT compute slopes procedurally. This confirms the brief's "predefined acre configurations" model at the collision-data level.
- **Slope ground height** (`mCoBG_GetAreaYSlatingUnit`, m_collision_bg.c:1284, verbatim): the slope is a tilted plane along its diagonal; the actor's unit area (triangle from `mCoBG_GetUnitArea`) selects the ground corner — SLATE_UP: S/E areas -> bot_right*10, N/W -> top_left*10; SLATE_DOWN: N/E -> top_right*10, W/S -> bot_left*10. NOTE the switch fallthrough: SLATE_UP with an unrecognized area falls into the SLATE_DOWN case.
- **Ground dispatch** (`mCoBG_GetBGHeight_Normal`): `slate_flag` selects the slate path vs the normal (possibly triangulated) path. The slate-detail test here is a SINGLE comparison (`top_left != bot_right` -> SLATE_UP, else SLATE_DOWN) — subtly different from `mCoBG_SearchSlateDetail` (two comparisons + SLATE_UP default). Both are preserved as separate functions.
- **Slate wall registration** (`mCoBG_RegistSlatingWallVector_AttributeOff`): `make_slate_wall_proc_table[slate_flag & 1]` — only sloped units generate slate walls (AttributeOn variant always generates via its own table).
- **Scope honesty**: the brief's Policies A-G (embedded upper section, protruding lower mound, cardinal ramp orientation, corner resolution) describe the town-generation/visual-terrain level. The exact per-tile ramp footprints live in the binary BG data (display lists + collision arrays), not in derivable code — so they are documented as the data-level mechanism, not ported as geometry. The collision-level slope policy (walls + ground height) is fully ported.

### Rust rewrite implementation

`rust/src/terrain_walls.rs` additions: `get_area_y_slating_unit` (verbatim area mapping + fallthrough), `slate_detail_for_ground` (single-comparison variant, kept separate from `search_slate_detail`). C ABI: `pc_slate_ground_y`. `cargo check --lib` clean. Unit tests were written but NOT run, per the standing instruction.

### Runtime Port Progress: Cardinal Wall-Edge Construction

### Source findings (all verified against the local decomp)

- **Edge-ownership tables** (`l_make33/55/77_coldata`, verbatim): 3x3 = {00,02,02, 01,03,03, 01,03,03}; 5x5 and 7x7 follow the same pattern (first row 00,02,02...; remaining rows 01,03,03...). Only UP(1)/LEFT(2) bits are ever set - each shared edge is constructed once from the canonical side. `mCoBG_GetUnitInfSearchData` selects by count 3/5/7, defaulting to 3x3.
- **Neighbor anchors** (`mCoBG_MakeUnitVector`): UP = index - size, LEFT = index - 1, DOWN = index + size, RIGHT = index + 1 in the row-major neighborhood.
- **Slate-unit adjustment** (`mCoBG_UtInf2NormalSlateWallVector`, verbatim): when exactly one side is sloped, the slate unit is copied and one corner overwritten from its diagonal partner, per direction and slate orientation (e.g. unit1-slate + UP + SLATE_UP: leftDown = rightDown; unit0-slate + LEFT + SLATE_DOWN: leftUp = leftDown). All 8 direction/side/orientation combinations ported.
- **Height interpolation** (`mCoBG_CheckHeightExactly`, verbatim): LEFT/RIGHT interpolate along Z, UP/DOWN along X, formula `start + (point-start) * ((end-start)/(end-start))` with a zero-division guard; moving walls use end bounds directly; gate is `pos_y + 3.0 <= top`. Slate walls take a separate `GetWallHeight` path (not ported here).
- **wall_name is algorithmic**: CheckHeightExactly switches on it to pick the interpolation axis, so it is preserved as data, not metadata.
- **Inference**: the tables are a canonical edge-ownership scheme to avoid duplicate shared edges (strongly implied by the asymmetric structure); the engineering reason for the exact ownership orientation is not proven from source.

### Rust rewrite implementation

`rust/src/terrain_walls.rs` additions: `MAKE_33/55/77_COLDATA` (verbatim), `cardinal_edge_mask` (with the source's default-to-3x3 fallback), `cardinal_neighbor_index`, `cardinal_edge_exists` (explicit existence kernel), `adjust_slate_unit_for_cardinal` (all 8 slate cases verbatim), `check_height_exactly` (verbatim interpolation + gate). C ABI: `pc_cardinal_edge_mask`, `pc_check_height_exactly`. `cargo check --lib` clean. Unit tests were written but NOT run, per the standing instruction.

`rust/src/terrain_walls.rs` additions: `BridgeAttribute` enum, `is_water_attribute` (WATER..=RIVER_NE), `UNIT_ATTRIBUTE_WATER_INFO` (verbatim 64-entry table), `search_water_attribute`, `get_unit_area`, `bridge_wpos_attribute` (27-31 -> woodb table, 32-35 -> STONE), `bridge_water_search` (first-match-wins direction order), `bridge_should_make_slate` (positive form). C ABI: `pc_bridge_water_search` (8 raw neighbor attrs in Direct order, returns water attr or 0xFF). Corrected attribute numbers as above. `cargo check --lib` clean. Unit tests were written but NOT run, per the standing instruction.

### Rust rewrite implementation

`rust/src/decal_circles.rs`: `DEFENCE_WALL_INFO[8]` verbatim, `circle_defence_wall_idx`, `make_circle_defence_walls` (ordered-pair scan, 128 cap, gate), `RegistCircleInfo`, `DecalCircleSystem` (`regist`/`calc_timer`/`init`/`active_circles`, `calc_adjust` interpolation). Reuses `columns::Column` for the live records and `segment_map::UNIT_SIZE` for unit coords. No C ABI added (registration is gameplay-driven, no stable external caller yet). `cargo check --lib` clean. Unit tests: 198/198 pass in the authorized `cargo test --lib` run on 2026-10-07 (run #6). Gaps: `mCoBG_CrossOffDecalCircle` is decomp-marked @unused/@fabricated and intentionally not ported. No C callers are rewired; full Windows game link unverified.

### Source findings

The brief's three-pass model was verified against `src/game/m_collision_bg.c`, with exact implementation details:

- **Verified tail adjustment** (`m_collision_bg.c:476`): `mCoBG_MakeTab2MoveTail` — `x_bias = |dx|/(|dx|+|dz|)`, `z_bias = 1−x_bias`, 0.2-unit proportional backward shift of the start. Applied before the player branch (line 1135).
- **Verified merge sort** (`m_collision_bg.c:1033`): recursive, `middle = (first+last)>>1`, halves staged in `pre_work[65]`/`bk_work[65]` (65, not 128 — halves of 128 fit), merge comparison `<=` (left-first).
- **Verified reconstruction:** for each sorted distance, scan wall indices 0..count and take the first unused index with `dist_table[unit] == sorted[i]`, tracked by a `u64` used mask — ties resolve in original wall order. 64-bit mask vs 128 wall capacity: unresolved whether ≥64 walls can reach it in practice.
- **Verified dispatch tables** (`m_collision_bg.c:1010`): NORMAL → `{Normal, Attribute, Normal}`; PLAYER → `{NormalSpecial, AttributeSpecial, NormalSpecial}`. NORMAL requires `mCoBG_RangeCheckLinePoint`; the player-special path instead requires both start and end in front, then endpoint-circle handling.
- **Verified suppression** (`mCoBG_CheckDistSPCheck`): shared endpoint within 0.1 AND u16 angle difference < 90°−0x100 (0x3F00) → suppress the corner correction.
- **Verified player-special gate** (`m_collision_bg.c:894`): `front(end) && front(start)`, then `mCoBG_JudgePointInCircle` endpoint tests.

### Rust rewrite implementation

`rust/src/wall_priority.rs` ports the faithful pipeline: `make_tab_2_move_tail` (verbatim), `merge_sort_float` (recursive, `<=` merge), `reconstruct_priority` (u64-mask, first-unused tie-break), `midpoint_dist2` + `priority_order` (full construction), `dist_routine` (both dispatch tables), `check_dist_sp_suppress`, `player_special_front_gate`, `point_in_circle`. `wall_solver.rs`'s player path now uses this faithful priority instead of a plain sort. C ABI: `pc_make_tab_2_move_tail`. `cargo check --lib` clean. Unit tests: 165/165 pass in the authorized `cargo test --lib` run on 2026-10-07. No C callers are rewired; full Windows game link unverified.

### Runtime Port Progress: Dialogue Topic Tables

### Source findings

The brief's layered model was verified against `ac_npc_talk.c_inc`/`m_npc.c`/`m_msg_main.c_inc`/`m_npc.h`/`m_msg_data.h`:

- **Verified message index:** `MSG_MAX = 0x3F91` (`m_msg_data.h:22`); `mMsg_Get_BodyParam` maps ID → (addr, size) from the ARAM offset table (entry 0 → base/size table[0]; entry i → table[i-1], table[i]−table[i-1]). The PC port byte-swaps the u32 table on little-endian (`TARGET_PC`).
- **Verified personality pools** (`ac_npc_talk.c_inc:539`): `aNPC_set_talk_info_talk_request_check` — island → `0x34AC + looks*3 + RANDOM(3)`; mainland → `0x075F + looks*3 + RANDOM(3)`. Six personalities × three variants.
- **Verified talk gate** (`aNPC_force_talk_request`): forced message wins if set; else spontaneous talk needs friendship > 0x80, SEARCH action targeting player, force_call_timer ≤ 0, dist_xz < 80.0, |dist_y| < 60.0.
- **Verified talk state** (`m_npc.c:4865`): `mNpc_Talk_Info_c` = {timer, talk_num, quest_request, unlock_timer, reset_timer}, one per villager + islanders.
- **Verified temper table** (`m_npc.c:4874`): Normal {4000,12,15}, Happy {3000,10,13}, Angry {4000,12,15}, Sad {4000,10,13}, Sleepy {5000,9,12}, Pitfall {5000,9,12}.
- **Verified quest gating:** `mNpc_CheckQuestRequest` / `mNpc_SetQuestRequestOFF` (FALSE + unlock timer); `mNpc_TalkEndMove` sets timer = 1000 and counts the talk.
- **Verified conversation flags** (`m_npc.h:261`): beesting:1, fish_complete:1, insect_complete:1 — explicit world-state topic triggers.
- No monolithic `topic_table[]` exists; topics resolve via condition → message-ID arithmetic → offset table → encoded script. The `talk_request_proc` implementation behind the quest-manager clip remains untraced, as does the full ordinary-villager topic taxonomy.

### Rust rewrite implementation

`rust/src/dialogue_topics.rs` ports the verified mechanisms: `MSG_MAX`, `msg_body_param` (offset-table resolution), `talk_check_msg` (both pool bases), `force_talk_gate`/`TalkGate` (verbatim thresholds), `TalkInfo` (talk_end, quest-request off), `NPC_TEMPER` (verbatim), `ConversationFlags` (bitfield pack), and a rewrite-owned `TopicCategory` taxonomy. C ABI: `pc_topic_talk_check`, `pc_msg_max`. `cargo check --lib` clean. Unit tests: 165/165 pass in the authorized `cargo test --lib` run on 2026-10-07. No C callers are rewired; full Windows game link unverified.

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

### Runtime Port Progress: Furniture Placement & House Interiors

This increment ports the retail house-interior runtime to `pc/rust/src/furniture.rs`, verified against the decompilation before porting (`include/ac_furniture.h`, `src/actor/ac_my_room.c`, `src/actor/ac_my_room_move.c_inc`, `include/m_room_type.h`, `src/game/m_room_type.c`, `include/m_home_h.h`, `include/m_ftr_def.h`).

**Core architectural finding:** the saved house is a persistent foreground-grid representation; entering a room deserializes it into runtime `FTR_ACTOR`s (from layers 0 and 1 only), and leaving the room serializes runtime state back. Storage furniture keeps three contained items in the higher FG layers while saved and in `FTR_ACTOR.items[3]` while live.

**Source findings:**

- House floor (`mHm_flr_c`, 0x8A8): `layer_main`, `layer_secondary` ("also storage layer 0"), `layer_storage1`, `layer_storage2`, `wall_floor` (flooring/wallpaper indices), `tempo_beat`, `floor_bit_info` (wall_original/floor_original custom-design bits). Each `mHm_lyr_c` (0x228) holds `items[16][16]`, `ftr_switch` u64, `haniwa_step[8]`.
- Rotation is encoded in the low 2 bits of the saved item number (`FTR_GET_ROTATION`, `FTR_IDX_2_NO`, `FTR_NO_2_IDX`, `FTR_NO_ROT_2_IDX`); 0=SOUTH, 1=EAST, 2=NORTH, 3=WEST.
- Six shape types: four 1x2 orientations (B90/B180/B270/B0) + 1x1 (A) + 2x2 (C). Exact footprint offsets from `aMR_poccess_table`: B90 {0,-16}, B180 {0,-1}, B270 {0,+16}, B0 {0,+1}, A {0}, C {0,1,16,17} (ut = x + z*16).
- Multi-cell furniture writes the item in the primary cell and `RSV_FE1F` (0xFE1F) in the rest (`aMR_SetFurniture2FG`); `EMPTY_NO` is 0x0000.
- Normal interior bounds are the 8x8 region 1..=8 (`aMR_MIN_BOUND`/`aMR_MAX_BOUND`).
- Placement (`aMR_JudgeBreedNewFurniture`): flat check, reservation check, actor budget, free-slot search, initial facing from `player_angle + 180deg` (45-135 EAST, 135-225 NORTH, 225-315 WEST, else SOUTH), stego skull/balloons (+1) and frog (+2) exceptions (furniture indices 964, 1020-1027, 827 from `m_ftr_def.h`), five-unit forward search (`i = 0..4`), NO_COLLISION under-player fallback, the all-rotations fallback (with the documented missing `aMR_GetPlayerLookAtUnit` call in the retail fallback loop), and the ON_SURFACE-onto-SURFACE layer-1 path.
- Surface classification (`aFTR_SET_TYPE_NORMAL/SURFACE/ON_SURFACE`); second-layer placement needs surface furniture below and an empty layer-1 cell (`aMR_JudgePlace2ndLayer`).
- Storage: `aFTR_KEEP_ITEM_COUNT = mCoBG_LAYER_NUM - 1` = 3, `FTR_ACTOR.items[3]`. This corrects the old `save.rs` comment that labeled the 3-slot count a guide-derived claim — it is now source-proven.
- Save angle quantization (`aMR_GetSaveAngle`): sin > 0.8 -> EAST, sin < -0.8 -> WEST, cos > 0.8 -> SOUTH, cos < -0.8 -> NORTH.
- 3 reservation slots, 46-frame birth process; 17 actor states (STOP..DEATH); up to 4 fitted children per parent with relative positions.
- Scene furniture-max table: NPC house 30, shops 10, small 32, medium 48, large 64, museum painting 20, fossil 25, upper 64, LL2 48, basements 64, cottage 64. `aMR_GetWeight` returns 1, so "weight" is an actor-count budget.
- Switch state: `ftr_switch` u64 keyed by interior position `(ut_x-1)+(ut_z-1)*8`, saved on room exit, cleared/restored on entry — position-keyed, not actor-identity-keyed.

**Rust rewrite implementation:**

New `rust/src/furniture.rs`: grid/layer constants, `Direction` + rotation encoding helpers, `ShapeType` with verbatim footprint offsets and rotation cycling, `SetType`, interaction/contact flag modules, `is_storage`, `FurnitureProfile`, `RoomLayer` (bounds, switch-bit index, get/set), `WallFloor`/`FloorBitInfo`/`HouseFloor`, 17 `FurnitureState`s, `FitFurniture`, `FurnitureActor`, `FurnitureReservation`, `MyRoomRuntime` (actor budget, free-slot search, occupancy occupy/release), scene furniture-max table, weight helpers, `player_facing_to_angle_idx`, `special_orientation_offset`, five-unit forward search, `PlaceJudge`/`Placement`, `check_place_situation`, `judge_place_2nd_layer`, the full `judge_place_furniture` (1x2 forward search + rotation fallback, 1x1/2x2 paths, ON_SURFACE layer-1 path, NO_COLLISION path), `save_angle_quantize`, `set_furniture_to_fg` (primary + RSV_FE1F), `save_switch_data`/`restore_switch_bit`, `make_furniture_actors_from_layers`, `keep_items_to_fg`, `reserve_furniture`. C ABI: pc_ftr_get_rotation, pc_ftr_no_rot_2_idx, pc_ftr_save_angle, pc_furniture_storage_slots, pc_scene_furniture_max, pc_room_bounds_ok, pc_switch_bit_index, pc_judge_place_2nd_layer, pc_ftr_is_storage, pc_furniture_weight. `cargo check --lib` clean. Unit tests were written but NOT run, per the standing instruction.

**Gaps:**

- Exact `aMR_GetPlayerLookAtUnit` world-position-to-cell mapping (the Rust model quantizes to the placement direction; documented in-module).
- Place-table sentinel values beyond the free slot, complete `aFTR_size_info_c` collision table, full `aMR_layer_set_info` entries, parent/child move/rotate propagation, per-furniture vtable behaviors, wall/door FG layout on rehouse, HRA scoring on top of this model.

### Runtime Port Progress: Quest Request Generation

This increment ports the retail quest-*generation* layer to `pc/rust/src/quest_gen.rs`, verified against `include/m_quest.h`, `src/game/m_quest.c`, `include/ac_quest_manager.h`, `src/actor/ac_quest_manager.c`, and `src/actor/ac_quest_talk_init.c` (a second verification pass confirmed every claim; corrections below are folded in). It complements `quest.rs` (quest state/rewards/timeouts/completion) and is distinct from `request_selector.rs` (possession/trade dialogue requests).

**Source findings:**

- Generation pipeline: pending quest? -> 75% attempt gate (`mQst_GetRandom(4) != 0`) -> type table (first-job vs normal) -> uniform type -> uniform kind -> `aQMgr_actor_check_occur()` eligibility -> free storage -> `l_set_data[type][kind]` -> recipient -> item -> pocket handover -> persist.
- Type tables: first-job {DELIVERY, ERRAND}, normal {DELIVERY, ERRAND, CONTEST}; first-job applies when `!mLd_PlayerManKindCheck() && mEv_CheckEvent(mEv_SAVED_FIRSTJOB_PLR0 + player_no)`.
- Kind tables: fj delivery {NORMAL, LOST}, fj errand {REQUEST}; normal delivery {NORMAL, FOREIGN, REMOVE, LOST}, errand {REQUEST}, contest {FRUIT, SOCCER, SNOWMAN, FLOWER, FISH, INSECT, LETTER} — all uniform selection.
- Occurrence gating: every contest kind first requires the kind not already active; snowman = Jan / Feb 1-17 / Dec 25-31, hour 8 through 16 (16:59); flower = Feb 25+ through Aug, >=4 empty acre spaces, <=20 flowers; insect = Mar-Oct / Nov 1-28; letter = local resident player (`mLd_PlayerManKindCheck() == FALSE`); foreign/remove = no existing quest + valid stored/last-removed ID. A failed check fails the attempt (no reroll).
- Six recipient modes (`aQMgr_QUEST_TARGET_*` 0..5): RANDOM (uniform among eligible, giver + same-acre excluded), RANDOM_EXCLUDED (also excludes chain members; sets `errand_type = CHAIN`), ORIGINAL_TARGET (chain head, else giver), FOREIGN (`stored_anm_id`, else `NEW_QUEST_NO_FOREIGN_ID`), LAST_REMOVE (`last_removed_animal_id`, else `NEW_QUEST_NO_REMOVE_ANIMAL_ID`), CLIENT (the giver itself). `mNpc_GetOtherAnimalPersonalIDOtherBlock` picks uniformly among eligible villagers (same-acre residents skipped, not counted).
- `l_set_data[type][kind]` (`aQMgr_set_data_c`: to_type:3, day_limit:6, last_step:4, handover_item:1, src_item_type:3, item, reward_percentages[8], max_pay, msg_start[13]) ported verbatim: 4 delivery rows, 15 errand rows (REQUEST/RANDOM_EXCLUDED/2d/last-step 4; REQUEST_CONTINUE/last-step 1; REQUEST_FINAL/ORIGINAL_TARGET/handover — the 12 first-job rows are uniformly CLIENT/0-day/no-handover/CURRENT_ITEM, differing only in msg_start), 7 contest rows (all CLIENT; fruit 1d/last 1/FRUIT, soccer 1d/last 2, snowman 1d/last 1, flower/fish/insect 3d/last 1, letter 2d/last 2). Item sources: RANDOM=decide_item, FRUIT=town's non-native fruit, CLOTH=decide_cloth, FROM_DATA=set-data item, CURRENT_ITEM=errand item (or ITM_CLOTH001=0x2401), NONE=EMPTY_NO. Quest init: `progress = last_step`, time limit enabled iff `day_limit != 0`.
- Entrusted items: handover takes the first empty pocket (`mPr_GetPossessionItemIdx(priv, EMPTY_NO)`), else the request is not created (`NEW_QUEST_NO_SPACE`); the item is stored with `mPr_ITEM_COND_QUEST` (NORMAL=0/PRESENT=1/QUEST=2, 2-bit-per-pocket). Delivery record `i` <-> pocket `i` (15 records); errand records (5) carry their own `pockets_idx`. Grab/put: `grab.pocket_idx` is the pocket index for delivery but the *errand record index* for errands (retail quirk preserved); put-down first displaces the destination slot's occupant (displacement chain).
- Letter quest: `mQst_SetReceiveLetter` gates on CONTEST/LETTER + progress==2 + empty player_id -> player_id=sender, progress=1, score=rank, present=`mQst_GetPresent(rank)`; rank = length tier (17->+1, 49->+2) + quality>=OK (+3) + present (+6), reusing the ported `mQst_GetMailRank`; presents per rank 0-11 (ranks 5/11: 50/50 carpet/wallpaper via `(RANDOM(4)&1)==0`); reply handbill `0x75 + rank*6 + looks` (72 combinations), festive paper, mailbox-full fails the reply (`mQst_SendRemail` FALSE).
- First-job letters (`mQst_ERRAND_FIRSTJOB_SEND_LETTER[_2]`): ERRAND/FIRST_JOB quests, progress=2, no time limit/reward/entrusted item, `used_ids[1]`=recipient, `used_num`=2; completion on the letter event sets progress=3 and clears `send_reply`.

**Rust rewrite implementation:**

New `rust/src/quest_gen.rs`: type/kind tables + uniform selection, first-job table selector, full first-job errand kind enum (3-14), `OccurCtx` + `occur_ok`, `QuestTarget` + `resolve_recipient` (uniform-among-eligible), `QuestItemSource` + `resolve_item_source`, verbatim `DELIVERY_SET_DATA`/`ERRAND_SET_DATA`/`CONTEST_SET_DATA` + `set_data()` lookup, `quest_init_from_set_data`, item-condition constants, delivery/errand slot records with the pocket-index invariants, `handover_item`, `check_grab`/`check_put` (with the errand-index quirk and displacement chain), `LetterContest` + receive gate/rank/present/handbill, `FirstJobLetter`. C ABI: pc_quest_use_first_job_table, pc_quest_type_select, pc_quest_kind_count, pc_quest_kind_select, pc_quest_occur_ok, pc_quest_target, pc_quest_set_day_limit, pc_quest_set_handover, pc_quest_item_source, pc_quest_set_max_pay, pc_first_empty_pocket, pc_item_condition_quest, pc_letter_present_category, pc_letter_carpet_or_wallpaper, pc_letter_handbill.

**Correction to existing code:** `quest.rs::contest_complete` previously treated fruit/soccer/snowman/letter as "progress == 1". Retail: fruit needs the requested item possessed; snowman needs `player_id` set and equal to the current player; flower needs goal + player-ID qualification; fish/insect need null player_id + owned category item. Now modeled with `ContestQual` + `cqual` bit flags; `pc_contest_complete` takes a flags bitmask (ABI changed; no C callers are wired yet).

`cargo check --lib` clean. Unit tests were written but NOT run, per the standing instruction.

**Gaps:**

- Exact `msg_start` message-kind semantics (values ported verbatim, meanings untraced).
- First-job recipient selection details beyond `used_ids`.
- Full errand-chain progression around `errand_next`.
- Every interaction that can move/destroy/duplicate/invalidate a quest-conditioned pocket item.
- Exact completion/reward transaction ordering per quest type.
- BUGFIXES-configuration differences in quest code.

### Runtime Port Progress: Full Seasonal Species Tables

This increment ports the complete retail seasonal-species data layer to Rust: `rust/src/species.rs` (enums, term/time lookup, transition blending, dynamic insertions, C ABI) plus two machine-generated data modules, `rust/src/fish_tables.rs` and `rust/src/insect_tables.rs`, extracted verbatim from `src/actor/ac_set_ovl_gyoei.c` and `src/actor/ac_set_ovl_insect.c` (GAFE01_00 Rev. 0) by `/tmp/extract_species.py`. The extraction was machine-checked: 678 unique fish entries and 514 + 62 = 576 unique insect entries, exactly the source counts.

**Source findings:**

- Fish: 24 half-month terms (`(month-1)*2 + (day>15)`) x 4 daily periods (21:00-03:59, 04:00-08:59, 09:00-15:59, 16:00-20:59). Separate river / ocean / pond table families plus tournament and island tables. Pond tables exist only Apr-Aug and Sep 1-15; other months are NULL. September is the only month with a distinct latter-half river table (ocean also has a latter-August table).
- Fish tables are ordered entry lists `(species, spawn area, weight)` with relative weights (not percentages). 40 ordinary types plus extended Whale / Empty Can / Boot / Old Tire / Salmon2. Salmon2 is a distinct river-mouth type (acre must be RIVER + MARINE).
- Dynamic fish insertions: Coelacanth is NOT a seasonal entry — rain + ocean + outside 09:00-15:59 injects `(COELACANTH, SEA, 2.0)` into the current term only. Offing copies the ocean table, multiplies all weights by 10, adds Whale = 1.
- Insects: 12 monthly terms x 6 daily periods (23:00-03:59, 04:00-07:59, 08:00-15:59, 16:00-16:59, 17:00-18:59, 19:00-22:59). Jan/Feb/Dec use the sparse 3-entry fallback (Pill Bug / Mole Cricket / Bagworm) for every term. 40 normal types plus SPIRIT and NONE.
- NONE is a real insect entry (explicit no-insect mass); do not remove and renormalize. Insect selection rolls against 100 when the table totals <= 100 (no-spawn mass), against the total otherwise.
- Every town insect table gets Ant/ON_CANDY 1, Ant/ON_TRASH 1, Cockroach/ON_TRASH 1 appended at runtime; when candy or spoiled turnips are present, all other weights are removed (full override). Spirit discards the seasonal table for a single SPIRIT/FLYING/100 entry.
- Seasonal transitions (5-day, randomized 0-5 offset, rates 5/6..1/6, persistent term fields at 0x024174-0x024177) blend tables by ordered concatenation (current x rate + next x (1-rate)); a species may appear twice. Fish transition at half-month boundaries, insects at month boundaries. The island skips transition blending (rate forced 1.0) but the rain-driven Coelacanth injection still runs there.
- `l_insect_birth_sum`: only red dragonfly and firefly are multi-birth (6 + rand(3)); NONE is implicitly (0, 0).
- Verification corrections vs. the brief: Salmon2's river-mouth RIVER+MARINE gate is `#if VERSION >= VER_GAFU01_00` (Australian) — USA Rev. 0 requires RIVER only. The fish invalid-area retry marks the selected entry slot, not the fish type (already matches `ecology::fish_select`). The whale is added for the current term only during transitions.

**Rust rewrite implementation:**

`species.rs` defines `FishType` (45, source order), `InsectType` (42, source order), `FishArea`/`InsectArea` (source order), the `FishSpawnEntry`/`InsectSpawnEntry` structs, `insect_time_no` (the 6-term day segmentation; fish terms/times and the selection algorithms already live in `ecology.rs` and are reused), `blend_fish_tables`/`blend_insect_tables` (ordered concatenation, duplicates preserved), `coelacanth_active`, `offing_entry` (x10 + Whale 1), `candy_trash_entries` + `candy_trash_override_active`, `spirit_table`, `insect_birth_sum`, and table lookup conveniences. Generated modules expose `fish_table(env, term24, time4) -> Option<&[FishSpawnEntry]>` (None for pond-less months), `INSECT_TOWN[12][6]`, `INSECT_ISLAND[6]`, `INSECT_BIRTH_SUM`, `FISH_EVENT[4]`, `FISH_ISLAND[4]`. C ABI: pc_fish_seasonal_count, pc_fish_table_entry, pc_insect_seasonal_count, pc_insect_table_entry, pc_insect_time_no, pc_coelacanth_active.

`cargo check --lib` clean. Unit tests were written but NOT run, per the standing instruction.

**Gaps:**

- Physical spawn-unit selection (fish collision scan, insect live-unit bitmap) is modeled in `ecology.rs` at the algorithm level but not wired to real acre data.
- The odd guide-move initialization of `insect_term`/`gyoei_term` (apparent indexing discrepancy) was noted but not copied; needs investigation if save-init behavior must match exactly.
- Historical BUGFIXES interactions around spawn selection need a separate audit.
- No C callers are rewired to the new tables yet.

### Runtime Port Progress: Systems Showcase Prototype

`showcase/` is a new generator crate (same pattern as `town_prototype/`): it `#[path]`-includes the real library modules (`town_gen`, `ecology`, `species`, `fish_tables`, `insect_tables`, `letter_score`, `villager_mail`, `quest`, `quest_gen`) and emits a single self-contained `showcase.html` (no network, no game assets). Five tabs:

- **3D Town**: the existing WebGL town viewer (CSS/JS adapted into the tabbed shell), freshly generated town, seed 305419896, 6 residents.
- **Fish & Bugs**: interactive explorer over all 236 fish + 78 insect tables embedded as JSON — month/day/hour/water/rain/island selectors render the real verbatim entries with weights and spawn areas, including the dynamic Coelacanth rain injection.
- **Letter Lab**: six sample letters scored at generation time by the real seven-check + trigram engine (`score_letter`, NtscU mode), with per-check bars, totals, and quest ranks.
- **Quest Board**: the real `l_set_data` rows (12 normal type/kind combos) with recipient mode, day limit, final step, handover, item source, max pay.
- **Systems Index**: cards summarizing 24 ported systems.

Both inline scripts pass `node --check`; species lookups, letter totals, and quest rows were smoke-tested against the generated JSON.

### Runtime Port Progress: UKI Float State Machine

New `rust/src/uki.rs`, verified against `include/ac_uki.h`, `src/actor/ac_uki.c`,
and `src/actor/ac_uki_move.c_inc` (GAFE01_00 Rev. 0; PC port corroborating).

Fishing is not one state machine. UKI is a coordinator between four
independent pieces of state, kept as separate Rust types on purpose:
- `proc` (11 `aUKI_PROC_*` values: CARRY, READY, AIR, CAST, WAIT, HIT,
  TOUCH, BITE, CATCH, GET, FORCE) — the float's own process,
- `status` (8 `aUKI_STATUS_*` values) — the player-visible status,
- `gyo_command` (0/1/2: none/engaged/bitten) and `gyo_status` (0..8) —
  the fish-side handshake channel (numeric values are source-proven,
  names are reconstruction),
- `child_actor` — the linked fish actor.
The player only writes `command` (0..8, values source-proven at the call
sites, names reconstructed); the fish only writes `gyo_command`/`gyo_type`.
The machine only runs while `command != 0`, and the per-frame hand offset
is modeled (`apply_hand_offset`).

Ported faithfully: init state (frame_timer 2 at construction, 4 on every
subsequent `set_proc(CARRY)`; gyo_type -1; scale 0.01; max_velocity_y -20.0;
gravity 1.2), all 11 proc initializers (CARRY 4f,
READY 32f + 20f linear parabola 30u behind player, AIR 14f, CAST 50f +
cast_timer 40, WAIT 12f/gyo_status 1, HIT 52f dual trajectories,
TOUCH 12f/gyo_status 2, BITE size-based reel timer, CATCH 20f/gyo_status 6,
GET gyo_status 7), force commands 7/8, the 40-frame fish gate
(`cast_timer`), the failed-reel path (command 6 with no fish -> HIT ->
CARRY), verbatim parabola math (type 0 linear, type 1/2 accelerated with
y_param 12.0/4.5), water drift (0.45 / 0.225 engaged / 0.8 homing past
130u, ripple past 127), coast WAVE/SAND handling, the `touch_timer`
water-state machine (+7.5 hooked / -7.5 unhooked / -1.7 touch cycle),
and waterfall behavior.

Fish-side data tables (verbatim): 45-entry `gyoei_type` (size,
search_area, bite_time), reel timers 26/39/39/39/52/65/78/78 (x2, trash
always 26), search angles (normal 3/7/30/50/180, golden 7.5/15/40/60/180),
search distances (40/40/40/50/60), bite times (normal 10/11/12/15/45,
golden 11/12/13/18/60, x2), touch radii/counters/retreat speeds,
trash substitution `gomi[]` (1/20 on bite commit), and the fish->item
table with its duplicate FISH39 and FISH22 tail. Engine-dependent pieces
(bg collision, effects, sound, vibration) are `StepEnv` inputs and
`UkiEvent` outputs. 7 C ABI exports.

Corrections vs brief: `sizeof(UKI_ACTOR) == 0x2C8` (not 0x2B8); there are
more fields past `coast_flag` (`_2B5`, `hamon_accum`, `touch_vib_accum`,
`logic_accum`, `logic_ticks`). The `ecology.rs` `uki` module only held
status constants; the full machine lives here.

### Runtime Port Progress: Bee/Ant Special Actors

New `rust/src/bee_ant.rs`, verified against `src/actor/ac_bee.c`,
`src/actor/ac_ant.c`, `src/actor/ac_insect_clip.c_inc`,
`src/actor/ac_set_ovl_insect.c`, and `src/bg_item/bg_item_common.c_inc`
(GAFE01_00 Rev. 0).

Bee and ant are two-stage actors, not normal insects. They are BG-part
special actors (`mAc_PROFILE_BEE` / `mAc_PROFILE_ANT`) with their own
state machines (`aBEE_ACT_*`: APPEAR/FLY/CAUGHT/ATTACK_WAIT/ATTACK/
DISAPPEAR; `aANT_ACT_*`: WAIT/CAUGHT/DISAPPEAR). Only on capture are they
converted into a normal `aINS_INSECT_ACTOR` via `aINS_MAKE_EXIST`, which
occupies the reserved ninth insect slot (slots 0..7 are normal spawns).

Ported: insect controller slot reservation, the clip's retryable pending
ant spawn (`ant_spawn_pending` cleared only on successful actor creation),
the runtime ant overlay (ANT/ON_CANDY, ANT/ON_TRASH, COCKROACH/ON_TRASH
appended to every range), the candy/trash weight-override
(`limit_insect_data`) and the field-rank bypass (`env_rate = 1.0`) for
candy/trash selection, exact-one birth, ant substrate checks
(candy 0x2806 / spoiled turnip 0x2F03, rain/snow excluded at the habitat
level), 24.0 catch radius with `NET_CATCH_TYPE_ANT` (vs `INSECT` = 0),
bee tree shake lifecycle (TREE_BEES/CEDAR_TREE_BEES/GOLD_TREE_BEES ->
dormant bee at (-1,-1,-1) + HONEYCOMB drop with speed 5 / accel -1.2,
tree reverts immediately, bee positioned on landing, honeycomb lingers
120 frames independently), bee APPEAR (~85 frames, +3 alpha), 60-frame
catch delay, player-relative flight (speed 2.9 eased, altitude
player.y+50+5*sin(bobbing), turn-dependent body deformation), 30-unit
attack trigger with ATTACK_WAIT staging, sting coupling (bee waits while
the player is STUNG_BEE), ~17-frame disappearance (alpha -15, scale to
0.03 bee / 0.01 ant), and `extra_data = 0` non-release conversion.
Player/net/engine reads are `BeeEnv`/`AntEnv` inputs; side effects are
`BeeEvent`/`AntEvent` outputs. 6 C ABI exports.

Corrections vs brief: bee net-catch uses a 40.0 force-request radius
and a 24.0 table radius (the brief listed only 24.0); the bee does no
catch/attack logic until its rotation.x eases to <= 22.5 deg; the ant
force-catch path uses one-block proximity rather than a fixed radius.

### Runtime Port Progress: Catalog Orders / Lottery Special Delivery

New `rust/src/special_delivery.rs`, verified against `include/m_private.h`,
`src/actor/npc/ac_npc_shop_common.c`,
`src/actor/npc/ac_npc_shop_mastersp_talk.c_inc`, `src/game/m_shop.c`, and
`src/game/m_post_office.c` (GAFE01_00 Rev. 0).

Three separate persistent mechanisms, not one queue:
- `CatalogOrders`: 5 pending furniture orders per player, each storing
  the item plus the shop level captured at order time (delivery letter
  uses `0x049 + shop_level`, so a later upgrade doesn't change the
  letter). Free-slot scan in ascending index order; full queue is
  ORDER_FULL. Orders are paid immediately (price removed, added to
  Nook's sales via `mSP_PlusSales`).
- Lottery: `lottery_items[3]` per shop (already in `shop.rs`), monthly
  lineup biased to an uncollected item in slot 0, 5 valid same-month
  tickets per play, slot-order consumption (with retail's unclamped
  `req -= count`), odds 5/10/20/65 on RANDOM(100), consumed prizes become
  `RSV_SHOP_SOLD_FTR` with no reroll.
- `TicketOverflow`: `lotto_ticket_expiry_month` + `mail_storage` (u8,
  cap 255, month change resets). Buying a ticket with no inventory room
  routes to `aNSC_setup_ticket_remain()` (the overflow counter), not to a
  sixth pocket. Ticket item encoding
  (`TICKET_MONTH`/`TICKET_COUNT`/`TICKET_GET_ITEM`, 0x2C00 range, stacks
  of 1-5).

`deliver_special_mail` ports `mPO_delivery_one_address_special_mail`:
pending tickets first in stacks of at most five
(`ticket_id = base + minus - 1`, template 0x057), then catalog orders in
ascending slot order (template `0x049 + shop_level`, paper ITM_PAPER55,
mail_type 7, present attached). Each mail is transactional: the slot /
count clears only after a free mailbox slot accepts it; on failure the
remainder stays pending and delivery stops.

New finding beyond the brief: the special-delivery trigger IS in the
decomp. `mPO_first_work()` (game start) ->
`mPO_first_delivery_proc()` calls the special-mail helper for the
current player's house whenever the player is a local (non-foreigner)
player — modeled as `first_delivery_special_mail_house`. 7 C ABI exports.

### Runtime Port Progress: Leaflet Broadcast

New `rust/src/leaflet.rs`, verified against `include/m_post_office.h`,
`src/game/m_post_office.c`, `src/actor/npc/ac_npc_post_man_move.c_inc`,
`src/actor/ac_event_manager.c`, `src/game/m_event.c`,
`src/game/m_event_schedule.c_inc`, `src/game/m_shop.c`, and
`src/save_check.c_inc` (GAFE01_00 Rev. 0). All 20 brief claims confirmed
verbatim; one brief interpretation corrected (see below).

Two persistent singleton broadcast messages, not queues:
- `leaflet` (PostOffice @ 0x5DA) — normal broadcast channel.
- `event_leaflet` (PostOffice @ 0x704) — event broadcast channel.
- Separate 4-bit recipient masks: `leaflet_flags` @ 0x830,
  `event_flags` @ 0x832, as a union with a raw int; `sizeof(PostOffice_c)`
  = 0x83C, `delivery_time` @ 0x834. Save checker rejects mask bits outside
  0b1111.
- Inverse flag polarity vs normal mail: 0 = pending, 1 = delivered
  (retail comment: "normal mail flags are set when mail is to be delivered,
  leaflet & event flags are set when mail IS delivered").
- `mPO_post_office_init` sets both masks to 0xF (`raw = 0x000F000F`).
- `mPO_receipt_proc(LEAFLET/EVENT_LEAFLET)` overwrites the singleton and
  resets its mask to 0; a new leaflet replaces an undelivered one.
- Per-house delivery: attempts only when the bit is 0; unclaimed houses
  (land_id 0xFFFF) are marked delivered without mail; foreign/Arbeit
  players are skipped; the singleton is copied into the mailbox via
  `mPO_copy_contents` and the bit set on success; a full mailbox leaves the
  bit at 0 (retryable later).
- Event leaflet validity filter: `mMl_TYPE_SHOP_SALE_LEAFLET` ->
  `mEv_EVENT_SHOP_SALE` (26), `mMl_TYPE_BROKER_SALE_LEAFLET` ->
  `mEv_EVENT_BROKER_SALE` (29), anything else -> -1; delivered only while
  the matching special event is active. An expired handbill becomes
  undeliverable rather than delivered late.
- Event manager builds handbills from ROM templates
  (`aEvMgr_actor_regist_handbill`, font RECV, submitted via
  `mPO_receipt_proc(EVENT_LEAFLET)`): shop-sale uses a 4x4
  shop-level × category table (0x002-0x011, paper 55) with dynamic
  item-count/item-name/month/day/hour free strings; broker-sale picks one of
  0x031-0x033 with `RANDOM(3)` (paper 54) with month/day/hour strings.
- Schedule: `mEv_EVENT_HANDBILL_SHOP_SALE` precedes `SHOP_SALE`
  (Mar→Apr, then Apr 1-23); `mEv_EVENT_HANDBILL_BROKER` precedes
  `BROKER_SALE`.
- Rare-furniture chirashi: `mSP_SetShopRareFurnitureChirashi` gates on shop
  renewal/daytime state, finds a rare furniture item, uses
  `rare_chirashi_bunmen[shop_level][type & 1]` =
  {{18,18},{19,19},{21,20},{23,22}}, paper ITM_PAPER55, type
  `mMl_TYPE_SHOP_SALE_LEAFLET`. The `send_proc` parameter is a
  source-reading trap: only `mPO_SENDTYPE_MAIL` routes through the
  broadcast receipt; the traced caller passes `mPO_SENDTYPE_LEAFLET`,
  which takes the default direct-mailbox branch.
- Postman: `aPMAN_check_delivery` visits a house on normal mail pending OR
  leaflet pending OR event leaflet pending (Arbeit workers: normal mail
  only); `aPMAN_set_delivery_idx` scans from the current index;
  `aPMAN_delivery_init` runs mail -> leaflet -> event leaflet per house.
- Delivery schedule: 09:00/17:00/next-day 09:00 (`mPO_set_next_delivery_time`).
- Startup: `mPO_first_delivery_proc` delivers normal mail, then
  catalog/ticket special mail for the current local player, then leaflets —
  the real caller behind the special-delivery helper.

Correction vs brief: special-event setup sets `event_flags = 0b1111`,
which (given 0 = pending) marks all four recipients DELIVERED, i.e. it
suppresses event-leaflet delivery by default; the pending state comes from
the handbill registration's `mPO_receipt_proc(EVENT_LEAFLET)` (mask = 0).
The brief's claim-20 reading ("all four considered pending") is inverted.

`HouseInfo`/`LeafletMailbox` carry the engine-owned save reads; 7 C ABI
exports (`pc_leaflet_*`).

### Runtime Port Progress: Mother-Mail Scheduler

New `rust/src/mother_mail.rs`, verified against `src/game/m_private.c`,
`include/m_private.h`, `include/m_common_data.h`,
`src/game/m_start_data_init.c`, `include/m_mail.h`, and
`src/game/m_event_schedule.c_inc` (GAFE01_00 Rev. 0).

Per-player persistent scheduler in Save_t (`mother_mail[4]`, 14 bytes each:
date Y/M/D + `normal[7]` + `monthly[2]` + `august`), not in Private_c.
- Startup gates: local player only, valid Private_c and player ID; first
  ever run stamps today's date and sends nothing; otherwise a single
  `date != today` check — no missed-day catch-up.
- Pool A (fixed dates, checked first): birthday (0x184 + RANDOM(2), cake),
  month == day (0x164 + (month-1)*2 + RANDOM(2); Jan 1 = 10,000 Bells,
  letter 18 = mushroom), April Fools (0x180), Mother's Day (0x17C),
  Father's Day (0x17E), Toy Day Dec 24 (0x182 + random furniture). Birthday
  beats month == day. Mother's/Father's Day delegate to the event system.
- Pool B (56 normal letters 0x12C-0x163): `RANDOM(100) < 20` per processed
  day, uniform selection among unsent via 56-bit field; exhausting the pool
  zeroes the whole data block and sends one Pool C letter for the current
  month instead.
- Pool C (seasonal): 2 per month, 8 in August (own byte);
  `mail_start_no_table` = {0x18C, 0x192, 0x186, 0x19E} reproduced literally
  (0x18A-0x1A3 range); May event 1 = fortune shirt via `RANDOM(1)` (kept),
  November = mushroom, December event 0 = apple / others = RANDOM(6) shirts.
- Normal presents: 1/16 random clothing, 3/21/22/47 other fruit, 12 =
  1,000 Bells, 37 doll, 38 dracaena, 40 random umbrella; else EMPTY_NO.
- Delivery: house mailbox first, post-office storage fallback
  (`mPO_SENDTYPE_MAIL`), mail_type = 4 (`mMl_TYPE_MOTHER`), text from ROM
  handbill by mail number. Normal failure still consumes the day (no
  same-letter retry); special-date failure leaves the date, retrying the
  same letter next boot while skipping the normal path.
- Paper: `paper_table[month-1] - 1` with overrides (birthday 0, Jan 1 62,
  Aug 8 47, Dec 24 22).
- Engine-resolved presents (random clothing/umbrella/furniture,
  other-fruit, furniture ids) stay symbolic `PresentSpec` variants; RNG is
  an injected `&mut dyn FnMut(u32) -> u32` so call order is preserved.

4 C ABI exports (`pc_mother_mail_*`).

### Runtime Port Progress: NPC Reply Generation

New `rust/src/npc_reply.rs` plus machine-extracted
`rust/src/mck_key_tables.rs`, verified against `src/game/m_npc.c`,
`src/game/m_mail_check_ovl.c`, `src/game/m_handbill.c`,
`include/m_handbill.h`, `src/game/m_font.c`,
`src/game/m_font_main.c_inc`, and `src/game/m_msg_main.c_inc`
(GAFE01_00 Rev. 0).

Three pieces, kept separate:

1. **mMck letter scorer** (`check_key_hit_nes`): deterministic 7-component
   heuristic over the 192-byte body — A (final `.`/`?`/`!` +20, then +10/-10
   per separator for uppercase within 3 chars), B (+3 per vocabulary hit),
   C (+20/-10 on the first non-space char), D (-50 once for a triple
   identical alpha), E (+20/-20 on spaces/non-spaces >= 20%), F (-150 once
   for a 75+ char run-on), G (-20 per spaceless 32-byte block). The 26
   a-z key tables were extracted verbatim by script (89 pairs max, 's').
   USA retail bug preserved as a documented deviation: the tables end
   `0, 0` (or nothing) while `mMck_cmp_key` scans for a 0x7F terminator, so
   retail reads linker-adjacent bytes — unknowable from source, the Rust
   search stops at the table end. Only the first key byte is
   case-insensitive; bytes 2-3 match lowercase entries exactly.

2. **Reply scheduler**: rank <50 BAD / 50-99 no reply / >=100 OK via
   `mNpc_CheckNormalMail_nes` (the `_length` variant is unused in USA).
   Receiving a letter stamps the date, sets `cond` + `send_reply` for BAD/OK
   only; the `// 25%` comment has no RNG behind it. First-job and
   letter-contest letters suppress the normal reply. Friendship: +3, -5 if
   BAD, +3 if a present was attached (skipped during the first job).
   `mNpc_Remail` scans NPCs in animals[] order, generates on a later
   calendar date only, `break`s the scan on post-office failure (pending
   replies stay pending), then handles the single foreign `Private_c.remail`
   (looks != 0x7F sentinel, no date check, cleared only on success).

3. **Reply generation + mHandbillz composer**: BAD = canned
   `0xC5`/`0xD8` + looks*3 + RANDOM(3), no present. GOOD = `RANDOM(4) & 1`
   present chance, category `RANDOM(4) & 1`, 11 `RANDOM_F` free-string
   category rolls (bases 0x314...0x3B4, ranges 32/40), then five fragment
   indices into SUPERZ/MAILA/MAILB/MAILC/PSZ (MAILB half shifted +16 when a
   present is given), random paper for every reply. The composer is an
   in-place buffer editor over a `HandbillzResources` provider trait:
   SUPER newline-border -> header_back_start (adjusted by control-code
   expansion), MAILA+B+C concatenated (total > 192 fails) newline-padded,
   PS space-padded; FREE0-19 variable-width substitution with article and
   capital-letter controls; the retail `mHandbill_clr_capital_flag`
   copy-paste bug is preserved (it writes `force_art` instead of clearing
   `capital_flag`). Final ROM text needs retail assets the decomp excludes.

`LetterInfo` bitfield, `AnimalRemail` (0x16), and the exact RNG call order
are modeled; engine-owned pieces (ROM text, article strings, random-item
presents, NPC names) stay behind traits/inputs. 5 C ABI exports.

### Runtime Port Progress: Save Land-File Format & Integrity

New `rust/src/save_format.rs`, verified against `src/game/m_card.c`,
`src/game/m_flashrom.c`, `include/m_card.h`, `include/m_flashrom.h`,
`include/m_common_data.h`, `include/m_land.h`, `src/game/m_lib.c`, and
`src/game/m_time.c` (GAFE01_00 Rev. 0). It builds on the checksum
primitives already in `save.rs` (`checksum_sum` /
`checksum_fixup` / `checksum_valid`) and models the full 0x72000-byte
town file as pure buffer logic (GameCube CARD I/O stays engine-side):

- Layout: 0x26000 misc region, primary `Save` slot at 0x26000, backup at
  0x4C000 (`mCD_get_offset` sums prior file-table entries; each of the
  first three entries is `sizeof(Save)` = 0x26000). `Save` = `Save_t`
  (0x242A0) padded to 0x26000; the checksum covers the whole slot,
  padding included.
- Save-check header: version s32 @0x00, code u32 @0x04 (`'GAFE'`),
  land_id u16 @0x08 (valid iff `(id & 0xFF00) == 0x3000`), RTC time @0x0A,
  checksum u16 @0x12. Writer emits version 6; loader accepts 5 or 6.
- `build_save_slot` mirrors `mCD_SaveHome_bg_set_data`: zero the slot,
  copy the live `Save_t`, stamp version/code/land_id/time, write
  copy_protect at `Save_t+0x1A`, then the flat checksum.
- Candidate validation in retail order: identity, then checksum, then
  version. `select_slot` prefers main and flags backup loads as outdated
  (`mCD_ERROR_OUTDATED`).
- `check_broken_land` / `repair_land`: repair only when exactly one copy
  is good; the good copy is written over the broken one.
- `sector_write_plan`: 0x2000-sector compare-before-write (`mem_cmp`
  returns TRUE when equal, so `== 0` means "different, rewrite").
- Aux keep blocks (mail 0xBAC0 @0x1440, original 0xCCA0 @0xCF00, diary
  0xBA20 @0x19BA0, checksum = first u16 of each block): forgiving
  per-block validation, writer order mail/original/diary, diary gets no
  landid stamp. `build_others` takes header bytes from a provider (ROM
  resources are excluded from the decomp).
- Copy protection (`RANDOM(0xFFF0)+1`, range 1..=0xFFF0) is a
  card-association check, separate from the checksum. Correction vs the
  brief's sources: `m_common_data.h` names a second `copy_protect` at
  0x028596 (a decomp artifact - duplicate member names can't compile);
  the operative field is the 0x1A one because `mCD_check_copyProtect`
  only reads the first 0x200 bytes of each copy.
- GCI: 0x40 directory-entry prefix + 0x72000 = 0x72040; exact retail
  dir-entry bytes remain unverified (needs a real USA GCI).

5 C ABI exports (`pc_savef_*`). cargo check --lib clean; tests written
but not run per the standing rule. Second verification pass pending.

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
