//! secret-hash: print `POSEIDON2([tag, s0, …, s7])`. Run off chain only:
//!   rand-guest run secret-hash/image.bin --input tag s0 … s7   →  out[0..8] = the digest
//! It is never deployed; it exists so that an operator's lock and an owner's cell key are
//! computed by exactly the hash the programs check them with (`kit/src/secret.rs`).
#![no_std]
#![no_main]

use guest_sdk::{halt, poseidon2, read_input, write_output};

#[no_mangle]
pub extern "C" fn main() -> ! {
    let mut buf = [
        read_input(0), read_input(1), read_input(2), read_input(3), read_input(4),
        read_input(5), read_input(6), read_input(7), read_input(8),
    ];
    poseidon2(buf.as_mut_ptr(), 9);
    for (j, w) in buf.iter().take(8).enumerate() {
        write_output(j as u32, *w);
    }
    halt();
}
