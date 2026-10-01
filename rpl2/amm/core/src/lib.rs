//! amm: one constant-product pool of RAND and one token, with liquidity shares the program
//! alone mints.
//!
//! The pool is one cell. Its reserves sit in the program's vault; its shares are an RPL token
//! registered with `rand token create --program <id>`. The token the pool trades is the program's
//! **deploy-time public input** (one word, its asset index), so each pair is its own program.
//!
//! ```text
//! key    [1, 0, 0, 0, 0, 0, 0, 0]
//! value  [rr_lo, rr_hi, rt_lo, rt_hi, s_lo, s_hi, lp, 1]     RAND reserve, token reserve, shares, share token, version
//! ```
//!
//! | method | private inputs | transition | rule |
//! |---|---|---|---|
//! | 1 add, first | `[1]` | pool absent → pool; RAND and token in; mint shares | `s² ≤ in_r · in_t`, minted = `s − 1000` (locked for ever) |
//! | 1 add | `[1]` | pool → pool; RAND and token in; mint shares | `minted · r ≤ in · s` on both sides |
//! | 2 remove | `[2]` | pool → pool; shares burned; pay RAND, pay token | `out · s ≤ burned · r` on both sides |
//! | 3 swap | `[3]` | pool → pool; RAND or token in; pay the other | `out · (1000 · r_in + 997 · in) ≤ 997 · in · r_out` |
//!
//! In every method the reserves and supply written are exactly those read moved by what came in
//! and went out, so a caller can only ever choose amounts — and every amount is checked against
//! an inequality that never lets the pool lose. The program never divides: the wallet finds the
//! best amount by searching the same inequality (`plan`).
#![cfg_attr(target_arch = "riscv32", no_std)]
#![forbid(unsafe_code)]

pub use rpl2_kit as kit;
use kit::{add_note, lt_note, prod, prod3, sub_note, words_of, Header, Source, RAND};

pub const ADD: u32 = 1;
pub const REMOVE: u32 = 2;
pub const SWAP: u32 = 3;

/// The pool's cell.
pub const POOL_KEY: [u32; 8] = [1, 0, 0, 0, 0, 0, 0, 0];
/// The pool value's last word: a live pool is never all zeros.
pub const VERSION: u32 = 1;
/// Shares locked for ever by the first deposit (Uniswap v2's defence against share inflation).
pub const MIN_LIQUIDITY: u64 = 1_000;
/// 0.30 % of what comes in stays in the pool.
pub const FEE_NUM: u64 = 997;
pub const FEE_DEN: u64 = 1_000;

/// The public input: the traded token's asset index.
pub const PUBLIC_TOKEN: u32 = 0;
pub const PUBLIC_WORDS: u32 = 1;

/// Why a transition was refused. The guest never says — a refused transition has no proof — so
/// these are for the tests and the `plan` tool.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    Version,
    Method,
    Shape,
    Inflow,
    Key,
    Value,
    Asset,
    Zero,
    Range,
    Reserves,
    Price,
    Pool,
}
use Refusal::*;

/// A pool, as its cell holds it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pool {
    pub rr: u64,
    pub rt: u64,
    pub s: u64,
    pub lp: u32,
}

impl Pool {
    pub fn value(&self) -> [u32; 8] {
        let (a, b, c) = (words_of(self.rr), words_of(self.rt), words_of(self.s));
        [a.0, a.1, b.0, b.1, c.0, c.1, self.lp, VERSION]
    }

    /// `None` for an absent cell (or anything that is not a live pool).
    pub fn from_value(v: &[u32; 8]) -> Option<Pool> {
        if v[7] != VERSION {
            return None;
        }
        Some(Pool { rr: kit::u64_of(v[0], v[1]), rt: kit::u64_of(v[2], v[3]), s: kit::u64_of(v[4], v[5]), lp: v[6] })
    }
}

