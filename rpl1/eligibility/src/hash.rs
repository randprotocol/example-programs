//! The two fixed-length Poseidon2 messages this program hashes, as the zkVM computes them.
//!
//! Shared by the guest (`src/main.rs`) and the off-chain hasher (`hash/src/main.rs`, through
//! `#[path]`), so the issuer's tree is built by exactly the code that checks it. guest-sdk's
//! sponge does not pad, so each message is a fixed length with its own domain tag
//! (`eligibility_core::TAG_LEAF`, `TAG_NODE`); the tags are the first word of each message and
//! are put there by `eligibility_core`, not here.

use guest_sdk::poseidon2;

/// `POSEIDON2(m)` over nine words (a leaf's message).
#[inline(never)]
pub fn hash9(m: [u32; 9]) -> [u32; 8] {
    let mut buf = m;
    poseidon2(buf.as_mut_ptr(), 9);
    [buf[0], buf[1], buf[2], buf[3], buf[4], buf[5], buf[6], buf[7]]
}

/// `POSEIDON2(m)` over seventeen words (a node's message).
#[inline(never)]
pub fn hash17(m: [u32; 17]) -> [u32; 8] {
    let mut buf = m;
    poseidon2(buf.as_mut_ptr(), 17);
    [buf[0], buf[1], buf[2], buf[3], buf[4], buf[5], buf[6], buf[7]]
}
