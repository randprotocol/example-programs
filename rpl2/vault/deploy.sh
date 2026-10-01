#!/usr/bin/env bash
# Deploy image.bin with lock.txt as its public input. Each lock is its own program (own id, own
# vault). Needs ./lock.sh first.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
need_rand
[ -f "$here/image.bin" ] || die "no image.bin: run ./build.sh"
[ -f "$here/lock.txt" ] || die "no lock.txt: run ./lock.sh"
out="$("$RAND" program deploy "$here/image.bin" --public "$here/lock.txt" | tee >(cat >&2))"
id="$(echo "$out" | sed -n 's/^program id: \([0-9a-f]\{64\}\).*/\1/p' | head -1)"
[ -n "$id" ] || die "no program id in the deploy's output"
save_id "$id" "$here"
