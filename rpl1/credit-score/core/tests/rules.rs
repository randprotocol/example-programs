//! The model's rules on the host: each band, each gate at its boundary, the input layout the
//! guest reads, and the commitment's message.
use credit_score_core::{
    commit_message, outputs, salt, score, Model, Score, Statements, Words, INPUT_WORDS, MSG_WORDS, SALT_AT, STMT_AT,
    STMT_WORDS, TAG_STMT,
};
use std::cell::Cell;

/// The model `model.txt` deploys with.
const MODEL: [u32; 6] = [10, 4000, 500, 1500, 2000, 3000];

fn model() -> Model {
    Model::from_words(MODEL).unwrap()
}

/// Twelve identical months.
fn flat(income: u32, payment: u32, balance: u32) -> Statements {
    Statements([[income, payment, balance]; 12])
}

fn band(s: &Statements) -> u32 {
    score(&model(), s).band
}

/// A call's words, as the guest sees them: public from a vector, private from a vector, and a
/// flag for a read past the end (the guest's refusal).
struct Mock {
    public: Vec<u32>,
    input: Vec<u32>,
    oob: Cell<bool>,
}

impl Words for Mock {
    fn public(&self, i: u32) -> u32 {
        *self.public.get(i as usize).unwrap_or_else(|| {
            self.oob.set(true);
            &0
        })
    }
    fn input(&self, i: u32) -> u32 {
        *self.input.get(i as usize).unwrap_or_else(|| {
            self.oob.set(true);
            &0
        })
    }
}

fn call(statement_words: &[u32], salt_words: [u32; 2]) -> Mock {
    let mut input = vec![0xb11d_0001, 0xb11d_0002];
    input.extend_from_slice(statement_words);
    input.extend_from_slice(&salt_words);
    Mock { public: MODEL.to_vec(), input, oob: Cell::new(false) }
}

#[test]
fn a_clean_year_is_band_3() {
    let s = flat(5000, 1000, 6000);
    assert_eq!(score(&model(), &s), Score { band: 3, months_positive: 12, sum_income: 60_000, sum_payment: 12_000, sum_balance: 72_000 });
}

#[test]
fn one_missed_month_under_the_cap_is_band_2() {
    // Eleven months of 4000 in, 1200 out; one month with no income. DTI 3272 bps: under the 4000
    // cap, over the 2000 prime; balances earn the prime point; 11 ≥ 10 months.
    let mut s = flat(4000, 1200, 4000);
    s.0[6][0] = 0;
    let sc = score(&model(), &s);
    assert_eq!((sc.band, sc.months_positive), (2, 11));
}

#[test]
fn a_stretched_borrower_is_band_1() {
    // DTI 3500 bps and an average balance of 700: both gates pass, neither prime point.
    assert_eq!(band(&flat(3000, 1050, 700)), 1);
}

#[test]
fn the_months_gate_is_a_gate() {
    // Three months with no income: DTI 2666 bps and good balances, but 9 < 10 months covered.
    let mut s = flat(5000, 1000, 6000);
    for m in 3..6 {
        s.0[m][0] = 0;
    }
    let sc = score(&model(), &s);
    assert_eq!((sc.band, sc.months_positive), (0, 9));
}

#[test]
fn the_dti_gate_at_its_boundary() {
    // 12 · 2000 · 10 000 == 12 · 5000 · 4000: exactly at the cap passes…
    assert!(band(&flat(5000, 2000, 6000)) >= 1);
    // …and one unit of payment over it in one month fails.
    let mut s = flat(5000, 2000, 6000);
    s.0[0][1] += 1;
    assert_eq!(band(&s), 0);
}

#[test]
fn the_prime_dti_point_at_its_boundary() {
    // 12 · 1000 · 10 000 == 12 · 5000 · 2000: at the prime bound, the point is earned.
    assert_eq!(band(&flat(5000, 1000, 6000)), 3);
    let mut s = flat(5000, 1000, 6000);
    s.0[11][1] += 1;
    assert_eq!(band(&s), 2);
}

