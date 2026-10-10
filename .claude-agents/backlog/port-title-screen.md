# Port the title screen
- [x] Map the original title-screen flow (trademark demo -> title demo) from the decomp modules
- [x] Port the title-screen logic to Rust (new module under rust/src/), preserving existing C ABIs
- [x] Verify against original GameCube behavior (no test runs without explicit authorization)
Files: src/game/m_trademark.c, src/game/m_titledemo.c, src/data/scene/title_demo.c, src/data/titledemo/
Note: this repo is the C/Rust PC port — Bevy is not used here (see bevy-crossing for the Bevy prototype). Repo-side fixes follow the fixes/ pattern (see fixes/m_trademark.c).
Done 2026-10-10 by muse: rust/src/title_demo.rs (mod title_demo) — stage machine, keydata decoder, frame advance, demono cycling, data tables; C ABI: mTD_demono_get, mTD_get_titledemo_no, mTD_tdemo_button_ok_check, pc_titledemo_decode_keydata, pc_titledemo_advance_frame. DOCUMENTATION.md section added; workbook rows 265-267 drafted.
