#!/usr/bin/env bash
# Open a position: ./open.sh <name> long|short <margin RAND> <notional RAND>
# Its secret is <name>.secret (made here, mode 600): the only key that closes it. One position per
# secret; the notional is at most ten times the margin.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
need_rand
id="$(load_id "$here")"
[ -f "$here/public.txt" ] || die "no public.txt: run ./operator.sh"
t="$(mktemp)"; i="$(mktemp)"; trap 'rm -f "$t" "$i"' EXIT
name="${1:?usage: ./open.sh <name> long|short <margin RAND> <notional RAND>}"
side="${2:?long or short}"
margin="$(rand_units "${3:?margin}")"; notional="$(rand_units "${4:?notional}")"
secret="$here/$name.secret"
[ -e "$secret" ] || new_secret "$secret"
key="$(plan "$here" key --secret "$secret")"
[ "$(cell "$id" "$key")" = "$(word8_hex 0 0 0 0 0 0 0 0)" ] || die "$name already has a position (key $key): close it first"
# The market and the open interest are shared by everyone: if either moves before this lands, the
# chain refuses it as a stale read (exit 3) and this script plans again from the new cells.
for attempt in 1 2 3; do
    market="$(cell "$id" "$(word8_hex 1 0 0 0 0 0 0 0)")"
    oi="$(cell "$id" "$(word8_hex 2 0 0 0 0 0 0 0)")"
    plan "$here" open --lock "$here/public.txt" --secret "$secret" --market "$market" --oi "$oi" --side "$side" --margin "$margin" --notional "$notional" --t "$t" --i "$i" >/dev/null
    set +e; "$RAND" program invoke "$id" --transition "$t" --inputs-file "$i"; rc=$?; set -e
    [ "$rc" -eq 3 ] && { echo "stale read: the market moved; planning again (attempt $attempt)"; continue; }
    exit "$rc"
done
die "the market kept moving; try again"
