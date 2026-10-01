#!/usr/bin/env bash
# Recompute a statements file's commitment with the guest's own hash (stmt-hash, on the emulator).
# A receipt's out[2..8] are the first six words printed. This is the consent step: a borrower hands
# over the file and its .secret, and whoever receives them runs this and compares.
#   ./commit.sh <statements file> [salt file]        default salt file: <statements file>.secret
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
need_rand_guest
file="${1:?usage: ./commit.sh <statements file> [salt file]}"
secret="${2:-${file%.*}.secret}"
[ -f "$here/stmt-hash/image.bin" ] || die "no stmt-hash/image.bin: run ./build.sh"
[ -f "$secret" ] || die "no salt file $secret (call.sh writes it beside the statements)"
words="$(sed 's/#.*//' "$file" | tr -s ' \t\n' ' ' | sed 's/^ //;s/ $//')"
[ "$(echo "$words" | wc -w | tr -d ' ')" -eq 36 ] || die "$file is not 36 numbers"
"$RG" run "$here/stmt-hash/image.bin" --input $words $(cat "$secret") | sed -n 's/^out\[[0-7]\] = //p' | tr '\n' ' ' | sed 's/ $//'
echo
