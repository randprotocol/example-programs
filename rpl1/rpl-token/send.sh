#!/usr/bin/env bash
# Send a token privately: a plain four-slot bundle that does not say which asset moved.
# The fee is RAND, so the wallet needs some RAND too.
#   ./send.sh <token index or rpl1… id> <rand1… recipient> <amount, display units> [rand send flags…]
# `rand send` asks before sending; from a script with no terminal, add --yes.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
need_rand
"$RAND" send --asset "${1:?usage: ./send.sh <token> <recipient> <amount> [--yes]}" "${2:?}" "${3:?}" "${@:4}"
