# amm — a constant-product pool of RAND and one token

Uniswap v2's market maker as an RPL-2 program. One cell holds the pool; the program's vault holds
its reserves; its liquidity shares are an RPL token only this program mints. The token it trades
is the program's **deploy-time public input** (one word, its asset index), so each pair is its own
program with its own id and vault.

```
key    [1, 0, 0, 0, 0, 0, 0, 0]
value  [rr_lo, rr_hi, rt_lo, rt_hi, s_lo, s_hi, lp, 1]      RAND reserve, token reserve, shares, share token, version
```

| method | private inputs | transition | rule |
|---|---|---|---|
| 1, add (first) | `[1]` | pool absent → pool; RAND and token in; mint shares | `s² ≤ in_r · in_t`; minted = `s − 1000` (1000 locked for ever) |
| 1, add | `[1]` | pool → pool; RAND and token in; mint shares | `minted · r ≤ in · s` on both sides |
| 2, remove | `[2]` | pool → pool; shares burned; pay RAND, pay token | `out · s ≤ burned · r` on both sides |
| 3, swap | `[3]` | pool → pool; RAND or token in; pay the other | `out · (1000 · r_in + 997 · in) ≤ 997 · in · r_out` |

**The written pool is always exactly the read pool moved by what came in and went out.** So a
caller only ever chooses amounts, and every amount meets an inequality that never lets the pool
lose: shares are never worth less after a deposit or a withdrawal, and `x · y` never falls on a
swap (0.30 % of what comes in stays in the pool).

**The program never divides.** It checks products — up to three amounts, in 256 bits (`kit`'s
`U256`) — and `plan` finds the best amount by bisection over the very same inequality, so the
wallet and the program can never disagree about rounding.

**The first deposit binds the share token.** It must mint, and the chain lets a program mint
only a token whose mint authority is that program, so a pool can only ever be created with a
share token the program really controls.

## How it is put together

This is the layout every DeFi example in `rpl2/` shares:

| path | |
|---|---|
| `core/src/lib.rs` | the rules: `check(source) → accept (eight output words) or refuse` — `no_std`, no panicking path, no division |
| `core/src/plan.rs` | the wallet's side: build the transitions `check` accepts, at the best amounts |
| `core/src/bin/plan.rs` | `plan`: the scripts' planner — cells in, `t.json` + private inputs out, checked against `check` first |
| `core/tests/rules.rs` | host tests: best amounts accepted, one unit more refused, and **no context word left unchecked** (every word of every accepted transition flipped, every flip refused) |
| `src/main.rs` | the guest: `check` behind the zkVM's syscalls; a refusal reads private input `u32::MAX` (no run, no proof) |
| `../kit` | what all the examples share: the context reader, 256-bit products, secrets, the host-side transition builder |

## Files

| file | |
|---|---|
| `build.sh` | build `image.bin` inside your circuits checkout, reject any panicking path, run the host tests |
| `run.sh` | every method on the emulator, accepted and refused, checked against the host rules |
| `deploy.sh <token>` | deploy for one token (its asset index is the public input); saves the id |
| `share-token.sh` | register the share token: `rand token create --program <id>` |
| `add.sh <RAND> <token units> [share asset]` | add liquidity; the first deposit names the share token |
| `remove.sh <shares>` | burn shares for RAND and the token |
| `swap.sh rand\|token <amount> [min out]` | swap; re-quotes on a stale read |
| `show.sh` | the pool's cell and the vault |

## Run it

```sh
./build.sh && ./run.sh
./deploy.sh 3                 # the pool for token 3
./share-token.sh              # say it prints index 4
./add.sh 5 20000000000 4      # 5 RAND and 20 tokens (9 decimals) create the pool
./swap.sh rand 1              # sell 1 RAND
./swap.sh token 3000000000    # sell 3 tokens
./remove.sh 1000000000
./show.sh
```

`run.sh`, off chain:

```
  ok    accept  first add: 5 RAND + 20 tokens creates the pool   [tier 12: fits (cycles 1801 of 4095, Poseidon2 permutations 334 of 512)]
  ok    refuse  first add minting one share too many
  ok    accept  add 1 RAND + 4 tokens   [tier 12: fits (cycles 2736 of 4095, Poseidon2 permutations 334 of 512)]
  ok    accept  swap 1 RAND for 3419751321 tokens   [tier 12: fits (cycles 3139 of 4095, Poseidon2 permutations 334 of 512)]
  ok    refuse  the same swap taking one unit more
  ok    accept  swap 3 tokens for 888243142 RAND units   [tier 12: fits (cycles 3135 of 4095, Poseidon2 permutations 334 of 512)]
  ok    refuse  a swap declaring a reserve the pool does not have
  ok    accept  remove half the shares for 3055878429 RAND units and 11790124339 tokens   [tier 12: fits (cycles 2664 of 4095, Poseidon2 permutations 335 of 512)]
  ok    refuse  the same removal with a third payout
  ok    refuse  an unknown method
10 steps agree with the host rules, 0 do not
```

The image is 1 270 words; every method proves at tier 12.

## What is public

The pool's reserves and supply (its cell), each deposit's and payout's amount, and each payout's
recipient key. Not who swapped or who provided liquidity: the bundle names nobody, and a share
is a shielded note like any other.

## Compared with durian.market

[durian.market](https://github.com/randprotocol/durian.market) is the full application: any number
of pools in one program, token-to-token swaps routed through RAND, a web front end. This example
is the same rules for a single pair, small enough to read in one sitting.

## On chain

Deployed and used on **chain 20**, the public testnet, on 2026-10-01, through the published RPC
`https://rpc.randprotocol.org`, with `rand` built from fullnode `5f872a58` (two deploy-config commits
past the v0.6.8 tag), by the scripts above exactly as written. Every proof was made on a laptop (an
M4 Max), the prover keeping about four cores busy on average. Fees are what the wallet paid; look a transaction up with `rand_getTransaction <hash>`, and
the cells and the vault as they are today with `rand program state <id>` / `rand program vault <id>`.

program id `032670a7febd9e8e78594a0706da44e7a9e72a993c045f750523871fab3f2a5b`

| step | transaction | tier | fee (RAND) | result |
|---|---|---|---|---|
| `rand token create --fixed-supply 1000000000000 …` (EXT, the traded token) | `d0fccfeab80cfb457000b82c22e391c8ddc4c8053adb3e1a4bebda2e6dd1771c` |  | 1.001 | token index 4 |
| `deploy.sh 4` | `ce504c8822d467c04c7fca79884b2d6a9c81d3def05c7cbcfd431e7b6feb5174` |  | 0.1281 | deployed |
| `share-token.sh ` | `1a1461680d52c9724d72d1fb9ae90493b26fc533fbfd63f0904c4db6b0d97d1b` |  | 1.001 | token index 5 |
| `add.sh 5 20000000000 5` | `c38f4999a6e0067168921772987b119a829008d8f37398f9135d0b87f5c44499` | 12 | 0.015672025 | outputs [1, 705032704, 1, 2820130816, 4, 0, 0, 0] |
| `swap.sh rand 1` | `82724e17892a2ba4602836f0a9895b094ecb6e4744bc1e77b9999a7c1b1857de` | 12 | 0.005931225 | outputs [3, 1000000000, 0, 3324995831, 0, 0, 0, 0] |
| `swap.sh token 3000000000` | `bf26c541466f6b7f3092721f2f05babd2349ced5ba31df99b8001cbc866f243c` | 12 | 0.005931225 | outputs [3, 3000000000, 0, 912539214, 0, 0, 0, 0] |
| `remove.sh 1000000000` | `b577c8fb5df8f781c55627c5af95c11d26bba7fb7318490a629330f85ddb050f` | 12 | 0.005801625 | outputs [2, 508746078, 0, 1967500416, 0, 0, 0, 0] |
