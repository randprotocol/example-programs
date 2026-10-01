#!/usr/bin/env bash
# Run the image on the zkVM's emulator, off chain, over a context built by hand: the eight
# call-binding words (zeros here; on chain they bind the transaction) and then the transition.
#   ./run.sh [n]     # declares "cell 1 held n, now holds n + 1" (default 41)
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
need_rand_guest
n="${1:-41}"
binding="0 0 0 0 0 0 0 0"
head="1  1 1 0 0  0 0 0 0 0 0"          # version; reads, writes, pays, mints; burn_r, inflow, burn_asset, burn_a
key="1 0 0 0 0 0 0 0"
ctx() { echo "$head  $key $1 0 0 0 0 0 0 0  $key $2 0 0 0 0 0 0 0"; }
echo "n = $n → $((n + 1)) (accepted):"
"$RG" run "$here/image.bin" --public $binding $(ctx "$n" "$((n + 1))") --tier 10 || true
echo
echo "n = $n → $((n + 2)) (refused — no run, so no proof):"
"$RG" run "$here/image.bin" --public $binding $(ctx "$n" "$((n + 2))") --tier 10 || true
