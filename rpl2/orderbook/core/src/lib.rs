//! orderbook: escrowed limit orders, filled in part by anyone.
//!
//! A maker **posts** an order: `give` units of one asset go into the program's vault, and the
//! order asks for `want` units of another in all — the price is `want / give`. Anyone **fills**
//! any part of it, paying in the asset it wants and taken out of the escrow, never below the
//! maker's price. The maker **closes** it whenever they like, collecting what the takers paid and
//! whatever escrow is left (a claim, a cancel, or both at once).
//!
//! Each order is one cell, named by its **ticket**: eight random words the maker keeps
//! (`<name>.secret`), whose digest is the key. Only the ticket's holder can close the order; any
//! taker can fill it knowing only the key, which `rand program state` lists.
//!
//! ```text
//! key    [1, d0, …, d6]                          d = POSEIDON2([TAG_TICKET, ticket]), "tick"
//! value  [give asset, want asset, give_rem lo, hi, want_rem lo, hi, proceeds lo, hi]
//! ```
//!
//! An order is live while its two assets differ (an absent cell, eight zeros, is not).
//! `give_rem` is the escrow left, `want_rem` what the maker still asks for it, and `proceeds`
//! what takers have paid and the maker has not yet collected.
//!
//! | method | private inputs | transition | rule |
//! |---|---|---|---|
//! | 1 post | `[1, ticket, terms]` | order absent → order; `give` in (RAND or a token) | the key is the ticket's; `give_rem` = what came in, of the asset that came in; `want_rem > 0` of another asset; `proceeds` 0; the order is the terms stated |
//! | 2 fill | `[2]` | order → order; `y` of the want asset in; pay `x` of the give asset | `0 < x ≤ give_rem`, `0 < y ≤ want_rem`, `x · want_rem ≤ y · give_rem`; both moved, `proceeds += y` |
//! | 3 close | `[3, ticket, terms]` | order → absent; pay `give_rem`, then `proceeds` (each if nonzero) | the key is the ticket's; exactly those amounts, in that order; the order read is the terms stated |
//!
//! `terms` is four words, `[give asset, want asset, want_rem lo, hi]`: the order as the maker
//! states it — posted (post) or read (close). They pin the words nothing else does: the asked
//! terms of a new order are the maker's free choice, and a close need not pay out every asset
//! nor anything of `want_rem`, yet every word a program is shown must be checked.
//!
//! **Why the proceeds wait in the cell.** A program decides amounts, never recipients: who is
//! paid is fixed by the call binding of the transaction that invokes it, and that is the
//! *taker's* transaction. So a fill cannot pay the maker — its payout goes where the taker says,
//! and the program cannot tell where that is. The taker's payment stays in the vault, counted in
//! the order's `proceeds`, until the maker proves the ticket in a transaction of their own.
//!
//! **The maker gets exactly `want` for `give`.** Every fill keeps `proceeds + want_rem` and
//! `give_rem + (escrow paid out)` constant, so they stay `want` and `give`. A fill that empties the
//! escrow (`x = give_rem`) must have `y · give_rem ≥ give_rem · want_rem`, so `y ≥ want_rem`, so
//! `y = want_rem`: the escrow runs out exactly when the ask does, and then `proceeds = want`.
//! A taker who overpays early lowers `want_rem` by exactly what they overpaid, so later takers
//! pay that much less: the remaining price `want_rem / give_rem` never rises (from
//! `x · w ≤ y · g`: `(w − y) · g ≤ w · (g − x)`), which is the same as saying the maker has always
//! been paid at least the posted price for what has been filled.
#![cfg_attr(target_arch = "riscv32", no_std)]
#![forbid(unsafe_code)]

pub use rpl2_kit as kit;
use kit::{add_note, digest, lt_note, owned_key, prod, secret_at, sub_note, words_of, Header, Source};

pub const POST: u32 = 1;
pub const FILL: u32 = 2;
pub const CLOSE: u32 = 3;

/// An order cell's key word 0.
pub const ORDER_TAG: u32 = 1;
/// The ticket's digest tag: "tick", little-endian.
pub const TAG_TICKET: u32 = 0x6b63_6974;
/// Where the ticket sits in the private inputs (after the method).
pub const TICKET_AT: u32 = 1;
/// Where the terms sit in post's and close's private inputs (after the ticket): give asset, want
/// asset, `want_rem` lo, hi.
pub const TERMS_AT: u32 = 9;

