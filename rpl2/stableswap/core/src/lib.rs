//! stableswap: one Curve-style StableSwap pool (n = 2) of RAND and one RAND-pegged token, with
//! liquidity shares the program alone mints.
//!
//! The pool is one cell. Its reserves sit in the program's vault; its shares are an RPL token
//! registered with `rand token create --program <id>`. The program's **deploy-time public input**
//! is two words: the traded token's asset index and the amplification `A` (1 ≤ A ≤ 10 000), so
//! each pair and each `A` is its own program.
//!
//! ```text
//! key    [1, 0, 0, 0, 0, 0, 0, 0]
//! value  [x_lo, x_hi, y_lo, y_hi, s_lo, s_hi, lp, 1]     RAND reserve, token reserve, shares, share token, version
//! ```
//!
//! Reserves are below 2^62, shares below 2^63.
//!
//! **The invariant.** For two coins Curve's StableSwap invariant `D` solves
//! `4A(x + y) + D = 4A·D + D³ / (4xy)`; times `4xy`, with everything on one side,
//!
//! ```text
//! f(x, y, D) = 4xy · (4A(x + y) + D) − 16A · D · xy − D³
//! ```
//!
//! `f` falls strictly as `D` grows (`∂f/∂D = 4xy(1 − 4A) − 3D² < 0` for `A ≥ 1`, `D > 0`) and
//! `f(x, y, 0) ≥ 0`, so the pool's `D` is **the largest integer with `G(x, y, D)`**, where
//! `G` is `f ≥ 0` — the floor of the real root. The program never computes it: the caller
//! declares `D` and the program checks it is exact, `G(D) ∧ ¬G(D + 1)`.
//!
//! | method | private inputs | transition | rule |
//! |---|---|---|---|
//! | 1 add, first | `[1, 0, 0, D1 lo, hi]` | pool absent → pool; RAND and token in; mint shares | `D1` exact for the reserves; supply = `D1`, minted = `D1 − 1000` (locked for ever) |
//! | 1 add | `[1, D0 lo, hi, D1 lo, hi]` | pool → pool; RAND and/or token in; mint shares | `D0`, `D1` exact before and after; `m · (D0 + 1) · 10000 ≤ S · (D1 − D0 − 1) · 9996` |
//! | 2 remove | `[2]` | pool → pool; shares burned; pay RAND, pay token | `out · S ≤ burned · reserve` on both sides |
//! | 3 swap | `[3, D lo, hi, fee lo, hi]` | pool → pool; RAND or token in; pay the other | `D` exact before; `fee · 10000 ≥ in · 4`, `fee ≤ in`; `G(r_in + in − fee, r_out − out, D)` |
//!
//! In every method the reserves and supply written are exactly those read moved by what came in
//! and went out, so a caller can only ever choose amounts (and declare `D`, which has one right
//! value) — and every amount is checked against an inequality that never lets the pool lose.
//! The program never divides: the wallet finds `D` and the best amounts by searching the same
//! inequalities (`plan`).
#![cfg_attr(target_arch = "riscv32", no_std)]
#![forbid(unsafe_code)]

pub use rpl2_kit as kit;
use kit::{add_note, lt_note, prod, prod3, sub_note, words_of, Header, Source, U256, INFLOW_DEPOSIT, RAND};

pub const ADD: u32 = 1;
pub const REMOVE: u32 = 2;
pub const SWAP: u32 = 3;

/// The pool's cell.
pub const POOL_KEY: [u32; 8] = [1, 0, 0, 0, 0, 0, 0, 0];
/// The pool value's last word: a live pool is never all zeros.
pub const VERSION: u32 = 1;
/// Shares locked for ever by the first deposit (Uniswap v2's defence against share inflation).
pub const MIN_LIQUIDITY: u64 = 1_000;
/// The swap fee: at least 4 / 10 000 (0.04 %) of what comes in stays in the pool.
pub const FEE_NUM: u64 = 4;
pub const FEE_DEN: u64 = 10_000;
/// Every add keeps 0.04 % of the shares its `D` gain would buy: 9 996 / 10 000.
pub const ADD_KEEP: u64 = FEE_DEN - FEE_NUM;
/// The amplification's range.
pub const A_MAX: u64 = 10_000;

/// The public input: the traded token's asset index, and `A`.
pub const PUBLIC_TOKEN: u32 = 0;
pub const PUBLIC_AMP: u32 = 1;
pub const PUBLIC_WORDS: u32 = 2;

