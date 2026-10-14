//! Cut-step check for the README Cargo actually ships in the `ripr` crate.
//!
//! #7475 part (2): `check-product-copy` and `check-readme-state` read working-tree
//! files. The crates.io page renders the packaged README. This gate binds the
//! current-release sentence and the default-base instruction to
//! `cargo package -p ripr --list` plus the file that list says is included.

use std::fs;
use std::path::{Path, PathBuf};

use crate::{FixKind, PolicyReportSpec, finish_policy_report};

const WORKSPACE_MANIFEST: &str = "Cargo.toml";
const CRATE_MANIFEST: &str = "crates/ripr/Cargo.toml";
const DEFAULT_README: &str = "README.md";
const PACKAGE_LIST_COMMAND: &str = "cargo package -p ripr --list";
const TAUGHT_DEFAULT_BASE: &str = "ripr check --base origin/main";
const CASCADE_HEAD: &str = "origin/HEAD";
const CURRENT_LINE_CLAIM: &str = "current public release line";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PackagedReadmeInputs<'a> {
    pub(crate) workspace_version: &'a str,
    pub(crate) package_list: &'a str,
    pub(crate) readme_filename: &'a str,
    pub(crate) readme: &'a str,
}

pub(crate) fn check_packaged_readme() -> Result<(), String> {
    let root = repo_root()?;
    let package_list = crate::run_output("cargo", &["package", "-p", "ripr", "--list"]).map_err(
        |err| {
            format!(
                "{PACKAGE_LIST_COMMAND} failed; the packaged README cannot be judged without the list cargo would ship: {err}"
            )
        },
    )?;
    let violations = packaged_readme_violations_at(&root, &package_list)?;
    finish_policy_report(report_spec(), &violations)
}

fn report_spec() -> PolicyReportSpec<'static> {
    PolicyReportSpec {
        report_file: "packaged-readme.md",
        check: "check-packaged-readme",
        why_it_matters: "crates.io renders the README packaged into the `.crate`, not the working-tree file reviewers edit. A stale current-release sentence or a false `origin/main` default-base instruction can ship even when the repo README is already corrected.",
        fix_kind: FixKind::AuthorDecisionRequired,
        recommended_fixes: &[
            "Name the workspace version in the crate README's current-release sentence.",
            "Document the default-base cascade starting at `origin/HEAD`; do not teach `ripr check --base origin/main` as the default.",
            "Keep `readme` / `include` in crates/ripr/Cargo.toml aligned so `cargo package -p ripr --list` actually contains that README.",
        ],
        rerun_command: "cargo xtask check-packaged-readme",
        exception_template: None,
    }
}

fn packaged_readme_violations_at(root: &Path, package_list: &str) -> Result<Vec<String>, String> {
    let workspace = read_text(&root.join(WORKSPACE_MANIFEST))?;
    let workspace_version = workspace_package_version(&workspace)?;
    let crate_manifest = read_text(&root.join(CRATE_MANIFEST))?;
    let readme_filename = crate_readme_filename(&crate_manifest);
    let readme_path = root.join("crates").join("ripr").join(&readme_filename);
    let readme = read_text(&readme_path)?;
    Ok(packaged_readme_violations(&PackagedReadmeInputs {
        workspace_version: &workspace_version,
        package_list,
        readme_filename: &readme_filename,
        readme: &readme,
    }))
}

pub(crate) fn packaged_readme_violations(inputs: &PackagedReadmeInputs<'_>) -> Vec<String> {
    let mut violations = Vec::new();

    if !package_list_contains_readme(inputs.package_list, inputs.readme_filename) {
        violations.push(format!(
            "{PACKAGE_LIST_COMMAND} does not include `{}`; crates.io cannot render a README that cargo does not package",
            inputs.readme_filename
        ));
    }

    if !inputs.readme.contains(inputs.workspace_version) {
        violations.push(format!(
            "packaged README does not name workspace version `{}`",
            inputs.workspace_version
        ));
    }

    violations.extend(stale_current_line_violations(
        inputs.readme,
        inputs.workspace_version,
    ));

    if !inputs.readme.contains(CASCADE_HEAD) {
        violations.push(format!(
            "packaged README is missing the default-base cascade start `{CASCADE_HEAD}`"
        ));
    }

    if teaches_origin_main_default(inputs.readme) {
        violations.push(format!(
            "packaged README teaches `{TAUGHT_DEFAULT_BASE}` as the default"
        ));
    }

    violations
}

