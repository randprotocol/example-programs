#!/usr/bin/env bash
# Call the deployed SPL program with an instruction's words:
#   ./call.sh <file of u32 words: the serialized accounts + instruction data>
# --expect-public refuses before proving unless the chain's copy of the ELF is program.so.
#
# Not runnable today: every SPL Token instruction runs ~700 000 cycles, tier 20, and a tier-20
# call proof needs ~330 GB of memory — more than any machine this was tested on. See README.md.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
need_rand
id="$(load_id "$here")"
words="${1:?usage: ./call.sh <words file>}"
args=(); for w in $(cat "$words"); do args+=(--input "$w"); done
"$RAND" call "$id" --expect-public "$here/program.so" "${args[@]}"
