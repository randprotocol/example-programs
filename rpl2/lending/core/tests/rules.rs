//! The lending market's rules, on the host: every method accepted at its best amounts and refused
//! one unit past them, no context word of any accepted transition left unchecked, lenders' shares
//! never worth less, and every accepted adjustment that adds risk healthy after it.
use lending_core::kit::host::{loose_words, test_hash, Out, Transition};
use lending_core::kit::host::Mock;
use lending_core::kit::RAND;
use lending_core::plan::{self, Change};
use lending_core::*;

const C: u32 = 5;
const SHARE: u32 = 6;
const R: u64 = 1_000_000_000;
const OP: [u32; 8] = [1, 2, 3, 4, 5, 6, 7, 8];
const ALICE: [u32; 8] = [9, 9, 9, 9, 9, 9, 9, 9];
const BOB: [u32; 8] = [7, 0, 7, 0, 7, 0, 7, 0];

fn public() -> Vec<u32> {
    plan::public(&plan::lock(test_hash, &OP), C)
}

fn run(t: &Transition, input: &[u32]) -> Result<[u32; 8], Refusal> {
    let m = Mock::new(&public(), t, input, test_hash);
    let r = check(&m);
    if m.oob.get() { Err(Refusal::Shape) } else { r }
}

fn op(price: u64, x: u32) -> Vec<u32> {
    plan::operator_input(&OP, price, x)
}

fn key(s: &[u32; 8]) -> [u32; 8] {
    plan::position_key(test_hash, s)
}

/// A market with 150 RAND supplied and price 2 RAND per C.
fn market() -> Pool {
    let (_, p) = plan::init(2 * R, SHARE);
    let (_, p, _) = plan::supply(p, SHARE, 100 * R).unwrap();
    plan::supply(p, SHARE, 50 * R).unwrap().1
}

/// The market after Alice deposits 50 C and borrows the most.
fn borrowed() -> (Pool, Position, Transition) {
    let pool = market();
    let x = plan::max_borrow(2 * R, &pool, Position::default(), 50 * R);
    let ch = Change { deposit: 50 * R, borrow: x, ..Change::default() };
    let (t, pool, pos) = plan::adjust(2 * R, pool, C, key(&ALICE), Position::default(), ch).unwrap();
    (pool, pos, t)
}

#[test]
fn operate_init_and_update() {
    let (t, pool) = plan::init(2 * R, SHARE);
    assert_eq!(run(&t, &op(2 * R, SHARE)).unwrap()[0], OPERATE);
    // Only the operator; never RAND or C as the share token; never a zero price.
    assert_eq!(run(&t, &plan::operator_input(&ALICE, 2 * R, SHARE)), Err(Refusal::Operator));
    for bad in [RAND, C] {
        let (t, _) = plan::init(2 * R, bad);
        assert_eq!(run(&t, &op(2 * R, bad)), Err(Refusal::Asset));
    }
    let (t0, _) = plan::init(0, SHARE);
    assert_eq!(run(&t0, &op(0, SHARE)), Err(Refusal::Price));
    // The written price must be the one the operator's input names.
    assert_eq!(run(&t, &op(2 * R + 1, SHARE)), Err(Refusal::Value));

    // Accrue the most: I' = 1.01 · I exactly, and one unit more is refused.
    let (t, after) = plan::update(pool, 3 * R, MAX_RATE).unwrap();
    assert_eq!(after.i, 1_010_000_000);
    assert!(run(&t, &op(3 * R, MAX_RATE as u32)).is_ok());
    let mut g = t.clone();
    g.writes[1].1 = Pool { i: after.i + 1, ..after }.value();
    assert_eq!(run(&g, &op(3 * R, MAX_RATE as u32)), Err(Refusal::Index));
    assert_eq!(run(&g, &op(3 * R, MAX_RATE as u32 + 1)), Err(Refusal::Index));
    assert!(plan::update(pool, 3 * R, MAX_RATE + 1).is_err());
    // Nor may it lower the index, or init a market that exists.
    let mut down = t.clone();
    down.writes[1].1 = Pool { i: pool.i - 1, ..pool }.value();
    assert_eq!(run(&down, &op(3 * R, 0)), Err(Refusal::Index));
    let mut again = plan::init(2 * R, SHARE).0;
    again.reads[0].1 = pool.value();
    assert_eq!(run(&again, &op(2 * R, SHARE)), Err(Refusal::Shape));
}

