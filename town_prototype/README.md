# Rust Resident Placement Prototype

This asset-free prototype runs the same Rust town generator used by the PC port. The console reports the generated 5 by 6 acre map and resident coordinates. It also writes a self-contained WebGL preview with 3D elevation shelves, cliff faces, curved river channels, water drops, bridges, facilities, villager houses/signs, vegetation, and a southern ocean edge.

The prototype imports `pc/rust/src/town_gen.rs` directly with a path module. It does not initialize the game, read a disc image, or depend on copyrighted game assets. The legacy C NPC/save path remains unchanged.

## Build on x86 Windows

Requirements: Rust stable and the MSYS2 **MINGW32** toolchain (`mingw-w64-i686-gcc`). The build script defaults to `C:\msys64`; set `MSYS2_ROOT` if yours is elsewhere. From this directory, run:

```bat
build_x86.bat
```

The script checks for x86 GCC, installs Rust's `i686-pc-windows-gnu` standard library if needed, builds in the Windows temp directory, and places the executable at `outputs\ac_town_prototype.exe`. This uses the same Rust target family as the PC port. The executable is generated locally by this build step; it is not checked into the repository. The current x86 build succeeded with `C:\msys64\mingw32\bin`, and the generated preview was opened and visually checked in Edge.

## Run

After building, run `run.bat` to generate and open the 3D preview in your default browser:

```bat
run.bat --seed 305419896 --villagers 6
```

The preview is `outputs\ac_town_preview.html`; it is self-contained and needs no server or network access. Drag to orbit, use the wheel to zoom, and hold Shift while dragging to pan. The panel toggles buildings/bridges, vegetation, water, and acre boundaries. You can also run the executable directly to regenerate the HTML without opening a browser.

`--seed` accepts an unsigned 32-bit integer. `--villagers` accepts 1 through 15; default values are seed `305419896` and six residents. The preview retries up to 64 consecutive seeds if a generated plan cannot satisfy placement constraints, and reports which seed it used. The resident IDs are demonstration IDs (`1000` onward), not roster IDs from the game.

## Interpretation

The 3D surface is generated from semantic acre/unit data, not extracted game maps. Acre tiers become raised terrain shelves; river edge flags route seeded curved blue channels between acre centers, with marked falls drawn over cliff edges. The house/facility/tree/flower forms are simple primitive geometry. As documented in `pc/DOCUMENTATION.md`, Rust synthesizes eligible grass-acre home sites and does not consume original `RSV` map tokens or connect generated homes to the game's live save/foreground arrays. This displays rewrite data rather than reproducing an original town or integrating with live gameplay.
