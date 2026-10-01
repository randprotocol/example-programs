//! counter: one cell, incremented by exactly one per invoke.
//!
//! An RPL-2 program (`Action::Invoke`). The caller declares the transition — "cell 1 held `n`,
//! it now holds `n + 1`" — and this program's whole job is to accept that transition or refuse
//! it. The ledger then checks the read against its own state (a stale `n` is refused as
//! `StaleRead`) and applies the write.
//!
//! The context (`docs/program-state.md` in fullnode, "What the program sees") starts after the
//! eight call-binding words, since this program has no deploy-time public input:
//!
//! | word    | field                               | must be                          |
//! |---------|-------------------------------------|----------------------------------|
//! | 0       | version                             | 1                                |
//! | 1..=4   | n_reads, n_writes, n_pays, n_mints  | 1, 1, 0, 0                       |
//! | 5..=10  | burn_r (2), inflow, burn_asset, burn_a (2) | all 0: nothing comes in   |
//! | 11..=18 | the read's key                      | `[1, 0, 0, 0, 0, 0, 0, 0]`       |
//! | 19..=26 | the read's value                    | `[lo, hi, 0, 0, 0, 0, 0, 0]` = n |
//! | 27..=34 | the write's key                     | the same key                     |
//! | 35..=42 | the write's value                   | n + 1, the same layout           |
//!
//! **Every word is checked.** A word this program did not constrain would be a word any caller
//! could set — a payout, a deposit, a second cell. An absent cell reads as zeros, so the first
//! invoke declares `n = 0`.
//!
//! Outputs: `[n + 1 (lo), n + 1 (hi), 0, …]`.
#![no_std]
#![no_main]

use guest_sdk::{halt, read_input, read_public, write_output};

/// Context word `i`: after the eight call-binding words (no deploy-time public input).
#[inline(always)]
fn ctx(i: u32) -> u32 {
    read_public(8 + i)
}

/// The counter's cell key: `[1, 0, 0, 0, 0, 0, 0, 0]`.
const KEY0: u32 = 1;

const READ_KEY: u32 = 11;
const READ_VALUE: u32 = 19;
const WRITE_KEY: u32 = 27;
const WRITE_VALUE: u32 = 35;

#[no_mangle]
pub extern "C" fn main() -> ! {
    // Version and shape: one read, one write, no payouts, no mints.
    require(ctx(0) == 1);
    require(ctx(1) == 1 && ctx(2) == 1 && ctx(3) == 0 && ctx(4) == 0);
    // Nothing comes in: no RAND, no token, inflow "none".
    let mut i = 5;
    while i <= 10 {
        require(ctx(i) == 0);
        i += 1;
    }
    // Both keys are the counter's.
    require(ctx(READ_KEY) == KEY0 && ctx(WRITE_KEY) == KEY0);
    let mut i = 1;
    while i < 8 {
        require(ctx(READ_KEY + i) == 0 && ctx(WRITE_KEY + i) == 0);
        // The value's upper six words are zero on both sides.
        if i >= 2 {
            require(ctx(READ_VALUE + i) == 0 && ctx(WRITE_VALUE + i) == 0);
        }
        i += 1;
    }
    // n → n + 1, without wrapping.
    let n = u64::from(ctx(READ_VALUE)) | (u64::from(ctx(READ_VALUE + 1)) << 32);
    require(n != u64::MAX);
    let next = n + 1;
    let (lo, hi) = (next as u32, (next >> 32) as u32);
    require(ctx(WRITE_VALUE) == lo && ctx(WRITE_VALUE + 1) == hi);

    write_output(0, lo);
    write_output(1, hi);
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
