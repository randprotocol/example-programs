//! lending: a Compound/Aave-style money market. Lenders supply RAND for shares the program alone
//! mints; borrowers post a collateral token `C` and borrow RAND against it; an operator sets the
//! price of `C` and accrues interest.
//!
//! The deploy-time **public input** is nine words: the operator's lock (`POSEIDON2([TAG_OPERATOR,
//! s0..s7])`, words 0..8) and `C`'s asset index (word 8). Amounts are base units; `E = 10⁹`.
//!
//! ```text
//! price     [1, 0, 0, 0, 0, 0, 0, 0]  [p_lo, p_hi, 0, 0, 0, 0, 0, 1]                P: RAND units per E units of C
//! pool      [2, 0, 0, 0, 0, 0, 0, 0]  [cash_lo, cash_hi, sb_lo, sb_hi, s_lo, s_hi, i_lo, i_hi]
//! position  [3, d0, …, d6]            [coll_lo, coll_hi, sd_lo, sd_hi, 0, 0, 0, 1]   d = POSEIDON2([TAG_OWNER, secret])
//! shares    [4, 0, 0, 0, 0, 0, 0, 0]  [share, 0, 0, 0, 0, 0, 0, 1]                  the share token, bound at init
//! ```
//!
//! `cash` is the RAND the pool holds, `sb` the scaled borrows, `s` the share supply, `I` the
//! borrow index (scaled by `E`, `E` at init, only ever rising). A position owes `sd · I / E` RAND.
//! Total assets, times `E`: `TA9 = cash · E + sb · I`. A live pool has `I ≥ E`, so it is never
//! all zeros.
//!
//! | method | private inputs | transition | rule |
//! |---|---|---|---|
//! | 1 operate, init | `[1, s, p_lo, p_hi, share]` | pool absent → price, pool `[0,0,0,E]`, shares | operator; `C`, share, RAND distinct; `P > 0` |
//! | 1 operate, update | `[1, s, p_lo, p_hi, rate]` | pool → price (blind), pool | operator; `P > 0`; `rate ≤ E/100`; `I'·E ≤ I·(E+rate) < I'·E + E` |
//! | 2 supply | `[2]` | RAND in; mint shares | first: `m + 1000 = a`; else `m · TA9 ≤ a · s · E` |
//! | 3 withdraw | `[3]` | shares burned; pay RAND | `x · s · E ≤ b · TA9` |
//! | 4 adjust | `[4, s0..s7]` | optional C in, RAND in; at most one pay (RAND or C) | `sd'·I + r·E ≥ sd·I + x·E`; if riskier, `sd'·I·100 ≤ coll'·P·75` |
//! | 5 liquidate | `[5]` | RAND in; pay C | `sd·I·100 > coll·P·85`; `sd'·I + r·E ≥ sd·I`; `c·P·100 ≤ (sd − sd')·I·110` |
//!
//! **Every word the program is shown is pinned by an equality**: each cell a method reads, it
//! also writes — moved by exactly what came in and went out, or unchanged — and each amount
//! appears both in the inflow or a payout and in a written cell. The inequalities above then only
//! ever decide *whether*; no word can be nudged and still be accepted. The operator's free
//! choices (the new price, the share token, the rate) are its private inputs, and the words it
//! writes must agree with them.
//!
//! The program never divides. The operator's interest is the honest substitute for a clock: a
//! program cannot see time, so the operator raises `I` by at most 1 % per update.
#![cfg_attr(target_arch = "riscv32", no_std)]
#![forbid(unsafe_code)]

pub use rpl2_kit as kit;
use kit::{
    add_note, lt_note, opens_lock, owned_key, prod, prod3, secret_at, sub_note, u64_of, words_of, Cell, Header,
    Source, INFLOW_BURN, INFLOW_DEPOSIT, RAND, U256,
};

pub const OPERATE: u32 = 1;
pub const SUPPLY: u32 = 2;
pub const WITHDRAW: u32 = 3;
pub const ADJUST: u32 = 4;
pub const LIQUIDATE: u32 = 5;

