use std::any::Any;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;

use chrono::Utc;

use crate::model::{Check, CheckOutcome, Ctx};
use crate::registry::RegistryRow;

enum Report {
    Pass,
    Pending(String),
    Fail(String),
}

pub fn list(registry: &[RegistryRow], checks: &[Check]) {
    let names: HashMap<&str, &str> = checks.iter().map(|check| (check.id, check.name)).collect();

    println!(
        "{:<8} {:<6} {:<11} {:<40} DESCRIPTION",
        "ID", "PHASE", "KIND", "FUNCTION"
    );
    for row in registry {
        let name = names.get(row.id.as_str()).copied().unwrap_or("-");
        println!(
            "{:<8} {:<6} {:<11} {:<40} {}",
            row.id, row.phase, row.kind, name, row.description
        );
    }
}

pub struct RunOptions<'a> {
    pub max_phase: u8,
    pub only: &'a HashSet<String>,
    pub allow_pending: bool,
    pub results_dir: &'a Path,
    pub repo_root: &'a Path,
}

pub fn run(
    ctx: &Ctx,
    registry: &[RegistryRow],
    checks: &[Check],
    options: &RunOptions<'_>,
) -> Result<i32, String> {
    let max_phase = options.max_phase;
    let only = options.only;
    let allow_pending = options.allow_pending;
    let results_dir = options.results_dir;
    let repo_root = options.repo_root;
    let by_id = index_checks(checks)?;

    std::fs::create_dir_all(results_dir)
        .map_err(|error| format!("cannot create {}: {error}", results_dir.display()))?;
    let stamp = Utc::now().format("%Y%m%dT%H%M%SZ");
    let results_path = results_dir.join(format!("{stamp}-{}-p{max_phase}.tsv", ctx.env_name));
    let file = File::create(&results_path)
        .map_err(|error| format!("cannot create {}: {error}", results_path.display()))?;
    let mut results = BufWriter::new(file);
    writeln!(results, "id\tstatus\tdescription\treason")
        .map_err(|error| format!("cannot write results: {error}"))?;

    println!(
        "GridForge proofs: phases 0..{max_phase} against '{}' (BASE_URL={})",
        ctx.env_name,
        ctx.base_url()
    );

    let mut pass = 0;
    let mut fail = 0;
    let mut pending = 0;
    let mut matched = 0;
    let mut last_phase = None;

    for row in registry {
        if row.phase > max_phase {
            continue;
        }
        if !only.is_empty() && !only.contains(&row.id) {
            continue;
        }
        matched += 1;

        if last_phase != Some(row.phase) {
            println!("\nPhase {}", row.phase);
            last_phase = Some(row.phase);
        }

        let report = match by_id.get(row.id.as_str()) {
            Some(check) => run_one(check, ctx),
            None => Report::Pending("not implemented".to_string()),
        };

        let (status, reason) = match report {
            Report::Pass => {
                pass += 1;
                ("PASS", None)
            }
            Report::Pending(reason) => {
                pending += 1;
                ("PENDING", Some(reason))
            }
            Report::Fail(reason) => {
                fail += 1;
                ("FAIL", Some(reason))
            }
        };

        println!("  {:<8} {:<8} {}", status, row.id, row.description);
        for line in reason.as_deref().unwrap_or_default().lines() {
            println!("             | {line}");
        }

        writeln!(
            results,
            "{}",
            result_row(
                &row.id,
                status,
                &row.description,
                reason.as_deref().unwrap_or_default()
            )
        )
        .map_err(|error| format!("cannot write results: {error}"))?;
    }

    if !only.is_empty() && matched == 0 {
        let mut requested: Vec<&str> = only.iter().map(String::as_str).collect();
        requested.sort_unstable();
        return Err(format!(
            "no checks matched --only filter: {}",
            requested.join(",")
        ));
    }

    results
        .flush()
        .map_err(|error| format!("cannot write results: {error}"))?;

    let display_path = results_path
        .strip_prefix(repo_root)
        .unwrap_or(&results_path);
    println!(
        "\nSummary: {pass} pass, {fail} fail, {pending} pending  (results: {})",
        display_path.display()
    );

    if fail > 0 {
        Ok(1)
    } else if pending > 0 && !allow_pending {
        eprintln!("PENDING checks are not allowed without --allow-pending");
        Ok(2)
    } else {
        Ok(0)
    }
}

fn index_checks(checks: &[Check]) -> Result<HashMap<&str, &Check>, String> {
    let mut by_id = HashMap::new();
    for check in checks {
        if by_id.insert(check.id, check).is_some() {
            return Err(format!("duplicate check ID: {}", check.id));
        }
    }
    Ok(by_id)
}

