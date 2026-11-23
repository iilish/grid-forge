use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

use crate::model::Check;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryRow {
    pub id: String,
    pub phase: u8,
    pub kind: String,
    pub description: String,
}

pub struct LintSummary {
    pub registered: usize,
    pub implemented: usize,
}

static CHECK_ID: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"P[0-9]+-[ASR][0-9]+").expect("check ID pattern is valid"));

pub fn parse_registry(contents: &str) -> Result<Vec<RegistryRow>, String> {
    let mut rows = Vec::new();

    for (line_number, line) in contents.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let fields: Vec<&str> = line.splitn(4, '\t').collect();
        if fields.len() != 4 {
            return Err(format!(
                "registry line {} has {} columns, expected 4",
                line_number + 1,
                fields.len()
            ));
        }

        let id = fields[0].trim().to_string();
        let phase: u8 = fields[1]
            .trim()
            .parse()
            .map_err(|_| format!("registry line {} has an invalid phase", line_number + 1))?;
        let kind = fields[2].trim().to_string();
        let description = fields[3].trim().to_string();

        if id.is_empty() || kind.is_empty() || description.is_empty() {
            return Err(format!(
                "registry line {} has an empty id, kind or description",
                line_number + 1
            ));
        }

        rows.push(RegistryRow {
            id,
            phase,
            kind,
            description,
        });
    }

    Ok(rows)
}

pub fn validate_only_ids(
    only: &HashSet<String>,
    registry: &[RegistryRow],
    max_phase: u8,
) -> Result<(), String> {
    let phases: HashMap<&str, u8> = registry
        .iter()
        .map(|row| (row.id.as_str(), row.phase))
        .collect();

    for id in only {
        let Some(&phase) = phases.get(id.as_str()) else {
            return Err(format!("unknown --only proof ID: {id}"));
        };
        if phase > max_phase {
            return Err(format!(
                "--only proof ID {id} is above the requested gate/phase {max_phase} (gate {phase})"
            ));
        }
    }

    Ok(())
}

pub fn collect_docs_ids(dir: &Path) -> Vec<String> {
    let mut ids = BTreeSet::new();
    walk_docs(dir, &mut ids);
    ids.into_iter().collect()
}

fn walk_docs(dir: &Path, ids: &mut BTreeSet<String>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk_docs(&path, ids);
        } else if let Ok(contents) = fs::read_to_string(&path) {
            for found in CHECK_ID.find_iter(&contents) {
                ids.insert(found.as_str().to_string());
            }
        }
    }
}

pub fn lint(
    registry: &[RegistryRow],
    checks: &[Check],
    docs_ids: Option<&[String]>,
) -> Result<LintSummary, Vec<String>> {
    let mut issues = Vec::new();

    let registered: BTreeSet<&str> = registry.iter().map(|row| row.id.as_str()).collect();
    let implemented: BTreeSet<&str> = checks.iter().map(|check| check.id).collect();

    let mut seen = HashSet::new();
    let mut duplicates = BTreeSet::new();
    for row in registry {
        if !seen.insert(row.id.as_str()) {
            duplicates.insert(row.id.as_str());
        }
    }
    if !duplicates.is_empty() {
        issues.push(section("Duplicate IDs in registry:", duplicates));
    }

    let mut seen_checks = HashSet::new();
    let mut duplicate_check_ids = BTreeSet::new();
    for check in checks {
        if !seen_checks.insert(check.id) {
            duplicate_check_ids.insert(check.id);
        }
    }
    if !duplicate_check_ids.is_empty() {
        issues.push(section(
            "Duplicate implemented check IDs:",
            duplicate_check_ids,
        ));
    }

    let unregistered: BTreeSet<&str> = implemented.difference(&registered).copied().collect();
    if !unregistered.is_empty() {
        issues.push(section("Implemented but not registered:", unregistered));
    }

    let mut names = HashSet::new();
    let mut duplicate_names = BTreeSet::new();
    for check in checks {
        if !names.insert(check.name) {
            duplicate_names.insert(check.name);
        }
    }
    if !duplicate_names.is_empty() {
        issues.push(section("Duplicate function names:", duplicate_names));
    }

    if let Some(docs_ids) = docs_ids {
        let docs: BTreeSet<&str> = docs_ids.iter().map(String::as_str).collect();

        let in_docs_only: BTreeSet<&str> = docs.difference(&registered).copied().collect();
        if !in_docs_only.is_empty() {
            issues.push(section("In docs but not in registry:", in_docs_only));
        }

        let in_registry_only: BTreeSet<&str> = registered.difference(&docs).copied().collect();
        if !in_registry_only.is_empty() {
            issues.push(section("In registry but not in docs:", in_registry_only));
        }
    }

    let missing_p0: BTreeSet<&str> = registry
        .iter()
        .filter(|row| row.phase == 0 && !implemented.contains(row.id.as_str()))
        .map(|row| row.id.as_str())
        .collect();
    if !missing_p0.is_empty() {
        issues.push(section(
            "P0 registry IDs without an implementation:",
            missing_p0,
        ));
    }

    if issues.is_empty() {
        Ok(LintSummary {
            registered: registered.len(),
            implemented: implemented.len(),
        })
    } else {
        Err(issues)
    }
}

