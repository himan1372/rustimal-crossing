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
- Correction: the decomp's `mCD_keep_original_c` fields only sum to
  0xCC68, contradicting the struct's own "force size to 0xCCA0" comment
  (the decomp struct is missing 0x38 bytes). The port keeps 0xCCA0
  because the struct's stated intent and an independent PC-port
  round-trip log agree on it; the decomp field arithmetic is what's
  suspect. Correction: `mCD_SaveHome_bg_write_others` passes
  `OTHERS_SIZE + sizeof(Save)*2` as the *expected on-card file length*
  (asserted against the existing file), not as the write byte count.
- GCI: 0x40 directory-entry prefix + 0x72000 = 0x72040; exact retail
  dir-entry bytes remain unverified (needs a real USA GCI).

5 C ABI exports (`pc_savef_*`). cargo check --lib clean; tests written
but not run per the standing rule. Second verification pass pending.

### Runtime Port Progress: Game-Layer Audio (Na_*)

New `rust/src/audio.rs`, verified against `src/audio.c`,
`src/static/jaudio_NES/game/game64.c_inc`,
`include/jaudio_NES/audiocommon.h`, `include/jaudio_NES/audiostruct.h`,
`src/static/jaudio_NES/internal/sub_sys.c`, and `include/m_config.h`
(GAFE01_00 Rev. 0).

Retail audio is a stateful stack, not `play_sound(id)` calls: game
systems -> sAdo_* -> the Na_* game layer (this module) -> the deferred
256-entry `AudioPort` command ring -> the JAudio engine (sequence VM,
DSP, NEOS, AI). This module implements the Na_* layer; the sequence VM,
sample banks, ADPCM, and DSP mixer stay engine-side behind the command
boundary (ROM assets are excluded from the decomp).

Ported:

- Engine constants: 5 groups, 16 subtracks/group, 128 notes, 4 notes per
  subtrack, 48 tatums/beat, 24 AGC channels, 0x90000 CPU audio heap,
  0x810000 ARAM, triple-buffered DAC (7x560), 64 DSP channels, 4 FX
  buffers.
- Group assignments: SE=0, BGM0=1, FTR_INST=2, BGM1=3, VOICE=4, with the
  BGM0/BGM1 crossfade handle alternation.
- `AudioPort` (opcode + 3 arg bytes + param union) and the 256-entry
  retail command ring (the PC port uses 2048; retail is 256).
- Sound-ID packing: upper nibble flags (MONO 0x1000, DIST_REVERB 0x2000,
  ECHO 0x4000, SINGLETON 0x8000), low byte index, 0x0F00 bank byte.
- Trigger SEs: 6 stateful slots, MONO subtrack-14 bypass, singleton
  rejection, free-slot preference, oldest-frames replacement gated by
  `TRGPRIO` priority (128 entries: 97x50, 8x60 at 97-104, 23x70 at
  105-127), per-frame parameter pushes (`Sou_TrgMake`).
- Positional math: quadratic `distance2vol` (540-unit radius, 1.15
  base), `angle2pan` + `pan_kochou` with output-mode scaling (x1.6 /
  x1 / x0.75), MD curve clamped [0.2, 0.8].
- Level SEs: 6 slots, each with a 4-deep `sou_ls_stack` push FIFO and an
  8-byte `_00.._07` dedup/pending list (the list is never populated with
  nonzero bytes in the decomp snapshot, so its dedup/promote paths are
  dormant; both are modeled).
- Ongen: 50-entry source cache with aging, 4 active continuous slots,
  retail foundIndex-stays-0 quirk.
- Room insects: 50 entries with randomized variance and the j=0
  restart bug.
- Voice: 2 alternating slots, Animalese/Click/Silent modes, spec ->
  sequence (243/244/245) and per-spec volume/pitch table.
- BGM: 24-entry fine/snow/sakura mute tables + 6-entry museum table
  (verbatim values), weather masking with the 0x45 substitution,
  per-subtrack filter pushes.
- Furniture instruments (2 melody identities) and the 14-slot rhythm
  allocator on group 2; scene audio state (echo, chime, BGM volume,
  persistent sequences 242/246).

Preserved retail quirks: the kiteki dead-branch/negative volume, the
ROOM_TYPE_OTHER AREA2 bug, the insect variance restart bug, the
foundIndex slot-0 quirk. Documented safety deviations (unknowable from
source): `TRGPRIO` out-of-bounds reads (120-entry table, no retail
bounds check) return `None`; the level-fade 8..14 indexing bug is noted
but not reproduced.

5 C ABI exports (`pc_audio_*`). cargo check --lib clean; tests written
but not run per the standing rule. Second verification pass pending.

### Runtime Port Progress: Title Screen (trademark demo -> title demo)

New `rust/src/title_demo.rs`, verified against `src/game/m_trademark.c`,
`src/game/m_titledemo.c`, `src/data/scene/title_demo.c`,
`src/data/titledemo/pact0.c`..`pact4.c`, `include/m_titledemo.h`,
`include/m_trademark.h`, `include/lb_rtc.h` (month enum), and
`include/m_kankyo.h` (weather enum).

Flow: `trademark_init` builds `GAME_TRADEMARK` (exec = `trademark_main`,
fade starts opaque `alpha = 0xFF00`, `logo_timer = 60`, `move_timer = 16`,
`stage = 0`; first boot forces `stage = 5` via `mTR_first_flag`) ->
`trademark_main` per frame (`fqrand` seed, cancel check, stage machine,
logo + fade draws; at stage 5 calls `trademark_goto_demo_scene`) ->
demo scene plays 60 s of recorded input -> wipe to next demo.

Ported (pure logic; display lists, fades, `GAME_PLAY`/`PLAYER_ACTOR`
writes, and RTC/common-data calls stay in C):

- Demo-id cycling (`mTD_demono_get`): `LOGO(-1) -> 1 -> 2 -> 3 -> 4 -> 5 -> 1...`;
  zero-based index (`mTD_get_titledemo_no`); button-ok cutoff
  (`mTD_tdemo_button_ok_check`: FALSE at frame >= 3530 = 3600 - 70).
- Key-data decoder: bit layout documented in `pact0.c` itself
  (`XXXXXXXB YYYYYYYA`: bit 0 = A, bits 7:1 = stick Y, bit 8 = B,
  bits 15:9 = stick X); sign-extended halves / 512, truncation toward
  zero — matches the C exactly.
- Zero-order-hold frame advance (`set_player_demo_keydata_hold`):
  `delta_time` clamped to [0, 1/30], converted to fractional 30 fps
  frames, accumulated; index saturates at sample 1799. The interpolation
  variant (`set_player_demo_keydata`) is `#if 0`'d out in source
  ("@fakematch?") and was not ported.
- Trademark stage machine as a pure step function
  (`trademark_move_step`): stage 0 plays the lost-fanfare
  (`s_titlebgm` = {83, 84, 85, 86, 87}), stage 1 waits `move_timer`,
  stage 2 fades the Nintendo logo in (`alpha2 += 0x880*dt` to `0xFF00`),
  stage 4 holds `logo_timer`, stage 3/cancel fades to black; returns
  `PlayFanfare` / `GotoDemo` events for the C side effects.
  START-cancel modeled (`trademark_cancel_check`: stage 4 + pad + START).
- Data tables (verbatim): 5 demo door positions, 5 tradeday presets
  (Apr 6 13:00 sakura, Jun 16 13:00 rain, Aug 1 6:00 clear,
  Nov 1 16:00 clear, Feb 1 2:00 snow), 5 `pactN_head_table`s
  (pos/angle/tool/size; tools: 0x2201 axe, 0x2203 rod, 0x2204 umbrella).
- Verified no-op: `mTD_player_keydata_init`'s tool filter assigns every
  branch back to itself, so the header tool id passes through unchanged.

C ABI: `pc_title_demono_get`, `pc_title_get_titledemo_no`,
`pc_title_tdemo_button_ok_check` (stateful, Mutex-guarded; pc_-prefixed
so they don't collide with `m_titledemo.c` while it's still compiled),
`pc_titledemo_decode_keydata`, `pc_titledemo_advance_frame`.
`cargo check --lib` clean (only the crate-standard "never used"
warnings); no C callers rewired yet, per the usual pattern. No test
runs, per the standing rule.

Gaps: `title_demo_move`'s settings-overlay hold and `mTD_rtc_set` /
`mTD_rtc_reserve` need C-side state (`GAME_PLAY`, common data) and stay
in C. The 1800-sample `pactN_key_data` tables stay in C as data.

### Runtime Port Progress: Museum Fossil Assessment (m_museum.c)

New `rust/src/museum.rs`, registered as `mod museum` in `rust/src/lib.rs`.
Decomp-verified against `src/game/m_museum.c`, `src/game/m_museum_display.c`,
`src/actor/npc/ac_npc_curator_move.c_inc`, `include/m_museum.h`,
`include/m_museum_display.h` (GAFE01_00 Rev 0 USA).

The retail flow: dig up generic `ITM_FOSSIL` (0x2511) -> mail it to the
Museum (`mMl_NAME_TYPE_MUSEUM` = 2) -> `mMsm_SendMuseumMail` increments
`stored_fossil_num` (cap `mMsm_REMAIL_SLOTS` = 30) -> at the daily 6 AM
grow tick `mMsm_DepositFossil` calls `mMsm_SendResultMail`, which
round-robins fossil replies (max `mMsm_MAX_MAIL` = 3 mails/player/pass).
Each reply carries a *different*, randomly selected identified fossil —
the mailed generic fossil is never transformed in place. Donating the
identified fossil to Blathers is a separate operation via
`mMmd_RequestMuseumDisplay`, which removes the inventory item only on
success. Players with `contacted == FALSE` (museum info mail not yet
received) get no replies.

Ported (pure kernels; mail queue, saved museum record, display
bitfields, RNG, and all donation/inventory side effects stay in C):

- Fossil furniture range: `FTR_DINO_START` = 0x1EEC ..
  `FTR_DINO_END` = 0x1F4F; index = `(item - 0x1EEC) >> 2` (0..24).
- `mMsm_GetFossilMailNo`: 25-entry `mail_no_table` reproduced verbatim
  (deliberately non-sequential order preserved); out-of-range -> index 0.
- `mMsm_SendResultMail` remail table: `{0x22D, 0x22B, 0x22C, 0x22E}`
  indexed by kind - 1.
- `aCR_get_fossil_type`: 7 dinosaur groups as item-id ranges
  (trikera/trex/bront/stego/ptera/hutaba/mammoth); singles -> -1.
- `aCR_get_idx_to_donate_fossil`: Blathers response indices — self -> 4,
  none -> 16 (trilobite 17, ammonite 18, egg 19, stump 20, amber 21),
  deleted player -> 12, other player -> 8.
- `aCR_chk_fossil_parts_complete_sub`: all parts' donators in 1..5
  (mammoth has 2 parts, rest 3); skeleton messages
  0x2F78..0x2F7E, default 0x2F84.
- `mMmd_GetDisplayInfo` fossil branch: donator nibble -> CANNOT/CAN/ALREADY.

C ABI (all `pc_`-prefixed; the C originals stay compiled):
`pc_museum_fossil_mail_no`, `pc_museum_fossil_index`,
`pc_museum_remail_mail_no`, `pc_museum_fossil_type`,
`pc_museum_donate_response`, `pc_museum_parts_complete`,
`pc_museum_display_classify`, `pc_museum_skeleton_msg_no`.
No C callers rewired yet, per the usual pattern. No test runs, per the
standing rule.

Gaps / do-not-port: `RANDOM` calls in `mMsm_GetFossil` (exactly 2 per
fossil: `RANDOM(2)` then `RANDOM(count)`); the single-vs-multi pool split
comes from ROM birth data and is unverified — do not reimplement the
selection distribution in Rust. Mailing a donatable item donates it
*at send time* (`mMsm_SetRemailInfo` -> `mMmd_RequestMuseumDisplay`).

### Runtime Port Progress: Special-Visitor Classification (m_npc.h)

New `rust/src/visitors.rs`, registered as `mod visitors` in
`rust/src/lib.rs`. Decomp-verified against `include/m_npc.h`,
`include/m_name_table.h`, `src/actor/npc/ac_npc_init.c_inc`,
`src/actor/npc/ac_npc_curator_move.c_inc` (GAFE01_00 Rev 0 USA).

Special visitors are not one system: NPC identity, the set-NPC manager
(`ac_set_npc_manager.c`), actor init branches, event scheduling, RNG,
and saved records all stay in C. There is NO single retail predicate
deciding "visitor X appears today" — appearance emerges from the
manager's regular/guest procedures plus event state — so no such kernel
exists here. Ported are only the source-proven pure classifications:

- NPC ID encoding: `mNpc_GET_IDX` (`& 0x0FFF`), `mNpc_GET_TYPE`
  (`& 0xF000`), `mNpc_IS_SPECIAL` (type `== 0xD000`);
  `ITEM_NAME_GET_TYPE` (upper nibble: 13 = special, 14 = villager).
  `mNpc_EVENT_NPC_NUM` = 5, `mNpc_MASK_NPC_NUM` = 3.
- Curator item ranges (verbatim, asymmetry preserved): art
  `0x12AC..=0x12E7` (inclusive), insect `0x2D00..0x2D28` (end
  EXCLUSIVE), fish `0x2300..=0x2340` (end inclusive).
- Curator offer classification in `aCR_msg_win_open_wait` priority
  order: empty / fossil / art / insect / fish / generic fossil / other.
  (The fossil check itself lives in `museum.rs`.)

C ABI (all `pc_`-prefixed): `pc_visitor_is_special`,
`pc_visitor_name_type`, `pc_visitor_npc_idx`, `pc_curator_is_art`,
`pc_curator_is_insect`, `pc_curator_is_fish`, `pc_curator_offer_class`.
No C callers rewired yet, per the usual pattern. No test runs, per the
standing rule.

Gaps / do-not-port: the `aNPC_actor_init` special path (event-NPC then
mask-NPC clothing fallback) needs actor pointers + save; the
`aSNMgr` manager, `mNpc_GetSameEventNpc`/`mNpc_GetSameMaskNpc`
record lookups, and all visitor scheduling stay in C.

### Runtime Port Progress: Wisp the Ghost (ac_ev_ghost.c)

New `rust/src/wisp.rs`, registered as `mod wisp` in `rust/src/lib.rs`.
Decomp-verified against `ac_ev_ghost_talk.c_inc`,
`ac_ev_ghost_schedule.c_inc`, `ac_ev_ghost.c`, `ac_event_manager.c`,
`m_event.c`, `m_event.h`, `m_name_table.h`, `ac_ev_ghost.h`
(GAFE01_00 Rev 0 USA). All 9 brief claims verified verbatim against the
decomp before porting; the one correction is that the reschedule window
is [today−7, today+4], not "previous Monday to upcoming Friday" (the
variable names are reused from the Gulliver block).

Ported (pure kernels; scheduler, event manager, actor lifecycle, spirit
spawn overrides, RNG, inventory mutation, and reward selection stay
in C):

- Spirit items `ITM_SPIRIT0..4` = 0x2D28..0x2D2C (stack-encoding 1..5
  spirits; they begin exactly where the exclusive-end insect range
  stops). `ITEM_IS_WISP`, `WISP_COUNT` preserved.
- `aEGH_hitodama_num`: weighted total `sum(count[i] * (i+1))` over the
  five per-id inventory counts.
- Weed-favor thresholds: strict `<50/<150/<450/<900` → 0x2EF1..0x2EF5.
- Greeting: 0 → `0x2EE7 + RANDOM(5)` (roll stays in C, passed in);
  1..4 → `0x2EEB + num`; >= 5 → 0x2EF0.
- `aEGH_time_over`: strict `now_sec > 4 * 3600`.
- Found alpha: 140 if found, 0 if hidden (`aEGH_actor_ct`).
- Flags: `ACTIVE` 0x4000, `RETURNED_SPIRITS` 0x8000,
  `COMMON_SPAWNED_SPIRITS` 0x8000, `SP_NPC_EV_GHOST` 0xD06F,
  `MINIMUM_GRASS_COUNT` 8.

C ABI (all `pc_`-prefixed): `pc_wisp_item_is_spirit`,
`pc_wisp_stack_count`, `pc_wisp_spirit_count`, `pc_wisp_weed_msg`,
`pc_wisp_greet_msg`, `pc_wisp_time_over`, `pc_wisp_found_alpha`.
No C callers rewired yet, per the usual pattern. No test runs, per the
standing rule.

Gaps / do-not-port: `init_weekly_event` date math + `RANDOM(3)`,
`ghost_start` placement (`bx = 1+RANDOM(5)`, `bz = 2+RANDOM(4)`,
uniqueness retry), the insect-spawn spirit override, `aEGH_not_collect_get`
reward selection (22 collection lists + RNG), weed-clear/roof-paint
side effects, and the handover choreography.

### Runtime Port Progress: Blanca the Faceless Cat (ac_npc_mask_cat.c)

New `rust/src/blanca.rs`, registered as `mod blanca` in `rust/src/lib.rs`.
Decomp-verified against `m_event.c`, `ac_event_manager.c`,
`ac_npc_mask_cat_move.c_inc`, `m_scene.c`, `m_name_table.h`, `m_npc.h`,
`m_private.h`, `m_mask_cat.h` (GAFE01_00 Rev 0 USA). All 10 brief claims
checked verbatim before porting; no corrections needed. The `// Blanca`
source comment in `m_npc.c` proves the English name directly.

Two special-NPC ids, one mask-cat actor family: `SP_NPC_MASK_CAT`
(0xD075) is the regular event visitor; `SP_NPC_MASK_CAT2` (0xD076) is
the travel-scene slot (Blanca or Rover). They use different registration
paths and must not be collapsed.

Ported (pure kernels; scheduler, mask registry, actor lifecycle, talk
state machine, travel selection, RNG, and save mutations stay in C):

- Talk message ids: first talk `talk_idx * 4 + 0x31E4` (pre-increment
  idx, verbatim); repeat `0x31E5 + 4 * (talk_idx - 1) + RANDOM(3)`
  with `talk_idx <= 0` clamped to 1 (roll stays in C, passed in).
- Birthday-branch weekday set: Sun/Mon/Wed/Fri rejected
  (`mMC_check_birth_day` minus the save read).
- Cloth item: `ITM_CLOTH_START` (0x2400) `+ cloth_no`
  (`regist_mask_maskcat`); travel-scene guard writes `cloth_no` back
  only when the selected cloth is in `[0x2400, 0x24FF)` (strict),
  else `EMPTY_NO`.
- Constants: `TALK_IDX_MAX` 10, `FLAG_MASK_CAT_SCHEDULED` 1<<0.

C ABI (all `pc_`-prefixed): `pc_blanca_first_talk_msg`,
`pc_blanca_repeat_talk_msg`, `pc_blanca_birth_day_ok`,
`pc_blanca_cloth_item`, `pc_blanca_travel_cloth_no`.
No C callers rewired yet, per the usual pattern. No test runs, per the
standing rule.

Gaps / do-not-port: `mMC_check_birth` (resets save at talk max),
`mMC_check_delete` (±1 week expiry), `gohome_mask_start` selection
order vs the returning-animal event, `mNpc_RegistMaskNpc`, the
`init_weekly_event` MASK_NPC override, and the SCENE_START_DEMO3
Blanca/Rover toggle.

### Runtime Port Progress: Gulliver the Seagull (ac_ev_dozaemon.c)

New `rust/src/gulliver.rs`, registered as `mod gulliver` in
`rust/src/lib.rs`. Decomp-verified against `m_event.c`,
`m_event_schedule.c_inc`, `ac_event_manager.c`,
`ac_ev_dozaemon_move.c_inc`, `m_name_table.h`, `m_event.h`,
`ac_ev_dozaemon.h`, `lb_rtc.h` (GAFE01_00 Rev 0 USA). 9 of 10 brief
claims checked verbatim; one correction — the WAKEUP save flag does
NOT delete the actor, it routes the schedule proc to
`aEDZ_THINK_WANDER` (post-wakeup wandering); the only deletion in
`aEDZ_actor_ct` is `mv_proc != NULL && dozaemon_completed`.

Identity: `SP_NPC_EV_DOZAEMON` (0xD064) is Gulliver's event identity;
`SP_NPC_CASTAWAY` (0xD07B) is the separate boat-travel castaway actor,
gated by Gulliver's event state. Do not merge them.

Ported (pure kernels; scheduler, seaside placement, actor lifecycle,
thought/talk state machine, furniture selector, inventory grant, and
save mutations stay in C):

- `after_n_day`: verbatim port of the pure packed month/day arithmetic
  (`raw = (month<<8)|day`), with the leap-year check preserved via a
  year argument (the decomp's `last_day_of_month` reads the RTC year,
  so the kernel takes it instead of reading RTC).
- Weekly date offset `1 + (today_date.raw + hour) % 5` — no RNG,
  deterministic; range 1–5.
- Talk message ids: normal talk strict `< 0.35f` → 0x23F8, else
  `0x23F9 + RANDOM(5)`; forced talk `0x23FE + RANDOM(6)`; reward branch
  0x2404/0x2408 (`mMsg_SET_CONTINUE_MSG_NUM`), later branch
  0x2405/0x2409 (`mDemo_Set_msg_num` — different macro, distinction
  preserved in C), post-give 0x2406/0x2407, wandering `0x240A+RANDOM(6)`.
  All RNG rolls stay in C and are passed in.
- Constants: save flags WAKEUP/GIVE/GAVE_FIRST, event window 06:00–22:00.

C ABI (all `pc_`-prefixed): `pc_gulliver_after_n_day`,
`pc_gulliver_weekly_ofs`, `pc_gulliver_norm_talk_msg`,
`pc_gulliver_force_talk_msg`, `pc_gulliver_ageru_msg`,
`pc_gulliver_ageru2_msg`, `pc_gulliver_give_msg`,
`pc_gulliver_wander_msg`.
No C callers rewired yet, per the usual pattern. No test runs, per the
standing rule.

Gaps / do-not-port: `init_weekly_event` priority vs bridge and
mask-cat events, `make_actor_in_seaside_block` placement,
`aEDZ_actor_ct` lifecycle, the full talk state machine,
`mSP_SelectRandomItem_New` reward distribution (JONASON list),
`mPr_SetFreePossessionItem`, and the boat-demo castaway path.

### Runtime Port Progress: Kapp'n's Boat Ferry (ac_boat.c)

New `rust/src/boat.rs`, registered as `mod boat` in `rust/src/lib.rs`.
Decomp-verified against `ac_boat.c`, `ac_boat_move.c_inc`,
`ac_boat_demo.c`, `ac_boat_demo_move.c_inc`, `ac_boat_demo.h`,
`m_name_table.h`, `m_lib.h`, `types.h` (GAFE01_00 Rev 0 USA). All 10
brief claims checked verbatim; only two precision nuances, no
corrections: the rudder-step halving is `(s16)(step * 0.5f)` (float
multiply-then-truncate), not `/ 2.0f`; and the vertical "reversal"
sets the velocity *cap* (`max_velocity_y` ±0.1f) and re-rolls gravity,
it does not flip the current velocity. Bonus find: a fourth song
table the brief missed — `sing_dialogue_msg_base_no[4]`.

Ported (pure data/kernels; the boat actor, demo controller, Kapp'n
NPC, player demo requests, frame-driven handshakes, GBA transfer, and
the island data transition all stay in C):

- Fixed 8-point route table with the source's F/G/H-5 unit comments.
- Direction data: `block_z == 6` → to-island; last-point {7, 0},
  first-checkpoint {1, 6}; init angles 0xE000/0x6000;
  checkpoint-advance rule (sign of dZ only, no distance threshold).
- Song tables and message math: `player_pattern_table`,
  first-part BGM/message tables, dialogue message base (bonus),
  second-part BGM (0x70–0x79) and message base; repeat-rejection
  acceptance test (the `do/while(TRUE)` rolls stay in C, passed per
  roll); `last_song_*` init 0xFF.
- Constants: `SP_NPC_SENDO` 0xD072, GBA loader 0xC83C / program
  0x15244 (+4 on Aus), USA FRAMERATE identity values.

C ABI (all `pc_`-prefixed): `pc_boat_to_island`,
`pc_boat_should_advance_checkpoint`, `pc_boat_player_pattern`,
`pc_boat_song_accepted`, `pc_boat_first_part_bgm`,
`pc_boat_first_part_msg`, `pc_boat_dialogue_msg`,
`pc_boat_second_part_bgm`, `pc_boat_second_part_msg`.
No C callers rewired yet, per the usual pattern. No test runs, per the
standing rule.

Gaps / do-not-port: steering (frame-rate-accumulator-coupled),
rocking/vertical motion, passenger attachment, the action state
machines and handshakes, Kapp'n's talk state machine, the island
environment transition, the GBA transfer routines, and the castaway
cameo lifecycle.

### Runtime Port Progress: Train Arrival/Departure (m_train_control.c)

New `rust/src/train.rs`, registered as `mod train` in `rust/src/lib.rs`.
Decomp-verified against `m_train_control.c`, `m_train_control.h`,
`ac_train1_move.c_inc`, `ac_train_door.c`, `ac_train_window.c`,
`ac_station.c`, `ac_npc_station_master_talk.c_inc`,
`ac_intro_demo_move.c_inc`, `m_name_table.h`, `ac_station_clip.h`,
`m_camera2.h` (GAFE01_00 Rev 0 USA). All 14 brief claims checked
verbatim; no corrections, five precision nuances carried:
(1) the controller's per-frame X advance is `0.5f * speed * dt` and all
`chase_f` rates are `rate * dt` — frame-rate coupled, integration loop
stays in C; (2) `mTRC_demo_init` uses `now_sec − 4:50`, a different
literal from the −4:10 timetable offset; (3) `anime_table`/`anime_end`
index by `seq_no` while `anime_spd` indexes by `action`;
(4) arg0 passenger force passes a zeroed rot with the ROTY flag, arg1
passes `rot = NULL` with no ROTY flag; (5) Porter's talk tables are
(msg, continuation, camera) structs, not bare message ids.
`ac_shasho.c` confirmed absent (only `ac_npc_station_master.c`).

