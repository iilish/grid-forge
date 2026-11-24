use std::fs;

const FORBIDDEN_IN_ADAPTERS: &[&str] = &["sea_orm::", "sqlx::", "gridforge_store::"];
const FORBIDDEN_IN_APP: &[&str] = &["axum::", "tower::", "sea_orm::", "sqlx::"];
const FORBIDDEN_IN_STORE: &[&str] = &["anyhow::"];
const FORBIDDEN_IN_CONFIG: &[&str] = &["anyhow::"];

fn main() {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("check-layering") => check_layering(),
        _ => {
            eprintln!("usage: cargo xtask check-layering");
            std::process::exit(64);
        }
    }
}

fn check_layering() {
    let mut violations = 0;

    violations += scan_files(
        &[
            "src/http.rs",
            "src/main.rs",
            "src/self_test.rs",
            "tests/walking_skeleton.rs",
        ],
        FORBIDDEN_IN_ADAPTERS,
        "adapter",
    );
    violations += scan_files(
        &[
            "crates/app/src/error.rs",
            "crates/app/src/health.rs",
            "crates/app/src/ingest.rs",
            "crates/app/src/lib.rs",
            "crates/app/src/metrics.rs",
            "crates/app/src/port.rs",
            "crates/app/src/state.rs",
            "crates/app/src/worker.rs",
        ],
        FORBIDDEN_IN_APP,
        "app",
    );
    violations += scan_files(
        &[
            "crates/store/src/db.rs",
            "crates/store/src/error.rs",
            "crates/store/src/events.rs",
            "crates/store/src/lib.rs",
            "crates/store/src/migrations.rs",
        ],
        FORBIDDEN_IN_STORE,
        "store",
    );
    violations += scan_files(&["src/config.rs"], FORBIDDEN_IN_CONFIG, "config");

    if violations == 0 {
        println!("layering: no violations");
    } else {
        println!("layering: {violations} violation(s)");
        std::process::exit(1);
    }
}

fn scan_files(paths: &[&str], forbidden: &[&str], layer: &str) -> usize {
    let mut count = 0;
    for path in paths {
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) => {
                println!("layering: cannot read {path}: {error}");
                count += 1;
                continue;
            }
        };

        for (index, line) in text.lines().enumerate() {
            for pattern in forbidden {
                if line.contains(pattern) {
                    println!("{path}:{}: {layer} must not use {pattern}", index + 1);
                    count += 1;
                }
            }
        }
    }

    count
}
