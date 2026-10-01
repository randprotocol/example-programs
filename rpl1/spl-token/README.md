# spl-token — a Solana program on Rand

The Solana Program Library's **SPL Token** program, unchanged: its sBPF ELF is translated to
RISC-V C by `sbpf2rv`, compiled into a Rand zkVM image, checked word for word against Rand's sBPF
interpreter, and deployed. This is how an existing Solana program comes to Rand without a rewrite.

```
spl_token.so (sBPF ELF, 108 600 bytes)
   │  sbpf2rv                        → program.c (≈520 KB of C) + a shim crate
   │  rand-guest build --max-words 65535
   ▼
image.bin (65 096 words)  +  program.so as the deploy-time public input (27 151 words)
```

The image reads the ELF from its public input at every call, and carries the ELF's digest as a
guard, so the program id binds this exact ELF.

## Files

| file | |
|---|---|
| `build.sh [program.so]` | translate and build (default: SPL Token from your circuits checkout); writes `image.bin` and copies the ELF to `program.so` |
| `run.sh` | execute Transfer, MintTo, Burn and the refused cases off chain, translated image vs the interpreter (`sbpf2rv`'s parity test) |
| `deploy.sh` | deploy `image.bin` with `program.so` as the public input (refused by v0.6.8 nodes — see "Deploy") |
| `call.sh <words file>` | call it — see "Calling it" below |
| `image.bin`, `program.so` | committed, so `deploy.sh` needs no toolchain |

## Build

Needs a circuits checkout and clang **23.1.1** with the RISC-V backend (`brew install llvm`). The
toolchain is pinned so the same ELF gives the same image on any machine; another clang is refused
unless `RAND_GUEST_CLANG_UNPINNED=1`.

```sh
CLANG=/opt/homebrew/opt/llvm/bin/clang ./build.sh
```

```
entry pc 225: 30 function(s), 3546 block(s), 12061 instruction(s), 30 callx target(s), 0 refusal(s), 4 warning(s)
…
65096 words against a cap of 65535 (fits); 2 ecall(s) with a non-static a7
OK
wrote …/image.bin (65096 words, hc 8ca905ae3c62f503de7de86829040f323e098b53dba5fffe09b42f1aad16758b, …)
```

`0 refusal(s)`: every instruction translates. The four warnings are syscalls `Transfer` never
reaches (`sol_set_return_data`, `sol_get_sysvar`).

## Run it off chain

`./run.sh` runs every SPL Token vector through both the translated image and the interpreter
and demands identical outputs:

| instruction | status | cycles (translated) | tier |
|---|---|---:|---|
| Transfer 250 | 1 (ok) | 765 851 | 20 |
| Transfer more than the balance | 0 | 748 836 | 20 |
| MintTo 250 | 1 | 708 648 | 20 |
| MintTo by a non-authority | 0 | 696 694 | 20 |
| Burn 250 | 1 | 710 245 | 20 |
| Burn above the balance | 0 | 695 434 | 20 |

(measured by `sbpf2rv`'s parity test on 2026-09-18; fullnode `docs/translators.md` §5.6)

## Deploy

**A v0.6.8 node refuses this deploy.** Since v0.6.8 a node checks at deploy time that a program
can ever be called, and this one cannot: a call proves at tier 14 at most, and every SPL Token
instruction needs about tier 20. On chain 20 (2026-10-01):

```
a program of 65096 words (public input 27151 words) can never be called: a call proves at most
0 program words beside that public input at the highest tier a call may use — split the program
```

So on chain 20 and later this example is **off chain only**: `build.sh` and `run.sh` work,
`deploy.sh` is refused. Older chains accepted it — chain 13 and the pre-rebase durian devnet,
whose limits allow 65 096 words with 27 151 public words; there the fee counts both:
`0.001 + 0.0001 × (65 096 + 27 151)` = **9.2257 RAND**.

```sh
./deploy.sh
```

The same image was deployed on chain 13 as program
`740236918310f8e52bb0c1ef49b2b0e0c018762e289666660685b0694c8dd00a`.

## Calling it

Not yet possible on any machine we have. Each SPL Token instruction runs about 700 000 zkVM
cycles, which is tier 20, and a tier-20 call proof needs about 330 GB of memory. The program is
on chain 13 and its execution is checked off chain (`run.sh`); on v0.6.8 chains the deploy itself is
refused for the same reason (see "Deploy"). Calling it waits on splitting the program or a higher
call tier. `call.sh` is the command it will be: it takes the
instruction's ~10 000 words (the serialized accounts and data, as `sbpf2rv`'s
`SbpfCall::input_words()` builds them) and checks the chain's copy of the ELF before proving.

For a program that proves on a laptop today, see [`../threshold`](../threshold/); for one with
state and a vault, [`../../rpl2`](../../rpl2/).
