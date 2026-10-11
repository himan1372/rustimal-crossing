# Port April Fools controller to Rust (Layer A helpers)
- [x] Research ac_aprilfool_control.c against public decomp (ACreTeam/ac-decomp)
- [x] Resolve SP_NPC_* numeric values from m_name_table.h
- [x] Implement rust/src/aprilfool.rs: NPC table, msg table, bitfield helpers, talk_chk/talk_set/get_msg_num
- [x] Expose C ABI: pc_aprilfool_npc_index, talk_seen, mark_seen, message_id, update_and_message, talk_chk
- [x] Unit tests for all 11 mappings, message IDs, bit ops, chk/set semantics
- [ ] Register mod aprilfool in rust/src/lib.rs (pending push)
- [ ] philip/claude: cargo test on Windows, wire C callers when ready
- [ ] Paste workbook row into outputs/gamedb_research/ (local)
Rules: C ABI preserved, pure helpers only, NPC timing stays in C until equivalence shown.
