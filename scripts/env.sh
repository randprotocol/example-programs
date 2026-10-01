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
    src="$(cd "$1" && pwd)"; shift
    need_rand_guest
    name="$(basename "$src")"
    dst="$CIRCUITS/example-programs/$name"
    mkdir -p "$dst"
    rsync -a --delete --exclude target --exclude 'image.bin*' "$src/" "$dst/"
    for extra in "$@"; do rsync -a --delete --exclude target "$extra" "$CIRCUITS/example-programs/"; done
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
