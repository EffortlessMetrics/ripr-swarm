//! Product-free docs gate (issue #14644, Leaf A).
//!
//! Runs the verified product-free subset of the precommit docs/spec gates
//! scoped to a changed-file list, without building `ripr` or `xtask`.
//! Classification mirrors the `detect-docs-only` allowlist in
//! `.github/workflows/routed-rust.yml` and fails closed: an empty or
//! unavailable diff, a manifest/lockfile/release surface, a capability
//! registry document, or any unrecognized path widens to full proof
//! (reported as a refusal, never as a pass).
//!
//! Retained product obligations NOT covered by this gate (run the full
//! `cargo xtask precommit` / docs-gate workflow for these):
//! `check-file-policy`, `check-covered-by`, `check-capabilities`,
//! `check-identity-registry`, `capability-docs --check`,
//! `check-capability-producers`, goldens, and the not-yet-ported
//! product-free gates listed in [`RETAINED_PRODUCT_FREE_GATES`].

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Product-free in `xtask` (text/TOML only, verified against
/// `xtask/src/main.rs` and `xtask/src/policy/*`) but not yet ported to a
/// scoped `repo-policy` implementation in this slice. A change touching
/// their inputs widens instead of silently passing (see `plan`).
pub const RETAINED_PRODUCT_FREE_GATES: &[&str] = &[
    "check-local-context",
    "check-droid-review-config",
    "check-traceability",
    "check-output-contracts",
    "check-doc-artifacts",
    "check-doc-index",
    "check-doc-roles",
    "check-readme-state",
    "check-badge-diff-policy",
    "check-release-targets",
    "check-dependencies",
];

/// Gates that require a built `ripr` binary or test binaries. The
/// capability-registry rendering is produced by
/// `domain::language_capability::render_registry_block` via
/// `ripr capabilities --markdown` (`xtask/src/policy/capability_docs.rs`),
/// so a hand edit of the generated block cannot be verified here.
pub const RETAINED_PRODUCT_GATES: &[&str] = &[
    "check-file-policy",
    "check-covered-by",
    "check-capabilities",
    "check-identity-registry",
    "capability-docs --check",
    "check-capability-producers",
];

const REGISTRY_DOCS: &[&str] = &[
    "docs/status/SUPPORT_TIERS.md",
    "docs/LANGUAGE_ADAPTER_PREVIEW.md",
];

const FORBIDDEN_STATIC_TERMS: &[&str] = &["killed", "survived", "untested", "proven", "adequate"];
const REQUIRED_SPEC_HEADINGS: &[&str] = &[
    "## Problem",
    "## Behavior",
    "## Required Evidence",
    "## Non-Goals",
    "## Acceptance Examples",
    "## Test Mapping",
    "## Implementation Mapping",
    "## Metrics",
];
const ALLOWED_DISPOSITIONS: &[&str] = &[
    "internal_shared_type",
    "private_vscode_compatibility",
    "migrate_to_standard_lsp",
    "migrate_to_mcp",
    "historical_fixture_only",
    "retire",
    "experimental_not_public",
];

#[derive(Debug, Default, PartialEq, Eq)]
pub struct DocsGatePlan {
    /// Scoped gates selected for this change set, in run order.
    pub gates: Vec<&'static str>,
    /// Reasons this change set must use full proof instead of this gate.
    pub widen_reasons: Vec<String>,
    /// Paths that touch a retained gate's inputs and therefore widen.
    pub retained_triggers: Vec<String>,
}

impl DocsGatePlan {
    pub fn is_docs_eligible(&self) -> bool {
        self.widen_reasons.is_empty()
    }
}

fn normalize(path: &str) -> String {
    path.trim().replace('\\', "/")
}

fn is_docs_path(path: &str) -> bool {
    path.ends_with(".md")
        || path.starts_with("docs/specs/")
        || path.starts_with("docs/handoffs/")
        || path.starts_with("docs/spec-index.d/")
        || path.starts_with("policy/doc-artifacts.d/")
        || path.starts_with(".ripr/traceability.d/")
        || path.starts_with("changelog.d/")
        || (path.starts_with("fixtures/") && path.contains("-verdict-corpus/spec-scopes/"))
}

fn is_widening_surface(path: &str) -> Option<&'static str> {
    match path {
        "Cargo.lock" | "Cargo.toml" | "CHANGELOG.md" | "docs/OUTPUT_SCHEMA.md" => {
            Some("manifest/lockfile/release surface")
        }
        _ if path == "Cargo.toml" || path.ends_with("/Cargo.toml") => {
            Some("manifest/lockfile/release surface")
        }
        _ if REGISTRY_DOCS.contains(&path) => {
            Some("capability registry document (generated block requires a built ripr)")
        }
        _ => None,
    }
}

