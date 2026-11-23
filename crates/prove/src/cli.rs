use std::collections::HashSet;

#[derive(Debug, PartialEq, Eq)]
pub enum Mode {
    Run {
        phase: u8,
        env: String,
        allow_pending: bool,
        only: HashSet<String>,
    },
    List,
    Lint,
    Help,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Options {
    pub mode: Mode,
}

pub const USAGE: &str = "\
usage: cargo run -p gridforge-prove --quiet -- <phase 0..12> [env=local] [--allow-pending] [--only ID[,ID...]] [--only ID]
       cargo run -p gridforge-prove --quiet -- --list
       cargo run -p gridforge-prove --quiet -- --lint";

pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Options, String> {
    let mut args = args.into_iter();
    let mut positionals = Vec::new();
    let mut allow_pending = false;
    let mut only = HashSet::new();
    let mut mode_flag: Option<Mode> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(Options { mode: Mode::Help }),
            "--list" => mode_flag = Some(Mode::List),
            "--lint" => mode_flag = Some(Mode::Lint),
            "--allow-pending" => allow_pending = true,
            "--only" => {
                let value = args
                    .next()
                    .ok_or_else(|| "missing value for --only".to_string())?;
                add_only(&mut only, &value)?;
            }
            _ if arg.starts_with("--only=") => {
                let value = &arg["--only=".len()..];
                add_only(&mut only, value)?;
            }
            _ if arg.starts_with('-') => return Err(format!("unknown option: {arg}")),
            _ => positionals.push(arg),
        }
    }

    if let Some(mode) = mode_flag {
        return Ok(Options { mode });
    }

    let phase = positionals
        .first()
        .ok_or_else(|| "missing phase (0..12)".to_string())?;
    let phase: u8 = phase
        .parse()
        .map_err(|_| format!("invalid phase: {phase}"))?;
    if phase > 12 {
        return Err(format!("phase out of range 0..12: {phase}"));
    }

    let env = positionals
        .get(1)
        .cloned()
        .unwrap_or_else(|| "local".to_string());

    if positionals.len() > 2 {
        return Err(format!("unexpected argument: {}", positionals[2]));
    }

    Ok(Options {
        mode: Mode::Run {
            phase,
            env,
            allow_pending,
            only,
        },
    })
}

fn add_only(only: &mut HashSet<String>, value: &str) -> Result<(), String> {
    let mut found = false;
    for id in value.split(',') {
        let id = id.trim();
        if !id.is_empty() {
            found = true;
            only.insert(id.to_string());
        }
    }
    if !found {
        return Err(format!("missing proof ID for --only: {value}"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_words(words: &[&str]) -> Result<Options, String> {
        parse(words.iter().map(|word| word.to_string()))
    }

    #[test]
    fn parses_phase_env_and_flags() {
        let options = parse_words(&[
            "0",
            "staging",
            "--allow-pending",
            "--only",
            "P0-A1,P0-S3",
            "--only",
            "P0-A2",
        ])
        .unwrap();

        let Mode::Run {
            phase,
            env,
            allow_pending,
            only,
        } = options.mode
        else {
            panic!("expected run mode");
        };

        assert_eq!(phase, 0);
        assert_eq!(env, "staging");
        assert!(allow_pending);
        assert!(only.contains("P0-A1"));
        assert!(only.contains("P0-S3"));
        assert!(only.contains("P0-A2"));
        assert_eq!(only.len(), 3);
    }

    #[test]
    fn defaults_to_local_env() {
        let options = parse_words(&["3"]).unwrap();
        let Mode::Run { phase, env, .. } = options.mode else {
            panic!("expected run mode");
        };
        assert_eq!(phase, 3);
        assert_eq!(env, "local");
    }

    #[test]
    fn rejects_out_of_range_phase() {
        assert!(parse_words(&["13"]).is_err());
    }

    #[test]
    fn rejects_missing_phase() {
        assert!(parse_words(&["--allow-pending"]).is_err());
    }

    #[test]
    fn rejects_only_without_value() {
        assert!(parse_words(&["0", "--only"]).is_err());
    }

    #[test]
    fn repeated_only_ids_are_valid_and_deduplicated() {
        let options = parse_words(&["0", "--only", "P0-A1", "--only", "P0-A1"]).unwrap();
        let Mode::Run { only, .. } = options.mode else {
            panic!("expected run mode");
        };
        assert_eq!(only.len(), 1);
        assert!(only.contains("P0-A1"));
    }

    #[test]
    fn rejects_only_with_no_ids() {
        assert!(parse_words(&["0", "--only", ",,"]).is_err());
        assert!(parse_words(&["0", "--only", "   "]).is_err());
    }

    #[test]
    fn accepts_trailing_comma() {
        let options = parse_words(&["0", "--only", "P0-A1,"]).unwrap();
        let Mode::Run { only, .. } = options.mode else {
            panic!("expected run mode");
        };
        assert_eq!(only.len(), 1);
        assert!(only.contains("P0-A1"));
    }
}
