#!/usr/bin/env bash
# Deploy image.bin with mode.txt as its public input: "0" clears highest-wins (an auction), "1"
# lowest-wins (a request for quote). The program id binds the mode, so each mode is its own program.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
need_rand
[ -f "$here/image.bin" ] || die "no image.bin: run ./build.sh"
mode="$(tr -d '[:space:]' < "$here/mode.txt")"
[ "$mode" = 0 ] || [ "$mode" = 1 ] || die "mode.txt must be 0 (highest wins) or 1 (lowest wins), not '$mode'"
out="$("$RAND" program deploy "$here/image.bin" --public "$here/mode.txt" | tee >(cat >&2))"
id="$(echo "$out" | sed -n 's/^program id: \([0-9a-f]\{64\}\).*/\1/p' | head -1)"
[ -n "$id" ] || die "no program id in the deploy's output"
save_id "$id" "$here"
