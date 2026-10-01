#!/usr/bin/env bash
# Increment the counter on chain: read cell 1, declare n → n + 1, prove, submit.
# If someone else increments first, the chain refuses ours as a stale read (`rand` exits 3);
# this script re-reads and tries again.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
need_rand
id="$(load_id "$here")"
key="$(word8_hex 1 0 0 0 0 0 0 0)"
for attempt in 1 2 3; do
    value="$("$RAND" program state "$id" --cell "$key" | sed -n 's/.*"value": "\([0-9a-f]\{64\}\)".*/\1/p')"
    read -r lo hi _ <<< "$(hex_words "$value")"
    n=$(( lo + (hi << 32) )); next=$(( n + 1 ))
    echo "cell 1 holds $n; declaring $n → $next"
    t="$(mktemp)"
    cat > "$t" <<JSON
{
  "reads":  [{ "key": "$key", "value": "$value" }],
  "writes": [{ "key": "$key", "value": "$(word8_hex $(( next & 0xffffffff )) $(( next >> 32 )) 0 0 0 0 0 0)" }]
}
JSON
    set +e; "$RAND" program invoke "$id" --transition "$t"; rc=$?; set -e
    rm -f "$t"
    [ "$rc" -eq 3 ] && { echo "stale read: the counter moved; re-reading (attempt $attempt)"; continue; }
    exit "$rc"
done
die "the counter kept moving; try again"
