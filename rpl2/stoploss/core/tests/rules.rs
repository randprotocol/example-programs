//! The stoploss's rules, on the host: every method accepted, every tampering refused, and no
//! context word of any accepted transition left unchecked.
use stoploss_core::kit::host::{loose_words, test_hash, Mock, Out, Transition};
use stoploss_core::{check, commitment, fires, plan, Opening, Oracle, Order, Refusal, CANCEL, FIRE, OPERATE, ORACLE_KEY, PLACE, STOP, TAG_OPERATOR, TAKE_PROFIT};

const SECRET: [u32; 8] = [1, 2, 3, 4, 5, 6, 7, 8];
const TICKET: [u32; 8] = [11, 12, 13, 14, 15, 16, 17, 18];
const OTHER: [u32; 8] = [21, 22, 23, 24, 25, 26, 27, 28];
const AMOUNT: u64 = 5_000_000_000;

fn lock() -> [u32; 8] {
    test_hash([TAG_OPERATOR, 1, 2, 3, 4, 5, 6, 7, 8])
}

fn run(t: &Transition, input: &[u32]) -> Result<[u32; 8], Refusal> {
    let m = Mock::new(&lock(), t, input, test_hash);
    let r = check(&m);
    if m.oob.get() { Err(Refusal::Shape) } else { r }
}

fn stop(threshold: u64) -> Opening {
    Opening { kind: STOP, threshold, salt: [1, 2, 3, 4] }
}

fn take_profit(threshold: u64) -> Opening {
    Opening { kind: TAKE_PROFIT, threshold, salt: [5, 6, 7, 8] }
}

fn key() -> [u32; 8] {
    plan::key_of(&TICKET, test_hash)
}

fn placed(o: &Opening) -> Order {
    plan::place(key(), [0; 8], AMOUNT, o, test_hash).unwrap().1
}

fn fire_at(price: u64, o: &Opening) -> Transition {
    plan::fire(key(), Oracle { price }, placed(o), o, test_hash, None).unwrap()
}

/// A fire transition `plan` would refuse to build (the condition is false, or the opening wrong):
/// the transition a dishonest wallet would send.
fn forced_fire(price: u64, order: Order) -> Transition {
    Transition::new()
        .read(ORACLE_KEY, Oracle { price }.value())
        .read(key(), order.value())
        .write(ORACLE_KEY, Oracle { price }.value())
        .write(key(), [0; 8])
        .pay(0, order.amount)
        .sorted()
}

#[test]
fn the_operator_sets_the_price_and_nobody_else() {
    let t = plan::operate(None, 100).unwrap();
    assert_eq!(run(&t, &plan::operate_inputs(&SECRET, 0, 100)).unwrap(), [OPERATE, 100, 0, 0, 0, 0, 0, 0]);
    assert_eq!(run(&t, &plan::operate_inputs(&OTHER, 0, 100)), Err(Refusal::Operator));
    // The instruction must say what it replaces: an update over an absent oracle, or a first
    // reading over a live one, is refused.
    assert_eq!(run(&t, &plan::operate_inputs(&SECRET, 90, 100)), Err(Refusal::Instruction));
    let t = plan::operate(Some(Oracle { price: 100 }), 85).unwrap();
    assert!(run(&t, &plan::operate_inputs(&SECRET, 100, 85)).is_ok());
    assert_eq!(run(&t, &plan::operate_inputs(&SECRET, 0, 85)), Err(Refusal::Instruction));
    assert_eq!(run(&t, &plan::operate_inputs(&SECRET, 99, 85)), Err(Refusal::Instruction));
    // A zero price cannot be set, nor one at or above 2^63.
    let mut z = t.clone();
    z.writes[0].1 = [0; 8];
    assert_eq!(run(&z, &plan::operate_inputs(&SECRET, 100, 0)), Err(Refusal::Range));
    assert!(plan::operate(None, 1 << 63).is_err());
}

