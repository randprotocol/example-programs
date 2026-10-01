#!/usr/bin/env bash
# Register a fixed-supply token: every unit is minted once, to this wallet, at registration, and
# no key can ever mint more (authority `none`).
#   ./create-fixed.sh <name> <SYMBOL> <decimals> <supply in whole tokens>
# e.g. ./create-fixed.sh "Example Gold" XGLD 6 1000000
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
need_rand
name="${1:?usage: ./create-fixed.sh <name> <SYMBOL> <decimals> <supply>}"; symbol="${2:?}"; decimals="${3:?}"; supply="${4:?}"
units="$supply$(printf '%0*d' "$decimals" 0)"            # whole tokens → base units
me="$("$RAND" address | tail -1)"
"$RAND" token create --name "$name" --symbol "$symbol" --decimals "$decimals" --fixed-supply "$units" --to "$me"
