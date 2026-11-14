//! Solana transaction messages, legacy and v0: what a signer signs.

use crate::util::Key;
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct CompiledInstruction {
    pub program: u8,
    pub accounts: Vec<u8>,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct Lookup {
    pub table: Key,
    pub writable: Vec<u8>,
    pub readonly: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct Message {
    /// `None` for a legacy message, `Some(0)` for v0.
    pub version: Option<u8>,
    pub required_signatures: u8,
    pub readonly_signed: u8,
    pub readonly_unsigned: u8,
    pub keys: Vec<Key>,
    pub blockhash: Key,
    pub instructions: Vec<CompiledInstruction>,
    pub lookups: Vec<Lookup>,
}

/// An instruction with its accounts resolved to addresses.
#[derive(Clone, Debug)]
pub struct Instruction {
    pub program: Key,
    pub accounts: Vec<Key>,
    pub data: Vec<u8>,
}

struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8], String> {
        let end = self
            .pos
            .checked_add(n)
            .filter(|end| *end <= self.bytes.len());
        let end = end.ok_or("message ends too early")?;
        let out = &self.bytes[self.pos..end];
        self.pos = end;
        Ok(out)
    }

    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }

    fn key(&mut self) -> Result<Key, String> {
        Ok(self.take(32)?.try_into().expect("32 bytes"))
    }

    /// Solana's compact-u16 ("shortvec") length.
    fn compact(&mut self) -> Result<usize, String> {
        let mut value = 0usize;
        for shift in [0, 7, 14] {
            let byte = self.u8()?;
            value |= ((byte & 0x7f) as usize) << shift;
            if byte & 0x80 == 0 {
                return Ok(value);
            }
        }
        Err("compact-u16 longer than 3 bytes".into())
    }

    fn bytes(&mut self) -> Result<Vec<u8>, String> {
        let len = self.compact()?;
        Ok(self.take(len)?.to_vec())
    }
}

pub fn decode(bytes: &[u8]) -> Result<Message, String> {
    let mut r = Reader { bytes, pos: 0 };
    let first = r.u8()?;
    let (version, required_signatures) = if first & 0x80 != 0 {
        let version = first & 0x7f;
        if version != 0 {
            return Err(format!("unsupported message version {version}"));
        }
        (Some(0), r.u8()?)
    } else {
        (None, first)
    };
    let readonly_signed = r.u8()?;
    let readonly_unsigned = r.u8()?;
    let mut keys = Vec::new();
    for _ in 0..r.compact()? {
        keys.push(r.key()?);
    }
    let blockhash = r.key()?;
    let mut instructions = Vec::new();
    for _ in 0..r.compact()? {
        let program = r.u8()?;
        let accounts = r.bytes()?;
        let data = r.bytes()?;
        instructions.push(CompiledInstruction {
            program,
            accounts,
            data,
        });
    }
    let mut lookups = Vec::new();
    if version.is_some() {
        for _ in 0..r.compact()? {
            let table = r.key()?;
            let writable = r.bytes()?;
            let readonly = r.bytes()?;
            lookups.push(Lookup {
                table,
                writable,
                readonly,
            });
        }
    }
    if r.pos != bytes.len() {
        return Err("trailing bytes after the message".into());
    }
    Ok(Message {
        version,
        required_signatures,
        readonly_signed,
        readonly_unsigned,
        keys,
        blockhash,
        instructions,
        lookups,
    })
}

impl Message {
    /// Every account address in index order: the static keys, then the
    /// lookups' writable addresses, then their read-only ones.
    pub fn all_keys(&self, tables: &HashMap<Key, Vec<Key>>) -> Result<Vec<Key>, String> {
        let mut keys = self.keys.clone();
        let mut readonly = Vec::new();
        for lookup in &self.lookups {
            let table = tables.get(&lookup.table).ok_or_else(|| {
                format!(
                    "lookup table {} not provided",
                    crate::util::b58(&lookup.table)
                )
            })?;
            let pick = |i: &u8| {
                table
                    .get(*i as usize)
                    .copied()
                    .ok_or_else(|| format!("index {i} past the end of a lookup table"))
            };
            for i in &lookup.writable {
                keys.push(pick(i)?);
            }
            for i in &lookup.readonly {
                readonly.push(pick(i)?);
            }
        }
        keys.extend(readonly);
        Ok(keys)
    }

    pub fn resolve(&self, tables: &HashMap<Key, Vec<Key>>) -> Result<Vec<Instruction>, String> {
        let keys = self.all_keys(tables)?;
        let get = |i: u8| {
            keys.get(i as usize)
                .copied()
                .ok_or_else(|| format!("account index {i} out of range"))
        };
        self.instructions
            .iter()
            .map(|ix| {
                Ok(Instruction {
                    program: get(ix.program)?,
                    accounts: ix
                        .accounts
                        .iter()
                        .map(|i| get(*i))
                        .collect::<Result<_, String>>()?,
                    data: ix.data.clone(),
                })
            })
            .collect()
    }
}

// --- Building messages, for the case generator -------------------------------

#[derive(Clone, Debug)]
pub struct AccountMeta {
    pub key: Key,
    pub signer: bool,
    pub writable: bool,
}

#[derive(Clone, Debug)]
pub struct NewInstruction {
    pub program: Key,
    pub accounts: Vec<AccountMeta>,
    pub data: Vec<u8>,
}

fn push_compact(out: &mut Vec<u8>, mut value: usize) {
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if value == 0 {
            break;
        }
    }
}

