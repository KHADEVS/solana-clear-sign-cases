//! Decoding instructions against Codama IDLs (official solana-program IDLs or
//! hand-written ones in the same format), for the advisories. The expected
//! displays come from Codama itself (tools/codama), not from this crate.

use crate::message::Instruction;
use crate::util::{from_b58, short, Key};
use serde_json::Value;
use std::collections::HashMap;

pub struct TokenInfo {
    pub symbol: String,
    pub decimals: u32,
}

pub struct TokenAccount {
    pub owner: Key,
    pub mint: Key,
}

/// The "additional metadata" of sRFC 39: what a wallet knows besides the
/// transaction and the IDLs (names, token details, account state).
#[derive(Default)]
pub struct Context {
    pub names: HashMap<Key, String>,
    pub tokens: HashMap<Key, TokenInfo>,
    pub token_accounts: HashMap<Key, TokenAccount>,
    pub accounts: HashMap<Key, Vec<u8>>,
    /// Programs whose display metadata this wallet has vetted (the curated
    /// list the sRFC 39 discussion asks for); `None` when the case has none.
    pub trusted_programs: Option<Vec<Key>>,
}

impl Context {
    pub fn from_json(value: &Value) -> Result<Self, String> {
        use base64::Engine;
        let mut ctx = Context::default();
        let map = |field: &str| {
            value
                .get(field)
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default()
        };
        for (key, name) in map("names") {
            ctx.names.insert(
                from_b58(&key)?,
                name.as_str().ok_or("a name is not text")?.to_string(),
            );
        }
        for (key, info) in map("tokens") {
            let symbol = info["symbol"]
                .as_str()
                .ok_or("token without symbol")?
                .to_string();
            let decimals = info["decimals"].as_u64().ok_or("token without decimals")? as u32;
            ctx.tokens
                .insert(from_b58(&key)?, TokenInfo { symbol, decimals });
        }
        for (key, info) in map("tokenAccounts") {
            let owner = from_b58(
                info["owner"]
                    .as_str()
                    .ok_or("token account without owner")?,
            )?;
            let mint = from_b58(info["mint"].as_str().ok_or("token account without mint")?)?;
            ctx.token_accounts
                .insert(from_b58(&key)?, TokenAccount { owner, mint });
        }
        for (key, entry) in map("accounts") {
            // Either the data alone, or { data, owner } as the Codama display needs.
            let text = entry.as_str().or(entry["data"].as_str());
            let data = base64::engine::general_purpose::STANDARD
                .decode(text.ok_or("account data is not text")?)
                .map_err(|e| format!("account data: {e}"))?;
            ctx.accounts.insert(from_b58(&key)?, data);
        }
        if let Some(list) = value.get("trustedPrograms").and_then(Value::as_array) {
            ctx.trusted_programs = Some(
                list.iter()
                    .map(|k| from_b58(k.as_str().unwrap_or("")))
                    .collect::<Result<_, _>>()?,
            );
        }
        Ok(ctx)
    }

    pub fn name_or_short(&self, key: &Key) -> String {
        self.names.get(key).cloned().unwrap_or_else(|| short(key))
    }

    /// A token account by its owner, as wallets show it; anything else by name.
    pub fn holder(&self, key: &Key) -> String {
        match self.token_accounts.get(key) {
            Some(account) => self.name_or_short(&account.owner),
            None => self.name_or_short(key),
        }
    }
}

/// Program IDLs by program id.
pub struct Programs(HashMap<Key, Value>);

impl Programs {
    /// From Codama roots (or bare program nodes), by program id.
    pub fn new(idls: Vec<Value>) -> Result<Self, String> {
        let mut map = HashMap::new();
        for idl in idls {
            let program = if idl["kind"] == "rootNode" {
                idl["program"].clone()
            } else {
                idl
            };
            let id = from_b58(
                program["publicKey"]
                    .as_str()
                    .ok_or("programNode without publicKey")?,
            )?;
            map.insert(id, program);
        }
        Ok(Programs(map))
    }

