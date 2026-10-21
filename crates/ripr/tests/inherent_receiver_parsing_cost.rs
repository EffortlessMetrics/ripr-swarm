//! Pinned measurement harness and behavioral witness for #8005.
//!
//! A source review flagged a *potential* repeated-work cost in the
//! inherent-receiver gate (`inherent_receiver_call_resolves`, #8004/#7415):
//! every owner/test query re-reads the nominally matching whole test and its
//! supported assertion operands. This harness pins a representative fixture
//! with many seams whose owners share the same tests, and the behavioral
//! invariants a parsing-cost decision must preserve on current main:
//!
//! - real receiver calls and standard assertion operands keep
//!   `direct_owner_call` attribution;
//! - complete related-test inventories are byte-stable across repeated runs;
//! - full (`repo-exposure-json`) and compact (`repo-exposure-summary-json`)
//!   reporting agree on grip counts per seam.
//!
//! Two further witnesses from the original study — live-shadowed/uncalled
//! owners never borrowing direct credit, and changed-source invalidation with
//! revert restoration — encode the #7415 1/n production rule
//! (`inherent_receiver_call_resolves`), which is not part of this PR; they
//! ride with that rule's landing lane and fail on current main, where the
//! live-shadow over-credit they pin is still observable.
//!
//! Parser-invocation accounting (one uncached parse per distinct text, no
//! matter how many owner/test queries repeat it) lives in the unit tests in
//! `analysis::syntax::nesting`. Wall-time sampling is an ignored probe at the
//! bottom of this file; it exists so the measurement method stays in the
//! tree and can be re-run on demand.

use ripr::{CheckInput, CheckOutput, Mode, OutputFormat, check_workspace};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

/// Every changed method in the fixture. `m09` is only exercised by a test
/// whose receiver binding is live-shadowed, `m10`/`m11` by no test at all.
const METHODS: usize = 12;

fn method_name(index: usize) -> String {
    format!("m{index:02}")
}

/// One impl block whose owners all share the same small test files, the
/// #7415 repeated-seam shape: every seam asks the same tests for related
/// owners, so one test body is queried once per owner.
fn lib_source() -> String {
    lib_source_with_methods(METHODS)
}

fn lib_source_with_methods(methods: usize) -> String {
    let methods = (0..methods)
        .map(|index| {
            format!(
                "    pub fn {name}(&self) -> u32 {{ self.balance + {index} }}\n",
                name = method_name(index)
            )
        })
        .collect::<String>();
    format!(
        "pub struct Ledger {{ balance: u32 }}\n\
         impl Ledger {{\n\
         \x20   pub fn new() -> Self {{ Self {{ balance: 0 }} }}\n\
         \x20   pub fn deposit(&mut self, amount: u32) {{ self.balance += amount; }}\n\
         \x20   pub fn balance(&self) -> u32 {{ self.balance }}\n\
         {methods}\
         }}\n"
    )
}

/// Changes every method body so each method is a seam of its own. The
/// methods are contiguous, so one zero-context hunk carries every seam.
fn diff_source() -> String {
    let pairs = (0..METHODS)
        .map(|index| {
            let name = method_name(index);
            format!(
                "-    pub fn {name}(&self) -> u32 {{ self.balance + {index} }}\n\
                 +    pub fn {name}(&self) -> u32 {{ self.balance + {index} + 1 }}\n"
            )
        })
        .collect::<String>();
    format!(
        "diff --git a/src/lib.rs b/src/lib.rs\n\
         index 0000000..1111111 100644\n\
         --- a/src/lib.rs\n\
         +++ b/src/lib.rs\n\
         @@ -6,{count} +6,{count} @@ impl Ledger {{\n\
         {pairs}",
        count = METHODS
    )
}

/// Real receivers in executable scope and inside standard assertion operands;
/// one UFCS spelling exercises the first-argument receiver path.
fn ledger_ops_test() -> &'static str {
    "use receiver_parsing_cost::Ledger;\n\
     #[test]\n\
     fn ledger_ops() {\n\
     \x20   let mut book = Ledger::new();\n\
     \x20   book.deposit(1);\n\
     \x20   let seen = book.m00();\n\
     \x20   assert_eq!(book.m01(), 2);\n\
     \x20   assert_ne!(book.m02(), 0);\n\
     \x20   assert_eq!(Ledger::m03(&book), 4);\n\
     \x20   assert_eq!(book.m04(), 5);\n\
     \x20   assert!(seen <= book.m00());\n\
     }\n"
}

