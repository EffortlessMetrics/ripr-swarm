//! CLI projection of an explicitly supplied Kani result and its source binding.
//! Import and native-result decisions belong to the app/domain owners; this command only reads bounded artifacts, writes reports, and never runs Kani.

use crate::cli::parse::expect_value;
use crate::cli::suggest::unknown_argument;
use crate::output;
use std::path::{Path, PathBuf};

const DEFAULT_JSON: &str = "target/ripr/reports/kani.json";
const DEFAULT_MARKDOWN: &str = "target/ripr/reports/kani.md";

#[derive(Debug, PartialEq, Eq)]
struct Options {
    root: PathBuf,
    input: PathBuf,
    binding: PathBuf,
    out: PathBuf,
    out_md: PathBuf,
    uses_default_output: bool,
}

pub(super) fn run(args: &[String]) -> Result<(), String> {
    let options = parse_options(args)?;
    let native_json = read_input(
        &options.input,
        "--input",
        crate::app::kani_import::MAX_NATIVE_BYTES as u64,
    )?;
    let binding_json = read_input(
        &options.binding,
        "--binding",
        crate::app::kani_import::MAX_BINDING_BYTES as u64,
    )?;
    let report =
        crate::app::kani_import::import_kani_report(&options.root, &native_json, &binding_json)?;
    let rendered_json = output::kani_report::render_json(&report)?;
    let rendered_markdown = output::kani_report::render_markdown(&report)?;

    // Resolve aliases before either report is written: replacing an input
    // artifact or checked source would destroy the evidence described here.
    validate_output_paths(&options, &report.checked_source_paths)?;
    if options.uses_default_output {
        crate::atomic_file::ensure_report_output_contained(&options.root)?;
    }
    for path in [&options.out, &options.out_md] {
        crate::atomic_file::ensure_relative_output_contained(path)?;
        output::file_write::validate_destination(path).map_err(|error| {
            format!(
                "reports kani output {} is unavailable: {error}",
                output::outcome::display_path(path)
            )
        })?;
    }
    super::write_text_file(&options.out, &rendered_json)?;
    // A previously absent leaf can become an alias on this filesystem.
    validate_output_paths(&options, &report.checked_source_paths)
        .map_err(|error| format!("{error}; the JSON report may already exist"))?;
    super::write_text_file(&options.out_md, &rendered_markdown)?;
    println!("Wrote {}", output::path::human_path(&options.out));
    println!("Wrote {}", output::path::human_path(&options.out_md));
    // Native failure is retained in the report, not converted into an import
    // failure. Returning Ok means parsing/binding/rendering completed only.
    Ok(())
}

fn parse_options(args: &[String]) -> Result<Options, String> {
    let mut root = None;
    let mut input = None;
    let mut binding = None;
    let mut out = None;
    let mut out_md = None;
    let mut index = 0usize;
    while index < args.len() {
        let flag = args[index].as_str();
        let slot = match flag {
            "--root" => &mut root,
            "--input" => &mut input,
            "--binding" => &mut binding,
            "--out" => &mut out,
            "--out-md" => &mut out_md,
            other => return Err(unknown_argument("reports kani", other)),
        };
        if slot.is_some() {
            return Err(format!("reports kani does not accept duplicate {flag}"));
        }
        index += 1;
        let value = expect_value(args, index, flag)?;
        if value.trim().is_empty() {
            return Err(format!("reports kani {flag} requires a non-empty path"));
        }
        *slot = Some(PathBuf::from(value));
        index += 1;
    }

    let input = input.ok_or_else(|| "reports kani requires --input PATH".to_string())?;
    let binding = binding.ok_or_else(|| "reports kani requires --binding PATH".to_string())?;
    let root = root.unwrap_or_else(|| PathBuf::from("."));
    let uses_default_output = out.is_none() || out_md.is_none();
    let out = out.unwrap_or_else(|| root.join(DEFAULT_JSON));
    let out_md = out_md.unwrap_or_else(|| root.join(DEFAULT_MARKDOWN));
    Ok(Options {
        root,
        input,
        binding,
        out,
        out_md,
        uses_default_output,
    })
}

fn read_input(path: &Path, flag: &str, limit: u64) -> Result<String, String> {
    crate::bounded_input::read_to_string_with_limit(path, limit).map_err(|error| {
        format!(
            "reports kani read {flag} {} failed: {}",
            output::outcome::display_path(path),
            crate::bounded_input::read_failure_detail(&error)
        )
    })
}

