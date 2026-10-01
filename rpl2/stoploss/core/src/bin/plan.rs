//! plan: build a stoploss transition from the cells, check it with the program's own rules, and
//! hand it to the scripts (`--t t.json --i inputs.json`; the emulator's words on stdout).
//!
//!   plan key     --secret <file>                                      the order key a ticket names (64 hex)
//!   plan operate --operator <file> --oracle <hex> --price <P>         the operator sets the price
//!   plan place   --secret <file> --order <hex> --rand <units>         escrow RAND under the secret's trigger
//!   plan fire    --secret <file> --oracle <hex> --order <hex> [--to rand1…]
//!   plan cancel  --secret <file> --order <hex>
//!   plan status  --oracle <hex> [--secret <file> --order <hex>]       the price; the order, and whether it would fire
//!   plan demo                                                         every method on the emulator, accepted and refused
//!
//! `--oracle` and `--order` are cell values as `rand program state` shows them (64 zeros when
//! absent). `<file>` for `--secret` is a `<name>.secret` (ticket, kind, threshold, salt); for
//! `--operator` it is `operator.secret` (eight words).
use stoploss_core::kit::host::{demo_step, emit, fail, hex8, real_hash, Args, Mock, Transition};
use stoploss_core::{check, commitment, fires, plan, Opening, Oracle, Order, CANCEL, FIRE, ORACLE_KEY, PLACE, STOP, TAG_OPERATOR, TAKE_PROFIT};

fn main() {
    let a = Args::parse();
    match a.cmd.as_str() {
        "key" => {
            let (ticket, _) = plan::read_secret(a.str("secret")).unwrap_or_else(|e| fail(&e));
            println!("{}", hex8(&plan::key_of(&ticket, real_hash)));
        }
        "status" => status(&a),
        "operate" | "place" | "fire" | "cancel" => one(&a),
        "demo" => demo(),
        _ => fail("usage: plan key|operate|place|fire|cancel|status|demo … (see the top of core/src/bin/plan.rs)"),
    }
}

/// The lock a `--operator` file's secret makes: the public input.
fn lock_of(secret: &[u32; 8]) -> [u32; 8] {
    let s = secret;
    real_hash([TAG_OPERATOR, s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7]])
}

fn status(a: &Args) {
    let oracle = Oracle::from_value(&a.cell("oracle"));
    match oracle {
        Some(o) => println!("price: {}", o.price),
        None => println!("price: none yet (the operator has not set one)"),
    }
    if let Some(path) = a.opt("secret") {
        let (ticket, o) = plan::read_secret(path).unwrap_or_else(|e| fail(&e));
        let key = plan::key_of(&ticket, real_hash);
        let what = if o.kind == STOP { "stop" } else { "take-profit" };
        match Order::from_value(&a.cell("order")) {
            None => println!("order {}: none resting (fired, cancelled, or never placed)", hex8(&key)),
            Some(order) => {
                let mine = commitment(&o, real_hash) == order.c;
                println!("order {}: {} RAND units in escrow; this secret's opening {}", hex8(&key), order.amount, if mine { "matches" } else { "does NOT match" });
                if let Some(p) = oracle {
                    println!("the {what} at {} would {} at price {}", o.threshold, if fires(o.kind, p.price, o.threshold) { "fire" } else { "not fire" }, p.price);
                }
            }
        }
    }
}

fn one(a: &Args) {
    let (public, t, input) = match a.cmd.as_str() {
        "operate" => {
            let secret = a.words8("operator");
            let oracle = Oracle::from_value(&a.cell("oracle"));
            let price = a.u64("price");
            let t = plan::operate(oracle, price).unwrap_or_else(|e| fail(&e));
            eprintln!("operate: price {} → {price}", plan::old_price(oracle));
            (lock_of(&secret), t, plan::operate_inputs(&secret, plan::old_price(oracle), price))
        }
        _ => {
            // place, fire and cancel never read the lock: only the public input's length (eight
            // words) matters, for the segment. Eight zeros stand in for it unless a file is given.
            let public = a.opt("public").map_or([0; 8], |_| a.words8("public"));
            let (ticket, o) = plan::read_secret(a.str("secret")).unwrap_or_else(|e| fail(&e));
            let key = plan::key_of(&ticket, real_hash);
            let order = a.cell("order");
            let live = || Order::from_value(&order).unwrap_or_else(|| fail("no live order under that ticket"));
            match a.cmd.as_str() {
                "place" => {
                    let (t, order) = plan::place(key, order, a.u64("rand"), &o, real_hash).unwrap_or_else(|e| fail(&e));
                    eprintln!("place: {} RAND units in escrow under {}", order.amount, hex8(&key));
                    (public, t, plan::inputs(PLACE, &ticket, &o))
                }
                "fire" => {
                    let oracle = Oracle::from_value(&a.cell("oracle")).unwrap_or_else(|| fail("the oracle has no price yet"));
                    let order = live();
                    let t = plan::fire(key, oracle, order, &o, real_hash, a.opt("to").map(str::to_string)).unwrap_or_else(|e| fail(&e));
                    eprintln!("fire: the condition holds at price {}; releases {} RAND units", oracle.price, order.amount);
                    (public, t, plan::inputs(FIRE, &ticket, &o))
                }
                _ => {
                    let order = live();
                    eprintln!("cancel: refunds {} RAND units", order.amount);
                    let mut t = plan::cancel(key, order);
                    t.pays[0].to = a.opt("to").map(str::to_string);
                    (public, t, plan::inputs(CANCEL, &ticket, &o))
                }
            }
        }
    };
    if Mock::new(&public, &t, &input, real_hash).accepts(check).is_none() {
        fail("the program would refuse this transition");
    }
    emit(a, &public, &t, &input);
}

