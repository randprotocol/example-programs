# RPL-2: programs with state and a vault

RPL-2 (fullnode v0.6.8) gives a program three things, behind the chain's `program_state` genesis
section:

| | |
|---|---|
| **cells** | a public map from an 8-word key to an 8-word value, per program. An absent cell reads as zeros; writing zeros removes it |
| **a vault** | a public balance per asset (0 is RAND). Value enters through an invoke's bundle and leaves as shielded notes the chain computes |
| **its own token** | a token registered with `rand token create --program <id>` is minted and burned only by that program's invokes |

| example | shows |
|---|---|
| [`counter/`](counter/) | one cell; a declared read and write; checking every context word; a stale read and the retry |
| [`vault/`](vault/) | a vault: deposits in, payouts out; a deploy-time public input (a lock) in the program id; a secret proved without being revealed; Poseidon2 inside a guest |
| [`kit/`](kit/) | what the DeFi examples below share: the context reader, 256-bit products, secrets as owners and operators, the host-side transition builder and the "flip every word" test |
| [`amm/`](amm/) | a constant-product pool of RAND and one token; shares the program alone mints; verify-don't-compute with a wallet that bisects the same inequality |
| [`stableswap/`](stableswap/) | Curve's StableSwap invariant for RAND and a pegged token; the invariant `D` is declared by the caller and checked exact, never computed |
| [`orderbook/`](orderbook/) | escrowed limit orders with partial fills; a ticket secret owns each order; why a taker's payment waits in the cell for the maker |
| [`lending/`](lending/) | a lending market: lenders' RAND for shares, borrowers' collateral token for RAND; an operator's price and a bounded interest index; 75 % to borrow, liquidation over 85 % |
| [`stablecoin/`](stablecoin/) | a collateralised-debt-position stablecoin: an operator's price, positions owned by secrets, 150 % to borrow, liquidation below 110 % |
| [`perp/`](perp/) | a perpetual-futures market: oracle price, an LP pool as every trader's counterparty, 10× leverage, a profit cap and a reserve rule that keep the vault solvent |
| [`crowdfund/`](crowdfund/) | all-or-nothing crowdfunding: pledges mint receipts, receipts refund, the creator claims once the goal is met |

## How an invoke works

```
rand program invoke <id> --transition t.json [--input …] [--inputs-file inputs.json]
```

`t.json` declares the whole transition (every field optional):

```json
{
  "reads":   [{ "key": "<64 hex>", "value": "<64 hex>" }],
  "writes":  [{ "key": "<64 hex>", "value": "<64 hex>" }],
  "deposit": { "rand": "<units>", "asset": 1, "amount": "<units>", "kind": "none | deposit | burn" },
  "pays":    [{ "asset": 0, "amount": "<units>", "to": "rand1…" }],
  "mints":   [{ "asset": 2, "amount": "<units>" }]
}
```

Keys and values are eight u32 words, each little-endian, as 64 hex. Amounts are base units
(1 RAND = 10⁹). `to` defaults to the invoking wallet.

The program is proved over its **context** — the transition as words — and only says yes or no.
The chain then applies the transition if every declared read still holds the value declared
(else `StaleRead`: `rand` exits 3; re-read and retry), the vault covers every payout, and every
mint is of the program's own token. Cells and vault balances are public; who paid in is not.

Context layout (`docs/program-state.md` in fullnode), after the program's public input and the
eight call-binding words:

| words | field |
|---|---|
| 0 | version, `1` |
| 1..=4 | `n_reads`, `n_writes`, `n_pays`, `n_mints` |
| 5, 6 | `burn_r` — RAND coming in (low, high) |
| 7 | inflow: 0 none, 1 deposit, 2 burn |
| 8 | `burn_asset` |
| 9, 10 | `burn_a` — the token coming in (low, high) |
| then | each read: key (8), value (8); each write: key (8), value (8) |
| then | each pay, then each mint: asset, amount low, amount high |

Limits: at most 8 reads, 8 writes, 4 payouts, and the context must fit 119 words for a program
with no public input. Creating a cell costs the chain's `cell_fee` (0.01 RAND on the devnet);
rewriting or deleting one is free.
