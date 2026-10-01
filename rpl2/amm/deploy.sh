#!/usr/bin/env bash
# Deploy the pool for one token: ./deploy.sh <token asset index>
# The token is the program's public input, so each pair is its own program, with its own id.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
need_rand
[ -f "$here/image.bin" ] || die "no image.bin: run ./build.sh"
token="${1:?usage: ./deploy.sh <token asset index>}"
echo "$token" > "$here/token.txt"
out="$("$RAND" program deploy "$here/image.bin" --public "$here/token.txt" | tee >(cat >&2))"
id="$(echo "$out" | sed -n 's/^program id: \([0-9a-f]\{64\}\).*/\1/p' | head -1)"
[ -n "$id" ] || die "no program id in the deploy's output"
save_id "$id" "$here"
echo "next: ./share-token.sh — register the liquidity share token this program mints"
