# Workbook row: Fish & bug records / collection tracking

> Paste into `outputs/gamedb_research/gamedb_research.xlsx` on the local machine.
> Date: 2026-10-11. Author: lapis (with philip's research brief).

## Research findings

**Key distinction (verified):** `m_fishrecord.c` is NOT the collection
tracker — it implements fishing-CONTEST records (5 dated size entries, NPC
rival generation, winner simulation, Fishing Day scheduling, reward mail).
The persistent museum fish/insect donation state lives in
`m_museum_display.c` / `m_museum_display.h`. Porting `m_fishrecord.c` alone
does not reproduce saved collection state.

**Sources verified:** `include/m_museum_display.h`, `src/game/m_museum_display.c`,
`src/game/m_fishrecord.c` (GAFE01_00 Rev 0 USA, via public decomp
`ACreTeam/ac-decomp`). All tables and formulas below copied from source.

### Museum donation data (`mMmd_info_c`, `sizeof == 0x3F`)

| Field | Size | Contents |
|-------|------|----------|
| `fossil_bit` | 13 bytes | 25 fossils, 4-bit donor each |
| `art_bit` | 8 bytes | 15 artworks, 4-bit donor each |
| `fish_bit` | 21 bytes | 40 fish, 4-bit donor each |
| `insect_bit` | 21 bytes | 40 insects, 4-bit donor each |

**Nibble packing:** byte = `index >> 1`, nibble = `(index & 1) * 4`.
Fish 0 = low nibble of `fish_bit[0]`; fish 1 = high nibble; fish 2 = low
nibble of `fish_bit[1]`. These are packed status fields, NOT byte arrays —
wrong layout corrupts adjacent species.

**Donor enum:** NONE=0, PLAYER1=1, PLAYER2=2, PLAYER3=3, PLAYER4=4,
DELETED_PLAYER=5. Values 6–15 outside the enum.
- `mMmd_IS_DONATED(d)`: `PLAYER1 <= d <= DELETED_PLAYER`.
- Setter writes `Common_Get(player_no) + 1`.
- `mMmd_DeletePresentedByPlayer`: rewrites matching donors to
  DELETED_PLAYER (species stays counted as donated).
- Counts (`mMmd_CountDisplayedFish/Insect`): donor in PLAYER1..DELETED_PLAYER.
- Museum completion needs fossils 25 + art 15 + insects 40 + fish 40, then
  schedules the completion letter (checked in `m_museum.c`).

**Donation check** (`mMmd_GetDisplayInfo`): insects =
`ITM_INSECT00 <= item < ITM_SPIRIT0`, index = `item - ITM_INSECT_START`;
fish = `ITM_FISH00 <= item <= ITM_FISH_END`, index = `item - ITM_FISH_START`.
Returns CANNOT_DONATE / CAN_DONATE / ALREADY_DONATED. No separate
"ever caught" table found — this tracks donation, not catch history.

### Contest records (`m_fishrecord.c`)

- 5 slots: `{ PersonalID pid; lbRTC_time_c time; int size }`.
- `mFR_fish_rndsize(class, rng)` with one `fqrand()` sample:
  - LARGE: `(50.0 + 20.0*rng*rng) / 2.54`, MEDIUM: `(30.0 + 20.0*rng) / 2.54`,
    SMALL: `(30.0 - 20.0*rng*rng) / 2.54` (int truncation).
- `mFR_make_NpcRecord(hour)`: hour<9 → SMALL; 9–14 → `RANDOM(2)`;
  15+ → `RANDOM(3)`. The `RANDOM` call and the `fqrand` size roll are
  separate RNG ops — order matters.
- Winner: NPC size must be STRICTLY greater; ties keep the player.
- `mEv_fishRecord_local` has an explicitly unused `fqrand()` call — do NOT
  remove (affects later RNG state).

## Additions (Rust, pushed to `main`)

**`rust/src/museum_display.rs`** (extended; already owned the nibble record):
- `pub mod collection`: `fish_info` / `insect_info` (validated, 0 if bad
  index), `set_fish_donor` / `set_insect_donor` (bool ok), `count_displayed`,
  `delete_presented_by_player` (fish+insect, rewrites to DELETED_PLAYER).
- Completed `donor` enum: added `PLAYER2 = 2`, `PLAYER3 = 3`.
- C ABI: `pc_md_fish_info`, `pc_md_insect_info`, `pc_md_set_fish_donor`,
  `pc_md_set_insect_donor`, `pc_md_count_displayed_fish`,
  `pc_md_count_displayed_insect`, `pc_md_delete_presented_fish_insect`.
- Tests: nibble packing incl. adjacent-species isolation, invalid indices,
  deleted-donor counting, delete-player rewrite.

**`rust/src/fishrecord.rs`** (new):
- `size_class::{SMALL,MEDIUM,LARGE}` (= 0,1,2); `RECORD_NUM = 5`.
- `fish_rndsize(class, rng_sample)`: exact f32 formulas.
- `npc_size_class(hour, rand2, rand3)`: hour-branch logic.
- C ABI: `pc_fr_fish_rndsize`, `pc_fr_npc_size_class`. Tests for formulas
  (incl. truncation) and hour branches.

**`rust/src/lib.rs`**: added `mod fishrecord;`.

## Changes / next steps

- [ ] `cargo test` on Windows.
- [ ] Wire C callers behind `#ifdef USE_RUST` (save-area access and
      `mLd_PlayerManKindCheck` stay in C; pass validated donor values).
- [ ] Full contest sim / scheduling / reward mail stays in C (needs RTC,
      RNG state, mail system).
- [ ] Open: whether any subsystem keeps a separate "ever caught" record
      (none found in inspected files).

## Confidence

- **Proven:** nibble layout, donor enum, getter/setter validation, count and
  delete semantics, size formulas, hour branches — all read from C source.
- **Strong reconstruction:** module split (collection vs contest).
- **Unresolved:** item-ID constants (`ITM_FISH_START` etc.) for
  `GetDisplayInfo`/`RequestMuseumDisplay` — not yet ported (needs name
  table values); catch-history existence.
