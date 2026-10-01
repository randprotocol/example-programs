//! credit-score's zkVM program. The rules are `credit_score_core`; this file only says where the
//! words come from — the model from the deploy-time public input, the statements and the salt
//! from the private inputs — and what acceptance and refusal are on this machine.
//!
//! An RPL-1 program: stateless, run by `Action::Call`. A receipt says: someone ran this model
//! (the program id binds the code and the six model words) over twelve months of statements they
//! committed to in `c0..c5`, and the model gave them `band`, with `months_positive` months in
//! which income covered the payment. Nothing else about the statements leaves the prover.
//!
//! Refusals — a model word out of range, or a call that committed fewer than 40 private words —
//! leave no proof: the run reads an input index no caller can have committed and has no trace.
#![no_std]
#![no_main]

use credit_score_core::{commit_message, outputs, salt, score, Model, Statements, Words, MSG_WORDS};
use guest_sdk::{halt, poseidon2, read_input, read_public, write_output};

struct Chain;

impl Words for Chain {
    #[inline(always)]
    fn public(&self, i: u32) -> u32 {
        read_public(i)
    }
    #[inline(always)]
    fn input(&self, i: u32) -> u32 {
        read_input(i)
    }
}

#[no_mangle]
pub extern "C" fn main() -> ! {
    // The two blind words: uniformly random per call, read first, never used. A proof leaks an
    // unsalted function of the words it read, and twelve months of round numbers would be
    // guessable without them.
    read_input(0);
    read_input(1);
    let model = match Model::read(&Chain) {
        Some(m) => m,
        None => refuse(),
    };
    let statements = Statements::read(&Chain);
    let sc = score(&model, &statements);
    let mut msg = commit_message(&statements, salt(&Chain));
    poseidon2(msg.as_mut_ptr(), MSG_WORDS);
    let digest = [msg[0], msg[1], msg[2], msg[3], msg[4], msg[5], msg[6], msg[7]];
    for (j, w) in outputs(&sc, &digest).iter().enumerate() {
        write_output(j as u32, *w);
    }
    halt()
}

/// A private input index no caller can have committed: the read is unsatisfiable, so the run has
/// no trace and the call no proof. The loop is only for the type; it is never reached.
#[inline(never)]
fn refuse() -> ! {
    read_input(u32::MAX);
    loop {}
}
