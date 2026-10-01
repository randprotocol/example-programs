//! The wallet's side (host only): build the transitions `check` accepts — finding `D`, the fee
//! and the best amounts by searching the very inequalities `check` tests.

use crate::*;
use kit::host::{max_satisfying, min_satisfying, Transition};

/// A planned invoke: the transition, its private inputs, the pool it leaves, and its amount
/// (shares minted, or what a swap pays).
pub type Planned = (Transition, Vec<u32>, Pool, u64);

/// The pool's invariant: the largest `D` with `G(x, y, D)` (bisection; `G` holds at 0 and never
/// above `x + y`).
pub fn d_of(a: u64, x: u64, y: u64) -> u64 {
    max_satisfying(x.saturating_add(y), |d| g(a, x, y, d))
}

/// The least fee a swap of `amount_in` may declare: `⌈in · 4 / 10000⌉`, found as the smallest
/// fee `fee_covers` accepts.
pub fn fee_of(amount_in: u64) -> u64 {
    min_satisfying(0, |f| fee_covers(amount_in, f)).unwrap_or(u64::MAX)
}

fn inputs(method: u32, p: u64, q: u64) -> Vec<u32> {
    let (p, q) = (words_of(p), words_of(q));
    vec![method, p.0, p.1, q.0, q.1]
}

fn reserves_fit(x: u64, y: u64) -> Result<(), String> {
    if lt_reserve(x) && lt_reserve(y) { Ok(()) } else { Err("reserves must stay below 2^62".into()) }
}

/// The most shares an add may mint for a given `D` before and after (0 if none).
pub fn add_mint(supply: u64, d0: u64, d1: u64) -> u64 {
    max_satisfying(u64::MAX >> 1, |m| add_ok(supply, d0, d1, m))
}

/// Shares a deposit of `in_r` RAND and `in_t` tokens (either may be 0 once the pool exists)
/// mints, and the pool it leaves.
pub fn add(pool: Option<Pool>, token: u32, a: u64, lp: u32, in_r: u64, in_t: u64) -> Result<Planned, String> {
    if in_r == 0 && in_t == 0 {
        return Err("add something".into());
    }
    let (minted, after, before_value, d0, d1) = match pool {
        None => {
            if in_r == 0 || in_t == 0 {
                return Err("the first deposit must bring both RAND and the token (a one-sided pool has D = 0)".into());
            }
            reserves_fit(in_r, in_t)?;
            let d1 = d_of(a, in_r, in_t);
            if d1 <= MIN_LIQUIDITY {
                return Err(format!("a first deposit must have D above {MIN_LIQUIDITY} (D = {d1})"));
            }
            (d1 - MIN_LIQUIDITY, Pool { x: in_r, y: in_t, s: d1, lp }, [0; 8], 0, d1)
        }
        Some(p) => {
            let (x, y) = (p.x.saturating_add(in_r), p.y.saturating_add(in_t));
            reserves_fit(x, y)?;
            let (d0, d1) = (d_of(a, p.x, p.y), d_of(a, x, y));
            let m = add_mint(p.s, d0, d1);
            if m == 0 {
                return Err("the deposit is too small to mint a share".into());
            }
            (m, Pool { x, y, s: p.s + m, lp: p.lp }, p.value(), d0, d1)
        }
    };
    let t = Transition::new().read(POOL_KEY, before_value).write(POOL_KEY, after.value()).rand_in(in_r);
    let t = if in_t > 0 { t.deposit(token, in_t) } else { t };
    let t = t.mint(after.lp, minted);
    Ok((t, inputs(ADD, d0, d1), after, minted))
}

/// What burning `shares` pays out, and the pool it leaves.
pub fn remove(p: Pool, token: u32, shares: u64) -> Result<(Transition, Vec<u32>, Pool, u64, u64), String> {
    if shares == 0 || shares > p.s {
        return Err(format!("burn between 1 and {} shares", p.s));
    }
    let out_r = max_satisfying(p.x, |o| prod(o, p.s).le(prod(shares, p.x)));
    let out_t = max_satisfying(p.y, |o| prod(o, p.s).le(prod(shares, p.y)));
    if out_r == 0 || out_t == 0 {
        return Err("too few shares: one side rounds to nothing".into());
    }
    let after = Pool { x: p.x - out_r, y: p.y - out_t, s: p.s - shares, lp: p.lp };
    let t = Transition::new()
        .read(POOL_KEY, p.value())
        .write(POOL_KEY, after.value())
        .burn(p.lp, shares)
        .pay(RAND, out_r)
        .pay(token, out_t);
    Ok((t, vec![REMOVE], after, out_r, out_t))
}

/// The most a swap of `amount_in` pays out, declaring `d` and `fee`.
pub fn quote_with(p: &Pool, a: u64, rand_in: bool, amount_in: u64, d: u64, fee: u64) -> u64 {
    let (r_in, r_out) = if rand_in { (p.x, p.y) } else { (p.y, p.x) };
    max_satisfying(r_out, |o| swap_ok(a, d, r_in, r_out, amount_in, fee, o))
}

/// The most a swap of `amount_in` pays out, honestly: the pool's `D`, the least fee.
pub fn quote(p: &Pool, a: u64, rand_in: bool, amount_in: u64) -> u64 {
    quote_with(p, a, rand_in, amount_in, d_of(a, p.x, p.y), fee_of(amount_in))
}

/// What a constant-product pool of the same reserves and the same fee would pay:
/// the largest `out` with `out · (r_in + in − fee) ≤ (in − fee) · r_out`. For comparison only.
pub fn quote_constant_product(p: &Pool, rand_in: bool, amount_in: u64) -> u64 {
    let (r_in, r_out) = if rand_in { (p.x, p.y) } else { (p.y, p.x) };
    let net = amount_in - fee_of(amount_in).min(amount_in);
    max_satisfying(r_out, |o| prod(o, r_in + net).le(prod(net, r_out)))
}

/// A swap transition paying `out`, declaring `d` and `fee` (whatever they are: the demo and the
/// tests tamper with them).
pub fn swap_tx(p: Pool, token: u32, rand_in: bool, amount_in: u64, out: u64, d: u64, fee: u64) -> (Transition, Vec<u32>, Pool) {
    let after = if rand_in {
        Pool { x: p.x + amount_in, y: p.y - out, ..p }
    } else {
        Pool { x: p.x - out, y: p.y + amount_in, ..p }
    };
    let t = Transition::new().read(POOL_KEY, p.value()).write(POOL_KEY, after.value());
    let t = if rand_in { t.rand_in(amount_in).pay(token, out) } else { t.deposit(token, amount_in).pay(RAND, out) };
    (t, inputs(SWAP, d, fee), after)
}

/// Sell `amount_in` of RAND (`rand_in`) or of the token, for at least `min_out` of the other.
pub fn swap(p: Pool, token: u32, a: u64, rand_in: bool, amount_in: u64, min_out: u64) -> Result<Planned, String> {
    if amount_in == 0 {
        return Err("sell something".into());
    }
    let (d, fee) = (d_of(a, p.x, p.y), fee_of(amount_in));
    let out = quote_with(&p, a, rand_in, amount_in, d, fee);
    if out == 0 || out < min_out {
        return Err(format!("this swap pays {out}, under the minimum {min_out}"));
    }
    let (t, i, after) = swap_tx(p, token, rand_in, amount_in, out, d, fee);
    Ok((t, i, after, out))
}
