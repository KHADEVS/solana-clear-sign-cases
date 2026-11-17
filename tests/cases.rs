//! Every case in `cases/` decodes, and its advisories match.

use clear_sign_cases::case::{check, load};
use std::path::PathBuf;

#[test]
fn every_case_passes() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut count = 0;
    for entry in std::fs::read_dir(root.join("cases")).expect("cases directory") {
        let dir = entry.expect("entry").path();
        if !dir.join("case.json").exists() {
            continue;
        }
        let case =
            load(&dir, &root.join("idl")).unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
        let problems = check(&case);
        assert!(problems.is_empty(), "{}:\n{}", case.id, problems.join("\n"));
        count += 1;
    }
    assert!(count >= 5, "only {count} cases found");
}
