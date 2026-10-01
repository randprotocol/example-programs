//! perp's rules, on the host: every method accepted at its best amounts and refused one unit past
//! them, no context word of any accepted transition left unchecked, and the pool solvent through
//! a long random sequence of trades, price moves and liquidity changes.
use perp_core::kit::host::{loose_words, test_hash, Mock, Out, Transition};
use perp_core::kit::RAND;
use perp_core::plan::{self, inputs, key_of, lock_of};
use perp_core::*;

const LP: u32 = 6;
const R: u64 = 1_000_000_000;
const OPERATOR: [u32; 8] = [1, 2, 3, 4, 5, 6, 7, 8];
const ALICE: [u32; 8] = [9, 9, 9, 9, 9, 9, 9, 1];
const BOB: [u32; 8] = [9, 9, 9, 9, 9, 9, 9, 2];

fn public() -> [u32; 8] {
    lock_of(&OPERATOR, test_hash)
}

fn run(t: &Transition, input: &[u32]) -> Result<[u32; 8], Refusal> {
    let m = Mock::new(&public(), t, input, test_hash);
    let r = check(&m);
    if m.oob.get() { Err(Refusal::Shape) } else { r }
}

fn op(t: &Transition) -> Vec<u32> {
    plan::operate_inputs(&OPERATOR, t)
}

/// A market at 2 RAND with 1000 RAND of liquidity and no positions.
fn funded() -> Market {
    let (_, m) = plan::operate(None, 2 * R, LP).unwrap();
    plan::lp_add(m, Oi::default(), 1_000 * R).unwrap().1
}

/// `funded()` with Alice long at 10x (10 on 100) and Bob short at 2x (50 on 100).
fn with_positions() -> (Market, Oi, Position, Position) {
    let m = funded();
    let (_, oi, pa) = plan::open(m, Oi::default(), key_of(&ALICE, test_hash), LONG, 10 * R, 100 * R).unwrap();
    let (_, oi, pb) = plan::open(m, oi, key_of(&BOB, test_hash), SHORT, 50 * R, 100 * R).unwrap();
    (m, oi, pa, pb)
}

fn at(m: Market, price: u64) -> Market {
    Market { price, ..m }
}

#[test]
fn the_operator_creates_the_market_and_alone_moves_the_price() {
    let (t, m) = plan::operate(None, 2 * R, LP).unwrap();
    assert_eq!(run(&t, &op(&t)).unwrap()[0], OPERATE);
    assert_eq!(run(&t, &plan::operate_inputs(&ALICE, &t)), Err(Refusal::Operator));
    // The LP token cannot be RAND; the price cannot be zero; the pool starts empty.
    let mut bad = t.clone();
    bad.writes[0].1 = Market { lp: RAND, ..m }.value();
    assert_eq!(run(&bad, &op(&bad)), Err(Refusal::Asset));
    bad.writes[0].1 = Market { price: 0, ..m }.value();
    assert_eq!(run(&bad, &op(&bad)), Err(Refusal::Zero));
    bad.writes[0].1 = Market { cash: 1, ..m }.value();
    assert_eq!(run(&bad, &op(&bad)), Err(Refusal::Value));

    let m = funded();
    let (t, after) = plan::operate(Some(m), 3 * R, LP).unwrap();
    assert!(run(&t, &op(&t)).is_ok());
    for moved in [Market { cash: m.cash + 1, ..after }, Market { supply: m.supply - 1, ..after }, Market { lp: 7, ..after }] {
        let mut bad = t.clone();
        bad.writes[0].1 = moved.value();
        assert_eq!(run(&bad, &op(&bad)), Err(Refusal::Value));
    }
}

