#!/usr/bin/env bash
# Register the pool's liquidity share: an RPL token whose only minter is this program.
# Prints its asset index; ./add.sh's first deposit binds it to the pool.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
need_rand
id="$(load_id "$here")"
"$RAND" token create --name "AMM share $(cat "$here/token.txt")" --symbol AMMLP --decimals 9 --program "$id"
