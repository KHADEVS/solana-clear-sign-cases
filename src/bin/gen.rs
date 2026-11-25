//! Writes the case inputs: `cargo run --bin gen`. Messages are rebuilt the
//! same way every time. The expected displays are then filled in by Codama's
//! own instruction display (`node tools/codama/generate.mjs`); the advisories,
//! which are ours and not normative, are written here by hand.

use base64::Engine;
use clear_sign_cases::message::{
    compile_legacy, compile_v0, compile_v1, decode as decode_message, AccountMeta,
    CompiledInstruction, Config, NewInstruction,
};
use clear_sign_cases::util::{anchor_discriminator, b58, find_pda, from_b58, short, test_key, Key};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

fn key(text: &str) -> Key {
    from_b58(text).expect("address")
}

fn signer(key: Key, writable: bool) -> AccountMeta {
    AccountMeta {
        key,
        signer: true,
        writable,
    }
}

fn account(key: Key, writable: bool) -> AccountMeta {
    AccountMeta {
        key,
        signer: false,
        writable,
    }
}

fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn advisory(kind: &str, detail: String) -> Value {
    json!({ "kind": kind, "detail": detail })
}

struct Ids {
    system: Key,
    token: Key,
    token_2022: Key,
    ata: Key,
    compute_budget: Key,
    loader: Key,
    squads: Key,
    recent_blockhashes: Key,
    wsol: Key,
    usdc: Key,
}

fn ids() -> Ids {
    Ids {
        system: key("11111111111111111111111111111111"),
        token: key("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"),
        token_2022: key("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb"),
        ata: key("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL"),
        compute_budget: key("ComputeBudget111111111111111111111111111111"),
        loader: key("BPFLoaderUpgradeab1e11111111111111111111111"),
        squads: key("SQDS4ep65T869zMMBKyuUq6aD6EgTu8psMjkvj52pCf"),
        recent_blockhashes: key("SysvarRecentB1ockHashes11111111111111111111"),
        wsol: key("So11111111111111111111111111111111111111112"),
        usdc: key("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"),
    }
}

fn durable_nonce_detail(nonce: &str) -> String {
    format!(
        "this signature stays valid until nonce account {nonce} is advanced; a normal signature expires within minutes"
    )
}

fn system_transfer(ids: &Ids, from: Key, to: Key, lamports: u64) -> NewInstruction {
    let mut data = 2u32.to_le_bytes().to_vec();
    data.extend(lamports.to_le_bytes());
    NewInstruction {
        program: ids.system,
        accounts: vec![signer(from, true), account(to, true)],
        data,
    }
}

fn advance_nonce(ids: &Ids, nonce: Key, authority: Key) -> NewInstruction {
    NewInstruction {
        program: ids.system,
        accounts: vec![
            account(nonce, true),
            account(ids.recent_blockhashes, false),
            signer(authority, false),
        ],
        data: 4u32.to_le_bytes().to_vec(),
    }
}

fn transfer_checked(
    program: Key,
    source: Key,
    mint: Key,
    destination: Key,
    owner: Key,
    amount: u64,
    decimals: u8,
) -> NewInstruction {
    let mut data = vec![12];
    data.extend(amount.to_le_bytes());
    data.push(decimals);
    NewInstruction {
        program,
        accounts: vec![
            account(source, true),
            account(mint, false),
            account(destination, true),
            signer(owner, false),
        ],
        data,
    }
}

fn approve(ids: &Ids, source: Key, delegate: Key, owner: Key, amount: u64) -> NewInstruction {
    let mut data = vec![4];
    data.extend(amount.to_le_bytes());
    NewInstruction {
        program: ids.token,
        accounts: vec![
            account(source, true),
            account(delegate, false),
            signer(owner, false),
        ],
        data,
    }
}

fn token_transfer(
    ids: &Ids,
    source: Key,
    destination: Key,
    owner: Key,
    amount: u64,
) -> NewInstruction {
    let mut data = vec![3];
    data.extend(amount.to_le_bytes());
    NewInstruction {
        program: ids.token,
        accounts: vec![
            account(source, true),
            account(destination, true),
            signer(owner, false),
        ],
        data,
    }
}

/// A Token `Batch` (tag 255): each instruction as a count of accounts, the
/// length of its data and the data, with every account in one list.
fn batch(ids: &Ids, inner: &[NewInstruction]) -> NewInstruction {
    let mut data = vec![255];
    let mut accounts = Vec::new();
    for ix in inner {
        data.push(ix.accounts.len() as u8);
        data.push(ix.data.len() as u8);
        data.extend(&ix.data);
        accounts.extend(ix.accounts.iter().cloned());
    }
    NewInstruction {
        program: ids.token,
        accounts,
        data,
    }
}

