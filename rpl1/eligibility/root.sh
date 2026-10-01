#!/usr/bin/env bash
# The issuer's step: build the Merkle tree over a credentials file (hashing on the emulator, with
# the guest's own code) and write issuer.txt — the deploy's public input: the eight root words,
# then the cutoff year ("born in this year or earlier").
#   ./root.sh <credentials file> <cutoff_year>
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
need_rand_guest
roll="${1:?usage: ./root.sh <credentials file> <cutoff_year>}"
cutoff="${2:?usage: ./root.sh <credentials file> <cutoff_year>}"
[ -f "$here/hash/image.bin" ] || die "no hash/image.bin: run ./build.sh"
cargo +"$TOOLCHAIN" run -q --release --manifest-path "$here/core/Cargo.toml" --bin issuer -- root "$roll" "$cutoff" > "$here/issuer.txt"
[ "$(wc -w < "$here/issuer.txt")" -eq 9 ] || die "issuer did not print nine words"
echo "issuer.txt: $(cat "$here/issuer.txt")"
