# ballot — a private ballot with a public tally

> "The tally is a program. Ballots are private inputs weighted by a private proof of holdings,
> and the receipt publishes the totals." — [randprotocol.org/usecases](https://randprotocol.org/usecases/)

An RPL-1 program: stateless, run once per election with `rand call`. An **organiser** publishes
the eligible-voter roll — who may vote, with what weight — and deploys the program with the roll's
digest, so the program id binds the roll. A **tallier** collects the ballots and calls the program
with every one of them as private inputs. The receipt publishes the four totals and proves they
are a weighted count over exactly the committed roll: nobody added, dropped, re-weighted or
reordered, every voter's weight landed in one option or in the abstentions, and the arithmetic is
exact. The ballots themselves never leave the tallier's machine.

```
public input (deploy)   [n_options, R0, …, R7]                                      vote.txt, 9 words
                        n_options in 2..=4; R the roll's digest (below)
private input (call)    [blind0, blind1, n_voters, (voter_tag, w_lo, w_hi, choice) × n_voters]
                        in roll order; n_voters ≤ 16; each weight and their sum < 2^63
outputs (receipt)       [t0_lo, t0_hi, t1_lo, t1_hi, t2_lo, t2_hi, t3_lo, t3_hi]      four u64 totals
```

A `choice` that is not an option (`>= n_options`) is an **abstention**. Abstentions are not in the
receipt — eight words hold four u64 totals and nothing else — but the roll is public, so anyone
computes them: the roll's total weight less the four totals. Options the ballot does not have
(`t2`, `t3` for a three-way ballot) are always 0.

The program refuses — no run, so no proof — when `n_options` is outside `2..=4`, `n_voters` is 0
or above 16, a weight is 2^63 or more, the weights' running sum reaches 2^63, or the
`(voter_tag, weight)` pairs it was handed do not fold to the public `R`.

## The roll and its digest

The roll is a list of `(voter_tag, weight)` pairs: a tag is a `u32` that names the voter on the
roll (a member number, a key hash truncated to a word — public, as the roll is), a weight is the
`u64` their ballot counts for (shares, tokens, seats). `roll.txt` is the demo's:

```
options 3
101 500
102 300
103 200
104 150
105 50
```

Its digest is a fold over the zkVM's Poseidon2, in `core/src/lib.rs` (`seed`, `step`):

```
state₀   = [n_voters, 0, 0, 0, 0, 0, 0, 0]
stateᵢ₊₁ = POSEIDON2([TAG, stateᵢ (8 words), voter_tagᵢ, w_loᵢ, w_hiᵢ])      TAG = "roll"
R        = stateₙ
```

Every message is a fixed twelve words with its own domain tag, because the sponge does not pad;
the seed is the roll's length, so a roll and a prefix of it never share a digest; the chain is
sequential, so the order is bound too. `fold/` is a second guest, never deployed, that only
computes this fold with the very same code: both guests depend on the `core/` crate and call its
`seed` and `step`, as the tests and the tool do. `roll.sh` runs it on the emulator to write
`vote.txt`, so the `R` the organiser deploys with is computed by exactly the code that checks it.

`n_options` is not inside the fold; it is the first public word beside `R`, and the program id
(`rand-program-2` hashes code and public input) binds both.

## What is proved, and what is not

**Proved by the receipt.** Someone holding `n_voters` four-word records whose `(voter_tag, weight)`
pairs fold to the deployed `R` — that is, exactly the published roll, in order — assigned each
record's weight to one of the options or to abstention, and the receipt's totals are those sums.
Anyone can check the receipt against the public roll: the four totals plus the implied abstentions
equal the roll's total weight, and no weight the roll does not list can be in them.

