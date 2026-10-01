//! credit-score: a lender's published scoring model, run over a borrower's private bank
//! statements. The receipt is the score band; the lender never sees a transaction.
//!
//! The rules live here so that the guest, the off-chain hasher (`stmt-hash/`), the `score` tool
//! and the tests all run the same code. The guest answers [`Words`] with syscalls; the host with
//! vectors.
//!
//! ```text
//! public input (deploy, model.txt)   6 words: the model's parameters, see [`Model`]
//! private input (call)               40 words:
//!     0, 1        two blind words, uniformly random per call (never output)
//!     2 + 3m      month m's income            (m = 0..12, whole currency units)
//!     3 + 3m      month m's rent or debt payment
//!     4 + 3m      month m's end balance
//!     38, 39      the statements' salt, two random words the borrower keeps
//! outputs (receipt)                  [band, months_positive, c0, c1, c2, c3, c4, c5]
//!     c0..c5 = the first six words of POSEIDON2([TAG_STMT, the 36 statement words, salt0, salt1])
//! ```
//!
//! **The band**, 0..=3. Four gates, then two points:
//!
//! | | passes when |
//! |---|---|
//! | months gate | `months_positive ≥ min_months_positive`, a month being positive when `income ≥ payment` |
//! | DTI gate | `Σpayment · 10 000 ≤ Σincome · max_dti_bps` |
//! | balance gate | `Σbalance ≥ 12 · min_avg_balance` |
//! | income gate | `Σincome ≥ 12 · min_monthly_income` |
//!
//! Any gate failing is **band 0**. All four passing is band 1, plus one point for
//! `Σpayment · 10 000 ≤ Σincome · prime_dti_bps` and one for `Σbalance ≥ 12 · prime_avg_balance`.
//! Nothing divides: every rule is a product compared with a product, so no word of the model or
//! the statements can make the guest trap, and no average is rounded.
//!
//! Twelve u32 words sum to below 2^36; the two basis-point words are bounded at 10 000 < 2^14
//! ([`Model::from_words`] refuses a model above it), so every product here is below 2^50 and the
//! `u64` arithmetic cannot overflow.
#![cfg_attr(target_arch = "riscv32", no_std)]
#![forbid(unsafe_code)]

/// Twelve months, fixed: there is no count word to cap, and a shorter file reads past its own
/// committed inputs (the call has no proof).
pub const MONTHS: usize = 12;
/// Each month is `[income, payment, end_balance]`.
pub const PER_MONTH: usize = 3;
pub const STMT_WORDS: usize = MONTHS * PER_MONTH;

/// Private inputs 0 and 1: the blind words.
pub const BLIND_WORDS: u32 = 2;
/// Private inputs 2..38: the statements, month-major.
pub const STMT_AT: u32 = BLIND_WORDS;
/// Private inputs 38, 39: the salt.
pub const SALT_AT: u32 = STMT_AT + STMT_WORDS as u32;
pub const SALT_WORDS: u32 = 2;
/// A call commits exactly this many private words.
pub const INPUT_WORDS: u32 = SALT_AT + SALT_WORDS;

/// The public input's length.
pub const MODEL_WORDS: usize = 6;

/// The domain tag of the statements' commitment: `"stmt"`, little-endian.
pub const TAG_STMT: u32 = 0x746d_7473;
/// The commitment's message: the tag, the 36 statement words, the two salt words. Fixed length,
/// because guest-sdk's Poseidon2 does not pad.
pub const MSG_WORDS: usize = 1 + STMT_WORDS + SALT_WORDS as usize;
/// How many words of the digest the receipt carries (192 bits).
pub const COMMIT_WORDS: usize = 6;

/// One hundred percent, in basis points.
pub const BPS: u64 = 10_000;

/// Where a program reads from. The guest answers with syscalls; tests and tools with vectors.
pub trait Words {
    /// The deploy-time public input, word `i`.
    fn public(&self, i: u32) -> u32;
    /// Private input word `i`.
    fn input(&self, i: u32) -> u32;
}

/// The lender's model: the six public words, in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Model {
    /// Gate: at least this many of the twelve months must have `income ≥ payment`.
    pub min_months_positive: u32,
    /// Gate: the year's payments over its income, at most this many basis points.
    pub max_dti_bps: u32,
    /// Gate: the average end balance, at least this.
    pub min_avg_balance: u32,
    /// Gate: the average monthly income, at least this.
    pub min_monthly_income: u32,
    /// A point: DTI at or under this.
    pub prime_dti_bps: u32,
    /// A point: average end balance at or over this.
    pub prime_avg_balance: u32,
}

impl Model {
    /// The model from its six words, or `None` for one no borrower can be scored by: a DTI word
    /// over 10 000 bps. (The bound is also what keeps the products in `score` inside a `u64`.)
    pub fn from_words(w: [u32; MODEL_WORDS]) -> Option<Model> {
        let m = Model {
            min_months_positive: w[0],
            max_dti_bps: w[1],
            min_avg_balance: w[2],
            min_monthly_income: w[3],
            prime_dti_bps: w[4],
            prime_avg_balance: w[5],
        };
        if (m.max_dti_bps as u64 > BPS) | (m.prime_dti_bps as u64 > BPS) {
            return None;
        }
        Some(m)
    }

