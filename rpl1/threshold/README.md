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

### Chain 20, the Rand testnet

Run 2026-10-01 through `https://rpc.randprotocol.org` with a v0.6.8 (`main`) `rand` and these
scripts, unmodified:

| | |
|---|---|
| program id (deployed with `1000 0`) | `71f6c2e0d1772cdcfeba767ceca2939761d7feda0b7bed96dc366096a5aaa541` |
| deploy | `fee259de43b482b1396eb8cf8791adef7f10d3fc5dd306ab3e9cabf59e9245b1`, fee 0.0063 RAND |
| `./call.sh 2500` | `1cd6237db84a688e1f1213567b299b58c1e607362eea9a714ef2f2fb8102bcac`, outputs `[1, 1000, 0, 0, 0, 0, 0, 0]`, tier 10 |

The program id is the same on every chain: it is a hash of the code and the threshold.


The image `build.sh` produces (built images are not committed; `build.sh` reproduces them byte
for byte with the pinned toolchain):

| | |
|---|---|
| `image.bin` sha256 | `f07e53a9878da70e1c1e4e46e57b1f48f336993fecd75765814b970cb229abaf` |
| `hc` (`rand-guest`) | `5f74e137f4cb75e8723853f50060c225d2854bb1f6ba8e8e3f6b1902285ffd7e` |
| program id, code only | `6b5ccea3633cd11f71a8f59b6a517311861b0dc3f02a6d8c3db22cd8616c0dfd` |
| program id deployed with `threshold.txt` = `1000 0` | `71f6c2e0d1772cdcfeba767ceca2939761d7feda0b7bed96dc366096a5aaa541` |


### Earlier, on the durian devnet (chain 1919, retired 2026-10-01)

From a fresh faucet wallet, with the scripts above (the pre-rebase `feat/rpl2` build):

```
$ ./deploy.sh
program id: 71f6c2e0d1772cdcfeba767ceca2939761d7feda0b7bed96dc366096a5aaa541 (51 words, …, public input 2 words, …)
authorisation proved in 22.7s: tier 10, 1375787 bytes
proved in 383.4s: tier 14, 1494600 bytes
submitted deploy 7cbb286e16fc8fccec7043555dccfed44d7d93c29bb61465f8a505ad7672c7fe
  0 RAND out, 99.9937 RAND change, fee 0.0063 RAND, …

$ ./call.sh 2500
fee 0.0043032 RAND
proved in 26.0s: tier 10, 1372333 bytes, outputs [1, 1000, 0, 0, 0, 0, 0, 0]
authorisation proved in 25.0s: tier 10, 1367819 bytes
proved in 378.4s: tier 14, 1495688 bytes
submitted call 3ef1d975b4cb87967892c18768983bad9e5f7c78bddd1e38bd9e71c83f7160e4
{ "height": 2759, "outputs": [1, 1000, 0, 0, 0, 0, 0, 0], "tier": 10, … }
```

The call proof (tier 10) took 26 s; most of the wall time is the bundle that pays the fee
(tier 14: ~6 minutes on that 4-vCPU cloud machine with the pre-rebase build; the v0.6.8 build
proves it in about 20 seconds on an M4 laptop).