#[test]
fn every_method_accepts_its_best_and_refuses_one_more() {
    let (m, oi, pa, pb) = with_positions();
    let (ka, kb) = (key_of(&ALICE, test_hash), key_of(&BOB, test_hash));

    // lp_add: the NAV-priced mint, and one more.
    let (t, after, minted) = plan::lp_add(m, oi, 7 * R + 3).unwrap();
    assert!(run(&t, &inputs(LP_ADD, None)).is_ok());
    let mut g = t.clone();
    g.mints[0].amount = minted + 1;
    g.writes[0].1 = Market { supply: after.supply + 1, ..after }.value();
    assert_eq!(run(&g, &inputs(LP_ADD, None)), Err(Refusal::Nav));

    // lp_remove: the share, and one more.
    let (t, _, x) = plan::lp_remove(m, oi, 123 * R + 7).unwrap();
    assert!(run(&t, &inputs(LP_REMOVE, None)).is_ok());
    let (g, _) = plan::lp_remove_tx(m, oi, 123 * R + 7, x + 1);
    assert_eq!(run(&g, &inputs(LP_REMOVE, None)), Err(Refusal::Nav));

    // open: the best size each side, one unit better for the trader refused; 10x, not more.
    let price = 3 * R + 1; // a price that does not divide evenly
    let m3 = at(funded(), price);
    for side in [LONG, SHORT] {
        let (t, _, p) = plan::open(m3, Oi::default(), ka, side, 10 * R, 100 * R).unwrap();
        assert!(run(&t, &inputs(OPEN, Some(&ALICE))).is_ok());
        let better = Position { q: if side == LONG { p.q + 1 } else { p.q - 1 }, ..p };
        let (g, _) = plan::open_tx(m3, Oi::default(), ka, better);
        assert_eq!(run(&g, &inputs(OPEN, Some(&ALICE))), Err(Refusal::Price));
        let over = Position { cost: 100 * R + 1, ..p };
        let (g, _) = plan::open_tx(m3, Oi::default(), ka, over);
        assert_eq!(run(&g, &inputs(OPEN, Some(&ALICE))), Err(Refusal::Leverage));
    }

    // close: the best payout, and one more — at a profit and at a loss.
    for price in [25 * R / 10, 19 * R / 10] {
        let mp = at(m, price);
        let (t, _, _, x) = plan::close(mp, oi, ka, pa);
        assert!(x > 0);
        assert!(run(&t, &inputs(CLOSE, Some(&ALICE))).is_ok());
        let (g, _, _) = plan::settle_tx(mp, oi, ka, pa, x + 1);
        assert_eq!(run(&g, &inputs(CLOSE, Some(&ALICE))), Err(Refusal::Payout));
    }
    // 2.5 → 35 RAND for Alice: 10 margin, 25 profit.
    assert_eq!(plan::close_quote(&pa, 25 * R / 10), 35 * R);

    // liquidate: Bob is short 50 X (100 RAND at 2) on 50 of margin, so his equity is 150 − 50 · P.
    // At 2.9 it is 5 RAND, 5 % of the notional — not below; at 2.91 it is 4.5, liquidatable.
    assert!(!liquidatable(&pb, 29 * R / 10));
    let ml = at(m, 291 * R / 100);
    let (t, _, _, x) = plan::liquidate(ml, oi, kb, pb).unwrap();
    assert_eq!(x, R); // capped at 1 % of 100 RAND
    assert!(run(&t, &inputs(LIQUIDATE, None)).is_ok());
    let (g, _, _) = plan::settle_tx(ml, oi, kb, pb, x + 1);
    assert_eq!(run(&g, &inputs(LIQUIDATE, None)), Err(Refusal::Payout));
    // Deeper: equity 0.15 RAND, so the reward is the equity, and one unit more is refused.
    let md = at(m, 2_997 * R / 1_000);
    let (t, _, _, x) = plan::liquidate(md, oi, kb, pb).unwrap();
    assert_eq!(x, 15 * R / 100);
    assert!(run(&t, &inputs(LIQUIDATE, None)).is_ok());
    let (g, _, _) = plan::settle_tx(md, oi, kb, pb, x + 1);
    assert_eq!(run(&g, &inputs(LIQUIDATE, None)), Err(Refusal::Payout));
}

