# Workbook row: April Fools controller (ac_aprilfool_control.c → Rust)

> Paste into `outputs/gamedb_research/gamedb_research.xlsx` on the local machine.
> Date: 2026-10-11. Author: lapis (with philip's research brief).

## Research findings

**Source:** `src/actor/ac_aprilfool_control.c` (GAFE01_00 Rev 0 USA), verified
against public decomp `ACreTeam/ac-decomp` (same file, same tables).
`SP_NPC_*` numeric values resolved from `include/m_name_table.h`
(`SP_NPC_START = 0xD000`).

**What it is:** a shared April Fools dialogue manager (control-part actor,
`Aprilfool_Control_Profile`). It tracks, per local player, which of 11
special NPCs have already had their April Fools interaction, and supplies the
replacement message ID to each NPC's existing talk code via three callbacks
installed in `Common_Get(clip).aprilfool_control_clip`.

**Save layout:** `u16 talk_bitfield[PLAYER_NUM]` — one 16-bit mask per player,
low 11 bits used. Per-player, not global. Cleared per-player by
`mEv_ClearPersonalEventFlag` (only the requested player's mask).

**NPC table (bit position = array position — do NOT reorder):**

| Bit | Actor name (hex) | Character | Message ID |
|-----|-----------------|-----------|------------|
| 0 | SP_NPC_STATION_MASTER (0xD00E) | Porter | 0x3BB5 |
| 1 | SP_NPC_SHOP_MASTER (0xD008) | Tom Nook | 0x3BAC |
| 2 | SP_NPC_CURATOR (0xD06D) | Blathers | 0x3BB0 |
| 3 | SP_NPC_NEEDLEWORK0 (0xD070) | Mable | 0x3BB1 |
| 4 | SP_NPC_NEEDLEWORK1 (0xD071) | Sable | 0x3BB2 |
| 5 | SP_NPC_POLICE (0xD00D) | Copper | 0x3BB3 |
| 6 | SP_NPC_POLICE2 (0xD010) | Booker | 0x3BB4 |
| 7 | SP_NPC_POST_GIRL (0xD003) | Pelly | 0x3BAE |
| 8 | SP_NPC_POST_GIRL2 (0xD012) | Phyllis | 0x3BAF |
| 9 | SP_NPC_POST_MAN (0xD011) | Pete | 0x3BAD |
| 10 | SP_NPC_SENDO (0xD072) | Kapp'n | 0x3BB6 |

**Callback semantics (exact):**
- `talk_chk`: returns FALSE only for local player + tracked NPC + bit clear
  (FALSE = "use April Fools dialogue"). TRUE for foreigners, untracked NPCs,
  or already-consumed bits. Callers test `!talk_chk(...)`.
- `talk_set`: ORs the bit; no-op for untracked; never clears other bits.
- `get_msg_num`: returns table message ID (-1 if untracked); if
  `update_talked`, sets the bit first. NPCs differ in when they mark —
  do not unify the timing.
- Event window: April 1, 00:00–23:00 (`mEv_EVENT_APRILFOOLS_DAY`); separate
  mayor event 10:00–17:00 (`mEv_EVENT_SONCHO_APRILFOOLS_DAY`) — do not merge.

## Additions (Rust)

**New file:** `rust/src/aprilfool.rs` (pushed to `main`).
- `npc_name` constants (all 11 `SP_NPC_*` values).
- `npc_index(u16) -> Option<usize>`: linear table search, `None` if untracked.
- `get_npc_bit` / `set_npc_bit`: exact `aAPC_GET/SET_NPC_BIT` macro behavior.
- `talk_chk`, `talk_set`, `get_msg_num`: pure cores with exact C semantics.
- C ABI: `pc_aprilfool_npc_index`, `pc_aprilfool_talk_seen`,
  `pc_aprilfool_mark_seen`, `pc_aprilfool_message_id`,
  `pc_aprilfool_update_and_message`, `pc_aprilfool_talk_chk`.
- Unit tests: all 11 mappings, all message IDs, bit independence (incl. bit 10),
  chk/set semantics, C ABI edge cases (`-1` handling).

**Changed:** `rust/src/lib.rs` — added `mod aprilfool;` after `mod event_day;`.

## Changes / next steps

- [ ] `cargo test -p rustimal` (or workspace test) on Windows to verify.
- [ ] Wire C callers behind `#ifdef USE_RUST` per wave3-runbook pattern
      (keep NPC-specific mark timing as-is per caller).
- [ ] Actor lifecycle, clip registration, event-manager start/stop stay in C
      until Rust equivalence is demonstrated.

## Confidence

- **Proven:** NPC table, message IDs, bitfield layout, callback semantics —
  all read directly from the C source.
- **Not established:** rendered English text for the message IDs; full
  per-NPC talk timing (requires tracing each caller's state machine).
