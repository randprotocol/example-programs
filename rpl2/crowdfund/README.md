# crowdfund — an all-or-nothing campaign with refundable pledges

Kickstarter as an RPL-2 program. Backers pledge RAND and get the same number of **receipts**, a
token only this program mints; with their receipts they can take their RAND back at any time until
the creator claims. The creator can claim everything raised once it reaches the goal, and only
then. One cell holds the campaign; the program's vault holds the pledges.

The program's **deploy-time public input** is ten words: the creator's lock
`POSEIDON2(["crtr", s0..s7])` (8 words) and the goal in RAND base units (low, high; more than 0).
Both are part of the program id, so the goal cannot be moved and no one but the creator can open
or claim the campaign. Each campaign is its own program, with its own id and vault.

```
key    [1, 0, 0, 0, 0, 0, 0, 0]
value  [raised_lo, raised_hi, receipt, claimed, 0, 0, 0, 1]      RAND raised, receipt token, 0 or 1, version
```

| method | private inputs | transition | rule |
|---|---|---|---|
| 1, init | `[1, s0..s7, receipt]` (creator) | campaign absent → `[0, 0, receipt, 0, 0, 0, 0, 1]`; nothing in or out | the secret opens the lock; `receipt ≠ RAND` |
| 2, pledge | `[2]` (anyone) | campaign → campaign; `a` RAND in; mint `a` receipts | not claimed; `a > 0`; the mint is the bound receipt; `raised' = raised + a` |
| 3, refund | `[3]` (anyone holding receipts) | campaign → campaign; `b` receipts burned; pay `b` RAND | not claimed; `0 < b ≤ raised`; the burn is the bound receipt; `raised' = raised − b` |
| 4, claim | `[4, s0..s7]` (creator) | campaign → claimed; pay `raised` RAND | not claimed; the secret opens the lock; `raised ≥ goal`; one payout of exactly `raised`; `raised` kept, `claimed' = 1` |

**The written campaign is always exactly the read one moved by what came in and went out**, and
every word of it is pinned, so a caller only ever chooses an amount. While the campaign is open,
**receipts outstanding = `raised` = the vault's RAND**: a pledge mints exactly what it brings in,
a refund pays exactly what it burns, so every receipt can always be redeemed one for one and a
claim empties the vault exactly. The tests replay pledges and refunds in any order and check it.

**A claim is final.** Every method refuses a campaign whose `claimed` word is 1 — no pledge, no
refund, no second claim, no re-init. The receipts stay in the backers' wallets as badges.

**The receipt token is bound by `init`.** Anyone can register a token with `--program <id>`, so the
campaign records the creator's choice and every later mint and burn must be of that asset. `init`
cannot mint, so it cannot prove the token is the program's own: if the creator binds a token the
program does not control, every pledge fails at the chain and the campaign is stuck at zero
(nobody loses anything — nothing was pledged). `init.sh` takes the index `receipt-token.sh`
printed. The receipt is a private input of `init`, not merely a word of the write, so that every
context word is checked against something the prover committed to.

**The program never divides — or multiplies.** Its arithmetic is `raised ± amount` (checked to
stay below 2^63) and `raised ≥ goal`; `plan` searches `goal_met` and `refund_ok`, the predicates
the program checks, for "how much more to the goal" and "how much can I refund".

## No deadline: what that means

RPL-2 shows a program no clock — no height, no time — so this campaign has **no deadline**:

- backers may refund at any moment until the creator claims, **even after the goal is met** (a
  refund can take the campaign back below it, and the creator then cannot claim);
- the creator can claim the moment `raised ≥ goal`, however early;
- a campaign that never reaches its goal never ends: backers simply refund whenever they like.

