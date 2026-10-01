#!/usr/bin/env bash
# Make a fresh secret (secret.txt, 8 random words, mode 600 — keep it, it is the only way out)
# and its lock (lock.txt, the deploy's public input), computed by lock-hash with the vault's code.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
need_rand_guest
[ -e "$here/secret.txt" ] && die "secret.txt exists; move it away first (losing it locks the vault for good)"
umask 077
od -An -N32 -tu4 /dev/urandom | tr -s ' \n' ' ' | sed 's/^ //;s/ $//' > "$here/secret.txt"
"$RG" run "$here/lock-hash/image.bin" --input $(cat "$here/secret.txt") \
    | sed -n 's/^out\[[0-7]\] = //p' | tr '\n' ' ' | sed 's/ $//' > "$here/lock.txt"
[ "$(wc -w < "$here/lock.txt")" -eq 8 ] || die "lock-hash did not print eight words"
echo "secret.txt (keep it private) and lock.txt: $(cat "$here/lock.txt")"
