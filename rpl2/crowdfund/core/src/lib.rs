//! crowdfund: one all-or-nothing campaign, Kickstarter-style. Backers pledge RAND and get
//! receipts; they may take their RAND back with their receipts until the creator claims; the
//! creator may claim everything raised once it reaches the goal, and only then.
//!
//! The campaign is one cell; the pledged RAND sits in the program's vault; the receipts are an
//! RPL token registered with `rand token create --program <id>` and bound to the campaign by its
//! `init`. A receipt is a shielded note like any other, so **who backed the campaign is private**:
//! a refund only shows that some receipts were burned.
//!
//! The program's **deploy-time public input** is ten words: the creator's lock
//! `POSEIDON2([TAG_CREATOR, s0..s7])` (8 words), then the goal in RAND base units (low, high).
//! Both are part of the program id, so neither can be changed or front-run; each campaign is its
//! own program, with its own id and vault.
//!
//! ```text
//! key    [1, 0, 0, 0, 0, 0, 0, 0]
//! value  [raised_lo, raised_hi, receipt, claimed, 0, 0, 0, 1]     RAND raised, receipt token, 0 or 1, version
//! ```
//!
//! | method | private inputs | transition | rule |
//! |---|---|---|---|
//! | 1 init | `[1, s0..s7, receipt]` (creator) | campaign absent → `[0, 0, receipt, 0, 0, 0, 0, 1]`; nothing in or out | the secret opens the lock; `receipt ≠ RAND` |
//! | 2 pledge | `[2]` | campaign → campaign; `a` RAND in; mint `a` receipts | not claimed; `a > 0`; `raised' = raised + a` |
//! | 3 refund | `[3]` | campaign → campaign; `b` receipts burned; pay `b` RAND | not claimed; `0 < b ≤ raised`; `raised' = raised − b` |
//! | 4 claim | `[4, s0..s7]` (creator) | campaign → claimed; pay `raised` RAND | not claimed; the secret opens the lock; `raised ≥ goal`; `raised'` = `raised`, `claimed' = 1` |
//!
//! While the campaign is open, **receipts outstanding = `raised` = the vault's RAND**: every
//! pledge mints exactly what it brings in, every refund pays exactly what it burns. So every
//! receipt can always be redeemed one for one, and a claim empties the vault exactly. After the
//! claim no method accepts the campaign any more: it is final, and the receipts stay as badges.
//!
//! There is **no deadline**: RPL-2 shows a program no clock. See the README.
#![cfg_attr(target_arch = "riscv32", no_std)]
#![forbid(unsafe_code)]

pub use rpl2_kit as kit;
pub use kit::RAND;
use kit::{add_note, lt_note, opens_lock, sub_note, u64_of, words_of, Header, Source};

pub const INIT: u32 = 1;
pub const PLEDGE: u32 = 2;
pub const REFUND: u32 = 3;
pub const CLAIM: u32 = 4;

/// The campaign's cell.
pub const CAMPAIGN_KEY: [u32; 8] = [1, 0, 0, 0, 0, 0, 0, 0];
/// The campaign value's last word: a live campaign is never all zeros.
pub const VERSION: u32 = 1;
/// The creator's secret domain tag: "crtr", little-endian.
pub const TAG_CREATOR: u32 = 0x7274_7263;

/// The public input: the creator's lock (words 0..8), then the goal (low, high).
pub const PUBLIC_LOCK: u32 = 0;
pub const PUBLIC_GOAL: u32 = 8;
pub const PUBLIC_WORDS: u32 = 10;
/// Where the creator's secret sits in the private inputs, after the method.
pub const SECRET_AT: u32 = 1;
/// Where init's receipt token sits in the private inputs, after the secret.
pub const RECEIPT_AT: u32 = 9;

