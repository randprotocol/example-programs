//! private-join's rules on the host, with the stand-in hash: both modes accepted at their
//! expected values, every refusal reached, and the output independent of the blinds.
use private_join_core::host::{input_words, parse_list, public_words, test_hash, Mock, Party};
use private_join_core::{commitment, Refusal, Source, BLOCK, INPUT_WORDS, MAX, MODE_COUNT, MODE_MATCH, MSG};

const ID_A: u64 = 0xaaaa_0000_0000_0001;
const ID_B: u64 = 0xbbbb_0000_0000_0002;

fn block(id: u64, keys: &[u64], salt: u32) -> [u32; BLOCK] {
    Party { id, keys: keys.to_vec(), salt: [salt, 1, 2, 3, 4, 5, 6, 7] }.block().unwrap()
}

/// The stand-in commitment of a block, through the same `commitment` the commit guest uses (the
/// block as its only private inputs).
fn commit(b: &[u32; BLOCK]) -> [u32; 8] {
    struct H(Vec<u32>);
    impl Source for H {
        fn input(&self, i: u32) -> u32 { self.0[i as usize] }
        fn public(&self, _: u32) -> u32 { 0 }
        fn hash(&self, buf: &mut [u32; MSG]) {
            let d = test_hash(*buf);
            buf[..8].copy_from_slice(&d);
        }
    }
    commitment(&H(b.to_vec()), 0)
}

fn run(public: Vec<u32>, input: Vec<u32>) -> Result<[u32; 8], Option<Refusal>> {
    Mock::new(public, input, &test_hash).accepts()
}

/// Both lists committed honestly, run in `mode` with the given blinds.
fn honest(a: &[u32; BLOCK], b: &[u32; BLOCK], mode: u32, blinds: [u32; 2]) -> Result<[u32; 8], Option<Refusal>> {
    let input = input_words(blinds, a, b);
    assert_eq!(input.len(), INPUT_WORDS as usize);
    run(public_words(&commit(a), &commit(b), mode), input)
}

#[test]
fn counts_the_common_keys_and_nothing_else() {
    let a = block(ID_A, &[3, 10, 20, 30, 40], 1);
    let b = block(ID_B, &[1, 10, 30, 35, 40, 41], 2);
    assert_eq!(honest(&a, &b, MODE_COUNT, [7, 9]), Ok([3, 0, 0, 0, 0, 0, 0, 0]));
    // Disjoint, empty, and two full identical lists.
    assert_eq!(honest(&a, &block(ID_B, &[4, 11], 2), MODE_COUNT, [0, 0]).unwrap()[0], 0);
    assert_eq!(honest(&block(0, &[], 1), &b, MODE_COUNT, [0, 0]).unwrap()[0], 0);
    let full: Vec<u64> = (0..MAX as u64).map(|i| i * 1_000_000_007).collect();
    assert_eq!(honest(&block(1, &full, 1), &block(2, &full, 9), MODE_COUNT, [0, 0]).unwrap()[0], 16);
}

#[test]
fn match_needs_both_sides() {
    let a_lists_b = block(ID_A, &[5, ID_B], 1);
    let a_silent = block(ID_A, &[5, 6], 1);
    let b_lists_a = block(ID_B, &[ID_A, u64::MAX], 2);
    let b_silent = block(ID_B, &[7], 2);
    assert_eq!(honest(&a_lists_b, &b_lists_a, MODE_MATCH, [1, 2]), Ok([1, 1, 0, 0, 0, 0, 0, 0]));
    // A "no" looks the same whichever side declined.
    let no = Ok([0, 1, 0, 0, 0, 0, 0, 0]);
    assert_eq!(honest(&a_lists_b, &b_silent, MODE_MATCH, [1, 2]), no);
    assert_eq!(honest(&a_silent, &b_lists_a, MODE_MATCH, [1, 2]), no);
    assert_eq!(honest(&a_silent, &b_silent, MODE_MATCH, [1, 2]), no);
    // The ids are not records: in mode 0 the same two lists share nothing.
    assert_eq!(honest(&a_lists_b, &b_lists_a, MODE_COUNT, [1, 2]).unwrap()[0], 0);
}

#[test]
fn blinds_do_not_change_the_output() {
    let a = block(ID_A, &[1, 2, 3], 1);
    let b = block(ID_B, &[2, 3, 4], 2);
    let x = honest(&a, &b, MODE_COUNT, [0, 0]).unwrap();
    let y = honest(&a, &b, MODE_COUNT, [u32::MAX, 0x1234_5678]).unwrap();
    assert_eq!(x, y);
    assert_eq!(x[0], 2);
}

