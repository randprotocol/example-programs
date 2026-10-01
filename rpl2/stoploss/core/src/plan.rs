//! The wallet's side (host only): build the transitions `check` accepts, and refuse to send a
//! fire whose condition does not hold — by asking the very predicate `check` asks.

use crate::*;
use kit::host::{Hasher, Transition};

/// The order key a ticket names.
pub fn key_of(ticket: &[u32; 8], hash: Hasher) -> [u32; 8] {
    let t = ticket;
    order_key(&hash([TAG_TICKET, t[0], t[1], t[2], t[3], t[4], t[5], t[6], t[7]]))
}

/// The private inputs of place, fire and cancel: the method, the ticket, the opening.
pub fn inputs(method: u32, ticket: &[u32; 8], o: &Opening) -> Vec<u32> {
    let t = words_of(o.threshold);
    let mut v = vec![method];
    v.extend_from_slice(ticket);
    v.extend_from_slice(&[o.kind, t.0, t.1]);
    v.extend_from_slice(&o.salt);
    debug_assert_eq!(v.len(), OWNER_INPUTS);
    v
}

/// The private inputs of operate: the method, the operator's secret, the price replaced (0 for
/// the first reading) and the new price.
pub fn operate_inputs(secret: &[u32; 8], old: u64, new: u64) -> Vec<u32> {
    let (o, n) = (words_of(old), words_of(new));
    let mut v = vec![OPERATE];
    v.extend_from_slice(secret);
    v.extend_from_slice(&[o.0, o.1, n.0, n.1]);
    v
}

/// The operator sets `price`, replacing `oracle` (none: the first reading). The inputs are
/// [`operate_inputs`] with the price replaced (`oracle`'s, or 0).
pub fn operate(oracle: Option<Oracle>, price: u64) -> Result<Transition, String> {
    if price == 0 || !lt_note(price) {
        return Err("the price must be in 1..2^63".into());
    }
    let before = oracle.map_or([0; 8], |o| o.value());
    Ok(Transition::new().read(ORACLE_KEY, before).write(ORACLE_KEY, Oracle { price }.value()))
}

/// What [`operate_inputs`] wants for `oracle`: its price, or 0 when there is none yet.
pub fn old_price(oracle: Option<Oracle>) -> u64 {
    oracle.map_or(0, |o| o.price)
}

/// Escrow `amount` RAND under the key `key` (which must be empty: `cell` is what it holds now),
/// committed to the opening `o`.
pub fn place(key: [u32; 8], cell: [u32; 8], amount: u64, o: &Opening, hash: Hasher) -> Result<(Transition, Order), String> {
    if cell != [0; 8] {
        return Err("that ticket already names an order: use a fresh ticket".into());
    }
    if amount == 0 || !lt_note(amount) {
        return Err("escrow between 1 and 2^63 − 1 units".into());
    }
    if o.kind != STOP && o.kind != TAKE_PROFIT {
        return Err(format!("kind {} is neither a stop (1) nor a take-profit (2)", o.kind));
    }
    let order = Order { amount, c: commitment(o, hash) };
    let t = Transition::new().read(key, [0; 8]).write(key, order.value()).rand_in(amount);
    Ok((t, order))
}

/// Fire the order under `key`, if the opening is its and the oracle's price meets it. The
/// payout goes to `to` (`None`: the invoking wallet).
pub fn fire(key: [u32; 8], oracle: Oracle, order: Order, o: &Opening, hash: Hasher, to: Option<String>) -> Result<Transition, String> {
    if commitment(o, hash) != order.c {
        return Err("this opening is not the one the order was placed with".into());
    }
    if !fires(o.kind, oracle.price, o.threshold) {
        let what = if o.kind == STOP { "stop" } else { "take-profit" };
        return Err(format!(
            "the condition does not hold: the price is {}, the {what} is at {}; nothing to prove, nothing sent",
            oracle.price, o.threshold
        ));
    }
    let mut t = Transition::new()
        .read(ORACLE_KEY, oracle.value())
        .read(key, order.value())
        .write(ORACLE_KEY, oracle.value())
        .write(key, [0; 8])
        .pay(RAND, order.amount)
        .sorted();
    t.pays[0].to = to;
    Ok(t)
}

/// Cancel the order under `key`: its escrow back, whatever the price.
pub fn cancel(key: [u32; 8], order: Order) -> Transition {
    Transition::new().read(key, order.value()).write(key, [0; 8]).pay(RAND, order.amount)
}

/// A `<name>.secret` file: the ticket (eight words), then the kind and the threshold, then the
/// salt (four words) — fourteen whitespace-separated decimals.
pub fn read_secret(path: &str) -> Result<([u32; 8], Opening), String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    let v: Vec<u64> = text
        .split_whitespace()
        .map(|t| t.parse::<u64>())
        .collect::<Result<_, _>>()
        .map_err(|e| format!("{path}: {e}"))?;
    if v.len() != 14 {
        return Err(format!("{path}: expected 14 numbers (ticket, kind, threshold, salt), found {}", v.len()));
    }
    let word = |x: u64, what: &str| u32::try_from(x).map_err(|_| format!("{path}: {what} is not a word"));
    let mut ticket = [0u32; 8];
    for (i, w) in ticket.iter_mut().enumerate() {
        *w = word(v[i], "the ticket")?;
    }
    let mut salt = [0u32; 4];
    for (i, w) in salt.iter_mut().enumerate() {
        *w = word(v[10 + i], "the salt")?;
    }
    Ok((ticket, Opening { kind: word(v[8], "the kind")?, threshold: v[9], salt }))
}