/// Why a transition was refused. The guest never says — a refused transition has no proof — so
/// these are for the tests and the `plan` tool.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    Version,
    Public,
    Method,
    Shape,
    Inflow,
    Key,
    Value,
    Asset,
    Zero,
    Range,
    Reserves,
    Invariant,
    Fee,
    Price,
    Pool,
}
use Refusal::*;

/// A pool, as its cell holds it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pool {
    /// RAND reserve.
    pub x: u64,
    /// Token reserve.
    pub y: u64,
    /// Share supply.
    pub s: u64,
    /// Share token.
    pub lp: u32,
}

impl Pool {
    pub fn value(&self) -> [u32; 8] {
        let (a, b, c) = (words_of(self.x), words_of(self.y), words_of(self.s));
        [a.0, a.1, b.0, b.1, c.0, c.1, self.lp, VERSION]
    }

    /// `None` for an absent cell (or anything that is not a live pool).
    pub fn from_value(v: &[u32; 8]) -> Option<Pool> {
        if v[7] != VERSION {
            return None;
        }
        Some(Pool { x: kit::u64_of(v[0], v[1]), y: kit::u64_of(v[2], v[3]), s: kit::u64_of(v[4], v[5]), lp: v[6] })
    }
}

/// A reserve: below 2^62, so that `x + y < 2^63` and `4x < 2^64`.
#[inline(always)]
pub fn lt_reserve(x: u64) -> bool {
    (x >> 62) == 0
}

/// `A` is in range.
#[inline(always)]
pub fn amp_ok(a: u64) -> bool {
    (a != 0) & (a <= A_MAX)
}

/// The invariant's terms for one pair of reserves, computed once and shared by `G(D)` and
/// `G(D + 1)`.
struct Curve {
    /// Both reserves below 2^62 and `A` in range: the bounds below hold.
    ok: bool,
    /// `x + y`, below 2^63.
    s: u64,
    /// `4xy`, below 2^126.
    p4: U256,
    /// `16A · xy`, below 2^126 · 2^16 = 2^142 (`4A ≤ 40 000 < 2^16`).
    p16a: U256,
}

impl Curve {
    #[inline(never)]
    fn new(a: u64, x: u64, y: u64) -> Curve {
        // x < 2^62, so 4x < 2^64: `prod(4x, y)` is exactly 4xy (and wraps harmlessly if not ok).
        let p4 = prod(x << 2, y);
        Curve { ok: lt_reserve(x) & lt_reserve(y) & amp_ok(a), s: x.wrapping_add(y), p4, p16a: p4.mul(a << 2) }
    }

    /// `G(x, y, D)`: `4xy · (4A(x + y) + D) ≥ 16A · D · xy + D³`.
    ///
    /// Computed as `D ≤ x + y ∧ 16A·xy·(x + y − D) + 4xy·D ≥ D³` — the same inequality with
    /// `16A·D·xy` moved across, which needs `D ≤ x + y` to keep the difference non-negative.
    /// Nothing is lost by asking it: `f(x, y, x + y) = −(x + y)(x − y)² ≤ 0` and `f` falls in `D`,
    /// so `G` never holds above `x + y`.
    ///
    /// Bounds (with `x, y < 2^62`, so `x + y < 2^63`, `D ≤ x + y`, `A ≤ 10 000 < 2^14`):
    /// `16A·xy·(x + y − D) < 2^142 · 2^63 = 2^205`, `4xy·D < 2^126 · 2^63 = 2^189`, their sum
    /// `< 2^206`; `D³ < 2^189`. Every product is far below 2^256, so nothing wraps. (In the
    /// spec's form both sides are below 2^206 as well: `4A(x + y) + D < 2^80`, `4xy < 2^126`.)
    #[inline(never)]
    fn holds(&self, d: u64) -> bool {
        let lhs = self.p16a.mul(self.s.wrapping_sub(d)).add(self.p4.mul(d));
        self.ok & (d <= self.s) & prod3(d, d, d).le(lhs)
    }

    /// `D` is the pool's invariant: the largest integer with `G`.
    #[inline(always)]
    fn exact(&self, d: u64) -> bool {
        self.holds(d) & !self.holds(d.wrapping_add(1))
    }
}

/// `G(x, y, D)` (see `Curve::holds`): `D` is at most the pool's invariant. False whenever a
/// reserve is not below 2^62 or `A` is out of range.
pub fn g(a: u64, x: u64, y: u64, d: u64) -> bool {
    Curve::new(a, x, y).holds(d)
}

