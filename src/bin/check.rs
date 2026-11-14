//! Checks the renderer against every case: `cargo run --bin check [cases dir]`.

use clear_sign_cases::case::{check, load};
use std::path::PathBuf;

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let cases_dir = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("cases"));
    let idl_dir = root.join("idl");
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(&cases_dir)
        .expect("cases directory")
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| path.join("case.json").exists())
        .collect();
    dirs.sort();
    let mut failed = 0;
    for dir in &dirs {
        let name = dir
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        match load(dir, &idl_dir) {
            Ok(case) => {
                let problems = check(&case);
                if problems.is_empty() {
                    println!("PASS {}", case.id);
                } else {
                    failed += 1;
                    println!("FAIL {}", case.id);
                    for problem in problems {
                        println!("  {problem}");
                    }
                }
            }
            Err(e) => {
                failed += 1;
                println!("FAIL {name}: cannot load: {e}");
            }
        }
    }
    println!("{} cases, {} failed", dirs.len(), failed);
    if failed > 0 {
        std::process::exit(1);
    }
}
