#!/usr/bin/env bash
# Build the guest into image.bin and the off-chain committer into commit/image.bin (needs a
# circuits checkout: CIRCUITS=…), then run the rules' host tests.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
build_guest "$here"
"$RG" build "$CIRCUITS/example-programs/private-join/commit"
cp "$CIRCUITS/example-programs/private-join/commit/image.bin" "$here/commit/"
echo "image: $here/commit/image.bin (off chain only)"
(cd "$here/core" && cargo +"$TOOLCHAIN" test -q)
