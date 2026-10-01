//! private-join-commit: a party's list commitment, off chain. Run on the emulator only:
//!   rand-guest run commit/image.bin --input <block: 43 words>   →  out[0..8] = the commitment
//! The block is `[n, id_lo, id_hi, k0_lo, k0_hi, …, k15_lo, k15_hi, s0, …, s7]`, exactly the
//! words the party will later hand the program. It is never deployed; it exists so that a party
//! commits with exactly the code the deployed program checks the commitment with
//! (`private_join_core::commitment`).
#![no_std]
#![no_main]

use guest_sdk::{halt, poseidon2, read_input, read_public, write_output};
use private_join_core::{commitment, Source, MSG};

struct Chain;

impl Source for Chain {
    #[inline(always)]
    fn input(&self, i: u32) -> u32 {
        read_input(i)
    }
    #[inline(always)]
    fn public(&self, i: u32) -> u32 {
        read_public(i)
    }
    #[inline(always)]
    fn hash(&self, buf: &mut [u32; MSG]) {
        poseidon2(buf.as_mut_ptr(), MSG);
    }
}

#[no_mangle]
pub extern "C" fn main() -> ! {
    let d = commitment(&Chain, 0);
    for (j, w) in d.iter().enumerate() {
        write_output(j as u32, *w);
    }
    halt();
}
