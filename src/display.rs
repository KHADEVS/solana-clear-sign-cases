//! Codama's instruction display, written a second time.
//!
//! `@codama/dynamic-instructions` is the reference: it reads a Codama IDL and
//! its display nodes, and returns an intent, a sentence and a list of fields
//! for an instruction. This module does the same from the specification and
//! from that renderer's observable behaviour, with no code in common, so that
//! the two can be compared on every case. Where the specification leaves the
//! result open, the reference is followed and the choice is listed in
//! `DISPLAY.md`.

use crate::util::{b58, from_b58, Key};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

/// An account of an instruction, as the message lists it.
#[derive(Clone, Debug)]
pub struct Meta {
    pub key: Key,
    pub signer: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Field {
    pub label: String,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Display {
    pub intent: String,
    pub interpolated_intent: Option<String>,
    pub fields: Vec<Field>,
}

#[derive(Debug, PartialEq)]
pub enum Outcome {
    Shown(Display),
    /// No IDL for the program, or no instruction of it matches the data.
    Unknown,
    /// The IDL or an account it needs cannot be read: the reference throws.
    Failed(String),
}

/// Codama roots, by the address of their main program.
pub struct Idls {
    roots: Vec<Value>,
    by_program: HashMap<Key, usize>,
}

impl Idls {
    pub fn new(idls: Vec<Value>) -> Result<Self, String> {
        let mut by_program = HashMap::new();
        let mut roots = Vec::new();
        for idl in idls {
            let root = if idl["kind"] == "rootNode" {
                idl
            } else {
                serde_json::json!({ "kind": "rootNode", "program": idl })
            };
            let id = root["program"]["publicKey"]
                .as_str()
                .ok_or("programNode without publicKey")?;
            by_program.insert(from_b58(id)?, roots.len());
            roots.push(root);
        }
        // A program that only comes along in another IDL is found there.
        for (index, root) in roots.iter().enumerate() {
            for program in programs(root).skip(1) {
                if let Some(Ok(key)) = program["publicKey"].as_str().map(from_b58) {
                    by_program.entry(key).or_insert(index);
                }
            }
        }
        Ok(Idls { roots, by_program })
    }
}

/// A decoded value, shaped as the reference's decoder shapes it: 64-bit and
/// wider integers are big integers, bytes are `["base64", text]`, an option is
/// `{__option, value}`, an enum variant `{__discriminator, __kind, ...}`.
#[derive(Clone, Debug, PartialEq)]
pub enum Js {
    Undefined,
    Num(f64),
    Big(Big),
    Bool(bool),
    Str(String),
    Arr(Vec<Js>),
    Obj(Vec<(String, Js)>),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Big {
    pub negative: bool,
    pub magnitude: u128,
}

impl Big {
    fn text(&self) -> String {
        format!("{}{}", if self.negative { "-" } else { "" }, self.magnitude)
    }

    fn to_f64(self) -> f64 {
        let value = self.magnitude as f64;
        if self.negative {
            -value
        } else {
            value
        }
    }
}

impl Js {
    fn get(&self, name: &str) -> Option<&Js> {
        match self {
            Js::Obj(fields) => fields.iter().find(|(k, _)| k == name).map(|(_, v)| v),
            _ => None,
        }
    }

    fn is_numeric(&self) -> bool {
        matches!(self, Js::Num(_) | Js::Big(_))
    }

    fn number(&self) -> Option<f64> {
        match self {
            Js::Num(n) => Some(*n),
            Js::Big(b) => Some(b.to_f64()),
            _ => None,
        }
    }
}

/// Sets a property the way an object literal does: a repeated key keeps its
/// first position and takes the last value.
fn set(fields: &mut Vec<(String, Js)>, key: &str, value: Js) {
    match fields.iter_mut().find(|(k, _)| k == key) {
        Some(entry) => entry.1 = value,
        None => fields.push((key.to_string(), value)),
    }
}

enum Fail {
    /// The data does not decode: the reference shows nothing.
    Decode(String),
    /// The IDL cannot be turned into a decoder: the reference throws.
    Throw(String),
}

type R<T> = Result<T, Fail>;

fn decode_error<T>(why: impl Into<String>) -> R<T> {
    Err(Fail::Decode(why.into()))
}

fn throw<T>(why: impl Into<String>) -> R<T> {
    Err(Fail::Throw(why.into()))
}

fn kind(node: &Value) -> &str {
    node["kind"].as_str().unwrap_or("")
}

/// Where links are looked up: the root, and the program the current node is in.
#[derive(Clone, Copy)]
struct Scope<'a> {
    root: &'a Value,
    program: &'a Value,
}

fn programs(root: &Value) -> impl Iterator<Item = &Value> {
    std::iter::once(&root["program"]).chain(
        root["additionalPrograms"]
            .as_array()
            .map(|list| list.iter())
            .into_iter()
            .flatten(),
    )
}

fn list<'a>(node: &'a Value, name: &str) -> &'a [Value] {
    node[name].as_array().map(Vec::as_slice).unwrap_or(&[])
}

/// A defined type or an account, from a link node.
fn linked<'a>(scope: Scope<'a>, link: &Value, among: &str) -> Option<(Scope<'a>, &'a Value)> {
    let name = link["name"].as_str()?;
    let program = match link["program"]["name"].as_str() {
        Some(wanted) => programs(scope.root).find(|p| p["name"] == wanted)?,
        None => scope.program,
    };
    let found = list(program, among).iter().find(|n| n["name"] == name)?;
    Some((
        Scope {
            root: scope.root,
            program,
        },
        found,
    ))
}

// --- Names -------------------------------------------------------------------

fn capitalize(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase(),
        None => String::new(),
    }
}

/// `SOME_CONSTANT` and `someName` both become `Some Name`.
pub fn title_case(text: &str) -> String {
    let upper = |c: char| c.is_ascii_uppercase() || c.is_ascii_digit();
    let mut parts = text.split('_');
    let first = parts.next().unwrap_or("");
    let rest: Vec<&str> = parts.collect();
    let constant = first.starts_with(|c: char| c.is_ascii_uppercase())
        && first.chars().all(upper)
        && !rest.is_empty()
        && rest.iter().all(|p| !p.is_empty() && p.chars().all(upper));
    if constant {
        return text
            .to_lowercase()
            .split('_')
            .map(capitalize)
            .collect::<Vec<_>>()
            .join(" ");
    }
    let mut spaced = String::new();
    for c in text.chars() {
        if c.is_ascii_uppercase() {
            spaced.push(' ');
        }
        spaced.push(c);
    }
    spaced
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(capitalize)
        .collect::<Vec<_>>()
        .join(" ")
}

fn pascal_case(text: &str) -> String {
    title_case(text).replace(' ', "")
}

fn camel_case(text: &str) -> String {
    let pascal = pascal_case(text);
    let mut chars = pascal.chars();
    match chars.next() {
        Some(first) => first.to_lowercase().collect::<String>() + chars.as_str(),
        None => pascal,
    }
}

// --- Numbers and text, as JavaScript prints them -------------------------------

/// `Number.prototype.toString`.
fn js_number(value: f64) -> String {
    if value.is_nan() {
        return "NaN".into();
    }
    if value.is_infinite() {
        return if value > 0.0 { "Infinity" } else { "-Infinity" }.into();
    }
    if value == 0.0 {
        return "0".into();
    }
    let sign = if value < 0.0 { "-" } else { "" };
    // The shortest digits that read back as the same number, and where the point goes.
    let scientific = format!("{:e}", value.abs());
    let (mantissa, exponent) = scientific.split_once('e').expect("an exponent");
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    let k = digits.len() as i32;
    let n = exponent.parse::<i32>().expect("a number") + 1;
    let body = if k <= n && n <= 21 {
        format!("{digits}{}", "0".repeat((n - k) as usize))
    } else if 0 < n && n <= 21 {
        format!("{}.{}", &digits[..n as usize], &digits[n as usize..])
    } else if -6 < n && n <= 0 {
        format!("0.{}{digits}", "0".repeat((-n) as usize))
    } else {
        let e = n - 1;
        let tail = if k > 1 {
            format!(".{}", &digits[1..])
        } else {
            String::new()
        };
        format!(
            "{}{tail}e{}{}",
            &digits[..1],
            if e < 0 { "-" } else { "+" },
            e.abs()
        )
    };
    format!("{sign}{body}")
}

