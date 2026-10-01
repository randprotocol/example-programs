//! stablecoin: debt positions backed by RAND, minting the program's own stable token
//! (MakerDAO's vaults, Liquity's troves).
//!
//! An owner locks RAND in the program's vault and borrows the stable token against it, up to a
//! minimum collateral ratio of 150 %; repaying burns the token. A position below 110 % can be
//! liquidated by anyone who burns its whole debt, for all of its collateral. The price — stable
//! base units per whole RAND — is set by the **operator**, whose lock (eight words) is the
//! program's deploy-time public input. The stable token is an RPL token registered with
//! `rand token create --program <id>`; the operator binds its asset index once, at the first
//! price.
//!
//! ```text
//! config    key [1, 0, 0, 0, 0, 0, 0, 0]
//!           value [p_lo, p_hi, stable, 0, 0, 0, 0, 1]          price, stable asset, version
//! position  key [2, d0, …, d6]                                 d = POSEIDON2([TAG_OWNER, secret])
//!           value [c_lo, c_hi, d_lo, d_hi, 0, 0, 0, 1]          collateral (RAND), debt (stable)
//!           (absent — all zeros — when both are zero)
//! ```
//!
//! | method | private inputs | transition | rule |
//! |---|---|---|---|
//! | 1 operate | `[1, s0..s7, stable, old_lo, old_hi, new_lo, new_hi]` | config → config; nothing in or out | the operator's secret opens the lock; first: config absent, `stable ≠ RAND`; then: stable unchanged; price > 0 |
//! | 2 adjust | `[2, s0..s7]` | config → config (unchanged), position → position; RAND in, ≤ 1 RAND pay, ≤ 1 stable mint, stable burned or nothing | `c' = c + in − out`, `d' = d + minted − burned`; something changes; if `d'` rose or `c'` fell: `c'·P·100 ≥ d'·10⁹·150` |
//! | 3 liquidate | `[3]` | config → config (unchanged), position → nothing; burn exactly `d`, pay exactly `c` RAND | `c·P·100 < d·10⁹·110`, `c, d > 0` |
//!
//! The written position is always exactly the read one moved by what came in and went out, and
//! the config is written back exactly as read (so that every word of it is pinned by an equality,
//! as well as by the chain's read check). The operator's whole instruction — the stable asset,
//! the price it replaces and the new price — is a private input the transition must match word
//! for word. The program never divides: the wallet finds the largest mint or withdrawal by
//! searching [`healthy`], the very predicate the program checks (`plan`).
#![cfg_attr(target_arch = "riscv32", no_std)]
#![forbid(unsafe_code)]

pub use rpl2_kit as kit;
use kit::{
    add_note, digest, lt_note, opens_lock, owned_key, prod3, secret_at, sub_note, u64_of, words_of, Header, Source,
    INFLOW_BURN, INFLOW_NONE, RAND,
};

pub const OPERATE: u32 = 1;
pub const ADJUST: u32 = 2;
pub const LIQUIDATE: u32 = 3;

/// The config cell.
pub const CONFIG_KEY: [u32; 8] = [1, 0, 0, 0, 0, 0, 0, 0];
/// A position's key starts with this word; the next seven are its owner's digest.
pub const POSITION_TAG: u32 = 2;
/// The last word of a live cell's value: a live cell is never all zeros.
pub const VERSION: u32 = 1;

/// "scop", little-endian: the domain tag of the operator's lock.
pub const TAG_OPERATOR: u32 = 0x706f_6373;
/// "scow", little-endian: the domain tag of a position owner's digest.
pub const TAG_OWNER: u32 = 0x776f_6373;

/// RAND base units per RAND: the price is quoted per whole RAND.
pub const ONE_RAND: u64 = 1_000_000_000;
/// Borrowing or withdrawing must leave the position at or above 150 %.
pub const MCR_PCT: u64 = 150;
/// Below 110 %, anyone may liquidate.
pub const LIQUIDATION_PCT: u64 = 110;

/// The public input: the operator's lock, `POSEIDON2([TAG_OPERATOR, s0..s7])`.
pub const PUBLIC_LOCK: u32 = 0;
pub const PUBLIC_WORDS: u32 = 8;
/// Where a secret starts among the private inputs (after the method).
pub const SECRET_AT: u32 = 1;
/// operate's instruction, after the operator's secret: stable asset, old price, new price.
const OP_STABLE: u32 = 9;
const OP_OLD: u32 = 10;
const OP_NEW: u32 = 12;

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
    /// The operator's secret does not open the lock.
    Operator,
    /// The secret does not name the position declared.
    Owner,
    /// The config is absent or malformed, or an update would rebind the stable token.
    Config,
    /// The operator's instruction does not match the transition.
    Instruction,
    /// An adjustment that changes nothing.
    Unchanged,
    /// More repaid than owed, or more withdrawn than held.
    Underflow,
    /// A riskier position below the minimum collateral ratio.
    Unhealthy,
    /// A liquidation of a position at or above 110 %.
    Healthy,
}
use Refusal::*;

