# Clear-sign cases for Solana

Shared test cases for clear signing on Solana: real and adversarial transactions, with what [Codama](https://github.com/codama-idl/codama)'s own instruction display shows for them, so any other implementation (a wallet, a hardware signer, a Rust or mobile decoder) can check that it shows the same thing.

Clear signing on Solana started as [sRFC 39](https://github.com/solana-foundation/SRFCs/discussions/4) and now lives in the [Codama spec](https://github.com/codama-idl/spec) (display nodes), in the display metadata of the official [solana-program](https://github.com/solana-program) IDLs, and in `@codama/dynamic-instructions`, which renders them. These cases take that as the reference.

This is an early draft, built to be discussed with the people working on it. The format and the cases will change with their feedback.

## What a case holds

| Field | Content |
| --- | --- |
| `message` | The message a signer signs, base64 (legacy or v0) |
| `lookupTables` | For v0 messages: each lookup table's addresses |
| `idls` | The Codama IDLs used, under `idl/` |
| `context.accounts` | Account state the display may read (`{ data, owner }`, base64) |
| `expected.display` | What Codama's stable display shows, per instruction (`null`: it cannot identify the instruction) |
| `expected.displayNext` | The same with the Codama 2.0 release candidate |
| `expected.advisories` | What a careful wallet should flag on top: ours, not part of any spec |
| `notes` | Why the case is there, and what it shows |

`context.names`, `context.tokenAccounts` and `context.trustedPrograms` only feed the advisories: names and symbols are the wallet's business, not the display's.

## Cases

| Case | What it checks |
| --- | --- |
| `system-transfer` | A plain SOL transfer |
| `token-2022-transfer-checked` | The transfer of sRFC 39's own example, with the official Token-2022 IDL |
| `v0-lookup-transfer` | A USDC transfer whose mint and destination come from an address lookup table |
| `ata-create-both-forms` | Associated token account creation with data `[0]` and with empty data, both valid for the program |
| `durable-nonce-transfer` | A transfer signed with a durable nonce: the signature does not expire |
| `unlimited-approve` | An approval of `u64::MAX` to an unknown delegate |
| `near-max-approve` | An approval just under `u64::MAX` |
| `trezor-6843-co-signed-transfer` | A real Mainnet message from trezor-firmware#6843: a transfer that lists an extra signer |
| `squads-nested-approval` | A durable-nonce approval of a Squads proposal that hands over a program's upgrade authority, modeled on the Drift exploit |
| `lying-metadata` | A program whose own IDL calls an instruction an airdrop claim |

## What the cases show today

Last checked on 2026-11-17 with `@codama/dynamic-instructions` 1.0.0 and 2.0.0-rc.0:

1. **An extra account is invisible in the stable display.** In the `trezor-6843` message, the transfer lists an extra signer; 1.0.0 does not show it, 2.0.0-rc.0 lists it as a "Remaining Account" in the fields (not in the sentence).
2. **Two valid forms, two different gaps.** The Associated Token Account program reads both empty data and `[0]` as Create. The official IDL matches `[0]` only, so Codama shows the empty form as unknown; Trezor's firmware handles the empty form only ([trezor-firmware#4387](https://github.com/trezor/trezor-firmware/issues/4387)).
3. **Approvals read in base units.** `approve` carries no mint, so `u64::MAX` reads "18446744073709551615 base units". Ledger proposed a `maxThreshold` in the sRFC 39 discussion.
4. **A Squads approval shows addresses only.** What it executes sits in another account, which the instruction does not reference.
5. **A durable nonce shows as an account, not as a signature that never expires.**
6. **An absent optional value** reads "none" in 1.0.0 and "None" in 2.0.0-rc.0.
7. **Metadata that lies is displayed faithfully.** Only where the metadata comes from can be checked (curated lists, signed IDLs).

## Running

```
cargo run --bin gen                       # build the case inputs (same bytes every time)
npm --prefix tools/codama install
node tools/codama/generate.mjs            # fill the expected displays with Codama
node tools/codama/generate.mjs --check    # check they still match
cargo run --bin check                     # check decoding and advisories
cargo test
```

`idl/official/` holds the official IDLs at the commits listed in `idl/official/SOURCES.md`. `idl/custom/` holds IDLs written for these cases in the same format (Squads, the upgradeable loader, the lying airdrop). `real/` holds real transactions.

## License

MIT
