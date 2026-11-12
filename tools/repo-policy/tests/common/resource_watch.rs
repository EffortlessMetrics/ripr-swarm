//! Reused-target resource additions must refresh the dev-only identity producer.
use super::{Fixture, artifact, backdate, messages};
use std::fs;
use std::path::{Path, PathBuf};

const RESOURCE_FRONTDOOR: &str = r#"#[path = "../source_identity.rs"]
mod source_identity;
const IDENTITIES: (&str, &str) = repo_policy_test_identity::identities!();
#[test]
fn resource_custody() -> Result<(), String> {
    let manifest = std::env::var_os("CARGO_MANIFEST_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| env!("CARGO_MANIFEST_DIR").into());
    let root = manifest.join("../..");
    println!("resource_custody_selected=1");
    assert_eq!(source_identity::implementation_identity(&root)?, IDENTITIES.0);
    assert_eq!(env!("REPO_POLICY_SOURCE_ID"), IDENTITIES.0);
    assert_eq!(source_identity::source_identity(&root)?, IDENTITIES.1,
        "resource identity stale");
    Ok(())
}
"#;

fn write_resource_root(fixture: &Fixture, name: &str, mode: &str) -> Result<PathBuf, String> {
    let root = fixture.write_root(name)?;
    let workspace = concat!(
        "[workspace]\nmembers = [\"tools/repo-policy\", ",
        "\"tools/repo-policy-test-identity\"]\nresolver = \"2\"\n",
        "[workspace.package]\nedition = \"2024\"\nlicense = \"MIT\"\n",
        "rust-version = \"1.95\"\n[workspace.lints.rust]\nunused_must_use = \"deny\"\n"
    );
    fs::write(root.join("Cargo.toml"), workspace).map_err(|e| e.to_string())?;
    let manifest = root.join("tools/repo-policy/Cargo.toml");
    let mut package = fs::read_to_string(&manifest).map_err(|e| e.to_string())?;
    package.push_str(concat!(
        "\n[dev-dependencies]\nrepo-policy-test-identity = ",
        "{ path = \"../repo-policy-test-identity\" }\n"
    ));
    fs::write(manifest, package).map_err(|e| e.to_string())?;
    fs::write(
        root.join("tools/repo-policy/src/main.rs"),
        super::PROBE.replace(
            "source_identity::source_identity",
            "source_identity::implementation_identity",
        ),
    )
    .map_err(|e| e.to_string())?;
    fs::write(
        root.join("tools/repo-policy/tests/frontdoor.rs"),
        RESOURCE_FRONTDOOR,
    )
    .map_err(|e| e.to_string())?;
    fs::create_dir_all(root.join("tools/repo-policy-contracts")).map_err(|e| e.to_string())?;
    fs::write(root.join("tools/repo-policy-contracts/resource.txt"), name)
        .map_err(|e| e.to_string())?;

    let original = include_str!("../../../repo-policy-test-identity/build.rs");
    let build_script = match mode {
        "normal" => original.to_string(),
        "missing-sibling-watch" => {
            original.replace("        \"tools/repo-policy-contracts\",\n", "")
        }
        "compile-time-manifest" => original.replace(
            "std::env::var_os(\"CARGO_MANIFEST_DIR\")",
            "Some(std::ffi::OsString::from(env!(\"CARGO_MANIFEST_DIR\")))",
        ),
        _ => return Err(format!("unknown resource-watch control: {mode}")),
    };
    if mode != "normal" {
        assert_ne!(
            original, build_script,
            "resource-watch mutation was a no-op"
        );
    }
    for (path, text) in [
        (
            "Cargo.toml",
            include_str!("../../../repo-policy-test-identity/Cargo.toml"),
        ),
        ("build.rs", build_script.as_str()),
        (
            "src/lib.rs",
            include_str!("../../../repo-policy-test-identity/src/lib.rs"),
        ),
    ] {
        let path = root.join("tools/repo-policy-test-identity").join(path);
        fs::create_dir_all(path.parent().ok_or("macro fixture parent")?)
            .map_err(|e| e.to_string())?;
        fs::write(path, text).map_err(|e| e.to_string())?;
    }
    Ok(root)
}

fn verify_resource_run(output: &std::process::Output, expected: i32) -> Result<PathBuf, String> {
    let events = messages(output);
    assert!(
        events
            .iter()
            .any(|event| event["reason"] == "build-finished" && event["success"] == true),
        "resource probe did not compile: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains("resource_custody_selected=1"),
        "zero resource subjects: {stdout}"
    );
    assert_eq!(
        output.status.code(),
        Some(expected),
        "resource probe: {stdout}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    if expected != 0 {
        assert!(
            stdout.contains("resource identity stale"),
            "wrong refusal: {stdout}"
        );
    }
    Ok(PathBuf::from(
        artifact(&events, "test")?["executable"]
            .as_str()
            .ok_or("resource probe executable")?,
    ))
}

fn retained_resource_output(
    executable: &Path,
    a: &Path,
    b: &Path,
) -> Result<std::process::Output, String> {
    // Deliberately retain A as cwd while remapping runtime resources to B.
    std::process::Command::new(executable)
        .args(["--exact", "resource_custody", "--nocapture"])
        .current_dir(a)
        .env("CARGO_MANIFEST_DIR", b.join("tools/repo-policy"))
        .output()
        .map_err(|e| e.to_string())
}

#[test]
fn relocated_macro_recovers_new_resources_and_rejects_watch_mutants() -> Result<(), String> {
    for mode in ["normal", "missing-sibling-watch", "compile-time-manifest"] {
        let fixture = Fixture::new()?;
        let a = write_resource_root(&fixture, "a", mode)?;
        let b = write_resource_root(&fixture, "b", mode)?;
        let lock = fixture.cargo(&a, &["generate-lockfile", "--offline"])?;
        assert!(lock.status.success(), "resource lock setup: {lock:?}");
        fs::copy(a.join("Cargo.lock"), b.join("Cargo.lock")).map_err(|e| e.to_string())?;
        backdate(&a)?;
        backdate(&b)?;
        let args = [
            "test",
            "--locked",
            "--offline",
            "-p",
            "policy-worktree-probe",
            "--test",
            "frontdoor",
            "--message-format=json",
            "--",
            "--nocapture",
        ];
        verify_resource_run(&fixture.cargo(&a, &args)?, 0)?;
        let selected = verify_resource_run(&fixture.cargo(&b, &args)?, 0)?;
        let retained = fixture.0.join("retained-resource-harness");
        fs::copy(selected, &retained).map_err(|e| e.to_string())?;
        fs::write(
            b.join("tools/repo-policy-contracts/new-resource.profraw"),
            "new resource",
        )
        .map_err(|e| e.to_string())?;
        let refused = retained_resource_output(&retained, &a, &b)?;
        assert_eq!(refused.status.code(), Some(101));
        assert!(
            format!(
                "{}\n{}",
                String::from_utf8_lossy(&refused.stdout),
                String::from_utf8_lossy(&refused.stderr)
            )
            .contains("resource identity stale")
        );
        let recovered = fixture.cargo(&b, &args)?;
        verify_resource_run(&recovered, if mode == "normal" { 0 } else { 101 })?;
        assert_eq!(artifact(&messages(&recovered), "bin")?["fresh"], true);
    }
    Ok(())
}

#[test]
fn unknown_production_input_forces_cargo_to_refresh_the_producer() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let root = write_resource_root(&fixture, "unknown-production", "normal")?;
    let lock = fixture.cargo(&root, &["generate-lockfile", "--offline"])?;
    assert!(lock.status.success(), "resource lock setup: {lock:?}");
    let args = [
        "build",
        "--locked",
        "--offline",
        "-p",
        "policy-worktree-probe",
        "--message-format=json",
    ];
    let initial = fixture.cargo(&root, &args)?;
    assert!(initial.status.success(), "initial build: {initial:?}");
    let initial_events = messages(&initial);
    let selected = PathBuf::from(
        artifact(&initial_events, "bin")?["executable"]
            .as_str()
            .ok_or("initial binary")?,
    );
    let retained = fixture.0.join("retained-production-probe");
    fs::copy(selected, &retained).map_err(|e| e.to_string())?;
    assert!(
        std::process::Command::new(&retained)
            .current_dir(&root)
            .output()
            .map_err(|e| e.to_string())?
            .status
            .success()
    );
    fs::write(root.join("tools/repo-policy/new-unknown-input"), "new")
        .map_err(|e| e.to_string())?;
    let stale = std::process::Command::new(&retained)
        .current_dir(&root)
        .output()
        .map_err(|e| e.to_string())?;
    assert!(
        !stale.status.success(),
        "retained producer accepted unknown input"
    );
    assert!(String::from_utf8_lossy(&stale.stderr).contains("stale source identity"));
    let recovered = fixture.cargo(&root, &args)?;
    assert!(recovered.status.success(), "rebuild: {recovered:?}");
    let events = messages(&recovered);
    let binary = artifact(&events, "bin")?;
    assert_eq!(
        binary["fresh"], false,
        "unknown production input did not rebuild producer"
    );
    let executable = binary["executable"].as_str().ok_or("recovered binary")?;
    assert!(
        std::process::Command::new(executable)
            .current_dir(&root)
            .output()
            .map_err(|e| e.to_string())?
            .status
            .success()
    );
    Ok(())
}