#[test]
fn unsorted_or_repeated_keys_are_refused() {
    let b = block(ID_B, &[1, 2], 2);
    assert_eq!(honest(&block(ID_A, &[2, 1], 1), &b, MODE_COUNT, [0, 0]), Err(Some(Refusal::Unsorted)));
    assert_eq!(honest(&block(ID_A, &[1, 1], 1), &b, MODE_COUNT, [0, 0]), Err(Some(Refusal::Unsorted)));
    // Either list, either mode.
    assert_eq!(honest(&b, &block(ID_A, &[9, 3], 1), MODE_MATCH, [0, 0]), Err(Some(Refusal::Unsorted)));
    // Keys past n are not read, so junk there is harmless.
    let mut junk = block(ID_A, &[5], 1);
    junk[5] = 1;
    junk[6] = 0;
    assert_eq!(honest(&junk, &b, MODE_COUNT, [0, 0]).unwrap()[0], 0);
}

#[test]
fn more_than_max_keys_is_refused() {
    let mut a = block(ID_A, &[1, 2, 3], 1);
    a[0] = MAX as u32 + 1;
    assert_eq!(honest(&a, &block(ID_B, &[1], 2), MODE_COUNT, [0, 0]), Err(Some(Refusal::TooMany)));
    a[0] = u32::MAX;
    assert_eq!(honest(&a, &block(ID_B, &[1], 2), MODE_COUNT, [0, 0]), Err(Some(Refusal::TooMany)));
    // The host tool will not even build such a block.
    assert!(Party { id: 0, keys: vec![0; MAX + 1], salt: [0; 8] }.block().is_err());
}

#[test]
fn a_list_must_be_the_one_committed() {
    let a = block(ID_A, &[1, 2, 3], 1);
    let b = block(ID_B, &[2, 3, 4], 2);
    let public = public_words(&commit(&a), &commit(&b), MODE_COUNT);
    // A record dropped after committing; a key changed; a wrong salt; the two blocks swapped.
    for bad in [block(ID_A, &[1, 2], 1), block(ID_A, &[1, 2, 4], 1), block(ID_A, &[1, 2, 3], 99)] {
        assert_eq!(run(public.clone(), input_words([0, 0], &bad, &b)), Err(Some(Refusal::Commitment)));
    }
    assert_eq!(run(public.clone(), input_words([0, 0], &b, &a)), Err(Some(Refusal::Commitment)));
    assert_eq!(run(public, input_words([0, 0], &a, &b)).unwrap()[0], 2);
}

#[test]
fn an_unknown_mode_is_refused() {
    let a = block(ID_A, &[1], 1);
    let b = block(ID_B, &[1], 2);
    for mode in [2, 3, u32::MAX] {
        assert_eq!(honest(&a, &b, mode, [0, 0]), Err(Some(Refusal::Mode)));
    }
}

#[test]
fn short_inputs_have_no_proof() {
    let a = block(ID_A, &[1], 1);
    let b = block(ID_B, &[1], 2);
    let mut input = input_words([0, 0], &a, &b);
    input.pop();
    assert_eq!(run(public_words(&commit(&a), &commit(&b), MODE_COUNT), input), Err(None));
}

#[test]
fn list_files_parse() {
    let text = "# comment\nself 0x52135cb13ccc38b7\n0x098a4df022e3f570  7 # two on a line\n42\n";
    let (id, keys) = parse_list(text).unwrap();
    assert_eq!(id, 0x5213_5cb1_3ccc_38b7);
    assert_eq!(keys, vec![0x098a_4df0_22e3_f570, 7, 42]);
    assert_eq!(parse_list("1 2 3").unwrap(), (0, vec![1, 2, 3]));
    assert!(parse_list("self 1\nself 2").is_err());
    assert!(parse_list("0x1ffffffffffffffff").is_err());
    assert!(parse_list("abc").is_err());
    let p = Party { id, keys, salt: [8, 7, 6, 5, 4, 3, 2, 1] };
    let b = p.block().unwrap();
    assert_eq!(b[0], 3);
    assert_eq!((b[1], b[2]), (0x3ccc_38b7, 0x5213_5cb1));
    assert_eq!((b[3], b[4]), (0x22e3_f570, 0x098a_4df0));
    assert_eq!(&b[BLOCK - 8..], &[8, 7, 6, 5, 4, 3, 2, 1]);
}
