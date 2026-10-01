#!/usr/bin/env bash
# Run the image on the zkVM's emulator, off chain: the four demo bids cleared as an auction
# (highest wins, pays the second-highest) and as a request for quote (lowest wins, paid the
# second-lowest), then three calls the program refuses — one bid, two bids under one tag, a bid of
# 2^63 — which trap: no run, so no proof.
#   ./run.sh [bids file]
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
need_rand_guest
[ -f "$here/image.bin" ] && [ -f "$here/commit/image.bin" ] || die "no image.bin: run ./build.sh"
bids="${1:-$here/demo-bids.txt}"
tier=12
auction() { cargo +"$TOOLCHAIN" run -q --manifest-path "$here/core/Cargo.toml" --bin auction -- "$@"; }
# Two fresh blind words per call, as call.sh draws them; they are never output.
blind() { od -An -N8 -tu4 /dev/urandom | tr -s ' \n' ' '; }

words="$(auction words "$bids")"
for mode in 0 1; do
    echo "== $(basename "$bids"), mode $mode (public):"
    auction show "$bids" --mode "$mode"
    "$RG" run "$here/image.bin" --public "$mode" --input $(blind) $words --tier "$tier" || true
    echo
done

echo "== one bid (refused):"
"$RG" run "$here/image.bin" --public 0 --input $(blind) 1 7 500 0 1 2 --tier "$tier" || true
echo
echo "== two bids under tag 7 (refused):"
"$RG" run "$here/image.bin" --public 0 --input $(blind) 2 7 500 0 1 2 7 600 0 3 4 --tier "$tier" || true
echo
echo "== a bid of 2^63 (refused):"
"$RG" run "$here/image.bin" --public 0 --input $(blind) 2 7 0 2147483648 1 2 8 600 0 3 4 --tier "$tier" || true
