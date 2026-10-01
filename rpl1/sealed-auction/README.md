# sealed-auction — a sealed-bid second-price auction, cleared in a proof

> Bids are committed as private inputs before the close, the program computes a clearing price
> from all of them simultaneously, and the receipt publishes the winner and the price.
>
> Each dealer submits a quote as a private input, the program selects the winner and generates a
> settlement instruction…
>
> — [randprotocol.org/usecases](https://randprotocol.org/usecases/)

An RPL-1 program: stateless, run with `rand call`. The auctioneer collects sealed bids, runs the
program once over all of them, and the chain records a receipt naming the winner and the price —
the second-highest bid (Vickrey). The losing bids never reach the chain. With one public word
flipped the same program is a sealed **request for quote**: the lowest quote wins and is paid the
second-lowest.

```
public input (deploy)   mode: one word                 0 highest wins (auction), 1 lowest wins (RFQ)     mode.txt
private inputs (call)   [blind0, blind1, n, then n bids of 5 words: tag, bid_lo, bid_hi, salt0, salt1]    2 ≤ n ≤ 8
outputs (receipt)       [winner's tag, price_lo, price_hi, fold0, fold1, fold2, fold3, fold4]
```

| word | |
|---|---|
| `blind0`, `blind1` | two uniformly random words the scripts draw from `/dev/urandom` per call. A call's proof leaks an unsalted function of its input words, so small inputs could be brute-forced; the blind words make that impossible. Read by the program, never used, never output |
| `n` | the number of bids, 2 to 8; the program refuses any other count before its loops run |
| `tag` | who bid: a nonzero word the auctioneer gave the bidder when bidding opened, distinct per bidder. The receipt names the winner by it |
| `bid_lo`, `bid_hi` | the bid, a u64 little-endian, below 2^63 |
| `salt0`, `salt1` | 64 random bits the bidder chose and keeps: they make the bid's commitment unguessable |

**Clearing.** In mode 0 the winner is the highest bid and the price the highest of the others;
in mode 1 the winner is the lowest and the price the lowest of the others. Ties go to the first of
the tied bids, which then pays (is paid) its own bid. The mode is the deploy-time public input, so
it is part of the program id: an auction and an RFQ are two deployments of one image.

**Refused** — no proof at all: fewer than two or more than eight bids, a zero tag, two bids under
one tag, a bid of 2^63 or more, a mode other than 0 or 1.

## What the receipt binds

Each bid has a commitment, `c_i = POSEIDON2([TAG_BID, tag, bid_lo, bid_hi, salt0, salt1])`, and
the program folds the commitments in order — `acc_0 = 0`, `acc_i = POSEIDON2([TAG_FOLD, acc_{i−1},
c_i])`, seventeen words, the whole of both digests — and publishes the fold's first five words in
the receipt. Both messages have a fixed length and a domain tag of their own, since the sponge
does not pad.

The protocol around the program:

1. The auctioneer assigns tags, bidders send `tag bid salt` lines to the auctioneer by whatever
   private channel the sale uses.
2. The auctioneer runs `call.sh`. It prints the **commitment list** `c_1 … c_n` — the auctioneer
   publishes it — and proves the call. The receipt carries the winner, the price and the fold.
3. Each bidder runs `verify.sh <tag> <bid> <salt>` and finds their own commitment in the list;
   anyone runs `verify.sh --fold <list>` and gets the receipt's `out[3..8]`.

Both checks use `commit/`, a second guest built from the same `core/` code and run on the
emulator, never deployed: the commitment a bidder recomputes is made by exactly the hash the
program used. A bidder whose commitment is not in the list was left out; a list that does not
fold to the receipt is not the list the program cleared.

## Trust model — read this before using it

**The auctioneer sees every bid.** It runs the program, so the bids are its private inputs. What
the proof guarantees is narrower, and exact:

- For the set of bids whose commitments it published, the auctioneer **cannot misreport the
  winner or the price**: the receipt's words are the program's output over exactly those bids.
- It cannot add a bid after seeing the others without that bid's commitment appearing in the
  list; it cannot drop a bid without the bidder noticing their commitment is missing.
- Losing bids and their bidders' tags never reach the chain. The receipt names one tag and one
  price. The commitment list reveals nothing about the bids: a commitment is salted with 64 bits
  the bidder chose, so it cannot be opened by guessing bids.

What it does **not** achieve:

- Bids are **not hidden from the auctioneer**. A fully sealed auction — bids hidden from everyone
  until the close — needs a commit phase (bidders post their own commitments to RPL-2 cells
  before the close, then open to the auctioneer after it) or encrypted inputs. Out of scope here.
- It does not stop the auctioneer from **running the program over a set of bids it invented**:
  the list it publishes could omit every real bidder. The defence is social, not cryptographic —
  a bidder who is not in the list says so.
- Nothing is settled: no asset moves, no bidder is charged. The receipt is a verifiable record of
  the clearing, which a settlement (a transfer, an RPL-2 invoke) can then cite. The mode-1
  "settlement instruction" is the receipt itself: pay tag `out[0]` the amount `out[1..3]`.
- The fold is truncated to 160 bits in the receipt. Finding a second list with the same five
  words is a 2^160 preimage search; it is not a practical concern.

## Files

| file | |
|---|---|
| `src/main.rs`, `src/chain.rs` | the guest: the syscalls behind `core`'s `Source`, acceptance and refusal |
| `core/src/lib.rs` | the rules — commitment, fold, `clear`, `check` — `no_std` on the guest, tested on the host |
| `core/src/host.rs` | the host side: a `Mock` source for the tests, the bids file, `commit/` on the emulator |
| `core/src/bin/auction.rs` | `auction words\|show\|commit\|fold`: the tool the scripts call |
| `core/tests/rules.rs` | the host tests: both modes, ties, every refusal, the receipt's binding |
| `commit/` | the off-chain helper guest (never deployed): one bid's commitment, or the fold of a list |
| `mode.txt` | the deploy-time public input: `0` auction, `1` request for quote |
| `demo-bids.txt` | four demo bids, `tag bid salt` per line. Real bids go in a `*.secret` file (gitignored) |
| `build.sh` | build both images inside your circuits checkout, reject any panicking path, run the tests |
| `run.sh [bids file]` | emulator: the bids in both modes, then three refusals |
| `deploy.sh` | deploy with `mode.txt` as the public input; saves the id to `program.id` |
| `call.sh <bids file>` | print the commitment list and the expected receipt, then prove the call on chain |
| `verify.sh <tag> <bid> <salt>`, `verify.sh --fold <list>` | a bidder's commitment; a published list's fold |

## Run it

```sh
export CIRCUITS=~/src/circuits            # see the top-level README for the rest
./build.sh
./run.sh
./deploy.sh                               # mode.txt = 0; set it to 1 and redeploy for an RFQ
./call.sh round1.secret
./verify.sh 7 500 1234567890123           # a bidder
./verify.sh --fold published.txt          # anyone
```

`run.sh`, off chain (the blind words differ each run; the outputs do not):

```
== demo-bids.txt, mode 0 (public):
mode 0: highest wins, pays the second-highest (auction)
4 bids; commitments, in order (publish this list):
  1  75f046028c494840a41af9f00a48430aa4eba91444aa00c576c8aa387d0490e1
  2  4200b3419bd404cb514cc8e2812456cb7828727f7e7ac84ed6aabff94c9ee1c7
  3  e3d6f8291cf15a74ad868bff3d6eec37deb9d47109348eac2255dcd9ae55b09b
  4  16a2872bc82ccdb979d4f8a6b222f7103c3d90940f130282199fc073ecf45fdb
expected receipt: winner tag 8, price 700
  out[0..3] = 8 700 0
  out[3..8] = 2924312494 3347677334 3360650506 2056117393 3726953813
out[0] = 8
out[1] = 700
out[2] = 0
out[3] = 2924312494
out[4] = 3347677334
out[5] = 3360650506
out[6] = 2056117393
out[7] = 3726953813
cycles 882
tier 10
tier 12: fits (cycles 967 of 4095, Poseidon2 permutations 113 of 512)

== demo-bids.txt, mode 1 (public):
mode 1: lowest wins, paid the second-lowest (request for quote)
4 bids; commitments, in order (publish this list):
  1  75f046028c494840a41af9f00a48430aa4eba91444aa00c576c8aa387d0490e1
  2  4200b3419bd404cb514cc8e2812456cb7828727f7e7ac84ed6aabff94c9ee1c7
  3  e3d6f8291cf15a74ad868bff3d6eec37deb9d47109348eac2255dcd9ae55b09b
  4  16a2872bc82ccdb979d4f8a6b222f7103c3d90940f130282199fc073ecf45fdb
expected receipt: winner tag 10, price 500
  out[0..3] = 10 500 0
  out[3..8] = 2924312494 3347677334 3360650506 2056117393 3726953813
out[0] = 10
out[1] = 500
out[2] = 0
out[3] = 2924312494
out[4] = 3347677334
out[5] = 3360650506
out[6] = 2056117393
out[7] = 3726953813
cycles 875
tier 10
tier 12: fits (cycles 960 of 4095, Poseidon2 permutations 113 of 512)

== one bid (refused):
trap: InputIndex(4294967295)

== two bids under tag 7 (refused):
trap: InputIndex(4294967295)

== a bid of 2^63 (refused):
trap: InputIndex(4294967295)
```

The same four bids clear to different receipts in the two modes — bidder 8 wins the auction at
700, bidder 10 wins the RFQ at 500 — while `out[3..8]`, the fold, is the same: it binds the bids,
not the mode. The expected receipt the tool prints is the receipt the emulator produces.

The image is **308 words**. Two to four bids fit **tier 10** (four bids: 967 of 1023 cycles);
five to eight bids need **tier 12** (eight bids: 1697 of 4095 cycles, 146 Poseidon2 permutations
of 512). `run.sh` asks the emulator about tier 12 so any bids file up to eight fits; the `tier 10`
line above it is the smallest tier the run fits, and the wallet proves at that one.

## On chain

Deployed and used on **chain 20**, the public testnet, on 2026-10-01, through the published RPC
`https://rpc.randprotocol.org`, with `rand` built from fullnode `5f872a58` (two deploy-config commits
past the v0.6.8 tag), by the scripts above exactly as written. Every proof was made on a laptop (an
M4 Max), the prover keeping about four cores busy on average. Fees are what the wallet paid; look a transaction up with `rand_getTransaction <hash>`, and
the cells and the vault as they are today with `rand program state <id>` / `rand program vault <id>`.

program id `72b753e655aca25bafecbcd632f9808b22a913288480c7353098471bbbca3d56`

| step | transaction | tier | fee (RAND) | result |
|---|---|---|---|---|
| `deploy.sh ` | `a72e2c3849f867ccf68397c1581a7bf0a23d3f516327b03d363f5ac5dd888312` |  | 0.0319 | deployed |
| `call.sh demo-bids.txt` | `d6afbeb2163456d8da1826add29e367cbafc4c71f33cfaf4b91cff03a319f0a2` | 10 | 0.00557571 | outputs [8, 700, 0, 2924312494, 3347677334, 3360650506, 2056117393, 3726953813] |
