# Rand Protocol example programs

Small, complete programs for [Rand Protocol](https://randprotocol.org), the fully shielded,
post-quantum chain, each with the scripts that build it, run it off chain, deploy it and use it on
chain.

| folder | what | runs as |
|---|---|---|
| [`rpl1/`](rpl1/) | **RPL-1**: stateless programs and RPL tokens | `Call`, token actions |
| [`rpl1/threshold`](rpl1/threshold/) | prove a private amount is at least a public threshold | a Rust guest, `rand call` |
| [`rpl1/spl-token`](rpl1/spl-token/) | the Solana SPL Token program, translated to RISC-V and deployed | `sbpf2rv`, `rand program deploy` |
| [`rpl1/rpl-token`](rpl1/rpl-token/) | create, mint, send and burn a shielded RPL token | `rand token …`, `rand send` |
| [`rpl2/`](rpl2/) | **RPL-2**: programs with state and a vault | `Invoke` |
| [`rpl2/counter`](rpl2/counter/) | one cell, incremented by one per invoke | a Rust guest, `rand program invoke` |
| [`rpl2/vault`](rpl2/vault/) | anyone pays RAND in; only whoever knows a secret pays it out | a Rust guest, `rand program invoke` |

For a full application on RPL-2, see [durian.market](https://github.com/randprotocol/durian.market),
a constant-product AMM.

## RPL-1 and RPL-2 in one paragraph each

**RPL-1.** A program is RISC-V code the chain stores by its hash. A `Call` runs it in the zkVM:
the caller proves, on their own machine, that the program ran on their private inputs and
produced certain outputs, and the chain checks the proof and keeps a receipt of the outputs.
Programs are **stateless** — a call writes only its receipt — so a token is not a contract but a
ledger object: an **RPL token** is a registry entry whose balances are shielded notes, exactly
like RAND's.

**RPL-2.** A program gets **cells** (public key/value storage), a **vault** (a public balance per
asset) and optionally **a token of its own** that only it mints. An `Invoke` declares a whole
state transition — the cells it read and writes, what comes in, what is paid out, what is minted
— and proves the program accepts it. The chain applies it only if every declared read still
matches. The program never computes anything the caller can't see; it checks.

## What you need

| | |
|---|---|
| `rand` | the wallet CLI from [fullnode](https://github.com/randprotocol/fullnode), **v0.6.8 or later** (RPL-2). `cargo build --release -p randprotocol-client` → `target/release/rand` |
| a node | a chain with the `program_state` genesis section for RPL-2. The public durian devnet (chain 1919) is the default: `https://durian.market/api/wallet-rpc`, with a faucet |
| a wallet | `rand --key ~/.rand/wallet.key.json keygen`, then `rand --key … faucet` on a test chain |
| to build guests | a checkout of the circuits repo (`guest-sdk`, `rand-guest`, `sbpf2rv`), Rust **1.98.1** with `riscv32im-unknown-none-elf` and `llvm-tools`; clang **23.1.1** for the SPL example |
| to prove | about 6 GB of free memory; a bundle proof takes 1–4 minutes on a CPU |

```sh
rustup toolchain install 1.98.1
rustup +1.98.1 target add riscv32im-unknown-none-elf
rustup +1.98.1 component add llvm-tools
```

Every script reads its settings from [`scripts/env.sh`](scripts/env.sh); override them in your
environment:

| variable | default |
|---|---|
| `RAND` | `rand` (on `PATH`) |
| `RAND_RPC` | `https://durian.market/api/wallet-rpc` |
| `RAND_KEY` | `~/.rand/wallet.key.json` |
| `CIRCUITS` | `~/circuits` |

```sh
export CIRCUITS=~/src/circuits RAND_KEY=~/.rand/wallet.key.json
rand --key "$RAND_KEY" keygen && rand --key "$RAND_KEY" faucet --rpc "$RAND_RPC"
cd rpl2/counter && ./build.sh && ./run.sh && ./deploy.sh && ./invoke.sh && ./state.sh
```

## Two rules every program here keeps

1. **A refusal leaves no proof.** `guest-sdk`'s panic handler halts, and a halted run is a
   provable run whatever its outputs say. So these programs contain no panicking path, and refuse
   by reading a private input index no caller can have committed (`read_input(u32::MAX)`): the run
   has no trace, so there is nothing to prove. Each `build.sh` rejects an image that links the
   panic machinery.
2. **An RPL-2 program checks every word it is shown.** A context word left unchecked is a word any
   caller can set — a payout, a deposit, another cell.

## Licence

GPL-3.0-only, as the circuits repository's `guest-sdk` these programs link. `rpl1/spl-token/program.so`
is the SPL Token program (Apache-2.0, the Solana Program Library), as committed in the circuits
repository.
