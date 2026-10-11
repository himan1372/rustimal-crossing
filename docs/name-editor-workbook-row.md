# Workbook row: Name editor, character entry, NG-word filter

> Paste into `outputs/gamedb_research/gamedb_research.xlsx` on the local machine.
> Date: 2026-10-11. Author: lapis (with philip's research brief).

## Research findings

**File responsibilities (verified):**
- `m_editor_ovl.c` — shared character-entry engine: keyboard, cursor,
  insert/delete, width limits, NG-word check.
- `m_ledit_ovl.c` — single-line name-entry wrapper: picks destination
  buffer, capacity, title; opens the shared editor as `mED_TYPE_LEDIT`.
- `m_editEndChk_ovl.c` — generic "Is this OK?" confirmation overlay
  (Yes / Rewrite / Throw it out). NOT the NG filter; NOT auto-opened by
  the name editor.

**Sources verified:** `src/game/m_editor_ovl.c`, `src/game/m_ledit_ovl.c`,
`include/m_editor_ovl.h`, `include/m_ledit_ovl.h`, `include/m_font.h`
(GAFE01_00 Rev 0 USA, via public decomp `ACreTeam/ac-decomp`).

### NG-word filter (`mED_ng_word_check`, `m_editor_ovl.c:1088`)

- Constants: `mED_NG_WORD_START = 0x74A`, `mED_NG_WORD_NUM = 14`.
- Lengths (hard-coded table): `4, 4, 4, 4, 4, 5, 5, 7, 7, 7, 7, 7, 9, 10`.
- Patterns are game assets loaded via `mString_Load_StringFromRom`
  (resource IDs `0x74A`..`0x757`) — NOT in the C source. Contents must be
  extracted from the USA build; do not invent them.
- Algorithm: for each pattern (0..13), for each start offset
  `0..=(input_length - len)`, byte-compare. Substring scan over the FULL
  buffer capacity (`input_length`), not the logical text length. No case
  folding, no normalization, no word boundaries.
- On match: `mED_clear_input_data()` (fill `input_length` bytes with
  `CHAR_SPACE`, zero `now_str_len`/`cursor_idx`/`_22`/`_26`), return TRUE.
  The caller plays error sound `0x1003` and does NOT exit.
- Only runs when `menu_info->data0 == mED_TYPE_LEDIT`.
- Called after `mED_all_space_check()`, before `mED_end_edit_func()`.

### Buffer model

- Fixed-capacity, space-filled byte buffer (`CHAR_SPACE = 32`,
  `include/m_font.h`). Three distinct values: capacity (`input_length`),
  logical length (`now_str_len`), cursor (`cursor_idx`). Not NUL-terminated.
- `mED_clear_input_data`: fills `input_length` bytes with `CHAR_SPACE`;
  zeroes `now_str_len`, `_22`, `cursor_idx`, `_26`.
- `mED_all_space_check`: rejects (clears + TRUE) if the first
  `now_str_len` bytes are all spaces; catchphrase has a data2-buffer
  special case (stays in C).

### Entry types (`m_ledit_ovl.h`, exact order)

0. `mLE_TYPE_PLAYER_NAME` — player name
1. `mLE_TYPE_COUNTRY_NAME` — town name
2. `mLE_TYPE_EPHRASE` — catchphrase (temp buffer `mLE_npc_end_word_edit`)
3. `mLE_TYPE_RESET`
4. `mLE_TYPE_REQUEST` — song request (minidisk name lookup)
5. `mLE_TYPE_MYORIGINAL_NAME`
6. `mLE_TYPE_ISLAND_NAME`

On open: destination selected, buffer cleared with `CHAR_SPACE`
(`mem_clear`), shared editor opened as `mED_TYPE_LEDIT`. Player/town
buffers are deliberately NOT preserved.

### Keyboard

- `mED_COLUMNS = 10`, `mED_ROWS = 4` → 40 positions per table.
- Modes: shift LOWER/UPPER; arrange QWERTY/ALPHA; input LETTER/SIGN/MARK.
- `CHAR_SPACE_3` = unavailable-key sentinel (not an insertable space).
- D-pad repeat and analog-stick repeat are SEPARATE mechanisms with
  different timings — do not unify.

## Additions (Rust, pushed to `main`)

**`rust/src/name_editor.rs`** (new):
- `ng` mod: `WORD_START` (0x74A), `WORD_NUM` (14), `WORD_LENGTHS` table,
  `check(input, input_length, patterns)` → `Option<usize>` (first matching
  pattern index; substring scan over full capacity; raw bytes).
- `buffer` mod: `SPACE = 32`, `clear(buf)`, `is_all_spaces(input, len)`.
- `entry_type` mod: 7 types in source order. `keyboard` mod: 10×4, 40
  positions, shift/arrange/mode constants.
- C ABI: `pc_ed_ng_check` (patterns as 14×10 flat buffer from C; returns
  pattern index or -1), `pc_ed_clear_buffer`, `pc_ed_is_all_spaces`.
- Tests: length table, no-match, offset 0, final offset
  (`input_length - len`), substring-in-longer-string, capacity-vs-logical
  scan, first-pattern-wins, all 14 lengths, clear/all-spaces, C ABI.

**`rust/src/lib.rs`**: added `mod name_editor;`.

## Changes / next steps

- [ ] `cargo test` on Windows.
- [ ] Extract the 14 NG patterns from USA assets (resource `0x74A`..`0x757`).
- [ ] Wire `pc_ed_ng_check` into `mED_ng_word_check` behind `#ifdef USE_RUST`
      (C keeps pattern loading + buffer-clear side effects).
- [ ] Later: keyboard code tables, insert/delete, width enforcement
      (`mFont_GetCodeWidth`), wrapper routing, `m_editEndChk_ovl` overlay.
- [ ] Open: numeric values of symbolic capacities; catchphrase data2
      relationship; full caller lifecycle per entry type.

## Confidence

- **Proven:** NG constants, lengths, scan algorithm, clear semantics,
  CHAR_SPACE=32, entry-type order, keyboard geometry — all read from C
  source/headers.
- **Strong reconstruction:** pure-kernel boundary (C loads patterns, Rust
  matches); buffer model separation.
- **Unresolved:** actual 14 pattern contents (assets); symbolic capacities;
  keyboard byte tables.
