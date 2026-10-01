#!/usr/bin/env bash
# Deploy image.bin with issuer.txt (the issuer's root and the cutoff year) as its public input.
# The program id binds both, so another issuer or another cutoff is another program. Needs
# ./root.sh first.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
need_rand
[ -f "$here/image.bin" ] || die "no image.bin: run ./build.sh"
[ -f "$here/issuer.txt" ] || die "no issuer.txt: run ./root.sh <credentials file> <cutoff_year>"
out="$("$RAND" program deploy "$here/image.bin" --public "$here/issuer.txt" | tee >(cat >&2))"
id="$(echo "$out" | sed -n 's/^program id: \([0-9a-f]\{64\}\).*/\1/p' | head -1)"
[ -n "$id" ] || die "no program id in the deploy's output"
save_id "$id" "$here"
