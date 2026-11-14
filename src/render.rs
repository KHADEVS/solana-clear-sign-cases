//! A renderer for sRFC 39 display metadata, written to follow the draft as
//! literally as possible. Where the draft is ambiguous, the choice made is
//! named in a comment and in the cases' notes.

use crate::message::Instruction;
use crate::util::{format_amount, from_b58, short, Key};
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
        for (key, data) in map("accounts") {
            let data = base64::engine::general_purpose::STANDARD
                .decode(data.as_str().ok_or("account data is not text")?)
                .map_err(|e| format!("account data: {e}"))?;
            ctx.accounts.insert(from_b58(&key)?, data);
        }
        Ok(ctx)
    }

    pub fn name_or_short(&self, key: &Key) -> String {
        self.names.get(key).cloned().unwrap_or_else(|| short(key))
    }
}

/// Program IDLs by program id.
pub struct Programs(HashMap<Key, Value>);

impl Programs {
    pub fn new(programs: Vec<Value>) -> Result<Self, String> {
        let mut map = HashMap::new();
        for program in programs {
            let id = from_b58(
                program["publicKey"]
                    .as_str()
                    .ok_or("programNode without publicKey")?,
            )?;
            map.insert(id, program);
        }
        Ok(Programs(map))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum V {
    Int(u128),
    Str(String),
    Bytes(Vec<u8>),
    Key(Key),
    None,
}

/// An instruction matched against its IDL node.
pub struct Decoded<'a> {
    pub node: &'a Value,
    pub accounts: Vec<(&'a Value, Key)>,
    pub args: Vec<(&'a Value, V)>,
}

impl Decoded<'_> {
    fn account(&self, name: &str) -> Option<&Key> {
        self.accounts
            .iter()
            .find(|(node, _)| node["name"] == name)
            .map(|(_, key)| key)
    }

    fn arg(&self, name: &str) -> Option<&(&Value, V)> {
        self.args.iter().find(|(node, _)| node["name"] == name)
    }
}

fn kind(node: &Value) -> &str {
    node["kind"].as_str().unwrap_or("")
}

fn number_width(ty: &Value) -> Result<usize, String> {
    match ty["format"].as_str() {
        Some("u8") => Ok(1),
        Some("u16") => Ok(2),
        Some("u32") => Ok(4),
        Some("u64") => Ok(8),
        Some("u128") => Ok(16),
        other => Err(format!("number format {other:?} not supported yet")),
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

fn read(ty: &Value, data: &[u8], pos: &mut usize) -> Result<V, String> {
    match kind(ty) {
        "numberTypeNode" => {
            let bytes = take(data, pos, number_width(ty)?)?;
            Ok(V::Int(
                bytes
                    .iter()
                    .rev()
                    .fold(0u128, |acc, b| (acc << 8) | *b as u128),
            ))
        }
        "publicKeyTypeNode" => Ok(V::Key(take(data, pos, 32)?.try_into().expect("32"))),
        "optionTypeNode" => {
            let default_prefix = serde_json::json!({ "kind": "numberTypeNode", "format": "u8" });
            let prefix = ty.get("prefix").unwrap_or(&default_prefix);
            match read(prefix, data, pos)? {
                V::Int(0) => Ok(V::None),
                V::Int(1) => read(&ty["item"], data, pos),
                other => Err(format!("bad option tag {other:?}")),
            }
        }
        "sizePrefixTypeNode" => {
            let V::Int(len) = read(&ty["prefix"], data, pos)? else {
                return Err("size prefix is not a number".into());
            };
            read_sized(&ty["type"], data, pos, len as usize)
        }
        "fixedSizeTypeNode" => {
            let size = ty["size"].as_u64().ok_or("fixed size without size")? as usize;
            read_sized(&ty["type"], data, pos, size)
        }
        other => Err(format!("type {other} not supported yet")),
    }
}

fn read_sized(ty: &Value, data: &[u8], pos: &mut usize, len: usize) -> Result<V, String> {
    let bytes = take(data, pos, len)?;
    match kind(ty) {
        "stringTypeNode" => Ok(V::Str(
            String::from_utf8(bytes.to_vec()).map_err(|_| "a string is not valid UTF-8")?,
        )),
        "bytesTypeNode" => Ok(V::Bytes(bytes.to_vec())),
        other => Err(format!("sized type {other} not supported yet")),
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
        if kind(disc) != "fieldDiscriminatorNode" {
            return Err(format!("discriminator {} not supported yet", kind(disc)));
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
    if ix.accounts.len() < account_nodes.len() {
        return Err(format!(
            "{} expects {} accounts, got {}",
            node["name"],
            account_nodes.len(),
            ix.accounts.len()
        ));
    }
    let accounts = account_nodes
        .iter()
        .zip(ix.accounts.iter().copied())
        .collect();
    let mut args = Vec::new();
    let mut pos = 0;
    for arg in node["arguments"]
        .as_array()
        .ok_or("instruction without arguments")?
    {
        args.push((arg, read(&arg["type"], &ix.data, &mut pos)?));
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

/// sRFC 39 `skip`. "withAdditionalMetadata" follows the draft's example (the
/// mint shows only when no metadata is available), not its definition, which
/// says the opposite. See the cases' notes.
fn shown(node: &Value, with_metadata: bool) -> bool {
    match node["display"]["skip"].as_str().unwrap_or("never") {
        "always" => false,
        "withAdditionalMetadata" => !with_metadata,
        _ => true,
    }
}

fn label(node: &Value) -> String {
    node["display"]["label"]
        .as_str()
        .or(node["name"].as_str())
        .unwrap_or("?")
        .to_string()
}

/// AccountFormatter. With metadata, a token account shows as its owner (the
/// draft's example does this; its last open question asks whether it should).
pub fn account_text(key: &Key, ctx: Option<&Context>) -> String {
    let Some(ctx) = ctx else {
        return short(key);
    };
    if let Some(account) = ctx.token_accounts.get(key) {
        return ctx.name_or_short(&account.owner);
    }
    if let Some(token) = ctx.tokens.get(key) {
        return token.symbol.clone();
    }
    ctx.name_or_short(key)
}

fn plain(value: &V) -> String {
    match value {
        V::Int(n) => n.to_string(),
        V::Str(s) => s.clone(),
        V::Bytes(b) => b.iter().map(|x| format!("{x:02x}")).collect(),
        V::Key(k) => short(k),
        // The draft does not say how to show an absent optional value.
        V::None => "none".into(),
    }
}

fn arg_text(node: &Value, value: &V, d: &Decoded, ctx: Option<&Context>) -> Result<String, String> {
    let formatter = &node["display"]["formatter"];
    match kind(formatter) {
        "" => Ok(plain(value)),
        "amountFormatterNode" => {
            let V::Int(raw) = value else {
                return Err("amountFormatterNode on a value that is not a number".into());
            };
            let mut decimals = None;
            let mut symbol = None;
            if formatter["isNative"].as_bool() == Some(true) {
                decimals = Some(9);
                symbol = Some("SOL".to_string());
            }
            // The draft defines `decimals` as a number of places; its example
            // passes the name of an argument. Both are accepted.
            match &formatter["decimals"] {
                Value::Number(n) => decimals = n.as_u64().map(|n| n as u32),
                Value::String(name) => match d.arg(name) {
                    Some((_, V::Int(n))) => decimals = Some(*n as u32),
                    _ => return Err(format!("decimals names no number argument: {name}")),
                },
                _ => {}
            }
            if let (Some(account), Some(ctx)) = (formatter["token"].as_str(), ctx) {
                let key = d
                    .account(account)
                    .ok_or_else(|| format!("token names no account: {account}"))?;
                // The draft's `token` names the mint; for instructions that
                // carry no mint (approve), it names a token account, read
                // through its mint.
                let mint = ctx.token_accounts.get(key).map(|a| a.mint).unwrap_or(*key);
                if let Some(info) = ctx.tokens.get(&mint) {
                    symbol = Some(info.symbol.clone());
                    decimals = decimals.or(Some(info.decimals));
                }
            }
            let number = match decimals {
                Some(places) => format_amount(*raw, places),
                None => raw.to_string(),
            };
            Ok(match symbol {
                Some(symbol) => format!("{number} {symbol}"),
                None => number,
            })
        }
        other => Err(format!("formatter {other} not supported yet")),
    }
}

/// `None` when a placeholder needs metadata this context does not have.
fn interpolate(template: &str, d: &Decoded, ctx: &Context) -> Result<Option<String>, String> {
    let mut out = String::new();
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let close = rest[open..]
            .find('}')
            .ok_or("unclosed { in an interpolated intent")?
            + open;
        let path = &rest[open + 1..close];
        let (name, field) = match path.split_once('.') {
            Some((name, field)) => (name, Some(field)),
            None => (path, None),
        };
        let text = if let Some(key) = d.account(name) {
            match field {
                None => account_text(key, Some(ctx)),
                Some("owner") => match ctx.token_accounts.get(key) {
                    Some(account) => ctx.name_or_short(&account.owner),
                    None => return Ok(None),
                },
                Some(other) => return Err(format!("field .{other} not supported in {{{path}}}")),
            }
        } else if let Some((node, value)) = d.arg(name) {
            arg_text(node, value, d, Some(ctx))?
        } else {
            return Err(format!("{{{path}}} names no account or argument"));
        };
        out.push_str(&text);
        rest = &rest[close + 1..];
    }
    out.push_str(rest);
    Ok(Some(out))
}

#[derive(Debug, PartialEq)]
pub struct Rendered {
    /// The interpolated intents joined with " and "; only with metadata, and
    /// only when every instruction has one.
    pub interpolated: Option<String>,
    pub fallback: Vec<String>,
}

pub fn render(
    programs: &Programs,
    instructions: &[Instruction],
    ctx: Option<&Context>,
) -> Result<Rendered, String> {
    let with_metadata = ctx.is_some();
    let mut fallback = Vec::new();
    let mut intents: Option<Vec<String>> = ctx.map(|_| Vec::new());
    for ix in instructions {
        let Some(d) = decode(programs, ix)? else {
            // The draft says nothing about programs without an IDL.
            fallback.push(format!("Unknown program: {}", short(&ix.program)));
            fallback.push(format!("Data: {}", plain(&V::Bytes(ix.data.clone()))));
            intents = None;
            continue;
        };
        let display = &d.node["display"];
        let intent = display["intent"]
            .as_str()
            .or(d.node["name"].as_str())
            .unwrap_or("?");
        fallback.push(intent.to_string());
        for (node, key) in &d.accounts {
            if shown(node, with_metadata) {
                fallback.push(format!("{}: {}", label(node), account_text(key, ctx)));
            }
        }
        for (node, value) in &d.args {
            if shown(node, with_metadata) {
                fallback.push(format!(
                    "{}: {}",
                    label(node),
                    arg_text(node, value, &d, ctx)?
                ));
            }
        }
        if let (Some(ctx), Some(_)) = (ctx, intents.as_ref()) {
            let piece = match display["interpolatedIntent"].as_str() {
                Some(template) => interpolate(template, &d, ctx)?,
                None => Some(intent.to_string()),
            };
            match piece {
                Some(piece) => intents.as_mut().expect("checked").push(piece),
                None => intents = None,
            }
        }
    }
    Ok(Rendered {
        interpolated: intents.map(|pieces| pieces.join(" and ")),
        fallback,
    })
}
