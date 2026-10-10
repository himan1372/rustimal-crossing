# rustimal-crossing roadmap (2026-10-10)

Claimable tasks live in `backlog/` (one file per task). Claim in
`.claude-agents/claims.jsonl` before starting work; announce claims,
completions, blockers, and questions on `comms.txt` (append-only).

## Bugs / fixes
- [ ] **acre-boundary-bug** (backlog) — pre-existing escape bug; Philip's chosen next item. Research done, workbook row 259 drafted.
- [ ] **audio-issues** (backlog) — money-pickup ching, wrong dig sound, museum music stacking; likely Rust audio port.
- [ ] **wave1-playtest-fixes** (backlog) — lowercase "g", lighting stutter, hidden inventory slot, wrong-acre landmarks, dragonfly swarms.
- [ ] Name filter rejects "phili" — BLOCKED on Philip's decision (remove offending substring vs whole-word matching in m_editor_ovl.c:1154).

## Wiring
- [ ] **wire-c-callers-to-rust** (backlog) — most Rust modules export C ABIs with no C callers rewired yet; follow wave3-runbook.md.

## Task board (already claimed / done)
- port-title-screen (backlog, unclaimed) — reworded for the PC port (no Bevy).
- setup-bevy-rust-skills — claimed by lapis.
- write-brp-runbook — claimed by cosmo.
- document-asset-pipeline — done by muse (`.claude/skills/asset-pipeline/SKILL.md`).

## Parked (do not claim until Philip says so)
- Ecology wiring — Philip: "dont do it just yet".
- Quirks workbook row.

## Docs / infra
- Live workbook `outputs/gamedb_research/` (Philip's Windows machine) through row 264; row 259 drafted, not pasted.
- `acdc` repo still empty — C decomp base not pushed anywhere reachable.
