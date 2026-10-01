//! The sealed-auction's rules, on the host: clearing in both modes, ties, every refusal, and the
//! receipt's binding to exactly the bids that were cleared.
use sealed_auction_core::host::{call_words, parse_bids, parse_hex8, hex8, Mock};
use sealed_auction_core::{check, clear, commitment, fold_all, Bid, Refusal, MODE_HIGHEST, MODE_LOWEST};

fn bid(tag: u32, amount: u64) -> Bid {
    Bid { tag, amount, salt: [tag.wrapping_mul(0x9e37_79b9), 0x5a5a_0000 | tag] }
}

fn four() -> Vec<Bid> {
    vec![bid(7, 500), bid(8, 900), bid(9, 700), bid(10, 100)]
}

fn run(mode: u32, bids: &[Bid]) -> Option<[u32; 8]> {
    Mock::call(mode, bids).accepts(check)
}

#[test]
fn highest_wins_and_pays_the_second_highest() {
    let out = run(MODE_HIGHEST, &four()).unwrap();
    assert_eq!(&out[..3], &[8, 700, 0]);
}

#[test]
fn lowest_wins_and_is_paid_the_second_lowest() {
    let out = run(MODE_LOWEST, &four()).unwrap();
    assert_eq!(&out[..3], &[10, 500, 0]);
}

#[test]
fn a_tie_goes_to_the_first_at_its_own_bid() {
    assert_eq!(clear(MODE_HIGHEST, &[bid(7, 900), bid(8, 900), bid(9, 100)]), Ok((7, 900)));
    assert_eq!(clear(MODE_HIGHEST, &[bid(7, 100), bid(8, 900), bid(9, 900)]), Ok((8, 900)));
    assert_eq!(clear(MODE_LOWEST, &[bid(7, 100), bid(8, 100), bid(9, 900)]), Ok((7, 100)));
    // All equal: the first wins and pays the common bid.
    assert_eq!(clear(MODE_LOWEST, &[bid(7, 5), bid(8, 5)]), Ok((7, 5)));
}

#[test]
fn amounts_use_both_words() {
    let big = (1u64 << 63) - 1;
    let out = run(MODE_HIGHEST, &[bid(1, big), bid(2, big - 1)]).unwrap();
    assert_eq!(&out[..3], &[1, (big - 1) as u32, ((big - 1) >> 32) as u32]);
    let out = run(MODE_LOWEST, &[bid(1, big), bid(2, 1 << 40)]).unwrap();
    assert_eq!(&out[..3], &[2, big as u32, (big >> 32) as u32]);
}

#[test]
fn refusals() {
    assert_eq!(clear(MODE_HIGHEST, &[bid(7, 500)]), Err(Refusal::Count));
    assert_eq!(run(MODE_HIGHEST, &[bid(7, 500)]), None);
    let nine: Vec<Bid> = (1..=9).map(|t| bid(t, 100 * u64::from(t))).collect();
    assert_eq!(clear(MODE_HIGHEST, &nine), Err(Refusal::Count));
    assert_eq!(run(MODE_HIGHEST, &nine), None);
    let eight: Vec<Bid> = (1..=8).map(|t| bid(t, 100 * u64::from(t))).collect();
    assert_eq!(&run(MODE_HIGHEST, &eight).unwrap()[..3], &[8, 700, 0]);

    assert_eq!(clear(MODE_HIGHEST, &[bid(0, 500), bid(8, 900)]), Err(Refusal::Tag));
    assert_eq!(clear(MODE_HIGHEST, &[bid(7, 500), bid(7, 900)]), Err(Refusal::Duplicate));
    assert_eq!(clear(MODE_HIGHEST, &[bid(7, 500), bid(8, 900), bid(7, 1)]), Err(Refusal::Duplicate));
    assert_eq!(clear(MODE_HIGHEST, &[bid(7, 1 << 63), bid(8, 900)]), Err(Refusal::Range));
    assert_eq!(clear(MODE_HIGHEST, &[bid(7, 1), bid(8, u64::MAX)]), Err(Refusal::Range));
    assert_eq!(clear(2, &four()), Err(Refusal::Mode));
    assert_eq!(run(2, &four()), None);
    for bad in [&[bid(0, 500), bid(8, 900)][..], &[bid(7, 500), bid(7, 900)], &[bid(7, 1 << 63), bid(8, 900)]] {
        assert_eq!(run(MODE_HIGHEST, bad), None);
    }
}