/// The scale of prices and of the borrow index.
pub const E: u64 = 1_000_000_000;
/// The most an update may accrue: 1 % of the index (`E / 100`).
pub const MAX_RATE: u64 = 10_000_000;
/// Borrow up to 75 % of the collateral's value; liquidatable above 85 %; liquidators get 10 %.
pub const LTV_PCT: u64 = 75;
pub const LIQ_PCT: u64 = 85;
pub const BONUS_PCT: u64 = 110;
/// Shares locked for ever by the first supply (the defence against share inflation).
pub const MIN_SHARES: u64 = 1_000;

pub const PRICE_KEY: [u32; 8] = [1, 0, 0, 0, 0, 0, 0, 0];
pub const POOL_KEY: [u32; 8] = [2, 0, 0, 0, 0, 0, 0, 0];
/// A position's key is `[POSITION_TAG, d0, …, d6]`.
pub const POSITION_TAG: u32 = 3;
pub const SHARES_KEY: [u32; 8] = [4, 0, 0, 0, 0, 0, 0, 0];
/// The last word of a live price, position or shares cell.
pub const VERSION: u32 = 1;

/// Domain tags, four ASCII characters little-endian: "lnop" (the operator's lock) and "lnow"
/// (a position owner's secret).
pub const TAG_OPERATOR: u32 = 0x706f_6e6c;
pub const TAG_OWNER: u32 = 0x776f_6e6c;

/// The public input: the operator's lock (eight words), then the collateral token.
pub const PUBLIC_LOCK: u32 = 0;
pub const PUBLIC_COLL: u32 = 8;
pub const PUBLIC_WORDS: u32 = 9;

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
    Price,
    Index,
    Pool,
    Shares,
    Debt,
    Unchanged,
    Health,
    Healthy,
    Seize,
}
use Refusal::*;

/// The pool, as its cell holds it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pool {
    pub cash: u64,
    pub sb: u64,
    pub s: u64,
    pub i: u64,
}

impl Pool {
    pub fn value(&self) -> [u32; 8] {
        let (a, b, c, d) = (words_of(self.cash), words_of(self.sb), words_of(self.s), words_of(self.i));
        [a.0, a.1, b.0, b.1, c.0, c.1, d.0, d.1]
    }

    /// `None` for an absent cell (or anything that is not a live pool).
    pub fn from_value(v: &[u32; 8]) -> Option<Pool> {
        let p = Pool { cash: u64_of(v[0], v[1]), sb: u64_of(v[2], v[3]), s: u64_of(v[4], v[5]), i: u64_of(v[6], v[7]) };
        if p.i == 0 { None } else { Some(p) }
    }
}

/// A borrower's position. Both zero is an absent cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Position {
    pub coll: u64,
    pub sdebt: u64,
}

impl Position {
    pub fn value(&self) -> [u32; 8] {
        if (self.coll | self.sdebt) == 0 {
            return [0; 8];
        }
        let (a, b) = (words_of(self.coll), words_of(self.sdebt));
        [a.0, a.1, b.0, b.1, 0, 0, 0, VERSION]
    }

    pub fn from_value(v: &[u32; 8]) -> Position {
        Position { coll: u64_of(v[0], v[1]), sdebt: u64_of(v[2], v[3]) }
    }
}

/// The price cell's value.
pub fn price_value(p: u64) -> [u32; 8] {
    let (lo, hi) = words_of(p);
    [lo, hi, 0, 0, 0, 0, 0, VERSION]
}

/// The shares cell's value.
pub fn shares_value(share: u32) -> [u32; 8] {
    [share, 0, 0, 0, 0, 0, 0, VERSION]
}

/// Accept or refuse the transition `s` shows. On acceptance, the receipt's eight output words:
/// `[method, a_lo, a_hi, b_lo, b_hi, 0, 0, 0]` — operate: price, index; supply: RAND in, shares
/// minted; withdraw: shares burned, RAND out; adjust: collateral, scaled debt after; liquidate:
/// RAND in, C seized.
pub fn check<S: Source>(s: &S) -> Result<[u32; 8], Refusal> {
    let h = Header::read(s).ok_or(Version)?;
    let (a, b) = match s.input(0) {
        OPERATE => operate(s, &h)?,
        SUPPLY => supply(s, &h)?,
        WITHDRAW => withdraw(s, &h)?,
        ADJUST => adjust(s, &h)?,
        LIQUIDATE => liquidate(s, &h)?,
        _ => return Err(Method),
    };
    let (a, b) = (words_of(a), words_of(b));
    Ok([s.input(0), a.0, a.1, b.0, b.1, 0, 0, 0])
}

