# Wave 3 runbook — Philip's-machine steps

Everything Rust-side is done and verified. Two items need your Windows
machine (MSYS2/MinGW tree at `C:\Users\phili\game c\shmanimal crossing\ACGC-PC-Port`).
Do them in order: mtx first, vi last.

## Part 1 — mtx differential test (~10 minutes)

**Why:** the Rust port uses `f32::sin_cos()` where the C code calls `sinf()`
then `cosf()` separately (see `guRotateF`). Everything else in `mtx.rs`
was verified line-by-line, so this bit check is the only gate.

**Step 1 — pull the new test files.** They were added in
`tools/wave3-mtx-diff/` (`mtx_diff_test.c`, `run_mtx_diff.sh`).

**Step 2 — run the differential test** from the repo root in MSYS2 bash:

```bash
bash tools/wave3-mtx-diff/run_mtx_diff.sh
```

What it does: compiles `rust/src/mtx.rs` standalone with your rustc,
links it against a driver holding a verbatim copy of the C `guRotateF`
formula, then compares all 16 output floats as raw bit patterns across
432 angle/axis cases.

**Step 3 — read the result.**

- `ALL TESTS PASSED` → go to Step 4.
- `FAILED` with hex diffs → stop, do **not** enable the exclusion. Paste
  the first few `MISMATCH` lines back to me and I'll fix the Rust side.

**Step 4 — enable the exclusion.** In `CMakeLists.txt`, uncomment the
mtx line (keep the comment above it):

```cmake
list(FILTER GAME_C_SOURCES EXCLUDE REGEX ".*/pc_mtx\\.c$")
```

**Step 5 — rebuild and smoke-test.** Rebuild as usual, boot the game,
and confirm: town renders, the camera rotates (that exercises
`guRotateF`/`guLookAt` heavily), no visual glitches. If anything looks
wrong, re-comment the line, rebuild, and tell me.

## Part 2 — vi boot validation (~15 minutes)

**Why:** `vi.rs` is the frame boundary (event polling, buffer swap, frame
pacing, retrace callbacks). It's wired last because if it misbehaves you
get a black screen or a frozen window with no other diagnostics.

**Step 1 — enable the exclusion.** In `CMakeLists.txt`, uncomment the
vi line:

```cmake
list(FILTER GAME_C_SOURCES EXCLUDE REGEX ".*/pc_vi\\.c$")
```

(The profiler exclusion it pairs with is already active.)

**Step 2 — rebuild and boot.** Watch for, in order:

1. The window opens and gets its title (title-setting is vi-owned).
2. Boot completes to the title screen — if you get a black window that
   never progresses, the swap/poll wiring is wrong.
3. Gameplay runs at ~60fps (`g_frame_limiter = 60`); movement and NPCs
   animate (retrace callbacks firing).
4. Audio plays normally (vi calls into the audio buffer-fill check).

**Step 3 — if anything fails:** re-comment the line, rebuild, and report
which step broke. The Rust side stays in the tree either way.

## After both

Mark the rows done in `rewiring.md` (Wave 3 table) and the workbook
(row 256 area). That's the end of the Wave 3 machine-side work.