#[test]
fn the_index_never_rises_more_than_one_percent() {
    let mut pool = plan::init(R, SHARE).1;
    for k in 0..200u64 {
        let rate = (k * 7_919_113) % (MAX_RATE + 1);
        let (t, after) = plan::update(pool, R, rate).unwrap();
        assert!(run(&t, &op(R, rate as u32)).is_ok());
        assert!(after.i >= pool.i);
        assert!((after.i as u128) * 100 <= (pool.i as u128) * 101, "{} → {}", pool.i, after.i);
        pool = after;
    }
}

#[test]
fn supply_and_withdraw_at_their_best() {
    let (_, p0) = plan::init(2 * R, SHARE);
    let (t, _, m) = plan::supply(p0, SHARE, 100 * R).unwrap();
    assert_eq!(m, 100 * R - MIN_SHARES);
    assert!(run(&t, &[SUPPLY]).is_ok());
    let mut g = t.clone();
    g.mints[0].amount = m + 1;
    assert_eq!(run(&g, &[SUPPLY]), Err(Refusal::Price));
    assert!(plan::supply(p0, SHARE, MIN_SHARES).is_err());

    // After interest the share price is above 1: the best mint, and one more refused.
    let (pool, _, _) = borrowed();
    let pool = plan::update(pool, 2 * R, MAX_RATE).unwrap().1;
    let (t, after, m) = plan::supply(pool, SHARE, 7 * R + 3).unwrap();
    assert!(run(&t, &[SUPPLY]).is_ok());
    let mut g = t.clone();
    g.mints[0].amount = m + 1;
    g.writes[0].1 = Pool { s: after.s + 1, ..after }.value();
    assert_eq!(run(&g, &[SUPPLY]), Err(Refusal::Price));

    let (t, after, x) = plan::withdraw(pool, SHARE, 40 * R).unwrap();
    assert!(run(&t, &[WITHDRAW]).is_ok());
    let mut g = t.clone();
    g.pays[0].amount = x + 1;
    g.writes[0].1 = Pool { cash: after.cash - 1, ..after }.value();
    assert_eq!(run(&g, &[WITHDRAW]), Err(Refusal::Price));
    // A share token other than the bound one is neither minted nor burned.
    let mut h = t.clone();
    h.burn_asset = SHARE + 1;
    assert_eq!(run(&h, &[WITHDRAW]), Err(Refusal::Inflow));
}

#[test]
fn adjust_at_its_best_and_one_past() {
    let pool = market();
    let k = key(&ALICE);
    let x = plan::max_borrow(2 * R, &pool, Position::default(), 50 * R);
    assert!(x <= 75 * R && x > 74 * R, "max borrow {x}");
    let ch = Change { deposit: 50 * R, borrow: x, ..Change::default() };
    let (t, _, pos) = plan::adjust(2 * R, pool, C, k, Position::default(), ch).unwrap();
    assert!(run(&t, &plan::owner_input(&ALICE)).is_ok());
    let (g, _, _) = plan::adjust_tx(2 * R, pool, C, k, Position::default(), Change { borrow: x + 1, ..ch });
    assert_eq!(run(&g, &plan::owner_input(&ALICE)), Err(Refusal::Health));
    // Declaring one scaled unit less of debt than the borrow needs.
    let mut g = t.clone();
    let after = Pool::from_value(&t.writes[1].1).unwrap();
    g.writes[1].1 = Pool { sb: after.sb - 1, ..after }.value();
    g.writes[2].1 = Position { sdebt: pos.sdebt - 1, ..pos }.value();
    assert_eq!(run(&g, &plan::owner_input(&ALICE)), Err(Refusal::Debt));
    // Someone else's secret names another key.
    assert_eq!(run(&t, &plan::owner_input(&BOB)), Err(Refusal::Key));

    // Repay: the least scaled debt left, and one less refused.
    let (pool, pos, _) = borrowed();
    let pool = plan::update(pool, 2 * R, MAX_RATE).unwrap().1;
    let (t, after, pos2) = plan::adjust(2 * R, pool, C, k, pos, Change { repay: 10 * R, ..Change::default() }).unwrap();
    assert!(run(&t, &plan::owner_input(&ALICE)).is_ok());
    let mut g = t.clone();
    g.writes[1].1 = Pool { sb: after.sb - 1, ..after }.value();
    g.writes[2].1 = Position { sdebt: pos2.sdebt - 1, ..pos2 }.value();
    assert_eq!(run(&g, &plan::owner_input(&ALICE)), Err(Refusal::Debt));

    // Take collateral out: the most, and one more refused.
    let c = plan::max_withdraw(2 * R, &pool, pos2);
    let (t, _, _) = plan::adjust(2 * R, after, C, k, pos2, Change { withdraw: c, ..Change::default() }).unwrap();
    assert!(run(&t, &plan::owner_input(&ALICE)).is_ok());
    let (g, _, _) = plan::adjust_tx(2 * R, after, C, k, pos2, Change { withdraw: c + 1, ..Change::default() });
    assert_eq!(run(&g, &plan::owner_input(&ALICE)), Err(Refusal::Health));

    // Nothing changing is refused; so is a second payout.
    let same = plan::adjust_tx(2 * R, after, C, k, pos2, Change::default()).0;
    assert_eq!(run(&same, &plan::owner_input(&ALICE)), Err(Refusal::Unchanged));
    let mut two = t.clone();
    two.pays.push(Out::new(RAND, 1));
    assert_eq!(run(&two, &plan::owner_input(&ALICE)), Err(Refusal::Shape));
}

