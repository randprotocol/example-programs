#!/usr/bin/env bash
# Build the vault into image.bin and lock-hash into lock-hash/image.bin (needs CIRCUITS=…).
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
build_guest "$here"
"$RG" build "$CIRCUITS/example-programs/vault/lock-hash"
cp "$CIRCUITS/example-programs/vault/lock-hash/image.bin" "$here/lock-hash/"
echo "image: $here/lock-hash/image.bin (off chain only)"
