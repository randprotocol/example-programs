#!/usr/bin/env bash
# Register the stable token: an RPL token whose only minter is this program. Prints its asset
# index; the operator's first ./set-price.sh binds it.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
need_rand
id="$(load_id "$here")"
"$RAND" token create --name "RAND-backed stable" --symbol RUSD --decimals 9 --program "$id"
