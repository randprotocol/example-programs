#!/usr/bin/env bash
# Build the guest into image.bin (needs a circuits checkout: CIRCUITS=…).
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
build_guest "$here"