/// The pool a cell of the context holds, whatever its version word says.
#[inline(always)]
fn pool_at<S: Source>(s: &S, c: kit::Cell) -> Pool {
    Pool { rr: c.val64(s, 0), rt: c.val64(s, 2), s: c.val64(s, 4), lp: c.val(s, 6) }
}

/// Accept or refuse the transition `s` shows. On acceptance, the receipt's eight output words:
/// `[method, in_lo, in_hi, out_lo, out_hi, 0, 0, 0]` (for add: RAND in, token in; for remove:
/// RAND out, token out).
pub fn check<S: Source>(s: &S) -> Result<[u32; 8], Refusal> {
    let h = Header::read(s).ok_or(Version)?;
    let token = s.public(PUBLIC_TOKEN);
    // Every method reads and writes the one pool cell.
    let (r, w) = (h.read_cell(0), h.write_cell(0));
    if (h.n_reads != 1) | (h.n_writes != 1) {
        return Err(Shape);
    }
    if !r.key_is(s, &POOL_KEY) || !w.key_is(s, &POOL_KEY) {
        return Err(Key);
    }
    if w.val(s, 7) != VERSION {
        return Err(Value);
    }
    let after = pool_at(s, w);
    if !lt_note(after.rr) | !lt_note(after.rt) | !lt_note(after.s) {
        return Err(Range);
    }
    let (x, y) = match s.input(0) {
        ADD => add(s, &h, token, r, after)?,
        REMOVE => remove(s, &h, token, live(s, r)?, after)?,
        SWAP => swap(s, &h, token, live(s, r)?, after)?,
        _ => return Err(Method),
    };
    let (x, y) = (words_of(x), words_of(y));
    Ok([s.input(0), x.0, x.1, y.0, y.1, 0, 0, 0])
}

/// The read cell is a live pool.
#[inline(always)]
fn live<S: Source>(s: &S, r: kit::Cell) -> Result<Pool, Refusal> {
    if r.val(s, 7) != VERSION {
        return Err(Pool);
    }
    Ok(pool_at(s, r))
}

/// Add liquidity: RAND through `burn_r` and the token as a deposit, for newly minted shares.
/// The first deposit creates the pool, and binds the share token: it must be minted, which the
/// chain allows only for a token whose mint authority is this program.
fn add<S: Source>(s: &S, h: &Header, token: u32, r: kit::Cell, after: Pool) -> Result<(u64, u64), Refusal> {
    if (h.n_pays != 0) | (h.n_mints != 1) {
        return Err(Shape);
    }
    if (h.inflow != kit::INFLOW_DEPOSIT) | (h.burn_asset != token) | (token == RAND) {
        return Err(Inflow);
    }
    let (in_r, in_t) = (h.burn_r, h.burn_a);
    let (lp, minted) = h.mint(s, 0);
    if (in_r == 0) | (in_t == 0) | (minted == 0) {
        return Err(Zero);
    }
    if !lt_note(in_r) | !lt_note(in_t) | !lt_note(minted) {
        return Err(Range);
    }
    if after.lp != lp {
        return Err(Asset);
    }
    if r.is_zero(s) {
        // The first deposit.
        if (lp == RAND) | (lp == token) {
            return Err(Asset);
        }
        if (after.rr != in_r) | (after.rt != in_t) {
            return Err(Reserves);
        }
        // supply² ≤ in_r · in_t; the minted shares are the supply less the locked minimum.
        if (after.s <= MIN_LIQUIDITY) | (minted != after.s - MIN_LIQUIDITY) {
            return Err(Price);
        }
        if !prod(after.s, after.s).le(prod(in_r, in_t)) {
            return Err(Price);
        }
    } else {
        let before = live(s, r)?;
        if before.lp != lp {
            return Err(Asset);
        }
        let rr = add_note(before.rr, in_r).ok_or(Range)?;
        let rt = add_note(before.rt, in_t).ok_or(Range)?;
        let supply = add_note(before.s, minted).ok_or(Range)?;
        if (after.rr != rr) | (after.rt != rt) | (after.s != supply) {
            return Err(Reserves);
        }
        // minted · reserve ≤ in · supply on both sides: no share is worth less than before.
        if !prod(minted, before.rr).le(prod(in_r, before.s)) | !prod(minted, before.rt).le(prod(in_t, before.s)) {
            return Err(Price);
        }
    }
    Ok((in_r, in_t))
}

