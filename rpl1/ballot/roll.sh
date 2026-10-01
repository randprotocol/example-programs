#!/usr/bin/env bash
# Fold a roll into vote.txt, the deploy's public input: `n_options R0 … R7`, with R computed by the
# fold helper guest on the emulator — the program's own hash.
#   ./roll.sh [roll file]          (default roll.txt)
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
need_rand_guest
roll="${1:-$here/roll.txt}"
[ -f "$roll" ] || die "no roll file: $roll"
[ -f "$here/fold/image.bin" ] || die "no fold/image.bin: run ./build.sh"
export FOLD_IMAGE="$here/fold/image.bin"
cargo +"$TOOLCHAIN" run -q --release --manifest-path "$here/core/Cargo.toml" --bin ballot -- roll "$roll" > "$here/vote.txt"
[ "$(wc -w < "$here/vote.txt")" -eq 9 ] || die "the fold did not print nine words"
echo "vote.txt: $(cat "$here/vote.txt")"
