#!/usr/bin/env bash
# Register the pool's LP token: an RPL token whose only minter is this program. Prints its asset
# index; ./price.sh's first call (creating the market) binds it.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
need_rand
id="$(load_id "$here")"
"$RAND" token create --name "perp LP" --symbol PERPLP --decimals 9 --program "$id"
