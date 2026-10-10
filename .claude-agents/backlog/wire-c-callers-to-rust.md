# Wire remaining C callers to Rust exports
- [ ] Inventory rust/src/ modules whose C ABI exports have no C callers rewired yet (most modules are in this state).
- [ ] For each module: add the C plug in wiring/ behind #ifdef USE_RUST — follow the established pattern in wave3-runbook.md (C-side state gathering, helper-only functions where needed).
- [ ] Hand Philip copy commands plus one-at-a-time playtest steps; wait for his reports before moving on.
- [ ] Do NOT touch ecology modules — parked (Philip said "dont do it just yet").
Rules: keep C/C++ decomp logic as-is, preserve existing C ABIs, no test runs without authorization.
