//! Writes the cases: `cargo run --bin gen`. Messages are rebuilt the same way
//! every time; expected outputs are written here by hand, never computed by
//! the renderer they check.

use base64::Engine;
use clear_sign_cases::message::{compile_legacy, AccountMeta, NewInstruction};
use clear_sign_cases::util::{anchor_discriminator, b58, find_pda, from_b58, short, test_key, Key};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

const SRFC: &str = "sRFC 39 draft (solana-foundation/SRFCs discussion #4), as of 2026-06-11";

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

struct Ids {
    system: Key,
    token: Key,
    token_2022: Key,
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
        loader: key("BPFLoaderUpgradeab1e11111111111111111111111"),
        squads: key("SQDS4ep65T869zMMBKyuUq6aD6EgTu8psMjkvj52pCf"),
        recent_blockhashes: key("SysvarRecentB1ockHashes11111111111111111111"),
        wsol: key("So11111111111111111111111111111111111111112"),
        usdc: key("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"),
    }
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

fn advisory(kind: &str, detail: String) -> Value {
    json!({ "kind": kind, "detail": detail })
}

fn write(root: &Path, case: Value) {
    let dir = root.join("cases").join(case["id"].as_str().expect("id"));
    std::fs::create_dir_all(&dir).expect("case directory");
    let text = serde_json::to_string_pretty(&case).expect("json") + "\n";
    std::fs::write(dir.join("case.json"), text).expect("case.json");
    println!("wrote {}", dir.display());
}

/// The draft's own example: Token-2022 transferChecked of 10 wSOL to toly.sol.
fn spec_transfer_checked(ids: &Ids) -> Value {
    let alice = test_key("alice");
    let toly = test_key("toly");
    let alice_wsol = test_key("alice wSOL account");
    // The token account the draft's example shows, so the output without
    // metadata can match its "86xCn...o2MMY".
    let toly_wsol = key("86xCnPeV69n6t3DnyGvkKobf9FdN2H9oiVDdaMpo2MMY");
    let mut data = vec![12];
    data.extend(10_000_000_000u64.to_le_bytes());
    data.push(9);
    let ix = NewInstruction {
        program: ids.token_2022,
        accounts: vec![
            account(alice_wsol, true),
            account(ids.wsol, false),
            account(toly_wsol, true),
            signer(alice, false),
        ],
        data,
    };
    let message = compile_legacy(alice, &[ix], test_key("blockhash 1"));
    json!({
        "id": "spec-transfer-checked",
        "title": "The draft's own example: Token-2022 transferChecked, 10 wSOL to toly.sol",
        "class": "ordinary",
        "srfc39": SRFC,
        "idls": ["token-2022.json"],
        "message": b64(&message),
        "context": {
            "names": { b58(&toly): "toly.sol" },
            "tokens": { b58(&ids.wsol): { "symbol": "wSOL", "decimals": 9 } },
            "tokenAccounts": {
                b58(&toly_wsol): { "owner": b58(&toly), "mint": b58(&ids.wsol) },
                b58(&alice_wsol): { "owner": b58(&alice), "mint": b58(&ids.wsol) }
            }
        },
        "expected": {
            "interpolated": "Transfer 10 wSOL to toly.sol",
            "fallback": ["Transfer", "To: toly.sol", "Amount: 10 wSOL"],
            "fallbackWithoutMetadata": ["Transfer", "Token Mint: So111...11112", "To: 86xCn...o2MMY", "Amount: 10"],
            "advisories": []
        },
        "notes": [
            "The draft's expected text ends with a period (\"Transfer 10 wSOL to toly.sol.\") that its template (\"Transfer {amount} to {destination}\") does not have. This case expects the template as written, with no period added.",
            "{destination} names the token account, but the draft's expected text shows its owner (toly.sol); its Interpolation section writes the same intent as {destination.owner}. This case shows a token account as its owner when the owner is known, as the example does; the draft's last open question asks whether that should be the rule.",
            "The mint's label is \"Token Mint\" in the IDL, but the draft's output without metadata reads \"Token: So111...11112\". This case uses the label, as the accountDisplayNode definition says.",
            "skip \"withAdditionalMetadata\" is defined as \"SHOULD NOT display the account unless additional metadata is available\", yet the example shows the mint only when metadata is missing. This case follows the example: hidden when metadata is available.",
            "amountFormatterNode's \"decimals\" is defined as a number of decimal places, while the example passes the name of an argument (\"decimals\": \"decimals\") and \"token\" names an account. This case reads both as references, as the example does."
        ]
    })
}

