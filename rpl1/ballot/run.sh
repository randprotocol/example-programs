#!/usr/bin/env bash
# Run the ballot on the zkVM's emulator, off chain, over the demo roll (roll.txt: five voters,
# three options) and ballots (ballots.txt: two abstain). The public words are the honest roll's
# fold throughout; each case hands the program different private words:
#   - the honest roll and ballots                     accepted: the totals
#   - the roll with one voter's weight changed        refused (no run, so no proof)
#   - the roll with one voter dropped                 refused
#   - one ballot with choice 7                        accepted: counted as an abstention
#   - a roll of 16 voters, the cap                    accepted: the tier a full call needs
#   - a roll of 17 voters                             refused
# Each call gets two fresh blind words from /dev/urandom, as call.sh does.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
need_rand_guest
[ -f "$here/image.bin" ] && [ -f "$here/fold/image.bin" ] || die "no image.bin: run ./build.sh"
export FOLD_IMAGE="$here/fold/image.bin"
tier=12
tmp="$(mktemp -d)"; trap 'rm -rf "$tmp"' EXIT

ballot() { cargo +"$TOOLCHAIN" run -q --release --manifest-path "$here/core/Cargo.toml" --bin ballot -- "$@"; }
blind() { od -An -N8 -tu4 /dev/urandom | tr -s ' \n' ' ' | sed 's/^ //;s/ $//'; }

# case <label> <roll> <ballots> <public words>: the words from the tool, then the real run.
case_() {
    local label="$1" roll="$2" ballots="$3" public="$4" plan words
    plan="$(ballot inputs "$roll" "$ballots")"
    words="$(echo "$plan" | sed -n 's/^input: //p')"
    echo "$label:"
    echo "  expected: tally $(echo "$plan" | sed -n 's/^tally: //p'), abstain $(echo "$plan" | sed -n 's/^abstain: //p')"
    "$RG" run "$here/image.bin" --public $public --input $(blind) $words --tier "$tier" || true
    echo
}

public="$(ballot roll "$here/roll.txt")"
echo "public input (vote.txt): $public"
echo

case_ "5 voters, 3 options, two abstain (honest roll and ballots)" "$here/roll.txt" "$here/ballots.txt" "$public"

sed 's/^102 300$/102 301/' "$here/roll.txt" > "$tmp/reweighted.txt"
case_ "voter 102's weight 300 → 301 (the roll no longer folds to R)" "$tmp/reweighted.txt" "$here/ballots.txt" "$public"

grep -v '^105 ' "$here/roll.txt" > "$tmp/dropped.txt"
case_ "voter 105 dropped from the roll" "$tmp/dropped.txt" "$here/ballots.txt" "$public"

sed 's/^103 0$/103 7/' "$here/ballots.txt" > "$tmp/seven.txt"
case_ "voter 103 chooses 7 (not an option: an abstention)" "$here/roll.txt" "$tmp/seven.txt" "$public"

{ echo "options 4"; for i in $(seq 1 16); do echo "$((200 + i)) $((i * 1000))"; done; } > "$tmp/roll16.txt"
{ for i in $(seq 1 16); do echo "$((200 + i)) $(((i - 1) % 4))"; done; } > "$tmp/ballots16.txt"
public16="$(ballot roll "$tmp/roll16.txt")"
case_ "16 voters, 4 options (the cap): the tier a full call needs" "$tmp/roll16.txt" "$tmp/ballots16.txt" "$public16"

echo "17 voters (over the cap):"
echo "  the tool refuses the roll before any run:"
{ echo "options 2"; for i in $(seq 1 17); do echo "$((300 + i)) 1"; done; } > "$tmp/roll17.txt"
ballot roll "$tmp/roll17.txt" || true
echo "  and the program refuses the words if handed them anyway:"
words17="17"; for i in $(seq 1 17); do words17+=" $((300 + i)) 1 0 0"; done
"$RG" run "$here/image.bin" --public $public --input $(blind) $words17 --tier "$tier" || true
