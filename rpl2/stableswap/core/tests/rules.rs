//! The stableswap's rules, on the host: every method accepted at its best amounts and refused
//! one unit past them, `D` checked exact, no context word of any accepted transition left
//! unchecked, and the invariant's own properties.
use stableswap_core::kit::host::{loose_words, test_hash, Mock, Out, Transition};
use stableswap_core::{
    add_ok, check, d_exact, fee_covers, g, plan, swap_ok, Pool, Refusal, ADD, A_MAX, MIN_LIQUIDITY, POOL_KEY,
};

const T: u32 = 5;
const LP: u32 = 6;
const A: u64 = 100;
const R: u64 = 1_000_000_000;

fn run_a(a: u64, t: &Transition, input: &[u32]) -> Result<[u32; 8], Refusal> {
    let m = Mock::new(&[T, a as u32], t, input, test_hash);
    let r = check(&m);
    if m.oob.get() { Err(Refusal::Shape) } else { r }
}

fn run(t: &Transition, input: &[u32]) -> Result<[u32; 8], Refusal> {
    run_a(A, t, input)
}

fn seeded() -> Pool {
    plan::add(None, T, A, LP, 1_000 * R, 1_000 * R).unwrap().2
}

/// A pool off its peg: more RAND than token.
fn skewed() -> Pool {
    let p = seeded();
    plan::swap(p, T, A, true, 600 * R, 1).unwrap().2
}

fn with_d(i: &[u32], at: usize, d: u64) -> Vec<u32> {
    let mut i = i.to_vec();
    i[at] = d as u32;
    i[at + 1] = (d >> 32) as u32;
    i
}

#[test]
fn first_add_mints_d_less_the_locked_minimum() {
    let (t, i, p, minted) = plan::add(None, T, A, LP, 1_000 * R, 1_000 * R).unwrap();
    // A balanced pool's invariant is the sum of its reserves.
    assert_eq!(p.s, 2_000 * R);
    assert_eq!(minted, 2_000 * R - MIN_LIQUIDITY);
    assert_eq!(run(&t, &i).unwrap()[0], ADD);
    // A first deposit cannot name RAND or the traded token as the share token.
    for bad in [0, T] {
        let (t, i, _, _) = plan::add(None, T, A, bad, 1_000 * R, 1_000 * R).unwrap();
        assert_eq!(run(&t, &i), Err(Refusal::Asset));
    }
    // Nor be one-sided, nor declare a D0.
    let one = Transition::new().read(POOL_KEY, [0; 8]).write(POOL_KEY, Pool { x: R, y: 0, s: 2_000, lp: LP }.value()).rand_in(R).mint(LP, 1_000);
    assert_eq!(run(&one, &[ADD, 0, 0, 2_000, 0]), Err(Refusal::Zero));
    assert_eq!(run(&t, &with_d(&i, 1, 1)), Err(Refusal::Invariant));
}

#[test]
fn every_method_accepts_its_best_and_refuses_one_more() {
    for p in [seeded(), skewed()] {
        for (in_r, in_t) in [(10 * R, 10 * R), (50 * R, 0), (0, 50 * R), (3 * R, 70 * R)] {
            let (t, i, after, m) = plan::add(Some(p), T, A, LP, in_r, in_t).unwrap();
            assert!(run(&t, &i).is_ok(), "add {in_r} {in_t}");
            let mut g = t.clone();
            g.mints[0].amount = m + 1;
            g.writes[0].1 = Pool { s: after.s + 1, ..after }.value();
            assert_eq!(run(&g, &i), Err(Refusal::Price));
        }

        for rand_in in [true, false] {
            let (t, i, after, out) = plan::swap(p, T, A, rand_in, 20 * R, 1).unwrap();
            assert!(run(&t, &i).is_ok());
            let mut g = t.clone();
            g.pays[0].amount = out + 1;
            let after = if rand_in { Pool { y: after.y - 1, ..after } } else { Pool { x: after.x - 1, ..after } };
            g.writes[0].1 = after.value();
            assert_eq!(run(&g, &i), Err(Refusal::Price));
        }

        let (t, i, _, r, _) = plan::remove(p, T, p.s / 3).unwrap();
        assert!(run(&t, &i).is_ok());
        let mut g = t.clone();
        g.pays[0].amount = r + 1;
        g.writes[0].1 = Pool { x: p.x - r - 1, ..Pool::from_value(&t.writes[0].1).unwrap() }.value();
        assert_eq!(run(&g, &i), Err(Refusal::Price));
    }
}

