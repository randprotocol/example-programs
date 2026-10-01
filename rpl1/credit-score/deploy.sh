#!/usr/bin/env bash
# Deploy image.bin with model.txt as its public input. The program id binds both, so a new model is
# a new program — a borrower who checks the id knows exactly which rules scored them.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
need_rand
[ -f "$here/image.bin" ] || die "no image.bin: run ./build.sh"
[ "$(wc -w < "$here/model.txt")" -eq 6 ] || die "model.txt is six words: see README"
out="$("$RAND" program deploy "$here/image.bin" --public "$here/model.txt" | tee >(cat >&2))"
id="$(echo "$out" | sed -n 's/^program id: \([0-9a-f]\{64\}\).*/\1/p' | head -1)"
[ -n "$id" ] || die "no program id in the deploy's output"
save_id "$id" "$here"
