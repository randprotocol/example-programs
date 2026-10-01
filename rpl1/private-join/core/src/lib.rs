//! private-join: two parties each commit to a list of 64-bit keys; a program computes, over the
//! two committed lists, either the size of their intersection or whether each party lists the
//! other, and the receipt carries only that one number.
//!
//! **A party's list** is at most [`MAX`] keys (two words each, little-endian u64), **strictly
//! ascending** — the program refuses an unsorted or repeated key, so a committed count `n` is
//! exact and the intersection size is a set size, not a multiplicity — together with the party's
//! own id (one key, the name the other party knows it by; mode 1 reads it, mode 0 ignores it)
//! and an eight-word salt. Its commitment is `POSEIDON2` over a fixed [`MSG`]-word message:
//!
//! ```text
//! [TAG, n, id_lo, id_hi, k0_lo, k0_hi, …, k15_lo, k15_hi, s0, …, s7]      absent keys are zeros
//! ```
//!
//! guest-sdk's sponge does not pad, so the message is fixed-length with a domain tag of its own;
//! the salt keeps a list from being brute-forced from its commitment.
//!
//! **Public input** (deploy time, so part of the program id): `[C_A (8), C_B (8), mode]`.
//! **Private input**: `[blind, blind, A's block (43), B's block (43)]`, a block being the
//! commitment's message without the tag. The two blind words are uniformly random per call and
//! never used: a call's proof leaks an unsalted function of its input words, and the blinds keep
//! that function from being enumerable.
//!
//! The program recomputes both commitments from the private inputs and refuses unless they are
//! the two public ones; then
//!
//! | mode | output |
//! |---|---|
//! | 0, count | `[|A ∩ B|, 0, 0, …]` |
//! | 1, match | `[1 if id_A ∈ B and id_B ∈ A else 0, 1, 0, …]` |
//!
//! A "no match" is `0` whichever side declined, so the output says nothing about who did.
//!
//! Nothing here may panic: a panic halts, and a halted run is a provable run.
#![cfg_attr(target_arch = "riscv32", no_std)]
#![forbid(unsafe_code)]

#[cfg(not(target_arch = "riscv32"))]
pub mod host;

/// At most this many keys per list.
pub const MAX: usize = 16;
/// The commitment's domain tag: "join", little-endian.
pub const TAG: u32 = 0x6e69_6f6a;
/// One party's block of private input words: `n`, the id (2), the keys (32), the salt (8).
pub const BLOCK: usize = 1 + 2 + 2 * MAX + 8;
/// The commitment's message: the tag, then the block.
pub const MSG: usize = 1 + BLOCK;
/// Where each party's block starts among the private inputs (after the two blinds).
pub const A_AT: u32 = 2;
pub const B_AT: u32 = A_AT + BLOCK as u32;
/// How many private input words a call commits.
pub const INPUT_WORDS: u32 = B_AT + BLOCK as u32;
/// The public input: `C_A` at 0, `C_B` at 8, the mode at 16.
pub const PUBLIC_A: u32 = 0;
pub const PUBLIC_B: u32 = 8;
pub const PUBLIC_MODE: u32 = 16;
pub const PUBLIC_WORDS: u32 = 17;

pub const MODE_COUNT: u32 = 0;
pub const MODE_MATCH: u32 = 1;

/// Where the words come from: syscalls on the guest, vectors in a test or the `join` tool.
pub trait Source {
    fn input(&self, i: u32) -> u32;
    fn public(&self, i: u32) -> u32;
    /// `POSEIDON2` over exactly [`MSG`] words, in place: the digest is then `buf[0..8]`.
    fn hash(&self, buf: &mut [u32; MSG]);
}

/// Why a call was refused. The guest never says — a refused call has no proof — so these are for
/// the tests and the `join` tool.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// The public mode word is neither 0 nor 1.
    Mode,
    /// A block's commitment is not the public one.
    Commitment,
    /// A list claims more than [`MAX`] keys.
    TooMany,
    /// A list's keys are not strictly ascending.
    Unsorted,
}
use Refusal::*;

/// One party's list, as its message holds it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct List {
    pub n: usize,
    pub id: u64,
    pub keys: [u64; MAX],
}

impl List {
    pub const EMPTY: List = List { n: 0, id: 0, keys: [0; MAX] };

    /// Decode a message (before it is hashed in place) into `self`. Keys past `n` are whatever
    /// the message holds; the rules never read them. In place, like [`read_message`]: a `List`
    /// returned by value is a 144-byte `memcpy` on the guest, and cycles are what a tier pays for.
    pub fn decode(&mut self, m: &[u32; MSG]) {
        let mut it = m.iter().copied().skip(1);
        self.n = it.next().unwrap_or(0) as usize;
        self.id = pair(&mut it);
        for k in self.keys.iter_mut() {
            *k = pair(&mut it);
        }
    }

