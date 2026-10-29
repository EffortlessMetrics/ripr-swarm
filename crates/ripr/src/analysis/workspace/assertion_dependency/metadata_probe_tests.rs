//! Controls for the transport boundary, not property/provider admission.

use super::*;
use std::ffi::OsStr;
use std::time::Duration;

struct CaptureFixture {
    root: PathBuf,
}

impl CaptureFixture {
    fn new() -> Result<Self, String> {
        let root = std::env::temp_dir().join(format!(
            "ripr-metadata-probe-control-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        // Never take ownership of a pre-existing directory.
        std::fs::create_dir(&root).map_err(|error| error.to_string())?;
        Ok(Self { root })
    }

    fn output(&self) -> PathBuf {
        self.root.join("metadata.json")
    }
}

impl Drop for CaptureFixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn metadata_command_remains_offline_locked_and_non_installing() -> Result<(), String> {
    let fixture = CaptureFixture::new()?;
    let output = std::fs::File::create(fixture.output()).map_err(|error| error.to_string())?;
    let root = fixture.root.join("workspace with spaces");
    let command = metadata_command(&root, output);
    assert_eq!(command.get_program(), OsStr::new("cargo"));
    assert_eq!(
        command.get_args().collect::<Vec<_>>(),
        [
            "metadata",
            "--offline",
            "--locked",
            "--format-version",
            "1",
            "--manifest-path",
            "Cargo.toml",
        ]
        .map(OsStr::new)
        .to_vec()
    );
    assert_eq!(command.get_current_dir(), Some(root.as_path()));
    assert!(command.get_envs().any(|(name, value)| {
        name == OsStr::new("RUSTUP_AUTO_INSTALL") && value == Some(OsStr::new("0"))
    }));
    // Inspect the actual command constructor without spawning Cargo.
    Ok(())
}

#[test]
fn metadata_budget_preserves_exact_output_and_deadline_boundaries() {
    assert_eq!(BUDGET, 8 * 1024 * 1024);
    assert_eq!(metadata_probe_budget(0, Duration::ZERO), Ok(()));
    assert_eq!(
        metadata_probe_budget(8 * 1024 * 1024, Duration::from_secs(119)),
        Ok(())
    );
    assert_eq!(
        metadata_probe_budget(8 * 1024 * 1024 + 1, Duration::ZERO),
        Err(MetadataProbeFailure::OutputLimit)
    );
    assert_eq!(
        metadata_probe_budget(0, Duration::from_secs(120)),
        Err(MetadataProbeFailure::TimedOut)
    );
    // Both exhausted bounds remain a refusal, with deterministic precedence.
    assert_eq!(
        metadata_probe_budget(8 * 1024 * 1024 + 1, Duration::from_secs(120)),
        Err(MetadataProbeFailure::OutputLimit)
    );
}

#[test]
fn metadata_poll_distinguishes_running_and_wait_failure() {
    assert_eq!(metadata_probe_poll(Ok(None)), Ok(false));
    assert_eq!(
        metadata_probe_poll(Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "private environment-specific diagnostic must not escape",
        ))),
        Err(MetadataProbeFailure::Wait(
            std::io::ErrorKind::PermissionDenied
        ))
    );
}

#[cfg(unix)]
#[test]
fn metadata_poll_distinguishes_success_exit_code_and_signal() {
    use std::os::unix::process::ExitStatusExt;
    assert_eq!(
        metadata_probe_poll(Ok(Some(std::process::ExitStatus::from_raw(0)))),
        Ok(true)
    );
    assert_eq!(
        metadata_probe_poll(Ok(Some(std::process::ExitStatus::from_raw(101 << 8)))),
        Err(MetadataProbeFailure::UnsuccessfulExit(Some(101)))
    );
    assert_eq!(
        metadata_probe_poll(Ok(Some(std::process::ExitStatus::from_raw(9)))),
        Err(MetadataProbeFailure::UnsuccessfulExit(None))
    );
}

