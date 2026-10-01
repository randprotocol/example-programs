#!/usr/bin/env bash
# Deploy with public.txt (the operator's lock and the collateral token) as the public input. Each
# operator and collateral pair is its own program, with its own id and vault. Needs ./operator.sh.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
need_rand
[ -f "$here/image.bin" ] || die "no image.bin: run ./build.sh"
[ -f "$here/public.txt" ] || die "no public.txt: run ./operator.sh <collateral asset>"
out="$("$RAND" program deploy "$here/image.bin" --public "$here/public.txt" | tee >(cat >&2))"
id="$(echo "$out" | sed -n 's/^program id: \([0-9a-f]\{64\}\).*/\1/p' | head -1)"
[ -n "$id" ] || die "no program id in the deploy's output"
save_id "$id" "$here"
echo "next: ./share-token.sh, then ./operate.sh init <price> <share asset>"
