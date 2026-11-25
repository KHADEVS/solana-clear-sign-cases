# Real transactions

Messages as they were signed on Mainnet, with the lookup tables they load. Each `.json` file was saved from a public RPC node: the message is the signed part of the transaction (signatures removed), and its lookup tables were checked against the addresses the chain loaded for it.

- `trezor-firmware-6843.hex`: the Mainnet message posted in https://github.com/trezor/trezor-firmware/issues/6843 (2026-04-28), a transfer that lists an extra signer.
- `jupiter-swap-wrap-sol.json`: a Jupiter swap from wrapped SOL into a token (v0, one lookup table), slot 472227052.
- `jupiter-swap-v1.json`: a Jupiter swap in the v1 format of SIMD-0385, slot 472230419.
- `native-stake-delegate.json`: a stake account created with a seed, initialized and delegated (legacy), slot 472228684.

The cases built on them give the signature in `source`.

## Surveyed blocks

`survey/` holds four Mainnet blocks of 2026-11-24 and 2026-11-25 (slots 473481103, 473545903, 473637703 and 473648503), as `tools/codama/survey.mjs` records them: every instruction, top-level or inner, of a program with an IDL here (6,435 of them in 1,431 transactions), with what both Codama releases show for it, and the accounts whose state a display read, as they were on 2026-11-25. `cargo run --release --bin compare` checks the Rust display against it.

`survey-day.json` lists the forty blocks of the larger run, from slot 473437903 to slot 473648503: 16,272 transactions and 75,938 instructions, 34,230 of them inner; 32,673 came in v0 transactions, 25,781 in legacy ones and 17,484 in v1. Slot 473545903 holds the one Token `Batch` of the run, an inner instruction with two transfers inside.

## v1 transactions on Mainnet

SIMD-0385 (transaction v1) is active. On 2026-11-21, six Mainnet blocks between slots 472226850 and 472227600 held 1,916 non-vote transactions: 940 v1, 573 v0 and 403 legacy. Of the successful ones, 536 were v1, 419 v0 and 341 legacy. Only 8 of the 940 v1 transactions carried compute budget instructions, which v1 ignores.
