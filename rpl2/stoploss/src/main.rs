//! stoploss's zkVM program. All of the rules are `stoploss_core::check`; this file only says where
//! the words come from — the operator's lock from the public input, the transition's context after
//! it and the eight call-binding words, the method, the ticket and the opening from the private
//! inputs — and what acceptance and refusal are on this machine.
#![no_std]
#![no_main]

use guest_sdk::{halt, poseidon2, read_input, read_public, write_output};
use stoploss_core::{check, kit::Source, PUBLIC_WORDS};

struct Chain;

impl Source for Chain {
    #[inline(always)]
    fn public(&self, i: u32) -> u32 {
        read_public(i)
    }
    #[inline(always)]
    fn ctx(&self, i: u32) -> u32 {
        read_public(PUBLIC_WORDS.wrapping_add(8).wrapping_add(i))
    }
    #[inline(always)]
    fn input(&self, i: u32) -> u32 {
        read_input(i)
    }
    #[inline(always)]
    fn hash9(&self, msg: [u32; 9]) -> [u32; 8] {
        let mut buf = msg;
        poseidon2(buf.as_mut_ptr(), 9);
        [buf[0], buf[1], buf[2], buf[3], buf[4], buf[5], buf[6], buf[7]]
    }
}

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
/// no trace and the transition no proof. The loop is only for the type; it is never reached.
#[inline(never)]
fn refuse() -> ! {
    read_input(u32::MAX);
    loop {}
}
