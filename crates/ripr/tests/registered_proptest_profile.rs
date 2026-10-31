//! #10839: actual provider/registration/failure controls for the #6643 slice.
//! The ordinary analyzer runs in a child so runner-configuration changes do
//! not mutate the concurrent Rust test process's environment.
use serde_json::Value;
use std::ffi::OsStr;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

const SOURCE: &str = include_str!("fixtures/proptest_singleton/input/src/lib.rs");
const PROPERTY: &str = include_str!("fixtures/proptest_singleton/input/tests/boundary.rs");
const MANIFEST: &str = include_str!("fixtures/proptest_singleton/input/Cargo.toml");
const LOCK: &str = include_str!("fixtures/proptest_singleton/input/Cargo.lock");
const DIFF: &str = include_str!("fixtures/proptest_singleton/diff.patch");
const ORDINARY: &str = "#[test]\nfn exact_boundary() {\n    assert_eq!(lib_proptest::discounted_total(100i32, 100), 90);\n}\n";
const PROBE_ROOT: &str = "RIPR_PROPTEST_PROFILE_PROBE_ROOT";
const PROBE_REPO: &str = "RIPR_PROPTEST_PROFILE_REPO";
const PROBE_HUMAN: &str = "RIPR_PROPTEST_PROFILE_HUMAN";

#[path = "registered_proptest_profile/provider_diagnostics.rs"]
mod provider_diagnostics;
const REPORT: &str = "RIPR_PROPERTY_REPORT=";
const REPORT_END: &str = "RIPR_PROPERTY_REPORT_END";

#[derive(Default)]
struct ProbeEnvironment {
    cargo_home: Option<PathBuf>,
    human: bool,
}

struct Fixture(PathBuf, ProbeEnvironment);
impl Fixture {
    fn new() -> Result<Self, String> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let temporary = std::env::temp_dir();
        std::fs::create_dir_all(&temporary).map_err(|error| error.to_string())?;
        let root = temporary.join(format!(
            "ripr-proptest-profile-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).map_err(|error| error.to_string())?;
        let fixture = Self(root, ProbeEnvironment::default());
        std::fs::create_dir(fixture.0.join("src")).map_err(|error| error.to_string())?;
        std::fs::create_dir(fixture.0.join("tests")).map_err(|error| error.to_string())?;
        fixture.write("src/lib.rs", SOURCE)?;
        fixture.write("tests/boundary.rs", PROPERTY)?;
        fixture.write("Cargo.toml", MANIFEST)?;
        fixture.write("Cargo.lock", LOCK)?;
        fixture.write("diff.patch", DIFF)?;
        Ok(fixture)
    }
    fn write(&self, path: &str, value: &str) -> Result<(), String> {
        std::fs::write(self.0.join(path), value).map_err(|error| error.to_string())
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!("retained failed property fixture {}", self.0.display());
        } else if let Err(error) = std::fs::remove_dir_all(&self.0) {
            eprintln!("property fixture cleanup {}: {error}", self.0.display());
        }
    }
}

fn read_stream(path: &Path) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|error| error.to_string())?
        .take(256 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() > 256 * 1024 {
        return Err("property proof stream exceeds 256 KiB".into());
    }
    Ok(bytes)
}

fn run(
    fixture: &Fixture,
    program: &OsStr,
    arguments: &[&OsStr],
    label: &str,
    cases: Option<&str>,
    probe: bool,
    repository_cache: Option<&Path>,
) -> Result<Output, String> {
    let stdout = fixture.0.join(format!("{label}.stdout"));
    let stderr = fixture.0.join(format!("{label}.stderr"));
    let mut command = Command::new(program);
    command
        .current_dir(&fixture.0)
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(std::fs::File::create(&stdout).map_err(|error| error.to_string())?)
        .stderr(std::fs::File::create(&stderr).map_err(|error| error.to_string())?)
        .env("CARGO_TARGET_DIR", fixture.0.join("native-target"))
        .env("CARGO_BUILD_JOBS", "1")
        .env("CARGO_INCREMENTAL", "0");
    for (name, _) in std::env::vars_os() {
        if name
            .to_string_lossy()
            .to_ascii_uppercase()
            .starts_with("PROPTEST_")
        {
            command.env_remove(name);
        }
    }
    if let Some(cases) = cases {
        command.env("PROPTEST_CASES", cases);
    }
    if probe {
        command.env(PROBE_ROOT, &fixture.0);
        if let Some(home) = &fixture.1.cargo_home {
            command.env("CARGO_HOME", home);
        }
        if fixture.1.human {
            command.env(PROBE_HUMAN, "1");
        } else {
            command.env_remove(PROBE_HUMAN);
        }
        if let Some(cache) = repository_cache {
            command
                .env(PROBE_REPO, "1")
                .env("RIPR_CACHE_DIR", cache)
                .env("RIPR_REPO_EXPOSURE_LATENCY_TRACE", "1");
        } else {
            command
                .env_remove(PROBE_REPO)
                .env("RIPR_CACHE_DIR", fixture.0.join("target/diff-cache"));
        }
    } else {
        command.env_remove(PROBE_ROOT);
        command.env_remove(PROBE_REPO);
        // Fixed for every native variant: an earlier wrong run must not add
        // a persisted replay to the deliberately zero-generated-cases row.
        command.env("PROPTEST_DISABLE_FAILURE_PERSISTENCE", "1");
    }
    command.env("RUSTUP_AUTO_INSTALL", "0");
    let mut child =
        ripr::process_owner::OwnedProcess::spawn(command).map_err(|error| error.to_string())?;
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
            break status;
        }
        if started.elapsed() > Duration::from_mins(5) {
            child.terminate_tree()?;
            return Err(format!(
                "{label}: bounded property proof timed out; process reaped"
            ));
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    Ok(Output {
        status,
        stdout: read_stream(&stdout)?,
        stderr: read_stream(&stderr)?,
    })
}

/// An instrumentation entry, not a standalone positive. The parent requires
/// the marked report and its nonempty exact finding before accepting a run.
#[test]
fn property_profile_probe_child() -> Result<(), String> {
    let Some(root) = std::env::var_os(PROBE_ROOT).map(PathBuf::from) else {
        return Ok(());
    };
    let repository = std::env::var_os(PROBE_REPO).is_some();
    // The public API defaults do not load ripr.toml. Use the ordinary CLI
    // loader for both producers so the policy controls exercise the file
    // they write, including its classified-cache identity.
    let mut args = vec![
        "ripr".to_string(),
        "check".to_string(),
        "--root".to_string(),
        root.to_str().ok_or("probe root is not UTF-8")?.to_string(),
        "--mode".to_string(),
        "fast".to_string(),
        "--format".to_string(),
        if repository {
            "repo-exposure-json"
        } else if std::env::var_os(PROBE_HUMAN).is_some() {
            "human-full"
        } else {
            "json"
        }
        .to_string(),
        "--quiet".to_string(),
    ];
    if !repository {
        args.push("--diff".to_string());
        args.push(
            root.join("diff.patch")
                .to_str()
                .ok_or("probe diff path is not UTF-8")?
                .to_string(),
        );
    }
    println!("{REPORT}");
    ripr::cli::run(args)
        .map_err(|error| format!("ripr check failed (exit {}): {error}", error.exit_code()))?;
    println!("\n{REPORT_END}");
    Ok(())
}

