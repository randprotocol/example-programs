//! perp: a perpetual-futures market on one index asset X, priced in RAND by an operator, with a
//! pool of liquidity providers as every trader's counterparty (GMX's design, cut to its core).
//!
//! Margin, settlement and the pool are all RAND, held in the program's vault. LPs deposit RAND
//! for the program's own LP token; traders post margin and open a long or a short at the
//! operator's price; when they close, the pool pays their profit or keeps their loss. The
//! operator's **lock** (eight words, `POSEIDON2([TAG_OPERATOR, secret])`) is the program's
//! deploy-time public input, so it is part of the program id.
//!
//! `E = 10^9`. A price `P` is RAND base units per `E` base units of X; a position's `cost` is its
//! notional at entry in RAND base units. Every amount is below 2^63.
//!
//! ```text
//! market    [1, 0, 0, 0, 0, 0, 0, 0]
//!           [P_lo, P_hi, cash_lo, cash_hi, supply_lo, supply_hi, lp, 1]
//!           price; the pool's RAND; LP tokens outstanding; the LP token; version
//! oi        [2, 0, 0, 0, 0, 0, 0, 0]
//!           [Lq_lo, Lq_hi, Lc_lo, Lc_hi, Sq_lo, Sq_hi, Sc_lo, Sc_hi]
//!           open interest: long size and cost, short size and cost (absent = all zero)
//! position  [3, d0, …, d6]    d = POSEIDON2([TAG_OWNER, secret])
//!           [margin_lo, margin_hi, q_lo, q_hi, cost_lo, cost_hi, side, 1]
//!           side 1 long, 2 short; version
//! ```
//!
//! The price and the pool share one cell so that a close — which needs the price, the pool, the
//! open interest and the position, and pays — fits the segment: three reads, three writes and a
//! payout are 110 context words, against 111 for a program with an eight-word public input.
//!
//! | method | private inputs | cells read → written | rule |
//! |---|---|---|---|
//! | 1 operate | `[1, operator secret, P, lp, old P]` | market → market | absent: init (`P > 0`, cash = supply = 0, `lp ≠ RAND`); live: set `P > 0`, all else unchanged |
//! | 2 lp_add | `[2]` | market, oi → market, oi | RAND `a` in, mint `m` LP: `S = 0` ⇒ `m = a`; else `m · NAV9 ≤ a · S · E` |
//! | 3 lp_remove | `[3]` | market, oi → market, oi | burn `b` LP, pay `x`: `x · S · E ≤ b · NAV9`; `cash − x ≥ Lc + Sc` |
//! | 4 open | `[4, secret]` | market, oi, position (absent) → market, oi, position | margin `m` in; long `cost · E ≥ q · P`, short `cost · E ≤ q · P`; `cost ≤ 10 · m`; `cash ≥ Lc' + Sc'` |
//! | 5 close | `[5, secret]` | market, oi, position → market, oi, (deleted) | pay `x`: `x · E ≤ value9`; `cash' = cash + margin − x` |
//! | 6 liquidate | `[6]` | market, oi, position → market, oi, (deleted) | `value9 · 100 < cost · E · 5`; pay `x`: `x · E ≤ value9`, `x · 100 ≤ cost` |
//!
//! `NAV9 = (cash + Lc) · E + Sq · P − Sc · E − Lq · P`, the pool less the traders' aggregate
//! (uncapped) profit, times `E`. A position's `pnl9` is `q · P − cost · E` long, `cost · E − q · P`
//! short, capped at `+cost · E`; `value9 = max(0, margin · E + pnl9)`.
//!
//! **Why the pool can always pay.** No payout exceeds `margin + cost` (the cap), and the pool
//! keeps `cash ≥ Lc + Sc` (open and LP withdrawals check it; a price move does not touch either
//! side; a close or liquidation lowers both by at most the position's cost). So the vault, which
//! holds `cash + Σ margin`, covers every position closing at once.
//!
//! Every read cell is written back — unchanged where the method does not move it — so every word
//! the program is shown is pinned by an equality. In every method the cells written are exactly
//! those read moved by what came in and went out; the caller only chooses amounts, and every
//! amount meets an inequality checked with multiplications only. `plan` finds the best amounts by
//! searching the same inequalities.
#![cfg_attr(target_arch = "riscv32", no_std)]
#![forbid(unsafe_code)]

