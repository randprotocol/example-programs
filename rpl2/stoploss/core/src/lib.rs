//! stoploss: a conditional order whose trigger price is hidden while it rests — a stop-loss
//! nobody can hunt.
//!
//! An owner escrows RAND in the program's vault together with a **commitment** to a trigger: its
//! kind (a *stop* fires when the price is at or below a threshold, a *take-profit* at or above),
//! the threshold, and a salt. The price is a public oracle reading set by the **operator**, whose
//! lock (eight words) is the program's deploy-time public input. To fire, whoever holds the
//! ticket and the opening proves, inside the call proof, that the opening matches the commitment
//! and that the oracle's price satisfies it, and the escrow is paid out. The threshold, the kind
//! and the salt never appear on chain — not while the order rests, and not when it fires: the
//! receipt says only that the condition held. A firing attempt whose condition does not hold is
//! refused, and a refusal is no proof at all, so nobody can even tell it was tried.
//!
//! ```text
//! oracle  key   [1, 0, 0, 0, 0, 0, 0, 0]
//!         value [p_lo, p_hi, 0, 0, 0, 0, 0, 1]                  price, version
//! order   key   [2, d0, …, d6]                                   d = POSEIDON2([TAG_TICKET, ticket])
//!         value [a_lo, a_hi, c0, c1, c2, c3, c4, 1]              escrow (RAND), commitment, version
//!         c = POSEIDON2([TAG_TRIGGER, kind, t_lo, t_hi, salt0, salt1, salt2, salt3, 0])[0..5]
//! ```
//!
//! | method | private inputs | transition | rule |
//! |---|---|---|---|
//! | 1 operate | `[1, s0..s7, old_lo, old_hi, new_lo, new_hi]` | oracle → oracle; nothing in or out | the secret opens the lock; the oracle read is absent (`old = 0`) or holds exactly `old`; `0 < new < 2^63` |
//! | 2 place | `[2, ticket, kind, t_lo, t_hi, salt]` | order absent → order; RAND in (`burn_r`) | the key is the ticket's; `amount = burn_r > 0`; `kind ∈ {1, 2}`; `c` is the opening's commitment |
//! | 3 fire | `[3, ticket, kind, t_lo, t_hi, salt]` | oracle → oracle (unchanged), order → absent; pay `amount` RAND | the key is the ticket's; `c` is the opening's; stop: `price ≤ t`, take-profit: `price ≥ t` |
//! | 4 cancel | `[4, ticket, kind, t_lo, t_hi, salt]` | order → absent; pay `amount` RAND | the key is the ticket's; `c` is the opening's; no condition |
//!
//! `ticket` is eight random words the owner keeps: their digest names the order's cell, so only
//! the ticket's holder can fire or cancel it. The opening — `kind`, the threshold and the salt —
//! is kept beside it. `fire` and `cancel` take the opening as well as the ticket so that every
//! word of the order they read is pinned by an equality (the commitment words would otherwise be
//! words the program is shown and does not check). The operator's whole instruction — the price
//! it replaces and the new price — is likewise a private input the transition must match word
//! for word, and `fire` writes the oracle back exactly as read, so its words are pinned too (the
//! rewrite is free, and does not contend: two fires in one block leave the same value).
//!
//! What "fires" here is the release of the escrow to whoever the firing transaction names — the
//! program decides amounts, never recipients. A real stop-loss would hand the escrow to a venue
//! (the amm example's pool, say) in the same transition: one more read and write of the pool
//! cell, 32 context words, which still fits the segment (fire is 78 context words of the 111 a
//! program with an eight-word public input may use).
#![cfg_attr(target_arch = "riscv32", no_std)]
#![forbid(unsafe_code)]

pub use rpl2_kit as kit;
use kit::{digest, lt_note, opens_lock, owned_key, secret_at, u64_of, words_of, Header, Source, RAND};

pub const OPERATE: u32 = 1;
pub const PLACE: u32 = 2;
pub const FIRE: u32 = 3;
pub const CANCEL: u32 = 4;

/// A stop fires when the price is at or below its threshold.
pub const STOP: u32 = 1;
/// A take-profit fires when the price is at or above its threshold.
pub const TAKE_PROFIT: u32 = 2;

/// The oracle's cell.
pub const ORACLE_KEY: [u32; 8] = [1, 0, 0, 0, 0, 0, 0, 0];
/// An order's key starts with this word; the next seven are its ticket's digest.
pub const ORDER_TAG: u32 = 2;
/// The last word of a live cell's value: a live cell is never all zeros.
pub const VERSION: u32 = 1;