/// The associated token account of `owner` for `mint` (SPL Token).
fn ata(ids: &Ids, owner: &Key, mint: &Key) -> Key {
    find_pda(&[owner, &ids.token, mint], &ids.ata).0
}

fn base_case(id: &str, title: &str, class: &str, idls: &[&str], message: &[u8]) -> Value {
    json!({
        "id": id,
        "title": title,
        "class": class,
        "idls": idls,
        "message": b64(message),
        "context": {},
        "expected": { "advisories": [] },
        "notes": []
    })
}

fn write(root: &Path, case: Value) {
    let dir = root.join("cases").join(case["id"].as_str().expect("id"));
    std::fs::create_dir_all(&dir).expect("case directory");
    let text = serde_json::to_string_pretty(&case).expect("json") + "\n";
    std::fs::write(dir.join("case.json"), text).expect("case.json");
    println!("wrote {}", dir.display());
}

fn token_2022_transfer_checked(ids: &Ids) -> Value {
    let alice = test_key("alice");
    let alice_wsol = test_key("alice wSOL account");
    // The destination of sRFC 39's own example.
    let toly_wsol = key("86xCnPeV69n6t3DnyGvkKobf9FdN2H9oiVDdaMpo2MMY");
    let ix = transfer_checked(
        ids.token_2022,
        alice_wsol,
        ids.wsol,
        toly_wsol,
        alice,
        10_000_000_000,
        9,
    );
    let mut case = base_case(
        "token-2022-transfer-checked",
        "Token-2022 transferChecked of 10 wrapped SOL, the transfer of sRFC 39's own example",
        "ordinary",
        &["official/token-2022.json"],
        &compile_legacy(alice, &[ix], test_key("blockhash 1")),
    );
    case["notes"] = json!([
        "The official IDL shows the mint as an address and takes the decimals from the instruction itself: names and symbols are left to the wallet."
    ]);
    case
}

fn system_transfer_case(ids: &Ids) -> Value {
    let alice = test_key("alice");
    let bob = test_key("bob");
    base_case(
        "system-transfer",
        "A plain SOL transfer",
        "ordinary",
        &["official/system.json"],
        &compile_legacy(
            alice,
            &[system_transfer(ids, alice, bob, 1_500_000_000)],
            test_key("blockhash 2"),
        ),
    )
}

fn durable_nonce_transfer(ids: &Ids) -> Value {
    let alice = test_key("alice");
    let eve = test_key("eve");
    let nonce = test_key("alice nonce account");
    let ixs = [
        advance_nonce(ids, nonce, alice),
        system_transfer(ids, alice, eve, 1_500_000_000),
    ];
    let mut case = base_case(
        "durable-nonce-transfer",
        "A SOL transfer signed with a durable nonce: the signature does not expire",
        "adversarial",
        &["official/system.json"],
        &compile_legacy(alice, &ixs, test_key("stored nonce value")),
    );
    case["expected"]["advisories"] = json!([advisory(
        "durable-nonce",
        durable_nonce_detail(&short(&nonce))
    )]);
    case["notes"] = json!([
        "The display names the nonce account but says nothing about how long the signature stays valid; SIMD discussion #513 proposes an explicit warning, and SIMD-0723 proposes removing durable nonces."
    ]);
    case
}

fn approval_case(ids: &Ids, id: &str, title: &str, amount: u64, note: &str) -> Value {
    let alice = test_key("alice");
    let alice_usdc = ata(ids, &alice, &ids.usdc);
    let delegate = test_key("unknown delegate");
    let mut case = base_case(
        id,
        title,
        "adversarial",
        &["official/token.json"],
        &compile_legacy(
            alice,
            &[approve(ids, alice_usdc, delegate, alice, amount)],
            test_key("blockhash 4"),
        ),
    );
    case["context"] = json!({
        "names": { b58(&alice): "alice.sol" },
        "tokenAccounts": { b58(&alice_usdc): { "owner": b58(&alice), "mint": b58(&ids.usdc) } }
    });
    case["expected"]["advisories"] = json!([advisory(
        "unlimited-approval",
        format!(
            "{} may spend practically every token in alice.sol's account, now and later, until the approval is revoked",
            short(&delegate)
        )
    )]);
    case["notes"] = json!([note]);
    case
}

