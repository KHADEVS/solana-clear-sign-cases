//! Runs the Rust display on the scenarios written by `tools/codama/peers.mjs`
//! from another implementation's conformance fixtures, and prints where it
//! differs from Codama's stable release on them. It only reports: the file is
//! not part of the repository, so the test passes when it is absent.
//!
//!   PEER_SCENARIOS=<out.json> cargo test --test peers -- --nocapture

use clear_sign_cases::display::{display, Idls, Meta, Outcome};
use clear_sign_cases::util::{from_b58, Key};
use serde_json::{json, Value};
use std::collections::HashMap;

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

fn shown(scenario: &Value) -> Outcome {
    use base64::Engine;
    let idls = match Idls::new(vec![scenario["idl"].clone()]) {
        Ok(idls) => idls,
        Err(why) => return Outcome::Failed(why),
    };
    let key = |text: &Value| -> Key { from_b58(text.as_str().unwrap_or("")).expect("an address") };
    let metas: Vec<Meta> = scenario["accounts"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|a| Meta {
            key: key(&a[0]),
            signer: a[1] == true,
        })
        .collect();
    let mut fetch: HashMap<Key, Vec<u8>> = HashMap::new();
    for (address, entry) in scenario["fetch"].as_object().into_iter().flatten() {
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
        &key(&scenario["program"]),
        &metas,
        &from_hex(scenario["data"].as_str().unwrap_or("")),
        &fetch,
    )
}

#[test]
fn report_on_another_implementation_s_scenarios() {
    let Ok(path) = std::env::var("PEER_SCENARIOS") else {
        return;
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let scenarios: Value = serde_json::from_str(&text).unwrap_or_else(|e| panic!("{path}: {e}"));
    let scenarios = scenarios.as_array().expect("a list of scenarios");
    let (mut same, mut same_online, mut online, mut different) = (0, 0, 0, Vec::new());
    let mut as_recorded = 0;
    for scenario in scenarios {
        let name = scenario["name"].as_str().expect("a name");
        let outcome = shown(scenario);
        let (got, failed) = as_json(&outcome);
        // What the fixture itself recorded, with the Codama release it is pinned to.
        let recorded = &scenario["recorded"];
        let recorded_failed = recorded.get("error").is_some() && recorded.get("fields").is_none();
        as_recorded += usize::from(if recorded_failed {
            failed
        } else {
            !failed && got == *recorded
        });
        let want = &scenario["stable"]["display"];
        let want_failed = scenario["stable"].get("error").is_some();
        let agrees = got == *want && failed == want_failed;
        let is_online = scenario["offline"] != true;
        online += usize::from(is_online);
        if agrees {
            same += 1;
            same_online += usize::from(is_online);
        } else {
            different.push(format!(
                "{name}{}\n  codama 1.0.0: {want}{}\n  rust:         {got}{}",
                if is_online {
                    ""
                } else {
                    "  (no account data given)"
                },
                scenario["stable"]["error"]
                    .as_str()
                    .map(|e| format!(" (error: {e})"))
                    .unwrap_or_default(),
                match &outcome {
                    Outcome::Failed(why) => format!(" (error: {why})"),
                    _ => String::new(),
                }
            ));
        }
    }
    println!(
        "{} scenarios: the Rust display gives what Codama 1.0.0 gives on {same} ({same_online} of the {online} with account data), and what the fixtures recorded on {as_recorded}",
        scenarios.len()
    );
    for d in &different {
        println!("\n{d}");
    }
}