fn system_transfer_case(ids: &Ids) -> Value {
    let alice = test_key("alice");
    let bob = test_key("bob");
    let message = compile_legacy(
        alice,
        &[system_transfer(ids, alice, bob, 1_500_000_000)],
        test_key("blockhash 2"),
    );
    json!({
        "id": "system-transfer",
        "title": "A plain SOL transfer to a named address",
        "class": "ordinary",
        "srfc39": SRFC,
        "idls": ["system.json"],
        "message": b64(&message),
        "context": { "names": { b58(&bob): "bob.sol" } },
        "expected": {
            "interpolated": "Transfer 1.5 SOL to bob.sol",
            "fallback": ["Transfer SOL", "To: bob.sol", "Amount: 1.5 SOL"],
            "fallbackWithoutMetadata": ["Transfer SOL", format!("To: {}", short(&bob)), "Amount: 1.5 SOL"],
            "advisories": []
        },
        "notes": ["isNative needs no metadata: the amount keeps its unit when nothing else is known."]
    })
}

fn durable_nonce_transfer(ids: &Ids) -> Value {
    let alice = test_key("alice");
    let eve = test_key("eve");
    let nonce = test_key("alice nonce account");
    let ixs = [
        advance_nonce(ids, nonce, alice),
        system_transfer(ids, alice, eve, 1_500_000_000),
    ];
    let message = compile_legacy(alice, &ixs, test_key("stored nonce value"));
    json!({
        "id": "durable-nonce-transfer",
        "title": "A SOL transfer signed with a durable nonce: the signature does not expire",
        "class": "adversarial",
        "srfc39": SRFC,
        "idls": ["system.json"],
        "message": b64(&message),
        "context": { "names": { b58(&alice): "alice.sol" } },
        "expected": {
            "interpolated": format!("Advance durable nonce {} and Transfer 1.5 SOL to {}", short(&nonce), short(&eve)),
            "fallback": [
                "Advance durable nonce", format!("Nonce account: {}", short(&nonce)), "Nonce authority: alice.sol",
                "Transfer SOL", format!("To: {}", short(&eve)), "Amount: 1.5 SOL"
            ],
            "fallbackWithoutMetadata": [
                "Advance durable nonce", format!("Nonce account: {}", short(&nonce)), format!("Nonce authority: {}", short(&alice)),
                "Transfer SOL", format!("To: {}", short(&eve)), "Amount: 1.5 SOL"
            ],
            "advisories": [advisory("durable-nonce", format!(
                "this signature stays valid until nonce account {} is advanced; a normal signature expires within minutes",
                short(&nonce)
            ))]
        },
        "notes": ["The rendering says nothing about how long the signature stays valid; only the advisory does. SIMD discussion #513 proposes an explicit warning."]
    })
}