fn json_string(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// `JSON.stringify`, with big integers written as strings.
fn json(value: &Js) -> Option<String> {
    Some(match value {
        Js::Undefined => return None,
        Js::Num(n) if n.is_finite() => js_number(*n),
        Js::Num(_) => "null".into(),
        Js::Big(b) => json_string(&b.text()),
        Js::Bool(b) => b.to_string(),
        Js::Str(s) => json_string(s),
        Js::Arr(items) => {
            let parts: Vec<String> = items
                .iter()
                .map(|item| json(item).unwrap_or_else(|| "null".into()))
                .collect();
            format!("[{}]", parts.join(","))
        }
        Js::Obj(fields) => {
            let parts: Vec<String> = fields
                .iter()
                .filter_map(|(k, v)| json(v).map(|text| format!("{}:{text}", json_string(k))))
                .collect();
            format!("{{{}}}", parts.join(","))
        }
    })
}

/// A value with no presentation of its own.
fn raw(value: &Js) -> String {
    match value {
        Js::Undefined => String::new(),
        Js::Str(s) => s.clone(),
        Js::Big(b) => b.text(),
        Js::Num(n) => js_number(*n),
        Js::Bool(b) => b.to_string(),
        Js::Obj(_) => match value.get("__kind") {
            Some(Js::Str(name)) => title_case(name),
            _ => json(value).unwrap_or_default(),
        },
        Js::Arr(_) => json(value).unwrap_or_default(),
    }
}

/// A property key made from a value, for maps.
fn property_key(value: &Js) -> String {
    match value {
        Js::Undefined => "undefined".into(),
        Js::Str(s) => s.clone(),
        Js::Num(n) => js_number(*n),
        Js::Big(b) => b.text(),
        Js::Bool(b) => b.to_string(),
        Js::Arr(items) => items
            .iter()
            .map(|item| match item {
                Js::Undefined => String::new(),
                other => property_key(other),
            })
            .collect::<Vec<_>>()
            .join(","),
        Js::Obj(_) => "[object Object]".into(),
    }
}

/// The order in which an object lists its keys: array indices first, in
/// ascending order, then the others as they were added.
fn object_order(entries: Vec<(String, Js)>) -> Vec<(String, Js)> {
    let mut fields: Vec<(String, Js)> = Vec::new();
    for (key, value) in entries {
        set(&mut fields, &key, value);
    }
    let index = |key: &str| -> Option<u32> {
        let n: u32 = key.parse().ok()?;
        (n.to_string() == key && n != u32::MAX).then_some(n)
    };
    let (mut indices, others): (Vec<_>, Vec<_>) =
        fields.into_iter().partition(|(k, _)| index(k).is_some());
    indices.sort_by_key(|(k, _)| index(k));
    indices.extend(others);
    indices
}

// --- Decoding ----------------------------------------------------------------

const MAX_DEPTH: usize = 64;

fn number_width(format: &str) -> R<Option<usize>> {
    Ok(Some(match format {
        "u8" | "i8" => 1,
        "u16" | "i16" => 2,
        "u32" | "i32" | "f32" => 4,
        "u64" | "i64" | "f64" => 8,
        "u128" | "i128" => 16,
        "shortU16" => return Ok(None),
        other => return throw(format!("number format {other} is not known")),
    }))
}

fn number_format(ty: &Value) -> R<&str> {
    match ty["format"].as_str() {
        Some(format) if kind(ty) == "numberTypeNode" => Ok(format),
        _ => throw("a number type is expected"),
    }
}

fn read_number(ty: &Value, bytes: &[u8], at: usize) -> R<(Js, usize)> {
    let format = number_format(ty)?;
    let Some(width) = number_width(format)? else {
        // shortU16: seven bits a byte, low bits first, at most three bytes.
        let mut value = 0u32;
        for (i, shift) in [0, 7, 14].into_iter().enumerate() {
            let Some(byte) = bytes.get(at + i) else {
                return decode_error("a short u16 ends too early");
            };
            value |= ((byte & 0x7f) as u32) << shift;
            if byte & 0x80 == 0 {
                if value > 0xffff {
                    return decode_error("a short u16 above 65535");
                }
                return Ok((Js::Num(value as f64), at + i + 1));
            }
        }
        return decode_error("a short u16 longer than three bytes");
    };
    let Some(window) = bytes.get(at..at + width) else {
        return decode_error(format!("{format} needs {width} bytes"));
    };
    let mut le = [0u8; 16];
    le[..width].copy_from_slice(window);
    if ty["endian"].as_str() == Some("be") {
        le[..width].reverse();
    }
    let bits = u128::from_le_bytes(le);
    let signed = |bits: u128| -> i128 {
        let shift = 128 - 8 * width as u32;
        ((bits << shift) as i128) >> shift
    };
    let big = |value: i128| {
        Js::Big(Big {
            negative: value < 0,
            magnitude: value.unsigned_abs(),
        })
    };
    let value = match format {
        "u8" | "u16" | "u32" => Js::Num(bits as f64),
        "i8" | "i16" | "i32" => Js::Num(signed(bits) as f64),
        "u64" | "u128" => Js::Big(Big {
            negative: false,
            magnitude: bits,
        }),
        "i64" | "i128" => big(signed(bits)),
        "f32" => Js::Num(f32::from_bits(bits as u32) as f64),
        _ => Js::Num(f64::from_bits(bits as u64)),
    };
    Ok((value, at + width))
}

/// A size, a count or a tag: a number used as an index.
fn read_index(ty: &Value, bytes: &[u8], at: usize) -> R<(f64, usize)> {
    let (value, next) = read_number(ty, bytes, at)?;
    Ok((value.number().unwrap_or(f64::NAN), next))
}

/// A part that every node of its kind has: without it, the reference throws.
fn part<'a>(node: &'a Value, name: &str) -> R<&'a Value> {
    match &node[name] {
        Value::Null => throw(format!("a {} without its {name}", kind(node))),
        found => Ok(found),
    }
}

fn encoding(ty: &Value) -> R<&str> {
    match ty["encoding"].as_str() {
        Some(name) => Ok(name),
        None => throw("text without its encoding"),
    }
}

fn base64(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn text_of(bytes: &[u8], encoding: &str) -> R<String> {
    Ok(match encoding {
        "utf8" => {
            // Malformed bytes become U+FFFD, a leading byte order mark and every NUL go.
            let text = String::from_utf8_lossy(bytes);
            text.strip_prefix('\u{feff}')
                .unwrap_or(&text)
                .replace('\0', "")
        }
        "base16" => bytes.iter().map(|b| format!("{b:02x}")).collect(),
        "base58" => bs58::encode(bytes).into_string(),
        "base64" => base64(bytes),
        other => return throw(format!("bytes encoding {other} is not known")),
    })
}

fn bytes_of(text: &str, encoding: &str) -> R<Vec<u8>> {
    use base64::Engine;
    let bad = || Fail::Throw(format!("{text:?} is not {encoding}"));
    match encoding {
        "utf8" => Ok(text.as_bytes().to_vec()),
        "base16" => {
            if !text.len().is_multiple_of(2) || !text.is_ascii() {
                return Err(bad());
            }
            (0..text.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&text[i..i + 2], 16).map_err(|_| bad()))
                .collect()
        }
        "base58" => bs58::decode(text).into_vec().map_err(|_| bad()),
        "base64" => base64::engine::general_purpose::STANDARD
            .decode(text)
            .map_err(|_| bad()),
        other => throw(format!("bytes encoding {other} is not known")),
    }
}

