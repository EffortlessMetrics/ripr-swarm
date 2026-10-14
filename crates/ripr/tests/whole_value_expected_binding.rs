//! Public-path and independently valued runtime controls for SPEC-0225 example 5.

use ripr::{CheckInput, Mode, OutputFormat, check_workspace, render_check};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::process::{Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

const SOURCE: &str = include_str!(
    "../../../fixtures/rust-verdict-corpus/subjects/authored-spec0225-5/src/lib.rs.txt"
);
const MANIFEST: &str =
    include_str!("../../../fixtures/rust-verdict-corpus/subjects/authored-spec0225-5/Cargo.toml");
const DIFF: &str =
    include_str!("../../../fixtures/rust-verdict-corpus/cases/spec0225-wv-expected-binding.diff");

struct Scratch(PathBuf);

impl Scratch {
    fn create() -> Result<Self, String> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-expected-binding-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(std::env::temp_dir()).map_err(|error| error.to_string())?;
        std::fs::create_dir(&root).map_err(|error| error.to_string())?;
        Ok(Self(root))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn source(body: &str) -> Result<String, String> {
    let (production, _) = SOURCE
        .split_once("#[cfg(test)]")
        .ok_or("missing retained tests")?;
    Ok(format!(
        "{}#[cfg(test)]\nmod tests {{\nuse super::*;\n#[test]\nfn builds_config() {{\n{body}\n}}\n}}\n",
        production.replace("retries: n + 1,", "retries: 1 + n,")
    ))
}

const STREAM_LIMIT: u64 = 64 * 1024;

fn read_bounded_stream(path: &Path) -> Result<Vec<u8>, String> {
    let file = std::fs::File::open(path).map_err(|error| error.to_string())?;
    let mut bytes = Vec::new();
    file.take(STREAM_LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > STREAM_LIMIT {
        return Err(format!(
            "stream {} exceeds {STREAM_LIMIT} bytes",
            path.display()
        ));
    }
    Ok(bytes)
}

// File-backed streams avoid pipe backpressure. The shared process owner
// terminates and reaps on timeout before Scratch removes its owned directory.
fn run_bounded(
    mut command: Command,
    root: &Path,
    label: &str,
    budget: Duration,
) -> Result<Output, String> {
    let stdout = root.join(format!("{label}.stdout"));
    let stderr = root.join(format!("{label}.stderr"));
    command
        .stdin(Stdio::null())
        .stdout(std::fs::File::create(&stdout).map_err(|error| error.to_string())?)
        .stderr(std::fs::File::create(&stderr).map_err(|error| error.to_string())?);
    let mut child =
        ripr::process_owner::OwnedProcess::spawn(command).map_err(|error| error.to_string())?;
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
            break status;
        }
        if started.elapsed() >= budget {
            child.terminate_tree()?;
            return Err(format!(
                "{label} exceeded its {budget:?} budget; owned process terminated and reaped"
            ));
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    Ok(Output {
        status,
        stdout: read_bounded_stream(&stdout)?,
        stderr: read_bounded_stream(&stderr)?,
    })
}

fn runtime(source: &str, fails: bool, label: &str) -> Result<(), String> {
    // Runtime sources live outside the analyzed root; the index sees only the
    // one intended lib.rs and cannot borrow a mutant's or duplicate test's oracle.
    let scratch = Scratch::create()?;
    let input = scratch.0.join("runtime.rs");
    let binary = scratch
        .0
        .join(format!("runtime{}", std::env::consts::EXE_SUFFIX));
    std::fs::write(&input, source).map_err(|error| error.to_string())?;
    let mut compile = Command::new("rustc");
    compile
        .args(["--edition=2024", "--test"])
        .arg(&input)
        .arg("-o")
        .arg(&binary);
    let compiled = run_bounded(compile, &scratch.0, "compile", Duration::from_mins(2))?;
    assert!(
        compiled.status.success(),
        "{label}: compile failed: {}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    let mut execute = Command::new(&binary);
    execute.args(["--exact", "tests::builds_config"]);
    let result = run_bounded(execute, &scratch.0, "execute", Duration::from_secs(10))?;
    let stdout = String::from_utf8_lossy(&result.stdout);
    assert!(
        stdout.contains("running 1 test"),
        "{label}: nonempty executed test: {stdout}"
    );
    assert_eq!(result.status.success(), !fails, "{label}: {stdout}");
    Ok(())
}

fn classification(source: &str, exposed: bool, label: &str) -> Result<(), String> {
    let scratch = Scratch::create()?;
    std::fs::create_dir(scratch.0.join("src")).map_err(|error| error.to_string())?;
    std::fs::write(scratch.0.join("Cargo.toml"), MANIFEST).map_err(|error| error.to_string())?;
    std::fs::write(scratch.0.join("src/lib.rs"), source).map_err(|error| error.to_string())?;
    let diff = scratch.0.join("change.diff");
    std::fs::write(&diff, DIFF).map_err(|error| error.to_string())?;
    let report = check_workspace(CheckInput {
        root: scratch.0.clone(),
        diff_file: Some(diff),
        mode: Mode::Fast,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })?;
    let json: serde_json::Value =
        serde_json::from_str(&render_check(&report, &OutputFormat::Json)?)
            .map_err(|error| error.to_string())?;
    let findings = json["findings"].as_array().ok_or("missing findings")?;
    assert_eq!(
        findings.len(),
        1,
        "{label}: one anchored field subject: {json}"
    );
    let finding = &findings[0];
    assert_eq!(
        finding["probe"]["file"]
            .as_str()
            .ok_or("missing probe path")?
            .trim_start_matches("./"),
        "src/lib.rs",
        "{label}"
    );
    assert_eq!(finding["probe"]["line"], 11, "{label}");
    assert_eq!(finding["probe"]["family"], "field_construction", "{label}");
    let tests = finding["related_tests"]
        .as_array()
        .ok_or("missing related tests")?;
    assert_eq!(tests.len(), 1, "{label}: one related test: {json}");
    assert_eq!(tests[0]["file"], "src/lib.rs", "{label}");
    assert_eq!(finding["canonical_gap"]["owner"], "build", "{label}");
    assert_eq!(
        finding["classification"] == "exposed",
        exposed,
        "{label}: {json}"
    );
    let missing = finding["missing_discriminators"]
        .as_array()
        .ok_or("missing discriminator facts")?;
    let misses_field = missing.iter().any(|fact| {
        fact["flow_sink"]["kind"] == "struct_field"
            && fact["flow_sink"]["text"]
                .as_str()
                .is_some_and(|text| text.starts_with("retries:"))
    });
    assert_eq!(
        misses_field, !exposed,
        "{label}: exact changed field credit: {json}"
    );
    if !exposed && !matches!(label, "uncalled" | "shadowed_macro") {
        assert_eq!(
            finding["classification"], "weakly_exposed",
            "{label}: {json}"
        );
    }
    if exposed {
        assert_eq!(finding["classification"], "exposed", "{label}");
        assert_eq!(finding["oracle_strength"], "strong", "{label}");
    }
    Ok(())
}

#[test]
fn independent_expected_binding_observes_the_changed_field() -> Result<(), String> {
    for (label, body) in [
        (
            "retained",
            "let c = build(3);\nlet e = Config { retries: 4, name: \"x\".into() };\nassert_eq!(c, e);",
        ),
        (
            "inline",
            "let c = build(3);\nassert_eq!(c, Config { retries: 4, name: \"x\".into() });",
        ),
        (
            "reversed",
            "let c = build(3);\nlet e = Config { retries: 4, name: \"x\".into() };\nassert_eq!(e, c);",
        ),
        (
            "direct_owner",
            "let e = Config { retries: 4, name: \"x\".into() };\nassert_eq!(build(3), e);",
        ),
        (
            "typed",
            "let c = build(3);\nlet e: Config = Config { retries: 4, name: \"x\".into() };\nassert_eq!(c, e);",
        ),
    ] {
        let source = source(body)?;
        runtime(&source, false, label)?;
        for value in ["n + 2", "n"] {
            let mutant = source.replacen("retries: 1 + n,", &format!("retries: {value},"), 1);
            assert_ne!(mutant, source, "{label}: targeted mutation");
            runtime(&mutant, true, label)?;
        }
        classification(&source, true, label)?;
    }
    Ok(())
}

#[test]
fn dependent_or_replaced_expected_bindings_keep_conservative_credit() -> Result<(), String> {
    for (label, body) in [
        (
            "dependent",
            "let c = build(3);\nlet e = Config { retries: c.retries, name: \"x\".into() };\nassert_eq!(c, e);",
        ),
        (
            "indirect",
            "let c = build(3);\nlet r = c.retries;\nlet e = Config { retries: r, name: \"x\".into() };\nassert_eq!(c, e);",
        ),
        ("self", "let c = build(3);\nassert_eq!(c, c.clone());"),
        (
            "owner_overwritten",
            "let mut c = build(3);\nc.retries = 4;\nlet e = Config { retries: 4, name: \"x\".into() };\nassert_eq!(c, e);",
        ),
        (
            "expected_overwritten",
            "let c = build(3);\nlet mut e = Config { retries: 4, name: \"x\".into() };\ne.retries = c.retries;\nassert_eq!(c, e);",
        ),
        (
            "expected_shadowed",
            "let c = build(3);\nlet e = Config { retries: 4, name: \"x\".into() };\nlet e = c.clone();\nassert_eq!(c, e);",
        ),
        (
            "raw_shadow",
            "let c = build(3);\nlet e = Config { retries: 4, name: \"x\".into() };\nlet r#e = c.clone();\nassert_eq!(c, e);",
        ),
        (
            "uncalled",
            "let c = build(3);\nlet _check = || { let e = Config { retries: 4, name: \"x\".into() }; assert_eq!(c, e); };",
        ),
        (
            "shadowed_macro",
            "macro_rules! assert_eq { ($($t:tt)*) => {} }\nlet c = build(3);\nlet e = Config { retries: 4, name: \"x\".into() };\nassert_eq!(c, e);",
        ),
        (
            "functional_update",
            "let c = build(3);\nlet e = Config { name: \"x\".into(), ..c.clone() };\nassert_eq!(c, e);",
        ),
        (
            "helper_from_owner",
            "fn expected(c: &Config) -> Config { c.clone() }\nlet c = build(3);\nlet e = expected(&c);\nassert_eq!(c, e);",
        ),
        ("sibling", "let c = build(3);\nassert_eq!(c.name, \"x\");"),
        (
            "inequality",
            "let c = build(3);\nlet e = Config { retries: 9, name: \"x\".into() };\nassert_ne!(c, e);",
        ),
    ] {
        let source = source(body)?;
        runtime(&source, false, label)?;
        for value in ["n + 2", "n"] {
            let mutant = source.replacen("retries: 1 + n,", &format!("retries: {value},"), 1);
            assert_ne!(mutant, source);
            runtime(&mutant, false, label)?;
        }
        classification(&source, false, label)?;
    }
    let manual = source(
        "let c = build(3);\nlet e = Config { retries: 4, name: \"x\".into() };\nassert_eq!(c, e);",
    )?
    .replace(
        "#[derive(Debug, Clone, PartialEq)]",
        "#[derive(Debug, Clone)]",
    ) + "impl PartialEq for Config { fn eq(&self, other: &Self) -> bool { self.name == other.name } }\n";
    runtime(&manual, false, "manual_eq")?;
    for value in ["n + 2", "n"] {
        runtime(
            &manual.replacen("retries: 1 + n,", &format!("retries: {value},"), 1),
            false,
            "manual_eq",
        )?;
    }
    classification(&manual, false, "manual_eq")?;
    {
        let body = "#[cfg(any())]\nlet e = Config { retries: 4, name: String::new() };\nassert_eq!(build(3), e);";
        // Both runtime operands follow the mutant through the outer constant;
        // a cfg-elided or out-of-scope literal cannot supply independence.
        let source = source(body)?
            .replace("pub fn build", "pub const fn build")
            .replace("\"x\".into()", "String::new()")
            + "const e: Config = build(3);\n";
        runtime(&source, false, "outer_computed_expected")?;
        for value in ["n + 2", "n"] {
            runtime(
                &source.replacen("retries: 1 + n,", &format!("retries: {value},"), 1),
                false,
                "outer_computed_expected",
            )?;
        }
        classification(&source, false, "outer_computed_expected")?;
    }
    let sibling = source(
        "let e = build(3);\n{ let e = Config { retries: 4, name: \"x\".into() }; }\nassert_eq!(build(3), e);",
    )?;
    runtime(&sibling, false, "sibling_block")?;
    for value in ["n + 2", "n"] {
        runtime(
            &sibling.replacen("retries: 1 + n,", &format!("retries: {value},"), 1),
            false,
            "sibling_block",
        )?;
    }
    classification(&sibling, false, "sibling_block")?;
    let helper = source(
        "let e = Config { retries: expected_retries(), name: \"x\".into() };\nassert_eq!(build(3), e);",
    )? + "fn expected_retries() -> u32 { build(3).retries }\n";
    runtime(&helper, false, "helper_field_from_owner")?;
    for value in ["n + 2", "n"] {
        runtime(
            &helper.replacen("retries: 1 + n,", &format!("retries: {value},"), 1),
            false,
            "helper_field_from_owner",
        )?;
    }
    classification(&helper, false, "helper_field_from_owner")?;
    Ok(())
}
