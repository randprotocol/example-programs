#!/usr/bin/env bash
# Run the image on the zkVM's emulator, off chain, over the demo lists in lists/ with fresh
# salts: the intersection of A and B (3 common keys), a match between A and B, no match between
# A and C, then an unsorted list and a list changed after it was committed (each traps — no run,
# so no proof).
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
need_rand_guest
[ -f "$here/image.bin" ] && [ -f "$here/commit/image.bin" ] || die "no image.bin: run ./build.sh"
tier=12
join() { JOIN_COMMIT_IMAGE="$here/commit/image.bin" cargo +"$TOOLCHAIN" run -q --release --manifest-path "$here/core/Cargo.toml" --bin join -- "$@"; }
words() { join inputs "$@" | sed -n 's/^input: //p'; }
run() { "$RG" run "$here/image.bin" --public $1 --input $blinds $2 --tier $tier || true; }

# Salts for this run only (a real party keeps its salt beside its list: ./commit.sh).
sa="$(mktemp)"; sb="$(mktemp)"; sc="$(mktemp)"; su="$(mktemp)"; altered="$(mktemp)"
trap 'rm -f "$sa" "$sb" "$sc" "$su" "$altered"' EXIT
for s in "$sa" "$sb" "$sc" "$su"; do od -An -N32 -tu4 /dev/urandom | tr -s ' \n' ' ' | sed 's/^ //;s/ $//' > "$s"; done
blinds="$(od -An -N8 -tu4 /dev/urandom | tr -s ' \n' ' ' | sed 's/^ //;s/ $//')"
l="$here/lists"
ca="$(join commit "$l/a.txt" "$sa")"; cb="$(join commit "$l/b.txt" "$sb")"
cc="$(join commit "$l/c.txt" "$sc")"; cu="$(join commit "$l/unsorted.txt" "$su")"
echo "C_A = $(echo "$ca" | cut -d' ' -f1-2)…   C_B = $(echo "$cb" | cut -d' ' -f1-2)…   (public, with the mode)"
echo
echo "mode 0, A and B (15 records each, 3 in common): accepted"
run "$ca $cb 0" "$(words "$l/a.txt" "$sa" "$l/b.txt" "$sb")"
echo
echo "mode 1, A and B (each lists the other's id): accepted, match"
run "$ca $cb 1" "$(words "$l/a.txt" "$sa" "$l/b.txt" "$sb")"
echo
echo "mode 1, A and C (A lists C; C does not list A): accepted, no match"
run "$ca $cc 1" "$(words "$l/a.txt" "$sa" "$l/c.txt" "$sc")"
echo
echo "mode 0, an unsorted list committed as A: refused"
run "$cu $cb 0" "$(words "$l/unsorted.txt" "$su" "$l/b.txt" "$sb")"
echo
echo "mode 0, A with its last record dropped after committing: refused"
sed '$d' "$l/a.txt" > "$altered"
run "$ca $cb 0" "$(words "$altered" "$sa" "$l/b.txt" "$sb")"
