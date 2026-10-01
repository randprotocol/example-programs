//! The stablecoin's rules, on the host: every method accepted at its best amounts and refused one
//! unit past them, no context word of any accepted transition left unchecked, and the safety
//! properties — only the operator sets the price, only the owner moves a position, nothing
//! riskier below 150 %, no liquidation at or above 110 %, and the books always balance.
use stablecoin_core::kit::host::{loose_words, test_hash, Mock, Out, Transition};
use stablecoin_core::kit::RAND;
use stablecoin_core::plan::{self, Moves};
use stablecoin_core::{
    check, healthy, Config, Position, Refusal, ADJUST, CONFIG_KEY, LIQUIDATE, LIQUIDATION_PCT, MCR_PCT,
};

const STABLE: u32 = 6;
const OPERATOR: [u32; 8] = [11, 12, 13, 14, 15, 16, 17, 18];
const OWNER: [u32; 8] = [21, 22, 23, 24, 25, 26, 27, 28];
const STRANGER: [u32; 8] = [31, 32, 33, 34, 35, 36, 37, 38];
const PRICE: u64 = 2_000_000_000;
const R: u64 = 1_000_000_000;

fn lock() -> [u32; 8] {
    plan::lock_of(test_hash, &OPERATOR)
}

fn key() -> [u32; 8] {
    plan::key_of(test_hash, &OWNER)
}

fn run(t: &Transition, input: &[u32]) -> Result<[u32; 8], Refusal> {
    let m = Mock::new(&lock(), t, input, test_hash);
    let r = check(&m);
    if m.oob.get() { Err(Refusal::Shape) } else { r }
}

fn cfg() -> Config {
    Config { price: PRICE, stable: STABLE }
}

fn adjust(p: Position, m: Moves) -> (Transition, Vec<u32>, Position) {
    plan::adjust(&OWNER, key(), cfg(), p, m).unwrap()
}

fn with_secret(input: &[u32], s: &[u32; 8]) -> Vec<u32> {
    let mut v = input.to_vec();
    v[1..9].copy_from_slice(s);
    v
}

/// 10 RAND locked, the most borrowed.
fn opened() -> Position {
    let most = plan::max_mint(&cfg(), 10 * R, 0);
    adjust(Position::default(), Moves { deposit: 10 * R, mint: most, ..Moves::default() }).2
}

#[test]
fn operator_binds_once_and_sets_the_price() {
    let (t, input, c) = plan::operate(&OPERATOR, None, STABLE, PRICE).unwrap();
    assert_eq!(c, cfg());
    assert_eq!(run(&t, &input).unwrap()[..6], [1, PRICE as u32, 0, 0, 0, STABLE]);
    // Anyone else: refused.
    assert_eq!(run(&t, &with_secret(&input, &STRANGER)), Err(Refusal::Operator));
    // RAND as the stable token: refused.
    let (mut t2, mut input2, _) = plan::operate(&OPERATOR, None, 7, PRICE).unwrap();
    input2[9] = RAND;
    t2.writes[0].1 = Config { price: PRICE, stable: RAND }.value();
    assert_eq!(run(&t2, &input2), Err(Refusal::Config));

    // An update keeps the token; a zero price or a rebinding is refused.
    let (t, input, _) = plan::operate(&OPERATOR, Some(cfg()), 0, 1_300_000_000).unwrap();
    assert!(run(&t, &input).is_ok());
    let mut rebind = t.clone();
    let mut rebind_in = input.clone();
    rebind.writes[0].1 = Config { price: 1_300_000_000, stable: 7 }.value();
    rebind_in[9] = 7;
    assert_eq!(run(&rebind, &rebind_in), Err(Refusal::Instruction));
    let mut zero = t.clone();
    let mut zero_in = input.clone();
    zero.writes[0].1 = Config { price: 0, stable: STABLE }.value();
    zero_in[12] = 0;
    zero_in[13] = 0;
    assert_eq!(run(&zero, &zero_in), Err(Refusal::Config));
    // A first price over a live config (the instruction says absent, the read says otherwise).
    let (mut again, again_in, _) = plan::operate(&OPERATOR, None, STABLE, PRICE).unwrap();
    again.reads[0].1 = cfg().value();
    assert_eq!(run(&again, &again_in), Err(Refusal::Instruction));
}

