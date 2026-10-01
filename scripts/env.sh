# Sourced by every example's scripts. Override any of these in your environment.
#
#   RAND        the `rand` wallet CLI (fullnode v0.6.8 or later for RPL-2)
#   RAND_RPC    the node; default the durian devnet's public wallet endpoint (chain 1919)
#   RAND_KEY    the wallet's spend-key file; its notes live beside it in <key>.notes.json
#   CIRCUITS    a checkout of the circuits repo (guest-sdk, rand-guest, sbpf2rv)
#   TOOLCHAIN   the pinned Rust toolchain guests build with
#
# Example: RAND_RPC=http://127.0.0.1:8545 RAND_KEY=~/rand/wallet.key.json ./deploy.sh

: "${RAND:=rand}"
: "${RAND_RPC:=https://durian.market/api/wallet-rpc}"
: "${RAND_KEY:=$HOME/.rand/wallet.key.json}"
: "${CIRCUITS:=$HOME/circuits}"
: "${TOOLCHAIN:=1.98.1}"
export RAND RAND_RPC RAND_KEY CIRCUITS TOOLCHAIN

RG="$CIRCUITS/rand-guest/target/release/rand-guest"
# Where this repository is, and the off-chain hasher the RPL-2 examples' tools use.
# (bash only: every script here is `#!/usr/bin/env bash`; sourced from another shell this is unset.)
EXAMPLES=""
[ -n "${BASH_SOURCE[0]:-}" ] && EXAMPLES="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SECRET_HASH="$EXAMPLES/rpl2/kit/secret-hash/image.bin"
export SECRET_HASH

die() { echo "error: $*" >&2; exit 1; }

need_rand() {
    command -v "$RAND" >/dev/null || die "no \`$RAND\` on PATH (set RAND=/path/to/rand)"
    [ -f "$RAND_KEY" ] || die "no wallet at $RAND_KEY: run \`$RAND --key $RAND_KEY keygen\` (and \`$RAND --key $RAND_KEY faucet\` on a test chain)"
}

need_rand_guest() {
    [ -d "$CIRCUITS/guest-sdk" ] || die "CIRCUITS=$CIRCUITS is not a circuits checkout (no guest-sdk/)"
    if [ ! -x "$RG" ]; then
        echo "building rand-guest once…" >&2
        (cd "$CIRCUITS/rand-guest" && cargo +"$TOOLCHAIN" build --release -q)
    fi
}

# build_guest <crate dir> [extra dirs to copy beside it…]
#
# `rand-guest build` only builds a guest that sits inside a circuits checkout (it finds
# guest-sdk/ above the guest directory), so this copies the crate to
# $CIRCUITS/example-programs/<name>/ and builds there, then refuses an image that links the
# panic machinery — guest-sdk's panic handler halts, and a halted run is a provable run, so a
# program whose refusal can panic would accept what it means to refuse. The image comes back as
# <crate dir>/image.bin (+ .sha256).
build_guest() {
    local src name dst elf nm extra
    src="$(cd "$1" 2>/dev/null && pwd)" || die "build_guest: no such directory: $1"
    shift
    # Never let an empty or root path reach `rsync --delete` below.
    [ -n "$src" ] && [ "$src" != / ] && [ -f "$src/Cargo.toml" ] || die "build_guest: not a crate: $src"
    need_rand_guest
    name="$(basename "$src")"
    [ -n "$name" ] && [ "$name" != / ] || die "build_guest: no crate name in $src"
    dst="$CIRCUITS/example-programs/$name"
    mkdir -p "$dst"
    rsync -a --delete --exclude target --exclude 'image.bin*' "$src/" "$dst/"
    for extra in "$@"; do
        extra="$(cd "$extra" 2>/dev/null && pwd)" || die "build_guest: no such directory: $extra"
        [ "$extra" != / ] && [ -f "$extra/Cargo.toml" ] || die "build_guest: not a crate: $extra"
        mkdir -p "$CIRCUITS/example-programs/$(basename "$extra")"
        rsync -a --delete --exclude target "$extra/" "$CIRCUITS/example-programs/$(basename "$extra")/"
    done
    "$RG" build "$dst"
    elf="$(find "$dst/target/riscv32im-unknown-none-elf/release" -maxdepth 1 -type f -perm -u+x | head -1)"
    nm="$(find "$(rustc +"$TOOLCHAIN" --print sysroot)" -name llvm-nm | head -1)"
    [ -n "$nm" ] || die "llvm-nm not found: rustup +$TOOLCHAIN component add llvm-tools"
    if "$nm" "$elf" | grep -i -E 'panic|unwind|core..fmt|bounds_check|slice_index'; then
        die "the image links a panicking path (above): a panic halts, and a halted run is provable"
    fi
    cp "$dst/image.bin" "$dst/image.bin.sha256" "$src/"
    echo "image: $src/image.bin"
}

# word8_hex w0 … w7 → 64 hex, each word little-endian (the chain's spelling of a cell key/value)
word8_hex() {
    local w out=""
    for w in "$@"; do out+="$(printf '%02x%02x%02x%02x' $((w & 255)) $(((w >> 8) & 255)) $(((w >> 16) & 255)) $(((w >> 24) & 255)))"; done
    echo "$out"
}

