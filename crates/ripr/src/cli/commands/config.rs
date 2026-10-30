//! Repository configuration commands.

use crate::cli::help;
use crate::cli::suggest::{unknown_argument, unknown_value};
use crate::config::load_for_root;
use crate::output::path::human_path;
use std::path::{Path, PathBuf};

pub(in crate::cli) fn config(args: &[String]) -> Result<(), String> {
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        help::print_config_help();
        return Ok(());
    }

    let Some((subcommand, rest)) = args.split_first() else {
        help::print_config_help();
        return Ok(());
    };
    if subcommand != "validate" {
        return Err(unknown_value(
            "config subcommand",
            subcommand,
            &["validate"],
        ));
    }

    println!("{}", validate_config(&parse_validate_root(rest)?)?);
    Ok(())
}

fn parse_validate_root(args: &[String]) -> Result<PathBuf, String> {
    match args {
        [] => Ok(PathBuf::from(".")),
        [flag] if flag == "--root" => Err("missing value for --root".to_string()),
        // `--root` is an option token here; `./--root` names the literal path.
        [flag, value, ..] if flag == "--root" && value == "--root" => {
            Err("missing value for --root".to_string())
        }
        [flag, value] if flag == "--root" => Ok(PathBuf::from(value)),
        [flag, _value, extra, ..] if flag == "--root" => {
            Err(unknown_argument("config validate", extra))
        }
        [other, ..] => Err(unknown_argument("config validate", other)),
    }
}