pub use rpl2_kit as kit;
use kit::{add_note, digest, lt_note, opens_lock, owned_key, prod, prod3, secret_at, sub_note, words_of, Header, Source, RAND, U256};

pub const OPERATE: u32 = 1;
pub const LP_ADD: u32 = 2;
pub const LP_REMOVE: u32 = 3;
pub const OPEN: u32 = 4;
pub const CLOSE: u32 = 5;
pub const LIQUIDATE: u32 = 6;

pub const MARKET_KEY: [u32; 8] = [1, 0, 0, 0, 0, 0, 0, 0];
pub const OI_KEY: [u32; 8] = [2, 0, 0, 0, 0, 0, 0, 0];
/// A position's key starts with this word; the other seven are its owner's digest.
pub const POSITION_TAG: u32 = 3;
/// The last word of a live market and of a live position: never all zeros.
pub const VERSION: u32 = 1;

pub const LONG: u32 = 1;
pub const SHORT: u32 = 2;

/// Fixed point: a price is RAND base units per `E` base units of X.
pub const E: u64 = 1_000_000_000;
/// Leverage: notional at most ten times the margin.
pub const MAX_LEVERAGE: u64 = 10;
/// Liquidatable when the position's value is below 5 % of its notional…
pub const MAINT_NUM: u64 = 5;
/// …and the liquidator's reward is at most 1 % of the notional.
pub const REWARD_NUM: u64 = 1;
pub const PCT: u64 = 100;

/// `POSEIDON2` domain tags, four ASCII characters read little-endian: "popr" for the operator's
/// lock, "pown" for a position owner's digest.
pub const TAG_OPERATOR: u32 = u32::from_le_bytes(*b"popr");
pub const TAG_OWNER: u32 = u32::from_le_bytes(*b"pown");

/// The public input: the operator's lock, eight words.
pub const PUBLIC_LOCK: u32 = 0;
pub const PUBLIC_WORDS: u32 = 8;
/// Where a secret starts among the private inputs (after the method).
pub const SECRET_AT: u32 = 1;
/// operate's private inputs after the secret: the price (low, high) and the LP token the
/// operator writes, and the price it replaces (0 when creating the market) — a compare-and-set,
/// so that every market word an update is shown is pinned, the old price too.
pub const PRICE_AT: u32 = 9;
pub const LP_AT: u32 = 11;
pub const OLD_PRICE_AT: u32 = 12;

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
    Operator,
    Market,
    Exists,
    Position,
    Price,
    Leverage,
    Reserve,
    Nav,
    Payout,
    Healthy,
}
use Refusal::*;

/// The market cell: the price and the LP pool.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Market {
    pub price: u64,
    pub cash: u64,
    pub supply: u64,
    pub lp: u32,
}

impl Market {
    pub fn value(&self) -> [u32; 8] {
        let (a, b, c) = (words_of(self.price), words_of(self.cash), words_of(self.supply));
        [a.0, a.1, b.0, b.1, c.0, c.1, self.lp, VERSION]
    }

    /// `None` for an absent cell (or anything that is not a live market).
    pub fn from_value(v: &[u32; 8]) -> Option<Market> {
        if v[7] != VERSION {
            return None;
        }
        Some(Market { price: kit::u64_of(v[0], v[1]), cash: kit::u64_of(v[2], v[3]), supply: kit::u64_of(v[4], v[5]), lp: v[6] })
    }

    #[inline(always)]
    fn in_range(&self) -> bool {
        lt_note(self.price) & lt_note(self.cash) & lt_note(self.supply)
    }
}

