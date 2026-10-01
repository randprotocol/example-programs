#!/usr/bin/env bash
# Pay RAND out of the vault, proving knowledge of secret.txt without revealing it:
#   ./withdraw.sh <RAND> [rand1… recipient]     (default recipient: this wallet)
# The payout is a new shielded note; its amount and the recipient's key are public, like a mint.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
need_rand
id="$(load_id "$here")"
[ -f "$here/secret.txt" ] || die "no secret.txt: only its holder can withdraw"
units="$(rand_units "${1:?usage: ./withdraw.sh <RAND> [recipient]}")"
to="${2:-}"
t="$(mktemp)"; i="$(mktemp)"; trap 'rm -f "$t" "$i"' EXIT
if [ -n "$to" ]; then
    echo "{ \"pays\": [{ \"asset\": 0, \"amount\": \"$units\", \"to\": \"$to\" }] }" > "$t"
else
    echo "{ \"pays\": [{ \"asset\": 0, \"amount\": \"$units\" }] }" > "$t"
fi
# The method and the secret as private inputs: they never leave this machine.
echo "[2, $(sed 's/ /, /g' "$here/secret.txt")]" > "$i"
"$RAND" program invoke "$id" --transition "$t" --inputs-file "$i"
