//! The probes of `tools/codama/probes.mjs`: small IDLs written to reach what
//! the real programs do not. The Rust display must give what Codama gave for
//! each of them, except where `tests/probe-differences.json` says why not.

use clear_sign_cases::display::{display, Idls, Meta, Outcome};
use clear_sign_cases::util::{from_b58, Key};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;

/// The outcome in the shape the probes store: a display or nothing, and whether it is an error.
fn as_json(outcome: &Outcome) -> (Value, bool) {
    match outcome {
        Outcome::Shown(d) => (
            json!({
                "fields": d.fields.iter().map(|f| json!({ "label": f.label, "value": f.value })).collect::<Vec<_>>(),
                "intent": d.intent,
                "interpolatedIntent": d.interpolated_intent,
            }),
            false,
        ),
        Outcome::Unknown => (Value::Null, false),
        Outcome::Failed(_) => (Value::Null, true),
    }
}

fn from_hex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).expect("hex"))
        .collect()
}

fn read(name: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join(name);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn shown(probe: &Value) -> Outcome {
    use base64::Engine;
    let idls = match Idls::new(vec![probe["idl"].clone()]) {
        Ok(idls) => idls,
        Err(why) => return Outcome::Failed(why),
    };
    let key = |text: &Value| -> Key { from_b58(text.as_str().unwrap_or("")).expect("an address") };
    let metas: Vec<Meta> = probe["accounts"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|a| Meta {
            key: key(&a[0]),
            signer: a[1] == true,
        })
        .collect();
    let mut fetch: HashMap<Key, Vec<u8>> = HashMap::new();
    for (address, entry) in probe["fetch"].as_object().into_iter().flatten() {
        // The data alone, or the data with the program that owns the account.
        let data = entry.as_str().or(entry["data"].as_str()).unwrap_or("");
        fetch.insert(
            from_b58(address).expect("an address"),
            base64::engine::general_purpose::STANDARD
                .decode(data)
                .expect("base64"),
        );
    }
    display(
        &idls,
        &key(&probe["program"]),
        &metas,
        &from_hex(probe["data"].as_str().unwrap_or("")),
        &fetch,
    )
}

#[test]
fn agrees_with_the_stable_release_on_every_probe() {
    let probes = read("probes.json");
    let probes = probes.as_array().expect("a list of probes");
    let differences = read("probe-differences.json");
    let differences = differences.as_object().expect("differences by probe name");
    let mut problems = Vec::new();
    let mut used = 0;
    for probe in probes {
        let name = probe["name"].as_str().expect("a name");
        let outcome = shown(probe);
        let (got, failed) = as_json(&outcome);
        let want = &probe["stable"]["display"];
        let want_failed = probe["stable"].get("error").is_some();
        let same = got == *want && failed == want_failed;
        let detail = || {
            format!(
                "{name}\n  codama: {want}{}\n  rust:   {got}{}",
                match probe["stable"]["error"].as_str() {
                    Some(error) => format!(" (error: {error})"),
                    None => String::new(),
                },
                match &outcome {
                    Outcome::Failed(why) => format!(" (error: {why})"),
                    _ => String::new(),
                }
            )
        };
        match differences.get(name) {
            None if same => {}
            None => problems.push(detail()),
            Some(_) if same => problems.push(format!(
                "{name}: listed as a difference, but the two now agree"
            )),
            Some(entry) => {
                used += 1;
                if entry["rust"] != got || failed {
                    problems.push(format!("a listed difference that changed: {}", detail()));
                }
                // Where the file says the release candidate already gives what Rust gives.
                if entry["asTheReleaseCandidate"] == true && probe["next"]["display"] != got {
                    problems.push(format!(
                        "{name}: said to be as the release candidate, which gives {}",
                        probe["next"]["display"]
                    ));
                }
                assert!(
                    entry["why"].as_str().is_some_and(|why| !why.is_empty()),
                    "{name}: a difference needs its reason"
                );
            }
        }
    }
    for name in differences.keys() {
        assert!(
            probes.iter().any(|p| p["name"] == name.as_str()),
            "{name}: a difference for a probe that does not exist"
        );
    }
    assert!(
        problems.is_empty(),
        "{} of {} probes:\n{}",
        problems.len(),
        probes.len(),
        problems.join("\n")
    );
    assert!(probes.len() >= 200, "only {} probes", probes.len());
    assert_eq!(used, differences.len());
}