/// No public input.
pub const PUBLIC_WORDS: u32 = 0;

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
    Order,
    Price,
}
use Refusal::*;

/// An order, as its cell holds it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Order {
    pub give: u32,
    pub want: u32,
    pub give_rem: u64,
    pub want_rem: u64,
    pub proceeds: u64,
}

impl Order {
    pub fn value(&self) -> [u32; 8] {
        let (g, w, p) = (words_of(self.give_rem), words_of(self.want_rem), words_of(self.proceeds));
        [self.give, self.want, g.0, g.1, w.0, w.1, p.0, p.1]
    }

    /// `None` for an absent cell (or anything that is not a live order).
    pub fn from_value(v: &[u32; 8]) -> Option<Order> {
        let o = Order {
            give: v[0],
            want: v[1],
            give_rem: kit::u64_of(v[2], v[3]),
            want_rem: kit::u64_of(v[4], v[5]),
            proceeds: kit::u64_of(v[6], v[7]),
        };
        if o.give == o.want {
            None
        } else {
            Some(o)
        }
    }
}

/// The order a cell of the context holds, live or not.
#[inline(always)]
fn order_at<S: Source>(s: &S, c: kit::Cell) -> Order {
    Order { give: c.val(s, 0), want: c.val(s, 1), give_rem: c.val64(s, 2), want_rem: c.val64(s, 4), proceeds: c.val64(s, 6) }
}

/// The key of the order whose ticket is `ticket`, under the source's Poseidon2.
pub fn order_key<S: Source>(s: &S, ticket: &[u32; 8]) -> [u32; 8] {
    owned_key(ORDER_TAG, &digest(s, TAG_TICKET, ticket))
}

/// Accept or refuse the transition `s` shows. On acceptance, the receipt's eight output words:
/// `[method, a_lo, a_hi, b_lo, b_hi, 0, 0, 0]` — post: give, want; fill: x paid out, y paid in;
/// close: escrow refunded, proceeds collected.
pub fn check<S: Source>(s: &S) -> Result<[u32; 8], Refusal> {
    let h = Header::read(s).ok_or(Version)?;
    // Every method reads and writes the one order cell, under one key.
    if (h.n_reads != 1) | (h.n_writes != 1) | (h.n_mints != 0) {
        return Err(Shape);
    }
    let (r, w) = (h.read_cell(0), h.write_cell(0));
    let key = r.keys(s);
    if !w.key_is(s, &key) | (key[0] != ORDER_TAG) {
        return Err(Key);
    }
    let (a, b) = match s.input(0) {
        POST => post(s, &h, r, w, &key)?,
        FILL => fill(s, &h, live(s, r)?, order_at(s, w))?,
        CLOSE => close(s, &h, live(s, r)?, w, &key)?,
        _ => return Err(Method),
    };
    let (a, b) = (words_of(a), words_of(b));
    Ok([s.input(0), a.0, a.1, b.0, b.1, 0, 0, 0])
}

/// The read cell is a live order, its amounts in range.
#[inline(always)]
fn live<S: Source>(s: &S, r: kit::Cell) -> Result<Order, Refusal> {
    let o = order_at(s, r);
    if o.give == o.want {
        return Err(Order);
    }
    if !lt_note(o.give_rem) | !lt_note(o.want_rem) | !lt_note(o.proceeds) {
        return Err(Range);
    }
    Ok(o)
}

/// The private inputs carry the ticket that names `key`.
#[inline(always)]
fn holds_ticket<S: Source>(s: &S, key: &[u32; 8]) -> bool {
    kit::eq8(&order_key(s, &secret_at(s, TICKET_AT)), key)
}

/// The private inputs state the order's terms: its two assets and `want_rem`.
#[inline(always)]
fn states_terms<S: Source>(s: &S, o: &Order) -> bool {
    let want = kit::u64_of(s.input(TERMS_AT + 2), s.input(TERMS_AT + 3));
    (o.give == s.input(TERMS_AT)) & (o.want == s.input(TERMS_AT + 1)) & (o.want_rem == want)
}

