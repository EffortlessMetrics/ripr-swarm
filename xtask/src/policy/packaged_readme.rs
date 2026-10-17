//! Cut-step check for the README Cargo actually ships in the `ripr` crate.
//!
//! #7475 part (2): `check-product-copy` and `check-readme-state` read working-tree
//! files. The crates.io page renders the packaged README. This gate binds the
//! current-release sentence and the default-base instruction to
//! `cargo package -p ripr --list` plus the file that list says is included.

use std::fs;
use std::path::{Component, Path, PathBuf};

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
    match crate_readme_field(&crate_manifest) {
        CrateReadme::Disabled => Ok(vec![
            "crates/ripr/Cargo.toml sets `readme = false`; crates.io will not render a packaged README"
                .to_string(),
        ]),
        CrateReadme::Filename(readme_filename) => {
            let readme_path = root.join("crates").join("ripr").join(&readme_filename);
            let readme = read_text(&readme_path)?;
            Ok(packaged_readme_violations(&PackagedReadmeInputs {
                workspace_version: &workspace_version,
                package_list,
                readme_filename: &readme_filename,
                readme: &readme,
            }))
        }
    }
}

pub(crate) fn packaged_readme_violations(inputs: &PackagedReadmeInputs<'_>) -> Vec<String> {
    let mut violations = Vec::new();

    if !package_list_contains_readme(inputs.package_list, inputs.readme_filename) {
        violations.push(format!(
            "{PACKAGE_LIST_COMMAND} does not include `{}`; crates.io cannot render a README that cargo does not package",
            inputs.readme_filename
        ));
    }

    if !names_version(inputs.readme, inputs.workspace_version) {
        violations.push(format!(
            "packaged README does not name workspace version `{}`",
            inputs.workspace_version
        ));
    }

    violations.extend(stale_current_line_violations(
        inputs.readme,
        inputs.workspace_version,
    ));

    if !documents_cascade_start(inputs.readme) {
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
        } else if line.contains(CURRENT_LINE_CLAIM) && !names_version(line, workspace_version) {
            violations.push(format!(
                "packaged README names a current public release line that is not workspace version `{workspace_version}`: {}",
                line.trim()
            ));
        }
        if let Some(claimed) = claimed_current_x_line(line)
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

fn names_version(text: &str, version: &str) -> bool {
    text.match_indices(version).any(|(idx, _)| {
        let before = text[..idx].chars().next_back();
        let after_rest = &text[idx + version.len()..];
        let mut after_chars = after_rest.chars();
        let after_ok = match after_chars.next() {
            None => true,
            Some('.') => !after_chars.next().is_some_and(|c| c.is_ascii_digit()),
            Some(c) => !(c.is_ascii_digit() || c == '-'),
        };
        !before.is_some_and(|c| c.is_ascii_digit() || c == '.') && after_ok
    })
}

fn documents_cascade_start(readme: &str) -> bool {
    readme.lines().any(line_documents_cascade_start)
}

fn line_documents_cascade_start(line: &str) -> bool {
    if !line.contains(CASCADE_HEAD) || locally_negated_origin_head(line) {
        return false;
    }
    let lower = line.to_ascii_lowercase();
    lower.contains("default") || lower.contains("cascade") || lower.contains("trying")
}

fn locally_negated_origin_head(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    let Some(idx) = lower.find("origin/head") else {
        return false;
    };
    let before: Vec<&str> = lower[..idx].split_whitespace().collect();
    let start = before.len().saturating_sub(4);
    before[start..].iter().any(|word| {
        let token = word.trim_matches(|c: char| !c.is_ascii_alphabetic() && c != '\'');
        matches!(token, "not" | "don't" | "never")
    })
}

fn claimed_current_x_line(line: &str) -> Option<String> {
    for clause in line.split(';') {
        if !clause.contains("current")
            || historical_line_marker(clause)
            || negated_current_claim(clause)
        {
            continue;
        }
        for chunk in clause.split('`').skip(1).step_by(2) {
            let (major_minor, rest) = match chunk.split_once(".x") {
                Some(parts) => parts,
                None => continue,
            };
            if (rest.starts_with(" line") || rest.is_empty()) && looks_like_version(major_minor) {
                return Some(major_minor.to_string());
            }
        }
    }
    None
}

fn negated_current_claim(clause: &str) -> bool {
    let lower = clause.to_ascii_lowercase();
    lower.contains("not current")
        || lower.contains("isn't current")
        || lower.contains("is not current")
        || lower.contains("no longer current")
}

fn historical_line_marker(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    ["historical", "previous", "older", "prior", "former"]
        .iter()
        .any(|marker| lower.contains(marker))
}

fn teaches_origin_main_default(readme: &str) -> bool {
    readme.lines().any(|line| {
        let trimmed = line.trim().trim_start_matches('$').trim();
        let whole = command_text(trimmed);
        (!negated_instruction(whole) && ripr_check_teaches_origin_main(whole))
            || taught_inline_origin_main(trimmed)
    })
}

fn taught_inline_origin_main(line: &str) -> bool {
    let mut preceding = "";
    for (idx, chunk) in line.split('`').enumerate() {
        if idx % 2 == 1 {
            if !negated_instruction(preceding) && ripr_check_teaches_origin_main(chunk.trim()) {
                return true;
            }
        } else {
            preceding = chunk;
        }
    }
    false
}

fn negated_instruction(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("do not")
        || lower.contains("don't")
        || lower.contains("never ")
        || lower.contains("not run")
        || lower.contains("obsolete")
}

fn ripr_check_teaches_origin_main(command: &str) -> bool {
    let tokens: Vec<&str> = command.split_whitespace().collect();
    if tokens.first().copied() != Some("ripr") || tokens.get(1).copied() != Some("check") {
        return false;
    }
    tokens
        .windows(2)
        .any(|pair| pair[0] == "--base" && pair[1] == "origin/main")
        || tokens.contains(&"--base=origin/main")
}

fn command_text(line: &str) -> &str {
    let without_prompt = line.trim_start_matches(['>', '#']).trim();
    without_prompt
        .split_once('#')
        .map_or(without_prompt, |(command, _)| command)
        .trim()
}

fn package_list_contains_readme(package_list: &str, readme_filename: &str) -> bool {
    let expected = packaged_readme_list_path(readme_filename);
    package_list
        .lines()
        .any(|line| listed_package_path(line.trim()).is_some_and(|listed| listed == expected))
}

fn packaged_readme_list_path(readme_filename: &str) -> PathBuf {
    let path = Path::new(readme_filename);
    if path_escapes_crate(path) {
        path.file_name()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(readme_filename))
    } else {
        normalize_path(path)
    }
}

