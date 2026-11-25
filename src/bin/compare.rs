//! Renders the instructions of a survey (`tools/codama/survey.mjs`) with the Rust
//! display, and reports every difference with what Codama gave for them.
//!
//! `cargo run --release --bin compare [survey dir]`

use clear_sign_cases::display::{display, Idls, Meta, Outcome};
use clear_sign_cases::util::{from_b58, Key};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

fn as_json(outcome: &Outcome) -> Value {
    match outcome {
        Outcome::Shown(d) => json!({
            "fields": d.fields.iter().map(|f| json!({ "label": f.label, "value": f.value })).collect::<Vec<_>>(),
            "intent": d.intent,
            "interpolatedIntent": d.interpolated_intent,
        }),
        _ => Value::Null,
    }
}

/// The first thing that differs between two displays, in a line.
fn first_difference(want: &Value, got: &Value) -> String {
    if want.is_null() != got.is_null() {
        return format!(
            "codama {}, rust {}",
            if want.is_null() {
                "shows nothing"
            } else {
                "shows it"
            },
            if got.is_null() {
                "shows nothing"
            } else {
                "shows it"
            }
        );
    }
    for key in ["intent", "interpolatedIntent"] {
        if want[key] != got[key] {
            return format!("{key}: codama {} | rust {}", want[key], got[key]);
        }
    }
    let (a, b) = (want["fields"].as_array(), got["fields"].as_array());
    let (a, b) = (
        a.map(Vec::as_slice).unwrap_or(&[]),
        b.map(Vec::as_slice).unwrap_or(&[]),
    );
    for i in 0..a.len().max(b.len()) {
        let (x, y) = (
            a.get(i).unwrap_or(&Value::Null),
            b.get(i).unwrap_or(&Value::Null),
        );
        if x != y {
            return format!("field {i}: codama {x} | rust {y}");
        }
    }
    "an error on one side only".into()
}

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let args: Vec<String> = std::env::args().skip(1).collect();
    let key = "stable";
    let dir = args
        .iter()
        .find(|a| !a.starts_with("--"))
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("real/survey"));
    let read = |name: &str| {
        std::fs::read_to_string(dir.join(name))
            .unwrap_or_else(|e| panic!("{}: {e}", dir.join(name).display()))
    };
    let summary: Value = serde_json::from_str(&read("summary.json")).expect("summary.json");
    let mut names: HashMap<String, String> = HashMap::new();
    let idls: Vec<Value> = summary["idls"]
        .as_array()
        .expect("the list of IDLs")
        .iter()
        .map(|name| {
            let path = root.join("idl").join(name.as_str().expect("a name"));
            let idl: Value = serde_json::from_str(&std::fs::read_to_string(&path).expect("an IDL"))
                .expect("JSON");
            names.insert(
                idl["program"]["publicKey"]
                    .as_str()
                    .unwrap_or("")
                    .to_string(),
                idl["program"]["name"].as_str().unwrap_or("?").to_string(),
            );
            idl
        })
        .collect();
    let idls = Idls::new(idls).expect("the IDLs load");
    let accounts: Value = serde_json::from_str(&read("accounts.json")).expect("accounts.json");
    let mut fetch: HashMap<Key, Vec<u8>> = HashMap::new();
    for (address, entry) in accounts.as_object().into_iter().flatten() {
        if let Some(data) = entry["data"].as_str() {
            use base64::Engine;
            fetch.insert(
                from_b58(address).expect("an address"),
                base64::engine::general_purpose::STANDARD
                    .decode(data)
                    .expect("base64"),
            );
        }
    }

    let (mut total, mut same) = (0usize, 0usize);
    // program / instruction -> (compared, different, examples)
    let mut by_kind: BTreeMap<String, (usize, usize, Vec<String>)> = BTreeMap::new();
    for line in read("instructions.jsonl").lines().filter(|l| !l.is_empty()) {
        let record: Value = serde_json::from_str(line).expect("a record");
        let program = record["program"].as_str().unwrap_or("");
        let metas: Vec<Meta> = record["accounts"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|a| Meta {
                key: from_b58(a[0].as_str().unwrap_or("")).expect("an address"),
                signer: a[1] == true,
            })
            .collect();
        use base64::Engine;
        let data = base64::engine::general_purpose::STANDARD
            .decode(record["data"].as_str().unwrap_or(""))
            .expect("base64");
        let outcome = display(
            &idls,
            &from_b58(program).expect("an address"),
            &metas,
            &data,
            &fetch,
        );
        let want = &record[key];
        let got = as_json(&outcome);
        let agree = got == want["display"]
            && matches!(outcome, Outcome::Failed(_)) == want.get("error").is_some();
        let kind = format!(
            "{} / {}",
            names.get(program).map(String::as_str).unwrap_or("?"),
            want["display"]["intent"]
                .as_str()
                .unwrap_or(if want.get("error").is_some() {
                    "(codama throws)"
                } else {
                    "(codama shows nothing)"
                })
        );
        let entry = by_kind.entry(kind).or_default();
        total += 1;
        entry.0 += 1;
        if agree {
            same += 1;
            continue;
        }
        entry.1 += 1;
        if entry.2.len() < 2 {
            let mut detail = first_difference(&want["display"], &got);
            if let Outcome::Failed(why) = &outcome {
                detail.push_str(&format!(" (rust: {why})"));
            }
            if let Some(error) = want["error"].as_str() {
                detail.push_str(&format!(" (codama: {error})"));
            }
            entry.2.push(format!(
                "slot {} tx {}: {detail}",
                record["slot"], record["tx"]
            ));
        }
    }
    println!("{key}: {same} of {total} instructions agree");
    println!(
        "\n{:<58}{:>9}{:>10}",
        "program / instruction", "compared", "different"
    );
    for (kind, (compared, different, examples)) in &by_kind {
        println!("{kind:<58}{compared:>9}{different:>10}");
        for example in examples {
            println!("    {}", example.chars().take(420).collect::<String>());
        }
    }
    if same != total {
        std::process::exit(1);
    }
}
