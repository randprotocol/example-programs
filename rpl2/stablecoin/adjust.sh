#!/usr/bin/env bash
# Open or adjust your position (position.secret is made on first use; keep it, it is the only key
# to the position):
#   ./adjust.sh [--deposit <RAND>] [--withdraw <RAND>|max] [--mint <stable units>|max] [--repay <stable units>|all]
# e.g.  ./adjust.sh --deposit 10 --mint max        open: lock 10 RAND, borrow the most 150 % allows
#       ./adjust.sh --repay 3000000000             burn 3.0 stable
#       ./adjust.sh --withdraw 2                   take 2 RAND back
#       ./adjust.sh --repay all --withdraw max     close the position
# Retries if the config or the position moved (a stale read, exit 3).
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
need_rand
id="$(load_id "$here")"
[ -f "$here/public.txt" ] || die "no public.txt: it is the program's public input (the operator's lock)"
args=()
while [ $# -gt 0 ]; do
    case "$1" in
        --deposit) args+=(--deposit "$(rand_units "${2:?--deposit <RAND>}")") ;;
        --withdraw) [ "${2:?--withdraw <RAND>|max}" = max ] && args+=(--withdraw max) || args+=(--withdraw "$(rand_units "$2")") ;;
        --mint) args+=(--mint "${2:?--mint <stable units>|max}") ;;
        --repay) args+=(--repay "${2:?--repay <stable units>|all}") ;;
        *) die "unknown flag $1 (see the top of adjust.sh)" ;;
    esac
    shift 2
done
[ ${#args[@]} -gt 0 ] || die "nothing to do (see the top of adjust.sh)"
[ -f "$here/position.secret" ] || new_secret "$here/position.secret"
key="$(plan "$here" key --secret "$here/position.secret")"
t="$(mktemp)"; i="$(mktemp)"; trap 'rm -f "$t" "$i"' EXIT
for attempt in 1 2 3; do
    config="$(cell "$id" "$(word8_hex 1 0 0 0 0 0 0 0)")"
    position="$(cell "$id" "$key")"
    plan "$here" adjust --secret "$here/position.secret" --lock "$here/public.txt" --config "$config" \
        --position "$position" "${args[@]}" --t "$t" --i "$i" >/dev/null
    set +e; "$RAND" program invoke "$id" --transition "$t" --inputs-file "$i"; rc=$?; set -e
    [ "$rc" -eq 3 ] && { echo "stale read: the price or the position changed; planning again (attempt $attempt)"; continue; }
    exit "$rc"
done
die "the cells kept changing; try again"
