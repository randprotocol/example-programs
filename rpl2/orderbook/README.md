# orderbook — escrowed limit orders with partial fills

An on-chain order book as an RPL-2 program. A maker **posts** an order: `give` units of one asset
go into the program's vault as escrow, and the order asks `want` units of another asset in all
(the price is `want / give`). Anyone **fills** any part of it, paying the asset it wants and
taking from the escrow, never below the maker's price. The maker **closes** it at any time,
collecting what the takers paid and whatever escrow is left. That one method is both a claim and
a cancel. One program, with no public input, holds every order for every pair of assets (RAND is
asset 0).

Each order is one cell, named by its **ticket**: eight random words the maker keeps in
`<name>.secret`. Their digest is the cell's key, so only the ticket's holder can close the order,
and any taker can fill it knowing only the key, which `rand program state` lists.

```
key    [1, d0, …, d6]        d = POSEIDON2([TAG_TICKET, ticket]),  TAG_TICKET = 0x6b636974 ("tick")
value  [give asset, want asset, give_rem lo, hi, want_rem lo, hi, proceeds lo, hi]
```

An order is live while its two assets differ, so an absent cell (eight zeros) never is.
`give_rem` is the escrow left, `want_rem` is what the maker still asks for it, and `proceeds` is
what takers have paid in that the maker has not yet collected.

| method | private inputs | transition | rule |
|---|---|---|---|
| 1, post (maker) | `[1, ticket, terms]` | order absent → order; `give` in (RAND by `burn_r`, or a token as a deposit) | the key is the ticket's; `give_rem` = what came in, of the asset that came in; `want_rem > 0`, of another asset; `proceeds` 0 |
| 2, fill (anyone) | `[2]` | order → order; `y` of the want asset in; pay `x` of the give asset | `0 < x ≤ give_rem`, `0 < y ≤ want_rem`, `x · want_rem ≤ y · give_rem`; `give_rem −= x`, `want_rem −= y`, `proceeds += y` |
| 3, close (maker) | `[3, ticket, terms]` | order → absent; pay `give_rem` of the give asset, then `proceeds` of the want asset, each only if nonzero (0, 1 or 2 payouts) | the key is the ticket's; exactly those amounts, in that order |

`terms` is `[give asset, want asset, want_rem lo, hi]`, the order as the maker states it: as
posted (post) or as read (close). It pins the words that nothing else checks. A new order's ask
is the maker's free choice. A close need not pay out both assets, and never pays anything of
`want_rem`. Every word a program is shown must still be checked (the tests flip each one).

## Why the proceeds wait in the cell

**This is the main RPL-2 lesson of this example.** An RPL-2 program decides *amounts*, never
*recipients*. Who receives each payout is fixed by the call binding of the transaction that
invokes the program, and that is not in the context the program sees. A fill is the *taker's*
transaction, so its payouts go wherever the taker says, and the program cannot tell where that
is. If the program tried to send the taker's payment to the maker, the taker could simply name
themselves as the recipient.

So a fill pays the taker only the escrow they bought, and the taker's payment stays in the
program's vault, counted in the order's `proceeds`. The maker collects it later in a transaction
of their own (`close`), proving the ticket. Any design here that pays someone other than the
invoker has to work this way: value meant for a third party is credited to a cell that only that
party's secret can open.

## Fills can never short the maker

The program checks the price inequality `x · want_rem ≤ y · give_rem` on the amounts the taker
declares. Both sides are products of two amounts below 2^63, so they stay below 2^126 in
256 bits. The program never divides: `plan` finds the most `x` a given `y` buys (or the least `y`
for a given `x`) by bisection over that same inequality.

**The maker gets exactly `want` for `give`.** Every fill keeps `proceeds + want_rem = want` and
`give_rem + (escrow paid out) = give`. A fill that empties the escrow (`x = give_rem`) needs
`y · give_rem ≥ give_rem · want_rem`, so `y ≥ want_rem`, and with `y ≤ want_rem` that means
`y = want_rem`. The escrow therefore runs out exactly when the ask does, at which point
`proceeds = want`.

