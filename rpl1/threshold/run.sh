#!/usr/bin/env bash
# Run the image on the zkVM's emulator, off chain: one amount above the threshold (halts), one
# below (traps — no run, so no proof).
#   ./run.sh [amount]
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
need_rand_guest
read -r lo hi < "$here/threshold.txt"
amount="${1:-2500}"
echo "threshold $lo (public), amount $amount (private):"
"$RG" run "$here/image.bin" --public "$lo" "$hi" --input "$amount" 0 --tier 10 || true
echo
echo "amount 1 (below the threshold):"
"$RG" run "$here/image.bin" --public "$lo" "$hi" --input 1 0 --tier 10 || true
