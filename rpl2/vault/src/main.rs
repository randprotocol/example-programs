//! vault: anyone may pay RAND in; only whoever knows a secret may pay it out.
//!
//! An RPL-2 program that holds value. Its vault (a public balance per asset, kept by the chain)
//! fills from `Invoke`s whose bundle burns RAND (`burn_r`), and empties through payouts the chain
//! turns into ordinary shielded notes. The program decides only whether a transition is allowed:
//!
//! - **method 1, deposit** — private inputs `[1]`. Any amount of RAND in, nothing else.
//! - **method 2, withdraw** — private inputs `[2, s0, …, s7]`. One RAND payout, nothing else, and
//!   `POSEIDON2([TAG, s0..s7])` must equal the lock. The secret never leaves the prover; the
//!   proof shows only that it was known.
//!
//! The lock is the program's **deploy-time public input** (eight words), so it is part of the
//! program id: there is no "set the lock" step anyone could front-run. `lock-hash/` computes it
//! from a secret with this same code (`src/lock.rs`).
//!
//! Who gets paid is not in the context — the transaction binding fixes it, so a withdrawal seen
//! in flight cannot be redirected. The ledger checks the vault covers the payout.
//!
//! The context starts after the eight public words and the eight call-binding words:
//!
//! | word   | deposit           | withdraw                      |
//! |--------|-------------------|-------------------------------|
//! | 0      | 1 (version)       | 1                             |
//! | 1..=4  | 0, 0, 0, 0        | 0, 0, 1, 0 (one payout)       |
//! | 5, 6   | burn_r, nonzero   | 0, 0                          |
//! | 7..=10 | 0 (inflow none, no token) | 0                     |
//! | 11..=13| —                 | asset 0, amount lo, hi (nonzero) |
//!
//! Outputs: `[method, amount lo, amount hi, 0, …]`.
#![no_std]
#![no_main]

mod lock;

use guest_sdk::{halt, read_input, read_public, write_output};

const DEPOSIT: u32 = 1;
const WITHDRAW: u32 = 2;

/// Context word `i`: after the lock (8 public words) and the call binding (8).
#[inline(always)]
fn ctx(i: u32) -> u32 {
    read_public(16 + i)
}

#[no_mangle]
pub extern "C" fn main() -> ! {
    let method = read_input(0);
    require(ctx(0) == 1);
    // No cells, no mints, and no token coming in, whichever the method.
    require(ctx(1) == 0 && ctx(2) == 0 && ctx(4) == 0);
    require(ctx(7) == 0 && ctx(8) == 0 && ctx(9) == 0 && ctx(10) == 0);
    let (lo, hi) = if method == DEPOSIT {
        require(ctx(3) == 0);
        let (lo, hi) = (ctx(5), ctx(6));
        require(lo != 0 || hi != 0);
        (lo, hi)
    } else if method == WITHDRAW {
        require(ctx(3) == 1 && ctx(5) == 0 && ctx(6) == 0);
        // The payout: RAND, a nonzero amount.
        require(ctx(11) == 0);
        let (lo, hi) = (ctx(12), ctx(13));
        require(lo != 0 || hi != 0);
        let secret = [
            read_input(1), read_input(2), read_input(3), read_input(4),
            read_input(5), read_input(6), read_input(7), read_input(8),
        ];
        let digest = lock::lock_of(secret);
        for (j, w) in digest.iter().take(8).enumerate() {
            require(*w == read_public(j as u32));
        }
        (lo, hi)
    } else {
        refuse()
    };
    write_output(0, method);
    write_output(1, lo);
    write_output(2, hi);
    halt();
}

#[inline(always)]
fn require(ok: bool) {
    if !ok {
        refuse();
    }
}

/// A private input index no caller can have committed: the read is unsatisfiable, so the run has
/// no trace and the transition no proof. The loop is only for the type; it is never reached.
#[inline(never)]
fn refuse() -> ! {
    read_input(u32::MAX);
    loop {}
}
