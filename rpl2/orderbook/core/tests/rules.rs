//! The orderbook's rules, on the host: every method accepted at its best amounts and refused one
//! unit past them, no context word of any accepted transition left unchecked, and the maker paid
//! exactly what was asked.
use orderbook_core::kit::host::{loose_words, test_hash, Mock, Out, Transition};
use orderbook_core::plan::{self, Fill};
use orderbook_core::{check, price_ok, Order, Refusal, CLOSE, FILL, POST};

const T: u32 = 5;
const U: u64 = 1_000_000_000;
const TA: [u32; 8] = [1, 2, 3, 4, 5, 6, 7, 8];
const TB: [u32; 8] = [9, 10, 11, 12, 13, 14, 15, 16];

fn run(t: &Transition, input: &[u32]) -> Result<[u32; 8], Refusal> {
    let m = Mock::new(&[], t, input, test_hash);
    let r = check(&m);
    if m.oob.get() {
        Err(Refusal::Shape)
    } else {
        r
    }
}

/// Post's private inputs for the order `t` posts, as `plan` states them.
fn pi(ticket: &[u32; 8], t: &Transition) -> Vec<u32> {
    plan::inputs(POST, ticket, &Order::from_value(&t.writes[0].1).unwrap())
}

fn key(ticket: &[u32; 8]) -> [u32; 8] {
    plan::key_of(ticket, test_hash)
}

/// 10 RAND for 25 tokens, posted with ticket A.
fn posted() -> Order {
    plan::post(key(&TA), [0; 8], 0, 10 * U, T, 25 * U).unwrap().1
}

/// A, filled for 10 tokens: 4 RAND out.
fn partly() -> Order {
    plan::fill(key(&TA), posted(), Fill::Pay(10 * U)).unwrap().1
}

#[test]
fn post_takes_the_escrow_and_nothing_else() {
    for (give, want) in [(0, T), (T, 0), (T, 7)] {
        let (t, o) = plan::post(key(&TA), [0; 8], give, 3 * U, want, 4 * U).unwrap();
        let inp = plan::inputs(POST, &TA, &o);
        assert_eq!(run(&t, &inp).unwrap()[..5], [POST, (3 * U) as u32, 0, (4 * U) as u32, 0]);
        // Escrow one unit short; an ask of nothing; the same asset on both sides; proceeds from nowhere.
        let mut g = t.clone();
        if give == 0 { g.burn_r -= 1 } else { g.burn_a -= 1 }
        assert_eq!(run(&g, &inp), Err(Refusal::Value));
        for bad in [Order { want_rem: 0, ..o }, Order { want: give, ..o }, Order { proceeds: 1, ..o }] {
            let mut g = t.clone();
            g.writes[0].1 = bad.value();
            assert!(run(&g, &inp).is_err(), "{bad:?}");
            assert!(run(&g, &plan::inputs(POST, &TA, &bad)).is_err(), "{bad:?}");
        }
        // Terms other than the maker stated.
        for bad in [Order { want_rem: o.want_rem + 1, ..o }, Order { want: 9, ..o }] {
            assert_eq!(run(&t, &plan::inputs(POST, &TA, &bad)), Err(Refusal::Value));
        }
    }
    // Under a key the ticket does not name, or over a live order.
    let (t, _) = plan::post(key(&TA), [0; 8], 0, U, T, U).unwrap();
    assert_eq!(run(&t, &plan::inputs(POST, &TB, &Order::from_value(&t.writes[0].1).unwrap())), Err(Refusal::Key));
    let mut over = t.clone();
    over.reads[0].1 = posted().value();
    assert_eq!(run(&over, &pi(&TA, &t)), Err(Refusal::Order));
    // Two assets in.
    assert_eq!(run(&t.clone().deposit(T, 1), &pi(&TA, &t)), Err(Refusal::Inflow));
}

#[test]
fn fill_accepts_its_best_and_refuses_one_more() {
    let o = posted();
    for y in [3, 7, 3 * U + 1, 10 * U, 25 * U] {
        let (t, _, x, _) = plan::fill(key(&TA), o, Fill::Pay(y)).unwrap_or_else(|e| panic!("{y}: {e}"));
        assert!(run(&t, &[FILL]).is_ok());
        assert_eq!(x, (y as u128 * o.give_rem as u128 / o.want_rem as u128) as u64);
        // One unit more out for the same in is under the price.
        if x < o.give_rem {
            assert_eq!(run(&plan::fill_exact(key(&TA), o, x + 1, y), &[FILL]), Err(Refusal::Price));
        }
    }
    for x in [1, 3, 4 * U + 1, 10 * U] {
        let (t, _, _, y) = plan::fill(key(&TA), o, Fill::Take(x)).unwrap();
        assert!(run(&t, &[FILL]).is_ok());
        assert_eq!(run(&plan::fill_exact(key(&TA), o, x, y - 1), &[FILL]), Err(Refusal::Price));
    }
    // More than the order holds or asks, and nothing for nothing.
    assert_eq!(run(&plan::fill_exact(key(&TA), o, o.give_rem + 1, o.want_rem), &[FILL]), Err(Refusal::Order));
    assert_eq!(run(&plan::fill_exact(key(&TA), o, 1, o.want_rem + 1), &[FILL]), Err(Refusal::Order));
    assert_eq!(run(&plan::fill_exact(key(&TA), o, 0, 1), &[FILL]), Err(Refusal::Zero));
}

