#!/usr/bin/env bash
# Build the guest into image.bin and the off-chain hasher into hash/image.bin (needs a circuits
# checkout: CIRCUITS=…), then run the rules' host tests.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
build_guest "$here"
"$RG" build "$CIRCUITS/example-programs/eligibility/hash"
cp "$CIRCUITS/example-programs/eligibility/hash/image.bin" "$here/hash/"
echo "image: $here/hash/image.bin (off chain only)"
(cd "$here/core" && cargo +"$TOOLCHAIN" test -q)