fn result_row(id: &str, status: &str, description: &str, reason: &str) -> String {
    format!(
        "{}\t{}\t{}\t{}",
        sanitize(id),
        sanitize(status),
        sanitize(description),
        sanitize(reason)
    )
}

fn sanitize(value: &str) -> String {
    value
        .chars()
        .map(|ch| match ch {
            '\t' | '\n' | '\r' => ' ',
            ch => ch,
        })
        .collect()
}

fn run_one(check: &Check, ctx: &Ctx) -> Report {
    match catch_unwind(AssertUnwindSafe(|| (check.run)(ctx))) {
        Ok(Ok(CheckOutcome::Pass)) => Report::Pass,
        Ok(Ok(CheckOutcome::Pending(reason))) => Report::Pending(reason),
        Ok(Err(error)) => Report::Fail(error.to_string()),
        Err(payload) => Report::Fail(format!("panic: {}", panic_message(payload.as_ref()))),
    }
}

fn panic_message(payload: &(dyn Any + Send)) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_string()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "non-string panic payload".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::CheckError;

    fn pass(_ctx: &Ctx) -> Result<CheckOutcome, CheckError> {
        Ok(CheckOutcome::Pass)
    }

    fn pending(_ctx: &Ctx) -> Result<CheckOutcome, CheckError> {
        Ok(CheckOutcome::Pending("waiting".to_string()))
    }

    fn fail(_ctx: &Ctx) -> Result<CheckOutcome, CheckError> {
        Err(CheckError::new("broken"))
    }

    fn panics(_ctx: &Ctx) -> Result<CheckOutcome, CheckError> {
        panic!("boom")
    }

    fn ctx() -> Ctx {
        Ctx::test()
    }

    fn check(
        id: &'static str,
        name: &'static str,
        run: fn(&Ctx) -> Result<CheckOutcome, CheckError>,
    ) -> Check {
        Check { id, name, run }
    }

    #[test]
    fn maps_outcomes_and_panics() {
        let ctx = ctx();
        assert!(matches!(
            run_one(&check("X-1", "pass", pass), &ctx),
            Report::Pass
        ));
        assert!(matches!(
            run_one(&check("X-1", "pending", pending), &ctx),
            Report::Pending(_)
        ));
        assert!(matches!(
            run_one(&check("X-1", "fail", fail), &ctx),
            Report::Fail(_)
        ));
        assert!(matches!(
            run_one(&check("X-1", "panic", panics), &ctx),
            Report::Fail(_)
        ));
    }

    #[test]
    fn result_row_has_four_columns_and_sanitizes_every_field() {
        let row = result_row("P0-A1", "FAIL", "desc\twith\nnewlines", "bad\rreason");
        let fields: Vec<&str> = row.split('\t').collect();
        assert_eq!(fields.len(), 4);
        assert_eq!(fields[0], "P0-A1");
        assert_eq!(fields[1], "FAIL");
        assert!(!row.contains('\n') && !row.contains('\r'));
        assert_eq!(fields[2], "desc with newlines");
        assert_eq!(fields[3], "bad reason");
    }

    #[test]
    fn result_row_leaves_pass_reason_empty() {
        let row = result_row("P0-A1", "PASS", "desc", "");
        assert_eq!(row, "P0-A1\tPASS\tdesc\t");
    }

    fn run_options<'a>(only: &'a HashSet<String>, results_dir: &'a Path) -> RunOptions<'a> {
        RunOptions {
            max_phase: 0,
            only,
            allow_pending: true,
            results_dir,
            repo_root: Path::new("."),
        }
    }

    #[test]
    fn duplicate_check_ids_fail_the_run() {
        let checks = [check("X-1", "first", pass), check("X-1", "second", pass)];
        let only: HashSet<String> = HashSet::new();
        let results_dir = Path::new("target/prove-runner-duplicate-test");
        let options = run_options(&only, results_dir);
        let error = run(&ctx(), &[], &checks, &options).unwrap_err();
        assert!(error.contains("duplicate check ID: X-1"), "{error}");
    }

    #[test]
    fn run_errors_when_only_matches_nothing() {
        let registry = [RegistryRow {
            id: "P0-A1".to_string(),
            phase: 0,
            kind: "acceptance".to_string(),
            description: "desc".to_string(),
        }];
        let only: HashSet<String> = ["P9-Z9".to_string()].into_iter().collect();
        let results_dir =
            std::env::temp_dir().join(format!("gridforge-prove-no-match-{}", std::process::id()));
        let options = run_options(&only, &results_dir);
        let error = run(&ctx(), &registry, &[], &options).unwrap_err();
        assert!(error.contains("no checks matched"), "{error}");
        let _ = std::fs::remove_dir_all(&results_dir);
    }
}