#[test]
fn d_must_be_exact() {
    let p = skewed();
    let (t, i, _, _) = plan::swap(p, T, A, true, 20 * R, 1).unwrap();
    let d = plan::d_of(A, p.x, p.y);
    for wrong in [d - 1, d + 1, 0] {
        assert_eq!(run(&t, &with_d(&i, 1, wrong)), Err(Refusal::Invariant));
    }
    // And it matters: D − 1 would let the same swap take more.
    let fee = plan::fee_of(20 * R);
    let honest = plan::quote_with(&p, A, true, 20 * R, d, fee);
    assert!(plan::quote_with(&p, A, true, 20 * R, d - 1, fee) > honest);

    let (t, i, after, _) = plan::add(Some(p), T, A, LP, 5 * R, 0).unwrap();
    let d1 = plan::d_of(A, after.x, after.y);
    for (at, v) in [(1, d - 1), (1, d + 1), (3, d1 - 1), (3, d1 + 1)] {
        assert_eq!(run(&t, &with_d(&i, at, v)), Err(Refusal::Invariant));
    }
}

#[test]
fn the_fee_is_pinned() {
    let p = seeded();
    let amount = 20 * R;
    let (t, i, _, _) = plan::swap(p, T, A, false, amount, 1).unwrap();
    let fee = plan::fee_of(amount);
    assert_eq!(fee, amount * 4 / 10_000);
    assert_eq!(plan::fee_of(1), 1);
    assert_eq!(plan::fee_of(2_501), 2);
    assert_eq!(run(&t, &with_d(&i, 3, fee - 1)), Err(Refusal::Fee));
    assert_eq!(run(&t, &with_d(&i, 3, amount + 1)), Err(Refusal::Fee));
    // A larger fee is the caller's own loss, and allowed (it buys a little less).
    let d = plan::d_of(A, p.x, p.y);
    let out = plan::quote_with(&p, A, false, amount, d, fee + 1);
    let (t2, i2, _) = plan::swap_tx(p, T, false, amount, out, d, fee + 1);
    assert!(run(&t2, &i2).is_ok());
    assert_eq!(run(&t2, &with_d(&i2, 3, fee + 2)), Err(Refusal::Price));
    assert!(fee_covers(amount, fee) && !fee_covers(amount, fee - 1));
}

#[test]
fn no_word_is_left_unchecked() {
    let p = skewed();
    let cases = [
        plan::add(None, T, A, LP, 1_000 * R, 700 * R).map(|x| (x.0, x.1)).unwrap(),
        plan::add(Some(p), T, A, LP, 10 * R, 10 * R).map(|x| (x.0, x.1)).unwrap(),
        plan::add(Some(p), T, A, LP, 10 * R, 0).map(|x| (x.0, x.1)).unwrap(),
        plan::add(Some(p), T, A, LP, 0, 10 * R).map(|x| (x.0, x.1)).unwrap(),
        plan::swap(p, T, A, true, 20 * R, 1).map(|x| (x.0, x.1)).unwrap(),
        plan::swap(p, T, A, false, 20 * R, 1).map(|x| (x.0, x.1)).unwrap(),
        plan::remove(p, T, p.s / 3).map(|x| (x.0, x.1)).unwrap(),
    ];
    for (t, input) in cases {
        let loose = loose_words(&[T, A as u32], &t, &input, test_hash, check);
        assert!(loose.is_empty(), "method {}: words accepted when changed: {loose:?}", input[0]);
    }
}

