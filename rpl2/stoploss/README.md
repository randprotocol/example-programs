# stoploss — a stop-loss nobody can hunt

A conditional order whose trigger price is **hidden while it rests**. An owner escrows RAND in the
program's vault together with a commitment to a trigger — a *stop* (fire when the price is at or
below a threshold) or a *take-profit* (at or above) — and the chain holds the escrow and the
commitment, nothing more. The price is a public oracle reading set by the **operator**, whose
lock (eight words) is the program's deploy-time public input. When the price meets the trigger,
whoever holds the ticket and the opening proves so inside the call proof and the escrow is
released. **The threshold is a private input, the price is a public oracle reading verified inside
a proof, and the receipt is a legal order that fires only when the condition holds.**

On an ordinary venue a resting stop is public, and a book full of stops at 90 is an invitation to
push the price through 90. Here nothing on chain says 90 — not while the order rests, and not when
it fires: the receipt says only that the condition held. And an attempt to fire while the
condition is false is refused, which on this chain means **no proof and no transaction at all**:
nobody can even tell it was tried.

```
oracle  key   [1, 0, 0, 0, 0, 0, 0, 0]
        value [p_lo, p_hi, 0, 0, 0, 0, 0, 1]                 price, version
order   key   [2, d0, …, d6]                                  d = POSEIDON2([TAG_TICKET, ticket]),   TAG_TICKET  = 0x6b746c73 ("sltk")
        value [a_lo, a_hi, c0, c1, c2, c3, c4, 1]             escrow (RAND units), commitment, version
        c = POSEIDON2([TAG_TRIGGER, kind, t_lo, t_hi, salt0, salt1, salt2, salt3, 0])[0..5]      TAG_TRIGGER = 0x72746c73 ("sltr")
```

The operator's lock is `POSEIDON2([TAG_OPERATOR, s0..s7])`, `TAG_OPERATOR = 0x706f6c73` ("slop").
`kind` is 1 for a stop and 2 for a take-profit; the threshold is a `u64` in whatever unit the
operator quotes the price in.

| method | private inputs | transition | rule |
|---|---|---|---|
| 1, operate (operator) | `[1, s0..s7, old_lo, old_hi, new_lo, new_hi]` | oracle → oracle; nothing in or out | the secret opens the lock; the oracle read is absent (`old = 0`) or holds exactly `old`; `0 < new < 2^63` |
| 2, place (owner) | `[2, ticket, kind, t_lo, t_hi, salt]` | order absent → order; RAND in (`burn_r`) | the key is the ticket's; `amount = burn_r > 0`; `kind ∈ {1, 2}`; the cell's `c` is the opening's commitment |
| 3, fire (ticket + opening) | `[3, ticket, kind, t_lo, t_hi, salt]` | oracle → oracle (unchanged), order → absent; pay `amount` RAND | the key is the ticket's; the commitment is the opening's; stop: `price ≤ t`, take-profit: `price ≥ t` |
| 4, cancel (ticket + opening) | `[4, ticket, kind, t_lo, t_hi, salt]` | order → absent; pay `amount` RAND | the key is the ticket's; the commitment is the opening's; no condition |

`ticket` is eight random words the owner keeps; their digest names the order's cell, so only the
ticket's holder can fire or cancel it. The opening — `kind`, the threshold and a four-word salt —
is kept beside it in `<name>.secret`. `place` reads no oracle: the trigger is the owner's business
until it fires.