#[test]
fn a_count_past_the_committed_words_has_no_proof() {
    // n says three bids but only two were committed: the guest's read is unsatisfiable.
    let mut input = vec![1, 2];
    input.extend(call_words(&four()[..2]));
    input[2] = 3;
    let m = Mock::new(&[MODE_HIGHEST], &input);
    assert!(m.accepts(check).is_none());
    assert!(m.oob.get());
    // A call with no public word at all is unsatisfiable too.
    assert!(Mock::new(&[], &input).accepts(check).is_none());
}

#[test]
fn the_receipt_binds_exactly_the_bids_cleared() {
    let bids = four();
    let m = Mock::call(MODE_HIGHEST, &bids);
    let out = m.accepts(check).unwrap();
    let cs: Vec<[u32; 8]> = bids.iter().map(|b| commitment(&m, b)).collect();
    assert_eq!(&out[3..], &fold_all(&m, &cs)[..5]);
    // The binding does not depend on the mode…
    assert_eq!(&run(MODE_LOWEST, &bids).unwrap()[3..], &out[3..]);
    // …and changes with any bid's salt, amount or tag, and with the order.
    let mut salted = bids.clone();
    salted[2].salt[1] ^= 1;
    assert_ne!(&run(MODE_HIGHEST, &salted).unwrap()[3..], &out[3..]);
    let mut shaded = bids.clone();
    shaded[3].amount += 1;
    assert_ne!(&run(MODE_HIGHEST, &shaded).unwrap()[3..], &out[3..]);
    let mut renamed = bids.clone();
    renamed[0].tag = 70;
    assert_ne!(&run(MODE_HIGHEST, &renamed).unwrap()[3..], &out[3..]);
    let mut swapped = bids.clone();
    swapped.swap(0, 1);
    assert_ne!(&run(MODE_HIGHEST, &swapped).unwrap()[3..], &out[3..]);
    // A dropped last bid is a different fold, not a prefix that still matches.
    let m3 = Mock::call(MODE_HIGHEST, &bids[..3]);
    assert_ne!(&m3.accepts(check).unwrap()[3..], &out[3..]);
}

#[test]
fn the_bids_file_and_the_words() {
    let text = "# tag bid salt\n7 500 1234567890123\n8 900 1\n\n9 700 18446744073709551615 # max salt\n";
    let bids = parse_bids(text).unwrap();
    assert_eq!(bids.len(), 3);
    assert_eq!(bids[0], Bid { tag: 7, amount: 500, salt: [1234567890123u64 as u32, (1234567890123u64 >> 32) as u32] });
    assert_eq!(bids[2].salt, [u32::MAX, u32::MAX]);
    let w = call_words(&bids);
    assert_eq!(w.len(), 1 + 3 * 5);
    assert_eq!(&w[..6], &[3, 7, 500, 0, bids[0].salt[0], bids[0].salt[1]]);
    assert!(parse_bids("7 500\n").is_err());
    assert!(parse_bids("7 500 x\n").is_err());
    assert!(parse_bids("7 18446744073709551616 1\n").is_err());
    let c = [1, 0x0403_0201, 0, 0, 0, 0, 0, 0xffff_ffff];
    assert_eq!(parse_hex8(&hex8(&c)).unwrap(), c);
    assert!(hex8(&c).starts_with("0100000001020304"));
}
