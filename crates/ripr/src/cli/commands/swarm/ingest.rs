use crate::cli::commands_context::ensure_command_root;
use crate::cli::parse::expect_value;
use crate::cli::suggest::{unknown_argument, unknown_value};
use crate::output;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Options {
    pub(super) root: PathBuf,
    pub(super) result: PathBuf,
}

pub(super) fn parse_options(args: &[String]) -> Result<Options, String> {
    let mut root = PathBuf::from(".");
    let mut result = None;

    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--root" => {
                i += 1;
                let value = expect_value(args, i, "--root")?;
                if value.trim().is_empty() {
                    return Err("swarm ingest --root requires a non-empty path".to_string());
                }
                root = PathBuf::from(value);
            }
            "--result" => {
                i += 1;
                let value = expect_value(args, i, "--result")?;
                if value.trim().is_empty() {
                    return Err("swarm ingest --result requires a non-empty path".to_string());
                }
                result = Some(PathBuf::from(value));
            }
            "--format" => {
                i += 1;
                let value = expect_value(args, i, "--format")?;
                if value != "json" {
                    return Err(unknown_value("swarm ingest format", value, &["json"]));
                }
            }
            "--json" => {}
            other => return Err(unknown_argument("swarm ingest", other)),
        }
        i += 1;
    }

    Ok(Options {
        root,
        result: result.ok_or_else(|| "swarm ingest requires --result <path>".to_string())?,
    })
}

pub(super) fn run(options: Options) -> Result<(), String> {
    ensure_command_root(&options.root, "swarm ingest")?;
    let root = options.root.canonicalize().map_err(|err| {
        format!(
            "canonicalize swarm ingest root {} failed: {err}",
            options.root.display()
        )
    })?;
    let result_path = validate_result_path(&root, &options.result)?;
    let contents = crate::bounded_input::read_to_string(&result_path).map_err(|err| {
        format!(
            "read swarm ingest --result {} failed: {err}",
            options.result.display()
        )
    })?;
    let rendered = output::swarm_ingest::render_swarm_ingest_json(
        &contents,
        &output::outcome::display_path(&options.result),
        &root,
    )?;
    print!("{rendered}");
    Ok(())
}

