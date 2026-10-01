//! The wallet's side (host only): build the transitions `check` accepts, choosing the best
//! amounts by searching the very inequalities `check` tests.

use crate::*;
use kit::host::{max_satisfying, min_satisfying, Hasher, Transition};

/// The operator's lock, `POSEIDON2([TAG_OPERATOR, secret])` under `hash`.
pub fn lock(hash: Hasher, secret: &[u32; 8]) -> [u32; 8] {
    hash(tagged(TAG_OPERATOR, secret))
}

/// The program's public input: the lock, then the collateral token.
pub fn public(lock: &[u32; 8], coll_asset: u32) -> Vec<u32> {
    let mut p = lock.to_vec();
    p.push(coll_asset);
    p
}

/// The key of the position a secret owns.
pub fn position_key(hash: Hasher, secret: &[u32; 8]) -> [u32; 8] {
    owned_key(POSITION_TAG, &hash(tagged(TAG_OWNER, secret)))
}

fn tagged(tag: u32, s: &[u32; 8]) -> [u32; 9] {
    [tag, s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7]]
}

/// The operator's private inputs: `[1, secret, p_lo, p_hi, x]` (x: the share token, or the rate).
pub fn operator_input(secret: &[u32; 8], price: u64, x: u32) -> Vec<u32> {
    let mut v = vec![OPERATE];
    v.extend_from_slice(secret);
    v.extend_from_slice(&[price as u32, (price >> 32) as u32, x]);
    v
}

/// An owner's private inputs: `[4, secret]`.
pub fn owner_input(secret: &[u32; 8]) -> Vec<u32> {
    let mut v = vec![ADJUST];
    v.extend_from_slice(secret);
    v
}

/// Set the market up: the price, an empty pool at index `E`, the share token.
pub fn init(price: u64, share: u32) -> (Transition, Pool) {
    let pool = Pool { cash: 0, sb: 0, s: 0, i: E };
    let t = Transition::new()
        .read(POOL_KEY, [0; 8])
        .write(PRICE_KEY, price_value(price))
        .write(POOL_KEY, pool.value())
        .write(SHARES_KEY, shares_value(share));
    (t, pool)
}

/// Set the price and accrue `rate` (parts per `E`, at most `MAX_RATE`).
pub fn update(pool: Pool, price: u64, rate: u64) -> Result<(Transition, Pool), String> {
    if rate > MAX_RATE {
        return Err(format!("the rate is at most {MAX_RATE} per {E} (1 %) per update"));
    }
    let i = max_satisfying(u64::MAX >> 1, |j| prod(j, E).le(prod(pool.i, E + rate)));
    let after = Pool { i, ..pool };
    let t = Transition::new().read(POOL_KEY, pool.value()).write(PRICE_KEY, price_value(price)).write(POOL_KEY, after.value());
    Ok((t, after))
}

/// Shares `a` RAND mints, and the pool it leaves.
pub fn supply(pool: Pool, share: u32, a: u64) -> Result<(Transition, Pool, u64), String> {
    let (m, after) = if pool.s == 0 {
        if a <= MIN_SHARES {
            return Err(format!("the first supply must be more than {MIN_SHARES} units ({MIN_SHARES} shares stay locked)"));
        }
        (a - MIN_SHARES, Pool { cash: pool.cash + a, s: a, ..pool })
    } else {
        let m = max_satisfying(u64::MAX >> 1, |m| supply_ok(&pool, a, m));
        if m == 0 {
            return Err("the supply is too small to mint a share".into());
        }
        (m, Pool { cash: pool.cash + a, s: pool.s + m, ..pool })
    };
    let t = Transition::new()
        .read(POOL_KEY, pool.value())
        .read(SHARES_KEY, shares_value(share))
        .write(POOL_KEY, after.value())
        .write(SHARES_KEY, shares_value(share))
        .rand_in(a)
        .mint(share, m);
    Ok((t, after, m))
}

/// What burning `b` shares pays, and the pool it leaves.
pub fn withdraw(pool: Pool, share: u32, b: u64) -> Result<(Transition, Pool, u64), String> {
    if b == 0 || b > pool.s {
        return Err(format!("burn between 1 and {} shares", pool.s));
    }
    let x = max_satisfying(pool.cash, |x| withdraw_ok(&pool, b, x));
    if x == 0 {
        return Err("those shares are worth nothing the pool holds in cash right now".into());
    }
    let after = Pool { cash: pool.cash - x, s: pool.s - b, ..pool };
    let t = Transition::new()
        .read(POOL_KEY, pool.value())
        .read(SHARES_KEY, shares_value(share))
        .write(POOL_KEY, after.value())
        .write(SHARES_KEY, shares_value(share))
        .burn(share, b)
        .pay(RAND, x);
    Ok((t, after, x))
}

/// What a borrower asks of their position. At most one of `withdraw` and `borrow` (one payout).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Change {
    pub deposit: u64,
    pub withdraw: u64,
    pub borrow: u64,
    pub repay: u64,
}

/// The smallest scaled debt that covers `pos`'s, plus `x` borrowed, less `r` repaid.
pub fn debt_after(sdebt: u64, i: u64, r: u64, x: u64) -> u64 {
    min_satisfying(0, |d| debt_covered(sdebt, d, i, r, x)).unwrap_or(u64::MAX >> 1)
}

/// The RAND that repays all of `sdebt` at index `i`.
pub fn payoff(sdebt: u64, i: u64) -> u64 {
    min_satisfying(0, |r| debt_covered(sdebt, 0, i, r, 0)).unwrap_or(u64::MAX >> 1)
}