#[test]
fn liquidate_only_past_the_threshold() {
    let (pool, pos, _) = borrowed();
    let k = key(&ALICE);
    // Healthy at 2 RAND per C (and still at 1.75: LTV ≈ 85.6 % > 85 % only below ~1.765).
    let (t, _, _, _) = plan::liquidation(2 * R, pool, C, k, pos, 10 * R);
    assert_eq!(run(&t, &[LIQUIDATE]), Err(Refusal::Healthy));
    assert!(plan::liquidate(2 * R, pool, C, k, pos, 10 * R).is_err());

    let p = 1_500_000_000;
    assert!(liquidatable(pos.coll, pos.sdebt, p, pool.i));
    let (t, after, pos2, c) = plan::liquidate(p, pool, C, k, pos, 10 * R).unwrap();
    assert!(run(&t, &[LIQUIDATE]).is_ok());
    // At most a 10 % bonus on the debt cleared.
    let cleared_value = ((pos.sdebt - pos2.sdebt) as u128) * (pool.i as u128);
    assert!((c as u128) * (p as u128) * 100 <= cleared_value * 110);
    let mut g = t.clone();
    g.pays[0].amount = c + 1;
    g.writes[2].1 = Position { coll: pos2.coll - 1, ..pos2 }.value();
    assert_eq!(run(&g, &[LIQUIDATE]), Err(Refusal::Seize));
    // Clearing one scaled unit more than the repayment covers.
    let mut g = t.clone();
    g.writes[1].1 = Pool { sb: after.sb - 1, ..after }.value();
    g.writes[2].1 = Position { sdebt: pos2.sdebt - 1, ..pos2 }.value();
    assert_eq!(run(&g, &[LIQUIDATE]), Err(Refusal::Debt));
    // Not a position cell.
    let mut g = t.clone();
    g.reads[2].0[0] = 5;
    g.writes[2].0[0] = 5;
    assert_eq!(run(&g, &[LIQUIDATE]), Err(Refusal::Key));
}

