//! auction: the auctioneer's and the bidders' off-chain tool. It turns a bids file into the
//! call's private input words, predicts the receipt with the program's own rules, and computes
//! every commitment and the fold with the program's own hash (the `commit/` guest on the
//! emulator).
//!
//!   auction words  <bids file>               the private input words after the two blind words
//!   auction show   <bids file> [--mode 0|1]  the commitment list to publish and the receipt to expect
//!   auction commit <tag> <bid> <salt>        one bid's commitment (a bidder checks the published list)
//!   auction fold   <commitments file>        the fold of a published list (anyone checks the receipt)
//!
//! A bids file has one bid per line: `tag bid salt`, decimal, the salt a u64 the bidder chose.
use sealed_auction_core::host::{call_words, fail, hex8, join, parse_bids, parse_commitments, Emu};
use sealed_auction_core::{clear, Bid, Refusal, MODE_HIGHEST, MODE_LOWEST};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let arg = |i: usize| args.get(i).map(|s| s.as_str());
    match (arg(0), arg(1)) {
        (Some("words"), Some(path)) => {
            let bids = bids_of(path);
            // Refuse here what the program would refuse, with the reason the program never gives.
            if let Err(e) = clear(MODE_HIGHEST, &bids) {
                fail(&format!("{path}: the program would refuse these bids: {}", why(e)));
            }
            println!("{}", join(&call_words(&bids)));
        }
        (Some("show"), Some(path)) => {
            let mode = match (arg(2), arg(3)) {
                (Some("--mode"), Some(m)) => m.parse::<u32>().unwrap_or_else(|_| fail("--mode: 0 or 1")),
                (None, _) => MODE_HIGHEST,
                _ => fail("usage: auction show <bids file> [--mode 0|1]"),
            };
            show(path, mode);
        }
        (Some("commit"), Some(tag)) => {
            let line = format!("{tag} {} {}", arg(2).unwrap_or(""), arg(3).unwrap_or(""));
            let bids = parse_bids(&line).unwrap_or_else(|e| fail(&e));
            let emu = Emu::new().unwrap_or_else(|e| fail(&e));
            let c = emu.commitment(&bids[0]).unwrap_or_else(|e| fail(&e));
            println!("{}", hex8(&c));
        }
        (Some("fold"), Some(path)) => {
            let text = std::fs::read_to_string(path).unwrap_or_else(|e| fail(&format!("{path}: {e}")));
            let cs = parse_commitments(&text).unwrap_or_else(|e| fail(&format!("{path}: {e}")));
            let emu = Emu::new().unwrap_or_else(|e| fail(&e));
            let f = emu.fold(&cs).unwrap_or_else(|e| fail(&e));
            println!("fold of {} commitments: {}", cs.len(), hex8(&f));
            println!("receipt out[3..8] must be: {}", join(&f[..5]));
        }
        _ => fail("usage: auction words|show|commit|fold … (see the top of core/src/bin/auction.rs)"),
    }
}

fn bids_of(path: &str) -> Vec<Bid> {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| fail(&format!("{path}: {e}")));
    parse_bids(&text).unwrap_or_else(|e| fail(&format!("{path}: {e}")))
}

fn why(e: Refusal) -> &'static str {
    match e {
        Refusal::Mode => "the mode is not 0 or 1",
        Refusal::Count => "fewer than two or more than eight bids",
        Refusal::Tag => "a bidder tag is zero",
        Refusal::Duplicate => "two bids share a tag",
        Refusal::Range => "a bid is 2^63 or more",
    }
}

fn show(path: &str, mode: u32) {
    let bids = bids_of(path);
    let what = match mode {
        MODE_HIGHEST => "highest wins, pays the second-highest (auction)",
        MODE_LOWEST => "lowest wins, paid the second-lowest (request for quote)",
        _ => fail("--mode: 0 or 1"),
    };
    println!("mode {mode}: {what}");
    let (winner, price) = match clear(mode, &bids) {
        Ok(r) => r,
        Err(e) => fail(&format!("{path}: the program would refuse these bids: {}", why(e))),
    };
    let emu = Emu::new().unwrap_or_else(|e| fail(&e));
    println!("{} bids; commitments, in order (publish this list):", bids.len());
    let mut cs = vec![];
    for (i, b) in bids.iter().enumerate() {
        let c = emu.commitment(b).unwrap_or_else(|e| fail(&e));
        println!("  {}  {}", i + 1, hex8(&c));
        cs.push(c);
    }
    let f = emu.fold(&cs).unwrap_or_else(|e| fail(&e));
    println!("expected receipt: winner tag {winner}, price {price}");
    println!("  out[0..3] = {winner} {} {}", price as u32, (price >> 32) as u32);
    println!("  out[3..8] = {}", join(&f[..5]));
}