fn stale_current_line_violations(readme: &str, workspace_version: &str) -> Vec<String> {
    let mut violations = Vec::new();
    let workspace_line = major_minor(workspace_version);

    for line in readme.lines() {
        if let Some(claimed) = claimed_current_release_version(line)
            && claimed != workspace_version
        {
            violations.push(format!(
                "packaged README names `{claimed}` as the current public release line; workspace version is `{workspace_version}`"
            ));
        } else if line.contains(CURRENT_LINE_CLAIM) && !line.contains(workspace_version) {
            violations.push(format!(
                "packaged README names a current public release line that is not workspace version `{workspace_version}`: {}",
                line.trim()
            ));
        }
        if let Some(claimed) = claimed_x_line(line)
            && Some(claimed.as_str()) != workspace_line.as_deref()
        {
            violations.push(format!(
                "packaged README still describes the `{claimed}.x` line as current; workspace version is `{workspace_version}`"
            ));
        }
    }

    violations
}

fn claimed_current_release_version(line: &str) -> Option<String> {
    let before = line.split_once(" is the current public release line")?.0;
    let version = before.rsplit('`').nth(1)?.trim();
    if looks_like_version(version) {
        Some(version.to_string())
    } else {
        None
    }
}

fn looks_like_version(value: &str) -> bool {
    let mut parts = value.split('.');
    let major = parts.next();
    let minor = parts.next();
    match (major, minor) {
        (Some(major), Some(minor)) => {
            !major.is_empty()
                && !minor.is_empty()
                && major.bytes().all(|byte| byte.is_ascii_digit())
                && minor.bytes().all(|byte| byte.is_ascii_digit())
        }
        _ => false,
    }
}

fn claimed_x_line(line: &str) -> Option<String> {
    let after_tick = line.split('`').nth(1)?;
    let (major_minor, rest) = after_tick.split_once(".x")?;
    if (rest.starts_with(" line") || rest.is_empty()) && looks_like_version(major_minor) {
        return Some(major_minor.to_string());
    }
    None
}

fn teaches_origin_main_default(readme: &str) -> bool {
    readme.lines().any(|line| {
        let trimmed = line.trim().trim_start_matches('$').trim();
        command_text(trimmed) == TAUGHT_DEFAULT_BASE
    })
}

fn command_text(line: &str) -> &str {
    let without_prompt = line.trim_start_matches(['>', '#']).trim();
    without_prompt
        .split_once('#')
        .map_or(without_prompt, |(command, _)| command)
        .trim()
}

fn package_list_contains_readme(package_list: &str, readme_filename: &str) -> bool {
    package_list.lines().any(|line| {
        Path::new(line.trim())
            .file_name()
            .and_then(|name| name.to_str())
            == Some(readme_filename)
    })
}

pub(crate) fn workspace_package_version(workspace_toml: &str) -> Result<String, String> {
    let mut in_workspace_package = false;
    for raw in workspace_toml.lines() {
        let line = strip_toml_comment(raw).trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') {
            in_workspace_package = line == "[workspace.package]";
            continue;
        }
        if in_workspace_package && let Some(version) = toml_string_value(line, "version") {
            if version.is_empty() {
                return Err("workspace.package version is empty".to_string());
            }
            return Ok(version);
        }
    }
    Err("Cargo.toml [workspace.package] version is missing".to_string())
}

pub(crate) fn crate_readme_filename(crate_toml: &str) -> String {
    let mut in_package = false;
    for raw in crate_toml.lines() {
        let line = strip_toml_comment(raw).trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') {
            in_package = line == "[package]";
            continue;
        }
        if in_package
            && let Some(readme) = toml_string_value(line, "readme")
            && !readme.is_empty()
        {
            return readme;
        }
    }
    DEFAULT_README.to_string()
}

fn toml_string_value(line: &str, key: &str) -> Option<String> {
    let rest = line.strip_prefix(key)?.trim_start();
    let rest = rest.strip_prefix('=')?.trim_start();
    let quote = rest.as_bytes().first().copied()?;
    if quote != b'"' && quote != b'\'' {
        return None;
    }
    let body = rest.get(1..)?;
    let end = body.find(quote as char)?;
    Some(body[..end].to_string())
}

fn strip_toml_comment(line: &str) -> &str {
    let mut in_string = false;
    let mut quote = None;
    for (idx, byte) in line.bytes().enumerate() {
        if let Some(current) = quote {
            if byte == current {
                in_string = false;
                quote = None;
            }
            continue;
        }
        if byte == b'"' || byte == b'\'' {
            in_string = true;
            quote = Some(byte);
            continue;
        }
        if byte == b'#' && !in_string {
            return &line[..idx];
        }
    }
    line
}

