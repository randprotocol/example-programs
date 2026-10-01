#!/usr/bin/env bash
# Back the campaign: ./pledge.sh <RAND>
# The RAND goes into the program's vault and the same number of receipts come back to this wallet
# as a shielded note: who backed the campaign is not public. Refused once the creator has claimed.
# If someone else pledges or refunds first, the chain refuses ours as a stale read (exit 3) and
# this script plans again.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
need_rand
id="$(load_id "$here")"
units="$(rand_units "${1:?usage: ./pledge.sh <RAND>}")"
t="$(mktemp)"; i="$(mktemp)"; trap 'rm -f "$t" "$i"' EXIT
for attempt in 1 2 3; do
    campaign="$(cell "$id" "$(word8_hex 1 0 0 0 0 0 0 0)")"
    plan "$here" pledge --amount "$units" --public "$here/public.txt" --campaign "$campaign" --t "$t" --i "$i" >/dev/null
    set +e; "$RAND" program invoke "$id" --transition "$t" --inputs-file "$i"; rc=$?; set -e
    [ "$rc" -eq 3 ] && { echo "stale read: the campaign moved; planning again (attempt $attempt)"; continue; }
    exit "$rc"
done
die "the campaign kept moving; try again"