// --- the predicates: the program checks them, the wallet searches them ----------------------------

/// `TA9 = cash · E + sb · I`: below 2^93 + 2^126 < 2^127 for amounts below 2^63.
pub fn ta9(p: &Pool) -> U256 {
    prod(p.cash, E).add(prod(p.sb, p.i))
}

/// Minting `m` shares for `a` RAND does not dilute: `m · TA9 ≤ a · s · E` (below 2^190 and 2^156),
/// against a pool that has shares and assets.
pub fn supply_ok(p: &Pool, a: u64, m: u64) -> bool {
    let ta = ta9(p);
    (p.s != 0) & !ta.le(U256::ZERO) & ta.mul(m).le(prod3(a, p.s, E))
}

/// Burning `b` shares for `x` RAND does not dilute: `x · s · E ≤ b · TA9` (below 2^156 and 2^190).
pub fn withdraw_ok(p: &Pool, b: u64, x: u64) -> bool {
    prod3(x, p.s, E).le(ta9(p).mul(b))
}

/// The new scaled debt covers what was borrowed less what was repaid, rounded for the pool:
/// `sd'·I + r·E ≥ sd·I + x·E` (each side below 2^127).
pub fn debt_covered(sdebt: u64, sdebt2: u64, i: u64, r: u64, x: u64) -> bool {
    prod(sdebt, i).add(prod(x, E)).le(prod(sdebt2, i).add(prod(r, E)))
}

/// Within the loan-to-value limit: `sd · I · 100 ≤ coll · P · 75` (each side below 2^133).
pub fn healthy(coll: u64, sdebt: u64, p: u64, i: u64) -> bool {
    prod3(sdebt, i, 100).le(prod3(coll, p, LTV_PCT))
}

/// Past the liquidation threshold: `sd · I · 100 > coll · P · 85` (each side below 2^133).
pub fn liquidatable(coll: u64, sdebt: u64, p: u64, i: u64) -> bool {
    prod3(coll, p, LIQ_PCT).lt(prod3(sdebt, i, 100))
}

/// Seizing `c` of C for clearing `cleared` scaled debt pays at most a 10 % bonus on the debt
/// cleared: `c · P · 100 ≤ cleared · I · 110` (each side below 2^133).
pub fn seize_ok(c: u64, p: u64, cleared: u64, i: u64) -> bool {
    prod3(c, p, 100).le(prod3(cleared, i, BONUS_PCT))
}

/// The index after accruing `rate` (parts per `E`, at most 1 %), rounded down:
/// `I'·E ≤ I·(E + rate) < I'·E + E` (below 2^94). Exact up to rounding, so `I'` is fixed by `I`
/// and `rate`; `I' ≤ 1.01 · I`.
pub fn accrue_ok(i: u64, i2: u64, rate: u64) -> bool {
    let target = prod(i, E.wrapping_add(rate));
    let low = prod(i2, E);
    (rate <= MAX_RATE) & (i != 0) & lt_note(i2) & low.le(target) & target.lt(low.add(U256::from(E)))
}

// --- reading cells ----------------------------------------------------------------------------------

/// The pool a cell holds, whatever it says.
#[inline(always)]
fn pool_at<S: Source>(s: &S, c: Cell) -> Pool {
    Pool { cash: c.val64(s, 0), sb: c.val64(s, 2), s: c.val64(s, 4), i: c.val64(s, 6) }
}

/// A read pool cell: live (`I ≠ 0`) and every amount in range.
fn live_pool<S: Source>(s: &S, c: Cell) -> Result<Pool, Refusal> {
    let p = pool_at(s, c);
    if p.i == 0 {
        return Err(Pool);
    }
    in_range(&p)?;
    Ok(p)
}

fn in_range(p: &Pool) -> Result<(), Refusal> {
    if lt_note(p.cash) & lt_note(p.sb) & lt_note(p.s) & lt_note(p.i) { Ok(()) } else { Err(Range) }
}

/// The pool read at `r` and written at `w`, under `POOL_KEY`; the written one in range.
fn pools<S: Source>(s: &S, r: Cell, w: Cell) -> Result<(Pool, Pool), Refusal> {
    if !r.key_is(s, &POOL_KEY) | !w.key_is(s, &POOL_KEY) {
        return Err(Key);
    }
    let before = live_pool(s, r)?;
    let after = pool_at(s, w);
    in_range(&after)?;
    Ok((before, after))
}