    pub fn name(&self, id: &Key) -> Option<&str> {
        self.0.get(id).and_then(|p| p["name"].as_str())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum V {
    /// A number's bits, little end first in the data; signed numbers keep
    /// their two's complement bits.
    Int(u128),
    Bool(bool),
    Str(String),
    Bytes(Vec<u8>),
    Key(Key),
    None,
    List(Vec<V>),
    Struct(Vec<(String, V)>),
    Variant(String, Box<V>),
}

/// An instruction matched against its IDL node.
pub struct Decoded<'a> {
    pub node: &'a Value,
    pub accounts: Vec<(&'a Value, Key)>,
    pub args: Vec<(&'a Value, V)>,
}

impl Decoded<'_> {
    pub fn account(&self, name: &str) -> Option<&Key> {
        self.accounts
            .iter()
            .find(|(node, _)| node["name"] == name)
            .map(|(_, key)| key)
    }

    pub fn arg(&self, name: &str) -> Option<&(&Value, V)> {
        self.args.iter().find(|(node, _)| node["name"] == name)
    }
}

fn kind(node: &Value) -> &str {
    node["kind"].as_str().unwrap_or("")
}

fn number_width(ty: &Value) -> Result<usize, String> {
    match ty["format"].as_str() {
        Some("u8" | "i8") => Ok(1),
        Some("u16" | "i16") => Ok(2),
        Some("u32" | "i32" | "f32") => Ok(4),
        Some("u64" | "i64" | "f64") => Ok(8),
        Some("u128" | "i128") => Ok(16),
        other => Err(format!("number format {other:?} not supported yet")),
    }
}

/// How deep defined types may nest: a type that refers to itself is refused
/// rather than followed forever.
const MAX_TYPE_DEPTH: usize = 32;

/// The program whose `definedTypes` links point to.
struct Types<'a> {
    program: &'a Value,
}

impl Types<'_> {
    fn defined(&self, name: &str) -> Result<&Value, String> {
        self.program["definedTypes"]
            .as_array()
            .and_then(|types| types.iter().find(|t| t["name"] == name))
            .map(|t| &t["type"])
            .ok_or_else(|| format!("defined type {name} not found"))
    }
}

fn take<'d>(data: &'d [u8], pos: &mut usize, n: usize) -> Result<&'d [u8], String> {
    let end = pos
        .checked_add(n)
        .filter(|end| *end <= data.len())
        .ok_or("instruction data too short")?;
    let out = &data[*pos..end];
    *pos = end;
    Ok(out)
}

fn u8_node() -> Value {
    serde_json::json!({ "kind": "numberTypeNode", "format": "u8", "endian": "le" })
}

fn read_number(ty: &Value, data: &[u8], pos: &mut usize) -> Result<u128, String> {
    let bytes = take(data, pos, number_width(ty)?)?;
    let little = ty["endian"].as_str() != Some("be");
    let fold = |acc: u128, b: &u8| (acc << 8) | *b as u128;
    Ok(if little {
        bytes.iter().rev().fold(0, fold)
    } else {
        bytes.iter().fold(0, fold)
    })
}

/// A count or a tag as a number, from the node that encodes it.
fn read_count(ty: &Value, data: &[u8], pos: &mut usize) -> Result<usize, String> {
    let n = read_number(ty, data, pos)?;
    usize::try_from(n).map_err(|_| "a count does not fit".to_string())
}