    /// `n ≤ MAX` and the first `n` keys strictly ascending.
    pub fn well_formed(&self) -> Result<(), Refusal> {
        if self.n > MAX {
            return Err(TooMany);
        }
        let pairs = self.keys.iter().zip(self.keys.iter().skip(1)).take(self.n.saturating_sub(1));
        for (p, k) in pairs {
            if *k <= *p {
                return Err(Unsorted);
            }
        }
        Ok(())
    }

    /// Whether `key` is among the first `n` keys.
    pub fn contains(&self, key: u64) -> bool {
        let mut found = false;
        for k in self.keys.iter().take(self.n) {
            found |= *k == key;
        }
        found
    }
}

/// Two words, little-endian, as a u64; zeros past the end.
#[inline(always)]
fn pair(it: &mut impl Iterator<Item = u32>) -> u64 {
    let lo = it.next().unwrap_or(0);
    let hi = it.next().unwrap_or(0);
    u64::from(lo) | (u64::from(hi) << 32)
}

/// `|a ∩ b|` for two well-formed lists: one merge pass, at most `a.n + b.n` steps.
pub fn intersection(a: &List, b: &List) -> u32 {
    let (mut i, mut j, mut c) = (0usize, 0usize, 0u32);
    while i < a.n && j < b.n {
        match (a.keys.get(i), b.keys.get(j)) {
            (Some(x), Some(y)) => {
                if x == y {
                    c = c.wrapping_add(1);
                    i = i.wrapping_add(1);
                    j = j.wrapping_add(1);
                } else if x < y {
                    i = i.wrapping_add(1);
                } else {
                    j = j.wrapping_add(1);
                }
            }
            _ => break,
        }
    }
    c
}

/// Fill `m` with a party's commitment message: the tag, then private input words
/// `at .. at + BLOCK`. In place: a 176-byte array returned by value is a `memcpy` on the guest.
pub fn read_message<S: Source>(s: &S, at: u32, m: &mut [u32; MSG]) {
    m[0] = TAG;
    for (i, w) in m.iter_mut().skip(1).enumerate() {
        *w = s.input(at.wrapping_add(i as u32));
    }
}

/// The commitment of the block at private inputs `at .. at + BLOCK`: `POSEIDON2([TAG, block…])`.
pub fn commitment<S: Source>(s: &S, at: u32) -> [u32; 8] {
    let mut m = [0u32; MSG];
    read_message(s, at, &mut m);
    s.hash(&mut m);
    [m[0], m[1], m[2], m[3], m[4], m[5], m[6], m[7]]
}

/// The hashed message's digest, `m[0..8]`, is the public input's eight words at `at`.
fn digest_is_public<S: Source>(s: &S, m: &[u32; MSG], at: u32) -> bool {
    let mut same = true;
    for (i, w) in m.iter().take(8).enumerate() {
        same &= *w == s.public(at.wrapping_add(i as u32));
    }
    same
}

/// Accept or refuse the call `s` shows. On acceptance, the receipt's eight output words:
/// `[result, mode, 0, 0, 0, 0, 0, 0]`.
pub fn check<S: Source>(s: &S) -> Result<[u32; 8], Refusal> {
    // The two blind words: read, so they are part of what the proof commits to, and never used.
    let _ = (s.input(0), s.input(1));
    let mode = s.public(PUBLIC_MODE);
    if mode != MODE_COUNT && mode != MODE_MATCH {
        return Err(Mode);
    }
    // Each list: read once into its message, decoded, then hashed in place and compared.
    let mut m = [0u32; MSG];
    let (mut a, mut b) = (List::EMPTY, List::EMPTY);
    read_message(s, A_AT, &mut m);
    a.decode(&m);
    s.hash(&mut m);
    if !digest_is_public(s, &m, PUBLIC_A) {
        return Err(Commitment);
    }
    read_message(s, B_AT, &mut m);
    b.decode(&m);
    s.hash(&mut m);
    if !digest_is_public(s, &m, PUBLIC_B) {
        return Err(Commitment);
    }
    a.well_formed()?;
    b.well_formed()?;
    let result = if mode == MODE_COUNT {
        intersection(&a, &b)
    } else {
        (b.contains(a.id) & a.contains(b.id)) as u32
    };
    Ok([result, mode, 0, 0, 0, 0, 0, 0])
}