/// Classify a changed-file list. Pure: no filesystem or git access.
pub fn plan(changed: &[String]) -> DocsGatePlan {
    let mut result = DocsGatePlan::default();
    if changed.is_empty() {
        result
            .widen_reasons
            .push("empty or unavailable diff: cannot classify as docs-only".to_string());
        return result;
    }
    let mut has_spec = false;
    let mut has_changelog = false;
    let mut has_disposition = false;
    for raw in changed {
        let path = normalize(raw);
        if path.is_empty() {
            result.widen_reasons.push("empty changed path".to_string());
            continue;
        }
        if let Some(reason) = is_widening_surface(&path) {
            result.widen_reasons.push(format!("{path}: {reason}"));
            result.retained_triggers.push(path.clone());
            continue;
        }
        if !is_docs_path(&path) {
            result
                .widen_reasons
                .push(format!("{path}: unrecognized path, not a docs surface"));
            continue;
        }
        has_spec |= path.starts_with("docs/specs/") && path.ends_with(".md");
        has_changelog |= path.starts_with("changelog.d/");
        has_disposition |= path == "policy/ripr-agent-disposition.toml";
    }
    if result.is_docs_eligible() {
        result.gates.push("check-text-encoding");
        result.gates.push("check-static-language");
        result.gates.push("markdown-links");
        result.gates.push("check-generated-clean");
        if has_spec {
            result.gates.push("check-spec-format");
            result.gates.push("check-spec-numbering");
        }
        if has_changelog {
            result.gates.push("check-changelog-fragments");
        }
        if has_disposition {
            result.gates.push("check-ripr-agent-disposition");
        }
    }
    result
}

#[derive(Debug, PartialEq, Eq)]
pub struct DocsGateReport {
    pub plan: DocsGatePlan,
    pub violations: Vec<String>,
}

impl DocsGateReport {
    pub fn render(&self) -> String {
        let mut out = String::from("# docs-gate (repo-policy, product-free)\n\n");
        if !self.plan.is_docs_eligible() {
            out.push_str("Status: widen (full proof required, not a pass)\n\nReasons:\n");
            for reason in &self.plan.widen_reasons {
                out.push_str(&format!("- {reason}\n"));
            }
        } else if self.violations.is_empty() {
            out.push_str("Status: pass (scoped product-free subset only)\n\nRan:\n");
            for gate in &self.plan.gates {
                out.push_str(&format!("- {gate}\n"));
            }
        } else {
            out.push_str("Status: fail\n\nViolations:\n");
            for violation in &self.violations {
                out.push_str(&format!("- {violation}\n"));
            }
        }
        out.push_str("\nNot covered (retained obligations):\n");
        for gate in RETAINED_PRODUCT_GATES
            .iter()
            .chain(RETAINED_PRODUCT_FREE_GATES)
        {
            out.push_str(&format!("- {gate}\n"));
        }
        out
    }
}

fn read_text(root: &Path, rel: &str, violations: &mut Vec<String>) -> Option<String> {
    let path = root.join(rel);
    match std::fs::read(&path) {
        Ok(bytes) => {
            if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
                violations.push(format!(
                    "{rel} starts with a UTF-8 BOM (EF BB BF); save as UTF-8 without BOM [check-text-encoding]"
                ));
            }
            Some(String::from_utf8_lossy(&bytes).into_owned())
        }
        Err(error) => {
            violations.push(format!("{rel}: cannot read changed file: {error}"));
            None
        }
    }
}

fn contains_word(lower: &str, term: &str) -> bool {
    lower
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .any(|word| word == term)
}

fn check_static_language(rel: &str, text: &str, violations: &mut Vec<String>) {
    for (index, line) in text.lines().enumerate() {
        let lower = line.to_ascii_lowercase();
        if lower.contains("ripr-allow: static-language:") {
            continue;
        }
        for term in FORBIDDEN_STATIC_TERMS {
            if contains_word(&lower, term) {
                violations.push(format!(
                    "{rel}:{} uses forbidden static term `{term}` [check-static-language]",
                    index + 1
                ));
            }
        }
    }
}

fn check_markdown_links(root: &Path, rel: &str, text: &str, violations: &mut Vec<String>) {
    let base = Path::new(rel).parent().map(Path::to_path_buf);
    for (index, line) in text.lines().enumerate() {
        let mut rest = line;
        while let Some(start) = rest.find("](") {
            let after = &rest[start + 2..];
            let Some(end) = after.find(')') else { break };
            let target = after[..end].trim().trim_matches(|c| c == '<' || c == '>');
            let target = target.split_whitespace().next().unwrap_or_default();
            rest = &after[end + 1..];
            if target.is_empty()
                || target.starts_with('#')
                || target.starts_with("http://")
                || target.starts_with("https://")
                || target.starts_with("mailto:")
            {
                continue;
            }
            let file_part = target.split('#').next().unwrap_or_default();
            if file_part.is_empty() {
                continue;
            }
            let decoded = file_part.replace("%20", " ");
            let candidate: PathBuf = match &base {
                Some(dir) if !dir.as_os_str().is_empty() => root.join(dir).join(&decoded),
                _ => root.join(&decoded),
            };
            if !candidate.exists() {
                violations.push(format!(
                    "{rel}:{} relative link target does not exist: {target} [markdown-links]",
                    index + 1
                ));
            }
        }
    }
}

