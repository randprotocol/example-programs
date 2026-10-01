#!/usr/bin/env bash
# Execute SPL Token instructions off chain and compare the translated image with the sBPF
# interpreter, word for word: Transfer, MintTo, Burn, and the refused cases. The instruction
# words (accounts + data, ~10 000 words) are built in code by sbpf2rv's parity test; there is no
# CLI that prints them yet.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
cd "$CIRCUITS/sbpf2rv"
cargo +"$TOOLCHAIN" test --release --test parity the_spl_token -- --nocapture
