//! threshold: prove that a private amount is at least a public threshold, and reveal nothing else.
//!
//! An RPL-1 program: stateless, run by `Action::Call`. The threshold is the program's deploy-time
//! public input (two words, a u64 little-endian), so it is part of the program id and every call
//! proves against the same one. The amount is two private input words. The proof commits to them
//! through `H_IN`, which the prover salts, so the commitment says nothing about the amount.
//!
//! A receipt for this program therefore says exactly one thing: someone knew an amount
//! `>= threshold`. Below the threshold there is no proof at all — the run never halts — so
//! there is no receipt that says "no" either.
//!
//! Outputs: `[1, threshold_lo, threshold_hi, 0, 0, 0, 0, 0]`.
#![no_std]
#![no_main]

use guest_sdk::{halt, read_input, read_public, write_output};

#[no_mangle]
pub extern "C" fn main() -> ! {
    let threshold = u64::from(read_public(0)) | (u64::from(read_public(1)) << 32);
    let amount = u64::from(read_input(0)) | (u64::from(read_input(1)) << 32);
    if amount < threshold {
        refuse();
    }
    write_output(0, 1);
    write_output(1, read_public(0));
    write_output(2, read_public(1));
    halt();
}

/// A private input index no caller can have committed: the read is unsatisfiable, so the run has
/// no trace and the call no proof. The loop is only for the type; it is never reached.
#[inline(never)]
fn refuse() -> ! {
    read_input(u32::MAX);
    loop {}
}
