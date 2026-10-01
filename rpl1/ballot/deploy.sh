#!/usr/bin/env bash
# Deploy image.bin with vote.txt (the option count and the roll's digest) as its public input. The
# program id binds the roll, so a new roll is a new program. Needs ./roll.sh first.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
need_rand
[ -f "$here/image.bin" ] || die "no image.bin: run ./build.sh"
[ -f "$here/vote.txt" ] || die "no vote.txt: run ./roll.sh <roll file>"
out="$("$RAND" program deploy "$here/image.bin" --public "$here/vote.txt" | tee >(cat >&2))"
id="$(echo "$out" | sed -n 's/^program id: \([0-9a-f]\{64\}\).*/\1/p' | head -1)"
[ -n "$id" ] || die "no program id in the deploy's output"
save_id "$id" "$here"
