# Villager AI & schedules: gap analysis + port (need-to-port.md "Villager AI & schedules")
- [x] Inventory m_npc.c, m_npc_schedule.c, m_npc_walk.c, ac_set_npc_manager.c, m_quest.c vs existing Rust (npc.rs, npc_ai.rs, quest.rs, quest_gen.rs, villager_home.rs, npc_reply.rs, npc_event_mail.rs)
- [~] (walk + set_npc_manager + schedule done; quest first-job + m_npc subset pending) Research the uncovered logic (schedule tables, daily routine, walking/visits, set-npc manager, quest/favor bookkeeping)
- [ ] Port pure logic/data as new modules (no edits to other agents' files beyond registration); cargo check host+i686
- [ ] Docs, workbook, comms, need-to-port.md; push
Requested by Philip 2026-10-12; owner: claude.
PAUSED 2026-10-12 (usage limit): research notes in workbook rows 335-336; next = write npc_walk.rs + npc_set_manager.rs, then quest first-job + m_npc villager-life subset.