#[test]
fn the_public_input_is_pinned() {
    let (t, i, _, _) = plan::add(None, T, 1, LP, 1_000 * R, 700 * R).unwrap();
    assert!(run_a(1, &t, &i).is_ok());
    let (t, i, _, _) = plan::add(None, T, A_MAX, LP, 1_000 * R, 700 * R).unwrap();
    assert!(run_a(A_MAX, &t, &i).is_ok());
    for bad in [0, A_MAX + 1, 1 << 31] {
        assert_eq!(run_a(bad, &t, &i), Err(Refusal::Public));
    }
    let m = Mock::new(&[0, A as u32], &t, &i, test_hash);
    assert_eq!(check(&m), Err(Refusal::Public));
}

#[test]
fn shape_inflow_and_range_are_pinned() {
    let p = seeded();
    let (t, i, _, _) = plan::swap(p, T, A, true, 20 * R, 1).unwrap();
    let mut a = t.clone();
    a.pays.push(Out::new(0, 1));
    assert_eq!(run(&a, &i), Err(Refusal::Shape));
    let mut b = t.clone();
    b.mints.push(Out::new(LP, 1));
    assert_eq!(run(&b, &i), Err(Refusal::Shape));
    let c = t.clone().deposit(T, 1);
    assert_eq!(run(&c, &i), Err(Refusal::Inflow));
    let mut d = t.clone();
    d.reads[0].0 = [2, 0, 0, 0, 0, 0, 0, 0];
    assert_eq!(run(&d, &i), Err(Refusal::Key));
    let mut e = t.clone();
    e.reads[0] = (POOL_KEY, [0; 8]);
    assert_eq!(run(&e, &i), Err(Refusal::Pool));
    // A swap's private inputs must all be there.
    assert_eq!(run(&t, &i[..3]), Err(Refusal::Shape));
    // An add of another token.
    let (t, i, _, _) = plan::add(Some(p), T, A, LP, R, R).unwrap();
    let mut f = t.clone();
    f.burn_asset = T + 1;
    assert_eq!(run(&f, &i), Err(Refusal::Inflow));
    // Reserves at 2^62 are refused, even where the invariant would hold.
    assert!(plan::add(None, T, A, LP, 1 << 62, R).is_err());
    let big = Pool { x: 1 << 62, y: R, s: 1 << 62, lp: LP };
    let t = Transition::new().read(POOL_KEY, [0; 8]).write(POOL_KEY, big.value()).rand_in(1 << 62).deposit(T, R).mint(LP, (1 << 62) - 1_000);
    assert_eq!(run(&t, &[ADD, 0, 0, 0, 1 << 30]), Err(Refusal::Range));
}

#[test]
fn g_is_monotone_and_matches_its_definition() {
    // f = 4xy(4A(x + y) + D) − 16A·D·xy − D³, exactly, for small values.
    let f = |a: i128, x: i128, y: i128, d: i128| 4 * x * y * (4 * a * (x + y) + d) - 16 * a * d * x * y - d * d * d;
    for a in [1u64, 2, 100, A_MAX] {
        for (x, y) in [(0u64, 0u64), (0, 7), (1, 1), (3, 1_000), (1_000, 1_000), (12_345, 54_321), (1 << 20, 3)] {
            let d = plan::d_of(a, x, y);
            assert!(d_exact(a, x, y, d));
            for dd in d.saturating_sub(3)..d + 4 {
                assert_eq!(g(a, x, y, dd), f(a as i128, x as i128, y as i128, dd as i128) >= 0, "a {a} x {x} y {y} d {dd}");
            }
            // Falling in D: true up to the invariant, false after.
            assert!([0, d / 3, d / 2, d].iter().all(|&dd| g(a, x, y, dd)));
            assert!(!g(a, x, y, d + 1) && !g(a, x, y, d + 1_000));
            // Rising in each reserve, symmetric.
            assert!(g(a, x + 1, y, d) && g(a, x, y + 1, d) && g(a, y, x, d));
        }
    }
    // Near the edge of the range: the 256-bit products neither wrap nor disagree.
    let (x, y) = ((1u64 << 62) - 1, (1u64 << 61) + 12_345);
    for a in [1, A_MAX] {
        let d = plan::d_of(a, x, y);
        assert!(d_exact(a, x, y, d) && d <= x + y && d > x.max(y));
        assert!(!g(a, 1 << 62, y, 1) && !g(0, x, y, 1));
    }
}

