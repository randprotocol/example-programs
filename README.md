# Rand Protocol example programs

Small, complete programs for [Rand Protocol](https://randprotocol.org), the fully shielded,
post-quantum chain, each with the scripts that build it, run it off chain, deploy it and use it on
chain.

| folder | what | runs as |
|---|---|---|
| [`rpl1/`](rpl1/) | **RPL-1**: stateless programs and RPL tokens | `Call`, token actions |
| [`rpl1/threshold`](rpl1/threshold/) | prove a private amount is at least a public threshold | a Rust guest, `rand call` |
| [`rpl1/spl-token`](rpl1/spl-token/) | the Solana SPL Token program, translated to RISC-V and deployed | `sbpf2rv`, `rand program deploy` |
| [`rpl1/eligibility`](rpl1/eligibility/) | prove a credential in an issuer's Merkle tree meets a predicate, without revealing which | a Rust guest + `issuer` tool, `rand call` |
| [`rpl1/private-join`](rpl1/private-join/) | private set intersection / two-sided matching over committed lists | a Rust guest + `join` tool, `rand call` |
| [`rpl1/credit-score`](rpl1/credit-score/) | a published scoring model over private bank statements; the receipt is the band | a Rust guest + `score` tool, `rand call [--auditor]` |
| [`rpl1/sealed-auction`](rpl1/sealed-auction/) | a Vickrey auction / sealed RFQ over private bids; the receipt binds every bid's commitment | a Rust guest + `auction` tool, `rand call` |
| [`rpl1/ballot`](rpl1/ballot/) | private ballots over a committed voter roll, a public weighted tally | a Rust guest + `ballot` tool, `rand call` |
| [`rpl1/rpl-token`](rpl1/rpl-token/) | create, mint, send and burn a shielded RPL token | `rand token …`, `rand send` |
| [`rpl2/`](rpl2/) | **RPL-2**: programs with state and a vault | `Invoke` |
| [`rpl2/counter`](rpl2/counter/) | one cell, incremented by one per invoke | a Rust guest, `rand program invoke` |
| [`rpl2/vault`](rpl2/vault/) | anyone pays RAND in; only whoever knows a secret pays it out | a Rust guest, `rand program invoke` |
| [`rpl2/kit`](rpl2/kit/) | the library the DeFi examples share: context, 256-bit math, secrets, a host-side transition builder | a Rust crate (guest and host) |
| [`rpl2/amm`](rpl2/amm/) | a constant-product AMM for RAND and one token, with liquidity shares | a Rust guest + `plan` tool, `rand program invoke` |
| [`rpl2/stableswap`](rpl2/stableswap/) | a Curve-style StableSwap pool; the invariant is declared and checked, not computed | a Rust guest + `plan` tool |
| [`rpl2/orderbook`](rpl2/orderbook/) | escrowed limit orders with partial fills and ticket secrets | a Rust guest + `plan` tool |
| [`rpl2/lending`](rpl2/lending/) | on-chain lending: supply RAND for shares, borrow against a token at 75 % LTV, liquidations | a Rust guest + `plan` tool (tier 14) |
| [`rpl2/stablecoin`](rpl2/stablecoin/) | a CDP stablecoin: oracle price, 150 % collateral, liquidation under 110 % | a Rust guest + `plan` tool |
| [`rpl2/perp`](rpl2/perp/) | a perp DEX: oracle-priced, LP pool as counterparty, 10× leverage, liquidations | a Rust guest + `plan` tool (tier 14) |
| [`rpl2/stoploss`](rpl2/stoploss/) | a stop-loss nobody can hunt: the trigger is a commitment, fired against an oracle cell | a Rust guest + `plan` tool |
| [`rpl2/crowdfund`](rpl2/crowdfund/) | all-or-nothing crowdfunding with refundable receipt tokens | a Rust guest + `plan` tool |

For a full application on RPL-2, see [durian.market](https://github.com/randprotocol/durian.market),
a constant-product AMM.

The DeFi examples (`rpl2/amm` onwards) share one layout: `core/src/lib.rs` holds the rules as a
`check(source) → accept | refuse` function that runs unchanged on the zkVM and on your machine;
`core/src/plan.rs` is the wallet's side, finding the best amounts by bisection over the very
inequalities the program checks; `core/tests/rules.rs` accepts the best amount, refuses one unit
more, and flips every context word of every accepted transition to show none is left unchecked;
`run.sh` runs every method on the emulator, accepted and refused, and checks the guest agrees
with the host rules. See [`rpl2/amm`](rpl2/amm/) for the walk-through.

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
| a node | a chain with the `program_state` genesis section for RPL-2. The public testnet, **chain 20**, is the default: `https://rpc.randprotocol.org` (its front must pass the RPL-2 read methods, which it does since randprotocol.org `e77ec7a`) |
| a wallet | `rand --key ~/.rand/wallet.key.json keygen`. Chain 20's faucet mints only to its genesis wallets, so `rand faucet` is refused there (`rand_mint` is not public): ask the operators for test RAND — about 30 RAND runs every example here once |
| to build guests | a checkout of the circuits repo (`guest-sdk`, `rand-guest`, `sbpf2rv`), Rust **1.98.1** with `riscv32im-unknown-none-elf` and `llvm-tools`; clang **23.1.1** for the SPL example |
| to prove | about 6 GB of free memory. With a `main`-branch `rand` a tier-14 bundle proof takes ~20 s and a tier-12 call ~6 s on an M4 laptop, the prover keeping about four cores busy (the pre-rebase `feat/rpl2` build is 5–6× slower); an RPL-2 invoke, three proofs, lands in 35–45 s |

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
| `RAND_RPC` | `https://rpc.randprotocol.org` (chain 20) |
| `RAND_KEY` | `~/.rand/wallet.key.json` |
| `CIRCUITS` | `~/circuits` |

```sh
export CIRCUITS=~/src/circuits RAND_KEY=~/.rand/wallet.key.json
rand --key "$RAND_KEY" keygen            # then get test RAND for its address (see above)
cd rpl2/counter && ./build.sh && ./run.sh && ./deploy.sh && ./invoke.sh && ./state.sh
```

## Everything here has run on chain 20

On 2026-10-01 every example in this repository was deployed and used on chain 20, the public
testnet, through `https://rpc.randprotocol.org`, by its own scripts — 20 programs, 9 RPL tokens (7 of
them minted only by their program), 11 calls and 41 invokes, every receipt equal to what `run.sh`
predicted on the emulator; the one exception is the SPL Token translation, which the chain refuses
to deploy (below).
Each README's **On chain** section has the program id and every transaction. The same flows ran
unchanged on a local chain cut with the same genesis flags; the RPL-2 vaults ended at identical
values on both.

What running it on chain found, and where it was fixed:

| what | where |
|---|---|
| the chain-20 public RPC front refused `rand_getProgramCell`/`Cells`/`Vault`, so no RPL-2 program could be driven from the published endpoint (`rand program invoke` checks every declared read first) | randprotocol.org `e77ec7a`: its allowlist now mirrors the node's `PUBLIC_METHODS` |
| durian.market's wallet proxy refused `rand_getRawTransaction`, which every scan needs after a faucet mint, so no external wallet could sync | durian.market `887252d` |
| a `main`-built wallet refuses chain 1919 (BIND-1: no `binding_domain` in that genesis); chains from 20 on carry one | moot: the devnet is retired for chain 20 |
| `rand_status` says `faucet: true` on chain 20 but `rand_mint` is not public and the faucet mints only to genesis wallets | docs: how a chain-20 user gets test RAND |
| the `feat/rpl2` build proves 5–6× slower than `main`; `main`'s prover averages ~4 of 16 cores (Plonky3's `parallel` is on, but the serial phases are long) | run the `main` / v0.6.8 build; wider parallelism is the open prover item |
| the SPL Token translation (65 096 words, 27 151 public words) is refused at deploy on chain 20: "can never be called — a call proves at most 0 program words beside that public input at the highest tier a call may use" (calls cap at tier 14; its instructions need tier 20) | `rpl1/spl-token` runs off chain only until calls may use higher tiers, or the program is split |

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
