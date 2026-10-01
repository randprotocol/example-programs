//! stmt-hash: print a statements file's commitment. Run off chain only:
//!   rand-guest run stmt-hash/image.bin --input <36 statement words> <salt0> <salt1>
//!   → out[0..8] = POSEIDON2([TAG_STMT, the 36 words, salt0, salt1]); a receipt carries out[0..6]
//! It is never deployed; it exists so a borrower who later shows their statements, and whoever
//! they show them to, recompute the commitment with exactly the guest's code.
#![no_std]
#![no_main]

use credit_score_core::{commit_message, Statements, MSG_WORDS, STMT_WORDS};
use guest_sdk::{halt, poseidon2, read_input, write_output};

#[no_mangle]
pub extern "C" fn main() -> ! {
    let mut words = [0u32; STMT_WORDS];
    for (i, w) in words.iter_mut().enumerate() {
        *w = read_input(i as u32);
    }
    let salt = [read_input(STMT_WORDS as u32), read_input(STMT_WORDS as u32 + 1)];
    let mut msg = commit_message(&Statements::from_words(&words), salt);
    poseidon2(msg.as_mut_ptr(), MSG_WORDS);
    for (j, w) in msg.iter().take(8).enumerate() {
        write_output(j as u32, *w);
    }
    halt()
}