fn finding(fixture: &Fixture, label: &str, cases: Option<&str>) -> Result<Value, String> {
    let (report, _) = report(fixture, label, cases, None)?;
    let findings = report["findings"].as_array().ok_or("missing findings")?;
    let [finding] = findings.as_slice() else {
        return Err(format!(
            "{label}: expected the one changed threshold: {report}"
        ));
    };
    Ok(finding.clone())
}

fn report(
    fixture: &Fixture,
    label: &str,
    cases: Option<&str>,
    repository_cache: Option<&Path>,
) -> Result<(Value, String), String> {
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    let output = run(
        fixture,
        executable.as_os_str(),
        &[
            OsStr::new("--exact"),
            OsStr::new("property_profile_probe_child"),
            OsStr::new("--nocapture"),
        ],
        label,
        cases,
        true,
        repository_cache,
    )?;
    if !output.status.success() {
        return Err(format!(
            "{label}: analyzer instrumentation failed: {}\n{}\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let stdout = String::from_utf8(output.stdout).map_err(|error| error.to_string())?;
    if stdout.matches(REPORT).count() != 1 || stdout.matches(REPORT_END).count() != 1 {
        return Err(format!(
            "{label}: expected exactly one marked analyzer report: {stdout}"
        ));
    }
    let report = stdout
        .split_once(REPORT)
        .and_then(|(_, marked)| marked.split_once(REPORT_END).map(|(report, _)| report))
        .ok_or_else(|| format!("{label}: malformed analyzer report framing: {stdout}"))?;
    let report = serde_json::from_str(report.trim()).map_err(|error| error.to_string())?;
    Ok((report, String::from_utf8_lossy(&output.stderr).into_owned()))
}

fn boundary(report: &Value) -> Result<Value, String> {
    assert_eq!(report["run_status"], "complete", "{report}");
    let matching = report["seams"]
        .as_array()
        .ok_or("missing repository seams")?
        .iter()
        .filter(|seam| {
            seam["owner"] == "src/lib.rs::discounted_total" && seam["kind"] == "predicate_boundary"
        })
        .collect::<Vec<_>>();
    let [seam] = matching.as_slice() else {
        return Err(format!("expected one owned predicate boundary: {report}"));
    };
    Ok((*seam).clone())
}

#[test]
fn classified_cache_revokes_nested_target_configuration_and_preserves_revert() -> Result<(), String>
{
    let fixture = Fixture::new()?;
    let cache = fixture.0.join("target/repository-cache");
    let (cold, _) = report(&fixture, "repo-cold", None, Some(&cache))?;
    assert_eq!(boundary(&cold)?["grip_class"], "strongly_gripped", "{cold}");
    let (warm, trace) = report(&fixture, "repo-warm", None, Some(&cache))?;
    assert_eq!(warm["seams"], cold["seams"]);
    assert!(
        trace.contains("cache_hit"),
        "classified cache was not reused: {trace}"
    );

    // This configuration is not an analysis-root/ancestor input. It is a
    // target-directory premise consulted by the property admission owner.
    // Keep Rust bytes, manifests and lockfile unchanged across this edit.
    std::fs::create_dir(fixture.0.join("tests/.cargo")).map_err(|error| error.to_string())?;
    fixture.write("tests/.cargo/config.toml", "[env]\nPROPTEST_CASES='0'\n")?;
    let (changed, _) = report(&fixture, "repo-nested-warm", None, Some(&cache))?;
    let (independent, _) = report(
        &fixture,
        "repo-nested-cold",
        None,
        Some(&fixture.0.join("target/repository-independent")),
    )?;
    assert_ne!(boundary(&independent)?["grip_class"], "strongly_gripped");
    assert_eq!(changed["seams"], independent["seams"]);
    std::fs::remove_file(fixture.0.join("tests/.cargo/config.toml"))
        .map_err(|error| error.to_string())?;
    std::fs::remove_dir(fixture.0.join("tests/.cargo")).map_err(|error| error.to_string())?;
    let (restored, trace) = report(&fixture, "repo-restored", None, Some(&cache))?;
    assert_eq!(restored["seams"], cold["seams"]);
    assert!(
        trace.contains("cache_hit"),
        "revert must reuse preserved entry: {trace}"
    );
    Ok(())
}

#[test]
fn ordinary_qualified_typed_boundary_uses_the_same_discriminator() -> Result<(), String> {
    let fixture = Fixture::new()?;
    fixture.write("tests/boundary.rs", ORDINARY)?;
    assert_eq!(
        finding(&fixture, "ordinary-typed-boundary", None)?["classification"],
        "exposed"
    );
    let (repository, _) = report(
        &fixture,
        "ordinary-typed-repository",
        None,
        Some(&fixture.0.join("target/repository-cache")),
    )?;
    assert_eq!(
        boundary(&repository)?["grip_class"],
        "strongly_gripped",
        "{repository}"
    );
    Ok(())
}

#[test]
fn ordinary_nonfree_equal_typed_arguments_keep_existing_boundary_pairing() -> Result<(), String> {
    for (style, signature, setup, call) in [
        (
            "associated",
            "amount: i32, threshold: i32",
            "",
            "Price::discounted_total(100i32, 100i32)",
        ),
        (
            "receiver",
            "&self, amount: i32, threshold: i32",
            "let price = Price;\n    ",
            "price.discounted_total(100i32, 100i32)",
        ),
    ] {
        let fixture = Fixture::new()?;
        let cache = fixture.0.join("target/repository-cache");
        fixture.write(
            "src/lib.rs",
            &format!(
                "pub struct Price;\n\nimpl Price {{\n    pub fn discounted_total({signature}) -> i32 {{\n        if amount >= threshold {{\n            amount - 10\n        }} else {{\n            amount\n        }}\n    }}\n}}\n"
            ),
        )?;
        let ordinary = format!(
            "use lib_proptest::Price;\n\n#[test]\nfn exact_boundary() {{\n    {setup}assert_eq!({call}, 90);\n}}\n"
        );
        // Same runtime values, different text: the added scalar reader must
        // not extend nonfree authority merely because both normalize to 100.
        let differently_typed = ordinary.replace("100i32, 100i32", "100i32, 100");
        assert_ne!(differently_typed, ordinary);
        for (label, test_source, missing) in [
            ("identical", ordinary.as_str(), false),
            ("different-text", differently_typed.as_str(), true),
        ] {
            fixture.write("tests/boundary.rs", test_source)?;
            let label = format!("{style}-{label}");
            let (result, _) = report(&fixture, &label, None, Some(&cache))?;
            assert_eq!(result["run_status"], "complete", "{label}: {result}");
            let matching = result["seams"]
                .as_array()
                .ok_or("missing nonfree boundary seams")?
                .iter()
                .filter(|seam| {
                    seam["owner"] == "src/lib.rs::impl Price::discounted_total"
                        && seam["kind"] == "predicate_boundary"
                })
                .collect::<Vec<_>>();
            let [seam] = matching.as_slice() else {
                return Err(format!("expected one {style} boundary: {result}"));
            };
            // The bounded inherent-receiver rule (#7415) admits the
            // associated UFCS spelling and refuses the receiver spelling's
            // `let price = Price;` binding, because `Price` reaches the test
            // only through a cross-file import whose identity this bounded
            // admission cannot resolve; restoring that credit is owned by
            // #8782. The associated pairing stays pinned; the receiver
            // spelling pins the disclosed honest refusal until it lands.
            let (grip_class, activate, discriminate, missing_pairing) = if style == "associated" {
                ("activation_unknown", "unknown", "yes", missing)
            } else {
                ("ungripped", "no", "no", true)
            };
            assert_eq!(seam["grip_class"], grip_class, "{label}: {result}");
            assert_eq!(seam["evidence"]["activate"], activate, "{label}: {result}");
            assert_eq!(
                seam["evidence"]["discriminate"], discriminate,
                "{label}: {result}"
            );
            assert_eq!(
                seam["observed_values"].as_array().map(Vec::len),
                Some(0),
                "{label}: {result}"
            );
            let missing_values = seam["missing_discriminators"]
                .as_array()
                .ok_or("missing discriminator array")?;
            assert_eq!(
                !missing_values.is_empty(),
                missing_pairing,
                "{label}: {result}"
            );
        }
    }
    Ok(())
}

#[test]
fn typed_repository_inputs_require_the_boundary_and_the_owner() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let cache = fixture.0.join("target/repository-cache");
    let unused_parameter = PROPERTY.replace("Just(100i32)", "Just(7i32)").replace(
        "discounted_total(amount, 100)",
        "discounted_total(100, 100)",
    );
    fixture.write("tests/boundary.rs", &unused_parameter)?;
    let (literal_property, _) = report(&fixture, "unused-property", None, Some(&cache))?;
    assert_eq!(
        boundary(&literal_property)?["grip_class"],
        "strongly_gripped",
        "{literal_property}"
    );
    for (name, source) in [
        (
            "ordinary-off-boundary",
            ORDINARY.replace("100i32, 100), 90", "99i32, 100), 99"),
        ),
        (
            "property-off-boundary",
            PROPERTY
                .replace("Just(100i32)", "Just(99i32)")
                .replace(", 90);", ", 99);"),
        ),
        (
            "opaque-identifier",
            ORDINARY
                .replace(
                    "    assert_eq!",
                    "    let amount = std::hint::black_box(100i32);\n    assert_eq!",
                )
                .replace("discounted_total(100i32,", "discounted_total(amount,"),
        ),
    ] {
        fixture.write("tests/boundary.rs", &source)?;
        let (result, _) = report(&fixture, name, None, Some(&cache))?;
        assert_ne!(
            boundary(&result)?["grip_class"],
            "strongly_gripped",
            "{name}: {result}"
        );
    }
    for (style, source) in [
        ("ordinary", ORDINARY),
        ("property", PROPERTY),
        ("unused-property", unused_parameter.as_str()),
    ] {
        for (name, declaration) in [
            (
                "module",
                "mod lib_proptest { pub fn discounted_total(_: i32, _: i32) -> i32 { 90 } }",
            ),
            (
                "type",
                "struct lib_proptest;\nimpl lib_proptest { fn discounted_total(_: i32, _: i32) -> i32 { 90 } }",
            ),
            (
                "raw-module",
                "mod r#lib_proptest { pub fn discounted_total(_: i32, _: i32) -> i32 { 90 } }",
            ),
            (
                "raw-type",
                "struct r#lib_proptest;\nimpl lib_proptest { fn discounted_total(_: i32, _: i32) -> i32 { 90 } }",
            ),
        ] {
            fixture.write("tests/boundary.rs", &format!("{declaration}\n{source}"))?;
            let label = format!("{style}-shadow-{name}");
            let (result, _) = report(&fixture, &label, None, Some(&cache))?;
            let seam = boundary(&result)?;
            assert_ne!(seam["grip_class"], "strongly_gripped", "{label}: {result}");
            assert_eq!(
                seam["observed_values"].as_array().map(Vec::len),
                Some(0),
                "{label}: {result}"
            );
            if style != "ordinary" {
                let owner_seams = result["seams"]
                    .as_array()
                    .ok_or("missing repository seams")?
                    .iter()
                    .filter(|seam| seam["owner"] == "src/lib.rs::discounted_total")
                    .collect::<Vec<_>>();
                assert!(!owner_seams.is_empty(), "{label}: {result}");
                // Property attribution is refused before every stage,
                // including direct-call activation on value-insensitive seams.
                for seam in owner_seams {
                    assert_eq!(seam["related_tests_total"], 0, "{label}: {result}");
                    assert_ne!(seam["grip_class"], "strongly_gripped", "{label}: {result}");
                }
                let result = finding(&fixture, &format!("{label}-diff"), None)?;
                assert_eq!(result["related_tests_total"], 0, "{label}: {result}");
                assert_eq!(
                    result["related_tests"].as_array().map(Vec::len),
                    Some(0),
                    "{label}: {result}"
                );
                for values in [
                    &result["observed_values"],
                    &result["activation"]["observed_values"],
                ] {
                    assert_eq!(
                        values.as_array().map(Vec::len),
                        Some(0),
                        "{label}: {result}"
                    );
                }
            }
            if style == "ordinary" {
                // The old unsuffixed input row remains available. Only
                // the new typed threshold would establish equality, so
                // this control independently requires its binding guard.
                let threshold_only = ORDINARY.replace("100i32, 100", "100, 100i32");
                fixture.write(
                    "tests/boundary.rs",
                    &format!("{declaration}\n{threshold_only}"),
                )?;
                let label = format!("ordinary-typed-threshold-shadow-{name}");
                let (result, _) = report(&fixture, &label, None, Some(&cache))?;
                assert_ne!(
                    boundary(&result)?["grip_class"],
                    "strongly_gripped",
                    "{label}: {result}"
                );
            }
        }
    }
    Ok(())
}

#[test]
fn configured_oracle_cannot_turn_a_registered_property_into_an_ordinary_test() -> Result<(), String>
{
    let fixture = Fixture::new()?;
    let cache = fixture.0.join("target/repository-cache");
    let source =
        "pub fn expect_ready(amount: i32, threshold: i32) -> bool {\n    amount >= threshold\n}\n";
    let property = "proptest::proptest! {\n    #[test]\n    fn exact_boundary(amount in proptest::strategy::Just(7i32)) {\n        proptest::prop_assert!(lib_proptest::expect_ready(100, 100));\n    }\n}\n";
    let shadow = format!(
        "mod lib_proptest {{ pub fn expect_ready(_: i32, _: i32) -> bool {{ true }} }}\n{property}"
    );
    fixture.write("src/lib.rs", source)?;
    fixture.write("tests/boundary.rs", property)?;
    fixture.write(
        "diff.patch",
        "--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,3 @@\n pub fn expect_ready(amount: i32, threshold: i32) -> bool {\n-    amount > threshold\n+    amount >= threshold\n }\n",
    )?;
    let (canonical, _) = report(&fixture, "policy-real-default", None, Some(&cache))?;
    assert_eq!(canonical["run_status"], "complete");
    let canonical_boundary = canonical["seams"]
        .as_array()
        .ok_or("missing canonical seams")?
        .iter()
        .filter(|seam| {
            seam["owner"] == "src/lib.rs::expect_ready" && seam["kind"] == "predicate_boundary"
        })
        .collect::<Vec<_>>();
    let [canonical_boundary] = canonical_boundary.as_slice() else {
        return Err(format!("expected one canonical bool boundary: {canonical}"));
    };
    assert_eq!(canonical_boundary["related_tests_total"], 1, "{canonical}");

    // The existing expect_ spelling heuristic gives this assertion a mock
    // oracle. The real supported policy changes its strength after property
    // registration, invalidating exact TestFact equality. Neither that
    // mutation nor unsuffixed, parameter-independent arguments may lend a
    // shadowed function's evidence to the production owner.
    for (label, test_source, configured) in [
        ("policy-shadow-default", shadow.as_str(), false),
        ("policy-shadow-configured", shadow.as_str(), true),
        ("policy-real-configured", property, true),
    ] {
        fixture.write("tests/boundary.rs", test_source)?;
        if configured {
            fixture.write(
                "ripr.toml",
                "[oracles]\nmock_expectation_strength = \"strong\"\n",
            )?;
        }
        let (result, _) = report(&fixture, label, None, Some(&cache))?;
        assert_eq!(result["run_status"], "complete", "{label}: {result}");
        let owned = result["seams"]
            .as_array()
            .ok_or("missing policy-control seams")?
            .iter()
            .filter(|seam| seam["owner"] == "src/lib.rs::expect_ready")
            .collect::<Vec<_>>();
        assert!(!owned.is_empty(), "{label}: {result}");
        assert!(
            owned
                .iter()
                .any(|seam| seam["kind"] == "predicate_boundary"),
            "{label}: {result}"
        );
        for seam in owned {
            assert_eq!(seam["related_tests_total"], 0, "{label}: {result}");
            assert_eq!(
                seam["observed_values"].as_array().map(Vec::len),
                Some(0),
                "{label}: {result}"
            );
        }
        if configured {
            let (diff, _) = report(&fixture, &format!("{label}-diff"), None, None)?;
            assert_eq!(
                diff["analysis_outcome"]["analysis_complete"], true,
                "{label}: {diff}"
            );
            let findings = diff["findings"].as_array().ok_or("missing bool findings")?;
            // This Boolean tail is both a changed predicate and a returned
            // value. Neither subject may borrow the property's evidence.
            assert_eq!(findings.len(), 2, "{label}: {diff}");
            for (kind, family) in [
                ("predicate_boundary", "predicate"),
                ("return_value", "return_value"),
            ] {
                let matching = findings
                    .iter()
                    .filter(|finding| {
                        finding["canonical_gap"]["file"] == "src/lib.rs"
                            && finding["canonical_gap"]["owner"] == "expect_ready"
                            && finding["canonical_gap"]["behavior_kind"] == kind
                            && finding["probe"]["family"] == family
                            && finding["probe"]["line"] == 2
                            && finding["probe"]["expression"] == "amount >= threshold"
                    })
                    .collect::<Vec<_>>();
                let [finding] = matching.as_slice() else {
                    return Err(format!("{label}: expected one {kind} finding: {diff}"));
                };
                assert_eq!(finding["classification"], "no_static_path", "{diff}");
                assert_eq!(finding["related_tests_total"], 0, "{diff}");
                for values in [
                    &finding["related_tests"],
                    &finding["observed_values"],
                    &finding["activation"]["observed_values"],
                ] {
                    assert_eq!(values.as_array().map(Vec::len), Some(0), "{diff}");
                }
                assert_eq!(
                    finding["assertion_texts"]
                        .as_object()
                        .map(serde_json::Map::len),
                    Some(0),
                    "{diff}"
                );
                assert_eq!(finding["oracle_kind"], "unknown", "{diff}");
                assert_eq!(finding["oracle_strength"], "none", "{diff}");
                for stage in ["reach", "observe", "discriminate"] {
                    assert_eq!(finding["ripr"][stage]["state"], "no", "{diff}");
                }
            }
        }
    }
    std::fs::remove_file(fixture.0.join("ripr.toml")).map_err(|error| error.to_string())?;
    fixture.write("tests/boundary.rs", property)?;
    let (restored, _) = report(&fixture, "policy-real-restored", None, Some(&cache))?;
    assert_eq!(restored["seams"], canonical["seams"]);
    Ok(())
}

#[test]
fn property_mixed_owner_calls_cannot_lend_each_other_boundary_values() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let cache = fixture.0.join("target/repository-cache");
    let mixed = format!(
        "mod other {{ pub const discounted_total: fn(i32, i32) -> i32 = |_, _| 90; }}\n{}",
        PROPERTY.replace(
            "proptest::prop_assert_eq!(lib_proptest::discounted_total(amount, 100), 90);",
            "proptest::prop_assert_eq!(lib_proptest::discounted_total(101, 100), 91);\n    proptest::prop_assert_eq!(other::discounted_total(100, 100), 90);",
        ),
    );
    fixture.write("tests/boundary.rs", &mixed)?;
    let (result, _) = report(&fixture, "mixed-owner-property", None, Some(&cache))?;
    let seam = boundary(&result)?;
    assert_eq!(seam["related_tests_total"], 0, "{result}");
    assert_eq!(
        seam["observed_values"].as_array().map(Vec::len),
        Some(0),
        "{result}"
    );
    assert_ne!(seam["grip_class"], "strongly_gripped", "{result}");
    let diff = finding(&fixture, "mixed-owner-property-diff", None)?;
    assert_eq!(diff["related_tests_total"], 0, "{diff}");
    for values in [
        &diff["observed_values"],
        &diff["activation"]["observed_values"],
    ] {
        assert_eq!(values.as_array().map(Vec::len), Some(0), "{diff}");
    }

    // The same declaration and both literal assertions are readable when
    // every call names the real owner. An any-call identity test would
    // wrongly accept the mixed property above and borrow the other boundary.
    fixture.write(
        "tests/boundary.rs",
        &mixed.replace("other::discounted_total", "lib_proptest::discounted_total"),
    )?;
    let (result, _) = report(&fixture, "all-owner-property", None, Some(&cache))?;
    assert_eq!(
        boundary(&result)?["grip_class"],
        "strongly_gripped",
        "{result}"
    );
    assert_eq!(
        finding(&fixture, "all-owner-property-diff", None)?["classification"],
        "exposed"
    );
    Ok(())
}

#[test]
fn canonical_property_is_exposed_and_revokes_warm_configuration_and_target_receipts()
-> Result<(), String> {
    let fixture = Fixture::new()?;
    let cold = finding(&fixture, "cold", None)?;
    assert_eq!(cold["classification"], "exposed", "{cold}");
    assert!(
        cold["related_tests"]
            .as_array()
            .is_some_and(|tests| tests.iter().any(|test| test["name"] == "exact_boundary")),
        "{cold}"
    );
    assert_eq!(
        finding(&fixture, "warm", None)?["classification"],
        "exposed"
    );
    assert_ne!(
        finding(&fixture, "zero-cases", Some("0"))?["classification"],
        "exposed"
    );
    assert_ne!(
        finding(&fixture, "override-cases", Some("1"))?["classification"],
        "exposed",
        "all overrides are outside the initial profile"
    );
    assert_eq!(
        finding(&fixture, "restored-env", None)?["classification"],
        "exposed"
    );
    std::fs::create_dir(fixture.0.join(".cargo")).map_err(|error| error.to_string())?;
    fixture.write(".cargo/config.toml", "[env]\nPROPTEST_CASES='0'\n")?;
    assert_ne!(
        finding(&fixture, "cargo-env", None)?["classification"],
        "exposed"
    );
    std::fs::remove_file(fixture.0.join(".cargo/config.toml"))
        .map_err(|error| error.to_string())?;
    for (name, suffix) in [
        (
            "harness",
            "\n[[test]]\nname='boundary'\npath='tests/boundary.rs'\nharness=false\n",
        ),
        (
            "test-off",
            "\n[[test]]\nname='boundary'\npath='tests/boundary.rs'\ntest=false\n",
        ),
        (
            "feature",
            "\n[[test]]\nname='boundary'\npath='tests/boundary.rs'\nrequired-features=['not_enabled']\n[features]\nnot_enabled=[]\n",
        ),
    ] {
        fixture.write("Cargo.toml", &format!("{MANIFEST}{suffix}"))?;
        assert_ne!(
            finding(&fixture, name, None)?["classification"],
            "exposed",
            "{name}"
        );
    }
    fixture.write("Cargo.toml", MANIFEST)?;
    // An added unrelated source must not remove the positive or let a stale
    // registration survive its subsequent source edit.
    fixture.write("tests/other.rs", "#[test]\nfn other() {}\n")?;
    assert_eq!(
        finding(&fixture, "augmented", None)?["classification"],
        "exposed"
    );
    fixture.write(
        "tests/boundary.rs",
        &PROPERTY.replace("#[test]", "#[ignore]\n    #[test]"),
    )?;
    assert_ne!(
        finding(&fixture, "ignored-warm", None)?["classification"],
        "exposed"
    );
    fixture.write("tests/boundary.rs", PROPERTY)?;
    assert_eq!(
        finding(&fixture, "restored-source", None)?["classification"],
        "exposed"
    );
    Ok(())
}

#[test]
fn raw_library_shadows_cannot_supply_generated_tests_or_activation_rows() -> Result<(), String> {
    let fixture = Fixture::new()?;
    for (name, declaration) in [
        (
            "raw-library-module",
            "mod r#lib_proptest { pub fn discounted_total(_: i32, _: i32) -> i32 { 90 } }",
        ),
        (
            "raw-library-type",
            "struct r#lib_proptest;\nimpl lib_proptest { fn discounted_total(_: i32, _: i32) -> i32 { 90 } }",
        ),
    ] {
        // Both local paths return 90 without calling the changed library
        // owner. A final non-exposed verdict alone would not detect borrowed
        // generated-test or exact-boundary authority at an earlier stage.
        fixture.write("tests/boundary.rs", &format!("{declaration}\n{PROPERTY}"))?;
        let result = finding(&fixture, name, None)?;
        assert_eq!(result["related_tests_total"], 0, "{name}: {result}");
        assert_eq!(
            result["related_tests"].as_array().map(Vec::len),
            Some(0),
            "{name}: {result}"
        );
        assert_eq!(
            result["static_limit_kind"], "rust_macro_reach_unresolved",
            "{name}: {result}"
        );
        for values in [
            &result["observed_values"],
            &result["activation"]["observed_values"],
        ] {
            assert_eq!(values.as_array().map(Vec::len), Some(0), "{name}: {result}");
        }
    }
    Ok(())
}

#[test]
fn unsupported_property_shapes_never_borrow_direct_assertion_credit() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let statement = "proptest::prop_assert_eq!(lib_proptest::discounted_total(amount, 100), 90);";
    let cases = [
        ("missing-registration", PROPERTY.replace("#[test]", "")),
        ("prelude", PROPERTY.replace("proptest::proptest!", "proptest!").replace("proptest::strategy::Just", "Just").replace("proptest::prop_assert_eq", "prop_assert_eq")),
        ("range", PROPERTY.replace("proptest::strategy::Just(100i32)", "0i32..200")),
        ("unsuffixed-input", PROPERTY.replace("Just(100i32)", "Just(100)")),
        ("empty-range", PROPERTY.replace("proptest::strategy::Just(100i32)", "0i32..0")),
        ("filter", PROPERTY.replace("proptest::strategy::Just(100i32)", "proptest::strategy::Just(100i32).prop_filter(\"reject all\", |_| false)")),
        ("custom-config", PROPERTY.replace("proptest::proptest! {", "proptest::proptest! {\n#![proptest_config(proptest::test_runner::Config::with_cases(0))]")),
        ("closure", PROPERTY.replace(statement, &format!("let _ignored = || {{ {statement} Ok(()) }};"))),
        ("early-ok", PROPERTY.replace(statement, &format!("return Ok(()); {statement}"))),
        ("helper-result", PROPERTY.replace(statement, "let _ = helper(amount);")),
        ("loop", PROPERTY.replace(statement, &format!("for _ in 0..1 {{ {statement} }}"))),
        ("self-comparison", PROPERTY.replace(", 90);", ", lib_proptest::discounted_total(amount, 100));")),
        ("diagnostic-return", PROPERTY.replace(", 90);", ", 90, \"{}\", { return Ok(()); \"discarded\" });")),
        ("callee-shadow", PROPERTY.replace("amount in", "discounted_total in").replace("lib_proptest::discounted_total(amount, 100)", "discounted_total(100, 100)")),
        ("wrong-owner", PROPERTY.replace("lib_proptest::discounted_total", "other::discounted_total")),
        ("shadow-library-module", format!("mod lib_proptest {{ pub fn discounted_total(_: i32, _: i32) -> i32 {{ 90 }} }}\n{PROPERTY}")),
        ("shadow-library-type", format!("struct lib_proptest;\nimpl lib_proptest {{ fn discounted_total(_: i32, _: i32) -> i32 {{ 90 }} }}\n{PROPERTY}")),
        ("weak-ne", PROPERTY.replace("prop_assert_eq", "prop_assert_ne").replace(", 90);", ", 0);")),
        ("rebound-root", format!("mod proptest {{}}\n{PROPERTY}")),
        ("rebound-attribute", format!("use foreign::test;\n{PROPERTY}")),
        ("inline-module", format!("mod inner {{\n{PROPERTY}\n}}")),
        ("same-line-module", format!("  mod inner {{ {} }}", PROPERTY.replace('\n', " "))),
    ];
    for (name, property) in cases {
        assert_ne!(property, PROPERTY, "{name}: negative must change a premise");
        fixture.write("tests/boundary.rs", &property)?;
        let result = finding(&fixture, name, None)?;
        assert_ne!(result["classification"], "exposed", "{name}: {result}");
    }
    // A same-line module has a different value namespace even when the
    // generated test's line starts outside its scope. Its const function
    // pointer always returns 90, independently of the changed root owner.
    fixture.write("tests/boundary.rs", "")?;
    fixture.write("src/lib.rs", &format!("{SOURCE}\n  mod inner {{ const discounted_total: fn(i32, i32) -> i32 = |_, _| 90; proptest::proptest! {{ #[test] fn shadow(amount in proptest::strategy::Just(100i32)) {{ proptest::prop_assert_eq!(discounted_total(amount, 100), 90); }} }} }}\n"))?;
    assert_ne!(
        finding(&fixture, "same-line-const-shadow", None)?["classification"],
        "exposed"
    );
    // Refusal must not erase a separately supported ordinary unit assertion.
    fixture.write(
        "src/lib.rs",
        &format!(
            "{SOURCE}\n#[test]\nfn ordinary() {{ assert_eq!(discounted_total(100, 100), 90); }}\n"
        ),
    )?;
    fixture.write("tests/boundary.rs", &PROPERTY.replace("#[test]", ""))?;
    assert_eq!(
        finding(&fixture, "independent-ordinary", None)?["classification"],
        "exposed"
    );
    Ok(())
}

