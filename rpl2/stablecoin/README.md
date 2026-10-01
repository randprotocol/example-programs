# stablecoin — debt positions backed by RAND

MakerDAO's vaults (Liquity's troves) as an RPL-2 program. An owner locks RAND in the program's
vault and borrows the program's own stable token against it, up to a **minimum collateral ratio
of 150 %**; repaying burns the token. A position that falls **below 110 %** can be liquidated by
anyone who burns its whole debt, for all of its collateral. The price is set by an **operator**,
whose lock is the program's deploy-time public input, so each operator is its own program with its
own id and vault. The stable token is an RPL token only this program mints; the operator binds its
asset index once, with the first price.

```
config    key   [1, 0, 0, 0, 0, 0, 0, 0]
          value [p_lo, p_hi, stable, 0, 0, 0, 0, 1]       price, stable asset, version
position  key   [2, d0, …, d6]                            d = POSEIDON2([TAG_OWNER, secret])
          value [c_lo, c_hi, d_lo, d_hi, 0, 0, 0, 1]       collateral (RAND units), debt (stable units)
```

The price `P` is stable base units per whole RAND (10⁹ RAND units), so collateral `c` is worth
`c · P / 10⁹`. A position with nothing in it is the absent cell (all zeros): closing one deletes it.
Domain tags: `TAG_OPERATOR` = `"scop"` (`0x706f6373`), `TAG_OWNER` = `"scow"` (`0x776f6373`),
both little-endian ASCII.

| method | private inputs | transition | rule |
|---|---|---|---|
| 1, operate | `[1, s0..s7, stable, old_lo, old_hi, new_lo, new_hi]` | config → config; nothing in or out | `POSEIDON2([TAG_OPERATOR, s]) == lock`; first: config absent, `stable ≠ RAND`; later: same `stable`, read price `= old`; new price `> 0`, written exactly |
| 2, adjust | `[2, s0..s7]` | config → config (unchanged); position → position; RAND in (`burn_r` ≥ 0); ≤ 1 RAND pay; ≤ 1 stable mint; stable burned or nothing | the key is the secret's; `c' = c + in − out`, `d' = d + minted − burned` (no underflow); something changes; if `d'` rose or `c'` fell: `c'·P·100 ≥ d'·10⁹·150` |
| 3, liquidate | `[3]` | config → config (unchanged); position → deleted; burn exactly `d`; pay exactly `c` RAND | `c·P·100 < d·10⁹·110`; `c, d > 0` |

**The written position is always exactly the read one moved by what came in and went out**, and
the stable token minted less the stable token burned is exactly the change in debt. So the vault
holds exactly the sum of all collateral, and the stable supply is exactly the sum of all debt
(`core/tests/rules.rs` walks a position through prices and moves and checks both after each step).

**Borrowing or withdrawing needs 150 %; repaying and adding collateral never do.** An owner whose
position is under water can always repay or top up — but cannot take anything out or borrow more
until it is back at 150 %.

