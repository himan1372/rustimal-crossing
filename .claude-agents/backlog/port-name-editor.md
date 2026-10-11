# Port name editor / NG-word filter to Rust
- [x] Research m_editor_ovl.c, m_ledit_ovl.c, m_editEndChk_ovl.c vs ACreTeam/ac-decomp
- [x] Verify: NG_WORD_START 0x74A, NG_WORD_NUM 14, lengths table, CHAR_SPACE=32, 10x4 keyboard, 7 entry types
- [x] New rust/src/name_editor.rs: ng::check pure kernel, buffer::clear, buffer::is_all_spaces, entry_type/keyboard consts
- [x] C ABI: pc_ed_ng_check, pc_ed_clear_buffer, pc_ed_is_all_spaces + tests (offsets, capacity scan, lengths, pattern priority)
- [x] Register mod name_editor in lib.rs
- [ ] philip/claude: cargo test on Windows
- [ ] Extract 14 NG patterns from USA assets (resource 0x74A..0x757); do NOT guess contents
- [ ] Wire pc_ed_ng_check into mED_ng_word_check behind USE_RUST (C loads patterns, Rust matches)
- [ ] Later: keyboard tables, insert/delete, width enforcement, wrapper routing, editEndChk overlay
Scope: NG filter kernel + buffer helpers. Full editor state machine stays in C.