#[test]
fn substituted_provider_and_lock_changes_revoke_the_profile() -> Result<(), String> {
    let fixture = Fixture::new()?;
    assert_eq!(
        finding(&fixture, "before-provider-change", None)?["classification"],
        "exposed"
    );
    fixture.write(
        "Cargo.lock",
        &LOCK.replace(
            "4b45fcc2344c680f5025fe57779faef368840d0bd1f42f216291f0dc4ace4744",
            "0000000000000000000000000000000000000000000000000000000000000000",
        ),
    )?;
    assert_ne!(
        finding(&fixture, "lock-checksum", None)?["classification"],
        "exposed"
    );
    std::fs::create_dir(fixture.0.join("fake")).map_err(|error| error.to_string())?;
    fixture.write(
        "fake/Cargo.toml",
        "[package]\nname='proptest'\nversion='1.11.0'\nedition='2021'\n[lib]\npath='lib.rs'\n",
    )?;
    fixture.write(
        "fake/lib.rs",
        "#[macro_export]\nmacro_rules! proptest { ($($ignored:tt)*) => {} }\n",
    )?;
    fixture.write(
        "Cargo.toml",
        &MANIFEST.replace("proptest = \"=1.11.0\"", "proptest = { path='fake' }"),
    )?;
    fixture.write("Cargo.lock", "version=4\n[[package]]\nname='lib_proptest'\nversion='0.1.0'\ndependencies=['proptest']\n[[package]]\nname='proptest'\nversion='1.11.0'\n")?;
    let result = finding(&fixture, "path-provider", None)?;
    assert_ne!(result["classification"], "exposed", "{result}");
    assert_eq!(
        result["static_limit_kind"], "rust_macro_reach_unresolved",
        "{result}"
    );
    Ok(())
}