**The program never divides.** It checks `c'·P·100 ≥ d'·10⁹·150` — two products of three amounts
each, in 256 bits (`kit`'s `U256`; both sides below 2^133) — and `plan` finds the largest mint or
withdrawal by bisection over `healthy`, the very same predicate, so wallet and program never
disagree about rounding.

**Every word is pinned by an equality.** The config is written back exactly as read in adjust and
liquidate (rewriting a cell is free), and the operator's whole instruction — the token, the price
it replaces, the new price — is a private input the transition must match word for word. So
flipping any single word of any accepted transition is refused (`loose_words` in the tests).

## Trust model and limitations — read this

- **The operator is the price oracle, and is trusted completely.** A program sees only its own
  cells: no other program's state, no price feed, no chain data. So the price is whatever the
  operator's secret says it is. An operator who sets a false price can make every position
  liquidatable and liquidate them itself (or freeze borrowing with a price of 1). The operator
  cannot touch a position's collateral directly, cannot mint, and cannot rebind the stable token
  — but through the price, it can do almost as much harm. Use this with an operator you trust,
  or put the operator's secret behind something you do trust.
- **No clock.** A program cannot read the time, so there is **no staleness check** on the price
  (a price set a month ago is as good as one set now) and **no stability fee** or interest: a
  debt never grows. A real deployment would need both.
- **Liquidation is all or nothing.** Whoever burns a position's whole debt receives all of its
  collateral. Just under 110 % that is a premium of up to ~10 % over the debt; a position that
  fell far below 100 % before anyone acted is not worth liquidating, and its debt stays
  outstanding — there is no stability pool, no redistribution, no surplus buffer. The owner keeps
  nothing after a liquidation.
- **Losing `position.secret` loses the position.** It is the only key to it; there is no recovery.
  Losing `operator.secret` freezes the price for ever.
- **The stable token is bound by the operator's first price.** Anyone can register a token with
  `--program <id>`; the program trusts only the asset the operator bound, and pins every mint and
  burn to it.

## How it is put together

The layout every DeFi example in `rpl2/` shares (see [`amm/`](../amm/)):

| path | |
|---|---|
| `core/src/lib.rs` | the rules: `check(source) → accept (eight output words) or refuse` — `no_std`, no panicking path, no division |
| `core/src/plan.rs` | the wallet's side: build the transitions `check` accepts; the largest mint and withdrawal; the liquidation price |
| `core/src/bin/plan.rs` | `plan`: the scripts' planner — cells in, `t.json` + private inputs out, checked against `check` first (with the real Poseidon2) |
| `core/tests/rules.rs` | host tests: best amounts accepted, one unit more refused; no context word left unchecked; only the operator sets the price; only the owner moves a position; repay and top-up allowed under water; the books balance |
| `src/main.rs` | the guest: `check` behind the zkVM's syscalls; a refusal reads private input `u32::MAX` (no run, no proof) |
| `../kit` | what all the examples share: the context reader, 256-bit products, secrets, the host-side transition builder |

## Files

| file | |
|---|---|
| `build.sh` | build `image.bin` inside your circuits checkout, reject any panicking path, run the host tests |
| `run.sh` | every method on the emulator, accepted and refused, checked against the host rules |
| `operator.sh` | make `operator.secret` (mode 600) and `public.txt`, its lock — the program's public input |
| `deploy.sh` | deploy with `public.txt`; saves the id |
| `stable-token.sh` | register the stable token: `rand token create --program <id>` |
| `set-price.sh <P> [stable asset]` | the operator sets the price (stable units per RAND); the first time also binds the stable token |
| `adjust.sh [--deposit R] [--withdraw R\|max] [--mint u\|max] [--repay u\|all]` | open or adjust your position; makes `position.secret` on first use; re-plans on a stale read |
| `liquidate.sh <position key>` | liquidate a position below 110 % |
| `show.sh` | every cell, the vault, and your position's key |

`*.secret` and `public.txt` are in `.gitignore`. Keep the secrets.

## Run it

```sh
./build.sh && ./run.sh
./operator.sh && ./deploy.sh
./stable-token.sh                         # say it prints index 4
./set-price.sh 2000000000 4               # 1 RAND = 2.0 stable; binds token 4
./adjust.sh --deposit 10 --mint max       # lock 10 RAND, borrow 13.333333333
./adjust.sh --repay 3333333333            # repay 3.333333333
./adjust.sh --withdraw 2                  # take 2 RAND back (160 % after)
./show.sh                                 # your position's key: 02000000…
./set-price.sh 1300000000                 # the price falls: the position is at 104 %
./liquidate.sh 02000000…                  # anyone: burn 10.0 stable, take 8 RAND
```

`run.sh`, off chain:

```
  ok    accept  operator binds stable token 6 at price 2.0 stable per RAND   [tier 12: fits (cycles 1169 of 4095, Poseidon2 permutations 343 of 512)]
  ok    refuse  a non-operator setting the price to 5.0
  ok    accept  open: lock 10 RAND, mint 13333333333 stable units (the most at 150 %)   [tier 12: fits (cycles 3306 of 4095, Poseidon2 permutations 351 of 512)]
  ok    refuse  the same open minting one unit more
  ok    accept  repay 3333333333 stable units   [tier 12: fits (cycles 3295 of 4095, Poseidon2 permutations 350 of 512)]
  ok    refuse  the same repayment with a wrong owner secret
  ok    accept  withdraw 2 RAND   [tier 12: fits (cycles 3304 of 4095, Poseidon2 permutations 351 of 512)]
  ok    refuse  withdraw 2500000001 RAND units, one past 150 %
  ok    refuse  liquidate at price 2.0 (the position is at 160.00 %)
  ok    accept  operator drops the price to 1374999999   [tier 12: fits (cycles 1178 of 4095, Poseidon2 permutations 343 of 512)]
  ok    refuse  a liquidation taking one RAND unit more than the collateral
  ok    refuse  a liquidation burning one stable unit less than the debt
  ok    accept  liquidate: burn 10000000000 stable units, take 8000000000 RAND units (at 109.99 %)   [tier 12: fits (cycles 3113 of 4095, Poseidon2 permutations 346 of 512)]
  ok    refuse  an unknown method
14 steps agree with the host rules, 0 do not
```

The image is 1 278 words; every method proves at tier 12. With the eight-word lock as public
input, the context may be 111 words: adjust uses at most 81 (two reads, two writes, a pay and a
mint), liquidate 78, operate 43.

## What is public

The price and the stable token (the config cell), every position's collateral and debt (its
cell), each deposit's, payout's, mint's and burn's amount, and each payout's and mint's recipient
key. Not who owns a position: its key is a digest of a secret that never leaves the owner's
machine, and the bundle names nobody. A liquidation reveals who was paid the collateral (the
recipient's key), not who owned the position.

## On chain

Deployed and used on **chain 20**, the public testnet, on 2026-10-01, through the published RPC
`https://rpc.randprotocol.org`, with `rand` built from fullnode `5f872a58` (two deploy-config commits
past the v0.6.8 tag), by the scripts above exactly as written. Every proof was made on one CPU core of
a laptop. Fees are what the wallet paid; look a transaction up with `rand_getTransaction <hash>`, and
the cells and the vault as they are today with `rand program state <id>` / `rand program vault <id>`.

program id `89da2fc34aec789eb7401a83e9862cde1fe76df6852b6fc0c1490139deb415e3`

| step | transaction | tier | fee (RAND) | result |
|---|---|---|---|---|
| `deploy.sh ` | `d194d020958ba2ef90fe8b207c9163b396c85bf30e31428669f35e79be1177ce` |  | 0.1296 | deployed |
| `stable-token.sh ` | `09d5dc436e749a2c27799014d6cfcf8fa53e8338c48e990c450efdffcdb3f017` |  | 1.001 | token index 7 |
| `set-price.sh 2000000000 7` | `6cdeb9240261083cc4ec4265aa3b89b6ebf149ef0b69370c1d6a16748a5f3e51` | 12 | 0.015672025 | outputs [1, 2000000000, 0, 0, 0, 7, 0, 0] |
| `adjust.sh --deposit 3 --mint 2000000000` | `a071533e9a908986849c8f1e5c6389bab1e297498aaed5aeef67b28277da24a0` | 12 | 0.015931225 | outputs [2, 3000000000, 0, 2000000000, 0, 0, 0, 0] |
| `adjust.sh --repay all --withdraw max` | `f9f735c266eaa584424a0c17b24dbdfc6be1801f5098b46d7820e1159cc999cf` | 12 | 0.005931225 | outputs [2, 0, 0, 0, 0, 0, 0, 0] |