/// A transfer anyone would expect, and an unlimited approval next to it, both
/// inside one Batch instruction.
fn token_batch_hidden_approve(ids: &Ids) -> Value {
    let alice = test_key("alice");
    let alice_usdc = ata(ids, &alice, &ids.usdc);
    let bob = test_key("bob");
    let bob_usdc = ata(ids, &bob, &ids.usdc);
    let delegate = test_key("unknown delegate");
    let inner = [
        token_transfer(ids, alice_usdc, bob_usdc, alice, 5_000_000),
        approve(ids, alice_usdc, delegate, alice, u64::MAX),
    ];
    let mut case = base_case(
        "token-batch-hidden-approve",
        "A Token Batch instruction that carries a transfer and an unlimited approval",
        "adversarial",
        &["official/token.json"],
        &compile_legacy(alice, &[batch(ids, &inner)], test_key("blockhash batch")),
    );
    case["context"] = json!({
        "names": { b58(&alice): "alice.sol", b58(&bob): "bob.sol" },
        "tokenAccounts": {
            b58(&alice_usdc): { "owner": b58(&alice), "mint": b58(&ids.usdc) },
            b58(&bob_usdc): { "owner": b58(&bob), "mint": b58(&ids.usdc) }
        }
    });
    case["expected"]["advisories"] = json!([
        advisory(
            "batched-instructions",
            "2 instructions are packed in one Batch instruction, which the display shows as bytes: Transfer tokens; Approve delegate".into()
        ),
        advisory(
            "unlimited-approval",
            format!(
                "inside the batch: {} may spend practically every token in alice.sol's account, now and later, until the approval is revoked",
                short(&delegate)
            )
        )
    ]);
    case["notes"] = json!([
        "The Token program runs every instruction packed in a Batch (tag 255), each with the next accounts of the list. Simulated on Mainnet on 2026-11-25 with no signature (node tools/codama/batch.mjs): a batch of a transfer and an unlimited approval left a real token account exactly as the two instructions did on their own, with the delegate set for 18446744073709551615. A batch with an unknown tag, and a batch inside a batch, were refused as invalid instructions.",
        "The official IDL types a batch as a list of (number of accounts, bytes) and gives the instruction no account. Codama 1.0.0 shows the bytes as base64 and no account at all; the 2.0 release candidate shows them as hex and the six accounts as remaining accounts. Neither says what the second instruction does.",
        "No display can do better from this IDL, and no type in the spec says that bytes hold an instruction. The advisory reads them the way the program does."
    ]);
    case
}

/// A confidential withdrawal whose two proofs were verified beforehand: the
/// instructions sysvar is not in the list, and the accounts after it move up.
fn confidential_withdraw_omitted_accounts(ids: &Ids) -> Value {
    let alice = test_key("alice");
    let mint = test_key("confidential mint");
    let alice_account = find_pda(&[&alice, &ids.token_2022, &mint], &ids.ata).0;
    let equality = test_key("equality proof context");
    let range = test_key("range proof context");
    // ConfidentialTransferExtension (27), Withdraw (6), amount, decimals, the
    // new decryptable balance (36 bytes, not shown), and the two proof
    // offsets: 0 means the proof waits in a context state account.
    let mut data = vec![27, 6];
    data.extend(5_000_000u64.to_le_bytes());
    data.push(6);
    data.extend([0u8; 36]);
    data.extend([0, 0]);
    let withdraw = NewInstruction {
        program: ids.token_2022,
        accounts: vec![
            account(alice_account, true),
            account(mint, false),
            account(equality, false),
            account(range, false),
            signer(alice, false),
        ],
        data,
    };
    let mut case = base_case(
        "confidential-withdraw-omitted-accounts",
        "A Token-2022 confidential withdrawal whose proofs wait in context accounts",
        "ordinary",
        &["official/token-2022.json"],
        &compile_legacy(alice, &[withdraw], test_key("blockhash confidential")),
    );
    case["context"] = json!({ "names": { b58(&alice): "alice.sol" } });
    case["expected"]["advisories"] = json!([advisory(
        "omitted-accounts",
        "\"Withdraw confidential tokens\" leaves out 1 of the 6 accounts its description lists, and the IDL does not say which: from account 3 on, a label may sit on another account".into()
    )]);
    case["notes"] = json!([
        "With both proof offsets at 0, each proof waits in a context state account and the instructions sysvar is not passed: the notes of the official IDL say so. The accounts are the token account, the mint, the equality context, the range context and the authority.",
        "The IDL marks the three accounts in the middle as optional, left out when absent. Codama 1.0.0 names accounts by position: it takes the equality context for the instructions sysvar, which is never shown, shows the range context as Equality Record and the authority as Range Record, and shows no Authority. The 2.0 release candidate names none of the accounts after the mint: Unidentified Account #1 to #3.",
        "Which accounts are present follows from the two offsets in the data, and no node of the spec can say that. Six Token-2022 instructions are built this way: confidential transfer, transfer with fee, withdraw, mint, burn and permissioned burn."
    ]);
    case
}

