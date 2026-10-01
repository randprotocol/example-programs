//! The RPL-2 transition context, as a program reads it.
//!
//! The call proof is made over `public ‖ call_binding ‖ context` (fullnode
//! `docs/program-state.md`, "What the program sees"):
//!
//! | words | field |
//! |---|---|
//! | 0 | version, `1` |
//! | 1..=4 | `n_reads`, `n_writes`, `n_pays`, `n_mints` |
//! | 5, 6 | `burn_r` — RAND coming in (low, high) |
//! | 7 | inflow: 0 none, 1 deposit, 2 burn |
//! | 8 | `burn_asset` |
//! | 9, 10 | `burn_a` — the token coming in (low, high) |
//! | then | each read: key (8), value (8); each write: key (8), value (8) |
//! | then | each pay, then each mint: asset, amount low, amount high |
//!
//! **A program must check every word it is shown.** [`Header::shape`] pins the four counts and
//! the [`Header`]'s inflow helpers pin the five inflow words; a program then pins every key and
//! every value word of every cell it is shown, and the asset of every payout.

/// The context layout's version.
pub const CONTEXT_VERSION: u32 = 1;
/// RAND's asset index.
pub const RAND: u32 = 0;

pub const INFLOW_NONE: u32 = 0;
pub const INFLOW_DEPOSIT: u32 = 1;
pub const INFLOW_BURN: u32 = 2;

const HEADER: u32 = 11;
const CELL: u32 = 16;
const PAYOUT: u32 = 3;

/// Where a program reads from. The guest answers with syscalls; tests and the host tools with
/// vectors.
pub trait Source {
    /// The program's deploy-time public input, word `i`.
    fn public(&self, i: u32) -> u32;
    /// Context word `i` (counted from the word after the call binding).
    fn ctx(&self, i: u32) -> u32;
    /// Private input word `i`.
    fn input(&self, i: u32) -> u32;
    /// `POSEIDON2(msg)`, the first eight words of the sponge's output.
    fn hash9(&self, msg: [u32; 9]) -> [u32; 8];
}

/// Two words, low first, as a u64.
#[inline(always)]
pub fn u64_of(lo: u32, hi: u32) -> u64 {
    (lo as u64) | ((hi as u64) << 32)
}

/// A u64 as two words, low first.
#[inline(always)]
pub fn words_of(x: u64) -> (u32, u32) {
    (x as u32, (x >> 32) as u32)
}

/// The context's header, read once.
#[derive(Clone, Copy, Debug)]
pub struct Header {
    pub n_reads: u32,
    pub n_writes: u32,
    pub n_pays: u32,
    pub n_mints: u32,
    pub burn_r: u64,
    pub inflow: u32,
    pub burn_asset: u32,
    pub burn_a: u64,
}

impl Header {
    /// The header, or `None` if the context is not version 1.
    #[inline(always)]
    pub fn read<S: Source>(s: &S) -> Option<Header> {
        if s.ctx(0) != CONTEXT_VERSION {
            return None;
        }
        Some(Header {
            n_reads: s.ctx(1),
            n_writes: s.ctx(2),
            n_pays: s.ctx(3),
            n_mints: s.ctx(4),
            burn_r: u64_of(s.ctx(5), s.ctx(6)),
            inflow: s.ctx(7),
            burn_asset: s.ctx(8),
            burn_a: u64_of(s.ctx(9), s.ctx(10)),
        })
    }

    /// The four counts are exactly these.
    #[inline(always)]
    pub fn shape(&self, reads: u32, writes: u32, pays: u32, mints: u32) -> bool {
        (self.n_reads == reads) & (self.n_writes == writes) & (self.n_pays == pays) & (self.n_mints == mints)
    }

    /// Nothing comes in: no RAND, no token, inflow none.
    #[inline(always)]
    pub fn nothing_in(&self) -> bool {
        (self.burn_r == 0) & (self.inflow == INFLOW_NONE) & (self.burn_asset == 0) & (self.burn_a == 0)
    }

    /// No token comes in (RAND may: `burn_r` is whatever it is).
    #[inline(always)]
    pub fn no_token_in(&self) -> bool {
        (self.inflow == INFLOW_NONE) & (self.burn_asset == 0) & (self.burn_a == 0)
    }