/// Why a transition was refused. The guest never says — a refused transition has no proof — so
/// these are for the tests and the `plan` tool.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    Version,
    Goal,
    Method,
    Shape,
    Inflow,
    Key,
    Value,
    Asset,
    Zero,
    Range,
    State,
    Claimed,
    Secret,
    Short,
    Payout,
}
use Refusal::*;

/// A campaign, as its cell holds it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Campaign {
    pub raised: u64,
    pub receipt: u32,
    pub claimed: bool,
}

impl Campaign {
    pub fn value(&self) -> [u32; 8] {
        let r = words_of(self.raised);
        [r.0, r.1, self.receipt, self.claimed as u32, 0, 0, 0, VERSION]
    }

    /// `None` for an absent cell (or anything that is not a well-formed campaign).
    pub fn from_value(v: &[u32; 8]) -> Option<Campaign> {
        if (v[7] != VERSION) | (v[3] > 1) | ((v[4] | v[5] | v[6]) != 0) {
            return None;
        }
        Some(Campaign { raised: u64_of(v[0], v[1]), receipt: v[2], claimed: v[3] == 1 })
    }
}

/// The goal, from the public input.
#[inline(always)]
pub fn goal_of<S: Source>(s: &S) -> u64 {
    u64_of(s.public(PUBLIC_GOAL), s.public(PUBLIC_GOAL + 1))
}

/// The creator may claim: the goal is met. Plain comparison — the campaign never needs a product.
#[inline(always)]
pub fn goal_met(raised: u64, goal: u64) -> bool {
    raised >= goal
}

/// A refund of `b` receipts is payable from a campaign holding `raised`: `0 < b ≤ raised`.
#[inline(always)]
pub fn refund_ok(raised: u64, b: u64) -> bool {
    (b != 0) & sub_note(raised, b).is_some()
}

/// Accept or refuse the transition `s` shows. On acceptance, the receipt's eight output words:
/// `[method, amount_lo, amount_hi, raised_lo, raised_hi, claimed, 0, 0]` — the amount pledged,
/// refunded or claimed (0 for init), and the campaign as written.
pub fn check<S: Source>(s: &S) -> Result<[u32; 8], Refusal> {
    let h = Header::read(s).ok_or(Version)?;
    let goal = goal_of(s);
    if (goal == 0) | !lt_note(goal) {
        return Err(Goal);
    }
    // Every method reads and writes the one campaign cell.
    let (r, w) = (h.read_cell(0), h.write_cell(0));
    if (h.n_reads != 1) | (h.n_writes != 1) {
        return Err(Shape);
    }
    if !r.key_is(s, &CAMPAIGN_KEY) || !w.key_is(s, &CAMPAIGN_KEY) {
        return Err(Key);
    }
    let method = s.input(0);
    let (amount, after) = match method {
        INIT => init(s, &h, r)?,
        PLEDGE => pledge(s, &h, open(s, r)?)?,
        REFUND => refund(s, &h, open(s, r)?)?,
        CLAIM => claim(s, &h, goal, open(s, r)?)?,
        _ => return Err(Method),
    };
    // The written cell is exactly the campaign each method computed: all eight words.
    if !w.value_is(s, &after.value()) {
        return Err(Value);
    }
    let (a, c) = (words_of(amount), words_of(after.raised));
    Ok([method, a.0, a.1, c.0, c.1, after.claimed as u32, 0, 0])
}

/// The read cell is a well-formed, unclaimed campaign. Every word of it is pinned here: the
/// version, the zeros, `claimed`, `raised` (below 2^63) and the receipt token (never RAND).
#[inline(always)]
fn open<S: Source>(s: &S, r: kit::Cell) -> Result<Campaign, Refusal> {
    if (r.val(s, 7) != VERSION) | ((r.val(s, 4) | r.val(s, 5) | r.val(s, 6)) != 0) {
        return Err(State);
    }
    if r.val(s, 3) != 0 {
        // Claimed (or not a campaign at all): nothing changes it any more.
        return Err(Claimed);
    }
    let c = Campaign { raised: r.val64(s, 0), receipt: r.val(s, 2), claimed: false };
    if !lt_note(c.raised) {
        return Err(Range);
    }
    if c.receipt == RAND {
        return Err(Asset);
    }
    Ok(c)
}

