//! The rules on the host, with the stand-in hash: a credential in the tree that meets the cutoff
//! is accepted and every other call is refused — the cutoff missed, a wrong sibling, a wrong
//! direction, a non-boolean direction, a credential not in the tree, a tampered year, a path
//! against another issuer's root, a short input, and an empty slot.
use eligibility_core::host::{parse_id, parse_roll, witness, Mock, TestHash, Tree};
use eligibility_core::{check, Credential, Refusal, EMPTY, INPUT_WORDS, LEAVES};

fn roll() -> Vec<Credential> {
    (0..5)
        .map(|i| Credential { id: [i, 10 + i, 20 + i, 30 + i], birth_year: 1990 + 5 * i, nonce: 0x9e37_79b9 ^ i })
        .collect()
}

/// Public words for `root` and `cutoff`, inputs = two blinds then the witness.
fn call(root: [u32; 8], cutoff: u32, w: &[u32]) -> (Result<[u32; 8], Refusal>, bool) {
    let mut public = root.to_vec();
    public.push(cutoff);
    let mut input = vec![0xb11d_0001, 0xb11d_0002];
    input.extend_from_slice(w);
    let m = Mock::new(&public, &input, TestHash);
    let r = check(&m);
    (r, m.oob.get())
}

fn accepted(root: [u32; 8], cutoff: u32, w: &[u32]) -> [u32; 8] {
    let (r, oob) = call(root, cutoff, w);
    assert!(!oob, "read past the inputs");
    r.expect("accepted")
}

fn refused(root: [u32; 8], cutoff: u32, w: &[u32]) -> Refusal {
    let (r, oob) = call(root, cutoff, w);
    assert!(!oob, "read past the inputs");
    r.expect_err("refused")
}

#[test]
fn every_credential_that_meets_the_cutoff_is_accepted_with_the_receipt_words() {
    let roll = roll();
    let tree = Tree::build(&TestHash, &roll).unwrap();
    let root = tree.root();
    for (i, c) in roll.iter().enumerate() {
        let w = witness(c, &tree, i);
        assert_eq!(w.len() as u32 + 2, INPUT_WORDS);
        let out = accepted(root, c.birth_year, &w);
        assert_eq!(out, [1, c.birth_year, root[0], root[1], root[2], root[3], root[4], root[5]]);
        assert_eq!(accepted(root, 3000, &w)[1], 3000);
    }
}

#[test]
fn the_cutoff_missed_by_one_year_is_refused() {
    let roll = roll();
    let tree = Tree::build(&TestHash, &roll).unwrap();
    let w = witness(&roll[2], &tree, 2);
    assert_eq!(roll[2].birth_year, 2000);
    assert!(accepted(tree.root(), 2000, &w)[0] == 1);
    assert_eq!(refused(tree.root(), 1999, &w), Refusal::Predicate);
}

#[test]
fn a_wrong_sibling_or_direction_is_refused_at_every_level() {
    let roll = roll();
    let tree = Tree::build(&TestHash, &roll).unwrap();
    let w = witness(&roll[3], &tree, 3);
    for level in 0..8 {
        let at = 6 + 9 * level;
        let mut bad = w.clone();
        bad[at + 5] ^= 1; // one sibling word
        assert_eq!(refused(tree.root(), 2100, &bad), Refusal::Root, "level {level} sibling");
        let mut bad = w.clone();
        bad[at + 8] ^= 1; // the other side
        assert_eq!(refused(tree.root(), 2100, &bad), Refusal::Root, "level {level} direction");
        let mut bad = w.clone();
        bad[at + 8] = 2; // not a bit
        assert_eq!(refused(tree.root(), 2100, &bad), Refusal::Direction, "level {level} non-boolean");
    }
}

#[test]
fn a_credential_outside_the_tree_or_a_tampered_year_is_refused() {
    let roll = roll();
    let tree = Tree::build(&TestHash, &roll).unwrap();
    // Not in the roll: the leaf is wrong, so the root is.
    let stranger = Credential { id: [7, 7, 7, 7], birth_year: 1950, nonce: 1 };
    let w = witness(&stranger, &tree, 0);
    assert_eq!(refused(tree.root(), 2100, &w), Refusal::Root);
    // In the roll, but claiming an earlier year than the issuer wrote down.
    let mut w = witness(&roll[4], &tree, 4);
    w[4] -= 1;
    assert_eq!(refused(tree.root(), 2100, &w), Refusal::Root);
    // The right path, but for another issuer's root.
    let other = Tree::build(&TestHash, &roll[..3]).unwrap();
    assert_ne!(other.root(), tree.root());
    assert_eq!(refused(other.root(), 2100, &witness(&roll[1], &tree, 1)), Refusal::Root);
    // The right credential, the wrong slot.
    assert_eq!(refused(tree.root(), 2100, &witness(&roll[1], &tree, 2)), Refusal::Root);
}

#[test]
fn an_empty_slot_opens_for_nobody_and_a_short_input_has_no_proof() {
    let roll = roll();
    let tree = Tree::build(&TestHash, &roll).unwrap();
    // Slot 200 is EMPTY (eight zeros): no credential hashes to it.
    assert_eq!(tree.levels[0][200], EMPTY);
    let zero = Credential { id: [0; 4], birth_year: 0, nonce: 0 };
    assert_eq!(refused(tree.root(), 2100, &witness(&zero, &tree, 200)), Refusal::Root);
    // One word short: the guest reads past the committed inputs, which cannot be proved.
    let w = witness(&roll[0], &tree, 0);
    let (_, oob) = call(tree.root(), 2100, &w[..w.len() - 1]);
    assert!(oob);
    // The tree is full-size whatever the roll: 256 leaves, one root.
    assert_eq!(tree.levels[0].len(), LEAVES);
    assert_eq!(tree.levels[8].len(), 1);
    assert!(Tree::build(&TestHash, &vec![zero; LEAVES + 1]).is_err());
}

#[test]
fn the_roll_parses_as_the_scripts_write_it() {
    let text = "# id  birth_year  nonce\n0102030405060708090a0b0c0d0e0f10 1990 7\n\n0x00000000000000000000000000000001 2008 9 # trailing\n";
    let roll = parse_roll(text).unwrap();
    assert_eq!(roll.len(), 2);
    assert_eq!(roll[0].id, [0x0403_0201, 0x0807_0605, 0x0c0b_0a09, 0x100f_0e0d]);
    assert_eq!((roll[0].birth_year, roll[0].nonce), (1990, 7));
    assert_eq!(roll[1].id, [0, 0, 0, 0x0100_0000]);
    assert!(parse_id("0102").is_err());
    assert!(parse_roll("0102030405060708090a0b0c0d0e0f10 1990").is_err());
    assert!(parse_roll("zz02030405060708090a0b0c0d0e0f10 1990 1").is_err());
}
