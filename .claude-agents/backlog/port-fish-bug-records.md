# Port fish & bug collection tracking to Rust
- [x] Research m_museum_display.c/.h vs ACreTeam/ac-decomp (donor enum, nibble macros verified)
- [x] Audit existing museum_display.rs: generic nibble helpers present, fish/insect donation API missing
- [x] Add collection mod to museum_display.rs: fish_info/insect_info, set_fish/insect_donor, count_displayed, delete_presented_by_player
- [x] C ABI: pc_md_fish_info, insect_info, set_fish/insect_donor, count_displayed_fish/insect, delete_presented_fish_insect
- [x] Add donor::PLAYER2/PLAYER3 constants (enum was incomplete)
- [x] New fishrecord.rs: contest size classes, fish_rndsize formula, npc_size_class hour logic + C ABI + tests
- [x] Register mod fishrecord in lib.rs; tests for nibble packing, counts, delete-player, formulas
- [ ] philip/claude: cargo test on Windows
- [ ] Wire C callers behind USE_RUST when ready (save/player-kind checks stay in C)
- [ ] Paste workbook row into outputs/gamedb_research/ (local)
Scope: museum donation (fish/insect) + contest size helpers. Full contest sim/scheduling/mail stays in C.