fn admitted_property(fixture: &Fixture, label: &str) -> Result<(Value, Value), String> {
    let diff = finding(fixture, &format!("{label}-diff"), None)?;
    assert_eq!(diff["classification"], "exposed", "{diff}");
    assert_eq!(diff["related_tests_total"], 1, "{diff}");
    assert_eq!(diff["related_tests"][0]["name"], "exact_boundary", "{diff}");
    let cache = fixture.0.join("target/acceptance-cache");
    let (repo, _) = report(fixture, &format!("{label}-repo"), None, Some(&cache))?;
    let seam = boundary(&repo)?;
    assert_eq!(seam["grip_class"], "strongly_gripped", "{repo}");
    assert_eq!(seam["related_tests_total"], 1, "{repo}");
    Ok((diff, seam))
}

fn refused_property(fixture: &Fixture, label: &str) -> Result<(), String> {
    let diff = finding(fixture, &format!("{label}-diff"), None)?;
    assert_eq!(diff["classification"], "no_static_path", "{diff}");
    assert_eq!(diff["related_tests_total"], 0, "{diff}");
    for values in [
        &diff["related_tests"],
        &diff["observed_values"],
        &diff["activation"]["observed_values"],
    ] {
        assert_eq!(values.as_array().map(Vec::len), Some(0), "{diff}");
    }
    assert_eq!(
        diff["assertion_texts"]
            .as_object()
            .map(serde_json::Map::len),
        Some(0),
        "{diff}"
    );
    assert_eq!(diff["oracle_kind"], "unknown", "{diff}");
    assert_eq!(diff["oracle_strength"], "none", "{diff}");
    assert_eq!(
        diff["static_limit_kind"], "rust_macro_reach_unresolved",
        "{diff}"
    );
    let cache = fixture.0.join("target/acceptance-cache");
    let (repo, _) = report(fixture, &format!("{label}-repo"), None, Some(&cache))?;
    let seam = boundary(&repo)?;
    assert_ne!(seam["grip_class"], "strongly_gripped", "{repo}");
    assert_eq!(seam["related_tests_total"], 0, "{repo}");
    for key in ["related_tests", "observed_values"] {
        assert_eq!(seam[key].as_array().map(Vec::len), Some(0), "{repo}");
    }
    Ok(())
}

