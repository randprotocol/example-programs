//! The guest's `Source`: the zkVM's syscalls. Shared by the program (`main.rs`) and the off-chain
//! `commit/` helper, so a bidder recomputes a commitment with exactly the code the program hashes
//! with.

use guest_sdk::{poseidon2, read_input, read_public};
use sealed_auction_core::Source;

pub struct Chain;

impl Source for Chain {
    #[inline(always)]
    fn public(&self, i: u32) -> u32 {
        read_public(i)
    }

    #[inline(always)]
    fn input(&self, i: u32) -> u32 {
        read_input(i)
    }

    /// Six words in, the eight-word digest out. The sponge overwrites `ptr..ptr+8` with the
    /// digest, so the buffer is eight words; the last two are never read.
    #[inline(always)]
    fn hash6(&self, m: [u32; 6]) -> [u32; 8] {
        let mut buf = [m[0], m[1], m[2], m[3], m[4], m[5], 0, 0];
        poseidon2(buf.as_mut_ptr(), 6);
        buf
    }

    #[inline(always)]
    fn hash17(&self, m: [u32; 17]) -> [u32; 8] {
        let mut buf = m;
        poseidon2(buf.as_mut_ptr(), 17);
        [buf[0], buf[1], buf[2], buf[3], buf[4], buf[5], buf[6], buf[7]]
    }
}
