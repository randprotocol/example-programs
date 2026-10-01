//! eligibility: a credential an issuer put in a Merkle tree satisfies one predicate — and the
//! proof says neither which credential nor anything else about it.
//!
//! The issuer holds a roll of credentials, each `[id0, id1, id2, id3, birth_year, nonce]`: a
//! 128-bit holder id, a year, and a random word so that a leaf cannot be brute-forced from the
//! root even when the id and the year are guessable. It publishes the root of a depth-[`DEPTH`]
//! Poseidon2 Merkle tree over them ([`LEAVES`] slots; an unused slot is [`EMPTY`], eight zero
//! words, which no credential can hash to):
//!
//! ```text
//! leaf  = POSEIDON2([TAG_LEAF, id0, id1, id2, id3, birth_year, nonce, 0, 0])     9 words, fixed
//! node  = POSEIDON2([TAG_NODE, left0..7, right0..7])                             17 words, fixed
//! ```
//!
//! The program's **deploy-time public input** is `[root0..7, cutoff_year]`, so the program id
//! binds the issuer and the predicate; a different root or cutoff is a different program. The
//! predicate is the one inequality `birth_year <= cutoff_year` ("born in `cutoff_year` or
//! earlier": for "18 or older in 2026", `cutoff_year = 2008`).
//!
//! Private inputs, [`INPUT_WORDS`] in all:
//!
//! ```text
//! 0, 1          blind0, blind1       two random words, never output (salt against brute force)
//! 2..=5         id0..id3
//! 6             birth_year
//! 7             nonce
//! 8 + 9·l ..    sib0..sib7, dir      level l's sibling node and which side the path is on:
//!   (l < DEPTH)                      dir 0 = the path is the LEFT child, 1 = the RIGHT child
//! ```
//!
//! [`check`] recomputes the leaf, folds the path up to a root, and accepts only if the root is
//! the public one, every `dir` is 0 or 1, and the predicate holds. Outputs
//! `[1, cutoff_year, root0..root5]`: a receipt names the issuer (six words of its root — the
//! program id already binds all eight) and the bar that was cleared.
#![cfg_attr(target_arch = "riscv32", no_std)]
#![forbid(unsafe_code)]

#[cfg(not(target_arch = "riscv32"))]
pub mod host;

/// The tree's depth: 2^8 = 256 credential slots.
pub const DEPTH: usize = 8;
pub const LEAVES: usize = 1 << DEPTH;

/// "leaf", little-endian: the leaf message's domain tag.
pub const TAG_LEAF: u32 = 0x6661_656c;
/// "node", little-endian: the node message's domain tag.
pub const TAG_NODE: u32 = 0x6564_6f6e;

/// An unused leaf slot: eight zero words. No credential hashes to it, so an empty slot opens for
/// nobody.
pub const EMPTY: [u32; 8] = [0; 8];

/// The public input: the root at 0..8, the cutoff year at 8.
pub const PUBLIC_WORDS: u32 = 9;
pub const PUBLIC_CUTOFF: u32 = 8;

/// The private inputs: two blinds, six credential words, then nine words per level.
pub const INPUT_WORDS: u32 = 2 + 6 + 9 * DEPTH as u32;
/// Where the credential starts, and where the path starts.
pub const INPUT_CREDENTIAL: u32 = 2;
pub const INPUT_PATH: u32 = 8;

/// The two hashes, with the fixed message lengths the program uses. The guest answers with the
/// `POSEIDON2` syscall, the host by running `hash/image.bin` on the emulator (or, in unit tests,
/// with a stand-in).
pub trait Hash {
    fn hash9(&self, m: [u32; 9]) -> [u32; 8];
    fn hash17(&self, m: [u32; 17]) -> [u32; 8];
}

/// Where the words come from: the guest answers with syscalls, a test with vectors.
pub trait Source: Hash {
    fn public(&self, i: u32) -> u32;
    fn input(&self, i: u32) -> u32;
}

/// One credential, as the issuer holds it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Credential {
    pub id: [u32; 4],
    pub birth_year: u32,
    pub nonce: u32,
}

/// Why a call was refused. The guest never says — a refused call has no proof — so these are for
/// the tests and the `issuer` tool.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// A direction word that is neither 0 nor 1.
    Direction,
    /// The path does not lead to the public root.
    Root,
    /// The credential is in the tree but misses the cutoff.
    Predicate,
}

/// `POSEIDON2([TAG_LEAF, id0..id3, birth_year, nonce, 0, 0])`.
#[inline(always)]
pub fn leaf_of<H: Hash + ?Sized>(h: &H, c: &Credential) -> [u32; 8] {
    h.hash9([TAG_LEAF, c.id[0], c.id[1], c.id[2], c.id[3], c.birth_year, c.nonce, 0, 0])
}

/// `POSEIDON2([TAG_NODE, left0..7, right0..7])`.
#[inline(always)]
pub fn node_of<H: Hash + ?Sized>(h: &H, l: &[u32; 8], r: &[u32; 8]) -> [u32; 8] {
    h.hash17([
        TAG_NODE, l[0], l[1], l[2], l[3], l[4], l[5], l[6], l[7], r[0], r[1], r[2], r[3], r[4], r[5], r[6], r[7],
    ])
}

#[inline(always)]
fn eq8(a: &[u32; 8], b: &[u32; 8]) -> bool {
    let mut same = true;
    for (x, y) in a.iter().zip(b.iter()) {
        same &= *x == *y;
    }
    same
}

/// The rules. `Ok(outputs)` is a provable run; `Err` is a refusal (no proof). Reads every word of
/// the two blinds, the credential and the whole path, so all [`INPUT_WORDS`] are bound to `H_IN`.
pub fn check<S: Source + ?Sized>(s: &S) -> Result<[u32; 8], Refusal> {
    // The blinds: read so that they are part of the input commitment, used for nothing else.
    let _ = (s.input(0), s.input(1));
    let cred = Credential {
        id: [
            s.input(INPUT_CREDENTIAL),
            s.input(INPUT_CREDENTIAL + 1),
            s.input(INPUT_CREDENTIAL + 2),
            s.input(INPUT_CREDENTIAL + 3),
        ],
        birth_year: s.input(INPUT_CREDENTIAL + 4),
        nonce: s.input(INPUT_CREDENTIAL + 5),
    };
    let mut cur = leaf_of(s, &cred);
    let mut at = INPUT_PATH;
    for _ in 0..DEPTH {
        let mut sib = [0u32; 8];
        for (j, w) in sib.iter_mut().enumerate() {
            *w = s.input(at.wrapping_add(j as u32));
        }
        let dir = s.input(at.wrapping_add(8));
        cur = if dir == 0 {
            node_of(s, &cur, &sib)
        } else if dir == 1 {
            node_of(s, &sib, &cur)
        } else {
            return Err(Refusal::Direction);
        };
        at = at.wrapping_add(9);
    }
    let mut root = [0u32; 8];
    for (j, w) in root.iter_mut().enumerate() {
        *w = s.public(j as u32);
    }
    if !eq8(&cur, &root) {
        return Err(Refusal::Root);
    }
    let cutoff = s.public(PUBLIC_CUTOFF);
    if cred.birth_year > cutoff {
        return Err(Refusal::Predicate);
    }
    Ok([1, cutoff, root[0], root[1], root[2], root[3], root[4], root[5]])
}
