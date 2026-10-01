# credit-score — a lender's model, a borrower's private statements, a band in the receipt

An RPL-1 program: stateless, run with `rand call`. From [randprotocol.org/usecases](https://randprotocol.org/usecases/):

> The scoring model is the lender's program, published so the borrower can read it. The statements
> are the private input. The receipt is the score, and the lender never sees an individual
> transaction.

The lender deploys this guest with its six model parameters as the deploy-time public input, so the
program id names the exact rules. The borrower runs it over twelve months of bank statements —
income, rent or debt payment, end balance — that never leave their machine. The receipt carries the
**band** (0..=3) the model gave, how many months income covered the payment, and six words of a
salted commitment to the statements, so that the borrower can later *choose* to show them and
prove they are the ones that were scored.

```
public input (deploy)   model.txt: 6 words, the model's parameters (below)
private input (call)    40 words: [blind0, blind1, 12 × (income, payment, end_balance), salt0, salt1]
                        amounts in whole currency units (u32 each); the statements never leave your machine
outputs (receipt)       [band, months_positive, c0, c1, c2, c3, c4, c5]
                        c0..c5 = the first six words of POSEIDON2([TAG_STMT, the 36 statement words, salt0, salt1])
```

The two **blind** words are uniformly random, drawn from `/dev/urandom` per call and never output:
a call's proof leaks an unsalted function of the words the guest read, and twelve months of round
numbers would be guessable without them.

## The model

Six words, so a lender tunes the rules without changing code (and every change is a new program
id a borrower can see). `model.txt` here is `10 4000 500 1500 2000 3000`:

| word | name | example | |
|---|---|---|---|
| 0 | `min_months_positive` | 10 | gate: at least this many months with `income ≥ payment` |
| 1 | `max_dti_bps` | 4000 | gate: the year's payments over its income, at most this (basis points) |
| 2 | `min_avg_balance` | 500 | gate: average end balance at least this |
| 3 | `min_monthly_income` | 1500 | gate: average income at least this |
| 4 | `prime_dti_bps` | 2000 | a point: DTI at or under this |
| 5 | `prime_avg_balance` | 3000 | a point: average end balance at or over this |

**The band.** Any gate failing is band 0. All four passing is band 1, plus one point for the prime
DTI and one for the prime balance: 1, 2 or 3. Nothing divides: each rule is a product compared with
a product (`Σpayment · 10 000 ≤ Σincome · max_dti_bps`, `Σbalance ≥ 12 · min_avg_balance`), so no
word can make the guest trap and no average is rounded. Twelve u32 words sum to under 2^36 and the
two bps words are bounded at 10 000, so every product fits a u64; a model with a DTI word above
10 000 is **refused on every call** (no borrower can be scored by it, and no receipt says so).

The rules are one `no_std` crate, [`core/`](core/), that the guest, the off-chain hasher, the
`score` tool and the tests all run; the guest ([`src/main.rs`](src/main.rs)) only says where the
words come from.

## The commitment, and consent

The receipt's six words `c0..c5` are the first 192 bits of `POSEIDON2([TAG_STMT, the 36 statement
words, salt0, salt1])`, a fixed 39-word message with its own domain tag (`"stmt"`), since guest-sdk's
sponge does not pad. The salt is two random words `call.sh` draws once per statements file into
`<file>.secret` (mode 600, gitignored).

- **Without the salt**, the commitment says nothing: a lender who guessed the whole year could not
  confirm the guess without 2^64 hashes. (One word would be 2^32, minutes on a GPU; hence two.)
- **With the salt**, the borrower can later show the statements — to the lender on a dispute, to a
  court, to an auditor — and whoever receives them runs `./commit.sh <file> <salt file>`, which
  recomputes the digest with exactly the guest's hash code (`stmt-hash/`, a second guest never
  deployed) and compares it with the receipt. Numbers that differ by one unit in one month give a
  different digest.
- The commitment is to the statements alone, not the model: the same file with the same salt gives
  the same `c0..c5` under every lender's program (see the two `clean.txt` runs below). A borrower
  who does not want two applications linked deletes the `.secret` between them.

## Envelopes, auditors, and sealing nothing

By default `rand call` also publishes the call's **input envelope**: the forty private words, sealed
under a fresh per-call key that is wrapped to the caller's own viewing key, so the borrower can
`rand open-call <tx>` on any machine that holds their wallet key. The chain checks the envelope's
size and nothing else; it holds no key that opens it (fullnode `docs/confidential.md`, "Call input
envelopes"). `call.sh` adds two options from `docs/cli.md`:

```sh
./call.sh statements.txt                      # sealed to this wallet only
AUDITOR=rand1… ./call.sh statements.txt       # also sealed to that address
NO_ENVELOPE=1 ./call.sh statements.txt        # nothing sealed
```

- **`--auditor <rand1…>`** wraps the per-call key a second time, under an ML-KEM-768 encapsulation
  to that address. The named party — a regulator, or the lender's compliance desk — and only them
  can later run `rand open-call <tx> --as-auditor`, which decrypts the forty words, checks they are
  the preimage of the receipt's `H_IN`, re-runs the program on them and compares the outputs with the
  receipt's; a transcript that does not match **exits non-zero**. The lender who scores the
  application is not the auditor here, on purpose: naming the lender would hand them the statements
  the use case says they never see. Nothing in the envelope is visible to anyone else, and the
  per-call key is drawn fresh, so opening one call opens nothing about any other.
- **`--no-envelope`** publishes nothing. Once the borrower's salt for `H_IN` is gone (the wallet
  does not keep it), nobody — the borrower included — can ever open that call. `open-call` has
  nothing to fetch. The borrower who wants nothing sealed at all, ever, asks for this explicitly;
  a call is never quietly downgraded to it.

The input envelope and the statements' commitment are different things: the envelope is the
*whole input*, openable by the keys above; the commitment is six public words that can only ever
be *matched* against a file the borrower chooses to produce.

## What is proved, and what is not

A receipt for this program proves: someone ran exactly this model (the program id binds the code
and the six words; `call.sh` passes `--expect-public model.txt` so the wallet refuses before
proving if the chain's copy differs) over twelve months of statements they committed to in
`c0..c5`, and the model gave them `band` with `months_positive` covered months. The chain and the
lender learn those two numbers and nothing else about the year — not an income, not a balance, not
whether the missed month was the third or the ninth.

Be clear about what it does **not** achieve:

- **Nothing binds the statements to a bank.** The 36 words are typed by the borrower; a borrower
  can score any year they like. The receipt proves the model's verdict *on the numbers the borrower
  chose*. The commitment helps only after the fact: if the borrower consents to show the statements
  (or is compelled to), they must be the ones that were scored, and against the bank's own copies a
  lie is then caught. Binding at proof time needs the bank in the loop — a signature over the
  statements that the guest verifies, or a digest the bank publishes that becomes part of the public
  input — and that is a different, larger program.
- **The receipt names nobody.** A Rand transaction has no sender. Which applicant a receipt belongs
  to is the lender's business off chain (the borrower sends the transaction hash with the
  application; an auditor envelope can carry the link for a regulator).
- **`months_positive` is a leak**, a small one by design: thirteen possible values, so the lender
  can see *why* a band is 0. A lender who wants only the band removes the output word.
- **The model is public and the borrower knows it**, which is the point of the use case, and also
  means the statements can be tuned to it. That is true of any published rule.
- A refusal leaves no proof, so there is no receipt that says "eleven months" or "no such model";
  the model's "declined" is band 0 in an ordinary receipt.

**The same shape as the tax use case.** "A taxpayer proves that the tax computed on private income
is correct" is this program with the roles renamed: the authority publishes the schedule — brackets
and rates — as the deploy-time public input, so the program id *is* the year's tax law; the
taxpayer's income lines are the private input, with two blind words and a salt in front; the
receipt is the tax due (an amount in place of a band) and six words of a commitment to the return,
so an audit later can demand the lines and check them against what was proved. Both programs
prove the computation and leave the provenance of the inputs — a bank's statement, an employer's
wage slip — to a commitment someone else has to vouch for.

## Files

| file | |
|---|---|
| `core/src/lib.rs` | the rules: the input layout, the model, `score`, the commitment's message |
| `core/tests/rules.rs` | host tests: every band, every gate at its boundary, the layout, the message (14) |
| `core/src/bin/score.rs` | `score <model> <statements>`: what the receipt will say, before proving |
| `src/main.rs` | the guest: syscalls in, `core` rules, Poseidon2, eight words out |
| `stmt-hash/` | the off-chain helper guest (never deployed) that recomputes a file's commitment |
| `model.txt` | the deploy-time public input: six words |
| `statements/*.txt` | the demo years `run.sh` scores: 12 lines of `income payment balance`, `#` comments |
| `build.sh` | build `image.bin` and `stmt-hash/image.bin` in your circuits checkout, reject any panicking path, run the tests |
| `run.sh` | emulator: four accepted years, two refusals, and one year under a different model |
| `deploy.sh` | deploy with `model.txt` as the public input; saves the id to `program.id` |
| `call.sh <statements file>` | prove on chain; `AUDITOR=rand1…`, `NO_ENVELOPE=1` as above; the salt goes to `<file>.secret` |
| `score.sh <statements file> [model]` | the `score` tool |
| `commit.sh <statements file> [salt file]` | the commitment, recomputed with the guest's own hash |

`*.secret` and `program.id` are in `.gitignore`.

## Run it

```sh
export CIRCUITS=~/src/circuits            # see the top-level README for the rest
./build.sh
./run.sh
./score.sh statements/clean.txt
./deploy.sh
./call.sh statements/clean.txt
```

`run.sh`, off chain:

```
model (public): 10 4000 500 1500 2000 3000

clean.txt — every month covered, DTI 1960 bps, average balance 7141:
out[0] = 3
out[1] = 12
out[2] = 788916579
out[3] = 2382297328
out[4] = 4209283424
out[5] = 2558565551
out[6] = 3279955185
out[7] = 777863880
cycles 1634
tier 12
tier 12: fits (cycles 1757 of 4095, Poseidon2 permutations 133 of 512)

missed.txt — one month without income, DTI 3272 bps:
out[0] = 2
out[1] = 11
out[2] = 390563854
out[3] = 26646254
out[4] = 4161226443
out[5] = 906609869
out[6] = 3586907482
out[7] = 3701590753
cycles 1634
tier 12
tier 12: fits (cycles 1757 of 4095, Poseidon2 permutations 133 of 512)

stretched.txt — DTI 3500 bps, average balance 746:
out[0] = 1
out[1] = 12
out[2] = 427382305
out[3] = 3262865025
out[4] = 3222847015
out[5] = 2800321647
out[6] = 1999461890
out[7] = 2604659171
cycles 1634
tier 12
tier 12: fits (cycles 1757 of 4095, Poseidon2 permutations 133 of 512)

gap.txt — three months without income (9 of 12 covered), DTI 2666 bps:
out[0] = 0
out[1] = 9
out[2] = 1042149369
out[3] = 2468667922
out[4] = 414784959
out[5] = 3105685281
out[6] = 2433500445
out[7] = 523311907
cycles 1585
tier 12
tier 12: fits (cycles 1708 of 4095, Poseidon2 permutations 133 of 512)

eleven.txt — eleven months, 33 words:
trap: InputIndex(37)

clean.txt under a model with max_dti_bps = 0:
out[0] = 0
out[1] = 12
out[2] = 788916579
out[3] = 2382297328
out[4] = 4209283424
out[5] = 2558565551
out[6] = 3279955185
out[7] = 777863880
cycles 1599
tier 12
tier 12: fits (cycles 1722 of 4095, Poseidon2 permutations 133 of 512)

clean.txt under a model with max_dti_bps = 20000 (no such model):
trap: InputIndex(4294967295)
```

Reading the cases: the gap year has a DTI well under the cap and good balances, and is band 0
because 9 of 12 covered months is under the model's 10 — a gate is a gate, and `out[1] = 9` says
which. Eleven months is 33 words plus the blinds and salt, 37 in all, and the guest reads a fixed
twelve, so `read_input(37)` is past what the call committed: no run, no proof. The same clean year
under a model whose cap is 0 bps is band 0 with the *same* commitment words: the model, not the
data, set the bar, and the data are visibly the same. `run.sh` uses a fixed salt (`7 11`) so the
commitment words above are reproducible; `./commit.sh statements/clean.txt <a file holding "7 11">`
prints them, with the digest's last two words after.

The `score` tool says the same thing without the zkVM:

```
$ ./score.sh statements/missed.txt
months covered   11 of 12 (gate: at least 10)
income 44000, payments 14400: DTI 3272 bps (gate: at most 4000 bps; prime: at most 2000)
average income   3666 (gate: at least 1500)
average balance  4058 (gate: at least 500; prime: at least 3000)
band 2
```

The image is 437 words and every call proves at tier 12 (1 757 cycles and 133 Poseidon2
permutations: 110 for the program's own digest, 10 for the forty inputs, the rest for the public
words and the commitment — tier 10's budget is 1 023 cycles and 128 permutations). `stmt-hash` is
285 words and is never deployed.

## On chain

```sh
./deploy.sh                                   # rand program deploy image.bin --public model.txt → program.id
./call.sh statements/clean.txt                # draws statements/clean.secret, proves, prints the receipt
AUDITOR=rand1… ./call.sh statements/clean.txt # the regulator can `rand open-call <tx> --as-auditor`
rand open-call <tx>                           # the borrower, on any machine with their wallet key
```

The receipt's `outputs` are the eight words above, `h_in` is the salted commitment to all forty
private words, and `h_pub` is the digest of `model.txt`. A lender checks three things: the
`program` is the id they deployed, `outputs[0]` is the band, and the transaction hash is the one
the applicant sent them.