/// The number of bytes a type always takes, when it has one.
fn fixed_size(scope: Scope, ty: &Value, depth: usize) -> R<Option<usize>> {
    if depth > MAX_DEPTH {
        return throw("types nested too deep");
    }
    let next = depth + 1;
    let sum = |scope: Scope, parts: &[Value], field: Option<&str>| -> R<Option<usize>> {
        let mut total = 0;
        for part in parts {
            let ty = field.map_or(part, |f| &part[f]);
            match fixed_size(scope, ty, next)? {
                Some(size) => total += size,
                None => return Ok(None),
            }
        }
        Ok(Some(total))
    };
    let constants = |parts: &[Value]| -> R<usize> {
        let mut total = 0;
        for part in parts {
            total += encode(scope, &part["type"], &part["value"], next)?.len();
        }
        Ok(total)
    };
    Ok(match kind(ty) {
        "numberTypeNode" => number_width(number_format(ty)?)?,
        "booleanTypeNode" => fixed_size(scope, part(ty, "size")?, next)?,
        "publicKeyTypeNode" => Some(32),
        "fixedSizeTypeNode" => ty["size"].as_u64().map(|n| n as usize),
        "amountTypeNode" | "dateTimeTypeNode" | "solAmountTypeNode" => {
            fixed_size(scope, &ty["number"], next)?
        }
        "structTypeNode" => sum(scope, list(ty, "fields"), Some("type"))?,
        "tupleTypeNode" => sum(scope, list(ty, "items"), None)?,
        "arrayTypeNode" | "setTypeNode" => {
            match (kind(&ty["count"]), ty["count"]["value"].as_u64()) {
                ("fixedCountNode", Some(0)) => Some(0),
                ("fixedCountNode", Some(n)) => {
                    fixed_size(scope, &ty["item"], next)?.map(|size| size * n as usize)
                }
                _ => None,
            }
        }
        "optionTypeNode" if ty["fixed"] == true => {
            match (
                fixed_size(scope, part(ty, "prefix")?, next)?,
                fixed_size(scope, &ty["item"], next)?,
            ) {
                (Some(prefix), Some(item)) => Some(prefix + item),
                _ => None,
            }
        }
        "zeroableOptionTypeNode" => fixed_size(scope, &ty["item"], next)?,
        "enumTypeNode" => {
            let tag = fixed_size(scope, part(ty, "size")?, next)?;
            let mut sizes = HashSet::new();
            for variant in list(ty, "variants") {
                sizes.insert(match kind(variant) {
                    "enumStructVariantTypeNode" => fixed_size(scope, &variant["struct"], next)?,
                    "enumTupleVariantTypeNode" => fixed_size(scope, &variant["tuple"], next)?,
                    _ => Some(0),
                });
            }
            match (tag, sizes.len(), sizes.into_iter().next().flatten()) {
                (Some(tag), 1, Some(payload)) => Some(tag + payload),
                (_, 0, _) => Some(0),
                _ => None,
            }
        }
        "definedTypeLinkNode" => match linked(scope, ty, "definedTypes") {
            Some((inner, defined)) => fixed_size(inner, &defined["type"], next)?,
            None => return throw(format!("defined type {} not found", ty["name"])),
        },
        "hiddenPrefixTypeNode" => fixed_size(scope, &ty["type"], next)?
            .map(|size| constants(list(ty, "prefix")).map(|extra| size + extra))
            .transpose()?,
        "hiddenSuffixTypeNode" => fixed_size(scope, &ty["type"], next)?
            .map(|size| constants(list(ty, "suffix")).map(|extra| size + extra))
            .transpose()?,
        "preOffsetTypeNode" | "postOffsetTypeNode" => {
            let padding = if ty["strategy"] == "padded" {
                ty["offset"].as_i64().unwrap_or(0)
            } else {
                0
            };
            fixed_size(scope, &ty["type"], next)?
                .map(|size| (size as i64 + padding).max(0) as usize)
        }
        _ => None,
    })
}

/// The bytes of a constant: a discriminator, a hidden prefix, a zero value.
fn encode(scope: Scope, ty: &Value, value: &Value, depth: usize) -> R<Vec<u8>> {
    if depth > MAX_DEPTH {
        return throw("types nested too deep");
    }
    let next = depth + 1;
    let unsupported = || -> R<Vec<u8>> {
        throw(format!(
            "a constant {} of type {} is not supported",
            kind(value),
            kind(ty)
        ))
    };
    match (kind(ty), kind(value)) {
        ("numberTypeNode", "numberValueNode") => encode_number(ty, &value["number"]),
        ("amountTypeNode" | "dateTimeTypeNode" | "solAmountTypeNode", _) => {
            encode(scope, &ty["number"], value, next)
        }
        ("fixedSizeTypeNode", _) => {
            let mut bytes = encode(scope, &ty["type"], value, next)?;
            bytes.resize(ty["size"].as_u64().unwrap_or(0) as usize, 0);
            Ok(bytes)
        }
        // Padding before or after a constant is part of it, as zeros. An offset
        // of another kind moves within the constant's own bytes, and is refused
        // when it leaves them.
        ("preOffsetTypeNode" | "postOffsetTypeNode", _) => {
            let body = encode(scope, &ty["type"], value, next)?;
            let before = kind(ty) == "preOffsetTypeNode";
            let offset = ty["offset"].as_i64().unwrap_or(0);
            let length = body.len() as i64;
            let strategy = ty["strategy"].as_str().unwrap_or("relative");
            if strategy == "padded" {
                if offset < 0 {
                    return unsupported();
                }
                let zeros = vec![0; offset as usize];
                return Ok(if before {
                    [zeros, body].concat()
                } else {
                    [body, zeros].concat()
                });
            }
            let position = match (strategy, before) {
                ("absolute", _) if offset < 0 && length > 0 => offset.rem_euclid(length),
                ("absolute", _) | ("preOffset", false) | (_, true) => offset,
                (_, false) => length + offset,
            };
            // Written from there, the constant must still fit; after it, the place must exist.
            let stays = if before {
                position == 0
            } else {
                (0..=length).contains(&position)
            };
            if !stays {
                return throw("an offset that leaves the bytes of its constant");
            }
            Ok(body)
        }
        ("sizePrefixTypeNode", _) => {
            let body = encode(scope, &ty["type"], value, next)?;
            let length = serde_json::json!(body.len());
            let mut bytes = encode_number(&ty["prefix"], &length)?;
            bytes.extend(body);
            Ok(bytes)
        }
        ("bytesTypeNode", "bytesValueNode") => bytes_of(
            value["data"].as_str().unwrap_or(""),
            value["encoding"].as_str().unwrap_or(""),
        ),
        ("bytesTypeNode", "constantValueNode") => {
            encode(scope, &value["type"], &value["value"], next)
        }
        ("stringTypeNode", "stringValueNode") => {
            bytes_of(value["string"].as_str().unwrap_or(""), encoding(ty)?)
        }
        ("booleanTypeNode", "booleanValueNode") => {
            let flag = serde_json::json!(u8::from(value["boolean"] == true));
            encode_number(part(ty, "size")?, &flag)
        }
        ("publicKeyTypeNode", "publicKeyValueNode") => {
            from_b58(value["publicKey"].as_str().unwrap_or(""))
                .map(|key| key.to_vec())
                .map_err(Fail::Throw)
        }
        ("definedTypeLinkNode", _) => match linked(scope, ty, "definedTypes") {
            Some((inner, defined)) => encode(inner, &defined["type"], value, next),
            None => throw(format!("defined type {} not found", ty["name"])),
        },
        ("enumTypeNode", "enumValueNode") => {
            // The value names its enum by a link, which must lead somewhere.
            if linked(scope, &value["enum"], "definedTypes").is_none() {
                return throw(format!("defined type {} not found", value["enum"]["name"]));
            }
            let wanted = pascal_case(value["variant"].as_str().unwrap_or(""));
            let found = list(ty, "variants")
                .iter()
                .enumerate()
                .find(|(_, v)| pascal_case(v["name"].as_str().unwrap_or("")) == wanted);
            let Some((index, variant)) = found else {
                return throw(format!("enum variant {} not found", value["variant"]));
            };
            let tag = match &variant["discriminator"] {
                Value::Null => serde_json::json!(index),
                given => given.clone(),
            };
            let mut bytes = encode_number(part(ty, "size")?, &tag)?;
            match kind(variant) {
                "enumStructVariantTypeNode" => {
                    bytes.extend(encode(scope, &variant["struct"], &value["value"], next)?)
                }
                "enumTupleVariantTypeNode" => {
                    bytes.extend(encode(scope, &variant["tuple"], &value["value"], next)?)
                }
                _ => {}
            }
            Ok(bytes)
        }
        ("tupleTypeNode", "tupleValueNode") => {
            let (types, values) = (list(ty, "items"), list(value, "items"));
            if types.len() != values.len() {
                return unsupported();
            }
            let mut bytes = Vec::new();
            for (ty, value) in types.iter().zip(values) {
                bytes.extend(encode(scope, ty, value, next)?);
            }
            Ok(bytes)
        }
        ("structTypeNode", "structValueNode") => {
            let mut bytes = Vec::new();
            for field in list(ty, "fields") {
                let given = list(value, "fields")
                    .iter()
                    .find(|f| f["name"] == field["name"]);
                let Some(given) = given else {
                    return unsupported();
                };
                bytes.extend(encode(scope, &field["type"], &given["value"], next)?);
            }
            Ok(bytes)
        }
        ("arrayTypeNode" | "setTypeNode", "arrayValueNode" | "setValueNode") => {
            let items = list(value, "items");
            let count = part(ty, "count")?;
            let mut bytes = match kind(count) {
                "prefixedCountNode" => {
                    encode_number(part(count, "prefix")?, &serde_json::json!(items.len()))?
                }
                "fixedCountNode" if count["value"].as_u64() == Some(items.len() as u64) => {
                    Vec::new()
                }
                "remainderCountNode" => Vec::new(),
                _ => return unsupported(),
            };
            for item in items {
                bytes.extend(encode(scope, &ty["item"], item, next)?);
            }
            Ok(bytes)
        }
        ("optionTypeNode", "someValueNode") => {
            let mut bytes = encode_number(part(ty, "prefix")?, &serde_json::json!(1))?;
            bytes.extend(encode(scope, &ty["item"], &value["value"], next)?);
            Ok(bytes)
        }
        ("optionTypeNode", "noneValueNode") => {
            let mut bytes = encode_number(part(ty, "prefix")?, &serde_json::json!(0))?;
            if ty["fixed"] == true {
                // An absent value of a fixed size leaves zeros where it would be.
                match fixed_size(scope, &ty["item"], next)? {
                    Some(size) => bytes.resize(bytes.len() + size, 0),
                    None => return unsupported(),
                }
            }
            Ok(bytes)
        }
        _ => unsupported(),
    }
}

