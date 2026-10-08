#!/usr/bin/env bash
# run_mtx_diff.sh -- Wave 3 mtx differential test, run from the repo root.
# Requires: MSYS2/MinGW bash with gcc and rustc on PATH (both already needed
# to build the game). Compiles rust/src/mtx.rs standalone with the same rustc
# that builds the game, then links it against the differential driver.
set -euo pipefail

if [ ! -f rust/src/mtx.rs ] || [ ! -f tools/wave3-mtx-diff/mtx_diff_test.c ]; then
    echo "ERROR: run this script from the rustimal-crossing repo root." >&2
    exit 2
fi

TMP="${TMPDIR:-/tmp}/mtx_diff_$$"
mkdir -p "$TMP"
trap 'rm -rf "$TMP"' EXIT

echo "== compiling rust/src/mtx.rs with rustc =="
rustc --edition 2021 --emit obj --crate-name mtx_diff \
    --target i686-pc-windows-gnu \
    rust/src/mtx.rs -o "$TMP/mtx_rust.o"

echo "== compiling + linking differential driver =="
gcc -O2 -o "$TMP/mtx_diff_test.exe" \
    tools/wave3-mtx-diff/mtx_diff_test.c "$TMP/mtx_rust.o" -lm

echo "== running differential test =="
"$TMP/mtx_diff_test.exe"