fn spec_id_from_file_name(file_name: &str) -> Option<String> {
    let mut parts = file_name.split('-');
    let (prefix, kind, number) = (parts.next()?, parts.next()?, parts.next()?);
    (prefix == "RIPR"
        && kind == "SPEC"
        && number.len() == 4
        && number.bytes().all(|b| b.is_ascii_digit()))
    .then(|| format!("{prefix}-{kind}-{number}"))
}

fn check_spec(
    root: &Path,
    rel: &str,
    text: &str,
    violations: &mut Vec<String>,
    seen: &mut BTreeSet<String>,
) {
    let file_name = Path::new(rel)
        .file_name()
        .and_then(|v| v.to_str())
        .unwrap_or_default();
    let Some(spec_id) = spec_id_from_file_name(file_name) else {
        violations.push(format!(
            "{rel} has invalid RIPR-SPEC filename [check-spec-format]"
        ));
        return;
    };
    if !seen.insert(spec_id.clone()) {
        violations.push(format!(
            "{rel} duplicates spec id {spec_id} [check-spec-numbering]"
        ));
    }
    if !text
        .lines()
        .next()
        .unwrap_or_default()
        .starts_with(&format!("# {spec_id}: "))
    {
        violations.push(format!(
            "{rel}:1 title must start with `# {spec_id}: ` [check-spec-format]"
        ));
    }
    match text
        .lines()
        .find_map(|l| l.strip_prefix("Status: "))
        .map(str::trim)
    {
        Some("proposed" | "planned" | "accepted" | "deprecated") => {}
        Some(value) => violations.push(format!(
            "{rel} has invalid status `{value}` [check-spec-format]"
        )),
        None => violations.push(format!(
            "{rel} is missing `Status: ...` [check-spec-format]"
        )),
    }
    for heading in REQUIRED_SPEC_HEADINGS {
        if !text.lines().any(|l| l.trim_end() == *heading) {
            violations.push(format!("{rel} is missing `{heading}` [check-spec-format]"));
        }
    }
    // Index membership, only when an index surface exists in this root.
    let index_file = root.join(format!("docs/spec-index.d/{spec_id}.md"));
    let readme = root.join("docs/specs/README.md");
    if (index_file.exists() || readme.exists())
        && !index_file.exists()
        && !std::fs::read_to_string(&readme)
            .map(|t| t.contains(&spec_id))
            .unwrap_or(false)
    {
        violations.push(format!(
            "{rel} spec {spec_id} has no docs/spec-index.d/{spec_id}.md row [check-spec-numbering]"
        ));
    }
}

fn check_changelog_fragment(rel: &str, text: &str, violations: &mut Vec<String>) {
    let first = text.lines().next().unwrap_or_default();
    if !(first.starts_with("<!-- section:") && first.ends_with("-->")) {
        violations.push(format!(
            "{rel}:1 first line must be a `<!-- section: ... -->` marker [check-changelog-fragments]"
        ));
    }
    if !text.lines().any(|l| l.starts_with("- ")) {
        violations.push(format!(
            "{rel} has no `- ` changelog bullet [check-changelog-fragments]"
        ));
    }
}

/// The ledger is TOML in `xtask` (`toml::from_str` into a serde struct);
/// parsing it faithfully is why `repo-policy` depends on `toml`.
fn check_disposition(root: &Path, violations: &mut Vec<String>) -> Result<(), String> {
    let rel = "policy/ripr-agent-disposition.toml";
    let text =
        std::fs::read_to_string(root.join(rel)).map_err(|e| format!("{rel}: cannot read: {e}"))?;
    let value: toml::Value = toml::from_str(&text).map_err(|e| format!("parse {rel}: {e}"))?;
    if value.get("schema_version").and_then(toml::Value::as_str) != Some("1") {
        violations.push(format!(
            "{rel} schema_version must be \"1\" [check-ripr-agent-disposition]"
        ));
    }
    let mut ids = BTreeSet::new();
    if let Some(elements) = value.get("element").and_then(toml::Value::as_array) {
        for element in elements {
            let id = element
                .get("id")
                .and_then(toml::Value::as_str)
                .unwrap_or_default();
            if id.is_empty() || !ids.insert(id.to_string()) {
                violations.push(format!(
                    "{rel} duplicate or empty element id `{id}` [check-ripr-agent-disposition]"
                ));
            }
            let disposition = element
                .get("disposition")
                .and_then(toml::Value::as_str)
                .unwrap_or_default();
            if !ALLOWED_DISPOSITIONS.contains(&disposition) {
                violations.push(format!(
                    "{rel} element `{id}` has unallowed disposition `{disposition}` [check-ripr-agent-disposition]"
                ));
            }
        }
    }
    Ok(())
}

