//! plan: build a stableswap transition from the pool's cell, check it with the program's own
//! rules, and hand it to the scripts (`--t t.json --i inputs.json`; the emulator's words on
//! stdout). `D` and the swap fee are private inputs: `plan` finds them.
//!
//!   plan add    --token T --amp A --pool <hex> --rand <units> --amount <units> [--lp L]   (L: first deposit)
//!   plan remove --token T --amp A --pool <hex> --shares <units>
//!   plan swap   --token T --amp A --pool <hex> --sell rand|token --amount <units> [--min-out <units>]
//!   plan quote  --token T --amp A --pool <hex>      the pool's D, and what 1 % of it would swap for
//!   plan demo   [--token T --amp A --lp L]          every method on the emulator, accepted and refused
use stableswap_core::kit::host::{demo_step, emit, fail, real_hash, Args, Mock, Out, Transition};
use stableswap_core::{check, plan, Pool};

fn main() {
    let a = Args::parse();
    match a.cmd.as_str() {
        "add" | "remove" | "swap" => one(&a),
        "quote" => quote(&a),
        "demo" => demo(a.u64_or("token", 5) as u32, a.u64_or("amp", 100), a.u64_or("lp", 6) as u32),
        _ => fail("usage: plan add|remove|swap|quote|demo … (see the top of core/src/bin/plan.rs)"),
    }
}

fn one(a: &Args) {
    let (token, amp) = (a.u32("token"), a.u64("amp"));
    let pool = Pool::from_value(&a.cell("pool"));
    let need = || pool.unwrap_or_else(|| fail("the pool does not exist yet: add liquidity first"));
    let (t, input) = match a.cmd.as_str() {
        "add" => {
            let lp = match pool {
                Some(p) => p.lp,
                None => a.u32("lp"),
            };
            let (t, i, after, minted) =
                plan::add(pool, token, amp, lp, a.u64("rand"), a.u64("amount")).unwrap_or_else(|e| fail(&e));
            eprintln!("add: mints {minted} shares (asset {lp}); D {} → {}; pool after: {after:?}", i_d(&i, 1), i_d(&i, 3));
            (t, i)
        }
        "remove" => {
            let (t, i, after, r, x) = plan::remove(need(), token, a.u64("shares")).unwrap_or_else(|e| fail(&e));
            eprintln!("remove: pays {r} RAND units and {x} token units; pool after: {after:?}");
            (t, i)
        }
        _ => {
            let rand_in = match a.str("sell") {
                "rand" => true,
                "token" => false,
                _ => fail("--sell rand|token"),
            };
            let (t, i, after, out) = plan::swap(need(), token, amp, rand_in, a.u64("amount"), a.u64_or("min-out", 1))
                .unwrap_or_else(|e| fail(&e));
            eprintln!("swap: pays {out} (D {}, fee {}); pool after: {after:?}", i_d(&i, 1), i_d(&i, 3));
            (t, i)
        }
    };
    let public = [token, amp as u32];
    if Mock::new(&public, &t, &input, real_hash).accepts(check).is_none() {
        fail("the program would refuse this transition");
    }
    emit(a, &public, &t, &input);
}

/// The u64 in private input words `at`, `at + 1`.
fn i_d(i: &[u32], at: usize) -> u64 {
    (i[at] as u64) | ((i[at + 1] as u64) << 32)
}

fn quote(a: &Args) {
    let amp = a.u64("amp");
    let p = Pool::from_value(&a.cell("pool")).unwrap_or_else(|| fail("the pool does not exist yet"));
    let d = plan::d_of(amp, p.x, p.y);
    let amount = (d / 100).max(1);
    println!("pool: {p:?}");
    println!("D = {d}, value per share {:.9}", d as f64 / p.s as f64);
    println!(
        "selling {amount} RAND units pays {} tokens; selling {amount} tokens pays {} RAND units",
        plan::quote(&p, amp, true, amount),
        plan::quote(&p, amp, false, amount)
    );
}

