#!/usr/bin/env bash
# Translate a Solana (sBPF) program to RISC-V C with sbpf2rv and build it into image.bin.
#   ./build.sh [program.so]     (default: SPL Token, the ELF the circuits repo commits)
# Needs a circuits checkout (CIRCUITS=…) and clang 23.1.1 with the RISC-V backend
# (`brew install llvm`; set CLANG=… if it is not on PATH). Another clang version is refused
# unless RAND_GUEST_CLANG_UNPINNED=1 — and then the image, and so the program id, differ.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
need_rand_guest
elf="${1:-$CIRCUITS/guests-compiled/sbpf/programs/spl_token.so}"
[ -f "$elf" ] || die "no ELF at $elf"
S2R="$CIRCUITS/sbpf2rv/target/release/sbpf2rv"
[ -x "$S2R" ] || (cd "$CIRCUITS/sbpf2rv" && cargo +"$TOOLCHAIN" build --release -q)
dst="$CIRCUITS/example-programs/spl-token"
rm -rf "$dst"; mkdir -p "$(dirname "$dst")"
"$S2R" "$elf" --out "$dst" --name spl-token
"$RG" build "$dst" --max-words 65535
cp "$dst/image.bin" "$dst/image.bin.sha256" "$here/"
# The ELF is the program's public input: the image reads it from the public tape at every call.
cp "$elf" "$here/program.so"
echo "image: $here/image.bin; public input: $here/program.so"
