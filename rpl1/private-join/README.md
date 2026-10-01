# private-join — two committed lists in, one agreed number out

An RPL-1 program: stateless, run with `rand call`. Two institutions each commit to a list of
records (64-bit keys — hashed customer ids, say). The program takes both lists as private inputs,
checks each against its commitment, and publishes only the result the two agreed to release when
the program was deployed: the **size of the intersection** (mode 0), or whether the two parties
**match** — each lists the other's id (mode 1). From [randprotocol.org/usecases](https://randprotocol.org/usecases/):

> The join is a program. Each institution supplies its records as a private input, the program
> computes the intersection or aggregate statistic, and the receipt publishes only the result both
> parties agreed to release.

> The matching rule is a program with both parties' preferences as private inputs. The receipt
> says match or no match.

```
public input (deploy)   [C_A (8 words), C_B (8 words), mode]                  join.txt, 17 words
private input (call)    [blind, blind, A: n, id, 16 keys, salt (43 words), B: the same (43)]   88 words
outputs (receipt)       mode 0: [|A ∩ B|, 0, 0, 0, 0, 0, 0, 0]
                        mode 1: [1 if id_A ∈ B and id_B ∈ A else 0, 1, 0, 0, 0, 0, 0, 0]
```

A list is at most **16 keys, strictly ascending**: the program refuses an unsorted or repeated key,
so the committed count is exact and the intersection size is a set size. Keys are two words each
(a u64 little-endian), absent keys are zeros. Each party also commits its **own id** (one key, the
name the other party knows it by; mode 1 reads it, mode 0 ignores it) and an eight-word **salt**,
so a list cannot be brute-forced from its commitment.

The first two private words are **blinds**: uniformly random per call (`/dev/urandom`), read and
never used. A call's proof leaks an unsalted function of its input words, and a few small words
could be matched by enumeration; the blinds make that function unenumerable. They are never output.

## What a receipt says

The program id (`rand-program-2` hashes the code and the public input) binds `C_A`, `C_B` and the
mode. A receipt for that id therefore says: *the result is exactly the agreed function of exactly
the two committed lists* — the prover knew both lists' openings (keys, ids, salts), both lists were
well formed, and `out[0]` is their intersection size (or the match bit). Each party checks the
program id before accepting the receipt, then reads `out[0]`. Nothing about the lists themselves
is on chain: not a key, not a count, not which keys were common.

A "no match" is `0` whichever side declined (or both), so the output does not say who did.

Any other claim has **no proof at all**: an unsorted list, a list that does not open the public
commitment, more than 16 keys, an unknown mode — the program refuses by reading a private input
index no caller can have committed, so the run has no trace.

## Trust model — read this before using it

**Whoever runs the call holds both lists in the clear.** The program is run by one party or by a
neutral third, with both lists and both salts as its private inputs. The proof guarantees that
the *published* result is the agreed function of the two *committed* lists — the other party's
list cannot be swapped, trimmed, or padded after it committed, and nobody on the chain or outside
it learns either list. It does **not** hide A's list from B when B is the prover, or either list
from a third party who proves. Hiding each list from the other party as well would need encrypted
inputs and a two-party protocol inside the program; that is out of scope here. Pick the prover
accordingly: a neutral matchmaker, or the party you were going to show your records to anyway.

**A commitment binds a list, not an identity.** In mode 1, each party commits to the id the other
knows it by; the program checks that the ids are on each other's lists, not that the committer *is*
that id. Accept a counterparty's commitment only over a channel you already trust to be theirs —
which, since you are handing the prover your list, you have to anyway.

**The result is the whole leak, by design.** `|A ∩ B|` is a number both agreed to publish; for
small lists it is a lot of information about them (two one-record lists and a count of 1 reveal
that the records are equal). Choose the mode, and whether the receipt goes on a public chain at
all, with that in mind. `n_A` and `n_B` are *not* output: they are inside the commitments, and
publishing them was not part of the agreement.

**What is public beside the result:** the program id, the two commitments and the mode (the
public input), the tier, and `H_IN`, a salted commitment to the 88 input words. By default
`rand call` also publishes the inputs sealed to the prover's own key (an *input envelope*,
reopened with `rand open-call <tx>`, which re-runs the program on them and compares with the
receipt). For a join that includes the other party's records you may prefer `./call.sh … --no-envelope`
(nothing published, no key opens the inputs later) or `--auditor <rand1…>` (a named auditor can
reopen them too, with `rand open-call <tx> --as-auditor`).

## The commitment

