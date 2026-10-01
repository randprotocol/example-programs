#!/usr/bin/env bash
# Deploy the pool for one token and one amplification: ./deploy.sh <token asset index> <A>
# Both are the program's public input (public.txt: "token A"), so each pair and each A is its own
# program, with its own id. A is 1..=10000; 100 is a common choice for a tight peg.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
need_rand
[ -f "$here/image.bin" ] || die "no image.bin: run ./build.sh"
token="${1:?usage: ./deploy.sh <token asset index> <A>}"
amp="${2:?usage: ./deploy.sh <token asset index> <A>}"
[[ "$amp" =~ ^[0-9]+$ ]] && [ "$amp" -ge 1 ] && [ "$amp" -le 10000 ] || die "A must be 1..10000 (the program refuses everything otherwise)"
[ "$token" != 0 ] || die "the token cannot be RAND (asset 0)"
echo "$token $amp" > "$here/public.txt"
out="$("$RAND" program deploy "$here/image.bin" --public "$here/public.txt" | tee >(cat >&2))"
id="$(echo "$out" | sed -n 's/^program id: \([0-9a-f]\{64\}\).*/\1/p' | head -1)"
[ -n "$id" ] || die "no program id in the deploy's output"
save_id "$id" "$here"
echo "next: ./share-token.sh — register the liquidity share token this program mints"