fn major_minor(version: &str) -> Option<String> {
    let mut parts = version.split('.');
    let major = parts.next()?;
    let minor = parts.next()?;
    if major.is_empty() || minor.is_empty() {
        return None;
    }
    Some(format!("{major}.{minor}"))
}

fn read_text(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|err| format!("read {}: {err}", path.display()))
}

fn repo_root() -> Result<PathBuf, String> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.parent().map(Path::to_path_buf).ok_or_else(|| {
        format!(
            "failed to resolve repo root from {}",
            manifest_dir.display()
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const CURRENT_LIKE: &str = r#"# ripr

This source package is **0.11.0 development, pending publication**. The latest
GitHub release is 0.10.0.

The 0.10 release resolves its default base by trying `origin/HEAD`, then `origin/main`,
`origin/master`, `main`, and `master`; use `--base REF` to choose another
existing branch or commit:

```bash
ripr check
```
"#;

    const STALE_0_10_SHIPPED: &str = r#"# ripr

The `0.8.x` line keeps the analyzer, editor, CI, and agent loops aligned around static evidence, preview-language visibility, and repo-local operating packets for targeted tests.

# Analyze the current Git diff against origin/main
ripr check --base origin/main

| Distribution | `0.8.0` is the current public release line for the Rust crate, GitHub Release server assets, VS Code/Open VSX extension metadata, generated CI workflow artifacts, and Rust 1.95 MSRV. |
"#;

    fn inputs<'a>(
        version: &'a str,
        list: &'a str,
        filename: &'a str,
        readme: &'a str,
    ) -> PackagedReadmeInputs<'a> {
        PackagedReadmeInputs {
            workspace_version: version,
            package_list: list,
            readme_filename: filename,
            readme,
        }
    }

    #[test]
    fn current_like_readme_with_listed_readme_passes() {
        let violations = packaged_readme_violations(&inputs(
            "0.11.0",
            "Cargo.toml\nREADME.md\nsrc/lib.rs\n",
            "README.md",
            CURRENT_LIKE,
        ));
        assert!(
            violations.is_empty(),
            "expected current-like packaged README to pass; got {violations:?}"
        );
    }

    #[test]
    fn package_list_prefix_still_counts_as_included() {
        let violations = packaged_readme_violations(&inputs(
            "0.11.0",
            "ripr-0.11.0/Cargo.toml\nripr-0.11.0/README.md\nripr-0.11.0/src/lib.rs\n",
            "README.md",
            CURRENT_LIKE,
        ));
        assert!(
            violations.is_empty(),
            "expected prefixed package-list README to count; got {violations:?}"
        );
    }

    #[test]
    fn missing_readme_from_package_list_fails_even_when_content_is_current() {
        let violations = packaged_readme_violations(&inputs(
            "0.11.0",
            "Cargo.toml\nsrc/lib.rs\n",
            "README.md",
            CURRENT_LIKE,
        ));
        assert!(
            violations
                .iter()
                .any(|v| v.contains("does not include `README.md`")),
            "expected inclusion failure; got {violations:?}"
        );
    }

    #[test]
    fn shipped_0_10_readme_fails_current_line_and_default_base() {
        let violations = packaged_readme_violations(&inputs(
            "0.11.0",
            "README.md\n",
            "README.md",
            STALE_0_10_SHIPPED,
        ));
        let joined = violations.join("\n");
        assert!(
            joined.contains("does not name workspace version `0.11.0`"),
            "expected missing workspace version; got {violations:?}"
        );
        assert!(
            joined.contains("`0.8.x` line as current"),
            "expected stale 0.8.x current-line claim; got {violations:?}"
        );
        assert!(
            joined.contains("names `0.8.0` as the current public release line"),
            "expected stale current-public-release-line claim; got {violations:?}"
        );
        assert!(
            joined.contains("missing the default-base cascade start `origin/HEAD`"),
            "expected missing cascade; got {violations:?}"
        );
        assert!(
            joined.contains("teaches `ripr check --base origin/main` as the default"),
            "expected taught origin/main default; got {violations:?}"
        );
    }

    #[test]
    fn origin_main_in_cascade_is_not_the_taught_default() {
        let violations =
            packaged_readme_violations(&inputs("0.11.0", "README.md\n", "README.md", CURRENT_LIKE));
        assert!(
            !violations
                .iter()
                .any(|v| v.contains("teaches `ripr check --base origin/main`")),
            "cascade mention of origin/main must not be treated as the taught default; got {violations:?}"
        );
    }

    #[test]
    fn dollar_prompt_still_counts_as_taught_default() {
        let readme = CURRENT_LIKE.replace("ripr check\n", "$ ripr check --base origin/main\n");
        let violations =
            packaged_readme_violations(&inputs("0.11.0", "README.md\n", "README.md", &readme));
        assert!(
            violations
                .iter()
                .any(|v| v.contains("teaches `ripr check --base origin/main` as the default")),
            "expected prompted command to count; got {violations:?}"
        );
    }

    #[test]
    fn wrong_current_line_with_workspace_version_still_fails() {
        let readme = CURRENT_LIKE.replace(
            "This source package is **0.11.0 development, pending publication**.",
            "This source package is **0.11.0 development, pending publication**. `0.8.0` is the current public release line.",
        );
        let violations =
            packaged_readme_violations(&inputs("0.11.0", "README.md\n", "README.md", &readme));
        assert!(
            violations
                .iter()
                .any(|v| v.contains("names `0.8.0` as the current public release line")),
            "expected wrong current-line claim to fail even when the line also names 0.11.0; got {violations:?}"
        );
    }

    #[test]
    fn matching_current_line_claim_is_allowed() {
        let readme = CURRENT_LIKE.replace(
            "This source package is **0.11.0 development, pending publication**.",
            "`0.11.0` is the current public release line.",
        );
        let violations =
            packaged_readme_violations(&inputs("0.11.0", "README.md\n", "README.md", &readme));
        assert!(
            !violations
                .iter()
                .any(|v| v.contains("current public release line")),
            "expected matching current-line claim to pass; got {violations:?}"
        );
    }

    #[test]
    fn live_crate_readme_matches_workspace_version() -> Result<(), String> {
        let root = repo_root()?;
        let workspace = read_text(&root.join(WORKSPACE_MANIFEST))?;
        let version = workspace_package_version(&workspace)?;
        let crate_manifest = read_text(&root.join(CRATE_MANIFEST))?;
        let filename = crate_readme_filename(&crate_manifest);
        let readme = read_text(&root.join("crates").join("ripr").join(&filename))?;
        let violations = packaged_readme_violations(&inputs(
            &version,
            &format!("{filename}\n"),
            &filename,
            &readme,
        ));
        if !violations.is_empty() {
            return Err(format!(
                "live crates/ripr README should satisfy the packaged-README contract; got {violations:?}"
            ));
        }
        Ok(())
    }

    #[test]
    fn workspace_package_version_reads_quoted_value() -> Result<(), String> {
        let version = workspace_package_version(
            "[workspace]\nresolver = \"2\"\n\n[workspace.package]\nversion = \"0.11.0\"\nedition = \"2024\"\n",
        )?;
        if version != "0.11.0" {
            return Err(format!("expected 0.11.0, got {version}"));
        }
        Ok(())
    }

    #[test]
    fn workspace_package_version_ignores_other_sections() -> Result<(), String> {
        let Err(err) = workspace_package_version(
            "[package]\nversion = \"0.8.0\"\n[workspace]\nresolver = \"2\"\n",
        ) else {
            return Err("other-section version must not count".to_string());
        };
        if !err.contains("missing") {
            return Err(format!(
                "expected missing workspace.package version, got {err}"
            ));
        }
        Ok(())
    }

    #[test]
    fn crate_readme_filename_reads_package_field() {
        let name = crate_readme_filename("[package]\nname = \"ripr\"\nreadme = \"README.md\"\n");
        assert_eq!(name, "README.md");
    }

    #[test]
    fn crate_readme_filename_defaults_when_absent() {
        let name = crate_readme_filename("[package]\nname = \"ripr\"\n");
        assert_eq!(name, "README.md");
    }

    #[test]
    fn missing_cascade_is_independent_of_taught_command() {
        let readme = "# ripr\n\nThis source package is **0.11.0 development**.\n\n```bash\nripr check\n```\n";
        let violations =
            packaged_readme_violations(&inputs("0.11.0", "README.md\n", "README.md", readme));
        assert!(
            violations
                .iter()
                .any(|v| v.contains("missing the default-base cascade start `origin/HEAD`")),
            "expected missing cascade; got {violations:?}"
        );
        assert!(
            !violations
                .iter()
                .any(|v| v.contains("teaches `ripr check --base origin/main`")),
            "bare `ripr check` must not be reported as the origin/main default; got {violations:?}"
        );
    }
}