fn validate_output_paths(options: &Options, checked_sources: &[PathBuf]) -> Result<(), String> {
    let out = output_identity(&options.out)?;
    let out_md = output_identity(&options.out_md)?;
    if paths_alias(&out, &out_md) {
        return Err("reports kani requires distinct --out and --out-md paths".to_string());
    }
    for (flag, input) in [("--input", &options.input), ("--binding", &options.binding)] {
        let identity = std::fs::canonicalize(input).map_err(|error| {
            format!(
                "reports kani resolve {flag} {} failed: {error}",
                output::outcome::display_path(input)
            )
        })?;
        if paths_alias(&identity, &out) || paths_alias(&identity, &out_md) {
            return Err(format!(
                "reports kani output must not replace the {flag} artifact"
            ));
        }
    }
    if checked_sources
        .iter()
        .any(|source| paths_alias(source, &out) || paths_alias(source, &out_md))
    {
        return Err("reports kani output must not replace a checked source file".to_string());
    }
    Ok(())
}

fn paths_alias(left: &Path, right: &Path) -> bool {
    if left == right {
        return true;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if let (Ok(left), Ok(right)) = (left.metadata(), right.metadata()) {
            return left.dev() == right.dev() && left.ino() == right.ino();
        }
    }
    false
}

/// Resolve existing aliases while allowing report directories not yet created.
/// The ordinary atomic output writer still owns destination acquisition.
fn output_identity(path: &Path) -> Result<PathBuf, String> {
    let absolute = std::path::absolute(path).map_err(|error| {
        format!(
            "reports kani resolve output {} failed: {error}",
            path.display()
        )
    })?;
    let mut cursor = absolute.as_path();
    let mut suffix = Vec::<std::ffi::OsString>::new();
    loop {
        match std::fs::canonicalize(cursor) {
            Ok(mut identity) => {
                for component in suffix.iter().rev() {
                    identity.push(component);
                }
                return Ok(identity);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(format!(
                    "reports kani resolve output {} failed: {error}",
                    path.display()
                ));
            }
        }
        let name = cursor.file_name().ok_or_else(|| {
            format!(
                "reports kani cannot resolve output {}; use an existing parent without unresolved '..' components",
                path.display()
            )
        })?;
        suffix.push(name.to_os_string());
        cursor = cursor.parent().ok_or_else(|| {
            format!(
                "reports kani output {} has no existing parent",
                path.display()
            )
        })?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    fn text(path: &Path) -> Result<&str, String> {
        path.to_str()
            .ok_or_else(|| "test path must be UTF-8".to_string())
    }

    fn fixture(label: &str) -> Result<(PathBuf, Vec<String>, serde_json::Value), String> {
        let root = crate::app::kani_import::tests_root(label)?;
        let (native, binding) = crate::app::kani_import::test_inputs(&root)?;
        let mut arguments = args(&["kani", "--root", text(&root)?]);
        for (flag, name, contents) in [
            ("--input", "native.json", &native),
            ("--binding", "binding.json", &binding),
        ] {
            let path = root.join(name);
            std::fs::write(&path, contents).map_err(|error| error.to_string())?;
            arguments.extend(args(&[flag, text(&path)?]));
        }
        let native = serde_json::from_str(&native).map_err(|error| error.to_string())?;
        Ok((root, arguments, native))
    }

    #[test]
    fn required_artifacts_are_enforced_through_reports_dispatch() {
        assert_eq!(
            super::super::reports(&args(&["kani"])),
            Err("reports kani requires --input PATH".to_string())
        );
        assert_eq!(
            super::super::reports(&args(&["kani", "--input", "native.json"])),
            Err("reports kani requires --binding PATH".to_string())
        );
    }

    #[test]
    fn duplicate_blank_missing_and_unknown_options_are_refused() -> Result<(), String> {
        for flag in ["--input", "--binding", "--root", "--out", "--out-md"] {
            assert_eq!(
                parse_options(&args(&[flag, "first", flag, "second"])),
                Err(format!("reports kani does not accept duplicate {flag}"))
            );
            assert_eq!(
                parse_options(&args(&[flag, " "])),
                Err(format!("reports kani {flag} requires a non-empty path"))
            );
            assert_eq!(
                parse_options(&args(&[flag])),
                Err(format!("missing value for {flag}"))
            );
        }
        let error = parse_options(&args(&["--run-kani"]))
            .err()
            .ok_or("the report must not accept a verifier execution flag")?;
        assert!(error.contains("unknown reports kani argument"), "{error}");
        assert!(
            parse_options(&args(&["--bindng"]))
                .is_err_and(|error| error.contains("Did you mean `--binding`?"))
        );
        Ok(())
    }

    #[test]
    fn native_failure_import_writes_reports_without_promoting_provenance() -> Result<(), String> {
        let (root, arguments, native) = fixture("kani-native-failure")?;
        super::super::reports(&arguments)?;
        let report: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(root.join(DEFAULT_JSON)).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        let markdown = std::fs::read_to_string(root.join(DEFAULT_MARKDOWN))
            .map_err(|error| error.to_string())?;
        assert_eq!(report["provenance"], "external_unattested");
        assert_eq!(report["native"]["document"], native);
        assert_eq!(report["native"]["document"]["run_state"], "COMPLETE");
        assert_eq!(
            report["native"]["document"]["harnesses"][0]["outcome"]["verdict"],
            "FAILURE"
        );
        assert!(!markdown.trim().is_empty());
        assert!(markdown.contains("Kani"));
        std::fs::remove_dir_all(root).map_err(|error| error.to_string())?;
        Ok(())
    }

    #[test]
    fn unreadable_input_writes_no_reports() -> Result<(), String> {
        let (root, arguments, _) = fixture("kani-missing-input")?;
        std::fs::remove_file(root.join("native.json")).map_err(|error| error.to_string())?;
        let result = super::super::reports(&arguments);
        assert!(result.is_err_and(|error| error.contains("read --input")));
        assert!(!root.join(DEFAULT_JSON).exists());
        assert!(!root.join(DEFAULT_MARKDOWN).exists());
        std::fs::remove_dir_all(root).map_err(|error| error.to_string())?;
        Ok(())
    }

    #[test]
    fn output_aliases_cannot_replace_inputs_sources_or_each_other() -> Result<(), String> {
        let (root, base, _) = fixture("kani-output-alias")?;
        std::fs::create_dir(root.join("nested")).map_err(|error| error.to_string())?;
        #[cfg(unix)]
        std::fs::hard_link(root.join("test.rs"), root.join("source-alias"))
            .map_err(|error| error.to_string())?;
        for (name, reason) in [
            ("native.json", "the --input artifact"),
            ("binding.json", "the --binding artifact"),
            ("test.rs", "a checked source file"),
            ("Cargo.lock", "a checked source file"),
            #[cfg(unix)]
            ("source-alias", "a checked source file"),
        ] {
            let path = root.join(name);
            let original = std::fs::read(&path).map_err(|error| error.to_string())?;
            let alias = root.join("nested").join("..").join(name);
            for flag in ["--out", "--out-md"] {
                let mut arguments = base.clone();
                arguments.extend(args(&[flag, text(&alias)?]));
                assert_eq!(
                    super::super::reports(&arguments),
                    Err(format!("reports kani output must not replace {reason}"))
                );
                assert_eq!(
                    std::fs::read(&path).map_err(|error| error.to_string())?,
                    original
                );
            }
        }
        let output = root.join("report.json");
        let mut arguments = base.clone();
        arguments.extend(args(&["--out", text(&output)?, "--out-md", text(&output)?]));
        assert_eq!(
            super::super::reports(&arguments),
            Err("reports kani requires distinct --out and --out-md paths".into())
        );
        assert!(!root.join(DEFAULT_JSON).exists());
        assert!(!root.join(DEFAULT_MARKDOWN).exists());
        assert!(!root.join("report.json").exists());
        assert!(!root.join("report.md").exists());
        let json = root.join("fresh.json");
        let markdown = root.join("FRESH.JSON");
        let case_alias = root
            .join("TEST.RS")
            .try_exists()
            .map_err(|e| e.to_string())?;
        let mut arguments = base.clone();
        arguments.extend(args(&["--out", text(&json)?, "--out-md", text(&markdown)?]));
        match (case_alias, super::super::reports(&arguments)) {
            (false, Ok(())) => {
                let text = std::fs::read_to_string(markdown).map_err(|e| e.to_string())?;
                assert!(text.starts_with("# Kani observation"));
            }
            (true, Err(error)) => assert!(error.contains("distinct"), "{error}"),
            (_, result) => return Err(format!("unexpected case-alias outcome: {result:?}")),
        }
        if json.exists() {
            let text = std::fs::read_to_string(json).map_err(|e| e.to_string())?;
            let value: serde_json::Value =
                serde_json::from_str(&text).map_err(|e| e.to_string())?;
            assert_eq!(value["schema_version"], "ripr_kani_report.v1");
        }
        #[cfg(unix)]
        {
            let outside = root.with_extension("outside");
            std::fs::create_dir(&outside).map_err(|error| error.to_string())?;
            std::os::unix::fs::symlink(&outside, root.join("target"))
                .map_err(|error| error.to_string())?;
            let error = super::super::reports(&base)
                .err()
                .ok_or("default reports must refuse a target link outside the root")?;
            assert!(error.contains("resolves outside the workspace"), "{error}");
            let entries = std::fs::read_dir(&outside).map_err(|error| error.to_string())?;
            assert_eq!(entries.count(), 0);
            std::fs::remove_file(root.join("target")).map_err(|error| error.to_string())?;
            std::fs::remove_dir(outside).map_err(|error| error.to_string())?;
        }
        std::fs::remove_dir_all(root).map_err(|error| error.to_string())?;
        Ok(())
    }
}
