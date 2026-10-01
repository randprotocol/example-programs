#!/usr/bin/env bash
# Make the operator's secret (operator.secret, mode 600 — the only key to the price) and its lock
# (public.txt: POSEIDON2([TAG_OPERATOR, s0..s7]), the program's public input). Run before deploy.sh.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
TAG_OPERATOR=1886348147   # "scop", little-endian (core/src/lib.rs)
new_secret "$here/operator.secret"
digest "$TAG_OPERATOR" "$here/operator.secret" > "$here/public.txt"
[ "$(wc -w < "$here/public.txt")" -eq 8 ] || die "secret-hash did not print eight words"
echo "public.txt (the lock, deployed as the public input): $(cat "$here/public.txt")"