#[test]
fn place_escrows_what_came_in_under_a_fresh_ticket() {
    let o = stop(90);
    let (t, order) = plan::place(key(), [0; 8], AMOUNT, &o, test_hash).unwrap();
    assert_eq!(order.amount, AMOUNT);
    assert_eq!(order.c, commitment(&o, test_hash));
    assert_eq!(run(&t, &plan::inputs(PLACE, &TICKET, &o)).unwrap()[0], PLACE);
    // Nothing in, a token in beside the RAND, a cell already taken, another's ticket, a kind that
    // is neither a stop nor a take-profit.
    let mut dry = t.clone();
    dry.burn_r = 0;
    assert_eq!(run(&dry, &plan::inputs(PLACE, &TICKET, &o)), Err(Refusal::Zero));
    assert_eq!(run(&t.clone().deposit(3, 1), &plan::inputs(PLACE, &TICKET, &o)), Err(Refusal::Inflow));
    let mut taken = t.clone();
    taken.reads[0].1 = order.value();
    assert_eq!(run(&taken, &plan::inputs(PLACE, &TICKET, &o)), Err(Refusal::Occupied));
    assert_eq!(run(&t, &plan::inputs(PLACE, &OTHER, &o)), Err(Refusal::Key));
    assert_eq!(run(&t, &plan::inputs(PLACE, &TICKET, &Opening { kind: 3, ..o })), Err(Refusal::Kind));
    assert!(plan::place(key(), order.value(), AMOUNT, &o, test_hash).is_err());
}

#[test]
fn a_stop_fires_at_or_below_and_a_take_profit_at_or_above() {
    for (o, fires_at, holds_at) in [(stop(90), [90, 89, 1], [91, 100]), (take_profit(90), [90, 91, u64::MAX >> 1], [89, 1])] {
        for p in fires_at {
            assert!(fires(o.kind, p, o.threshold));
            let t = fire_at(p, &o);
            assert_eq!(run(&t, &plan::inputs(FIRE, &TICKET, &o)).unwrap(), [FIRE, AMOUNT as u32, (AMOUNT >> 32) as u32, 0, 0, 0, 0, 0]);
        }
        for p in holds_at {
            assert!(!fires(o.kind, p, o.threshold));
            assert!(plan::fire(key(), Oracle { price: p }, placed(&o), &o, test_hash, None).is_err());
            assert_eq!(run(&forced_fire(p, placed(&o)), &plan::inputs(FIRE, &TICKET, &o)), Err(Refusal::Condition));
        }
    }
}

#[test]
fn only_the_opening_the_order_was_placed_with_fires_it() {
    let o = stop(80);
    let order = placed(&o);
    // At price 100 a stop at 100 would fire — but this order is not that stop, and saying so
    // fails the commitment, not the condition.
    for lie in [Opening { threshold: 100, ..o }, Opening { kind: TAKE_PROFIT, ..o }, Opening { salt: [1, 2, 3, 5], ..o }] {
        assert!(plan::fire(key(), Oracle { price: 100 }, order, &lie, test_hash, None).is_err());
        assert_eq!(run(&forced_fire(100, order), &plan::inputs(FIRE, &TICKET, &lie)), Err(Refusal::Commitment));
    }
    // The right opening, the wrong ticket: the cell is not the one that ticket names.
    assert_eq!(run(&forced_fire(80, order), &plan::inputs(FIRE, &OTHER, &o)), Err(Refusal::Key));
}

#[test]
fn fire_and_cancel_release_exactly_the_escrow_once() {
    let o = stop(90);
    let order = placed(&o);
    for (t, input) in [(fire_at(85, &o), plan::inputs(FIRE, &TICKET, &o)), (plan::cancel(key(), order), plan::inputs(CANCEL, &TICKET, &o))] {
        assert!(run(&t, &input).is_ok());
        let mut more = t.clone();
        more.pays[0].amount += 1;
        assert_eq!(run(&more, &input), Err(Refusal::Amount));
        let mut less = t.clone();
        less.pays[0].amount -= 1;
        assert_eq!(run(&less, &input), Err(Refusal::Amount));
        // The order's cell must be deleted, not kept or shrunk.
        let mut kept = t.clone();
        let i = kept.writes.iter().position(|(k, _)| *k == key()).unwrap();
        kept.writes[i].1 = Order { amount: 1, ..order }.value();
        assert_eq!(run(&kept, &input), Err(Refusal::Value));
        // Once released the cell reads as zeros, and zeros are no order: the escrow cannot be
        // released twice (and on chain the first release makes the second a stale read anyway).
        let mut again = t.clone();
        let i = again.reads.iter().position(|(k, _)| *k == key()).unwrap();
        again.reads[i].1 = [0; 8];
        assert_eq!(run(&again, &input), Err(Refusal::NoOrder));
    }
    // A cancel needs the ticket and the opening it was placed with, but no price at all.
    assert_eq!(run(&plan::cancel(key(), order), &plan::inputs(CANCEL, &OTHER, &o)), Err(Refusal::Key));
    assert_eq!(run(&plan::cancel(key(), order), &plan::inputs(CANCEL, &TICKET, &stop(91))), Err(Refusal::Commitment));
}