/// `D` is exactly the invariant of reserves `x`, `y`: `G(D) ∧ ¬G(D + 1)`.
pub fn d_exact(a: u64, x: u64, y: u64, d: u64) -> bool {
    Curve::new(a, x, y).exact(d)
}

/// The fee covers 0.04 % of what comes in: `fee · 10000 ≥ in · 4` (products below 2^78).
pub fn fee_covers(amount_in: u64, fee: u64) -> bool {
    prod(amount_in, FEE_NUM).le(prod(fee, FEE_DEN))
}

/// A swap keeps the invariant: `G(r_in + in − fee, r_out − out, D)` — the pool, counting only
/// what comes in net of the fee, still has an invariant of at least `D`. (`G` is symmetric in
/// `x` and `y`, so which side is RAND does not matter.)
pub fn swap_ok(a: u64, d: u64, r_in: u64, r_out: u64, amount_in: u64, fee: u64, out: u64) -> bool {
    let x = r_in.wrapping_add(amount_in).wrapping_sub(fee);
    (out <= r_out) & (fee <= amount_in) & lt_note(r_in) & lt_note(amount_in) & g(a, x, r_out.wrapping_sub(out), d)
}

/// An add mints no more than its share of the invariant's growth, less 0.04 %, rounded against
/// the depositor: `minted · (D0 + 1) · 10000 ≤ S · (D1 − D0 − 1) · 9996`. (`D0` and `D1` are
/// floors of the true invariants, so `D0 + 1` bounds the old one from above and `D1 − D0 − 1`
/// the growth from below.) Products of three amounts below 2^63 and a constant below 2^14:
/// below 2^140.
pub fn add_ok(supply: u64, d0: u64, d1: u64, minted: u64) -> bool {
    let grew = d1.wrapping_sub(d0).wrapping_sub(1);
    lt_note(d1) & (d1 > d0.wrapping_add(1)) & prod3(minted, d0.wrapping_add(1), FEE_DEN).le(prod3(supply, grew, ADD_KEEP))
}

/// The pool a cell of the context holds, whatever its version word says.
#[inline(always)]
fn pool_at<S: Source>(s: &S, c: kit::Cell) -> Pool {
    Pool { x: c.val64(s, 0), y: c.val64(s, 2), s: c.val64(s, 4), lp: c.val(s, 6) }
}

/// Private input words `i`, `i + 1` as a u64.
#[inline(always)]
fn input64<S: Source>(s: &S, i: u32) -> u64 {
    kit::u64_of(s.input(i), s.input(i.wrapping_add(1)))
}

/// Accept or refuse the transition `s` shows. On acceptance, the receipt's eight output words:
/// `[method, p_lo, p_hi, q_lo, q_hi, D_lo, D_hi, 0]` — for add: RAND in, token in, `D1`; for
/// remove: RAND out, token out, 0; for swap: in, out, the declared `D`.
pub fn check<S: Source>(s: &S) -> Result<[u32; 8], Refusal> {
    let h = Header::read(s).ok_or(Version)?;
    let token = s.public(PUBLIC_TOKEN);
    let a = s.public(PUBLIC_AMP) as u64;
    if (token == RAND) | !amp_ok(a) {
        return Err(Public);
    }
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
    if !lt_reserve(after.x) | !lt_reserve(after.y) | !lt_note(after.s) {
        return Err(Range);
    }
    let (p, q, d) = match s.input(0) {
        ADD => add(s, &h, token, a, r, after)?,
        REMOVE => remove(s, &h, token, live(s, r)?, after)?,
        SWAP => swap(s, &h, token, a, live(s, r)?, after)?,
        _ => return Err(Method),
    };
    let (p, q, d) = (words_of(p), words_of(q), words_of(d));
    Ok([s.input(0), p.0, p.1, q.0, q.1, d.0, d.1, 0])
}

/// The read cell is a live pool.
#[inline(always)]
fn live<S: Source>(s: &S, r: kit::Cell) -> Result<Pool, Refusal> {
    if r.val(s, 7) != VERSION {
        return Err(Pool);
    }
    Ok(pool_at(s, r))
}