#[test]
fn every_method_accepts_its_best_and_refuses_one_more() {
    // Open at the most 150 % allows; one unit more is refused.
    let most = plan::max_mint(&cfg(), 10 * R, 0);
    assert_eq!(most, 13_333_333_333);
    let (t, input, p) = adjust(Position::default(), Moves { deposit: 10 * R, mint: most, ..Moves::default() });
    assert_eq!(run(&t, &input).unwrap()[..5], [ADJUST, (10 * R) as u32, 2, most as u32, (most >> 32) as u32]);
    let mut g = t.clone();
    g.mints[0].amount += 1;
    g.writes[1].1 = Position { debt: most + 1, ..p }.value();
    assert_eq!(run(&g, &input), Err(Refusal::Unhealthy));

    // Withdraw the most; one unit more is refused.
    let p = adjust(p, Moves { repay: 3_333_333_333, ..Moves::default() }).2;
    let w = plan::max_withdraw(&cfg(), p.coll, p.debt);
    assert_eq!(w, 2_500_000_000);
    let (t, input, _) = adjust(p, Moves { withdraw: w, ..Moves::default() });
    assert!(run(&t, &input).is_ok());
    let mut g = t.clone();
    g.pays[0].amount += 1;
    g.writes[1].1 = Position { coll: p.coll - w - 1, ..p }.value();
    assert_eq!(run(&g, &input), Err(Refusal::Unhealthy));

    // Liquidate at the highest price below 110 %; one unit of price more is refused.
    let low = plan::liquidation_price(&p);
    assert!(!healthy(p.coll, p.debt, low, LIQUIDATION_PCT) && healthy(p.coll, p.debt, low + 1, LIQUIDATION_PCT));
    let c = Config { price: low, stable: STABLE };
    let (t, input) = plan::liquidate(key(), c, p).unwrap();
    assert_eq!(run(&t, &input).unwrap()[0], LIQUIDATE);
    let mut g = t.clone();
    let up = Config { price: low + 1, ..c }.value();
    g.reads[0].1 = up;
    g.writes[0].1 = up;
    assert_eq!(run(&g, &input), Err(Refusal::Healthy));
    // Taking one unit more, or burning one unit less.
    let mut g = t.clone();
    g.pays[0].amount += 1;
    assert_eq!(run(&g, &input), Err(Refusal::Asset));
    let mut g = t.clone();
    g.burn_a -= 1;
    assert_eq!(run(&g, &input), Err(Refusal::Inflow));
}

#[test]
fn no_word_is_left_unchecked() {
    let p = opened();
    let low = plan::liquidation_price(&p);
    let mut cases = vec![
        plan::operate(&OPERATOR, None, STABLE, PRICE).map(|x| (x.0, x.1)).unwrap(),
        plan::operate(&OPERATOR, Some(cfg()), 0, 1_300_000_000).map(|x| (x.0, x.1)).unwrap(),
        plan::liquidate(key(), Config { price: low, stable: STABLE }, p).unwrap(),
    ];
    let moves = [
        (Position::default(), Moves { deposit: 10 * R, mint: 5 * R, ..Moves::default() }),
        (Position::default(), Moves { deposit: 10 * R, ..Moves::default() }),
        (p, Moves { deposit: R, ..Moves::default() }),
        (p, Moves { mint: R / 1000, deposit: R, ..Moves::default() }),
        (p, Moves { repay: R, ..Moves::default() }),
        (p, Moves { repay: 3 * R, withdraw: R, ..Moves::default() }),
        (p, Moves { repay: p.debt, withdraw: p.coll, ..Moves::default() }),
    ];
    for (q, m) in moves {
        let (t, input, _) = adjust(q, m);
        cases.push((t, input));
    }
    for (t, input) in cases {
        let loose = loose_words(&lock(), &t, &input, test_hash, check);
        assert!(loose.is_empty(), "method {}: words accepted when changed: {loose:?}", input[0]);
    }
}