/// The real Mainnet message of trezor-firmware#6843.
fn trezor_6843(root: &Path) -> Value {
    let hex =
        std::fs::read_to_string(root.join("real/trezor-firmware-6843.hex")).expect("real message");
    let hex = hex.trim();
    let bytes: Vec<u8> = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).expect("hex"))
        .collect();
    let co_signer = key("trfb53BmkHNeoqaa3REgqnrbwUZqAFYdjTkivkJ6aWg");
    let mut case = base_case(
        "trezor-6843-co-signed-transfer",
        "A real Mainnet message: a SOL transfer that lists an extra signer, so the app co-signs and decides when it is sent",
        "adversarial",
        &[
            "official/compute-budget.json",
            "official/associated-token-account.json",
            "official/system.json",
            "official/token.json",
        ],
        &bytes,
    );
    case["expected"]["advisories"] = json!([
        advisory(
            "co-signed",
            format!(
                "valid only once {} also signs; whoever holds that signature decides when it is sent",
                short(&co_signer)
            )
        ),
        advisory(
            "extra-signers",
            format!(
                "\"Transfer SOL\" also passes a signer its description does not list: {}",
                short(&co_signer)
            )
        )
    ]);
    case["notes"] = json!([
        "Posted in trezor-firmware#6843 (2026-04-28): an app adds its own signer to a transfer so that it alone decides when the transaction is sent. Trezor's firmware refused it until #6992 (2026-05-26). It stays here as a regression case.",
        "Codama's stable display (@codama/dynamic-instructions 1.0.0) does not show the extra signer at all; the 2.0 release candidate lists it as a \"Remaining Account\" in the fallback fields (codama#1255). Neither interpolated sentence mentions it.",
        "Seven instructions, five of them setup: the sentence a wallet builds by joining the interpolated intents is long, the concern Trezor and Ledger raised in the sRFC 39 discussion."
    ]);
    case
}

fn ata_create_forms(ids: &Ids) -> Value {
    let alice = test_key("alice");
    let bob = test_key("bob");
    let carol = test_key("carol");
    let create = |owner: Key, data: Vec<u8>| NewInstruction {
        program: ids.ata,
        accounts: vec![
            signer(alice, true),
            account(ata(ids, &owner, &ids.usdc), true),
            account(owner, false),
            account(ids.usdc, false),
            account(ids.system, false),
            account(ids.token, false),
        ],
        data,
    };
    let ixs = [create(bob, vec![0]), create(carol, vec![])];
    let mut case = base_case(
        "ata-create-both-forms",
        "Create associated token accounts with data [0] and with empty data, both of which the program reads as Create",
        "ordinary",
        &["official/associated-token-account.json"],
        &compile_legacy(alice, &ixs, test_key("blockhash 7")),
    );
    case["expected"]["advisories"] = json!([advisory(
        "unidentified-instruction",
        "no instruction in the IDL of associatedTokenAccount matches this data (empty), so a display can only show it as unknown".into()
    )]);
    case["notes"] = json!([
        "The Associated Token Account program reads both empty data and [0] as Create. The official IDL matches [0] only, so the empty form is unknown to Codama's display; Trezor's firmware has the reverse gap, the empty form only (trezor-firmware#4387, open)."
    ]);
    case
}

fn v0_lookup_transfer(ids: &Ids) -> Value {
    let alice = test_key("alice");
    let bob = test_key("bob");
    let alice_usdc = ata(ids, &alice, &ids.usdc);
    let bob_usdc = ata(ids, &bob, &ids.usdc);
    let table = test_key("lookup table");
    let table_keys = [
        test_key("table entry 0"),
        ids.usdc,
        bob_usdc,
        test_key("table entry 3"),
    ];
    let ix = transfer_checked(
        ids.token, alice_usdc, ids.usdc, bob_usdc, alice, 25_000_000, 6,
    );
    let message = compile_v0(alice, &[ix], test_key("blockhash 8"), (table, &table_keys));
    let mut case = base_case(
        "v0-lookup-transfer",
        "A v0 USDC transfer whose mint and destination come from an address lookup table",
        "ordinary",
        &["official/token.json"],
        &message,
    );
    case["lookupTables"] = json!({ b58(&table): table_keys.iter().map(b58).collect::<Vec<_>>() });
    case["notes"] = json!([
        "The mint and the destination are not in the message: a signer must be given the lookup table's addresses to show the transfer at all. sRFC 39 does not say where a hardware wallet gets them."
    ]);
    case
}

