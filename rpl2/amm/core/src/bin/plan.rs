//! plan: build an amm transition from the pool's cell, check it with the program's own rules,
//! and hand it to the scripts (`--t t.json --i inputs.json`; the emulator's words on stdout).
//!
//!   plan add    --token T --pool <hex> --rand <units> --amount <units> [--lp L]   (L: first deposit)
//!   plan remove --token T --pool <hex> --shares <units>
//!   plan swap   --token T --pool <hex> --sell rand|token --amount <units> [--min-out <units>]
//!   plan demo   [--token T --lp L]      every method on the emulator, accepted and refused
use amm_core::kit::host::{demo_step, emit, fail, test_hash, Args, Mock, Transition};
use amm_core::{check, plan, Pool, ADD, REMOVE, SWAP};

fn main() {
    let a = Args::parse();
    match a.cmd.as_str() {
        "add" | "remove" | "swap" => one(&a),
        "demo" => demo(a.u64_or("token", 5) as u32, a.u64_or("lp", 6) as u32),
        _ => fail("usage: plan add|remove|swap|demo … (see the top of core/src/bin/plan.rs)"),
    }
}

fn one(a: &Args) {
    let token = a.u32("token");
    let pool = Pool::from_value(&a.cell("pool"));
    let need = || pool.unwrap_or_else(|| fail("the pool does not exist yet: add liquidity first"));
    let (t, method) = match a.cmd.as_str() {
        "add" => {
            let lp = match pool {
                Some(p) => p.lp,
                None => a.u32("lp"),
            };
            let (t, after, minted) = plan::add(pool, token, lp, a.u64("rand"), a.u64("amount")).unwrap_or_else(|e| fail(&e));
            eprintln!("add: mints {minted} shares (asset {lp}); pool after: {after:?}");
            (t, ADD)
        }
        "remove" => {
            let (t, after, r, x) = plan::remove(need(), token, a.u64("shares")).unwrap_or_else(|e| fail(&e));
            eprintln!("remove: pays {r} RAND units and {x} token units; pool after: {after:?}");
            (t, REMOVE)
        }
        _ => {
            let rand_in = match a.str("sell") {
                "rand" => true,
                "token" => false,
                _ => fail("--sell rand|token"),
            };
            let (t, after, out) =
                plan::swap(need(), token, rand_in, a.u64("amount"), a.u64_or("min-out", 1)).unwrap_or_else(|e| fail(&e));
            eprintln!("swap: pays {out}; pool after: {after:?}");
            (t, SWAP)
        }
    };
    let public = [token];
    if Mock::new(&public, &t, &[method], test_hash).accepts(check).is_none() {
        fail("the program would refuse this transition");
    }
    emit(a, &public, &t, &[method]);
}

/// Every method, accepted, and one tampered or greedy variant of each, refused.
fn demo(token: u32, lp: u32) {
    let public = [token];
    let step = |label: &str, t: &Transition, method: u32, accept: bool| {
        let host = Mock::new(&public, t, &[method], test_hash).accepts(check).is_some();
        assert_eq!(host, accept, "the host rules disagree with the demo's expectation: {label}");
        demo_step(label, &public, t, &[method], accept);
    };
    let (t, p, _) = plan::add(None, token, lp, 5_000_000_000, 20_000_000_000).unwrap();
    step("first add: 5 RAND + 20 tokens creates the pool", &t, ADD, true);
    let mut greedy = t.clone();
    greedy.mints[0].amount += 1;
    step("first add minting one share too many", &greedy, ADD, false);

    let (t, p2, _) = plan::add(Some(p), token, lp, 1_000_000_000, 4_000_000_000).unwrap();
    step("add 1 RAND + 4 tokens", &t, ADD, true);
    let p = p2;

    let (t, p2, out) = plan::swap(p, token, true, 1_000_000_000, 1).unwrap();
    step(&format!("swap 1 RAND for {out} tokens"), &t, SWAP, true);
    let mut greedy = t.clone();
    greedy.pays[0].amount += 1;
    if let Some(w) = greedy.writes.first_mut() {
        *w = (w.0, Pool { rt: p2.rt - 1, ..p2 }.value());
    }
    step("the same swap taking one unit more", &greedy, SWAP, false);
    let p = p2;

    let (t, p2, out) = plan::swap(p, token, false, 3_000_000_000, 1).unwrap();
    step(&format!("swap 3 tokens for {out} RAND units"), &t, SWAP, true);
    let mut wrong = t.clone();
    wrong.reads[0].1 = Pool { rr: p.rr + 1, ..p }.value();
    step("a swap declaring a reserve the pool does not have", &wrong, SWAP, false);
    let p = p2;

    let (t, _, r, x) = plan::remove(p, token, p.s / 2).unwrap();
    step(&format!("remove half the shares for {r} RAND units and {x} tokens"), &t, REMOVE, true);
    let mut extra = t.clone();
    extra.pays.push(amm_core::kit::host::Out::new(0, 1));
    step("the same removal with a third payout", &extra, REMOVE, false);
    step("an unknown method", &t, 9, false);
}
