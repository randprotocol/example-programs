#!/usr/bin/env bash
# The pool's cell, its invariant D and a quote, and the vault that holds its reserves.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
id="$(load_id "$here")"
read -r token amp < "$here/public.txt"
"$RAND" program state "$id"
"$RAND" program vault "$id"
pool="$(cell "$id" "$(word8_hex 1 0 0 0 0 0 0 0)")"
plan "$here" quote --token "$token" --amp "$amp" --pool "$pool" || true
