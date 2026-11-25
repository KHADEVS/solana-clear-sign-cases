//! Advisories: what a careful wallet flags on top of the display. Ours, not
//! part of any spec; kept apart so the expected displays stay Codama's own.

use crate::decode::{decode, Context, Programs};
use crate::message::{Config, Instruction};
use crate::util::{anchor_discriminator, find_pda, format_amount, from_b58, short, Key};

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
    compute_budget: Key,
    stake: Key,
    clock: Key,
}

fn ids() -> Ids {
    let key = |s: &str| from_b58(s).expect("program id");
    Ids {
        system: key("11111111111111111111111111111111"),
        token: key("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"),
        token_2022: key("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb"),
        loader: key("BPFLoaderUpgradeab1e11111111111111111111111"),
        squads: key("SQDS4ep65T869zMMBKyuUq6aD6EgTu8psMjkvj52pCf"),
        compute_budget: key("ComputeBudget111111111111111111111111111111"),
        stake: key("Stake11111111111111111111111111111111111111"),
        clock: key("SysvarC1ock11111111111111111111111111111111"),
    }
}

fn sol(lamports: u64) -> String {
    format!("{} SOL", format_amount(lamports as u128, 9))
}

fn u32_le(data: &[u8]) -> Option<u32> {
    Some(u32::from_le_bytes(data.get(..4)?.try_into().ok()?))
}

fn u64_at(data: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_le_bytes(data.get(at..at + 8)?.try_into().ok()?))
}

/// `signers`: the message's required signers, fee payer first; `config`: the
/// header config of a v1 message, `None` for legacy and v0.
pub fn advise(
    programs: &Programs,
    signers: &[Key],
    instructions: &[Instruction],
    ctx: &Context,
    config: Option<&Config>,
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
    // v1 (SIMD-0385) sets the fee and the limits in the message header: no
    // instruction display shows them, and compute budget instructions, if
    // any, are ignored for them.
    if let Some(config) = config {
        let limit = config
            .compute_unit_limit
            .map(|cu| format!(", compute unit limit {cu}"))
            .unwrap_or_default();
        out.push(advisory(
            "header-fee",
            format!(
                "this v1 transaction sets its fee in the message header, which no instruction shows: priority fee {}{limit}",
                sol(config.priority_fee)
            ),
        ));
        let ignored = instructions
            .iter()
            .filter(|ix| ix.program == ids.compute_budget)
            .count();
        if ignored > 0 {
            out.push(advisory(
                "ignored-compute-budget",
                format!(
                    "{} compute budget instruction{} in this v1 transaction {} ignored for the fee and the limits: a fee worked out from {} is not the fee paid",
                    ignored,
                    if ignored == 1 { "" } else { "s" },
                    if ignored == 1 { "is" } else { "are" },
                    if ignored == 1 { "it" } else { "them" }
                ),
            ));
        }
    }
    for ix in instructions {
        out.extend(advise_one(programs, ix, ctx, &ids, signers));
    }
    out
}

