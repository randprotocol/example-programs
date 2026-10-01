#!/usr/bin/env bash
# Commit to a list: make its salt (<list>.secret, eight random words, mode 600) unless it exists,
# and print the eight commitment words — computed by commit/image.bin, the program's own hash.
#   ./commit.sh <list file>
# Hand the printed words to the other party (and to ./public.sh); keep the salt with the list.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
need_rand_guest
[ -f "$here/commit/image.bin" ] || die "no commit/image.bin: run ./build.sh"
list="${1:?usage: ./commit.sh <list file>}"
[ -f "$list" ] || die "no such list: $list"
salt="${list%.*}.secret"
[ -f "$salt" ] || new_secret "$salt" >&2
JOIN_COMMIT_IMAGE="$here/commit/image.bin" cargo +"$TOOLCHAIN" run -q --release \
    --manifest-path "$here/core/Cargo.toml" --bin join -- commit "$list" "$salt"
