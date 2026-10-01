#!/usr/bin/env bash
# Claim everything raised (the creator only), once it meets the goal: ./claim.sh
# Pays exactly the raised RAND to this wallet and makes the campaign final. The private inputs are
# [4, secret]; the secret never leaves this machine. Retries on a stale read (exit 3).
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
need_rand
id="$(load_id "$here")"
[ -f "$here/creator.secret" ] || die "no creator.secret: only the creator can claim"
t="$(mktemp)"; i="$(mktemp)"; trap 'rm -f "$t" "$i"' EXIT
for attempt in 1 2 3; do
    campaign="$(cell "$id" "$(word8_hex 1 0 0 0 0 0 0 0)")"
    plan "$here" claim --secret "$here/creator.secret" --public "$here/public.txt" --campaign "$campaign" --t "$t" --i "$i" >/dev/null
    set +e; "$RAND" program invoke "$id" --transition "$t" --inputs-file "$i"; rc=$?; set -e
    [ "$rc" -eq 3 ] && { echo "stale read: the campaign moved; planning again (attempt $attempt)"; continue; }
    exit "$rc"
done
die "the campaign kept moving; try again"