fn section<'a>(title: &str, ids: impl IntoIterator<Item = &'a str>) -> String {
    let mut text = title.to_string();
    for id in ids {
        text.push_str("\n  ");
        text.push_str(id);
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rows_and_skips_comments_and_blanks() {
        let rows = parse_registry(
            "# id\tphase\tkind\tdescription\n\nP0-A1\t0\tacceptance\tdesc one\nP1-S2\t1\tsilent\tdesc two\n",
        )
        .unwrap();

        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].id, "P0-A1");
        assert_eq!(rows[0].phase, 0);
        assert_eq!(rows[0].kind, "acceptance");
        assert_eq!(rows[0].description, "desc one");
        assert_eq!(rows[1].id, "P1-S2");
    }

    #[test]
    fn rejects_rows_with_missing_columns() {
        assert!(parse_registry("P0-A1\t0\tacceptance\n").is_err());
    }

    fn only(ids: &[&str]) -> HashSet<String> {
        ids.iter().map(|id| id.to_string()).collect()
    }

    fn rows() -> Vec<RegistryRow> {
        parse_registry("P0-A1\t0\tacceptance\tdesc one\nP1-A1\t1\tacceptance\tdesc two\n").unwrap()
    }

    #[test]
    fn validate_only_accepts_known_ids_at_or_below_the_gate() {
        let registry = rows();
        assert_eq!(validate_only_ids(&only(&["P0-A1"]), &registry, 0), Ok(()));
        assert_eq!(
            validate_only_ids(&only(&["P0-A1", "P1-A1"]), &registry, 1),
            Ok(())
        );
    }

    #[test]
    fn validate_only_rejects_unknown_ids() {
        let error = validate_only_ids(&only(&["P9-Z9"]), &rows(), 0).unwrap_err();
        assert!(error.contains("P9-Z9"), "{error}");
        assert!(error.contains("unknown"), "{error}");
    }

    #[test]
    fn validate_only_rejects_ids_above_the_requested_gate() {
        let error = validate_only_ids(&only(&["P1-A1"]), &rows(), 0).unwrap_err();
        assert!(error.contains("P1-A1"), "{error}");
        assert!(error.contains("above"), "{error}");
        assert!(error.contains("gate/phase 0"), "{error}");
    }

    #[test]
    fn duplicate_implemented_check_ids_report_an_issue() {
        let registry = rows();
        let checks = [
            Check {
                id: "P0-A1",
                name: "first",
                run: |_ctx| Ok(crate::model::CheckOutcome::Pass),
            },
            Check {
                id: "P0-A1",
                name: "second",
                run: |_ctx| Ok(crate::model::CheckOutcome::Pass),
            },
        ];
        let Err(issues) = lint(&registry, &checks, None) else {
            panic!("expected lint to report duplicate check IDs");
        };
        assert!(
            issues
                .iter()
                .any(|issue| issue.contains("Duplicate implemented check IDs:")),
            "{issues:?}"
        );
    }
}
