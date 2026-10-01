#!/usr/bin/env bash
# Run every method on the emulator, off chain — each accepted at the amounts `plan` chooses, and a
# greedy or tampered variant of each refused (no run, so no proof) — and check the guest agrees
# with the host rules on every step.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
[ -f "$here/image.bin" ] || die "no image.bin: run ./build.sh"
run_demo "$here/image.bin" "$here" --lp 6