Ported (pure data/kernels; the controller, both train actors,
door/window/station actors, Porter's state machine, the intro demo,
and the Rover/Blanca scene selection all stay in C):

- Timetable: 25-entry table (hours 00–24 at hour:19:00), first entry
  `>=` now, minus 4:10 — verbatim scan; day-change `>=` rollover rule.
- USA speed constants incl. the un-cleaned `0.21000001f` literal.
- Four init paths: X starts, initial actions, control states, Z 740.0,
  and the Y rule (`mRF_CheckFieldStep3() ? 300 : 180` stays in C).
- Transition thresholds: `pos.x > 2165.0`, `fabsf(speed) < 0.008`,
  timers 48/84/180, `start_timer += 310`, `pos.x > 4400.0`.
- Train1 animation tables with the seq_no/action asymmetry;
  passenger offsets (60,20,20)/(2,16,20) with the rot-flag asymmetry;
  door sound frames 2.0/27.0.
- Window: scroll speeds per scene, palette threshold table with the
  verbatim first-match scan, tree-scroll +5.
- Station variant math (`type / 5`, type/pal offsets, winter skeleton
  index stays in C); Porter forced/normal talk tables with
  continuations and cameras; identities GUIDE D004, DEPART_MASTER
  D00B, STATION_MASTER D00E, RCN_GUIDE D014; intro-demo values
  (flag 3, scene_mode 9, doorway (2180,0,820), walk (2220,840) @ 2.5).

C ABI (all `pc_`-prefixed): `pc_train_depart_timer`,
`pc_train_day_rollover`, `pc_train_tree_palette_idx`,
`pc_train_station_arg0`, `pc_train1_anim_speed`,
`pc_train1_anim_end`.
No C callers rewired yet, per the usual pattern. No test runs, per the
standing rule.

Gaps / do-not-port: the `dt`-coupled controller transitions, train
actor lifecycles and parent/child linkage, door/window animation
state, Porter's think table, the intro-demo choreography, and the
SCENE_START_DEMO3 Rover/Blanca selection (covered by the Blanca
module's travel-scene kernels).

### Runtime Port Progress: Tortimer the Mayor (ac_ev_soncho.c)

New `rust/src/tortimer.rs`, registered as `mod tortimer` in
`rust/src/lib.rs`. Decomp-verified against `m_event.c`,
`ac_event_manager.c`, `ac_ev_soncho.c`, `ac_ev_soncho_talk.c_inc`,
`ac_ev_soncho2.c`, `ac_ev_soncho2_think.c_inc`,
`ac_ev_speech_soncho_talk.c_inc`, `m_name_table.h`, `m_event.h`,
`m_kankyo.h`, `ac_groundhog_control.h` (GAFE01_00 Rev 0 USA). 10 of 11
brief claims checked verbatim; 1 headline correction — the brief
assigned the bridge dialogue backwards: the FIRST conversation shows
the time-of-day greeting, SUBSEQUENT conversations show the proposal
0x2F3E (the talk-info callback checks the first-talk bit while
`aESC_talk_init` sets it, the same pattern as Blanca's mask-cat
actor). The decline path was backwards too: first-conversation
declines play the rotating follow-up, later declines use the
secondary dialogue. A port following the brief would show the
proposal on the wrong conversation. Other nuances: the villager gate
is `>= ANIMAL_NUM_MAX` (15), not `==`; the `1<<7` high bit is the top
bit of the 7-bit `locations_used` field; the speech actor has a
Groundhog Day pre-branch before the weather switch; the brief's event
list omitted `mEv_EVENT_SONCHO_METEOR_SHOWER`. The `// Tortimer`
comment in `m_npc.c` proves the English name.

Ported (pure data/kernels; the three actor paths, calendar/event
scheduler, bridge candidate generation, bridge acceptance writes,
NPC state machines, dialogue, and save mutations all stay in C):

- Six identities (EV_SONCHO D06E, EV_SONCHO2 D074, D078, D079,
  SONCHO D07F, EV_SPEECH_SONCHO D087) with shared metadata; kept
  separate — profiles and callers differ.
- Bridge weekday gate (Saturday, or non-Sunday with a stale weekly
  date); candidate index `remain = (spots-1) - used`; final-candidate
  high-bit rule.
- Time-of-day greeting with verbatim hour boundaries; initial-message
  and decline-path selection with the CORRECTED first/repeat
  semantics; pending message `0x2F44 + roll` (roll in C); follow-up
  rotation `0x2F47`/`0x2F39 + msg_no` with wrap at 5.
- Speech message: Groundhog pre-branch, then clear/snow/other weather
  branches off `mString_SPEECH_SONCHO_START` 0x3DB5 (rolls in C);
  forced-speech base.
- Constants: ANIMAL_NUM_MAX 15, tug-of-war timer 0x12C,
  FIRST_TALK_BIT 5.

C ABI (all `pc_`-prefixed): `pc_tortimer_bridge_day_eligible`,
`pc_tortimer_bridge_remain`, `pc_tortimer_time_talk_msg`,
`pc_tortimer_initial_msg`, `pc_tortimer_pending_msg`,
`pc_tortimer_followup_msg`, `pc_tortimer_next_followup_msg`,
`pc_tortimer_speech_msg`.
No C callers rewired yet, per the usual pattern. No test runs, per the
standing rule.

Gaps / do-not-port: `mSC_get_soncho_event()` date rules,
`mSC_trophy_item()` mappings, event-day spawn locations,
`river_stream`/`bridge_stand_river` geometry, the full
`mEv_EVENT_BRIDGE_MAKE` construction lifecycle, and the Groundhog
Day control clip.

### Runtime Port Progress: K.K. Slider (ac_npc_totakeke.c)

New `rust/src/totakeke.rs`, registered as `mod totakeke` in
`rust/src/lib.rs`. Decomp-verified against `m_event.c`,
`m_event_schedule.c_inc`, `ac_event_manager.c`,
`ac_npc_totakeke_talk.c_inc`, `ac_npc_totakeke_think.c_inc`,
`ac_mikanbox_clip.c_inc`, `m_name_table.h`, `ac_npc_totakeke.h`,
`m_private.h`, `audio_defs.h` (GAFE01_00 Rev 0 USA). All 12 brief
claims checked verbatim; no corrections. Two omission-type bonuses
from verification: four extra "no-intensity" weather thresholds in
the performance chain, and the Resetti-cameo spawn window
(`show_timer >= FRAMERATE_TIMER(3600) && < FRAMERATE_SELECT(3610,3010)`).

Ported (pure data/kernels; the actor, weekly scheduler, event
manager, talk/think state machines, mikanbox clip, inventory/aircheck
writes, RNG, and staff-roll audio/weather side effects all stay in C):

- The collection selector, bit-for-bit: `count_uncollected` and
  `select_song` reimplement `aMKBC_count_merody` /
  `aMKBC_clip_search_merody` given the two collection words and the
  already-generated random index (the single `RANDOM(count)` and the
  all-collected fallback bitmap clear stay in C). `collect`
  reimplements `aMKBC_clip_check_merody` (`md_no < 32` → word0 bit,
  else word1 bit `md_no - 32`, only when `md_no < 52`).
- Reroll acceptance: up to 3 retries while `song == _99f`; the final
  song may still equal `_99f` (rolls in C).
- Request path: `0x37 + RANDOM_F(3)` fallback, `item − 0x2A00` index,
  `song >= 0x34` continuation branch.
- Greeting `MSG_TOTAKEKE_START + {0,1,2,0xD}[talk]`; live BGM
  `BGM_TOTAKEKE_LIVE0 (192) + song`.
- Constants: identities (TOTAKEKE D05D, P_SEL D018, RTC D066 — kept
  separate), Saturday 20:00–23:00, placement block {1,3} unit {7,7}
  arg 0x51, save flags, think states, minidisk range 0x2A00–0x2A37
  with 52 selectable songs.

C ABI (all `pc_`-prefixed): `pc_totakeke_greet_msg`,
`pc_totakeke_needs_reroll`, `pc_totakeke_request_fallback_song`,
`pc_totakeke_request_song_index`, `pc_totakeke_live_bgm`,
`pc_totakeke_count_uncollected`, `pc_totakeke_select_song`.
No C callers rewired yet, per the usual pattern. No test runs, per the
standing rule.

Gaps / do-not-port: the alternate identities' creation paths, the
submenu request-text path, the staff-roll audio synchronization,
the weather-change counter chain, the Resetti cameo, and the full
mikanbox clip lifecycle.

### Runtime Port Progress: Mr. Resetti (ac_npc_majin.c)

New `rust/src/majin.rs`, registered as `mod majin` in `rust/src/lib.rs`.
Decomp-verified against `ac_reset_demo.c`, `ac_npc_majin.c`,
`ac_npc_majin_schedule.c_inc`, `ac_npc_majin_talk.c_inc`,
`ac_npc_majin3_talk.c_inc`, `ac_npc_majin4_talk.c_inc`,
`ac_npc_majin4_schedule.c_inc`, `ac_ev_majin_move.c_inc`,
`m_name_table.h` (GAFE01_00 Rev 0 USA). All 8 brief claims checked
verbatim; no corrections. Nuances carried: the `default:` call state
is CALL_4 (covers MAJIN2/3/4); `se_clear_timer = -1` one-shot disarm;
the `-1` chg_length sentinel can never trigger; the light window is
strict (`> 19:00` / `< 05:00`, boundary seconds excluded); the bounds
analysis proves `majin_name[reset-1]` can never go out of bounds (the
demo is only created when `reset_flag` is set, which happens alongside
the count increment, so `reset_count >= 1`).

Ported (pure data/kernels; the reset-demo controller, the Resetti/Don
actor variants, their think/talk state machines, the text-entry
overlay, Don's reset/fade sequence, the lighting, and the Groundhog
Day event actor all stay in C):

- Reset-count normalization (`>= 9` → `6 + ((reset-9) % 3)`) and the
  8-entry identity table: D07C, D07D, D03E, D065, D07E (Don), D068,
  D069, D080.
- Initial call states per identity (MAJIN→CALL, D07C→CALL_2,
  D07D→CALL_3, BROTHER→CALL_4, D080→CALL_5, default→CALL_4).
- Force-talk table `{0x1B3B, 0x3A38, 0x3A42, 0x3A4C, 0x3B05}`;
  Don's `{0x250F, 0x2512}`, timers 180/8, fade 255−8/update.
- The word-entry test, bit-for-bit, over caller-provided buffers (the
  ROM string tables stay in C): full-16-byte good-word comparison,
  the ng-word substring scan with the exact `chg_length` boundary
  stepping (`len` 2→10 across the 32 words), `17 − len` offsets per
  word, `mem_cmp`-returns-TRUE-on-equality semantics, GOOD > NG >
  WRONG priority, and the outcome messages
  (GOOD→0x23EC, NG→0x23EF, WRONG→0x23EE).
- Groundhog event-actor weather messages (rolls in C); the strict
  point-light time window.
- Constants: EV_MAJIN D081, MAJIN5 D06B, think states, word counts
  (16 good, 32 ng), string buffer size 16.

C ABI (all `pc_`-prefixed): `pc_majin_reset_identity`,
`pc_majin_initial_call_state`, `pc_majin_force_talk_msg`,
`pc_majin_ng_word_len`, `pc_majin_word_msg`, `pc_majin_ev_msg`,
`pc_majin_point_light_active`. The full buffer-taking scan
(`check_ng_word`/`check_word`) is implemented as pure Rust over
caller-provided buffers for future differential testing; the ROM
string tables themselves are retail string data and stay in C.
No C callers rewired yet, per the usual pattern. No test runs, per the
standing rule.

Gaps / do-not-port: the reset-hole effect, the text-entry overlay,
the demo-clip coordination, the weather-clip sound handling, and the
Groundhog controller handshake.

### Runtime Port Progress: Post Office Clerks (Pelly / Phyllis / Pete)

New `rust/src/post_office.rs`, registered as `mod post_office` in
`rust/src/lib.rs`. Decomp-verified against `m_post_office.c`,
`m_npc.c`, `ac_npc_post_girl.c`, `ac_npc_post_girl.c_inc`,
`ac_npc_post_man.c`, `ac_npc_post_man_move.c_inc`,
`ac_aprilfool_control.c` (GAFE01_00 Rev 0 USA). All 14 brief claims
checked verbatim; no corrections. Nuances carried: the choice
adjustment is an EXACT-VALUE switch on status (0 or 1 only — status 3
gets no adjustment, do not rewrite as a bitmask test); the loan-balance
comma byte is written at `str[l]` but the free-string copy receives
length `l` — reproduce, do not "fix"; the delivery advance and
force-flag clear happen only if `mPO_make_post_man()` succeeds;
delivery is scene-gated (skipped in PLAYERSELECT/TITLE_DEMO); Pete's
start action and Y position are birth-type-indexed; the receipt path's
ignored `mPO_receipt_proc()` return is preserved (do not fix without
retail evidence). Bonus finds: the second 8-entry status-indexed table
for the repay-after path, the 5-entry receive table embedding the
refusal IDs, the desk-full → `force_mail_delivery_flag = TRUE` path in
`aPG_actor_dt`, and `draw_type = (npc_id == POST_GIRL) ? 0 : 1`.

Ported (pure data/kernels; the counter state machine, the mail
subsystem, Pete's movement/delivery actor, the submenu UI, the font
formatting, and the April Fools overrides all stay in C):

- Clerk status bitmask (desk-full from the aggregate stored-mail
  count vs 5, loan/first-job/bank-account branches) — dual bitmask and
  8-entry-table index.
- Opening greeting (`msg_no[status] + draw_type`) and the repay-after
  table; `draw_type` 0 for Pelly, 1 for Phyllis.
- Business-menu choice adjustment with the exact-value condition.
- Destination-validation outcome codes and the refusal tables
  (`{0x8e1,0x8b5,0x1bd7}` by `dest-1`; the 5-entry receive table by
  `dest+1`).
- Bank-account init eligibility (native, no loan, house size >= 3,
  renew == FALSE) and the loan millions/remainder split.
- Delivery schedule (`<9` → 09:00, `<17` → 17:00, else next-day 09:00,
  min/sec zeroed).
- Pete's dialogue bases/bounds by talk_type (rolls in C), the
  14-entry talk-permit table, birth-type start Y/actions, April Fools
  indices (Pelly 0x0007, Phyllis 0x0008, Pete 0x0009).
- Constants: POST_GIRL D003, POST_GIRL2 D012, POST_MAN D011, name
  indices 0x20D/0x20E/0x20F, MAIL_STORAGE_SIZE 5, HOME_MAILBOX_SIZE 10.

C ABI (all `pc_`-prefixed): `pc_post_clerk_status`,
`pc_post_greet_msg`, `pc_post_repay_after_msg`,
`pc_post_adjust_choice`, `pc_post_refuse_msg`,
`pc_post_bank_account_eligible`, `pc_post_next_delivery_hour`,
`pc_post_next_delivery_next_day`, `pc_post_pete_msg_base`,
`pc_post_pete_talk_permit`, `pc_post_draw_type`.
No C callers rewired yet, per the usual pattern. No test runs, per the
standing rule.

Gaps / do-not-port: the 30-action counter machine, `mPO_receipt_proc()`
and the delivery procs, Pete's route tables and mailbox animation,
the April Fools dialogue overrides, and the loan-balance font
formatting.

### Runtime Port Progress: Police Officers (Copper / Booker)

New `rust/src/police.rs`, registered as `mod police` in
`rust/src/lib.rs`. Decomp-verified against `ac_npc_police.c`,
`ac_npc_police_move.c_inc`, `ac_npc_police2.c`,
`ac_npc_police2_move.c_inc`, `m_police_box.c`, `m_npc.c`,
`ac_aprilfool_control.c` (GAFE01_00 Rev 0 USA). All 20 brief claims
checked verbatim; no corrections. Nuances carried: the movement
thresholds are kept as written — `dist < (SQ(50.0f)−50.0f)` = 2450.0
and `dist < (SQ(70.0f)+100.0f)` = 5000.0 on the SQUARED distance, do
not simplify; the zone `ux` comparisons are strict `>` at 7/4/1 with
`uz` clamped 2–6; the same-zone far case returns
RUN_PL_OTHER_ZONE (verbatim retail quirk, RUN_PL_SAME_ZONE is never
produced by the decider); the sleepy branch is an `else if` on the
aerobics condition; the talk-request gate checks `last_field_id`
while talk-init checks `last_scene_no`; the ticket branch is
`TYPE == NAME_TYPE_ITEM1 && CAT == ITEM1_CAT_TICKET` specifically;
the 0xF128 constant is shared with DUMMY_BOAT (per the decomp's own
comment); the init uses `mSP_LISTTYPE_ABC`, not "common-priority";
the `(int)qrand() >= 0` expression is preserved without resolving
its semantics; clearing in `mPB_keep_all_item_in_block` is
unconditional for every accepted item and the RANDOM(20) overwrite
hits the temp array.

Ported (pure data/kernels; both actors, the PoliceBox_c store, the
field-item relocation, the item-generation tables, the message
formatting, and the April Fools overrides all stay in C):

- Copper: time-of-day talk index and message (+4 on the
  player-kind/map-flag test), entrance greeting 0x0771, choice
  messages (hint path stays in C), the three event-hint tables with
  per-state selectors and the 0x2C2D/0x2C37 fallbacks, the aerobics
  entry gate (06:00–07:00, not rain, outside 67.5°) with its
  animation loops {2,4,1} and exit test, and the sleepy gate
  (02:00–04:00, ≥60 units) with its morph/loop sequencing.
- Booker: zone math, movement decision with speeds, slot index from
  the reserved prop, greeting/talk messages by store count and
  `_99C`, item-name and inventory-full messages, the dummy pickup
  position, and the station-exit constants.
- Lost-and-found: storage constants, forced-generation category and
  goods sub-category from the roll (rolls in C), the grow eligibility
  gate, the FIFO append index, and the acre-cleanup merge math.
- Constants: POLICE D00D, POLICE2 D010, name indices 0x210/0x6D9,
  April Fools 0x0005/0x0006, RSV_POLICE_ITEM_0 0xF128.

C ABI (all `pc_`-prefixed, 15 exports): `pc_police_copper_talk_msg`,
`pc_police_copper_choice_msg`, `pc_police_copper_aerobics_gate`,
`pc_police_copper_sleepy_gate`, `pc_police_hint_active_msg`,
`pc_police_hint_today_msg`, `pc_police_hint_later_msg`,
`pc_police_booker_zone`, `pc_police_booker_move_action`,
`pc_police_booker_slot_idx`, `pc_police_booker_greet_msg`,
`pc_police_booker_talk_msg`, `pc_police_force_category`,
`pc_police_goods_category`, `pc_police_keep_append_index`.
No C callers rewired yet, per the usual pattern. No test runs, per the
standing rule.

Gaps / do-not-port: the actor state machines and RNG, the save-store
mutations, the item-generation item tables, the message free-string
formatting, and the event-controller checks.

### Runtime Port Progress: Event Visitors (Katrina / Saharah / Wendell / Gracie / Redd)

New `rust/src/event_visitors.rs`, registered as `mod event_visitors`
in `rust/src/lib.rs`. Decomp-verified against `ac_ev_gypsy_move.c_inc`,
`ac_ev_carpetPeddler.c_inc`, `ac_ev_artist_move.c_inc`,
`ac_ev_designer_talk.c_inc`, `ac_ev_broker_move.c_inc`,
`ac_ev_miko_move.c_inc`, `ac_event_manager.c`, `bg_item_common.c_inc`,
`m_mail_password_check.c`, `m_npc.c` (GAFE01_00 Rev 0 USA). 10 of 12
brief claims checked verbatim; 2 HEADLINE CORRECTIONS carried.

CORRECTION 1 — Katrina's fortune distribution: the brief claimed
Normal 10% / Money luck 50%. Tracing the transform (`RANDOM(10)`
values 0–3 forced to 4, then `−= 4`) gives index 0 = NORMAL at 50%
and every other destiny at 10%. The decomp's own comment confirms
("50% chance of 'normal' luck, 10% chance for other types").

CORRECTION 2 — Wendell's fruit path: the brief claimed fish and fruit
both go to the gift demo. The `next_act_idx` table shows only FISH
goes to DEMO_START_WAIT; FOOD takes the ordinary TALK_END_WAIT path.

Resolved items the brief called unresolved: Saharah's price formula —
`price = 3000 × 2^used` with the arabian event-save count capped at 4
(max 48,000); the event-save key is `arabian`; the polarity is
trade-in (the player must POSSESS a carpet to proceed). Bonus find:
EV_MIKO (D03D), Katrina's New Year omikuji actor, with its own
4-entry destiny table, `RANDOM(4)`, message 0x1911+omikuji, granting
ITM_FORTUNE_SLIP and a mail letter. Other bonuses: MONEY_LUCK makes
bell trees drop ITM_MONEY_1000; Redd has four identities
(D001/D013/D02C/D039); Katrina has two (D00C/D03D); the event
absolute values (ARTIST 74 … CARPET_PEDDLER 79); confirmed none of the
five appear in the April Fools table.

Ported (pure data/kernels; the event scheduling, the event manager,
all five actors' state machines, the wallet/inventory/save mutations,
the item-selection tables, the tent actor, the handbill generation,
and the RNG all stay in C):

- Katrina: the fortune-index transform (roll stays in C), the
  six-destiny table, result/saved-destiny/talk/call-in messages, the
  fortune-string bases (rolls in C, wallet decremented first), and the
  EV_MIKO omikuji mapping.
- Saharah: the price curve, the used-count increment cap, and the
  second-menu check messages with the possession-before-money order.
- Wendell: the item classifier (decoded type/category in, class
  out), the message table, and the corrected next-action markers.
- Gracie: both 27-entry dialogue tables verbatim (duplicates
  preserved), the reward-result message indices, and the event-list
  vs common-list selector.
- Redd: the handbill templates and the tent-liveness rule.
- Constants: all nine identities, the five name indices, the event
  values, the destiny enum, the item-name encoding constants.

C ABI (all `pc_`-prefixed, 15 exports): `pc_ev_fortune_index`,
`pc_ev_fortune_result_msg`, `pc_ev_fortune_saved_msg`,
`pc_ev_fortune_talk_msg`, `pc_ev_fortune_string_no`,
`pc_ev_miko_destiny`, `pc_ev_miko_msg`, `pc_ev_saharah_price`,
`pc_ev_saharah_check_msg`, `pc_ev_wendell_classify`,
`pc_ev_wendell_msg`, `pc_ev_wendell_next_action`,
`pc_ev_gracie_msg`, `pc_ev_gracie_result_msg_idx`,
`pc_ev_redd_handbill`.
No C callers rewired yet, per the usual pattern. No test runs, per the
standing rule.

Gaps / do-not-port: the actor state machines and RNG, the save-store
mutations, the item-generation item tables, the tent actor's
merchandise code, the message formatting, and the event-controller
checks.

### Runtime Port Progress: Stalk Market (Turnips)

New `rust/src/stalk_market.rs`, registered as `mod stalk_market` in
`rust/src/lib.rs`. Decomp-verified against `m_kabu_manager.c`,
`ac_ev_kabuPeddler_move.c_inc`, `ac_npc_shop_common.c`,
`m_all_grow_ovl.c`, `ac_event_manager.c` (GAFE01_00 Rev 0 USA). All 13
brief claims checked verbatim; no corrections. Nuances carried: trend
B has an EXTRA probability clamp the brief missed — after the ×0.3
drop, `price_decrease_percent` is clamped to ≤ 0.1 (fires whenever the
pre-drop chance exceeded 1/3); the trend-B minimum comparison is
literally `>` (not `<`) — do not "fix" it; trend C keeps
`0.14999998f` verbatim; the `Kabu_manager()` week window is
bidirectional (±1 week); the typeB decomp comment has a typo
(`//0.1f` labeled `(20%)`) but the behavior is residual 20%. Bonus
resolutions: the spoilage timing the brief called unresolved is partly
answered in code — `mAGrw_CheckSpoilKabuTime` spoils when the coming
Sunday is overdue, inventory/house/haniwa turnips ARE converted via
`mAGrw_SpoilAllPossession` (the unconditional null-`grow_info` path),
and `mAGrw_ZuruSpoilKabu` is the anti-time-travel path.

Ported as a deterministic differential harness (every kernel takes the
roll results as inputs; nothing rolls):

- The Sunday price formula (`1.0 + (r − 0.5) × 0.6`, ×100, truncated
  to u16 → 70–129).
- Trend selection with the single-draw subtract-select and the
  previous-trend tables.
- Trend A: the ×8.0 spike and the Mon–Fri spike day.
- Trend B: the full six-day state machine with the exact operation
  order, the literal `>` minimum comparison, and the probability
  clamp.
- Trend C: the chained factor with the verbatim constant.
- Joan: purchase/sale tables, transaction totals, sale messages,
  greetings, identity D007, the turnip item IDs.
- Nook: the quest → spoiled → Sunday → ordinary branch order and
  the payout formula.
- Spoilage: the spoil decision (flag vs unconditional null path) and
  the coming-Sunday weekday offset.

Constants: weekdays, trends A/B/C, the 0x18-byte save layout, the
turnip IDs (ITM_KABU_START 0x2F00), price bounds 10/2000.

C ABI (all `pc_`-prefixed, 14 exports): `pc_kabu_sunday_price`,
`pc_kabu_next_trend`, `pc_kabu_spike_price`, `pc_kabu_spike_day`,
`pc_kabu_trend_b_init`, `pc_kabu_trend_b_min_price`,
`pc_kabu_trend_b_step`, `pc_kabu_trend_c_factor`,
`pc_kabu_joan_total`, `pc_kabu_joan_sale_msg`,
`pc_kabu_nook_payout`, `pc_kabu_nook_turnip_branch`,
`pc_kabu_spoil_kabu`, `pc_kabu_spoil_weekday_offset`.
No C callers rewired yet, per the usual pattern. No test runs, per the
standing rule.

Gaps / do-not-port: the schedule generation driver, the date-window
logic, Joan's actor and transaction, Nook's shop transaction, the
field-growth spoilage passes, the event-manager callbacks, and the
RNG.

### Runtime Port Progress: Lost-and-Found Storage & Police Station

New `rust/src/lost_found.rs`, registered as `mod lost_found` in
`rust/src/lib.rs`. Decomp-verified against `m_police_box.c`,
`ac_police_box.c`, `ac_police_box_move.c_inc`,
`bg_police_item_move.c_inc`, `m_npc.c`, `m_start_data_init.c`,
`m_all_grow_ovl.c`, `ac_event_manager.c` (GAFE01_00 Rev 0 USA). All 12
brief claims checked verbatim; no corrections. Nuances carried: the
brief's `mPB_keep_item()` caller list was incomplete — 8 more call
sites found (br_shop, douzou, event_manager, house, shop_level,
m_field_info, m_start_data_init, and m_police_box itself); bulk
collection has FOUR cases (POOL, SHRINE, STATION, PLAYER), not two;
the double `mPB_police_box_init` call is version-gated — exactly one
compiles for GAFE01_00; the display Y function takes `(pos, 1.0f)`;
the in-place compaction is proven safe by the `keep_item <= item_buf`
invariant.

Ported (pure buffer/data kernels; the save array, the field state,
the item-name decoding, the item selectors, and the RNG stay in C):

- The stored-item count (scans all 20, never stops at holes).
- The order-preserving compaction, safe for in-place use.
- Single-item insertion with the FIFO eviction (the ITEM1/FTR type
  filter stays in C as a gate argument); appends at the count, not
  the first hole.
- The floor-display coordinate table with the exact nibble usage
  (x = low nibble, z = high nibble).
- The station light expression and its angle targets/chase rate.
- The reserved-ID range test, the door constants, and the init slot
  layout.

Scope split: the sibling `police` module keeps Booker's interaction
math and the bulk-merge index math; this module owns the storage
buffer operations and the building.

C ABI (all `pc_`-prefixed, 7 exports): `pc_lf_count`,
`pc_lf_compact`, `pc_lf_insert`, `pc_lf_display_x`,
`pc_lf_display_z`, `pc_lf_station_light_on`, `pc_lf_is_rsvpolice`.
No C callers rewired yet, per the usual pattern. No test runs, per the
standing rule.

Gaps / do-not-port: the save mutations, the bulk acre collection, the
random generation, the display draw calls, the door transitions, and
the item-name decoding.

### Runtime Port Progress: ABD Bank

New `rust/src/bank.rs`, registered as `mod bank` in
`rust/src/lib.rs`. Decomp-verified against `m_bank_ovl.c`,
`m_bank_ovl.h`, `m_private.h`, `m_mail.c`, `ac_npc_post_girl.c`,
`ac_npc_post_girl.c_inc`, `save_check_NSW.c_inc` (GAFE01_00 Rev 0
USA). All 13 brief claims checked; no behavioral corrections. Two
brief imprecisions carried: the smallest place value is 1, not 10
(keta table 100000/10000/1000/100/10/1, cursor 6 = OK); and
`player_max_bell` is exactly
`99999 + 30000 × (money bags of any denomination) + 30000 × (empty
slots)`, valuing every existing bag at the largest bag size.

Ported (pure amount-editing arithmetic; the wallet/bag inventory, the
save writes, and the submenu input machine stay in C):

- Cursor-to-place-value and the displayed cash change (ABS of the
  proposed-minus-opening difference).
- The deposit branch verbatim: cash clamp → bank-max clamp → zero
  check (buzzer, return before the crossing check) → the
  opening-cash crossing cap.
- The withdrawal branch verbatim: bank clamp → capacity clamp →
  zero check → the mirrored crossing cap.
- Pelly's bank-account eligibility (native, loan == 0, house size ≥ 3,
  not mid-renewal).
