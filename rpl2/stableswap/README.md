# stableswap — a Curve-style StableSwap pool of RAND and one pegged token

Curve's StableSwap market maker, for two coins, as an RPL-2 program: the second AMM next to
[`amm/`](../amm/) (constant product), with the same shape. One cell holds the pool; the program's
vault holds its reserves; its liquidity shares are an RPL token only this program mints. The
program's **deploy-time public input** is two words — the traded token's asset index and the
amplification `A` (1 ≤ A ≤ 10 000; anything else and the program refuses every transition) — so
each pair and each `A` is its own program with its own id and vault.

It is for a token meant to trade at **one RAND** (a wrapped or bridged RAND, a RAND-backed
stablecoin). Near that peg it trades almost 1:1; far from it, it behaves like a constant-product
pool, so it never runs out of either side.

```
key    [1, 0, 0, 0, 0, 0, 0, 0]
value  [x_lo, x_hi, y_lo, y_hi, s_lo, s_hi, lp, 1]      RAND reserve, token reserve, shares, share token, version
```

Reserves are below 2^62, the share supply below 2^63.

## The invariant

For two coins, StableSwap's invariant `D` solves `4A(x + y) + D = 4A·D + D³ / (4xy)`. Multiplied
through by `4xy` and gathered on one side:

```
f(x, y, D) = 4xy · (4A(x + y) + D) − 16A · D · xy − D³
```

At `D = x + y` this is `−(x + y)(x − y)² ≤ 0`, and `f` falls as `D` grows (`∂f/∂D = 4xy(1 − 4A) − 3D²`,
negative for `A ≥ 1`). So for any reserves there is one real root, at most `x + y` (exactly
`x + y` when the pool is balanced), and the pool's `D` is its floor — **the largest integer with
`G(x, y, D)`**, where `G` is `f ≥ 0`:

```
G(x, y, D):   4xy · (4A(x + y) + D)  ≥  16A · D · xy + D³
```

`D` measures the pool's value in units of the peg. A large `A` makes the curve flat near
`x = y` (prices stay close to 1:1 until the pool is badly unbalanced); `A → 0` would be the
constant product. Every term is non-negative, and with `x, y < 2^62` and `A ≤ 10 000 < 2^14`
both sides are below 2^206: the program compares them exactly in `kit`'s 256-bit `U256`. (The
code computes the same inequality as `D ≤ x + y ∧ 16A·xy·(x + y − D) + 4xy·D ≥ D³`, which shares
work between `G(D)` and `G(D + 1)`; the comment on `Curve::holds` gives the bounds term by term.)

### Why `D` is declared and checked, not computed

Curve computes `D` by Newton's method, a loop of divisions. This program does neither: the
caller **declares `D`** as a private input and the program checks it is **exact** —
`G(D) ∧ ¬G(D + 1)` — a few 256-bit multiplications, no division, no loop.
The wallet (`plan`) finds `D` by bisection over the very same `G`, so the two can never disagree
about rounding.

Exactness matters. `G` falls in `D`, so a *smaller* declared `D` is *easier* to satisfy: if the
program only checked `G(D)`, a swapper could declare `D − 1` (or `D / 2`) and take more out of the
pool than the curve allows. `¬G(D + 1)` pins it to the one right value. The demo shows both
refused.

## Methods

| method | private inputs | transition | rule |
|---|---|---|---|
| 1, add (first) | `[1, 0, 0, D1 lo, hi]` | pool absent → pool; RAND and token in; mint shares | `D1` exact for the reserves; supply = `D1`; minted = `D1 − 1000` (1000 locked for ever); binds the share token (≠ RAND, ≠ the token) |
| 1, add | `[1, D0 lo, hi, D1 lo, hi]` | pool → pool; RAND and/or token in; mint shares | `D0` exact before, `D1` exact after; `m · (D0 + 1) · 10000 ≤ S · (D1 − D0 − 1) · 9996` |
| 2, remove | `[2]` | pool → pool; shares burned; pay RAND, pay token | `out · S ≤ burned · reserve` on both sides, RAND first (as amm) |
| 3, swap | `[3, D lo, hi, fee lo, hi]` | pool → pool; RAND or token in; pay the other | `D` exact before; `fee · 10000 ≥ in · 4`, `fee ≤ in`; `G(r_in + in − fee, r_out − out, D)` |

