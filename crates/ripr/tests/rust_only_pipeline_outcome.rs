//! #15724: exercise the public check outcome with the Python adapter absent.

#![cfg(all(feature = "lang-rust", not(feature = "lang-python")))]

use ripr::config::RiprConfig;
use ripr::domain::LanguageId;
use ripr::{CheckInput, OutputFormat, render_check, try_check_workspace_with_config};
use serde_json::Value;
use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

struct FixtureRoot(PathBuf);

impl FixtureRoot {
    fn create() -> std::io::Result<Self> {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let temp = std::env::temp_dir();
        fs::create_dir_all(&temp)?;
        let root = temp.join(format!(
            "ripr-feature-outcome-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root)?;
        Ok(Self(root))
    }
}

impl Drop for FixtureRoot {
    fn drop(&mut self) {
        let _cleanup = fs::remove_dir_all(&self.0);
    }
}

fn check_single_line_added_file(
    path: &str,
    line: &str,
    config: &RiprConfig,
) -> Result<Value, Box<dyn Error>> {
    // The helper supplies the same final LF to the file and its one-line hunk.
    if line.is_empty() || line.contains(['\r', '\n']) {
        return Err("fixture requires one nonempty line without CR or LF".into());
    }
    let workspace = FixtureRoot::create()?;
    let root = &workspace.0;
    fs::create_dir(root.join("src"))?;
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"scope-fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )?;
    fs::write(root.join("src/lib.rs"), "")?;
    fs::write(root.join(path), format!("{line}\n"))?;
    let diff_file = root.join("change.diff");
    fs::write(
        &diff_file,
        format!(
            "diff --git a/{path} b/{path}\nnew file mode 100644\n--- /dev/null\n+++ b/{path}\n@@ -0,0 +1 @@\n+{line}\n"
        ),
    )?;

    let output = try_check_workspace_with_config(
        CheckInput {
            root: root.to_path_buf(),
            diff_file: Some(diff_file),
            ..CheckInput::default()
        },
        config,
    )?;
    Ok(serde_json::from_str(&render_check(
        &output,
        &OutputFormat::Json,
    )?)?)
}

fn limitations(report: &Value) -> Result<&[Value], String> {
    report
        .pointer("/analysis_outcome/outcome/limitations")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .ok_or_else(|| format!("missing outcome limitations: {report}"))
}

fn require_unavailable_python(report: &Value) -> Result<(), String> {
    let limitations = limitations(report)?;
    if limitations.iter().any(|row| {
        matches!(
            row["kind"].as_str(),
            Some("requested_scope_excluded" | "language_scope_unsupported")
        )
    }) {
        return Err("changed file was treated as an exclusion or preview skip".into());
    }
    let changed_python = report
        .pointer("/summary/changed_files_by_language")
        .and_then(Value::as_array)
        .is_some_and(|rows| {
            rows.iter()
                .any(|row| row["language"] == "python" && row["files"].as_u64() == Some(1))
        });
    if !changed_python
        || report.pointer("/analysis_outcome/analysis_complete") != Some(&Value::Bool(false))
        || report
            .pointer("/analysis_outcome/outcome/kind")
            .and_then(Value::as_str)
            != Some("partial_with_limitations")
        || !limitations.iter().any(|row| {
            row["kind"] == "language_adapter_unavailable"
                && row["bounded_detail"]
                    .as_str()
                    .is_some_and(|detail| detail.contains("python"))
        })
    {
        return Err(format!(
            "expected one changed Python file to stay partial: {report}"
        ));
    }
    Ok(())
}

#[test]
fn excluded_python_input_stays_partial_when_the_adapter_is_unavailable()
-> Result<(), Box<dyn Error>> {
    let mut config = RiprConfig::default();
    config.languages.enabled.push(LanguageId::Python);
    config.languages.python.excluded_file_patterns = vec!["src/value.py".into()];

    for invalid in ["", "first\nsecond", "first\r\nsecond", "first\n", "first\r"] {
        assert_eq!(
            check_single_line_added_file("src/value.py", invalid, &config)
                .err()
                .map(|error| error.to_string())
                .as_deref(),
            Some("fixture requires one nonempty line without CR or LF"),
            "fixture unexpectedly accepted {invalid:?}"
        );
    }

    let python = check_single_line_added_file("src/value.py", "VALUE = 1", &config)?;
    require_unavailable_python(&python)?;

    // Exercise the competing outcome through the same public API. An available
    // adapter can apply its configured exclusion and report complete scope.
    let mut rust_config = RiprConfig::default();
    rust_config.languages.rust.generated_file_patterns = vec!["src/value.rs".into()];
    let rust =
        check_single_line_added_file("src/value.rs", "pub const VALUE: u32 = 1;", &rust_config)?;
    assert_eq!(
        rust.pointer("/analysis_outcome/analysis_complete"),
        Some(&Value::Bool(true)),
        "{rust}"
    );
    assert!(
        limitations(&rust)?.iter().any(|row| {
            row["kind"] == "requested_scope_excluded"
                && row["bounded_detail"]
                    .as_str()
                    .is_some_and(|detail| detail.contains("src/value.rs"))
        }),
        "{rust}"
    );
    assert_eq!(
        require_unavailable_python(&rust).err().as_deref(),
        Some("changed file was treated as an exclusion or preview skip")
    );
    Ok(())
}