fn metadata(fixture: &Fixture, label: &str) -> Result<Value, String> {
    let output = run(
        fixture,
        OsStr::new("cargo"),
        &["metadata", "--offline", "--locked", "--format-version", "1"].map(OsStr::new),
        label,
        None,
        false,
        None,
    )?;
    assert!(
        output.status.success(),
        "{label}: metadata setup failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).map_err(|error| error.to_string())
}

fn package<'a>(metadata: &'a Value, name: &str) -> Result<&'a Value, String> {
    let matches = metadata["packages"]
        .as_array()
        .ok_or("missing Cargo packages")?
        .iter()
        .filter(|package| package["name"] == name)
        .collect::<Vec<_>>();
    let [package] = matches.as_slice() else {
        return Err(format!("expected one resolved {name} package: {metadata}"));
    };
    Ok(package)
}

#[test]
fn patched_registry_dependency_cannot_register_a_property() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let before = admitted_property(&fixture, "before-patch")?;
    std::fs::create_dir(fixture.0.join("fake")).map_err(|error| error.to_string())?;
    fixture.write(
        "fake/Cargo.toml",
        "[package]\nname='proptest'\nversion='1.11.0'\nedition='2021'\n[lib]\npath='lib.rs'\n",
    )?;
    fixture.write(
        "fake/lib.rs",
        "#[macro_export]\nmacro_rules! proptest { ($($ignored:tt)*) => {} }\n",
    )?;
    fixture.write(
        "Cargo.toml",
        &format!("{MANIFEST}\n[patch.crates-io]\nproptest={{path='fake'}}\n"),
    )?;
    fixture.write("Cargo.lock", "version=4\n[[package]]\nname='lib_proptest'\nversion='0.1.0'\ndependencies=['proptest']\n[[package]]\nname='proptest'\nversion='1.11.0'\n")?;
    let resolved = metadata(&fixture, "patch-metadata")?;
    let provider = package(&resolved, "proptest")?;
    assert_eq!(provider["version"], "1.11.0");
    assert!(provider["source"].is_null(), "{resolved}");
    let expected = fixture
        .0
        .join("fake/Cargo.toml")
        .canonicalize()
        .map_err(|error| error.to_string())?;
    assert_eq!(
        Path::new(
            provider["manifest_path"]
                .as_str()
                .ok_or("missing patched manifest")?
        )
        .canonicalize()
        .map_err(|error| error.to_string())?,
        expected
    );
    refused_property(&fixture, "patched")?;
    fixture.write("Cargo.toml", MANIFEST)?;
    fixture.write("Cargo.lock", LOCK)?;
    std::fs::remove_dir_all(fixture.0.join("fake")).map_err(|error| error.to_string())?;
    assert_eq!(admitted_property(&fixture, "patch-restored")?, before);
    Ok(())
}