fn listed_package_path(line: &str) -> Option<PathBuf> {
    let path = normalize_path(Path::new(line));
    let mut components = path.components();
    match components.next() {
        Some(Component::Normal(first))
            if first
                .to_str()
                .is_some_and(|name| name.strip_prefix("ripr-").is_some_and(looks_like_version)) =>
        {
            Some(components.collect())
        }
        Some(_) => Some(path),
        None => None,
    }
}

fn path_escapes_crate(path: &Path) -> bool {
    path.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    })
}

fn normalize_path(path: &Path) -> PathBuf {
    path.components()
        .filter(|component| !matches!(component, Component::CurDir))
        .collect()
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

#[derive(Clone, Debug, Eq, PartialEq)]
enum CrateReadme {
    Disabled,
    Filename(String),
}

#[cfg(test)]
fn crate_readme_filename(crate_toml: &str) -> String {
    match crate_readme_field(crate_toml) {
        CrateReadme::Disabled => DEFAULT_README.to_string(),
        CrateReadme::Filename(name) => name,
    }
}

fn crate_readme_field(crate_toml: &str) -> CrateReadme {
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
        if in_package && toml_bool_false(line, "readme") {
            return CrateReadme::Disabled;
        }
        if in_package
            && let Some(readme) = toml_string_value(line, "readme")
            && !readme.is_empty()
        {
            return CrateReadme::Filename(readme);
        }
    }
    CrateReadme::Filename(DEFAULT_README.to_string())
}

