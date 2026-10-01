//! The wallet's side (host only): build the transitions `check` accepts, choosing the best
//! amounts by searching the very inequalities `check` tests.

use crate::*;
use kit::host::{max_satisfying, min_satisfying, Hasher, Transition};

/// The private inputs for `method`, with a secret after it where the method needs one.
pub fn inputs(method: u32, secret: Option<&[u32; 8]>) -> Vec<u32> {
    let mut v = vec![method];
    if let Some(s) = secret {
        v.extend_from_slice(s);
    }
    v
}

/// operate's private inputs: the method, the operator's secret, the price and LP token the
/// transition writes, and the price it reads (zeros when creating the market).
pub fn operate_inputs(secret: &[u32; 8], t: &Transition) -> Vec<u32> {
    let w = t.writes.first().map(|w| w.1).unwrap_or([0; 8]);
    let r = t.reads.first().map(|r| r.1).unwrap_or([0; 8]);
    let mut i = inputs(OPERATE, Some(secret));
    i.extend_from_slice(&[w[0], w[1], w[6], r[0], r[1]]);
    i
}

/// The operator's lock — the deploy's public input — for a secret.
pub fn lock_of(secret: &[u32; 8], hash: Hasher) -> [u32; 8] {
    let mut m = [0u32; 9];
    m[0] = TAG_OPERATOR;
    m[1..].copy_from_slice(secret);
    hash(m)
}

/// The key of the position a secret owns.
pub fn key_of(secret: &[u32; 8], hash: Hasher) -> [u32; 8] {
    let mut m = [0u32; 9];
    m[0] = TAG_OWNER;
    m[1..].copy_from_slice(secret);
    position_key(&hash(m))
}

/// The three cells every trade touches, read and written: `(key, before, after)` each.
fn cells(t: Transition, rw: &[([u32; 8], [u32; 8], [u32; 8])]) -> Transition {
    rw.iter().fold(t, |t, (k, before, after)| t.read(*k, *before).write(*k, *after)).sorted()
}

/// Create the market (`market` absent) or move its price.
pub fn operate(market: Option<Market>, price: u64, lp: u32) -> Result<(Transition, Market), String> {
    if price == 0 || !lt_note(price) {
        return Err("the price must be positive and below 2^63".into());
    }
    let (before, after) = match market {
        None => {
            if lp == RAND {
                return Err("the LP token cannot be RAND (asset 0)".into());
            }
            ([0; 8], Market { price, cash: 0, supply: 0, lp })
        }
        Some(m) => (m.value(), Market { price, ..m }),
    };
    Ok((cells(Transition::new(), &[(MARKET_KEY, before, after.value())]), after))
}

/// LP tokens `a` RAND mints, and the market it leaves.
pub fn lp_add(m: Market, oi: Oi, a: u64) -> Result<(Transition, Market, u64), String> {
    // The first deposit mints one for one; any other, the most its share of the NAV allows.
    let minted = if m.supply == 0 { a } else { max_satisfying(u64::MAX >> 1, |x| add_ok(&m, &oi, a, x)) };
    if !add_ok(&m, &oi, a, minted) || !lt_note(a) {
        return Err("this deposit mints nothing (or the pool's net value is not positive)".into());
    }
    if minted == 0 {
        return Err("this deposit mints nothing (or the pool's net value is not positive)".into());
    }
    let after = Market { cash: m.cash + a, supply: m.supply + minted, ..m };
    let t = cells(Transition::new(), &[(MARKET_KEY, m.value(), after.value()), (OI_KEY, oi.value(), oi.value())])
        .rand_in(a)
        .mint(m.lp, minted);
    Ok((t, after, minted))
}

/// The RAND `burned` LP tokens are worth (the reserve aside).
pub fn lp_quote(m: &Market, oi: &Oi, burned: u64) -> u64 {
    match nav9(m, oi) {
        Some(nav) => max_satisfying(u64::MAX >> 1, |x| prod3(x, m.supply, E).le(nav.mul(burned))),
        None => 0,
    }
}

/// An LP withdrawal paying `x` for `burned`, whether or not the program would accept it.
pub fn lp_remove_tx(m: Market, oi: Oi, burned: u64, x: u64) -> (Transition, Market) {
    let after = Market { cash: m.cash.wrapping_sub(x), supply: m.supply.wrapping_sub(burned), ..m };
    let t = cells(Transition::new(), &[(MARKET_KEY, m.value(), after.value()), (OI_KEY, oi.value(), oi.value())])
        .burn(m.lp, burned)
        .pay(RAND, x);
    (t, after)
}