/// Modeled on the Drift exploit: a council member pre-signs, with a durable
/// nonce, the approval of a Squads proposal whose transaction hands over a
/// program's upgrade authority.
fn squads_nested_approval(ids: &Ids) -> Value {
    let create_key = test_key("council create key");
    let (multisig, _) = find_pda(&[b"multisig", b"multisig", &create_key], &ids.squads);
    let member = test_key("council member 2");
    let nonce = test_key("council nonce account");
    let index = 7u64.to_le_bytes();
    let (proposal, proposal_bump) = find_pda(
        &[b"multisig", &multisig, b"transaction", &index, b"proposal"],
        &ids.squads,
    );
    let (transaction, transaction_bump) = find_pda(
        &[b"multisig", &multisig, b"transaction", &index],
        &ids.squads,
    );
    let (vault, vault_bump) = find_pda(&[b"multisig", &multisig, b"vault", &[0u8]], &ids.squads);
    let program = test_key("protocol program");
    let (program_data, _) = find_pda(&[&program], &ids.loader);
    let new_authority = test_key("unknown key");

    // Proposal { multisig, transaction_index, status: Active { timestamp }, bump, approved, rejected, cancelled }
    let mut proposal_data = anchor_discriminator("account", "Proposal").to_vec();
    proposal_data.extend(multisig);
    proposal_data.extend(index);
    proposal_data.push(1);
    proposal_data.extend(1_775_000_000i64.to_le_bytes());
    proposal_data.push(proposal_bump);
    for _ in 0..3 {
        proposal_data.extend(0u32.to_le_bytes());
    }
    // VaultTransaction { multisig, creator, index, bump, vault_index, vault_bump, ephemeral_signer_bumps, message }
    let mut transaction_data = anchor_discriminator("account", "VaultTransaction").to_vec();
    transaction_data.extend(multisig);
    transaction_data.extend(member);
    transaction_data.extend(index);
    transaction_data.extend([transaction_bump, 0, vault_bump]);
    transaction_data.extend(0u32.to_le_bytes());
    transaction_data.extend([1, 0, 1]); // one signer (the vault), read-only; one writable non-signer
    transaction_data.extend(4u32.to_le_bytes());
    for k in [vault, program_data, new_authority, ids.loader] {
        transaction_data.extend(k);
    }
    transaction_data.extend(1u32.to_le_bytes());
    transaction_data.push(3); // the loader
    transaction_data.extend(3u32.to_le_bytes());
    transaction_data.extend([1, 0, 2]); // program data, current authority (vault), new authority
    transaction_data.extend(4u32.to_le_bytes());
    transaction_data.extend(4u32.to_le_bytes()); // SetAuthority
    transaction_data.extend(0u32.to_le_bytes()); // no address lookup tables

    let mut approve_data = anchor_discriminator("global", "proposal_approve").to_vec();
    approve_data.push(0); // memo: None
    let approve = NewInstruction {
        program: ids.squads,
        accounts: vec![
            account(multisig, false),
            signer(member, true),
            account(proposal, true),
        ],
        data: approve_data,
    };
    let council = "Protocol council (2 of 5)";
    let mut case = base_case(
        "squads-nested-approval",
        "A durable-nonce approval of a Squads proposal whose transaction hands over a program's upgrade authority",
        "adversarial",
        &[
            "official/system.json",
            "custom/squads-v4.json",
            "custom/bpf-loader-upgradeable.json",
        ],
        &compile_legacy(member, &[advance_nonce(ids, nonce, member), approve], test_key("council stored nonce")),
    );
    case["context"] = json!({
        "names": { b58(&multisig): council, b58(&member): "council member 2", b58(&program_data): "Protocol program" },
        "accounts": {
            b58(&proposal): { "data": b64(&proposal_data), "owner": b58(&ids.squads) },
            b58(&transaction): { "data": b64(&transaction_data), "owner": b58(&ids.squads) }
        }
    });
    case["expected"]["advisories"] = json!([
        advisory("durable-nonce", durable_nonce_detail(&short(&nonce))),
        advisory(
            "nested-transaction",
            format!("approves transaction 7 of {council}, which runs: Set upgrade authority")
        ),
        advisory(
            "authority-change",
            format!(
                "inside transaction 7: the upgrade authority of Protocol program goes to {}",
                short(&new_authority)
            )
        )
    ]);
    case["notes"] = json!([
        "Modeled on the Drift exploit of April 1, 2026: an approval pre-signed with a durable nonce. Here the proposal's transaction changes a program's upgrade authority.",
        "What the proposal executes is stored in the VaultTransaction account, which the approval does not even reference: no display of this instruction alone can show it. Ledger asked for account-data formatters for Squads in the sRFC 39 discussion. The advisories decode it from the account data in the context, after checking the proposal and transaction addresses against their seeds.",
        "custom/squads-v4.json and custom/bpf-loader-upgradeable.json are written for these cases, in the official IDLs' format."
    ]);
    case
}

