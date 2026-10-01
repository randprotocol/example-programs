#!/usr/bin/env bash
# Prove, on chain, that the credential in slot <index> of the issuer's roll meets the deployed
# cutoff — without saying which slot, or anything else about it.
#   ./call.sh <credentials file> <index> [rand call flags, e.g. --auditor rand1…]
# The holder's side needs only their own `issuer path` words; this script plays both sides from
# the roll for the demo. The credential and its path never leave this machine; the receipt's
# outputs are [1, cutoff_year, root0..root5]. Two random blind words go first, so the proof's
# input commitment cannot be brute-forced from a guessable credential.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
need_rand
need_rand_guest
id="$(load_id "$here")"
roll="${1:?usage: ./call.sh <credentials file> <index> [rand call flags]}"
index="${2:?usage: ./call.sh <credentials file> <index> [rand call flags]}"
shift 2
[ -f "$here/issuer.txt" ] || die "no issuer.txt: run ./root.sh"
[ -f "$here/hash/image.bin" ] || die "no hash/image.bin: run ./build.sh"
words="$(od -An -N8 -tu4 /dev/urandom | tr -s ' \n' ' ') $(cargo +"$TOOLCHAIN" run -q --release --manifest-path "$here/core/Cargo.toml" --bin issuer -- path "$roll" "$index")"
args=()
for w in $words; do args+=(--input "$w"); done
# --expect-public: refuse before proving unless the chain's copy of the root and cutoff is ours.
"$RAND" call "$id" --expect-public "$here/issuer.txt" "${args[@]}" "$@"
