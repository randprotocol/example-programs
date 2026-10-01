#!/usr/bin/env bash
# Register a token mintable later by a post-quantum (Dilithium2) authority key, written to
# authority.key.json here (mode 600 — whoever holds it can mint). Optionally mint some now.
#   ./create-mintable.sh <name> <SYMBOL> <decimals> [initial whole tokens]
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
need_rand
name="${1:?usage: ./create-mintable.sh <name> <SYMBOL> <decimals> [initial]}"; symbol="${2:?}"; decimals="${3:?}"
[ -e "$here/authority.key.json" ] && die "authority.key.json exists; move it away first"
args=(--name "$name" --symbol "$symbol" --decimals "$decimals" --authority-key-out "$here/authority.key.json")
if [ -n "${4:-}" ]; then
    args+=(--initial "${4}$(printf '%0*d' "$decimals" 0)" --to "$("$RAND" address | tail -1)")
fi
"$RAND" token create "${args[@]}"