fn advise_one(
    programs: &Programs,
    ix: &Instruction,
    ctx: &Context,
    ids: &Ids,
    signers: &[Key],
) -> Vec<Advisory> {
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
                        intent(decoded.node),
                        ctx.name_or_short(&ix.program)
                    ),
                ));
            }
        }
        // Signers the instruction's description does not list, other than
        // the fee payer: harmless to the program, but they change who must
        // sign (trezor-firmware#6843). Unlisted accounts that do not sign are
        // common (a router passes its pools that way, and the wallet's own
        // key among them) and are left to the display.
        let declared = decoded.node["accounts"].as_array().map_or(0, Vec::len);
        let has_remaining = decoded.node["remainingAccounts"]
            .as_array()
            .is_some_and(|r| !r.is_empty());
        if ix.accounts.len() > declared && !has_remaining {
            let mut seen = Vec::new();
            let extra: Vec<String> = ix.accounts[declared..]
                .iter()
                .filter(|k| signers.get(1..).is_some_and(|others| others.contains(k)))
                .filter(|k| {
                    let new = !seen.contains(*k);
                    seen.push(**k);
                    new
                })
                .map(|k| ctx.name_or_short(k))
                .collect();
            if !extra.is_empty() {
                out.push(advisory(
                    "extra-signers",
                    format!(
                        "\"{}\" also passes {} its description does not list: {}",
                        intent(decoded.node),
                        if extra.len() == 1 {
                            "a signer"
                        } else {
                            "signers"
                        },
                        extra.join(", ")
                    ),
                ));
            }
        }
        // Optional accounts that are left out when absent, with a required
        // account after them: a shorter list does not tell which ones are
        // missing, and names given by position move onto other accounts.
        // (A list made longer again by accounts after the named ones is not
        // caught by its length.)
        let named = decoded.node["accounts"]
            .as_array()
            .map_or(&[][..], Vec::as_slice);
        let optional = |a: &serde_json::Value| a["isOptional"] == true;
        if let Some(first) = named.iter().position(optional) {
            if decoded.node["optionalAccountStrategy"] == "omitted"
                && ix.accounts.len() < named.len()
                && named[first..].iter().any(|a| !optional(a))
            {
                out.push(advisory(
                    "omitted-accounts",
                    format!(
                        "\"{}\" leaves out {} of the {} accounts its description lists, and the IDL does not say which: from account {} on, a label may sit on another account",
                        intent(decoded.node),
                        named.len() - ix.accounts.len(),
                        named.len(),
                        first + 1
                    ),
                ));
            }
        }
        // Stake instructions keep an older account list, told apart by the
        // Clock sysvar in one position (stake program, processor.rs); the IDL
        // describes only the newer, shorter list.
        if ix.program == ids.stake {
            let branch = match u32_le(&ix.data) {
                Some(1 | 5) => Some(1),     // authorize, deactivate
                Some(2 | 4 | 7) => Some(2), // delegate, withdraw, merge
                _ => None,
            };
            if let Some(n) = branch.filter(|n| ix.accounts.get(*n) == Some(&ids.clock)) {
                out.push(advisory(
                    "older-account-list",
                    format!(
                        "\"{}\" uses the older account list, with the Clock sysvar as account {}, which the IDL does not describe: its account labels from account {} on do not match these accounts",
                        intent(decoded.node),
                        n + 1,
                        n + 1
                    ),
                ));
            }
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
    // SPL Token SetAuthority: [6, authority type, option tag, new authority].
    if is_token && ix.data.first() == Some(&6) {
        if let ([target, ..], Some(kind)) = (&ix.accounts[..], ix.data.get(1)) {
            let new = match (ix.data.get(2), ix.data.get(3..35)) {
                (Some(1), Some(key)) => ctx.name_or_short(&<Key>::try_from(key).expect("32 bytes")),
                _ => "nobody".to_string(),
            };
            let what = match kind {
                0 => format!("the mint authority of mint {}", ctx.name_or_short(target)),
                1 => format!("the freeze authority of mint {}", ctx.name_or_short(target)),
                2 => format!("the owner of {}'s token account", ctx.holder(target)),
                3 => format!(
                    "the close authority of {}'s token account",
                    ctx.holder(target)
                ),
                other => format!("authority {other} of {}", ctx.name_or_short(target)),
            };
            out.push(advisory(
                "authority-change",
                format!("{what} goes to {new}"),
            ));
        }
    }
    // A Token Batch (tag 255) runs the instructions packed in its data, each
    // with the next accounts of its list. The IDL gives them as bytes, so a
    // display shows neither what they do nor which accounts they touch.
    if is_token && ix.data.first() == Some(&255) {
        match batched(ix) {
            Ok(inner) => {
                let what = inner
                    .iter()
                    .map(|inner_ix| describe(programs, inner_ix))
                    .collect::<Vec<_>>()
                    .join("; ");
                out.push(advisory(
                    "batched-instructions",
                    format!(
                        "{} instruction{} packed in one Batch instruction, which the display shows as bytes: {what}",
                        inner.len(),
                        if inner.len() == 1 { " is" } else { "s are" }
                    ),
                ));
                for inner_ix in &inner {
                    // The program refuses a batch inside a batch: nothing to read in it.
                    if inner_ix.data.first() == Some(&255) {
                        continue;
                    }
                    for mut found in advise_one(programs, inner_ix, ctx, ids, signers) {
                        found.detail = format!("inside the batch: {}", found.detail);
                        out.push(found);
                    }
                }
            }
            Err(e) => out.push(advisory(
                "batched-instructions",
                format!("a Batch instruction that does not read as one: {e}"),
            )),
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
    // The inner transaction is signed by the vault, not by this message's
    // signers: no inner account is one of them.
    for inner_ix in &inner {
        for mut found in advise_one(programs, inner_ix, ctx, ids, &[]) {
            found.detail = format!("inside transaction {index}: {}", found.detail);
            out.push(found);
        }
    }
    out
}

/// The instructions inside a Token Batch: for each one, a count of accounts,
/// the length of its data, then the data (p-token, `process_batch`).
fn batched(ix: &Instruction) -> Result<Vec<Instruction>, String> {
    let mut out = Vec::new();
    let (mut pos, mut next_account) = (1usize, 0usize);
    while pos < ix.data.len() {
        let (Some(&count), Some(&len)) = (ix.data.get(pos), ix.data.get(pos + 1)) else {
            return Err("an instruction header is cut short".into());
        };
        let (count, len) = (count as usize, len as usize);
        if len == 0 {
            return Err("an instruction without data".into());
        }
        let data = ix
            .data
            .get(pos + 2..pos + 2 + len)
            .ok_or("an instruction's data is cut short")?;
        let accounts = ix
            .accounts
            .get(next_account..next_account + count)
            .ok_or("fewer accounts than the instructions ask for")?;
        out.push(Instruction {
            program: ix.program,
            accounts: accounts.to_vec(),
            data: data.to_vec(),
        });
        pos += 2 + len;
        next_account += count;
    }
    if out.is_empty() {
        return Err("no instruction inside".into());
    }
    Ok(out)
}

/// An instruction node's display intent, or its name when it has no display
/// metadata (IDLs converted from Anchor).
fn intent(node: &serde_json::Value) -> &str {
    node["display"]["intent"]
        .as_str()
        .or(node["name"].as_str())
        .unwrap_or("?")
}

/// An instruction by its IDL intent, or by its program when no IDL knows it.
fn describe(programs: &Programs, ix: &Instruction) -> String {
    match decode(programs, ix) {
        Ok(Some(d)) => intent(d.node).to_string(),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn batch(data: &[u8], accounts: &[u8]) -> Instruction {
        Instruction {
            program: [9; 32],
            accounts: accounts.iter().map(|n| [*n; 32]).collect(),
            data: data.to_vec(),
        }
    }

    #[test]
    fn a_batch_gives_each_instruction_its_own_accounts() {
        let inner = batched(&batch(&[255, 2, 1, 9, 1, 3, 7, 7, 7], &[1, 2, 3])).expect("a batch");
        assert_eq!(inner.len(), 2);
        assert_eq!(inner[0].accounts, vec![[1; 32], [2; 32]]);
        assert_eq!(inner[0].data, vec![9]);
        assert_eq!(inner[1].accounts, vec![[3; 32]]);
        assert_eq!(inner[1].data, vec![7, 7, 7]);
        assert_eq!(inner[1].program, [9; 32]);
    }

    #[test]
    fn a_batch_the_program_refuses_is_not_read() {
        let refused: [(&[u8], &[u8]); 6] = [
            (&[255], &[]),                 // nothing inside
            (&[255, 1], &[1]),             // a header cut short
            (&[255, 1, 0], &[1]),          // an instruction without data
            (&[255, 1, 2, 9], &[1]),       // data cut short
            (&[255, 2, 1, 9], &[1]),       // fewer accounts than asked for
            (&[255, 1, 1, 9, 1], &[1, 2]), // a second header cut short
        ];
        for (data, accounts) in refused {
            assert!(batched(&batch(data, accounts)).is_err(), "{data:?}");
        }
    }
}