#[test]
fn shape_and_inflow_are_pinned() {
    let o = stop(90);
    let t = fire_at(85, &o);
    let input = plan::inputs(FIRE, &TICKET, &o);
    // A second payout, a mint, RAND coming in, a token coming in, the oracle not written back,
    // the oracle changed on the way, a fire against a missing oracle.
    let mut a = t.clone();
    a.pays.push(Out::new(0, 1));
    assert_eq!(run(&a, &input), Err(Refusal::Shape));
    let mut b = t.clone();
    b.mints.push(Out::new(3, 1));
    assert_eq!(run(&b, &input), Err(Refusal::Shape));
    assert_eq!(run(&t.clone().rand_in(1), &input), Err(Refusal::Inflow));
    assert_eq!(run(&t.clone().deposit(3, 1), &input), Err(Refusal::Inflow));
    let mut c = t.clone();
    c.writes.remove(0);
    assert_eq!(run(&c, &input), Err(Refusal::Shape));
    let mut d = t.clone();
    d.writes[0].1 = Oracle { price: 84 }.value();
    assert_eq!(run(&d, &input), Err(Refusal::Value));
    let mut e = t.clone();
    e.reads[0].1 = [0; 8];
    e.writes[0].1 = [0; 8];
    assert_eq!(run(&e, &input), Err(Refusal::NoOracle));
    // A cancel with the fire's shape, and a fire with the cancel's.
    assert_eq!(run(&t, &plan::inputs(CANCEL, &TICKET, &o)), Err(Refusal::Shape));
    assert_eq!(run(&plan::cancel(key(), placed(&o)), &input), Err(Refusal::Shape));
    // The oracle under another key.
    let mut f = t.clone();
    f.reads[0].0 = [3, 0, 0, 0, 0, 0, 0, 0];
    f.writes[0].0 = [3, 0, 0, 0, 0, 0, 0, 0];
    assert_eq!(run(&f, &input), Err(Refusal::Key));
}

#[test]
fn no_word_is_left_unchecked() {
    let (s, tp) = (stop(90), take_profit(90));
    let cases = [
        (plan::operate(None, 100).unwrap(), plan::operate_inputs(&SECRET, 0, 100)),
        (plan::operate(Some(Oracle { price: 100 }), 85).unwrap(), plan::operate_inputs(&SECRET, 100, 85)),
        (plan::place(key(), [0; 8], AMOUNT, &s, test_hash).unwrap().0, plan::inputs(PLACE, &TICKET, &s)),
        (fire_at(85, &s), plan::inputs(FIRE, &TICKET, &s)),
        (fire_at(95, &tp), plan::inputs(FIRE, &TICKET, &tp)),
        (plan::cancel(key(), placed(&s)), plan::inputs(CANCEL, &TICKET, &s)),
    ];
    for (t, input) in cases {
        let loose = loose_words(&lock(), &t, &input, test_hash, check);
        assert!(loose.is_empty(), "method {}: words accepted when changed: {loose:?}", input[0]);
    }
}

#[test]
fn the_receipt_never_shows_the_threshold() {
    // Whatever the threshold, an accepted fire's outputs are the method and the escrow only.
    for thr in [1u64, 85, 90, 12_345_678_901_234] {
        let o = stop(thr);
        let out = run(&fire_at(1, &o), &plan::inputs(FIRE, &TICKET, &o)).unwrap();
        assert_eq!(out, [FIRE, AMOUNT as u32, (AMOUNT >> 32) as u32, 0, 0, 0, 0, 0]);
        // Nor does the cell: the escrow, the commitment and the version, and the commitment's
        // words are a hash's, not the threshold's.
        let cell = placed(&o).value();
        assert_eq!(cell[7], stoploss_core::VERSION);
        assert!(!cell[2..7].contains(&(thr as u32)));
        assert!(!cell[2..7].contains(&((thr >> 32) as u32)));
    }
}

#[test]
fn every_segment_fits() {
    let o = stop(90);
    for t in [plan::operate(None, 100).unwrap(), plan::place(key(), [0; 8], AMOUNT, &o, test_hash).unwrap().0, fire_at(85, &o), plan::cancel(key(), placed(&o))] {
        assert!(t.fits(8), "{} context words do not fit with an eight-word public input", t.context().len());
    }
    // fire, the widest, leaves room for a venue's pool: one more read and one more write.
    let fire = fire_at(85, &o);
    assert_eq!(fire.context().len(), 78);
    assert!(8 + 8 + 78 + 32 <= 127);
}
