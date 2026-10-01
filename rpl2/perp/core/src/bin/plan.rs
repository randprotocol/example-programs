//! plan: build a perp transition from the cells it touches, check it with the program's own rules
//! (and the real Poseidon2), and hand it to the scripts (`--t t.json --i inputs.json`; the
//! emulator's words on stdout). `--lock` is the deploy's public input (public.txt); amounts are
//! base units (1 RAND = 10^9); a price is RAND units per 10^9 units of X.
//!
//!   plan key       --secret F                                          a position's key (64 hex)
//!   plan operate   --lock L --operator F --market <hex> --price P [--lp A]   (A: creating the market)
//!   plan lp-add    --lock L --market <hex> --oi <hex> --rand <units>
//!   plan lp-remove --lock L --market <hex> --oi <hex> --shares <units>
//!   plan open      --lock L --secret F --market <hex> --oi <hex> --side long|short --margin <units> --notional <units>
//!   plan close     --lock L --secret F --market <hex> --oi <hex> --position <hex>
//!   plan liquidate --lock L --market <hex> --oi <hex> --key <hex> --position <hex>
//!   plan demo      [--lp A]       every method on the emulator, accepted and refused
use perp_core::kit::host::{demo_step, emit, fail, hex8, parse_hex8, real_hash, Args, Mock, Transition};
use perp_core::plan::{self, inputs, key_of, lock_of};
use perp_core::*;

fn main() {
    let a = Args::parse();
    match a.cmd.as_str() {
        "key" => println!("{}", hex8(&key_of(&a.words8("secret"), real_hash))),
        "operate" | "lp-add" | "lp-remove" | "open" | "close" | "liquidate" => one(&a),
        "demo" => demo(a.u64_or("lp", 6) as u32),
        _ => fail("usage: plan key|operate|lp-add|lp-remove|open|close|liquidate|demo … (see the top of core/src/bin/plan.rs)"),
    }
}

fn one(a: &Args) {
    let public = a.words8("lock");
    let market = Market::from_value(&a.cell("market"));
    let live = || market.unwrap_or_else(|| fail("the market does not exist yet: ./price.sh <price> <lp asset> first"));
    let oi = Oi::from_value(&a.cell("oi"));
    let position = || {
        Position::from_value(&a.cell("position")).unwrap_or_else(|| fail("there is no position under this key"))
    };
    let (t, input): (Transition, Vec<u32>) = match a.cmd.as_str() {
        "operate" => {
            let lp = market.map(|m| m.lp).unwrap_or_else(|| a.u32("lp"));
            let (t, after) = plan::operate(market, a.u64("price"), lp).unwrap_or_else(|e| fail(&e));
            eprintln!("operate: market after {after:?}");
            {
            let i = plan::operate_inputs(&a.words8("operator"), &t);
            (t, i)
        }
        }
        "lp-add" => {
            let (t, after, minted) = plan::lp_add(live(), oi, a.u64("rand")).unwrap_or_else(|e| fail(&e));
            eprintln!("lp-add: mints {minted} LP units; market after {after:?}");
            (t, inputs(LP_ADD, None))
        }
        "lp-remove" => {
            let (t, after, x) = plan::lp_remove(live(), oi, a.u64("shares")).unwrap_or_else(|e| fail(&e));
            eprintln!("lp-remove: pays {x} RAND units; market after {after:?}");
            (t, inputs(LP_REMOVE, None))
        }
        "open" => {
            let secret = a.words8("secret");
            let side = match a.str("side") {
                "long" => LONG,
                "short" => SHORT,
                _ => fail("--side long|short"),
            };
            let key = key_of(&secret, real_hash);
            let (t, _, p) = plan::open(live(), oi, key, side, a.u64("margin"), a.u64("notional")).unwrap_or_else(|e| fail(&e));
            eprintln!("open: {p:?} under key {}", hex8(&key));
            (t, inputs(OPEN, Some(&secret)))
        }
        "close" => {
            let secret = a.words8("secret");
            let (t, after, _, x) = plan::close(live(), oi, key_of(&secret, real_hash), position());
            eprintln!("close: pays {x} RAND units; pool cash after {}", after.cash);
            (t, inputs(CLOSE, Some(&secret)))
        }
        _ => {
            let key = parse_hex8(a.str("key")).unwrap_or_else(|e| fail(&e));
            let (t, after, _, x) = plan::liquidate(live(), oi, key, position()).unwrap_or_else(|e| fail(&e));
            eprintln!("liquidate: rewards {x} RAND units; pool cash after {}", after.cash);
            (t, inputs(LIQUIDATE, None))
        }
    };
    if Mock::new(&public, &t, &input, real_hash).accepts(check).is_none() {
        fail("the program would refuse this transition");
    }
    emit(a, &public, &t, &input);
}