/// "slop", little-endian: the domain tag of the operator's lock.
pub const TAG_OPERATOR: u32 = 0x706f_6c73;
/// "sltk", little-endian: the domain tag of a ticket's digest (an order's key).
pub const TAG_TICKET: u32 = 0x6b74_6c73;
/// "sltr", little-endian: the domain tag of a trigger's commitment.
pub const TAG_TRIGGER: u32 = 0x7274_6c73;

/// The public input: the operator's lock, `POSEIDON2([TAG_OPERATOR, s0..s7])`.
pub const PUBLIC_LOCK: u32 = 0;
pub const PUBLIC_WORDS: u32 = 8;
/// Where the eight-word secret starts among the private inputs (after the method): the
/// operator's secret for operate, the ticket for place, fire and cancel.
pub const SECRET_AT: u32 = 1;
/// operate's instruction, after the secret: the price replaced (0: none yet), the new price.
const OP_OLD: u32 = 9;
const OP_NEW: u32 = 11;
/// The opening, after the ticket: kind, threshold (lo, hi), salt (four words).
pub const KIND_AT: u32 = 9;
pub const THRESHOLD_AT: u32 = 10;
pub const SALT_AT: u32 = 12;
/// place, fire and cancel take 16 private inputs: method, ticket (8), kind, threshold (2), salt (4).
pub const OWNER_INPUTS: usize = 16;

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
    /// The operator's instruction does not match the oracle read or written.
    Instruction,
    /// The oracle is absent or malformed.
    NoOracle,
    /// The order is absent or malformed.
    NoOrder,
    /// A new order over a cell that is not empty.
    Occupied,
    /// Neither a stop nor a take-profit.
    Kind,
    /// The opening is not the one the order was placed with.
    Commitment,
    /// The payout is not the whole escrow.
    Amount,
    /// The price does not meet the trigger.
    Condition,
}
use Refusal::*;

/// The oracle's reading, as its cell holds it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Oracle {
    /// The price, in whatever unit the operator and the owners agree on. Never zero.
    pub price: u64,
}

impl Oracle {
    pub fn value(&self) -> [u32; 8] {
        let p = words_of(self.price);
        [p.0, p.1, 0, 0, 0, 0, 0, VERSION]
    }

    /// `None` for an absent cell (or anything that is not a live reading).
    pub fn from_value(v: &[u32; 8]) -> Option<Oracle> {
        let o = Oracle { price: u64_of(v[0], v[1]) };
        if (o.value() != *v) | (o.price == 0) | !lt_note(o.price) {
            return None;
        }
        Some(o)
    }
}

/// An order, as its cell holds it: the escrow and the trigger's commitment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Order {
    /// RAND base units in escrow. Never zero.
    pub amount: u64,
    /// The first five words of the trigger's hash.
    pub c: [u32; 5],
}

impl Order {
    pub fn value(&self) -> [u32; 8] {
        let a = words_of(self.amount);
        [a.0, a.1, self.c[0], self.c[1], self.c[2], self.c[3], self.c[4], VERSION]
    }

    /// `None` for an absent cell (or anything that is not a live order).
    pub fn from_value(v: &[u32; 8]) -> Option<Order> {
        let o = Order { amount: u64_of(v[0], v[1]), c: [v[2], v[3], v[4], v[5], v[6]] };
        if (o.value() != *v) | (o.amount == 0) | !lt_note(o.amount) {
            return None;
        }
        Some(o)
    }
}

/// A trigger, as its owner knows it: what the cell's commitment hides.
///
/// **The commitment is five words (160 bits) of `POSEIDON2` over the opening, and the opening
/// carries four random words (128 bits) of salt.** Five words because the cell has room for
/// exactly that beside the escrow and the version — and 160 bits are binding enough: once an
/// order is placed, firing it with any other threshold means finding a second preimage of a
/// 160-bit value, work no one can do. The salt is what hides the threshold: the kind is one bit
/// and a threshold is a plausible price, a few dozen bits of entropy at most, so without a salt
/// (or with one word of it) a hunter could hash every plausible opening and read the stop off the
/// chain. With 128 bits of salt that search is out of reach for good.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Opening {
    pub kind: u32,
    pub threshold: u64,
    pub salt: [u32; 4],
}

