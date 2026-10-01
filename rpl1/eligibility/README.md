# eligibility — prove one fact about a credential, not which credential

An RPL-1 program: stateless, run with `rand call`. From [randprotocol.org/usecases](https://randprotocol.org/usecases/),
"Prove eligibility without surrendering identity":

> A credential signed by an issuer is a private input, and the program checks the single
> predicate the service requires. The receipt is the answer, bound to the issuer's key and
> nothing else.

Here the issuer's "signature" is a **Poseidon2 Merkle root** over its roll of credentials, and
the predicate is one inequality, `birth_year ≤ cutoff_year` ("born in 2008 or earlier": 18 or
older in 2026). The holder proves that *some* credential under the issuer's root satisfies it.
The chain learns that, the root and the cutoff — and not which credential, whose it is, or the
birth year itself.

```
public input (deploy)   issuer.txt: root0..7 (the issuer's tree), cutoff_year            9 words
private input (call)    blind0, blind1, id0..id3, birth_year, nonce,                     80 words
                        then 8 × (sib0..sib7, dir)         — the credential and its path, never leave your machine
outputs (receipt)       [1, cutoff_year, root0, root1, root2, root3, root4, root5]
```

A credential that misses the cutoff, a path that does not lead to the root, or a direction word
that is not 0 or 1 all **refuse**: the program reads a private input index no caller can have
committed, so the run has no trace and there is no proof. A receipt for this program can only
ever say "yes".

## The tree

A credential is `[id0, id1, id2, id3, birth_year, nonce]`: a 128-bit holder id, a year, and a
random word so that a leaf cannot be brute-forced from the root even when the id and the year
are guessable. The issuer puts up to 256 of them in a depth-8 tree; an unused slot is eight zero
words, which no credential hashes to.

```
leaf = POSEIDON2([TAG_LEAF, id0, id1, id2, id3, birth_year, nonce, 0, 0])    9 words, TAG_LEAF = "leaf"
node = POSEIDON2([TAG_NODE, left0..7, right0..7])                            17 words, TAG_NODE = "node"
```

Both messages are a fixed length with their own domain tag, since guest-sdk's sponge does not
pad. The hashing code is one file, `src/hash.rs`, which the deployed guest and the off-chain
hasher `hash/` both include, and the `issuer` tool computes every leaf and node by running
`hash/image.bin` on the zkVM's emulator — so the root the issuer publishes is computed by
exactly the code the program recomputes it with, not a host reimplementation. (Memoised: the demo
roll of five credentials costs 23 emulator runs; a full tree 511, each a few milliseconds.)

The holder's path is, per level from the leaf up, the sibling node (eight words) and a
direction word: `0` when the path is the left child at that level, `1` when it is the right.
The program recomputes the leaf, folds the path to a root, and accepts only if that root is the
public one and the year clears the cutoff. **The program id binds the root and the cutoff**
(`rand-program-2` hashes the code and the public input), so a different issuer or a different
bar is a different program; the receipt repeats six words of the root so that anyone reading it
sees the issuer without looking the program up.

## Trust model, and what is not achieved

- **The issuer is trusted for the roll's content**, as any credential issuer is. The proof says
  that the issuer put a credential with that birth year in its tree, not that the year is true.
- **The chain learns that some credential in the tree satisfies the predicate**, not which: the
  credential, its slot and its path are private inputs, committed only through the salted `H_IN`.
  Two random **blind words** go first in every call (the scripts draw them from `/dev/urandom`),
  so a proof's input commitment cannot be brute-forced from a guessable credential; they are
  never output.
- **The holder's id never leaves their machine.** The issuer hands each holder their own
  `issuer path` words once; after that the issuer is not involved in a call.
- **A receipt is not a login.** Nothing ties a receipt to a session, a service or a moment: anyone
  who sees one can show it to anyone. A service that wants "this holder, now" must put its
  challenge in the call — as a public word it checks (a new program per challenge) or, more
  practically, by having the holder run `rand call --auditor <the service's address>`: the call's
  input envelope is then sealed to the holder *and* the service, and the service alone, with
  `rand open-call <tx> --as-auditor`, can open the inputs, check them against the receipt's `H_IN`
  and re-run the program on them. That makes the service the one party able to see the credential,
  which is the usual trade for a login; a receipt alone is only "someone in this tree qualifies".
- **No revocation.** A credential in the tree stays provable for as long as the root is deployed.
  Revoking is a new root — a new program — and services moving to it; the use-case page's
  warning stands: a credential that cannot be revoked will be sold.
- **Two holders cannot be told apart**, and one holder calling twice cannot be linked: every
  call's `H_IN` is freshly salted. The flip side is that nothing stops one credential from
  producing unlimited receipts.

## Files

| file | |
|---|---|
| `src/main.rs` | the guest: syscalls in, `eligibility_core::check`, outputs or refusal out |
| `src/hash.rs` | the two Poseidon2 messages; shared with `hash/` by `#[path]` |
| `core/src/lib.rs` | the rules, `no_std` on the guest and `std` on a host, through a `Source` trait |
| `core/src/host.rs` | the tree, the holder's witness, the emulator-backed hash, a `Mock` source |
| `core/src/bin/issuer.rs` | `issuer root <roll> [cutoff]` → `issuer.txt`'s words; `issuer path <roll> <index>` → a holder's words |
| `core/tests/rules.rs` | host tests: accepted at every slot; refused for the cutoff, every sibling, every direction, a stranger, a tampered year, another root, a wrong slot, an empty slot, a short input |
| `hash/` | the off-chain hasher guest (never deployed): mode 0 hashes 9 words, mode 1 hashes 17 |
| `credentials.example` | a demo roll: `<32 hex id> <birth_year> <nonce>` per line, slot order |
| `issuer.txt` | the deploy-time public input: the demo roll's root and the cutoff 2008 |
| `build.sh` | build both images inside your circuits checkout, reject any panicking path, run the host tests |
| `root.sh <roll> <cutoff_year>` | the issuer's step: build the tree on the emulator and write `issuer.txt` |
| `run.sh [cutoff_year]` | emulator: two credentials accepted, then the cutoff missed, a wrong sibling, a direction of 2 |
| `deploy.sh` | deploy with `issuer.txt` as the public input; saves the id to `program.id` |
| `call.sh <roll> <index> [rand call flags]` | prove on chain for the credential in that slot; pass `--auditor rand1…` to let a service open the inputs |

A real roll is the issuer's private file (`credentials.secret`, gitignored); `credentials.example`
is made-up data for the demo. Each nonce is a random `u32` (`od -An -N4 -tu4 /dev/urandom`).

## Run it

```sh
export CIRCUITS=~/src/circuits            # see the top-level README for the rest
./build.sh
./root.sh credentials.example 2008        # writes issuer.txt (already committed for the demo roll)
./run.sh
./deploy.sh
./call.sh credentials.example 1           # slot 1: born 2004, clears 2008
```

`run.sh`, off chain:

```
5 credentials in 256 slots; 23 distinct hashes run on the emulator
issuer root 717960973 1268762641… (public), cutoff 2008 (public)

slot 0, born 1990 (private): accepted
out[0] = 1
out[1] = 2008
out[2] = 717960973
out[3] = 1268762641
out[4] = 1496169649
out[5] = 1648399979
out[6] = 1053893688
out[7] = 1041240009
cycles 3383
tier 12
tier 12: fits (cycles 3518 of 4095, Poseidon2 permutations 178 of 512)

slot 1, born 2004 (private): accepted
out[0] = 1
out[1] = 2008
out[2] = 717960973
out[3] = 1268762641
out[4] = 1496169649
out[5] = 1648399979
out[6] = 1053893688
out[7] = 1041240009
cycles 3381
tier 12
tier 12: fits (cycles 3516 of 4095, Poseidon2 permutations 178 of 512)

slot 1 against cutoff 2003: refused
trap: InputIndex(4294967295)

slot 1 with one sibling word changed: refused
trap: InputIndex(4294967295)

slot 1 with a direction word of 2: refused
trap: InputIndex(4294967295)
```

The image is 441 words and every accepted call proves at tier 12: eight 17-word node hashes
and one 9-word leaf hash, with the digests of the image and of the inputs, are 178 Poseidon2
permutations, over tier 10's 128 (and 3 516 cycles, over its 1 023). The `hash/` image is 363
words and is never deployed.

`call.sh` passes `--expect-public issuer.txt`, so the wallet refuses before proving if the
program on chain was deployed with another issuer's root or another cutoff. By default
`rand call` also publishes the call's inputs sealed to your own key (an *input envelope*, which
`rand open-call <tx>` reopens); `--auditor` seals them to one more party, as described above,
and `--no-envelope` keeps even that off chain.

## On chain

Deployed and used on **chain 20**, the public testnet, on 2026-10-01, through the published RPC
`https://rpc.randprotocol.org`, with `rand` built from fullnode `5f872a58` (two deploy-config commits
past the v0.6.8 tag), by the scripts above exactly as written. Every proof was made on one CPU core of
a laptop. Fees are what the wallet paid; look a transaction up with `rand_getTransaction <hash>`, and
the cells and the vault as they are today with `rand program state <id>` / `rand program vault <id>`.

program id `251f911628e659601f3e68127ab8837572f4364daeb7a08c43cad88820c43b25`

| step | transaction | tier | fee (RAND) | result |
|---|---|---|---|---|
| `deploy.sh ` | `eaff073788c6fde7fec1830f7c60426c2949510be1d3fda49479b6ac59e81939` |  | 0.046 | deployed |
| `call.sh credentials.example 0` | `1eb5cf60b49b069f580b5290fca019ddabe8d35f06dd1ac94e2ca091063fb094` | 12 | 0.005932237 | outputs [1, 2008, 717960973, 1268762641, 1496169649, 1648399979, 1053893688, 1041240009] |
