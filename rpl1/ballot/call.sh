#!/usr/bin/env bash
# Tally the ballots on chain: prove that the published totals are exactly these ballots over the
# deployed roll, and publish the totals in the receipt.
#   ./call.sh <roll file> <ballots file> [auditor rand1…]
# The ballots never leave this machine; the receipt's outputs are the four totals, each a u64
# little-endian. With an auditor named, the call's input transcript is also sealed to that key,
# so the auditor can later `rand open-call <tx> --as-auditor` and check the ballots against the
# receipt; without one, only this wallet can reopen it.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
need_rand
need_rand_guest
id="$(load_id "$here")"
roll="${1:?usage: ./call.sh <roll file> <ballots file> [auditor]}"
ballots="${2:?usage: ./call.sh <roll file> <ballots file> [auditor]}"
auditor="${3:-}"
[ -f "$here/vote.txt" ] || die "no vote.txt: run ./roll.sh <roll file>"
[ -f "$here/fold/image.bin" ] || die "no fold/image.bin: run ./build.sh"
export FOLD_IMAGE="$here/fold/image.bin"
plan="$(cargo +"$TOOLCHAIN" run -q --release --manifest-path "$here/core/Cargo.toml" --bin ballot -- inputs "$roll" "$ballots")"
# Refuse before proving if this roll is not the one vote.txt was folded from.
[ "$(echo "$plan" | sed -n 's/^public: //p')" = "$(tr -s ' \n' ' ' < "$here/vote.txt" | sed 's/^ //;s/ $//')" ] \
    || die "$roll does not fold to vote.txt: the deployed program binds a different roll"
echo "expected: tally $(echo "$plan" | sed -n 's/^tally: //p'), abstain $(echo "$plan" | sed -n 's/^abstain: //p')"
# Two blind words from /dev/urandom first, then the ballots: one --input per word.
args=()
for w in $(od -An -N8 -tu4 /dev/urandom) $(echo "$plan" | sed -n 's/^input: //p'); do args+=(--input "$w"); done
[ -n "$auditor" ] && args+=(--auditor "$auditor")
# --expect-public: refuse before proving unless the chain's copy of vote.txt is ours.
"$RAND" call "$id" --expect-public "$here/vote.txt" "${args[@]}"
