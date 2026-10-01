#!/usr/bin/env bash
# Run the join on chain: prove, from both lists and both salts, the result join.txt's mode asks
# for, and print the receipt.
#   ./call.sh <A list> <A salt> <B list> <B salt> [more `rand call` flags]
# Whoever runs this holds both lists in the clear; the lists never leave this machine. The two
# blind words come from /dev/urandom. Add --no-envelope to publish no sealed copy of the inputs,
# or --auditor <rand1…> to let an auditor reopen them (see the README).
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
need_rand
need_rand_guest
id="$(load_id "$here")"
[ $# -ge 4 ] || die "usage: ./call.sh <A list> <A salt> <B list> <B salt> [rand call flags]"
la="$1"; sa="$2"; lb="$3"; sb="$4"; shift 4
[ -f "$here/join.txt" ] || die "no join.txt: run ./public.sh"
plan="$(JOIN_COMMIT_IMAGE="$here/commit/image.bin" cargo +"$TOOLCHAIN" run -q --release \
    --manifest-path "$here/core/Cargo.toml" --bin join -- inputs "$la" "$sa" "$lb" "$sb")"
echo "$plan" | grep -v '^input: ' >&2
blinds="$(od -An -N8 -tu4 /dev/urandom | tr -s ' \n' ' ' | sed 's/^ //;s/ $//')"
args=()
for w in $blinds $(echo "$plan" | sed -n 's/^input: //p'); do args+=(--input "$w"); done
# --expect-public: refuse before proving unless the chain's copy of join.txt is ours.
"$RAND" call "$id" --expect-public "$here/join.txt" "${args[@]}" "$@"
