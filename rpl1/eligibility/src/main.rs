//! eligibility's zkVM program. The rules are `eligibility_core::check`; this file only says where
//! the words come from — the issuer's root and the cutoff from the public input, the credential
//! and its Merkle path from the private inputs, the hash from the `POSEIDON2` syscall — and what
//! acceptance and refusal are on this machine.
//!
//! Outputs: `[1, cutoff_year, root0, root1, root2, root3, root4, root5]`.
#![no_std]
#![no_main]

mod hash;

use eligibility_core::{check, Hash, Source};
use guest_sdk::{halt, read_input, read_public, write_output};

struct Chain;

impl Hash for Chain {
    #[inline(always)]
    fn hash9(&self, m: [u32; 9]) -> [u32; 8] {
        hash::hash9(m)
    }
    #[inline(always)]
    fn hash17(&self, m: [u32; 17]) -> [u32; 8] {
        hash::hash17(m)
    }
}

impl Source for Chain {
    #[inline(always)]
    fn public(&self, i: u32) -> u32 {
        read_public(i)
    }
    #[inline(always)]
    fn input(&self, i: u32) -> u32 {
        read_input(i)
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