#[test]
fn only_the_owner_moves_a_position() {
    let p = opened();
    let (t, input, _) = adjust(p, Moves { repay: R, ..Moves::default() });
    assert_eq!(run(&t, &with_secret(&input, &STRANGER)), Err(Refusal::Owner));
    // Nor can a stranger declare a position under their own key with someone else's value.
    let theirs = plan::key_of(test_hash, &STRANGER);
    let (t2, input2, _) = plan::adjust(&STRANGER, theirs, cfg(), Position::default(), Moves { deposit: R, ..Moves::default() }).unwrap();
    assert!(run(&t2, &input2).is_ok());
    // A stranger naming their own key but declaring the owner's position as its value is accepted
    // by the program — and refused by the chain, whose read check compares the declared value with
    // what the stranger's cell really holds (nothing). A cell's value is the chain's to say.
    let mut forged = t.clone();
    forged.reads[1].0 = theirs;
    forged.writes[1].0 = theirs;
    assert!(run(&forged, &with_secret(&input, &STRANGER)).is_ok());
}

#[test]
fn repaying_and_adding_collateral_are_always_allowed() {
    let p = opened();
    // The price halves: the position is under water.
    let c = Config { price: PRICE / 2, stable: STABLE };
    assert!(!healthy(p.coll, p.debt, c.price, MCR_PCT));
    let try_ = |m: Moves| {
        let q = plan::after(&c, p, m);
        let t = Transition::new()
            .read(CONFIG_KEY, c.value())
            .read(key(), p.value())
            .write(CONFIG_KEY, c.value())
            .write(key(), Position { coll: p.coll + m.deposit - m.withdraw, debt: p.debt + m.mint - m.repay }.value())
            .rand_in(m.deposit);
        let t = if m.repay > 0 { t.burn(STABLE, m.repay) } else { t };
        let t = if m.withdraw > 0 { t.pay(RAND, m.withdraw) } else { t };
        let t = if m.mint > 0 { t.mint(STABLE, m.mint) } else { t };
        let mut input = vec![ADJUST];
        input.extend_from_slice(&OWNER);
        (q.is_ok(), run(&t, &input))
    };
    assert!(try_(Moves { repay: R, ..Moves::default() }).1.is_ok());
    assert!(try_(Moves { deposit: R, ..Moves::default() }).1.is_ok());
    assert_eq!(try_(Moves { withdraw: 1, ..Moves::default() }), (false, Err(Refusal::Unhealthy)));
    assert_eq!(try_(Moves { mint: 1, deposit: R, ..Moves::default() }), (false, Err(Refusal::Unhealthy)));
    assert!(try_(Moves { repay: p.debt, withdraw: 1, ..Moves::default() }).1.is_ok());
}