/// Burn `burned` LP for their full share, if the reserve allows it.
pub fn lp_remove(m: Market, oi: Oi, burned: u64) -> Result<(Transition, Market, u64), String> {
    if burned == 0 || burned > m.supply {
        return Err(format!("burn between 1 and {} LP units", m.supply));
    }
    let x = lp_quote(&m, &oi, burned);
    if x == 0 {
        return Err("these LP units are worth nothing now".into());
    }
    if !remove_ok(&m, &oi, burned, x) {
        let free = m.cash.saturating_sub(oi.long_cost + oi.short_cost);
        return Err(format!(
            "{burned} LP units are worth {x}, but only {free} of the pool's cash is free of the open positions' reserve: burn fewer, or wait"
        ));
    }
    let (t, after) = lp_remove_tx(m, oi, burned, x);
    Ok((t, after, x))
}

/// The best size for `cost` of notional at `price`, rounded against the trader: the most X a
/// long's cost buys, the least X a short's cost sells.
pub fn size_for(side: u32, cost: u64, price: u64) -> u64 {
    if side == LONG {
        max_satisfying(u64::MAX >> 1, |q| entry_ok(LONG, q, cost, price))
    } else {
        min_satisfying(1, |q| entry_ok(SHORT, q, cost, price)).unwrap_or(0)
    }
}

/// The transition opening `p` under `key`, whether or not the program would accept it.
pub fn open_tx(m: Market, oi: Oi, key: [u32; 8], p: Position) -> (Transition, Oi) {
    let after = oi.moved(&p, true).unwrap_or(oi);
    let t = cells(
        Transition::new(),
        &[(MARKET_KEY, m.value(), m.value()), (OI_KEY, oi.value(), after.value()), (key, [0; 8], p.value())],
    )
    .rand_in(p.margin);
    (t, after)
}

/// Open a position of `cost` notional on `margin` under `key`, at the best size.
pub fn open(m: Market, oi: Oi, key: [u32; 8], side: u32, margin: u64, cost: u64) -> Result<(Transition, Oi, Position), String> {
    if side != LONG && side != SHORT {
        return Err("side: long or short".into());
    }
    if margin == 0 || cost == 0 {
        return Err("margin and notional must be positive".into());
    }
    if !leverage_ok(margin, cost) {
        return Err(format!("{cost} of notional needs at least a tenth of it as margin (leverage at most {MAX_LEVERAGE}x)"));
    }
    let q = size_for(side, cost, m.price);
    if q == 0 {
        return Err("the notional buys no X at this price".into());
    }
    let p = Position { margin, q, cost, side };
    let (t, after) = open_tx(m, oi, key, p);
    if !reserve_ok(m.cash, &after) {
        return Err(format!("the pool's {} RAND units cannot back another {cost} of notional", m.cash));
    }
    Ok((t, after, p))
}

/// The most a close pays, and the most a liquidation rewards.
pub fn close_quote(p: &Position, price: u64) -> u64 {
    max_satisfying(u64::MAX >> 1, |x| payout_ok(p, price, x))
}

pub fn reward_quote(p: &Position, price: u64) -> u64 {
    max_satisfying(u64::MAX >> 1, |x| reward_ok(p, price, x))
}

/// Settle the position under `key` paying `x` (nothing if 0), whether or not it is allowed.
pub fn settle_tx(m: Market, oi: Oi, key: [u32; 8], p: Position, x: u64) -> (Transition, Market, Oi) {
    let after = Market { cash: (m.cash + p.margin).wrapping_sub(x), ..m };
    let oi_after = oi.moved(&p, false).unwrap_or(oi);
    let t = cells(
        Transition::new(),
        &[(MARKET_KEY, m.value(), after.value()), (OI_KEY, oi.value(), oi_after.value()), (key, p.value(), [0; 8])],
    );
    let t = if x == 0 { t } else { t.pay(RAND, x) };
    (t, after, oi_after)
}

/// Close at the best payout.
pub fn close(m: Market, oi: Oi, key: [u32; 8], p: Position) -> (Transition, Market, Oi, u64) {
    let x = close_quote(&p, m.price);
    let (t, after, oi_after) = settle_tx(m, oi, key, p, x);
    (t, after, oi_after, x)
}

/// Liquidate at the best reward, if the position is liquidatable.
pub fn liquidate(m: Market, oi: Oi, key: [u32; 8], p: Position) -> Result<(Transition, Market, Oi, u64), String> {
    if !liquidatable(&p, m.price) {
        return Err("the position is healthy: its value is at least 5 % of its notional".into());
    }
    let x = reward_quote(&p, m.price);
    let (t, after, oi_after) = settle_tx(m, oi, key, p, x);
    Ok((t, after, oi_after, x))
}