/// The open-interest cell. All zero (absent) is a valid, empty book.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Oi {
    pub long_q: u64,
    pub long_cost: u64,
    pub short_q: u64,
    pub short_cost: u64,
}

impl Oi {
    pub fn value(&self) -> [u32; 8] {
        let (a, b, c, d) = (words_of(self.long_q), words_of(self.long_cost), words_of(self.short_q), words_of(self.short_cost));
        [a.0, a.1, b.0, b.1, c.0, c.1, d.0, d.1]
    }

    pub fn from_value(v: &[u32; 8]) -> Oi {
        Oi {
            long_q: kit::u64_of(v[0], v[1]),
            long_cost: kit::u64_of(v[2], v[3]),
            short_q: kit::u64_of(v[4], v[5]),
            short_cost: kit::u64_of(v[6], v[7]),
        }
    }

    #[inline(always)]
    fn in_range(&self) -> bool {
        lt_note(self.long_q) & lt_note(self.long_cost) & lt_note(self.short_q) & lt_note(self.short_cost)
    }

    /// The book with a position added (`grow`) or taken out, on its own side; `None` out of range.
    #[inline(always)]
    pub fn moved(&self, p: &Position, grow: bool) -> Option<Oi> {
        let f = |a: u64, b: u64| if grow { add_note(a, b) } else { sub_note(a, b) };
        if p.side == LONG {
            Some(Oi { long_q: f(self.long_q, p.q)?, long_cost: f(self.long_cost, p.cost)?, ..*self })
        } else {
            Some(Oi { short_q: f(self.short_q, p.q)?, short_cost: f(self.short_cost, p.cost)?, ..*self })
        }
    }
}

/// A position cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Position {
    pub margin: u64,
    pub q: u64,
    pub cost: u64,
    pub side: u32,
}

impl Position {
    pub fn value(&self) -> [u32; 8] {
        let (a, b, c) = (words_of(self.margin), words_of(self.q), words_of(self.cost));
        [a.0, a.1, b.0, b.1, c.0, c.1, self.side, VERSION]
    }

    pub fn from_value(v: &[u32; 8]) -> Option<Position> {
        if v[7] != VERSION {
            return None;
        }
        Some(Position { margin: kit::u64_of(v[0], v[1]), q: kit::u64_of(v[2], v[3]), cost: kit::u64_of(v[4], v[5]), side: v[6] })
    }

    /// In range, a real side, a nonzero size and cost.
    #[inline(always)]
    fn well_formed(&self) -> bool {
        lt_note(self.margin)
            & lt_note(self.q)
            & lt_note(self.cost)
            & ((self.side == LONG) | (self.side == SHORT))
            & (self.q != 0)
            & (self.cost != 0)
    }
}

/// The key of the position owned by the secret whose digest is `d`.
pub fn position_key(d: &[u32; 8]) -> [u32; 8] {
    owned_key(POSITION_TAG, d)
}

#[inline(never)]
fn market_at<S: Source>(s: &S, c: kit::Cell) -> Market {
    Market { price: c.val64(s, 0), cash: c.val64(s, 2), supply: c.val64(s, 4), lp: c.val(s, 6) }
}

#[inline(never)]
fn oi_at<S: Source>(s: &S, c: kit::Cell) -> Oi {
    Oi { long_q: c.val64(s, 0), long_cost: c.val64(s, 2), short_q: c.val64(s, 4), short_cost: c.val64(s, 6) }
}

#[inline(never)]
fn position_at<S: Source>(s: &S, c: kit::Cell) -> Position {
    Position { margin: c.val64(s, 0), q: c.val64(s, 2), cost: c.val64(s, 4), side: c.val(s, 6) }
}

// ---- The inequalities, shared with the wallet (`plan`) -------------------------------------
//
// Every amount is below 2^63 and E, PCT below 2^7·2^30, so: q · P < 2^126; cost · E,
// margin · E, x · E < 2^93; each sum below of at most four such terms < 2^128; times one more
// amount (or PCT) < 2^191. Nothing here comes near 2^256.

