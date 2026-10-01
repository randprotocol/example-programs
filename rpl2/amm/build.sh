#!/usr/bin/env bash
# Build the guest into image.bin (with the kit beside it, inside your circuits checkout: CIRCUITS=…)
# and run the rules' host tests.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
build_guest "$here" "$here/../kit"
(cd "$here/core" && cargo +"$TOOLCHAIN" test -q)
