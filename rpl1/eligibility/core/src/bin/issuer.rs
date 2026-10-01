//! issuer: the issuer's side of eligibility, off chain. Every hash is computed by running
//! `hash/image.bin` on the zkVM's emulator, so the root is exactly what the deployed program
//! recomputes.
//!
//!   issuer root <credentials file> [cutoff_year]   the tree's root (eight words), then the
//!                                                  cutoff if given — issuer.txt's words
//!   issuer path <credentials file> <index>         the holder of slot <index>'s private input
//!                                                  words, minus the two blinds (the scripts
//!                                                  prepend them): 6 + 9·DEPTH words
//!
//! The credentials file has one credential per line, `<32 hex id> <birth_year> <nonce>`, slot
//! order; `#` starts a comment. It is the issuer's roll and stays with the issuer; a holder gets
//! only their own `path` line.
use eligibility_core::host::{fail, join, parse_roll, witness, Emulator, Tree};
use eligibility_core::LEAVES;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let roll_of = |p: &str| {
        let text = std::fs::read_to_string(p).unwrap_or_else(|e| fail(&format!("{p}: {e}")));
        parse_roll(&text).unwrap_or_else(|e| fail(&format!("{p}: {e}")))
    };
    let emu = || Emulator::new().unwrap_or_else(|e| fail(&e));
    match args.iter().map(|s| s.as_str()).collect::<Vec<_>>().as_slice() {
        ["root", file, rest @ ..] if rest.len() <= 1 => {
            let roll = roll_of(file);
            let h = emu();
            let tree = Tree::build(&h, &roll).unwrap_or_else(|e| fail(&e));
            let mut words = tree.root().to_vec();
            if let Some(c) = rest.first() {
                words.push(c.parse::<u32>().unwrap_or_else(|_| fail("cutoff_year is not a u32")));
            }
            eprintln!("{} credentials in {LEAVES} slots; {} distinct hashes run on the emulator", roll.len(), h.runs.get());
            println!("{}", join(&words));
        }
        ["path", file, index] => {
            let roll = roll_of(file);
            let i: usize = index.parse().unwrap_or_else(|_| fail("index is not a number"));
            let c = roll.get(i).unwrap_or_else(|| fail(&format!("no credential in slot {i} ({} in the roll)", roll.len())));
            let h = emu();
            let tree = Tree::build(&h, &roll).unwrap_or_else(|e| fail(&e));
            println!("{}", join(&witness(c, &tree, i)));
        }
        _ => fail("usage: issuer root <credentials file> [cutoff_year] | issuer path <credentials file> <index>"),
    }
}