/// `NAV9 = (cash + Lc) · E + Sq · P − Sc · E − Lq · P` when it is positive.
pub fn nav9(m: &Market, oi: &Oi) -> Option<U256> {
    let pos = prod(m.cash, E).add(prod(oi.long_cost, E)).add(prod(oi.short_q, m.price));
    let neg = prod(oi.short_cost, E).add(prod(oi.long_q, m.price));
    if neg.lt(pos) { Some(pos.sub(neg)) } else { None }
}

/// Minting `minted` LP for `a` RAND: the first deposit (no supply) mints one for one; any other
/// mints no more than its share of the pool's net asset value.
pub fn add_ok(m: &Market, oi: &Oi, a: u64, minted: u64) -> bool {
    if m.supply == 0 {
        return minted == a;
    }
    match nav9(m, oi) {
        Some(nav) => nav.mul(minted).le(prod3(a, m.supply, E)),
        None => false,
    }
}

/// The pool's reserve: its cash covers every open position's notional, the most the capped
/// profits can ever claim.
pub fn reserve_ok(cash: u64, oi: &Oi) -> bool {
    U256::from(oi.long_cost).add(U256::from(oi.short_cost)).le(U256::from(cash))
}

/// Burning `burned` LP for `x` RAND: no more than its share of the net asset value, and the
/// reserve still holds afterwards.
pub fn remove_ok(m: &Market, oi: &Oi, burned: u64, x: u64) -> bool {
    let share = match nav9(m, oi) {
        Some(nav) => prod3(x, m.supply, E).le(nav.mul(burned)),
        None => false,
    };
    let reserve = match sub_note(m.cash, x) {
        Some(left) => reserve_ok(left, oi),
        None => false,
    };
    share & reserve & (burned <= m.supply)
}

/// Entering at `price`, rounded against the trader: a long pays at least the price, a short
/// sells at no more than it.
pub fn entry_ok(side: u32, q: u64, cost: u64, price: u64) -> bool {
    let (c, v) = (prod(cost, E), prod(q, price));
    if side == LONG { v.le(c) } else { c.le(v) }
}

/// Notional at most `MAX_LEVERAGE` times the margin.
pub fn leverage_ok(margin: u64, cost: u64) -> bool {
    prod(cost, 1).le(prod(margin, MAX_LEVERAGE))
}

/// A position's value at `price`, as `(plus, minus)`: `value9 = max(0, plus − minus)`. The
/// profit cap (`pnl9 ≤ cost · E`) binds only a long: a short's profit is at most `cost · E`.
pub fn value9(p: &Position, price: u64) -> (U256, U256) {
    let (cost9, mark9, margin9) = (prod(p.cost, E), prod(p.q, price), prod(p.margin, E));
    if p.side == LONG {
        let capped = cost9.add(cost9);
        let gain = if mark9.le(capped) { mark9 } else { capped };
        (margin9.add(gain), cost9)
    } else {
        (margin9.add(cost9), mark9)
    }
}

/// Paying `x` for the position: `x · E ≤ value9` (as `x · E + minus ≤ plus`).
pub fn payout_ok(p: &Position, price: u64, x: u64) -> bool {
    let (plus, minus) = value9(p, price);
    prod(x, E).add(minus).le(plus)
}

/// `value9 · 100 < cost · E · 5` (as `plus · 100 < minus · 100 + cost · E · 5`, which also holds
/// when the value is nothing, since cost > 0).
pub fn liquidatable(p: &Position, price: u64) -> bool {
    let (plus, minus) = value9(p, price);
    plus.mul(PCT).lt(minus.mul(PCT).add(prod3(p.cost, E, MAINT_NUM)))
}

/// A liquidator's reward: paid out of the position's value, and at most 1 % of its notional.
pub fn reward_ok(p: &Position, price: u64, x: u64) -> bool {
    payout_ok(p, price, x) & prod(x, PCT).le(prod(p.cost, REWARD_NUM))
}

