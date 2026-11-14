//! A case on disk: `cases/<id>/case.json`, with the IDLs it names in `idl/`.

use crate::advise::advise;
use crate::message::{decode, Instruction};
use crate::render::{render, Context, Programs};
use crate::util::{from_b58, Key};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

pub struct Case {
    pub id: String,
    pub json: Value,
    pub instructions: Vec<Instruction>,
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
    let message = decode(&bytes)?;
    let mut tables: HashMap<Key, Vec<Key>> = HashMap::new();
    if let Some(map) = json["lookupTables"].as_object() {
        for (table, addresses) in map {
            let addresses = addresses
                .as_array()
                .ok_or("a lookup table is not a list")?
                .iter()
                .map(|a| from_b58(a.as_str().unwrap_or("")))
                .collect::<Result<_, _>>()?;
            tables.insert(from_b58(table)?, addresses);
        }
    }
    let instructions = message.resolve(&tables)?;
    let mut idls = Vec::new();
    for name in json["idls"].as_array().ok_or("case without idls")? {
        let path = idl_dir.join(name.as_str().ok_or("an idl name is not text")?);
        idls.push(
            serde_json::from_str(&read(&path)?).map_err(|e| format!("{}: {e}", path.display()))?,
        );
    }
    Ok(Case {
        id: json["id"].as_str().unwrap_or("?").to_string(),
        programs: Programs::new(idls)?,
        context: Context::from_json(&json["context"])?,
        instructions,
        json,
    })
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .map(|s| s.as_str().unwrap_or("").to_string())
                .collect()
        })
        .unwrap_or_default()
}

/// What differs between the renderer and the case's expected output; empty
/// when the case passes.
pub fn check(case: &Case) -> Vec<String> {
    let mut problems = Vec::new();
    let expected = &case.json["expected"];
    let mut compare = |what: &str, got: Vec<String>, want: Vec<String>| {
        if got != want {
            problems.push(format!(
                "{what}:\n    expected {want:?}\n    rendered {got:?}"
            ));
        }
    };
    match render(&case.programs, &case.instructions, Some(&case.context)) {
        Ok(rendered) => {
            let want = expected["interpolated"].as_str().map(str::to_string);
            compare(
                "interpolated",
                rendered.interpolated.into_iter().collect(),
                want.into_iter().collect(),
            );
            compare(
                "fallback",
                rendered.fallback,
                strings(&expected["fallback"]),
            );
        }
        Err(e) => compare("render with metadata", vec![format!("error: {e}")], vec![]),
    }
    match render(&case.programs, &case.instructions, None) {
        Ok(rendered) => compare(
            "fallback without metadata",
            rendered.fallback,
            strings(&expected["fallbackWithoutMetadata"]),
        ),
        Err(e) => compare(
            "render without metadata",
            vec![format!("error: {e}")],
            vec![],
        ),
    }
    let got = advise(&case.programs, &case.instructions, &case.context)
        .into_iter()
        .map(|a| format!("{}: {}", a.kind, a.detail))
        .collect();
    let want = expected["advisories"]
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
    compare("advisories", got, want);
    problems
}
