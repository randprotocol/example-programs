#!/usr/bin/env bash
# Register the pool's liquidity share: an RPL token whose only minter is this program.
# Prints its asset index; ./add.sh's first deposit binds it to the pool.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
need_rand
id="$(load_id "$here")"
read -r token amp < "$here/public.txt"
"$RAND" token create --name "StableSwap share $token A$amp" --symbol SSLP --decimals 9 --program "$id"