/// A cell read at `r` and written back unchanged at `w`, under `key`, holding a value of the
/// form `make(x)` for `x` = its first two words. Returns `x`.
fn unchanged<S: Source>(s: &S, r: Cell, w: Cell, key: &[u32; 8], make: fn(u64) -> [u32; 8]) -> Result<u64, Refusal> {
    if !r.key_is(s, key) | !w.key_is(s, key) {
        return Err(Key);
    }
    let x = r.val64(s, 0);
    let v = make(x);
    if !r.value_is(s, &v) | !w.value_is(s, &v) {
        return Err(Value);
    }
    Ok(x)
}

/// The price cell, read and written back unchanged: `P > 0`.
fn price<S: Source>(s: &S, r: Cell, w: Cell) -> Result<u64, Refusal> {
    let p = unchanged(s, r, w, &PRICE_KEY, price_value)?;
    if (p == 0) | !lt_note(p) {
        return Err(Price);
    }
    Ok(p)
}

/// The shares cell, read and written back unchanged: the share token.
fn share_token<S: Source>(s: &S, r: Cell, w: Cell) -> Result<u32, Refusal> {
    let x = unchanged(s, r, w, &SHARES_KEY, |x| shares_value(x as u32))?;
    // `shares_value` keeps only the low word: a high word that is not zero fails `value_is`.
    let share = x as u32;
    if share == RAND {
        return Err(Shares);
    }
    Ok(share)
}

/// A read position: absent (all zeros) or live (`VERSION`, words 4..7 zero, not both amounts
/// zero — a live cell with nothing in it is never written).
fn read_position<S: Source>(s: &S, c: Cell) -> Result<Position, Refusal> {
    let p = Position { coll: c.val64(s, 0), sdebt: c.val64(s, 2) };
    if c.is_zero(s) {
        return Ok(p);
    }
    if !c.value_is(s, &p.value()) | ((p.coll | p.sdebt) == 0) {
        return Err(Value);
    }
    if !lt_note(p.coll) | !lt_note(p.sdebt) {
        return Err(Range);
    }
    Ok(p)
}

/// A written position: exactly `Position::value` of its amounts — zeros (deleted) when both are.
fn written_position<S: Source>(s: &S, c: Cell) -> Result<Position, Refusal> {
    let p = Position { coll: c.val64(s, 0), sdebt: c.val64(s, 2) };
    if !c.value_is(s, &p.value()) {
        return Err(Value);
    }
    if !lt_note(p.coll) | !lt_note(p.sdebt) {
        return Err(Range);
    }
    Ok(p)
}

/// `a + b == c + d`, all four amounts, neither sum past 2^63.
#[inline(always)]
fn sums_equal(a: u64, b: u64, c: u64, d: u64) -> bool {
    match (add_note(a, b), add_note(c, d)) {
        (Some(x), Some(y)) => x == y,
        _ => false,
    }
}

// --- the methods ----------------------------------------------------------------------------------

/// The operator sets up the market, or moves the price and accrues interest. Private inputs
/// `[1, s0..s7, p_lo, p_hi, x]`: the secret, the price to set, and the share token (init) or the
/// rate (update).
fn operate<S: Source>(s: &S, h: &Header) -> Result<(u64, u64), Refusal> {
    if !opens_lock(s, TAG_OPERATOR, 1, PUBLIC_LOCK) {
        return Err(Operator);
    }
    if !h.nothing_in() {
        return Err(Inflow);
    }
    let p = u64_of(s.input(9), s.input(10));
    let x = s.input(11);
    if (p == 0) | !lt_note(p) {
        return Err(Price);
    }
    // Every operate reads the pool and writes the price without reading it: the old price is of
    // no consequence, and a word read but unused would be a word anyone could change.
    let r = h.read_cell(0);
    let wp = h.write_cell(0);
    if !r.key_is(s, &POOL_KEY) | !wp.key_is(s, &PRICE_KEY) {
        return Err(Key);
    }
    if !wp.value_is(s, &price_value(p)) {
        return Err(Value);
    }
    let wl = h.write_cell(1);
    if !wl.key_is(s, &POOL_KEY) {
        return Err(Key);
    }
    if r.is_zero(s) {
        // Init: the pool does not exist (and, since a pool is never deleted, never has).
        if !h.shape(1, 3, 0, 0) {
            return Err(Shape);
        }
        let (share, c) = (x, s.public(PUBLIC_COLL));
        if (c == RAND) | (share == RAND) | (share == c) {
            return Err(Asset);
        }
        let ws = h.write_cell(2);
        if !ws.key_is(s, &SHARES_KEY) {
            return Err(Key);
        }
        let fresh = Pool { cash: 0, sb: 0, s: 0, i: E };
        if !wl.value_is(s, &fresh.value()) | !ws.value_is(s, &shares_value(share)) {
            return Err(Value);
        }
        return Ok((p, E));
    }
    if !h.shape(1, 2, 0, 0) {
        return Err(Shape);
    }
    let before = live_pool(s, r)?;
    let after = pool_at(s, wl);
    if (after.cash != before.cash) | (after.sb != before.sb) | (after.s != before.s) {
        return Err(Value);
    }
    if !accrue_ok(before.i, after.i, x as u64) {
        return Err(Index);
    }
    Ok((p, after.i))
}

