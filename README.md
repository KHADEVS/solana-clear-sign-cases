# Clear-sign cases for Solana

Shared test cases for clear signing on Solana: real and adversarial transactions, with what [Codama](https://github.com/codama-idl/codama)'s own instruction display shows for them, so any other implementation (a wallet, a hardware signer, a Rust or mobile decoder) can check that it shows the same thing.

Clear signing on Solana started as [sRFC 39](https://github.com/solana-foundation/SRFCs/discussions/4) and now lives in the [Codama spec](https://github.com/codama-idl/spec) (display nodes), in the display metadata of the official [solana-program](https://github.com/solana-program) IDLs, and in `@codama/dynamic-instructions`, which renders them. These cases take that as the reference.

This is an early draft, built to be discussed with the people working on it. The format and the cases will change with their feedback.

## What a case holds

| Field | Content |
| --- | --- |
| `message` | The message a signer signs, base64 (legacy, v0 or v1) |
| `lookupTables` | For v0 messages: each lookup table's addresses |
| `idls` | The Codama IDLs used, under `idl/` |
| `context.accounts` | Account state the display may read (`{ data, owner }`, base64) |
| `expected.display` | What Codama's stable display shows, per instruction (`null`: it cannot identify the instruction) |
| `expected.displayNext` | The same with the Codama 2.0 release candidate |
| `expected.advisories` | What a careful wallet should flag on top: ours, not part of any spec |
| `notes` | Why the case is there, and what it shows |
| `source` | For a real transaction: network, signature and slot |

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
| `jupiter-swap-wrap-sol` | A real Mainnet Jupiter swap: wrap SOL, route it into a token, close the wrapped account (v0, lookup table) |
| `jupiter-swap-v1` | A real Mainnet Jupiter swap in a v1 transaction (SIMD-0385), whose fee is in the header |
| `native-stake-delegate` | A real Mainnet stake delegation, in the older account list the official IDL does not describe |
| `jupiter-swap-owner-change` | The real swap with one more instruction: the token account it fills changes owner |
| `v1-decoy-compute-budget` | A v1 transaction whose compute budget instructions say 1 micro-lamport per unit while its header pays 0.5 SOL |

## What the cases show today

Last checked on 2026-11-21 with `@codama/dynamic-instructions` 1.0.0 and 2.0.0-rc.0:

1. **An extra account is invisible in the stable display.** In the `trezor-6843` message, the transfer lists an extra signer; 1.0.0 does not show it, 2.0.0-rc.0 lists it as a "Remaining Account" in the fields (not in the sentence).
2. **Two valid forms, two different gaps.** The Associated Token Account program reads both empty data and `[0]` as Create. The official IDL matches `[0]` only, so Codama shows the empty form as unknown; Trezor's firmware handles the empty form only ([trezor-firmware#4387](https://github.com/trezor/trezor-firmware/issues/4387)).
3. **Approvals read in base units.** `approve` carries no mint, so `u64::MAX` reads "18446744073709551615 base units". Ledger proposed a `maxThreshold` in the sRFC 39 discussion.
4. **A Squads approval shows addresses only.** What it executes sits in another account, which the instruction does not reference.
5. **A durable nonce shows as an account, not as a signature that never expires.**
6. **An absent optional value** reads "none" in 1.0.0 and "None" in 2.0.0-rc.0.
7. **Metadata that lies is displayed faithfully.** Only where the metadata comes from can be checked (curated lists, signed IDLs).
8. **The fee of a v1 transaction is in no instruction.** SIMD-0385 is active, and v1 made 940 of 1,916 non-vote transactions in six Mainnet blocks on 2026-11-21 (`real/README.md`). Its priority fee and compute limit sit in the message header, so a display that works one instruction at a time never shows them. Compute budget instructions in a v1 transaction are ignored for the fee, so a fee worked out from them can be a decoy.
9. **The Stake IDL describes one of two account lists.** The stake program still accepts the older account list for Delegate, Authorize, Withdraw, Deactivate and Merge, told apart by the Clock sysvar in one position. The official IDL describes only the newer list, so on a real delegation built the older way, Codama labels the Clock sysvar "Stake Authority". Only labels are affected: the program checks every signer whatever the list.
10. **A program without display metadata shows raw fields.** Jupiter publishes an Anchor IDL. Converted with Codama's own converter, its swap shows the discriminator as the first field, amounts in base units with no mint, and the route plan as JSON (1.0.0) or as labeled values (2.0.0-rc.0).

## Running

```
npm --prefix tools/codama install
node tools/codama/from-anchor.mjs         # convert the Anchor IDLs with Codama's converter
cargo run --bin gen                       # build the case inputs (same bytes every time)
node tools/codama/generate.mjs            # fill the expected displays with Codama
node tools/codama/generate.mjs --check    # check they still match
cargo run --bin check                     # check decoding and advisories
cargo test
```

`idl/official/` holds the official IDLs at the commits listed in `idl/official/SOURCES.md`. `idl/anchor/` holds IDLs that programs publish in Anchor's format, as read on chain and as converted by Codama (`idl/anchor/SOURCES.md`). `idl/custom/` holds IDLs written for these cases in the official format (Squads, the upgradeable loader, the lying airdrop). `real/` holds real transactions and a count of transaction versions on Mainnet.

## License

MIT
