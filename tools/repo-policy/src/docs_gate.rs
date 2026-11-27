//! Product-free docs gate (issue #14644, Leaf A).
//!
//! Runs the verified product-free subset of the precommit docs/spec gates
//! scoped to a changed-file list, without building `ripr` or `xtask`.
//! Classification mirrors the `detect-docs-only` allowlist in
//! `.github/workflows/routed-rust.yml` and fails closed: an empty or
//! unavailable diff, a manifest/lockfile/release surface, a capability
//! registry document, changed release-note command inputs, or any unrecognized path widens to full proof
//! (reported as a refusal, never as a pass).
//!
//! Retained product obligations NOT covered by this gate (run the full
//! `cargo xtask precommit` / docs-gate workflow for these):
//! `check-file-policy`, `check-covered-by`, `check-capabilities`,
//! `check-identity-registry`, `capability-docs --check`,
//! `check-capability-producers`, goldens, and the not-yet-ported
//! product-free gates listed in [`RETAINED_PRODUCT_FREE_GATES`].

/// Pure public-guide contract helpers shared with the portable candidate help harness.
#[path = "../../../crates/ripr/tests/common/help_contract.rs"]
pub mod guide_help_contract;

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

/// Paths whose owning checks are retained product-free gates: a change
/// touching them widens instead of passing on the scoped subset.
fn retained_input_gate(path: &str) -> Option<&'static str> {
    if path.starts_with("docs/spec-index.d/") {
        Some("check-doc-index")
    } else if path.starts_with("policy/doc-artifacts.d/") {
        Some("check-doc-artifacts")
    } else if path.starts_with(".ripr/traceability.d/") {
        Some("check-traceability")
    } else {
        None
    }
}

