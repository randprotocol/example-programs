//! plan: build an orderbook transition from an order's cell, check it with the program's own
//! rules, and hand it to the scripts (`--t t.json --i inputs.json`; the emulator's words on stdout).
//!
//!   plan key   --ticket <file>                         the order key a ticket names (64 hex)
//!   plan post  --ticket <file> --order <hex> --give-asset A --give <units> --want-asset B --want <units>
//!   plan fill  --key <hex> --order <hex> (--pay <units of B> | --take <units of A>)
//!   plan close --ticket <file> --order <hex>
//!   plan demo                                          every method on the emulator, accepted and refused
//!
//! `--order` is the order cell's value as `rand program state` shows it (64 zeros when absent).
use orderbook_core::kit::host::{demo_step, emit, fail, hex8, parse_hex8, real_hash, Args, Mock, Transition};
use orderbook_core::plan::{self, Fill};
use orderbook_core::{check, Order, CLOSE, FILL, POST};

fn main() {
    let a = Args::parse();
    match a.cmd.as_str() {
        "key" => println!("{}", hex8(&plan::key_of(&a.words8("ticket"), real_hash))),
        "post" | "fill" | "close" => one(&a),
        "demo" => demo(a.u64_or("token", 5) as u32),
        _ => fail("usage: plan key|post|fill|close|demo … (see the top of core/src/bin/plan.rs)"),
    }
}

fn one(a: &Args) {
    let order = a.cell("order");
    let live = || Order::from_value(&order).unwrap_or_else(|| fail("no live order in that cell"));
    let (t, input) = match a.cmd.as_str() {
        "post" => {
            let ticket = a.words8("ticket");
            let key = plan::key_of(&ticket, real_hash);
            let (t, o) = plan::post(key, order, a.u32("give-asset"), a.u64("give"), a.u32("want-asset"), a.u64("want"))
                .unwrap_or_else(|e| fail(&e));
            eprintln!("post: {o:?}");
            eprintln!("order key: {}", hex8(&key));
            (t, plan::inputs(POST, &ticket, &o))
        }
        "fill" => {
            let key = parse_hex8(a.str("key")).unwrap_or_else(|e| fail(&format!("--key: {e}")));
            let f = match (a.opt("pay"), a.opt("take")) {
                (Some(_), None) => Fill::Pay(a.u64("pay")),
                (None, Some(_)) => Fill::Take(a.u64("take")),
                _ => fail("give one of --pay <units> or --take <units>"),
            };
            let (t, after, x, y) = plan::fill(key, live(), f).unwrap_or_else(|e| fail(&e));
            eprintln!("fill: pays {y} of asset {} for {x} of asset {}; order after: {after:?}", after.want, after.give);
            (t, vec![FILL])
        }
        _ => {
            let ticket = a.words8("ticket");
            let key = plan::key_of(&ticket, real_hash);
            let o = live();
            eprintln!("close: refunds {} of asset {}, pays {} of asset {}", o.give_rem, o.give, o.proceeds, o.want);
            (plan::close(key, o), plan::inputs(CLOSE, &ticket, &o))
        }
    };
    if Mock::new(&[], &t, &input, real_hash).accepts(check).is_none() {
        fail("the program would refuse this transition");
    }
    emit(a, &[], &t, &input);
}

/// Every method, accepted, and greedy or tampered variants of each, refused.
fn demo(token: u32) {
    let step = |label: &str, t: &Transition, input: &[u32], accept: bool| {
        let host = Mock::new(&[], t, input, real_hash).accepts(check).is_some();
        assert_eq!(host, accept, "the host rules disagree with the demo's expectation: {label}");
        demo_step(label, &[], t, input, accept);
    };
    let (ta, tb, tc) = ([1, 2, 3, 4, 5, 6, 7, 8], [9, 10, 11, 12, 13, 14, 15, 16], [17, 18, 19, 20, 21, 22, 23, 24]);
    let (ka, kb, kc) = (plan::key_of(&ta, real_hash), plan::key_of(&tb, real_hash), plan::key_of(&tc, real_hash));
    let units = 1_000_000_000;

    // Order A: 10 RAND for 25 tokens (2.5 tokens a RAND).
    let (t, a) = plan::post(ka, [0; 8], 0, 10 * units, token, 25 * units).unwrap();
    step("post A: 10 RAND for 25 tokens", &t, &plan::inputs(POST, &ta, &a), true);

    let (t, a, x, y) = plan::fill(ka, a, Fill::Pay(10 * units)).unwrap();
    step(&format!("fill A: pay {y} token units for {x} RAND units"), &t, &[FILL], true);

    let y = plan::least_for(&a, 2 * units).unwrap();
    step(&format!("fill A: take 2 RAND for {} token units, one unit under the price", y - 1), &plan::fill_exact(ka, a, 2 * units, y - 1), &[FILL], false);
    step(
        "fill A: take one RAND unit more than the order holds",
        &plan::fill_exact(ka, a, a.give_rem + 1, a.want_rem),
        &[FILL],
        false,
    );

    let (t, a, x, y) = plan::fill(ka, a, Fill::Pay(a.want_rem)).unwrap();
    step(&format!("fill A: pay the remaining {y} token units for the remaining {x} RAND units"), &t, &[FILL], true);
    step("fill A again, now complete: one unit for one unit", &plan::fill_exact(ka, a, 1, 1), &[FILL], false);

    let wrong = plan::close(ka, a);
    step("close A with another order's ticket", &wrong, &plan::inputs(CLOSE, &tb, &a), false);
    step(&format!("close A: collect the {} token units paid in", a.proceeds), &plan::close(ka, a), &plan::inputs(CLOSE, &ta, &a), true);

    // Order B: 30 tokens for 12 RAND, cancelled at once.
    let (t, b) = plan::post(kb, [0; 8], token, 30 * units, 0, 12 * units).unwrap();
    step("post B: 30 tokens for 12 RAND", &t, &plan::inputs(POST, &tb, &b), true);
    step(&format!("close B at once: the {} token units of escrow back", b.give_rem), &plan::close(kb, b), &plan::inputs(CLOSE, &tb, &b), true);

    // Order C: 5 RAND for 10 tokens, filled in part, then closed with both payouts.
    let (t, c) = plan::post(kc, [0; 8], 0, 5 * units, token, 10 * units).unwrap();
    step("post C: 5 RAND for 10 tokens", &t, &plan::inputs(POST, &tc, &c), true);
    let (mut over, o) = plan::post(kc, [0; 8], 0, units, token, units).unwrap();
    over.reads[0].1 = c.value();
    step("post into C's key while C is live", &over, &plan::inputs(POST, &tc, &o), false);
    let (t, c, x, y) = plan::fill(kc, c, Fill::Take(2 * units)).unwrap();
    step(&format!("fill C: take {x} RAND units for {y} token units"), &t, &[FILL], true);
    let mut greedy = plan::close(kc, c);
    greedy.pays[1].amount += 1;
    step("close C collecting one token unit more than was paid in", &greedy, &plan::inputs(CLOSE, &tc, &c), false);
    step(
        &format!("close C: {} RAND units back and {} token units collected", c.give_rem, c.proceeds),
        &plan::close(kc, c),
        &plan::inputs(CLOSE, &tc, &c),
        true,
    );
    step("an unknown method", &plan::close(kc, c), &plan::inputs(9, &tc, &c), false);
}