    /// The model from the public input.
    #[inline(always)]
    pub fn read<W: Words>(w: &W) -> Option<Model> {
        Model::from_words([w.public(0), w.public(1), w.public(2), w.public(3), w.public(4), w.public(5)])
    }

    pub fn words(&self) -> [u32; MODEL_WORDS] {
        [
            self.min_months_positive,
            self.max_dti_bps,
            self.min_avg_balance,
            self.min_monthly_income,
            self.prime_dti_bps,
            self.prime_avg_balance,
        ]
    }
}

/// Twelve months of `[income, payment, end_balance]`, whole currency units each.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Statements(pub [[u32; PER_MONTH]; MONTHS]);

impl Statements {
    /// The statements from private inputs `STMT_AT..SALT_AT`.
    #[inline(always)]
    pub fn read<W: Words>(w: &W) -> Statements {
        let mut rows = [[0u32; PER_MONTH]; MONTHS];
        for (m, row) in rows.iter_mut().enumerate() {
            let at = STMT_AT + (m as u32) * (PER_MONTH as u32);
            row[0] = w.input(at);
            row[1] = w.input(at + 1);
            row[2] = w.input(at + 2);
        }
        Statements(rows)
    }

    /// The statements from 36 words, month-major (a statements file, or `stmt-hash`'s inputs).
    pub fn from_words(words: &[u32; STMT_WORDS]) -> Statements {
        let mut rows = [[0u32; PER_MONTH]; MONTHS];
        for (dst, src) in rows.iter_mut().flatten().zip(words.iter()) {
            *dst = *src;
        }
        Statements(rows)
    }

    /// The 36 words, month-major.
    pub fn words(&self) -> [u32; STMT_WORDS] {
        let mut out = [0u32; STMT_WORDS];
        for (dst, src) in out.iter_mut().zip(self.0.iter().flatten()) {
            *dst = *src;
        }
        out
    }
}

/// What the model says of a borrower. Only `band` and `months_positive` reach the receipt; the
/// sums are for the `score` tool and the tests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Score {
    pub band: u32,
    pub months_positive: u32,
    pub sum_income: u64,
    pub sum_payment: u64,
    pub sum_balance: u64,
}

/// Run the model over the statements.
#[inline(never)]
pub fn score(model: &Model, s: &Statements) -> Score {
    let mut months_positive = 0u32;
    let (mut sum_income, mut sum_payment, mut sum_balance) = (0u64, 0u64, 0u64);
    for row in s.0.iter() {
        let (income, payment, balance) = (row[0], row[1], row[2]);
        if income >= payment {
            months_positive += 1;
        }
        sum_income += income as u64;
        sum_payment += payment as u64;
        sum_balance += balance as u64;
    }
    // Each sum is below 2^36 and each bps word at most 10 000: the products are below 2^50.
    let dti_at_most = |bps: u32| sum_payment * BPS <= sum_income * bps as u64;
    let avg_at_least = |sum: u64, min: u32| sum >= (MONTHS as u64) * (min as u64);
    let gates = (months_positive >= model.min_months_positive)
        & dti_at_most(model.max_dti_bps)
        & avg_at_least(sum_balance, model.min_avg_balance)
        & avg_at_least(sum_income, model.min_monthly_income);
    let band = if gates {
        1 + dti_at_most(model.prime_dti_bps) as u32 + avg_at_least(sum_balance, model.prime_avg_balance) as u32
    } else {
        0
    };
    Score { band, months_positive, sum_income, sum_payment, sum_balance }
}

/// The salt, private inputs `SALT_AT`, `SALT_AT + 1`.
#[inline(always)]
pub fn salt<W: Words>(w: &W) -> [u32; 2] {
    [w.input(SALT_AT), w.input(SALT_AT + 1)]
}

/// The commitment's message, `[TAG_STMT, the 36 statement words, salt0, salt1]`, ready for
/// `poseidon2(msg.as_mut_ptr(), MSG_WORDS)`, which leaves the digest in its first eight words.
pub fn commit_message(s: &Statements, salt: [u32; 2]) -> [u32; MSG_WORDS] {
    let mut msg = [0u32; MSG_WORDS];
    msg[0] = TAG_STMT;
    for (dst, src) in msg[1..1 + STMT_WORDS].iter_mut().zip(s.0.iter().flatten()) {
        *dst = *src;
    }
    msg[1 + STMT_WORDS] = salt[0];
    msg[2 + STMT_WORDS] = salt[1];
    msg
}

/// The receipt's eight words: `[band, months_positive, c0, …, c5]`.
pub fn outputs(sc: &Score, digest: &[u32; 8]) -> [u32; 8] {
    [sc.band, sc.months_positive, digest[0], digest[1], digest[2], digest[3], digest[4], digest[5]]
}
