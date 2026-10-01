#!/usr/bin/env bash
# Build the guest into image.bin and stmt-hash into stmt-hash/image.bin (needs a circuits checkout:
# CIRCUITS=…), then run the rules' host tests.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
build_guest "$here"
"$RG" build "$CIRCUITS/example-programs/credit-score/stmt-hash"
cp "$CIRCUITS/example-programs/credit-score/stmt-hash/image.bin" "$here/stmt-hash/"
echo "image: $here/stmt-hash/image.bin (off chain only)"
(cd "$here/core" && cargo +"$TOOLCHAIN" test -q)