fn is_docs_path(path: &str) -> bool {
    path == "policy/ripr-agent-disposition.toml"
        || path.ends_with(".md")
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
        if let Some(gate) = retained_input_gate(&path) {
            result.widen_reasons.push(format!(
                "{path}: input of retained gate {gate}; run full proof"
            ));
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
        result.gates.push("check-doc-roles");
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

fn check_encoding(rel: &str, bytes: &[u8], violations: &mut Vec<String>) {
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        violations.push(format!(
            "{rel} starts with a UTF-8 BOM (EF BB BF); save as UTF-8 without BOM [check-text-encoding]"
        ));
    }
}

fn read_text(root: &Path, rel: &str, violations: &mut Vec<String>) -> Option<String> {
    // Encoding is checked once by the caller for every scoped candidate;
    // checking here again would report a BOM twice for the same file.
    match std::fs::read(root.join(rel)) {
        Ok(bytes) => Some(String::from_utf8_lossy(&bytes).into_owned()),
        Err(error) => {
            violations.push(format!("{rel}: cannot read changed file: {error}"));
            None
        }
    }
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct StaticAllowlist {
    schema_version: u32,
    #[serde(default)]
    allow: Vec<StaticAllow>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct StaticAllow {
    path: Option<String>,
    glob: Option<String>,
    owner: String,
    reason: String,
}

/// Whole-file exceptions from `.ripr/static-language-allowlist.toml`:
/// exact paths plus the documentation globs the allowlist itself declares.
/// The file is parsed as TOML with the canonical validation shape: every
/// entry must declare exactly one matcher (`path` or `glob`) with a
/// nonempty owner and reason, and the only supported globs are `docs/*.md`
/// and `docs/**/*.md`. A missing or malformed allowlist grants nothing
/// (fail closed to scanning).
fn static_language_allowed(root: &Path, rel: &str) -> bool {
    let Ok(text) = std::fs::read_to_string(root.join(".ripr/static-language-allowlist.toml"))
    else {
        return false;
    };
    let Ok(parsed) = toml::from_str::<StaticAllowlist>(&text) else {
        return false;
    };
    if parsed.schema_version != 1 {
        return false;
    }
    let mut allowed_paths: Vec<&str> = Vec::new();
    let mut allowed_globs: Vec<&str> = Vec::new();
    for entry in &parsed.allow {
        let well_formed = matches!(
            (entry.path.as_deref(), entry.glob.as_deref()),
            (Some(_), None) | (None, Some(_))
        ) && !entry.owner.trim().is_empty()
            && !entry.reason.trim().is_empty();
        if !well_formed {
            return false;
        }
        if let Some(path) = entry.path.as_deref() {
            allowed_paths.push(path);
        }
        if let Some(glob) = entry.glob.as_deref() {
            if glob != "docs/*.md" && glob != "docs/**/*.md" {
                return false;
            }
            allowed_globs.push(glob);
        }
    }
    if allowed_paths.contains(&rel) {
        return true;
    }
    let docs_depth = rel
        .strip_prefix("docs/")
        .map(|rest| rest.matches('/').count());
    allowed_globs.iter().any(|glob| match *glob {
        "docs/*.md" => docs_depth == Some(0) && rel.ends_with(".md"),
        "docs/**/*.md" => docs_depth.is_some() && rel.ends_with(".md"),
        _ => false,
    })
}

fn contains_word(lower: &str, term: &str) -> bool {
    lower
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .any(|word| word == term)
}

fn check_static_language(rel: &str, text: &str, violations: &mut Vec<String>) {
    for (index, line) in text.lines().enumerate() {
        let lower = line.to_ascii_lowercase();
        // An inline allow suppresses only with a nonempty reason, matching
        // the canonical recognizer; a bare marker suppresses nothing.
        if lower
            .split("ripr-allow: static-language:")
            .nth(1)
            .is_some_and(|reason| !reason.trim().is_empty())
        {
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

// Preserve the live outgoing-link contract. Revision fallback compares the
// separate canonical full-gate anchors offered to unchanged referrers.
/// GitHub heading slugs for one document, excluding fenced headings, with
/// `-1`, `-2` suffixes for repeated headings.
fn outgoing_heading_slugs(text: &str) -> BTreeSet<String> {
    let mut slugs = BTreeSet::new();
    let mut counts: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    let mut fenced = false;
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            fenced = !fenced;
            continue;
        }
        if fenced {
            continue;
        }
        let Some(heading) = line.trim_start().strip_prefix('#') else {
            continue;
        };
        let heading = heading.trim_start_matches('#').trim();
        if heading.is_empty() {
            continue;
        }
        let mut slug = String::new();
        for ch in heading.chars() {
            if ch.is_alphanumeric() || ch == '-' || ch == '_' {
                slug.push(ch.to_ascii_lowercase());
            } else if ch.is_whitespace() {
                slug.push('-');
            }
        }
        let seen = counts.entry(slug.clone()).or_default();
        if *seen == 0 {
            slugs.insert(slug);
        } else {
            slugs.insert(format!("{slug}-{seen}"));
        }
        *seen += 1;
    }
    slugs
}

fn check_markdown_links(root: &Path, rel: &str, text: &str, violations: &mut Vec<String>) {
    let base = Path::new(rel).parent().map(Path::to_path_buf);
    let source_slugs = outgoing_heading_slugs(text);
    let mut fenced = false;
    for (index, line) in text.lines().enumerate() {
        if line.trim_start().starts_with("```") {
            fenced = !fenced;
            continue;
        }
        if fenced {
            continue;
        }
        let mut rest = line;
        while let Some(start) = rest.find("](") {
            let after = &rest[start + 2..];
            let Some(end) = after.find(')') else { break };
            let raw = after[..end].trim();
            rest = &after[end + 1..];
            // Angle-bracket destinations may contain spaces; a bare
            // destination ends at whitespace (an optional title follows).
            let target = if let Some(inner) = raw
                .strip_prefix('<')
                .and_then(|inner| inner.split('>').next())
                .filter(|_| raw.starts_with('<'))
            {
                inner
            } else {
                raw.split_whitespace().next().unwrap_or_default()
            };
            if target.is_empty()
                || target.starts_with("http://")
                || target.starts_with("https://")
                || target.starts_with("mailto:")
            {
                continue;
            }
            let (file_part, fragment) = match target.split_once('#') {
                Some((file, fragment)) => (file, Some(fragment)),
                None => (target, None),
            };
            if file_part.is_empty() {
                // A fragment-only link names a heading in this document.
                if let Some(fragment) = fragment
                    && !fragment.is_empty()
                    && !source_slugs.contains(fragment)
                {
                    violations.push(format!(
                        "{rel}:{} link fragment #{fragment} has no heading in this document [markdown-links]",
                        index + 1
                    ));
                }
                continue;
            }
            let decoded = file_part.replace("%20", " ");
            // A root-relative target names a path from the repository root,
            // as GitHub resolves it; joining it as-is would replace the base
            // with the filesystem root.
            let candidate: PathBuf = if let Some(stripped) = decoded.strip_prefix('/') {
                root.join(stripped.trim_start_matches('/'))
            } else {
                match &base {
                    Some(dir) if !dir.as_os_str().is_empty() => root.join(dir).join(&decoded),
                    _ => root.join(&decoded),
                }
            };
            if !candidate.exists() {
                violations.push(format!(
                    "{rel}:{} relative link target does not exist: {target} [markdown-links]",
                    index + 1
                ));
                continue;
            }
            if let Some(fragment) = fragment
                && !fragment.is_empty()
                && candidate
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
                && let Ok(target_text) = std::fs::read_to_string(&candidate)
                && !outgoing_heading_slugs(&target_text).contains(fragment)
            {
                violations.push(format!(
                    "{rel}:{} link fragment #{fragment} has no heading in {file_part} [markdown-links]",
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

/// Every spec id present under `docs/specs/`, mapped to its file names, so
/// duplicate detection and index membership see the whole inventory and
/// not only the changed files.
fn spec_inventory(root: &Path) -> std::collections::BTreeMap<String, Vec<String>> {
    let mut inventory: std::collections::BTreeMap<String, Vec<String>> = Default::default();
    if let Ok(entries) = std::fs::read_dir(root.join("docs/specs")) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if let Some(id) = spec_id_from_file_name(&name) {
                inventory.entry(id).or_default().push(name);
            }
        }
    }
    inventory
}

fn check_spec(
    root: &Path,
    rel: &str,
    text: &str,
    violations: &mut Vec<String>,
    inventory: &std::collections::BTreeMap<String, Vec<String>>,
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
    if inventory.get(&spec_id).is_some_and(|files| files.len() > 1) {
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
    if text.lines().any(|line| {
        let trimmed = line.trim();
        trimmed == "-" || trimmed == "- " || trimmed.starts_with("- [ ]") && trimmed.len() <= 5
    }) {
        violations.push(format!(
            "{rel} has an empty placeholder list item [check-spec-format]"
        ));
    }
    // Index membership: an index fragment for this spec, or a README index
    // row linking this exact file (a bare mention of the id is not a row).
    let index_file = root.join(format!("docs/spec-index.d/{spec_id}.md"));
    let readme = root.join("docs/specs/README.md");
    if (index_file.exists() || readme.exists()) && !index_file.exists() {
        let linked = std::fs::read_to_string(&readme)
            .map(|t| t.contains(&format!("[{spec_id}]({file_name})")))
            .unwrap_or(false);
        if !linked {
            violations.push(format!(
                "{rel} spec {spec_id} has no docs/spec-index.d/{spec_id}.md row [check-spec-numbering]"
            ));
        }
    }
}

const ALLOWED_CHANGELOG_SECTIONS: &[&str] = &[
    "Added",
    "Changed",
    "Deprecated",
    "Removed",
    "Fixed",
    "Security",
    "Docs",
    "Internal / CI",
];

fn has_issue_reference(line: &str) -> bool {
    let bytes = line.as_bytes();
    for (index, byte) in bytes.iter().enumerate() {
        if *byte == b'#'
            && (index == 0 || matches!(bytes[index - 1], b' ' | b'(' | b'['))
            && bytes[index + 1..]
                .iter()
                .take_while(|b| b.is_ascii_digit())
                .count()
                > 0
        {
            return true;
        }
    }
    line.contains("/issues/") || line.contains("/pull/")
}

/// Mirrors the canonical fragment validator in
/// `xtask/src/policy/changelog_fragments.rs` for one changed fragment.
fn check_changelog_fragment(rel: &str, text: &str, violations: &mut Vec<String>) {
    let file_name = Path::new(rel)
        .file_name()
        .and_then(|v| v.to_str())
        .unwrap_or_default();
    if file_name != "README.md" {
        match file_name.strip_suffix(".md") {
            None => violations.push(format!(
                "{rel}: fragments must be `.md` files [check-changelog-fragments]"
            )),
            Some(stem) => {
                let kebab = !stem.is_empty()
                    && stem.split('-').all(|part| {
                        !part.is_empty()
                            && part
                                .bytes()
                                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
                    });
                if !kebab {
                    violations.push(format!(
                        "{rel}: name must be lowercase kebab case [check-changelog-fragments]"
                    ));
                } else if stem.bytes().all(|b| b.is_ascii_digit() || b == b'-') {
                    violations.push(format!(
                        "{rel}: name needs a slug, not just a number [check-changelog-fragments]"
                    ));
                }
            }
        }
    }
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut lines = text.lines();
    let first = lines.next().unwrap_or_default();
    let section = first
        .trim()
        .strip_prefix("<!--")
        .and_then(|v| v.strip_suffix("-->"))
        .map(str::trim)
        .and_then(|v| v.strip_prefix("section:"))
        .map(str::trim);
    match section {
        Some(section) if ALLOWED_CHANGELOG_SECTIONS.contains(&section) => {}
        Some(section) => violations.push(format!(
            "{rel}: unknown section `{section}` [check-changelog-fragments]"
        )),
        None => violations.push(format!(
            "{rel}:1 first line must be `<!-- section: X -->` [check-changelog-fragments]"
        )),
    }
    let body: Vec<&str> = lines.collect();
    if body.iter().any(|line| line.contains("<!--")) {
        violations.push(format!(
            "{rel}: only the first line may hold an HTML comment [check-changelog-fragments]"
        ));
    }
    let mut entries: Vec<&str> = Vec::new();
    for line in &body {
        if line.trim().is_empty() || line.trim_start().starts_with("<!--") {
            continue;
        }
        if line.starts_with(char::is_whitespace) {
            continue;
        }
        if !line.starts_with("- ") {
            violations.push(format!(
                "{rel}: entry must start with a `- ` bullet at column 0 [check-changelog-fragments]"
            ));
        } else if line[2..].trim().is_empty() {
            violations.push(format!(
                "{rel}: the bullet line has no entry text [check-changelog-fragments]"
            ));
        }
        entries.push(line);
    }
    if entries.is_empty() {
        violations.push(format!("{rel}: entry is empty [check-changelog-fragments]"));
    }
    for entry in entries {
        if !has_issue_reference(entry) {
            violations.push(format!(
                "{rel}: entry `{}` names no issue or PR [check-changelog-fragments]",
                entry.trim()
            ));
        }
        if is_internal_bookkeeping_line(entry) && section != Some("Internal / CI") {
            violations.push(format!(
                "{rel}: entry `{}` is bookkeeping; use the Internal / CI section [check-changelog-fragments]",
                entry.trim()
            ));
        }
    }
}

/// Bookkeeping entries that belong under `Internal / CI`, ported from the
/// canonical `is_internal_bookkeeping_line` in
/// `xtask/src/policy/changelog_fragments.rs`.
fn is_internal_bookkeeping_line(line: &str) -> bool {
    let text = line.strip_prefix("- ").unwrap_or(line).trim_start();
    text.starts_with("Verdict corpus:")
        || text.starts_with("Repo ops:")
        || text.starts_with("Labeled Rust verdict corpus")
        || opens_with_maintainer_path(text)
}

/// Maintainer-tooling subjects: an entry whose first token is a backticked
/// `cargo xtask` command, workflow file, `policy/` file, agent instruction
/// file, or maintainer doc (canonical `opens_with_maintainer_path`).
fn opens_with_maintainer_path(text: &str) -> bool {
    const MAINTAINER_PREFIXES: &[&str] = &[
        "cargo xtask",
        ".github/",
        "policy/",
        "AGENTS.md",
        "CLAUDE.md",
        "docs/CI.md",
        "docs/CLIPPY_POLICY.md",
        "docs/IMPLEMENTATION_PLAN.md",
        "docs/MERGE_FLOW.md",
        "docs/POLICY_ALLOWLISTS.md",
    ];
    let Some(rest) = text.strip_prefix('`') else {
        return false;
    };
    let Some(token) = rest.split('`').next() else {
        return false;
    };
    MAINTAINER_PREFIXES
        .iter()
        .any(|prefix| token.starts_with(prefix))
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
    // The canonical loader deserializes `element` into `Vec<Element>`, so a
    // present-but-non-array value is a parse error there, not a skip here.
    if value
        .get("element")
        .is_some_and(|element| element.as_array().is_none())
    {
        violations.push(format!(
            "{rel} element must be an array of tables [check-ripr-agent-disposition]"
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
    if changed.iter().any(|path| release_notes(&normalize(path))) {
        return Ok(widen_report(changed, vec![
            "release-note CLI contract requires committed before/after inputs; use --base and --head or run the protected docs gate including cargo test -p xtask --test ripr_public_guide_contract".into(),
        ]));
    }
    check_scoped_tree(root, changed)
}

fn check_scoped_tree(root: &Path, changed: &[String]) -> Result<DocsGateReport, String> {
    let plan = plan(changed);
    if !plan.is_docs_eligible() {
        return Ok(DocsGateReport {
            plan,
            violations: Vec::new(),
        });
    }
    let mut violations = crate::doc_roles::staged_violations(root)?;
    let inventory = spec_inventory(root);
    for raw in changed {
        let rel = normalize(raw);
        // Deleted or renamed-away files name no content to check.
        if !root.join(&rel).exists() {
            continue;
        }
        // Encoding is checked for every scoped text candidate, not only
        // the Markdown/fragment paths that go on to read_text.
        if (rel.ends_with(".md")
            || rel.ends_with(".toml")
            || rel.ends_with(".json")
            || rel.starts_with("changelog.d/"))
            && let Ok(bytes) = std::fs::read(root.join(&rel))
        {
            check_encoding(&rel, &bytes, &mut violations);
        }
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
        if !static_language_allowed(root, &rel) {
            check_static_language(&rel, &text, &mut violations);
        }
        if rel.ends_with(".md") {
            check_markdown_links(root, &rel, &text, &mut violations);
        }
        // Directory READMEs are indexes, not spec records or fragments.
        if rel.starts_with("docs/specs/") && rel.ends_with(".md") && rel != "docs/specs/README.md" {
            check_spec(root, &rel, &text, &mut violations, &inventory);
        }
        if rel.starts_with("changelog.d/") && rel != "changelog.d/README.md" {
            check_changelog_fragment(&rel, &text, &mut violations);
        }
    }
    Ok(DocsGateReport { plan, violations })
}

/// Revision mode: classify the `base...head` diff in `checkout`, then run
/// the scoped gates against the committed tree of `head`, materialized
/// from `git archive`. Working-file bytes are never read in this mode, so
/// a dirty or mismatched working tree cannot produce a pass for the wrong
/// content; `--changed` remains the deliberate live-worktree mode.
pub fn run_docs_gate_at_revision(
    checkout: &Path,
    base: &str,
    head: &str,
) -> Result<DocsGateReport, String> {
    // Resolve both revs to commit IDs once and use only those IDs afterwards.
    // The checkout check, the diff selection and the archive therefore bind to
    // the same two commits: a ref that moves after the check can no longer make
    // the gate classify one commit's diff and read another commit's tree.
    let base_id = resolve_commit(checkout, base)?;
    let head_id = resolve_commit(checkout, head)?;
    verify_checkout_at(checkout, &head_id, head)?;
    let (changed, risks) = revision_changes(checkout, &base_id, &head_id)?;
    if !risks.is_empty() {
        return Ok(widen_report(&changed, risks));
    }
    let tree = match materialize_tree(checkout, &head_id) {
        Ok(tree) => tree,
        Err(reason) => return Ok(widen_report(&changed, vec![reason])),
    };
    let result = verify_archived_roles(checkout, &head_id, &tree).map_or_else(
        |reason| Ok(widen_report(&changed, vec![reason])),
        |()| check_scoped_tree(&tree, &changed),
    );
    let _ = std::fs::remove_dir_all(&tree);
    result
}

/// Git archive honors export-ignore and export-subst. The whole-tree role check
/// can discharge its obligation only when the role files have their Git bytes.
fn verify_archived_roles(checkout: &Path, head: &str, tree: &Path) -> Result<(), String> {
    let listing = offline_git()
        .args(["ls-tree", "-rz", head, "--", "docs"])
        .current_dir(checkout)
        .output()
        .map_err(|error| format!("cannot list committed role inputs: {error}"))?;
    if !listing.status.success() {
        return Err("cannot establish the committed role inputs".into());
    }
    let mut files = Vec::new();
    let mut expected = Vec::new();
    for record in listing
        .stdout
        .split(|byte| *byte == 0)
        .filter(|r| !r.is_empty())
    {
        let tab = record
            .iter()
            .position(|byte| *byte == b'\t')
            .ok_or("malformed git ls-tree role record")?;
        let (header, path) = record.split_at(tab);
        let path = path
            .strip_prefix(b"\t")
            .ok_or("missing git ls-tree role path")?;
        if !(path == b"docs"
            || path == b"docs/adr"
            || path == b"docs/proposals"
            || path.starts_with(b"docs/adr/")
            || path.starts_with(b"docs/proposals/"))
        {
            continue;
        }
        let header = std::str::from_utf8(header).map_err(|error| error.to_string())?;
        let path = std::str::from_utf8(path).map_err(|error| error.to_string())?;
        let fields: Vec<_> = header.split_ascii_whitespace().collect();
        let ["100644" | "100755", "blob", oid] = fields.as_slice() else {
            return Err(format!(
                "non-regular document-role input {path} requires full proof"
            ));
        };
        if path.contains(['\\', ':']) {
            return Err(format!(
                "ambiguous document-role path {path:?} requires full proof"
            ));
        }
        if crate::doc_roles::required_sections(Path::new(path)).is_none() {
            continue;
        }
        files.push(tree.join(path));
        expected.push(*oid);
    }
    if files.is_empty() {
        return Ok(());
    }
    let hashes = offline_git()
        .args(["hash-object", "--no-filters", "--"])
        .args(files)
        .current_dir(checkout)
        .output()
        .map_err(|error| format!("cannot verify archived role bytes: {error}"))?;
    if !hashes.status.success() || !String::from_utf8_lossy(&hashes.stdout).lines().eq(expected) {
        return Err("archived role inputs differ from the commit; full proof required".into());
    }
    Ok(())
}

fn widen_report(changed: &[String], reasons: Vec<String>) -> DocsGateReport {
    let mut plan = plan(changed);
    plan.gates.clear();
    plan.widen_reasons.extend(reasons);
    DocsGateReport {
        plan,
        violations: Vec::new(),
    }
}

/// Newer Git honors the no-lazy-fetch setting. Older Git may start a fetch
/// subprocess, so deny every transport as well. Both settings are local to
/// this child and leave the user's repository and Git configuration untouched.
fn offline_git() -> std::process::Command {
    let mut git = std::process::Command::new("git");
    git.env("GIT_NO_LAZY_FETCH", "1")
        .env("GIT_ALLOW_PROTOCOL", "");
    git
}

fn materialize_tree(checkout: &Path, head_id: &str) -> Result<PathBuf, String> {
    let tree = create_staging_dir(head_id)?;
    // `git archive` applies checkout end-of-line conversion. With
    // `core.autocrlf=true` (the Git for Windows and hosted Windows runner
    // default) or a native CRLF `core.eol`, every text file would arrive with
    // CRLF and no archived role input could match its blob, so the gate
    // widened on every Windows checkout (#16105). Pin both settings for this
    // child only; an in-tree `eol`/`text` attribute still converts, and
    // `verify_archived_roles` then widens instead of checking other bytes.
    let mut archive = offline_git()
        .args(["-c", "core.autocrlf=false", "-c", "core.eol=lf"])
        .args(["archive", "--format=tar", head_id])
        .current_dir(checkout)
        .stdout(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("docs-gate: cannot run git archive: {e}"))?;
    let archive_out = archive
        .stdout
        .take()
        .ok_or("docs-gate: git archive produced no stdout")?;
    let extract = std::process::Command::new("tar")
        .args(["-x", "-C"])
        .arg(&tree)
        .stdin(archive_out)
        .output()
        .map_err(|e| format!("docs-gate: cannot run tar extract: {e}"))?;
    let archive_status = archive
        .wait()
        .map_err(|e| format!("docs-gate: cannot wait for git archive: {e}"))?;
    if !archive_status.success() || !extract.status.success() {
        let _ = std::fs::remove_dir_all(&tree);
        return Err(format!(
            "docs-gate: materializing {head_id} failed (git archive: {archive_status}, tar: {})",
            String::from_utf8_lossy(&extract.stderr).trim()
        ));
    }
    Ok(tree)
}

/// Create a staging directory that this process owns.
///
/// `create_dir` fails when the path exists, so an existing path — a leftover
/// from a killed run, another process's tree, or a path a user chose — is
/// never deleted or reused. The name is built from the resolved commit ID, so
/// a caller-supplied revision string cannot shape the path.
fn create_staging_dir(head_id: &str) -> Result<PathBuf, String> {
    let root = std::env::temp_dir();
    let stem = format!(
        "ripr-docs-gate-tree-{}-{}",
        std::process::id(),
        head_id.chars().take(12).collect::<String>()
    );
    for attempt in 0..64u32 {
        let candidate = root.join(format!("{stem}-{attempt}"));
        match std::fs::create_dir(&candidate) {
            Ok(()) => return Ok(candidate),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(format!(
                    "docs-gate: cannot create {}: {error}",
                    candidate.display()
                ));
            }
        }
    }
    Err(format!(
        "docs-gate: cannot create a staging directory for {head_id}: 64 candidates already exist"
    ))
}

/// CLI entry: `cargo policy docs-gate --base <sha> --head <sha>` or
/// `cargo policy docs-gate --changed <path>...` (repeatable / comma list
/// via repeated flags). Exit via `Err` on widen or violations.
pub fn command(args: &[String]) -> Result<(), String> {
    let report = match parse_mode(args)? {
        CommandMode::Changed(changed) => {
            eprintln!(
                "docs-gate: live paths check outgoing links only; use --base and --head to check for removed Markdown destinations and anchors"
            );
            run_docs_gate(Path::new("."), &changed)?
        }
        CommandMode::Revision { base, head } => {
            run_docs_gate_at_revision(Path::new("."), &base, &head)?
        }
    };
    println!("{}", report.render());
    if !report.plan.is_docs_eligible() {
        return Err(format!(
            "docs-gate: widening to full proof ({} reason(s)); run the protected docs gate, including cargo xtask precommit and its Markdown contract tests",
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

enum CommandMode {
    Changed(Vec<String>),
    Revision { base: String, head: String },
}

/// Parse every argument first: mixing the explicit-path mode with the
/// `--base`/`--head` mode, or trailing arguments after the pair, must
/// refuse rather than silently discarding part of the request.
fn parse_mode(args: &[String]) -> Result<CommandMode, String> {
    let mut changed = Vec::new();
    let mut base: Option<String> = None;
    let mut head: Option<String> = None;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--changed" => {
                let value = iter.next().ok_or("docs-gate: --changed needs a path")?;
                changed.push(value.clone());
            }
            "--base" => {
                if base.is_some() {
                    return Err("docs-gate: --base given twice".into());
                }
                base = Some(iter.next().ok_or("docs-gate: --base needs a sha")?.clone());
            }
            "--head" => {
                if head.is_some() {
                    return Err("docs-gate: --head given twice".into());
                }
                head = Some(iter.next().ok_or("docs-gate: --head needs a sha")?.clone());
            }
            other if !other.starts_with('-') => changed.push(other.to_string()),
            other => {
                return Err(format!(
                    "docs-gate: unknown argument `{other}`; usage: cargo policy docs-gate (--changed <path>... | --base <sha> --head <sha>)"
                ));
            }
        }
    }
    match (base, head) {
        (None, None) => Ok(CommandMode::Changed(changed)),
        (Some(base), Some(head)) => {
            if !changed.is_empty() {
                return Err("docs-gate: --changed paths cannot be mixed with --base/--head".into());
            }
            Ok(CommandMode::Revision { base, head })
        }
        _ => Err("docs-gate: --base and --head must be given together".into()),
    }
}

#[cfg(test)]
fn changed_files(args: &[String]) -> Result<Vec<String>, String> {
    match parse_mode(args)? {
        CommandMode::Changed(changed) => Ok(changed),
        CommandMode::Revision { base, head } => {
            let head_id = resolve_commit(Path::new("."), &head)?;
            verify_checkout_at(Path::new("."), &head_id, &head)?;
            revision_changes(Path::new("."), &base, &head).map(|(paths, _)| paths)
        }
    }
}

/// Resolve a revision to the full commit ID. Every revision the gate acts on is
/// resolved once here and used by ID afterwards, so a symbolic or mutable name
/// cannot denote different commits at selection time and at read time.
fn resolve_commit(checkout: &Path, rev: &str) -> Result<String, String> {
    let output = offline_git()
        .args(["rev-parse", &format!("{rev}^{{commit}}")])
        .current_dir(checkout)
        .output()
        .map_err(|e| format!("docs-gate: cannot run git rev-parse: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "docs-gate: git rev-parse {rev} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let resolved = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if resolved.is_empty() {
        return Err(format!("docs-gate: git rev-parse {rev} returned no commit"));
    }
    Ok(resolved)
}

/// The gate reads file contents from the committed tree of `--head`, and
/// the checkout is the git authority for that tree, so the checkout must
/// be at `--head`; otherwise the requested subject is ambiguous. `head_id` is
/// the already-resolved commit, so this compares IDs and never re-resolves the
/// caller's ref.
fn verify_checkout_at(checkout: &Path, head_id: &str, head_label: &str) -> Result<(), String> {
    let checkout_sha = resolve_commit(checkout, "HEAD")?;
    if checkout_sha != head_id {
        return Err(format!(
            "docs-gate: checkout is at {checkout_sha}, not --head {head_label} ({head_id}); check out the head commit first"
        ));
    }
    Ok(())
}

fn release_notes(path: &str) -> bool {
    path.starts_with("docs/releases/") && path.ends_with("-release-notes.md")
}

/// The line number locates a failure in prose. Only command paths and flags
/// reach the public-guide contract's candidate help lookup.
fn same_documented_commands(before: &str, after: &str) -> bool {
    fn commands(text: &str) -> impl Iterator<Item = (Vec<String>, Vec<String>)> {
        guide_help_contract::doc_invocations(text)
            .into_iter()
            .map(|invocation| (invocation.words, invocation.flags))
    }
    // Bare commands and bracketed option examples can be absent from the
    // flag oracle. Retain their source lines (and shell continuations) as
    // opaque inputs so an unrecognized spelling can only widen the route.
    fn potential_command_lines(text: &str) -> impl Iterator<Item = &str> {
        let mut continued = false;
        text.lines().filter(move |line| {
            let selected = continued
                || line.as_bytes().windows(5).any(|span| {
                    span.eq_ignore_ascii_case(b"ripr ") || span.eq_ignore_ascii_case(b"ripr\t")
                });
            continued = selected && line.trim_end().ends_with('\\');
            selected
        })
    }
    commands(before).eq(commands(after))
        && potential_command_lines(before).eq(potential_command_lines(after))
}

/// Read before/after blob identities from the same three-dot diff as the path roster.
fn revision_changes(
    checkout: &Path,
    base: &str,
    head: &str,
) -> Result<(Vec<String>, Vec<String>), String> {
    // Fail closed on any git error: an unavailable diff widens, never passes.
    let output = offline_git()
        .args([
            "diff",
            "--no-renames",
            "--raw",
            "--no-abbrev",
            "-z",
            &format!("{base}...{head}"),
        ])
        .current_dir(checkout)
        .output()
        .map_err(|e| format!("docs-gate: cannot run git diff: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "docs-gate: git diff {base}...{head} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let mut paths = Vec::new();
    let mut risks = Vec::new();
    let raw = std::str::from_utf8(&output.stdout)
        .map_err(|e| format!("docs-gate: unreadable Git diff: {e}"))?;
    let mut records = raw.split_terminator('\0');
    while let Some(header) = records.next() {
        let path = records.next().ok_or("docs-gate: missing Git diff path")?;
        let fields: Vec<_> = header
            .trim_start_matches(':')
            .split_ascii_whitespace()
            .collect();
        let [old_mode, new_mode, old_blob, new_blob, status] = fields.as_slice() else {
            return Err(format!("docs-gate: unexpected Git diff record {header:?}"));
        };
        paths.push(path.to_owned());
        if normalize(path) != path {
            risks.push(format!(
                "Git path {path:?} would be normalized to another path; full proof required"
            ));
            continue;
        }
        // Unchanged referrers may name a removed destination or heading.
        match (*status, *old_mode, *new_mode) {
            ("A", "000000", "100644") if release_notes(path) => {
                match markdown_blob(checkout, new_blob) {
                    Ok(new) if !same_documented_commands("", &new) => {
                        risks.push(format!("{path}: new documented CLI example; candidate CLI proof required: cargo test -p xtask --test ripr_public_guide_contract"));
                    }
                    Err(reason) => risks.push(format!("{path}: {reason}; full proof required")),
                    Ok(_) => (),
                }
            }
            ("A", "000000", "100644") => (),
            ("M", "100644", "100644") if !path.to_ascii_lowercase().ends_with(".md") => (),
            ("M", "100644", "100644") => {
                let blobs = markdown_blob(checkout, old_blob)
                    .and_then(|old| markdown_blob(checkout, new_blob).map(|new| (old, new)));
                match blobs {
                    Ok((old, new)) => {
                        let old_headings = crate::markdown_headings::heading_slugs(&old);
                        let new_headings = crate::markdown_headings::heading_slugs(&new);
                        if let Some(missing) = old_headings.difference(&new_headings).next() {
                            risks.push(format!("{path}: removed Markdown anchor #{missing}; full incoming-link check required"));
                        }
                        if release_notes(path) && !same_documented_commands(&old, &new) {
                            risks.push(format!("{path}: documented CLI example changed; candidate CLI proof required: cargo test -p xtask --test ripr_public_guide_contract"));
                        }
                    }
                    Err(reason) => risks.push(format!("{path}: {reason}; full proof required")),
                }
            }
            _ => risks.push(format!(
                "{path}: removed destination or changed file mode; full incoming-link check required"
            )),
        }
    }
    Ok((paths, risks))
}

fn markdown_blob(checkout: &Path, oid: &str) -> Result<String, String> {
    let output = offline_git()
        .args(["cat-file", "blob", oid])
        .current_dir(checkout)
        .output()
        .map_err(|e| format!("docs-gate: cannot read Markdown blob {oid}: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "docs-gate: reading Markdown blob {oid}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    String::from_utf8(output.stdout)
        .map_err(|e| format!("docs-gate: Markdown blob {oid} is not UTF-8: {e}"))
}

#[cfg(test)]
mod tests {
    use super::{changed_files, create_staging_dir, resolve_commit, verify_checkout_at};
    use std::path::Path;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|v| (*v).to_string()).collect()
    }

    #[test]
    fn mixed_argument_modes_are_refused() {
        let mixed = changed_files(&args(&[
            "--changed",
            "Cargo.lock",
            "--base",
            "a",
            "--head",
            "b",
        ]));
        assert!(
            matches!(&mixed, Err(e) if e.contains("cannot be mixed")),
            "{mixed:?}"
        );
        let trailing = changed_files(&args(&["--base", "a", "--head", "b", "extra.md"]));
        assert!(
            matches!(&trailing, Err(e) if e.contains("cannot be mixed")),
            "{trailing:?}"
        );
    }

    #[test]
    fn base_without_head_is_refused() {
        let lone = changed_files(&args(&["--base", "a"]));
        assert!(
            matches!(&lone, Err(e) if e.contains("must be given together")),
            "{lone:?}"
        );
    }

    #[test]
    fn checkout_at_head_is_accepted_and_unknown_rev_refused() -> Result<(), String> {
        // The test process runs inside this repository checkout.
        let head_id = resolve_commit(Path::new("."), "HEAD")?;
        verify_checkout_at(Path::new("."), &head_id, "HEAD")?;
        let refused = resolve_commit(Path::new("."), "refs/heads/does-not-exist-14772");
        assert!(
            matches!(&refused, Err(e) if e.contains("rev-parse")),
            "{refused:?}"
        );
        Ok(())
    }

    #[test]
    fn resolved_head_is_a_stable_commit_id() -> Result<(), String> {
        // The gate passes IDs, not refs, to the diff and the archive. Resolving
        // the same commit twice — once through a ref, once through its own ID —
        // must yield the same subject, and the ID must be a full commit ID.
        let head_id = resolve_commit(Path::new("."), "HEAD")?;
        assert_eq!(head_id.len(), 40, "{head_id}");
        assert!(head_id.chars().all(|c| c.is_ascii_hexdigit()), "{head_id}");
        assert_eq!(resolve_commit(Path::new("."), &head_id)?, head_id);
        let checkout_sha = resolve_commit(Path::new("."), "HEAD")?;
        assert_eq!(checkout_sha, head_id);
        Ok(())
    }

    #[test]
    fn staging_dir_never_reuses_or_deletes_an_existing_path() -> Result<(), String> {
        // Occupy both the legacy unsuffixed name (which the previous
        // implementation cleared with `remove_dir_all` and reused) and the
        // first name this allocator would try. Neither may be reused or
        // deleted; the run must take a fresh directory instead.
        let head_id = resolve_commit(Path::new("."), "HEAD")?;
        let stem = format!(
            "ripr-docs-gate-tree-{}-{}",
            std::process::id(),
            head_id.chars().take(12).collect::<String>()
        );
        let legacy = std::env::temp_dir().join(&stem);
        let occupied = std::env::temp_dir().join(format!("{stem}-0"));
        let sentinels = [legacy.join("not-ours.txt"), occupied.join("not-ours.txt")];
        for (dir, sentinel) in [(&legacy, &sentinels[0]), (&occupied, &sentinels[1])] {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
            std::fs::write(sentinel, b"sentinel").map_err(|e| e.to_string())?;
        }

        let created = create_staging_dir(&head_id)?;
        let result = (|| -> Result<(), String> {
            if created == legacy || created == occupied {
                return Err("allocator reused a pre-existing path".into());
            }
            for sentinel in &sentinels {
                if !sentinel.is_file() {
                    return Err(format!(
                        "allocator deleted a pre-existing path: {}",
                        sentinel.display()
                    ));
                }
            }
            let entries = std::fs::read_dir(&created)
                .map_err(|e| e.to_string())?
                .count();
            if entries != 0 {
                return Err("allocator returned a non-empty directory".into());
            }
            Ok(())
        })();
        let _ = std::fs::remove_dir_all(&created);
        let _ = std::fs::remove_dir_all(&legacy);
        let _ = std::fs::remove_dir_all(&occupied);
        result
    }
}
