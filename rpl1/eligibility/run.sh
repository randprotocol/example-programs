#!/usr/bin/env bash
# Run the image on the zkVM's emulator, off chain, over the demo roll (credentials.example):
# two credentials that meet the cutoff (halt, with outputs), then the same credential against a
# cutoff it misses, a wrong sibling, and a non-boolean direction (each traps — no run, no proof).
#   ./run.sh [cutoff_year]      default 2008: "18 or older in 2026"
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
need_rand_guest
[ -f "$here/image.bin" ] && [ -f "$here/hash/image.bin" ] || die "no image.bin: run ./build.sh"
roll="$here/credentials.example"
cutoff="${1:-2008}"
tier=12
issuer() { cargo +"$TOOLCHAIN" run -q --release --manifest-path "$here/core/Cargo.toml" --bin issuer -- "$@"; }
root="$(issuer root "$roll")"
blinds="$(od -An -N8 -tu4 /dev/urandom | tr -s ' \n' ' ' | sed 's/^ //;s/ $//')"
run() { "$RG" run "$here/image.bin" --public $root "$1" --input $blinds $2 --tier $tier || true; }

year() { awk -v i="$1" '!/^[[:space:]]*(#|$)/ { if (n++ == i) print $2 }' "$roll"; }
w0="$(issuer path "$roll" 0)"
w1="$(issuer path "$roll" 1)"
echo "issuer root $(echo "$root" | cut -d' ' -f1-2)… (public), cutoff $cutoff (public)"
echo
echo "slot 0, born $(year 0) (private): accepted"
run "$cutoff" "$w0"
echo
echo "slot 1, born $(year 1) (private): accepted"
run "$cutoff" "$w1"
echo
echo "slot 1 against cutoff $(( $(year 1) - 1 )): refused"
run "$(( $(year 1) - 1 ))" "$w1"
echo
echo "slot 1 with one sibling word changed: refused"
run "$cutoff" "$(echo "$w1" | awk '{ $10 = $10 + 1; print }')"
echo
echo "slot 1 with a direction word of 2: refused"
run "$cutoff" "$(echo "$w1" | awk '{ $15 = 2; print }')"