#[test]
fn fill_is_pinned_to_the_order() {
    let o = partly();
    let (t, after, _, _) = plan::fill(key(&TA), o, Fill::Pay(U)).unwrap();
    // Paid in the wrong asset, paid out in the wrong asset, proceeds not credited, a second key.
    let mut a = t.clone();
    a.burn_asset = 7;
    assert_eq!(run(&a, &[FILL]), Err(Refusal::Asset));
    let mut b = t.clone();
    b.pays[0].asset = T;
    assert_eq!(run(&b, &[FILL]), Err(Refusal::Asset));
    let mut c = t.clone();
    c.writes[0].1 = Order { proceeds: o.proceeds, ..after }.value();
    assert_eq!(run(&c, &[FILL]), Err(Refusal::Value));
    let mut d = t.clone();
    d.writes[0].0 = key(&TB);
    assert_eq!(run(&d, &[FILL]), Err(Refusal::Key));
    // A second payout, a mint, an absent order.
    let mut e = t.clone();
    e.pays.push(Out::new(0, 1));
    assert_eq!(run(&e, &[FILL]), Err(Refusal::Shape));
    let mut f = t.clone();
    f.mints.push(Out::new(T, 1));
    assert_eq!(run(&f, &[FILL]), Err(Refusal::Shape));
    let mut g = t.clone();
    g.reads[0].1 = [0; 8];
    assert_eq!(run(&g, &[FILL]), Err(Refusal::Order));
}

#[test]
fn close_pays_exactly_what_is_left_then_the_proceeds() {
    // Unfilled: the escrow back. Part filled: both. Filled: the proceeds.
    let full = plan::fill(key(&TA), partly(), Fill::Pay(15 * U)).unwrap().1;
    for (o, pays) in [(posted(), vec![(0, 10 * U)]), (partly(), vec![(0, 6 * U), (T, 10 * U)]), (full, vec![(T, 25 * U)])] {
        let t = plan::close(key(&TA), o);
        assert_eq!(t.pays.iter().map(|p| (p.asset, p.amount)).collect::<Vec<_>>(), pays);
        assert!(run(&t, &plan::inputs(CLOSE, &TA, &o)).is_ok());
        assert_eq!(run(&t, &plan::inputs(CLOSE, &TB, &o)), Err(Refusal::Key));
        for i in 0..t.pays.len() {
            let mut g = t.clone();
            g.pays[i].amount += 1;
            assert_eq!(run(&g, &plan::inputs(CLOSE, &TA, &o)), Err(Refusal::Asset));
        }
        let mut g = t.clone();
        g.pays.push(Out::new(0, 1));
        assert_eq!(run(&g, &plan::inputs(CLOSE, &TA, &o)), Err(Refusal::Shape));
        let mut g = t.clone();
        g.pays.reverse();
        if pays.len() == 2 {
            assert_eq!(run(&g, &plan::inputs(CLOSE, &TA, &o)), Err(Refusal::Asset));
        }
        let mut g = t.clone();
        g.writes[0].1 = o.value();
        assert_eq!(run(&g, &plan::inputs(CLOSE, &TA, &o)), Err(Refusal::Value));
        assert_eq!(run(&t.clone().rand_in(1), &plan::inputs(CLOSE, &TA, &o)), Err(Refusal::Inflow));
        // Terms other than the order read.
        for bad in [Order { want_rem: o.want_rem + 1, ..o }, Order { give: 9, ..o }, Order { want: 9, ..o }] {
            assert_eq!(run(&t, &plan::inputs(CLOSE, &TA, &bad)), Err(Refusal::Value));
        }
    }
}