fn lying_metadata(ids: &Ids, root: &Path) -> Value {
    let alice = test_key("alice");
    let alice_usdc = ata(ids, &alice, &ids.usdc);
    let program = test_key("airdrop program");
    let idl = json!({
        "kind": "rootNode", "standard": "codama", "version": "1.0.0",
        "program": {
            "kind": "programNode", "name": "airdropClaim", "publicKey": b58(&program), "version": "0.0.0",
            "docs": ["A program that describes itself kindly: the display text below is its own, and nothing checks it."],
            "accounts": [], "definedTypes": [], "pdas": [], "errors": [],
            "instructions": [{
                "kind": "instructionNode", "name": "claim", "docs": [], "optionalAccountStrategy": "programId",
                "accounts": [
                    { "kind": "instructionAccountNode", "name": "claimer", "isWritable": true, "isSigner": true, "isOptional": false, "docs": [],
                      "display": { "kind": "instructionAccountDisplayNode", "skip": "always" } },
                    { "kind": "instructionAccountNode", "name": "userTokenAccount", "isWritable": true, "isSigner": false, "isOptional": false, "docs": [],
                      "display": { "kind": "instructionAccountDisplayNode", "label": "Your token account" } }
                ],
                "arguments": [
                    { "kind": "instructionArgumentNode", "name": "discriminator", "docs": [],
                      "type": { "kind": "numberTypeNode", "format": "u8", "endian": "le" },
                      "defaultValue": { "kind": "numberValueNode", "number": 1 }, "defaultValueStrategy": "omitted",
                      "display": { "kind": "structFieldDisplayNode", "skip": "always" } }
                ],
                "discriminators": [ { "kind": "fieldDiscriminatorNode", "name": "discriminator", "offset": 0 } ],
                "display": { "kind": "instructionDisplayNode", "intent": "Claim airdrop",
                    "interpolatedIntent": "Claim your 500 USDC airdrop into ${accounts.userTokenAccount}" }
            }]
        },
        "additionalPrograms": []
    });
    let path = root.join("idl/custom/airdrop-claim.json");
    std::fs::write(
        &path,
        serde_json::to_string_pretty(&idl).expect("json") + "\n",
    )
    .expect("idl");
    let ix = NewInstruction {
        program,
        accounts: vec![signer(alice, true), account(alice_usdc, true)],
        data: vec![1],
    };
    let mut case = base_case(
        "lying-metadata",
        "A program whose own IDL describes an instruction as an airdrop claim",
        "adversarial",
        &["custom/airdrop-claim.json"],
        &compile_legacy(alice, &[ix], test_key("blockhash 10")),
    );
    let trusted: Vec<String> = [
        ids.system,
        ids.token,
        ids.token_2022,
        ids.ata,
        ids.compute_budget,
    ]
    .iter()
    .map(b58)
    .collect();
    case["context"] = json!({ "trustedPrograms": trusted });
    case["expected"]["advisories"] = json!([advisory(
        "untrusted-metadata",
        format!(
            "\"Claim airdrop\" is how program {} describes itself; it is not on this wallet's trusted list, so the text may not match what the instruction does",
            short(&program)
        )
    )]);
    case["notes"] = json!([
        "No display can tell that this text is false: the program's own IDL says it. What can be checked is where the metadata comes from: the sRFC 39 discussion asks for curated lists of trusted programs, VisualSign checks IDL signatures against a curator allowlist, and the advisory uses such a list."
    ]);
    case
}

/// A real Mainnet transaction saved under `real/`: the message that was
/// signed, the lookup tables it loads, and where it comes from.
struct Real {
    message: Vec<u8>,
    tables: Value,
    signature: String,
    slot: u64,
}

fn real(root: &Path, name: &str) -> Real {
    let text = std::fs::read_to_string(root.join("real").join(name)).expect("real transaction");
    let json: Value = serde_json::from_str(&text).expect("real transaction json");
    Real {
        message: base64::engine::general_purpose::STANDARD
            .decode(json["message"].as_str().expect("message"))
            .expect("base64"),
        tables: json["lookupTables"].clone(),
        signature: json["signature"].as_str().expect("signature").to_string(),
        slot: json["slot"].as_u64().expect("slot"),
    }
}

fn real_case(id: &str, title: &str, class: &str, idls: &[&str], real: &Real) -> Value {
    let mut case = base_case(id, title, class, idls, &real.message);
    if real.tables.as_object().is_some_and(|t| !t.is_empty()) {
        case["lookupTables"] = real.tables.clone();
    }
    case["source"] =
        json!({ "network": "mainnet", "signature": real.signature, "slot": real.slot });
    case
}

const SWAP_IDLS: &[&str] = &[
    "official/compute-budget.json",
    "official/associated-token-account.json",
    "official/system.json",
    "official/token.json",
    "anchor/jupiter.json",
];

fn jupiter_swap_wrap_sol(root: &Path) -> Value {
    let tx = real(root, "jupiter-swap-wrap-sol.json");
    let mut case = real_case(
        "jupiter-swap-wrap-sol",
        "A real Jupiter swap: wrap SOL, route it into a token, close the wrapped account",
        "ordinary",
        SWAP_IDLS,
        &tx,
    );
    case["notes"] = json!([
        "A Mainnet transaction as signed (v0, one lookup table). Eight instructions, two of them the swap itself: compute budget, account creation, wrapping and closing make up the rest.",
        "Jupiter publishes an Anchor IDL with no display metadata. Converted with Codama's own converter (@codama/nodes-from-anchor), the route shows as \"Route V2\" with raw fields: the discriminator itself as the first field, amounts in base units with no mint, and the route plan as JSON (1.0.0) or as labeled values (2.0.0-rc.0). Which token is bought is in the account creation before it, not in the swap's display.",
        "The setup instructions are displayed one by one and correctly: they are what a summary would hide, which is what jupiter-swap-owner-change uses."
    ]);
    case
}

