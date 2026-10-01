#!/usr/bin/env bash
# Place an order: ./place.sh stop|tp <RAND> <threshold> [name]
#   stop  fires when the price is at or below <threshold>
#   tp    (take-profit) fires when the price is at or above <threshold>
# The RAND goes into the program's vault as escrow. Writes <name>.secret (mode 600): the ticket
# (eight random words, whose digest is the order's key), the kind, the threshold and a 128-bit
# salt. The chain sees the escrow and a commitment to the trigger — never the threshold. Whoever
# holds this file can fire or cancel the order: keep it, and share it only with a keeper you
# trust (the keeper's transaction fixes who is paid).
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
need_rand
id="$(load_id "$here")"
usage="usage: ./place.sh stop|tp <RAND> <threshold> [name]"
case "${1:?$usage}" in
    stop) kind=1 ;;
    tp) kind=2 ;;
    *) die "$usage" ;;
esac
units="$(rand_units "${2:?$usage}")"
thr="${3:?$usage}"
[[ "$thr" =~ ^[0-9]+$ ]] && [ "$thr" -gt 0 ] || die "the threshold is a whole number above 0"
secret="$here/${4:-$1-$(date +%Y%m%d-%H%M%S)}.secret"
[ -e "$secret" ] && die "$secret exists; move it away first (it is the only key to what it owns)"
words() { od -An -N"$1" -tu4 /dev/urandom | tr -s ' \n' ' ' | sed 's/^ //;s/ $//'; }
( umask 077; { words 32; echo; echo "$kind $thr"; words 16; echo; } > "$secret" )
echo "new secret: $secret (keep it private)"
key="$(plan "$here" key --secret "$secret")"
t="$(mktemp)"; i="$(mktemp)"; trap 'rm -f "$t" "$i"' EXIT
order="$(cell "$id" "$key")"
plan "$here" place --secret "$secret" --order "$order" --rand "$units" --t "$t" --i "$i" >/dev/null
"$RAND" program invoke "$id" --transition "$t" --inputs-file "$i"
echo "order key: $key"
echo "secret:    $secret (fire it with ./fire.sh $secret, or take it back with ./cancel.sh $secret)"
