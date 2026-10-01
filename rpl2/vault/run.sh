#!/usr/bin/env bash
# Run the vault on the emulator over hand-built contexts: a deposit, a withdrawal with the right
# secret, and one with a wrong secret (refused — no run, so no proof). Needs ./lock.sh first.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
need_rand_guest
lock="$(cat "$here/lock.txt")"; secret="$(cat "$here/secret.txt")"
binding="0 0 0 0 0 0 0 0"
deposit="1  0 0 0 0  705032704 1  0 0 0 0"                # 5 RAND in: 5 000 000 000 units = lo 705032704, hi 1
withdraw="1  0 0 1 0  0 0  0 0 0 0  0 2000000000 0"        # pay out 2 RAND
echo "deposit 5 RAND:";   "$RG" run "$here/image.bin" --public $lock $binding $deposit --input 1 --tier 10 || true
echo; echo "withdraw 2 RAND, right secret:"; "$RG" run "$here/image.bin" --public $lock $binding $withdraw --input 2 $secret --tier 10 || true
echo; echo "withdraw 2 RAND, wrong secret:"; "$RG" run "$here/image.bin" --public $lock $binding $withdraw --input 2 1 2 3 4 5 6 7 8 --tier 10 || true