/// The most a position may borrow, after depositing `deposit` of C.
pub fn max_borrow(p: u64, pool: &Pool, pos: Position, deposit: u64) -> u64 {
    let coll = pos.coll + deposit;
    max_satisfying(pool.cash, |x| healthy(coll, debt_after(pos.sdebt, pool.i, 0, x), p, pool.i))
}

/// The most collateral a position may take out.
pub fn max_withdraw(p: u64, pool: &Pool, pos: Position) -> u64 {
    max_satisfying(pos.coll, |c| healthy(pos.coll - c, pos.sdebt, p, pool.i))
}

/// The transition for `change` to the position under `key`. A repayment past the debt is cut to
/// the exact payoff.
pub fn adjust(
    p: u64,
    pool: Pool,
    coll_asset: u32,
    key: [u32; 8],
    pos: Position,
    change: Change,
) -> Result<(Transition, Pool, Position), String> {
    let Change { deposit, withdraw, borrow, repay } = change;
    if withdraw != 0 && borrow != 0 {
        return Err("one payout per invoke: borrow or take collateral out, not both".into());
    }
    let repay = repay.min(payoff(pos.sdebt, pool.i) + if borrow > 0 { borrow } else { 0 });
    if withdraw > pos.coll + deposit {
        return Err(format!("the position holds {} of collateral", pos.coll + deposit));
    }
    if borrow > pool.cash + repay {
        return Err(format!("the pool has {} RAND units to lend", pool.cash + repay));
    }
    let out = adjust_tx(p, pool, coll_asset, key, pos, Change { repay, ..change });
    let pos2 = out.2;
    if pos2 == pos {
        return Err("nothing changes".into());
    }
    let riskier = pos2.sdebt > pos.sdebt || pos2.coll < pos.coll;
    if riskier && !healthy(pos2.coll, pos2.sdebt, p, pool.i) {
        return Err("the position would pass the 75 % loan-to-value limit".into());
    }
    Ok(out)
}

/// [`adjust`]'s transition as asked, unchecked (the demo's and the tests' greedy variants).
pub fn adjust_tx(p: u64, pool: Pool, coll_asset: u32, key: [u32; 8], pos: Position, change: Change) -> (Transition, Pool, Position) {
    let Change { deposit, withdraw, borrow, repay } = change;
    let sdebt = debt_after(pos.sdebt, pool.i, repay, borrow);
    let pos2 = Position { coll: pos.coll + deposit - withdraw, sdebt };
    let after = Pool { cash: pool.cash + repay - borrow, sb: pool.sb + pos2.sdebt - pos.sdebt, ..pool };
    let mut t = Transition::new()
        .read(PRICE_KEY, price_value(p))
        .read(POOL_KEY, pool.value())
        .read(key, pos.value())
        .write(PRICE_KEY, price_value(p))
        .write(POOL_KEY, after.value())
        .write(key, pos2.value())
        .rand_in(repay);
    if deposit > 0 {
        t = t.deposit(coll_asset, deposit);
    }
    if borrow > 0 {
        t = t.pay(RAND, borrow);
    }
    if withdraw > 0 {
        t = t.pay(coll_asset, withdraw);
    }
    (t, after, pos2)
}

/// Repay `repay` RAND of the position under `key` and seize the most collateral that buys,
/// whether or not the position is liquidatable (the program decides that).
pub fn liquidation(p: u64, pool: Pool, coll_asset: u32, key: [u32; 8], pos: Position, repay: u64) -> (Transition, Pool, Position, u64) {
    let repay = repay.min(payoff(pos.sdebt, pool.i));
    let sdebt = debt_after(pos.sdebt, pool.i, repay, 0);
    let cleared = pos.sdebt - sdebt;
    let c = max_satisfying(pos.coll, |c| seize_ok(c, p, cleared, pool.i));
    let pos2 = Position { coll: pos.coll - c, sdebt };
    let after = Pool { cash: pool.cash + repay, sb: pool.sb - cleared, ..pool };
    let t = Transition::new()
        .read(PRICE_KEY, price_value(p))
        .read(POOL_KEY, pool.value())
        .read(key, pos.value())
        .write(PRICE_KEY, price_value(p))
        .write(POOL_KEY, after.value())
        .write(key, pos2.value())
        .rand_in(repay)
        .pay(coll_asset, c);
    (t, after, pos2, c)
}

/// [`liquidation`], for a position that is past the threshold.
pub fn liquidate(p: u64, pool: Pool, coll_asset: u32, key: [u32; 8], pos: Position, repay: u64) -> Result<(Transition, Pool, Position, u64), String> {
    if !liquidatable(pos.coll, pos.sdebt, p, pool.i) {
        return Err("the position is within the 85 % threshold: it cannot be liquidated".into());
    }
    let out = liquidation(p, pool, coll_asset, key, pos, repay);
    if out.3 == 0 {
        return Err("that repayment buys no collateral".into());
    }
    Ok(out)
}

/// A position's debt in RAND units, rounded up, and its loan-to-value in basis points.
pub fn debt_and_ltv(p: u64, i: u64, pos: Position) -> (u64, u64) {
    let debt = payoff(pos.sdebt, i);
    let value = (pos.coll as u128 * p as u128 / E as u128) as u64;
    let ltv = if value == 0 { 0 } else { (debt as u128 * 10_000 / value as u128) as u64 };
    (debt, ltv)
}