#[test]
fn no_word_is_left_unchecked() {
    let (m, oi, pa, pb) = with_positions();
    let (ka, kb) = (key_of(&ALICE, test_hash), key_of(&BOB, test_hash));
    let fresh = funded();
    let (first, _) = plan::operate(None, 2 * R, LP).unwrap();
    let (_, empty) = plan::operate(None, 2 * R, LP).unwrap();
    let reprice = plan::operate(Some(m), 3 * R, LP).unwrap().0;
    let cases: Vec<(Transition, Vec<u32>)> = vec![
        (first.clone(), op(&first)),
        (reprice.clone(), op(&reprice)),
        (plan::lp_add(empty, Oi::default(), 1_000 * R).unwrap().0, inputs(LP_ADD, None)),
        (plan::lp_add(m, oi, 7 * R).unwrap().0, inputs(LP_ADD, None)),
        (plan::lp_remove(m, oi, 100 * R).unwrap().0, inputs(LP_REMOVE, None)),
        (plan::open(fresh, Oi::default(), ka, LONG, 10 * R, 100 * R).unwrap().0, inputs(OPEN, Some(&ALICE))),
        (plan::open(fresh, Oi::default(), kb, SHORT, 10 * R, 30 * R).unwrap().0, inputs(OPEN, Some(&BOB))),
        (plan::close(at(m, 25 * R / 10), oi, ka, pa).0, inputs(CLOSE, Some(&ALICE))),
        // Under water: closed for nothing (no payout at all).
        (plan::close(at(m, R), oi, ka, pa).0, inputs(CLOSE, Some(&ALICE))),
        (plan::liquidate(at(m, 291 * R / 100), oi, kb, pb).unwrap().0, inputs(LIQUIDATE, None)),
        (plan::liquidate(at(m, 4 * R), oi, kb, pb).unwrap().0, inputs(LIQUIDATE, None)),
    ];
    assert_eq!(cases[8].0.pays.len(), 0);
    assert_eq!(cases[10].0.pays.len(), 0);
    for (t, input) in cases {
        let loose = loose_words(&public(), &t, &input, test_hash, check);
        assert!(loose.is_empty(), "method {}: words accepted when changed: {loose:?}", input[0]);
    }
}

#[test]
fn profit_is_capped_at_the_notional() {
    let (m, oi, pa, _) = with_positions();
    let ka = key_of(&ALICE, test_hash);
    // X triples: an uncapped long would make 200 on 100; it is paid margin + 100.
    let (t, _, _, x) = plan::close(at(m, 6 * R), oi, ka, pa);
    assert_eq!(x, 110 * R);
    assert!(run(&t, &inputs(CLOSE, Some(&ALICE))).is_ok());
    // A short's profit is at most its notional anyway (X at zero): price 1 base unit.
    let (_, _, pb) = plan::open(funded(), Oi::default(), ka, SHORT, 50 * R, 100 * R).unwrap();
    assert!(plan::close_quote(&pb, 1) <= 150 * R);
}

#[test]
fn the_reserve_holds_on_open_and_on_withdrawal() {
    let (_, m) = plan::operate(None, 2 * R, LP).unwrap();
    let (_, m, _) = plan::lp_add(m, Oi::default(), 100 * R).unwrap();
    let ka = key_of(&ALICE, test_hash);
    // 100 RAND of pool backs 100 of notional, not one unit more.
    assert!(plan::open(m, Oi::default(), ka, LONG, 10 * R, 100 * R).is_ok());
    let p = Position { margin: 11 * R, q: plan::size_for(LONG, 100 * R + 1, m.price), cost: 100 * R + 1, side: LONG };
    let (t, _) = plan::open_tx(m, Oi::default(), ka, p);
    assert_eq!(run(&t, &inputs(OPEN, Some(&ALICE))), Err(Refusal::Reserve));
    // With 100 of notional open, the pool's cash above it is all an LP may take.
    let (_, oi, _) = plan::open(m, Oi::default(), ka, LONG, 10 * R, 60 * R).unwrap();
    assert!(plan::lp_remove(m, oi, m.supply).is_err());
    let (t, _) = plan::lp_remove_tx(m, oi, 40 * R, 40 * R);
    assert!(run(&t, &inputs(LP_REMOVE, None)).is_ok());
    let (t, _) = plan::lp_remove_tx(m, oi, 41 * R, 40 * R + 1);
    assert_eq!(run(&t, &inputs(LP_REMOVE, None)), Err(Refusal::Nav));
}

