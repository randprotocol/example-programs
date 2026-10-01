//! The ballot's rules, on the host: a tally accepted and published as expected, every way the
//! roll can be tampered with refused, the bounds, and the files the tool reads.
use ballot_core::host::{test_hash, Fn12, Mock, Roll};
use ballot_core::{Refusal, MAX_VOTERS};

const ROLL: &str = "# five members\noptions 3\n101 500\n102 300\n103 200\n104 150\n105 50\n";
const BALLOTS: &str = "101 0\n102 1\n103 0\n104 abstain\n# 105 cast no ballot\n";

fn roll() -> Roll {
    Roll::parse(ROLL).unwrap()
}

fn r_of(roll: &Roll) -> [u32; 8] {
    roll.digest(&Fn12(test_hash))
}

/// The call as the guest sees it: the honest roll's public words, these inputs after two blinds.
fn run(public: &[u32], input_after_blinds: &[u32]) -> Result<[u32; 8], Refusal> {
    let mut input = vec![0xdead_0001, 0xdead_0002];
    input.extend_from_slice(input_after_blinds);
    Mock::new(public, &input, test_hash).run()
}

fn totals(out: [u32; 8]) -> [u64; 4] {
    let w = |k: usize| u64::from(out[2 * k]) | (u64::from(out[2 * k + 1]) << 32);
    [w(0), w(1), w(2), w(3)]
}

#[test]
fn an_honest_tally_is_published() {
    let roll = roll();
    let public = roll.public_words(&r_of(&roll));
    let choices = roll.ballots(BALLOTS).unwrap();
    assert_eq!(choices, [0, 1, 0, 3, 3]);
    let out = run(&public, &roll.input_words(&choices)).unwrap();
    assert_eq!(totals(out), [700, 300, 0, 0]);
    assert_eq!(roll.expected(&choices), ([700, 300, 0, 0], 200));
    assert_eq!(roll.total() - 700 - 300, 200, "abstentions are the roll's total less the tallies");
}

#[test]
fn any_choice_outside_the_options_is_an_abstention() {
    let roll = roll();
    let public = roll.public_words(&r_of(&roll));
    let out = run(&public, &roll.input_words(&[0, 1, 7, 3, u32::MAX])).unwrap();
    assert_eq!(totals(out), [500, 300, 0, 0]);
    // Option 3 only counts when the ballot has four options.
    let four = Roll { n_options: 4, ..roll.clone() };
    let out = run(&four.public_words(&r_of(&four)), &four.input_words(&[3, 3, 3, 3, 3])).unwrap();
    assert_eq!(totals(out), [0, 0, 0, 1200]);
}

#[test]
fn the_blind_words_change_nothing() {
    let roll = roll();
    let public = roll.public_words(&r_of(&roll));
    let words = roll.input_words(&[0, 1, 0, 3, 3]);
    for blinds in [[0u32, 0], [u32::MAX, u32::MAX], [0x1234_5678, 0x9abc_def0]] {
        let mut input = blinds.to_vec();
        input.extend_from_slice(&words);
        assert_eq!(totals(Mock::new(&public, &input, test_hash).run().unwrap()), [700, 300, 0, 0]);
    }
}

#[test]
fn every_change_to_the_roll_is_refused() {
    let honest = roll();
    let public = honest.public_words(&r_of(&honest));
    let choices = [0, 1, 0, 3, 3];
    let tamper = |f: &dyn Fn(&mut Roll)| {
        let mut r = honest.clone();
        f(&mut r);
        let c: Vec<u32> = choices.iter().copied().take(r.voters.len()).chain(std::iter::repeat(0)).take(r.voters.len()).collect();
        run(&public, &r.input_words(&c))
    };
    assert_eq!(tamper(&|r| r.voters[1].1 += 1), Err(Refusal::Roll), "a weight changed");
    assert_eq!(tamper(&|r| r.voters[1].0 += 1), Err(Refusal::Roll), "a tag changed");
    assert_eq!(tamper(&|r| { r.voters.pop(); }), Err(Refusal::Roll), "the last voter dropped");
    assert_eq!(tamper(&|r| { r.voters.remove(2); }), Err(Refusal::Roll), "a middle voter dropped");
    assert_eq!(tamper(&|r| r.voters.push((106, 1))), Err(Refusal::Roll), "a voter added");
    assert_eq!(tamper(&|r| r.voters.push((106, 0))), Err(Refusal::Roll), "a zero-weight voter added");
    assert_eq!(tamper(&|r| r.voters.swap(0, 1)), Err(Refusal::Roll), "two voters swapped");
    // Moving weight between voters keeps the total and still fails.
    assert_eq!(tamper(&|r| { r.voters[0].1 -= 100; r.voters[1].1 += 100; }), Err(Refusal::Roll), "weight moved");
    // A different roll of the same length and total.
    assert_eq!(tamper(&|r| r.voters = vec![(1, 500), (2, 300), (3, 200), (4, 150), (5, 50)]), Err(Refusal::Roll));
}

