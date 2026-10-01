#!/usr/bin/env bash
# What the receipt will say for a statements file, before anything is proved (the host runs the
# guest's own rules, core/).
#   ./score.sh <statements file> [model file]
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
file="${1:?usage: ./score.sh <statements file> [model file]}"
cargo +"$TOOLCHAIN" run -q --release --manifest-path "$here/core/Cargo.toml" --bin score -- "${2:-$here/model.txt}" "$file"
