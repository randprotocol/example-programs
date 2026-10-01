//! fold: print the roll's digest `R`. Run off chain only:
//!   rand-guest run fold/image.bin --input n tag0 w0_lo w0_hi tag1 w1_lo w1_hi …   →  out[0..8] = R
//! It is never deployed; it exists so the `R` the organiser deploys with is computed by exactly
//! the ballot program's fold (`ballot_core::{seed, step}`) and the zkVM's own Poseidon2.
#![no_std]
#![no_main]

use ballot_core::{seed, step, Hash, MAX_VOTERS};
use guest_sdk::{halt, poseidon2, read_input, write_output};

struct Chain;

impl Hash for Chain {
    #[inline(always)]
    fn hash12(&self, msg: [u32; 12]) -> [u32; 8] {
        let mut buf = msg;
        poseidon2(buf.as_mut_ptr(), 12);
        [buf[0], buf[1], buf[2], buf[3], buf[4], buf[5], buf[6], buf[7]]
    }
}

#[no_mangle]
pub extern "C" fn main() -> ! {
    let n = read_input(0);
    if n == 0 || n > MAX_VOTERS {
        refuse();
    }
    let mut st = seed(n);
    let mut i = 0u32;
    while i < n {
        let at = 1u32.wrapping_add(i.wrapping_mul(3));
        st = step(&Chain, st, read_input(at), read_input(at.wrapping_add(1)), read_input(at.wrapping_add(2)));
        i = i.wrapping_add(1);
    }
    for (j, w) in st.iter().enumerate() {
        write_output(j as u32, *w);
    }
    halt();
}

#[inline(never)]
fn refuse() -> ! {
    read_input(u32::MAX);
    loop {}
}
