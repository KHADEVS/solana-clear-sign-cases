//! Advisories: what a careful wallet flags on top of the sRFC 39 rendering.
//! Not part of the draft; kept apart so the rendering cases stay normative.

use crate::decode::{decode, Context, Programs};
use crate::message::Instruction;
use crate::util::{anchor_discriminator, find_pda, from_b58, short, Key};

#[derive(Clone, Debug, PartialEq)]
pub struct Advisory {
    pub kind: String,
    pub detail: String,
}

fn advisory(kind: &str, detail: String) -> Advisory {
    Advisory {
        kind: kind.into(),
        detail,
    }
}

struct Ids {
    system: Key,
    token: Key,
    token_2022: Key,
    loader: Key,
    squads: Key,
}

fn ids() -> Ids {
    let key = |s: &str| from_b58(s).expect("program id");
    Ids {
        system: key("11111111111111111111111111111111"),
        token: key("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"),
        token_2022: key("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb"),
        loader: key("BPFLoaderUpgradeab1e11111111111111111111111"),
        squads: key("SQDS4ep65T869zMMBKyuUq6aD6EgTu8psMjkvj52pCf"),
    }
}

fn u32_le(data: &[u8]) -> Option<u32> {
    Some(u32::from_le_bytes(data.get(..4)?.try_into().ok()?))
}

fn u64_at(data: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_le_bytes(data.get(at..at + 8)?.try_into().ok()?))
}

/// `signers`: the message's required signers, fee payer first.
pub fn advise(
    programs: &Programs,
    signers: &[Key],
    instructions: &[Instruction],
    ctx: &Context,
) -> Vec<Advisory> {
    let ids = ids();
    let mut out = Vec::new();
    if signers.len() > 1 {
        let others: Vec<String> = signers[1..].iter().map(|k| ctx.name_or_short(k)).collect();
        out.push(advisory(
            "co-signed",
            format!(
                "valid only once {} also sign{}; whoever holds {} decides when it is sent",
                others.join(", "),
                if others.len() == 1 { "s" } else { "" },
                if others.len() == 1 {
                    "that signature"
                } else {
                    "those signatures"
                }
            ),
        ));
    }
    if let Some(first) = instructions.first() {
        if first.program == ids.system && u32_le(&first.data) == Some(4) {
            if let Some(nonce) = first.accounts.first() {
                out.push(advisory(
                    "durable-nonce",
                    format!(
                        "this signature stays valid until nonce account {} is advanced; a normal signature expires within minutes",
                        ctx.name_or_short(nonce)
                    ),
                ));
            }
        }
    }
    for ix in instructions {
        out.extend(advise_one(programs, ix, ctx, &ids));
    }
    out
}

fn advise_one(programs: &Programs, ix: &Instruction, ctx: &Context, ids: &Ids) -> Vec<Advisory> {
    let mut out = Vec::new();
    if let Ok(Some(decoded)) = decode(programs, ix) {
        // Metadata a wallet has not vetted can say anything about what an
        // instruction does: the rendering shows it, the advisory says so.
        if let Some(trusted) = &ctx.trusted_programs {
            if !trusted.contains(&ix.program) {
                out.push(advisory(
                    "untrusted-metadata",
                    format!(
                        "\"{}\" is how program {} describes itself; it is not on this wallet's trusted list, so the text may not match what the instruction does",
                        decoded.node["display"]["intent"].as_str().unwrap_or("?"),
                        ctx.name_or_short(&ix.program)
                    ),
                ));
            }
        }
        // Accounts the instruction's description does not list: harmless to
        // the program, but they can change who must sign (trezor-firmware#6843).
        let declared = decoded.node["accounts"].as_array().map_or(0, Vec::len);
        let has_remaining = decoded.node["remainingAccounts"]
            .as_array()
            .is_some_and(|r| !r.is_empty());
        if ix.accounts.len() > declared && !has_remaining {
            let extra: Vec<String> = ix.accounts[declared..]
                .iter()
                .map(|k| ctx.name_or_short(k))
                .collect();
            out.push(advisory(
                "extra-accounts",
                format!(
                    "\"{}\" also passes {} its description does not list: {}",
                    decoded.node["display"]["intent"].as_str().unwrap_or("?"),
                    if extra.len() == 1 {
                        "an account"
                    } else {
                        "accounts"
                    },
                    extra.join(", ")
                ),
            ));
        }
    }
    // A program the wallet has an IDL for, but no instruction of it matches:
    // a display shows it as unknown (trezor-firmware#4387, the reverse case).
    if let (Ok(None), Some(name)) = (decode(programs, ix), programs.name(&ix.program)) {
        out.push(advisory(
            "unidentified-instruction",
            format!(
                "no instruction in the IDL of {name} matches this data ({}), so a display can only show it as unknown",
                if ix.data.is_empty() {
                    "empty".to_string()
                } else {
                    ix.data.iter().map(|b| format!("{b:02x}")).collect::<String>()
                }
            ),
        ));
    }
    let is_token = ix.program == ids.token || ix.program == ids.token_2022;
    // u64::MAX, and anything from half of it up: no real balance comes close.
    let huge = |amount: Option<u64>| amount.is_some_and(|a| a >= 1u64 << 63);
    if is_token && ix.data.first() == Some(&4) && huge(u64_at(&ix.data, 1)) {
        if let [source, delegate, ..] = ix.accounts[..] {
            out.push(advisory(
                "unlimited-approval",
                format!(
                    "{} may spend practically every token in {}'s account, now and later, until the approval is revoked",
                    ctx.name_or_short(&delegate),
                    ctx.holder(&source)
                ),
            ));
        }
    }
    if ix.program == ids.loader && u32_le(&ix.data) == Some(4) {
        if let [program_data, _, ..] = ix.accounts[..] {
            let to = match ix.accounts.get(2) {
                Some(key) => ctx.holder(key),
                None => "nobody: the program becomes immutable".into(),
            };
            out.push(advisory(
                "authority-change",
                format!(
                    "the upgrade authority of {} goes to {to}",
                    ctx.holder(&program_data)
                ),
            ));
        }
    }
    if ix.program == ids.squads
        && ix.data.get(..8) == Some(&anchor_discriminator("global", "proposal_approve"))
    {
        out.extend(squads_approval(programs, ix, ctx, ids));
    }
    out
}

