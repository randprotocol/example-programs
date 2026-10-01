#!/usr/bin/env bash
# Deploy image.bin with join.txt as its public input. The program id binds both commitments and
# the mode, so each pair of lists (and each mode) is its own program. Needs ./public.sh first.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
need_rand
[ -f "$here/image.bin" ] || die "no image.bin: run ./build.sh"
[ -f "$here/join.txt" ] || die "no join.txt: run ./public.sh"
out="$("$RAND" program deploy "$here/image.bin" --public "$here/join.txt" | tee >(cat >&2))"
id="$(echo "$out" | sed -n 's/^program id: \([0-9a-f]\{64\}\).*/\1/p' | head -1)"
[ -n "$id" ] || die "no program id in the deploy's output"
save_id "$id" "$here"
