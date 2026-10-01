#!/usr/bin/env bash
# Register the lenders' share token: an RPL token whose only minter is this program. Prints its
# asset index; ./operate.sh init binds it to the market.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
need_rand
id="$(load_id "$here")"
"$RAND" token create --name "Lending share" --symbol LEND --decimals 9 --program "$id"
