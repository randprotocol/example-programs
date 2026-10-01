# lending — lend RAND for shares, borrow RAND against a token

A Compound/Aave-style money market as an RPL-2 program. Lenders supply RAND and receive shares,
an RPL token only this program mints. Borrowers post a collateral token `C` and borrow RAND
against it, up to 75 % of its value. Anyone may liquidate a position past 85 % and gets a 10 %
bonus for doing it. An operator sets `C`'s price and accrues interest.

The program's **deploy-time public input** is nine words: the operator's lock
(`POSEIDON2([TAG_OPERATOR, s0..s7])`) and `C`'s asset index. Each operator and collateral pair is
its own program, with its own id and vault.

Amounts are base units. `E = 10⁹` scales the price and the borrow index.

```
price     [1, 0, 0, 0, 0, 0, 0, 0]  [p_lo, p_hi, 0, 0, 0, 0, 0, 1]                  P: RAND units per E units of C
pool      [2, 0, 0, 0, 0, 0, 0, 0]  [cash_lo, cash_hi, sb_lo, sb_hi, s_lo, s_hi, i_lo, i_hi]
position  [3, d0, …, d6]            [coll_lo, coll_hi, sd_lo, sd_hi, 0, 0, 0, 1]     d = POSEIDON2([TAG_OWNER, secret])
shares    [4, 0, 0, 0, 0, 0, 0, 0]  [share, 0, 0, 0, 0, 0, 0, 1]                    the share token, bound at init
```

The pool cell holds four numbers:

- `cash`, the RAND the pool holds.
- `sb`, the scaled borrows.
- `s`, the share supply.
- `I`, the borrow index. It is scaled by `E`, starts at `E` and never falls.

A position whose scaled debt is `sd` owes `sd · I / E` RAND. The pool's total assets, times `E`,
are `TA9 = cash · E + sb · I`, and a share is worth `TA9 / (E · s)`.

| method | private inputs | transition | rule |
|---|---|---|---|
| 1, operate (init) | `[1, s, p_lo, p_hi, share]` | pool absent → price, pool `[0, 0, 0, E]`, shares | operator's secret; `C`, the share token and RAND all different; `P > 0` |
| 1, operate (update) | `[1, s, p_lo, p_hi, rate]` | pool → price (written blind), pool | operator's secret; `P > 0`; `rate ≤ E/100`; `I'·E ≤ I·(E + rate) < I'·E + E` |
| 2, supply | `[2]` | RAND in; mint shares | first supply: `m + 1000 = a` (1000 shares locked for ever); after that: `m · TA9 ≤ a · s · E` |
| 3, withdraw | `[3]` | shares burned; pay RAND | `x · s · E ≤ b · TA9`, and `x ≤ cash` |
| 4, adjust | `[4, s0..s7]` | C in (optional), RAND in (optional), at most one payout (RAND or C) | `sd'·I + r·E ≥ sd·I + x·E`; if debt rose or collateral fell, `sd'·I·100 ≤ coll'·P·75` |
| 5, liquidate | `[5]` | RAND in; pay C | `sd·I·100 > coll·P·85`; `sd'·I + r·E ≥ sd·I`; `c·P·100 ≤ (sd − sd')·I·110` |

**What each method reads and writes.** In every method the written cells are exactly the read
cells moved by what came in and went out. Each cell a method reads, it also writes, either moved
or unchanged. So every word the program is shown is pinned by an equality. The inequalities only
decide whether an amount is allowed, so no word can be nudged and still pass.

The operator's choices (the new price, the share token, the rate) are its private inputs, and
the words it writes must match them. The tests flip every word of every accepted transition and
require each flip to be refused.

**The program never divides.** It checks products of at most three amounts in 256 bits.
`plan` finds each best amount by bisection over the same predicates (`supply_ok`, `withdraw_ok`,
`debt_covered`, `healthy`, `liquidatable`, `seize_ok`, `accrue_ok`). Rounding always favours the
pool:

- A borrow's scaled debt is rounded up.
- A repayment's credit is rounded down.
- Shares minted and RAND paid out are rounded down.

**A share is never worth less.** Supply and withdraw never dilute. A borrow adds at least as much
`sb · I` as it takes in `cash`, and a repayment or a liquidation adds at least as much `cash` as
it takes in `sb · I`. Accrual only raises `I`. `core/tests/rules.rs` checks `TA9 / s` across 600
random operations of every kind.

**A position can only get riskier while it stays healthy.** An adjustment that adds debt or
removes collateral must end within 75 % LTV. A test sends thousands of arbitrary declared
outcomes and checks that every accepted one passes that check.

## Trust model and limits

- **The operator is the price oracle.** Its price is taken as given. A wrong or malicious price
  can make healthy positions liquidatable, or let borrowers take out more than their collateral
  is worth. The operator cannot take funds directly, and cannot change `C` (it is in the program
  id) or the share token (bound at init).