#[test]
fn positions_belong_to_their_secret() {
    let (m, oi, pa, pb) = with_positions();
    let (ka, kb) = (key_of(&ALICE, test_hash), key_of(&BOB, test_hash));
    // Bob cannot close Alice's position, nor open a second one under his key.
    let (t, _, _, _) = plan::close(m, oi, ka, pa);
    assert_eq!(run(&t, &inputs(CLOSE, Some(&BOB))), Err(Refusal::Key));
    let (mut t, _) = plan::open_tx(m, oi, kb, Position { margin: R, q: 1, cost: R, side: LONG });
    t.reads[2].1 = pb.value();
    assert_eq!(run(&t, &inputs(OPEN, Some(&BOB))), Err(Refusal::Exists));
    // A healthy position cannot be liquidated, at any reward.
    let (t, _, _) = plan::settle_tx(m, oi, kb, pb, 0);
    assert_eq!(run(&t, &inputs(LIQUIDATE, None)), Err(Refusal::Healthy));
    // A liquidation's key must be a position's.
    let (mut t, _, _, _) = plan::liquidate(at(m, 3 * R), oi, kb, pb).unwrap();
    assert!(run(&t, &inputs(LIQUIDATE, None)).is_ok());
    t.reads[2].0[0] = 4;
    t.writes[2].0[0] = 4;
    assert_eq!(run(&t, &inputs(LIQUIDATE, None)), Err(Refusal::Key));
}

#[test]
fn shape_and_inflow_are_pinned() {
    let (m, oi, pa, _) = with_positions();
    let ka = key_of(&ALICE, test_hash);
    let (t, _, _, _) = plan::close(at(m, 25 * R / 10), oi, ka, pa);
    let mut a = t.clone();
    a.pays.push(Out::new(0, 1));
    assert_eq!(run(&a, &inputs(CLOSE, Some(&ALICE))), Err(Refusal::Shape));
    let mut b = t.clone();
    b.mints.push(Out::new(LP, 1));
    assert_eq!(run(&b, &inputs(CLOSE, Some(&ALICE))), Err(Refusal::Shape));
    let c = t.clone().rand_in(1);
    assert_eq!(run(&c, &inputs(CLOSE, Some(&ALICE))), Err(Refusal::Inflow));
    let mut d = t.clone();
    d.pays[0].asset = LP;
    assert_eq!(run(&d, &inputs(CLOSE, Some(&ALICE))), Err(Refusal::Asset));
    // LP minted or burned must be the market's own token.
    let (t, _, _) = plan::lp_add(m, oi, R).unwrap();
    let mut e = t.clone();
    e.mints[0].asset = 7;
    assert_eq!(run(&e, &inputs(LP_ADD, None)), Err(Refusal::Asset));
    let (t, _, _) = plan::lp_remove(m, oi, R).unwrap();
    let f = t.clone().burn(7, R);
    assert_eq!(run(&f, &inputs(LP_REMOVE, None)), Err(Refusal::Inflow));
    // Trading against a market that does not exist.
    let (mut g, _) = plan::open_tx(m, Oi::default(), ka, pa);
    g.reads[0].1 = [0; 8];
    assert_eq!(run(&g, &inputs(OPEN, Some(&ALICE))), Err(Refusal::Market));
}

/// A tiny deterministic generator for the solvency run.
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.0 >> 33
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