// ---- The rules ------------------------------------------------------------------------------

/// Accept or refuse the transition `s` shows. On acceptance, the receipt's eight output words:
///
/// | method | outputs |
/// |---|---|
/// | operate | `[1, P_lo, P_hi, 0, 0, 0, 0, 0]` |
/// | lp_add | `[2, a_lo, a_hi, minted_lo, minted_hi, 0, 0, 0]` |
/// | lp_remove | `[3, burned_lo, burned_hi, x_lo, x_hi, 0, 0, 0]` |
/// | open | `[4, margin_lo, margin_hi, q_lo, q_hi, cost_lo, cost_hi, side]` |
/// | close, liquidate | `[5 or 6, x_lo, x_hi, margin_lo, margin_hi, cost_lo, cost_hi, side]` |
pub fn check<S: Source>(s: &S) -> Result<[u32; 8], Refusal> {
    let h = Header::read(s).ok_or(Version)?;
    match s.input(0) {
        OPERATE => operate(s, &h),
        LP_ADD => lp_add(s, &h),
        LP_REMOVE => lp_remove(s, &h),
        OPEN => open(s, &h),
        CLOSE => settle(s, &h, false),
        LIQUIDATE => settle(s, &h, true),
        _ => Err(Method),
    }
}

/// Output words: a method and three amounts (the last only partly, with a side).
#[inline(always)]
fn out(method: u32, a: u64, b: u64, c: u64, side: u32) -> [u32; 8] {
    let (a, b, c) = (words_of(a), words_of(b), words_of(c));
    [method, a.0, a.1, b.0, b.1, c.0, c.1, side]
}

/// The market, read and written: read 0 and write 0 under the market's key, the read live and in
/// range, the written one with its version word. `(before, after)`.
#[inline(never)]
fn market_rw<S: Source>(s: &S, h: &Header) -> Result<(Market, Market), Refusal> {
    let (r, w) = (h.read_cell(0), h.write_cell(0));
    if !r.key_is(s, &MARKET_KEY) || !w.key_is(s, &MARKET_KEY) {
        return Err(Key);
    }
    if (r.val(s, 7) != VERSION) | (w.val(s, 7) != VERSION) {
        return Err(Market);
    }
    let (before, after) = (market_at(s, r), market_at(s, w));
    if !before.in_range() | !after.in_range() {
        return Err(Range);
    }
    Ok((before, after))
}

/// The open interest, read and written: read 1 and write 1 under its key. `(before, after)`.
#[inline(never)]
fn oi_rw<S: Source>(s: &S, h: &Header) -> Result<(Oi, Oi), Refusal> {
    let (r, w) = (h.read_cell(1), h.write_cell(1));
    if !r.key_is(s, &OI_KEY) || !w.key_is(s, &OI_KEY) {
        return Err(Key);
    }
    let (before, after) = (oi_at(s, r), oi_at(s, w));
    if !before.in_range() | !after.in_range() {
        return Err(Range);
    }
    Ok((before, after))
}

/// The key of the position whose owner's secret is the private input at `SECRET_AT`.
#[inline(never)]
fn owner_key<S: Source>(s: &S) -> [u32; 8] {
    position_key(&digest(s, TAG_OWNER, &secret_at(s, SECRET_AT)))
}