/// Supply `a` RAND (through `burn_r`) for `m` newly minted shares.
fn supply<S: Source>(s: &S, h: &Header) -> Result<(u64, u64), Refusal> {
    if !h.shape(2, 2, 0, 1) {
        return Err(Shape);
    }
    let a = h.burn_r;
    if !h.no_token_in() {
        return Err(Inflow);
    }
    let (before, after) = pools(s, h.read_cell(0), h.write_cell(0))?;
    let share = share_token(s, h.read_cell(1), h.write_cell(1))?;
    let (asset, m) = h.mint(s, 0);
    if asset != share {
        return Err(Asset);
    }
    if (a == 0) | (m == 0) {
        return Err(Zero);
    }
    if !lt_note(a) | !lt_note(m) {
        return Err(Range);
    }
    if (after.sb != before.sb) | (after.i != before.i) | (Some(after.cash) != add_note(before.cash, a)) {
        return Err(Value);
    }
    if before.s == 0 {
        // The first supply: one share per unit, less the locked minimum.
        if (Some(a) != add_note(m, MIN_SHARES)) | (after.s != a) {
            return Err(Price);
        }
    } else {
        if Some(after.s) != add_note(before.s, m) {
            return Err(Value);
        }
        if !supply_ok(&before, a, m) {
            return Err(Price);
        }
    }
    Ok((a, m))
}

/// Burn `b` shares for `x` RAND.
fn withdraw<S: Source>(s: &S, h: &Header) -> Result<(u64, u64), Refusal> {
    if !h.shape(2, 2, 1, 0) {
        return Err(Shape);
    }
    let (before, after) = pools(s, h.read_cell(0), h.write_cell(0))?;
    let share = share_token(s, h.read_cell(1), h.write_cell(1))?;
    if (h.burn_r != 0) | (h.inflow != INFLOW_BURN) | (h.burn_asset != share) {
        return Err(Inflow);
    }
    let b = h.burn_a;
    let (asset, x) = h.pay(s, 0);
    if asset != RAND {
        return Err(Asset);
    }
    if (b == 0) | (x == 0) {
        return Err(Zero);
    }
    let cash = sub_note(before.cash, x).ok_or(Pool)?;
    let supply = sub_note(before.s, b).ok_or(Pool)?;
    if (after.cash != cash) | (after.s != supply) | (after.sb != before.sb) | (after.i != before.i) {
        return Err(Value);
    }
    if !withdraw_ok(&before, b, x) {
        return Err(Price);
    }
    Ok((b, x))
}

