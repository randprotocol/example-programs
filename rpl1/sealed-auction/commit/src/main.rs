//! commit: the sealed-auction's hashes, computed by the program's own code. Run off chain only:
//!
//!   rand-guest run commit/image.bin --input 0 tag bid_lo bid_hi salt0 salt1   → out[0..8] = the bid's commitment
//!   rand-guest run commit/image.bin --input 1 n c1[0..8] … cn[0..8]           → out[0..8] = the fold of c1 … cn
//!
//! It is never deployed; it exists so a bidder finding their commitment in the published list,
//! and anyone folding that list to the receipt's words, use exactly the hash the program used.
#![no_std]
#![no_main]

#[path = "../../src/chain.rs"]
mod chain;

use chain::Chain;
use guest_sdk::{halt, read_input, write_output};
use sealed_auction_core::{commitment, fold, read_bid, read_words8, Source, MAX_BIDS};

#[no_mangle]
pub extern "C" fn main() -> ! {
    let s = Chain;
    let out = match s.input(0) {
        0 => commitment(&s, &read_bid(&s, 1)),
        1 => {
            let n = s.input(1);
            if (n < 1) | (n > MAX_BIDS as u32) {
                refuse();
            }
            let mut acc = [0u32; 8];
            for i in 0..n {
                let c = read_words8(&s, 2u32.wrapping_add(i.wrapping_mul(8)));
                acc = fold(&s, &acc, &c);
            }
            acc
        }
        _ => refuse(),
    };
    for (j, w) in out.iter().enumerate() {
        write_output(j as u32, *w);
    }
    halt();
}

/// As the program's: an unsatisfiable read, so no run.
#[inline(never)]
fn refuse() -> ! {
    read_input(u32::MAX);
    loop {}
}
