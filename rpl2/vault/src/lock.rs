//! The lock: `POSEIDON2([TAG, s0, …, s7])`, the first eight words of the sponge's output.
//!
//! Shared by the vault (which compares it with its public input) and `lock-hash` (which prints
//! it, so the owner can deploy with it). The message is a fixed nine words with a domain tag of
//! its own: guest-sdk's sponge does not pad, so a variable-length message would not be safe.

use guest_sdk::poseidon2;

/// "lock", little-endian.
pub const TAG: u32 = 0x6b63_6f6c;

pub fn lock_of(s: [u32; 8]) -> [u32; 9] {
    let mut buf = [TAG, s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7]];
    poseidon2(buf.as_mut_ptr(), 9);
    buf
}
