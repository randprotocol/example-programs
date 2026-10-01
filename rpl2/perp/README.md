# perp — a perpetual-futures market priced by an operator

A GMX-style perpetual DEX as an RPL-2 program, cut to its core: **one market** on an index asset
X, priced in RAND by an **operator**; margin, settlement and liquidity all in RAND; and a pool of
**liquidity providers** who are every trader's counterparty. LPs deposit RAND for the program's own
LP token; traders post margin and go long or short at the operator's price; when a position
closes, the pool pays its profit or keeps its loss. Everything sits in the program's vault.

The operator's **lock** — `POSEIDON2([TAG_OPERATOR, secret])`, eight words — is the program's
deploy-time public input, so it is part of the program id: each operator is its own program.

**Not production.** Read "Trust and limits" before you put anything in it.

## Cells

`E = 10^9`. A price `P` is RAND base units per `E` base units of X (so `P = 2·10^9` is "X at
2 RAND"); a position's `cost` is its notional at entry, in RAND base units.

```
market    key [1, 0, 0, 0, 0, 0, 0, 0]
          [P_lo, P_hi, cash_lo, cash_hi, supply_lo, supply_hi, lp, 1]
          price · the pool's RAND · LP tokens outstanding · the LP token · version
oi        key [2, 0, 0, 0, 0, 0, 0, 0]
          [Lq_lo, Lq_hi, Lc_lo, Lc_hi, Sq_lo, Sq_hi, Sc_lo, Sc_hi]
          open interest: long size and cost, short size and cost — absent is all zeros, an empty book
position  key [3, d0, …, d6]      d = POSEIDON2([TAG_OWNER, secret])
          [margin_lo, margin_hi, q_lo, q_hi, cost_lo, cost_hi, side, 1]
          side 1 long, 2 short · version
```

**The price and the pool share a cell** so that a close fits the segment. A close needs the price,
the pool, the open interest and the position, and pays: three reads, three writes and one payout
are `11 + 16·6 + 3 = 110` context words, and with the 8-word lock and the 8 binding words the
segment's cap leaves 111. A separate price cell would not fit.

Tags (`POSEIDON2` domains, four ASCII characters read little-endian): `"popr"` = `0x72706f70`
for the operator's lock, `"pown"` = `0x6e776f70` for a position owner's digest.

## Methods

| method | private inputs | cells read → written | in / out | rule |
|---|---|---|---|---|
| 1, operate | `[1, op secret, P, lp, old P]` | market → market | — | the operator's secret opens the lock. Market absent: create it (`P > 0`, cash = supply = 0, `lp ≠ RAND`). Live: set `P > 0`, everything else unchanged |
| 2, lp_add | `[2]` | market, oi → market, oi | RAND `a` in; mint `m` LP | `S = 0` ⇒ `m = a`; else `NAV9 > 0` and `m · NAV9 ≤ a · S · E` |
| 3, lp_remove | `[3]` | market, oi → market, oi | burn `b` LP; pay `x` RAND | `x · S · E ≤ b · NAV9`; reserve after: `cash − x ≥ Lc + Sc` |
| 4, open | `[4, secret]` | market, oi, position (absent) → market, oi, position | RAND margin in | `q, cost > 0`; long `cost · E ≥ q · P`, short `cost · E ≤ q · P`; `cost ≤ 10 · margin`; reserve after: `cash ≥ Lc' + Sc'` |
| 5, close | `[5, secret]` | market, oi, position → market, oi, (deleted) | pay `x` RAND (or nothing) | `x · E ≤ value9`; `cash' = cash + margin − x` |
| 6, liquidate | `[6]` | market, oi, position → market, oi, (deleted) | pay `x` RAND (or nothing) | `value9 · 100 < cost · E · 5`; `x · E ≤ value9`, `x · 100 ≤ cost`; `cash' = cash + margin − x` |

where

- `NAV9 = (cash + Lc) · E + Sq · P − Sc · E − Lq · P` — the pool less the traders' aggregate
  profit, times `E` (computed as a positive part minus a negative part, in 256 bits);
- `pnl9 = q · P − cost · E` for a long, `cost · E − q · P` for a short, **capped at
  `+cost · E`**; `value9 = max(0, margin · E + pnl9)`.

**Opening rounds against the trader**: a long pays at least the price for its size, a short sells
at no more than it, so a position never starts in profit. `plan` picks the most X a long's cost
buys and the least X a short's cost sells.

**Every word is pinned by an equality.** The cells written are exactly the cells read moved by what
came in and went out: the open interest moves by exactly the position's size and cost on its side;
the pool's cash by exactly `a`, `−x` or `margin − x`; the supply by exactly the LP minted or
burned. A cell a method reads but does not change (the market on open, the open interest on
lp_add / lp_remove) is **written back unchanged** — rewriting a cell is free, and reading it
already contends with every other writer — so even its unused words are pinned. The operator's
price, LP token and the price it replaces (a compare-and-set) are in its private inputs, for the
same reason. A caller only ever chooses amounts, and every amount meets an inequality the program
checks with multiplications alone; `plan` finds the best amount by bisection over the same
predicate (`add_ok`, `remove_ok`, `entry_ok`, `payout_ok`, `reward_ok` in `core/src/lib.rs`).

**The LP token is bound at creation**: the market cell names it, every mint and burn must be it,
and the chain lets a program mint and burn only a token whose mint authority is that program.

## Why the vault always covers every payout

The vault holds `cash + Σ margin` (plus anything sent to it otherwise). Two rules make that enough:

1. **The profit cap.** No position is ever paid more than `margin + cost`.
2. **The reserve.** `cash ≥ Lc + Sc` — the pool's cash covers every open position's notional.
   Opening and LP withdrawals check it. A price move changes neither side. A close or a
   liquidation pays at most `margin + cost` and takes `margin` in, so `cash` falls by at most
   `cost`, and `Lc + Sc` falls by exactly `cost`: the reserve still holds.

So every open position closing at once is paid at most `Σ margin + Σ cost ≤ Σ margin + cash`,
the vault. `core/tests/rules.rs` checks this after every step of a 3 000-step random sequence of
price moves (±30 %), opens at random leverage, closes, liquidation sweeps and LP deposits and
withdrawals, and then closes every position.

## Trust and limits

- **The operator is the oracle.** It sets the price, alone, whenever it likes; whoever holds
  `operator.secret` decides every position's profit and loss and who is liquidatable. The program
  bounds what that can cost the pool (the cap and the reserve hold at any price), not what it can
  cost a trader or an LP. There is no second source, no delay, no bound on a move.
- **No funding rate, no borrow fee, no price impact.** The program has no clock, so nothing accrues
  over time; it is priced by the oracle, so a trade's size does not move its price. An imbalanced
  book costs one side nothing to hold.
- **Per-position profit is capped at 100 % of notional** (a long that more than doubles is paid as
  if it had doubled). A short's profit is at most its notional anyway.
- **The LP NAV uses the aggregate, uncapped PnL.** Ignoring the cap overstates what traders are
  owed, which is conservative for withdrawals (and slightly generous to depositors). It also
  counts a losing position's loss in full even beyond its margin — bad debt the pool cannot
  collect — which is optimistic until that position is liquidated. Either way it only moves value
  between LPs; it cannot break the reserve, which is checked on cash. A NAV at or below zero
  refuses both deposits and withdrawals until the price or the book changes.
- **LPs can be locked in**: cash backing open notional cannot be withdrawn until those positions
  close. If every LP leaves, any cash left over goes to the next depositor (minted one for one).
- **Liquidation needs a keeper.** Nobody is paid to watch; a position that goes deep under water
  before anyone liquidates it is closed for nothing, and its loss beyond its margin is not
  recovered (the reserve still covers every payout).
- One position per secret; no adding to or partly closing a position.
- **Not production**: no audit, one market, an operator you must trust.

## How it is put together

| path | |
|---|---|
| `core/src/lib.rs` | the rules: `check(source) → accept (eight output words) or refuse` — `no_std`, no panicking path, no division |
| `core/src/plan.rs` | the wallet's side: build the transitions `check` accepts, at the best amounts |
| `core/src/bin/plan.rs` | `plan`: the scripts' planner — cells in, `t.json` + private inputs out, checked against `check` (with the real Poseidon2) first; and `demo` |
| `core/tests/rules.rs` | host tests: best amounts accepted, one unit more refused, **no context word left unchecked** for every accepted kind, the profit cap, the reserve, ownership, and the solvency run |
| `src/main.rs` | the guest: `check` behind the zkVM's syscalls; a refusal reads private input `u32::MAX` (no run, no proof) |
| `../kit` | what all the examples share: the context reader, 256-bit products, secrets, the host-side transition builder |

## Files

| file | |
|---|---|
| `build.sh` | build `image.bin` inside your circuits checkout, reject any panicking path, run the host tests |
| `run.sh` | every method on the emulator, accepted and refused, checked against the host rules |
| `operator.sh` | make `operator.secret` (mode 600) and its lock, `public.txt` |
| `deploy.sh` | deploy with `public.txt` as the public input; saves the id |
| `lp-token.sh` | register the LP token: `rand token create --program <id>` |
| `price.sh <RAND per X> [LP asset]` | the operator: create the market (naming the LP token) or move the price |
| `lp-add.sh <RAND>` | provide liquidity for LP tokens |
| `lp-remove.sh <LP units>` | burn LP tokens for RAND |
| `open.sh <name> long\|short <margin RAND> <notional RAND>` | open a position; its key is `<name>.secret`, made here |
| `close.sh <name>` | close it at the best payout |
| `liquidate.sh <position key hex>` | liquidate someone's position for the best reward |
| `show.sh` | every cell and the vault |

Every method script re-plans from fresh cells and retries when the chain refuses a stale read
(exit 3): the market and the open interest are shared by everyone. **Keep your `*.secret`
files**: a position's secret is the only way to close it, and `operator.secret` the only way to
move the price. They are in `.gitignore`.

## Run it

```sh
./build.sh && ./run.sh
./operator.sh && ./deploy.sh
./lp-token.sh                 # say it prints index 4
./price.sh 2 4                # create the market: X at 2 RAND, LP token 4
./lp-add.sh 1000
./open.sh alice long 10 100   # 10x
./price.sh 2.5
./close.sh alice              # 35 RAND back
./show.sh
```

`run.sh`, off chain:

```
  ok    accept  operator creates the market: X at 2 RAND   [tier 14: fits (cycles 1655 of 16383, Poseidon2 permutations 786 of 2048)]
  ok    accept  LP adds 1000 RAND for 1000000000000 LP units   [tier 14: fits (cycles 2737 of 16383, Poseidon2 permutations 789 of 2048)]
  ok    refuse  the same deposit minting one LP unit more
  ok    accept  Alice opens a 10x long: 10 RAND margin, 100 RAND notional, 50000000000 units of X   [tier 14: fits (cycles 5183 of 16383, Poseidon2 permutations 801 of 2048)]
  ok    refuse  Bob opens an 11x long (110 RAND on 10)
  ok    accept  operator moves the price: X at 2.5 RAND   [tier 14: fits (cycles 1719 of 16383, Poseidon2 permutations 786 of 2048)]
  ok    refuse  a price update that also moves the pool's cash
  ok    refuse  the same price update with Bob's secret, not the operator's
  ok    accept  Alice closes her long for 35000000000 RAND units (10 margin + 25 profit)   [tier 14: fits (cycles 5822 of 16383, Poseidon2 permutations 802 of 2048)]
  ok    refuse  the same close taking one unit more
  ok    accept  Carol opens a 10x short at 2.5: 40000000000 units of X   [tier 14: fits (cycles 5181 of 16383, Poseidon2 permutations 801 of 2048)]
  ok    accept  Dave opens a 2x short: 50 RAND margin, 100 RAND notional   [tier 14: fits (cycles 5181 of 16383, Poseidon2 permutations 801 of 2048)]
  ok    accept  operator moves the price: X at 2.65 RAND   [tier 14: fits (cycles 1719 of 16383, Poseidon2 permutations 786 of 2048)]
  ok    accept  a keeper liquidates Carol's short for a 1000000000-unit reward (1 % of notional)   [tier 14: fits (cycles 9790 of 16383, Poseidon2 permutations 797 of 2048)]
  ok    refuse  the same liquidation taking one unit more
  ok    refuse  a keeper liquidating Dave's healthy short
  ok    accept  a second LP adds 100 RAND for 101010101010 LP units   [tier 14: fits (cycles 7077 of 16383, Poseidon2 permutations 789 of 2048)]
  ok    accept  an LP burns 500 LP units for 495000000000 RAND units   [tier 14: fits (cycles 7589 of 16383, Poseidon2 permutations 789 of 2048)]
  ok    refuse  burning 556 LP units for 550440000000, leaving 38560000000 of cash behind Dave's 100 RAND notional
  ok    refuse  Dave's position closed with Bob's secret
  ok    accept  Dave closes his short for 44000000000 RAND units   [tier 14: fits (cycles 5427 of 16383, Poseidon2 permutations 802 of 2048)]
  ok    refuse  an unknown method
22 steps agree with the host rules, 0 do not
```

The image is 3 051 words; every method proves at **tier 14** (tier 12's 512 permutations are not
enough to hash a program this size, and a liquidation's 256-bit arithmetic takes ~9 800 cycles).

## What is public

The price, the pool, the open interest and every position's margin, size, cost and side (its
cells); each deposit's and payout's amount and each payout's recipient key. Not who holds a
position or who provides liquidity: a position is named by a secret's digest, the bundle names
nobody, and an LP token is a shielded note like any other. Who liquidated whom is not public
either; that a position was liquidated, and for how much, is.

## On chain

Deployed and used on **chain 20**, the public testnet, on 2026-10-01, through the published RPC
`https://rpc.randprotocol.org`, with `rand` built from fullnode `5f872a58` (two deploy-config commits
past the v0.6.8 tag), by the scripts above exactly as written. Every proof was made on a laptop (an
M4 Max), the prover keeping about four cores busy on average. Fees are what the wallet paid; look a transaction up with `rand_getTransaction <hash>`, and
the cells and the vault as they are today with `rand program state <id>` / `rand program vault <id>`.

program id `bdf8abe3b0ba951b21c8642507f09b1a7fc07cfb17cbaf6596ac00af90797657`

| step | transaction | tier | fee (RAND) | result |
|---|---|---|---|---|
| `deploy.sh ` | `563cac42df743374d2bd4be47fbaa95dba1112ffcbdb52b636550254d1628a1e` |  | 0.3069 | deployed |
| `lp-token.sh ` | `b240bd6076ed0013fd6075ef2fdafb0dd50a11df789de4761b363416f85a34cb` |  | 1.001 | token index 10 |
| `price.sh 1000000000 10` | `477eab4cad5e980b42a5138d78fc1556e815cb2cc853943b6bb61a15ca86e2bc` | 14 | 0.015931225 | outputs [1, 2808348672, 232830643, 0, 0, 0, 0, 0] |
| `lp-add.sh 10` | `8a8bd3a2f3ede524587efafb74dcbc00658db420a6a57693abc12175b1ba26ea` | 14 | 0.005931225 | outputs [2, 1410065408, 2, 1410065408, 2, 0, 0, 0] |
| `open.sh p1 long 1 5` | `94c7ed727c7c696763708b4b1e2394cfcf33f1b1bffac8c2b3ffa1ed0d595bbb` | 14 | 0.026449625 | outputs [4, 1000000000, 0, 5, 0, 705032704, 1, 1] |
| `price.sh 1100000000` | `d2a69c46e1400d97a2ab5569bea3cd8dd312b28c4a989b642f293225fd9d35a0` | 14 | 0.005931225 | outputs [1, 82706432, 256113708, 0, 0, 0, 0, 0] |
| `close.sh p1` | `e45abf377076d2b852c4ac44569807cb6b1fc96680d265a99f7149b7b7b20cf9` | 14 | 0.006449625 | outputs [5, 1500000000, 0, 1000000000, 0, 705032704, 1, 1] |
| `lp-remove.sh 1000000000` | `3b555e4acfd8424ea7fc49f123d6668e1cade5f390bd5eb710ac75edd527c18c` | 14 | 0.006449625 | outputs [3, 1000000000, 0, 950000000, 0, 0, 0, 0] |