fn jupiter_swap_v1(root: &Path) -> Value {
    let tx = real(root, "jupiter-swap-v1.json");
    let mut case = real_case(
        "jupiter-swap-v1",
        "A real Jupiter swap in a v1 transaction (SIMD-0385): the fee is in the header",
        "ordinary",
        SWAP_IDLS,
        &tx,
    );
    case["expected"]["advisories"] = json!([advisory(
        "header-fee",
        "this v1 transaction sets its fee in the message header, which no instruction shows: priority fee 0.000012605 SOL, compute unit limit 229149".into()
    )]);
    case["notes"] = json!([
        "A Mainnet transaction as signed, in the v1 format of SIMD-0385: no lookup tables, no compute budget instructions, and the priority fee and compute limit in a config in the header. Of 1,916 non-vote transactions in six Mainnet blocks on 2026-11-21, 940 were v1.",
        "An instruction display, Codama's included, works one instruction at a time, so it never sees the fee of a v1 transaction: a wallet has to read the header for it.",
        "After the swap, two SOL transfers (0.0016 and 0.0004 SOL) go to addresses the swap does not use, most likely the fees of the app that built it. They display as plain transfers."
    ]);
    case
}

fn native_stake_delegate(root: &Path) -> Value {
    let tx = real(root, "native-stake-delegate.json");
    let mut case = real_case(
        "native-stake-delegate",
        "A real stake delegation: create a stake account with a seed, initialize it, delegate it",
        "ordinary",
        &[
            "official/compute-budget.json",
            "official/system.json",
            "official/stake.json",
        ],
        &tx,
    );
    case["expected"]["advisories"] = json!([advisory(
        "older-account-list",
        "\"Delegate stake\" uses the older account list, with the Clock sysvar as account 3, which the IDL does not describe: its account labels from account 3 on do not match these accounts".into()
    )]);
    case["notes"] = json!([
        "A Mainnet transaction as signed (legacy): 145.6 SOL into a new stake account, delegated to a validator.",
        "The stake program accepts two account lists for DelegateStake: the older one (stake, vote, Clock, stake history, config, then the authority) and a newer one (stake, vote, authority). It tells them apart by the Clock sysvar in third place (process_delegate in program/src/processor.rs). The official IDL describes only the newer list, so on this transaction Codama's display labels the Clock sysvar \"Stake Authority\". Authorize, Withdraw, Deactivate and Merge have the same two lists.",
        "Nothing here moves funds where the display does not say, and the program checks every signer whatever the list; only the labels are wrong."
    ]);
    case
}

/// The real Jupiter swap with one instruction added at the end: SPL Token
/// SetAuthority, AccountOwner, on the token account the swap fills.
fn jupiter_swap_owner_change(ids: &Ids, root: &Path) -> Value {
    let tx = real(root, "jupiter-swap-wrap-sol.json");
    let mut message = decode_message(&tx.message).expect("real message");
    let mut tables: HashMap<Key, Vec<Key>> = HashMap::new();
    for (table, addresses) in tx.tables.as_object().expect("tables") {
        let addresses = addresses
            .as_array()
            .expect("addresses")
            .iter()
            .map(|a| key(a.as_str().expect("address")))
            .collect();
        tables.insert(key(table), addresses);
    }
    let all = message.all_keys(&tables).expect("keys");
    let resolved = message.resolve(&tables).expect("instructions");
    let user = message.keys[0];
    // The token account the swap fills: the one the second account creation makes.
    let creations: Vec<_> = resolved.iter().filter(|ix| ix.program == ids.ata).collect();
    let bought = creations[1].accounts[1];
    let mint = creations[1].accounts[3];
    let index = |k: &Key| all.iter().position(|a| a == k).expect("key in message") as u8;
    let drainer = test_key("drainer");
    let mut data = vec![6, 2, 1];
    data.extend(drainer);
    message.instructions.push(CompiledInstruction {
        program: index(&ids.token),
        accounts: vec![index(&bought), index(&user)],
        data,
    });
    let mut case = base_case(
        "jupiter-swap-owner-change",
        "The real Jupiter swap with one more instruction: the token account it fills changes owner",
        "adversarial",
        SWAP_IDLS,
        &message.encode().expect("encode"),
    );
    case["lookupTables"] = tx.tables.clone();
    case["context"] = json!({
        "names": { b58(&user): "the signer" },
        "tokenAccounts": { b58(&bought): { "owner": b58(&user), "mint": b58(&mint) } }
    });
    case["expected"]["advisories"] = json!([advisory(
        "authority-change",
        format!(
            "the owner of the signer's token account goes to {}",
            short(&drainer)
        )
    )]);
    case["notes"] = json!([
        format!("Built from the real swap of jupiter-swap-wrap-sol ({}), with one instruction added at the end: SPL Token SetAuthority, AccountOwner, on the token account the swap fills. The swap still runs, and the tokens it buys belong to the new owner. The original signature does not cover this message.", tx.signature),
        "Codama's display shows the ninth instruction correctly. A wallet that sums up a swap in one line and folds the setup instructions away folds this one too: every instruction has to be accounted for, not only the swap."
    ]);
    case
}

