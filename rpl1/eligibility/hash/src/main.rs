//! eligibility-hash: the program's two hashes, off chain. Run on the emulator only:
//!   rand-guest run hash/image.bin --input 0 m0 … m8      →  out[0..8] = POSEIDON2(m), a leaf
//!   rand-guest run hash/image.bin --input 1 m0 … m16     →  out[0..8] = POSEIDON2(m), a node
//! It is never deployed; it exists so that the issuer's tree is built by exactly the code the
//! deployed program checks it with (`src/hash.rs`, included here by path). Any other mode word
//! refuses (no output at all), so a caller cannot mistake one mode for the other.
#![no_std]
#![no_main]

#[path = "../../src/hash.rs"]
mod hash;

use guest_sdk::{halt, read_input, write_output};

#[no_mangle]
pub extern "C" fn main() -> ! {
    let mode = read_input(0);
    let d = if mode == 0 {
        hash::hash9([
            read_input(1), read_input(2), read_input(3), read_input(4), read_input(5),
            read_input(6), read_input(7), read_input(8), read_input(9),
        ])
    } else if mode == 1 {
        hash::hash17([
            read_input(1), read_input(2), read_input(3), read_input(4), read_input(5),
            read_input(6), read_input(7), read_input(8), read_input(9), read_input(10),
            read_input(11), read_input(12), read_input(13), read_input(14), read_input(15),
            read_input(16), read_input(17),
        ])
    } else {
        read_input(u32::MAX);
        loop {}
    };
    for (j, w) in d.iter().enumerate() {
        write_output(j as u32, *w);
    }
    halt();
}