fn validate_config(root: &Path) -> Result<String, String> {
    if !root.is_dir() {
        return Err(format!(
            "config validate root {} is not a directory; pass the directory that contains the workspace (for a Cargo.toml path, its parent directory)",
            root.display()
        ));
    }
    // A missing file is a valid first run, but saying "ripr.toml valid" there
    // told users a file they never wrote had been checked. Ancestor
    // discovery can also validate a parent's file the user did not point at,
    // so the success line names the file that actually governed (#9604).
    match load_for_root(root) {
        Ok(config) => Ok(match &config.source_path {
            // `human_path` is the lossless renderer: the escaping display
            // renderer would rewrite a real `%` or `\` in the filename and
            // name a file that does not exist (#9604 review).
            Some(path) => format!("✓ ripr.toml valid ({})", human_path(path)),
            None => "✓ no ripr.toml found; built-in defaults apply".to_string(),
        }),
        // Marker detection can refuse a root that has no config file at all
        // (Python markers in a binary without `lang-python`). That is a
        // setup advisory about a file the user never wrote, not a validation
        // failure — the command stays green and carries the note. The
        // discovered-file guard matters: a parse/read error is prefixed with
        // the file path, which can itself start with this text when a parent
        // directory is named after the marker sentence; a malformed config
        // must still fail instead of passing as an advisory (#9604 review).
        Err(error)
            if error.starts_with("Python project markers were detected")
                && !crate::config::config_discovered_for_root(root) =>
        {
            Ok(format!(
                "✓ no ripr.toml found; built-in defaults apply (note: {error})"
            ))
        }
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::CONFIG_FILE_NAME;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    fn temp_dir(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("ripr-config-{label}-{nonce}"))
    }

    #[test]
    fn validate_accepts_a_valid_root_config() -> Result<(), String> {
        let root = temp_dir("valid");
        fs::create_dir_all(&root).map_err(|error| error.to_string())?;
        fs::write(
            root.join(CONFIG_FILE_NAME),
            "[analysis]\nmode = \"draft\"\ninclude_unchanged_tests = true\n",
        )
        .map_err(|error| error.to_string())?;

        let result = validate_config(&root);
        fs::remove_dir_all(&root).map_err(|error| error.to_string())?;
        let message = result?;
        // #9604 batch B: the success line names the file that governed, so
        // ancestor discovery cannot silently validate a parent's config.
        if !message.starts_with("✓ ripr.toml valid (") || !message.contains(CONFIG_FILE_NAME) {
            return Err(format!(
                "valid configuration returned the wrong success message: {message}"
            ));
        }
        Ok(())
    }

    #[test]
    fn validate_accepts_a_missing_config_using_built_in_defaults() -> Result<(), String> {
        let root = temp_dir("missing");
        fs::create_dir_all(&root).map_err(|error| error.to_string())?;

        let result = validate_config(&root);
        fs::remove_dir_all(&root).map_err(|error| error.to_string())?;
        if result? != "✓ no ripr.toml found; built-in defaults apply" {
            return Err("missing configuration returned the wrong success message".to_string());
        }
        Ok(())
    }

    #[test]
    fn validate_names_the_discovered_file_losslessly() -> Result<(), String> {
        // #9604 review: a real `%` in a directory name must survive the
        // success line. The escaping display renderer would rewrite it and
        // name a file that does not exist.
        let root = temp_dir("lossless").join("100%");
        fs::create_dir_all(&root).map_err(|error| error.to_string())?;
        fs::write(
            root.join(CONFIG_FILE_NAME),
            "[analysis]\nmode = \"draft\"\ninclude_unchanged_tests = true\n",
        )
        .map_err(|error| error.to_string())?;

        let result = validate_config(&root);
        fs::remove_dir_all(&root).map_err(|error| error.to_string())?;
        let message = result?;
        if !message.starts_with("✓ ripr.toml valid (") || !message.contains("100%") {
            return Err(format!(
                "the success line must name the discovered file losslessly: {message}"
            ));
        }
        Ok(())
    }

    #[test]
    fn validate_rejects_a_malformed_config_under_a_marker_named_root() -> Result<(), String> {
        // #9604 review: loader errors are prefixed with the file path, which
        // can itself start with the marker sentence when a parent directory
        // is named after it. The advisory branch must not swallow a real
        // parse failure.
        let parent = temp_dir("marker-named-root");
        let root = parent.join("Python project markers were detected");
        fs::create_dir_all(&root).map_err(|error| error.to_string())?;
        fs::write(root.join(CONFIG_FILE_NAME), "not = [valid toml\n")
            .map_err(|error| error.to_string())?;

        let result = validate_config(&root);
        fs::remove_dir_all(&parent).map_err(|error| error.to_string())?;
        result
            .err()
            .ok_or_else(|| {
                "a malformed config must fail even when the root path starts with the marker sentence"
                    .to_string()
            })
            .map(|_| ())
    }

    #[cfg(not(feature = "lang-python"))]
    #[test]
    fn validate_carries_python_marker_refusal_as_an_advisory() -> Result<(), String> {
        // Without `lang-python`, marker detection refuses a marker-only root;
        // `config validate` downgrades that to a green advisory because the
        // user never wrote a config file (#9604).
        let root = temp_dir("python-marker-advisory");
        fs::create_dir_all(&root).map_err(|error| error.to_string())?;
        fs::write(root.join("pyproject.toml"), "[project]\nname = \"demo\"\n")
            .map_err(|error| error.to_string())?;

        let result = validate_config(&root);
        fs::remove_dir_all(&root).map_err(|error| error.to_string())?;
        let message = result?;
        if !message.starts_with("✓ no ripr.toml found; built-in defaults apply (note: Python project markers were detected")
        {
            return Err(format!(
                "marker refusal must stay a green advisory naming the note: {message}"
            ));
        }
        Ok(())
    }

    #[test]
    fn validate_rejects_missing_and_file_roots() -> Result<(), String> {
        let missing = temp_dir("missing-root");
        let missing_error = validate_config(&missing).err().ok_or_else(|| {
            "a nonexistent validation root must not use built-in defaults".to_string()
        })?;
        if !missing_error.contains("is not a directory") {
            return Err(format!("unexpected missing-root error: {missing_error}"));
        }

        let file_root = temp_dir("file-root");
        fs::write(&file_root, "not a directory\n").map_err(|error| error.to_string())?;
        let file_error = validate_config(&file_root)
            .err()
            .ok_or_else(|| "a file validation root must not use built-in defaults".to_string())?;
        fs::remove_file(&file_root).map_err(|error| error.to_string())?;
        if !file_error.contains("is not a directory") {
            return Err(format!("unexpected file-root error: {file_error}"));
        }
        Ok(())
    }

    #[test]
    fn validate_rejects_a_nul_path_in_valid_toml() -> Result<(), String> {
        let root = temp_dir("nul-path");
        fs::create_dir_all(&root).map_err(|error| error.to_string())?;
        fs::write(
            root.join(CONFIG_FILE_NAME),
            r#"[suppressions]
path = ".ripr/suppressions\u0000.toml"
"#,
        )
        .map_err(|error| error.to_string())?;

        let result = validate_config(&root);
        fs::remove_dir_all(&root).map_err(|error| error.to_string())?;
        let error = result
            .err()
            .ok_or_else(|| "an unusable path must not validate".to_string())?;
        assert!(error.contains("ripr.toml"), "{error}");
        assert!(
            error.contains("suppressions.path must not contain NUL characters"),
            "{error}"
        );
        assert!(!error.contains('\0'));
        Ok(())
    }

    #[test]
    fn validate_rejects_unusable_perl_executable_paths() -> Result<(), String> {
        let root = temp_dir("perl-executable");
        fs::create_dir_all(&root).map_err(|error| error.to_string())?;
        let result = (|| {
            for (value, expected) in [
                ("''", "perl.executable must not be empty"),
                (
                    r#""tools/\u0000exporter""#,
                    "perl.executable must not contain NUL characters",
                ),
            ] {
                fs::write(
                    root.join(CONFIG_FILE_NAME),
                    format!("[perl]\nexecutable = {value}\n"),
                )
                .map_err(|error| error.to_string())?;
                let error = validate_config(&root)
                    .err()
                    .ok_or_else(|| format!("unusable Perl executable {value} validated"))?;
                if !error.contains("ripr.toml") || !error.contains(expected) || error.contains('\0')
                {
                    return Err(format!("unexpected executable validation error: {error:?}"));
                }
            }
            Ok(())
        })();
        fs::remove_dir_all(&root).map_err(|error| error.to_string())?;
        result
    }

    #[test]
    fn validate_returns_the_loader_error_for_invalid_toml() -> Result<(), String> {
        let root = temp_dir("invalid");
        fs::create_dir_all(&root).map_err(|error| error.to_string())?;
        fs::write(root.join(CONFIG_FILE_NAME), "[analysis\n").map_err(|error| error.to_string())?;

        let result = validate_config(&root);
        fs::remove_dir_all(&root).map_err(|error| error.to_string())?;
        let error = match result {
            Ok(message) => {
                return Err(format!(
                    "invalid TOML unexpectedly passed validation with {message}"
                ));
            }
            Err(error) => error,
        };
        if !error.contains("ripr.toml") || !error.contains("invalid ripr.toml") {
            return Err(format!("expected a path-qualified TOML error, got {error}"));
        }
        Ok(())
    }

    #[test]
    fn unknown_config_subcommand_lists_validate_without_a_near_miss() {
        assert_eq!(
            config(&args(&["unknown"])),
            Err("unknown config subcommand \"unknown\". Accepted: validate.".to_string())
        );
    }

    #[test]
    fn unknown_config_subcommand_suggests_validate_for_validat() {
        assert_eq!(
            config(&args(&["validat"])),
            Err(
                "unknown config subcommand \"validat\". Did you mean `validate`? Accepted: validate."
                    .to_string()
            )
        );
    }

    #[test]
    fn validate_parser_rejects_unknown_arguments_and_missing_root() -> Result<(), String> {
        let cases = [
            (
                args(&["validate", "--root"]),
                "missing value for --root".to_string(),
            ),
            (
                args(&["validate", "--wat"]),
                "unknown config validate argument \"--wat\". Run `ripr config validate --help`."
                    .to_string(),
            ),
            (
                args(&["validate", "--root", "/tmp/workspace", "extra"]),
                "unknown config validate argument \"extra\". Run `ripr config validate --help`."
                    .to_string(),
            ),
        ];
        for (input, expected) in cases {
            let actual = config(&input);
            if actual != Err(expected.clone()) {
                return Err(format!("expected {expected:?}, got {actual:?}"));
            }
        }
        Ok(())
    }
}
