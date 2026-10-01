#!/usr/bin/env bash
# Make the operator's secret (operator.secret, mode 600) and the deploy's public input
# (public.txt: the lock's eight words, then the collateral token): ./operator.sh <collateral asset>
# The operator sets the collateral's price and accrues interest; keep the secret.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
coll="${1:?usage: ./operator.sh <collateral asset index>}"
[ "$coll" -ne 0 ] || die "the collateral cannot be RAND (asset 0)"
new_secret "$here/operator.secret"
lock="$(digest 1886350956 "$here/operator.secret")"     # TAG_OPERATOR, "lnop"
[ "$(echo "$lock" | wc -w)" -eq 8 ] || die "secret-hash did not print eight words"
echo "$lock $coll" > "$here/public.txt"
echo "public.txt: $(cat "$here/public.txt")"