**Every word is pinned by an equality, not just by the chain's read check.** `fire` and `cancel`
take the opening as well as the ticket so that the commitment words of the order they read are
checked (the orderbook example's lesson: a cell word the program is merely shown is a word any
caller may set). The operator's instruction — the price replaced and the new price — is a private
input the transition must match word for word. And `fire` writes the oracle back exactly as read,
so its price words are pinned too; the rewrite is free (rewriting a cell costs nothing) and does
not contend (two fires in one block leave the same value, so the second's read still matches).

## The commitment: 160 bits, and a 128-bit salt

The cell has room for five words of hash beside the escrow and the version, and 160 bits are
**binding** enough: once an order is placed, firing it under any other threshold means a second
preimage of a 160-bit value — work no one can do. What **hides** the threshold is the salt. The
kind is one bit and a threshold is a plausible price, a few dozen bits of entropy at most; without
a salt, or with one word of it, a hunter could hash every plausible opening and read the stop off
the chain. Four random words (128 bits) put that search out of reach for good. `core/src/lib.rs`
says the same beside `Opening`.

## What "fires" means here, and who can pull the trigger

**Firing releases the escrow to whoever the firing transaction names.** An RPL-2 program decides
amounts, never recipients: who is paid is fixed by the call binding of the transaction that invokes
it, outside the context the program sees. So this example's `fire` is the *trigger*, and the
payout is the *action*. A real stop-loss would hand the escrow to a venue in the same transition —
the [amm](../amm/) example's pool, say: one more read and one more write (the pool cell), 32 more
context words, and the amm's swap rule checked on the way. `fire` is 78 context words; with an
eight-word public input a program may use 111, so the pool fits (8 + 8 + 78 + 32 = 126 ≤ 127).
That extension is the natural next step and the reason the trigger is kept this small.

**Whoever holds the ticket and the opening can fire.** That is what lets a *keeper* watch the
price and fire for you while you sleep: give them `<name>.secret` and they can. But the keeper's
transaction fixes the recipient, so a keeper can fire the order to themselves. Share the file only
with a keeper you trust — or fire yourself (`fire.sh` pays your own wallet by default). A keeper
cannot learn the threshold from the chain, only from you.

**The oracle is the operator, and there is no clock.** The price is whatever the operator last
wrote; the program cannot see time, so it cannot tell a fresh reading from a stale one, and an
owner trusts the operator not to print a price that fires their stop. (A multi-signer oracle, or
a reading carried in with a signature the program checks, would be the production answer; the
lock-as-public-input is the smallest.) The lock is part of the program id, so an operator can never
be replaced: each operator is its own program, with its own vault.

**One escrow, released once.** `fire` and `cancel` pay exactly the escrow and delete the cell;
zeros are no order, so a second release has nothing to read. The vault only ever holds what
resting orders escrowed, and only ever pays it to the one who proves the ticket.

## How it is put together

| path | |
|---|---|
| `core/src/lib.rs` | the rules: `check(source) → accept (eight output words) or refuse` — `no_std`, no panicking path, no division |
| `core/src/plan.rs` | the wallet's side: build the transitions `check` accepts; refuse to build a fire whose condition does not hold, by the same `fires` predicate |
| `core/src/bin/plan.rs` | `plan`: the scripts' planner — cells and a secret file in, `t.json` + private inputs out, checked against `check` first; `status`; `demo` |
| `core/tests/rules.rs` | host tests: every method accepted and every tampering refused, the boundaries of both triggers, a wrong opening, release exactly once, **no context word left unchecked** |
| `src/main.rs` | the guest: `check` behind the zkVM's syscalls; a refusal reads private input `u32::MAX` (no run, no proof) |
| `../kit` | what all the examples share: the context reader, 256-bit products, secrets, the host-side transition builder |

## Files

| file | |
|---|---|
| `build.sh` | build `image.bin` inside your circuits checkout, reject any panicking path, run the host tests |
| `run.sh` | every method on the emulator, accepted and refused, checked against the host rules |
| `operator.sh` | make `operator.secret` (8 random words, mode 600) and `public.txt`, its lock — the deploy's public input |
| `deploy.sh` | deploy with `public.txt`; saves the id |
| `set-price.sh <price>` | the operator sets the price (the first call creates the oracle) |
| `place.sh stop\|tp <RAND> <threshold> [name]` | escrow RAND under a hidden trigger; writes `<name>.secret` (ticket, kind, threshold, salt) |
| `fire.sh <secret file> [rand1…]` | fire if the condition holds (else nothing is sent); pays the recipient, default this wallet; re-plans on a stale read |
| `cancel.sh <secret file> [rand1…]` | take a resting order back, whatever the price |
| `show.sh [secret file]` | the price, every cell and the vault; with a secret, that order and whether it would fire now |

**Keep `<name>.secret`.** It is the only key to the escrow; lose it and the escrow is locked for
good. It, `operator.secret`, `public.txt` and `program.id` are in `.gitignore`.

## Run it

```sh
./build.sh && ./run.sh
./operator.sh && ./deploy.sh
./set-price.sh 100
./place.sh stop 5 90 mine       # 5 RAND, fires at or below 90 → mine.secret
./fire.sh mine.secret           # "the condition does not hold … nothing sent"
./set-price.sh 85
./fire.sh mine.secret           # 5 RAND back to this wallet
./place.sh tp 3 120 later && ./show.sh later.secret
./cancel.sh later.secret
```

`run.sh`, off chain:

```
  ok    accept  operate: the first price, 100   [tier 12: fits (cycles 1199 of 4095, Poseidon2 permutations 287 of 512)]
  ok    refuse  operate with a wrong secret
  ok    accept  place A: 5 RAND, a stop at 90 (the chain sees only a commitment)   [tier 12: fits (cycles 1050 of 4095, Poseidon2 permutations 290 of 512)]
  ok    refuse  place A with no RAND coming in
  ok    refuse  fire A at price 100: the condition is false (no proof, so no trace of the attempt)
  ok    accept  operate: the price moves to 85   [tier 12: fits (cycles 1195 of 4095, Poseidon2 permutations 287 of 512)]
  ok    accept  fire A at price 85: 5 RAND released   [tier 12: fits (cycles 1827 of 4095, Poseidon2 permutations 299 of 512)]
  ok    refuse  fire A taking one unit more
  ok    accept  place B: 3 RAND, a take-profit at 95   [tier 12: fits (cycles 1050 of 4095, Poseidon2 permutations 290 of 512)]
  ok    refuse  fire B at price 85: the condition is false
  ok    accept  operate: the price moves to 100   [tier 12: fits (cycles 1195 of 4095, Poseidon2 permutations 287 of 512)]
  ok    accept  fire B at price 100: 3 RAND released   [tier 12: fits (cycles 1832 of 4095, Poseidon2 permutations 299 of 512)]
  ok    accept  place C: 2 RAND, a stop at 80   [tier 12: fits (cycles 1050 of 4095, Poseidon2 permutations 290 of 512)]
  ok    refuse  fire C claiming a threshold of 100: the commitment does not match
  ok    refuse  cancel C with a wrong ticket
  ok    accept  cancel C: 2 RAND back   [tier 12: fits (cycles 1222 of 4095, Poseidon2 permutations 291 of 512)]
  ok    refuse  an unknown method
17 steps agree with the host rules, 0 do not
```

The image is 1 053 words; every method proves at tier 12. `fire` is the widest at 1 832 cycles,
with two Poseidon2 hashes inside (the ticket's digest and the trigger's commitment).

## What is public

The oracle's price. For each resting order: its escrow and a commitment — never the kind or the
threshold. For each fire or cancel: the amount released and the recipient's public key, as for
any payout. Not who placed an order (the bundle names nobody), not what its trigger was, and not
that anyone ever tried to fire it before its time: a refusal is no transaction.

## On chain

Deployed and used on **chain 20**, the public testnet, on 2026-10-01, through the published RPC
`https://rpc.randprotocol.org`, with `rand` built from fullnode `5f872a58` (two deploy-config commits
past the v0.6.8 tag), by the scripts above exactly as written. Every proof was made on a laptop (an
M4 Max), the prover keeping about four cores busy on average. Fees are what the wallet paid; look a transaction up with `rand_getTransaction <hash>`, and
the cells and the vault as they are today with `rand program state <id>` / `rand program vault <id>`.

program id `7e92e337bd08b21e7fde15b07723f52bb22f485d7e71be4deb7db38f3313aff1`

| step | transaction | tier | fee (RAND) | result |
|---|---|---|---|---|
| `deploy.sh ` | `5ba712cb0e67cc36327b4b3f4c31839a8748f9c33ccead62225e2c2f196485eb` |  | 0.1071 | deployed |
| `set-price.sh 100` | `de4507bbb26b742663c575a9681ec82f19cf1ff10003fb50d33b6f4fb940362c` | 12 | 0.015672025 | outputs [1, 100, 0, 0, 0, 0, 0, 0] |
| `place.sh stop 1 90 s1` | `eacde428d545f3ab3f14ca8603fb04d2d675db6177e9859739513f5af4f42028` | 12 | 0.015672025 | outputs [2, 1000000000, 0, 0, 0, 0, 0, 0] |
| `set-price.sh 85` | `c30fd8e6a0e95fed2f0c7b16da2c2e03f20cccd1972bd28d7f198e3db4dbecd8` | 12 | 0.005672025 | outputs [1, 85, 0, 0, 0, 0, 0, 0] |
| `fire.sh s1.secret` | `56f18ebe1574a0a00ae440b660ed68a9653edad7c344368ec03e158677a0c838` | 12 | 0.005672025 | outputs [3, 1000000000, 0, 0, 0, 0, 0, 0] |
