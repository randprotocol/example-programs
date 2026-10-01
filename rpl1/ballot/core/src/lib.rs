//! ballot: a private ballot with a public tally.
//!
//! The organiser publishes an **eligible-voter roll** — `(voter_tag, weight)` pairs — and
//! deploys the program with the roll's digest `R` and the number of options as its public input,
//! so the program id binds who may vote and with what weight. The tallier then calls the program
//! once with every ballot as private inputs. The program recomputes the roll's digest from the
//! `(voter_tag, weight)` pairs it was given and refuses unless it equals `R`, so nobody can be
//! added, dropped, re-weighted or reordered; it adds each weight to the option chosen, or to the
//! abstentions when the choice is not an option; and it publishes the four totals.
//!
//! ```text
//! public input    [n_options, R0, …, R7]                                 n_options in 2..=4
//! private inputs  [blind0, blind1, n_voters, (voter_tag, w_lo, w_hi, choice) × n_voters]
//! outputs         [t0_lo, t0_hi, t1_lo, t1_hi, t2_lo, t2_hi, t3_lo, t3_hi]
//! ```
//!
//! The roll's digest is a fold: `state₀ = [n_voters, 0, 0, 0, 0, 0, 0, 0]` and
//! `stateᵢ₊₁ = POSEIDON2([TAG, stateᵢ, voter_tagᵢ, w_loᵢ, w_hiᵢ])` — a fixed twelve-word message
//! with its own domain tag, since the sponge does not pad — and `R = stateₙ`. Seeding with the
//! length and chaining every pair makes every change to the roll a different `R`.
//!
//! Nothing here may panic: a panic halts, and a halted run is a provable run. No division, no
//! variable indexing, no `unwrap`; `u64` sums are checked and a failure is a refusal.
#![cfg_attr(target_arch = "riscv32", no_std)]
#![forbid(unsafe_code)]

#[cfg(not(target_arch = "riscv32"))]
pub mod host;

/// "roll", little-endian: the fold's domain tag.
pub const TAG: u32 = 0x6c6c_6f72;
/// The roll's size cap. At 16 voters a call fits tier 12 with room to spare (`run.sh` measures
/// it); the cap exists because `n_voters` is a private word and a loop bound must be fixed
/// before any loop uses it.
pub const MAX_VOTERS: u32 = 16;
pub const MIN_OPTIONS: u32 = 2;
pub const MAX_OPTIONS: u32 = 4;

/// The public input: the number of options, then the roll's digest.
pub const PUBLIC_OPTIONS: u32 = 0;
pub const PUBLIC_ROLL: u32 = 1;
pub const PUBLIC_WORDS: u32 = 9;
/// The private inputs: two blind words, the count, then four words per voter.
pub const INPUT_VOTERS: u32 = 2;
pub const FIRST_VOTER: u32 = 3;
pub const VOTER_WORDS: u32 = 4;

/// Twelve words in, the sponge's first eight out: `poseidon2` on the guest, a stand-in in tests.
pub trait Hash {
    fn hash12(&self, msg: [u32; 12]) -> [u32; 8];
}

/// Where the words come from: syscalls on the guest, vectors in a test or the host tool.
pub trait Source: Hash {
    fn public(&self, i: u32) -> u32;
    fn input(&self, i: u32) -> u32;
}

/// Why a call was refused. The guest never says — a refused call has no proof — so these are for
/// the tests and the host tool.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// `n_options` outside `2..=4`.
    Options,
    /// `n_voters` zero or above [`MAX_VOTERS`].
    Voters,
    /// A weight of 2^63 or more.
    Weight,
    /// The roll's total weight reached 2^63.
    Overflow,
    /// The `(voter_tag, weight)` pairs do not fold to the public `R`.
    Roll,
}
use Refusal::*;

#[inline(always)]
pub fn u64_of(lo: u32, hi: u32) -> u64 {
    u64::from(lo) | (u64::from(hi) << 32)
}

#[inline(always)]
pub fn words_of(x: u64) -> (u32, u32) {
    (x as u32, (x >> 32) as u32)
}

/// Below 2^63: every amount the chain holds is, and so is every weight here.
#[inline(always)]
pub fn lt_note(x: u64) -> bool {
    x >> 63 == 0
}

/// The fold's starting state: the roll's length, so a roll and a prefix of it never share `R`.
#[inline(always)]
pub fn seed(n_voters: u32) -> [u32; 8] {
    [n_voters, 0, 0, 0, 0, 0, 0, 0]
}

/// One voter folded in.
#[inline(always)]
pub fn step<H: Hash>(h: &H, st: [u32; 8], voter_tag: u32, w_lo: u32, w_hi: u32) -> [u32; 8] {
    h.hash12([TAG, st[0], st[1], st[2], st[3], st[4], st[5], st[6], st[7], voter_tag, w_lo, w_hi])
}

/// Accept or refuse the call `s` shows. On acceptance, the receipt's eight output words: the
/// four totals, each a u64 little-endian. Abstentions are not output; they are the roll's total
/// weight less the four totals, and the roll is public.
pub fn check<S: Source>(s: &S) -> Result<[u32; 8], Refusal> {
    // The two blind words: read, never used, never output. They make the inputs unguessable.
    let _ = s.input(0);
    let _ = s.input(1);
    let n_options = s.public(PUBLIC_OPTIONS);
    if n_options < MIN_OPTIONS || n_options > MAX_OPTIONS {
        return Err(Options);
    }
    let n = s.input(INPUT_VOTERS);
    if n == 0 || n > MAX_VOTERS {
        return Err(Voters);
    }
    let mut st = seed(n);
    let mut t = [0u64; 4];
    let mut total = 0u64;
    let mut i = 0u32;
    while i < n {
        let at = FIRST_VOTER.wrapping_add(i.wrapping_mul(VOTER_WORDS));
        let voter_tag = s.input(at);
        let w_lo = s.input(at.wrapping_add(1));
        let w_hi = s.input(at.wrapping_add(2));
        let choice = s.input(at.wrapping_add(3));
        let w = u64_of(w_lo, w_hi);
        if !lt_note(w) {
            return Err(Weight);
        }
        total = match total.checked_add(w) {
            Some(x) if lt_note(x) => x,
            _ => return Err(Overflow),
        };
        st = step(s, st, voter_tag, w_lo, w_hi);
        // Every total is at most `total`, so none of these adds can wrap.
        if choice < n_options {
            match choice {
                0 => t[0] = t[0].wrapping_add(w),
                1 => t[1] = t[1].wrapping_add(w),
                2 => t[2] = t[2].wrapping_add(w),
                _ => t[3] = t[3].wrapping_add(w),
            }
        }
        i = i.wrapping_add(1);
    }
    let mut same = true;
    for (j, w) in st.iter().enumerate() {
        if *w != s.public(PUBLIC_ROLL.wrapping_add(j as u32)) {
            same = false;
        }
    }
    if !same {
        return Err(Roll);
    }
    let (a, b, c, d) = (words_of(t[0]), words_of(t[1]), words_of(t[2]), words_of(t[3]));
    Ok([a.0, a.1, b.0, b.1, c.0, c.1, d.0, d.1])
}