#[test]
fn the_vault_always_covers_every_payout() {
    let mut rng = Lcg(0x5eed);
    let (t, mut m) = plan::operate(None, 2 * R, LP).unwrap();
    run(&t, &op(&t)).unwrap();
    let mut oi = Oi::default();
    let mut vault: u64 = 0; // the program's RAND: everything in less everything out
    let mut open: Vec<([u32; 8], [u32; 8], Position)> = vec![];
    let mut next_secret = 100u32;
    let (mut opened, mut closed, mut liquidated, mut refused_reserve) = (0, 0, 0, 0);

    let invariant = |m: &Market, oi: &Oi, vault: u64, open: &[([u32; 8], [u32; 8], Position)]| {
        let margins: u64 = open.iter().map(|(_, _, p)| p.margin).sum();
        assert_eq!(vault, m.cash + margins, "the vault is the pool plus the margins");
        assert!(reserve_ok(m.cash, oi), "the reserve broke: {m:?} {oi:?}");
        let sum = |side| open.iter().filter(|(_, _, p)| p.side == side).fold((0, 0), |(q, c), (_, _, p)| (q + p.q, c + p.cost));
        assert_eq!((oi.long_q, oi.long_cost), sum(LONG));
        assert_eq!((oi.short_q, oi.short_cost), sum(SHORT));
        // Every position closing at once, each at its best: the vault pays all of it.
        let all: u64 = open.iter().map(|(_, _, p)| plan::close_quote(p, m.price)).sum();
        assert!(all <= vault, "closing everything pays {all}, the vault holds {vault}");
        for (_, _, p) in open {
            assert!(plan::close_quote(p, m.price) <= p.margin + p.cost);
        }
    };

    for _ in 0..3_000 {
        match rng.below(10) {
            0 | 1 => {
                // The price moves up to 30 % either way.
                let p = (m.price as u128 * (70 + rng.below(61)) as u128 / 100).max(1) as u64;
                let (t, after) = plan::operate(Some(m), p, LP).unwrap();
                run(&t, &op(&t)).unwrap();
                m = after;
            }
            2..=4 => {
                let secret = [next_secret, 1, 2, 3, 4, 5, 6, 7];
                next_secret += 1;
                let key = key_of(&secret, test_hash);
                let side = if rng.below(2) == 0 { LONG } else { SHORT };
                let margin = (1 + rng.below(50)) * R + rng.below(R);
                let cost = margin * (1 + rng.below(10));
                match plan::open(m, oi, key, side, margin, cost) {
                    Ok((t, after, p)) => {
                        run(&t, &inputs(OPEN, Some(&secret))).unwrap();
                        oi = after;
                        vault += margin;
                        open.push((secret, key, p));
                        opened += 1;
                    }
                    Err(_) => {
                        // Only the reserve may refuse a sane open; and the program agrees.
                        let p = Position { margin, q: plan::size_for(side, cost, m.price), cost, side };
                        if p.q > 0 {
                            let (t, _) = plan::open_tx(m, oi, key, p);
                            assert_eq!(run(&t, &inputs(OPEN, Some(&secret))), Err(Refusal::Reserve));
                            refused_reserve += 1;
                        }
                    }
                }
            }
            5 | 6 if !open.is_empty() => {
                let i = rng.below(open.len() as u64) as usize;
                let (secret, key, p) = open.swap_remove(i);
                let (t, after, oi_after, x) = plan::close(m, oi, key, p);
                assert!(x <= vault);
                run(&t, &inputs(CLOSE, Some(&secret))).unwrap();
                (m, oi) = (after, oi_after);
                vault -= x;
                closed += 1;
            }
            7 => {
                // A keeper sweeps every liquidatable position.
                let mut i = 0;
                while i < open.len() {
                    let (_, key, p) = open[i];
                    if liquidatable(&p, m.price) {
                        let (t, after, oi_after, x) = plan::liquidate(m, oi, key, p).unwrap();
                        assert!(x <= vault);
                        run(&t, &inputs(LIQUIDATE, None)).unwrap();
                        (m, oi) = (after, oi_after);
                        vault -= x;
                        open.swap_remove(i);
                        liquidated += 1;
                    } else {
                        i += 1;
                    }
                }
            }
            8 => {
                let a = (10 + rng.below(500)) * R;
                if let Ok((t, after, _)) = plan::lp_add(m, oi, a) {
                    run(&t, &inputs(LP_ADD, None)).unwrap();
                    m = after;
                    vault += a;
                }
            }
            _ => {
                if m.supply > 0 {
                    let b = 1 + rng.below(m.supply);
                    if let Ok((t, after, x)) = plan::lp_remove(m, oi, b) {
                        assert!(x <= vault);
                        run(&t, &inputs(LP_REMOVE, None)).unwrap();
                        m = after;
                        vault -= x;
                    }
                }
            }
        }
        invariant(&m, &oi, vault, &open);
    }
    // Everyone closes; every payout is covered.
    while let Some((secret, key, p)) = open.pop() {
        let (t, after, oi_after, x) = plan::close(m, oi, key, p);
        assert!(x <= vault);
        run(&t, &inputs(CLOSE, Some(&secret))).unwrap();
        (m, oi) = (after, oi_after);
        vault -= x;
        invariant(&m, &oi, vault, &open);
    }
    assert_eq!(oi, Oi::default());
    assert_eq!(vault, m.cash);
    assert!(opened > 300 && closed > 100 && liquidated > 10 && refused_reserve > 0, "{opened} {closed} {liquidated} {refused_reserve}");
}