#[test]
fn mixed_build_dependency_cannot_register_a_property() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let before = admitted_property(&fixture, "before-build-edge")?;
    // Retain the canonical dev declaration: a build-only manifest would be
    // refused before the resolved dependency-context consumer is reached.
    fixture.write(
        "Cargo.toml",
        &format!("{MANIFEST}\n[build-dependencies]\nproptest='=1.11.0'\n"),
    )?;
    let resolved = metadata(&fixture, "build-edge-metadata")?;
    let owner = package(&resolved, "lib_proptest")?;
    let provider = package(&resolved, "proptest")?;
    assert_eq!(provider["version"], "1.11.0");
    assert_eq!(
        provider["source"],
        "registry+https://github.com/rust-lang/crates.io-index"
    );
    let node = resolved["resolve"]["nodes"]
        .as_array()
        .ok_or("missing resolved nodes")?
        .iter()
        .find(|node| node["id"] == owner["id"])
        .ok_or("missing owner node")?;
    let edges = node["deps"]
        .as_array()
        .ok_or("missing dependency edges")?
        .iter()
        .filter(|edge| edge["name"] == "proptest")
        .collect::<Vec<_>>();
    let [edge] = edges.as_slice() else {
        return Err(format!("expected one proptest edge: {resolved}"));
    };
    assert_eq!(edge["pkg"], provider["id"]);
    let kinds = edge["dep_kinds"]
        .as_array()
        .ok_or("missing dependency contexts")?;
    assert_eq!(kinds.len(), 2, "{resolved}");
    assert!(
        kinds.iter().all(|kind| kind["target"].is_null()),
        "{resolved}"
    );
    assert!(kinds.iter().any(|kind| kind["kind"] == "dev"), "{resolved}");
    assert!(
        kinds.iter().any(|kind| kind["kind"] == "build"),
        "{resolved}"
    );
    refused_property(&fixture, "build-edge")?;
    fixture.write("Cargo.toml", MANIFEST)?;
    assert_eq!(admitted_property(&fixture, "build-edge-restored")?, before);
    Ok(())
}