#[cfg(windows)]
#[test]
fn metadata_poll_distinguishes_success_and_windows_exit_code() {
    use std::os::windows::process::ExitStatusExt;
    assert_eq!(
        metadata_probe_poll(Ok(Some(std::process::ExitStatus::from_raw(0)))),
        Ok(true)
    );
    assert_eq!(
        metadata_probe_poll(Ok(Some(std::process::ExitStatus::from_raw(101)))),
        Err(MetadataProbeFailure::UnsuccessfulExit(Some(101)))
    );
}

#[test]
fn metadata_capture_preserves_zero_target_data_and_unparsed_bytes() -> Result<(), String> {
    let fixture = CaptureFixture::new()?;
    // Successful transport does not establish JSON validity, a target or a
    // registry receipt. Those remain the existing downstream owners' checks.
    for bytes in [
        br#"{"packages":[],"workspace_members":[],"resolve":null}"#.as_slice(),
        b"not metadata".as_slice(),
        b"".as_slice(),
    ] {
        std::fs::write(fixture.output(), bytes).map_err(|error| error.to_string())?;
        assert_eq!(
            read_metadata_probe_output(&fixture.output()),
            Ok(bytes.to_vec())
        );
    }
    Ok(())
}

#[test]
fn metadata_capture_refuses_oversize_instead_of_accepting_truncated_bytes() -> Result<(), String> {
    let fixture = CaptureFixture::new()?;
    let output = std::fs::File::create(fixture.output()).map_err(|error| error.to_string())?;
    output.set_len(BUDGET).map_err(|error| error.to_string())?;
    assert_eq!(
        read_metadata_probe_output(&fixture.output())
            .map_err(|failure| format!("{failure:?}"))?
            .len() as u64,
        BUDGET
    );
    output.set_len(BUDGET + 1).map_err(|error| error.to_string())?;
    assert_eq!(
        read_metadata_probe_output(&fixture.output()),
        Err(MetadataProbeFailure::OutputLimit)
    );
    Ok(())
}