fn read(
    ty: &Value,
    data: &[u8],
    pos: &mut usize,
    types: &Types,
    depth: usize,
) -> Result<V, String> {
    if depth > MAX_TYPE_DEPTH {
        return Err("types nested too deep".into());
    }
    let next = depth + 1;
    match kind(ty) {
        "numberTypeNode" => Ok(V::Int(read_number(ty, data, pos)?)),
        "booleanTypeNode" => {
            let size = ty.get("size").cloned().unwrap_or_else(u8_node);
            match read_number(&size, data, pos)? {
                0 => Ok(V::Bool(false)),
                1 => Ok(V::Bool(true)),
                other => Err(format!("a boolean is {other}")),
            }
        }
        "publicKeyTypeNode" => Ok(V::Key(take(data, pos, 32)?.try_into().expect("32"))),
        "optionTypeNode" => {
            let prefix = ty.get("prefix").cloned().unwrap_or_else(u8_node);
            match read_number(&prefix, data, pos)? {
                0 => Ok(V::None),
                1 => read(&ty["item"], data, pos, types, next),
                other => Err(format!("bad option tag {other}")),
            }
        }
        "sizePrefixTypeNode" => {
            let len = read_count(&ty["prefix"], data, pos)?;
            read_sized(&ty["type"], data, pos, len, types, next)
        }
        "fixedSizeTypeNode" => {
            let size = ty["size"].as_u64().ok_or("fixed size without size")? as usize;
            read_sized(&ty["type"], data, pos, size, types, next)
        }
        // Bare bytes and strings take the rest of the data.
        "bytesTypeNode" | "stringTypeNode" => {
            let len = data.len() - *pos;
            read_sized(ty, data, pos, len, types, next)
        }
        "structTypeNode" => {
            let mut fields = Vec::new();
            for field in ty["fields"].as_array().ok_or("struct without fields")? {
                let name = field["name"].as_str().unwrap_or("?").to_string();
                fields.push((name, read(&field["type"], data, pos, types, next)?));
            }
            Ok(V::Struct(fields))
        }
        "tupleTypeNode" => {
            let mut items = Vec::new();
            for item in ty["items"].as_array().ok_or("tuple without items")? {
                items.push(read(item, data, pos, types, next)?);
            }
            Ok(V::List(items))
        }
        "arrayTypeNode" => {
            let count = &ty["count"];
            let n = match kind(count) {
                "prefixedCountNode" => Some(read_count(&count["prefix"], data, pos)?),
                "fixedCountNode" => Some(count["value"].as_u64().ok_or("fixed count")? as usize),
                "remainderCountNode" => None,
                other => return Err(format!("array count {other} not supported yet")),
            };
            let mut items = Vec::new();
            match n {
                Some(n) => {
                    // Every item takes at least a byte here: a count past the
                    // data is refused before allocating for it.
                    if n > data.len() - *pos {
                        return Err(format!(
                            "an array of {n} items in {} bytes",
                            data.len() - *pos
                        ));
                    }
                    for _ in 0..n {
                        items.push(read(&ty["item"], data, pos, types, next)?);
                    }
                }
                None => {
                    while *pos < data.len() {
                        items.push(read(&ty["item"], data, pos, types, next)?);
                    }
                }
            }
            Ok(V::List(items))
        }
        "enumTypeNode" => {
            let size = ty.get("size").cloned().unwrap_or_else(u8_node);
            let tag = read_count(&size, data, pos)?;
            let variant = ty["variants"]
                .as_array()
                .and_then(|v| v.get(tag))
                .ok_or_else(|| format!("enum tag {tag} names no variant"))?;
            let name = variant["name"].as_str().unwrap_or("?").to_string();
            let value = match kind(variant) {
                "enumEmptyVariantTypeNode" => V::None,
                "enumStructVariantTypeNode" => read(&variant["struct"], data, pos, types, next)?,
                "enumTupleVariantTypeNode" => read(&variant["tuple"], data, pos, types, next)?,
                other => return Err(format!("enum variant {other} not supported yet")),
            };
            Ok(V::Variant(name, Box::new(value)))
        }
        "definedTypeLinkNode" => {
            let name = ty["name"].as_str().ok_or("type link without name")?;
            read(types.defined(name)?, data, pos, types, next)
        }
        other => Err(format!("type {other} not supported yet")),
    }
}

fn read_sized(
    ty: &Value,
    data: &[u8],
    pos: &mut usize,
    len: usize,
    types: &Types,
    depth: usize,
) -> Result<V, String> {
    let bytes = take(data, pos, len)?;
    match kind(ty) {
        "stringTypeNode" => Ok(V::Str(
            String::from_utf8(bytes.to_vec()).map_err(|_| "a string is not valid UTF-8")?,
        )),
        "bytesTypeNode" => Ok(V::Bytes(bytes.to_vec())),
        // Any other type inside a sized window must fill it exactly.
        _ => {
            let mut inner = 0;
            let value = read(ty, bytes, &mut inner, types, depth)?;
            if inner != bytes.len() {
                return Err(format!(
                    "{} bytes left in a sized value",
                    bytes.len() - inner
                ));
            }
            Ok(value)
        }
    }
}