- The four milestone tiers with the ascending first-eligible
  selection and the per-player claimed flags.
- The save-validation comparison, preserved literally (signed).

Deliberately NOT ported: `mBN_bank_ovl_init`, `mBN_bank_ok` (the
Bell-bag removal/upgrade/creation loops), the submenu input state
machine, Pelly's dialogue, and the mail send. Confirmed: the overlay
computes NO interest — `bank_account` is referenced only in the
overlay, the milestone mail, Pelly's balance message, and save
validation.

C ABI (all `pc_`-prefixed, 7 exports): `pc_bn_deposit`,
`pc_bn_withdraw`, `pc_bn_keta`, `pc_bn_bell_change`,
`pc_bn_has_bank_account`, `pc_bn_milestone`,
`pc_bn_deposit_save_error`. No C callers rewired yet, per the usual
pattern. No test runs, per the standing rule.

### Runtime Port Progress: Catalog UI & Order Transaction

New `rust/src/catalog.rs`, registered as `mod catalog` in
`rust/src/lib.rs`. Decomp-verified against `m_catalog_ovl.c`,
`m_catalog_ovl.h`, `m_tag_ovl.c`, `ac_npc_shop_common.c`,
`ac_npc_shop_common.h`, `m_shop.c`, `m_private.h` (GAFE01_00 Rev 0
USA). All 12 brief claims checked; no behavioral corrections. The
catalog-to-order chain turned out fully traceable (stronger than the
brief's "strong reconstruction"): the "Order" tag runs
`mTG_order_proc`, which converts the FTR-space selection to
ITEM1-space and carries `price == 0` as `slot_no = 0`, mapping to
ORDER_UNAVAILABLE.

Ported (pure data/selection/transaction kernels; rendering,
animation, input, item-name decoding, the FTR index tables, and the
save stay in C):

- The collection-bit test (`idx >> 5` / `idx & 0x1F`).
- The menu constants, the nine category ids, and the five collected
  bitfield sizes (other categories reuse the furniture bitfield).
- The completion rule and the `top_idx + y_idx` selection.
- The price rules: unconditional zero for haniwa/fossils/music and
  the non-FTR fallback; list-gated and always-priced cases documented
  with the verbatim special index ranges.
- The order-selection mapping (`price == 0` → slot 0 →
  ORDER_UNAVAILABLE; slot 1 → ORDER_OFFER; EMPTY_NO → cancel).
- The order-confirmation outcome, the funds check (wallet + bags),
  the payment wallet arithmetic (bags consumed smallest-first, the
  remainder stays in the wallet and may exceed WALLET_MAX), and the
  sales cap clamp.

Deliberately NOT ported: the rendering/animation/input state
machines, `aNSC_set_ftr_order`'s inventory writes (no NULL check in
retail — the caller guarantees a free slot), the FTR→ITEM1 id
conversion, the per-shop-level sales caps' values, and the
five-slot queue itself (owned by the sibling `special_delivery`
module).

C ABI (all `pc_`-prefixed, 8 exports): `pc_cl_check_collect_bit`,
`pc_cl_completed`, `pc_cl_selected_index`, `pc_cl_order_slot`,
`pc_cl_order_action`, `pc_cl_order_check_msg`, `pc_cl_money_check`,
`pc_cl_sell_price_wallet`. No C callers rewired yet, per the usual
pattern. No test runs, per the standing rule.

### Runtime Port Progress: Able Sisters Design Patterns

New `rust/src/design.rs`, registered as `mod design` in
`rust/src/lib.rs`. Decomp-verified against `m_needlework.h`,
`m_needlework.c`, `m_design_ovl.c`, `m_design_ovl.h`,
`m_cporiginal_ovl.h`, `m_private.h`, `m_common_data.h`,
`ac_needlework_indoor.c`, `ac_npc_needlework_talk.c_inc`,
`ac_npc_needlework.h`, `ac_flag.c` (GAFE01_00 Rev 0 USA). All 12
brief claims checked; no behavioral corrections. The brief's open
`flag_design_set` question is CLOSED by a full use-site search: the
byte at record offset 0x011 is a "custom design applied" marker
whose only live consumer is the island flagpole (set on apply,
tested at render); it is dead state on player/shop records in
retail — preserved, never invented for.

Ported (pure record/texture/pixel/color kernels; the drawing input,
the rendering, the submenu machines, the NPC dialogue, the shop
actor's double-buffer handoff, the ARAM/ROM default loading, and
the save records stay in C):

- The record layout constants (0x220 record, 0x20 texture offset,
  512-byte 4-bit texture).
- The tile-aware `mDE_POS2TEXEL` mapping with the verified nibble
  order (odd x = low nibble), plus pixel get/set that preserve the
  sibling nibble.
- The RGB5A3→RGB8 conversion with the exact integer math (opaque
  `(ch*255)/31`, transparent `ch*17`; alpha not converted).
- The GBA nibble swap (self-inverse) and the `& 15` palette mask.
- The blank-pattern literal (full 16-char `"blank           "`),
  the player/shop default palette tables, and the display-order
  resolution through `my_org_no_table`.
- The 350-Bell design price, Sable's visit-day counter rule (new
  date only, non-foreign, cap 10), the shop cloth/umbrella slot
  split, the 8×12 card-folder constants, and the editor
  mode/tool ids.

Deliberately NOT ported: the tool/controller behavior, the
rendering, the submenu and dialogue state machines, the shop
actor's buffered copy/exchange handoff, the worn-design branch,
and `ac_broker_design.c` (a furniture broker actor, not the
pattern editor).

C ABI (all `pc_`-prefixed, 10 exports): `pc_ds_pos2texel`,
`pc_ds_pixel_get`, `pc_ds_pixel_set`, `pc_ds_nibble_swap`,
`pc_ds_rgb5a3_to_rgb8`, `pc_ds_palette_mask`,
`pc_ds_visit_next_days`, `pc_ds_shop_slot_kind`,
`pc_ds_shop_slot_idx`, `pc_ds_resolve_slot`. No C callers rewired
yet, per the usual pattern. No test runs, per the standing rule.

### Runtime Port Progress: Town Tune Composer

New `rust/src/melody.rs`, registered as `mod melody` in
`rust/src/lib.rs`. Decomp-verified against `m_melody.c`,
`m_melody.h`, `m_mscore_ovl.c`, `ac_mscore_control.c`,
`ac_mscore_control.h`, `m_name_table.h`, `m_bgm.c`,
`m_start_data_init.c` (GAFE01_00 Rev 0 USA). All 12 brief claims
checked; no behavioral corrections.

Ported (pure encoding and transition kernels; the overlay input,
the audio calls, the rendering, the actor checks, the submenu
lifecycle, the GBA state machine, and the save stay in C):

- The packed-u64 ↔ 16-step conversions with the exact nibble order
  and low-nibble masking.
- The literal default tune and the all-HOLD clear action.
- The asymmetric C-up/C-down transitions with the exact sound
  behavior (0→15 silent, 15→14 plays `SysTrgStart(0)`).
- The cursor advance/back with the index-16 end position, and the
  edit-end save/continue/restore branches.
- The music-board item check, the GBA message/timeout constants,
  and the note names.

Deliberately NOT ported: the overlay input polling, the audio and
rendering, the board facing/interaction checks, the submenu and
dialogue state machines, and the GBA transfer flow. The caller
audit is complete: the composer overlay, the GBA read path, the
NPC/quest melody hooks, and `m_bgm.c`'s hourly time-signal
(instruments 0xFE island / 0xFF mainland) all consume the same
saved u64 — keep it authoritative.

C ABI (all `pc_`-prefixed, 11 exports): `pc_ml_unpack`,
`pc_ml_pack`, `pc_ml_clear`, `pc_ml_step_up`,
`pc_ml_step_up_sound`, `pc_ml_step_down`,
`pc_ml_step_down_sound`, `pc_ml_cursor_advance`,
`pc_ml_cursor_back`, `pc_ml_edit_end`, `pc_ml_is_music_board`. No C
callers rewired yet, per the usual pattern. No test runs, per the
standing rule.

### Runtime Port Progress: Museum Building & Exhibit Actors

New `rust/src/museum_display.rs`, registered as `mod museum_display`
in `rust/src/lib.rs`. Decomp-verified against
`m_museum_display.h`, `m_museum_display.c`, `ac_museum_fossil.c`,
`ac_museum_picture.c`, `ac_museum_fish.c`, `ac_museum_insect.c`,
`ac_museum_indoor.c`, `ac_museum.c`, `m_name_table.h`,
`m_room_type.h` (GAFE01_00 Rev 0 USA). All 12 brief claims checked;
no behavioral corrections.

Ported (pure state/table/mapping kernels; species simulation,
rendering, camera, player rotation, and the save stay in C):

- The 63-byte nibble-packed display record: even/odd nibble
  read/write, bounds-checked queries, the donor predicates.
- The fossil floor-placement table (25 entries) and the
  donated-fossil item formula (`0x1EEC + (idx << 2) + (rotation &
  3)`); the dummy items stay C-side symbols.
- The fossil talk-position table (25 entries) — a SEPARATE table
  from the floor placement; never derived from it.
- The 15 artwork positions with the draw offsets and the DIFFERENT
  talk-check offsets plus the 16-entry facing-direction gate.
- The donor-sensitive talk messages (12191/12192/12193) and the
  fish/insect continuation pair (12194/12195).
- The fish tank centers, the aquarium light-power formula, the
  five fish info stations with their resolved species-index lists,
  and the six insect stations with their lists.
- The station filtered-list construction (fixed ordering,
  nonzero-donor filter).
- The room wall alpha transitions (±24, clamped) and the
  camera-state fade sets.
- The exterior: entrance door data, exit rewrite values, the night
  light expression (06:00 boundary — not the police station's
  05:00), and the front-door check.

Deliberately NOT ported: the species-specific fish/insect
simulation, the skeleton animation, the point-light reservation
lifecycle, the rendering and display lists, the camera API, the
player-rotation checks, the shop-sales-adjacent assessment (owned
by the sibling `museum` module), and the save mutations.

C ABI (all `pc_`-prefixed, 10 exports): `pc_md_query`,
`pc_md_bit_store`, `pc_md_talk_msg`, `pc_md_station_msg`,
`pc_md_alpha_on`, `pc_md_alpha_off`, `pc_md_fade_out`,
`pc_md_light_power`, `pc_md_museum_light_on`,
`pc_md_fossil_display_item`. No C callers rewired yet, per the
usual pattern. No test runs, per the standing rule.

### Runtime Port Progress: Shop Interiors & Fitting Rooms

New `rust/src/shop_interior.rs`, registered as `mod shop_interior`
in `rust/src/lib.rs`. Decomp-verified against
`ac_shop_design.c`, `ac_shop_manekin.c`, `ac_shop_umbrella.c`,
`ac_shop_indoor.c`, `ac_shop_level.c`, `ac_shop_goods.c`,
`ac_shop_goods.h`, `m_name_table.h`, `m_shop.h` (GAFE01_00 Rev 0
USA). Two brief corrections carried: saplings and
Halloween-status flower bags map to `RSV_SHOP_HALLOWEEN`, not
`RSV_SHOP_PLANT`; and the carpet default at index 3 (DEPART_1F) is
`ITM_CARPET70`, not `ITM_CARPET68`.

Ported (pure mapping/state kernels; rendering, texture/DMA,
collision registration, the actor engine callbacks, and the save
stay in C):

- All 26 reserved shop IDs (`RSV_SHOP_*` 0xFE00-0xFE18 plus
  `RSV_FE1F`).
- The item→reserved-slot classification, verbatim, including the
  source-commented unreachable signboard check inside the tool
  range and the sold-sentinel round-trips.
- The sold-slot routing: per-category route, goods-list sentinel,
  and field-ground write, with the retail quirks preserved (sold
  clothing's FG write is `RSV_NO`, not `RSV_SHOP_SOLD_CLOTH`;
  stationery takes the bare `return TRUE` path with no report and
  no write; grab bags report and write immediately).
- The mannequin counting predicate (clothing OR the
  `RSV_SHOP_SOLD_CLOTH` sentinel), the clothing/naked texture
  indices, the `(count+1)` bank sizes, the coordinate normalization
  with first-match search semantics.
- The umbrella state machine (WAIT/DELETE/NONE), the 0.001f
  delete step, the `item - ITM_UMBRELLA_START` model index, and a
  draw-scale accessor that preserves the retail first-record
  behavior verbatim.
- The five indoor room types, the wall/carpet defaults, and the
  texture-bank alternation.
- The lottery drum positions per shop index (the 5th entry is
  unreachable — DEPART_2 keeps `actor_specific = -1`).
- The shop-building scan bounds (`[SHOP0, SHOP3+1]`, with the
  source's own `@BUG` comment that the upper bound includes the
  post office) and the three renewal-date exclusions (Nook sale
  within 2 days, lottery day within 2 days, Black Friday).
- The thrown-item display pool size (`aSG_SINGLE_DRAW_NUM` = 9).

Deliberately NOT ported: merchandise rendering and display
lists, the drop-animation physics and sound, the wall/carpet
texture copying and DMA, the mannequin/umbrella collision
registration and teardown, the shop-level actor's field-ground
rewrite iteration and lost-and-found/deposit side effects (the
C caller supplies the items; Rust only answers the bounds and
constants), and the save mutations.

C ABI (all `pc_`-prefixed, 11 exports): `pc_si_item_to_slot`,
`pc_si_sold_route`, `pc_si_sold_sentinel`,
`pc_si_sold_fg_write`, `pc_si_is_mannequin_spot`,
`pc_si_umbrella_delete_step`, `pc_si_umbrella_draw_scale`,
`pc_si_alternate_bank`, `pc_si_drum_pos`,
`pc_si_is_shop_building`, `pc_si_renewal_blocked`. No C callers
rewired yet, per the usual pattern. No test runs, per the
standing rule.

### Runtime Port Progress: Player House Interiors

New `rust/src/house_interior.rs`, registered as `mod house_interior`
in `rust/src/lib.rs`. Decomp-verified against `ac_my_indoor.c`,
`ac_my_indoor.h`, `ac_house_clock.c`, `ac_house_clock.h`,
`ac_arrange_room.c`, `ac_my_house.c`, `ac_arrange_ftr.c`,
`m_scene_table.h`, `m_name_table_floor_decl.h`, `audio_defs.h`
(GAFE01_00 Rev 0 USA). All 10 brief claims checked; 0 DIFFERS.

Ported (pure state/table/mapping kernels; actor construction,
rendering, texture DMA, collision, and the save stay in C):