fn encode_number(ty: &Value, number: &Value) -> R<Vec<u8>> {
    let format = number_format(ty)?;
    let out_of_range = || Fail::Throw(format!("{number} does not fit in {format}"));
    let Some(width) = number_width(format)? else {
        let mut value = number
            .as_u64()
            .filter(|n| *n <= 0xffff)
            .ok_or_else(out_of_range)?;
        let mut bytes = Vec::new();
        loop {
            let low = (value & 0x7f) as u8;
            value >>= 7;
            if value == 0 {
                bytes.push(low);
                return Ok(bytes);
            }
            bytes.push(low | 0x80);
        }
    };
    let mut bytes = match format {
        "f32" => (number.as_f64().ok_or_else(out_of_range)? as f32)
            .to_le_bytes()
            .to_vec(),
        "f64" => number
            .as_f64()
            .ok_or_else(out_of_range)?
            .to_le_bytes()
            .to_vec(),
        _ if format.starts_with('u') => {
            let value = number.as_u64().ok_or_else(out_of_range)? as u128;
            if width < 16 && value >> (8 * width) != 0 {
                return Err(out_of_range());
            }
            value.to_le_bytes()[..width].to_vec()
        }
        _ => {
            let value = number
                .as_i64()
                .or_else(|| number.as_u64().and_then(|n| i64::try_from(n).ok()))
                .ok_or_else(out_of_range)? as i128;
            let bits = 8 * width as u32;
            if bits < 128 && (value < -(1i128 << (bits - 1)) || value >= 1i128 << (bits - 1)) {
                return Err(out_of_range());
            }
            value.to_le_bytes()[..width].to_vec()
        }
    };
    if ty["endian"].as_str() == Some("be") {
        bytes.reverse();
    }
    Ok(bytes)
}

fn starts_with_at(bytes: &[u8], wanted: &[u8], at: usize) -> bool {
    bytes.get(at..at + wanted.len()) == Some(wanted)
}

fn option(value: Option<Js>) -> Js {
    match value {
        Some(value) => Js::Obj(vec![
            ("__option".into(), Js::Str("Some".into())),
            ("value".into(), value),
        ]),
        None => Js::Obj(vec![("__option".into(), Js::Str("None".into()))]),
    }
}

/// Reads a value at `at` and returns it with the position after it. `bytes` is
/// the window the type lives in: a type with no size of its own takes it whole.
fn read(scope: Scope, ty: &Value, bytes: &[u8], at: usize, depth: usize) -> R<(Js, usize)> {
    if depth > MAX_DEPTH {
        return throw("types nested too deep");
    }
    let next = depth + 1;
    let item_size = |ty: &Value| -> R<usize> {
        match fixed_size(scope, ty, next)? {
            Some(size) => Ok(size),
            None => throw("an option of this kind needs an item of fixed size"),
        }
    };
    match kind(ty) {
        "numberTypeNode" => read_number(ty, bytes, at),
        "amountTypeNode" | "dateTimeTypeNode" | "solAmountTypeNode" => {
            read(scope, &ty["number"], bytes, at, next)
        }
        "booleanTypeNode" => {
            let (value, after) = read_index(part(ty, "size")?, bytes, at)?;
            Ok((Js::Bool(value == 1.0), after))
        }
        "publicKeyTypeNode" => match bytes.get(at..at + 32) {
            Some(key) => Ok((Js::Str(bs58::encode(key).into_string()), at + 32)),
            None => decode_error("an address needs 32 bytes"),
        },
        "bytesTypeNode" => {
            let rest = bytes.get(at..).unwrap_or(&[]);
            Ok((
                Js::Arr(vec![Js::Str("base64".into()), Js::Str(base64(rest))]),
                bytes.len().max(at),
            ))
        }
        "stringTypeNode" => {
            let rest = bytes.get(at..).unwrap_or(&[]);
            let text = text_of(rest, encoding(ty)?)?;
            Ok((Js::Str(text), bytes.len().max(at)))
        }
        "fixedSizeTypeNode" => {
            let size = ty["size"].as_u64().unwrap_or(0) as usize;
            let Some(window) = at.checked_add(size).and_then(|end| bytes.get(at..end)) else {
                return decode_error(format!("a fixed size of {size} bytes ends too early"));
            };
            // An inner type with its own size reads that many bytes, zeros added if they lack.
            let mut window = window.to_vec();
            if let Some(inner) = fixed_size(scope, &ty["type"], next)? {
                window.resize(inner, 0);
            }
            let (value, _) = read(scope, &ty["type"], &window, 0, next)?;
            Ok((value, at + size))
        }
        "sizePrefixTypeNode" => {
            let (size, start) = read_index(&ty["prefix"], bytes, at)?;
            let size = size as usize;
            let Some(window) = start
                .checked_add(size)
                .and_then(|end| bytes.get(start..end))
            else {
                return decode_error(format!("a size prefix of {size} bytes ends too early"));
            };
            let (value, _) = read(scope, &ty["type"], window, 0, next)?;
            Ok((value, start + size))
        }
        "optionTypeNode" => {
            let (tag, start) = read_index(part(ty, "prefix")?, bytes, at)?;
            if tag == 1.0 {
                let (value, after) = read(scope, &ty["item"], bytes, start, next)?;
                return Ok((option(Some(value)), after));
            }
            if ty["fixed"] == true {
                let skipped = item_size(&ty["item"])?;
                if bytes.len() < start + skipped {
                    return decode_error("a fixed option ends too early");
                }
                return Ok((option(None), start + skipped));
            }
            Ok((option(None), start))
        }
        "remainderOptionTypeNode" => {
            if at >= bytes.len() {
                return Ok((option(None), at));
            }
            let (value, after) = read(scope, &ty["item"], bytes, at, next)?;
            Ok((option(Some(value)), after))
        }
        "zeroableOptionTypeNode" => {
            let size = item_size(&ty["item"])?;
            let zero = match ty.get("zeroValue").filter(|v| !v.is_null()) {
                Some(constant) => encode(scope, &constant["type"], &constant["value"], next)?,
                None => vec![0; size],
            };
            if starts_with_at(bytes, &zero, at) {
                return Ok((option(None), at + zero.len()));
            }
            let (value, after) = read(scope, &ty["item"], bytes, at, next)?;
            Ok((option(Some(value)), after))
        }
        "structTypeNode" => {
            let mut fields = Vec::new();
            let mut at = at;
            for field in list(ty, "fields") {
                let (value, after) = read(scope, &field["type"], bytes, at, next)?;
                set(&mut fields, field["name"].as_str().unwrap_or(""), value);
                at = after;
            }
            Ok((Js::Obj(object_order(fields)), at))
        }
        "tupleTypeNode" => {
            let mut items = Vec::new();
            let mut at = at;
            for item in list(ty, "items") {
                let (value, after) = read(scope, item, bytes, at, next)?;
                items.push(value);
                at = after;
            }
            Ok((Js::Arr(items), at))
        }
        "arrayTypeNode" | "setTypeNode" => {
            read_items(scope, &ty["item"], &ty["count"], bytes, at, next)
                .map(|(items, after)| (Js::Arr(items), after))
        }
        "mapTypeNode" => {
            let pair = serde_json::json!({ "kind": "tupleTypeNode", "items": [ty["key"].clone(), ty["value"].clone()] });
            let (entries, after) = read_items(scope, &pair, &ty["count"], bytes, at, next)?;
            let entries = entries
                .into_iter()
                .filter_map(|entry| match entry {
                    Js::Arr(mut pair) if pair.len() == 2 => {
                        let value = pair.pop()?;
                        Some((property_key(&pair[0]), value))
                    }
                    _ => None,
                })
                .collect();
            Ok((Js::Obj(object_order(entries)), after))
        }
        "enumTypeNode" => {
            let size = part(ty, "size")?;
            let (tag, _) = read_index(size, bytes, at)?;
            let variants = list(ty, "variants");
            let tag_of = |index: usize, variant: &Value| {
                variant["discriminator"].as_f64().unwrap_or(index as f64)
            };
            let Some((index, variant)) = variants
                .iter()
                .enumerate()
                .find(|(index, variant)| tag_of(*index, variant) == tag)
            else {
                return decode_error(format!("enum tag {tag} names no variant"));
            };
            // The tag is read again as a constant, in the one form the encoder writes.
            let discriminator = variant["discriminator"].clone();
            let number = if discriminator.is_null() {
                serde_json::json!(index)
            } else {
                discriminator
            };
            let constant = encode_number(size, &number)?;
            if !starts_with_at(bytes, &constant, at) {
                return decode_error("an enum tag in an unusual form");
            }
            let start = at + constant.len();
            let mut fields = vec![
                (
                    "__discriminator".to_string(),
                    Js::Num(tag_of(index, variant)),
                ),
                (
                    "__kind".to_string(),
                    Js::Str(pascal_case(variant["name"].as_str().unwrap_or(""))),
                ),
            ];
            let after = match kind(variant) {
                "enumStructVariantTypeNode" => {
                    let (value, after) = read(scope, &variant["struct"], bytes, start, next)?;
                    if let Js::Obj(inner) = value {
                        for (key, value) in inner {
                            set(&mut fields, &key, value);
                        }
                    }
                    after
                }
                "enumTupleVariantTypeNode" => {
                    let (value, after) = read(scope, &variant["tuple"], bytes, start, next)?;
                    set(&mut fields, "fields", value);
                    after
                }
                _ => start,
            };
            Ok((Js::Obj(object_order(fields)), after))
        }
        "definedTypeLinkNode" => match linked(scope, ty, "definedTypes") {
            Some((inner, defined)) => read(inner, &defined["type"], bytes, at, next),
            None => throw(format!("defined type {} not found", ty["name"])),
        },
        "hiddenPrefixTypeNode" => {
            let at = skip_constants(scope, list(ty, "prefix"), bytes, at, next)?;
            read(scope, &ty["type"], bytes, at, next)
        }
        "hiddenSuffixTypeNode" => {
            let (value, after) = read(scope, &ty["type"], bytes, at, next)?;
            Ok((
                value,
                skip_constants(scope, list(ty, "suffix"), bytes, after, next)?,
            ))
        }
        "sentinelTypeNode" => {
            let sentinel = encode(
                scope,
                &ty["sentinel"]["type"],
                &ty["sentinel"]["value"],
                next,
            )?;
            let rest = bytes.get(at..).unwrap_or(&[]);
            let found = (!sentinel.is_empty())
                .then(|| rest.windows(sentinel.len()).position(|w| w == sentinel))
                .flatten();
            let Some(end) = found else {
                return decode_error("the sentinel is missing");
            };
            let (value, _) = read(scope, &ty["type"], &rest[..end], 0, next)?;
            Ok((value, at + end + sentinel.len()))
        }
        "preOffsetTypeNode" | "postOffsetTypeNode" => {
            let offset = ty["offset"].as_i64().unwrap_or(0);
            let length = bytes.len() as i64;
            let wrap = |n: i64| if length == 0 { 0 } else { n.rem_euclid(length) };
            let within = |n: i64| -> R<usize> {
                if n < 0 || n > length {
                    return decode_error("an offset outside the data");
                }
                Ok(n as usize)
            };
            let strategy = ty["strategy"].as_str().unwrap_or("relative");
            if kind(ty) == "preOffsetTypeNode" {
                let start = match strategy {
                    "absolute" if offset < 0 => wrap(offset),
                    "absolute" => offset,
                    _ => at as i64 + offset,
                };
                return read(scope, &ty["type"], bytes, within(start)?, next);
            }
            let (value, after) = read(scope, &ty["type"], bytes, at, next)?;
            let end = match strategy {
                "absolute" if offset < 0 => wrap(offset),
                "absolute" => offset,
                "preOffset" => at as i64 + offset,
                _ => after as i64 + offset,
            };
            Ok((value, within(end)?))
        }
        other => throw(format!("type {other} is not supported")),
    }
}

