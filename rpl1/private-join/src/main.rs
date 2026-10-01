//! private-join's zkVM program. The rules are `private_join_core::check`; this file only says
//! where the words come from — the two commitments and the mode from the public input, the two
//! lists (with their salts) from the private inputs, the hash from the `POSEIDON2` syscall — and
//! what acceptance and refusal are on this machine.
//!
//! Outputs: `[result, mode, 0, 0, 0, 0, 0, 0]` — the intersection size (mode 0) or the match bit
//! (mode 1). A list that is unsorted, too long, or not the one committed to has no proof at all.
#![no_std]
#![no_main]

use guest_sdk::{halt, poseidon2, read_input, read_public, write_output};
use private_join_core::{check, Source, MSG};

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
