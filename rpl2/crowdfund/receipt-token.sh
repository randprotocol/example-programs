#!/usr/bin/env bash
# Register the campaign's receipt: an RPL token whose only minter is this program.
# Prints its asset index; ./init.sh <index> binds it to the campaign.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
need_rand
id="$(load_id "$here")"
"$RAND" token create --name "Crowdfund receipt ${id:0:8}" --symbol CFR --decimals 9 --program "$id"