#[test]
fn metadata_capture_read_failure_is_not_an_empty_success() -> Result<(), String> {
    let fixture = CaptureFixture::new()?;
    assert_eq!(
        read_metadata_probe_output(&fixture.output()),
        Err(MetadataProbeFailure::CaptureRead(
            std::io::ErrorKind::NotFound
        ))
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn metadata_launch_failure_retains_kind_and_legacy_refusal() -> Result<(), String> {
    let fixture = CaptureFixture::new()?;
    let missing = fixture.root.join("missing-workspace");
    assert!(!missing.exists());
    // No Cargo code can run with this nonexistent working directory. This
    // exercises the actual launch/error path without changing shared PATH.
    assert_eq!(
        metadata_probe_result(&missing),
        Err(MetadataProbeFailure::Launch(std::io::ErrorKind::NotFound))
    );
    assert!(metadata_probe(&missing).is_none());
    Ok(())
}

#[test]
fn metadata_failure_messages_do_not_invent_package_or_toolchain_causes() {
    let failures = [
        MetadataProbeFailure::ToolchainPathPin,
        MetadataProbeFailure::CaptureCreate(std::io::ErrorKind::PermissionDenied),
        MetadataProbeFailure::Launch(std::io::ErrorKind::NotFound),
        MetadataProbeFailure::CaptureInspect(std::io::ErrorKind::NotFound),
        MetadataProbeFailure::OutputLimit,
        MetadataProbeFailure::TimedOut,
        MetadataProbeFailure::Wait(std::io::ErrorKind::PermissionDenied),
        MetadataProbeFailure::UnsuccessfulExit(Some(101)),
        MetadataProbeFailure::UnsuccessfulExit(None),
        MetadataProbeFailure::CaptureRead(std::io::ErrorKind::NotFound),
    ];
    let messages = failures.map(|failure| failure.to_string());
    assert_eq!(messages.iter().collect::<std::collections::BTreeSet<_>>().len(), 10);
    assert_eq!(
        messages[7],
        "Offline Cargo metadata exited unsuccessfully (status 101)"
    );
    for message in messages {
        assert!(message.starts_with("Offline Cargo metadata"));
        assert!(!message.contains("zerocopy"));
        assert!(!message.contains("missing toolchain"));
        assert!(!message.contains("private environment-specific diagnostic"));
    }
}

#[test]
fn metadata_real_cargo_probe_preserves_legacy_refusal_and_recovers() -> Result<(), String> {
    let fixture = CaptureFixture::new()?;
    let root = fixture.root.join("workspace with spaces");
    std::fs::create_dir_all(root.join("src")).map_err(|error| error.to_string())?;
    let root = root.canonicalize().map_err(|error| error.to_string())?;
    let manifest = "\
[package]
name = 'ripr-metadata-probe-control'
version = '0.0.0'
edition = '2024'
[lib]
path = 'src/lib.rs'
[workspace]
";
    let lock = "\
version = 4
[[package]]
name = 'ripr-metadata-probe-control'
version = '0.0.0'
";
    let source = "pub fn observed() -> bool { true }\n";
    let inputs = [
        ("Cargo.toml", manifest),
        ("Cargo.lock", lock),
        ("src/lib.rs", source),
    ];
    for (path, contents) in inputs {
        std::fs::write(root.join(path), contents).map_err(|error| error.to_string())?;
    }

    // Exercise the real fixed-argv probe: no mock executable, injected command,
    // shared environment mutation, dependency preparation or project build.
    // A missing/broken Cargo installation fails this positive; it is not a skip.
    let before = metadata_probe_result(&root)
        .map_err(|failure| format!("real Cargo metadata positive: {failure}"))?;
    let metadata: serde_json::Value =
        serde_json::from_slice(&before).map_err(|error| error.to_string())?;
    let workspace = metadata["workspace_root"]
        .as_str()
        .ok_or("missing workspace root in real metadata")?;
    assert_eq!(
        Path::new(workspace)
            .canonicalize()
            .map_err(|error| error.to_string())?,
        root
    );
    let packages = metadata["packages"]
        .as_array()
        .ok_or("missing packages in real metadata")?;
    assert_eq!(packages.len(), 1, "the positive must contain its exact package");
    let package = &packages[0];
    assert_eq!(package["name"], "ripr-metadata-probe-control");
    assert_eq!(package["version"], "0.0.0");
    assert_eq!(
        metadata["workspace_members"],
        serde_json::json!([package["id"]])
    );
    let targets = package["targets"]
        .as_array()
        .ok_or("missing targets in real metadata")?;
    assert_eq!(targets.len(), 1, "zero targets cannot satisfy the positive");
    assert_eq!(targets[0]["kind"], serde_json::json!(["lib"]));
    let target = targets[0]["src_path"]
        .as_str()
        .ok_or("missing source path in real metadata")?;
    assert_eq!(
        Path::new(target)
            .canonicalize()
            .map_err(|error| error.to_string())?,
        root.join("src/lib.rs")
    );
    assert_eq!(metadata_probe(&root), Some(before.clone()));

    // A native Cargo refusal is neither a launch failure nor an empty success.
    // The malformed manifest is the stimulus, not an inferred missing package.
    let invalid_manifest = "[package\n";
    std::fs::write(root.join("Cargo.toml"), invalid_manifest)
        .map_err(|error| error.to_string())?;
    let refused = metadata_probe_result(&root);
    assert!(
        matches!(
            refused,
            Err(MetadataProbeFailure::UnsuccessfulExit(Some(code))) if code != 0
        ),
        "expected native Cargo refusal, got {refused:?}"
    );
    assert!(metadata_probe(&root).is_none());
    assert_eq!(
        std::fs::read_to_string(root.join("Cargo.toml"))
            .map_err(|error| error.to_string())?,
        invalid_manifest
    );

    // Restore only the owned stimulus. A prior failure must not poison either
    // interface, and --locked metadata must leave all three fixture inputs alone.
    std::fs::write(root.join("Cargo.toml"), manifest).map_err(|error| error.to_string())?;
    let restored = metadata_probe_result(&root)
        .map_err(|failure| format!("restored real Cargo metadata: {failure}"))?;
    assert_eq!(restored, before);
    assert_eq!(metadata_probe(&root), Some(before));
    for (path, contents) in inputs {
        assert_eq!(
            std::fs::read_to_string(root.join(path)).map_err(|error| error.to_string())?,
            contents,
            "metadata changed {path}"
        );
    }
    Ok(())
}
