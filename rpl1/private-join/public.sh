#!/usr/bin/env bash
# Write join.txt, the deploy-time public input: both commitments and the mode.
#   ./public.sh "<C_A: 8 words>" "<C_B: 8 words>" <mode>        mode 0: intersection size, 1: match
# The program id binds all 17 words, so each party checks the id before accepting a receipt.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
[ $# -ge 3 ] || die 'usage: ./public.sh "<C_A words>" "<C_B words>" <mode>'
# Validated first, written second: a refused mode leaves no half-written join.txt behind.
words="$(cargo +"$TOOLCHAIN" run -q --release --manifest-path "$here/core/Cargo.toml" --bin join -- public "$@")"
echo "$words" > "$here/join.txt"
echo "join.txt: $words"