impl Opening {
    /// The commitment's preimage: nine words with a domain tag first, as the sponge takes them
    /// (guest-sdk's sponge does not pad, so the message is always exactly nine words).
    pub fn message(&self) -> [u32; 9] {
        let t = words_of(self.threshold);
        [TAG_TRIGGER, self.kind, t.0, t.1, self.salt[0], self.salt[1], self.salt[2], self.salt[3], 0]
    }

    /// The opening at private inputs `KIND_AT..`.
    #[inline(always)]
    pub fn read<S: Source>(s: &S) -> Opening {
        Opening {
            kind: s.input(KIND_AT),
            threshold: u64_of(s.input(THRESHOLD_AT), s.input(THRESHOLD_AT + 1)),
            salt: [s.input(SALT_AT), s.input(SALT_AT + 1), s.input(SALT_AT + 2), s.input(SALT_AT + 3)],
        }
    }
}

/// The commitment an opening makes: the first five words of its hash.
#[inline(always)]
pub fn commitment(o: &Opening, hash9: impl FnOnce([u32; 9]) -> [u32; 8]) -> [u32; 5] {
    let h = hash9(o.message());
    [h[0], h[1], h[2], h[3], h[4]]
}

/// The trigger: a stop fires at or below its threshold, a take-profit at or above. Any other
/// kind never fires. The wallet (`plan`) asks this same predicate before it sends anything.
pub fn fires(kind: u32, price: u64, threshold: u64) -> bool {
    ((kind == STOP) & (price <= threshold)) | ((kind == TAKE_PROFIT) & (price >= threshold))
}

/// The order key a ticket's holder owns.
#[inline(always)]
pub fn order_key(d: &[u32; 8]) -> [u32; 8] {
    owned_key(ORDER_TAG, d)
}

/// Accept or refuse the transition `s` shows. On acceptance, the receipt's eight output words:
/// `[method, x_lo, x_hi, 0, 0, 0, 0, 0]` — for operate `x` is the new price; for place, fire and
/// cancel the escrow placed or released. Never the threshold.
pub fn check<S: Source>(s: &S) -> Result<[u32; 8], Refusal> {
    let h = Header::read(s).ok_or(Version)?;
    let method = s.input(0);
    let x = match method {
        OPERATE => operate(s, &h)?,
        PLACE => place(s, &h)?,
        FIRE => fire(s, &h)?,
        CANCEL => cancel(s, &h)?,
        _ => return Err(Method),
    };
    let x = words_of(x);
    Ok([method, x.0, x.1, 0, 0, 0, 0, 0])
}

/// Five words are equal, without a branch per word.
#[inline(always)]
fn eq5(a: &[u32; 5], b: &[u32; 5]) -> bool {
    let mut acc = 0;
    for (x, y) in a.iter().zip(b.iter()) {
        acc |= x ^ y;
    }
    acc == 0
}

/// The oracle a cell of the context holds, if it is a live one: every word pinned (the value is
/// exactly the one its price makes), a price in `1..2^63`.
#[inline(always)]
fn oracle_at<S: Source>(s: &S, c: kit::Cell) -> Result<Oracle, Refusal> {
    let o = Oracle { price: c.val64(s, 0) };
    if !c.value_is(s, &o.value()) | (o.price == 0) | !lt_note(o.price) {
        return Err(NoOracle);
    }
    Ok(o)
}

/// Read 0 and write 0 are the oracle, live, written back exactly as read.
#[inline(never)]
fn oracle_kept<S: Source>(s: &S, h: &Header) -> Result<Oracle, Refusal> {
    let (r, w) = (h.read_cell(0), h.write_cell(0));
    if !r.key_is(s, &ORACLE_KEY) | !w.key_is(s, &ORACLE_KEY) {
        return Err(Key);
    }
    let o = oracle_at(s, r)?;
    if !w.value_is(s, &o.value()) {
        return Err(Value);
    }
    Ok(o)
}

/// The order a cell of the context holds, if it is a live one: every word pinned, an escrow in
/// `1..2^63`.
#[inline(always)]
fn order_at<S: Source>(s: &S, c: kit::Cell) -> Result<Order, Refusal> {
    let o = Order { amount: c.val64(s, 0), c: [c.val(s, 2), c.val(s, 3), c.val(s, 4), c.val(s, 5), c.val(s, 6)] };
    if !c.value_is(s, &o.value()) | (o.amount == 0) | !lt_note(o.amount) {
        return Err(NoOrder);
    }
    Ok(o)
}