/// What the reference refuses before it reads any data, because it builds its
/// decoder for the whole instruction first: a node with a part missing, a
/// kind, a format or an encoding it does not know, a link to nothing. A type
/// that holds itself is not refused here: it is read as far as the data goes.
fn validate<'a>(scope: Scope<'a>, ty: &'a Value, seen: &mut Vec<*const Value>) -> R<()> {
    let count = |seen: &mut Vec<*const Value>| -> R<()> {
        let count = part(ty, "count")?;
        match kind(count) {
            "prefixedCountNode" => validate(scope, part(count, "prefix")?, seen),
            "fixedCountNode" | "remainderCountNode" => Ok(()),
            other => throw(format!("a count of kind {other:?} is not known")),
        }
    };
    match kind(ty) {
        "numberTypeNode" => number_width(number_format(ty)?).map(|_| ()),
        "publicKeyTypeNode" | "bytesTypeNode" => Ok(()),
        "stringTypeNode" => text_of(&[], encoding(ty)?).map(|_| ()),
        "booleanTypeNode" => validate(scope, part(ty, "size")?, seen),
        "amountTypeNode" | "dateTimeTypeNode" | "solAmountTypeNode" => {
            validate(scope, part(ty, "number")?, seen)
        }
        "fixedSizeTypeNode"
        | "hiddenPrefixTypeNode"
        | "hiddenSuffixTypeNode"
        | "sentinelTypeNode"
        | "preOffsetTypeNode"
        | "postOffsetTypeNode" => validate(scope, part(ty, "type")?, seen),
        "sizePrefixTypeNode" => {
            validate(scope, part(ty, "prefix")?, seen)?;
            validate(scope, part(ty, "type")?, seen)
        }
        "optionTypeNode" => {
            validate(scope, part(ty, "prefix")?, seen)?;
            validate(scope, part(ty, "item")?, seen)
        }
        "remainderOptionTypeNode" | "zeroableOptionTypeNode" => {
            validate(scope, part(ty, "item")?, seen)
        }
        "structTypeNode" => {
            for field in list(ty, "fields") {
                validate(scope, part(field, "type")?, seen)?;
            }
            Ok(())
        }
        "tupleTypeNode" => {
            for item in list(ty, "items") {
                validate(scope, item, seen)?;
            }
            Ok(())
        }
        "arrayTypeNode" | "setTypeNode" => {
            count(seen)?;
            validate(scope, part(ty, "item")?, seen)
        }
        "mapTypeNode" => {
            count(seen)?;
            validate(scope, part(ty, "key")?, seen)?;
            validate(scope, part(ty, "value")?, seen)
        }
        "enumTypeNode" => {
            validate(scope, part(ty, "size")?, seen)?;
            for variant in list(ty, "variants") {
                match kind(variant) {
                    "enumStructVariantTypeNode" => validate(scope, part(variant, "struct")?, seen)?,
                    "enumTupleVariantTypeNode" => validate(scope, part(variant, "tuple")?, seen)?,
                    _ => {}
                }
            }
            Ok(())
        }
        "definedTypeLinkNode" => match linked(scope, ty, "definedTypes") {
            Some((inner, defined)) => {
                if seen.contains(&(defined as *const Value)) {
                    return Ok(());
                }
                seen.push(defined);
                validate(inner, &defined["type"], seen)
            }
            None => throw(format!("defined type {} not found", ty["name"])),
        },
        other => throw(format!("type {other} is not supported")),
    }
}

fn skip_constants(
    scope: Scope,
    constants: &[Value],
    bytes: &[u8],
    at: usize,
    depth: usize,
) -> R<usize> {
    let mut at = at;
    for constant in constants {
        let wanted = encode(scope, &constant["type"], &constant["value"], depth)?;
        if !starts_with_at(bytes, &wanted, at) {
            return decode_error("a constant is not where it is expected");
        }
        at += wanted.len();
    }
    Ok(at)
}

