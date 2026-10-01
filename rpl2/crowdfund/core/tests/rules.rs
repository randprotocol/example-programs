//! crowdfund's rules, on the host: every method accepted at its exact amounts and refused one unit
//! past them, only the creator's secret opens init and claim, the campaign is final once claimed,
//! receipts always cover what was raised, and no context word of any accepted transition is left
//! unchecked.
use crowdfund_core::kit::host::{loose_words, test_hash, Mock, Out, Transition};
use crowdfund_core::{
    check, plan, refund_ok, Campaign, Refusal, CAMPAIGN_KEY, CLAIM, INIT, PLEDGE, RAND, REFUND, TAG_CREATOR,
};

const R: u32 = 6;
const SECRET: [u32; 8] = [1, 2, 3, 4, 5, 6, 7, 8];
const WRONG: [u32; 8] = [1, 2, 3, 4, 5, 6, 7, 9];
const GOAL: u64 = 10_000_000_000;

fn lock() -> [u32; 8] {
    let s = SECRET;
    test_hash([TAG_CREATOR, s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7]])
}

fn public() -> Vec<u32> {
    plan::public(&lock(), GOAL)
}

fn creator(m: u32) -> Vec<u32> {
    plan::creator_input(m, &SECRET)
}

fn init_in(receipt: u32) -> Vec<u32> {
    plan::init_input(&SECRET, receipt)
}

fn run_with(public: &[u32], t: &Transition, input: &[u32]) -> Result<[u32; 8], Refusal> {
    let m = Mock::new(public, t, input, test_hash);
    let r = check(&m);
    if m.oob.get() { Err(Refusal::Shape) } else { r }
}

fn run(t: &Transition, input: &[u32]) -> Result<[u32; 8], Refusal> {
    run_with(&public(), t, input)
}

fn opened() -> Campaign {
    plan::init(R).unwrap().1
}

/// A campaign holding `raised`, unclaimed.
fn holding(raised: u64) -> Campaign {
    Campaign { raised, ..opened() }
}

#[test]
fn init_is_the_creators_and_binds_a_token() {
    let (t, c) = plan::init(R).unwrap();
    assert_eq!(c.value(), [0, 0, R, 0, 0, 0, 0, 1]);
    assert_eq!(run(&t, &init_in(R)).unwrap(), [INIT, 0, 0, 0, 0, 0, 0, 0]);
    assert_eq!(run(&t, &plan::init_input(&WRONG, R)), Err(Refusal::Secret));
    // Without a secret, or without the receipt: the guest reads past the inputs (no proof).
    assert!(run(&t, &[INIT]).is_err());
    assert!(run(&t, &creator(INIT)).is_err());
    // The write naming another token than the creator's input.
    assert_eq!(run(&t, &init_in(R + 1)), Err(Refusal::Value));
    // RAND as the receipt.
    let bad = Transition::new().read(CAMPAIGN_KEY, [0; 8]).write(CAMPAIGN_KEY, [0, 0, RAND, 0, 0, 0, 0, 1]);
    assert_eq!(run(&bad, &init_in(RAND)), Err(Refusal::Asset));
    // Over a live campaign: it would reset what was raised.
    let c = holding(5);
    let again = Transition::new().read(CAMPAIGN_KEY, c.value()).write(CAMPAIGN_KEY, opened().value());
    assert_eq!(run(&again, &init_in(R)), Err(Refusal::State));
    // Opened with something raised, or already claimed.
    for v in [[1, 0, R, 0, 0, 0, 0, 1], [0, 0, R, 1, 0, 0, 0, 1]] {
        let t = Transition::new().read(CAMPAIGN_KEY, [0; 8]).write(CAMPAIGN_KEY, v);
        assert_eq!(run(&t, &init_in(R)), Err(Refusal::Value));
    }
}

#[test]
fn a_zero_goal_refuses_everything() {
    let p = plan::public(&lock(), 0);
    let (t, _) = plan::init(R).unwrap();
    assert_eq!(run_with(&p, &t, &init_in(R)), Err(Refusal::Goal));
    let p = plan::public(&lock(), 1 << 63);
    assert_eq!(run_with(&p, &t, &init_in(R)), Err(Refusal::Goal));
}