/// A Squads v4 approval shows only the proposal's address; what it lets
/// execute is a transaction stored in another account. With that account's
/// data in the context, decode it and say what it does.
fn squads_approval(
    programs: &Programs,
    ix: &Instruction,
    ctx: &Context,
    ids: &Ids,
) -> Vec<Advisory> {
    let [multisig, _, proposal, ..] = ix.accounts[..] else {
        return vec![];
    };
    let unknown = || {
        vec![advisory(
            "nested-transaction",
            format!(
                "approves a proposal of {}; what it executes is stored on chain, not in this transaction",
                ctx.holder(&multisig)
            ),
        )]
    };
    let Some(data) = ctx.accounts.get(&proposal) else {
        return unknown();
    };
    if data.get(..8) != Some(&anchor_discriminator("account", "Proposal"))
        || data.get(8..40) != Some(&multisig)
    {
        return vec![advisory(
            "context-mismatch",
            "the proposal account does not belong to this multisig".into(),
        )];
    }
    let Some(index) = u64_at(data, 40) else {
        return unknown();
    };
    let index_bytes = index.to_le_bytes();
    let (expected_proposal, _) = find_pda(
        &[
            b"multisig",
            &multisig,
            b"transaction",
            &index_bytes,
            b"proposal",
        ],
        &ids.squads,
    );
    if expected_proposal != proposal {
        return vec![advisory(
            "context-mismatch",
            "the proposal address does not match its index".into(),
        )];
    }
    let (transaction, _) = find_pda(
        &[b"multisig", &multisig, b"transaction", &index_bytes],
        &ids.squads,
    );
    let Some(tx_data) = ctx.accounts.get(&transaction) else {
        return unknown();
    };
    let inner = match vault_instructions(tx_data, &multisig) {
        Ok(inner) => inner,
        Err(e) => {
            return vec![advisory(
                "context-mismatch",
                format!("the vault transaction does not decode: {e}"),
            )]
        }
    };
    let what = inner
        .iter()
        .map(|ix| describe(programs, ix))
        .collect::<Vec<_>>()
        .join("; ");
    let mut out = vec![advisory(
        "nested-transaction",
        format!(
            "approves transaction {index} of {}, which runs: {what}",
            ctx.holder(&multisig)
        ),
    )];
    for inner_ix in &inner {
        for mut found in advise_one(programs, inner_ix, ctx, ids) {
            found.detail = format!("inside transaction {index}: {}", found.detail);
            out.push(found);
        }
    }
    out
}

/// An instruction by its IDL intent, or by its program when no IDL knows it.
fn describe(programs: &Programs, ix: &Instruction) -> String {
    match decode(programs, ix) {
        Ok(Some(d)) => d.node["display"]["intent"]
            .as_str()
            .or(d.node["name"].as_str())
            .unwrap_or("?")
            .to_string(),
        _ => format!(
            "an instruction no IDL here describes, for program {}",
            programs
                .name(&ix.program)
                .map(str::to_string)
                .unwrap_or_else(|| short(&ix.program))
        ),
    }
}

/// Squads v4 `VaultTransaction` (state/vault_transaction.rs), Borsh-encoded.
fn vault_instructions(data: &[u8], multisig: &Key) -> Result<Vec<Instruction>, String> {
    let mut pos = 0usize;
    let mut take = |n: usize| -> Result<&[u8], String> {
        let end = pos
            .checked_add(n)
            .filter(|end| *end <= data.len())
            .ok_or("account data too short")?;
        let out = &data[pos..end];
        pos = end;
        Ok(out)
    };
    if take(8)? != anchor_discriminator("account", "VaultTransaction") {
        return Err("not a VaultTransaction".into());
    }
    if take(32)? != multisig {
        return Err("it belongs to another multisig".into());
    }
    take(32 + 8 + 1 + 1 + 1)?; // creator, index, bump, vault_index, vault_bump
    let len = |b: &[u8]| u32::from_le_bytes(b.try_into().expect("4")) as usize;
    let bumps = len(take(4)?);
    take(bumps)?;
    take(3)?; // num_signers, num_writable_signers, num_writable_non_signers
    let key_count = len(take(4)?);
    let mut keys = Vec::new();
    for _ in 0..key_count {
        keys.push(<Key>::try_from(take(32)?).expect("32"));
    }
    let mut instructions = Vec::new();
    for _ in 0..len(take(4)?) {
        let program = take(1)?[0];
        let account_count = len(take(4)?);
        let accounts = take(account_count)?.to_vec();
        let data_len = len(take(4)?);
        let ix_data = take(data_len)?.to_vec();
        let get = |i: u8| {
            keys.get(i as usize)
                .copied()
                .ok_or("account index out of range")
        };
        instructions.push(Instruction {
            program: get(program)?,
            accounts: accounts.iter().map(|i| get(*i)).collect::<Result<_, _>>()?,
            data: ix_data,
        });
    }
    if len(take(4)?) != 0 {
        return Err("address lookup tables in a vault transaction are not supported yet".into());
    }
    Ok(instructions)
}