#[test]
fn no_word_is_left_unchecked() {
    let o = partly();
    let full = plan::fill(key(&TA), o, Fill::Pay(15 * U)).unwrap().1;
    let token_order = plan::post(key(&TB), [0; 8], T, 30 * U, 0, 12 * U).unwrap();
    let cases: Vec<(Transition, Vec<u32>)> = vec![
        (plan::post(key(&TA), [0; 8], 0, 10 * U, T, 25 * U).unwrap().0, plan::inputs(POST, &TA, &posted())),
        (token_order.0.clone(), plan::inputs(POST, &TB, &token_order.1)),
        (plan::fill(key(&TA), posted(), Fill::Pay(10 * U)).unwrap().0, vec![FILL]),
        (plan::fill(key(&TB), token_order.1, Fill::Pay(U)).unwrap().0, vec![FILL]),
        (plan::close(key(&TA), posted()), plan::inputs(CLOSE, &TA, &posted())),
        (plan::close(key(&TA), o), plan::inputs(CLOSE, &TA, &o)),
        (plan::close(key(&TA), full), plan::inputs(CLOSE, &TA, &full)),
    ];
    for (t, input) in cases {
        let loose = loose_words(&[], &t, &input, test_hash, check);
        assert!(loose.is_empty(), "method {}: words accepted when changed: {loose:?}", input[0]);
    }
}

/// Walk an order through fills of every size, overpaying sometimes: the remaining price never
/// rises, the maker has always been paid at least the posted price for what was filled, the
/// escrow runs out exactly when the ask does, and a full fill pays the maker exactly `want`.
#[test]
fn the_maker_gets_exactly_want_for_give() {
    let (give, want) = (7_000_000_003u64, 19_000_000_011u64);
    let mut seed = 0x2545_f491_4f6c_dd1du64;
    for _ in 0..200 {
        let mut o = plan::post(key(&TA), [0; 8], 0, give, T, want).unwrap().1;
        let mut paid_in = 0u64;
        let mut paid_out = 0u64;
        while o.give_rem > 0 {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            let y = 1 + seed % o.want_rem.min(want / 3);
            let (x, y) = if seed & 4 == 0 || o.give_rem == 1 {
                (plan::most_for(&o, y), y)
            } else {
                // Overpay: take less than the most y buys (but at least one unit).
                ((plan::most_for(&o, y) / 2).max(1), y)
            };
            if x == 0 || !price_ok(o.give_rem, o.want_rem, x, y) {
                continue;
            }
            let t = plan::fill_exact(key(&TA), o, x, y);
            assert!(run(&t, &[FILL]).is_ok());
            let next = Order::from_value(&t.writes[0].1).unwrap();
            // The remaining price never rises: w'/g' ≤ w/g.
            assert!((next.want_rem as u128) * (o.give_rem as u128) <= (o.want_rem as u128) * (next.give_rem as u128));
            o = next;
            paid_in += y;
            paid_out += x;
            assert_eq!(o.proceeds, paid_in);
            assert_eq!(o.proceeds + o.want_rem, want);
            assert_eq!(o.give_rem + paid_out, give);
            // At least the posted price for everything filled: paid_in / paid_out ≥ want / give.
            assert!((paid_in as u128) * (give as u128) >= (paid_out as u128) * (want as u128));
            if o.want_rem == 0 {
                break;
            }
        }
        if o.give_rem == 0 {
            assert_eq!(o.want_rem, 0, "the escrow ran out before the ask");
        }
        // Whatever the path, closing pays the maker back give_rem and want − want_rem: when filled,
        // exactly `want` for `give`.
        let t = plan::close(key(&TA), o);
        assert!(run(&t, &plan::inputs(CLOSE, &TA, &o)).is_ok());
        let got: u64 = t.pays.iter().filter(|p| p.asset == T).map(|p| p.amount).sum();
        let back: u64 = t.pays.iter().filter(|p| p.asset == 0).map(|p| p.amount).sum();
        assert_eq!(got, want - o.want_rem);
        assert_eq!(back + paid_out, give);
        if o.give_rem == 0 {
            assert_eq!(got, want);
        }
    }
}

#[test]
fn a_completed_order_takes_no_more_fills() {
    let full = plan::fill(key(&TA), partly(), Fill::Pay(15 * U)).unwrap().1;
    assert_eq!((full.give_rem, full.want_rem, full.proceeds), (0, 0, 25 * U));
    assert_eq!(run(&plan::fill_exact(key(&TA), full, 1, 1), &[FILL]), Err(Refusal::Order));
    // An order whose ask is met with escrow left (a taker overpaid it to zero) is complete too.
    let o = posted();
    let t = plan::fill_exact(key(&TA), o, 9 * U, 25 * U);
    assert!(run(&t, &[FILL]).is_ok());
    let met = Order::from_value(&t.writes[0].1).unwrap();
    assert_eq!(run(&plan::fill_exact(key(&TA), met, 1, 1), &[FILL]), Err(Refusal::Order));
    let t = plan::close(key(&TA), met);
    assert_eq!(t.pays.iter().map(|p| (p.asset, p.amount)).collect::<Vec<_>>(), vec![(0, U), (T, 25 * U)]);
}