#[test]
fn swaps_never_decrease_d() {
    for p in [seeded(), skewed()] {
        let d0 = plan::d_of(A, p.x, p.y);
        for amount in [1u64, 999, 2_500, 1_000_000, 3 * R, 300 * R, 5_000 * R] {
            for rand_in in [true, false] {
                let Ok((t, i, after, out)) = plan::swap(p, T, A, rand_in, amount, 1) else { continue };
                assert!(run(&t, &i).is_ok());
                let (r_in, r_out) = if rand_in { (p.x, p.y) } else { (p.y, p.x) };
                assert!(swap_ok(A, d0, r_in, r_out, amount, plan::fee_of(amount), out));
                let d1 = plan::d_of(A, after.x, after.y);
                assert!(d1 >= d0, "D fell: {d0} → {d1}");
                // With a fee of at least one unit kept, D grows.
                if amount >= 2_500 {
                    assert!(d1 > d0, "D did not grow on a swap of {amount}");
                }
            }
        }
    }
}

#[test]
fn near_the_peg_it_beats_the_constant_product() {
    let p = seeded();
    for amount in [R, 10 * R, 100 * R] {
        let ss = plan::quote(&p, A, true, amount);
        let cp = plan::quote_constant_product(&p, true, amount);
        let net = amount - plan::fee_of(amount);
        assert!(ss > cp && ss <= net, "{amount}: stableswap {ss}, constant product {cp}");
        // Within 0.1 % of 1:1 after the fee, for trades up to a tenth of the pool at A = 100.
        assert!((net - ss) * 1_000 <= net, "{amount}: {ss} of {net}");
    }
    // A = 1 is much closer to the constant product than A = 1000.
    let amount = 100 * R;
    assert!(plan::quote(&p, 1, true, amount) < plan::quote(&p, 1_000, true, amount));
}

#[test]
fn adding_one_side_then_removing_is_never_a_free_swap() {
    for p in [seeded(), skewed()] {
        for (in_r, in_t) in [(50 * R, 0), (0, 50 * R), (R, 0), (0, 3_000)] {
            let (_, _, mid, m) = plan::add(Some(p), T, A, LP, in_r, in_t).unwrap();
            let (_, _, _, out_r, out_t) = plan::remove(mid, T, m).unwrap();
            // Whatever came back, swapped back into what was put in, never exceeds it.
            let back = if in_r > 0 {
                out_r + plan::quote(&Pool { x: mid.x - out_r, y: mid.y - out_t, s: mid.s - m, lp: LP }, A, false, out_t)
            } else {
                out_t + plan::quote(&Pool { x: mid.x - out_r, y: mid.y - out_t, s: mid.s - m, lp: LP }, A, true, out_r)
            };
            assert!(back < in_r + in_t, "put in {in_r} + {in_t}, got back {back}");
        }
    }
    // And the share price never falls on an add.
    let p = skewed();
    let (_, i, after, m) = plan::add(Some(p), T, A, LP, 7 * R, 0).unwrap();
    let (d0, d1) = (plan::d_of(A, p.x, p.y), plan::d_of(A, after.x, after.y));
    assert_eq!(i[0], ADD);
    assert!(add_ok(p.s, d0, d1, m) && !add_ok(p.s, d0, d1, m + 1));
    assert!((d1 as u128) * (p.s as u128) >= (d0 as u128) * ((p.s + m) as u128));
}