fn read_items(
    scope: Scope,
    item: &Value,
    count: &Value,
    bytes: &[u8],
    at: usize,
    depth: usize,
) -> R<(Vec<Js>, usize)> {
    let mut items = Vec::new();
    let mut at = at;
    let empty_item = fixed_size(scope, item, depth)? == Some(0);
    let wanted = match kind(count) {
        "prefixedCountNode" => {
            // A counted list with nothing left to read is an empty list.
            if at >= bytes.len() {
                return Ok((items, at));
            }
            let (count, after) = read_index(&count["prefix"], bytes, at)?;
            at = after;
            Some(count)
        }
        "fixedCountNode" => Some(count["value"].as_f64().unwrap_or(0.0)),
        "remainderCountNode" => None,
        other => return throw(format!("a count of kind {other:?} is not known")),
    };
    match wanted {
        Some(count) => {
            if empty_item && count > 1_000_000.0 {
                return decode_error("a list of too many empty items");
            }
            let mut index = 0.0;
            while index < count {
                let (value, after) = read(scope, item, bytes, at, depth)?;
                items.push(value);
                at = after;
                index += 1.0;
            }
        }
        None => {
            if empty_item {
                return throw("a list that takes the rest of the data needs items with a size");
            }
            while at < bytes.len() {
                let (value, after) = read(scope, item, bytes, at, depth)?;
                if after <= at {
                    return decode_error("a list item that reads nothing");
                }
                items.push(value);
                at = after;
            }
        }
    }
    Ok((items, at))
}

// --- Finding the instruction ---------------------------------------------------

fn matches(scope: Scope, instruction: &Value, discriminator: &Value, data: &[u8]) -> R<bool> {
    let offset = discriminator["offset"].as_u64().unwrap_or(0) as usize;
    match kind(discriminator) {
        "constantDiscriminatorNode" => {
            let constant = &discriminator["constant"];
            let wanted = encode(scope, &constant["type"], &constant["value"], 0)?;
            Ok(starts_with_at(data, &wanted, offset))
        }
        "fieldDiscriminatorNode" => {
            let name = &discriminator["name"];
            let Some(field) = list(instruction, "arguments")
                .iter()
                .find(|a| a["name"] == *name)
            else {
                return throw(format!("discriminator field {name} not found"));
            };
            if field["defaultValue"].is_null() {
                return throw(format!("discriminator field {name} has no default value"));
            }
            let wanted = encode(scope, &field["type"], &field["defaultValue"], 0)?;
            Ok(starts_with_at(data, &wanted, offset))
        }
        "sizeDiscriminatorNode" => Ok(discriminator["size"].as_u64() == Some(data.len() as u64)),
        other => throw(format!("discriminator {other} is not known")),
    }
}

fn identify<'a>(root: &'a Value, program: &Key, data: &[u8]) -> R<Option<(Scope<'a>, &'a Value)>> {
    let address = b58(program);
    let candidates: Vec<&Value> = programs(root)
        .filter(|p| p["publicKey"].as_str() == Some(address.as_str()))
        .collect();
    for program in &candidates {
        let scope = Scope { root, program };
        for instruction in list(program, "instructions") {
            let discriminators = list(instruction, "discriminators");
            if discriminators.is_empty() {
                continue;
            }
            let mut all = true;
            for discriminator in discriminators {
                if !matches(scope, instruction, discriminator, data)? {
                    all = false;
                    break;
                }
            }
            if all {
                return Ok(Some((scope, instruction)));
            }
        }
    }
    // A program with a single instruction and no discriminator: that one.
    for program in &candidates {
        if let [only] = list(program, "instructions") {
            if list(only, "discriminators").is_empty() {
                return Ok(Some((Scope { root, program }, only)));
            }
        }
    }
    Ok(None)
}

// --- The display ---------------------------------------------------------------

struct Context<'a> {
    scope: Scope<'a>,
    instruction: &'a Value,
    data: Js,
    /// The accounts the IDL names, with their addresses.
    accounts: Vec<(&'a str, Key)>,
    remaining: &'a [Meta],
    fetch: &'a HashMap<Key, Vec<u8>>,
    /// Arguments and accounts already shown through another value.
    consumed: HashSet<String>,
}

impl<'a> Context<'a> {
    fn account(&self, name: &str) -> Option<String> {
        self.accounts
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, key)| b58(key))
    }

    fn provided(&self, key: &str) -> Option<&'a Value> {
        list(self.instruction, "provides")
            .iter()
            .rev()
            .find(|p| p["name"] == key)
            .map(|p| &p["node"])
    }
}

/// The inner type of wrappers that only say how a value sits in the bytes.
fn unwrapped(ty: &Value) -> &Value {
    match kind(ty) {
        "fixedSizeTypeNode"
        | "hiddenPrefixTypeNode"
        | "hiddenSuffixTypeNode"
        | "postOffsetTypeNode"
        | "preOffsetTypeNode"
        | "sentinelTypeNode"
        | "sizePrefixTypeNode" => unwrapped(&ty["type"]),
        _ => ty,
    }
}

/// The type whose display metadata applies: through options and links.
fn display_type<'a>(scope: Scope<'a>, ty: &'a Value) -> (Scope<'a>, &'a Value) {
    let mut scope = scope;
    let mut ty = unwrapped(ty);
    let mut followed: Vec<*const Value> = Vec::new();
    loop {
        match kind(ty) {
            "optionTypeNode" | "remainderOptionTypeNode" | "zeroableOptionTypeNode" => {
                ty = unwrapped(&ty["item"]);
            }
            "definedTypeLinkNode" => {
                let Some((inner, defined)) = linked(scope, ty, "definedTypes") else {
                    break;
                };
                if followed.contains(&(defined as *const Value)) {
                    break;
                }
                followed.push(defined);
                scope = inner;
                ty = unwrapped(&defined["type"]);
            }
            _ => break,
        }
    }
    (scope, ty)
}

static UNDEFINED: Js = Js::Undefined;

/// The value behind any number of `Some`, or `None` if one of them is empty.
fn through_options(value: &Js) -> Option<&Js> {
    match value.get("__option") {
        Some(Js::Str(tag)) if tag == "None" => None,
        Some(Js::Str(_)) => through_options(value.get("value").unwrap_or(&UNDEFINED)),
        _ => Some(value),
    }
}

/// Follows an injected value to what provides it.
fn injection_target<'a>(
    context: &Context<'a>,
    node: &'a Value,
    seen: &mut Vec<String>,
) -> Option<&'a Value> {
    if kind(node) != "injectedValueNode" {
        return Some(node);
    }
    let key = node["key"].as_str().unwrap_or("").to_string();
    if seen.contains(&key) {
        return None;
    }
    seen.push(key.clone());
    if let Some(provided) = context.provided(&key) {
        return injection_target(context, provided, seen);
    }
    match node.get("fallback").filter(|f| !f.is_null()) {
        Some(fallback) => injection_target(context, fallback, seen),
        None => None,
    }
}

/// A number or a text for a display node, or nothing.
fn injected<'a>(context: &Context<'a>, node: &'a Value) -> R<Option<Js>> {
    let plain = |value: Option<&Js>| match value {
        Some(value @ (Js::Num(_) | Js::Big(_) | Js::Str(_))) => Some(value.clone()),
        _ => None,
    };
    Ok(match kind(node) {
        "numberValueNode" => node["number"].as_f64().map(Js::Num),
        "stringValueNode" => node["string"].as_str().map(|s| Js::Str(s.to_string())),
        "injectedValueNode" => match injection_target(context, node, &mut Vec::new()) {
            Some(target) => injected(context, target)?,
            None => None,
        },
        "argumentValueNode" => plain(context.data.get(node["name"].as_str().unwrap_or(""))),
        "accountValueNode" => context
            .account(node["name"].as_str().unwrap_or(""))
            .map(Js::Str),
        "accountFieldValueNode" => {
            let name = node["account"].as_str().unwrap_or("");
            let Some(address) = context.account(name) else {
                return Ok(None);
            };
            let Some(bytes) = from_b58(&address)
                .ok()
                .and_then(|key| context.fetch.get(&key))
            else {
                return Ok(None);
            };
            // The account's layout comes from the instruction account's link.
            let wanted = camel_case(name);
            let link = list(context.instruction, "accounts")
                .iter()
                .find(|a| a["name"] == wanted.as_str())
                .map(|a| &a["accountLink"])
                .filter(|l| !l.is_null());
            let Some((scope, account)) = link.and_then(|l| linked(context.scope, l, "accounts"))
            else {
                return Ok(None);
            };
            match read(scope, &account["data"], bytes, 0, 0) {
                Ok((data, _)) => match node["path"].as_str() {
                    Some(path) => plain(data.get(path)),
                    None => None,
                },
                Err(Fail::Decode(why) | Fail::Throw(why)) => {
                    return throw(format!("account {address} does not decode: {why}"))
                }
            }
        }
        _ => None,
    })
}