A taker who overpays early (or rounds in the maker's favour) lowers `want_rem` by exactly the
overpayment, so later takers pay that much less in all. From `x · w ≤ y · g` it follows that
`(w − y) · g ≤ w · (g − x)`, so the remaining price `want_rem / give_rem` never rises. That is the
same as saying the maker has always been paid at least the posted price for whatever has been
filled. `core/tests/rules.rs` (`the_maker_gets_exactly_want_for_give`) checks all of this over
200 random fill sequences that include overpayments.

An order whose ask is met while escrow remains (because takers overpaid it to zero) takes no
more fills. Its maker closes it and gets back both the leftover escrow and `want`.

## Trust and limits, plainly

- **No matching, no order of priority.** A taker picks the order to fill. The program does not
  know other orders exist (it sees only the cells a transition declares), so it cannot enforce
  best price or time priority. Takers compare orders themselves (`show.sh`).
- **Fills race.** Two fills of one order in the same block cannot both apply. The second is a
  stale read (`rand` exits 3), and `fill.sh` quotes again. The new quote is never worse per unit,
  but less of the order may be left.
- **No expiry.** The program has no clock. An order stays until its maker closes it.
- **The ticket is the order.** Whoever holds `<name>.secret` collects the proceeds and the escrow.
  A lost ticket leaves both in the vault for ever. Nobody, not even the maker, can then close
  the order (takers can still fill what escrow is left).
- **Assets are taken at their word.** A maker can ask for an asset index that no one holds. Such
  an order is simply never filled, and the maker can close it.
- **Creating an order costs the chain's `cell_fee`** (0.01 RAND on the devnet). Closing deletes
  the cell for free.

## How it is put together

| path | |
|---|---|
| `core/src/lib.rs` | the rules: `check(source)` accepts (eight output words) or refuses. It is `no_std`, with no panicking path and no division; `price_ok` is the one inequality |
| `core/src/plan.rs` | the wallet's side: post, fill (`most_for` / `least_for` search `price_ok`), close |
| `core/src/bin/plan.rs` | `plan`: the scripts' planner. It takes cells in, writes `t.json` and the private inputs, and checks them against `check` first; `plan key` prints a ticket's order key |
| `core/tests/rules.rs` | host tests: best amounts accepted and one unit more refused; every close shape; **no context word left unchecked**; the maker's exact `want` |
| `src/main.rs` | the guest: `check` behind the zkVM's syscalls. A refusal reads private input `u32::MAX` (no run, no proof) |
| `../kit` | what all the examples share |

## Files

| file | |
|---|---|
| `build.sh` | build `image.bin` inside your circuits checkout, reject any panicking path, run the host tests |
| `run.sh` | every method on the emulator, accepted and refused, checked against the host rules |
| `deploy.sh` | deploy (no public input) and save the id |
| `post.sh <give asset> <give> <want asset> <want> [name]` | post an order (base units). Writes `<name>.secret` (the ticket, mode 600) and prints the order key |
| `fill.sh <order key hex> <pay units>` | pay up to what the order asks and take the most escrow that buys; re-quotes on a stale read |
| `close.sh <ticket file>` | close your order: escrow left and proceeds to this wallet |
| `show.sh` | every order (`rand program state`) and the vault |

## Run it

```sh
./build.sh && ./run.sh
./deploy.sh
./post.sh 0 10000000000 3 25000000000 sell-rand    # 10 RAND for 25 of token 3 (9 decimals); prints the key
./fill.sh <key> 10000000000                        # a taker pays 10 tokens, takes 4 RAND
./show.sh
./close.sh sell-rand.secret                        # the maker: 6 RAND back, 10 tokens collected
```

`run.sh`, off chain:

```
  ok    accept  post A: 10 RAND for 25 tokens   [tier 12: fits (cycles 883 of 4095, Poseidon2 permutations 299 of 512)]
  ok    accept  fill A: pay 10000000000 token units for 4000000000 RAND units   [tier 12: fits (cycles 1644 of 4095, Poseidon2 permutations 294 of 512)]
  ok    refuse  fill A: take 2 RAND for 4999999999 token units, one unit under the price
  ok    refuse  fill A: take one RAND unit more than the order holds
  ok    accept  fill A: pay the remaining 15000000000 token units for the remaining 6000000000 RAND units   [tier 12: fits (cycles 1642 of 4095, Poseidon2 permutations 294 of 512)]
  ok    refuse  fill A again, now complete: one unit for one unit
  ok    refuse  close A with another order's ticket
  ok    accept  close A: collect the 25000000000 token units paid in   [tier 12: fits (cycles 894 of 4095, Poseidon2 permutations 300 of 512)]
  ok    accept  post B: 30 tokens for 12 RAND   [tier 12: fits (cycles 881 of 4095, Poseidon2 permutations 299 of 512)]
  ok    accept  close B at once: the 30000000000 token units of escrow back   [tier 12: fits (cycles 890 of 4095, Poseidon2 permutations 300 of 512)]
  ok    accept  post C: 5 RAND for 10 tokens   [tier 12: fits (cycles 883 of 4095, Poseidon2 permutations 299 of 512)]
  ok    refuse  post into C's key while C is live
  ok    accept  fill C: take 2000000000 RAND units for 4000000000 token units   [tier 12: fits (cycles 1644 of 4095, Poseidon2 permutations 294 of 512)]
  ok    refuse  close C collecting one token unit more than was paid in
  ok    accept  close C: 3000000000 RAND units back and 4000000000 token units collected   [tier 12: fits (cycles 910 of 4095, Poseidon2 permutations 301 of 512)]
  ok    refuse  an unknown method
16 steps agree with the host rules, 0 do not
```

The image is 1 109 words, and every method proves at tier 12. The largest context, a close with
two payouts, is 49 words, well inside the 119 a program with no public input may use.

## What is public

Every order's terms and progress (its cell): the two assets, the escrow left, what it still asks,
and the uncollected proceeds. Each fill's amounts are public too. So are each payout's amount and
recipient key, as for any program payout.

Not public: who posted an order or who filled it, because the bundle names nobody and the ticket
never leaves the maker's machine. Only its digest appears, as the key. A maker's post and close
can be linked to each other through the order key, but not to a wallet.

## On chain

Deployed and used on **chain 20**, the public testnet, on 2026-10-01, through the published RPC
`https://rpc.randprotocol.org`, with `rand` built from fullnode `5f872a58` (two deploy-config commits
past the v0.6.8 tag), by the scripts above exactly as written. Every proof was made on one CPU core of
a laptop. Fees are what the wallet paid; look a transaction up with `rand_getTransaction <hash>`, and
the cells and the vault as they are today with `rand program state <id>` / `rand program vault <id>`.

program id `aaafb7a12afc59f90edaef5479c03e01e1e8d9914689da90f53af49dbd694b21`

| step | transaction | tier | fee (RAND) | result |
|---|---|---|---|---|
| `deploy.sh ` | `11c3bd0a5f0b3531aea75eab8a93cc81eff1917a68afe706562d3a3e703dab81` |  | 0.1119 | deployed |
| `post.sh 0 2000000000 4 4000000000 o1` | `4d37ea35b6d9d3a717156291df3dc031980f8fc2dbdd3395c1c8798d4303656c` | 12 | 0.015542425 | outputs [1, 2000000000, 0, 4000000000, 0, 0, 0, 0] |
| `fill.sh 010000003fd268052c5d4b4f1ff4bc907e216b68031169886e0b3c807141fa98 2000000000` | `f551e8befaf8f0d41c0a174e218a1f8ea2fa9760b9cb1dd647c55b16b9b991f9` | 12 | 0.005672025 | outputs [2, 1000000000, 0, 2000000000, 0, 0, 0, 0] |
| `close.sh o1.secret` | `4516348d8c2cbb1a084af962e91083aa4d2e35e64aa6dd95364cdca73885e551` | 12 | 0.005542425 | outputs [3, 1000000000, 0, 2000000000, 0, 0, 0, 0] |
