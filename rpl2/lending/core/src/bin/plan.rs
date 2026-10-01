//! plan: build a lending transition from the cells it touches, check it with the program's own
//! rules, and hand it to the scripts (`--t t.json --i inputs.json`; the emulator's words on stdout).
//! `--public` is the deploy's public input file (the lock's eight words, then the collateral token).
//!
//!   plan key       --secret F                                            the position key, as hex
//!   plan operate   --public F --operator F --pool <hex> --new-price <units> (--share S | --rate R)
//!   plan supply    --public F --pool <hex> --shares <hex> --amount <units>
//!   plan withdraw  --public F --pool <hex> --shares <hex> --burn <shares>
//!   plan adjust    --public F --secret F --price <hex> --pool <hex> --position <hex>
//!                  [--deposit <units>] [--withdraw <units>|max] [--borrow <units>|max] [--repay <units>|all]
//!   plan liquidate --public F --price <hex> --pool <hex> --key <hex> --position <hex> --repay <units>
//!   plan demo      [--coll C --share S]     every method on the emulator, accepted and refused
use lending_core::kit::host::{demo_step, emit, fail, hex8, parse_hex8, real_hash, Args, Mock, Transition};
use lending_core::kit::u64_of;
use lending_core::plan::{self, Change};
use lending_core::*;

fn main() {
    let a = Args::parse();
    match a.cmd.as_str() {
        "key" => println!("{}", hex8(&plan::position_key(real_hash, &a.words8("secret")))),
        "operate" | "supply" | "withdraw" | "adjust" | "liquidate" => one(&a),
        "demo" => demo(a.u64_or("coll", 5) as u32, a.u64_or("share", 6) as u32),
        _ => fail("usage: plan key|operate|supply|withdraw|adjust|liquidate|demo … (see the top of core/src/bin/plan.rs)"),
    }
}

/// The nine words of a `public.txt`.
fn public_of(a: &Args) -> Vec<u32> {
    let path = a.str("public");
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| fail(&format!("{path}: {e}")));
    let v: Vec<u32> = text.split_whitespace().map(|t| t.parse().unwrap_or_else(|_| fail(&format!("{path}: not words")))).collect();
    if v.len() != PUBLIC_WORDS as usize {
        fail(&format!("{path}: expected {PUBLIC_WORDS} words (the lock, then the collateral token)"));
    }
    v
}

fn pool_of(a: &Args) -> Pool {
    Pool::from_value(&a.cell("pool")).unwrap_or_else(|| fail("the market is not set up: the operator runs ./operate.sh init first"))
}

fn price_of(a: &Args) -> u64 {
    let v = a.cell("price");
    u64_of(v[0], v[1])
}

fn share_of(a: &Args) -> u32 {
    let v = a.cell("shares");
    if v[7] != VERSION {
        fail("no shares cell: the market is not set up");
    }
    v[0]
}