**Private.** The ballots. The chain holds only `H_IN`, a salted commitment to the input words, and
the call's input envelope sealed to the tallier's key (and to an auditor's, if one was named). The
two **blind words** at the front of the inputs are fresh random words from `/dev/urandom` on every
call, never read for anything, never output: a proof leaks an unsalted function of its input words,
and without them a five-voter ballot has few enough possible input lists to brute-force. The tier
the proof reveals depends on the roll's size — rolls of up to about eleven voters prove at tier 10,
twelve to sixteen at tier 12 — and not on the choices.

**Not achieved — be clear about this before using it.**

- **The tallier sees every ballot.** This is a returning officer's position, not a voting booth's.
  Ballot secrecy toward the tallier, coercion resistance, receipt-freeness: none of it. It needs
  encrypted ballots, or a per-voter commitment phase on RPL-2 where each voter publishes
  `H(choice, salt)` under their tag before the count and the tally proves against those.
- **The receipt does not prove the choices are the voters' own.** The program cannot tell a ballot
  a voter cast from a choice the tallier typed: both are a private word. A dishonest tallier can
  swap choices and produce a receipt that verifies. What the proof rules out is the *roll* being
  tampered with (and arithmetic errors); what it does not rule out is the *ballots* being. The
  check on the ballots is off chain: voters keeping their own records, or an auditor named with
  `--auditor` opening the call's inputs and comparing them with what voters say they cast.
- **The weights are not a "private proof of holdings".** The use-case sentence says ballots are
  "weighted by a private proof of holdings"; here the weight is a public roll entry the organiser
  fixed at deploy. Binding a weight to holdings — a shielded note, a credential — needs that
  proof inside the program (an RPL-2 cell, or a commitment to a credential set) and is not done
  here. The roll is a plain list, and the proof is that the count used exactly that list.
- **Eligibility is the organiser's word.** The program checks that the count used the roll, not
  that the roll is right. A bad roll — a stranger on it, a member missing, a duplicate tag with
  two weights — is visible to everyone because the roll is public, and that is the only check on
  it. (The `ballot` tool refuses duplicate tags; the program does not look for them.)
- **The chain cannot show how anyone voted.** Nothing on chain names a voter's choice. But the
  tallier can, and so can anyone the tallier seals the envelope to.

## Files

| file | |
|---|---|
| `src/main.rs` | the guest: syscall plumbing around `ballot_core::check` |
| `core/src/lib.rs` | the rules, `no_std` on the guest and `std` on the host: the fold, the tally, every refusal |
| `core/src/host.rs` | the roll and ballots files, the call's words, a `Mock` source for tests, the real `R` via the fold guest |
| `core/src/bin/ballot.rs` | the tool: `ballot roll <roll>` prints the public words, `ballot inputs <roll> <ballots>` the call's words and the expected totals |
| `core/tests/rules.rs` | nine host tests: accepted tallies, every roll tampering refused, the bounds, the files |
| `fold/` | the off-chain helper guest (never deployed) that computes `R` with the program's own code |
| `Cargo.toml` | a `no_std`, `no_main` crate on `guest-sdk` and `core/`, `panic = "abort"` |
| `roll.txt` | the demo roll: five voters, three options (public; a real roll is published) |
| `ballots.txt` | the demo ballots: two abstain (private in a real election; name yours `*.secret`) |
| `vote.txt` | the deploy-time public input `roll.sh` writes: `n_options R0 … R7` |
| `build.sh` | build both images inside your circuits checkout, reject any panicking path, run the tests |
| `roll.sh [roll]` | fold a roll into `vote.txt` on the emulator |
| `run.sh` | the emulator, off chain: honest, two tampered rolls, a choice of 7, the 16-voter cap, 17 voters |
| `deploy.sh` | deploy with `vote.txt` as the public input; saves the id to `program.id` |
| `call.sh <roll> <ballots> [auditor]` | tally on chain and print the receipt |

## Run it

```sh
export CIRCUITS=~/src/circuits            # see the top-level README for the rest
./build.sh
./roll.sh roll.txt                        # vote.txt
./run.sh
./deploy.sh
./call.sh roll.txt ballots.txt            # or: ./call.sh roll.txt ballots.secret rand1…auditor
```

`run.sh`, off chain (the blind words differ on every run; the outputs do not):

```
public input (vote.txt): 3 1564136462 2094373079 4142529591 2295160030 548895725 2334792111 234521662 2753435187

5 voters, 3 options, two abstain (honest roll and ballots):
  expected: tally 700 300 0 0, abstain 200
out[0] = 700
out[1] = 0
out[2] = 300
out[3] = 0
out[4] = 0
out[5] = 0
out[6] = 0
out[7] = 0
cycles 547
tier 10
tier 12: fits (cycles 614 of 4095, Poseidon2 permutations 82 of 512)

voter 102's weight 300 → 301 (the roll no longer folds to R):
  expected: tally 700 301 0 0, abstain 200
trap: InputIndex(4294967295)

voter 105 dropped from the roll:
  expected: tally 700 300 0 0, abstain 150
trap: InputIndex(4294967295)

voter 103 chooses 7 (not an option: an abstention):
  expected: tally 500 300 0 0, abstain 400
out[0] = 500
out[1] = 0
out[2] = 300
out[3] = 0
out[4] = 0
out[5] = 0
out[6] = 0
out[7] = 0
cycles 541
tier 10
tier 12: fits (cycles 608 of 4095, Poseidon2 permutations 82 of 512)

16 voters, 4 options (the cap): the tier a full call needs:
  expected: tally 28000 32000 36000 40000, abstain 0
out[0] = 28000
out[1] = 0
out[2] = 32000
out[3] = 0
out[4] = 36000
out[5] = 0
out[6] = 40000
out[7] = 0
cycles 1275
tier 12
tier 12: fits (cycles 1353 of 4095, Poseidon2 permutations 126 of 512)

17 voters (over the cap):
  the tool refuses the roll before any run:
error: roll: 17 voters; the program takes at most 16
  and the program refuses the words if handed them anyway:
trap: InputIndex(4294967295)
```

The "expected" lines are the `ballot` tool running the same rules on the host; for the two tampered
rolls they are what the tallier *would* publish, and the program's answer is that there is no
proof of it. The `tier N` line is the smallest tier the run fits.

The image is **228 words**. Every call fits **tier 12** (the 16-voter cap: 1353 of 4095 cycles,
126 of 512 Poseidon2 permutations); a roll of up to about eleven voters fits tier 10. The cap of
16 is a cap on the loop, not on what fits: each voter costs about 66 cycles and four permutations,
so a few dozen more would still fit tier 12 and a few hundred tier 14 — raise `MAX_VOTERS` and
rebuild for a bigger roll (a new image, so a new program). `rand call` picks the smallest tier the
run fits; pass `--tier 12` to it to prove every election at the same tier whatever the roll's
size.

`call.sh` passes `--expect-public vote.txt`, so the wallet refuses before proving if the program on
chain was deployed with a different roll, and compares the roll file with `vote.txt` first, so a
tallier counting against the wrong roll learns it before a wasted proof. With a third argument, a
`rand1…` address, it passes `--auditor`: the call's input transcript — the salt and every input
word, the ballots included — is then sealed to that key as well as the tallier's own, and the
auditor can run `rand open-call <tx> --as-auditor` to recover the ballots, check them against the
receipt's `H_IN` and re-run the program on them (the command exits non-zero if they are not the
receipt's inputs). That is the one hook this example has for checking the *ballots*, as opposed to
the roll: it moves the trust from the tallier to the auditor, it does not remove it.

## On chain

Deployed and used on **chain 20**, the public testnet, on 2026-10-01, through the published RPC
`https://rpc.randprotocol.org`, with `rand` built from fullnode `5f872a58` (two deploy-config commits
past the v0.6.8 tag), by the scripts above exactly as written. Every proof was made on a laptop (an
M4 Max), the prover keeping about four cores busy on average. Fees are what the wallet paid; look a transaction up with `rand_getTransaction <hash>`, and
the cells and the vault as they are today with `rand program state <id>` / `rand program vault <id>`.

program id `c73448baf6ae3ab4f034d159cc542c1010f0ef2be858a9d88e8c0eca24793f80`

| step | transaction | tier | fee (RAND) | result |
|---|---|---|---|---|
| `deploy.sh ` | `2ea81ea1c2654c9ea21238dd8835dcbaa9574e2e4b58180c9aaaadbfe1be6964` |  | 0.0247 | deployed |
| `call.sh roll.txt ballots.txt` | `d9d9455ea7cef92e59365cec7608467e7bc409f4e3dd7f5f910e73ada33ccde3` | 10 | 0.005511037 | outputs [700, 0, 300, 0, 0, 0, 0, 0] |
