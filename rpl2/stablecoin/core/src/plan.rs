//! The wallet's side (host only): build the transitions `check` accepts, choosing the largest
//! mint or withdrawal by searching [`healthy`], the very predicate `check` tests.

use crate::*;
use kit::host::{max_satisfying, Hasher, Transition};

/// The largest amount below 2^63.
const MAX_NOTE: u64 = (1 << 63) - 1;

/// `POSEIDON2([tag, s0..s7])` with the given hasher.
pub fn digest_with(hash: Hasher, tag: u32, s: &[u32; 8]) -> [u32; 8] {
    hash([tag, s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7]])
}

/// The operator's lock: the program's public input.
pub fn lock_of(hash: Hasher, operator: &[u32; 8]) -> [u32; 8] {
    digest_with(hash, TAG_OPERATOR, operator)
}

/// The key of the position a secret owns.
pub fn key_of(hash: Hasher, owner: &[u32; 8]) -> [u32; 8] {
    position_key(&digest_with(hash, TAG_OWNER, owner))
}

/// Set the price to `price`; with no config yet, bind `stable` as the stable token too.
/// Returns the transition and the private inputs.
pub fn operate(operator: &[u32; 8], before: Option<Config>, stable: u32, price: u64) -> Result<(Transition, Vec<u32>, Config), String> {
    if price == 0 || price > MAX_NOTE {
        return Err(format!("the price must be between 1 and {MAX_NOTE}"));
    }
    let (read, old, stable) = match before {
        None => {
            if stable == RAND {
                return Err("the stable token cannot be RAND (asset 0): register one with ./stable-token.sh".into());
            }
            ([0; 8], 0, stable)
        }
        Some(c) => (c.value(), c.price, c.stable),
    };
    let after = Config { price, stable };
    let t = Transition::new().read(CONFIG_KEY, read).write(CONFIG_KEY, after.value());
    let (o, n) = (words_of(old), words_of(price));
    let mut input = vec![OPERATE];
    input.extend_from_slice(operator);
    input.extend_from_slice(&[stable, o.0, o.1, n.0, n.1]);
    Ok((t, input, after))
}

/// One adjustment of a position: amounts in base units, zero for "none".
#[derive(Clone, Copy, Debug, Default)]
pub struct Moves {
    /// RAND locked as collateral.
    pub deposit: u64,
    /// RAND taken out.
    pub withdraw: u64,
    /// Stable borrowed (minted).
    pub mint: u64,
    /// Stable repaid (burned).
    pub repay: u64,
}

/// The position `moves` leaves, or why `check` would refuse it.
pub fn after(cfg: &Config, p: Position, m: Moves) -> Result<Position, String> {
    let coll = p.coll.checked_add(m.deposit).filter(|x| lt_note(*x)).ok_or("too much collateral")?;
    let coll = coll.checked_sub(m.withdraw).ok_or(format!("the position holds {coll} RAND units after the deposit"))?;
    let debt = p.debt.checked_add(m.mint).filter(|x| lt_note(*x)).ok_or("too much debt")?;
    let debt = debt.checked_sub(m.repay).ok_or(format!("the position owes {debt} after the mint"))?;
    let q = Position { coll, debt };
    if q == p {
        return Err("this changes nothing".into());
    }
    let riskier = debt > p.debt || coll < p.coll;
    if riskier && !healthy(coll, debt, cfg.price, MCR_PCT) {
        return Err(format!("the position would be under {MCR_PCT} % (collateral {coll}, debt {debt}, price {})", cfg.price));
    }
    Ok(q)
}

/// The most that can be borrowed against `coll` with `debt` already owed, at 150 %.
pub fn max_mint(cfg: &Config, coll: u64, debt: u64) -> u64 {
    max_satisfying(MAX_NOTE - debt, |m| healthy(coll, debt + m, cfg.price, MCR_PCT))
}

/// The most collateral that can be taken out of `coll` with `debt` owed, at 150 %.
pub fn max_withdraw(cfg: &Config, coll: u64, debt: u64) -> u64 {
    max_satisfying(coll, |w| healthy(coll - w, debt, cfg.price, MCR_PCT))
}

/// The highest price at which a position can be liquidated (it is below 110 % at it).
pub fn liquidation_price(p: &Position) -> u64 {
    max_satisfying(MAX_NOTE, |price| !healthy(p.coll, p.debt, price, LIQUIDATION_PCT))
}

/// Adjust the position `key` (owned by `owner`) by `m`. Returns the transition, the private
/// inputs and the position after.
pub fn adjust(owner: &[u32; 8], key: [u32; 8], cfg: Config, p: Position, m: Moves) -> Result<(Transition, Vec<u32>, Position), String> {
    let q = after(&cfg, p, m)?;
    let mut t = Transition::new()
        .read(CONFIG_KEY, cfg.value())
        .read(key, p.value())
        .write(CONFIG_KEY, cfg.value())
        .write(key, q.value())
        .rand_in(m.deposit);
    if m.repay > 0 {
        t = t.burn(cfg.stable, m.repay);
    }
    if m.withdraw > 0 {
        t = t.pay(RAND, m.withdraw);
    }
    if m.mint > 0 {
        t = t.mint(cfg.stable, m.mint);
    }
    let mut input = vec![ADJUST];
    input.extend_from_slice(owner);
    Ok((t.sorted(), input, q))
}

/// Liquidate the position `key`: burn its whole debt, take its whole collateral.
pub fn liquidate(key: [u32; 8], cfg: Config, p: Position) -> Result<(Transition, Vec<u32>), String> {
    if p.coll == 0 || p.debt == 0 {
        return Err("only a position with both collateral and debt can be liquidated".into());
    }
    if healthy(p.coll, p.debt, cfg.price, LIQUIDATION_PCT) {
        return Err(format!(
            "the position is at or above {LIQUIDATION_PCT} % at price {}; it becomes liquidatable at {} or below",
            cfg.price,
            liquidation_price(&p)
        ));
    }
    let t = Transition::new()
        .read(CONFIG_KEY, cfg.value())
        .read(key, p.value())
        .write(CONFIG_KEY, cfg.value())
        .write(key, [0; 8])
        .burn(cfg.stable, p.debt)
        .pay(RAND, p.coll);
    Ok((t.sorted(), vec![LIQUIDATE]))
}
