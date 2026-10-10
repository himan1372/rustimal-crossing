# Fix the audio issues (money pickup, dig sound, music stacking)
- [ ] Money-pickup "ching" plays the wrong sound — trace the money-pickup sound trigger; likely in the Rust audio port.
- [ ] Digging plays the wrong sound — compare the triggered sound ID against the decomp.
- [ ] Museum sprint music stacks/overlaps instead of replacing the current BGM — find the BGM state handling.
- [ ] Verify each fix against original GameCube behavior.
Rules: no test runs without Philip's explicit authorization; preserve existing C ABIs.
