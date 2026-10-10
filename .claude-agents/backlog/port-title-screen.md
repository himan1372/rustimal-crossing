# Port the title screen
- [ ] Map the original title-screen flow (trademark demo -> title demo) from the decomp modules
- [ ] Port the title-screen logic to Rust (new module under rust/src/), preserving existing C ABIs
- [ ] Verify against original GameCube behavior (no test runs without explicit authorization)
Files: src/game/m_trademark.c, src/game/m_titledemo.c, src/data/scene/title_demo.c, src/data/titledemo/
Note: this repo is the C/Rust PC port — Bevy is not used here (see bevy-crossing for the Bevy prototype). Repo-side fixes follow the fixes/ pattern (see fixes/m_trademark.c).