/// The bytes an argument's default value encodes to, for discriminators.
fn default_bytes(arg: &Value) -> Result<Vec<u8>, String> {
    let value = &arg["defaultValue"];
    match kind(value) {
        "numberValueNode" => {
            let n = value["number"].as_u64().ok_or("default number")? as u128;
            Ok(n.to_le_bytes()[..number_width(&arg["type"])?].to_vec())
        }
        "bytesValueNode" => {
            let hex = value["data"].as_str().ok_or("default bytes")?;
            (0..hex.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).map_err(|e| e.to_string()))
                .collect()
        }
        other => Err(format!(
            "default value {other} not supported as a discriminator"
        )),
    }
}

fn matches(node: &Value, data: &[u8]) -> Result<bool, String> {
    let Some(discriminators) = node["discriminators"].as_array() else {
        return Ok(false);
    };
    for disc in discriminators {
        if kind(disc) == "sizeDiscriminatorNode" {
            if data.len() as u64
                != disc["size"]
                    .as_u64()
                    .ok_or("size discriminator without size")?
            {
                return Ok(false);
            }
            continue;
        }
        // A discriminator kind not supported here never matches: the
        // instruction is then reported as unknown, never misread.
        if kind(disc) != "fieldDiscriminatorNode" {
            return Ok(false);
        }
        let name = disc["name"].as_str().ok_or("discriminator without name")?;
        let offset = disc["offset"].as_u64().unwrap_or(0) as usize;
        let arg = node["arguments"]
            .as_array()
            .and_then(|args| args.iter().find(|a| a["name"] == name))
            .ok_or_else(|| format!("discriminator {name} names no argument"))?;
        let expected = default_bytes(arg)?;
        if data.get(offset..offset + expected.len()) != Some(&expected[..]) {
            return Ok(false);
        }
    }
    Ok(true)
}

/// `None` when the program has no IDL here, or no instruction of it matches.
pub fn decode<'a>(programs: &'a Programs, ix: &Instruction) -> Result<Option<Decoded<'a>>, String> {
    let Some(program) = programs.0.get(&ix.program) else {
        return Ok(None);
    };
    let instructions = program["instructions"]
        .as_array()
        .ok_or("programNode without instructions")?;
    let mut found = None;
    for node in instructions {
        if matches(node, &ix.data)? {
            found = Some(node);
            break;
        }
    }
    let Some(node) = found else {
        return Ok(None);
    };
    let account_nodes = node["accounts"]
        .as_array()
        .ok_or("instruction without accounts")?;
    // Optional accounts may be left out at the end (official IDLs mark them).
    let required = account_nodes
        .iter()
        .filter(|a| a["isOptional"].as_bool() != Some(true))
        .count();
    if ix.accounts.len() < required {
        return Err(format!(
            "{} expects at least {} accounts, got {}",
            node["name"],
            required,
            ix.accounts.len()
        ));
    }
    let accounts = account_nodes
        .iter()
        .zip(ix.accounts.iter().copied())
        .collect();
    let mut args = Vec::new();
    let mut pos = 0;
    let types = Types { program };
    for arg in node["arguments"]
        .as_array()
        .ok_or("instruction without arguments")?
    {
        args.push((arg, read(&arg["type"], &ix.data, &mut pos, &types, 0)?));
    }
    // Bytes no argument accounts for could carry anything: refuse them
    // rather than show a reading that leaves part of the data out.
    if pos != ix.data.len() {
        return Err(format!(
            "{} has {} bytes no argument describes",
            node["name"],
            ix.data.len() - pos
        ));
    }
    Ok(Some(Decoded {
        node,
        accounts,
        args,
    }))
}