/// Set up the market, or move its price. Only the operator.
fn operate<S: Source>(s: &S, h: &Header) -> Result<[u32; 8], Refusal> {
    if !h.shape(1, 1, 0, 0) {
        return Err(Shape);
    }
    if !h.nothing_in() {
        return Err(Inflow);
    }
    if !opens_lock(s, TAG_OPERATOR, SECRET_AT, PUBLIC_LOCK) {
        return Err(Operator);
    }
    let (r, w) = (h.read_cell(0), h.write_cell(0));
    if !r.key_is(s, &MARKET_KEY) || !w.key_is(s, &MARKET_KEY) {
        return Err(Key);
    }
    if w.val(s, 7) != VERSION {
        return Err(Value);
    }
    let after = market_at(s, w);
    if !after.in_range() {
        return Err(Range);
    }
    let old_price = kit::u64_of(s.input(OLD_PRICE_AT), s.input(OLD_PRICE_AT + 1));
    if (after.price != kit::u64_of(s.input(PRICE_AT), s.input(PRICE_AT + 1))) | (after.lp != s.input(LP_AT)) {
        return Err(Value);
    }
    if after.price == 0 {
        return Err(Zero);
    }
    if r.is_zero(s) {
        // The market's birth: an empty pool, and the LP token bound for good. The chain lets a
        // program mint and burn only its own token, so a wrong one leaves the pool unusable, never
        // open to another token.
        if (after.cash != 0) | (after.supply != 0) | (old_price != 0) {
            return Err(Value);
        }
        if after.lp == RAND {
            return Err(Asset);
        }
    } else {
        if r.val(s, 7) != VERSION {
            return Err(Market);
        }
        let before = market_at(s, r);
        if before.price != old_price {
            return Err(Value);
        }
        if (after.cash != before.cash) | (after.supply != before.supply) | (after.lp != before.lp) {
            return Err(Value);
        }
    }
    Ok(out(OPERATE, after.price, 0, 0, 0))
}

/// RAND in for newly minted LP.
fn lp_add<S: Source>(s: &S, h: &Header) -> Result<[u32; 8], Refusal> {
    if !h.shape(2, 2, 0, 1) {
        return Err(Shape);
    }
    let a = h.burn_r;
    if (a == 0) | !h.no_token_in() {
        return Err(Inflow);
    }
    let (before, after) = market_rw(s, h)?;
    let (oi, oi_after) = oi_rw(s, h)?;
    if oi_after != oi {
        return Err(Value);
    }
    let (lp, minted) = h.mint(s, 0);
    if lp != before.lp {
        return Err(Asset);
    }
    if minted == 0 {
        return Err(Zero);
    }
    if !lt_note(a) | !lt_note(minted) {
        return Err(Range);
    }
    let cash = add_note(before.cash, a).ok_or(Range)?;
    let supply = add_note(before.supply, minted).ok_or(Range)?;
    if (after.price != before.price) | (after.cash != cash) | (after.supply != supply) | (after.lp != before.lp) {
        return Err(Value);
    }
    if !add_ok(&before, &oi, a, minted) {
        return Err(Nav);
    }
    Ok(out(LP_ADD, a, minted, 0, 0))
}

/// Burn LP for RAND.
fn lp_remove<S: Source>(s: &S, h: &Header) -> Result<[u32; 8], Refusal> {
    if !h.shape(2, 2, 1, 0) {
        return Err(Shape);
    }
    let (before, after) = market_rw(s, h)?;
    let burned = h.burn_a;
    if (h.burn_r != 0) | (h.inflow != kit::INFLOW_BURN) | (h.burn_asset != before.lp) {
        return Err(Inflow);
    }
    let (oi, oi_after) = oi_rw(s, h)?;
    if oi_after != oi {
        return Err(Value);
    }
    let (asset, x) = h.pay(s, 0);
    if asset != RAND {
        return Err(Asset);
    }
    if (burned == 0) | (x == 0) {
        return Err(Zero);
    }
    if !lt_note(burned) | !lt_note(x) {
        return Err(Range);
    }
    let cash = sub_note(before.cash, x).ok_or(Reserve)?;
    let supply = sub_note(before.supply, burned).ok_or(Nav)?;
    if (after.price != before.price) | (after.cash != cash) | (after.supply != supply) | (after.lp != before.lp) {
        return Err(Value);
    }
    if !remove_ok(&before, &oi, burned, x) {
        return Err(Nav);
    }
    Ok(out(LP_REMOVE, burned, x, 0, 0))
}

