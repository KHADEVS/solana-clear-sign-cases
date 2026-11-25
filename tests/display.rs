//! The Rust display must give, for every instruction of every case, what
//! Codama's own renderer gave when the case was generated.

use clear_sign_cases::case::load;
use clear_sign_cases::display::{display, Outcome};
use serde_json::{json, Value};
use std::path::PathBuf;

/// The outcome in the shape the cases store.
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

#[test]
fn agrees_with_the_stable_release_on_every_case() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(root.join("cases"))
        .expect("cases directory")
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| path.join("case.json").exists())
        .collect();
    dirs.sort();
    let (mut compared, mut differences) = (0, Vec::new());
    for dir in &dirs {
        let case = load(dir, &root.join("idl")).expect("the case loads");
        let expected = case.json["expected"]["display"]["instructions"]
            .as_array()
            .expect("expected displays");
        assert_eq!(expected.len(), case.instructions.len(), "{}", case.id);
        for (i, ix) in case.instructions.iter().enumerate() {
            let outcome = display(
                &case.idls,
                &ix.program,
                &case.metas[i],
                &ix.data,
                &case.context.accounts,
            );
            compared += 1;
            let want = &expected[i]["display"];
            // An error on one side must be an error on the other; its wording is not compared.
            let failed = matches!(outcome, Outcome::Failed(_));
            let want_failed = expected[i].get("error").is_some();
            if as_json(&outcome) != *want || failed != want_failed {
                differences.push(format!(
                    "{} instruction {i}\n  codama: {}{}\n  rust:   {}{}",
                    case.id,
                    want,
                    if want_failed { " (error)" } else { "" },
                    as_json(&outcome),
                    match &outcome {
                        Outcome::Failed(why) => format!(" (error: {why})"),
                        _ => String::new(),
                    }
                ));
            }
        }
    }
    assert!(
        differences.is_empty(),
        "{} of {compared} instructions differ:\n{}",
        differences.len(),
        differences.join("\n")
    );
    assert!(compared >= 50, "only {compared} instructions compared");
}
