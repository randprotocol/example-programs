//! sealed-auction: a sealed-bid second-price (Vickrey) auction, or — the same program with one
//! public word flipped — a sealed request for quote where the lowest quote wins.
//!
//! The auctioneer collects the bids off chain and runs the program once over all of them:
//!
//! ```text
//! public input (deploy)   [mode]                       0: highest wins, 1: lowest wins
//! private inputs (call)   [blind0, blind1, n,  tag bid_lo bid_hi salt0 salt1  × n]     2 ≤ n ≤ 8
//! outputs (receipt)       [winner tag, price lo, price hi, fold0 … fold4]
//! ```
//!
//! **Clearing.** The winner is the best bid — highest in mode 0, lowest in mode 1 — and the
//! price is the best bid among the others: the second-highest (second-lowest). Ties go to the
//! first of the tied bids, which then pays its own bid.
//!
//! **Binding.** Each bid's commitment is `c_i = POSEIDON2([TAG_BID, tag, bid_lo, bid_hi, salt0,
//! salt1])`, and the program folds them in order, `acc_i = POSEIDON2([TAG_FOLD, acc_{i-1}, c_i])`
//! from `acc_0 = 0`, publishing the fold's first five words in the receipt. Both messages are
//! fixed-length with a domain tag of their own, since the sponge does not pad. The auctioneer
//! publishes the list `c_1 … c_n`; each bidder checks their own commitment is in it (`commit/`
//! recomputes it), and anyone checks the list folds to the receipt's words. The proof then says:
//! these outputs are the clearing of exactly those bids.
//!
//! **Refused** (no proof at all): a mode other than 0 or 1, fewer than two or more than eight
//! bids, a zero tag, two bids under one tag, a bid of 2^63 or more.
//!
//! Nothing here may panic: a panic halts, and a halted run is a provable run. So there is no
//! indexing that can fail, no division, and every count is capped before a loop uses it.
#![cfg_attr(target_arch = "riscv32", no_std)]
#![forbid(unsafe_code)]

/// The most bids one call clears. Each costs two Poseidon2 messages, and the tier pays for them.
pub const MAX_BIDS: usize = 8;

/// The public input: how the bids compare.
pub const MODE_HIGHEST: u32 = 0;
pub const MODE_LOWEST: u32 = 1;

/// Domain tags: `"bid "` and `"fold"`, little-endian.
pub const TAG_BID: u32 = 0x2064_6962;
pub const TAG_FOLD: u32 = 0x646c_6f66;

/// Private input word 2 is the count; the bids follow, five words each.
pub const COUNT_AT: u32 = 2;
pub const FIRST_BID: u32 = 3;
pub const BID_WORDS: u32 = 5;

/// Where the words come from. The guest answers with syscalls, the tests with vectors.
pub trait Source {
    fn public(&self, i: u32) -> u32;
    fn input(&self, i: u32) -> u32;
    /// `POSEIDON2` over exactly six words.
    fn hash6(&self, msg: [u32; 6]) -> [u32; 8];
    /// `POSEIDON2` over exactly seventeen words.
    fn hash17(&self, msg: [u32; 17]) -> [u32; 8];
}

/// One bid: who (a nonzero tag the auctioneer assigned), how much, and the bidder's own salt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bid {
    pub tag: u32,
    pub amount: u64,
    pub salt: [u32; 2],
}

impl Bid {
    pub const ZERO: Bid = Bid { tag: 0, amount: 0, salt: [0, 0] };

    /// The bid's five private input words.
    pub fn words(&self) -> [u32; 5] {
        [self.tag, self.amount as u32, (self.amount >> 32) as u32, self.salt[0], self.salt[1]]
    }
}

/// Why a call was refused. The guest never says — a refused call has no proof — so these are
/// for the tests and the `auction` tool.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    Mode,
    Count,
    Tag,
    Duplicate,
    Range,
}
use Refusal::*;

/// The five private input words at `at`, as a bid.
#[inline(always)]
pub fn read_bid<S: Source>(s: &S, at: u32) -> Bid {
    Bid {
        tag: s.input(at),
        amount: u64::from(s.input(at.wrapping_add(1))) | (u64::from(s.input(at.wrapping_add(2))) << 32),
        salt: [s.input(at.wrapping_add(3)), s.input(at.wrapping_add(4))],
    }
}

