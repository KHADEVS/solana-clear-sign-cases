//! Writes the case inputs: `cargo run --bin gen`. Messages are rebuilt the
//! same way every time. The expected displays are then filled in by Codama's
//! own instruction display (`node tools/codama/generate.mjs`); the advisories,
//! which are ours and not normative, are written here by hand.

use base64::Engine;
use clear_sign_cases::message::{compile_legacy, compile_v0, AccountMeta, NewInstruction};
use clear_sign_cases::util::{anchor_discriminator, b58, find_pda, from_b58, short, test_key, Key};
use serde_json::{json, Value};
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
            "extra-accounts",
            format!(
                "\"Transfer SOL\" also passes an account its description does not list: {}",
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