#[test]
fn the_public_input_is_checked() {
    let roll = roll();
    let r = r_of(&roll);
    let words = roll.input_words(&[0, 1, 0, 3, 3]);
    for bad in [0, 1, 5, u32::MAX] {
        let public = Roll { n_options: bad, ..roll.clone() }.public_words(&r);
        assert_eq!(run(&public, &words), Err(Refusal::Options), "n_options {bad}");
    }
    // A roll digest of another roll (or a corrupted one).
    let mut public = roll.public_words(&r);
    public[5] ^= 1;
    assert_eq!(run(&public, &words), Err(Refusal::Roll));
    // A deploy whose n_options differs from the roll file's still accepts the ballots — the roll
    // digest does not cover n_options — but the tallier's tool would read choices against it.
    let public = Roll { n_options: 2, ..roll.clone() }.public_words(&r);
    assert_eq!(totals(run(&public, &words).unwrap()), [700, 300, 0, 0]);
}

#[test]
fn the_voter_count_is_capped_before_the_loop() {
    let roll = roll();
    let public = roll.public_words(&r_of(&roll));
    // Zero voters: refused outright.
    assert_eq!(run(&public, &[0]), Err(Refusal::Voters));
    // One over the cap, with words for every voter: refused before any is read.
    let mut big = vec![MAX_VOTERS + 1];
    for i in 0..=MAX_VOTERS {
        big.extend_from_slice(&[i, 1, 0, 0]);
    }
    assert_eq!(run(&public, &big), Err(Refusal::Voters));
    // A count larger than the words committed: unsatisfiable on the guest (oob here).
    let mut words = roll.input_words(&[0, 1, 0, 3, 3]);
    words[0] = 6;
    assert_eq!(run(&public, &words), Err(Refusal::Voters));
    // Exactly the cap is accepted.
    let voters: Vec<(u32, u64)> = (0..MAX_VOTERS).map(|i| (1000 + i, u64::from(i) + 1)).collect();
    let full = Roll { n_options: 2, voters };
    let choices: Vec<u32> = (0..MAX_VOTERS).map(|i| i % 2).collect();
    let out = run(&full.public_words(&r_of(&full)), &full.input_words(&choices)).unwrap();
    assert_eq!(totals(out), [64, 72, 0, 0]);
}

#[test]
fn weights_and_totals_stay_below_two_to_the_63() {
    let huge = Roll { n_options: 2, voters: vec![(1, 1 << 62), (2, 1 << 62)] };
    let public = huge.public_words(&r_of(&huge));
    assert_eq!(run(&public, &huge.input_words(&[0, 1])), Err(Refusal::Overflow), "the total reaches 2^63");
    let one = Roll { n_options: 2, voters: vec![(1, (1 << 63) - 1)] };
    let public = one.public_words(&r_of(&one));
    assert_eq!(totals(run(&public, &one.input_words(&[0])).unwrap()), [(1 << 63) - 1, 0, 0, 0]);
    // A weight word pair of 2^63 is refused as a weight before anything else; the host side
    // never writes one, so build the words by hand.
    assert_eq!(run(&public, &[1, 1, 0, 1 << 31, 0]), Err(Refusal::Weight));
}

#[test]
fn the_fold_is_seeded_with_the_length_and_chained() {
    let h = Fn12(test_hash);
    let roll = roll();
    let mut prefix = roll.clone();
    prefix.voters.pop();
    assert_ne!(roll.digest(&h), prefix.digest(&h));
    assert_eq!(roll.digest(&h), Roll::parse(ROLL).unwrap().digest(&h));
    assert_eq!(roll.fold_words(), [5, 101, 500, 0, 102, 300, 0, 103, 200, 0, 104, 150, 0, 105, 50, 0]);
    let wide = Roll { n_options: 2, voters: vec![(7, (3 << 32) | 9)] };
    assert_eq!(wide.fold_words(), [1, 7, 9, 3]);
}

#[test]
fn the_files_are_checked() {
    assert!(Roll::parse("101 5\n").unwrap_err().contains("no `options"));
    assert!(Roll::parse("options 1\n101 5\n").unwrap_err().contains("options must be"));
    assert!(Roll::parse("options 5\n101 5\n").unwrap_err().contains("options must be"));
    assert!(Roll::parse("options 2\n").unwrap_err().contains("no voters"));
    assert!(Roll::parse("options 2\n101 5\n101 6\n").unwrap_err().contains("already on the roll"));
    assert!(Roll::parse("options 2\n101 9223372036854775808\n").unwrap_err().contains("below 2^63"));
    assert!(Roll::parse("options 2\n1 4611686018427387904\n2 4611686018427387904\n").unwrap_err().contains("sum to 2^63"));
    let seventeen: String = "options 2\n".to_string() + &(0..17).map(|i| format!("{i} 1\n")).collect::<String>();
    assert!(Roll::parse(&seventeen).unwrap_err().contains("at most 16"));
    let roll = roll();
    assert!(roll.ballots("999 0\n").unwrap_err().contains("not on the roll"));
    assert!(roll.ballots("101 0\n101 1\n").unwrap_err().contains("already has a ballot"));
    assert!(roll.ballots("101 x\n").unwrap_err().contains("not a u32"));
    assert_eq!(roll.ballots("").unwrap(), [3, 3, 3, 3, 3], "no ballots: everyone abstains");
    assert_eq!(roll.ballots("103 7\n").unwrap(), [3, 3, 7, 3, 3]);
}