/// An integer divided by a power of ten, without trailing zeros.
fn scaled(value: &Js, decimals: f64) -> Option<String> {
    let (negative, magnitude) = match value {
        Js::Big(b) => (b.negative, b.magnitude),
        Js::Num(n) if n.fract() == 0.0 && n.abs() < 3.4e38 => (*n < 0.0, n.abs() as u128),
        _ => return None,
    };
    if decimals.fract() != 0.0 || !(0.0..=10_000.0).contains(&decimals) {
        return None;
    }
    let decimals = decimals as usize;
    let sign = if negative { "-" } else { "" };
    if decimals == 0 {
        return Some(match value {
            Js::Num(n) => js_number(*n),
            _ => format!("{sign}{magnitude}"),
        });
    }
    let digits = format!("{magnitude:0>width$}", width = decimals + 1);
    let (whole, fraction) = digits.split_at(digits.len() - decimals);
    let fraction = fraction.trim_end_matches('0');
    Some(if fraction.is_empty() {
        format!("{sign}{whole}")
    } else {
        format!("{sign}{whole}.{fraction}")
    })
}

/// `Date.prototype.toISOString` of a number of milliseconds since 1970.
fn iso_date(milliseconds: f64) -> Option<String> {
    let ms = milliseconds.trunc();
    if !ms.is_finite() || ms.abs() > 8.64e15 {
        return None;
    }
    let ms = ms as i64;
    let days = ms.div_euclid(86_400_000);
    let rest = ms.rem_euclid(86_400_000);
    // Days since 1970 to a date of the Gregorian calendar.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let mp = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    let year = if (0..=9999).contains(&year) {
        format!("{year:04}")
    } else {
        format!("{}{:06}", if year < 0 { "-" } else { "+" }, year.abs())
    };
    Some(format!(
        "{year}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        rest / 3_600_000,
        rest / 60_000 % 60,
        rest / 1_000 % 60,
        rest % 1_000
    ))
}

fn seconds(value: &Js, node: &Value) -> Option<f64> {
    let ticks = value.number()?;
    let per_second = match &node["ticksPerSecond"] {
        Value::Null => 1.0,
        other => other.as_f64()?,
    };
    (ticks.is_finite() && per_second > 0.0).then(|| ticks / per_second)
}

/// A number shown through its display node; `None` when it cannot be.
fn number_text<'a>(context: &Context<'a>, display: &'a Value, value: &Js) -> R<Option<String>> {
    Ok(match kind(display) {
        "amountNumberDisplayNode" => {
            let decimals = match display.get("decimals").filter(|d| !d.is_null()) {
                Some(node) => injected(context, node)?,
                None => Some(Js::Num(0.0)),
            };
            let Some(decimals) = decimals.as_ref().and_then(Js::number) else {
                return Ok(None);
            };
            let Some(amount) = scaled(value, decimals) else {
                return Ok(None);
            };
            let unit = match display.get("unit").filter(|u| !u.is_null()) {
                Some(node) => injected(context, node)?,
                None => None,
            };
            Some(match unit {
                Some(Js::Str(unit)) if !unit.is_empty() => format!("{amount} {unit}"),
                _ => amount,
            })
        }
        "dateTimeNumberDisplayNode" => seconds(value, display).and_then(|s| iso_date(s * 1e3)),
        "durationNumberDisplayNode" => seconds(value, display).filter(|s| *s >= 0.0).map(|s| {
            let whole = s.floor();
            let two = |n: f64| format!("{:0>2}", js_number(n));
            format!(
                "{}:{}:{}",
                two((whole / 3600.0).floor()),
                two((whole % 3600.0 / 60.0).floor()),
                two(whole % 60.0)
            )
        }),
        _ => None,
    })
}

fn variant_label(variant: &Value) -> String {
    match variant["display"]["label"].as_str() {
        Some(label) => label.to_string(),
        None => title_case(variant["name"].as_str().unwrap_or("")),
    }
}

fn enum_text(ty: &Value, value: &Js) -> String {
    let variants = list(ty, "variants");
    if let Some(tag) = value.number() {
        return variants
            .iter()
            .enumerate()
            .find(|(i, v)| v["discriminator"].as_f64().unwrap_or(*i as f64) == tag)
            .map(|(_, v)| variant_label(v))
            .unwrap_or_else(|| raw(value));
    }
    let name = match (value, value.get("__kind")) {
        (Js::Str(name), _) | (_, Some(Js::Str(name))) => pascal_case(name),
        _ => return raw(value),
    };
    variants
        .iter()
        .find(|v| pascal_case(v["name"].as_str().unwrap_or("")) == name)
        .map(variant_label)
        .unwrap_or_else(|| raw(value))
}

/// The text of a value, and whether it had to fall back to the raw number.
fn value_text<'a>(
    context: &Context<'a>,
    scope: Scope<'a>,
    ty: &'a Value,
    value: &Js,
) -> R<(bool, String)> {
    let Some(inner) = through_options(value) else {
        return Ok((false, "none".into()));
    };
    let (scope, resolved) = display_type(scope, ty);
    if let ("arrayTypeNode", Js::Arr(items)) = (kind(resolved), inner) {
        let mut degraded = false;
        let mut texts = Vec::new();
        for item in items {
            let (bad, text) = value_text(context, scope, &resolved["item"], item)?;
            degraded |= bad;
            texts.push(text);
        }
        return Ok((degraded, texts.join(", ")));
    }
    let display = &resolved["display"];
    if kind(resolved) == "numberTypeNode" && !display.is_null() && inner.is_numeric() {
        if let Some(text) = number_text(context, display, inner)? {
            return Ok((false, text));
        }
        if kind(display) == "amountNumberDisplayNode" {
            return Ok((true, format!("{} (raw)", raw(inner))));
        }
    }
    if let ("stringTypeNode", false, Js::Str(text)) = (kind(resolved), display.is_null(), inner) {
        // Slices count UTF-16 units, as the reference does.
        let units: Vec<u16> = text.encode_utf16().collect();
        let index = |v: &Value, default: i64| -> usize {
            let n = v.as_i64().unwrap_or(default);
            let n = if n < 0 { n + units.len() as i64 } else { n };
            n.clamp(0, units.len() as i64) as usize
        };
        if display["sliceStart"].is_null() && display["sliceEnd"].is_null() {
            return Ok((false, text.clone()));
        }
        let (start, end) = (
            index(&display["sliceStart"], 0),
            index(&display["sliceEnd"], units.len() as i64),
        );
        let slice = if start < end {
            &units[start..end]
        } else {
            &[][..]
        };
        return Ok((false, String::from_utf16_lossy(slice)));
    }
    if kind(resolved) == "enumTypeNode" {
        return Ok((false, enum_text(resolved, inner)));
    }
    Ok((false, raw(inner)))
}

fn skipped(context: &Context, skip: &Value, name: &str) -> bool {
    match skip.as_str() {
        Some("always") => true,
        Some("whenInjected") => context.consumed.contains(name),
        _ => false,
    }
}

fn label_of(node: &Value) -> String {
    match node["display"]["label"].as_str() {
        Some(label) => label.to_string(),
        None => title_case(node["name"].as_str().unwrap_or("")),
    }
}

/// One field for a member, or one per address for a list of addresses.
fn member_fields<'a>(
    context: &Context<'a>,
    scope: Scope<'a>,
    label: String,
    ty: &'a Value,
    value: &Js,
    out: &mut Vec<Field>,
) -> R<()> {
    let (inner_scope, resolved) = display_type(scope, ty);
    if let (Some(Js::Arr(items)), "arrayTypeNode") = (through_options(value), kind(resolved)) {
        let (_, item) = display_type(inner_scope, &resolved["item"]);
        if kind(item) == "publicKeyTypeNode" {
            for (index, element) in items.iter().enumerate() {
                let (_, text) = value_text(context, inner_scope, &resolved["item"], element)?;
                out.push(Field {
                    label: if items.len() > 1 {
                        format!("{label} #{}", index + 1)
                    } else {
                        label.clone()
                    },
                    value: text,
                });
            }
            return Ok(());
        }
    }
    let (_, text) = value_text(context, scope, ty, value)?;
    out.push(Field { label, value: text });
    Ok(())
}