/// Every method, accepted, and tampered or untimely variants of each, refused.
fn demo() {
    let secret = [1, 2, 3, 4, 5, 6, 7, 8];
    let public = lock_of(&secret);
    let step = |label: &str, t: &Transition, input: &[u32], accept: bool| {
        let host = Mock::new(&public, t, input, real_hash).accepts(check).is_some();
        assert_eq!(host, accept, "the host rules disagree with the demo's expectation: {label}");
        demo_step(label, &public, t, input, accept);
    };
    // The fire a dishonest wallet would send when `plan` refuses to build one (the condition
    // false, or the opening wrong): the honest shape, so that only the rule itself refuses it.
    let forced = |p: Oracle, key: [u32; 8], order: Order| {
        Transition::new().read(ORACLE_KEY, p.value()).read(key, order.value()).write(ORACLE_KEY, p.value()).write(key, [0; 8]).pay(0, order.amount).sorted()
    };
    let units = 1_000_000_000;
    // Three owners' tickets and openings, and a ticket nobody placed with.
    let (ta, tb, tc, td) = ([11, 12, 13, 14, 15, 16, 17, 18], [21, 22, 23, 24, 25, 26, 27, 28], [31, 32, 33, 34, 35, 36, 37, 38], [41, 42, 43, 44, 45, 46, 47, 48]);
    let (ka, kb, kc) = (plan::key_of(&ta, real_hash), plan::key_of(&tb, real_hash), plan::key_of(&tc, real_hash));
    let oa = Opening { kind: STOP, threshold: 90, salt: [0x5a17_0001, 0x5a17_0002, 0x5a17_0003, 0x5a17_0004] };
    let ob = Opening { kind: TAKE_PROFIT, threshold: 95, salt: [0x5a17_1001, 0x5a17_1002, 0x5a17_1003, 0x5a17_1004] };
    let oc = Opening { kind: STOP, threshold: 80, salt: [0x5a17_2001, 0x5a17_2002, 0x5a17_2003, 0x5a17_2004] };

    // The operator sets the first price.
    let t = plan::operate(None, 100).unwrap();
    step("operate: the first price, 100", &t, &plan::operate_inputs(&secret, 0, 100), true);
    step("operate with a wrong secret", &t, &plan::operate_inputs(&[8, 7, 6, 5, 4, 3, 2, 1], 0, 100), false);
    let mut p = Oracle { price: 100 };

    // A stop at 90 rests; nothing on chain says 90.
    let (t, a) = plan::place(ka, [0; 8], 5 * units, &oa, real_hash).unwrap();
    step("place A: 5 RAND, a stop at 90 (the chain sees only a commitment)", &t, &plan::inputs(PLACE, &ta, &oa), true);
    let mut dry = t.clone();
    dry.burn_r = 0;
    step("place A with no RAND coming in", &dry, &plan::inputs(PLACE, &ta, &oa), false);

    // At 100 the stop does not fire. No proof exists, so nobody can tell it was tried.
    assert!(plan::fire(ka, p, a, &oa, real_hash, None).is_err(), "plan must refuse to build a fire whose condition does not hold");
    step("fire A at price 100: the condition is false (no proof, so no trace of the attempt)", &forced(p, ka, a), &plan::inputs(FIRE, &ta, &oa), false);

    // The price falls to 85.
    let t = plan::operate(Some(p), 85).unwrap();
    step("operate: the price moves to 85", &t, &plan::operate_inputs(&secret, 100, 85), true);
    p = Oracle { price: 85 };

    // The stop fires for exactly its escrow.
    let t = plan::fire(ka, p, a, &oa, real_hash, None).unwrap();
    step("fire A at price 85: 5 RAND released", &t, &plan::inputs(FIRE, &ta, &oa), true);
    let mut greedy = t.clone();
    greedy.pays[0].amount += 1;
    step("fire A taking one unit more", &greedy, &plan::inputs(FIRE, &ta, &oa), false);

    // A take-profit at 95 rests, does not fire at 85, and fires once the price is back at 100.
    let (t, b) = plan::place(kb, [0; 8], 3 * units, &ob, real_hash).unwrap();
    step("place B: 3 RAND, a take-profit at 95", &t, &plan::inputs(PLACE, &tb, &ob), true);
    step("fire B at price 85: the condition is false", &forced(p, kb, b), &plan::inputs(FIRE, &tb, &ob), false);
    let t = plan::operate(Some(p), 100).unwrap();
    step("operate: the price moves to 100", &t, &plan::operate_inputs(&secret, 85, 100), true);
    p = Oracle { price: 100 };
    let t = plan::fire(kb, p, b, &ob, real_hash, None).unwrap();
    step("fire B at price 100: 3 RAND released", &t, &plan::inputs(FIRE, &tb, &ob), true);

    // A stop at 80 rests. Claiming it was a stop at 100 (which would fire now) fails the commitment.
    let (t, c) = plan::place(kc, [0; 8], 2 * units, &oc, real_hash).unwrap();
    step("place C: 2 RAND, a stop at 80", &t, &plan::inputs(PLACE, &tc, &oc), true);
    let lie = Opening { threshold: 100, ..oc };
    assert!(plan::fire(kc, p, c, &lie, real_hash, None).is_err());
    step("fire C claiming a threshold of 100: the commitment does not match", &forced(p, kc, c), &plan::inputs(FIRE, &tc, &lie), false);

    // The owner cancels C; a stranger's ticket cannot.
    let t = plan::cancel(kc, c);
    step("cancel C with a wrong ticket", &t, &plan::inputs(CANCEL, &td, &oc), false);
    step("cancel C: 2 RAND back", &t, &plan::inputs(CANCEL, &tc, &oc), true);
    step("an unknown method", &t, &plan::inputs(9, &tc, &oc), false);
}