/// Post margin and open a position at the operator's price. One position per secret.
fn open<S: Source>(s: &S, h: &Header) -> Result<[u32; 8], Refusal> {
    if !h.shape(3, 3, 0, 0) {
        return Err(Shape);
    }
    let margin = h.burn_r;
    if (margin == 0) | !h.no_token_in() {
        return Err(Inflow);
    }
    let (market, market_after) = market_rw(s, h)?;
    if market_after != market {
        return Err(Value);
    }
    let (oi, oi_after) = oi_rw(s, h)?;
    let key = owner_key(s);
    let (r, w) = (h.read_cell(2), h.write_cell(2));
    if !r.key_is(s, &key) || !w.key_is(s, &key) {
        return Err(Key);
    }
    if !r.is_zero(s) {
        return Err(Exists);
    }
    if w.val(s, 7) != VERSION {
        return Err(Value);
    }
    let p = position_at(s, w);
    if !p.well_formed() {
        return Err(Position);
    }
    if p.margin != margin {
        return Err(Value);
    }
    if Some(oi_after) != oi.moved(&p, true) {
        return Err(Value);
    }
    if !entry_ok(p.side, p.q, p.cost, market.price) {
        return Err(Price);
    }
    if !leverage_ok(p.margin, p.cost) {
        return Err(Leverage);
    }
    if !reserve_ok(market.cash, &oi_after) {
        return Err(Reserve);
    }
    Ok(out(OPEN, p.margin, p.q, p.cost, p.side))
}

/// Close a position (its owner) or liquidate it (anyone, once it is under water): the position is
/// deleted, its size and cost leave the open interest, and the pool takes its margin and pays `x`.
fn settle<S: Source>(s: &S, h: &Header, liquidation: bool) -> Result<[u32; 8], Refusal> {
    if (h.n_reads != 3) | (h.n_writes != 3) | (h.n_mints != 0) | (h.n_pays > 1) {
        return Err(Shape);
    }
    if !h.nothing_in() {
        return Err(Inflow);
    }
    let (before, after) = market_rw(s, h)?;
    let (oi, oi_after) = oi_rw(s, h)?;
    let (r, w) = (h.read_cell(2), h.write_cell(2));
    let key = if liquidation { r.keys(s) } else { owner_key(s) };
    if (key[0] != POSITION_TAG) || !r.key_is(s, &key) || !w.key_is(s, &key) {
        return Err(Key);
    }
    if r.val(s, 7) != VERSION {
        return Err(Position);
    }
    let p = position_at(s, r);
    if !p.well_formed() {
        return Err(Position);
    }
    if !w.is_zero(s) {
        return Err(Value);
    }
    let x = if h.n_pays == 1 {
        let (asset, x) = h.pay(s, 0);
        if asset != RAND {
            return Err(Asset);
        }
        if x == 0 {
            return Err(Zero);
        }
        if !lt_note(x) {
            return Err(Range);
        }
        x
    } else {
        0
    };
    let cash = sub_note(add_note(before.cash, p.margin).ok_or(Range)?, x).ok_or(Reserve)?;
    if (after.price != before.price) | (after.cash != cash) | (after.supply != before.supply) | (after.lp != before.lp) {
        return Err(Value);
    }
    if Some(oi_after) != oi.moved(&p, false) {
        return Err(Value);
    }
    // Paying nothing is always allowed: a position under water is closed (or liquidated) for
    // nothing, and the pool keeps its margin.
    if liquidation {
        if !liquidatable(&p, before.price) {
            return Err(Healthy);
        }
        if (x != 0) & !reward_ok(&p, before.price, x) {
            return Err(Payout);
        }
    } else if (x != 0) & !payout_ok(&p, before.price, x) {
        return Err(Payout);
    }
    Ok(out(if liquidation { LIQUIDATE } else { CLOSE }, x, p.margin, p.cost, p.side))
}

#[cfg(not(target_arch = "riscv32"))]
pub mod plan;