/// The key of the order the ticket at `SECRET_AT` names.
#[inline(always)]
fn ticket_key<S: Source>(s: &S) -> [u32; 8] {
    order_key(&digest(s, TAG_TICKET, &secret_at(s, SECRET_AT)))
}

/// The operator sets the price: the first reading creates the oracle, later ones replace it.
fn operate<S: Source>(s: &S, h: &Header) -> Result<u64, Refusal> {
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
    if !r.key_is(s, &ORACLE_KEY) | !w.key_is(s, &ORACLE_KEY) {
        return Err(Key);
    }
    let old = u64_of(s.input(OP_OLD), s.input(OP_OLD + 1));
    let new = u64_of(s.input(OP_NEW), s.input(OP_NEW + 1));
    // The reading this replaces: none (the cell absent) when `old` is 0, else exactly `old`.
    let before = if old == 0 { [0; 8] } else { Oracle { price: old }.value() };
    if !r.value_is(s, &before) {
        return Err(Instruction);
    }
    if (new == 0) | !lt_note(new) {
        return Err(Range);
    }
    if !w.value_is(s, &Oracle { price: new }.value()) {
        return Err(Instruction);
    }
    Ok(new)
}

/// An owner escrows RAND under a commitment to the trigger. No oracle is read: the trigger is
/// the owner's business until it fires.
fn place<S: Source>(s: &S, h: &Header) -> Result<u64, Refusal> {
    if !h.shape(1, 1, 0, 0) {
        return Err(Shape);
    }
    if !h.no_token_in() {
        return Err(Inflow);
    }
    let amount = h.burn_r;
    if amount == 0 {
        return Err(Zero);
    }
    if !lt_note(amount) {
        return Err(Range);
    }
    let key = ticket_key(s);
    let (r, w) = (h.read_cell(0), h.write_cell(0));
    if !r.key_is(s, &key) | !w.key_is(s, &key) {
        return Err(Key);
    }
    if !r.is_zero(s) {
        return Err(Occupied);
    }
    let o = Opening::read(s);
    if (o.kind != STOP) & (o.kind != TAKE_PROFIT) {
        return Err(Kind);
    }
    let order = Order { amount, c: commitment(&o, |m| s.hash9(m)) };
    if !w.value_is(s, &order.value()) {
        return Err(Value);
    }
    Ok(amount)
}

/// Read `i` is the live order the ticket names and the opening was committed to; write `i`
/// deletes it; payout 0 is its whole escrow, in RAND. What fire and cancel share.
#[inline(never)]
fn release<S: Source>(s: &S, h: &Header, i: u32) -> Result<(Order, Opening), Refusal> {
    let key = ticket_key(s);
    let (r, w) = (h.read_cell(i), h.write_cell(i));
    if !r.key_is(s, &key) | !w.key_is(s, &key) {
        return Err(Key);
    }
    let order = order_at(s, r)?;
    if !w.is_zero(s) {
        return Err(Value);
    }
    let o = Opening::read(s);
    if !eq5(&commitment(&o, |m| s.hash9(m)), &order.c) {
        return Err(Commitment);
    }
    let (asset, x) = h.pay(s, 0);
    if asset != RAND {
        return Err(Asset);
    }
    if x != order.amount {
        return Err(Amount);
    }
    Ok((order, o))
}

/// The order fires: the oracle's price meets the trigger, and the escrow is released.
fn fire<S: Source>(s: &S, h: &Header) -> Result<u64, Refusal> {
    if !h.shape(2, 2, 1, 0) {
        return Err(Shape);
    }
    if !h.nothing_in() {
        return Err(Inflow);
    }
    let oracle = oracle_kept(s, h)?;
    let (order, o) = release(s, h, 1)?;
    if !fires(o.kind, oracle.price, o.threshold) {
        return Err(Condition);
    }
    Ok(order.amount)
}

/// The owner takes the escrow back, whatever the price.
fn cancel<S: Source>(s: &S, h: &Header) -> Result<u64, Refusal> {
    if !h.shape(1, 1, 1, 0) {
        return Err(Shape);
    }
    if !h.nothing_in() {
        return Err(Inflow);
    }
    let (order, _) = release(s, h, 0)?;
    Ok(order.amount)
}

#[cfg(not(target_arch = "riscv32"))]
pub mod plan;