fn one(a: &Args) {
    let public = public_of(a);
    let c = public[PUBLIC_COLL as usize];
    let (t, input): (Transition, Vec<u32>) = match a.cmd.as_str() {
        "operate" => {
            let secret = a.words8("operator");
            let price = a.u64("new-price");
            match Pool::from_value(&a.cell("pool")) {
                None => {
                    let share = a.u32("share");
                    let (t, _) = plan::init(price, share);
                    eprintln!("operate: init — price {price}, share token {share}, index {E}");
                    (t, plan::operator_input(&secret, price, share))
                }
                Some(pool) => {
                    let rate = a.u64_or("rate", 0);
                    let (t, after) = plan::update(pool, price, rate).unwrap_or_else(|e| fail(&e));
                    eprintln!("operate: price {price}, index {} → {}", pool.i, after.i);
                    (t, plan::operator_input(&secret, price, rate as u32))
                }
            }
        }
        "supply" => {
            let (t, after, m) = plan::supply(pool_of(a), share_of(a), a.u64("amount")).unwrap_or_else(|e| fail(&e));
            eprintln!("supply: mints {m} shares; pool after: {after:?}");
            (t, vec![SUPPLY])
        }
        "withdraw" => {
            let (t, after, x) = plan::withdraw(pool_of(a), share_of(a), a.u64("burn")).unwrap_or_else(|e| fail(&e));
            eprintln!("withdraw: pays {x} RAND units; pool after: {after:?}");
            (t, vec![WITHDRAW])
        }
        "adjust" => {
            let secret = a.words8("secret");
            let (p, pool) = (price_of(a), pool_of(a));
            let pos = Position::from_value(&a.cell("position"));
            let key = plan::position_key(real_hash, &secret);
            let deposit = a.u64_or("deposit", 0);
            let withdraw = match a.opt("withdraw") {
                Some("max") => plan::max_withdraw(p, &pool, pos),
                _ => a.u64_or("withdraw", 0),
            };
            let borrow = match a.opt("borrow") {
                Some("max") => plan::max_borrow(p, &pool, pos, deposit),
                _ => a.u64_or("borrow", 0),
            };
            let repay = match a.opt("repay") {
                Some("all") => plan::payoff(pos.sdebt, pool.i),
                _ => a.u64_or("repay", 0),
            };
            let ch = Change { deposit, withdraw, borrow, repay };
            let (t, _, pos2) = plan::adjust(p, pool, c, key, pos, ch).unwrap_or_else(|e| fail(&e));
            let (debt, ltv) = plan::debt_and_ltv(p, pool.i, pos2);
            eprintln!("adjust: {ch:?}; position after: {pos2:?} (owes {debt} RAND units, LTV {}.{:02} %)", ltv / 100, ltv % 100);
            (t, plan::owner_input(&secret))
        }
        _ => {
            let (p, pool) = (price_of(a), pool_of(a));
            let key = parse_hex8(a.str("key")).unwrap_or_else(|e| fail(&e));
            let pos = Position::from_value(&a.cell("position"));
            let (t, _, pos2, seized) = plan::liquidate(p, pool, c, key, pos, a.u64("repay")).unwrap_or_else(|e| fail(&e));
            eprintln!("liquidate: seizes {seized} units of C; position after: {pos2:?}");
            (t, vec![LIQUIDATE])
        }
    };
    if Mock::new(&public, &t, &input, real_hash).accepts(check).is_none() {
        fail("the program would refuse this transition");
    }
    emit(a, &public, &t, &input);
}

const OPERATOR: [u32; 8] = [11, 12, 13, 14, 15, 16, 17, 18];
const BORROWER: [u32; 8] = [21, 22, 23, 24, 25, 26, 27, 28];
const MALLORY: [u32; 8] = [31, 32, 33, 34, 35, 36, 37, 38];
const RAND_: u64 = 1_000_000_000;

