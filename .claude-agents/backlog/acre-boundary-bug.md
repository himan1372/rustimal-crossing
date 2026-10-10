# Fix the acre-boundary escape bug
- [ ] Review the research: the GameCube decomp runs NO block-edge walls during normal walk (the walk path skips mCoBG_UniqueWallCheck; block-edge walls only run in wading/demo paths). Bug is direction-dependent (up easiest, down hardest, worse near trees).
- [ ] Diff the port-modified decomp collision code against clean upstream — the port modified decomp files directly (there are no pc_*.c files); the bug is in port-modified decomp code not yet diffed. Philip's normal-walk path (Player_actor_BGcheck_Walk) is clean.
- [ ] Fix so the player cannot leave the current acre during normal walk, without breaking the retail-accurate camera scroll that appears once triggered.
- [ ] Paste workbook row 259 (drafted, with muse) into the live workbook at outputs/gamedb_research/ on Philip's machine.
Context: pre-existing bug, NOT caused by the Rust plugs. Philip's chosen next item. No test runs without his explicit authorization.
