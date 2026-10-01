# RPL-1: stateless programs and RPL tokens

Everything here runs on any Rand chain (no `program_state` section needed).

| example | shows |
|---|---|
| [`threshold/`](threshold/) | a Rust guest; a deploy-time **public input**; **private inputs** that never leave the prover; a call whose receipt proves a fact without revealing the data; refusing by having no proof |
| [`spl-token/`](spl-token/) | a **Solana program** (the SPL Token ELF) translated to RISC-V by `sbpf2rv`, run against the sBPF interpreter for parity, and deployed with the ELF as its public input |
| [`eligibility/`](eligibility/) | prove a predicate (born on or before a cutoff) about a credential in an issuer's Poseidon2 Merkle tree, revealing neither the credential nor which one; the use-case page's "prove eligibility without surrendering identity" |
| [`ballot/`](ballot/) | a private ballot with a public tally: ballots are private inputs, the roll is a committed digest in the public input, the receipt is the weighted totals; the use-case page's "governance without a bribe market", with what this version does and does not deliver |
| [`rpl-token/`](rpl-token/) | an **RPL token**: fixed supply or mintable by a post-quantum key; private transfers; burning |

## How a call works

```
rand program deploy image.bin [--public words]     # the chain stores the code by its hash
rand call <id> --input w0 --input w1 …              # prove locally, pay from a bundle, get a receipt
```

1. The wallet runs the program in the zkVM on your **private inputs** and proves the run. Only
   the proof, the outputs and a salted commitment to the inputs (`H_IN`) leave your machine.
2. It pays the call's fee from a shielded bundle, proved too.
3. The chain verifies both proofs and records a **receipt**: program, outputs, tier, `H_IN`.

A program is **stateless**: a call writes only its receipt, and nothing a call does is visible to
the next one. For storage and balances held by a program, see [`../rpl2`](../rpl2/).

By default `rand call` also publishes the call's inputs sealed to your own key (an *input
envelope*, `rand open-call <tx>` reopens it); `--no-envelope` keeps even that off chain.

## Tokens are not programs

On Rand a token is not a contract. Programs have no storage to keep balances in, and a public
balance table would make tokens the one transparent asset on a shielded chain. An **RPL token** is
instead a registry entry — name, symbol, decimals, mint authority, public total supply — and its
balances are ordinary shielded notes in the same tree as RAND. A transfer is the same four-slot
bundle a RAND transfer is, and does not say which asset moved. See [`rpl-token/`](rpl-token/).