/// The config, as its cell holds it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Config {
    /// Stable base units per whole RAND (10⁹ RAND units).
    pub price: u64,
    pub stable: u32,
}

impl Config {
    pub fn value(&self) -> [u32; 8] {
        let p = words_of(self.price);
        [p.0, p.1, self.stable, 0, 0, 0, 0, VERSION]
    }

    /// `None` for an absent cell (or anything that is not a live config).
    pub fn from_value(v: &[u32; 8]) -> Option<Config> {
        let c = Config { price: u64_of(v[0], v[1]), stable: v[2] };
        if (c.value() != *v) | (c.price == 0) | !lt_note(c.price) | (c.stable == RAND) {
            return None;
        }
        Some(c)
    }
}

/// A position, as its cell holds it. Both zero is the absent cell.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Position {
    /// RAND base units locked.
    pub coll: u64,
    /// Stable base units owed.
    pub debt: u64,
}

impl Position {
    /// The cell's value: all zeros (the cell deleted) when the position is empty.
    pub fn value(&self) -> [u32; 8] {
        let (c, d) = (words_of(self.coll), words_of(self.debt));
        let live = ((self.coll | self.debt) != 0) as u32;
        [c.0, c.1, d.0, d.1, 0, 0, 0, live * VERSION]
    }

    /// `None` for anything that is neither absent nor a live position.
    pub fn from_value(v: &[u32; 8]) -> Option<Position> {
        let p = Position { coll: u64_of(v[0], v[1]), debt: u64_of(v[2], v[3]) };
        if (p.value() != *v) | !lt_note(p.coll) | !lt_note(p.debt) {
            return None;
        }
        Some(p)
    }
}

/// `coll · P · 100 ≥ debt · 10⁹ · ratio`: the collateral's value is at least `ratio` % of the
/// debt. Every amount is below 2^63, so the left side is below 2^63 · 2^63 · 2^7 = 2^133 and the
/// right below 2^63 · 2^30 · 2^8 = 2^101: both far inside 256 bits.
pub fn healthy(coll: u64, debt: u64, price: u64, ratio_pct: u64) -> bool {
    prod3(debt, ONE_RAND, ratio_pct).le(prod3(coll, price, 100))
}

/// The position key a secret's holder owns.
#[inline(always)]
pub fn position_key(d: &[u32; 8]) -> [u32; 8] {
    owned_key(POSITION_TAG, d)
}

/// Accept or refuse the transition `s` shows. On acceptance, the receipt's eight output words:
/// operate `[1, p_lo, p_hi, stable, 0, 0, 0, 0]`; adjust `[2, c'_lo, c'_hi, d'_lo, d'_hi, 0, 0, 0]`;
/// liquidate `[3, c_lo, c_hi, d_lo, d_hi, 0, 0, 0]` (collateral paid, debt burned).
pub fn check<S: Source>(s: &S) -> Result<[u32; 8], Refusal> {
    let h = Header::read(s).ok_or(Version)?;
    let (a, b, c) = match s.input(0) {
        OPERATE => operate(s, &h)?,
        ADJUST => adjust(s, &h)?,
        LIQUIDATE => liquidate(s, &h)?,
        _ => return Err(Method),
    };
    let (a, b) = (words_of(a), words_of(b));
    Ok([s.input(0), a.0, a.1, b.0, b.1, c, 0, 0])
}

/// The config a cell of the context holds, if it is a live one: every word pinned (the value is
/// exactly the one its price and asset make), a price in `1..2^63`, a stable asset that is not RAND.
#[inline(always)]
fn config_at<S: Source>(s: &S, c: kit::Cell) -> Result<Config, Refusal> {
    let cfg = Config { price: c.val64(s, 0), stable: c.val(s, 2) };
    if !c.value_is(s, &cfg.value()) | (cfg.price == 0) | !lt_note(cfg.price) | (cfg.stable == RAND) {
        return Err(Config);
    }
    Ok(cfg)
}

/// Read 0 and write 0 are the config, live, written back exactly as read.
#[inline(never)]
fn config_kept<S: Source>(s: &S, h: &Header) -> Result<Config, Refusal> {
    let (r, w) = (h.read_cell(0), h.write_cell(0));
    if !r.key_is(s, &CONFIG_KEY) | !w.key_is(s, &CONFIG_KEY) {
        return Err(Key);
    }
    let cfg = config_at(s, r)?;
    if !w.value_is(s, &cfg.value()) {
        return Err(Value);
    }
    Ok(cfg)
}

