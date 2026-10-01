#!/usr/bin/env bash
# Run every method on the emulator, off chain — the operator setting prices, a stop and a
# take-profit placed and fired when their conditions hold, refused while they do not (no run, so
# no proof, so no trace of the attempt), a wrong opening, a cancel — and check the guest agrees
# with the host rules on every step.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
[ -f "$here/image.bin" ] || die "no image.bin: run ./build.sh"
run_demo "$here/image.bin" "$here"