/// The eight private input words at `at`.
#[inline(always)]
pub fn read_words8<S: Source>(s: &S, at: u32) -> [u32; 8] {
    let mut w = [0u32; 8];
    for (j, x) in w.iter_mut().enumerate() {
        *x = s.input(at.wrapping_add(j as u32));
    }
    w
}

/// `c = POSEIDON2([TAG_BID, tag, bid_lo, bid_hi, salt0, salt1])`: what the auctioneer publishes
/// for each bid, and what a bidder recomputes to find their own line in the list.
#[inline(always)]
pub fn commitment<S: Source>(s: &S, b: &Bid) -> [u32; 8] {
    let w = b.words();
    s.hash6([TAG_BID, w[0], w[1], w[2], w[3], w[4]])
}

/// One step of the fold: `POSEIDON2([TAG_FOLD, acc, c])`, the whole of both digests.
#[inline(always)]
pub fn fold<S: Source>(s: &S, acc: &[u32; 8], c: &[u32; 8]) -> [u32; 8] {
    s.hash17([
        TAG_FOLD, acc[0], acc[1], acc[2], acc[3], acc[4], acc[5], acc[6], acc[7], c[0], c[1], c[2], c[3], c[4], c[5],
        c[6], c[7],
    ])
}

/// The fold of a list of commitments, in order, from eight zero words.
pub fn fold_all<S: Source>(s: &S, cs: &[[u32; 8]]) -> [u32; 8] {
    let mut acc = [0u32; 8];
    for c in cs {
        acc = fold(s, &acc, c);
    }
    acc
}

/// Clear the bids: `(winner's tag, price)`. Pure, so the host tool predicts the receipt with it.
pub fn clear(mode: u32, bids: &[Bid]) -> Result<(u32, u64), Refusal> {
    let lowest = match mode {
        MODE_HIGHEST => false,
        MODE_LOWEST => true,
        _ => return Err(Mode),
    };
    if (bids.len() < 2) | (bids.len() > MAX_BIDS) {
        return Err(Count);
    }
    for (i, b) in bids.iter().enumerate() {
        if b.tag == 0 {
            return Err(Tag);
        }
        if (b.amount >> 63) != 0 {
            return Err(Range);
        }
        if bids.iter().skip(i + 1).any(|o| o.tag == b.tag) {
            return Err(Duplicate);
        }
    }
    let better = |a: u64, b: u64| if lowest { a < b } else { a > b };
    // The winner: the first bid that no later bid strictly beats.
    let first = bids.first().ok_or(Count)?;
    let (mut winner, mut best) = (first.tag, first.amount);
    for b in bids.iter().skip(1) {
        if better(b.amount, best) {
            best = b.amount;
            winner = b.tag;
        }
    }
    // The price: the best bid among the others (tags are distinct, so "others" is by tag).
    let mut price: Option<u64> = None;
    for b in bids.iter() {
        if b.tag != winner {
            price = Some(match price {
                Some(p) if !better(b.amount, p) => p,
                _ => b.amount,
            });
        }
    }
    Ok((winner, price.ok_or(Count)?))
}

/// Accept or refuse a call. On acceptance, the receipt's eight output words:
/// `[winner tag, price lo, price hi, fold0, fold1, fold2, fold3, fold4]`.
pub fn check<S: Source>(s: &S) -> Result<[u32; 8], Refusal> {
    // The two blind words: read so they are part of what is proved over, never used, never output.
    let _blind = (s.input(0), s.input(1));
    let mode = s.public(0);
    let n = s.input(COUNT_AT);
    if (n < 2) | (n > MAX_BIDS as u32) {
        return Err(Count);
    }
    let mut all = [Bid::ZERO; MAX_BIDS];
    for (i, b) in all.iter_mut().enumerate().take(n as usize) {
        *b = read_bid(s, FIRST_BID.wrapping_add(BID_WORDS.wrapping_mul(i as u32)));
    }
    let bids = all.get(..n as usize).ok_or(Count)?;
    let (winner, price) = clear(mode, bids)?;
    let mut acc = [0u32; 8];
    for b in bids {
        acc = fold(s, &acc, &commitment(s, b));
    }
    Ok([winner, price as u32, (price >> 32) as u32, acc[0], acc[1], acc[2], acc[3], acc[4]])
}

#[cfg(not(target_arch = "riscv32"))]
pub mod host;