/// A borrower moves their own position: collateral in (a deposit of C) or out (a payout of C),
/// RAND borrowed (a payout) or repaid (`burn_r`). At most one payout.
fn adjust<S: Source>(s: &S, h: &Header) -> Result<(u64, u64), Refusal> {
    if (h.n_reads != 3) | (h.n_writes != 3) | (h.n_mints != 0) | (h.n_pays > 1) {
        return Err(Shape);
    }
    let c_asset = s.public(PUBLIC_COLL);
    let p = price(s, h.read_cell(0), h.write_cell(0))?;
    let (before, after) = pools(s, h.read_cell(1), h.write_cell(1))?;
    let (ro, wo) = (h.read_cell(2), h.write_cell(2));
    // Only the secret's holder names this position.
    let key = owned_key(POSITION_TAG, &kit::digest(s, TAG_OWNER, &secret_at(s, 1)));
    if !ro.key_is(s, &key) | !wo.key_is(s, &key) {
        return Err(Key);
    }
    let pos = read_position(s, ro)?;
    let pos2 = written_position(s, wo)?;

    let r = h.burn_r;
    let dc = if h.no_token_in() {
        0
    } else if (h.inflow == INFLOW_DEPOSIT) & (h.burn_asset == c_asset) & (h.burn_a != 0) & (c_asset != RAND) {
        h.burn_a
    } else {
        return Err(Inflow);
    };
    let (x, c_out) = if h.n_pays == 0 {
        (0, 0)
    } else {
        let (asset, v) = h.pay(s, 0);
        if v == 0 {
            return Err(Zero);
        }
        if asset == RAND {
            (v, 0)
        } else if (asset == c_asset) & (c_asset != RAND) {
            (0, v)
        } else {
            return Err(Asset);
        }
    };
    if !lt_note(r) | !lt_note(dc) | !lt_note(x) | !lt_note(c_out) {
        return Err(Range);
    }
    // Collateral and cash move by exactly what came in and went out; the pool's scaled borrows by
    // exactly the position's.
    if !sums_equal(pos2.coll, c_out, pos.coll, dc) {
        return Err(Value);
    }
    if !sums_equal(after.cash, x, before.cash, r) | !sums_equal(after.sb, pos.sdebt, before.sb, pos2.sdebt) {
        return Err(Value);
    }
    if (after.s != before.s) | (after.i != before.i) {
        return Err(Value);
    }
    if (pos2.coll == pos.coll) & (pos2.sdebt == pos.sdebt) {
        return Err(Unchanged);
    }
    if !debt_covered(pos.sdebt, pos2.sdebt, before.i, r, x) {
        return Err(Debt);
    }
    // Riskier — more debt or less collateral — must end within the loan-to-value limit.
    let riskier = (pos2.sdebt > pos.sdebt) | (pos2.coll < pos.coll);
    if riskier & !healthy(pos2.coll, pos2.sdebt, p, before.i) {
        return Err(Health);
    }
    Ok((pos2.coll, pos2.sdebt))
}

/// Anyone repays part of a position past the threshold and takes its collateral at a 10 % bonus.
fn liquidate<S: Source>(s: &S, h: &Header) -> Result<(u64, u64), Refusal> {
    if !h.shape(3, 3, 1, 0) {
        return Err(Shape);
    }
    let c_asset = s.public(PUBLIC_COLL);
    let p = price(s, h.read_cell(0), h.write_cell(0))?;
    let (before, after) = pools(s, h.read_cell(1), h.write_cell(1))?;
    let (ro, wo) = (h.read_cell(2), h.write_cell(2));
    let key = ro.keys(s);
    if (key[0] != POSITION_TAG) | !wo.key_is(s, &key) {
        return Err(Key);
    }
    let pos = read_position(s, ro)?;
    let pos2 = written_position(s, wo)?;
    let r = h.burn_r;
    if !h.no_token_in() {
        return Err(Inflow);
    }
    let (asset, c) = h.pay(s, 0);
    if (asset != c_asset) | (c_asset == RAND) {
        return Err(Asset);
    }
    if (r == 0) | (c == 0) {
        return Err(Zero);
    }
    if !lt_note(r) | !lt_note(c) {
        return Err(Range);
    }
    if !liquidatable(pos.coll, pos.sdebt, p, before.i) {
        return Err(Healthy);
    }
    let cleared = sub_note(pos.sdebt, pos2.sdebt).ok_or(Debt)?;
    if Some(pos2.coll) != sub_note(pos.coll, c) {
        return Err(Value);
    }
    if (Some(after.cash) != add_note(before.cash, r)) | (Some(after.sb) != sub_note(before.sb, cleared)) {
        return Err(Value);
    }
    if (after.s != before.s) | (after.i != before.i) {
        return Err(Value);
    }
    if !debt_covered(pos.sdebt, pos2.sdebt, before.i, r, 0) {
        return Err(Debt);
    }
    if !seize_ok(c, p, cleared, before.i) {
        return Err(Seize);
    }
    Ok((r, c))
}

#[cfg(not(target_arch = "riscv32"))]
pub mod plan;
