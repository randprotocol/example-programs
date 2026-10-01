#!/usr/bin/env bash
# Build the ballot program into image.bin and the fold helper into fold/image.bin (needs a circuits
# checkout: CIRCUITS=…), then run the rules' host tests.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
build_guest "$here"
"$RG" build "$CIRCUITS/example-programs/ballot/fold"
cp "$CIRCUITS/example-programs/ballot/fold/image.bin" "$here/fold/"
echo "image: $here/fold/image.bin (off chain only)"
(cd "$here/core" && cargo +"$TOOLCHAIN" test -q)
