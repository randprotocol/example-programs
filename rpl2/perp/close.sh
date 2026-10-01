#!/usr/bin/env bash
# Close a position at the best payout the price allows: ./close.sh <name> (uses <name>.secret).
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
need_rand
id="$(load_id "$here")"
[ -f "$here/public.txt" ] || die "no public.txt: run ./operator.sh"
t="$(mktemp)"; i="$(mktemp)"; trap 'rm -f "$t" "$i"' EXIT
name="${1:?usage: ./close.sh <name>}"
secret="$here/$name.secret"
[ -f "$secret" ] || die "no $secret: only its holder can close the position"
key="$(plan "$here" key --secret "$secret")"
# The market and the open interest are shared by everyone: if either moves before this lands, the
# chain refuses it as a stale read (exit 3) and this script plans again from the new cells.
for attempt in 1 2 3; do
    market="$(cell "$id" "$(word8_hex 1 0 0 0 0 0 0 0)")"
    oi="$(cell "$id" "$(word8_hex 2 0 0 0 0 0 0 0)")"
    position="$(cell "$id" "$key")"
    plan "$here" close --lock "$here/public.txt" --secret "$secret" --market "$market" --oi "$oi" --position "$position" --t "$t" --i "$i" >/dev/null
    set +e; "$RAND" program invoke "$id" --transition "$t" --inputs-file "$i"; rc=$?; set -e
    [ "$rc" -eq 3 ] && { echo "stale read: the market moved; planning again (attempt $attempt)"; continue; }
    exit "$rc"
done
die "the market kept moving; try again"
