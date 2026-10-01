# rpl-token — a shielded token, no contract

On Rand a token is not a program. An **RPL token** is an entry in the ledger's token registry —
name, symbol, decimals, a mint authority and a public total supply — and its balances are
ordinary shielded notes in the same commitment tree as RAND. Sending one is the same four-slot
bundle a RAND payment is, and the transaction does not say which asset moved.

| mint authority | created with | can mint later |
|---|---|---|
| none (fixed supply) | `./create-fixed.sh` | never: the whole supply is minted once, at registration |
| a key | `./create-mintable.sh` | whoever holds `authority.key.json` (a post-quantum Dilithium2 key), with `./mint.sh` |
| a program | `rand token create --program <id>` | only that RPL-2 program's invokes (see [`../../rpl2`](../../rpl2/)) |

Registration costs the registry's fee (1 RAND on the devnet) plus a bundle fee, in RAND.

## Scripts

| script | |
|---|---|
| `create-fixed.sh <name> <SYMBOL> <decimals> <supply>` | fixed supply, all to this wallet. `supply` is in whole tokens |
| `create-mintable.sh <name> <SYMBOL> <decimals> [initial]` | writes `authority.key.json` here (keep it private); optionally mints `initial` to this wallet |
| `mint.sh <token> <amount> [rand1…]` | mint more, signed by `authority.key.json` |
| `send.sh <token> <rand1…> <amount>` | a private transfer; the fee is paid in RAND |
| `burn.sh <token> <amount>` | destroy tokens this wallet holds; the public supply drops by exactly that |
| `info.sh [token]` | the token's public row (or every token) and this wallet's balance |

`<token>` is the registry index (`1`, `2`, …) or the token's id (`rpl1…` or 64 hex). Amounts in
`mint.sh`, `send.sh` and `burn.sh` are in the token's display units (`2.5` with 6 decimals is
2 500 000 base units).

## Run it

```sh
./create-mintable.sh "Example Coin" XMPL 6 1000     # registers token N, mints 1000 XMPL to you
./info.sh N
./mint.sh N 250
./send.sh N rand1… 100
./burn.sh N 50
./info.sh N                                          # supply 1200: 1000 + 250 minted − 50 burned (a send moves, it does not change supply)
```

## What is public

The registry row is: name, symbol, decimals, authority, total supply, and every mint's amount and
recipient key. Transfers are not: a send is a bundle of four nullifiers and four commitments
whose amounts, asset and parties are hidden. A burn reveals its amount (the supply has to drop by
it), not who burned.

The full standard, with the ERC-20 and SPL comparison, is `docs/tokens.md` in
[fullnode](https://github.com/randprotocol/fullnode).