fn argument_fields(context: &Context, out: &mut Vec<Field>) -> R<()> {
    for argument in list(context.instruction, "arguments") {
        let name = argument["name"].as_str().unwrap_or("");
        let display = &argument["display"];
        if skipped(context, &display["skip"], name) {
            continue;
        }
        let value = context.data.get(name).unwrap_or(&UNDEFINED);
        let (scope, resolved) = display_type(context.scope, &argument["type"]);
        if let (true, "structTypeNode", Some(Js::Obj(_))) = (
            display["flatten"] == true,
            kind(resolved),
            through_options(value),
        ) {
            let inner = through_options(value).expect("checked above");
            let prefix = display["flattenPrefix"].as_str().unwrap_or("");
            for field in list(resolved, "fields") {
                let field_name = field["name"].as_str().unwrap_or("");
                if skipped(context, &field["display"]["skip"], field_name) {
                    continue;
                }
                member_fields(
                    context,
                    scope,
                    format!("{prefix}{}", label_of(field)),
                    &field["type"],
                    inner.get(field_name).unwrap_or(&UNDEFINED),
                    out,
                )?;
            }
            continue;
        }
        member_fields(
            context,
            context.scope,
            label_of(argument),
            &argument["type"],
            value,
            out,
        )?;
    }
    Ok(())
}

fn account_fields(context: &Context, out: &mut Vec<Field>) {
    for account in list(context.instruction, "accounts") {
        let name = account["name"].as_str().unwrap_or("");
        if skipped(context, &account["display"]["skip"], name) {
            continue;
        }
        if let Some(address) = context.account(name) {
            out.push(Field {
                label: label_of(account),
                value: address,
            });
        }
    }
}

/// The accounts after the named ones, group by group.
fn remaining_fields(context: &Context, out: &mut Vec<Field>) {
    let groups = list(context.instruction, "remainingAccounts");
    if groups.is_empty() || context.remaining.is_empty() {
        return;
    }
    let mut cursor = 0;
    for (index, group) in groups.iter().enumerate() {
        let last = index == groups.len() - 1;
        let fits = |meta: &Meta| match &group["isSigner"] {
            Value::String(either) if either == "either" => true,
            other => other.as_bool().unwrap_or(false) == meta.signer,
        };
        let mut taken = Vec::new();
        while cursor < context.remaining.len() && (last || fits(&context.remaining[cursor])) {
            taken.push(b58(&context.remaining[cursor].key));
            cursor += 1;
        }
        let name = (kind(&group["value"]) == "argumentValueNode")
            .then(|| group["value"]["name"].as_str())
            .flatten();
        if skipped(context, &group["display"]["skip"], name.unwrap_or("")) {
            continue;
        }
        let label = match (group["display"]["label"].as_str(), name) {
            (Some(label), _) => label.to_string(),
            (None, Some(name)) => title_case(name),
            (None, None) => "Remaining Accounts".to_string(),
        };
        let many = taken.len() > 1;
        for (i, address) in taken.into_iter().enumerate() {
            out.push(Field {
                label: if many {
                    format!("{label} #{}", i + 1)
                } else {
                    label.clone()
                },
                value: address,
            });
        }
    }
}

/// `${data.name}` and `${accounts.name}` in a sentence; nothing if one of them has no value.
fn sentence(context: &Context) -> R<Option<String>> {
    let Some(template) = context.instruction["display"]["interpolatedIntent"].as_str() else {
        return Ok(None);
    };
    let mut out = String::new();
    let mut rest = template;
    while let Some(start) = rest.find("${") {
        let (before, tail) = rest.split_at(start);
        out.push_str(before);
        let Some(end) = tail.find('}') else {
            // Never closed: the rest is kept as written.
            rest = tail;
            break;
        };
        let inside = tail[2..end].trim_matches(|c: char| c.is_whitespace());
        let parsed = inside.split_once('.').filter(|(root, name)| {
            (*root == "data" || *root == "accounts")
                && !name.is_empty()
                && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        });
        let Some((root, name)) = parsed else {
            // Not a placeholder: kept as written.
            out.push_str("${");
            rest = &tail[2..];
            continue;
        };
        let text = if root == "data" {
            let argument = list(context.instruction, "arguments")
                .iter()
                .find(|a| a["name"] == name);
            match (argument, context.data.get(name)) {
                (Some(argument), Some(value)) => {
                    let (degraded, text) =
                        value_text(context, context.scope, &argument["type"], value)?;
                    (!degraded).then_some(text)
                }
                _ => None,
            }
        } else {
            context.account(name)
        };
        match text {
            Some(text) => out.push_str(&text),
            None => return Ok(None),
        }
        rest = &tail[end + 1..];
    }
    out.push_str(rest);
    Ok(Some(out))
}

/// The arguments and accounts that an amount already shows through its decimals or unit.
fn consumed(context: &Context) -> R<HashSet<String>> {
    fn collect<'a>(scope: Scope<'a>, ty: &'a Value, flatten: bool, out: &mut Vec<&'a Value>) {
        let (scope, resolved) = display_type(scope, ty);
        let display = &resolved["display"];
        if kind(resolved) == "numberTypeNode" && kind(display) == "amountNumberDisplayNode" {
            for name in ["decimals", "unit"] {
                if kind(&display[name]) == "injectedValueNode" {
                    out.push(&display[name]);
                }
            }
        } else if flatten && kind(resolved) == "structTypeNode" {
            for field in list(resolved, "fields") {
                collect(scope, &field["type"], false, out);
            }
        }
    }
    let mut nodes = Vec::new();
    for argument in list(context.instruction, "arguments") {
        collect(
            context.scope,
            &argument["type"],
            argument["display"]["flatten"] == true,
            &mut nodes,
        );
    }
    let mut targets: Vec<&Value> = Vec::new();
    for node in nodes {
        if let Some(target) = injection_target(context, node, &mut Vec::new()) {
            if !targets.iter().any(|t| std::ptr::eq(*t, target)) {
                targets.push(target);
            }
        }
    }
    let mut names = HashSet::new();
    for target in targets {
        if injected(context, target)?.is_none() {
            continue;
        }
        let name = match kind(target) {
            "accountValueNode" | "argumentValueNode" => &target["name"],
            "accountFieldValueNode" => &target["account"],
            _ => continue,
        };
        names.insert(name.as_str().unwrap_or("").to_string());
    }
    Ok(names)
}

fn shown(
    idls: &Idls,
    program: &Key,
    accounts: &[Meta],
    data: &[u8],
    fetch: &HashMap<Key, Vec<u8>>,
) -> R<Option<Display>> {
    let Some(root) = idls.by_program.get(program).map(|i| &idls.roots[*i]) else {
        return Ok(None);
    };
    let Some((scope, instruction)) = identify(root, program, data)? else {
        return Ok(None);
    };
    let mut seen = Vec::new();
    for argument in list(instruction, "arguments") {
        validate(scope, &argument["type"], &mut seen)?;
    }
    // The arguments one after the other; what follows them is not looked at.
    let mut fields = Vec::new();
    let mut at = 0;
    for argument in list(instruction, "arguments") {
        match read(scope, &argument["type"], data, at, 0) {
            Ok((value, after)) => {
                set(&mut fields, argument["name"].as_str().unwrap_or(""), value);
                at = after;
            }
            Err(Fail::Decode(_)) => return Ok(None),
            Err(other) => return Err(other),
        }
    }
    let named = list(instruction, "accounts");
    let mut context = Context {
        scope,
        instruction,
        data: Js::Obj(fields),
        accounts: named
            .iter()
            .zip(accounts)
            .map(|(node, meta)| (node["name"].as_str().unwrap_or(""), meta.key))
            .collect(),
        remaining: accounts.get(named.len()..).unwrap_or(&[]),
        fetch,
        consumed: HashSet::new(),
    };
    context.consumed = consumed(&context)?;
    let intent = match instruction["display"]["intent"].as_str() {
        Some(intent) => intent.to_string(),
        None => title_case(instruction["name"].as_str().unwrap_or("")),
    };
    let interpolated_intent = sentence(&context)?;
    let mut fields = Vec::new();
    argument_fields(&context, &mut fields)?;
    account_fields(&context, &mut fields);
    remaining_fields(&context, &mut fields);
    Ok(Some(Display {
        intent,
        interpolated_intent,
        fields,
    }))
}

/// What the reference (`@codama/dynamic-instructions` 1.0.0) shows for an instruction.
pub fn display(
    idls: &Idls,
    program: &Key,
    accounts: &[Meta],
    data: &[u8],
    fetch: &HashMap<Key, Vec<u8>>,
) -> Outcome {
    match shown(idls, program, accounts, data, fetch) {
        Ok(Some(display)) => Outcome::Shown(display),
        Ok(None) => Outcome::Unknown,
        Err(Fail::Decode(why) | Fail::Throw(why)) => Outcome::Failed(why),
    }
}
