#!/usr/bin/env bash
# Build secret-hash/image.bin (off chain only: digests of secrets, with the guests' own hash) and
# run the kit's host tests. Every example's scripts build it on first use (`need_secret_hash`).
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
build_guest "$here/secret-hash"
(cd "$here" && cargo +"$TOOLCHAIN" test -q)
