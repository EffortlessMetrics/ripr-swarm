//! Public-path metamorphic controls for the content-addressed finding contract.
#![cfg(feature = "lang-rust")]
use ripr::{CheckInput, CheckOutput, Mode, OutputFormat};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

type Rows = BTreeMap<(String, String, String), String>;

struct Fixture {
    root: PathBuf,
}
impl Fixture {
    fn new(files: &[(&str, &str)]) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let parent = std::env::temp_dir();
        std::fs::create_dir_all(&parent).map_err(|e| e.to_string())?;
        let root = parent.join(format!(
            "ripr-finding-identity-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).map_err(|e| format!("create {}: {e}", root.display()))?;
        let fixture = Self { root };
        fixture.write("Cargo.toml", "[package]\nname=\"identity_fixture\"\nversion=\"0.0.0\"\nedition=\"2024\"\n[workspace]\n")?;
        for (path, source) in files {
            fixture.write(path, source)?;
        }
        Ok(fixture)
    }
    fn write(&self, path: &str, text: &str) -> Result<(), String> {
        let path = self.root.join(path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(&path, text).map_err(|e| format!("write {}: {e}", path.display()))
    }
    fn check(&self, changes: &[(&str, &str, &str)]) -> Result<CheckOutput, String> {
        self.check_with_config(changes, &ripr::config::RiprConfig::default())
    }
    fn check_with_config(
        &self,
        changes: &[(&str, &str, &str)],
        config: &ripr::config::RiprConfig,
    ) -> Result<CheckOutput, String> {
        let mut diff = String::new();
        for (path, before, after) in changes {
            diff.push_str(&format!(
                "diff --git a/{path} b/{path}\n--- a/{path}\n+++ b/{path}\n@@ -1,{} +1,{} @@\n",
                before.lines().count(),
                after.lines().count()
            ));
            let old: Vec<_> = before.lines().collect();
            let new: Vec<_> = after.lines().collect();
            let mut common = vec![vec![0; new.len() + 1]; old.len() + 1];
            for i in (0..old.len()).rev() {
                for j in (0..new.len()).rev() {
                    common[i][j] = if old[i] == new[j] {
                        1 + common[i + 1][j + 1]
                    } else {
                        common[i + 1][j].max(common[i][j + 1])
                    };
                }
            }
            let (mut i, mut j) = (0, 0);
            while i < old.len() || j < new.len() {
                if i < old.len() && j < new.len() && old[i] == new[j] {
                    diff.push_str(&format!(" {}\n", old[i]));
                    i += 1;
                    j += 1;
                } else if i < old.len() && (j == new.len() || common[i + 1][j] >= common[i][j + 1])
                {
                    diff.push_str(&format!("-{}\n", old[i]));
                    i += 1;
                } else {
                    diff.push_str(&format!("+{}\n", new[j]));
                    j += 1;
                }
            }
        }
        self.write("change.diff", &diff)?;
        ripr::app::check_workspace_with_config(
            CheckInput {
                root: self.root.clone(),
                diff_file: Some(self.root.join("change.diff")),
                mode: Mode::Draft,
                format: OutputFormat::Json,
                suppression_policy: self
                    .root
                    .join("suppressions.toml")
                    .is_file()
                    .then(|| self.root.join("suppressions.toml")),
                ..CheckInput::default()
            },
            config,
        )
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
fn rows(output: CheckOutput) -> Rows {
    assert!(
        !output.findings.is_empty(),
        "fixture must exercise real findings"
    );
    let mut ids = BTreeSet::new();
    let mut rows = BTreeMap::new();
    for finding in output.findings {
        assert_eq!(finding.id, finding.probe.id.0);
        assert!(
            ids.insert(finding.id.clone()),
            "distinct findings share {}",
            finding.id
        );
        if !finding.source_currentness.permits_candidate_action() {
            continue;
        }
        let key = (
            finding.probe.family.as_str().to_string(),
            finding.probe.owner.map(|o| o.0).unwrap_or_default(),
            finding.probe.expression,
        );
        assert!(
            rows.insert(key, finding.id).is_none(),
            "fixture must not collapse distinct subjects"
        );
    }
    assert!(
        !rows.is_empty(),
        "fixture must contain candidate-current subjects"
    );
    rows
}
const ALPHA: &str = "pub fn alpha(x: i32) -> bool {\n    x >= 10\n}\n";
const ALPHA_BEFORE: &str = "pub fn alpha(x: i32) -> bool {\n    x > 10\n}\n";
const BETA: &str = "pub fn beta(x: i32) -> bool {\n    x <= 20\n}\n";
const BETA_BEFORE: &str = "pub fn beta(x: i32) -> bool {\n    x < 20\n}\n";

#[test]
fn finding_ids_ignore_diff_order_checkout_root_and_unrelated_files() -> Result<(), String> {
    let alpha = format!("mod other;\n{ALPHA}");
    let alpha_before = format!("mod other;\n{ALPHA_BEFORE}");
    let fixture = Fixture::new(&[("src/lib.rs", &alpha), ("src/other.rs", BETA)])?;
    let changes = [
        ("src/lib.rs", alpha_before.as_str(), alpha.as_str()),
        ("src/other.rs", BETA_BEFORE, BETA),
    ];
    let baseline = rows(fixture.check(&changes)?);
    assert!(
        baseline
            .keys()
            .any(|(_, owner, expression)| owner.ends_with("::alpha") && expression == "x >= 10")
    );
    assert!(
        baseline
            .keys()
            .any(|(_, owner, expression)| owner.ends_with("::beta") && expression == "x <= 20")
    );
    assert_eq!(baseline, rows(fixture.check(&[changes[1], changes[0]])?));
    fixture.write("src/a_unrelated.rs", "pub fn unrelated() -> usize { 42 }\n")?;
    fixture.write("src/lib.rs", &format!("{alpha}\nmod a_unrelated;\n"))?;
    assert_eq!(baseline, rows(fixture.check(&changes)?));
    let relocated = Fixture::new(&[("src/other.rs", BETA), ("src/lib.rs", &alpha)])?;
    assert_eq!(baseline, rows(relocated.check(&changes)?));
    Ok(())
}

#[test]
fn finding_ids_ignore_line_movement_and_owner_declaration_order() -> Result<(), String> {
    let original = format!("{ALPHA}{BETA}");
    let before = format!("{ALPHA_BEFORE}{BETA_BEFORE}");
    let fixture = Fixture::new(&[("src/lib.rs", &original)])?;
    let baseline = rows(fixture.check(&[("src/lib.rs", &before, &original)])?);
    let reordered = format!("\n\n{BETA}{ALPHA}");
    let reordered_before = format!("\n\n{BETA_BEFORE}{ALPHA_BEFORE}");
    fixture.write("src/lib.rs", &reordered)?;
    assert_eq!(
        baseline,
        rows(fixture.check(&[("src/lib.rs", &reordered_before, &reordered)])?)
    );
    Ok(())
}

#[test]
fn finding_ids_change_with_path_owner_and_expression() -> Result<(), String> {
    let fixture = Fixture::new(&[("src/lib.rs", ALPHA)])?;
    let baseline = rows(fixture.check(&[("src/lib.rs", ALPHA_BEFORE, ALPHA)])?);
    let original = baseline.values().collect::<BTreeSet<_>>();
    for (path, before, after) in [
        (
            "src/lib.rs",
            ALPHA_BEFORE.replace("alpha", "gamma"),
            ALPHA.replace("alpha", "gamma"),
        ),
        (
            "src/lib.rs",
            ALPHA_BEFORE.replace("10", "11"),
            ALPHA.replace("10", "11"),
        ),
        ("src/other.rs", ALPHA_BEFORE.to_string(), ALPHA.to_string()),
    ] {
        let changed = Fixture::new(&[(path, &after)])?;
        if path != "src/lib.rs" {
            changed.write("src/lib.rs", "mod other;\n")?;
        }
        let altered = rows(changed.check(&[(path, &before, &after)])?);
        assert!(
            altered.values().all(|id| !original.contains(id)),
            "material identity change must invalidate candidate finding IDs: {path}; baseline={baseline:?}; altered={altered:?}"
        );
    }
    Ok(())
}

fn literal_id(expression: &str) -> Result<String, String> {
    literal_id_with_type(expression, "&'static str", "\"previous\"")
}
fn literal_id_with_type(expression: &str, ty: &str, previous: &str) -> Result<String, String> {
    let source = format!("pub fn label() -> {ty} {{\n    {expression}\n}}\n");
    let before = format!("pub fn label() -> {ty} {{\n    {previous}\n}}\n");
    let fixture = Fixture::new(&[("src/lib.rs", &source)])?;
    let output = fixture.check(&[("src/lib.rs", &before, &source)])?;
    let selected = output
        .findings
        .iter()
        .filter(|f| {
            f.probe.expression == expression && f.probe.after.as_deref() == Some(expression)
        })
        .collect::<Vec<_>>();
    assert_eq!(
        selected.len(),
        1,
        "expected exactly the changed literal, got {output:?}"
    );
    Ok(selected[0].id.clone())
}
#[test]
fn finding_ids_preserve_semantic_string_whitespace() -> Result<(), String> {
    assert_eq!(
        literal_id("\"a b\"")?,
        "probe:src_lib.rs:return_value:6222ee80"
    );
    assert_eq!(
        literal_id("\"a  b\"")?,
        "probe:src_lib.rs:return_value:1c4ff0e4"
    );
    assert_ne!(
        literal_id("\"a b\"")?,
        literal_id("\"a  b\"")?,
        "different returned values must never share a suppression ID"
    );
    assert_ne!(literal_id("r#\"a b\"#")?, literal_id("r#\"a  b\"#")?);
    Ok(())
}

#[test]
fn byte_c_and_character_literals_keep_their_whitespace() -> Result<(), String> {
    for (left, right, ty, previous) in [
        (r#"b"a b""#, r#"b"a  b""#, "&'static [u8]", r#"b"previous""#),
        (
            r##"br#"a b"#"##,
            r##"br#"a  b"#"##,
            "&'static [u8]",
            r#"b"previous""#,
        ),
        (
            r#"c"a b""#,
            r#"c"a  b""#,
            "&'static core::ffi::CStr",
            r#"c"previous""#,
        ),
        ("' '", "'\t'", "char", "'x'"),
    ] {
        assert_ne!(
            literal_id_with_type(left, ty, previous)?,
            literal_id_with_type(right, ty, previous)?,
            "literal data changed: {left:?} -> {right:?}"
        );
    }
    Ok(())
}

#[test]
fn formatting_whitespace_outside_literal_data_keeps_the_id() -> Result<(), String> {
    let mut ids = Vec::new();
    for expression in [r#"x == "a  b""#, r#"x  ==   "a  b""#] {
        let source = format!("pub fn equal(x: &str) -> bool {{\n    {expression}\n}}\n");
        let before = "pub fn equal(x: &str) -> bool {\n    x == \"previous\"\n}\n";
        let fixture = Fixture::new(&[("src/lib.rs", &source)])?;
        let output = fixture.check(&[("src/lib.rs", before, &source)])?;
        let selected: Vec<_> = output
            .findings
            .iter()
            .filter(|f| f.probe.family.as_str() == "predicate" && f.probe.expression == expression)
            .collect();
        assert_eq!(selected.len(), 1, "missing exact predicate: {output:?}");
        ids.push(selected[0].id.clone());
    }
    assert_eq!(ids[0], ids[1]);
    Ok(())
}

fn line_comment_literal_cases() -> [(&'static str, &'static str); 6] {
    [
        ("x == // note\n        \"a b\"", "353daaf8"),
        ("x == // note\n        \"a  b\"", "bd483d1f"),
        ("x  == //  note\n          \"a  b\"", "bd483d1f"),
        ("x == // \" fake\n        \"a b\"", "2f8257db"),
        ("x == // \" fake\n        \"a  b\"", "0075a8f9"),
        ("x  == // \"  fake\n          \"a  b\"", "0075a8f9"),
    ]
}

fn comparison_source(expression: &str) -> String {
    format!("pub fn equal(x: &str) -> bool {{\n    {expression}\n}}\n")
}

fn current_predicate_id(output: &CheckOutput, expression: &str) -> String {
    let selected: Vec<_> = output
        .findings
        .iter()
        .filter(|finding| {
            finding.probe.family.as_str() == "predicate"
                && finding.probe.expression == expression
                && finding.source_currentness.permits_candidate_action()
        })
        .collect();
    assert_eq!(selected.len(), 1, "missing current predicate: {output:?}");
    selected[0].id.clone()
}

#[test]
fn line_comment_diff_ids_preserve_literal_values() -> Result<(), String> {
    let before = comparison_source("x == \"previous\"");
    for (expression, fingerprint) in line_comment_literal_cases() {
        let source = comparison_source(expression);
        let fixture = Fixture::new(&[("src/lib.rs", &source)])?;
        let output = fixture.check(&[("src/lib.rs", &before, &source)])?;
        assert_eq!(
            current_predicate_id(&output, expression),
            format!("probe:src_lib.rs:predicate:{fingerprint}"),
            "literal bytes after a line comment must survive diff identity normalization"
        );
    }
    Ok(())
}

#[test]
fn line_comment_repo_ids_preserve_literal_values() -> Result<(), String> {
    for (expression, fingerprint) in line_comment_literal_cases() {
        let source = comparison_source(expression);
        let fixture = Fixture::new(&[("src/lib.rs", &source)])?;
        let output = ripr::app::check_workspace_repo(CheckInput {
            root: fixture.root.clone(),
            mode: Mode::Draft,
            format: OutputFormat::Json,
            ..CheckInput::default()
        })?;
        assert_eq!(
            current_predicate_id(&output, expression),
            format!("repo-probe:src_lib.rs:predicate:{fingerprint}"),
            "literal bytes after a line comment must survive repo identity normalization"
        );
    }
    Ok(())
}

#[test]
fn line_comment_literal_change_rejects_stale_suppression() -> Result<(), String> {
    let before = comparison_source("x == \"previous\"");
    let source = comparison_source("x == // \" fake\n        \"a  b\"");
    let fixture = Fixture::new(&[("src/lib.rs", &source)])?;
    let policy = |fingerprint: &str| {
        format!(
            "schema_version = 1\n[[suppressions]]\nkind = \"exposure_gap\"\nfinding_id = \"probe:src_lib.rs:predicate:{fingerprint}\"\nreason = \"reviewed literal\"\nowner = \"identity-test\"\n"
        )
    };
    fixture.write("suppressions.toml", &policy("2f8257db"))?;
    let stale = fixture.check(&[("src/lib.rs", &before, &source)])?;
    let suppression = stale.suppression.ok_or("explicit policy outcome missing")?;
    assert!(
        suppression.suppressed.is_empty(),
        "the one-space literal's ID must not suppress the two-space value"
    );
    fixture.write("suppressions.toml", &policy("0075a8f9"))?;
    let current = fixture.check(&[("src/lib.rs", &before, &source)])?;
    let suppression = current
        .suppression
        .ok_or("current policy outcome missing")?;
    assert_eq!(suppression.suppressed.len(), 1);
    assert_eq!(
        suppression.suppressed[0].finding_id,
        "probe:src_lib.rs:predicate:0075a8f9"
    );
    Ok(())
}

#[test]
fn stale_literal_suppression_refuses_and_names_the_legacy_replacement() -> Result<(), String> {
    let before = "pub fn label() -> &'static str {\n    \"previous\"\n}\n";
    let source = "pub fn label() -> &'static str {\n    \"a  b\"\n}\n";
    let fixture = Fixture::new(&[("src/lib.rs", source)])?;
    let policy = |id: &str| {
        format!(
            "schema_version = 1\n[[suppressions]]\nkind = \"exposure_gap\"\nfinding_id = \"{id}\"\nreason = \"reviewed literal\"\nowner = \"identity-test\"\n"
        )
    };
    fixture.write(
        "suppressions.toml",
        &policy("probe:src_lib.rs:return_value:6222ee80"),
    )?;
    let stale = fixture.check(&[("src/lib.rs", before, source)])?;
    let suppression = stale.suppression.ok_or("explicit policy outcome missing")?;
    assert!(
        suppression.suppressed.is_empty(),
        "old literal ID must not suppress a different value"
    );
    assert!(
        suppression
            .warnings
            .iter()
            .any(|w| w.contains("0.10 id") && w.contains("probe:src_lib.rs:return_value:1c4ff0e4")),
        "legacy normalization must still reconstruct the actual old ID: {:?}",
        suppression.warnings
    );
    fixture.write(
        "suppressions.toml",
        &policy("probe:src_lib.rs:return_value:1c4ff0e4"),
    )?;
    let current = fixture.check(&[("src/lib.rs", before, source)])?;
    let suppression = current
        .suppression
        .ok_or("current policy outcome missing")?;
    assert_eq!(suppression.suppressed.len(), 1);
    assert_eq!(
        suppression.suppressed[0].finding_id,
        "probe:src_lib.rs:return_value:1c4ff0e4"
    );
    Ok(())
}

#[test]
fn duplicate_expression_occurrences_keep_distinct_ids() -> Result<(), String> {
    let source = "pub fn twice(x: i32) -> bool {\n    if x >= 10 { return true; }\n    if x >= 10 { return false; }\n    false\n}\n";
    let before = source.replace(">=", ">");
    let fixture = Fixture::new(&[("src/lib.rs", source)])?;
    let selected = |output: CheckOutput| {
        let mut matches: Vec<_> = output
            .findings
            .into_iter()
            .filter(|f| f.probe.family.as_str() == "predicate" && f.probe.expression == "x >= 10")
            .map(|f| (f.probe.location.line, f.id))
            .collect();
        matches.sort();
        assert_eq!(matches.len(), 2, "both duplicate occurrences must be found");
        assert_ne!(matches[0].1, matches[1].1);
        assert_eq!(matches[1].1, format!("{}.2", matches[0].1));
        matches
    };
    let baseline = selected(fixture.check(&[("src/lib.rs", &before, source)])?);
    fixture.write("src/unrelated.rs", "pub fn unrelated() {}\n")?;
    assert_eq!(
        baseline,
        selected(fixture.check(&[("src/lib.rs", &before, source)])?)
    );
    Ok(())
}

#[cfg(all(feature = "lang-python", feature = "lang-typescript"))]
#[test]
fn foreign_formatting_does_not_inherit_rust_literal_boundaries() -> Result<(), String> {
    for (path, sources, previous, expected) in [
        (
            "src/subject.py",
            [
                "def label(x):\n    return 'x\"y' + x + 'z\"w'\n",
                "def label(x):\n    return 'x\"y' +  x + 'z\"w'\n",
            ],
            "def label(x):\n    return x\n",
            "probe:src_subject.py:python_preview:c53015c3",
        ),
        (
            "src/subject.ts",
            [
                "export function label(x: string): string {\n  return 'x\"y' + x + 'z\"w';\n}\n",
                "export function label(x: string): string {\n  return 'x\"y' +  x + 'z\"w';\n}\n",
            ],
            "export function label(x: string): string {\n  return x;\n}\n",
            "probe:src_subject.ts:typescript_preview:70e065a7",
        ),
        (
            "src/subject.ts",
            [
                "export function label(x: string, y: string): boolean {\n  if (/\"/.test(x) && x !== \"a\" && /\"/.test(y)) {\n    return true;\n  }\n  return false;\n}\n",
                "export function label(x: string, y: string): boolean {\n  if (/\"/.test(x) && x  !== \"a\" && /\"/.test(y)) {\n    return true;\n  }\n  return false;\n}\n",
            ],
            "export function label(x: string, y: string): boolean {\n  if (/\"/.test(x) && x !== \"b\" && /\"/.test(y)) {\n    return true;\n  }\n  return false;\n}\n",
            "probe:src_subject.ts:typescript_preview:516ba317",
        ),
    ] {
        for source in sources {
            let fixture = Fixture::new(&[(path, source)])?;
            fixture.write(
                "ripr.toml",
                "[languages]\nenabled = [\"rust\", \"python\", \"typescript\"]\n",
            )?;
            fixture.write(
                "pyproject.toml",
                "[project]\nname=\"identity_fixture\"\nversion=\"0.0.0\"\n",
            )?;
            fixture.write(
                "package.json",
                "{\"name\":\"identity-fixture\",\"version\":\"0.0.0\"}",
            )?;
            let mut config = ripr::config::RiprConfig::default();
            config.languages.enabled = vec![
                ripr::domain::LanguageId::Rust,
                ripr::domain::LanguageId::Python,
                ripr::domain::LanguageId::TypeScript,
            ];
            let output = fixture.check_with_config(&[(path, previous, source)], &config)?;
            assert!(
                output.findings.iter().any(|f| f.id == expected),
                "formatting must retain the baseline ID for {path}: {output:?}"
            );
        }
    }
    Ok(())
}