**The written pool is always exactly the read pool moved by what came in and went out.** The
fee is not taken out of anything: the swap check counts only `in − fee` of what came in, but the
pool keeps all of `in`, so `D` grows on every swap (the tests check it never falls). `plan` takes
the least fee, `⌈in · 4 / 10000⌉` (0.04 %), found as the smallest fee the rule accepts.

**Why every add pays a fee.** Adds may be one-sided, and shares are minted for the growth of
`D`. Without a fee, adding only RAND and then removing proportionally would hand back some RAND
and some token — a swap, at the curve's price, for free. So an add mints 0.04 % fewer shares than
its share of `D`'s growth (`9996 / 10000`), the same as the swap fee: add-one-side-then-remove is
never cheaper than swapping (`adding_one_side_then_removing_is_never_a_free_swap` in the tests).
Balanced adds pay it too; the rule does not try to tell them apart.

**Rounding against the depositor.** `D0` and `D1` are floors of the true invariants, so the rule
uses `D0 + 1` (an upper bound on the old value) and `D1 − D0 − 1` (a lower bound on the growth).
This is a refinement of the obvious `m · D0 ≤ S · (D1 − D0) · 0.9996`, which could round a tiny
add (a few thousand base units, where the fee is under one unit) in the depositor's favour.

**The first deposit binds the share token.** It must mint, and the chain lets a program mint only
a token whose mint authority is that program. It must bring both sides: a one-sided pool has
`D = 0`.

## Limits, stated plainly

- **Two coins, RAND and one token.** `n = 2` is built into the invariant's constants (`n^n = 4`).
- **`A` is fixed at deploy.** It is the public input: no ramping, no governance. A different `A`
  is a different program, with its own pool.
- **The peg is 1:1 in base units.** The token is assumed to have RAND's 9 decimals and to be worth
  one RAND. If it depegs, the pool still never loses — `D` never falls — but liquidity providers
  end up holding mostly the cheaper side, as in any Curve pool.