/// Post: the escrow comes in, and an order is written where there was none.
fn post<S: Source>(s: &S, h: &Header, r: kit::Cell, w: kit::Cell, key: &[u32; 8]) -> Result<(u64, u64), Refusal> {
    if h.n_pays != 0 {
        return Err(Shape);
    }
    // The ticket names the key, so an order is only ever posted where its maker can close it.
    if !holds_ticket(s, key) {
        return Err(Key);
    }
    // The cell was empty: the chain refuses the transition as stale if it was not, and an order
    // posted over another would take its escrow.
    if !r.is_zero(s) {
        return Err(Order);
    }
    let (asset, give) = h.one_deposit().ok_or(Inflow)?;
    let o = order_at(s, w);
    if (o.give != asset) | (o.want == asset) {
        return Err(Asset);
    }
    if !lt_note(give) | !lt_note(o.want_rem) {
        return Err(Range);
    }
    if (o.give_rem != give) | (o.want_rem == 0) | (o.proceeds != 0) {
        return Err(Value);
    }
    // The order written asks what the maker stated: no word of it is left to whoever assembles
    // the transition.
    if !states_terms(s, &o) {
        return Err(Value);
    }
    Ok((give, o.want_rem))
}

/// Fill: the taker pays `y` of the want asset in and is paid `x` of the give asset out.
fn fill<S: Source>(s: &S, h: &Header, before: Order, after: Order) -> Result<(u64, u64), Refusal> {
    if h.n_pays != 1 {
        return Err(Shape);
    }
    let (in_asset, y) = h.one_deposit().ok_or(Inflow)?;
    let (out_asset, x) = h.pay(s, 0);
    if (in_asset != before.want) | (out_asset != before.give) {
        return Err(Asset);
    }
    if (after.give != before.give) | (after.want != before.want) {
        return Err(Value);
    }
    if (x == 0) | (y == 0) {
        return Err(Zero);
    }
    if !lt_note(x) | !lt_note(y) {
        return Err(Range);
    }
    // x ≤ give_rem and y ≤ want_rem, or the subtraction is refused.
    let g = sub_note(before.give_rem, x).ok_or(Order)?;
    let w = sub_note(before.want_rem, y).ok_or(Order)?;
    let p = add_note(before.proceeds, y).ok_or(Range)?;
    if (after.give_rem != g) | (after.want_rem != w) | (after.proceeds != p) {
        return Err(Value);
    }
    if !price_ok(before.give_rem, before.want_rem, x, y) {
        return Err(Price);
    }
    Ok((x, y))
}

/// Close: the ticket's holder deletes the order and is paid what is left of the escrow, then
/// the proceeds — each only if nonzero (the chain refuses a zero payout).
fn close<S: Source>(s: &S, h: &Header, o: Order, w: kit::Cell, key: &[u32; 8]) -> Result<(u64, u64), Refusal> {
    if !holds_ticket(s, key) {
        return Err(Key);
    }
    // The order closed is the one the maker read: an asset with nothing left to pay, and
    // `want_rem` always, are otherwise unused here, so they are pinned to the maker's statement of
    // them (every word shown is checked).
    if !states_terms(s, &o) {
        return Err(Value);
    }
    if !h.nothing_in() {
        return Err(Inflow);
    }
    if !w.is_zero(s) {
        return Err(Value);
    }
    let (has_g, has_p) = (o.give_rem != 0, o.proceeds != 0);
    if h.n_pays != (has_g as u32) + (has_p as u32) {
        return Err(Shape);
    }
    if has_g && h.pay(s, 0) != (o.give, o.give_rem) {
        return Err(Asset);
    }
    if has_p && h.pay(s, has_g as u32) != (o.want, o.proceeds) {
        return Err(Asset);
    }
    Ok((o.give_rem, o.proceeds))
}

/// The maker's remaining price, never undercut: `x · want_rem ≤ y · give_rem` — `x` of the give
/// asset out for `y` of the want asset in is at least `want_rem / give_rem` per unit. Each side
/// is a product of two amounts below 2^63, so below 2^126.
pub fn price_ok(give_rem: u64, want_rem: u64, x: u64, y: u64) -> bool {
    prod(x, want_rem).le(prod(y, give_rem))
}

#[cfg(not(target_arch = "riscv32"))]
pub mod plan;
