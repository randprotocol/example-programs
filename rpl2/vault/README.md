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

The image is 305 words with the Poseidon2 call inside (`lock-hash` is 199); both methods prove at tier 10.

## On chain

The images `build.sh` produces (built images are not committed; `build.sh` reproduces them byte
for byte with the pinned toolchain):

| | vault | lock-hash |
|---|---|---|
| `image.bin` sha256 | `751dc1743988eeb7bc8696af6a60d35bec9ec5c167e280c3b9b66729a8f3a740` | — |
| `hc` (`rand-guest`) | `77c0215ea0055668bba46648275965f57a5209f44eacdb6a2949e65d5f912dd6` | `5264de0edea976386a113726d7df9bfc00b4f3da2808b5c0a240e223184388bd` |
| program id, code only | `8a6b5a6d8a9c8f0d7bcc3db36457467f1a522040ee10f68f3af8ef489c00389b` | (never deployed) |

The deployed id also binds the lock, so every `lock.sh` gives a different program and vault; the
run below is one of them.


Run on the durian devnet (chain 1919, fullnode v0.6.8, production FRI) on 2026-10-01, from a
wallet holding 99.93 RAND:

```
$ ./deploy.sh
program id: 08c2ccb5db4d4fd18b13e466c7704e3c6ecbfb024f22b6ba4b5ffd4a1fbba551 (305 words, …, public input 8 words, …)
submitted deploy ba445593f9807fe89727551c0234c710248a0eb500ed70f66993e46058af1f72   fee 0.0323 RAND

$ ./deposit.sh 5
proved in 28.5s: tier 10, 1383183 bytes, outputs [1, 705032704, 1, 0, 0, 0, 0, 0]
submitted invoke a0c5b644de97063f6b83c256d7e7976ac11af58c578de145e67f108aa8267efc
  0 RAND out, 5 RAND burned, 94.9271664 RAND change, fee 0.0043024 RAND, …
$ ./show.sh
"vault": [{ "amount": "5000000000", "asset": 0 }]

$ ./withdraw.sh 2
proved in 24.2s: tier 10, 1388302 bytes, outputs [2, 2000000000, 0, 0, 0, 0, 0, 0]
submitted invoke 9f2ab01a6f4f6309d6de88b1d684a66b8144e099e2a65162ccc2eab4f2dbb8bf
  0 RAND out, 94.9228384 RAND change, fee 0.004328 RAND, …
$ ./show.sh
"vault": [{ "amount": "3000000000", "asset": 0 }]
$ rand balance
balance: 96.9228384 RAND          94.92 change + the 2 RAND payout, a new note
```
