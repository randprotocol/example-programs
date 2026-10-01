#!/usr/bin/env bash
# Deploy the campaign with public.txt (the creator's lock and the goal) as its public input.
# Each campaign is its own program, with its own id and vault. Needs ./creator.sh first.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
need_rand
[ -f "$here/image.bin" ] || die "no image.bin: run ./build.sh"
[ -f "$here/public.txt" ] || die "no public.txt: run ./creator.sh <goal>"
out="$("$RAND" program deploy "$here/image.bin" --public "$here/public.txt" | tee >(cat >&2))"
id="$(echo "$out" | sed -n 's/^program id: \([0-9a-f]\{64\}\).*/\1/p' | head -1)"
[ -n "$id" ] || die "no program id in the deploy's output"
save_id "$id" "$here"
echo "next: ./receipt-token.sh — register the receipt token this program mints"