- **There is no clock.** A program cannot see time, so interest is whatever the operator accrues.
  The operator raises `I` by at most 1 % per update, by a rate it names, rounded down. This stands
  in honestly for a clock, but nothing stops several updates in a row. Lenders trust the operator
  to accrue at all, and borrowers trust it not to accrue too fast.
- **Liquidation is partial and has no close factor.** A liquidator chooses how much to repay and
  gets collateral worth up to 110 % of the debt it clears. When a position is deeply underwater
  (debt above about 91 % of its collateral's value), each liquidation makes the rest of it worse.
  Debt left without collateral stays in `sb`. **Bad debt is not socialised**: it overstates `TA9`
  until someone repays it, and lenders who withdraw first are paid in full.
- **Withdrawals are limited to `cash`.** RAND that is lent out comes back only as borrowers repay
  or are liquidated. There is no interest-rate curve that rises with utilisation. The rate is the
  operator's choice.
- **One payout per adjustment.** Borrowing RAND and taking collateral out are separate invokes.
  The segment rule leaves room for one payout with three cells read and three written
  (9 + 8 + 110 = 127 words).
- **Contention.** Every method touches the pool cell, so concurrent invokes see stale reads. The
  scripts re-read and retry on exit 3.

## How it is put together

| path | |
|---|---|
| `core/src/lib.rs` | the rules: `check(source) → accept (eight output words) or refuse` — `no_std`, no panicking path, no division |
| `core/src/plan.rs` | the wallet's side: build the transitions `check` accepts, at the best amounts |
| `core/src/bin/plan.rs` | `plan`: the scripts' planner — cells in, `t.json` + private inputs out, checked against `check` first; and `demo` |
| `core/tests/rules.rs` | host tests: best amounts accepted, one unit more refused, no context word left unchecked (11 transition kinds), shares never worth less, every riskier accepted adjustment healthy |
| `src/main.rs` | the guest: `check` behind the zkVM's syscalls; a refusal reads private input `u32::MAX` (no run, no proof) |
| `../kit` | shared by the examples: the context reader, 256-bit products, secrets, the host-side transition builder |

Domain tags: `TAG_OPERATOR = "lnop"` (`0x706f6e6c`) for the operator's lock, `TAG_OWNER = "lnow"`
(`0x776f6e6c`) for a position's secret.

## Files

| file | |
|---|---|
| `build.sh` | build `image.bin` inside your circuits checkout, reject any panicking path, run the host tests |
| `run.sh` | every method on the emulator, accepted and refused, checked against the host rules |
| `operator.sh <C>` | make `operator.secret` (mode 600) and `public.txt` (the lock, then `C`) |
| `deploy.sh` | deploy with `public.txt` as the public input; saves the id |
| `share-token.sh` | register the share token: `rand token create --program <id>` |
| `operate.sh init <price> <share>` / `update <price> [rate]` | the operator: set up; move the price, accrue interest |
| `supply.sh <RAND>` | lend |
| `withdraw.sh <shares>` | burn shares for RAND |
| `adjust.sh [--deposit u] [--withdraw u\|max] [--borrow u\|max] [--repay u\|all]` | move your position (`position.secret`, made on first use) |
| `liquidate.sh <position key> <RAND>` | liquidate a position past 85 % |
| `show.sh` | every cell and the vault |

Keep `operator.secret` and `position.secret`. They are the only keys to the market's operation and
to your position. Both are in `.gitignore` as `*.secret`.

## Run it

```sh
./build.sh && ./run.sh
./operator.sh 3                          # collateral: token 3
./deploy.sh
./share-token.sh                         # say it prints index 4
./operate.sh init 2 4                    # 1 C = 2 RAND; shares are token 4
./supply.sh 100
./adjust.sh --deposit 50000000000 --borrow max
./operate.sh update 2 10000000           # accrue 1 %
./adjust.sh --repay all --withdraw max
./withdraw.sh 50000000000
./show.sh
```

`run.sh`, off chain:

```
  ok    accept  operator: init — 1 C = 2 RAND, index 1.0, share token bound   [tier 14: fits (cycles 2138 of 16383, Poseidon2 permutations 700 of 2048)]
  ok    refuse  init with someone else's secret
  ok    accept  lender A supplies 100 RAND for 99999999000 shares (1000 locked)   [tier 14: fits (cycles 2075 of 16383, Poseidon2 permutations 696 of 2048)]
  ok    refuse  the same supply minting one share more
  ok    accept  lender B supplies 50 RAND for 50000000000 shares   [tier 14: fits (cycles 4543 of 16383, Poseidon2 permutations 696 of 2048)]
  ok    accept  borrower deposits 50 C and borrows the most, 75000000000 RAND units   [tier 14: fits (cycles 6603 of 16383, Poseidon2 permutations 709 of 2048)]
  ok    refuse  the same, borrowing one unit more (past 75 % LTV)
  ok    refuse  liquidating the healthy position
  ok    accept  operator accrues 1 %: index 1000000000 → 1010000000   [tier 14: fits (cycles 3141 of 16383, Poseidon2 permutations 696 of 2048)]
  ok    refuse  the operator raising the index one unit past 1 %
  ok    refuse  the same accrual claiming a 1.0000001 % rate
  ok    accept  borrower repays 20 RAND (owes 55750000001 units after)   [tier 14: fits (cycles 6713 of 16383, Poseidon2 permutations 708 of 2048)]
  ok    refuse  the same repayment proved with another secret
  ok    refuse  lender A withdrawing one unit more than half their shares are worth
  ok    accept  lender A burns half their shares for 50249999497 RAND units   [tier 14: fits (cycles 4340 of 16383, Poseidon2 permutations 696 of 2048)]
  ok    accept  operator: C falls to 1.2 RAND (the position's LTV 92.91 %)   [tier 14: fits (cycles 3141 of 16383, Poseidon2 permutations 696 of 2048)]
  ok    accept  liquidator repays 20 RAND and seizes 18333333333 units of C   [tier 14: fits (cycles 8159 of 16383, Poseidon2 permutations 704 of 2048)]
  ok    refuse  the same liquidation seizing one unit more
  ok    accept  borrower repays the last 35750000001 units and takes 31666666667 C out (position deleted)   [tier 14: fits (cycles 6763 of 16383, Poseidon2 permutations 709 of 2048)]
  ok    refuse  the same, repaying one unit less
  ok    refuse  an unknown method
21 steps agree with the host rules, 0 do not
```

The image is 2 680 words, and every method proves at tier 14. Tier 12 is out of reach for two
reasons. The program hash alone is about 670 permutations, over tier 12's 512. The adjust and
liquidate methods also run 6 600–8 200 cycles of 256-bit products, over tier 12's 4 095.

## What is public

All of the following is public:

- The price, the index, the pool's cash, borrows and share supply.
- Every position's collateral and scaled debt, under a key that is the hash of its owner's secret.
- Each deposit's and payout's amount, and each payout's recipient key.

Who lent, who borrowed and who liquidated is not public: the bundle names nobody, and a share is
a shielded note like any other. A position is linked to nothing but its secret. Every adjustment
of the same position is visibly the same key.

## On chain

Deployed and used on **chain 20**, the public testnet, on 2026-10-01, through the published RPC
`https://rpc.randprotocol.org`, with `rand` built from fullnode `5f872a58` (two deploy-config commits
past the v0.6.8 tag), by the scripts above exactly as written. Every proof was made on a laptop (an
M4 Max), the prover keeping about four cores busy on average. Fees are what the wallet paid; look a transaction up with `rand_getTransaction <hash>`, and
the cells and the vault as they are today with `rand program state <id>` / `rand program vault <id>`.

program id `95ab870c8ba5531466443faaa6d052445157e54142dec8935435d199847b34a3`

| step | transaction | tier | fee (RAND) | result |
|---|---|---|---|---|
| `deploy.sh ` | `7cc327096a38d858ca7e1cf3a3e3e8ab49ab65d9de5ec6cc8f55c9149be987da` |  | 0.2699 | deployed |
| `share-token.sh ` | `c2c91e5013ca28bffb03b95d3b31a84701f5002f186696e4b1500845a84b5666` |  | 1.001 | token index 9 |
| `operate.sh init 1000000000 9` | `cbac9aa8ad0db411021a58caee31e4ede97ad8aa7c9a836af742a1386e544244` | 14 | 0.035931225 | outputs [1, 2808348672, 232830643, 1000000000, 0, 0, 0, 0] |
| `supply.sh 5` | `a4163504aabfcdb6778566bab3f2fd708bc8c858072d8c03871131a24732329f` | 14 | 0.005931225 | outputs [2, 705032704, 1, 705031704, 1, 0, 0, 0] |
| `adjust.sh --deposit 4000000000 --borrow 2000000000` | `d0815298aa6d11bcd4de5d93faebd8b1100ca9c0d530c361bb705a4d4f5ae61b` | 14 | 0.016449625 | outputs [4, 4000000000, 0, 2000000000, 0, 0, 0, 0] |
| `operate.sh update 1000000000 10000000` | `d760b5ff40af7744b627fa7a32ef40a50f889b403d82bf4f78fdc342a931aab5` | 14 | 0.005931225 | outputs [1, 2808348672, 232830643, 1010000000, 0, 0, 0, 0] |
| `adjust.sh --repay all` | `5d7cbd20cbc2f89580485e3dd42444c84291859c76bf807ccee231aa31187a08` | 14 | 0.006449625 | outputs [4, 4000000000, 0, 0, 0, 0, 0, 0] |
| `withdraw.sh 1000000000` | `f795abc3f5a4bf63c9c99255e700b8812e55c5deff82b7732160ca96e1e322d6` | 14 | 0.006449625 | outputs [3, 1000000000, 0, 1004000000, 0, 0, 0, 0] |
