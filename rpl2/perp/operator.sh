#!/usr/bin/env bash
# Make the operator's secret (operator.secret, mode 600 — it is the only key to the price) and its
# lock, POSEIDON2([TAG_OPERATOR, secret]), as public.txt: the deploy's public input.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
TAG_OPERATOR=1919971184   # "popr", little-endian (core/src/lib.rs)
new_secret "$here/operator.secret"
digest "$TAG_OPERATOR" "$here/operator.secret" > "$here/public.txt"
[ "$(wc -w < "$here/public.txt")" -eq 8 ] || die "secret-hash did not print eight words"
echo "public.txt (the lock): $(cat "$here/public.txt")"