    /// Exactly one asset comes in, as a deposit into the vault: RAND through `burn_r`, or a
    /// token through `burn_a` with inflow `deposit` — never both, never nothing. `(asset, amount)`.
    #[inline(always)]
    pub fn one_deposit(&self) -> Option<(u32, u64)> {
        if (self.burn_r != 0) & self.no_token_in() {
            Some((RAND, self.burn_r))
        } else if (self.burn_r == 0) & (self.inflow == INFLOW_DEPOSIT) & (self.burn_a != 0) & (self.burn_asset != RAND) {
            Some((self.burn_asset, self.burn_a))
        } else {
            None
        }
    }

    /// Read `i`'s cell.
    #[inline(always)]
    pub fn read_cell(&self, i: u32) -> Cell {
        Cell(HEADER.wrapping_add(CELL.wrapping_mul(i)))
    }

    /// Write `i`'s cell.
    #[inline(always)]
    pub fn write_cell(&self, i: u32) -> Cell {
        Cell(HEADER.wrapping_add(CELL.wrapping_mul(self.n_reads.wrapping_add(i))))
    }

    /// Pay `i`: `(asset, amount)`.
    #[inline(always)]
    pub fn pay<S: Source>(&self, s: &S, i: u32) -> (u32, u64) {
        let cells = CELL.wrapping_mul(self.n_reads.wrapping_add(self.n_writes));
        let at = HEADER.wrapping_add(cells).wrapping_add(PAYOUT.wrapping_mul(i));
        (s.ctx(at), u64_of(s.ctx(at.wrapping_add(1)), s.ctx(at.wrapping_add(2))))
    }

    /// Mint `i`: `(asset, amount)`.
    #[inline(always)]
    pub fn mint<S: Source>(&self, s: &S, i: u32) -> (u32, u64) {
        self.pay(s, self.n_pays.wrapping_add(i))
    }
}

/// A cell of the context — a read or a write — by the index of its first key word.
///
/// Index arithmetic here wraps: the counts come from the caller, and a nonsense count must give
/// a nonsense index (whose read finds no word, or a word the rules then refuse), never a panic.
#[derive(Clone, Copy, Debug)]
pub struct Cell(pub u32);

impl Cell {
    #[inline(always)]
    pub fn key<S: Source>(&self, s: &S, i: u32) -> u32 {
        s.ctx(self.0.wrapping_add(i))
    }

    #[inline(always)]
    pub fn val<S: Source>(&self, s: &S, i: u32) -> u32 {
        s.ctx(self.0.wrapping_add(8).wrapping_add(i))
    }

    /// Value words `i` and `i + 1` as a u64.
    #[inline(always)]
    pub fn val64<S: Source>(&self, s: &S, i: u32) -> u64 {
        u64_of(self.val(s, i), self.val(s, i.wrapping_add(1)))
    }

    /// All eight key words.
    #[inline(always)]
    pub fn keys<S: Source>(&self, s: &S) -> [u32; 8] {
        [
            self.key(s, 0), self.key(s, 1), self.key(s, 2), self.key(s, 3),
            self.key(s, 4), self.key(s, 5), self.key(s, 6), self.key(s, 7),
        ]
    }

    /// All eight value words.
    #[inline(always)]
    pub fn value<S: Source>(&self, s: &S) -> [u32; 8] {
        [
            self.val(s, 0), self.val(s, 1), self.val(s, 2), self.val(s, 3),
            self.val(s, 4), self.val(s, 5), self.val(s, 6), self.val(s, 7),
        ]
    }

    /// The key is exactly `k`.
    #[inline(never)]
    pub fn key_is<S: Source>(&self, s: &S, k: &[u32; 8]) -> bool {
        eq8(&self.keys(s), k)
    }

    /// The value is exactly `v`.
    #[inline(never)]
    pub fn value_is<S: Source>(&self, s: &S, v: &[u32; 8]) -> bool {
        eq8(&self.value(s), v)
    }

    /// The value is eight zeros: an absent cell, or one being deleted.
    #[inline(always)]
    pub fn is_zero<S: Source>(&self, s: &S) -> bool {
        self.value_is(s, &[0; 8])
    }
}

/// Two eight-word arrays are equal, without a branch per word.
#[inline(always)]
pub fn eq8(a: &[u32; 8], b: &[u32; 8]) -> bool {
    let mut acc = 0;
    for (x, y) in a.iter().zip(b.iter()) {
        acc |= x ^ y;
    }
    acc == 0
}