fn toml_bool_false(line: &str, key: &str) -> bool {
    let Some(rest) = line.strip_prefix(key) else {
        return false;
    };
    let Some(rest) = rest.trim_start().strip_prefix('=') else {
        return false;
    };
    rest.trim_start() == "false"
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
    fn nested_readme_in_package_list_does_not_satisfy_crate_root() {
        let violations = packaged_readme_violations(&inputs(
            "0.11.0",
            "Cargo.toml\nexamples/foo/src/README.md\nsrc/lib.rs\n",
            "README.md",
            CURRENT_LIKE,
        ));
        assert!(
            violations
                .iter()
                .any(|v| v.contains("does not include `README.md`")),
            "basename-only match must not credit a nested README.md; got {violations:?}"
        );
    }

    #[test]
    fn nested_crate_readme_matches_listed_nested_path() {
        let violations = packaged_readme_violations(&inputs(
            "0.11.0",
            "Cargo.toml\ndocs/README.md\nsrc/lib.rs\n",
            "docs/README.md",
            CURRENT_LIKE,
        ));
        assert!(
            !violations.iter().any(|v| v.contains("does not include")),
            "listed docs/README.md must satisfy readme = \"docs/README.md\"; got {violations:?}"
        );
    }

    #[test]
    fn outside_crate_readme_matches_packaged_basename() {
        let violations = packaged_readme_violations(&inputs(
            "0.11.0",
            "Cargo.toml\nREADME.md\nsrc/lib.rs\n",
            "../../README.md",
            CURRENT_LIKE,
        ));
        assert!(
            !violations.iter().any(|v| v.contains("does not include")),
            "cargo packages an outside-crate readme as README.md; got {violations:?}"
        );
    }

    #[test]
    fn prefixed_nested_readme_still_counts_as_included() {
        let violations = packaged_readme_violations(&inputs(
            "0.11.0",
            "ripr-0.11.0/Cargo.toml\nripr-0.11.0/docs/README.md\nripr-0.11.0/src/lib.rs\n",
            "docs/README.md",
            CURRENT_LIKE,
        ));
        assert!(
            !violations.iter().any(|v| v.contains("does not include")),
            "prefixed docs/README.md must count; got {violations:?}"
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
    fn extra_flags_do_not_hide_taught_origin_main() {
        let readme = CURRENT_LIKE.replace("ripr check\n", "ripr check --base origin/main --json\n");
        let violations =
            packaged_readme_violations(&inputs("0.11.0", "README.md\n", "README.md", &readme));
        assert!(
            violations
                .iter()
                .any(|v| v.contains("teaches `ripr check --base origin/main` as the default")),
            "expected extra flags not to hide the taught base; got {violations:?}"
        );
    }

    #[test]
    fn equals_form_base_still_counts_as_taught_default() {
        let readme = CURRENT_LIKE.replace("ripr check\n", "ripr check --base=origin/main\n");
        let violations =
            packaged_readme_violations(&inputs("0.11.0", "README.md\n", "README.md", &readme));
        assert!(
            violations
                .iter()
                .any(|v| v.contains("teaches `ripr check --base origin/main` as the default")),
            "expected --base=origin/main to count; got {violations:?}"
        );
    }

    #[test]
    fn current_x_line_claim_fails_when_stale() {
        let readme = CURRENT_LIKE.replace(
            "This source package is **0.11.0 development, pending publication**.",
            "The `0.8.x` line is current.",
        );
        let violations =
            packaged_readme_violations(&inputs("0.11.0", "README.md\n", "README.md", &readme));
        assert!(
            violations
                .iter()
                .any(|v| v.contains("`0.8.x` line as current")),
            "expected current 0.8.x claim to fail; got {violations:?}"
        );
    }

    #[test]
    fn historical_x_line_is_not_a_current_claim() {
        let readme = CURRENT_LIKE.replace(
            "This source package is **0.11.0 development, pending publication**.",
            "The historical `0.10.x` line used the old settings.",
        );
        let violations =
            packaged_readme_violations(&inputs("0.11.0", "README.md\n", "README.md", &readme));
        assert!(
            !violations
                .iter()
                .any(|v| v.contains("`0.10.x` line as current")),
            "historical .x mention must not fail the cut; got {violations:?}"
        );
    }

    #[test]
    fn historical_clause_does_not_hide_later_stale_current_x_line() {
        let readme = CURRENT_LIKE.replace(
            "This source package is **0.11.0 development, pending publication**.",
            "The historical `0.8.x` line is retired; the `0.9.x` line is current.",
        );
        let violations =
            packaged_readme_violations(&inputs("0.11.0", "README.md\n", "README.md", &readme));
        assert!(
            violations
                .iter()
                .any(|v| v.contains("`0.9.x` line as current")),
            "stale current .x after a historical clause must still fail; got {violations:?}"
        );
    }

    #[test]
    fn not_current_x_line_is_not_a_current_claim() {
        let readme = CURRENT_LIKE.replace(
            "This source package is **0.11.0 development, pending publication**.",
            "The `0.8.x` line is not current.",
        );
        let violations =
            packaged_readme_violations(&inputs("0.11.0", "README.md\n", "README.md", &readme));
        assert!(
            !violations
                .iter()
                .any(|v| v.contains("`0.8.x` line as current")),
            "negated current .x mention must not fail the cut; got {violations:?}"
        );
    }

    #[test]
    fn longer_version_token_does_not_satisfy_workspace_version() {
        let readme = CURRENT_LIKE.replace("0.11.0", "10.11.0");
        let violations =
            packaged_readme_violations(&inputs("0.11.0", "README.md\n", "README.md", &readme));
        assert!(
            violations
                .iter()
                .any(|v| v.contains("does not name workspace version `0.11.0`")),
            "10.11.0 must not count as 0.11.0; got {violations:?}"
        );
    }

    #[test]
    fn sentence_ending_period_still_names_workspace_version() {
        let readme = CURRENT_LIKE.replace(
            "This source package is **0.11.0 development, pending publication**.",
            "The current release is 0.11.0.",
        );
        let violations =
            packaged_readme_violations(&inputs("0.11.0", "README.md\n", "README.md", &readme));
        assert!(
            !violations
                .iter()
                .any(|v| v.contains("does not name workspace version")),
            "a sentence-ending period after the version must still count; got {violations:?}"
        );
    }

    #[test]
    fn prerelease_suffix_does_not_satisfy_workspace_version() {
        let readme = CURRENT_LIKE.replace("0.11.0", "0.11.0-rc.1");
        let violations =
            packaged_readme_violations(&inputs("0.11.0", "README.md\n", "README.md", &readme));
        assert!(
            violations
                .iter()
                .any(|v| v.contains("does not name workspace version `0.11.0`")),
            "0.11.0-rc.1 must not count as 0.11.0; got {violations:?}"
        );
    }

    #[test]
    fn negated_origin_head_is_not_the_cascade_start() {
        let readme = CURRENT_LIKE.replace(
            "The 0.10 release resolves its default base by trying `origin/HEAD`, then `origin/main`,",
            "Do not use `origin/HEAD` as the default base; try `origin/main` first,",
        );
        let violations =
            packaged_readme_violations(&inputs("0.11.0", "README.md\n", "README.md", &readme));
        assert!(
            violations
                .iter()
                .any(|v| v.contains("missing the default-base cascade start `origin/HEAD`")),
            "negated origin/HEAD must not count as the cascade start; got {violations:?}"
        );
    }

    #[test]
    fn earlier_clause_negation_does_not_hide_cascade_start() {
        let readme = CURRENT_LIKE.replace(
            "The 0.10 release resolves its default base by trying `origin/HEAD`, then `origin/main`,",
            "`origin/main` is not the default; the cascade starts at `origin/HEAD`",
        );
        let violations =
            packaged_readme_violations(&inputs("0.11.0", "README.md\n", "README.md", &readme));
        assert!(
            !violations
                .iter()
                .any(|v| v.contains("missing the default-base cascade start `origin/HEAD`")),
            "later cascade-start clause must still count; got {violations:?}"
        );
    }

    #[test]
    fn comma_joined_negation_does_not_hide_cascade_start() {
        let readme = CURRENT_LIKE.replace(
            "The 0.10 release resolves its default base by trying `origin/HEAD`, then `origin/main`,",
            "`origin/main` is not the default, and the cascade starts at `origin/HEAD`",
        );
        let violations =
            packaged_readme_violations(&inputs("0.11.0", "README.md\n", "README.md", &readme));
        assert!(
            !violations
                .iter()
                .any(|v| v.contains("missing the default-base cascade start `origin/HEAD`")),
            "comma-joined cascade-start clause must still count; got {violations:?}"
        );
    }

    #[test]
    fn appositive_default_base_after_origin_head_still_counts() {
        let readme = CURRENT_LIKE.replace(
            "The 0.10 release resolves its default base by trying `origin/HEAD`, then `origin/main`,",
            "Use `origin/HEAD`, the default base, then `origin/main`",
        );
        let violations =
            packaged_readme_violations(&inputs("0.11.0", "README.md\n", "README.md", &readme));
        assert!(
            !violations
                .iter()
                .any(|v| v.contains("missing the default-base cascade start `origin/HEAD`")),
            "appositive default-base after origin/HEAD must still count; got {violations:?}"
        );
    }

    #[test]
    fn unrelated_origin_head_mention_is_not_the_cascade_start() {
        let readme = CURRENT_LIKE.replace(
            "The 0.10 release resolves its default base by trying `origin/HEAD`, then `origin/main`,",
            "Clone from `origin/HEAD` when bisecting.",
        );
        let violations =
            packaged_readme_violations(&inputs("0.11.0", "README.md\n", "README.md", &readme));
        assert!(
            violations
                .iter()
                .any(|v| v.contains("missing the default-base cascade start `origin/HEAD`")),
            "unrelated origin/HEAD must not count as the cascade start; got {violations:?}"
        );
    }

    #[test]
    fn inline_backtick_command_still_counts_as_taught_default() {
        let readme = CURRENT_LIKE.replace(
            "ripr check\n",
            "For the default check, run `ripr check --base origin/main`.\n",
        );
        let violations =
            packaged_readme_violations(&inputs("0.11.0", "README.md\n", "README.md", &readme));
        assert!(
            violations
                .iter()
                .any(|v| v.contains("teaches `ripr check --base origin/main` as the default")),
            "expected inline taught command to count; got {violations:?}"
        );
    }

    #[test]
    fn warning_against_obsolete_command_is_not_a_taught_default() {
        let readme = CURRENT_LIKE.replace(
            "ripr check\n",
            "Do not run `ripr check --base origin/main`; use `ripr check`.\n",
        );
        let violations =
            packaged_readme_violations(&inputs("0.11.0", "README.md\n", "README.md", &readme));
        assert!(
            !violations
                .iter()
                .any(|v| v.contains("teaches `ripr check --base origin/main` as the default")),
            "negated obsolete-command warning must not fail the cut; got {violations:?}"
        );
    }

    #[test]
    fn trailing_comment_does_not_hide_taught_origin_main() {
        let readme =
            CURRENT_LIKE.replace("ripr check\n", "ripr check --base origin/main # obsolete\n");
        let violations =
            packaged_readme_violations(&inputs("0.11.0", "README.md\n", "README.md", &readme));
        assert!(
            violations
                .iter()
                .any(|v| v.contains("teaches `ripr check --base origin/main` as the default")),
            "trailing comment must not hide a taught command; got {violations:?}"
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
    fn crate_readme_field_refuses_explicit_false() {
        assert_eq!(
            crate_readme_field("[package]\nname = \"ripr\"\nreadme = false\n"),
            CrateReadme::Disabled
        );
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
