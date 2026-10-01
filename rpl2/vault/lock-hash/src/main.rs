//! lock-hash: print the vault's lock for a secret. Run off chain only:
//!   rand-guest run lock-hash/image.bin --input s0 … s7   →  out[0..8] = the lock
//! It is never deployed; it exists so the lock is computed by exactly the vault's code.
#![no_std]
#![no_main]

#[path = "../../src/lock.rs"]
mod lock;

use guest_sdk::{halt, read_input, write_output};

#[no_mangle]
pub extern "C" fn main() -> ! {
    let secret = [
        read_input(0), read_input(1), read_input(2), read_input(3),
        read_input(4), read_input(5), read_input(6), read_input(7),
    ];
    let digest = lock::lock_of(secret);
    for (j, w) in digest.iter().take(8).enumerate() {
        write_output(j as u32, *w);
    }
    halt();
}
