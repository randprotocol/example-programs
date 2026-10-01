# threshold — prove a private amount clears a public bar

An RPL-1 program: stateless, run with `rand call`. The caller proves that a number they hold
privately is at least a threshold fixed when the program was deployed. The chain learns that the
statement is true, and nothing about the number.

```
public input (deploy)   threshold: two words, a u64 little-endian         threshold.txt = "1000 0"
private input (call)    amount: two words, a u64 little-endian             never leaves your machine
outputs (receipt)       [1, threshold_lo, threshold_hi, 0, 0, 0, 0, 0]
```

Below the threshold there is **no proof at all**: the program refuses by reading a private input
index no caller can have committed, so the run has no trace. A receipt for this program can only
ever say "yes".

What it proves is knowledge of a number, not ownership of funds: it is the smallest example of a
private input and a public one meeting in a proof. A real use would bind the amount to something
the chain checks (an RPL-2 cell, a note commitment).

## Files

| file | |
|---|---|
| `src/main.rs` | the guest |
| `Cargo.toml` | a `no_std`, `no_main` crate on `guest-sdk`, `panic = "abort"` |
| `threshold.txt` | the deploy-time public input: `lo hi` |
| `build.sh` | build `image.bin` inside your circuits checkout and reject any panicking path |
| `run.sh` | run it on the zkVM emulator, off chain, above and below the threshold |
| `deploy.sh` | deploy with `threshold.txt` as the public input; saves the id to `program.id` |
| `call.sh <amount>` | prove the statement on chain and print the receipt |

## Run it

```sh
export CIRCUITS=~/src/circuits            # see the top-level README for the rest
./build.sh
./run.sh 2500
./deploy.sh
./call.sh 2500
```

`run.sh`, off chain:

```
threshold 1000 (public), amount 2500 (private):
out[0] = 1
out[1] = 1000
out[2] = 0
…
tier 10: fits (cycles 56 of 1023, Poseidon2 permutations 16 of 128)

amount 1 (below the threshold):
trap: InputIndex(4294967295)
```

The image is 51 words and every call proves at tier 10, the smallest.

`call.sh` passes `--expect-public threshold.txt`, so the wallet refuses before proving if the
program on chain was deployed with a different threshold. The threshold is part of the program id
(`rand-program-2` hashes the code and the public input), so changing `threshold.txt` and
redeploying gives a new program.

## On chain
