//! rpl2-kit: what the RPL-2 examples in this folder share.
//!
//! - [`ctx`] reads a transition's context — the header, each read and write, each payout and
//!   mint — through a [`Source`], which the guest answers with syscalls and a test with slices.
//! - [`math`] is the arithmetic the rules need: products and comparisons in 256 bits. A program
//!   here **verifies** claimed amounts and never divides (`docs/program-state.md` in fullnode:
//!   "Verify claimed amounts; do not compute them"), so it needs no division at all.
//! - [`secret`] is how a program knows who may do what without a sender: a cell is owned by
//!   whoever knows the secret whose digest names it.
//! - `host` (not on the guest) builds a transition: its context words, for the emulator, and its
//!   `t.json`, for `rand program invoke`.
//!
//! Nothing in the guest-side modules may panic: a panic halts, and a halted run is a provable
//! run. Every example's `build.sh` refuses an image that links the panic machinery.
#![cfg_attr(target_arch = "riscv32", no_std)]
#![forbid(unsafe_code)]

pub mod ctx;
pub mod math;
pub mod secret;

#[cfg(not(target_arch = "riscv32"))]
pub mod host;

pub use ctx::*;
pub use math::*;
pub use secret::*;
