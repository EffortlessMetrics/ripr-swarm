//! Small controls for the production Cargo join and compiler comparison rule.
//! `--no-default-features` avoids building the automation binary for this target.
#[path = "../src/reports/dx_scoreboard/compiler.rs"]
mod compiler;

use serde_json::{Value, json};
use std::path::PathBuf;

const BEFORE: &str =
    "rustc-v1|1.95.0|59807616e1fa2540724bfbac14d7976d7e4a3860|x86_64-unknown-linux-gnu|22.1.2";
const AFTER: &str =
    "rustc-v1|1.100.0|0ef1bc386059596dada8fb65b50425aab2ea637a|x86_64-unknown-linux-gnu|23.1.5";
const PACKAGE: &str = "path+file:///fixture#ripr@0.11.0";

fn manifest() -> PathBuf {
    std::env::temp_dir().join("dx-compiler-fixture/crates/ripr/Cargo.toml")
}

fn events(fresh: bool) -> Vec<Value> {
    vec![
        json!({"reason": "build-script-executed", "package_id": PACKAGE,
            "env": [["RIPR_BUILD_RUSTC", BEFORE]]}),
        json!({"reason": "compiler-artifact", "package_id": PACKAGE,
            "manifest_path": manifest(), "target": {"name": "ripr", "kind": ["bin"]},
            "profile": {"test": false}, "fresh": fresh,
            "executable": std::env::temp_dir().join("dx-compiler-fixture/target/release/ripr")}),
        json!({"reason": "build-finished", "success": true}),
    ]
}

fn encoded(events: &[Value]) -> String {
    events
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn cargo_identity_joins_the_exact_binary_for_cold_and_fresh_builds() -> Result<(), String> {
    for fresh in [false, true] {
        let mut rows = events(fresh);
        rows.insert(0, json!({"reason": "build-script-executed", "package_id": "foreign", "env": [["RIPR_BUILD_RUSTC", AFTER]]}));
        let built = compiler::owned_build(&encoded(&rows), &manifest())?;
        assert_eq!(built.rustc.as_deref(), Some(BEFORE));
        assert_eq!(
            built.path,
            PathBuf::from(rows[2]["executable"].as_str().ok_or("path missing")?)
        );
    }
    Ok(())
}

#[test]
fn cargo_manifest_spelling_joins_only_the_same_canonical_file()
-> Result<(), Box<dyn std::error::Error>> {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "dx-compiler-manifest-{}-{unique}",
        std::process::id()
    ));
    std::fs::create_dir(&root)?;
    struct Cleanup(PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(root.clone());
    std::fs::create_dir(root.join("alias"))?;
    std::fs::write(root.join("Cargo.toml"), "fixture manifest")?;
    std::fs::write(root.join("Other.toml"), "fixture manifest")?;
    let expected = root.join("Cargo.toml").canonicalize()?;
    let mut rows = events(false);
    rows[1]["manifest_path"] = json!(root.join("alias/../Cargo.toml"));
    assert_eq!(
        compiler::owned_build(&encoded(&rows), &expected)?
            .rustc
            .as_deref(),
        Some(BEFORE)
    );
    rows[1]["manifest_path"] = json!(root.join("Other.toml"));
    assert!(compiler::owned_build(&encoded(&rows), &expected).is_err());
    Ok(())
}

#[test]
fn missing_malformed_or_ambiguous_compiler_records_stay_unrecorded() -> Result<(), String> {
    let mut missing = events(false);
    missing.remove(0);
    let mut duplicated = events(false);
    duplicated.insert(0, duplicated[0].clone());
    let mut cases = vec![missing, duplicated];
    for env in [
        Value::Null,
        json!([]),
        json!([["RIPR_BUILD_RUSTC", ""]]),
        json!([["RIPR_BUILD_RUSTC", "unavailable"]]),
        json!([["RIPR_BUILD_RUSTC", null]]),
        json!([["RIPR_BUILD_RUSTC", BEFORE, "extra"]]),
        json!([["RIPR_BUILD_RUSTC", BEFORE], ["RIPR_BUILD_RUSTC", BEFORE]]),
    ] {
        let mut rows = events(false);
        rows[0]["env"] = env;
        cases.push(rows);
    }
    let mut foreign = events(false);
    foreign[0]["package_id"] = json!("foreign");
    cases.push(foreign);
    for rows in cases {
        assert!(
            compiler::owned_build(&encoded(&rows), &manifest())?
                .rustc
                .is_none(),
            "{rows:?}"
        );
    }
    Ok(())
}

#[test]
fn cargo_artifact_and_completion_evidence_cannot_be_missing_or_ambiguous() {
    let mut cases = Vec::new();
    for (field, value) in [
        ("package_id", json!("")),
        ("manifest_path", json!("some-other-manifest")),
        ("executable", json!("relative/ripr")),
        ("target", json!({"name": "ripr", "kind": ["lib"]})),
        ("profile", json!({"test": true})),
    ] {
        let mut rows = events(false);
        rows[1][field] = value;
        cases.push(rows);
    }
    for removed in [1, 2] {
        let mut rows = events(false);
        rows.remove(removed);
        cases.push(rows);
    }
    for duplicated in [1, 2] {
        let mut rows = events(false);
        rows.insert(duplicated, rows[duplicated].clone());
        cases.push(rows);
    }
    let mut failed = events(false);
    failed[2]["success"] = json!(false);
    cases.push(failed);
    for rows in cases {
        assert!(
            compiler::owned_build(&encoded(&rows), &manifest()).is_err(),
            "{rows:?}"
        );
    }
    assert!(compiler::owned_build("{broken", &manifest()).is_err());
    assert!(compiler::owned_build(&"{}\n".repeat(4097), &manifest()).is_err());
}

#[test]
fn compiler_comparison_requires_two_complete_equal_identities() {
    assert!(compiler::comparison_issue(Some(BEFORE), Some(BEFORE)).is_none());
    assert!(compiler::comparison_issue(Some(AFTER), Some(BEFORE)).is_some());
    for identity in [None, Some(""), Some("unavailable"), Some("rustc 1.95.0")] {
        assert!(compiler::comparison_issue(identity, identity).is_some());
        assert!(compiler::comparison_issue(identity, Some(BEFORE)).is_some());
        assert!(compiler::comparison_issue(Some(BEFORE), identity).is_some());
    }
    for changed in [
        BEFORE.replace("1.95.0", "1.96.0"),
        BEFORE.replace("59807616", "00000000"),
        BEFORE.replace("x86_64", "aarch64"),
        BEFORE.replace("22.1.2", "22.1.3"),
    ] {
        assert!(compiler::comparison_issue(Some(&changed), Some(BEFORE)).is_some());
    }
    assert!(compiler::description(Some(AFTER)).contains("rustc 1.100.0"));
    assert!(compiler::description(Some(AFTER)).contains("LLVM 23.1.5"));
    assert_eq!(compiler::description(None), "rustc unrecorded");
}