#[test]
fn unreferenced_property_file_cannot_borrow_an_ordinary_test_target() -> Result<(), String> {
    let fixture = Fixture::new()?;
    fixture.write(
        "src/lib.rs",
        &format!("{SOURCE}\npub fn ordinary_total(value: i32) -> i32 {{ value + 1 }}\n"),
    )?;
    fixture.write(
        "tests/ordinary.rs",
        "#[test]\nfn ordinary_witness() { assert_eq!(lib_proptest::ordinary_total(2), 3); }\n",
    )?;
    let before = admitted_property(&fixture, "before-unreferenced")?;
    let ordinary = |label: &str| -> Result<Vec<Value>, String> {
        let cache = fixture.0.join("target/acceptance-cache");
        let (report, _) = report(&fixture, label, None, Some(&cache))?;
        let seams = report["seams"]
            .as_array()
            .ok_or("missing ordinary seams")?
            .iter()
            .filter(|seam| seam["owner"] == "src/lib.rs::ordinary_total")
            .cloned()
            .collect::<Vec<_>>();
        assert!(!seams.is_empty(), "{report}");
        assert!(
            seams.iter().any(|seam| seam["related_tests_total"] == 1),
            "{report}"
        );
        Ok(seams)
    };
    let ordinary_before = ordinary("ordinary-before")?;
    std::fs::create_dir(fixture.0.join("tests/support")).map_err(|error| error.to_string())?;
    std::fs::rename(
        fixture.0.join("tests/boundary.rs"),
        fixture.0.join("tests/support/property.rs"),
    )
    .map_err(|error| error.to_string())?;
    let resolved = metadata(&fixture, "unreferenced-metadata")?;
    let owner = package(&resolved, "lib_proptest")?;
    let targets = owner["targets"].as_array().ok_or("missing Cargo targets")?;
    let unreferenced = fixture
        .0
        .join("tests/support/property.rs")
        .canonicalize()
        .map_err(|error| error.to_string())?;
    let ordinary_path = fixture
        .0
        .join("tests/ordinary.rs")
        .canonicalize()
        .map_err(|error| error.to_string())?;
    let target_paths = targets
        .iter()
        .map(|target| {
            Path::new(target["src_path"].as_str().ok_or("missing target source")?)
                .canonicalize()
                .map_err(|error| error.to_string())
        })
        .collect::<Result<Vec<_>, String>>()?;
    assert!(!target_paths.contains(&unreferenced), "{resolved}");
    assert!(target_paths.contains(&ordinary_path), "{resolved}");
    refused_property(&fixture, "unreferenced")?;
    assert_eq!(ordinary("ordinary-unreferenced")?, ordinary_before);
    std::fs::rename(&unreferenced, fixture.0.join("tests/boundary.rs"))
        .map_err(|error| error.to_string())?;
    std::fs::remove_dir(fixture.0.join("tests/support")).map_err(|error| error.to_string())?;
    assert_eq!(
        admitted_property(&fixture, "unreferenced-restored")?,
        before
    );
    assert_eq!(ordinary("ordinary-restored")?, ordinary_before);
    Ok(())
}

