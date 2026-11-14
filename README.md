# Clear-sign cases for Solana

Conformance cases for [sRFC 39 (Solana Clear Sign)](https://github.com/solana-foundation/SRFCs/discussions/4), and a small renderer that follows the draft as literally as it can, to check them against.

Each case is a real serialized transaction, the enriched IDLs it needs, and the exact text a renderer must produce. Any wallet, hardware signer or decoder can run the cases to see whether it shows what the spec says.

This is an early draft, built to propose a format in the sRFC 39 discussion. The format and the cases will change with the feedback there.

## Two layers

- **Rendering** follows sRFC 39: the interpolated intent, the fallback list, and the fallback without additional metadata (the three outputs in the draft's own `transferChecked` example).
- **Advisories** are what a careful wallet should flag on top of the rendering: a durable nonce, an unlimited approval, an authority change, a transaction nested in a multisig approval. The draft does not cover them, so they are kept apart and are not normative.

## Cases

| Case | What it checks |
| --- | --- |
| `spec-transfer-checked` | The draft's own example: Token-2022 `transferChecked`, 10 wSOL to toly.sol |
| `system-transfer` | A plain SOL transfer to a named address |
| `durable-nonce-transfer` | A transfer signed with a durable nonce: the signature does not expire |
| `unlimited-approve` | An SPL Token approval of `u64::MAX` to an unknown delegate |
| `squads-nested-approval` | A durable-nonce approval of a Squads proposal that hands over a program's upgrade authority, modeled on the Drift exploit |

## Layout

```
cases/<id>/case.json   one case
idl/<program>.json     the enriched IDLs the cases name (Codama programNode, sRFC 39 display nodes)
```

`case.json`:

| Field | Content |
| --- | --- |
| `message` | The message a signer signs, base64 (legacy or v0) |
| `lookupTables` | For v0 messages: each lookup table's addresses |
| `idls` | IDL files under `idl/` |
| `context` | The additional metadata a wallet may have: `names` (address book, name service), `tokens` (symbol, decimals), `tokenAccounts` (owner, mint), `accounts` (raw account data, base64) |
| `expected.interpolated` | The interpolated intents, joined with " and " |
| `expected.fallback` | The fallback lines, with the context |
| `expected.fallbackWithoutMetadata` | The fallback lines, with no context at all |
| `expected.advisories` | `{ kind, detail }`, not normative |
| `notes` | Where the draft is ambiguous, and the reading this case takes |

## Running

```
cargo run --bin check            # the renderer against every case
cargo run --bin check -- <dir>   # against another set of cases
cargo run --bin gen              # rebuild the cases (same bytes every time)
cargo test
```

The expected outputs are written by hand in `src/bin/gen.rs`, never computed by the renderer they check.

## What the first cases found in the draft

1. The example's expected text ends with a period that its template does not have.
2. `{destination}` names the token account, but the expected text shows its owner; another section writes `{destination.owner}`.
3. The mint's label is "Token Mint", but the output without metadata reads "Token".
4. `skip: "withAdditionalMetadata"` is defined as "SHOULD NOT display the account unless additional metadata is available", yet the example shows the mint only when metadata is missing.
5. `amountFormatterNode.decimals` is defined as a number of places, but the example passes the name of an argument.
6. A Squads approval cannot be rendered meaningfully: what it executes lives in another account, which the instruction does not reference (the account-data formatters Ledger asked for).
7. Nothing says how to show an absent optional value, or an instruction from a program with no IDL.

Each case's `notes` say which reading it takes.

## License

MIT