/// Open the campaign: the creator binds the receipt token. Nothing comes in or goes out.
fn init<S: Source>(s: &S, h: &Header, r: kit::Cell) -> Result<(u64, Campaign), Refusal> {
    if (h.n_pays != 0) | (h.n_mints != 0) {
        return Err(Shape);
    }
    if !h.nothing_in() {
        return Err(Inflow);
    }
    if !r.is_zero(s) {
        return Err(State);
    }
    if !opens_lock(s, TAG_CREATOR, SECRET_AT, PUBLIC_LOCK) {
        return Err(Secret);
    }
    // The receipt token is the creator's private input, after the secret; `check` then pins the
    // whole write to the campaign it opens.
    let receipt = s.input(RECEIPT_AT);
    if receipt == RAND {
        return Err(Asset);
    }
    Ok((0, Campaign { raised: 0, receipt, claimed: false }))
}

/// Pledge: `a` RAND in through `burn_r`, nothing else; exactly `a` receipts minted.
fn pledge<S: Source>(s: &S, h: &Header, c: Campaign) -> Result<(u64, Campaign), Refusal> {
    if (h.n_pays != 0) | (h.n_mints != 1) {
        return Err(Shape);
    }
    if !h.no_token_in() {
        return Err(Inflow);
    }
    let a = h.burn_r;
    let (asset, minted) = h.mint(s, 0);
    if asset != c.receipt {
        return Err(Asset);
    }
    if a == 0 {
        return Err(Zero);
    }
    if minted != a {
        return Err(Payout);
    }
    let raised = add_note(c.raised, a).ok_or(Range)?;
    Ok((a, Campaign { raised, ..c }))
}

/// Refund: `b` receipts burned, nothing else in; exactly `b` RAND paid.
fn refund<S: Source>(s: &S, h: &Header, c: Campaign) -> Result<(u64, Campaign), Refusal> {
    if (h.n_pays != 1) | (h.n_mints != 0) {
        return Err(Shape);
    }
    if (h.burn_r != 0) | (h.inflow != kit::INFLOW_BURN) | (h.burn_asset != c.receipt) {
        return Err(Inflow);
    }
    let b = h.burn_a;
    let (asset, paid) = h.pay(s, 0);
    if asset != RAND {
        return Err(Asset);
    }
    if paid != b {
        return Err(Payout);
    }
    if !refund_ok(c.raised, b) {
        return Err(if b == 0 { Zero } else { Range });
    }
    let raised = sub_note(c.raised, b).ok_or(Range)?;
    Ok((b, Campaign { raised, ..c }))
}

/// Claim: the creator takes exactly everything raised, once it meets the goal. Nothing comes in.
fn claim<S: Source>(s: &S, h: &Header, goal: u64, c: Campaign) -> Result<(u64, Campaign), Refusal> {
    if (h.n_pays != 1) | (h.n_mints != 0) {
        return Err(Shape);
    }
    if !h.nothing_in() {
        return Err(Inflow);
    }
    let (asset, paid) = h.pay(s, 0);
    if asset != RAND {
        return Err(Asset);
    }
    if paid != c.raised {
        return Err(Payout);
    }
    if !goal_met(c.raised, goal) {
        return Err(Short);
    }
    if !opens_lock(s, TAG_CREATOR, SECRET_AT, PUBLIC_LOCK) {
        return Err(Secret);
    }
    // `raised` stays as the record of what was raised; `claimed` makes the campaign final.
    Ok((paid, Campaign { claimed: true, ..c }))
}

#[cfg(not(target_arch = "riscv32"))]
pub mod plan;