#[test]
fn active_unsupported_cfg_cannot_register_a_property() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let before = admitted_property(&fixture, "before-cfg")?;
    let configured = PROPERTY.replace("#[test]", "#[cfg(all())]\n    #[test]");
    assert_ne!(configured, PROPERTY);
    fixture.write("tests/boundary.rs", &configured)?;
    let listed = cargo(
        &fixture,
        "cfg-list",
        &["--", "--list", "--format", "terse"],
        None,
    )?;
    assert!(
        listed.status.success(),
        "{}",
        String::from_utf8_lossy(&listed.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&listed.stdout)
            .lines()
            .filter(|line| line.ends_with(": test"))
            .collect::<Vec<_>>(),
        ["exact_boundary: test"]
    );
    let native = cargo(
        &fixture,
        "cfg-native",
        &["exact_boundary", "--", "--exact"],
        None,
    )?;
    assert!(
        native.status.success(),
        "{}",
        String::from_utf8_lossy(&native.stderr)
    );
    assert!(String::from_utf8_lossy(&native.stdout).contains("1 passed; 0 failed"));
    refused_property(&fixture, "active-unsupported-cfg")?;
    fixture.write("tests/boundary.rs", PROPERTY)?;
    assert_eq!(admitted_property(&fixture, "cfg-restored")?, before);
    Ok(())
}

fn cargo(
    fixture: &Fixture,
    label: &str,
    extra: &[&str],
    cases: Option<&str>,
) -> Result<Output, String> {
    let arguments = ["test", "--offline", "--locked", "--test", "boundary"]
        .into_iter()
        .chain(extra.iter().copied())
        .map(OsStr::new)
        .collect::<Vec<_>>();
    run(
        fixture,
        OsStr::new("cargo"),
        &arguments,
        label,
        cases,
        false,
        None,
    )
}

#[test]
fn real_property_collects_and_rejects_wrong_owner_while_zero_cases_and_diagnostics_do_not()
-> Result<(), String> {
    let fixture = Fixture::new()?;
    let list = cargo(
        &fixture,
        "native-list",
        &["--", "--list", "--format", "terse"],
        None,
    )?;
    assert!(
        list.status.success(),
        "native setup: {}",
        String::from_utf8_lossy(&list.stderr)
    );
    let listed = String::from_utf8_lossy(&list.stdout);
    assert_eq!(
        listed
            .lines()
            .filter(|line| line.ends_with(": test"))
            .collect::<Vec<_>>(),
        ["exact_boundary: test"]
    );
    let correct = cargo(
        &fixture,
        "native-correct",
        &["exact_boundary", "--", "--exact"],
        None,
    )?;
    assert!(
        correct.status.success(),
        "correct owner failed: {}",
        String::from_utf8_lossy(&correct.stdout)
    );
    assert!(String::from_utf8_lossy(&correct.stdout).contains("1 passed; 0 failed"));
    fixture.write(
        "src/lib.rs",
        &SOURCE.replace("amount >= threshold", "amount > threshold"),
    )?;
    let wrong = cargo(
        &fixture,
        "native-wrong",
        &["exact_boundary", "--", "--exact", "--nocapture"],
        None,
    )?;
    assert_eq!(
        wrong.status.code(),
        Some(101),
        "{}",
        String::from_utf8_lossy(&wrong.stderr)
    );
    let wrong_text = format!(
        "{}{}",
        String::from_utf8_lossy(&wrong.stdout),
        String::from_utf8_lossy(&wrong.stderr)
    );
    assert!(
        wrong_text.contains("exact_boundary") && wrong_text.contains("0 passed; 1 failed"),
        "compiler/setup failure is not discrimination: {wrong_text}"
    );
    let compact = wrong_text.split_whitespace().collect::<String>();
    assert!(
        compact.contains("left:`100`")
            && compact.contains("right:`90`")
            && compact.contains("minimalfailinginput:amount=100"),
        "wrong owner must reach the unchanged equality: {wrong_text}"
    );
    let zero = cargo(
        &fixture,
        "native-zero",
        &["exact_boundary", "--", "--exact"],
        Some("0"),
    )?;
    assert!(
        zero.status.success(),
        "zero cases is an intentional vacuity control"
    );
    assert!(String::from_utf8_lossy(&zero.stdout).contains("1 passed; 0 failed"));
    fixture.write(
        "tests/boundary.rs",
        &PROPERTY.replace(", 90);", ", 90, \"{}\", { return Ok(()); \"discarded\" });"),
    )?;
    let discarded = cargo(
        &fixture,
        "native-diagnostic-return",
        &["exact_boundary", "--", "--exact"],
        None,
    )?;
    assert!(
        discarded.status.success(),
        "diagnostic return must demonstrate the failure-propagation counterexample: {}",
        String::from_utf8_lossy(&discarded.stderr)
    );
    assert!(String::from_utf8_lossy(&discarded.stdout).contains("1 passed; 0 failed"));
    fixture.write(
        "tests/boundary.rs",
        &PROPERTY
            .replace("prop_assert_eq", "prop_assert_ne")
            .replace(", 90);", ", 0);"),
    )?;
    let weak = cargo(
        &fixture,
        "native-weak-ne",
        &["exact_boundary", "--", "--exact"],
        None,
    )?;
    assert!(
        weak.status.success(),
        "a nonzero check does not distinguish 90 from 100"
    );
    assert!(String::from_utf8_lossy(&weak.stdout).contains("1 passed; 0 failed"));
    fixture.write("src/lib.rs", SOURCE)?;
    let weak_correct = cargo(
        &fixture,
        "native-weak-ne-correct",
        &["exact_boundary", "--", "--exact"],
        None,
    )?;
    assert!(weak_correct.status.success());
    assert!(String::from_utf8_lossy(&weak_correct.stdout).contains("1 passed; 0 failed"));
    Ok(())
}
