//! The wallet's side (host only): build the transitions `check` accepts, choosing the best
//! amounts by searching the very inequalities `check` tests.

use crate::*;
use kit::host::{max_satisfying, Transition};

/// Shares a deposit of `in_r` RAND and `in_t` tokens mints, and the pool it leaves.
pub fn add(pool: Option<Pool>, token: u32, lp: u32, in_r: u64, in_t: u64) -> Result<(Transition, Pool, u64), String> {
    let (minted, after, before_value) = match pool {
        None => {
            // The largest supply with supply² ≤ in_r · in_t.
            let s = max_satisfying(u64::MAX >> 1, |s| prod(s, s).le(prod(in_r, in_t)));
            if s <= MIN_LIQUIDITY {
                return Err(format!("a first deposit must mint more than {MIN_LIQUIDITY} shares (√(in_r · in_t) = {s})"));
            }
            (s - MIN_LIQUIDITY, Pool { rr: in_r, rt: in_t, s, lp }, [0; 8])
        }
        Some(p) => {
            let m = max_satisfying(u64::MAX >> 1, |m| {
                prod(m, p.rr).le(prod(in_r, p.s)) & prod(m, p.rt).le(prod(in_t, p.s))
            });
            if m == 0 {
                return Err("the deposit is too small to mint a share".into());
            }
            (m, Pool { rr: p.rr + in_r, rt: p.rt + in_t, s: p.s + m, lp: p.lp }, p.value())
        }
    };
    let t = Transition::new()
        .read(POOL_KEY, before_value)
        .write(POOL_KEY, after.value())
        .rand_in(in_r)
        .deposit(token, in_t)
        .mint(after.lp, minted);
    Ok((t, after, minted))
}

/// What burning `shares` pays out, and the pool it leaves.
pub fn remove(p: Pool, token: u32, shares: u64) -> Result<(Transition, Pool, u64, u64), String> {
    if shares == 0 || shares > p.s {
        return Err(format!("burn between 1 and {} shares", p.s));
    }
    let out_r = max_satisfying(p.rr, |x| prod(x, p.s).le(prod(shares, p.rr)));
    let out_t = max_satisfying(p.rt, |x| prod(x, p.s).le(prod(shares, p.rt)));
    if out_r == 0 || out_t == 0 {
        return Err("too few shares: one side rounds to nothing".into());
    }
    let after = Pool { rr: p.rr - out_r, rt: p.rt - out_t, s: p.s - shares, lp: p.lp };
    let t = Transition::new()
        .read(POOL_KEY, p.value())
        .write(POOL_KEY, after.value())
        .burn(p.lp, shares)
        .pay(RAND, out_r)
        .pay(token, out_t);
    Ok((t, after, out_r, out_t))
}

/// The most a swap of `amount_in` pays out.
pub fn quote(p: &Pool, rand_in: bool, amount_in: u64) -> u64 {
    let (r_in, r_out) = if rand_in { (p.rr, p.rt) } else { (p.rt, p.rr) };
    max_satisfying(r_out, |x| swap_ok(r_in, r_out, amount_in, x))
}

/// Sell `amount_in` of RAND (`rand_in`) or of the token, for at least `min_out` of the other.
pub fn swap(p: Pool, token: u32, rand_in: bool, amount_in: u64, min_out: u64) -> Result<(Transition, Pool, u64), String> {
    let out = quote(&p, rand_in, amount_in);
    if out == 0 || out < min_out {
        return Err(format!("this swap pays {out}, under the minimum {min_out}"));
    }
    let after = if rand_in {
        Pool { rr: p.rr + amount_in, rt: p.rt - out, ..p }
    } else {
        Pool { rr: p.rr - out, rt: p.rt + amount_in, ..p }
    };
    let t = Transition::new().read(POOL_KEY, p.value()).write(POOL_KEY, after.value());
    let t = if rand_in { t.rand_in(amount_in).pay(token, out) } else { t.deposit(token, amount_in).pay(RAND, out) };
    Ok((t, after, out))
}