fn unlimited_approve(ids: &Ids) -> Value {
    let alice = test_key("alice");
    let alice_usdc = test_key("alice USDC account");
    let drainer = test_key("unknown delegate");
    let mut data = vec![4];
    data.extend(u64::MAX.to_le_bytes());
    let ix = NewInstruction {
        program: ids.token,
        accounts: vec![
            account(alice_usdc, true),
            account(drainer, false),
            signer(alice, false),
        ],
        data,
    };
    let message = compile_legacy(alice, &[ix], test_key("blockhash 4"));
    json!({
        "id": "unlimited-approve",
        "title": "An SPL Token approval of u64::MAX to an unknown delegate",
        "class": "adversarial",
        "srfc39": SRFC,
        "idls": ["spl-token.json"],
        "message": b64(&message),
        "context": {
            "names": { b58(&alice): "alice.sol" },
            "tokens": { b58(&ids.usdc): { "symbol": "USDC", "decimals": 6 } },
            "tokenAccounts": { b58(&alice_usdc): { "owner": b58(&alice), "mint": b58(&ids.usdc) } }
        },
        "expected": {
            "interpolated": format!("Allow {} to spend 18446744073709.551615 USDC from alice.sol", short(&drainer)),
            "fallback": [
                "Approve a delegate", "From: alice.sol", format!("Delegate: {}", short(&drainer)),
                "Amount: 18446744073709.551615 USDC"
            ],
            "fallbackWithoutMetadata": [
                "Approve a delegate", format!("From: {}", short(&alice_usdc)), format!("Delegate: {}", short(&drainer)),
                "Amount: 18446744073709551615"
            ],
            "advisories": [advisory("unlimited-approval", format!(
                "{} may spend every token in alice.sol's account, now and later, until the approval is revoked",
                short(&drainer)
            ))]
        },
        "notes": [
            "The draft has no way to say that u64::MAX means unlimited, so the rendering shows the raw maximum; Ledger proposed maxThreshold and maxLabel for this.",
            "approve carries no mint: the formatter's \"token\" names the source token account, read through its mint when metadata is available."
        ]
    })
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
    for key in [vault, program_data, new_authority, ids.loader] {
        transaction_data.extend(key);
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
    let message = compile_legacy(
        member,
        &[advance_nonce(ids, nonce, member), approve],
        test_key("council stored nonce"),
    );
    let council = "Protocol council (2 of 5)";
    json!({
        "id": "squads-nested-approval",
        "title": "A durable-nonce approval of a Squads proposal that hands over a program's upgrade authority",
        "class": "adversarial",
        "srfc39": SRFC,
        "idls": ["system.json", "squads-v4.json", "bpf-loader-upgradeable.json"],
        "message": b64(&message),
        "context": {
            "names": {
                b58(&multisig): council,
                b58(&member): "council member 2",
                b58(&program_data): "Protocol program"
            },
            "accounts": {
                b58(&proposal): b64(&proposal_data),
                b58(&transaction): b64(&transaction_data)
            }
        },
        "expected": {
            "interpolated": format!("Advance durable nonce {} and Approve proposal {} of {council}", short(&nonce), short(&proposal)),
            "fallback": [
                "Advance durable nonce", format!("Nonce account: {}", short(&nonce)), "Nonce authority: council member 2",
                "Approve a multisig proposal", format!("Multisig: {council}"), format!("Proposal: {}", short(&proposal)), "Memo: none"
            ],
            "fallbackWithoutMetadata": [
                "Advance durable nonce", format!("Nonce account: {}", short(&nonce)), format!("Nonce authority: {}", short(&member)),
                "Approve a multisig proposal", format!("Multisig: {}", short(&multisig)), format!("Proposal: {}", short(&proposal)), "Memo: none"
            ],
            "advisories": [
                advisory("durable-nonce", format!(
                    "this signature stays valid until nonce account {} is advanced; a normal signature expires within minutes",
                    short(&nonce)
                )),
                advisory("nested-transaction", format!(
                    "approves transaction 7 of {council}, which will: Change the upgrade authority of Protocol program to {}",
                    short(&new_authority)
                )),
                advisory("authority-change", format!(
                    "inside transaction 7: the upgrade authority of Protocol program goes to {}",
                    short(&new_authority)
                ))
            ]
        },
        "notes": [
            "Modeled on the Drift exploit of April 1, 2026: an approval pre-signed with a durable nonce. Here the proposal's transaction changes a program's upgrade authority.",
            "The rendering cannot say what the proposal executes: it is stored in the VaultTransaction account, which this instruction does not even reference. Ledger asked for account-data formatters for Squads; until the draft has them, only the advisories can show it. They decode it from the account data in the context, after checking the proposal and transaction addresses against their seeds.",
            "The draft does not say how to show an absent optional value (the memo); \"none\" here."
        ]
    })
}

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let ids = ids();
    for case in [
        spec_transfer_checked(&ids),
        system_transfer_case(&ids),
        durable_nonce_transfer(&ids),
        unlimited_approve(&ids),
        squads_nested_approval(&ids),
    ] {
        write(&root, case);
    }
}