- The scene→room-kind table: six kinds, exactly 10 entries,
  first match wins, fallback S. (No LL2-basement or second-floor
  scene exists — the brief's phrasing overstated the table.)
- The texture-bank geometry: `aMI_PAL_SIZE`/`aMI_TEX_SIZE` and the
  derived wall/floor bank sizes, two banks each, with the
  both-banks initial load.
- The floor-sound selection: basement → 82, `0 <= floor_num <
  71` → floor_num, otherwise 83 (`FLOOR_ORIGINAL_SE`).
- The wallpaper/flooring validation: valid at `[0, 67)` or an
  original design; invalid resets to 0.
- The room step flags: M/L/LL1 + basement → step 0; LL1 →
  step 1; LL2 → step 2; floor-no 2 → step 3.
- The clock model/position selection: 9 clock types with their
  hand-joint indices, the 9-entry scene→position table, the
  120-frame phase accumulator with the verbatim
  `aHC_CrossedFrame` semantics, the 15/45 chime test, the 0.5f
  animation speed, the RTC-driven hand angles (`0x4000 -
  rad_hour/rad_min`), the chime sound ID `NA_SE_12D` (0x12D),
  and the raffle-hides-shop-clocks rule.
- The arrange-room packed-index extraction: wall = high byte,
  floor = low byte, BOTH clamped to 0..0x42 (the brief suggested
  the wall range differed — it doesn't).
- The exterior door action states (four, with the source's own
  `SCEEN` typo documented), the entry door data per house size,
  and the ±48.29f exit offsets.

Deliberately NOT ported: `ac_arrange_ftr.c` (an empty actor stub
— nothing to port), the keyframe/DMA/texture/display-list
rendering, the furniture actor lifecycle (owned by the sibling
`furniture` module and C), the furniture profile callbacks, the
clock's `rad_hour`/`rad_min` computation (in the time system),
and save mutations.

C ABI (all `pc_`-prefixed, 10 exports): `pc_hi_room_kind`,
`pc_hi_floor_se`, `pc_hi_validate_index`, `pc_hi_room_steps`,
`pc_hi_clock_pos`, `pc_hi_crossed_frame`, `pc_hi_clock_chime`,
`pc_hi_clock_hidden`, `pc_hi_arrange_extract`,
`pc_hi_entry_start`. No C callers rewired yet, per the usual
pattern. No test runs, per the standing rule.

### Runtime Port Progress: Event-Day NPCs and Controllers

New `rust/src/event_day.rs`, registered as `mod event_day` in
`rust/src/lib.rs`. Decomp-verified against `m_event.h`,
`m_event_schedule.c_inc`, `ac_event_manager.c`,
`m_event_map_npc.c`, `m_event_map_npc_data.c_inc`,
`ac_ev_turkey.c`, `ac_tokyoso_control.c`,
`ac_groundhog_control.c`, `ac_ev_santa.c`, `lb_rtc.h`
(GAFE01_00 Rev 0 USA). Every claim PROVEN; one count corrected
(the schedule table has 134 rows, not 137+).

Ported — Layer A only (pure data and helpers). The calendar
scheduler, event-manager placement, NPC registration, and all
NPC actor state machines stay in C:

- The event-ID encoding: 3-bit type / 29-bit subtype with the
  verbatim get/set semantics. The source's `mEv_SET` quirk
  (uses `mEv_SET_TYPE(subtype)`) is inert and resolved to
  VALUES — small subtypes land at `subtype << 29`.
- The schedule date-field codecs: weekly data, lunar month,
  save-month, hour-slot, and the TODAY/MULTIDAY flag bits.
- The 14 decomp-verified schedule rows as raw encoded data
  (the full 134-row table stays in C; date decoding stays in C).
- The special-visitor→NPC-kind mapping, verbatim — including
  the carpet peddler's kind being `mEv_SPNPC_ARABIAN` and the
  `default:` fall-through to SHOP.
- The 17-entry event-map type table (IDs only; index 6
  Halloween has no fixed data — dynamic naming), the
  `SP_NPC_EV_HALLOWEEN_0` (0xD020) naming rule, and the New
  Year layout positions with the NPC mask.
- Franklin's present logic: `PRESENT_NUM` 12, `ALL_BITS`
  0xFFF, the reset predicate, and the n-th-ungiven mapping
  (the RANDOM pick stays in C).
- The tokyoso controller states (nine, with the source's own
  `STRECH` typo carried), and Jingle's four wish categories.
- Tortimer's 30 `SONCHO_*` event IDs as a documented name list
  with the four index-verified pairings.

Deliberately NOT ported: the daily event-list construction and
active/inactive transitions, the event-manager block/unit
search and actor placement, NPC registration and
participation, the special-visitor schedule/save mutations, the
Jack/Franklin/Jingle/Tortimer/Wisp talk and think state
machines, the activity controllers' runtime behavior, and save
mutations.

C ABI (all `pc_`-prefixed, 7 exports): `pc_ed_get_type`,
`pc_ed_get_subtype`, `pc_ed_weekly_data`,
`pc_ed_special_type2kind`, `pc_ed_halloween_npc_name`,
`pc_ed_franklin_all_given`, `pc_ed_franklin_nth_ungiven`. No C
callers rewired yet, per the usual pattern. No test runs, per
the standing rule.

### Runtime Port Progress: New Year Countdown

New `rust/src/countdown.rs`, registered as `mod countdown` in
`rust/src/lib.rs`. Decomp-verified against `ac_count02.c`,
`ac_count02_move.c_inc`, `ac_countdown.c`,
`ac_countdown_move.c_inc`, `ac_countdown_clip.h`,
`ac_countdown_npc0.c`, `ac_countdown_npc0_talk.c_inc`,
`ac_countdown_npc1.c`, `ac_countdown_npc1_talk.c_inc`,
`m_event_map_npc_data.c_inc`, `m_time.h` (GAFE01_00 Rev 0 USA).
All 10 brief claims PROVEN.

Ported (pure timer/state kernels; the NPC talk/demo logic, the
digit-transition animation, the cracker tool actor, and the save
stay in C):

- The countdown calculation (`aCOU_setNowTime`): December from
  23:00 → seconds until midnight; other months → 0; December
  before 23:00 → 3599. The day is NOT checked — preserved
  verbatim, not "fixed".
- The MM:SS digit extraction (`aCOU_getTime`).
- The NPC0 phase function (seven phases, strict `>`/`<`
  comparisons) and the SIMPLER NPC1 variant (six phases, no
  AFTER_10_SEC branch).
- All seven countdown layout tables with the eight actor slots
  (the (2, 11) overlap in layout 1 preserved, not "fixed"), the
  five-NPC-slot mask, and the pool block kind.
- The prop action states (WAIT/MOVE/HAPPY_NEW_YEAR), the MOVE
  animation parameters (1.0f → 61.0f @ 0.5f), the happy-counter
  wrap (59 → 0), and the Dec-31-23:59 wait gate.
- The NPC1 movement: 60-unit radius with home-angle projection,
  0.0f/319.0f block-edge offsets (range check BEFORE block
  check), and the before/after-midnight speed triples.
- The NPC1 firing sequence constants: the 23:59:50 gate, loop
  count 2, fire at frame 3.0f (only when drawn), clap loop 43,
  and the four clap sounds indexed by NPC ID.
- The digit-change mask algorithm (most-significant-first,
  four-bit) and the prop-animation request rule (clip present,
  flag unset, arg0 <= 2).

Deliberately NOT ported: the countdown NPC message-number
selection (already in the sibling `talk_request` module), the
talk/demo/forced-talk logic, the digit-transition animation
(it runs in the DRAW callback), the cracker creation and
lifecycle, the block-lifetime deletion (needs the field/demo
state), and save mutations.

C ABI (all `pc_`-prefixed, 9 exports): `pc_cd_set_now_time`,
`pc_cd_get_time`, `pc_cd_term0`, `pc_cd_term1`,
`pc_cd_digit_change_mask`, `pc_cd_request_anime`,
`pc_cd_in_move_range`, `pc_cd_project_radius`,
`pc_cd_layout_pos`. No C callers rewired yet, per the usual
pattern. No test runs, per the standing rule.

### Runtime Port Progress: Halloween Special NPC

New `rust/src/halloween.rs`, registered as `mod halloween` in
`rust/src/lib.rs`. Decomp-verified against
`ac_halloween_npc.c`, `ac_halloween_npc.h`,
`ac_halloween_npc_talk.c_inc`,
`ac_halloween_npc_schedule.c_inc`,
`ac_event_manager.c`, `m_event_map_npc.c`,
`m_event_schedule.c_inc`, `m_name_table.h` (GAFE01_00 Rev 0
USA). All 10 brief claims PROVEN.

This actor is NOT Jack: Jack is the separate Pumpkin event
actor (`ac_ev_pumpkin.c`). This is the special Halloween NPC
layered on the ordinary NPC engine, doing trick-or-treat
interactions. Jack's actor remains research-only.

Ported (pure kernels; the talk state machine, RNG calls, the
schedule callbacks, and the save stay in C):

- The trick-candidate scan (`aHWN_get_trick_type`): FTR0/FTR1
  eligible if not quest and not the two excluded furniture
  (Jack-o'-lantern 295, Jack-in-the-box 305); ITEM1 eligible
  if not a tool and not quest. The pure branch decision
  (`count > 0 && random_f < 0.5`) is ported; the RNG and item
  selection stay in C to preserve call order.
- The trick-type enum (CHG_CLOTH 0 / CHG_ITEM 1).
- All 14 talk states (END_WAIT 13 → `none_proc1`) and the two
  think states (APPROACH / APPROACH_WAIT).
- The forced-talk trigger: horizontal strictly < 70.0f,
  vertical strictly < 20.0f, block coordinates must match.
- The message IDs: greeting 2442, normal talk 2479 + looks,
  first-call continuing 0x098B + looks * 6, item restarts
  (0x098C/098E/098F/0990 + looks * 6 — the scaling lives inside
  `aHWN_restart_msg_win`), clothing-trick completion
  0x098D + looks * 6.
- The tool ranges (net→rod 0x2200-0x2203, golden 0x2239-0x223C),
  candy 0x2806, the trick clothing 0x2411, and EMPTY_NO 0x0000.
- The event-manager spawn constants: SP_NPC_HALLOWEEN 0xD025,
  the contiguous SP_NPC_EV_HALLOWEEN_0..4 (0xD020-0xD024)
  derivation, and the 0x51 event parameter.
- The schedule model: step cap 2, request priority 4, the
  70-unit request arg.
- Also added to `event_day.rs`: the RUMOR_HALLOWEEN event ID
  (48) and the Oct 1 00:00 → Oct 30 23:00 rumor schedule row.

C ABI (all `pc_`-prefixed, 5 exports): `pc_hwn_trick_candidates`,
`pc_hwn_trick_decision`, `pc_hwn_force_talk`,
`pc_hwn_is_tool_range`, `pc_hwn_event_npc_name`. No C callers
rewired yet, per the usual pattern. No test runs, per the
standing rule.

### Runtime Port Progress: Fireworks Show NPCs

New `rust/src/fireworks.rs`, registered as `mod fireworks` in
`rust/src/lib.rs`. Decomp-verified against
`ac_hanabi_npc0.c`, `ac_hanabi_npc0_talk.c_inc`,
`ac_hanabi_npc0_anime.c_inc`, `ac_hanabi_npc1.c`,
`ac_hanabi_npc1_talk.c_inc`, `ac_hanabi_npc1_anime.c_inc`,
`m_event_map_npc_data.c_inc`, `m_event_schedule.c_inc`,
`ac_event_manager.c` (GAFE01_00 Rev 0 USA). All 13 brief claims
PROVEN.

Ported (pure kernels; the fan tool actor, the event-manager
handlers, the talk state machine, and RNG calls stay in C):

- NPC0: the 100.0f radius with home-angle projection, the
  0.0f/319.0f block-edge offsets, the walk-end rule (ANY
  collision flag, not just wall flags, or move_timer > 60),
  the think selection with the attention-gated banzai
  thresholds (0.05f / 0.3f) and the 135° turn test
  (DEG2SHORT_ANGLE2(135.0f) = 24576), and the message bases
  (base[looks] + RANDOM(3)).
- NPC1: the def_angle[4][7] table converted via
  DEG2SHORT_ANGLE into s16 units, the destination one
  field-unit base size (40.0f) from home along the angle,
  the action selection (no attention → fan; otherwise
  `action[(cur == FAN ? 1 : 0) + (npc_idx & 1)]`), the loop
  tables (base {0,1,3,3}, rnd {0.0,2.0,4.0,4.0}), the four
  clap sounds, and the message arithmetic
  (base[looks] + RANDOM(3) + npc_idx * 3).
- The seven pool layouts with the seven actor slots (the
  vendor and stall share coordinates in each layout,
  preserved verbatim), the two rosters (layouts 2/4/7 use
  YOMISE + FIREWORKS_STALL0, the rest use YOMISE2 +
  FIREWORKS_STALL1), the five-NPC mask, and the pool block
  kind.
- The actor IDs: SP_NPC_EV_HANABI_0..4 = 0xD027–0xD02B,
  SP_NPC_EV_YOMISE = 0xD02C, SP_NPC_EV_YOMISE2 = 0xD039.
- Also added to `event_day.rs`: RUMOR_FIREWORKS_SHOW (31),
  SONCHO_FIREWORKS_SHOW (97), and WEATHER_CLEAR (116) with
  their schedule rows (July 4 19:00–20:00 Tortimer, June 25 →
  July 3 23:00 rumor, July 4 00:00–23:00 weather override).

C ABI (all `pc_`-prefixed, 10 exports): `pc_fw_npc0_next`,
`pc_fw_npc0_out_of_range`, `pc_fw_npc0_project_radius`,
`pc_fw_npc1_next`, `pc_fw_npc1_default_angle`,
`pc_fw_npc1_dst_pos`, `pc_fw_layout_pos`,
`pc_fw_layout_roster`, `pc_fw_npc0_msg`, `pc_fw_npc1_msg`. No
C callers rewired yet, per the usual pattern. No test runs,
per the standing rule.

### Runtime Port Progress: Cherry Blossom Festival NPCs

New `rust/src/cherry_blossom.rs`, registered as `mod
cherry_blossom` in `rust/src/lib.rs`. Decomp-verified against
`ac_hanami_npc0.c`, `ac_hanami_npc0_talk.c_inc`,
`ac_hanami_npc0_anime.c_inc`, `ac_hanami_npc1.c`,
`ac_hanami_npc1_talk.c_inc`, `ac_hanami_npc1_anime.c_inc`,
`m_event_map_npc_data.c_inc`, `m_event_schedule.c_inc`
(GAFE01_00 Rev 0 USA). All 13 brief claims PROVEN.

Ported (pure kernels; the talk state machine, the tumbler and
fan tool actors, the positional sound, and RNG calls stay in
C):

- NPC0: the action selection with the parity rule
  (`idx = RANDOM(2); idx <<= (event_idx & 1)` → even events
  choose WAIT/MERRY, odd choose WAIT/DRINK), the loop tables
  (base {1,2,1}, rnd {2.0f,3.0f,1.0f}), the four clap sounds,
  the four initial angles via DEG2SHORT_ANGLE2, the talk
  message arithmetic (base[looks] + RANDOM(3) + event_idx *
  3), and the tumbler condition (odd event_idx and a NULL
  right-hand actor).
- NPC1: the seven-entry initial angle table via
  DEG2SHORT_ANGLE2 indexed by the pool layout, the think
  selection (next_action wins; otherwise strict 135° turn
  test, walk on no wall flags, else turn), the 100.0f squared
  radius (strict >) with home-angle projection, the 0.0f/319.0f
  block-edge clamps, the walk-end rule (ANY collision flag or
  move_timer > 60), and the talk message arithmetic
  (base[looks] + RANDOM(3), confirmed with no event_idx
  offset).
- The festival layout: eight slots (five hanami NPCs, the two
  Sakura tables, SONCHO2) with their field-unit coordinates, a
  single map variant on a shrine block, the five-NPC mask, and
  the IDs (SP_NPC_EV_HANAMI_0..4 = 0xD032–0xD036,
  SP_NPC_EV_SONCHO2 = 0xD074, SAKURA_TABLE0 = 0x582A,
  SAKURA_TABLE1 = 0x582B).
- Also added to `event_day.rs`: CHERRY_BLOSSOM_PETALS (65)
  with the April 3 00:00 → April 8 23:00 schedule row — a
  distinct event from the festival itself.

C ABI (all `pc_`-prefixed, 10 exports): `pc_cb_npc0_next`,
`pc_cb_npc0_msg`, `pc_cb_npc0_def_angle`,
`pc_cb_npc0_make_tumbler`, `pc_cb_npc1_next`,
`pc_cb_npc1_msg`, `pc_cb_npc1_def_angle`,
`pc_cb_npc1_out_of_range`, `pc_cb_npc1_project_radius`,
`pc_cb_layout_pos`. No C callers rewired yet, per the usual
pattern. No test runs, per the standing rule.

### Runtime Port Progress: Harvest Festival NPCs

New `rust/src/harvest.rs`, registered as `mod harvest` in
`rust/src/lib.rs`. Decomp-verified against
`ac_harvest_npc0.c`, `ac_harvest_npc0.c_inc`,
`ac_harvest_npc1.c`, `ac_harvest_npc1.c_inc`,
`m_event_map_npc_data.c_inc`, `m_event_schedule.c_inc`,
`ac_event_manager.c`, `ac_ev_turkey.c` (GAFE01_00 Rev 0 USA).
All 12 brief claims PROVEN.

Ported (pure kernels; Franklin's save/common area and talk
state machine, the tumbler tool actor, the event-manager
handlers, and RNG calls stay in C):

- NPC0: the think selection (`_998 != -1` → stored action,
  otherwise `RANDOM(2) << (_9A4 & 1)` → even raw indices
  choose WAIT/MERRY, odd choose WAIT/DRINK), the loop tables
  (base {1,2,1,1}, rnd {0.0f,3.0f,1.0f,0.0f}), the four clap
  sounds, the default angle via the masked index
  (`raw_idx & 3` → {45°,270°,315°,90°}), the talk message
  arithmetic (base[looks] + raw_idx * 3 + RANDOM(3)), and the
  tumbler condition (odd raw index and a NULL right-hand
  actor). The raw index and the masked angle index are kept
  distinct — the fifth NPC (raw 4) has even parity but angle
  index 0.
- NPC1: the think (stored slot wins and clears; otherwise NO
  action selection — only flag re-application, unlike NPC0),
  the to-default handler (queues WAIT when the shape rotation
  matches the zero default angle), the creation-time
  `world.position.x += 20.0f` constant, and the talk message
  arithmetic (base[looks] + RANDOM(3), no event-index offset).
- The festival layout: nine slots (five harvest NPCs,
  Tortimer, three tables) with their field-unit coordinates, a
  single map variant on a shrine block, the five-NPC mask,
  and the IDs (SP_NPC_EV_HARVEST_0..4 = 0xD082–0xD086,
  SP_NPC_EV_SONCHO2 = 0xD074, HTABLE0..2 = 0x5846–0x5848,
  SP_NPC_TURKEY = 0xD08D).
- Also added to `event_day.rs`: RUMOR_HARVEST_FESTIVAL (55),
  HARVEST_FESTIVAL_FRANKLIN (108), the
  `sched::day_after_weekly` codec, the schedule rows (Tortimer
  and Franklin at the same 4th-Thursday window, the Nov 11 →
  day-after-1st-Sunday rumor, and the festival-day weather
  override), and Franklin's full 12-entry present table IDs.

C ABI (all `pc_`-prefixed, 8 exports): `pc_hv_npc0_next`,
`pc_hv_npc0_msg`, `pc_hv_npc0_default_angle`,
`pc_hv_npc0_make_tumbler`, `pc_hv_npc1_think`,
`pc_hv_npc1_to_default`, `pc_hv_npc1_msg`, `pc_hv_layout_pos`.
No C callers rewired yet, per the usual pattern. No test runs,
per the standing rule.

### Runtime Port Progress: Moon Viewing (Tukimi)

New `rust/src/moon_viewing.rs`, registered as `mod moon_viewing`
in `rust/src/lib.rs`. Decomp-verified against `ac_tukimi.c`,
`ac_tukimi_move.c_inc`, `ac_tukimi_draw.c_inc`,
`ac_tukimi_npc0_talk.c_inc`, `ac_tukimi_npc1_talk.c_inc`,
`ac_tukimi_npc1_anime.c_inc`, `m_event_map_npc_data.c_inc`,
`m_event_schedule.c_inc`, `ac_event_manager.c`, `m_bgm.c`,
`m_name_table.h` (GAFE01_00 Rev 0 USA). 11 of 12 brief claims
PROVEN; the 12th DIFFERS: the structure actor is deleted only
when `ABS(tukimi_b - player_b) > 1` on either block axis, not on
a plain block mismatch — an adjacent block keeps the actor.
The brief's plain-mismatch version would delete it too
aggressively.

The key retail fact: `ac_tukimi_npc0`/`ac_tukimi_npc1` are spawned
by the METEOR SHOWER event manager, not the Harvest Moon
Festival manager. The two events share the pool block but keep
distinct event IDs, effects (eEC_EFFECT_NIGHT13_MOON vs.
eEC_EFFECT_SHOOTING_SET — and the meteor stop deletes
eEC_EFFECT_SHOOTING, not _SHOOTING_SET), and music tracks (30
vs. 250); they must not be merged into one Rust implementation.

Ported (pure kernels; NPC lifecycle, skeleton animation,
message resources, RNG calls, and the event-manager handlers
stay in C):

- The structure actor: variant index
  (`npc_id - NEWYEAR_SHRINE0`; valid IDs 0x582F/0x5830, left and
  right), the `id = id == 0 ? 6 : 6` background-offset constant
  (always ID 6, value 100), the 271.0f animation argument,
  DUMMY_TUKIMI (0xF10A), and the ABS-diff->1 deletion rule.
- NPC0: the eight-entry `def_angle` table (0xA000..0) plus the
  seven-entry X/Z offset tables (offsets are ADDED to world
  position), and the Meteor-Shower-gated dialogue formula
  `msg_base[looks] + RANDOM(3)` with both base families
  (0x1EB0/0x3F46). Head target: 40 units forward, 30 units
  BELOW the eye, priority 3, only in aNPC_ACT_SPECIAL.
- NPC1: the 5x7 `def_angle` table indexed by
  `[npc_id - 0xD03F][mFI_GetPuleIdx()]`, the animation index
  (KUISINBO1 only for event_idx == 4, WAIT1 otherwise), the
  fifth-NPC special case (no talk turning, KUISINBO1 default;
  dispatch tests `npc_id == SP_NPC_EV_TUKIMI_4`), the head
  target (40 forward, 30 ABOVE the eye, skipped for index 4),
  and the dialogue (`msg[looks] + RANDOM(3) + event_idx * 3`
  for ordinary indices; separate 0x1EBC/0x3F52 base family and
  no event_idx term for index 4).
- The Meteor Shower pool placement: seven 5-slot layouts
  (SP_NPC_EV_TUKIMI_0..3 = 0xD03F–0xD042, Tortimer fifth),
  mask 0b0011111; layouts 2/4/7 pair with the first roster,
  1/3/5/6 with the second (same pairing as fireworks). Layout
  2's slots 3 and 4 both sit at (13, 2) — preserved verbatim.
  `mFI_GetPuleIdx()` returns `type - 69` (0–6) for pool blocks,
  -1 otherwise, so NPC0's 8th angle entry is unreachable in
  practice.
- Also added to `event_day.rs`: SONCHO_METEOR_SHOWER (93),
  SONCHO_HARVEST_MOON_FESTIVAL (97), the Aug 12 and
  lunar-date schedule rows, plus two corrections from this
  pass — SONCHO_FIREWORKS_SHOW is 92 (not 97) and
  WEATHER_CLEAR is 109 (not 116); the earlier fireworks pass
  had counted raw file lines instead of enum entries. Verified
  by recounting all 120 `enum event_table` entries against
  six already-ported anchors.

C ABI (all `pc_`-prefixed, 10 exports):
`pc_tukimi_should_delete`, `pc_tukimi_npc0_facing`,
`pc_tukimi_npc0_pos_offset`, `pc_tukimi_npc0_message`,
`pc_tukimi_npc1_facing`, `pc_tukimi_npc1_animation_index`,
`pc_tukimi_npc1_is_fifth`, `pc_tukimi_npc1_message`,
`pc_tukimi_npc1_wants_head_target`, `pc_tukimi_ms_layout`.
No C callers rewired yet, per the usual pattern.
No test runs, per the standing rule.

### Runtime Port Progress: First Shrine Visit (Hatsumode)

New `rust/src/hatsumode.rs`, registered as `mod hatsumode` in
`rust/src/lib.rs`. Decomp-verified against
`ac_hatumode_control.c`, `ac_hatumode_control_move.c_inc`,
`ac_hatumode_npc0.c`, `ac_hatumode_npc0_talk.c_inc`,
`ac_hatumode_npc0_schedule.c_inc`,
`ac_hatumode_clip_move.c_inc`, `ac_hatumode_clip_think.c_inc`,
`ac_shrine.c`, `m_event_map_npc_data.c_inc`,
`m_event_schedule.c_inc`, `ac_event_manager.c` (GAFE01_00 Rev 0
USA). All 10 brief claims PROVEN, with brief-missed nuances
folded into the port.

Ported (pure kernels; actor lifecycle, the NPC clip,
message/demo ownership, RNG calls, player-action requests,
shrine animation, the global clip, save-area allocation, and the
`mSP_money_check`/`mSP_get_sell_price` debit semantics stay in
C):

- The controller dispatch constants: the 50-Bell price, messages
  4396/4398, continuations 0x112D/0x112E, the `flags0 & 0x8000`
  wait gate, and the `flags0 &= ~0x8000` clear at prayer end
  (brief-missed). The money check runs only after the speech
  demo is active.
- Shrine-relative positions with the confirmed [X, Z, Y] layout
  of `yashiro_pos[3]`: controller home (20+yp[0], yp[2],
  40+yp[1]); prayer/offering (20+yp[0], yp[2], 85+yp[1]).
- Dialogue: the `(4 + (npc_id - 0xD058) - cur_npc_idx) & 3`
  group formula (bitwise AND, not modulo), the {+12,+3,+6,+9}
  group offsets, the `base+15+RANDOM(3)` branch, and the
  offering-talk continuations (base+1 with next_think 0x22 vs.
  base+2).
- Routes: the 5-entry start-offset tables (+180° orientation),
  the pre-increment wrap (`root++`, then 10→5, 5→0 — not plain
  modulo), and the brief-missed `aHN0_posX[9]`/`aHN0_posZ[9]`
  destination tables with the 10-entry root remap.
- Flag handshakes: the exact wait masks (think 3 →
  `0x01 << (idx&1)`; 13 → `0x04 << ((idx&1)^1)`; 28 →
  `0x10 << ((idx&1)^1)`), consumed on read, plus think 28's
  0x4000 detour to think 43.
- The 8-slot New Year shrine spawn layout (HATUMODE_0..4 =
  0xD058–0xD05C, MIKO = 0xD03D, NEWYEAR_TABLE = 0x5831,
  SONCHO2), mask 0b00011111.
- Also added to `event_day.rs`: RUMOR_NEW_YEARS_DAY (0) with
  the Dec 25–31 rumor row, and the SONCHO_NEW_YEARS_DAY (80)
  Jan 1 06:00–10:00 row (the NEW_YEARS_DAY row already
  existed and matches the verified window).

Deliberately NOT ported as retail behavior: the
`root_table`/`proc_idx_table` 4-entry tables are exposed with
only 4 valid indices — 5 HATUMODE NPCs spawn, so index 4 is an
open retail out-of-bounds question that needs
assembly-level confirmation; the helper reports invalid instead
of reproducing the read.

C ABI (all `pc_`-prefixed, 19 exports):
`pc_hatsumode_offer_msg`, `pc_hatsumode_choice_continue`,
`pc_hatsumode_choice_state`, `pc_hatsumode_wait_ready`,
`pc_hatsumode_inori_end_flags`, `pc_hatsumode_ctrl_home`,
`pc_hatsumode_act_pos`, `pc_hatsumode_msg_group`,
`pc_hatsumode_make_msg`, `pc_hatsumode_talk_msg`,
`pc_hatsumode_saisen_continue`, `pc_hatsumode_start_offset`,
`pc_hatsumode_start_root`, `pc_hatsumode_ready2`,
`pc_hatsumode_next_root`, `pc_hatsumode_next_pos`,
`pc_hatsumode_wait_mask`, `pc_hatsumode_wait_advance`,
`pc_hatsumode_spawn_pos`. No C callers rewired
yet, per the usual pattern. No test runs, per the standing
rule.

### Runtime Port Progress: Tug of War (Tunahiki)

New `rust/src/tunahiki.rs`, registered as `mod tunahiki` in
`rust/src/lib.rs`. Decomp-verified against
`ac_tunahiki_control.c`, `ac_tunahiki_npc0.c`,
`ac_tunahiki_npc0_schedule.c_inc`,
`ac_tunahiki_npc0_talk.c_inc`, `ac_tunahiki_npc1.c`,
`ac_tunahiki_npc1_schedule.c_inc`,
`ac_tunahiki_npc1_talk.c_inc`,
`include/ac_tunahiki_control.h`, `m_event_map_npc_data.c_inc`,
`m_event_schedule.c_inc`, `ac_event_manager.c` (GAFE01_00 Rev 0
USA). All 8 brief claims PROVEN.

The design: one controller owns the shared rope simulation
(`aEv_tunahiki_c`, 0x18 bytes); NPC1 participants animate from
it and feed pull speeds back through the shared save area.

Ported (pure kernels; the event save area, actor lifecycle,
rope actor, NPC clip, animation frame-control, RNG calls,
effects, audio, and demo/talk stay in C):

- The full `aTNC_wait` controller update: ±0.3 shake from
  `game_frame & 1` while SHAKE is set, the TALK_ALL freeze,
  `rope_base += speed * 0.025`, the strict ±10 clamp with
  speed zeroing, `rope = rope_base + shake`, and the counter
  reset to 2 (not 0) at 65.
- The `aTNN1_aiko` deterministic parts: the speed-selection
  chain (`rng < 0.45` → `mag+3`; `< 0.9` → `-(mag+3)`; else 0 —
  RNG values supplied by the C caller), the footstep condition
  (`speed * dir > 8.0`), and the end-state decision
  (`-(base*dir) > 5` → 2; `< -5` with `next_speed*dir >= 0` →
  3; else 1, keeping the zero-speed equality on branch 3).
- ID decomposition: index, member bit, the raw team mask (0 or
  2 — not normalized), and the delete/talk flag shifts.
- Team direction (`(npc_id - TUNAHIKI_1) & 2`), the home
  offsets (dir×95 for ID indices 2–3, else dir×85; z dir×5),
  and the `home.x + rope_base` follow rule (not `rope`).
- Dialogue arithmetic (`base + msg_no + rand3`; NPC0 msg_no 0,
  NPC1 3/6/9) and the 6-slot shrine placement
  (TUNAHIKI_0..4 = 0xD05F–0xD063, Tortimer (10, 8)), mask
  0b0011111.
- Also added to `event_day.rs`: the two verified tug-of-war
  windows (Mar 19 15:00 → Mar 21 16:00, Sep 21 15:00 → Sep 23
  16:00; plain date ranges, no rumor row — the fair-level
  rumors cover it).

Deliberately NOT ported as retail behavior: the constructor
never initializes speed/next_speed/counter (their values come
from the save-area allocator — no zero-init added); several
NPC1 paths dereference the save area with no null check (only
NPC0's destructor guards), so the port preserves the
call-order assumptions; the redundant second save-area fetch
in `aTNC_wait` is folded (documented); the `#ifdef TARGET_PC`
tick loop in `aTNC_actor_move` is PC-port, not retail.

C ABI (all `pc_`-prefixed, 15 exports):
`pc_tunahiki_npc_index`, `pc_tunahiki_member`,
`pc_tunahiki_team_mask`, `pc_tunahiki_delete_flag`,
`pc_tunahiki_talk_flag`, `pc_tunahiki_dir`,
`pc_tunahiki_home_offset`, `pc_tunahiki_follow_x`,
`pc_tunahiki_rope_step`, `pc_tunahiki_counter_step`,
`pc_tunahiki_next_speed`, `pc_tunahiki_footstep`,
`pc_tunahiki_next_think`, `pc_tunahiki_msg`,
`pc_tunahiki_spawn_pos`. No C callers rewired
yet, per the usual pattern. No test runs, per the standing
rule.

### Runtime Port Progress: Morning Aerobics (Taisou)

New `rust/src/taisou.rs`, registered as `mod taisou` in
`rust/src/lib.rs`. Decomp-verified against
`ac_taisou_npc0.c`, `ac_taisou_npc0_talk.c_inc`,
`ac_taisou_npc0_anime.c_inc`, `include/ac_taisou_npc0.h`,
`include/audio.h`, `m_event_map_npc_data.c_inc`,
`m_event_schedule.c_inc`, `ac_event_manager.c`,
`ac_npc_anim_def.h` (GAFE01_00 Rev 0 USA). All 10 brief claims
PROVEN.

Ported (pure kernels; the radio counter, RNG call order, NPC
clip lifecycle, the leader/group globals, animation init,
demo/talk, Tortimer's radio-card inventory mutations, and the
event-manager handlers stay in C):

- The failure arithmetic: the five rates {0.0, 0.05, 0.1, 0.15,
  0.3} with retail's `> 4 → 0` type fallback, the
  `(measure + 1) % 10` next action, the eligibility predicate
  (no failure actor designated, next action neither 0 nor 9,
  float below rate), and the `1 + RANDOM(8)` rejection
  predicate (the re-roll loop stays in C).
- The 13-entry action→animation table with numeric IDs
  (INTRO1_A 81 … TIRED1 105) plus the `(anime_dir & 1)`
  variant bit.
- All ten segment start-ratio tables (varying lengths
  3/5/3/5/3/5/5/5/5/2), the end/morph ratios, the ratio
  lengths, and the `aTS0_set_current` frame-sync math. The
  taisou6 array's unsorted `{0.0, 0.5, 1.0, 1.5, 0.96875}`
  order is preserved as written.
- The delay math: NPC 0 never gets the RANDOM(48) offset; the
  per-step 0.2/0.8 ratio adjustment; the over-block test
  against `startRatio[seg][seq + 1]` (the NEXT boundary).
- Dialogue: row 0 while Morning Aerobics is active (row 1 for
  the Sports Fair variant), Copper's {9829, 9832} bases, the
  villager `base + type*3 + rand2` formula with the fixed +2
  for action == 10, the 180°/0° init-angle rule, the
  smallest-ID leader selection, and the 7-slot shrine layout
  (TAISOU_0..4 = 0xD04E–0xD052, radio 0x582C, Tortimer 0xD078).
- Also added to `event_day.rs`: RUMOR_MORNING_AEROBICS (33)
  with the Jul 14–24 rumor row and TALK_MORNING_AEROBICS (34)
  with the Jul 26–Aug 30 row (both MULTIDAY-encoded, appended
  at the end; the Jul 25–Aug 31 06:00 row already existed).

C ABI (all `pc_`-prefixed, 18 exports):
`pc_taisou_failure_rate`, `pc_taisou_next_action`,
`pc_taisou_failure_eligible`, `pc_taisou_failure_reject`,
`pc_taisou_anim_id`, `pc_taisou_anim_variant`,
`pc_taisou_start_ratio`, `pc_taisou_sync_frame`,
`pc_taisou_has_delay`, `pc_taisou_delay_step`,
`pc_taisou_over_block`, `pc_taisou_msg_row`,
`pc_taisou_npc_type`, `pc_taisou_villager_msg`,
`pc_taisou_copper_msg`, `pc_taisou_init_angle`,
`pc_taisou_leader_update`, `pc_taisou_spawn_pos`. No C callers
rewired yet, per the usual pattern. No test runs, per the
standing rule.

### Runtime Port Progress: Fishing Tourney (Turi)

New `rust/src/fishing_tourney.rs`, registered as
`mod fishing_tourney` in `rust/src/lib.rs`. Decomp-verified
against `ac_turi.c`, `ac_turi_npc0.c`,
`ac_turi_npc0_move.c_inc`, `ac_turi_npc0_anime.c_inc`,
`ac_ev_angler.c`, `ac_ev_angler_move.c_inc`,
`ac_turi_clip.c_inc`, `m_fishrecord.c`, `m_event_schedule.c_inc`,
`m_event_map_npc_data.c_inc`, `ac_event_manager.c`
(GAFE01_00 Rev 0 USA). All 7 sections PROVEN with one
DIFFERS-level correction: the stand's self-deletion test is
`ABS(block diff) > 1` in either axis, NOT `!=` — adjacent
blocks do NOT delete the stand (same pattern as the
Hatsumode brief fix).

Ported (pure kernels; the structure-actor framework, actor
lifecycle, NPC clip/schedule/demo/talk, the rod tool-actor,
the save/common areas, the record database
insertion/recycling, mail, and the event-manager pool
orchestration stay in C):

- The fish-size formulas: `pc_fish_size_from_rank` with f32
  arithmetic and truncation — LARGE `(50+20·rng²)/2.54`,
  MEDIUM `(30+20·rng)/2.54`, SMALL `(30−20·rng²)/2.54` — plus
  the hour-driven rank branch (`<9` → fixed small, `<15` →
  C draws RANDOM(2), else C draws RANDOM(3)).
- The strict record comparison (`current < submitted` — ties
  do not beat), the host's time gate (`hour < 6 ||
  hour >= 18`), the measurement rank (FISH07 → large, FISH06
  → medium, else small), the item classification (FISH05–07
  → bass), and the 40-entry fish→message-number table with
  its two out-of-order entries (FISH16 → 0x110C, FISH33 →
  0x2FAF) copied verbatim.
- The stand variant (`npc_id − 0x5838`), the corrected
  block-deletion rule, the 5×7 participant facing-angle table
  (degrees converted with C's wrapping `(s16)` semantics, so
  180° yields −32768), the participant dialogue
  (`base + rand3 + idx*3` with the six 0x1F0A-based bases),
  the looks-based animation frame speeds, the entry bit
  (`1 << player_no`), and the clothing/furniture reward
  branch.
- Also added to `event_day.rs`: RUMOR/TALK/SONCHO consts for
  both tournaments (27/28/90 and 52/53/102) and all eight
  schedule rows — every Sunday of June/November 06:00–17:00
  via `weekly_data(7, SUNDAY)`, the May/October rumor rows,
  and the June/November talk rows ending on the last Sunday
  (`weekly_data(6, SUNDAY)`). ROWS 20→28.

Deliberately NOT ported: all RNG call order (including the
extra apparently-unused fqrand() in `mEv_fishRecord_local`
and the RANDOM-before-fqrand order in `mFR_make_NpcRecord`),
PersonalID handling, the five-slot record database, mail,
the stand's 3×3 background-offset tables (structure-actor
territory), the 7 pool layouts (event-manager territory),
and the host's save-area lifecycle (including its
NULL-check-free teardown).

C ABI (all `pc_`-prefixed, 15 exports):
`pc_fish_size_from_rank`, `pc_fish_npc_rank_branch`,
`pc_fish_record_beaten`, `pc_fish_time_open`,
`pc_fish_measure_rank`, `pc_angler_item_type`,
`pc_angler_fish_msgno`, `pc_fish_entry_bit`,
`pc_fish_reward_is_cloth`, `pc_turi_stand_variant`,
`pc_turi_should_delete`, `pc_turi_npc_index`,
`pc_turi_def_angle`, `pc_turi_msg_no`,
`pc_turi_frame_speed`. No C callers rewired yet, per the
usual pattern. No test runs, per the standing rule.

### Runtime Port Progress: Sports Fair Ball Toss (Tamaire)

New `rust/src/tamaire.rs`, registered as `mod tamaire` in
`rust/src/lib.rs`. Decomp-verified against
`ac_tamaire_npc0.c`, `ac_tamaire_npc0_schedule.c_inc`,
`ac_tamaire_npc0_talk.c_inc`, `ac_tamaire_npc1.c`,
`ac_tamaire_npc1_schedule.c_inc`, `ac_tamaire_npc1_talk.c_inc`,
`ac_tamaire_npc0.h`, `ac_tamaire_npc1.h`,
`m_event_map_npc_data.c_inc`, `m_event_schedule.c_inc`
(GAFE01_00 Rev 0 USA). All 13 items PROVEN with two
DIFFERS-level corrections: throw-end timer expiry goes to
KYORO, not KYORO_MAE (confirmed in both the proc and the
dt_tbl); the ball prop's block-deletion test is strict `!=`
(adjacent blocks DO delete it) — the opposite of the turi
stand's `ABS > 1`.

Ported (pure kernels; NPC clip/action requests, think
scheduling, the tool-actor lifecycle including the
asymmetric destructor, animation frame control and
passCheck, all RNG call order, the eEC throw effect, the
shared static team flag with its per-construction reset,
talk/demo, and the map placement stay in C):

- NPC0's route destinations with the TRUE default branch
  (+80/+160, +200/+40, −40/+160, default −160/−40) in s16
  with wrapping, the turn gate (aNPC_ACT_TURN at
  aNPC_ACTION_END_STEP = 0xFF), the walk-end +1/−1
  advance/rollback decision (the rollback is a source-level
  recovery path, not incidental), and the cheer branch
  (CLAP1 67 / BANZAI1 66 from the C-side float).
- NPC1's team-relative origin (z +40 always; x +200 when
  `((id − TAMAIRE_1) & 2) != 0`, else −160), the raw team
  flag bits (1/2, not normalized), the throw team bit
  `((id − TAMAIRE_1) >> 1) & 1`, and the random-move angle
  subtraction with C's f32→s16 truncation (sin_s/cos_s stay
  in C).
- The ball tool table {49, 48, 51, 50} with the verbatim
  index expression (right = `((((id−TAMAIRE_1)>>1)&1)^1)`,
  left = same + 2), the pickup frame targets {15, 27, 39}
  (edge-crossing, never `==`), the re-throw gate (`left !=
  0`), and the throw angle addend (RANDOM(3000) + 12288,
  sample in C).
- Dialogue: NPC0's literal bases {7781, 7793, 7769, 7805,
  7817, 7829}, NPC1's offsets {3, 3, 3, 6, 6, 9}, message =
  base + offset + rand3 (RNG in C).
- The 10-slot shrine layout (TAMAIRE_0..4 = 0xD053–0xD057,
  paired balls/baskets, Tortimer), mask 0b0011111.
- Also added to `event_day.rs`: the two verified ball-toss
  windows (Mar 19 13:00 → Mar 21 14:00, Sep 21 13:00 → Sep
  23 14:00; plain ranges; event 12 already existed; ROWS
  28→30).

Deliberately NOT ported: the shared static team flag and
its per-construction reset (file-static semantics must be
preserved, not moved to a one-time initializer), the
#TARGET_PC-only timer helpers and the `timer_accum` field
(PC-port, not retail), the eEC_EFFECT_TAMAIRE trajectory
and scoring (not in these two files), and the event
origin/pool layout selection.

C ABI (all `pc_`-prefixed, 17 exports):
`pc_tamaire_npc0_destination`,
`pc_tamaire_npc0_turn_advance`, `pc_tamaire_npc0_move_step`,
`pc_tamaire_cheer_anim`, `pc_tamaire_npc1_origin`,
`pc_tamaire_team_bit`, `pc_tamaire_throw_team_bit`,
`pc_tamaire_tool_id`, `pc_tamaire_throw_angle`,
`pc_tamaire_next_move_angle`, `pc_tamaire_pickup_frame`,
`pc_tamaire_throw_again`, `pc_tamaire_talk_msg`,
`pc_tamaire_npc0_msg_base`, `pc_tamaire_npc1_msg_offset`,
`pc_tama_should_delete`, `pc_tamaire_spawn_pos`. No C
callers rewired yet, per the usual pattern. No test runs,
per the standing rule.

### Runtime Port Progress: Field Day Footrace (Tokyoso)

New `rust/src/tokyoso.rs`, registered as `mod tokyoso` in
`rust/src/lib.rs`. Decomp-verified against
`ac_tokyoso_control.c`, `ac_tokyoso_clip.c_inc`,
`ac_tokyoso_npc0.c`, `ac_tokyoso_npc0_schedule.c_inc`,
`ac_tokyoso_npc0_talk.c_inc`, `ac_tokyoso_npc1.c`,
`ac_tokyoso_npc1_schedule.c_inc`, `ac_tokyoso_npc1_talk.c_inc`,
`m_event_map_npc_data.c_inc`, `m_event_schedule.c_inc`
(GAFE01_00 Rev 0 USA). All 11 sections PROVEN with
DIFFERS-level nuances (see below).

Naming note: the request title said "summer festival
stalls," but these files are the Field Day Footrace (the
save area is keyed to mEv_EVENT_SPORTS_FAIR_FOOT_RACE; the
map setup calls it fdd_tokyoso). They are ported as the
footrace, not stalls. Real stall behavior lives under the
Fireworks Show's YOMISE/FIREWORKS_STALL actors — a separate
future commission.

Ported (pure kernels; the controller state machine, the
shared save area at slot 8, NPC clip/action requests,
animation frame control, all RNG call order, the eEC
effects, sounds, talk/demo, and the map placement stay in
C):

- The ID decomposition: index, raw team (0/1), raw role
  (0/2, never normalized), and NPC1's birth role test with
  the brief-missed −1 shift — `aTKC_NPCID2ROLE(npc_id − 1)`
  makes TOKYOSO_0/3/4 spectators and TOKYOSO_1/2 runners.
- The flag-bit shifts (delete/stretch/ready/kansen/goal)
  from the verbatim `NPCIDX2*FLG` macros, including the
  source-misspelled READYLFG.
- The lap-crossing test (odd lap: angle < −4000; even:
  angle > 4000), the goal decision (`lap > 8` with no goal
  → WON if the other has none, else LOSE), the angular
  target with C's truncating f32→s16 conversion, the run
  speed (`(4 + dist/90) − idx·0.5`, −1 when ahead), and the
  run destination — X = origin + dist·cos_s but Z = origin
  − dist·sin_s (the brief said "cos/sin" without the sign).
- The finish-line target with centered `(RANDOM_F(10) − 5)`
  draws (samples in C), the retail run-check (`angle >
  −1000 && angle < 4096` — the BUGFIXES variant differs),
  the ahead test (laps first, then positive angle diff),
  and the warmup destination math.
- Dialogue: NPC0's literal bases {6621, 6637, 6605, 6653,
  6669, 6685}, NPC1's 13-entry offsets
  {14,2,14,4,6,8,8,10,10,12,12,12,12}, message = base +
  offset + rand2 (RNG in C).
- The 6-slot shrine layout (TOKYOSO_0..4 = 0xD02D–0xD031,
  Tortimer), mask 0b0011111.
- Also added to `event_day.rs`: the two verified footrace
  windows (Mar 19 11:00 → Mar 21 12:00, Sep 21 11:00 → Sep
  23 12:00; plain ranges; event 15 already existed; ROWS
  30→32).

Deliberately NOT ported: the `@BUG` double
`setupAction(GO)` (documented; the C keeps it), the
`aTKC_set` gate `(flags & READY_ALL) == 0` (mask EMPTY,
not "incomplete"), the controller's `_00 == 3` move
override, NPC0's destructor ASSIGNING `flags =
DELETE_NPC0` (wipes other bits — not "fixed" to OR),
NPC1's `_00 == 3` self-delete living in the move callback
rather than the destructor, the `aTKC_FLAG_15` bit and the
full `_01` semantics (unresolved), and the eEC effect
trajectories.

C ABI (all `pc_`-prefixed, 22 exports):
`pc_tokyoso_npc_index`, `pc_tokyoso_team`,
`pc_tokyoso_role`, `pc_tokyoso_birth_role`,
`pc_tokyoso_delete_flag`, `pc_tokyoso_stretch_flag`,
`pc_tokyoso_ready_flag`, `pc_tokyoso_kansen_flag`,
`pc_tokyoso_goal_flag`, `pc_tokyoso_lap_advance`,
`pc_tokyoso_goal_decision`, `pc_tokyoso_next_angle`,
`pc_tokyoso_run_speed`, `pc_tokyoso_run_target`,
`pc_tokyoso_finish_target`, `pc_tokyoso_run_check`,
`pc_tokyoso_is_ahead`, `pc_tokyoso_warmup_target`,
`pc_tokyoso_talk_msg`, `pc_tokyoso_npc0_msg_base`,
`pc_tokyoso_npc1_msg_offset`, `pc_tokyoso_spawn_pos`. No C
callers rewired yet, per the usual pattern. No test runs,
per the standing rule.

### Runtime Port Progress: Snowman Building

New `rust/src/snowman.rs`, registered as `mod snowman` in
`rust/src/lib.rs`. Decomp-verified against `m_snowman.c`,
`ac_snowman.c`, `ac_psnowman.c`, `m_event_schedule.c_inc`,
`ac_event_manager.c` (GAFE01_00 Rev 0 USA). All 12
sections PROVEN with DIFFERS-level nuances (see below).

Ported (pure kernels; actor lifecycle, collision
ownership, the player-push demo calls, foreground
placement, save-pointer access, date-growth integration,
quest callbacks, dialogue/demo state, effects/audio, mail
submission, RNG call order, and the event-manager birth
orchestration stay in C):

- The growth step (`aSMAN_calc_scale`): `attr ≤ 3` grows by
  speed, otherwise shrinks at 75%; clamped to [0, 6400];
  normalized = dist/6400; actor scale = normalized·0.02 +
  0.01 capped at 0.03.
- The proportion score from the ACTUAL actor scales (0.01
  + 0.02·normalized — not the normalized values):
  `|head/body − 0.85| ≤ 0.05/0.15/0.25` → PERFECT/GOOD/OK/
  BAD, all boundaries inclusive.
- The message arithmetic: completed-snowman `2209 +
  ((msg_id + slot) % 3) + result·3`, assembly bases
  {2197, 2200, 2203, 2206} + RANDOM(3) (sample in C).
- The reward-index selection with exactly ONE re-roll (no
  loop; samples and ownership check in C), the 12-item
  table in source order (IDs computed from the X-macro
  expansion — verify against the build before wiring),
  and the mail base 0x202 + rnd.
- The 4-byte record packing (exists, trunc(norm·255),
  trunc(norm·255), score), the slot/sub-ID mapping
  (`(id − 0xA008)/3`, `% 3`), the per-day melt step
  (`trunc(0.8·size)` applied `days` times), the lifespan
  check (winter && (sub_id + days) < 3), and the negative-
  day normalization.
- The push-gate predicate (normalized > 0.2, distance <
  norm·20+25, timer saturating at 17 — not 16 — height gap
  in (−10, 25), player moving, speed < 3, |angle| < 10012)
  and the combine gate (different parts, both > 0.2, NOT
  both stationary — the brief said "either").
- The head selection (faster ball jumps; ties go to A).
- Also added to `event_day.rs`: SNOWMAN_SEASON (62) with
  the Dec 25 00:00 → Feb 17 23:00 row (plain range; ROWS
  32→33).

Deliberately NOT ported: the no-return function is
`mSN_ClearSnowmanData`, not `mSN_ClearSnowman` (the brief
mixed them up — documented); the registration gate
(PART0 + COMBINED + HEAD_JUMP + nonzero fg_pos + !DELETE);
the `move_dist` persistence through the event common area
(accessor stays in C); the ball-part selection via the
boolean-indexed `part_tbl`; the parabolic head-jump arc
and landing rotation; the assembly dust effects; the
`@BUG` NULL dereference in the foreground search.

C ABI (all `pc_`-prefixed, 14 exports):
`pc_snowman_scale_step`, `pc_snowman_score`,
`pc_snowman_msg_no`, `pc_snowman_assembly_msg`,
`pc_snowman_reward_index`, `pc_snowman_record_pack`,
`pc_snowman_slot_from_id`, `pc_snowman_sub_id`,
`pc_snowman_melt_step`, `pc_snowman_check_life`,
`pc_snowman_melt_days`, `pc_snowman_push_ok`,
`pc_snowman_combine_ok`, `pc_snowman_head_is_a`. No C
callers rewired yet, per the usual pattern. No test runs,
per the standing rule.

### Runtime Port Progress: Igloo

New `rust/src/kamakura.rs`, registered as `mod kamakura`
in `rust/src/lib.rs`. Decomp-verified against
`ac_kamakura.c`, `ac_kamakura_move.c_inc`,
`ac_kamakura_indoor.c`, `ac_kamakura_npc0.c`,
`m_event_schedule.c_inc`, `ac_event_manager.c`,
`m_npc.c`, `ac_set_npc_manager.c`, `ac_quest_manager.c`,
`ac_quest_talk_greeting.c`, `ac_quest_talk_normal_init.c`
(GAFE01_00 Rev 0 USA). 17/17 sections PROVEN with
DIFFERS-level nuances (see below).

Ported (pure kernels; actor lifecycle, foreground
placement, the door request/demo/wipe, save-area access,
NPC/move-actor creation, quest-manager state, RNG call
order, effects/audio dispatch, the `anim_frame`
accumulator, the steam cadence, and the villager-selection
policy stay in C):

- The entry check (`aKKR_check_player`): strict facing
  interval (24576, 40960) and squared distance < 1600
  from (actor X, actor Z + 40).
- The exit position (`aKKR_rewrite_out_data`): (actor X,
  ground Y, actor Z + 86) — also recording `exit_type = 0`
  and `extra_data = 3`.
- The window-lighting target (`aKKR_ctrl_light`): true
  outside 06:00 inclusive – 18:00 exclusive.
- The candle phase: `((u32)anim_frame − late) % period`
  (34 ring / 24 fire), and the `aKI_MakeSinPointHalf`
  midpoint/cosine interpolation verbatim (3.14f phase).
- The greeting selection: `hello_msg[] = {6367, 6376,
  6358, 6385, 6394, 6403}[looks]`, the money/item
  bitfield, `1 + base + RANDOM(2)` when both flags are set
  (sample in C), BELL_MIN = 3000.
- The furniture trade-list branch: kamakura + rnd ≥ 90 →
  list 19; tent + rnd ≥ 80 → list 23; otherwise the
  C-owned default (samples and lookup in C).
- Also added to `event_day.rs`: the Kamakura schedule row
  Jan 2 10:00 → Feb 23 21:00 (event 3; ROWS 33→34). The
  `event_day` consts and type-table entry already existed.

Deliberately NOT ported: the steam cadence is
`graph_dt_period_elapsed(game, &dt_accum, 8.0f)`, NOT
`game_frame & 7` (the brief was wrong — documented); the
candle phases run on `k_indoor->anim_frame`, NOT
`game->frame_counter` (the brief was wrong —
documented); the chase rate is `0.019532442f *
dt_num_60fps_frames`; the unused `aKI_mochi_anime_data`
table; the `aNPC_ct_data_c` trailing 0; the `mEvMN_GetNpcJointEv`
selection policy; the 0xFF-sentinel save-byte access.

C ABI (all `pc_`-prefixed, 8 exports):
`pc_kamakura_entry_check`, `pc_kamakura_exit_position`,
`pc_kamakura_light_target`, `pc_kamakura_candle_phase`,
`pc_kamakura_sin_point_half`, `pc_kamakura_greeting_msg`,
`pc_kamakura_trade_list`, `pc_kamakura_save_byte_valid`.
No C callers rewired yet, per the usual pattern. No test
runs, per the standing rule.

### Runtime Port Progress: Mushrooms

New `rust/src/mushroom.rs`, registered as `mod mushroom`
in `rust/src/lib.rs`. Decomp-verified against
`m_mushroom.c`, `m_mushroom.h`,
`m_event_schedule.c_inc`, `m_name_table.h` (GAFE01_00 Rev
0 USA). 16/16 sections PROVEN with DIFFERS-level nuances
(see below).

Ported (pure kernels; save-pointer access, the foreground
grid and deposit data, the `mFI_*` queries, RTC access,
all `fqrand()` call order, `mFI_SetFGUpData()`, and the
`lbRTC_*` time helpers stay in C):

- The target count (`mMsr_GetMushroomNum`): hour 8 →
  `5 − min/15`, hour 9 with min < 15 → 1, otherwise 0,
  gated on `active || saved date != today`.
- The time conversions: `Rtc2MushTime` (quarter =
  `min/15`, does not touch `active`) and `Mushtime2Rtc`
  (min = quarter × 15, sec = 0), the quarter-hour
  rounding table {45, 30, 15, 0} (rounds DOWN despite
  the source comment), and the all-zero timestamp check
  (the two init paths — previous day vs current time —
  stay in C).
- Tree eligibility (the 14 accepted ids), the five
  placement offsets {-1, 1, 15, 16, 17}, the boundary
  `ut_x < 15 && ut_z < 15`, and the hierarchical index
  selection (`(fqrand() * count)` as i32, sample in C).
- The two player exclusions as SEPARATE kernels:
  placement excludes the whole X column AND Z row
  (`bx != pbx − 1 && bz != pbz − 1`), cleanup excludes
  only the single player acre.
- Cleanup eligibility (mushroom item AND not buried),
  the clear-all decision (strict `total < clear_num`),
  and the `mMsr_SetShareNum` distribution verbatim: the
  deterministic spread phase, plus one random-
  allocation step per call with the sampled block index
  supplied by C.
- Also added to `event_day.rs`: MUSHROOM_SEASON (48)
  with the Oct 15 00:00 → Oct 25 23:00 row (plain range;
  ROWS 34→35).

Deliberately NOT ported: there is NO mushroom furniture
mechanism — `m_mushroom.c` only ever writes
ITM_FOOD_MUSHROOM, no mushroom-series furniture ids
exist in the decomp, and the mother's mail uses the
mushroom as a food present, not furniture. The pickup
path needs no furniture branch from this system. Also
not ported: the startup cleanup's (0, 0) player acre, its
lack of an event-status check, the column-refill logic,
and the per-row candidate bitfields.

C ABI (all `pc_`-prefixed, 14 exports):
`pc_mushroom_target_count`, `pc_mushroom_rtc_to_time`,
`pc_mushroom_time_to_rtc`, `pc_mushroom_round_quarter`,
`pc_mushroom_is_zero_time`, `pc_mushroom_tree_eligible`,
`pc_mushroom_boundary_ok`, `pc_mushroom_select_index`,
`pc_mushroom_placement_excluded`,
`pc_mushroom_cleanup_excluded`, `pc_mushroom_clearable`,
`pc_mushroom_clear_all`, `pc_mushroom_share_spread`,
`pc_mushroom_share_random_step`. No C
callers rewired yet, per the usual pattern. No test runs,
per the standing rule.

### Runtime Port Progress: Balloon Presents

New `rust/src/balloon.rs`, registered as `mod balloon`
in `rust/src/lib.rs`. Decomp-verified against
`m_fuusen.c`, `m_fuusen.h`, `m_common_data.h`,
`m_play.c`, `ac_fuusen.c`, `ac_fuusen.h`, `ac_balloon.c`,
`ac_balloon.h`, `m_private.c`, `m_name_table.h` (GAFE01_00
Rev 0 USA). 17/17 sections PROVEN with DIFFERS-level
nuances (see below).

Ported (pure kernels; actor lifecycle, the common-data
accessors, all RNG call order, foreground placement and
clip calls, save access, effects/audio dispatch, and the
held-balloon skeleton/animation setup stay in C):

- The spawn-minute predicate (`(min % 5) == 3`, not the
  commonly repeated "ends in 4"), the roll, the failure
  increment (no clamp), the post-success probability,
  and the LOOK_UP +0.25 bonus.
- The wind-bucket mapping (`data_index_data` with the
  sign-extending 0xF000 shift) and the entry-point
  tables: 8 perimeter positions plus the variation
  table, axis choice via `game_frame & 1`, variation =
  sample × |offset| with the table's sign, and
  `type_idx = game_frame % 5`.
- The movement speed, the escape bounds (map edges plus
  the station rectangle), the tree-interception
  thresholds (cedar 484 vs 225), the screen-visibility
  gate, the wall-hit y_offset update, and the
  movement-timer gate.
- The look-up distance gate (640.0), the deletion gate
  (ground + 500), the reward branch (`RANDOM(5) != 0`
  attempts furniture, otherwise/other-fruit on failure),
  and the held-balloon hide predicate (200.0 above the
  player).

Deliberately NOT ported: `escape_timer`/`timer` are f32,
not int (the brief was wrong — documented); the
present-drop check is `escape_timer <= 1554`, not `==`;
`fuwafuwa_cycle` is frame-rate scaled; the NULL guard
checks `item_tree_fruit_drop_proc` while the call uses
`fruit_set_proc` (kept as-is); the little-shake
double-set; the draw callback's present-skip condition;
the failed-roll state staying at CHK_SPAWN vs
PENDING_SPAWN retrying every frame; the rare-list
selection and `mFI_GetOtherFruit()` internals.

C ABI (all `pc_`-prefixed, 17 exports):
`pc_balloon_spawn_minute_ok`, `pc_balloon_roll_ok`,
`pc_balloon_fail_increment`,
`pc_balloon_post_success_percent`,
`pc_balloon_wind_bucket`, `pc_balloon_entry_point`,
`pc_balloon_type_idx`, `pc_balloon_speed`,
`pc_balloon_escape_bounds`,
`pc_balloon_tree_thresholds`,
`pc_balloon_screen_visible`,
`pc_balloon_wall_y_offset`,
`pc_balloon_look_up_dist_ok`, `pc_balloon_delete_ok`,
`pc_present_reward_branch`,
`pc_player_balloon_hide_ok`, `pc_balloon_move_timer_ok`.
No C callers rewired yet, per the usual pattern. No test
runs, per the standing rule.

### Runtime Port Progress: Cockroaches

New `rust/src/cockroach.rs`, registered as `mod
cockroach` in `rust/src/lib.rs`. Decomp-verified against
`m_cockroach.c`, `m_cockroach.h`, `ac_house_goki.c`,
`ac_house_goki.h` (GAFE01_00 Rev 0 USA). 19/19 sections
PROVEN, 0 DIFFERS.

Ported (pure kernels; RTC/`lbRTC_*` calls, house/cottage
save writes, actor lifecycle, collision queries,
`aMR_CLIP` calls, sound/effect creation,
shadow/rendering, RNG call order, and the
`mCkRh_DecideNowGokiFamilyCount` orchestration stay in
C):

- The infestation growth: strictly `day_gap > 6`, then
  the clamped sum of the saved count and — when the
  saved count is already positive — the entire day gap,
  otherwise the gap minus 6.
- The 0–10 clamp, the 0–3 visible-count math (with the
  FALSE-on-clamp return), and the `arrange_idx & 3`
  house index.
- The stomp predicate: player exists, is moving,
  strictly within 9.0f, and exactly at home Y (the
  `aMR_CLIP` and bg-flag conditions stay in C).
- The proximity predicate (strict 60.0f), the WAIT/MOVE
  decision (with the `||` short-circuit), the move-
  target offsets, the retail `RANDOM(1)` jump-angle bug
  (always player angle + 4096), the `& 0xC000` cardinal
  constraint, and the jump roll (0.5/0.2).
- The timer/alpha/shadow steps (0.5/3.5/2.5 × dt), the
  death flicker, and the AWAY/WAIT timer inits.

Deliberately NOT ported: the USA cottage branch uses
the pre-VER_GAFU01_00 path (copies current RTC y/m/d,
no year-zero guard); the cottage add/remove branches
write the count but return FALSE; `aHG_check_threshold`
always returns FALSE (dead doorway branch); the
wall-angle proc is a no-op within 20.0f of the player;
the MOVE transition draws `RANDOM_F(5.0f)` twice; the
saved-infestation-count lifecycle on stomp is untraced
(actor death only decrements the transient visible
count) — this port stays arithmetic-only.

C ABI (all `pc_`-prefixed, 17 exports):
`pc_cockroach_clamp_count`, `pc_cockroach_growth_count`,
`pc_cockroach_visible_count`,
`pc_cockroach_house_index`, `pc_goki_stomp_check`,
`pc_goki_player_near`, `pc_goki_decide_wait_move`,
`pc_goki_move_target`, `pc_goki_jump_angle_offset`,
`pc_goki_wall_cardinal`, `pc_goki_should_jump`,
`pc_goki_timer_step`, `pc_goki_alpha_step`,
`pc_goki_shadow_step`, `pc_goki_death_visible`,
`pc_goki_away_timer`, `pc_goki_wait_timer`. No C
callers rewired yet, per the usual pattern. No test
runs, per the standing rule.

### Runtime Port Progress: Gyroids

New `rust/src/haniwa.rs`, registered as `mod haniwa` in
`rust/src/lib.rs`. Decomp-verified against
`m_all_grow.c`, `m_all_grow_ovl.c`,
`m_all_grow_ovl.h`, `m_haniwa_ovl.c`,
`m_haniwa_ovl.h`, `m_haniwaPortrait_ovl.c`,
`ac_haniwa.c`, `ac_haniwa_move.c_inc`, `ac_haniwa.h`,
`m_home_h.h`, `m_common_data.h`, `m_name_table.h`,
`m_field_info.c` (GAFE01_00 Rev 0 USA). 14/14 sections
PROVEN.

Ported (pure kernels; the `haniwa_scheduled` flag and
RTC handling, foreground/deposit mutations,
`mSP_RandomHaniwaSelect` until `FTR_NO_2_IDX` is
traced, `mMsm_DepositItemBlock_cancel`, inventory
transfers, dialogue/demo callbacks, state transitions,
door requests, saves, rendering/collision, and the
full portrait overlay stay in C):

- The Haniwa item-range check (0x15B0–0x17AB,
  computed from the X-macro expansion — verify against
  the build before wiring).
- The storage message selection: occupied slots by
  exchange type (TRADE_0 → TADA/TADA2 by data0,
  TRADE_1 → MISE, TRADE_2 → BERU2, TRADE_3 and others
  → MISE via `default:`), empty slots → IKAGA/DONO2
  by data0, and the remaining tag arms (DONO,
  OIKURA).
- The house index (`npc_id − 0xA004`), the proceeds
  bag math (`money ≥ 99999 → (money−99999)/30000+1`),
  the dialogue category in verbatim branch order
  (NO_OWNER → NEED_FRIEND → PROCEEDS → NORMAL →
  OTHER_OWNER) and its message table
  {2356, 2357, 2341, 2344, 2350}.
- The interruption recheck, the dig-path predicate
  (a deposited non-hole tile returns the FG item
  directly — the FG entry IS the haniwa item id, no
  conversion exists), and the BERU2 price rebuild.

Deliberately NOT ported: `mAGrw_OrderSetHaniwa` has no
in-tree callers (the live arming path is the
GROW_FIRST branch); the random selector's
no-duplicates loop and `FTR_NO_2_IDX` mapping; the
`@unused` `mHW_MSG_MOU`; the `hanwiaPortrait_ovl`
decomp typo field; `mAGrw_SetDigItem` running
`mAGrw_SpoilKabu` over the stored haniwa slots; the
`mHW_MSG_DOREWO` interruption special case.

C ABI (all `pc_`-prefixed, 10 exports):
`pc_haniwa_item_valid`, `pc_haniwa_exchange_msg`,
`pc_haniwa_tag_msg`, `pc_haniwa_house_idx`,
`pc_haniwa_bell_bags`, `pc_haniwa_dialogue_category`,
`pc_haniwa_dialogue_msg`, `pc_haniwa_interrupt_keep`,
`pc_haniwa_dig_direct`, `pc_haniwa_beru2_rebuild`. No C
callers rewired yet, per the usual pattern. No test
runs, per the standing rule.

### Runtime Port Progress: Insect actors (shared controller)

New `rust/src/insect_actor.rs`, registered as `mod
insect_actor` in `rust/src/lib.rs`. Decomp-verified
against `ac_insect.c`, `ac_insect_h.h`,
`ac_insect_data.c_inc`, `ac_insect_move.c_inc`,
`ac_insect_clip.c_inc`, `ac_insect_draw.c_inc` plus
all 14 `ac_ins_*.c` behavior files (GAFE01_00 Rev 0
USA). 12/12 sections PROVEN; the 14 behavior-state
machines themselves remain untraced as complete
transition tables and stay in C.

Architectural finding: the 14 `ac_ins_*.c` files are
14 behavior programs shared by 41 insect types, not
one-species implementations. Every insect shares
`aINS_position_move`; the per-type `mv_proc` is the
behavior program. Work slots are reused with
different meanings per family and must not be given
one shared semantic interpretation.

Ported (pure kernels; pool slot ownership, field/actor
iteration, actual movement, collision checks, catch
registration, effects, lighting, cleanup, and all 14
family state machines stay in C):

- The 41-entry type→program table (all programs
  verified used).
- The catch-range rules: default 8.0; common/yellow
  butterflies 24.0 unconditional; cicadas/bee and
  all 8 beetle types 24.0 only when the player's
  relative angle is within 90° (else 0.0); the
  cockroach gets the facing test only when
  `flag == 4`.
- The stress radius addition table (`catch_ME_data`,
  negative entries shrink the radius), the stress
  band index and per-actor stress (max over
  UNUSED/PLAYER/NPC/BG lists; only moving actors
  contribute), and the patience step (decay is
  dt-scaled, growth is not: `−= 0.5·dt` vs
  `+= stress·0.5`, clamped 0/100).
- The fade rules (most types −11 alpha/frame from
  255; firefly's separate alpha2 path) and the
  despawn predicate (released & invisible, or
  > 600.0 in a different acre, both skipped while
  the insect is the net-catch target).
- Per-type shadow sizes (8.0/4.0/2.0/6.0).

Brief corrections: butterfly/mosquito spawn is
background height − 30.0f (brief said +30.0f); the
chou habitat check rejects units whose high id
digit is 5 (0x5xxx = houses/shops); the spider is
MINO-only (dango's SPIDER case is unreachable dead
code); the net-catch special case lives in the cull
check, not registration; a stale decomp comment
says "15 units" where the constant is 600.0f.

C ABI (all `pc_`-prefixed, 12 exports):
`pc_insect_program_for_type`,
`pc_insect_catch_range`, `pc_insect_catch_me_radius`,
`pc_insect_stress_idx`, `pc_insect_stress_calc`,
`pc_insect_patience_step`, `pc_insect_fade_alpha`,
`pc_insect_fade_done`, `pc_insect_firefly_alpha2`,
`pc_insect_firefly_fade_done`,
`pc_insect_despawn_check`, `pc_insect_shadow_size`.
No C callers rewired yet, per the usual pattern. No
test runs, per the standing rule.

### Runtime Port Progress: Fish actors

New `rust/src/fish_actor.rs`, registered as `mod
fish_actor` in `rust/src/lib.rs`. Decomp-verified
against `ac_gyoei.c`, `ac_gyoei.h`,
`ac_gyoei_move.c_inc`, `ac_gyoei_draw.c_inc`,
`ac_gyoei_clip.c_inc`, `ac_gyoei_type.c_inc`,
`ac_gyoei_model.c_inc`, `ac_gyo_kage.c`,
`ac_gyo_kage.h`, `ac_gyo_release.c`,
`ac_gyo_release.h`, `ac_gyo_test.c`,
`ac_gyo_kaseki.c` (GAFE01_00 Rev 0 USA). 11/11
sections PROVEN or DIFFERS-with-correction.

Architecture: swimming fish are *embedded
controller slots* (two `aGYO_CTRL_ACTOR` slots in
the `GYOEI_ACTOR`), not standalone actors per fish.
The kage actor is a separate short-lived
water-surface visual, not the ordinary ground
shadow. The release actor is a standalone actor
created on catch, linked to the float through
`UKI_ACTOR.child_actor`.

Ported (pure kernels; slot/actor allocation, clip
callbacks, catch registration, float linkage,
collision/background APIs, rendering,
effects/sound, and the full test/kaseki state
machines stay in C):

- The 45-entry size/search/bite tables, the
  shadow-scale table, the test/kaseki behavior
  dispatch (with SALMON2→SALMON normalization),
  and the rod-indexed search-area/bite-time
  tables.
- The release math: initial velocity toward the
  water target, gravity, the water-contact
  revert rule, the shrink/delete thresholds
  (`powf(rate, dt)`), and the shadow-size formula
  with the 6-entry correction arrays kept as-is.
- The kage alpha, the ripple-argument switch, the
  flee predicate, and the float search/bite
  helpers.

Brief corrections: `sizeof` values are GYOEI_ACTOR
0x634, aGYO_CTRL_ACTOR 0x254, GYO_RELEASE_ACTOR
0x1C0, `exist[4]` at 0x61C, `segment_type[4]` at
0x620; the shadow-scale table is not in
`ac_gyoei.c` itself; the regular and gold rod
search-area tables are identical; the kage
`draw_frame` wraps modularly and the ripple gate
is a one-shot; `aGYR_empty_area` returns 0 when
no segment is free; gyo_flags bit 0x10 is
kaseki-only; the whale gets no flee ripple; the
6-entry shadow-correction arrays carry a verbatim
`@BUG` comment and were NOT extended.

C ABI (all `pc_`-prefixed, 16 exports):
`pc_fish_size_class`, `pc_fish_search_class`,
`pc_fish_bite_class`, `pc_fish_shadow_scale`,
`pc_fish_behavior_program`,
`pc_fish_release_velocity`,
`pc_fish_release_gravity`, `pc_fish_release_revert`,
`pc_fish_release_shrink`, `pc_fish_release_shadow`,
`pc_fish_kage_alpha`, `pc_fish_kage_ripple`,
`pc_fish_flee`, `pc_fish_search_ok`,
`pc_fish_bite_time`, plus ITM_FISH_START. No C
callers rewired yet, per the usual pattern. No
test runs, per the standing rule.

### Runtime Port Progress: Bees (tree-shake helpers + bee_ant.rs audit fixes)

Extended `rust/src/bee_ant.rs` (already registered).
Decomp-verified against `ac_ghog.c`, `ac_ghog.h`,
`ac_bee.c`, `m_player_main_shake_tree.c_inc`,
`m_player_lib.c`, `bg_item_common.c_inc`
(GAFE01_00 Rev 0 USA).

The 2026-10-11 audit of the existing file found and
fixed 4 bugs: `insect_type::ANT` was 41 (it is 38;
41 is NONE), `COCKROACH` was 42 (it is 28); the bee
player-status values were swapped (retail: ATTACK =
1, ENTER_BUILDING = 2); the target frame ran on
degree diffs instead of the retail s16 short-angle
diff; the speed target was heading-dependent when
retail's `abs_speed_angle` is always zeroed, making
the speed target the constant 8.9f. The bee
`step()` also gained a dt-scaled `step_dt()`
(alpha, catch delay, bobbing), since retail scales
several updates by `graph_dt_60fps_frames`.

New tree-shake decision helpers (pure kernels; the
shake-tree table, fruit-drop callback, actor
allocation, FG mutation, honeycomb lifecycle, shock
request, status-for-bee, ghog rendering/shadow,
`atans_table`, net-catch/label APIs, catch-label
transfer, sAdo sound, keyframe animation, and
rendering stay in C):

- `pc_bee_spawn_eligible` — the exact
  `mPlib_able_birth_bee()` predicate (first intro,
  reset flag, bee chase flag, title demo).
- `pc_bee_shake_retry_step` — the five-count
  retry: success → timer 0 + spawned; failure →
  timer − dt clamped at 0.
- `pc_bee_tree_revert` — new C ABI (none
  existed): the normal tree, or −1.
- `pc_bee_drop_valid` — x ≥ 0 and z ≥ 0 (Y not
  checked in the source).
- `pc_bee_shock_request` — bee_flag and frame ≥
  30 → shock(24.0, bee angle).
- `pc_bee_no_bee_status` — frame == 29.5 →
  ATTACK status.
- `pc_bee_force_catch` — (swing or stopped net)
  and dist < 40.0; otherwise the table path at
  24.0.
- `pc_bee_speed_target` (8.9f),
  `pc_bee_target_frame` (s16 diff).

`ac_ghog.c` is not the tree-shake decision logic;
it is the hive field-item/shadow visual and stays
in C. No test runs, per the standing rule.

### Runtime Port Progress: NES games (Famicom subsystem)

New `rust/src/famicom.rs`, registered as `mod
famicom` in `rust/src/lib.rs`. Decomp-verified
against `famicom_emu.c`, `famicom.h`,
`famicom_nesinfo.cpp`, `m_fbdemo.c`,
`m_fbdemo_fade.c`, `m_fbdemo_triforce.c`,
`m_fbdemo_wipe1.c`, and `ac_famicom_common.c`
(GAFE01_00 Rev 0 USA). 5/5 sections PROVEN or
DIFFERS-with-correction.

Scope: the actual 6502 NES core, the GAME state
handoff, XFB/display-manager switching, the audio
lifecycle, the THA/zelda arena, controller
polling, card I/O, the tag-parser side effects,
fbdemo rendering, and the house message controller
stay in C.

Ported (pure kernels):

- The 20-entry furniture tables (game ids with
  the repeated final 14; GBA ids with 0xFF at
  7/9/14/18/19), the furniture index, the
  metadata index (`rom_no − 1`, kept separate
  from GNO), and the debug ROM override.
- The exit wrapper: the exact L+R+Z / L+R+X+Y
  combos and the done/countdown/return machine.
- The save-header and per-player-record checksum
  validation and repair, including the looser
  save-check version test.
- The result→message mapping (7 distinct
  results; NOFILE → 0x3071) and the ROM-kind
  classification.
- The retail-form transition math: fade
  (timer gate, type-1 inversion, type-7 skip,
  s8 start frames −6/−85), triforce (bit-decoded
  texture/direction, truncated step), wipe1
  (integer-division step, 612/333 clamps).

Brief corrections: fbdemo draw binds segments
0x0A/0x0B, not 3/4; the fade start frames are s8
(−6/−85), not +250/+171; the decomp's `move()`
functions carry PC-port dt-accumulators, but the
retail-faithful simple form is ported here; the
Excitebike/Soccer ordering is kept as-is; the
`special_zelda()` BBRAM repair stays in C; and
`scene.rs` omits `famicom_emu_cleanup` for the
FamicomEmu scene entry (wiring follow-up, noted
in the leftovers).

C ABI (20 `pc_`-prefixed exports):
`pc_famicom_ftr_idx`, `pc_famicom_game_id`,
`pc_famicom_agb_id`, `pc_famicom_meta_idx`,
`pc_famicom_debug_rom`, `pc_famicom_exit_combo`,
`pc_famicom_exit_step`, `pc_famicom_rom_kind`,
`pc_famicom_result_msg`, `pc_famicom_header_valid`,
`pc_famicom_header_repair`, `pc_famicom_save_check`,
`pc_famicom_record_valid`, `pc_famicom_record_repair`,
`pc_fbdemo_fade_start`, `pc_fbdemo_fade_step`,
`pc_fbdemo_triforce_type`, `pc_fbdemo_triforce_step`,
`pc_fbdemo_wipe1_type`, `pc_fbdemo_wipe1_step`.
No C callers rewired yet, per the usual pattern.
No test runs, per the standing rule.

### Runtime Port Progress: Bulletin boards

New `rust/src/town_board.rs` and
`rust/src/house_board.rs`, registered as `mod
town_board` and `mod house_board` in
`rust/src/lib.rs`. Decomp-verified against
`m_notice.c`, `m_notice.h`, `m_notice_ovl.c`,
`m_board_ovl.c`, `m_board_ovl.h`,
`m_hboard_ovl.c`, `m_hboard_ovl.h`, `m_home_h.h`,
and `save_check_take.c_inc` (GAFE01_00 Rev 0
USA). 8/8 sections PROVEN.

Key resolution: the real town-board writer is
`m_notice_ovl.c`, not `m_board_ovl.c` — the
latter is the *general letter-editing overlay*.
Button A stamps the current RTC time into the
post at editor *open*; on confirmation the post
is written and the first-job errand is cleared.

Ported (pure kernels; RTC comparison and save
ownership, the editor/submenu flows, handbill ROM
loading, font rendering, the mail/letter
overlay, and the automatic-schedule date
selection stay in C):

- The 15-slot record model (0xC8 records: 192
  message bytes + timestamp at 0xC0): the
  first-empty-slot scan, the full-board shift
  (records 1..14 → 0..13, whole-record copies),
  and page clearing (CHAR_SPACE fill, not
  zeroed).
- The initial handbill ids {0x1E..0x21}, the
  automatic-post time (06:00:00), and the
  handbill-id formulas (0x1A4+id,
  0x242+((day/7)&3)).
- The house-board line check with the
  font-width function injected (OK/NEWLINE/
  OVER_WIDTH/OVER_STR_LEN), the 192-width /
  4-line constraints, and the 128-byte message
  layout.

Insertion is FIFO by array position, not a
timestamp sort. The save-validation loop checks
only the contiguous occupied prefix, not all 15
slots — preserved by construction.

C ABI (all `pc_`-prefixed, 11 exports):
`pc_board_first_empty`, `pc_board_shift_full`,
`pc_board_clear_page`, `pc_board_init_handbill`,
`pc_board_auto_post_time`,
`pc_board_standard_handbill_id`,
`pc_board_fishing_handbill_id`,
`pc_hboard_line_check`. No C callers rewired yet,
per the usual pattern. No test runs, per the
standing rule.

### Runtime Port Progress: Town map

New `rust/src/town_map.rs`, registered as `mod
town_map` in `rust/src/lib.rs`. Decomp-verified
against `m_map_ovl.c`, `m_map_ovl.h`,
`m_map_ovl_h.h`, and `m_field_make.h`
(GAFE01_00 Rev 0 USA). 10/10 sections PROVEN,
with one brief correction and one source-data gap.

Key correction: the brief's "6x7 = 42 acres" is
wrong. `FG_BLOCK_X_NUM` = 5, `FG_BLOCK_Z_NUM` =
6, so the interior map is 5x6 = 30 acres.
`mMP_BLOCKX_MAX`/`mMP_BLOCKZ_MAX` are only the
texture-table scan bounds. Verified against the
header.

Source-data gap: `mMP_house_pos_list` (the three
`(x, z)` villager-house positions) lives in a
binary `.inc` asset absent from the decomp, so
its contents are not recoverable here; the
matching algorithm is ported with the table
contents injected.

Ported (pure kernels; overlay allocation and
lifecycle, all save/field access, `mFI_`
conversions, collision queries, NPC/player
identity and name conversion, private/animal free
checks, input callbacks, transitions, sound,
font rendering, matrices, display lists,
textures, palettes, and the resident-cache
assembly stay in C):

- The full `l_map_texture` table (108 entries,
  compressed to 69 distinct `kan_tizu_*` texture
  indices) and the `l_map_pal` table (palette 1
  for exactly the 20 verified types, 0 elsewhere)
  with `pc_map_tex_idx`/`pc_map_pal_idx`.
- The full `pluss_bridge` table (19 non-0xFF
  entries verified verbatim) and the saved-
  bridge coordinate predicate in
  `pc_map_bridge_type`.
- `mMP_check_bg_kind`: rejects kinds 0-8, the
  two border-cliff transitions (61, 62), and the
  two tunnels (9, 10).
- `mMP_check_layer`: 2/1/0/default-1, minus one
  when `!mRF_CheckFieldStep3()`, clamped to zero.
- `mMP_set_house_data`'s position lookup:
  first entry with `x == ut_x && z == ut_z - 1`,
  entry-0 fallback (`pc_map_house_pos_match`).
- The ordered building-flag chain
  (SHOP/POLICE/POST/SHRINE/STATION/DUMP/MUSEUM/
  TAILORS/DOCK) as `pc_map_building_label`.
- The pure selection/bounds kernel of
  `mMP_move_Play` as `pc_map_cursor_step`: exit
  on A/B/Start (X only when data0 == 1), then
  C-left/down/up/right with bounds bx in
  [0,4], bz in [0,5] and +/-22.0 target steps.
- The 18-frame cursor pulse tables (brightness
  for the green prim-color component; scale)
  as `pc_map_cursor_pulse`.
- Cursor init math (bx*22.0, -bz*22.0, frame 0),
  the window-frame selection
  (4->4T, 3->3T, 1->T, 0->1T, else 2T),
  `mMP_get_label_cnt` (true count for NPC/
  player, 1 for buildings), the house-icon
  offsets ({5,13,17}/{−4,−11,−18} with %3//3),
  and the two-entry town-name color table.

Brief nuances preserved: the free-player name is
`"free    "` (4 spaces); the villager sort is a
swap-and-restart loop (stays in C); the cursor
frame is ported in its N64 `++`/`%=` form rather
than the PC dt-accumulating f32; and
`get_player_actor_withoutCheck` vs the structure-
exit-door position stays in C.

C ABI (all `pc_`-prefixed, 14 exports):
`pc_map_check_bg_kind`, `pc_map_tex_idx`,
`pc_map_pal_idx`, `pc_map_bridge_type`,
`pc_map_check_layer`, `pc_map_house_pos_match`,
`pc_map_building_label`, `pc_map_cursor_step`,
`pc_map_cursor_pulse`, `pc_map_cursor_init`,
`pc_map_win_frame`, `pc_map_label_top_cnt`,
`pc_map_house_offset`, `pc_map_land_color`. No C
callers rewired yet, per the usual pattern. All 13 unit tests
pass (two transcription bugs in the ported tables — a short
bridge table and a swapped palette pair — were caught by the
tests and fixed).

### Runtime Port Progress: Diary

New `rust/src/diary.rs`, registered as `mod
diary` in `rust/src/lib.rs`. Decomp-verified
against `m_diary.c`, `m_diary_ovl.c`,
`include/m_diary.h`, and `include/m_card.h`
(GAFE01_00 Rev 0 USA). 9/9 sections PROVEN.

Key facts: entries are exactly 992 bytes of
text with no date/player/checksum inside; the
persistent container is `mCD_keep_diary_c`
(`u16` checksum + `entries[4][12]`), 47,618
bytes unpadded, `ALIGN_NEXT(..., 32)` = 47,648
(0xBA20); the overlay selects
`entries[data1][data3]` (player slot, month
slot). Initialization is space-filled, not
zero-filled (`bzero` then `mem_clear(...,
CHAR_SPACE)`), because the editor measures
text by trailing spaces.

Ported (pure kernels; ARAM load/save, card
serialization and checksums, `mFont_
GetCodeWidth`, the shared editor and
edit-end-check overlays, `chase_f()` easing,
transitions, sounds, graphics, the static
overlay object and its allocation, the
calendar's open call, and controller polling
stay in C):

- `mDi_strlen`: the backward scan verbatim —
  one-based position of the first byte
  differing from `end_char`, 0 when all match,
  never stopping at zero bytes.
- `mDi_init_diary`: 992 bytes of `CHAR_SPACE`
  (0x20).
- Entry indexing: `2 + (player * 12 + month) *
  992` byte offset within the diary structure.
- `mDI_get_col_line_width`: the width-192 wrap
  scan verbatim with the font-width lookup
  injected — wrap on `width > 192` (strictly
  greater), newline (205) consumed and tracked
  separately, s16 counters, the 31→30 clamp
  quirks (`cur_line_count = line_width + 1`).
- `mDI_strLineCheck`: the 3/1/2/0 state
  machine verbatim, including the
  don't-advance-on-overflow rule.
- Scroll limits: one-line ±1 within
  `[0, line_count - 6]`; ten-line with the
  strict `_18 - 10 > 0` test and clamping.
- `mDI_Set_command_read`: the input-repeat
  state machine as a pure step function —
  fresh trigger sets delay 30 / repeat 26,
  held input decrements and fires commands 2/3
  (fast repeat `_30 = 3` when `_11 == 0`);
  C-up/C-down edge triggers return commands
  8/9.
- The 12-entry month frame adjustment table
  verbatim (0-based month: 0 = January, from
  the calendar's `data3`).
- The `mDI_move_Obey` confirmation branch:
  `data1 == 0` writes the whole structure to
  ARAM and opens the copy-warning overlay;
  `data1 == 1` returns to editing with no
  ARAM write.

Brief nuances preserved: the `mDi_data_c`
struct exists only as a comment in the decomp;
`mCD_DIARY_SAVE_SIZE` (0xC000) is the card-
file size, larger than the 0xBA20 structure;
destruct frees through the static
`diary_ovl_data.data`; the read→write scroll
animates `_28` quadratically to exactly
-100.0f.

C ABI (all `pc_`-prefixed, 11 exports):
`pc_di_strlen`, `pc_di_init_entry`,
`pc_di_entry_offset`, `pc_di_wrap_scan`,
`pc_di_line_check`, `pc_di_scroll_one`,
`pc_di_scroll_ten`, `pc_di_repeat_step`,
`pc_di_ten_line_cmd`, `pc_di_month_adjust`,
`pc_di_confirm_branch`. No C callers rewired
yet, per the usual pattern. No test runs, per
the standing rule.

### Runtime Port Progress: Passwords

New `rust/src/password.rs`, registered as `mod
password` in `rust/src/lib.rs`. Decomp-verified
against `m_mail_password_check.c`,
`include/m_mail_password_check.h`,
`m_passwordMake_ovl.c`,
`include/m_passwordMake_ovl.h`,
`m_passwordChk_ovl.c`,
`include/m_passwordChk_ovl.h`, the password
paths of `m_editor_ovl.c`, and
`ac_npc_shop_common.c` (GAFE01_00 Rev 0 USA).
6/6 sections PROVEN, with brief corrections.

Key facts: the overlays only collect fixed-size
byte strings — make returns town[8]+player[8]
(16 bytes), check returns line0[14]+line1[14]
(28 bytes), both by `mem_copy` with no
delimiters; `LAND_NAME_SIZE` = 8 and
`PLAYER_NAME_LEN` = 8. The codec turns a
21-byte payload into 28 six-bit characters
(168 bits = 168 bits). Payload byte 0 packs
type (bits 7-5), checksum (bits 4-3),
hit_rate (bits 2-1), npc_type (bit 0); byte 1
is the NPC code; bytes 2-9/10-17 are the two
8-byte names; bytes 18-19 are the item ID;
byte 20 is the RSA keysave. Checksum =
(Σstr0 + Σstr1 + item + npc_code) & 3.

Ported (pure kernels; overlay lifecycle and
allocation, the shared editor state machine,
sounds, warning windows, transitions, font
rendering, ARAM/card I/O, player/town identity
lookups, the item catalog and price checks,
inventory mutation, NPC dialogue/reward flow,
RNG, and the `pswd_famicom_list` extern stay
in C):

- The full encode pipeline verbatim:
  passcode build → 256-entry substitution →
  transposition (dir 1, table 0) → bit shuffle
  stage 0 → RSA (`b^r mod pq` over 8
  select-table bytes, 9th-bit keysave at byte
  20, 256 primes verbatim) → nibble-selected
  bit mixing → bit shuffle stage 1 →
  transposition (dir 0, table 1) → LSB-first
  21-byte→28-symbol packing → 64-character
  `usable_to_fontnum` alphabet mapping.
- The exact inverse decode pipeline,
  including the 0→O / 1→l normalization and
  the font→symbol lookup that keeps scanning
  (last match wins).
- All tables verbatim: 256 primes, 256-entry
  substitution, 16×8 bit-select indices, 32
  transposition strings (NUL-inclusive
  lengths; key byte 18 for table 0, 9 for
  table 1), 64-char alphabet, 64-entry
  allowed-character set.
- The `// @fakematch` bit routines
  (bit_shift skipping the key byte,
  bit_reverse, bit_arrange_reverse,
  bit_shuffle/decode) ported literally.
- Payload unpack (`mMpswd_password`) with the
  MAGAZINE npc_type-bit fold into
  hit_rate_index bit 2, and the zuru checksum
  check.
- The fixed-width 14-byte line editor:
  insert/shift state machine, password
  backspace (including the line1→line0
  switch), the `line1[13] != SPACE` charset-
  validation gate, and the all-28-bytes
  allowed check.
- Make-flow helpers (space init, town-
  nonempty gate) and check-init prefill from
  the 28-byte data2.
- Mail-body code extraction
  (`mMpswd_restore_code`): SYMBOL_KEY opening
  check, newline-pair scan, byte-14 newline
  strip.

Brief corrections: the check overlay
pre-fills its lines from `menu_info->data2`,
not spaces; POPULAR forces hit_rate = 4 with
a `// potential bug?` comment (it collides
with the checksum bits); unknown types fall
back to USER without resetting hit_rate/
npc_code; the magazine hit-rate table is f32
`{80.0, 60.0, 30.0, 0.0, 100.0}` with the
RANDOM_F draw only in that path; the shop
send call is `mMpswd_make_password(str, 0x4,
0x1, recip, town, item, 0x0, 0x0)` with
inventory consumed on commit.

C ABI (all `pc_`-prefixed, 17 exports):
`pc_pw_make_password`, `pc_pw_decode_code`,
`pc_pw_unpack`, `pc_pw_zuru_ok`,
`pc_pw_usable_letter`, `pc_pw_make_can_advance`,
`pc_pw_line_insert`, `pc_pw_backspace`,
`pc_pw_needs_charset_check`, `pc_pw_all_usable`,
`pc_pw_make_init`, `pc_pw_check_init`,
`pc_pw_extract_code`. No C callers rewired
yet, per the usual pattern. All 10 unit tests
pass.

### Runtime Port Progress: Notice-board overlay & scheduler

New `rust/src/notice_overlay.rs`, registered as
`mod notice_overlay` in `rust/src/lib.rs`.
Decomp-verified against `m_notice_ovl.c`,
`include/m_notice_ovl.h`, `m_notice.c`, and
`include/m_notice.h` (GAFE01_00 Rev 0 USA).
14/14 sections PROVEN, with brief corrections.

Key facts: the record layer (first-empty
scan, FIFO shift, page clear, init handbills,
auto-post time, handbill-id formulas) was
already ported in `town_board.rs` and is not
duplicated; the handbill composer primitives
live in `npc_reply.rs`. This module covers the
overlay state machine and the auto-post
scheduler.

Ported (pure kernels; RTC compares and
save/common reads-writes, the shared editor
and edit-end overlays, font-width lookup,
handbill ROM loading, fishing-result mail,
furniture selection, treasure placement,
animal-memory queries, first-job quest
mutation, sounds/music, graphics, easing,
transitions, and overlay allocation stay
in C):

- Navigation (`mNT_Play_page_read`) with the
  exact else-if priority: C-left → −1,
  C-right → +1, C-down → jump to slot 0,
  C-up → jump to newest, A → begin write,
  B/Start → exit. `move_time = min(|move_page|,
  3)` with the integer-division page steps
  (first step `move_page/move_time`, later
  forward/backward forms).
- Stick-area classification (0.2 threshold,
  eight signed-angle sectors, CENTER=8) and
  the 9-entry `kei_win_st_tex_tbl` as
  texture-slot indices with repeats
  preserved.
- The notice line-check state machine
  (codes 0–3, strict width > 192, font widths
  injected).
- Page-number display (`page_no+1`, working
  post capped at 15), the invalid-month → 13
  header rule, and the confirmation branches
  (0 = commit, 1 = re-edit, else discard).
- The 43-entry auto-post date table verbatim
  (including the brief-missed 0x18–0x1A
  fall-fair entries and the 0xFFFF unused
  slots), the packed-date operate arithmetic
  (equinox/DST additions carry into the month
  byte), the selection sort, the near-old
  cursor walk with the renew-hour tiebreak,
  and the reverse-indexed decide selection
  (fishing days first, then standard entries
  walking backward with year wrap).
- The USA treasure gate sequence (renew-hour,
  3-day interval, checked-today, animal
  count, 0.4 RNG draw — all injected) and the
  pitfall/lottery/event pick from a single
  draw.

C ABI (all `pc_`-prefixed, 15 exports):
`pc_notice_nav_step`, `pc_notice_move_time`,
`pc_notice_first_step`,
`pc_notice_later_step`,
`pc_notice_stick_area`, `pc_notice_kei_tex`,
`pc_notice_line_check`, `pc_notice_page_no`,
`pc_notice_month_idx`, `pc_notice_confirm`,
`pc_notice_operate_dates`,
`pc_notice_sort_pairs`,
`pc_notice_last_valid_idx`,
`pc_notice_near_old_cursor`,
`pc_notice_decide`, `pc_notice_treasure_ok`,
`pc_notice_treasure_pick`. No C callers
rewired yet, per the usual pattern. All 8
unit tests pass.

### Runtime Port Progress: Address book

New `rust/src/address_book.rs`, registered as
`mod address_book` in `rust/src/lib.rs`.
Decomp-verified against `m_address_ovl.c` and
`include/m_address_ovl.h` (GAFE01_00 Rev 0
USA). All sections PROVEN, with one
significant brief correction: the brief's
section 3 described the *notice* overlay's
struct (`notice_ovl_s`); the real address
overlay struct is `address_ovl_s` with
`page_bitfield`, `page_count`, `proc_idx`,
`curIdx`, `nextIdx`, `selected_entry`,
`show_museum_address`, `editor_move_down`,
`page_entry_count[3]`, `pos_x`/`speed_x`,
`pile[3]`/`goal_pile[3]`,
`player_mail_name[PLAYER_NUM+1]`,
`animal_mail_name[ANIMAL_NUM_MAX]`, and the
editor display-list handoff pointer.

Key facts: the book is rebuilt on open, not
saved. The player page holds the other valid
player identities in ascending slot order
(active slot skipped, empty slots excluded by
the personal-ID check) with the museum entry
appended after them when eligible
(`show_museum_address` explicitly FALSE
otherwise). Villagers qualify only with a
non-free identity AND a memory entry for the
current player; the first eight fill NPC page
0, the rest NPC page 1.

Ported (pure kernels; save/RTC access,
identity checks, museum eligibility, NPC
memory queries, name conversion and header
copy, the shared editor/board overlays,
sounds, display lists, transitions, easing,
and overlay allocation stay in C):

- Player-list builder (active-slot skip,
  validity filter, museum append).
- Villager 8-entry paging.
- Page model: bitfield built MSB-first,
  initial page (player else NPC0 else
  refusal), `mAD_PROC_REFUSE` on an empty
  book ("Your address book is empty!").
- `mAD_get_nextIdx` verbatim (-1 on a single
  page; ±1 with wrap, at most two candidates,
  first set bit wins).
- Recipient resolution (player → selected,
  NPC1 → 8 + selected, else selected) using
  the raw 0/2 comparisons from the C.
- Context-sensitive B (cancel vs header
  focus), the header_pos==1 WAIT reset, and
  the pile targets ({0,5,0}/{5,0,5} for two
  pages, rotated {0,5,10} for three).

C ABI (all `pc_`-prefixed, 8 exports):
`pc_ad_player_list`, `pc_ad_npc_pages`,
`pc_ad_page_model`, `pc_ad_get_next_idx`,
`pc_ad_resolve`, `pc_ad_b_cancel`,
`pc_ad_wait_reset`, `pc_ad_pile_init`. No C
callers rewired yet, per the usual pattern.
All 7 unit tests pass.

### Runtime Port Progress: Pause (debug helper)

New `rust/src/pause.rs`, registered as `mod
pause` in `rust/src/lib.rs`. Decomp-verified
against `m_pause.c`, `include/m_pause.h`,
`m_play.c`, and `zurumode.h` (ACGC-PC-Port
tree, GAFE01_00 Rev 0 USA base). 9/9 sections
PROVEN, with one significant brief correction.

Key finding: `m_pause.c` is not a pause-menu
UI. It is a tiny debug-mode-gated input/state
helper with no graphics, options, or overlay
state machine: it returns 1 when the gameplay
update may proceed and 0 when it stays gated,
and the caller inverts the result.

Brief correction: the brief's `int timer` /
`++timer > 8` matches the retail N64 form,
but the PC-port tree carries `f32 timer` with
a separate dt-accumulation block (`timer +=
dt_num_60fps_frames` while Z+R held without a
fresh R press; return check tests `timer >
8.0f` strictly). The port follows the tree.

Ported (`PauseState { enabled: i32, timer:
f32 }`, 8 bytes, `repr(C)`; pad `now`/`on`
bitfields and dt injected so the kernel stays
engine-free; the `ZURUMODE2_ENABLED()` gate,
the `!Pause_proc` inversion, dt sourcing, the
separate `g_pc_paused` coarse pause, and pad
polling stay in C):

- `pc_pause_init` (`Pause_ct`): zero the
  state.
- `pc_pause_proc`: R held + D-pad Down newly
  pressed toggles `enabled` before the return
  check; Z+R held without fresh R accumulates
  dt; returns 1 (and resets the timer) when
  disabled or when Z is held with a fresh R
  press or the timer past 8.0, else 0.

C ABI: `pc_pause_init`, `pc_pause_proc`. No C
callers rewired yet, per the usual pattern.
All 7 unit tests pass.

### Runtime Port Progress: Selection/choice UI

New `rust/src/choice.rs`, registered as `mod
choice` in `rust/src/lib.rs`. Decomp-verified
against `m_choice.c` (+ its `.c_inc` state
files), `m_msg_cursol.c_inc`, `m_select.c`,
and their headers (ACGC-PC-Port tree,
GAFE01_00 Rev 0 USA base). 14/14 sections
PROVEN, with corrections.

Key distinction: `m_choice.c` is the shared
dialogue/prompt widget; `m_select.c`
("FOREST MAP SELECT") is a separate debug
scene launcher, not the generic list UI. Only
its index wraparound is ported.

Ported (pure kernels; message-window
embedding, font widths, ROM/ARAM loading,
control-code substitution, the `mMsg`
dispatch, sounds, graphics, easing, and all
of `m_select.c`'s scene launching stay in C):

- Option admission (`Set`/`Add_choice_data`):
  non-null, 0 < len <= 16, free slot;
  trailing-space effective length; exactly
  `len` bytes copied with no row clearing.
- Cursor step with verbatim clamping (no
  wrap; empty list yields -1).
- The 5-state auto-repeat machine with float
  dt (18.0 wait, 9.0 repeat), fresh-direction
  immediate move, release reset, and cursor-
  sound suppression on clamping.
- B-flag rules (`no_b_flag` enables the
  B-to-last-option shortcut) and close-path
  selection (confirm vs short/long zoom-down
  + `NO_ZOOMDOWN` flag).
- Determination commit (16-byte copy +
  recorded length + confirmed index; cursor
  and result stay separate fields).
- Debug-selector wraparound.

Brief corrections: constructor sets
`choice_num = 6` (zero reset only in the Set
path); D-pad Up/Down also drive the cursor;
repeat uses dt accumulation; appear/
disappear animate via dt to 10.2f.

C ABI (all `pc_`-prefixed, 8 exports):
`pc_choice_begin`, `pc_choice_add_option`,
`pc_choice_cursor_step`,
`pc_choice_repeat_step`,
`pc_choice_b_shortcut`,
`pc_choice_determination`,
`pc_choice_close_kind`, `pc_select_wrap`. No
C callers rewired yet, per the usual pattern.
All 6 unit tests pass.

### Runtime Port Progress: Birthdays, Event NPC Placement, Holiday Scenery

By claude (task `port-event-holiday-actors`). Primary source: ac-decomp
(github.com/ACreTeam/ac-decomp @ 09ca8e8); secondary: ACGC-PC-Port
(diffed file by file); background: hunter-r.com (no text on these
systems, video-only) and Nookipedia's GameCube event list (dates and
locations agree with the decomp block kinds). Five new modules, pure logic
plus `pc_*` C ABI; no C callers rewired; no test runs.

- `birthday_ovl.rs` (`m_birthday_ovl.c`): the "When's your birthday?"
  picker. Cursor month/day/OK; C-up/down edits validate against the fixed
  leap year 2000 (Feb 29 always allowed) and repair invalid dates:
  month change clamps the day, day wraps 1 <-> month end, month wraps
  1 <-> 12. START or A on OK closes and stores `now_private->birthday`.
- `birth_control.rs` (`ac_birth_control.c`): not birthdays, but the
  per-acre actor spawner. Unit classification (ITEM2 / PROPS with
  snowman `EMPTY_NO` vs `RSV_NO` clear / STRUCT), unit-centre table
  (20..620), retail stale-actor rule (both axes must be stale, a quirk)
  vs the PC borderless rule (Chebyshev <= 1), move-actor bitfield, GBA
  island-boat poll in blocks (5,5)/(4,6) (GBA2 state values 1 =
  success, 9 = transmitting).
- `event_map_npc.rs` (`m_event_map_npc.c` + `_data.c_inc`): 17 event
  maps (well / 7 pond layouts / none), villager selection (drop
  villagers who don't know the player when there's a surplus, then
  `RANDOM`), refill on reload (lowest free index, discarded `RANDOM`
  kept for RNG parity), event-shirt cloth +1 for male looks, Halloween
  naming, lap check (2 structure / 1 near NPC / 0). Tables generated
  by `tools/gen_evmn.py` with names resolved from `m_name_table.h`;
  event ids confirmed by compiling `m_event.h`'s enum with mingw32 gcc.
- `go_home_npc.rs` (`ac_go_home_npc.c` + `mGH_*` in `m_event.c`): the
  visit by a villager who moved away. The window opens **more than** 7
  days after move-out (the decomp comment says "less than"; the code
  wins). One meeting per player; the record is cleared at 60 days, when
  every player has talked to them, or when the villager is back in town.
  Message `12827 + looks*4 (+1+RANDOM(3) on repeat)`. `lbRTC_Add_DD`
  single-month rollover reproduced.
- `holiday_scenery.rs`: koinobori, yatai (3x3 height tables), shrine =
  wishing well (leaf type: Christmas terms 15-16, cherry term 4; lights
  <6:00 / >=18:00; +-22.5 deg talk cone with no wrap-around; town-rating
  replies and the golden-axe/HEM gate; apology item refusals; present
  glide with accel/brake), lotus (palette by term, flower shown
  May 26 - Aug 25, bobber-hit sway with retail `add_calc`), tukimi.

**PC-port bug found: `lbRTC_IsEqualDate` on little-endian.** The function
compares `union { int raw; lbRTC_ymd_c ymd; }`. On mingw32 i686 `raw` is
day-major, so 2002-01-20 vs 2002-02-05 returns OVER (retail LESS);
reproduced with MSYS2 mingw32 gcc. 19 call sites are affected: go-home
visit/deletion, turnip prices (`m_kabu_manager.c`), fish records, bridge,
`m_soncho.c`, and `lbRTC_GetIntervalDays2`. The Rust
`game_time::interval_days_ymd` already orders correctly.
`pc_lbrtc_is_equal_date` is a drop-in correct version; the C side is not
changed.

Other PC divergences noted: tukimi despawns only beyond one block
(borderless) while the identical yatai rule was left at "any other
block"; shrine present speed is dt-scaled and clamped; shrine water
scroll uses a PC `texture_frame`; koinobori cull override skipped in
borderless mode.

C ABI (20): `pc_birthday_ovl_init/play`, `pc_birthday_is_valid_date`,
`pc_birth_control_classify_unit/should_delete/boat_item`,
`pc_evmn_get_map_idx/get_event_block_kind/get_joint_npc_max/
get_event_set_ut_in_block/get_npc_idx_random/set_npc_joint_random`,
`pc_lbrtc_is_equal_date`, `pc_gohome_check_birth/talk_msg_no`,
`pc_shrine_leaf_type/lights_on`, `pc_lotus_pal_no/flower_visible`,
`pc_scenery_should_despawn`. `cargo check` (host and
i686-pc-windows-gnu) and `cargo check --tests` are clean apart from the
crate-standard "never used" warnings; clippy is clean apart from two
indexed loops kept to mirror the C tables.

### Runtime Port Progress: Engine-Level and Misc Modules (logic/data parts)

By claude (task `port-engine-level-modules`). Sources: ac-decomp @ 09ca8e8
(primary; 102 files extracted with `git show`); ACGC-PC-Port (secondary,
diffed: 19 of the 61 .c/.c_inc files differ, mostly 60 Hz dt scaling);
hunter-r.com (video-only, nothing on these systems). Constants were checked
by compiling the C enums and macros with MSYS2 mingw32 gcc. Hand-copied
tables were checked against the C by script (rumble programs, HRA birth
points and letters, the warning catalog, the camera mic table). Drawing,
`GAME_PLAY` writes and hardware calls stay in C. These modules port the
decisions, state machines, math and tables, with no C rewiring.

New shared base:
- `game_math.rs`: `m_lib.c` (chase_*, add_calc*, add_calc_short_angle2/3,
  check_percent_abs, accel/brake, get_percent), libultra `sins`/`coss`,
  `sinf_table`/`cosf_table` (single precision, s16 wrap), `atans_table`,
  `f32_is_zero` (the real `F32_IS_ZERO` threshold of 0.008), and an
  `Xyz` type.
- `game_math_tables.rs`: generated by `tools/gen_trig_tables.py`.
- `holiday_scenery.rs` now reuses these helpers instead of private copies.

Modules:
- `controller.rs`: `mCon_calc` stick normalisation (dead zone
  9.899495, max 61); the `mVibctl` attack/sustain/release rumble mixer
  with all 16 programs and force-stop flags.
- `lights.rs`: point light → directional conversion (`1 - (d/r)^2`), N64
  point-light `kq`, the 7-light cap, and the 32-slot pool. Preserves the
  `Light_list_buf_delete` double-division cursor quirk.
- `view.rs`: viewport (the game draws 320×240 logical coordinates on a
  2× framebuffer), init defaults (retail `center.x ×3` copy-paste bug),
  the look-at nudge, the stretch effect, and ortho selection.
- `actor_shadow.rs`: sun angle by clock, height falloff, radius rules
  (fish vs non-fish), slide scroll, per-scene Y lift. Retail
  `SetTexScroll` lacks a `return` (fixed on PC).
- `engine_tables.rs`: the 246-entry actor profile table (generated;
  slot 185 is null; IDs verified with gcc), BG item respawn, BG texture
  buffers, pre-NMI line, the insect/fish spawn scheduler
  (`ac_set_manager`), waterfalls, and the empty Box*/Fieldm stubs.
- `game_string.rs`: `mFont_UnintToString`, `CutLeftSpace`,
  `Get_Length_String`, string-ROM copy semantics, clock strings, land
  name/ID rules, e-Reader CRC32 (seed 0xAA478422, no final XOR) and
  card-letter bits (the last card reports 2).
- `demo_actors.rs`: train arrival (shared), the intro script, the present
  priority (birthday NES 0x1DB0, lighthouse, golden rod 0x223C / net
  0x2239).
- `hand_over_item.rs`: hand-over keyframe tables (item/fish/mail),
  segment interpolation, and the clip protocol.
- `field_effects.rs`: water-sound nearest-two selection, the frog window
  (May–Aug, 18–21h and 4–6h), beach wave/colour math, tree shake/chop
  timing, and Katrina's fortune tent (`ac_buggy`).
- `hud.rs`: item bubble, bell counter (wallet + bell bags; shop scene
  list verified), and the flip clock (digit flips plus visibility
  debounce).
- `room_scoring.rs` + generated `room_scoring_tables.rs`
  (`tools/gen_room_scoring_tables.py`): feng shui
  (`mHsRm_HuusuiRoomOvl`) and the full Happy Room Academy scorer, letter
  choice and daily schedule. HRA bugs preserved: `CountCharactor`, the
  swapped theme recommendation category, and "hutaba".
- `menu_overlays.rs`: clock-setting edit/validate/hand spin. **Changing
  the time in the menu sets `cheated_flag`**; the PC year range is
  2001–2099, retail 2001–2030. Also the warning catalog (slots 5–8 are
  empty, including `SELL_QUEST_ITEM`), the diary prompt, ticket/wisp/
  stationery stacking (only the held item's condition is checked), bell
  bags, and the card-mail page turn (40 frames, swap at 20).
- `camera.rs`: goal distance/direction per mode (620 / 290 talk),
  indoor zoom steps, block-edge swing, polar eye, border box and block
  ratio, culling gates/area, and the mic-fix table. The per-mode state
  machines stay in C.

Kept in C (documented, not ported): GBI/display-list emission in every
module; `m_tag_ovl` (8,946 lines of inventory context-menu UI; only its
bell-bag rule was ported); the `m_hand_ovl` drop/open flows; the
`m_camera2` per-mode procs; `m_bg_tex` (BSS layout only).
`m_game_dlftbls` was already covered by `scene.rs`.

Other PC-port notes:
- The fortune tent, like the festival stall, kept the retail "despawn
  when the player leaves the acre" rule, unlike tukimi.
- The camera hint shows indoors only because PC allows the C-stick
  camera everywhere.
- Flagged: `furniture.rs` `ftr_idx_2_item_no` omits the 0x1000/0x3000
  bases that `mRmTp_FtrIdx2FtrItemNo` adds.

C ABI (8): `pc_math_sins`, `pc_math_atans_table`,
`pc_math_check_percent_abs`, `pc_mcon_calc`, `pc_vibctl_program_len`,
`pc_actor_dlftbls_num`, `pc_present_demo_follow_rate`,
`pc_intro_demo_start` (all unique).

`cargo check` (host + i686-pc-windows-gnu, lib and tests) is clean.
Clippy is clean except intentional indexed loops. Tests are written but
not run.

## Buildings, scenery and household actors (2026-10-11, claude)

Research covered 33 actor files in ac-decomp (primary), diffed against
ACGC-PC-Port (secondary; 21 files differ, mostly dt scaling). The pure
logic and data are in three new modules. Model, collision and display
code stays in C.

- `buildings.rs`:
  - Nook's Nook 'n' Go / Nookway door parameters, player check and
    closed-shop message choice. Nookway's "open" fallback uses its
    closed text.
  - Lighthouse night window, door test and beam colour pulse; the
    lighthouse switch status, reach test and pole spin model.
  - Cottage door/light rules, including the AUS rule. The tent's light,
    plus its never-despawn bug.
  - Statue offsets and sparkle timers, the talk gate and the eye-texture
    quirk.
  - The structure slot pool, and `STRUCTURE_SETUP`: 83 rows generated
    by `tools/gen_structure_table.py` through a gcc harness.
- `props.rs`:
  - Mailbox opening and the Resetti gate (`reset_count < 3`, AUS `< 4`).
  - Island flag hoist/lower curves.
  - Raffle-drum ball physics.
  - Harvest-table fork probe (cumulative-step quirk).
  - Signboard wobble and block/unit normalisation.
  - Notice/nameplate/dump/flag messages; radio note puffs; sports fair
    variants; sewing-machine cloth; debug moving platform.
  - Post office e-Reader letter parsing (256-byte buffer).
  - Title logo constants and the PRESS START pulse.
- `kk_credits.rs` (`ac_mikanbox` clip): K.K. show text-roll pages and
  alpha, song choice from the uncollected aircheck bits (52 songs, reset
  when all are owned), show timer (4800 frames), and the 20:45–20:55
  cameo of `SP_NPC_MAJIN5` in frames [3600, 3610). 50 Hz values are
  included.

Quirks and bugs:
- Cottage door angle: `y > -DEG2SHORT_ANGLE(180.0f)` is +32768 under
  GCC, so the check never passes there (verified at -O0 and -O2). The
  PC port patches it to -32768. Retail Metrowerks evidently folds it
  differently. `my_cottage_can_enter` exposes both behaviours.
- Tent: the despawn test reads both blocks from the tent's own position,
  so the tent never despawns.
- `ac_windmill` has no constructor and never sets `action_proc`, so its
  move would call NULL. It is dead N64-era code.
- The mailbox destructor frees only one of its two skeletons.
- The e-Reader letter "password" mail type never applies:
  `ITEM_NAME_GET_TYPE(npc_id == NAME_TYPE_SPNPC)` is always 0.
- Statue eyes: the girl face-7 entry points at the boy face-7 texture.
- K.K. songs on house radios are not played by `ac_radio`; that is
  `aMR_MiniDiskCommonMove` (my_room; see `radio.rs`). `ac_radio` is the aerobics radio.

PC-port notes:
- `TOUDAI_ACTOR` outgrew the structure slot, so the slots are 0x300
  bytes.
- `ac_dump` lost its acre-change despawn, and `ac_radio` despawns only
  when more than one acre away.
- The title logo gained a PC settings menu and reloads the save after
  the title demo.

C ABI (3): `pc_toudai_night`, `pc_str_setup_profile`,
`pc_mkbc_roll_alpha` (all unique).

`cargo check` (host + i686-pc-windows-gnu, lib and tests) is clean.
Clippy is clean for the new files. Tests are written but not run.
Tables were script-checked against the C.

## Calendar and radios (2026-10-11, claude)

Research covered `m_calendar.c`, `m_calendar_ovl.c` (+ draw), `ac_radio`,
`ac_radio_test`, `ac_sum_radio01/02` and the music code in `ac_my_room`.
The primary source was ac-decomp, diffed against ACGC-PC-Port.
`m_calendar.c` is unchanged on PC; the overlay and radios differ only in
dt/60 Hz handling, plus the radio's borderless despawn rule. Two new
modules hold the logic and data. Drawing stays in C.

- `calendar.rs`:
  - The 0x68-byte save record (`PlayerCalendar`) with its played-day and
    attended-event bits and the moving-event flags.
  - Month and day wiping when the clock moves (`check_delete`,
    `clear_interval`, `clear_day`), plus `welcome_on`, `event_on` and
    `event_check`.
  - The 36-month grid builder: 16 fixed-date events, 8 floating ones,
    Sunday fishing tourneys in June and November, equinox sports fairs,
    Town Day, the harvest moon, the birthday, and played/attended icons
    for the last 12 months.
  - Input: stick areas, page flips (+-11 months, C-up returns to now),
    the day cursor and event-list scrolling, cursor blink, and the diary
    hand-off with the lock warning.
- `radio.rs`:
  - K.K. disc BGM (`BGM_MD0 128 + n`, 55 discs), the random song pick
    when a stereo is placed, and removal of songs the music box no longer
    owns.
  - The room BGM reserve/change state machine, with the sound calls
    returned as `BgmOp`s.
  - The aerobics radio (BGM 27) furniture and actor, the 36-frame effect
    counters, the despawn rule (retail vs PC), the room-clock ticks, and
    16 stereo models mapped to speaker presets (`Conpo` 1-16).

Bugs and quirks:
- The decomp's `mCD_FLAG_MOTHERS_DAY` is really Father's Day (it is set
  by event 11 and lives in June).
- Father's Day attended icon: US/JP shows it on the Saturday before
  (`lbRTC_Week(day + 1)`). The AUS fix (`lbRTC_Week(day)`) misses a
  Father's Day that falls on the 21st.
- `mCD_calendar_check_delete` wipes the *current* player's calendar when
  the checked calendar is unset or a year or more stale. So opening
  another resident's old calendar erases your own history.
- A Feb 29 birthday in a non-leap year lands in cell 37 (out of range).
- `aMR_ChangeMDBgm` deletes the positional voice even after the aerobics
  track, which never made one.
- Correction to the previous section: K.K. discs play via
  `aMR_MiniDiskCommonMove`; `aMR_RadioCommonMove` is the aerobics radio.

C ABI (2): `pc_cd_event_check`, `pc_mr_conpo_index` (unique).
Event ids (29), and both event tables, were script-checked against the
C. Furniture and BGM ids were evaluated with gcc. `cargo check` is clean
(host + i686, lib and tests), with no clippy findings in the new files.
Tests are written but not run.

### Runtime Port Progress: Fireworks stall vendor (yomise)

New `rust/src/yomise.rs`, registered as `mod
yomise` in `rust/src/lib.rs`. Decomp-verified
against `ac_ev_yomise.c`,
`ac_ev_yomise_move.c_inc`, and
`include/ac_ev_yomise.h` (ACGC-PC-Port tree,
GAFE01_00 Rev 0 USA base). 22/22 sections
PROVEN, with corrections.

The fireworks stalls are three connected
systems: the stall prop actor (already in
`holiday_scenery.rs`), the vendor NPC
purchase state machine (this module — it had
zero Rust equivalent), and the event
scheduler/manager (stays in C).

Ported (pure kernels; save-area
reservation, RANDOM(), inventory/Bells
mutation, the mDemo_* protocol,
aNPC_DEMO_GIVE_ITEM, NPC lifecycle,
placement/deletion, bg offsets, message
resources, choice-window ops, and the
event-manager callbacks stay in C):

- Stock price/message table (780/680/480
  Bells; 0x1758/0x1759/0x175A; 0x1757 when
  empty).
- Stock init from an injected RNG kind:
  `goods[j] = base[kind] + j` over the three
  8-contiguous families (0x2244/0x224C/
  0x2254).
- Nonempty-stock counting, choice-to-stock
  index resolution (choice 3 is the
  sentinel → -1), and the set_choise_data
  page scan (max==3 fallback literals,
  next-start return).
- Next-talk-action selection
  (TALK_END vs TALK_ADD_ACTION) and the
  sentinel/page continuation messages
  (0x1761/0x1762/0x1763/0x1767).

Brief corrections: seven pule map
variants (not three); 4 normal NPCs / 7
variants / 7 total (not 5/1/8); block kind
mRF_BLOCKKIND_POOL (not shrine).

C ABI (all `pc_`-prefixed, 9 exports):
`pc_yomise_stock_price`,
`pc_yomise_opening_msg`,
`pc_yomise_init_stock`,
`pc_yomise_nonempty_count`,
`pc_yomise_choice_to_stock`,
`pc_yomise_next_after_purchase`,
`pc_yomise_choice_scan`,
`pc_yomise_sentinel_msg`,
`pc_yomise_page_msg`. No C callers rewired
yet, per the usual pattern. All 5 unit
tests pass.

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

## Non-Letter NPC Mail (m_npc.c)

Separate from normal letter replies (`npc_reply.rs`), retail has four
independent NPC-generated mail systems plus the password-mail protocol,
all hubbed through `m_npc.c`. New module: `rust/src/npc_event_mail.rs`.

### Event (Valentine) mail

- Relationship classifier (`mNpc_SendEventPresentMailSex`): the
  villager's best friend vs best opposite-sex friend ->
  BEST_FRIEND (0) / OK_FRIEND (1, friendship >= 80) / NOT_FRIEND (2).
- Present tables: {RARE, UNCOMMON, COMMON} x {FURNITURE, FURNITURE,
  CLOTH} indexed by class.
- `mail_no = 0x60 + looks * 3 + type`; FREE_STR0 = player,
  FREE_STR6 = NPC; random ABC paper.
- Scheduler (`mNpc_SendVtdayMail`): classify all 15 villagers, then
  process classes BEST -> OK -> NOT (not villager order). A
  player-level bitfield (starts 0b1111) drops a player from all later
  attempts after one failure; the routine ends when it hits 0.
- Delivery: house mailbox preferred, post-office fallback.

### Birthday mail

- `mail_no = 0xEA + looks * 3 + RANDOM(3)`; FREE_STR0/1/2 =
  player/NPC/item.
- Present: RANDOM(5) over {FURNITURE, FURNITURE, CLOTH, CLOTH,
  umbrella} = exactly 40/40/20; furniture/clothing via RARE list,
  umbrella via `mSP_RandomUmbSelect`.
- Eligibility: player must be the highest-friendship memory and not
  the recorded `birthday_present_npc`; every eligible villager sends
  (animals[] order), mailbox -> post-office fallback.

### Christmas mail

- Fixed: ROM handbill 0xD7, XMAS type, Famicom present, festive paper
  22. No RNG, no personality lookup. Mailbox only — no post-office
  fallback. Deliberately bypasses the common constructor.

### Goodbye mail

- Pending static record `{npc_id, deliver_to_bitfield}` survives the
  villager's `Animal_c`; 4-bit recipient mask over active players.
- `mail_no = 0x20E + looks * 3 + mQst_GetRandom(3)` (quest RNG, not
  RANDOM); FREE_STR0/1/3 = player/NPC/town; no present.
- Retry loop: bits clear on successful delivery, persist on failure.

### Password (HP) mail

- Per-villager `hp_mail[4]` slots `{receive_time, password[20]}`;
  incoming NPC-addressed mail is intercepted by `mNpc_ReceiveHPMail`
  before ordinary processing. Retail `@BUG`: the non-BUGFIX path
  copies the full password-data length into the 20-byte field.
- Responses delayed >= 1 day (interval-days); six generators
  dispatched by password type (Famicom, Popular, Card E, Magazine,
  User password, Card E mini).
- Message ranges: Famicom 0x24A/0x250 + looks; Popular
  0x256/0x25C/0x262/0x268+code/0x288 + looks; Magazine
  0x2A0/0x2A6/0x2AC + looks with hit table {80,60,30,0,100};
  Card-E hit table {80,60,40,20}. Invalid passwords still get
  personality-specific mail. Popular/Card-E overwrite the displayed
  sender with the password's NPC.
- Slots clear only after successful post-office delivery.

### Shared pieces

- `load_npc_mail_data_common2` (font = received, normal type, NPC
  sender, player recipient, present, paper), `mNpc_GetPaperType`
  (random ABC paper), and the three delivery policies
  (mailbox->PO / mailbox-only / PO-only). Generation and delivery
  are separate types (`GeneratedMail` vs `DeliveryResult`).

C ABI: `pc_npcmail_event_no`, `pc_npcmail_famicom_no`,
`pc_npcmail_popular_no`, `pc_npcmail_magazine_no`,
`pc_npcmail_goodbye_no`.

`cargo check --lib` clean. Tests written but NOT run (standing rule).
Second verification pass dispatched at implementation time.

## Graphics Pipeline (GBI / emu64)

Retail Animal Crossing's renderer is a hybrid: the game keeps an
N64/libultra-derived `Gfx` command representation and interprets it at
runtime through the `emu64` taskstart loop, translating state into
GameCube GX/TEV/texture operations. It is closer to a runtime
compiler for an N64-derived graphics language targeting Flipper GX
than to a conventional GameCube renderer.

New module: `rust/src/graphics/` (registered as `pub mod graphics`).

### Architecture

- `command.rs`: `Gfx` word layout, retail F3DEX_GBI opcode values,
  `G_FIRST_CMD = 0xCE`, `NUM_COMMANDS = 64`, `G_DL` params
  (PUSH=0/NOPUSH=1/GXDL=2), Dolphin texture-command vocabulary.
- `interpreter.rs`: the `emu64_taskstart` loop — fetch, decode,
  dispatch through the handler table, mutate state, next command.
  Real DL call stack (18 levels, overflow counted like retail),
  segmented-address resolution, `G_DL` with a native-GX-display-list
  target handed to the backend (`GXCallDisplayList`), `G_ENDDL`
  pop-or-terminate semantics. Dispatch index is the WRAPPING u8
  subtraction `opcode - G_FIRST_CMD` (verified against retail
  `dl_func_tbl`: indices 51-63 hold the RSP geometry handlers, so
  `G_VTX` (0x01) lands at index 51, `G_TRI1` (0x05) at 55, `G_TRIN`
  (0x09) at 59, `G_QUADN` (0x0B) at 63). Also exposes a `trace:
  Vec<CmdTrace>` diagnostic (per-command opcode, DL level, dirty
  flags, geometry mode, prim color) used by the showcase generator.
- `segments.rs`: 16-entry segment table (`gSPSegment`), `seg2k0`
  resolver, DL stack.
- `state.rs`: canonical N64-era GBI state (geometry mode, othermode
  hi/lo, combiner, tiles, TLUTs, colors, fog) with `emu64`-style
  dirty flags; GX translation happens behind them.
- `texture.rs`: verbatim `fmtxtbl[8][4]`, `cvtN64ToDol` (0xFFFF ->
  GX_TF_I4 fallback), verbatim `rgba5551_to_rgb5a3`, block sizes
  (4b: 8x8, 8b: 8x4, 16b/32b: 4x4), the TMEM bank/word swizzle
  `ofs ^ ((block >> 1) & 4)`, IA8 nibble rearrange, tile conversion,
  source-address-keyed conversion cache (with invalidation for
  dynamic textures), TLUT conversion, and the 32x32 4-bit
  player-design texture (512 bytes).
- `combine.rs`: N64 combiner -> TEV translation with the Auto/Tev/
  Manual/NotEv/Preconverted paths; TEXEL1 mentions fall back to
  NOTEV; 1-cycle vs 2-cycle stage counts.
- `vertex.rs`: `Vtx` decode (s16 -> f32, packed normals), normal
  normalization under `G_TEXTURE_GEN`, shared/nonshared matrix flag
  (texture-gen forces shared), TRI1 index/2.
- `frame.rs`: the nine display-list streams (POLY_OPA, POLY_XLU,
  OVERLAY, WORK, FONT, SHADOW, LIGHT, BG_OPA, BG_XLU), material
  presets modeled on `z_gsCPModeSet_Data`, and the customized NTSC
  render modes (640x480, XFB 660, viXOrigin 30, interlaced DF filter
  8/8/10/12/10/8/8, progressive SF filter 0/0/21/22/21/0/0).
- `gx_backend.rs`: GX display-list 32-byte alignment, `GXTexObj`
  model, tiled-storage tile sizes, projection compression (6 values
  to XF 32-37 + type at 38), XF register bases, EFB->XFB copy,
  EFB->texture capture (wipe path).

### Packed geometry

`G_TRIN`/`G_TRIN_INDEPEND` use the verbatim `POLY_GET_V*_5b/7b`
extraction macros; 5-bit mode packs 3 faces first then 4 per word,
7-bit mode packs 3 per word; face count is `((w0 >> 17) & 0x7F) + 1`.
`G_TEXRECT` consumes an extra Gfx word.

### Preserved retail behavior

- DL stack overflow is counted, not fatal; the jump still happens
  with the return address lost.
- Dolphin texture commands are the same opcodes with the
  `isDolphin` bit set; Dolphin image dims are `wd+1 x (ht+1)*4`.
- Texture identity is the source RAM address, not the texel hash.

### Engine boundary

Actual GX FIFO emission, EFB/RAM emulation, the Dolphin GX library,
and ROM model/texture assets stay engine-side. The interpreter
emits triangles/quads/texrects/GXDL calls through the `GxBackend`
trait and reads display-list/vertex bytes through `GfxMemory`.

C ABI: `pc_gbi_emu64_create/destroy`, `pc_gbi_set_segment`,
`pc_gbi_rgba5551_to_rgb5a3`, `pc_gbi_n64_to_gx_format`,
`pc_gbi_block_dims`, `pc_gbi_tmem_swizzle`, `pc_gbi_gx_dl_valid`,
`pc_gbi_cmds_processed`.

`cargo check --lib` clean. Tests written but NOT run (standing rule).
Second verification pass dispatched at implementation time.

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

## Wave 2: C-struct boundary corrections (2026-10-08)

Research brief + independent verification against the USA Rev. 0 decomp
(`acgc-upstream`). Rule established: **Rust almost never owns the retail
RNG call** — C does `r = RANDOM(n)` and passes the bounded value (or a
callback) into the Rust kernel. Verified verbatim against decomp sources:

- `aQMgr_decide_idx_prob_table` (`ac_quest_talk_normal_init.c:176`):
  100-entry table, 30x `RANDOM(100)` swap pairs, final `RANDOM(100)`
  pick — 61 RNG calls, not a weighted selector.
- Mail-receive friendship (`m_npc.c`): `+3`, `-5` if BAD rank, `+3` if
  present → good/+3, good+present/+6, bad/-2, bad+present/+1.
- `mQst_GetGoods_common` (`m_quest.c:926`): `RANDOM(10)` furniture
  branch, exactly as the brief described.
- `mFI_LineDepositON/OFF`, `mFI_GetLineDeposit` (`m_field_info.c`):
  operate on a `u16*` row pointer + `ut_x` bit ops.
- `game_get_next_game_dlftbl` (`graph.c:81`): init-function-pointer
  comparison chain, not a scene-ID lookup.
- Shop thresholds (`m_shop.h`): 25000 / 90000 / 240000.
- Wander borders (`ac_npc_think_wander.c_inc:80-86`): girl {3,6},
  ko-girl {6,8}, boy {5,7}, sport-man {2,4}, grim-man {3,6},
  naniwa {4,8} — match the Rust tables.
- `aNPC_chk_avoid_and_search` (`ac_npc_move.c_inc:505`): same-block
  gate, `*friendship + over_friendship`, `< 0` avoid, `> 128` search.
- `mNpc_GetOverImpatient` / `mNpc_TalkEndMove` (`m_npc.c`): the
  `l_npc_temper` table is declared per-feeling but indexed by `looks`
  (personality) — a genuine retail quirk, now preserved.

Corrections applied to the Rust code:

- `request_selector.rs`: `pick_random_eligible` takes `&[u8]`
  (nonzero = eligible) — no `Vec<bool>` allocation across the FFI
  boundary. `decide_idx_prob_table` now runs the verbatim retail
  shuffle, delegating to `talk_topics`'s shared implementation; the
  RNG arrives as a C callback (`Option<extern "C" fn(u32) -> u32>`,
  61 calls in retail order) because retail `RANDOM` is a macro — C
  passes a 3-line wrapper. **ABI change** on `pc_request_dispatch`
  (was `(probs, n, roll)`); nothing is wired yet.
- `behavior.rs`: `letter_friendship_delta(good, present)` implements
  the retail `+3 / -5 / +3` formula; the `m_quest.h`
  `LETTER_SCORE_BONUS`/`LETTER_PRESENT_BONUS` constants belong to the
  quest-*rank* path (`m_quest.c:660`), not friendship — the old code
  conflated the two systems.
- `player_move.rs`: `0.01f32` is already bit-identical to retail's
  `0.01f` (0x3C23D70A); the brief's `0.0099999998f` is the same float
  printed in exact decimal — no change, documented so nobody
  "fixes" it. `pc_locomotion_core` documented as a classification
  helper, not a movement replacement.
- `buried_items.rs`: added `pc_buried_line_get/set/clear` — the exact
  retail row-pointer boundary; the whole-array variants are marked as
  rewrite-side convenience, not the retail boundary.
- `interaction.rs`: temper table now indexed by `looks` (personality)
  per the retail quirk; `pc_npc_patience(talk_num, looks)`.
- `item_prefs.rs`: `select_reward_furniture` takes an already-bounded
  `selected_index` (`RANDOM(n)` from C), not an unbounded RNG word.
- `scene.rs`, `npc.rs`, `collision.rs`, `town_gen.rs`: boundary
  documentation — pointer scan stays in C, `saved_type` bridge,
  sub-operation scope, and `TownPlan` not replacing
  `mRF_MakeRandomField`.

Not changed (brief items for Philip's C tree, not this repo):
CMake Rust-dependency list, the actual `#ifdef USE_RUST` C rewires,
`PcPlayerMoveState` struct bridge (Wave 2D).

## Wave 3: engine plumbing (2026-10-08)

Build/linkage migration, not call-site rewiring: the Wave 3 Rust
modules already export the same ABI names as the PC C platform
replacements, so the plug is to stop compiling the C versions and let
the Rust archive satisfy the existing C calls. Verified symbol
coverage 1:1 for all seven modules (mtx has 2 extra Rust-only
exports, `PSVECDotProduct`/`PSVECMag`, which is harmless).

ABI corrections (verified against decomp + PC layer):

- `aram.rs`: `ARFree` now returns `()` to match the PC layer (`void
  ARFree(u32*)` in `pc_aram.c`), which is the ABI being replaced.
  The decomp's Dolphin header guesses `u32 ARFree(u32*)` but flags it
  "Unused/inlined in P2" (no retail implementation to check); no C
  callers of `ARFree` exist in the tree.
- `vi.rs`: `VISetPreRetraceCallback` / `VISetPostRetraceCallback` now
  take and return `*mut c_void`, matching `pc_vi.c` (`void*
  VISetPreRetraceCallback(void* cb)`). The Dolphin header's
  `VIRetraceCallback` (fn(u32)) is not used — the PC layer treats the
  callbacks as opaque, and the Rust side only stores them.

CMake (`CMakeLists.txt`):

- Wave 3 exclusions added for the green-lit modules: `pc_aram.c`,
  `pc_dvd.c` + `pc_disc.c` (migrated as one cluster — `dvd.rs`
  calls the Rust `pc_disc_*`), `pc_gbi_runtime.c`, `pc_profiler.c`.
  The C hot-path profiler inline wrappers keep calling the Rust
  `*_slow` exports; headers unchanged.
- `pc_mtx.c` / `pc_vi.c` exclusions are present but commented out:
  matrix needs the `f32.to_bits()` differential suite first
  (floating-point op-order sensitivity), VI goes last after the
  boot/game loop is validated on the Rust profiler.
- The hard-coded Rust `.rs` DEPENDS list is replaced by
  `file(GLOB_RECURSE PC_RUST_SOURCES CONFIGURE_DEPENDS
  "rust/src/*.rs")` so the archive rebuilds when any module changes.

Deliberately untouched: `pc_platform.c` (SDL/GX/audio stay C — Rust
`vi.rs` still calls `pc_platform_poll_events`,
`pc_platform_swap_buffers`, `pc_gx_draw_pending`,
`pc_audio_get_buffer_fill`); the profiler's C-owned emu64 counters;
the already-wired `letter_score.rs` (`mMck_check_key_hit`,
`mMck_check_key_hit_nes`).
