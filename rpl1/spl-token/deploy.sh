#!/usr/bin/env bash
# Deploy image.bin with program.so (the ELF) as its public input. The chain must allow
# max_program_words >= 65096 and max_program_public_words >= 27151 (the durian devnet and
# chains 13+ do); the fee counts code and public words alike (~9.23 RAND).
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
need_rand
[ -f "$here/image.bin" ] && [ -f "$here/program.so" ] || die "no image.bin/program.so: run ./build.sh"
out="$("$RAND" program deploy "$here/image.bin" --public "$here/program.so" | tee >(cat >&2))"
id="$(echo "$out" | sed -n 's/^program id: \([0-9a-f]\{64\}\).*/\1/p' | head -1)"
[ -n "$id" ] || die "no program id in the deploy's output"
save_id "$id" "$here"
"$RAND" program show "$id"
