#!/usr/bin/env bash
# What a bidder, or anyone, checks against what the auctioneer published — with the program's own
# hash, run on the emulator (commit/image.bin), never on chain.
#   ./verify.sh <tag> <bid> <salt>          your commitment: it must be one line of the published list
#   ./verify.sh --fold <commitments file>   the list's fold: it must equal the receipt's out[3..8]
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
need_rand_guest
[ -f "$here/commit/image.bin" ] || die "no commit/image.bin: run ./build.sh"
auction() { cargo +"$TOOLCHAIN" run -q --manifest-path "$here/core/Cargo.toml" --bin auction -- "$@"; }

if [ "${1:-}" = --fold ]; then
    auction fold "${2:?usage: ./verify.sh --fold <commitments file>}"
else
    # Accept the three words as arguments or as one quoted bids line.
    read -r tag bid salt _ <<< "$*"
    [ -n "${salt:-}" ] || die "usage: ./verify.sh <tag> <bid> <salt>  |  ./verify.sh --fold <commitments file>"
    auction commit "$tag" "$bid" "$salt"
fi