So "all or nothing" holds — the creator gets everything raised or nothing, and nobody's RAND is
ever stuck — but not "by Friday". A deadline would need a time the program can trust: the chain
putting its height into the context (not in RPL-2 today), or an oracle's signed time in a cell,
which moves the trust onto that oracle. With either, the rules would become: pledge and refund
only before the deadline, claim only after it with `raised ≥ goal`, and refunds re-open after it
if the goal was missed.

## Who sees what

| | public | private |
|---|---|---|
| the campaign | `raised`, the receipt token, `claimed`; the goal and the lock (in the program id) | the creator's secret |
| a pledge | its amount (a deposit into the vault) | who pledged; the receipts are a shielded note to the pledger |
| a refund | its amount (burned and paid out), the payout's recipient key | whose receipts they were |
| the claim | its amount and recipient key | — |

A pledger and a refunder of the same amount cannot be linked by the program: the receipts are
fungible notes, and a refund needs only receipts, not the pledge they came from.

## How it is put together

| path | |
|---|---|
| `core/src/lib.rs` | the rules: `check(source) → accept (eight output words) or refuse` — `no_std`, no panicking path, no division |
| `core/src/plan.rs` | the wallet's side: build the transitions `check` accepts |
| `core/src/bin/plan.rs` | `plan`: the scripts' planner — the cell in, `t.json` + private inputs out, checked against `check` (with the real Poseidon2) first; and `demo` |
| `core/tests/rules.rs` | host tests: exact amounts accepted, one unit more refused, the secret, finality, receipts = raised = vault, and **no context word left unchecked** for every accepted method |
| `src/main.rs` | the guest: `check` behind the zkVM's syscalls; a refusal reads private input `u32::MAX` (no run, no proof) |
| `../kit` | what all the examples share: the context reader, secrets, the host-side transition builder |

## Files

| file | |
|---|---|
| `build.sh` | build `image.bin` inside your circuits checkout, reject any panicking path, run the host tests |
| `run.sh` | every method on the emulator, accepted and refused, checked against the host rules |
| `creator.sh <goal RAND>` | make `creator.secret` (mode 600) and `public.txt` (the lock, then the goal) |
| `deploy.sh` | deploy with `public.txt` as the public input; saves the id |
| `receipt-token.sh` | register the receipt token: `rand token create --program <id>` |
| `init.sh <receipt asset>` | open the campaign (the creator) |
| `pledge.sh <RAND>` | pledge; receipts come back to this wallet |
| `refund.sh <receipt units>` | burn receipts for the same RAND back |
| `claim.sh` | claim everything raised (the creator), once the goal is met |
| `show.sh` | the campaign's cell, its progress, and the vault |

`init.sh`, `pledge.sh`, `refund.sh` and `claim.sh` re-read the campaign and plan again if the chain
refuses the invoke as a stale read (`rand` exits 3) — another backer moved it first.

## Run it

```sh
./build.sh && ./run.sh
./creator.sh 100              # a 100 RAND goal: creator.secret + public.txt
./deploy.sh
./receipt-token.sh            # say it prints index 7
./init.sh 7
./pledge.sh 40                # from any wallet: RAND_KEY=… ./pledge.sh 40
./refund.sh 5000000000        # 5 RAND of receipts back to RAND
./show.sh                     # raised …, … to go
./claim.sh                    # once raised ≥ 100 RAND
```

`run.sh`, off chain:

```
  ok    accept  init: a 10 RAND goal, receipts are asset 6   [tier 12: fits (cycles 994 of 4095, Poseidon2 permutations 185 of 512)]
  ok    refuse  init with a wrong secret
  ok    refuse  init declaring a receipt token other than the creator's
  ok    accept  backer A pledges 3 RAND for 3 RAND of receipts   [tier 12: fits (cycles 757 of 4095, Poseidon2 permutations 180 of 512)]
  ok    refuse  the same pledge minting a different asset
  ok    refuse  the same pledge minting one receipt too many
  ok    accept  backer B pledges 4 RAND   [tier 12: fits (cycles 757 of 4095, Poseidon2 permutations 180 of 512)]
  ok    refuse  the creator claims 7 RAND of a 10 RAND goal
  ok    accept  backer A burns 1 RAND of receipts for 1 RAND back   [tier 12: fits (cycles 758 of 4095, Poseidon2 permutations 180 of 512)]
  ok    refuse  the same refund paying one unit more
  ok    refuse  a refund of more than the campaign holds
  ok    accept  backer B pledges 4000000000 units more: the goal is met   [tier 12: fits (cycles 757 of 4095, Poseidon2 permutations 180 of 512)]
  ok    refuse  claim with a wrong secret
  ok    accept  the creator claims exactly 10000000000 units   [tier 12: fits (cycles 931 of 4095, Poseidon2 permutations 185 of 512)]
  ok    refuse  the same claim taking one unit more
  ok    refuse  a pledge after the claim
  ok    refuse  a refund after the claim
  ok    refuse  a second claim
  ok    refuse  an unknown method
19 steps agree with the host rules, 0 do not
```

The image is 647 words; every method proves at tier 12. The largest context (a pledge, a refund
or a claim: one read, one write, one payout or mint) is 46 words; with the ten public words and
the eight binding words the segment is 64 of 127.

## What is public

The campaign's cell (what has been raised, the receipt token, whether it is claimed), the goal
and the creator's lock (in the program id), and each pledge's, refund's and claim's amount and
each payout's recipient key. Not who backed the campaign: a pledge names nobody, and its receipts
are a shielded note like any other.

## On chain

Deployed and used on **chain 20**, the public testnet, on 2026-10-01, through the published RPC
`https://rpc.randprotocol.org`, with `rand` built from fullnode `5f872a58` (two deploy-config commits
past the v0.6.8 tag), by the scripts above exactly as written. Every proof was made on a laptop (an
M4 Max), the prover keeping about four cores busy on average. Fees are what the wallet paid; look a transaction up with `rand_getTransaction <hash>`, and
the cells and the vault as they are today with `rand program state <id>` / `rand program vault <id>`.

program id `e32f8fad39080079abe5f71ae4cf31db1716406843562adfbc12682095cef243`

| step | transaction | tier | fee (RAND) | result |
|---|---|---|---|---|
| `deploy.sh ` | `c8cd50ed48b5039df45f471fea4056f4ee1907b277884b5bc443c7ca96fdd7d1` |  | 0.0667 | deployed |
| `receipt-token.sh ` | `ea60d094bb94ccc046a18fb92fb9f7169357a7cac2893163cfe70008b88e73fe` |  | 1.001 | token index 6 |
| `init.sh 6` | `84eac7f301d4f3b5288e63d99c1f9dc8371b00b2f2bba9895474a9fcaa16dd7c` | 12 | 0.015542425 | outputs [1, 0, 0, 0, 0, 0, 0, 0] |
| `pledge.sh 2` | `651cc6d484e99c08c98aa5b66374445f9f1c789099e481d767c099c92f9b86d4` | 12 | 0.005542425 | outputs [2, 2000000000, 0, 2000000000, 0, 0, 0, 0] |
| `pledge.sh 1.5` | `b04060f68eb0b5154cfb691c6b9e13a4d0e93fc58bf474a1cabbd9cd3bacc9ea` | 12 | 0.005542425 | outputs [2, 1500000000, 0, 3500000000, 0, 0, 0, 0] |
| `refund.sh 500000000` | `9ebf314941405e4de5dd2670a1246092e66165f666ceaad90428509f86b063de` | 12 | 0.005542425 | outputs [3, 500000000, 0, 3000000000, 0, 0, 0, 0] |
| `claim.sh ` | `1bdce0aedaa6e43ffa901c7e98501725427f089fc844f3f7e9d005d2059e1ba4` | 12 | 0.005542425 | outputs [4, 3000000000, 0, 3000000000, 0, 1, 0, 0] |