/// The operator sets the price; the first time, it also binds the stable token.
fn operate<S: Source>(s: &S, h: &Header) -> Result<(u64, u64, u32), Refusal> {
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
    if !r.key_is(s, &CONFIG_KEY) | !w.key_is(s, &CONFIG_KEY) {
        return Err(Key);
    }
    let stable = s.input(OP_STABLE);
    let old = u64_of(s.input(OP_OLD), s.input(OP_OLD + 1));
    let new = u64_of(s.input(OP_NEW), s.input(OP_NEW + 1));
    if old == 0 {
        // The first price: the config is absent, and the stable token is bound now. It must be
        // minted and burned by this program, so the chain enforces that it is the program's own
        // at every mint; here it only must not be RAND.
        if !r.is_zero(s) {
            return Err(Instruction);
        }
    } else {
        // An update: the config is the one the instruction names — same token, the old price.
        if !r.value_is(s, &Config { price: old, stable }.value()) {
            return Err(Instruction);
        }
    }
    let after = Config { price: new, stable };
    if (new == 0) | !lt_note(new) | (stable == RAND) {
        return Err(Config);
    }
    if !w.value_is(s, &after.value()) {
        return Err(Instruction);
    }
    Ok((new, 0, stable))
}

/// An owner moves collateral and debt: RAND in, RAND out, stable minted, stable burned.
fn adjust<S: Source>(s: &S, h: &Header) -> Result<(u64, u64, u32), Refusal> {
    if (h.n_reads != 2) | (h.n_writes != 2) | (h.n_pays > 1) | (h.n_mints > 1) {
        return Err(Shape);
    }
    let cfg = config_kept(s, h)?;
    // The position is the one the secret names, read and written.
    let key = position_key(&digest(s, TAG_OWNER, &secret_at(s, SECRET_AT)));
    let (r, w) = (h.read_cell(1), h.write_cell(1));
    if !r.key_is(s, &key) | !w.key_is(s, &key) {
        return Err(Owner);
    }
    let before = Position { coll: r.val64(s, 0), debt: r.val64(s, 2) };
    if !r.value_is(s, &before.value()) {
        return Err(Value);
    }
    // What came in: RAND (any amount, zero included) and the stable token burned, or nothing.
    let added = h.burn_r;
    let repaid = if h.inflow == INFLOW_NONE {
        if (h.burn_asset != 0) | (h.burn_a != 0) {
            return Err(Inflow);
        }
        0
    } else if (h.inflow == INFLOW_BURN) & (h.burn_asset == cfg.stable) & (h.burn_a != 0) {
        h.burn_a
    } else {
        return Err(Inflow);
    };
    // What goes out: at most one RAND payout, at most one mint of the stable token.
    let withdrawn = if h.n_pays == 1 {
        let (asset, x) = h.pay(s, 0);
        if asset != RAND {
            return Err(Asset);
        }
        if x == 0 {
            return Err(Zero);
        }
        x
    } else {
        0
    };
    let minted = if h.n_mints == 1 {
        let (asset, x) = h.mint(s, 0);
        if asset != cfg.stable {
            return Err(Asset);
        }
        if x == 0 {
            return Err(Zero);
        }
        x
    } else {
        0
    };
    let coll = sub_note(add_note(before.coll, added).ok_or(Range)?, withdrawn).ok_or(Underflow)?;
    let debt = sub_note(add_note(before.debt, minted).ok_or(Range)?, repaid).ok_or(Underflow)?;
    if (coll == before.coll) & (debt == before.debt) {
        return Err(Unchanged);
    }
    // Borrowing more or taking collateral out must leave the position at 150 % or above;
    // repaying or adding collateral is always allowed, even to a position still under water.
    let riskier = (debt > before.debt) | (coll < before.coll);
    if riskier & !healthy(coll, debt, cfg.price, MCR_PCT) {
        return Err(Unhealthy);
    }
    let after = Position { coll, debt };
    if !w.value_is(s, &after.value()) {
        return Err(Value);
    }
    Ok((coll, debt, 0))
}

/// Anyone repays a position below 110 % in full and takes all of its collateral.
fn liquidate<S: Source>(s: &S, h: &Header) -> Result<(u64, u64, u32), Refusal> {
    if !h.shape(2, 2, 1, 0) {
        return Err(Shape);
    }
    let cfg = config_kept(s, h)?;
    let (r, w) = (h.read_cell(1), h.write_cell(1));
    // Any position: its key's first word is the tag, the rest is whoever's it is; the write is
    // the same cell.
    if (r.key(s, 0) != POSITION_TAG) | !w.key_is(s, &r.keys(s)) {
        return Err(Key);
    }
    let p = Position { coll: r.val64(s, 0), debt: r.val64(s, 2) };
    if !r.value_is(s, &p.value()) | !w.is_zero(s) {
        return Err(Value);
    }
    if (p.coll == 0) | (p.debt == 0) {
        return Err(Zero);
    }
    if !lt_note(p.coll) | !lt_note(p.debt) {
        return Err(Range);
    }
    // The whole debt burned; nothing else comes in.
    if (h.burn_r != 0) | (h.inflow != INFLOW_BURN) | (h.burn_asset != cfg.stable) | (h.burn_a != p.debt) {
        return Err(Inflow);
    }
    // The whole collateral paid out.
    let (asset, x) = h.pay(s, 0);
    if (asset != RAND) | (x != p.coll) {
        return Err(Asset);
    }
    if healthy(p.coll, p.debt, cfg.price, LIQUIDATION_PCT) {
        return Err(Healthy);
    }
    Ok((p.coll, p.debt, 0))
}

#[cfg(not(target_arch = "riscv32"))]
pub mod plan;
