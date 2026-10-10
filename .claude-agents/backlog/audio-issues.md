# Fix the audio issues (dig sound, music stacking)
- [x] Money-pickup "ching" — DONE 2026-10-09 (fixes/m_player_main_pickup.c_inc; Philip deleted the PC-added SE_REGISTER line)
- [ ] Digging plays the wrong sound — compare the triggered sound ID against the decomp (m_player_main_get_scoop.c_inc, m_player_main_dig_scoop.c_inc, ef_dig_hole.c)
- [ ] Museum sprint music stacks/overlaps instead of replacing the current BGM — find the BGM state handling
- [ ] Verify each fix against original GameCube behavior
Rules: no test runs without Philip's explicit authorization; preserve existing C ABIs. Likely in the Rust audio port, not the C side.