#[test]
fn shape_and_inflow_are_pinned() {
    let p = opened();
    let (t, input, _) = adjust(p, Moves { repay: 3 * R, withdraw: R, ..Moves::default() });
    assert!(run(&t, &input).is_ok());
    let mut a = t.clone();
    a.pays.push(Out::new(RAND, 1));
    assert_eq!(run(&a, &input), Err(Refusal::Shape));
    let mut b = t.clone();
    b.mints.push(Out::new(7, 1));
    assert_eq!(run(&b, &input), Err(Refusal::Asset));
    let mut c = t.clone();
    c.pays[0].asset = STABLE;
    assert_eq!(run(&c, &input), Err(Refusal::Asset));
    // Repaying with a deposit (not a burn), or burning another token.
    let mut d = t.clone();
    d.inflow = stablecoin_core::kit::host::Inflow::Deposit;
    assert_eq!(run(&d, &input), Err(Refusal::Inflow));
    let e = t.clone().burn(7, R);
    assert_eq!(run(&e, &input), Err(Refusal::Inflow));
    // Repaying more than is owed.
    let mut f = t.clone();
    f.burn_a = p.debt + 1;
    assert_eq!(run(&f, &input), Err(Refusal::Underflow));
    // Changing nothing.
    let nothing = Transition::new()
        .read(CONFIG_KEY, cfg().value())
        .read(key(), p.value())
        .write(CONFIG_KEY, cfg().value())
        .write(key(), p.value());
    assert_eq!(run(&nothing, &input), Err(Refusal::Unchanged));
    // Without a config.
    let mut g = t.clone();
    g.reads[0].1 = [0; 8];
    g.writes[0].1 = [0; 8];
    assert_eq!(run(&g, &input), Err(Refusal::Config));
    // A config written back changed.
    let mut h = t.clone();
    h.writes[0].1 = Config { price: PRICE + 1, ..cfg() }.value();
    assert_eq!(run(&h, &input), Err(Refusal::Value));
    // Liquidating an empty cell, or the config.
    let (lt, li) = plan::liquidate(key(), Config { price: 1, stable: STABLE }, p).unwrap();
    let mut i = lt.clone();
    i.reads[1].1 = [0; 8];
    assert_eq!(run(&i, &li), Err(Refusal::Zero));
    let mut j = lt.clone();
    j.reads[1].0 = [2, 0, 0, 0, 0, 0, 0, 0];
    assert_eq!(run(&j, &li), Err(Refusal::Key));
    let mut k = lt.clone();
    k.reads[1].0[0] = 1;
    k.writes[1].0[0] = 1;
    assert_eq!(run(&k, &li), Err(Refusal::Key));
}

#[test]
fn every_riskier_position_is_at_150_and_the_books_balance() {
    // A walk through prices and moves: every accepted transition keeps
    //   vault RAND = Σ collateral   and   stable supply = Σ debt,
    // and every accepted transition that borrows or withdraws leaves the position at ≥ 150 %.
    let mut p = Position::default();
    let (mut vault, mut supply, mut accepted) = (0u64, 0u64, 0);
    for (i, price) in [2_000_000_000u64, 1_999_999_999, 3, 7_777_777_777, 1_234_567_891].into_iter().enumerate() {
        let c = Config { price, stable: STABLE };
        for kind in 0..4 {
            let m = match kind {
                0 => Moves { deposit: (i as u64 + 1) * 3 * R + 17, ..Moves::default() },
                1 => Moves { mint: plan::max_mint(&c, p.coll, p.debt), ..Moves::default() },
                2 => Moves { repay: p.debt / 3 + 1, ..Moves::default() },
                _ => Moves { withdraw: plan::max_withdraw(&c, p.coll, p.debt), ..Moves::default() },
            };
            let Ok((t, input, q)) = plan::adjust(&OWNER, key(), c, p, m) else { continue };
            assert!(run(&t, &input).is_ok(), "price {price}: {m:?}");
            if q.debt > p.debt || q.coll < p.coll {
                assert!(healthy(q.coll, q.debt, price, MCR_PCT));
                // ≥ 150 %, checked in plain u128 arithmetic as well.
                assert!(q.coll as u128 * price as u128 * 100 >= q.debt as u128 * R as u128 * 150);
            }
            vault = vault + t.burn_r - t.pays.iter().map(|o| o.amount).sum::<u64>();
            supply = supply + t.mints.iter().map(|o| o.amount).sum::<u64>() - t.burn_a;
            p = q;
            assert_eq!((vault, supply), (p.coll, p.debt));
            accepted += 1;
        }
    }
    assert!(accepted >= 15, "only {accepted} moves were possible");
    // Liquidation: the whole debt burned, the whole collateral out, the cell gone.
    let c = Config { price: plan::liquidation_price(&p), stable: STABLE };
    let (t, input) = plan::liquidate(key(), c, p).unwrap();
    assert!(run(&t, &input).is_ok());
    assert_eq!((t.burn_a, t.pays[0].amount, t.writes[1].1), (supply, vault, [0; 8]));
}