/// Every method, accepted, and a greedy, tampered or unauthorised variant of each, refused. The
/// host rules run with the real Poseidon2, so the emulator and the host see the same keys.
fn demo(c: u32, share: u32) {
    let public = plan::public(&plan::lock(real_hash, &OPERATOR), c);
    let step = |label: &str, t: &Transition, input: &[u32], accept: bool| {
        let host = Mock::new(&public, t, input, real_hash).accepts(check).is_some();
        assert_eq!(host, accept, "the host rules disagree with the demo's expectation: {label}");
        demo_step(label, &public, t, input, accept);
    };
    let op = |price: u64, x: u32| plan::operator_input(&OPERATOR, price, x);
    let owner = plan::owner_input(&BORROWER);
    let key = plan::position_key(real_hash, &BORROWER);

    // The operator sets up the market: 1 C = 2 RAND.
    let mut price = 2 * RAND_;
    let (t, pool) = plan::init(price, share);
    step("operator: init — 1 C = 2 RAND, index 1.0, share token bound", &t, &op(price, share), true);
    step("init with someone else's secret", &t, &plan::operator_input(&MALLORY, price, share), false);

    // Two lenders.
    let (t, pool, m_a) = plan::supply(pool, share, 100 * RAND_).unwrap();
    step(&format!("lender A supplies 100 RAND for {m_a} shares (1000 locked)"), &t, &[SUPPLY], true);
    let mut greedy = t.clone();
    greedy.mints[0].amount += 1;
    greedy.writes[0].1 = Pool { s: pool.s + 1, ..pool }.value();
    step("the same supply minting one share more", &greedy, &[SUPPLY], false);
    let (t, pool, m_b) = plan::supply(pool, share, 50 * RAND_).unwrap();
    step(&format!("lender B supplies 50 RAND for {m_b} shares"), &t, &[SUPPLY], true);

    // A borrower posts 50 C and borrows the most it may.
    let pos = Position::default();
    let x = plan::max_borrow(price, &pool, pos, 50 * RAND_);
    let ch = Change { deposit: 50 * RAND_, borrow: x, ..Change::default() };
    let (t, pool2, pos2) = plan::adjust(price, pool, c, key, pos, ch).unwrap();
    step(&format!("borrower deposits 50 C and borrows the most, {x} RAND units"), &t, &owner, true);
    let (greedy, _, _) = plan::adjust_tx(price, pool, c, key, pos, Change { borrow: x + 1, ..ch });
    step("the same, borrowing one unit more (past 75 % LTV)", &greedy, &owner, false);
    let (pool, pos) = (pool2, pos2);

    // A liquidator tries the healthy position.
    let (t, _, _, _) = plan::liquidation(price, pool, c, key, pos, 10 * RAND_);
    step("liquidating the healthy position", &t, &[LIQUIDATE], false);

    // Interest: the operator raises the index by 1 %, and not more.
    let (t, pool2) = plan::update(pool, price, MAX_RATE).unwrap();
    step(&format!("operator accrues 1 %: index {} → {}", pool.i, pool2.i), &t, &op(price, MAX_RATE as u32), true);
    let mut greedy = t.clone();
    greedy.writes[1].1 = Pool { i: pool2.i + 1, ..pool2 }.value();
    step("the operator raising the index one unit past 1 %", &greedy, &op(price, MAX_RATE as u32), false);
    step("the same accrual claiming a 1.0000001 % rate", &greedy, &op(price, MAX_RATE as u32 + 1), false);
    let pool = pool2;

    // The borrower repays 20 RAND.
    let ch = Change { repay: 20 * RAND_, ..Change::default() };
    let (t, pool2, pos2) = plan::adjust(price, pool, c, key, pos, ch).unwrap();
    let (debt, _) = plan::debt_and_ltv(price, pool.i, pos2);
    step(&format!("borrower repays 20 RAND (owes {debt} units after)"), &t, &owner, true);
    step("the same repayment proved with another secret", &t, &plan::owner_input(&MALLORY), false);
    let (pool, pos) = (pool2, pos2);

    // Lender A withdraws half their shares.
    let (t, pool2, out) = plan::withdraw(pool, share, m_a / 2).unwrap();
    let mut greedy = t.clone();
    greedy.pays[0].amount += 1;
    greedy.writes[0].1 = Pool { cash: pool2.cash - 1, ..pool2 }.value();
    step("lender A withdrawing one unit more than half their shares are worth", &greedy, &[WITHDRAW], false);
    step(&format!("lender A burns half their shares for {out} RAND units"), &t, &[WITHDRAW], true);
    let pool = pool2;

    // C falls to 1.2 RAND: the position passes 85 %.
    price = 1_200_000_000;
    let (t, pool2) = plan::update(pool, price, 0).unwrap();
    let (_, ltv) = plan::debt_and_ltv(price, pool.i, pos);
    step(&format!("operator: C falls to 1.2 RAND (the position's LTV {}.{:02} %)", ltv / 100, ltv % 100), &t, &op(price, 0), true);
    let pool = pool2;

    // A liquidator repays 20 RAND of it and takes C at a 10 % bonus.
    let (t, pool2, pos2, seized) = plan::liquidate(price, pool, c, key, pos, 20 * RAND_).unwrap();
    step(&format!("liquidator repays 20 RAND and seizes {seized} units of C"), &t, &[LIQUIDATE], true);
    let mut greedy = t.clone();
    greedy.pays[0].amount += 1;
    greedy.writes[2].1 = Position { coll: pos2.coll - 1, ..pos2 }.value();
    step("the same liquidation seizing one unit more", &greedy, &[LIQUIDATE], false);
    let (pool, pos) = (pool2, pos2);

    // The borrower pays off the rest and takes the collateral home: the position is deleted.
    let r = plan::payoff(pos.sdebt, pool.i);
    let ch = Change { repay: r, withdraw: pos.coll, ..Change::default() };
    let (t, _, _) = plan::adjust(price, pool, c, key, pos, ch).unwrap();
    step(&format!("borrower repays the last {r} units and takes {} C out (position deleted)", pos.coll), &t, &owner, true);
    let (greedy, _, _) = plan::adjust_tx(price, pool, c, key, pos, Change { repay: r - 1, ..ch });
    step("the same, repaying one unit less", &greedy, &owner, false);
    step("an unknown method", &t, &[9], false);
}
