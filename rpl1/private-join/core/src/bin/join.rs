//! join: the off-chain side of private-join. Turns a party's list file and salt into its
//! commitment and its call words, by the program's own code (the commitment by running
//! `commit/image.bin` on the emulator; the expected result by the same rules the guest runs).
//!
//!   join commit <list> <salt>                      the list's eight commitment words
//!   join inputs <A list> <A salt> <B list> <B salt>
//!        input: <86 words>        the call's private inputs after the two blind words
//!        count: <n>               what mode 0 outputs, or   refused: <why>
//!        match: <0|1>             what mode 1 outputs, or   refused: <why>
//!   join public <C_A: 8 words> <C_B: 8 words> <mode>     the 17 public words (join.txt)
//!
//! A list file holds keys (decimal or 0x hex, up to 16) in the order the program will see them,
//! `#` comments, and an optional `self <key>` line, the party's own id. A salt file holds eight
//! decimal words (commit.sh makes one from /dev/urandom).
use private_join_core::host::{fail, join, public_words, Emulator, Mock, Party};
use private_join_core::{MODE_COUNT, MODE_MATCH, PUBLIC_WORDS};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let a: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    match a.as_slice() {
        ["commit", list, salt] => commit(list, salt),
        ["inputs", la, sa, lb, sb] => inputs(la, sa, lb, sb),
        ["public", rest @ ..] => public(rest),
        _ => fail("usage: join commit <list> <salt> | inputs <A> <A salt> <B> <B salt> | public <C_A…> <C_B…> <mode>"),
    }
}

fn party(list: &str, salt: &str) -> Party {
    Party::from_files(list, salt).unwrap_or_else(|e| fail(&e))
}

fn commit(list: &str, salt: &str) {
    let em = Emulator::new().unwrap_or_else(|e| fail(&e));
    let block = party(list, salt).block().unwrap_or_else(|e| fail(&format!("{list}: {e}")));
    println!("{}", join(&em.commitment(&block)));
}

fn inputs(la: &str, sa: &str, lb: &str, sb: &str) {
    let em = Emulator::new().unwrap_or_else(|e| fail(&e));
    let ba = party(la, sa).block().unwrap_or_else(|e| fail(&format!("{la}: {e}")));
    let bb = party(lb, sb).block().unwrap_or_else(|e| fail(&format!("{lb}: {e}")));
    let (c_a, c_b) = (em.commitment(&ba), em.commitment(&bb));
    let mut words = ba.to_vec();
    words.extend_from_slice(&bb);
    println!("input: {}", join(&words));
    let hasher = |m| em.hash(m);
    for (mode, name) in [(MODE_COUNT, "count"), (MODE_MATCH, "match")] {
        // Any blinds: the result does not depend on them.
        let input = private_join_core::host::input_words([0, 0], &ba, &bb);
        match Mock::new(public_words(&c_a, &c_b, mode), input, &hasher).accepts() {
            Ok(out) => println!("{name}: {}", out[0]),
            Err(Some(r)) => println!("{name}: refused: {r:?}"),
            Err(None) => println!("{name}: refused: read past the inputs"),
        }
    }
}

fn public(rest: &[&str]) {
    let words: Vec<u32> = rest
        .iter()
        .flat_map(|s| s.split_whitespace())
        .map(|t| t.parse::<u32>().unwrap_or_else(|_| fail(&format!("not a word: {t}"))))
        .collect();
    if words.len() != PUBLIC_WORDS as usize {
        fail(&format!("{} words; the public input is C_A (8), C_B (8) and the mode", words.len()));
    }
    let mode = words[16];
    if mode != MODE_COUNT && mode != MODE_MATCH {
        fail(&format!("mode {mode}: 0 (intersection size) or 1 (match)"));
    }
    println!("{}", join(&words));
}