#[test]
fn pledge_mints_exactly_what_comes_in() {
    let c = holding(2_000_000_000);
    let (t, after) = plan::pledge(Some(c), 3_000_000_000).unwrap();
    assert_eq!(after.raised, 5_000_000_000);
    assert_eq!(run(&t, &[PLEDGE]).unwrap(), [PLEDGE, 3_000_000_000u64 as u32, 0, 5_000_000_000u64 as u32, 1, 0, 0, 0]);
    // One receipt more or less.
    for d in [1u64, u64::MAX] {
        let mut g = t.clone();
        g.mints[0].amount = g.mints[0].amount.wrapping_add(d);
        assert_eq!(run(&g, &[PLEDGE]), Err(Refusal::Payout));
    }
    // Another asset minted; a token deposited beside the RAND; a payout taken; nothing pledged.
    let mut g = t.clone();
    g.mints[0].asset = R + 1;
    assert_eq!(run(&g, &[PLEDGE]), Err(Refusal::Asset));
    assert_eq!(run(&t.clone().deposit(9, 1), &[PLEDGE]), Err(Refusal::Inflow));
    let mut g = t.clone();
    g.pays.push(Out::new(RAND, 1));
    assert_eq!(run(&g, &[PLEDGE]), Err(Refusal::Shape));
    let zero = Transition::new().read(CAMPAIGN_KEY, c.value()).write(CAMPAIGN_KEY, c.value()).mint(R, 0);
    assert_eq!(run(&zero, &[PLEDGE]), Err(Refusal::Zero));
    // Writing more raised than came in.
    let mut g = t.clone();
    g.writes[0].1 = Campaign { raised: after.raised + 1, ..after }.value();
    assert_eq!(run(&g, &[PLEDGE]), Err(Refusal::Value));
    // A pledge to a campaign that does not exist.
    let mut g = t.clone();
    g.reads[0].1 = [0; 8];
    assert_eq!(run(&g, &[PLEDGE]), Err(Refusal::State));
}

#[test]
fn refund_pays_exactly_what_is_burned_up_to_raised() {
    let c = holding(7_000_000_000);
    let best = plan::refundable(&c, u64::MAX >> 1);
    assert_eq!(best, c.raised);
    let (t, after) = plan::refund(Some(c), best).unwrap();
    assert_eq!(after.raised, 0);
    assert!(run(&t, &[REFUND]).is_ok());
    // One more than raised.
    assert!(!refund_ok(c.raised, best + 1));
    let over = Transition::new()
        .read(CAMPAIGN_KEY, c.value())
        .write(CAMPAIGN_KEY, Campaign { raised: 0, ..c }.value())
        .burn(R, best + 1)
        .pay(RAND, best + 1);
    assert_eq!(run(&over, &[REFUND]), Err(Refusal::Range));
    // Paying one unit more than burned, in RAND or in another asset; burning another token;
    // depositing the receipts instead of burning them.
    let (t, _) = plan::refund(Some(c), 1_000_000_000).unwrap();
    assert!(run(&t, &[REFUND]).is_ok());
    let mut g = t.clone();
    g.pays[0].amount += 1;
    assert_eq!(run(&g, &[REFUND]), Err(Refusal::Payout));
    let mut g = t.clone();
    g.pays[0].asset = R;
    assert_eq!(run(&g, &[REFUND]), Err(Refusal::Asset));
    assert_eq!(run(&t.clone().burn(R + 1, 1_000_000_000), &[REFUND]), Err(Refusal::Inflow));
    assert_eq!(run(&t.clone().deposit(R, 1_000_000_000), &[REFUND]), Err(Refusal::Inflow));
    assert_eq!(run(&t.clone().rand_in(1), &[REFUND]), Err(Refusal::Inflow));
    let zero = Transition::new().read(CAMPAIGN_KEY, c.value()).write(CAMPAIGN_KEY, c.value()).burn(R, 0).pay(RAND, 0);
    assert_eq!(run(&zero, &[REFUND]), Err(Refusal::Zero));
}

#[test]
fn claim_needs_the_goal_the_secret_and_takes_exactly_raised() {
    // One unit short of the goal.
    let c = holding(GOAL - 1);
    let short = Transition::new()
        .read(CAMPAIGN_KEY, c.value())
        .write(CAMPAIGN_KEY, Campaign { claimed: true, ..c }.value())
        .pay(RAND, c.raised);
    assert_eq!(run(&short, &creator(CLAIM)), Err(Refusal::Short));
    assert_eq!(plan::to_goal(&c, GOAL), 1);
    // Exactly the goal, and above it.
    for raised in [GOAL, GOAL + 12_345] {
        let c = holding(raised);
        let (t, after) = plan::claim(Some(c), GOAL).unwrap();
        assert_eq!(after.value(), [raised as u32, (raised >> 32) as u32, R, 1, 0, 0, 0, 1]);
        assert_eq!(run(&t, &creator(CLAIM)).unwrap()[5], 1);
        assert_eq!(run(&t, &plan::creator_input(CLAIM, &WRONG)), Err(Refusal::Secret));
        for d in [1u64, u64::MAX] {
            let mut g = t.clone();
            g.pays[0].amount = g.pays[0].amount.wrapping_add(d);
            assert_eq!(run(&g, &creator(CLAIM)), Err(Refusal::Payout));
        }
        // Claiming but leaving the campaign open, or zeroing what was raised.
        let mut g = t.clone();
        g.writes[0].1 = c.value();
        assert_eq!(run(&g, &creator(CLAIM)), Err(Refusal::Value));
        let mut g = t.clone();
        g.writes[0].1 = Campaign { raised: 0, ..after }.value();
        assert_eq!(run(&g, &creator(CLAIM)), Err(Refusal::Value));
        // Two payouts, the second to someone else.
        let mut g = t.clone();
        g.pays = vec![Out::new(RAND, raised - 1), Out::new(RAND, 1)];
        assert_eq!(run(&g, &creator(CLAIM)), Err(Refusal::Shape));
    }
}

