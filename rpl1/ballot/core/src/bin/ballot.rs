//! ballot: the tallier's and the organiser's tool. Turns the human files into the program's words.
//!
//!   ballot roll <roll file>                      the deploy's public input: `n_options R0 … R7`
//!   ballot inputs <roll file> <ballots file>     the call's words and what the receipt will say:
//!                                                  public: n_options R0 … R7
//!                                                  input:  n (voter_tag w_lo w_hi choice) × n     (after the two blind words)
//!                                                  tally:  t0 t1 t2 t3
//!                                                  abstain: a
//!
//! `R` is computed by the `fold` helper guest on the zkVM's emulator — the program's own hash,
//! not a host reimplementation — through `CIRCUITS` and `FOLD_IMAGE`, which the scripts set.
//! The tally is checked by running the program's rules (`ballot_core::check`) on the host.
use ballot_core::host::{fail, join, test_hash, Fn12, Mock, Roll};

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let read = |p: &str| std::fs::read_to_string(p).unwrap_or_else(|e| fail(&format!("{p}: {e}")));
    match a.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["roll", roll] => {
            let roll = Roll::parse(&read(roll)).unwrap_or_else(|e| fail(&e));
            let r = roll.real_digest().unwrap_or_else(|e| fail(&e));
            println!("{}", join(&roll.public_words(&r)));
        }
        ["inputs", roll, ballots] => {
            let roll = Roll::parse(&read(roll)).unwrap_or_else(|e| fail(&e));
            let choices = roll.ballots(&read(ballots)).unwrap_or_else(|e| fail(&e));
            let input = roll.input_words(&choices);
            // The rules, on the host, under the stand-in hash and the matching R: what the guest
            // will publish if the roll is the deployed one.
            let r_test = roll.digest(&Fn12(test_hash));
            let mut words = vec![0, 0];
            words.extend_from_slice(&input);
            let out = Mock::new(&roll.public_words(&r_test), &words, test_hash)
                .run()
                .unwrap_or_else(|e| fail(&format!("the program would refuse these inputs: {e:?}")));
            let (t, abstain) = roll.expected(&choices);
            for (k, v) in t.iter().enumerate() {
                let got = u64::from(out[2 * k]) | (u64::from(out[2 * k + 1]) << 32);
                if got != *v {
                    fail(&format!("the rules and the expectation disagree on option {k}: {got} vs {v}"));
                }
            }
            let r = roll.real_digest().unwrap_or_else(|e| fail(&e));
            println!("public: {}", join(&roll.public_words(&r)));
            println!("input: {}", join(&input));
            println!("tally: {} {} {} {}", t[0], t[1], t[2], t[3]);
            println!("abstain: {abstain}");
        }
        _ => fail("usage: ballot roll <roll file> | ballot inputs <roll file> <ballots file>"),
    }
}
