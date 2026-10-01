#!/usr/bin/env bash
# Run the image on the zkVM's emulator, off chain, over the statement files in statements/: a clean
# year (band 3), one missed month (band 2), a stretched borrower (band 1), a three-month gap (band 0:
# the months gate), eleven months (refused — reads past the committed inputs), the clean year under
# a model with max_dti_bps = 0 (band 0: the model sets the bar), and a model no one can be scored by
# (refused). The blind words are fresh per run; the salt is fixed here so the commitment words below
# are reproducible (call.sh draws a real one).
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
need_rand_guest
model="$(cat "$here/model.txt")"
salt="7 11"
blind() { od -An -N8 -tu4 /dev/urandom | tr -s ' \n' ' '; }
words() { sed 's/#.*//' "$1" | tr -s ' \t\n' ' '; }
case_() { # <title> <statements file> [model words]
    local title="$1" file="$2"; shift 2
    echo "$title"
    "$RG" run "$here/image.bin" --public ${*:-$model} --input $(blind) $(words "$file") $salt --tier 12 || true
    echo
}
echo "model (public): $model"
echo
case_ "clean.txt — every month covered, DTI 1960 bps, average balance 7141:" "$here/statements/clean.txt"
case_ "missed.txt — one month without income, DTI 3272 bps:" "$here/statements/missed.txt"
case_ "stretched.txt — DTI 3500 bps, average balance 746:" "$here/statements/stretched.txt"
case_ "gap.txt — three months without income (9 of 12 covered), DTI 2666 bps:" "$here/statements/gap.txt"
case_ "eleven.txt — eleven months, 33 words:" "$here/statements/eleven.txt"
case_ "clean.txt under a model with max_dti_bps = 0:" "$here/statements/clean.txt" 10 0 500 1500 2000 3000
case_ "clean.txt under a model with max_dti_bps = 20000 (no such model):" "$here/statements/clean.txt" 10 20000 500 1500 2000 3000