#[test]
fn a_claimed_campaign_is_final() {
    let c = holding(GOAL);
    let (_, done) = plan::claim(Some(c), GOAL).unwrap();
    assert!(plan::pledge(Some(done), 1).is_err());
    assert!(plan::refund(Some(done), 1).is_err());
    assert!(plan::claim(Some(done), GOAL).is_err());
    let read = |w: Campaign| Transition::new().read(CAMPAIGN_KEY, done.value()).write(CAMPAIGN_KEY, w.value());
    let cases = [
        (read(Campaign { raised: GOAL + 1, ..done }).rand_in(1).mint(R, 1), vec![PLEDGE]),
        (read(Campaign { raised: GOAL - 1, ..done }).burn(R, 1).pay(RAND, 1), vec![REFUND]),
        (read(done).pay(RAND, GOAL), creator(CLAIM)),
        (read(Campaign { raised: GOAL - 1, claimed: false, ..done }).burn(R, 1).pay(RAND, 1), vec![REFUND]),
    ];
    for (t, input) in cases {
        assert_eq!(run(&t, &input), Err(Refusal::Claimed));
    }
    let reinit = Transition::new().read(CAMPAIGN_KEY, done.value()).write(CAMPAIGN_KEY, opened().value());
    assert_eq!(run(&reinit, &init_in(R)), Err(Refusal::State));
}

/// Pledges and refunds in any order: receipts outstanding, `raised` and the vault's RAND stay
/// equal, and a claim empties the vault exactly.
#[test]
fn receipts_always_cover_the_vault() {
    let (mut vault, mut receipts) = (0u64, 0u64);
    let mut c = opened();
    let moves: [(bool, u64); 8] = [
        (true, 3_000_000_000),
        (true, 4_000_000_000),
        (false, 1_000_000_000),
        (true, 1),
        (false, 6_000_000_001),
        (true, 9_999_999_999),
        (false, 2),
        (true, 2),
    ];
    for (is_pledge, x) in moves {
        let (t, after) = if is_pledge { plan::pledge(Some(c), x) } else { plan::refund(Some(c), x) }.unwrap();
        assert!(run(&t, &[if is_pledge { PLEDGE } else { REFUND }]).is_ok());
        vault = vault + t.burn_r - t.pays.iter().map(|o| o.amount).sum::<u64>();
        receipts = receipts + t.mints.iter().map(|o| o.amount).sum::<u64>() - if t.burn_asset == R { t.burn_a } else { 0 };
        c = after;
        assert_eq!((vault, receipts), (c.raised, c.raised));
    }
    let need = plan::to_goal(&c, GOAL);
    assert_eq!(need, GOAL - c.raised);
    let (t, c) = plan::pledge(Some(c), need).unwrap();
    assert!(run(&t, &[PLEDGE]).is_ok());
    let (t, _) = plan::claim(Some(c), GOAL).unwrap();
    assert!(run(&t, &creator(CLAIM)).is_ok());
    assert_eq!(t.pays[0].amount, GOAL);
    assert_eq!(vault + need - t.pays[0].amount, 0);
}

#[test]
fn the_read_campaign_is_pinned() {
    let c = holding(5_000_000_000);
    let (t, _) = plan::pledge(Some(c), 1).unwrap();
    // A stray word in the read value, a claimed word of 2, another version, RAND as the receipt.
    for (i, v) in [(4usize, 1u32), (6, 1), (3, 2), (7, 2), (2, RAND)] {
        let mut g = t.clone();
        g.reads[0].1[i] = v;
        assert!(run(&g, &[PLEDGE]).is_err(), "read word {i} = {v} accepted");
    }
    // The campaign under another key.
    let mut g = t.clone();
    g.reads[0].0 = [2, 0, 0, 0, 0, 0, 0, 0];
    assert_eq!(run(&g, &[PLEDGE]), Err(Refusal::Key));
}

#[test]
fn no_word_is_left_unchecked() {
    let c = holding(7_000_000_000);
    let p = public();
    let cases = [
        (plan::init(R).unwrap().0, init_in(R)),
        (plan::pledge(Some(opened()), 3_000_000_000).unwrap().0, vec![PLEDGE]),
        (plan::pledge(Some(c), (1 << 40) + 7).unwrap().0, vec![PLEDGE]),
        (plan::refund(Some(c), 1_000_000_000).unwrap().0, vec![REFUND]),
        (plan::refund(Some(c), c.raised).unwrap().0, vec![REFUND]),
        (plan::claim(Some(holding(GOAL + (1 << 35))), GOAL).unwrap().0, creator(CLAIM)),
    ];
    for (t, input) in cases {
        let loose = loose_words(&p, &t, &input, test_hash, check);
        assert!(loose.is_empty(), "method {}: words accepted when changed: {loose:?}", input[0]);
    }
}
