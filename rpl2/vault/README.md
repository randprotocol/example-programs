# vault — anyone pays in, only a secret's holder pays out

An RPL-2 program that **holds value**. Its vault — a public RAND balance the chain keeps for the
program — fills from deposits and empties through payouts the chain turns into ordinary shielded
notes. Withdrawing needs a secret; the proof shows the secret was known and reveals nothing of it.

| method | private inputs | transition | rule |
|---|---|---|---|
| 1, deposit | `[1]` | RAND in (`burn_r` > 0); nothing else | none: anyone may pay in |
| 2, withdraw | `[2, s0, …, s7]` | one RAND payout (> 0); nothing else | `POSEIDON2([TAG, s0..s7]) == lock` |

**The lock is the program's deploy-time public input** — eight words — so it is part of the
program id. There is no "set the lock" step anyone could front-run, and each lock is its own
program with its own vault.

**Who gets paid is not the program's business.** Recipients are not in the context; the
transaction binding fixes them. A withdrawal seen in the mempool cannot be redirected, because
its proof is bound to that transaction. The ledger checks that the vault covers the payout.

**What is public:** each deposit's amount (not who paid — the bundle names nobody), each payout's
amount and the recipient's public key, the vault's balance. The secret never leaves the
withdrawer's machine.

## The lock

`src/lock.rs` computes `POSEIDON2([TAG, s0, …, s7])` with guest-sdk's sponge — a fixed nine-word
message with its own domain tag, since the sponge does not pad. `lock-hash/` is a second guest
that only prints that hash; `lock.sh` runs it on the emulator, so the lock you deploy with is
computed by exactly the code that checks it.

## Files

| file | |
|---|---|
| `src/main.rs`, `src/lock.rs` | the vault guest |
| `lock-hash/` | the off-chain helper guest (never deployed) |
| `build.sh` | build both images and reject any panicking path |
| `lock.sh` | make `secret.txt` (8 random words, mode 600) and `lock.txt` (its lock) |
| `run.sh` | emulator: a deposit, a withdrawal with the secret, one with a wrong secret |
| `deploy.sh` | deploy with `lock.txt` as the public input; saves the id |
| `deposit.sh <RAND>` | pay RAND in |
| `withdraw.sh <RAND> [rand1…]` | pay RAND out, proving the secret; default recipient is this wallet |
| `show.sh` | the vault's balance |

**Keep `secret.txt`.** It is the only way to withdraw; lose it and the vault is locked for good.
It and `lock.txt` are in `.gitignore`.

## Run it

```sh
./build.sh && ./lock.sh && ./run.sh
./deploy.sh
./deposit.sh 5
./withdraw.sh 2
./show.sh
```

`run.sh`, off chain:

```
deposit 5 RAND:
out[0] = 1
out[1] = 705032704        5 000 000 000 units = lo 705032704, hi 1
out[2] = 1
tier 10: fits (cycles 166 of 1023, Poseidon2 permutations 86 of 128)

withdraw 2 RAND, right secret:
out[0] = 2
out[1] = 2000000000
out[2] = 0
tier 10: fits (cycles 376 of 1023, Poseidon2 permutations 92 of 128)

withdraw 2 RAND, wrong secret:
trap: InputIndex(4294967295)
```

The image is 199 words with the hash inside; both methods prove at tier 10.

## On chain