/// Run the scoped product-free gates for a docs-eligible change set.
/// Returns `Err` (widen/refusal) before running any gate when the plan
/// widens, and `Err` listing violations when a scoped gate fails.
pub fn run_docs_gate(root: &Path, changed: &[String]) -> Result<DocsGateReport, String> {
    let plan = plan(changed);
    if !plan.is_docs_eligible() {
        return Ok(DocsGateReport {
            plan,
            violations: Vec::new(),
        });
    }
    let mut violations = Vec::new();
    let mut seen_specs = BTreeSet::new();
    for raw in changed {
        let rel = normalize(raw);
        if rel.starts_with("badges/") && rel.ends_with(".json") {
            violations.push(format!(
                "{rel}: generated badge endpoint changed in an ordinary PR [check-generated-clean]"
            ));
            continue;
        }
        if rel.contains("/target/") || rel.starts_with("target/") {
            violations.push(format!(
                "{rel}: generated target artifact in diff [check-generated-clean]"
            ));
            continue;
        }
        if rel == "policy/ripr-agent-disposition.toml" {
            check_disposition(root, &mut violations)?;
            continue;
        }
        if !rel.ends_with(".md") && !rel.starts_with("changelog.d/") {
            continue;
        }
        let Some(text) = read_text(root, &rel, &mut violations) else {
            continue;
        };
        check_static_language(&rel, &text, &mut violations);
        if rel.ends_with(".md") {
            check_markdown_links(root, &rel, &text, &mut violations);
        }
        if rel.starts_with("docs/specs/") && rel.ends_with(".md") {
            check_spec(root, &rel, &text, &mut violations, &mut seen_specs);
        }
        if rel.starts_with("changelog.d/") {
            check_changelog_fragment(&rel, &text, &mut violations);
        }
    }
    Ok(DocsGateReport { plan, violations })
}

/// CLI entry: `cargo policy docs-gate --base <sha> --head <sha>` or
/// `cargo policy docs-gate --changed <path>...` (repeatable / comma list
/// via repeated flags). Exit via `Err` on widen or violations.
pub fn command(args: &[String]) -> Result<(), String> {
    let changed = changed_files(args)?;
    let report = run_docs_gate(Path::new("."), &changed)?;
    println!("{}", report.render());
    if !report.plan.is_docs_eligible() {
        return Err(format!(
            "docs-gate: widening to full proof ({} reason(s)); run cargo xtask precommit",
            report.plan.widen_reasons.len()
        ));
    }
    if report.violations.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "docs-gate: {} violation(s)",
            report.violations.len()
        ))
    }
}

fn changed_files(args: &[String]) -> Result<Vec<String>, String> {
    let mut changed = Vec::new();
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--changed" => {
                let value = iter.next().ok_or("docs-gate: --changed needs a path")?;
                changed.push(value.clone());
            }
            "--base" => {
                let base = iter.next().ok_or("docs-gate: --base needs a sha")?.clone();
                let head = match (iter.next(), iter.next()) {
                    (Some(flag), Some(value)) if flag == "--head" => value.clone(),
                    _ => return Err("docs-gate: --base must be followed by --head <sha>".into()),
                };
                return git_changed_files(&base, &head);
            }
            other if !other.starts_with('-') => changed.push(other.to_string()),
            other => {
                return Err(format!(
                    "docs-gate: unknown argument `{other}`; usage: cargo policy docs-gate (--changed <path>... | --base <sha> --head <sha>)"
                ));
            }
        }
    }
    Ok(changed)
}

fn git_changed_files(base: &str, head: &str) -> Result<Vec<String>, String> {
    // Fail closed on any git error: an unavailable diff widens, never passes.
    let output = std::process::Command::new("git")
        .args([
            "diff",
            "--no-renames",
            "--name-only",
            "-z",
            &format!("{base}...{head}"),
        ])
        .output()
        .map_err(|e| format!("docs-gate: cannot run git diff: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "docs-gate: git diff {base}...{head} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(output
        .stdout
        .split(|b| *b == 0)
        .filter(|chunk| !chunk.is_empty())
        .map(|chunk| String::from_utf8_lossy(chunk).into_owned())
        .collect())
}