#[test]
fn no_word_is_left_unchecked() {
    let k = key(&ALICE);
    let (t_init, p0) = plan::init(2 * R, SHARE);
    let (t_first, p1, _) = plan::supply(p0, SHARE, 100 * R).unwrap();
    let (t_supply, p2, _) = plan::supply(p1, SHARE, 50 * R).unwrap();
    let (pool, pos, t_open) = borrowed();
    let (t_accrue, pool) = plan::update(pool, 2 * R, MAX_RATE).unwrap();
    let (t_withdraw, _, _) = plan::withdraw(pool, SHARE, 30 * R).unwrap();
    let (t_repay, pool_r, pos_r) = plan::adjust(2 * R, pool, C, k, pos, Change { repay: 10 * R, ..Change::default() }).unwrap();
    let c = plan::max_withdraw(2 * R, &pool_r, pos_r);
    let (t_out, _, _) = plan::adjust(2 * R, pool_r, C, k, pos_r, Change { withdraw: c, ..Change::default() }).unwrap();
    let (t_liq, pool_l, pos_l, _) = plan::liquidate(1_500_000_000, pool, C, k, pos, 10 * R).unwrap();
    let close = Change { repay: plan::payoff(pos_l.sdebt, pool_l.i), withdraw: pos_l.coll, ..Change::default() };
    let (t_close, _, closed) = plan::adjust(1_500_000_000, pool_l, C, k, pos_l, close).unwrap();
    assert_eq!(closed, Position::default());
    let (t_dep, _, _) = plan::adjust(2 * R, p2, C, key(&BOB), Position::default(), Change { deposit: 3, ..Change::default() }).unwrap();
    let owner = plan::owner_input(&ALICE);
    let cases: Vec<(&str, Transition, Vec<u32>)> = vec![
        ("init", t_init, op(2 * R, SHARE)),
        ("accrue", t_accrue, op(2 * R, MAX_RATE as u32)),
        ("first supply", t_first, vec![SUPPLY]),
        ("supply", t_supply, vec![SUPPLY]),
        ("withdraw", t_withdraw, vec![WITHDRAW]),
        ("open: deposit + borrow", t_open, owner.clone()),
        ("repay", t_repay, owner.clone()),
        ("collateral out", t_out, owner.clone()),
        ("deposit only", t_dep, plan::owner_input(&BOB)),
        ("liquidate", t_liq, vec![LIQUIDATE]),
        ("close: repay all + collateral out", t_close, owner),
    ];
    for (label, t, input) in cases {
        assert!(t.fits(PUBLIC_WORDS as usize), "{label}: the context does not fit the segment");
        let loose = loose_words(&public(), &t, &input, test_hash, check);
        assert!(loose.is_empty(), "{label}: words accepted when changed: {loose:?}");
    }
}

/// A small deterministic generator.
struct Lcg(u64);
impl Lcg {
    fn next(&mut self, n: u64) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.0 >> 33) % n.max(1)
    }
}

/// `TA9 / s` as a cross-multiplied comparison: `after ≥ before`.
fn share_value_kept(before: &Pool, after: &Pool) -> bool {
    if before.s == 0 {
        return true;
    }
    let (a, b) = (ta9(after).mul(before.s), ta9(before).mul(after.s));
    b.le(a)
}

#[test]
fn a_share_is_never_worth_less() {
    // Random supplies, withdrawals, borrows, repayments, accruals and liquidations; after each
    // accepted one, the value of a share has not fallen.
    let mut g = Lcg(42);
    let mut pool = market();
    let mut price = 2 * R;
    let mut pos = [Position::default(); 2];
    let owners = [ALICE, BOB];
    let mut circulating = pool.s - MIN_SHARES;
    let mut seen = [0u32; 6];
    for _ in 0..600 {
        let before = pool;
        let who = g.next(2) as usize;
        let k = key(&owners[who]);
        let kind = g.next(6) as usize;
        let (t, input, after, p2) = match kind {
            0 => {
                let (t, a, _) = match plan::supply(pool, SHARE, 1 + g.next(20 * R)) { Ok(x) => x, Err(_) => continue };
                (t, vec![SUPPLY], a, pos[who])
            }
            1 => {
                let b = 1 + g.next(circulating / 3);
                let (t, a, _) = match plan::withdraw(pool, SHARE, b) { Ok(x) => x, Err(_) => continue };
                (t, vec![WITHDRAW], a, pos[who])
            }
            2 => {
                let dep = g.next(10 * R);
                let x = plan::max_borrow(price, &pool, pos[who], dep);
                let borrow = if g.next(2) == 0 { x } else { g.next(x + 1) };
                let ch = Change { deposit: dep, borrow, ..Change::default() };
                let (t, a, p) = match plan::adjust(price, pool, C, k, pos[who], ch) { Ok(x) => x, Err(_) => continue };
                (t, plan::owner_input(&owners[who]), a, p)
            }
            3 => {
                let ch = Change { repay: 1 + g.next(30 * R), ..Change::default() };
                let (t, a, p) = match plan::adjust(price, pool, C, k, pos[who], ch) { Ok(x) => x, Err(_) => continue };
                (t, plan::owner_input(&owners[who]), a, p)
            }
            4 => {
                price = 500_000_000 + g.next(5 * R / 2);
                let rate = g.next(MAX_RATE + 1);
                let (t, a) = plan::update(pool, price, rate).unwrap();
                (t, op(price, rate as u32), a, pos[who])
            }
            _ => {
                let (t, a, p, _) = match plan::liquidate(price, pool, C, k, pos[who], 1 + g.next(20 * R)) { Ok(x) => x, Err(_) => continue };
                (t, vec![LIQUIDATE], a, p)
            }
        };
        assert!(run(&t, &input).is_ok(), "the planner built a transition the rules refuse: {t:?}");
        assert!(share_value_kept(&before, &after), "a share lost value: {before:?} → {after:?}");
        seen[kind] += 1;
        if input[0] == SUPPLY {
            circulating += after.s - before.s;
        }
        if input[0] == WITHDRAW {
            circulating -= before.s - after.s;
        }
        pos[who] = p2;
        pool = after;
    }
    assert!(seen.iter().all(|n| *n >= 5), "some kinds were hardly exercised: {seen:?}");
}

