use std::path::{Path, PathBuf};

use gridforge_prove::checks;
use gridforge_prove::cli::{self, Mode};
use gridforge_prove::model::Ctx;
use gridforge_prove::registry::{self, LintSummary};
use gridforge_prove::runner;

fn main() {
    let options = match cli::parse(std::env::args().skip(1)) {
        Ok(options) => options,
        Err(error) => {
            eprintln!("{error}");
            eprintln!();
            eprintln!("{}", cli::USAGE);
            std::process::exit(64);
        }
    };

    let code = match execute(options.mode) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("{error}");
            1
        }
    };
    std::process::exit(code);
}

fn execute(mode: Mode) -> Result<i32, String> {
    if mode == Mode::Help {
        println!("{}", cli::USAGE);
        return Ok(0);
    }

    let repo_root = repo_root();
    let registry_path = repo_root.join("prove").join("registry.tsv");
    let registry_text = std::fs::read_to_string(&registry_path)
        .map_err(|error| format!("registry not found: {} ({error})", registry_path.display()))?;
    let registry = registry::parse_registry(&registry_text)
        .map_err(|error| format!("invalid {}: {error}", registry_path.display()))?;
    let checks = checks::all_checks();

    match mode {
        Mode::Help => Ok(0),
        Mode::List => {
            runner::list(&registry, &checks);
            Ok(0)
        }
        Mode::Lint => {
            let docs_dir = repo_root.join("docs").join("phases");
            let docs_ids = docs_dir
                .is_dir()
                .then(|| registry::collect_docs_ids(&docs_dir));

            match registry::lint(&registry, &checks, docs_ids.as_deref()) {
                Ok(LintSummary {
                    registered,
                    implemented,
                }) => {
                    println!("lint ok: {registered} checks registered, {implemented} implemented");
                    Ok(0)
                }
                Err(issues) => {
                    for issue in issues {
                        println!("{issue}");
                    }
                    Ok(1)
                }
            }
        }
        Mode::Run {
            phase,
            env,
            allow_pending,
            only,
        } => {
            if let Err(error) = registry::validate_only_ids(&only, &registry, phase) {
                eprintln!("error: {error}");
                return Ok(64);
            }

            let ctx = Ctx::load(&repo_root, &env)?;
            let results_dir = repo_root.join("prove").join("results");
            runner::run(
                &ctx,
                &registry,
                &checks,
                &runner::RunOptions {
                    max_phase: phase,
                    only: &only,
                    allow_pending,
                    results_dir: &results_dir,
                    repo_root: &repo_root,
                },
            )
        }
    }
}

fn repo_root() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    std::fs::canonicalize(manifest.join("..").join(".."))
        .unwrap_or_else(|_| manifest.join("..").join(".."))
}