/// Every method, accepted, and greedy or tampered variants of each, refused.
fn demo(token: u32, amp: u64, lp: u32) {
    let public = [token, amp as u32];
    let step = |label: &str, public: &[u32], t: &Transition, input: &[u32], accept: bool| {
        let host = Mock::new(public, t, input, real_hash).accepts(check).is_some();
        assert_eq!(host, accept, "the host rules disagree with the demo's expectation: {label}");
        demo_step(label, public, t, input, accept);
    };
    const R: u64 = 1_000_000_000; // one RAND, or one token at 9 decimals

    // First add: both sides; a balanced pool has D = x + y exactly.
    let (t, i, p, minted) = plan::add(None, token, amp, lp, 1_000 * R, 1_000 * R).unwrap();
    step(&format!("first add: 1000 RAND + 1000 tokens, D = {}, mints {minted}", p.s), &public, &t, &i, true);
    let mut greedy = t.clone();
    greedy.mints[0].amount += 1;
    step("first add minting one share too many", &public, &greedy, &i, false);

    let (t, i, p2, minted) = plan::add(Some(p), token, amp, lp, 100 * R, 100 * R).unwrap();
    step(&format!("balanced add: 100 RAND + 100 tokens mints {minted}"), &public, &t, &i, true);
    let p = p2;

    // One-sided: shares for the D it adds, less 0.04 %.
    let (t, i, p2, minted) = plan::add(Some(p), token, amp, lp, 50 * R, 0).unwrap();
    let (d0, d1) = (plan::d_of(amp, p.x, p.y), plan::d_of(amp, p2.x, p2.y));
    let fair = ((p.s as u128) * ((d1 - d0) as u128) / (d0 as u128)) as u64;
    step(&format!("one-sided add: 50 RAND, D {d0} → {d1}, mints {minted} (fee-free would be {fair})"), &public, &t, &i, true);
    let mut free = t.clone();
    free.mints[0].amount = fair;
    free.writes[0].1 = Pool { s: p.s + fair, ..p2 }.value();
    step("the same add minting the fee-free amount", &public, &free, &i, false);
    let p = p2;

    // A swap near the peg, against the constant product of the same reserves.
    let amount = 100 * R;
    let cp = plan::quote_constant_product(&p, true, amount);
    let (t, i, p2, out) = plan::swap(p, token, amp, true, amount, 1).unwrap();
    step(&format!("swap 100 RAND for {out} tokens (constant product, same reserves and fee: {cp})"), &public, &t, &i, true);
    let mut greedy = t.clone();
    greedy.pays[0].amount += 1;
    greedy.writes[0].1 = Pool { y: p2.y - 1, ..p2 }.value();
    step("the same swap taking one unit more", &public, &greedy, &i, false);

    // Exactness matters: a smaller D is easier to satisfy, so it would let the swap take more.
    let (d, fee) = (plan::d_of(amp, p.x, p.y), plan::fee_of(amount));
    let more = plan::quote_with(&p, amp, true, amount, d - 1, fee);
    let (t_low, i_low, _) = plan::swap_tx(p, token, true, amount, more, d - 1, fee);
    step(&format!("the swap declaring D − 1 to take {more} ({} more)", more - out), &public, &t_low, &i_low, false);
    let much = plan::quote_with(&p, amp, true, amount, d / 2, fee);
    let (t_half, i_half, _) = plan::swap_tx(p, token, true, amount, much, d / 2, fee);
    step(&format!("the swap declaring D / 2 to take {much}"), &public, &t_half, &i_half, false);
    let (_, i_cheap, _) = plan::swap_tx(p, token, true, amount, out, d, fee - 1);
    step(&format!("the swap declaring a fee of {} instead of {fee}", fee - 1), &public, &t, &i_cheap, false);
    let p = p2;

    let amount = 30 * R;
    let (t, i, p2, out) = plan::swap(p, token, amp, false, amount, 1).unwrap();
    step(&format!("swap 30 tokens for {out} RAND units"), &public, &t, &i, true);
    let mut wrong = t.clone();
    wrong.reads[0].1 = Pool { x: p.x + 1, ..p }.value();
    step("a swap declaring a reserve the pool does not have", &public, &wrong, &i, false);
    let p = p2;

    let (t, i, _, r, x) = plan::remove(p, token, p.s / 2).unwrap();
    step(&format!("remove half the shares for {r} RAND units and {x} tokens"), &public, &t, &i, true);
    let mut extra = t.clone();
    extra.pays.push(Out::new(0, 1));
    step("the same removal with a third payout", &public, &extra, &i, false);

    // A is fixed at deploy and must be in 1..=10000.
    let (t, i, _, _) = plan::add(None, token, amp, lp, 1_000 * R, 1_000 * R).unwrap();
    step("a program deployed with A = 10001", &[token, 10_001], &t, &i, false);
    step("an unknown method", &public, &t, &[9], false);
}