#[test]
fn the_balance_gate_and_point_at_their_boundaries() {
    // Sums, not rounded averages: 11 months at 500 and one at 499 is below 12 · 500. (DTI 2000 bps
    // earns the prime point here, so the band is 2 while the gate holds.)
    let mut s = flat(5000, 1000, 500);
    assert_eq!(band(&s), 2);
    s.0[5][2] = 499;
    assert_eq!(band(&s), 0);
    // The prime point likewise: 12 · 3000 exactly earns it, one unit under does not.
    assert_eq!(band(&flat(5000, 1000, 3000)), 3);
    let mut s = flat(5000, 1000, 3000);
    s.0[0][2] -= 1;
    assert_eq!(band(&s), 2);
}

#[test]
fn the_income_gate_stops_a_zero_income_year() {
    // No income and no payments: DTI 0 ≤ 0 would pass; the income gate does not.
    assert_eq!(band(&flat(0, 0, 100_000)), 0);
    // 12 · 1500 exactly passes; one unit under fails.
    assert!(band(&flat(1500, 100, 6000)) >= 1);
    let mut s = flat(1500, 100, 6000);
    s.0[0][0] -= 1;
    assert_eq!(band(&s), 0);
}

#[test]
fn the_model_sets_the_bar() {
    // max_dti_bps = 0: anyone who pays anything is band 0, however good the rest is.
    let strict = Model::from_words([10, 0, 500, 1500, 2000, 3000]).unwrap();
    assert_eq!(score(&strict, &flat(5000, 1000, 6000)).band, 0);
    // (Only a year with no payments at all passes a zero cap.)
    assert_eq!(score(&strict, &flat(5000, 0, 6000)).band, 3);
    // 10 000 bps is the most a model may ask; above it, no model — the guest refuses every call.
    assert!(Model::from_words([10, 10_000, 500, 1500, 10_000, 3000]).is_some());
    assert!(Model::from_words([10, 10_001, 500, 1500, 2000, 3000]).is_none());
    assert!(Model::from_words([10, 4000, 500, 1500, 10_001, 3000]).is_none());
    assert_eq!(model().words(), MODEL);
}

#[test]
fn sums_of_maximal_words_do_not_overflow() {
    let s = flat(u32::MAX, u32::MAX, u32::MAX);
    let loose = Model::from_words([12, 10_000, u32::MAX, u32::MAX, 10_000, u32::MAX]).unwrap();
    assert_eq!(score(&loose, &s).band, 3);
}

#[test]
fn the_guest_reads_the_layout_the_scripts_write() {
    let words: Vec<u32> = (100..100 + STMT_WORDS as u32).collect();
    let m = call(&words, [7, 11]);
    assert_eq!(Model::read(&m), Some(model()));
    let s = Statements::read(&m);
    assert_eq!(s.0[0], [100, 101, 102]);
    assert_eq!(s.0[11], [133, 134, 135]);
    assert_eq!(s.words().to_vec(), words);
    assert_eq!(s, Statements::from_words(words.as_slice().try_into().unwrap()));
    assert_eq!(salt(&m), [7, 11]);
    assert_eq!(m.input.len() as u32, INPUT_WORDS);
    assert!(!m.oob.get());
    assert_eq!(STMT_AT, 2);
    assert_eq!(SALT_AT, 38);
}

#[test]
fn eleven_months_read_past_the_committed_inputs() {
    let words: Vec<u32> = (100..100 + 33).collect();
    let m = call(&words, [7, 11]);
    Statements::read(&m);
    salt(&m);
    assert!(m.oob.get(), "the guest refuses: no proof");
}

#[test]
fn the_commitment_message_is_tag_statements_salt() {
    let words: Vec<u32> = (100..100 + STMT_WORDS as u32).collect();
    let s = Statements::from_words(words.as_slice().try_into().unwrap());
    let msg = commit_message(&s, [7, 11]);
    assert_eq!(msg.len(), MSG_WORDS);
    assert_eq!(msg[0], TAG_STMT);
    assert_eq!(&msg[1..37], words.as_slice());
    assert_eq!(&msg[37..], &[7, 11]);
    // The salt changes the message: two identical years commit differently.
    assert_ne!(commit_message(&s, [7, 12]), msg);
}

#[test]
fn the_receipt_is_band_months_and_six_digest_words() {
    let sc = score(&model(), &flat(5000, 1000, 6000));
    let digest = [1, 2, 3, 4, 5, 6, 7, 8];
    assert_eq!(outputs(&sc, &digest), [3, 12, 1, 2, 3, 4, 5, 6]);
}