/// Every account of the instructions, merged, ordered as Solana requires:
/// the fee payer, the writable signers, the read-only signers, the writable
/// non-signers, then the read-only non-signers (program ids among them).
fn ordered_metas(payer: Key, instructions: &[NewInstruction]) -> Vec<AccountMeta> {
    let mut metas: Vec<AccountMeta> = vec![AccountMeta {
        key: payer,
        signer: true,
        writable: true,
    }];
    let mut add = |meta: AccountMeta| match metas.iter_mut().find(|m| m.key == meta.key) {
        Some(existing) => {
            existing.signer |= meta.signer;
            existing.writable |= meta.writable;
        }
        None => metas.push(meta),
    };
    for ix in instructions {
        for meta in &ix.accounts {
            add(meta.clone());
        }
        add(AccountMeta {
            key: ix.program,
            signer: false,
            writable: false,
        });
    }
    let rank = |m: &AccountMeta| match (m.signer, m.writable) {
        (true, true) => 0,
        (true, false) => 1,
        (false, true) => 2,
        (false, false) => 3,
    };
    let payer_meta = metas.remove(0);
    metas.sort_by_key(rank); // stable: first-seen order within each group
    metas.insert(0, payer_meta);
    metas
}

/// Header, static keys, blockhash and instructions; `all_keys` are the
/// static keys followed by those loaded through lookup tables.
fn write_body(
    out: &mut Vec<u8>,
    statics: &[AccountMeta],
    all_keys: &[Key],
    instructions: &[NewInstruction],
    blockhash: Key,
) {
    let index = |key: &Key| {
        all_keys
            .iter()
            .position(|k| k == key)
            .expect("key compiled") as u8
    };
    out.extend([
        statics.iter().filter(|m| m.signer).count() as u8,
        statics.iter().filter(|m| m.signer && !m.writable).count() as u8,
        statics.iter().filter(|m| !m.signer && !m.writable).count() as u8,
    ]);
    push_compact(out, statics.len());
    for meta in statics {
        out.extend_from_slice(&meta.key);
    }
    out.extend_from_slice(&blockhash);
    push_compact(out, instructions.len());
    for ix in instructions {
        out.push(index(&ix.program));
        push_compact(out, ix.accounts.len());
        for meta in &ix.accounts {
            out.push(index(&meta.key));
        }
        push_compact(out, ix.data.len());
        out.extend_from_slice(&ix.data);
    }
}

/// A legacy message.
pub fn compile_legacy(payer: Key, instructions: &[NewInstruction], blockhash: Key) -> Vec<u8> {
    let metas = ordered_metas(payer, instructions);
    let keys: Vec<Key> = metas.iter().map(|m| m.key).collect();
    let mut out = Vec::new();
    write_body(&mut out, &metas, &keys, instructions, blockhash);
    out
}

/// A v0 message in which the accounts found in `table` (neither signers nor
/// program ids, which must stay in the message) are loaded through it.
pub fn compile_v0(
    payer: Key,
    instructions: &[NewInstruction],
    blockhash: Key,
    table: (Key, &[Key]),
) -> Vec<u8> {
    let (table_key, table_keys) = table;
    let programs: Vec<Key> = instructions.iter().map(|ix| ix.program).collect();
    let loadable =
        |m: &AccountMeta| !m.signer && !programs.contains(&m.key) && table_keys.contains(&m.key);
    let metas = ordered_metas(payer, instructions);
    let statics: Vec<AccountMeta> = metas.iter().filter(|m| !loadable(m)).cloned().collect();
    let writable: Vec<Key> = metas
        .iter()
        .filter(|m| loadable(m) && m.writable)
        .map(|m| m.key)
        .collect();
    let readonly: Vec<Key> = metas
        .iter()
        .filter(|m| loadable(m) && !m.writable)
        .map(|m| m.key)
        .collect();
    let mut all_keys: Vec<Key> = statics.iter().map(|m| m.key).collect();
    all_keys.extend(&writable);
    all_keys.extend(&readonly);
    let position = |key: &Key| table_keys.iter().position(|k| k == key).expect("in table") as u8;
    let mut out = vec![0x80];
    write_body(&mut out, &statics, &all_keys, instructions, blockhash);
    push_compact(&mut out, 1);
    out.extend_from_slice(&table_key);
    push_compact(&mut out, writable.len());
    out.extend(writable.iter().map(position));
    push_compact(&mut out, readonly.len());
    out.extend(readonly.iter().map(position));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::test_key;

    #[test]
    fn a_compiled_message_decodes_back() {
        let payer = test_key("payer");
        let to = test_key("to");
        let program = test_key("program");
        let ix = NewInstruction {
            program,
            accounts: vec![
                AccountMeta {
                    key: payer,
                    signer: true,
                    writable: true,
                },
                AccountMeta {
                    key: to,
                    signer: false,
                    writable: true,
                },
            ],
            data: vec![2, 0, 0, 0, 1, 2, 3],
        };
        let bytes = compile_legacy(payer, &[ix], test_key("blockhash"));
        let message = decode(&bytes).unwrap();
        assert_eq!(message.version, None);
        assert_eq!(message.required_signatures, 1);
        assert_eq!(message.readonly_unsigned, 1);
        let resolved = message.resolve(&HashMap::new()).unwrap();
        assert_eq!(resolved[0].program, program);
        assert_eq!(resolved[0].accounts, vec![payer, to]);
        assert_eq!(resolved[0].data, vec![2, 0, 0, 0, 1, 2, 3]);
    }
}
