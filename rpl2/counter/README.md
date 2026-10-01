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