fn ledger_more_test() -> &'static str {
    "use receiver_parsing_cost::Ledger;\n\
     #[test]\n\
     fn ledger_more() {\n\
     \x20   let mut book = Ledger::new();\n\
     \x20   book.deposit(1);\n\
     \x20   assert_eq!(book.m05(), 6);\n\
     \x20   assert_eq!(book.m06(), 7);\n\
     \x20   assert_eq!(book.m07(), 8);\n\
     \x20   assert_eq!(book.m08(), 9);\n\
     \x20   let _ = book;\n\
     }\n"
}

/// Nominally matches `m09` (the call text is present) but the receiver
/// binding is live-shadowed by another type, so no direct credit is due.
fn ledger_shadow_test() -> &'static str {
    "use receiver_parsing_cost::Ledger;\n\
     #[test]\n\
     fn ledger_shadow() {\n\
     \x20   let book = Ledger::new();\n\
     \x20   let book = String::new();\n\
     \x20   assert_eq!(book.m09(), 9);\n\
     \x20   let _ = book;\n\
     }\n"
}

/// Proximity-only test: no changed owner name appears, so it can never earn
/// direct credit; it keeps inventories honest.
fn ledger_uncalled_test() -> &'static str {
    "use receiver_parsing_cost::Ledger;\n\
     #[test]\n\
     fn ledger_uncalled() {\n\
     \x20   let book = Ledger::new();\n\
     \x20   assert!(book.balance() >= 0);\n\
     }\n"
}

