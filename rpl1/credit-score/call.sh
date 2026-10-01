#!/usr/bin/env bash
# Prove, on chain, the band the deployed model gives twelve months of statements.
#   ./call.sh <statements file>                   # inputs sealed to this wallet only (the default envelope)
#   AUDITOR=rand1… ./call.sh <statements file>    # also sealed to that address: `rand open-call --as-auditor`
#   NO_ENVELOPE=1 ./call.sh <statements file>     # nothing sealed: nobody, you included, can reopen the call
# The statements never leave this machine. The salt is drawn once per statements file into
# <file>.secret (mode 600) and reused, so one file has one commitment; keep it — it is what lets you
# show the statements against the receipt later.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
need_rand
id="$(load_id "$here")"
file="${1:?usage: [AUDITOR=rand1…] [NO_ENVELOPE=1] ./call.sh <statements file>}"
[ -f "$file" ] || die "no such file: $file"
words="$(sed 's/#.*//' "$file" | tr -s ' \t\n' ' ' | sed 's/^ //;s/ $//')"
n="$(echo "$words" | wc -w | tr -d ' ')"
[ "$n" -eq 36 ] || die "$file has $n numbers; twelve months of income, payment and end balance are 36"
secret="${file%.*}.secret"
if [ ! -f "$secret" ]; then
    ( umask 077; od -An -N8 -tu4 /dev/urandom | tr -s ' \n' ' ' | sed 's/^ //;s/ $//' > "$secret" )
    echo "salt drawn into $secret (keep it private; it opens the commitment)" >&2
fi
salt="$(cat "$secret")"
[ "$(echo "$salt" | wc -w | tr -d ' ')" -eq 2 ] || die "$secret is not two words"
# Two blind words per call, so the proof's trace is never a function of guessable numbers alone.
blind="$(od -An -N8 -tu4 /dev/urandom | tr -s ' \n' ' ')"
args=()
for w in $blind $words $salt; do args+=(--input "$w"); done
if [ -n "${AUDITOR:-}" ]; then args+=(--auditor "$AUDITOR"); fi
if [ -n "${NO_ENVELOPE:-}" ]; then args+=(--no-envelope); fi
# --expect-public: refuse before proving unless the chain's copy of the model is ours.
"$RAND" call "$id" --expect-public "$here/model.txt" "${args[@]}"
