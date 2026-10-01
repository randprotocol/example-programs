#!/usr/bin/env bash
# Open the campaign (the creator only): ./init.sh <receipt asset index>
# Binds the receipt token from ./receipt-token.sh. It must be this program's own token: a token
# the program cannot mint makes every pledge fail, and the campaign cannot be opened twice.
# The private inputs are [1, secret, receipt]; the secret never leaves this machine.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
need_rand
id="$(load_id "$here")"
[ -f "$here/creator.secret" ] || die "no creator.secret: only the creator can open the campaign"
receipt="${1:?usage: ./init.sh <receipt asset index>}"
t="$(mktemp)"; i="$(mktemp)"; trap 'rm -f "$t" "$i"' EXIT
for attempt in 1 2 3; do
    campaign="$(cell "$id" "$(word8_hex 1 0 0 0 0 0 0 0)")"
    plan "$here" init --secret "$here/creator.secret" --receipt "$receipt" --public "$here/public.txt" --campaign "$campaign" --t "$t" --i "$i" >/dev/null
    set +e; "$RAND" program invoke "$id" --transition "$t" --inputs-file "$i"; rc=$?; set -e
    [ "$rc" -eq 3 ] && { echo "stale read: the campaign moved; planning again (attempt $attempt)"; continue; }
    exit "$rc"
done
die "the campaign kept moving; try again"
