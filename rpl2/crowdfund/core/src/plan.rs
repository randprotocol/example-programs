//! The wallet's side (host only): build the transitions `check` accepts, and the private inputs
//! that go with them.

use crate::*;
use kit::host::{max_satisfying, Transition};

/// The ten public words: the creator's lock, then the goal.
pub fn public(lock: &[u32; 8], goal: u64) -> Vec<u32> {
    let g = words_of(goal);
    let mut p = lock.to_vec();
    p.extend_from_slice(&[g.0, g.1]);
    p
}

/// A creator's private inputs: the method, then the secret.
pub fn creator_input(method: u32, secret: &[u32; 8]) -> Vec<u32> {
    let mut i = vec![method];
    i.extend_from_slice(secret);
    i
}

/// init's private inputs: the method, the secret, the receipt token.
pub fn init_input(secret: &[u32; 8], receipt: u32) -> Vec<u32> {
    let mut i = creator_input(INIT, secret);
    i.push(receipt);
    i
}

/// Open the campaign with `receipt` (the token registered with `--program <id>`).
pub fn init(receipt: u32) -> Result<(Transition, Campaign), String> {
    if receipt == RAND {
        return Err("the receipt token cannot be RAND".into());
    }
    let after = Campaign { raised: 0, receipt, claimed: false };
    Ok((Transition::new().read(CAMPAIGN_KEY, [0; 8]).write(CAMPAIGN_KEY, after.value()), after))
}

fn open(c: Option<Campaign>) -> Result<Campaign, String> {
    match c {
        None => Err("the campaign does not exist yet: the creator runs ./init.sh first".into()),
        Some(c) if c.claimed => Err("the campaign has been claimed: it is final".into()),
        Some(c) => Ok(c),
    }
}

/// Pledge `a` RAND units, for `a` receipts.
pub fn pledge(c: Option<Campaign>, a: u64) -> Result<(Transition, Campaign), String> {
    let c = open(c)?;
    if a == 0 {
        return Err("pledge more than nothing".into());
    }
    let raised = add_note(c.raised, a).ok_or("the campaign cannot hold that much")?;
    let after = Campaign { raised, ..c };
    let t = Transition::new()
        .read(CAMPAIGN_KEY, c.value())
        .write(CAMPAIGN_KEY, after.value())
        .rand_in(a)
        .mint(c.receipt, a);
    Ok((t, after))
}

/// The smallest pledge that meets the goal (0 if it is met already) — searched over the very
/// predicate `claim` checks: one more than the largest pledge that still falls short.
pub fn to_goal(c: &Campaign, goal: u64) -> u64 {
    if goal_met(c.raised, goal) {
        return 0;
    }
    max_satisfying(goal, |a| !goal_met(c.raised.saturating_add(a), goal)) + 1
}

/// The most receipts a refund can burn now, given `held` receipts — searched over `refund_ok`.
pub fn refundable(c: &Campaign, held: u64) -> u64 {
    max_satisfying(held, |b| (b == 0) | refund_ok(c.raised, b))
}

/// Burn `b` receipts for `b` RAND units.
pub fn refund(c: Option<Campaign>, b: u64) -> Result<(Transition, Campaign), String> {
    let c = open(c)?;
    if !refund_ok(c.raised, b) {
        return Err(format!("refund between 1 and {} receipts", c.raised));
    }
    let after = Campaign { raised: c.raised - b, ..c };
    let t = Transition::new()
        .read(CAMPAIGN_KEY, c.value())
        .write(CAMPAIGN_KEY, after.value())
        .burn(c.receipt, b)
        .pay(RAND, b);
    Ok((t, after))
}

/// Claim everything raised, once it meets `goal`.
pub fn claim(c: Option<Campaign>, goal: u64) -> Result<(Transition, Campaign), String> {
    let c = open(c)?;
    if !goal_met(c.raised, goal) {
        return Err(format!("{} raised of a {} goal: {} more to go", c.raised, goal, to_goal(&c, goal)));
    }
    let after = Campaign { claimed: true, ..c };
    let t = Transition::new()
        .read(CAMPAIGN_KEY, c.value())
        .write(CAMPAIGN_KEY, after.value())
        .pay(RAND, c.raised);
    Ok((t, after))
}
