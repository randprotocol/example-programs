# counter — one cell, plus one per invoke

The smallest RPL-2 program with state. Cell `[1, 0, 0, 0, 0, 0, 0, 0]` holds a u64; each invoke
declares "it held `n`, it now holds `n + 1`", and the program accepts exactly that.

```
key    [1, 0, 0, 0, 0, 0, 0, 0]
value  [n_lo, n_hi, 0, 0, 0, 0, 0, 0]        an absent cell reads as zeros: the first invoke declares n = 0
outputs [n + 1 (lo), n + 1 (hi), 0, …]
```

The program does not read the chain's state — no program can. The **caller** declares the read,
the proof shows the program accepts the declared transition, and the **ledger** then compares the
declared read with its own cell. If someone else incremented first, the chain refuses ours as a
`StaleRead` (`rand` exits 3), and `invoke.sh` re-reads and tries again.

## What it checks

Every word of the context, because a word left unchecked is a word any caller could set:

| context words | must be |
|---|---|
| 0 | version 1 |
| 1..=4 | one read, one write, no payouts, no mints |
| 5..=10 | nothing coming in: `burn_r`, inflow, `burn_asset`, `burn_a` all zero |
| 11..=18, 27..=34 | both keys are `[1, 0, …]` |
| 19..=26, 35..=42 | value words 2..=7 zero on both sides; written value = read value + 1, no wrap |

Without the last-but-one row, a caller could declare a transition that also pays itself out of a
vault or writes a second cell — and this program would have proved it acceptable.

## Files

| file | |
|---|---|
| `src/main.rs` | the guest |
| `build.sh` | build `image.bin` inside your circuits checkout and reject any panicking path |
| `run.sh [n]` | run it on the emulator over a hand-built context: `n → n+1` (accepted), `n → n+2` (refused) |
| `deploy.sh` | deploy (no public input); saves the id to `program.id` |
| `invoke.sh` | read the cell, declare `n → n + 1`, prove, submit; retry on a stale read |
| `state.sh` | show the program's cells |

## Run it

```sh
./build.sh && ./run.sh 41
./deploy.sh
./invoke.sh
./state.sh
```

`run.sh 41`, off chain:

```
n = 41 → 42 (accepted):
out[0] = 42
out[1] = 0
…
tier 10: fits (cycles 271 of 1023, Poseidon2 permutations 40 of 128)

n = 41 → 43 (refused — no run, so no proof):
trap: InputIndex(4294967295)
```

The transition `invoke.sh` writes (`t.json`), for the first invoke:

```json
{
  "reads":  [{ "key": "0100000000000000000000000000000000000000000000000000000000000000",
               "value": "0000000000000000000000000000000000000000000000000000000000000000" }],
  "writes": [{ "key": "0100000000000000000000000000000000000000000000000000000000000000",
               "value": "0100000000000000000000000000000000000000000000000000000000000000" }]
}
```

The first invoke **creates** the cell, so it pays the chain's `cell_fee` on top of the call fee
(0.01 RAND on the devnet); later ones rewrite it for free.

## On chain

### Chain 20, the Rand testnet

Run 2026-10-01 through `https://rpc.randprotocol.org` with a v0.6.8 (`main`) `rand` and these
scripts, unmodified:

| | |
|---|---|
| program id | `b1440b21ab4e91560474d7239c980af26f46e759dcfb8a459ae72f0d4e4d31ec` |
| deploy | `a993459bbf66bf5cd34368df1d2eacb223d4ae9215f5c4ccb0608f9b0d79b586`, fee 0.0113 RAND |
| `./invoke.sh` (0 → 1) | `433b0dc28b6ea8bd2da2f35e2efc50745deef8fb011fea17132addcddfb3335c`, fee 0.015477625 RAND (the 0.01 cell fee included: it created the cell) |
| `./invoke.sh` (1 → 2) | `b691e19114d6a971e976362484f22d55a14d1acf9843db30f216315f1aa987a1`, fee 0.005477625 RAND (a rewrite: no cell fee) |

Both invokes prove at tier 10. The counter on chain 20 is public and shared: `echo
b1440b21ab4e91560474d7239c980af26f46e759dcfb8a459ae72f0d4e4d31ec > program.id` and `./invoke.sh`.


The image `build.sh` produces (built images are not committed; `build.sh` reproduces them byte
for byte with the pinned toolchain):

| | |
|---|---|
| `image.bin` sha256 | `2f18dd0aa6721fe1ede253eecde832f7361c647f8664c83e8bdceaf218d7e9f7` |
| `hc` (`rand-guest`) | `abf12528c2e7d8b4f98df55f448a11f3fa8c8646d38e37640bafaf97c8ace481` |
| program id (no public input, so the deployed id is the same) | `b1440b21ab4e91560474d7239c980af26f46e759dcfb8a459ae72f0d4e4d31ec` |

Anyone who builds this source gets **the same program id**. On chain 20 it is already
deployed, so you can skip `deploy.sh` there: `echo b1440b21ab4e91560474d7239c980af26f46e759dcfb8a459ae72f0d4e4d31ec > program.id`
and run `./invoke.sh` — it increments the same public counter everyone shares.


### Earlier, on the durian devnet (chain 1919, retired 2026-10-01)

```
$ ./deploy.sh
program id: b1440b21ab4e91560474d7239c980af26f46e759dcfb8a459ae72f0d4e4d31ec (103 words, …)
submitted deploy 7fcf9c27d10d01eec317e3e1eee7f933ae98db91751394bb056300e98e6d4326
  0 RAND out, …, fee 0.0113 RAND, …

$ ./invoke.sh
cell 1 holds 0; declaring 0 → 1
fee 0.014328 RAND (1 cell created at 0.01 RAND each)
proved in 23.6s: tier 10, 1366314 bytes, outputs [1, 0, 0, 0, 0, 0, 0, 0]
authorisation proved in 25.0s: tier 10, 1364522 bytes
proved in 372.0s: tier 14, 1497479 bytes
submitted invoke b7d7e0c721379d2d46512577364d0e32b602557bed5ee9138c929c2e922ea651

$ ./state.sh
"cells": [{ "key": "0100000000000000000000000000000000000000000000000000000000000000",
            "value": "0100000000000000000000000000000000000000000000000000000000000000" }]
```