#[test]
fn every_accepted_adjustment_that_adds_risk_is_healthy() {
    // Arbitrary declared outcomes, not the planner's: whatever the rules accept that adds debt or
    // removes collateral ends within 75 % LTV, and its debt covers what was borrowed.
    let mut g = Lcg(7);
    let (pool0, pos0, _) = borrowed();
    let k = key(&ALICE);
    let mut accepted = 0;
    for _ in 0..3000 {
        let price = 1_000_000_000 + g.next(2 * R);
        let pool = Pool { i: pool0.i + g.next(R / 10), ..pool0 };
        let ch = match g.next(3) {
            0 => Change { borrow: 1 + g.next(5 * R), ..Change::default() },
            1 => Change { withdraw: 1 + g.next(pos0.coll), ..Change::default() },
            _ => Change { deposit: g.next(5 * R), repay: g.next(5 * R), borrow: g.next(5 * R), ..Change::default() },
        };
        let (mut t, after, mut pos2) = plan::adjust_tx(price, pool, C, k, pos0, ch);
        // Shave a little off the declared debt now and then (and keep the pool consistent).
        let shave = g.next(3).min(pos2.sdebt);
        pos2.sdebt -= shave;
        t.writes[1].1 = Pool { sb: after.sb - shave, ..after }.value();
        t.writes[2].1 = pos2.value();
        let input = plan::owner_input(&ALICE);
        if run(&t, &input).is_ok() {
            accepted += 1;
            if pos2.sdebt > pos0.sdebt || pos2.coll < pos0.coll {
                assert!(healthy(pos2.coll, pos2.sdebt, price, pool.i), "accepted but unhealthy: {ch:?} → {pos2:?}");
            }
            let paid = t.pays.iter().filter(|o| o.asset == RAND).map(|o| o.amount).sum::<u64>();
            assert!(debt_covered(pos0.sdebt, pos2.sdebt, pool.i, t.burn_r, paid));
        }
    }
    assert!(accepted > 100, "only {accepted} accepted");
}

#[test]
fn shape_and_inflow_are_pinned() {
    let (pool, pos, t) = borrowed();
    let owner = plan::owner_input(&ALICE);
    // A mint, a third read, RAND as the "collateral" deposit, a token burned instead of deposited.
    let mut a = t.clone();
    a.mints.push(Out::new(SHARE, 1));
    assert_eq!(run(&a, &owner), Err(Refusal::Shape));
    let mut b = t.clone();
    b.inflow = lending_core::kit::host::Inflow::Burn;
    assert_eq!(run(&b, &owner), Err(Refusal::Inflow));
    let mut c = t.clone();
    c.burn_asset = SHARE;
    assert_eq!(run(&c, &owner), Err(Refusal::Inflow));
    // Paying the share token, or C, under the RAND borrow's amount.
    let mut d = t.clone();
    d.pays[0].asset = SHARE;
    assert_eq!(run(&d, &owner), Err(Refusal::Asset));
    // Borrowing against a price the program never wrote back.
    let mut e = t.clone();
    e.writes[0].1 = price_value(3 * R);
    assert_eq!(run(&e, &owner), Err(Refusal::Value));
    // Every accepted transition fits the segment with nine public words.
    let (tl, _, _, _) = plan::liquidate(1_500_000_000, pool, C, key(&ALICE), pos, R).unwrap();
    assert!(tl.fits(PUBLIC_WORDS as usize) && t.fits(PUBLIC_WORDS as usize));
    assert_eq!(public().len(), PUBLIC_WORDS as usize);
}
