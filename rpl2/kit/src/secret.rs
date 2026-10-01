//! Who may do what, without a sender.
//!
//! An RPL-2 program never learns who invoked it: a transaction names nobody, and recipients are
//! fixed by the call binding, not shown to the program. So a program that must know "this is the
//! owner of that position" or "this is the operator" asks for a **secret** instead: eight private
//! input words whose digest, `POSEIDON2([tag, s0..s7])`, is already on record — as the program's
//! deploy-time public input (an operator's *lock*, like `vault/`'s) or as a cell's key (an
//! owner's position). The proof shows the secret was known; it never leaves the prover.
//!
//! The message is a fixed nine words with a domain tag of its own per use, since guest-sdk's
//! sponge does not pad. A proof is bound to its transaction, so a secret's use seen in flight
//! cannot be replayed or redirected.

use crate::ctx::{eq8, Source};

/// Private input words `at..at + 8`: a secret.
#[inline(always)]
pub fn secret_at<S: Source>(s: &S, at: u32) -> [u32; 8] {
    [
        s.input(at), s.input(at + 1), s.input(at + 2), s.input(at + 3),
        s.input(at + 4), s.input(at + 5), s.input(at + 6), s.input(at + 7),
    ]
}

/// `POSEIDON2([tag, s0..s7])`.
#[inline(always)]
pub fn digest<S: Source>(s: &S, tag: u32, secret: &[u32; 8]) -> [u32; 8] {
    s.hash9([tag, secret[0], secret[1], secret[2], secret[3], secret[4], secret[5], secret[6], secret[7]])
}

/// The deploy-time public input words `at..at + 8`, the operator's lock.
#[inline(always)]
pub fn lock_at<S: Source>(s: &S, at: u32) -> [u32; 8] {
    [
        s.public(at), s.public(at + 1), s.public(at + 2), s.public(at + 3),
        s.public(at + 4), s.public(at + 5), s.public(at + 6), s.public(at + 7),
    ]
}

/// The private inputs at `at` are the secret whose `tag`-digest is the public lock at `lock`.
#[inline(never)]
pub fn opens_lock<S: Source>(s: &S, tag: u32, at: u32, lock: u32) -> bool {
    eq8(&digest(s, tag, &secret_at(s, at)), &lock_at(s, lock))
}

/// The key of a cell owned by a secret's holder: `[cell_tag, d0, …, d6]`, `d` the secret's
/// `secret_tag`-digest. Seven words (224 bits) of the digest are plenty to name it.
#[inline(always)]
pub fn owned_key(cell_tag: u32, d: &[u32; 8]) -> [u32; 8] {
    [cell_tag, d[0], d[1], d[2], d[3], d[4], d[5], d[6]]
}
