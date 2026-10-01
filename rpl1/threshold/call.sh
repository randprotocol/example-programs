#!/usr/bin/env bash
# Prove, on chain, that a private amount is at least the deployed threshold.
#   ./call.sh <amount>
# The amount never leaves this machine; the receipt's outputs are [1, threshold_lo, threshold_hi].
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
need_rand
id="$(load_id "$here")"
amount="${1:?usage: ./call.sh <amount>}"
lo=$(( amount & 0xffffffff )); hi=$(( amount >> 32 ))
# --expect-public: refuse before proving unless the chain's copy of the threshold is ours.
"$RAND" call "$id" --expect-public "$here/threshold.txt" --input "$lo" --input "$hi"
