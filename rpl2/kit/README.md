# kit — what the RPL-2 DeFi examples share

A small library (`rpl2-kit`, no dependencies) that every DeFi example in `rpl2/` builds on, and
`secret-hash/`, a helper guest that is never deployed.

| module | on the guest | what |
|---|---|---|
| `ctx` | yes | the transition context: `Header` (counts, inflow), each read/write `Cell`, each pay and mint; a `Source` trait the guest answers with syscalls and the host with vectors |
| `math` | yes | `U256` products and comparisons. Programs **verify** amounts with multiplications and never divide |
| `secret` | yes | who may do what without a sender: `opens_lock` (an operator's secret against the public input), `owned_key` (a cell named by its owner's secret) |
| `host` | no | `Transition` (context words and `t.json`), `Mock` (run the rules on the host), `loose_words` (flip every context word of an accepted transition; a sound program accepts none), `max_satisfying`/`min_satisfying` (the wallet's search), `digest_of` (the real Poseidon2, via `secret-hash` on the emulator), the `plan` tools' plumbing |

## Secrets, not senders

An RPL-2 program never learns who invoked it, and does not choose who it pays: recipients are fixed
by the transaction's call binding. So a program that must know "this is the operator" or "this is
the owner of that position" asks for a secret — eight private input words — and checks its digest
`POSEIDON2([tag, s0, …, s7])`:

- an **operator** (an oracle, a campaign's creator) is whoever opens the lock deployed as the
  program's public input, as in `vault/`;
- a **position** (a loan, a vault of collateral, a perp, an order) lives in the cell
  `[cell_tag, d0, …, d6]`, `d` the owner's digest, and only its owner can move it.

The message is nine words with a tag of its own per use (guest-sdk's sponge does not pad). The
proof is bound to its transaction, so a use seen in flight can be neither replayed nor redirected.

## secret-hash

`secret-hash/` prints `POSEIDON2([tag, s0..s7])` on the emulator, so a lock or a position key is
computed by exactly the hash the programs check it with. `scripts/env.sh`'s `digest` and the
`plan` tools run it; `need_secret_hash` builds it on first use (or run `./build.sh`, which also
runs the kit's tests).