/// A v1 transaction whose compute budget instructions ask for 1
/// micro-lamport per unit, while its header pays a priority fee of 0.5 SOL.
fn v1_decoy_compute_budget(ids: &Ids) -> Value {
    let alice = test_key("alice");
    let bob = test_key("bob");
    let mut limit = vec![2];
    limit.extend(200_000u32.to_le_bytes());
    let mut price = vec![3];
    price.extend(1u64.to_le_bytes());
    let budget = |data: Vec<u8>| NewInstruction {
        program: ids.compute_budget,
        accounts: vec![],
        data,
    };
    let ixs = [
        budget(limit),
        budget(price),
        system_transfer(ids, alice, bob, 10_000_000),
    ];
    let config = Config {
        priority_fee: 500_000_000,
        compute_unit_limit: Some(200_000),
        loaded_accounts_data_size_limit: None,
        heap_size: None,
    };
    let mut case = base_case(
        "v1-decoy-compute-budget",
        "A v1 transaction whose compute budget instructions say 1 micro-lamport per unit, while its header pays 0.5 SOL",
        "adversarial",
        &["official/compute-budget.json", "official/system.json"],
        &compile_v1(alice, &ixs, test_key("blockhash 11"), &config),
    );
    case["expected"]["advisories"] = json!([
        advisory(
            "header-fee",
            "this v1 transaction sets its fee in the message header, which no instruction shows: priority fee 0.5 SOL, compute unit limit 200000".into()
        ),
        advisory(
            "ignored-compute-budget",
            "2 compute budget instructions in this v1 transaction are ignored for the fee and the limits: a fee worked out from them is not the fee paid".into()
        )
    ]);
    case["notes"] = json!([
        "SIMD-0385: in a v1 transaction, compute budget instructions are ignored for the fee and the limits, and run as no-ops. The header's config sets them.",
        "Here the instructions set 1 micro-lamport per unit, less than a lamport in all, and the display shows exactly that; the header pays 0.5 SOL. A wallet that works out the fee from the instructions, as legacy and v0 require, shows the decoy. Trezor's firmware reads the header for v1 (core/src/apps/solana/transaction, since 2026-10-21).",
        "Real v1 transactions carry such instructions too: some apps add them out of habit (8 of 940 v1 transactions in the 2026-11-21 sample)."
    ]);
    case
}

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let ids = ids();
    let cases = [
        token_2022_transfer_checked(&ids),
        system_transfer_case(&ids),
        durable_nonce_transfer(&ids),
        approval_case(
            &ids,
            "unlimited-approve",
            "An SPL Token approval of u64::MAX to an unknown delegate",
            u64::MAX,
            "approve carries no mint, so the official IDL shows the amount in base units; Ledger proposed maxThreshold in the sRFC 39 discussion so that u64::MAX reads as unlimited.",
        ),
        approval_case(
            &ids,
            "near-max-approve",
            "An SPL Token approval just under u64::MAX",
            0xFEFF_FFFF_FFFF_FFFF,
            "Not u64::MAX, so a check for the exact maximum misses it; the advisory flags any approval from 2^63 up.",
        ),
        trezor_6843(&root),
        ata_create_forms(&ids),
        v0_lookup_transfer(&ids),
        squads_nested_approval(&ids),
        lying_metadata(&ids, &root),
        jupiter_swap_wrap_sol(&root),
        jupiter_swap_v1(&root),
        native_stake_delegate(&root),
        jupiter_swap_owner_change(&ids, &root),
        v1_decoy_compute_budget(&ids),
        token_batch_hidden_approve(&ids),
        confidential_withdraw_omitted_accounts(&ids),
    ];
    let keep: Vec<String> = cases
        .iter()
        .map(|c| c["id"].as_str().expect("id").to_string())
        .collect();
    for entry in std::fs::read_dir(root.join("cases"))
        .into_iter()
        .flatten()
        .flatten()
    {
        let name = entry.file_name().to_string_lossy().to_string();
        if entry.path().join("case.json").exists() && !keep.contains(&name) {
            println!("stale case directory, not generated any more: cases/{name}");
        }
    }
    for case in cases {
        write(&root, case);
    }
}
