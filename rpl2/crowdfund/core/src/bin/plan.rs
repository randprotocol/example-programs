//! plan: build a crowdfund transition from the campaign's cell, check it with the program's own
//! rules, and hand it to the scripts (`--t t.json --i inputs.json`; the emulator's words on
//! stdout). `--public` is the deploy's `public.txt` (the creator's lock, then the goal).
//!
//!   plan init   --public P --secret S --receipt R
//!   plan pledge --public P --campaign <hex> --amount <units>
//!   plan refund --public P --campaign <hex> --amount <units>
//!   plan claim  --public P --secret S --campaign <hex>
//!   plan status --public P --campaign <hex>
//!   plan demo   [--receipt R]        every method on the emulator, accepted and refused
use crowdfund_core::kit::host::{demo_step, emit, fail, real_hash, Args, Mock, Transition};
use crowdfund_core::{check, plan, Campaign, CAMPAIGN_KEY, CLAIM, PLEDGE, RAND, REFUND, TAG_CREATOR};

fn main() {
    let a = Args::parse();
    match a.cmd.as_str() {
        "init" | "pledge" | "refund" | "claim" => one(&a),
        "status" => status(&a),
        "demo" => demo(a.u64_or("receipt", 6) as u32),
        _ => fail("usage: plan init|pledge|refund|claim|status|demo … (see the top of core/src/bin/plan.rs)"),
    }
}

/// The ten words of `public.txt`, and the goal they end with.
fn public_of(a: &Args) -> (Vec<u32>, u64) {
    let p = a.str("public");
    let text = std::fs::read_to_string(p).unwrap_or_else(|e| fail(&format!("{p}: {e}")));
    let w: Vec<u32> = text
        .split_whitespace()
        .map(|t| t.parse::<u32>())
        .collect::<Result<_, _>>()
        .unwrap_or_else(|e| fail(&format!("{p}: {e}")));
    if w.len() != 10 {
        fail(&format!("{p}: not ten words (the lock, then the goal low and high)"));
    }
    let goal = (w[8] as u64) | ((w[9] as u64) << 32);
    (w, goal)
}

/// `--campaign`: the cell's 64 hex; empty or missing reads as an absent cell (zeros).
fn campaign_cell(a: &Args) -> [u32; 8] {
    match a.opt("campaign") {
        Some(h) if !h.trim().is_empty() => a.cell("campaign"),
        _ => [0; 8],
    }
}

fn status(a: &Args) {
    let (_, goal) = public_of(a);
    match Campaign::from_value(&campaign_cell(a)) {
        None => println!("no campaign yet (goal {goal} RAND units): the creator runs ./init.sh"),
        Some(c) => {
            let state = if c.claimed { "claimed: final" } else if c.raised >= goal { "goal met: claimable" } else { "open" };
            println!(
                "raised {} of a {} goal ({} to go); receipt token {}; {state}",
                c.raised,
                goal,
                plan::to_goal(&c, goal),
                c.receipt
            );
        }
    }
}

fn one(a: &Args) {
    let (public, goal) = public_of(a);
    let campaign = Campaign::from_value(&campaign_cell(a));
    let (t, input) = match a.cmd.as_str() {
        "init" => {
            if campaign.is_some() {
                fail("the campaign exists already");
            }
            let receipt = a.u32("receipt");
            let (t, _) = plan::init(receipt).unwrap_or_else(|e| fail(&e));
            eprintln!("init: receipts are asset {receipt}; goal {goal} RAND units");
            (t, plan::init_input(&a.words8("secret"), receipt))
        }
        "pledge" => {
            let (t, after) = plan::pledge(campaign, a.u64("amount")).unwrap_or_else(|e| fail(&e));
            eprintln!("pledge: mints {} receipts (asset {}); raised after: {} of {goal}", a.u64("amount"), after.receipt, after.raised);
            (t, vec![PLEDGE])
        }
        "refund" => {
            let (t, after) = plan::refund(campaign, a.u64("amount")).unwrap_or_else(|e| fail(&e));
            eprintln!("refund: pays {} RAND units; raised after: {} of {goal}", a.u64("amount"), after.raised);
            (t, vec![REFUND])
        }
        _ => {
            let (t, after) = plan::claim(campaign, goal).unwrap_or_else(|e| fail(&e));
            eprintln!("claim: pays {} RAND units; the campaign is final", after.raised);
            (t, plan::creator_input(CLAIM, &a.words8("secret")))
        }
    };
    if Mock::new(&public, &t, &input, real_hash).accepts(check).is_none() {
        fail("the program would refuse this transition (is it the creator's secret?)");
    }
    emit(a, &public, &t, &input);
}