- **No oracle, no clock, no admin.** Nothing can pause it or change its fee.
- **Cost.** The exactness checks are 256-bit products: add and swap prove at tier 14, remove (the
  same rule as amm's) at tier 12.

## How it is put together

| path | |
|---|---|
| `core/src/lib.rs` | the rules: `check(source) → accept (eight output words) or refuse` — `no_std`, no panicking path, no division; `g`, `d_exact`, `swap_ok`, `add_ok`, `fee_covers` are public, and `plan` searches them |
| `core/src/plan.rs` | the wallet's side: `D` by bisection, the fee by `min_satisfying`, the best amounts by `max_satisfying` |
| `core/src/bin/plan.rs` | `plan`: the scripts' planner — cells in, `t.json` + private inputs out, checked against `check` first; and `demo` |
| `core/tests/rules.rs` | host tests: best amounts accepted, one unit more refused, `D` and the fee pinned, no context word left unchecked, `G` monotone and equal to its definition, `D` never falls on a swap, near-peg swaps against a constant product |
| `src/main.rs` | the guest: `check` behind the zkVM's syscalls; a refusal reads private input `u32::MAX` (no run, no proof) |
| `../kit` | what all the examples share |

The receipt's output words are `[method, p, q, D, 0]` with `p`, `q` and `D` as two words each:
for add, RAND in, token in and `D1`; for remove, RAND out and token out; for swap, in, out and
the declared `D`.

## Files

| file | |
|---|---|
| `build.sh` | build `image.bin` inside your circuits checkout, reject any panicking path, run the host tests |
| `run.sh` | every method on the emulator, accepted and refused, checked against the host rules |
| `deploy.sh <token> <A>` | deploy for one token and one `A` (public.txt: `token A`); saves the id |
| `share-token.sh` | register the share token: `rand token create --program <id>` |
| `add.sh <RAND> <token units> [share asset]` | add liquidity (either side may be 0 once the pool exists); the first deposit names the share token |
| `remove.sh <shares>` | burn shares for RAND and the token |
| `swap.sh rand\|token <amount> [min out]` | swap; re-quotes on a stale read |
| `show.sh` | the pool's cell, the vault, and the pool's `D` and a quote |

## Run it

```sh
./build.sh && ./run.sh
./deploy.sh 3 100             # the pool for token 3, A = 100
./share-token.sh              # say it prints index 4
./add.sh 1000 1000000000000 4 # 1000 RAND and 1000 tokens (9 decimals) create the pool
./add.sh 50 0                 # one-sided: 50 RAND
./swap.sh rand 100            # sell 100 RAND, ~99.9 tokens back
./swap.sh token 30000000000   # sell 30 tokens
./remove.sh 1000000000000
./show.sh
```

`run.sh`, off chain (token 5, `A = 100`, share token 6):

```
  ok    accept  first add: 1000 RAND + 1000 tokens, D = 2000000000000, mints 1999999999000   [tier 14: fits (cycles 5593 of 16383, Poseidon2 permutations 414 of 2048)]
  ok    refuse  first add minting one share too many
  ok    accept  balanced add: 100 RAND + 100 tokens mints 199919999998   [tier 14: fits (cycles 11842 of 16383, Poseidon2 permutations 414 of 2048)]
  ok    accept  one-sided add: 50 RAND, D 2200000000000 → 2249997234686, mints 49975418437 (fee-free would be 49995416604)   [tier 14: fits (cycles 11842 of 16383, Poseidon2 permutations 414 of 2048)]
  ok    refuse  the same add minting the fee-free amount
  ok    accept  swap 100 RAND for 99892894595 tokens (constant product, same reserves and fee: 87967614963)   [tier 14: fits (cycles 9212 of 16383, Poseidon2 permutations 414 of 2048)]
  ok    refuse  the same swap taking one unit more
  ok    refuse  the swap declaring D − 1 to take 99892894596 (1 more)
  ok    refuse  the swap declaring D / 2 to take 1094652011485
  ok    refuse  the swap declaring a fee of 39999999 instead of 40000000
  ok    accept  swap 30 tokens for 30017745519 RAND units   [tier 14: fits (cycles 9212 of 16383, Poseidon2 permutations 414 of 2048)]
  ok    refuse  a swap declaring a reserve the pool does not have
  ok    accept  remove half the shares for 609991127240 RAND units and 515053552702 tokens   [tier 12: fits (cycles 2758 of 4095, Poseidon2 permutations 414 of 512)]
  ok    refuse  the same removal with a third payout
  ok    refuse  a program deployed with A = 10001
  ok    refuse  an unknown method
16 steps agree with the host rules, 0 do not
```

Near the peg, 100 RAND into a pool of 1150 RAND and 1100 tokens buys 99.89 tokens; a constant-product pool of the
same reserves and fee would pay 87.97. Declaring `D − 1` would have let the same swap take one
unit more — and every swap after it another — and `D / 2` most of the pool; both are refused.

The image is 1 587 words; add and swap prove at tier 14, remove at tier 12.

## What is public

The pool's reserves and supply (its cell), `A`, each deposit's and payout's amount, and each
payout's recipient key; `D` is a private input, but anyone can recompute it from the cell. Not
who swapped or who provided liquidity: the bundle names nobody, and a share is a shielded note
like any other.