const RAND_: u64 = 1_000_000_000;

/// The demo's story: a market, its LPs, a long that wins, a short that is liquidated — and a
/// refusal for every method.
fn demo(lp: u32) {
    let operator = [11, 12, 13, 14, 15, 16, 17, 18];
    let public = lock_of(&operator, real_hash);
    let step = |label: &str, t: &Transition, input: &[u32], accept: bool| {
        let host = Mock::new(&public, t, input, real_hash).accepts(check).is_some();
        assert_eq!(host, accept, "the host rules disagree with the demo's expectation: {label}");
        demo_step(label, &public, t, input, accept);
    };
    let op = |t: &Transition| plan::operate_inputs(&operator, t);

    // The operator creates the market: X at 2 RAND.
    let (t, m) = plan::operate(None, 2 * RAND_, lp).unwrap();
    step("operator creates the market: X at 2 RAND", &t, &op(&t), true);
    let mut oi = Oi::default();

    // An LP funds the pool.
    let (t, m2, minted) = plan::lp_add(m, oi, 1_000 * RAND_).unwrap();
    step(&format!("LP adds 1000 RAND for {minted} LP units"), &t, &inputs(LP_ADD, None), true);
    let mut greedy = t.clone();
    greedy.mints[0].amount += 1;
    greedy.writes[0].1 = Market { supply: m2.supply + 1, ..m2 }.value();
    step("the same deposit minting one LP unit more", &greedy, &inputs(LP_ADD, None), false);
    let m = m2;

    // Alice goes long at the maximum leverage: 10 RAND of margin, 100 RAND of notional.
    let alice = [21, 22, 23, 24, 25, 26, 27, 28];
    let alice_key = key_of(&alice, real_hash);
    let (t, oi2, pa) = plan::open(m, oi, alice_key, LONG, 10 * RAND_, 100 * RAND_).unwrap();
    step(&format!("Alice opens a 10x long: 10 RAND margin, 100 RAND notional, {} units of X", pa.q), &t, &inputs(OPEN, Some(&alice)), true);
    let bob = [31, 32, 33, 34, 35, 36, 37, 38];
    let eleven = Position { margin: 10 * RAND_, q: plan::size_for(LONG, 110 * RAND_, m.price), cost: 110 * RAND_, side: LONG };
    let (t11, _) = plan::open_tx(m, oi2, key_of(&bob, real_hash), eleven);
    step("Bob opens an 11x long (110 RAND on 10)", &t11, &inputs(OPEN, Some(&bob)), false);
    oi = oi2;

    // The price rises 25 %.
    let (t, m2) = plan::operate(Some(m), 25 * RAND_ / 10, lp).unwrap();
    step("operator moves the price: X at 2.5 RAND", &t, &op(&t), true);
    let mut wrong = t.clone();
    wrong.writes[0].1 = Market { cash: m2.cash + 1, ..m2 }.value();
    step("a price update that also moves the pool's cash", &wrong, &op(&wrong), false);
    step("the same price update with Bob's secret, not the operator's", &t, &plan::operate_inputs(&bob, &t), false);
    let m = m2;

    // Alice closes with her profit: 10 margin + 25 profit.
    let (t, m2, oi2, x) = plan::close(m, oi, alice_key, pa);
    step(&format!("Alice closes her long for {x} RAND units (10 margin + 25 profit)"), &t, &inputs(CLOSE, Some(&alice)), true);
    let (greedy, _, _) = plan::settle_tx(m, oi, alice_key, pa, x + 1);
    step("the same close taking one unit more", &greedy, &inputs(CLOSE, Some(&alice)), false);
    let m = m2;
    oi = oi2;

    // Carol shorts at 10x; Dave shorts at 2x.
    let carol = [41, 42, 43, 44, 45, 46, 47, 48];
    let carol_key = key_of(&carol, real_hash);
    let (t, oi2, pc) = plan::open(m, oi, carol_key, SHORT, 10 * RAND_, 100 * RAND_).unwrap();
    step(&format!("Carol opens a 10x short at 2.5: {} units of X", pc.q), &t, &inputs(OPEN, Some(&carol)), true);
    oi = oi2;
    let dave = [51, 52, 53, 54, 55, 56, 57, 58];
    let dave_key = key_of(&dave, real_hash);
    let (t, oi2, pd) = plan::open(m, oi, dave_key, SHORT, 50 * RAND_, 100 * RAND_).unwrap();
    step("Dave opens a 2x short: 50 RAND margin, 100 RAND notional", &t, &inputs(OPEN, Some(&dave)), true);
    oi = oi2;

    // The price rises 6 %: Carol's equity is 4 RAND, under 5 % of 100.
    let (t, m2) = plan::operate(Some(m), 265 * RAND_ / 100, lp).unwrap();
    step("operator moves the price: X at 2.65 RAND", &t, &op(&t), true);
    let m = m2;

    let (t, m2, oi2, x) = plan::liquidate(m, oi, carol_key, pc).unwrap();
    step(&format!("a keeper liquidates Carol's short for a {x}-unit reward (1 % of notional)"), &t, &inputs(LIQUIDATE, None), true);
    let (greedy, _, _) = plan::settle_tx(m, oi, carol_key, pc, x + 1);
    step("the same liquidation taking one unit more", &greedy, &inputs(LIQUIDATE, None), false);
    let m = m2;
    oi = oi2;
    let (healthy, _, _) = plan::settle_tx(m, oi, dave_key, pd, 0);
    step("a keeper liquidating Dave's healthy short", &healthy, &inputs(LIQUIDATE, None), false);

    // A second LP joins at the pool's net asset value (Dave's loss is the pool's gain).
    let (t, m2, minted) = plan::lp_add(m, oi, 100 * RAND_).unwrap();
    step(&format!("a second LP adds 100 RAND for {minted} LP units"), &t, &inputs(LP_ADD, None), true);
    let m = m2;

    // An LP withdraws; then one tries to take what backs Dave's position.
    let (t, m2, x) = plan::lp_remove(m, oi, 500 * RAND_).unwrap();
    step(&format!("an LP burns 500 LP units for {x} RAND units"), &t, &inputs(LP_REMOVE, None), true);
    let m = m2;
    // 556 LP units' fair share is in the pool's cash, but would leave less than Dave's notional.
    let b = 556 * RAND_;
    let fair = plan::lp_quote(&m, &oi, b);
    assert!(fair <= m.cash && m.cash - fair < pd.cost);
    let (over, _) = plan::lp_remove_tx(m, oi, b, fair);
    let left = m.cash - fair;
    step(&format!("burning 556 LP units for {fair}, leaving {left} of cash behind Dave's 100 RAND notional"), &over, &inputs(LP_REMOVE, None), false);

    // Only Dave's secret closes Dave's position.
    let (t, _, _, x) = plan::close(m, oi, dave_key, pd);
    step("Dave's position closed with Bob's secret", &t, &inputs(CLOSE, Some(&bob)), false);
    step(&format!("Dave closes his short for {x} RAND units"), &t, &inputs(CLOSE, Some(&dave)), true);
    step("an unknown method", &t, &[9], false);
}
