//! plan: build a stablecoin transition from the cells, check it with the program's own rules,
//! and hand it to the scripts (`--t t.json --i inputs.json`; the emulator's words on stdout).
//!
//!   plan key       --secret <file>                                   the position's key (64 hex)
//!   plan operate   --operator <file> --lock <file> --config <hex> --price <units> [--stable A]
//!   plan adjust    --secret <file> --lock <file> --config <hex> --position <hex>
//!                  [--deposit <RAND units>] [--withdraw <RAND units>|max]
//!                  [--mint <stable units>|max] [--repay <stable units>|all]
//!   plan liquidate --lock <file> --config <hex> --position <hex> --key <hex>
//!   plan demo      [--stable A]      every method on the emulator, accepted and refused
//!
//! The price is stable base units per whole RAND. `--stable` is needed only the first time: it
//! binds the stable token. `--lock` is `public.txt`, the program's public input.
use stablecoin_core::kit::host::{demo_step, emit, fail, hex8, parse_hex8, real_hash, Args, Mock, Transition};
use stablecoin_core::plan::{self, Moves};
use stablecoin_core::kit::RAND;
use stablecoin_core::{check, Config, Position};

fn main() {
    let a = Args::parse();
    match a.cmd.as_str() {
        "key" => println!("{}", hex8(&plan::key_of(real_hash, &a.words8("secret")))),
        "operate" | "adjust" | "liquidate" => one(&a),
        "demo" => demo(a.u64_or("stable", 6) as u32),
        _ => fail("usage: plan key|operate|adjust|liquidate|demo … (see the top of core/src/bin/plan.rs)"),
    }
}

fn config(a: &Args) -> Option<Config> {
    let v = a.cell("config");
    let c = Config::from_value(&v);
    if c.is_none() && v != [0; 8] {
        fail("the config cell is not a config");
    }
    c
}

fn position(a: &Args) -> Position {
    Position::from_value(&a.cell("position")).unwrap_or_else(|| fail("the position cell is not a position"))
}

/// An amount, or the word that names the largest one.
fn amount(a: &Args, k: &str, word: &str, best: impl Fn() -> u64) -> u64 {
    match a.opt(k) {
        None => 0,
        Some(w) if w == word => best(),
        Some(_) => a.u64(k),
    }
}

fn one(a: &Args) {
    let lock = a.words8("lock");
    let (t, input) = match a.cmd.as_str() {
        "operate" => {
            let before = config(a);
            if let (Some(c), Some(s)) = (before, a.opt("stable")) {
                if s.parse::<u32>().ok() != Some(c.stable) {
                    fail(&format!("the stable token is already bound to asset {}; it cannot change", c.stable));
                }
            }
            let stable = if before.is_none() { a.u32("stable") } else { 0 };
            let (t, input, after) = plan::operate(&a.words8("operator"), before, stable, a.u64("price")).unwrap_or_else(|e| fail(&e));
            eprintln!("operate: config after: {after:?}");
            (t, input)
        }
        "adjust" => {
            let cfg = config(a).unwrap_or_else(|| fail("no config yet: the operator sets the first price"));
            let p = position(a);
            let owner = a.words8("secret");
            let deposit = a.u64_or("deposit", 0);
            let repay = amount(a, "repay", "all", || p.debt);
            if a.opt("mint") == Some("max") && a.opt("withdraw") == Some("max") {
                fail("--mint max and --withdraw max together: choose one");
            }
            let explicit_w = if a.opt("withdraw") == Some("max") { 0 } else { a.u64_or("withdraw", 0) };
            let mint = amount(a, "mint", "max", || {
                plan::max_mint(&cfg, (p.coll + deposit).saturating_sub(explicit_w), p.debt.saturating_sub(repay))
            });
            let withdraw = amount(a, "withdraw", "max", || {
                plan::max_withdraw(&cfg, p.coll + deposit, (p.debt + mint).saturating_sub(repay))
            });
            let m = Moves { deposit, withdraw, mint, repay };
            let key = plan::key_of(real_hash, &owner);
            let (t, input, after) = plan::adjust(&owner, key, cfg, p, m).unwrap_or_else(|e| fail(&e));
            eprintln!("adjust: {m:?}; position after: {after:?}");
            (t, input)
        }
        _ => {
            let cfg = config(a).unwrap_or_else(|| fail("no config yet"));
            let key = parse_hex8(a.str("key")).unwrap_or_else(|e| fail(&format!("--key: {e}")));
            let p = position(a);
            let (t, input) = plan::liquidate(key, cfg, p).unwrap_or_else(|e| fail(&e));
            eprintln!("liquidate: burns {} stable units, takes {} RAND units", p.debt, p.coll);
            (t, input)
        }
    };
    if Mock::new(&lock, &t, &input, real_hash).accepts(check).is_none() {
        fail("the program would refuse this transition (a wrong secret, or a stale cell?)");
    }
    emit(a, &lock, &t, &input);
}