/// Add liquidity: RAND through `burn_r` and/or the token as a deposit, for newly minted shares.
/// The first deposit creates the pool (both sides: a one-sided pool has `D = 0`) and binds the
/// share token: it must be minted, which the chain allows only for a token whose mint authority
/// is this program.
fn add<S: Source>(s: &S, h: &Header, token: u32, a: u64, r: kit::Cell, after: Pool) -> Result<(u64, u64, u64), Refusal> {
    if (h.n_pays != 0) | (h.n_mints != 1) {
        return Err(Shape);
    }
    // The token comes in as a deposit of exactly this token, or not at all.
    let in_t = if h.inflow == INFLOW_DEPOSIT {
        if (h.burn_asset != token) | (h.burn_a == 0) {
            return Err(Inflow);
        }
        h.burn_a
    } else {
        if !h.no_token_in() {
            return Err(Inflow);
        }
        0
    };
    let in_r = h.burn_r;
    let (lp, minted) = h.mint(s, 0);
    if ((in_r == 0) & (in_t == 0)) | (minted == 0) {
        return Err(Zero);
    }
    if !lt_note(in_r) | !lt_note(in_t) | !lt_note(minted) {
        return Err(Range);
    }
    if after.lp != lp {
        return Err(Asset);
    }
    let (d0, d1) = (input64(s, 1), input64(s, 3));
    if r.is_zero(s) {
        // The first deposit.
        if (lp == RAND) | (lp == token) {
            return Err(Asset);
        }
        if (in_r == 0) | (in_t == 0) {
            return Err(Zero);
        }
        if (after.x != in_r) | (after.y != in_t) {
            return Err(Reserves);
        }
        if (d0 != 0) | !d_exact(a, in_r, in_t, d1) {
            return Err(Invariant);
        }
        // The supply is D1; the minted shares are the supply less the locked minimum.
        if (after.s != d1) | (d1 <= MIN_LIQUIDITY) | (minted != d1.wrapping_sub(MIN_LIQUIDITY)) {
            return Err(Price);
        }
    } else {
        let before = live(s, r)?;
        if before.lp != lp {
            return Err(Asset);
        }
        let x = add_note(before.x, in_r).ok_or(Range)?;
        let y = add_note(before.y, in_t).ok_or(Range)?;
        let supply = add_note(before.s, minted).ok_or(Range)?;
        if (after.x != x) | (after.y != y) | (after.s != supply) {
            return Err(Reserves);
        }
        if !d_exact(a, before.x, before.y, d0) | !d_exact(a, x, y, d1) {
            return Err(Invariant);
        }
        if !add_ok(before.s, d0, d1, minted) {
            return Err(Price);
        }
    }
    Ok((in_r, in_t, d1))
}

/// Remove liquidity: burn shares; RAND and the token out in proportion, RAND first.
fn remove<S: Source>(s: &S, h: &Header, token: u32, before: Pool, after: Pool) -> Result<(u64, u64, u64), Refusal> {
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
    let x = sub_note(before.x, out_r).ok_or(Reserves)?;
    let y = sub_note(before.y, out_t).ok_or(Reserves)?;
    if (after.s != supply) | (after.x != x) | (after.y != y) | (after.lp != before.lp) {
        return Err(Reserves);
    }
    // out · supply ≤ burned · reserve on both sides: no remaining share is worth less.
    if !prod(out_r, before.s).le(prod(burned, before.x)) | !prod(out_t, before.s).le(prod(burned, before.y)) {
        return Err(Price);
    }
    Ok((out_r, out_t, 0))
}

/// Swap RAND for the token or the token for RAND. The fee stays in the pool, so `D` grows.
fn swap<S: Source>(s: &S, h: &Header, token: u32, a: u64, before: Pool, after: Pool) -> Result<(u64, u64, u64), Refusal> {
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
        if rand_in { (before.x, before.y, after.x, after.y) } else { (before.y, before.x, after.y, after.x) };
    if (Some(a_in) != add_note(r_in, amount_in)) | (Some(a_out) != sub_note(r_out, amount_out)) {
        return Err(Reserves);
    }
    if (after.s != before.s) | (after.lp != before.lp) {
        return Err(Value);
    }
    let (d, fee) = (input64(s, 1), input64(s, 3));
    if !d_exact(a, before.x, before.y, d) {
        return Err(Invariant);
    }
    if !fee_covers(amount_in, fee) | (fee > amount_in) {
        return Err(Fee);
    }
    if !swap_ok(a, d, r_in, r_out, amount_in, fee, amount_out) {
        return Err(Price);
    }
    Ok((amount_in, amount_out, d))
}

#[cfg(not(target_arch = "riscv32"))]
pub mod plan;
