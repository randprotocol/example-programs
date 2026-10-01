#!/usr/bin/env bash
# Start a campaign: ./creator.sh <goal in RAND>
# Makes creator.secret (8 random words, mode 600 — it alone can open and claim the campaign) and
# public.txt, the deploy's public input: the lock POSEIDON2(["crtr", secret]), then the goal in
# base units (low word, high word). Both are part of the program id: neither can change later.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
goal="$(rand_units "${1:?usage: ./creator.sh <goal in RAND>}")"
[ "$goal" -gt 0 ] || die "the goal must be more than nothing"
[ -e "$here/public.txt" ] && die "public.txt exists; move it away first (it names a deployed campaign)"
new_secret "$here/creator.secret"
lock="$(digest $((0x72747263)) "$here/creator.secret")"
[ "$(echo "$lock" | wc -w)" -eq 8 ] || die "secret-hash did not print eight words"
echo "$lock $((goal & 0xffffffff)) $((goal >> 32))" > "$here/public.txt"
echo "public.txt: $(cat "$here/public.txt")"
echo "next: ./deploy.sh"