/// Every method, accepted, and a greedy or tampered variant of each, refused.
fn demo(stable: u32) {
    let operator = [11, 12, 13, 14, 15, 16, 17, 18];
    let owner = [21, 22, 23, 24, 25, 26, 27, 28];
    let stranger = [31, 32, 33, 34, 35, 36, 37, 38];
    let lock = plan::lock_of(real_hash, &operator);
    let key = plan::key_of(real_hash, &owner);
    let step = |label: &str, t: &Transition, input: &[u32], accept: bool| {
        let host = Mock::new(&lock, t, input, real_hash).accepts(check).is_some();
        assert_eq!(host, accept, "the host rules disagree with the demo's expectation: {label}");
        demo_step(label, &lock, t, input, accept);
    };
    let with_secret = |input: &[u32], s: &[u32; 8]| {
        let mut v = input.to_vec();
        v[1..9].copy_from_slice(s);
        v
    };

    // operate: bind the stable token and set the first price, 2.000000000 stable per RAND.
    let (t, input, cfg) = plan::operate(&operator, None, stable, 2_000_000_000).unwrap();
    step(&format!("operator binds stable token {stable} at price 2.0 stable per RAND"), &t, &input, true);
    let (t2, input2, _) = plan::operate(&operator, Some(cfg), stable, 5_000_000_000).unwrap();
    step("a non-operator setting the price to 5.0", &t2, &with_secret(&input2, &stranger), false);

    // adjust: open with 10 RAND and borrow the most 150 % allows.
    let none = Position::default();
    let most = plan::max_mint(&cfg, 10_000_000_000, 0);
    let m = Moves { deposit: 10_000_000_000, mint: most, ..Moves::default() };
    let (t, input, p) = plan::adjust(&owner, key, cfg, none, m).unwrap();
    step(&format!("open: lock 10 RAND, mint {most} stable units (the most at 150 %)"), &t, &input, true);
    let mut greedy = t.clone();
    greedy.mints[0].amount += 1;
    greedy.writes[1].1 = Position { debt: p.debt + 1, ..p }.value();
    step("the same open minting one unit more", &greedy, &input, false);

    let (t, input, p2) = plan::adjust(&owner, key, cfg, p, Moves { repay: 3_333_333_333, ..Moves::default() }).unwrap();
    step("repay 3333333333 stable units", &t, &input, true);
    step("the same repayment with a wrong owner secret", &t, &with_secret(&input, &stranger), false);
    let p = p2;

    let w = plan::max_withdraw(&cfg, p.coll, p.debt);
    let (t, input, p2) = plan::adjust(&owner, key, cfg, p, Moves { withdraw: 2_000_000_000, ..Moves::default() }).unwrap();
    step("withdraw 2 RAND", &t, &input, true);
    let (tw, inputw, _) = plan::adjust(&owner, key, cfg, p, Moves { withdraw: w, ..Moves::default() }).unwrap();
    let mut greedy = tw.clone();
    greedy.pays[0].amount += 1;
    greedy.writes[1].1 = Position { coll: p.coll - w - 1, ..p }.value();
    step(&format!("withdraw {} RAND units, one past 150 %", w + 1), &greedy, &inputw, false);
    let p = p2;

    // liquidate: refused while the position is at or above 110 %.
    let early = Transition::new()
        .read(stablecoin_core::CONFIG_KEY, cfg.value())
        .read(key, p.value())
        .write(stablecoin_core::CONFIG_KEY, cfg.value())
        .write(key, [0; 8])
        .burn(cfg.stable, p.debt)
        .pay(RAND, p.coll);
    step(&format!("liquidate at price 2.0 (the position is at {} %)", ratio(&cfg, &p)), &early, &[stablecoin_core::LIQUIDATE], false);

    // The operator drops the price to the highest at which the position is below 110 %.
    let low = plan::liquidation_price(&p);
    let (t, input, cfg) = plan::operate(&operator, Some(cfg), stable, low).unwrap();
    step(&format!("operator drops the price to {low}"), &t, &input, true);

    let (t, input) = plan::liquidate(key, cfg, p).unwrap();
    let mut greedy = t.clone();
    greedy.pays[0].amount += 1;
    step("a liquidation taking one RAND unit more than the collateral", &greedy, &input, false);
    let mut short = t.clone();
    short.burn_a -= 1;
    step("a liquidation burning one stable unit less than the debt", &short, &input, false);
    step(
        &format!("liquidate: burn {} stable units, take {} RAND units (at {} %)", p.debt, p.coll, ratio(&cfg, &p)),
        &t,
        &input,
        true,
    );
    step("an unknown method", &t, &[9], false);
}

/// The collateral ratio, in hundredths of a percent, for the labels.
fn ratio(cfg: &Config, p: &Position) -> String {
    let r = (p.coll as u128 * cfg.price as u128 * 10_000) / (p.debt as u128 * 1_000_000_000);
    format!("{}.{:02}", r / 100, r % 100)
}