`C = POSEIDON2([TAG, n, id_lo, id_hi, k0_lo, k0_hi, …, k15_lo, k15_hi, s0, …, s7])`, a fixed
44-word message with a domain tag of its own (`"join"`), since guest-sdk's sponge does not pad.
`commit/` is a second guest that only computes this hash; `commit.sh` runs it on the emulator, so
a party commits with exactly the code the deployed program checks the commitment with. The rules
live in `core/` and are the same code on the guest and on the host (the `join` tool, the tests).

## Files

| file | |
|---|---|
| `src/main.rs` | the guest: syscalls in, `private_join_core::check`, outputs out |
| `core/src/lib.rs` | the rules, `no_std` on the guest, `std` on a host |
| `core/src/host.rs`, `core/src/bin/join.rs` | the off-chain tool: list files to words, the real hash through `commit/image.bin` |
| `core/tests/rules.rs` | nine host tests: both modes, every refusal, blinds ignored, list files |
| `commit/` | the off-chain committer guest (never deployed) |
| `lists/*.txt` | demo lists: A and B share 3 records and list each other; C shares nothing with A |
| `build.sh` | build both images inside your circuits checkout, reject any panicking path, run the tests |
| `commit.sh <list>` | make `<list>.secret` (the salt, mode 600) if absent and print the list's commitment |
| `public.sh "<C_A>" "<C_B>" <mode>` | write `join.txt`, the deploy-time public input |
| `run.sh` | emulator: three accepted cases and two refused ones, over the demo lists with fresh salts |
| `deploy.sh` | deploy with `join.txt` as the public input; saves the id to `program.id` |
| `call.sh <A> <A salt> <B> <B salt> [flags]` | one on-chain call from both lists; prints the receipt |

A list file holds keys (decimal or `0x` hex) one or more per line, `#` comments, and an optional
`self <key>` line, the party's own id. Keys must be strictly ascending; the file is handed to the
program as written, and `join inputs` says what the program will do with it. **Keep the salt
beside the list** — without it a committed list cannot be opened. `*.secret` and `join.txt` are
in `.gitignore`.

## Run it

```sh
export CIRCUITS=~/src/circuits            # see the top-level README for the rest
./build.sh
./run.sh
```

`run.sh`, off chain:

```
C_A = 3462358221 2877204138…   C_B = 2989686636 3186086810…   (public, with the mode)

mode 0, A and B (15 records each, 3 in common): accepted
out[0] = 3
out[1] = 0
out[2] = 0
out[3] = 0
out[4] = 0
out[5] = 0
out[6] = 0
out[7] = 0
cycles 3219
tier 12
tier 12: fits (cycles 3338 of 4095, Poseidon2 permutations 141 of 512)

mode 1, A and B (each lists the other's id): accepted, match
out[0] = 1
out[1] = 1
out[2] = 0
out[3] = 0
out[4] = 0
out[5] = 0
out[6] = 0
out[7] = 0
cycles 3051
tier 12
tier 12: fits (cycles 3170 of 4095, Poseidon2 permutations 141 of 512)

mode 1, A and C (A lists C; C does not list A): accepted, no match
out[0] = 0
out[1] = 1
out[2] = 0
out[3] = 0
out[4] = 0
out[5] = 0
out[6] = 0
out[7] = 0
cycles 2883
tier 12
tier 12: fits (cycles 3002 of 4095, Poseidon2 permutations 141 of 512)

mode 0, an unsorted list committed as A: refused
trap: InputIndex(4294967295)

mode 0, A with its last record dropped after committing: refused
trap: InputIndex(4294967295)
```

The image is 363 words and every call proves at tier 12, two full 16-key lists included. The
commitments differ from run to run because `run.sh` draws fresh salts; the outputs do not.

## On chain

Deployed and used on **chain 20**, the public testnet, on 2026-10-01, through the published RPC
`https://rpc.randprotocol.org`, with `rand` built from fullnode `5f872a58` (two deploy-config commits
past the v0.6.8 tag), by the scripts above exactly as written. Every proof was made on a laptop (an
M4 Max), the prover keeping about four cores busy on average. Fees are what the wallet paid; look a transaction up with `rand_getTransaction <hash>`, and
the cells and the vault as they are today with `rand program state <id>` / `rand program vault <id>`.

program id `b5dfff7843bc682cb2f9c143f33fe9fae8756471444ac4b8c44c1687ab32cb23`

| step | transaction | tier | fee (RAND) | result |
|---|---|---|---|---|
| `deploy.sh ` | `a87df1dcd6c48019b8512078ce331f009d90cda7aa02e673b38eb8a688da6209` |  | 0.039 | deployed |
| `call.sh lists/a.txt lists/a.secret lists/b.txt lists/b.secret` | `12beb8c3384fc36f63fe5ff2577831d73819e8e9613f8f0bcaf0a56a05184f4a` | 12 | 0.005932237 | outputs [3, 0, 0, 0, 0, 0, 0, 0] |