/// Remove liquidity: burn shares; RAND and the token out in proportion, RAND first.
fn remove<S: Source>(s: &S, h: &Header, token: u32, before: Pool, after: Pool) -> Result<(u64, u64), Refusal> {
    if (h.n_pays != 2) | (h.n_mints != 0) {
        return Err(Shape);
    }
    if (h.burn_r != 0) | (h.inflow != kit::INFLOW_BURN) | (h.burn_asset != before.lp) {
        return Err(Inflow);
    }
    let burned = h.burn_a;
    let ((a0, out_r), (a1, out_t)) = (h.pay(s, 0), h.pay(s, 1));
    if (a0 != RAND) | (a1 != token) {
        return Err(Asset);
    }
    if (burned == 0) | (out_r == 0) | (out_t == 0) {
        return Err(Zero);
    }
    let supply = sub_note(before.s, burned).ok_or(Reserves)?;
    let rr = sub_note(before.rr, out_r).ok_or(Reserves)?;
    let rt = sub_note(before.rt, out_t).ok_or(Reserves)?;
    if (after.s != supply) | (after.rr != rr) | (after.rt != rt) | (after.lp != before.lp) {
        return Err(Reserves);
    }
    // out · supply ≤ burned · reserve on both sides: no remaining share is worth less.
    if !prod(out_r, before.s).le(prod(burned, before.rr)) | !prod(out_t, before.s).le(prod(burned, before.rt)) {
        return Err(Price);
    }
    Ok((out_r, out_t))
}

/// Swap RAND for the token or the token for RAND. The fee stays in the pool.
fn swap<S: Source>(s: &S, h: &Header, token: u32, before: Pool, after: Pool) -> Result<(u64, u64), Refusal> {
    if (h.n_pays != 1) | (h.n_mints != 0) {
        return Err(Shape);
    }
    let (in_asset, amount_in) = h.one_deposit().ok_or(Inflow)?;
    let (out_asset, amount_out) = h.pay(s, 0);
    let rand_in = in_asset == RAND;
    if (in_asset != RAND) & (in_asset != token) {
        return Err(Asset);
    }
    if out_asset != (if rand_in { token } else { RAND }) {
        return Err(Asset);
    }
    if amount_out == 0 {
        return Err(Zero);
    }
    if !lt_note(amount_in) | !lt_note(amount_out) {
        return Err(Range);
    }
    let (r_in, r_out, a_in, a_out) =
        if rand_in { (before.rr, before.rt, after.rr, after.rt) } else { (before.rt, before.rr, after.rt, after.rr) };
    if (Some(a_in) != add_note(r_in, amount_in)) | (Some(a_out) != sub_note(r_out, amount_out)) {
        return Err(Reserves);
    }
    if (after.s != before.s) | (after.lp != before.lp) {
        return Err(Value);
    }
    if !swap_ok(r_in, r_out, amount_in, amount_out) {
        return Err(Price);
    }
    Ok((amount_in, amount_out))
}

/// Uniswap v2's rule with the fee taken from what comes in:
/// `out · (1000 · r_in + 997 · in) ≤ 997 · in · r_out`. Every term is below 2^200.
pub fn swap_ok(r_in: u64, r_out: u64, amount_in: u64, amount_out: u64) -> bool {
    let denom = prod(FEE_DEN, r_in).add(prod(FEE_NUM, amount_in));
    denom.mul(amount_out).le(prod3(FEE_NUM, amount_in, r_out))
}

#[cfg(not(target_arch = "riscv32"))]
pub mod plan;
