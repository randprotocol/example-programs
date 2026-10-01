#!/usr/bin/env bash
# The campaign: its cell, how far it is from the goal, and the vault that holds the pledges.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
id="$(load_id "$here")"
"$RAND" program state "$id"
campaign="$(cell "$id" "$(word8_hex 1 0 0 0 0 0 0 0)")"
plan "$here" status --public "$here/public.txt" --campaign "$campaign"
"$RAND" program vault "$id"