/// Same-spelled test as `tests/ledger.rs::ledger_ops` but a different file:
/// relation rows must stay attributed to their own file, never merged.
fn ledger_twin_test() -> &'static str {
    "use receiver_parsing_cost::Ledger;\n\
     #[test]\n\
     fn ledger_ops() {\n\
     \x20   let book = Ledger::new();\n\
     \x20   assert_eq!(book.m00(), 0);\n\
     \x20   assert_eq!(book.m01(), 1);\n\
     }\n"
}

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn create() -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "ripr-inherent-receiver-parsing-cost-{}-{sequence}",
            std::process::id()
        ));
        Self::write(&root)?;
        Ok(Self { root })
    }

    fn write(root: &Path) -> Result<(), String> {
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create src directory failed: {error}"))?;
        std::fs::create_dir_all(root.join("tests"))
            .map_err(|error| format!("create tests directory failed: {error}"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"receiver-parsing-cost\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )
        .map_err(|error| format!("write Cargo.toml failed: {error}"))?;
        std::fs::write(root.join("src/lib.rs"), lib_source())
            .map_err(|error| format!("write lib.rs failed: {error}"))?;
        std::fs::write(root.join("tests/ledger.rs"), ledger_ops_test())
            .map_err(|error| format!("write tests/ledger.rs failed: {error}"))?;
        std::fs::write(root.join("tests/more.rs"), ledger_more_test())
            .map_err(|error| format!("write tests/more.rs failed: {error}"))?;
        std::fs::write(root.join("tests/shadow.rs"), ledger_shadow_test())
            .map_err(|error| format!("write tests/shadow.rs failed: {error}"))?;
        std::fs::write(root.join("tests/uncalled.rs"), ledger_uncalled_test())
            .map_err(|error| format!("write tests/uncalled.rs failed: {error}"))?;
        std::fs::write(root.join("tests/twin.rs"), ledger_twin_test())
            .map_err(|error| format!("write tests/twin.rs failed: {error}"))?;
        std::fs::write(root.join("diff.patch"), diff_source())
            .map_err(|error| format!("write diff.patch failed: {error}"))?;
        Ok(())
    }

    fn check(&self) -> Result<CheckOutput, String> {
        check_workspace(CheckInput {
            root: self.root.clone(),
            base: None,
            diff_file: Some(self.root.join("diff.patch")),
            mode: Mode::Ready,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })
        .map_err(|error| format!("check_workspace failed: {error}"))
    }

    fn run_ripr(&self, format: &str) -> Result<Vec<u8>, String> {
        let output = Command::new(env!("CARGO_BIN_EXE_ripr"))
            .args([
                "check",
                "--root",
                self.root.to_str().ok_or("utf-8 fixture root")?,
                "--format",
                format,
                "--quiet",
            ])
            .output()
            .map_err(|error| format!("run exact ripr binary: {error}"))?;
        if !output.status.success() {
            return Err(format!(
                "ripr check --format {format} failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        Ok(output.stdout)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// All related-test rows across findings that cover `method`'s changed line,
/// as `(file, name, reason)` triples. A method body change produces findings
/// on one line, so the line is the identity anchor.
fn rows_for_method(output: &CheckOutput, line: usize) -> Vec<(String, String, String)> {
    let mut rows = Vec::new();
    for finding in &output.findings {
        if finding.probe.location.line != line {
            continue;
        }
        for related in &finding.related_tests {
            let reason = related
                .relation_reason
                .map(|value| value.as_str().to_string())
                .unwrap_or_default();
            rows.push((
                related.file.to_string_lossy().replace('\\', "/"),
                related.name.clone(),
                reason,
            ));
        }
    }
    rows.sort();
    rows.dedup();
    rows
}

fn direct_rows(rows: &[(String, String, String)]) -> Vec<(String, String)> {
    rows.iter()
        .filter(|(_, _, reason)| reason == "direct_owner_call")
        .map(|(file, name, _)| (file.clone(), name.clone()))
        .collect()
}

/// 1-based line of each method body in `src/lib.rs` as written on disk.
fn method_line(index: usize) -> usize {
    // 1 struct, 2 impl open, 3 new(), 4 deposit(), 5 balance(), 6.. methods.
    6 + index
}

#[test]
fn shared_owner_test_seams_keep_receiver_attribution() -> Result<(), String> {
    let fixture = Fixture::create()?;
    let output = fixture.check()?;

    let direct_for = |index: usize| -> Result<Vec<(String, String)>, String> {
        let rows = rows_for_method(&output, method_line(index));
        let direct = direct_rows(&rows);
        if direct.is_empty() {
            return Err(format!(
                "method {index} lost all direct_owner_call rows; complete rows: {rows:?}"
            ));
        }
        Ok(direct)
    };

    // m00..m04: called with real receivers from tests/ledger.rs::ledger_ops.
    for index in 0..=4 {
        let direct = direct_for(index)?;
        if !direct.contains(&("tests/ledger.rs".to_string(), "ledger_ops".to_string())) {
            return Err(format!(
                "method {index} must credit tests/ledger.rs::ledger_ops; direct rows: {direct:?}"
            ));
        }
    }
    // m05..m08: called from tests/more.rs::ledger_more.
    for index in 5..=8 {
        let direct = direct_for(index)?;
        if !direct.contains(&("tests/more.rs".to_string(), "ledger_more".to_string())) {
            return Err(format!(
                "method {index} must credit tests/more.rs::ledger_more; direct rows: {direct:?}"
            ));
        }
    }
    // Every changed method produced finding rows: the inventory is complete.
    let seam_lines: Vec<usize> = output
        .findings
        .iter()
        .map(|finding| finding.probe.location.line)
        .collect();
    for index in 0..METHODS {
        if !seam_lines.contains(&method_line(index)) {
            return Err(format!("method {index} produced no finding row"));
        }
    }
    Ok(())
}

#[test]
fn repeated_runs_return_the_complete_inventory_unchanged() -> Result<(), String> {
    let fixture = Fixture::create()?;
    let first = fixture.check()?;
    let second = fixture.check()?;
    let first_json = serde_json::to_string(&first.findings)
        .map_err(|error| format!("serialize first findings: {error}"))?;
    let second_json = serde_json::to_string(&second.findings)
        .map_err(|error| format!("serialize second findings: {error}"))?;
    if first_json != second_json {
        return Err("repeated identical analyses must return identical relation rows".to_string());
    }
    Ok(())
}

#[test]
fn full_and_compact_reporting_agree_on_grip_counts() -> Result<(), String> {
    let fixture = Fixture::create()?;
    let full_a = fixture.run_ripr("repo-exposure-json")?;
    let full_b = fixture.run_ripr("repo-exposure-json")?;
    let compact_a = fixture.run_ripr("repo-exposure-summary-json")?;
    let compact_b = fixture.run_ripr("repo-exposure-summary-json")?;

    let full: serde_json::Value = serde_json::from_slice(&full_a)
        .map_err(|error| format!("parse full repo-exposure: {error}"))?;
    let compact: serde_json::Value = serde_json::from_slice(&compact_a)
        .map_err(|error| format!("parse compact repo-exposure: {error}"))?;
    let full_b: serde_json::Value = serde_json::from_slice(&full_b)
        .map_err(|error| format!("parse second full repo-exposure: {error}"))?;
    let compact_b: serde_json::Value = serde_json::from_slice(&compact_b)
        .map_err(|error| format!("parse second compact repo-exposure: {error}"))?;

    // Only the run-scoped artifact envelope (content digest and generation
    // time) may differ between identical runs; every analyzed field, seams
    // and metrics included, must be identical.
    let strip_artifact = |mut value: serde_json::Value| {
        if let Some(object) = value.as_object_mut() {
            object.remove("artifact");
        }
        value
    };
    if strip_artifact(full.clone()) != strip_artifact(full_b) {
        return Err("full repo-exposure analysis fields must be stable across runs".to_string());
    }
    if strip_artifact(compact.clone()) != strip_artifact(compact_b) {
        return Err("compact repo-exposure analysis fields must be stable across runs".to_string());
    }

    // The compact report aggregates per-class grip counts instead of
    // per-seam rows, so parity is: identical grip-class tallies, identical
    // seam totals, and a full report whose own per-seam rows match their
    // declared grip counts.
    let full_metrics = full["metrics"].clone();
    let compact_metrics = compact["metrics"].clone();
    if full_metrics["seams_total"] != compact_metrics["raw_seams"] {
        return Err(format!(
            "compact report disagrees on seam totals: full {} vs compact {}",
            full_metrics["seams_total"], compact_metrics["raw_seams"]
        ));
    }
    if full_metrics["headline_eligible"] != compact_metrics["headline_eligible_seams"] {
        return Err(format!(
            "compact report disagrees on headline-eligible seams: full {} vs compact {}",
            full_metrics["headline_eligible"], compact_metrics["headline_eligible_seams"]
        ));
    }
    let compact_classes = compact_metrics["grip_class"]
        .as_object()
        .ok_or("compact report without grip_class tally")?;
    let mut compared_classes = 0usize;
    for (class, compact_count) in compact_classes {
        let full_count = full_metrics
            .get(class)
            .ok_or("compact grip class missing from full metrics")?;
        if full_count != compact_count {
            return Err(format!(
                "compact grip class {class} differs: full {full_count} vs compact {compact_count}"
            ));
        }
        compared_classes += 1;
    }
    if compared_classes == 0 {
        return Err("compact report carried no grip classes".to_string());
    }

    let full_seams = full["seams"]
        .as_array()
        .ok_or("full repo-exposure must carry a seams array")?;
    let mut ledger_seams = 0usize;
    for seam in full_seams {
        let Some(file) = seam["file"].as_str() else {
            continue;
        };
        if !file.replace('\\', "/").starts_with("src/lib.rs") {
            continue;
        }
        let Some(owner) = seam["owner"].as_str() else {
            continue;
        };
        let method = owner.rsplit("::").next().unwrap_or_default();
        if method.len() != 3 || !method.starts_with('m') {
            continue;
        }
        ledger_seams += 1;
        let declared = seam["related_tests_total"]
            .as_u64()
            .ok_or("full seam without related_tests_total")?;
        let rows = seam["related_tests"]
            .as_array()
            .ok_or("full seam without related_tests")?;
        let listed = rows.len() as u64;
        // The full report caps listed rows at eight but keeps the uncapped
        // total, so equality only holds below the cap.
        if declared != listed && listed < 8 {
            return Err(format!(
                "seam {owner} declares {declared} grip rows but lists {listed}"
            ));
        }
    }
    if ledger_seams != METHODS {
        return Err(format!(
            "expected {METHODS} Ledger method seams in the full report, found {ledger_seams}"
        ));
    }
    Ok(())
}

/// Wall-time probe for #8005. Ignored: it measures rather than asserts, so
/// CI never runs it; the numbers it emits are the retained samples behind
/// the parsing-cost decision. Run explicitly, in the profile under study:
///
/// ```text
/// cargo test -p ripr --test inherent_receiver_parsing_cost \
///   perf_probe_wall_time_samples -- --ignored --nocapture
/// ```
///
/// `RIPR_PERF_METHODS` scales the seam count (default 64) and
/// `RIPR_PERF_SAMPLES` sets the samples per shape (default 7). JSON lands on
/// stdout: elapsed milliseconds per sample for the full and compact
/// repo-exposure reports over the same workspace, plus a same-queries
/// control whose tests each name exactly one owner (no shared test bodies).
#[test]
#[ignore = "perf probe: emits wall-time samples; run explicitly in the study profile"]
fn perf_probe_wall_time_samples() -> Result<(), String> {
    let methods: usize = std::env::var("RIPR_PERF_METHODS")
        .ok()
        .and_then(|value| value.parse().ok())
        .map_or(64, |value| value);
    let samples: usize = std::env::var("RIPR_PERF_SAMPLES")
        .ok()
        .and_then(|value| value.parse().ok())
        .map_or(7, |value| value);

    let mut report = serde_json::json!({
        "methods": methods,
        "samples_per_shape": samples,
        "shapes": {},
    });

    for (shape, source_fn, test_fn) in [
        (
            "shared_tests",
            lib_source_with_methods as fn(usize) -> String,
            shared_tests as fn(usize) -> String,
        ),
        (
            "split_tests",
            lib_source_with_methods as fn(usize) -> String,
            split_tests as fn(usize) -> String,
        ),
    ] {
        let root = std::env::temp_dir().join(format!(
            "ripr-perf-probe-{shape}-{}-{}",
            methods,
            std::process::id()
        ));
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create probe src: {error}"))?;
        std::fs::create_dir_all(root.join("tests"))
            .map_err(|error| format!("create probe tests: {error}"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"receiver-parsing-cost\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )
        .map_err(|error| format!("write probe manifest: {error}"))?;
        std::fs::write(root.join("src/lib.rs"), source_fn(methods))
            .map_err(|error| format!("write probe lib: {error}"))?;
        std::fs::write(root.join("tests/probe.rs"), test_fn(methods))
            .map_err(|error| format!("write probe tests: {error}"))?;
        let probe = PerfProbe { root: &root };
        for format in ["repo-exposure-json", "repo-exposure-summary-json"] {
            let mut elapsed = Vec::new();
            for _ in 0..samples {
                // Every sample starts cold: ripr persists classified seams
                // under target/, so without this reset samples after the
                // first would measure cache reads, not receiver parsing.
                let _ = std::fs::remove_dir_all(root.join("target"));
                let start = std::time::Instant::now();
                probe.run(format)?;
                elapsed.push(start.elapsed().as_secs_f64() * 1_000.0);
            }
            report["shapes"][shape][format] = serde_json::json!(elapsed);
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    println!(
        "{}",
        serde_json::to_string_pretty(&report).map_err(|error| error.to_string())?
    );
    Ok(())
}

struct PerfProbe<'a> {
    root: &'a Path,
}

impl PerfProbe<'_> {
    fn run(&self, format: &str) -> Result<(), String> {
        let output = Command::new(env!("CARGO_BIN_EXE_ripr"))
            .args([
                "check",
                "--root",
                self.root.to_str().ok_or("utf-8 probe root")?,
                "--format",
                format,
                "--quiet",
            ])
            .output()
            .map_err(|error| format!("run probe: {error}"))?;
        if !output.status.success() {
            return Err(format!(
                "probe {format} failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        Ok(())
    }
}

/// The repeated-seam shape: one test names every owner, so the gate queries
/// one shared body once per owner.
fn shared_tests(methods: usize) -> String {
    let calls = (0..methods)
        .map(|index| {
            format!(
                "    assert_eq!(book.{name}(), 1 + {index});\n",
                name = method_name(index)
            )
        })
        .collect::<String>();
    format!(
        "use receiver_parsing_cost::Ledger;\n#[test]\nfn probe_all() {{\n    let book = Ledger::new();\n{calls}}}\n"
    )
}

/// The same-queries control: every owner still gets queried by exactly one
/// test, but no test body is shared between owners.
fn split_tests(methods: usize) -> String {
    (0..methods)
        .map(|index| {
            format!(
                "use receiver_parsing_cost::Ledger;\n#[test]\nfn probe_{index}() {{\n    let book = Ledger::new();\n    assert_eq!(book.{name}(), 1 + {index});\n}}\n",
                name = method_name(index)
            )
        })
        .collect()
}
