//! A case on disk: `cases/<id>/case.json`, with the IDLs it names under `idl/`.
//!
//! The expected displays in a case come from Codama's own instruction display
//! (`tools/codama`); this crate checks the rest: that the message decodes, and
//! the advisories, which are ours and not normative.

use crate::advise::advise;
use crate::decode::{decode, Context, Programs};
use crate::message::{decode as decode_message, Instruction};
use crate::util::{from_b58, Key};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

pub struct Case {
    pub id: String,
    pub json: Value,
    pub instructions: Vec<Instruction>,
    /// The required signers, fee payer first.
    pub signers: Vec<Key>,
    pub programs: Programs,
    pub context: Context,
}

pub fn load(dir: &Path, idl_dir: &Path) -> Result<Case, String> {
    use base64::Engine;
    let read =
        |path: &Path| std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()));
    let json: Value =
        serde_json::from_str(&read(&dir.join("case.json"))?).map_err(|e| e.to_string())?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(json["message"].as_str().ok_or("case without message")?)
        .map_err(|e| format!("message: {e}"))?;
    let message = decode_message(&bytes)?;
    let mut tables: HashMap<Key, Vec<Key>> = HashMap::new();
    if let Some(map) = json["lookupTables"].as_object() {
        for (table, addresses) in map {
            let addresses = addresses
                .as_array()
                .ok_or("a lookup table is not a list")?
                .iter()
                .map(|a| from_b58(a.as_str().unwrap_or("")))
                .collect::<Result<Vec<Key>, String>>()?;
            tables.insert(from_b58(table)?, addresses);
        }
    }
    let instructions = message.resolve(&tables)?;
    let signers = message.keys[..message.required_signatures as usize].to_vec();
    let mut idls = Vec::new();
    for name in json["idls"].as_array().ok_or("case without idls")? {
        let path = idl_dir.join(name.as_str().ok_or("an idl name is not text")?);
        let idl: Value =
            serde_json::from_str(&read(&path)?).map_err(|e| format!("{}: {e}", path.display()))?;
        idls.push(idl);
    }
    Ok(Case {
        id: json["id"].as_str().unwrap_or("?").to_string(),
        programs: Programs::new(idls)?,
        context: Context::from_json(&json["context"])?,
        instructions,
        signers,
        json,
    })
}

/// What differs from the case's expectations; empty when the case passes.
pub fn check(case: &Case) -> Vec<String> {
    let mut problems = Vec::new();
    let displays = case.json["expected"]["display"]["instructions"].as_array();
    match displays {
        Some(displays) if displays.len() == case.instructions.len() => {}
        _ => problems.push(
            "expected.display does not list every instruction (run tools/codama/generate.mjs)"
                .into(),
        ),
    }
    for (i, ix) in case.instructions.iter().enumerate() {
        if let Err(e) = decode(&case.programs, ix) {
            problems.push(format!("instruction {i} does not decode: {e}"));
        }
    }
    let got: Vec<String> = advise(
        &case.programs,
        &case.signers,
        &case.instructions,
        &case.context,
    )
    .into_iter()
    .map(|a| format!("{}: {}", a.kind, a.detail))
    .collect();
    let want: Vec<String> = case.json["expected"]["advisories"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .map(|a| {
                    format!(
                        "{}: {}",
                        a["kind"].as_str().unwrap_or(""),
                        a["detail"].as_str().unwrap_or("")
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    if got != want {
        problems.push(format!(
            "advisories:\n    expected {want:?}\n    computed {got:?}"
        ));
    }
    problems
}
