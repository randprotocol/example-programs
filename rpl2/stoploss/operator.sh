#!/usr/bin/env bash
# Make the operator — the oracle: ./operator.sh
# Writes operator.secret (8 random words, mode 600 — it alone can set the price) and public.txt,
# the deploy's public input: the lock POSEIDON2(["slop", secret]). The lock is part of the program
# id, so a deployed program's oracle can never change hands.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
[ -e "$here/public.txt" ] && die "public.txt exists; move it away first (it names a deployed program)"
new_secret "$here/operator.secret"
lock="$(digest $((0x706f6c73)) "$here/operator.secret")"
[ "$(echo "$lock" | wc -w)" -eq 8 ] || die "secret-hash did not print eight words"
echo "$lock" > "$here/public.txt"
echo "public.txt: $(cat "$here/public.txt")"
echo "next: ./deploy.sh"
