//! sealed-auction's zkVM program. All of the rules are `sealed_auction_core::check`; this file
//! only says where the words come from — the mode from the deploy-time public input, the bids
//! from the private inputs, the hash from the `POSEIDON2` syscall — and what acceptance and
//! refusal are on this machine.
//!
//! Outputs: `[winner tag, price lo, price hi, fold0, fold1, fold2, fold3, fold4]`.
#![no_std]
#![no_main]

mod chain;

use chain::Chain;
use guest_sdk::{halt, read_input, write_output};
use sealed_auction_core::check;

#[no_mangle]
pub extern "C" fn main() -> ! {
    match check(&Chain) {
        Ok(out) => {
            for (j, w) in out.iter().enumerate() {
                write_output(j as u32, *w);
            }
            halt()
        }
        Err(_) => refuse(),
    }
}

/// A private input index no caller can have committed: the read is unsatisfiable, so the run has
/// no trace and the call no proof. The loop is only for the type; it is never reached.
#[inline(never)]
fn refuse() -> ! {
    read_input(u32::MAX);
    loop {}
}
