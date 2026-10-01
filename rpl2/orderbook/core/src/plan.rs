//! The wallet's side (host only): build the transitions `check` accepts, choosing the best
//! amounts by searching the very inequality `check` tests (`price_ok`).

use crate::*;
use kit::host::{max_satisfying, min_satisfying, Hasher, Mock, Transition};
use kit::RAND;

/// The key of the order `ticket` names, under `hash` (`real_hash` for the chain, `test_hash` in
/// the tests).
pub fn key_of(ticket: &[u32; 8], hash: Hasher) -> [u32; 8] {
    order_key(&Mock::new(&[], &Transition::new(), &[], hash), ticket)
}

/// The private inputs of post and close, the methods that prove the ticket:
/// `[method, ticket, give asset, want asset, want_rem lo, hi]` — the order's terms as the maker states them
/// (for post, the order written; for close, the order read).
pub fn inputs(method: u32, ticket: &[u32; 8], o: &Order) -> Vec<u32> {
    let mut v = vec![method];
    v.extend_from_slice(ticket);
    v.extend_from_slice(&[o.give, o.want, o.want_rem as u32, (o.want_rem >> 32) as u32]);
    v
}

/// `asset` coming in: RAND through `burn_r`, a token as a deposit.
fn pay_in(t: Transition, asset: u32, amount: u64) -> Transition {
    if asset == RAND {
        t.rand_in(amount)
    } else {
        t.deposit(asset, amount)
    }
}

/// Post an order at `key` (empty, as `existing` shows): `give` of `give_asset` into escrow for
/// `want` of `want_asset` in all.
pub fn post(key: [u32; 8], existing: [u32; 8], give_asset: u32, give: u64, want_asset: u32, want: u64) -> Result<(Transition, Order), String> {
    if existing != [0; 8] {
        return Err("an order already sits at this key".into());
    }
    if give_asset == want_asset {
        return Err("give and want must be different assets".into());
    }
    if give == 0 || want == 0 || !lt_note(give) || !lt_note(want) {
        return Err("give and want must each be between 1 and 2^63 − 1 units".into());
    }
    let o = Order { give: give_asset, want: want_asset, give_rem: give, want_rem: want, proceeds: 0 };
    let t = pay_in(Transition::new().read(key, [0; 8]).write(key, o.value()), give_asset, give);
    Ok((t, o))
}

/// The most of the give asset `y` of the want asset buys from `o`: the largest `x ≤ give_rem`
/// with `price_ok`.
pub fn most_for(o: &Order, y: u64) -> u64 {
    max_satisfying(o.give_rem, |x| price_ok(o.give_rem, o.want_rem, x, y))
}

/// The least of the want asset that buys `x` of the give asset from `o`: the smallest `y` with
/// `price_ok` (it may exceed `want_rem` only if `x` exceeds `give_rem`, which `fill` refuses).
pub fn least_for(o: &Order, x: u64) -> Option<u64> {
    min_satisfying(1, |y| price_ok(o.give_rem, o.want_rem, x, y))
}

/// A fill paying `y` of the want asset in for `x` of the give asset out; the order it leaves.
pub fn fill_exact(key: [u32; 8], o: Order, x: u64, y: u64) -> Transition {
    let after = Order {
        give_rem: o.give_rem.wrapping_sub(x),
        want_rem: o.want_rem.wrapping_sub(y),
        proceeds: o.proceeds.wrapping_add(y),
        ..o
    };
    pay_in(Transition::new().read(key, o.value()).write(key, after.value()), o.want, y).pay(o.give, x)
}

/// Fill: pay `y` of the want asset (`Pay`) or take `x` of the give asset (`Take`), at the best
/// amount for the taker.
pub enum Fill {
    Pay(u64),
    Take(u64),
}

pub fn fill(key: [u32; 8], o: Order, f: Fill) -> Result<(Transition, Order, u64, u64), String> {
    let (x, y) = match f {
        Fill::Pay(y) => {
            if y == 0 || y > o.want_rem {
                return Err(format!("pay between 1 and {} (what the order still asks)", o.want_rem));
            }
            (most_for(&o, y), y)
        }
        Fill::Take(x) => {
            if x == 0 || x > o.give_rem {
                return Err(format!("take between 1 and {} (what the order still holds)", o.give_rem));
            }
            (x, least_for(&o, x).ok_or("no amount buys that")?)
        }
    };
    if x == 0 {
        return Err(format!("{y} buys less than one unit at this order's price"));
    }
    let t = fill_exact(key, o, x, y);
    let after = Order { give_rem: o.give_rem - x, want_rem: o.want_rem - y, proceeds: o.proceeds + y, ..o };
    Ok((t, after, x, y))
}

/// Close: delete the order, refunding what is left of the escrow and paying out the proceeds.
pub fn close(key: [u32; 8], o: Order) -> Transition {
    let mut t = Transition::new().read(key, o.value()).write(key, [0; 8]);
    if o.give_rem != 0 {
        t = t.pay(o.give, o.give_rem);
    }
    if o.proceeds != 0 {
        t = t.pay(o.want, o.proceeds);
    }
    t
}