# hex_words <64 hex> → the eight u32 words, space-separated
hex_words() {
    local h="$1" i out=()
    for i in 0 8 16 24 32 40 48 56; do
        out+=("$((16#${h:i+6:2}${h:i+4:2}${h:i+2:2}${h:i:2}))")
    done
    echo "${out[*]}"
}

# The program id printed by `rand program deploy` is saved here, per example.
save_id() { echo "$1" > "$2/program.id"; echo "program id saved to $2/program.id"; }
load_id() { [ -f "$1/program.id" ] || die "no $1/program.id: run ./deploy.sh first (or write the id there)"; cat "$1/program.id"; }

# rand_units <amount in RAND, up to 9 decimals> → base units (1 RAND = 10^9), exactly, no floats
rand_units() {
    local a="$1" int frac
    [[ "$a" =~ ^[0-9]+(\.[0-9]{1,9})?$ ]] || die "not a RAND amount: $a"
    int="${a%%.*}"; frac=""
    [[ "$a" == *.* ]] && frac="${a#*.}"
    frac="${frac}000000000"; frac="${frac:0:9}"
    echo $(( 10#$int * 1000000000 + 10#$frac ))
}

# --- The RPL-2 examples built on rpl2/kit (amm, stablecoin, lending, perp, …) ---------------------

# need_secret_hash: build rpl2/kit/secret-hash/image.bin once (the guests' own Poseidon2, off chain).
need_secret_hash() {
    [ -n "$EXAMPLES" ] && [ -d "$EXAMPLES/rpl2/kit" ] || die "source scripts/env.sh from bash (the examples' scripts are bash scripts)"
    need_rand_guest
    [ -f "$SECRET_HASH" ] || build_guest "$EXAMPLES/rpl2/kit/secret-hash" >&2
}

# plan <example dir> <subcommand> [--flag value …]: the example's host-side planner (core/src/bin/
# plan.rs), which builds a transition and its private inputs from the cells it is given.
plan() {
    local dir="$1"; shift
    need_secret_hash
    cargo +"$TOOLCHAIN" run -q --release --manifest-path "$dir/core/Cargo.toml" --bin plan -- "$@"
}

# new_secret <file>: eight random words, mode 600, never over an existing file (a lost secret is a
# lost position).
new_secret() {
    [ -e "$1" ] && die "$1 exists; move it away first (it is the only key to what it owns)"
    ( umask 077; od -An -N32 -tu4 /dev/urandom | tr -s ' \n' ' ' | sed 's/^ //;s/ $//' > "$1" )
    echo "new secret: $1 (keep it private)"
}

# digest <tag> <secret file>: POSEIDON2([tag, s0..s7]) as eight words, via secret-hash.
digest() {
    need_secret_hash
    "$RG" run "$SECRET_HASH" --input "$1" $(cat "$2") | sed -n 's/^out\[[0-7]\] = //p' | tr '\n' ' ' | sed 's/ $//'
}

# cell <program id> <key hex>: the cell's value as 64 hex (64 zeros when absent).
cell() {
    local v
    v="$("$RAND" program state "$1" --cell "$2" | sed -n 's/.*"value": "\(0x\)\{0,1\}\([0-9a-f]\{64\}\)".*/\2/p' | head -1)"
    [ -n "$v" ] || die "could not read cell $2 of program $1 (is the node reachable, and does the program exist?)"
    echo "$v"
}

# smallest_tier <image> <public words> <input words>: the first tier the run fits, as rand-guest
# reports it ("tier 12: fits (cycles … of …, Poseidon2 permutations … of …)").
smallest_tier() {
    local t line
    for t in 10 12 14 16 18 20; do
        line="$("$RG" run "$1" --public $2 --input $3 --tier "$t" 2>&1 | grep "^tier $t: " || true)"
        case "$line" in *": fits"*) echo "$line"; return ;; esac
    done
    echo "fits no tier"
}

# run_demo <image> <example dir> [plan flags…]: run every step of `plan demo` on the emulator and
# check the guest agrees with the host rules (accept: a run with outputs; refuse: no run at all).
run_demo() {
    local image="$1" dir="$2" label="" public="" input="" expect="" ok=0 bad=0 out
    shift 2
    need_rand_guest
    while IFS= read -r line; do
        case "$line" in
            step:*) label="${line#step: }" ;;
            public:*) public="${line#public: }" ;;
            input:*) input="${line#input: }" ;;
            expect:*)
                expect="${line#expect: }"
                set +e; out="$("$RG" run "$image" --public $public --input $input 2>&1)"; set -e
                if echo "$out" | grep -q '^trap'; then got=refuse; else got=accept; fi
                if [ "$got" = "$expect" ]; then
                    ok=$((ok + 1)); printf '  ok    %-7s %s' "$got" "$label"
                    [ "$got" = accept ] && printf '   [%s]' "$(smallest_tier "$image" "$public" "$input")"
                    echo
                else
                    bad=$((bad + 1)); echo "  FAIL  expected $expect, got $got: $label"; echo "$out" | sed 's/^/        /'
                fi ;;
        esac
    done < <(plan "$dir" demo "$@")
    echo "$ok steps agree with the host rules, $bad do not"
    [ "$bad" -eq 0 ]
}
