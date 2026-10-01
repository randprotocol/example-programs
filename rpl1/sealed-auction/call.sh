#!/usr/bin/env bash
# Clear a round of sealed bids on chain: prove the deployed program over the bids file and print
# the receipt. Before proving, print the commitment list — publish it to the bidders — and the
# receipt to expect.
#   ./call.sh <bids file>         one bid per line: tag bid salt (keep it in a *.secret file)
# The bids never leave this machine; the receipt carries the winner's tag, the price and the
# fold of the commitments. Two blind words drawn from /dev/urandom lead the private inputs.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
need_rand
id="$(load_id "$here")"
bids="${1:?usage: ./call.sh <bids file>}"
[ -f "$here/commit/image.bin" ] || die "no commit/image.bin: run ./build.sh"
mode="$(tr -d '[:space:]' < "$here/mode.txt")"
auction() { cargo +"$TOOLCHAIN" run -q --manifest-path "$here/core/Cargo.toml" --bin auction -- "$@"; }

auction show "$bids" --mode "$mode"
args=()
for w in $(od -An -N8 -tu4 /dev/urandom) $(auction words "$bids"); do
    args+=(--input "$w")
done
# --expect-public: refuse before proving unless the chain's copy of the mode is ours.
"$RAND" call "$id" --expect-public "$here/mode.txt" "${args[@]}"