/// `root` must already be canonicalized: the forbidden-edit path comparison
/// anchors absolute edited-path spellings against it (#5984).
fn validate_result_path(root: &Path, path: &Path) -> Result<PathBuf, String> {
    let candidate = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    let candidate = candidate.canonicalize().map_err(|err| {
        format!(
            "canonicalize swarm ingest --result {} failed: {err}",
            path.display()
        )
    })?;

    if !candidate.starts_with(root) {
        return Err(format!(
            "swarm ingest --result {} must stay under root {}",
            crate::output::path::human_path(path),
            crate::output::path::human_path(root)
        ));
    }

    Ok(candidate)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn parses_result_and_format() {
        assert_eq!(
            parse_options(&args(&[
                "--root",
                ".",
                "--result",
                "target/ripr/workflow/agent-result.json",
                "--format",
                "json",
            ])),
            Ok(Options {
                root: PathBuf::from("."),
                result: PathBuf::from("target/ripr/workflow/agent-result.json"),
            })
        );
        assert_eq!(
            parse_options(&args(&["--format", "md"])),
            Err("unknown swarm ingest format \"md\". Accepted: json.".to_string())
        );
        assert_eq!(
            parse_options(&args(&["--format", "jsn"])),
            Err(
                "unknown swarm ingest format \"jsn\". Did you mean `json`? Accepted: json."
                    .to_string()
            )
        );
        assert_eq!(
            parse_options(&args(&[])),
            Err("swarm ingest requires --result <path>".to_string())
        );
    }

    fn parse(values: &[&str]) -> Result<Options, String> {
        parse_options(&args(values))
    }

    #[test]
    fn defaults_root_to_current_directory_and_accepts_json_spellings() -> Result<(), String> {
        let expected = Options {
            root: PathBuf::from("."),
            result: PathBuf::from("r.json"),
        };
        assert_eq!(parse(&["--result", "r.json"])?, expected);
        assert_eq!(parse(&["--result", "r.json", "--json"])?, expected);
        assert_eq!(
            parse(&["--result", "r.json", "--format", "json"])?,
            expected
        );
        // Positive control: a custom root is kept.
        assert_eq!(
            parse(&["--root", "ws", "--result", "r.json"])?.root,
            PathBuf::from("ws")
        );
        Ok(())
    }

    #[test]
    fn later_flag_values_replace_earlier_ones() -> Result<(), String> {
        let options = parse(&[
            "--result", "a.json", "--root", "one", "--result", "b.json", "--root", "two",
        ])?;
        assert_eq!(options.root, PathBuf::from("two"));
        assert_eq!(options.result, PathBuf::from("b.json"));
        Ok(())
    }

    #[test]
    fn flags_without_a_value_fail_closed() {
        assert_eq!(
            parse(&["--result", "r.json"]).map(|o| o.result),
            Ok(PathBuf::from("r.json"))
        );
        assert_eq!(
            parse(&["--result"]),
            Err("missing value for --result".to_string())
        );
        assert_eq!(
            parse(&["--result", "r.json", "--root"]),
            Err("missing value for --root".to_string())
        );
        assert_eq!(
            parse(&["--result", "r.json", "--format"]),
            Err("missing value for --format".to_string())
        );
    }

    #[test]
    fn blank_root_and_result_values_are_rejected() {
        assert_eq!(
            parse(&["--root", "ws", "--result", "r.json"]).map(|o| o.root),
            Ok(PathBuf::from("ws"))
        );
        assert_eq!(
            parse(&["--root", "  ", "--result", "r.json"]),
            Err("swarm ingest --root requires a non-empty path".to_string())
        );
        assert_eq!(
            parse(&["--root", ""]),
            Err("swarm ingest --root requires a non-empty path".to_string())
        );
        assert_eq!(
            parse(&["--result", " \t"]),
            Err("swarm ingest --result requires a non-empty path".to_string())
        );
    }

    #[test]
    fn unknown_arguments_are_rejected_with_a_help_pointer() -> Result<(), String> {
        assert_eq!(
            parse(&["--result", "r.json", "--zzz"]),
            Err(
                "unknown swarm ingest argument \"--zzz\". Run `ripr swarm ingest --help`."
                    .to_string()
            )
        );
        // A positional is not a silent alias for --result.
        let Err(message) = parse(&["r.json"]) else {
            return Err("a positional result must not parse".to_string());
        };
        assert!(
            message.starts_with("unknown swarm ingest argument \"r.json\"."),
            "{message}"
        );
        Ok(())
    }

    /// Unique per-test directory. `create_dir_all` makes the parents first
    /// because `TMPDIR` points under `target/`, where they may not exist yet.
    fn unique_dir(name: &str) -> Result<PathBuf, String> {
        let dir =
            std::env::temp_dir().join(format!("ripr-swarm-ingest-{name}-{}", std::process::id()));
        if dir.exists() {
            std::fs::remove_dir_all(&dir)
                .map_err(|err| format!("clear {}: {err}", dir.display()))?;
        }
        std::fs::create_dir_all(&dir).map_err(|err| format!("create {}: {err}", dir.display()))?;
        dir.canonicalize()
            .map_err(|err| format!("canonicalize {}: {err}", dir.display()))
    }

    fn options(root: &Path, result: &Path) -> Options {
        Options {
            root: root.to_path_buf(),
            result: result.to_path_buf(),
        }
    }

    #[test]
    fn validate_result_path_accepts_relative_and_absolute_paths_under_root() -> Result<(), String> {
        let root = unique_dir("validate-ok")?;
        std::fs::write(root.join("result.json"), "{}").map_err(|err| err.to_string())?;
        let expected = root.join("result.json");
        assert_eq!(
            validate_result_path(&root, Path::new("result.json"))?,
            expected
        );
        assert_eq!(validate_result_path(&root, &expected)?, expected);
        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn validate_result_path_refuses_missing_and_escaping_paths() -> Result<(), String> {
        let parent = unique_dir("validate-escape")?;
        let root = parent.join("root");
        std::fs::create_dir_all(&root).map_err(|err| err.to_string())?;
        std::fs::write(root.join("inside.json"), "{}").map_err(|err| err.to_string())?;
        std::fs::write(parent.join("outside.json"), "{}").map_err(|err| err.to_string())?;
        let root = root.canonicalize().map_err(|err| err.to_string())?;
        let outside = parent.join("outside.json");

        // Positive control: the outside file exists and an inside file
        // validates, so only the root boundary differs below.
        assert!(outside.is_file());
        assert_eq!(
            validate_result_path(&root, Path::new("inside.json")),
            Ok(root.join("inside.json"))
        );

        let Err(message) = validate_result_path(&root, &outside) else {
            return Err("an absolute path outside the root must be refused".to_string());
        };
        assert_eq!(
            message,
            format!(
                "swarm ingest --result {} must stay under root {}",
                crate::output::path::human_path(&outside),
                crate::output::path::human_path(&root)
            )
        );
        let Err(message) = validate_result_path(&root, Path::new("../outside.json")) else {
            return Err("a `..` climb out of the root must be refused".to_string());
        };
        assert!(message.contains("must stay under root"), "{message}");

        let Err(message) = validate_result_path(&root, Path::new("absent.json")) else {
            return Err("a missing result must be refused".to_string());
        };
        assert!(
            message.starts_with("canonicalize swarm ingest --result absent.json failed: "),
            "{message}"
        );
        let _ = std::fs::remove_dir_all(&parent);
        Ok(())
    }

    #[test]
    fn run_refuses_a_root_that_is_not_a_directory() -> Result<(), String> {
        let root = unique_dir("run-bad-root")?;
        let file = root.join("not-a-dir.txt");
        std::fs::write(&file, "x").map_err(|err| err.to_string())?;
        std::fs::write(root.join("result.json"), "{}").map_err(|err| err.to_string())?;
        // Positive control: the same result under the real root runs.
        assert_eq!(run(options(&root, Path::new("result.json"))), Ok(()));
        let Err(message) = run(options(&file, Path::new("result.json"))) else {
            return Err("a file root must be refused".to_string());
        };
        assert!(
            message.starts_with("swarm ingest root ") && message.contains("is not a directory"),
            "{message}"
        );
        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn run_reports_a_missing_or_escaping_result_and_malformed_json() -> Result<(), String> {
        let parent = unique_dir("run-failures")?;
        let root = parent.join("root");
        std::fs::create_dir_all(&root).map_err(|err| err.to_string())?;
        std::fs::write(root.join("good.json"), "{}").map_err(|err| err.to_string())?;
        std::fs::write(root.join("bad.json"), "{not json").map_err(|err| err.to_string())?;
        std::fs::write(parent.join("outside.json"), "{}").map_err(|err| err.to_string())?;

        assert_eq!(run(options(&root, Path::new("good.json"))), Ok(()));

        let Err(message) = run(options(&root, Path::new("absent.json"))) else {
            return Err("a missing result must fail".to_string());
        };
        assert!(
            message.starts_with("canonicalize swarm ingest --result absent.json failed: "),
            "{message}"
        );

        let Err(message) = run(options(&root, &parent.join("outside.json"))) else {
            return Err("a result outside the root must fail".to_string());
        };
        assert!(message.contains("must stay under root"), "{message}");

        let Err(message) = run(options(&root, Path::new("bad.json"))) else {
            return Err("malformed JSON must fail".to_string());
        };
        assert!(
            message.starts_with("failed to parse swarm ingest result JSON: "),
            "{message}"
        );
        let _ = std::fs::remove_dir_all(&parent);
        Ok(())
    }
}
