//! The amm's rules, on the host: every method accepted at its best amounts and refused one unit
//! past them, and no context word of any accepted transition left unchecked.
use amm_core::kit::host::{loose_words, test_hash, Mock, Out, Transition};
use amm_core::{check, plan, swap_ok, Pool, Refusal, ADD, POOL_KEY, REMOVE, SWAP};

const T: u32 = 5;
const LP: u32 = 6;

fn run(t: &Transition, method: u32) -> Result<[u32; 8], Refusal> {
    let m = Mock::new(&[T], t, &[method], test_hash);
    let r = check(&m);
    if m.oob.get() { Err(Refusal::Shape) } else { r }
}

fn seeded() -> Pool {
    plan::add(None, T, LP, 10_000_000_000, 40_000_000_000).unwrap().1
}

#[test]
fn first_add_mints_sqrt_less_the_locked_minimum() {
    let (t, p, minted) = plan::add(None, T, LP, 10_000_000_000, 40_000_000_000).unwrap();
    assert_eq!(p.s, 20_000_000_000);
    assert_eq!(minted, 20_000_000_000 - 1_000);
    assert_eq!(run(&t, ADD).unwrap()[0], ADD);
    // A first deposit cannot name RAND or the traded token as the share token.
    for bad in [0, T] {
        let (t, _, _) = plan::add(None, T, bad, 10_000_000_000, 40_000_000_000).unwrap();
        assert_eq!(run(&t, ADD), Err(Refusal::Asset));
    }
}

#[test]
fn every_method_accepts_its_best_and_refuses_one_more() {
    let p = seeded();
    let (t, _, m) = plan::add(Some(p), T, LP, 1_000_000_000, 4_000_000_000).unwrap();
    assert!(run(&t, ADD).is_ok());
    let mut g = t.clone();
    g.mints[0].amount = m + 1;
    g.writes[0].1 = Pool { s: p.s + m + 1, ..Pool::from_value(&t.writes[0].1).unwrap() }.value();
    assert_eq!(run(&g, ADD), Err(Refusal::Price));

    for rand_in in [true, false] {
        let (t, after, out) = plan::swap(p, T, rand_in, 2_000_000_000, 1).unwrap();
        assert!(run(&t, SWAP).is_ok());
        let mut g = t.clone();
        g.pays[0].amount = out + 1;
        let after = if rand_in { Pool { rt: after.rt - 1, ..after } } else { Pool { rr: after.rr - 1, ..after } };
        g.writes[0].1 = after.value();
        assert_eq!(run(&g, SWAP), Err(Refusal::Price));
    }

    let (t, _, r, _) = plan::remove(p, T, p.s / 3).unwrap();
    assert!(run(&t, REMOVE).is_ok());
    let mut g = t.clone();
    g.pays[0].amount = r + 1;
    g.writes[0].1 = Pool { rr: p.rr - r - 1, ..Pool::from_value(&t.writes[0].1).unwrap() }.value();
    assert_eq!(run(&g, REMOVE), Err(Refusal::Price));
}

#[test]
fn no_word_is_left_unchecked() {
    let p = seeded();
    let cases = [
        (plan::add(None, T, LP, 10_000_000_000, 40_000_000_000).unwrap().0, ADD),
        (plan::add(Some(p), T, LP, 1_000_000_000, 4_000_000_000).unwrap().0, ADD),
        (plan::swap(p, T, true, 2_000_000_000, 1).unwrap().0, SWAP),
        (plan::swap(p, T, false, 2_000_000_000, 1).unwrap().0, SWAP),
        (plan::remove(p, T, p.s / 3).unwrap().0, REMOVE),
    ];
    for (t, method) in cases {
        let loose = loose_words(&[T], &t, &[method], test_hash, check);
        assert!(loose.is_empty(), "method {method}: words accepted when changed: {loose:?}");
    }
}

#[test]
fn shape_and_inflow_are_pinned() {
    let p = seeded();
    let (t, _, _) = plan::swap(p, T, true, 2_000_000_000, 1).unwrap();
    // A second payout, a mint, a token deposit beside the RAND, a pool cell under another key.
    let mut a = t.clone();
    a.pays.push(Out::new(0, 1));
    assert_eq!(run(&a, SWAP), Err(Refusal::Shape));
    let mut b = t.clone();
    b.mints.push(Out::new(LP, 1));
    assert_eq!(run(&b, SWAP), Err(Refusal::Shape));
    let c = t.clone().deposit(T, 1);
    assert_eq!(run(&c, SWAP), Err(Refusal::Inflow));
    let mut d = t.clone();
    d.reads[0].0 = [2, 0, 0, 0, 0, 0, 0, 0];
    assert_eq!(run(&d, SWAP), Err(Refusal::Key));
    // Swapping against an absent pool.
    let mut e = t.clone();
    e.reads[0] = (POOL_KEY, [0; 8]);
    assert_eq!(run(&e, SWAP), Err(Refusal::Pool));
}

#[test]
fn the_constant_product_never_falls() {
    let p = seeded();
    for amount in [1u64, 999, 1_000_000, 3_000_000_000, 9_000_000_000_000] {
        for rand_in in [true, false] {
            let out = plan::quote(&p, rand_in, amount);
            let (r_in, r_out) = if rand_in { (p.rr, p.rt) } else { (p.rt, p.rr) };
            assert!(swap_ok(r_in, r_out, amount, out));
            let k0 = (p.rr as u128) * (p.rt as u128);
            let k1 = ((r_in + amount) as u128) * ((r_out - out) as u128);
            assert!(k1 >= k0, "k fell: {k0} → {k1}");
        }
    }
}