/// Every method, accepted, and a greedy, tampered or out-of-turn variant of each, refused.
fn demo(receipt: u32) {
    let secret: [u32; 8] = [11, 22, 33, 44, 55, 66, 77, 88];
    let wrong: [u32; 8] = [11, 22, 33, 44, 55, 66, 77, 89];
    let goal: u64 = 10_000_000_000;
    let lock = real_hash([TAG_CREATOR, secret[0], secret[1], secret[2], secret[3], secret[4], secret[5], secret[6], secret[7]]);
    let public = plan::public(&lock, goal);
    let step = |label: &str, t: &Transition, input: &[u32], accept: bool| {
        let host = Mock::new(&public, t, input, real_hash).accepts(check).is_some();
        assert_eq!(host, accept, "the host rules disagree with the demo's expectation: {label}");
        demo_step(label, &public, t, input, accept);
    };
    let creator = |m| plan::creator_input(m, &secret);
    let impostor = |m| plan::creator_input(m, &wrong);

    let (t, c) = plan::init(receipt).unwrap();
    step(&format!("init: a 10 RAND goal, receipts are asset {receipt}"), &t, &plan::init_input(&secret, receipt), true);
    step("init with a wrong secret", &t, &plan::init_input(&wrong, receipt), false);
    step("init declaring a receipt token other than the creator's", &t, &plan::init_input(&secret, receipt + 1), false);

    let (t, c) = plan::pledge(Some(c), 3_000_000_000).unwrap();
    step("backer A pledges 3 RAND for 3 RAND of receipts", &t, &[PLEDGE], true);
    let mut other = t.clone();
    other.mints[0].asset = receipt + 1;
    step("the same pledge minting a different asset", &other, &[PLEDGE], false);
    let mut greedy = t.clone();
    greedy.mints[0].amount += 1;
    step("the same pledge minting one receipt too many", &greedy, &[PLEDGE], false);
    let (t, c) = plan::pledge(Some(c), 4_000_000_000).unwrap();
    step("backer B pledges 4 RAND", &t, &[PLEDGE], true);

    let short = Transition::new()
        .read(CAMPAIGN_KEY, c.value())
        .write(CAMPAIGN_KEY, Campaign { claimed: true, ..c }.value())
        .pay(RAND, c.raised);
    step("the creator claims 7 RAND of a 10 RAND goal", &short, &creator(CLAIM), false);

    let (t, c) = plan::refund(Some(c), 1_000_000_000).unwrap();
    step("backer A burns 1 RAND of receipts for 1 RAND back", &t, &[REFUND], true);
    let mut greedy = t.clone();
    greedy.pays[0].amount += 1;
    step("the same refund paying one unit more", &greedy, &[REFUND], false);
    let over = c.raised + 1;
    let too_much = Transition::new()
        .read(CAMPAIGN_KEY, c.value())
        .write(CAMPAIGN_KEY, Campaign { raised: 0, ..c }.value())
        .burn(c.receipt, over)
        .pay(RAND, over);
    step("a refund of more than the campaign holds", &too_much, &[REFUND], false);

    let need = plan::to_goal(&c, goal);
    let (t, c) = plan::pledge(Some(c), need).unwrap();
    step(&format!("backer B pledges {need} units more: the goal is met"), &t, &[PLEDGE], true);

    let (t, done) = plan::claim(Some(c), goal).unwrap();
    step("claim with a wrong secret", &t, &impostor(CLAIM), false);
    step(&format!("the creator claims exactly {} units", c.raised), &t, &creator(CLAIM), true);
    let mut greedy = t.clone();
    greedy.pays[0].amount += 1;
    step("the same claim taking one unit more", &greedy, &creator(CLAIM), false);

    let late = Transition::new()
        .read(CAMPAIGN_KEY, done.value())
        .write(CAMPAIGN_KEY, Campaign { raised: done.raised + 1_000_000_000, ..done }.value())
        .rand_in(1_000_000_000)
        .mint(receipt, 1_000_000_000);
    step("a pledge after the claim", &late, &[PLEDGE], false);
    let late = Transition::new()
        .read(CAMPAIGN_KEY, done.value())
        .write(CAMPAIGN_KEY, Campaign { raised: done.raised - 1_000_000_000, ..done }.value())
        .burn(receipt, 1_000_000_000)
        .pay(RAND, 1_000_000_000);
    step("a refund after the claim", &late, &[REFUND], false);
    let again = Transition::new().read(CAMPAIGN_KEY, done.value()).write(CAMPAIGN_KEY, done.value()).pay(RAND, done.raised);
    step("a second claim", &again, &creator(CLAIM), false);
    step("an unknown method", &t, &[9], false);
}
