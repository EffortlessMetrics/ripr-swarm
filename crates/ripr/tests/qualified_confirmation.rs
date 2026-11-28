use ripr::{CheckInput, Mode, OutputFormat, check_workspace, render_check};
use std::path::Path;

struct Scratch(std::path::PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn copy_subject(source: &Path, target: &Path) -> Result<(), String> {
    std::fs::create_dir_all(target).map_err(|e| e.to_string())?;
    for entry in std::fs::read_dir(source).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        if entry.file_type().map_err(|e| e.to_string())?.is_dir() {
            copy_subject(&entry.path(), &target.join(entry.file_name()))?;
        } else {
            let name = entry
                .file_name()
                .to_string_lossy()
                .trim_end_matches(".txt")
                .to_string();
            std::fs::copy(entry.path(), target.join(name)).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn prepare_case(
    subject: &str,
    case: &str,
    edits: &[(&str, &str, &str)],
) -> Result<(Scratch, std::path::PathBuf), String> {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let corpus = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/rust-verdict-corpus");
    let root = Scratch(std::env::temp_dir().join(format!(
        "ripr-qualified-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    )));
    std::fs::create_dir_all(std::env::temp_dir()).map_err(|e| e.to_string())?;
    copy_subject(&corpus.join("subjects").join(subject), &root.0)?;
    for (file, before, after) in edits {
        let path = root.0.join(file);
        let source = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        assert_eq!(source.matches(before).count(), 1, "{file}: {before}");
        std::fs::write(path, source.replace(before, after)).map_err(|e| e.to_string())?;
    }
    Ok((root, corpus.join("cases").join(format!("{case}.diff"))))
}

fn run_case(
    subject: &str,
    case: &str,
    edits: &[(&str, &str, &str)],
) -> Result<serde_json::Value, String> {
    let (root, diff) = prepare_case(subject, case, edits)?;
    check(&root.0, &diff)
}

fn check(root: &Path, diff: &Path) -> Result<serde_json::Value, String> {
    let report = check_workspace(CheckInput {
        root: root.to_path_buf(),
        diff_file: Some(diff.to_path_buf()),
        mode: Mode::Fast,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })?;
    serde_json::from_str(&render_check(&report, &OutputFormat::Json)?).map_err(|e| e.to_string())
}

fn assert_return(report: &serde_json::Value, exposed: bool, label: &str) -> Result<(), String> {
    let findings: Vec<_> = report["findings"]
        .as_array()
        .ok_or("missing findings")?
        .iter()
        .filter(|finding| finding["probe"]["family"] == "return_value")
        .collect();
    assert_eq!(findings.len(), 1, "{label}: {report}");
    assert_eq!(
        findings[0]["classification"] == "exposed",
        exposed,
        "{label}: {}",
        findings[0]
    );
    Ok(())
}

#[test]
fn qualified_pretty_equality_keeps_registry_root_and_execution_refusals() -> Result<(), String> {
    for (label, macro_path, prefix, manifest, exposed) in [
        ("pretty", "pretty_assertions::assert_eq!", "", None, true),
        (
            "absolute-pretty",
            "::pretty_assertions::assert_eq!",
            "",
            None,
            true,
        ),
        ("std", "std::assert_eq!", "", None, true),
        ("core", "::core::assert_eq!", "", None, true),
        (
            "aliased-package",
            "pretty_assertions::assert_eq!",
            "",
            Some("pretty_assertions = { package = \"fake-assertions\", version = \"1\" }"),
            false,
        ),
        (
            "rebound-pretty",
            "pretty_assertions::assert_eq!",
            "mod pretty_assertions { macro_rules! fake_eq { ($($t:tt)*) => {}; } pub(crate) use fake_eq as assert_eq; }\n",
            None,
            false,
        ),
        (
            "rebound-std",
            "std::assert_eq!",
            "mod std {}\n",
            None,
            false,
        ),
        (
            "early-return",
            "pretty_assertions::assert_eq!",
            "",
            None,
            false,
        ),
        (
            "cfg-disabled",
            "pretty_assertions::assert_eq!",
            "",
            None,
            false,
        ),
        ("unrelated-path", "other::assert_eq!", "", None, false),
    ] {
        let original = "use lib_pretty_assertions::{line, total};";
        let prelude = format!("{prefix}{original}");
        let replacement = match label {
            "early-return" => format!("return; {macro_path}"),
            "cfg-disabled" => format!("#[cfg(any())] {macro_path}"),
            _ => macro_path.to_string(),
        };
        let mut edits = vec![
            ("src/lib.rs", "sum::<u64>() + 50", "sum::<u64>() + 25 * 2"),
            ("tests/totals.rs", original, prelude.as_str()),
            (
                "tests/totals.rs",
                "pretty_assertions::assert_eq!",
                replacement.as_str(),
            ),
        ];
        if let Some(manifest) = manifest {
            edits.push(("Cargo.toml", "pretty_assertions = \"1\"", manifest));
        }
        let report = run_case(
            "authored-lib-pretty-assertions",
            "lib-pretty-total-path-qualified",
            &edits,
        )?;
        assert_return(&report, exposed, label)?;
    }
    Ok(())
}

#[test]
fn associated_owner_glob_keeps_type_root_and_competing_import_refusals() -> Result<(), String> {
    for (label, import, library_extra, exposed) in [
        ("owner-glob", "use meter::*;", "", true),
        ("explicit-type", "use meter::Lamp;", "", true),
        ("wrong-glob", "use spare::*;", "", false),
        ("competing-glob", "use meter::*;\nuse spare::*;", "", false),
        (
            "rebound-root",
            "use spare as meter;\nuse meter::*;",
            "",
            false,
        ),
        ("rebound-type", "use meter::*;\nstruct Lamp;", "", false),
        ("disabled-glob", "#[cfg(any())]\nuse meter::*;", "", false),
        (
            "opaque-owner-root",
            "use meter::*;",
            "macro_rules! root { ($name:ident) => { pub mod $name {} }; } root!(meter);\n",
            false,
        ),
        (
            "exported-root",
            "use meter::*;",
            "pub mod meter {}\n",
            false,
        ),
    ] {
        let source = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(
            "../../fixtures/rust-verdict-corpus/subjects/authored-old-deps/meter/src/lib.rs.txt",
        ))
        .map_err(|e| e.to_string())?;
        let library = format!(
            "{}{library_extra}",
            source.replace("        800", "        8 * 100")
        );
        let report = run_case(
            "authored-old-deps",
            "old-deps-type-path-glob-import",
            &[
                ("meter/src/lib.rs", source.as_str(), library.as_str()),
                ("panel/tests/globbed.rs", "use meter::*;", import),
            ],
        )?;
        assert_return(&report, exposed, label)?;
    }
    Ok(())
}

#[test]
fn associated_glob_memo_refreshes_after_manifest_glob_and_root_mutations() -> Result<(), String> {
    let (root, diff) = prepare_case(
        "authored-old-deps",
        "old-deps-type-path-glob-import",
        &[("meter/src/lib.rs", "        800", "        8 * 100")],
    )?;
    let verify = |expected, label| -> Result<(), String> {
        assert_return(&check(&root.0, &diff)?, expected, label)
    };
    verify(true, "cold")?;
    verify(true, "repeated")?;
    let manifest = root.0.join("panel/Cargo.toml");
    let original_manifest = std::fs::read_to_string(&manifest).map_err(|e| e.to_string())?;
    let mutated_manifest = original_manifest.replace(
        "meter = { path = \"../meter\" }",
        "meter = { package = \"spare\", path = \"../spare\" }",
    );
    assert_ne!(
        original_manifest, mutated_manifest,
        "manifest mutation must change dependency identity"
    );
    std::fs::write(&manifest, mutated_manifest).map_err(|e| e.to_string())?;
    verify(false, "changed manifest")?;
    std::fs::write(&manifest, original_manifest).map_err(|e| e.to_string())?;
    verify(true, "manifest restored")?;
    let test = root.0.join("panel/tests/globbed.rs");
    let source = std::fs::read_to_string(&test).map_err(|e| e.to_string())?;
    assert!(source.contains("use meter::*;"));
    std::fs::write(&test, source.replace("use meter::*;", "use spare::*;"))
        .map_err(|e| e.to_string())?;
    verify(false, "wrong glob")?;
    std::fs::write(&test, source).map_err(|e| e.to_string())?;
    verify(true, "glob restored")?;
    let owner = root.0.join("meter/src/lib.rs");
    let source = std::fs::read_to_string(&owner).map_err(|e| e.to_string())?;
    std::fs::write(&owner, format!("{source}\npub mod meter {{}}\n")).map_err(|e| e.to_string())?;
    verify(false, "owner library exports root")?;
    std::fs::write(&owner, source).map_err(|e| e.to_string())?;
    verify(true, "root restored")?;
    Ok(())
}
