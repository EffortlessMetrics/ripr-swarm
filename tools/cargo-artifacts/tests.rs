use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct TempDirectory {
    path: PathBuf,
}

impl TempDirectory {
    fn new(name: &str) -> Self {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = env::temp_dir().join(format!(
            "ripr-cargo-artifacts-{name}-{}-{sequence}",
            process::id()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("create test directory");
        Self { path }
    }
}

impl Drop for TempDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn cargo_paths(repository: &Path, build_directory: PathBuf) -> CargoPaths {
    CargoPaths {
        workspace_root: repository.to_path_buf(),
        target_directory: repository.join("target"),
        build_directory,
    }
}

#[test]
fn repository_root_ignores_nested_cargo_config_without_git_marker() {
    let root = TempDirectory::new("repository-root");
    fs::write(root.path.join("Cargo.toml"), b"[workspace]\n").expect("root manifest");
    fs::create_dir_all(root.path.join(".cargo")).expect("root cargo config directory");
    fs::write(root.path.join(".cargo/config.toml"), b"[build]\n").expect("root config");
    fs::write(root.path.join(".git"), b"gitdir: elsewhere\n").expect("git marker");

    let nested = root.path.join("fixtures/nested");
    fs::create_dir_all(nested.join(".cargo")).expect("nested cargo config directory");
    fs::create_dir_all(nested.join("src")).expect("nested source directory");
    fs::write(nested.join("Cargo.toml"), b"[package]\nname='nested'\nversion='0.1.0'\n")
        .expect("nested manifest");
    fs::write(nested.join(".cargo/config.toml"), b"[build]\n").expect("nested config");

    assert_eq!(
        repository_root(&nested.join("src")).expect("checkout root"),
        root.path.canonicalize().expect("canonical root")
    );
}

#[test]
fn metadata_parser_unescapes_windows_paths() {
    let parsed = parse_cargo_paths(
        r#"{"workspace_root":"C:\\Code\\ripr-swarm","target_directory":"C:\\Code\\ripr-swarm\\target","build_directory":"C:\\Code\\ripr-swarm\\target"}"#,
    )
    .expect("parse paths");
    assert_eq!(
        parsed.workspace_root,
        PathBuf::from(r"C:\Code\ripr-swarm")
    );
    assert_eq!(
        parsed.target_directory,
        PathBuf::from(r"C:\Code\ripr-swarm\target")
    );
}

#[test]
fn metadata_parser_ignores_nested_lookalike_fields() {
    let parsed = parse_cargo_paths(
        r#"{"packages":[{"metadata":{"workspace_root":"/wrong","target_directory":"/also-wrong","build_directory":"/still-wrong"}}],"target_directory":"/repo/target","build_directory":"/repo/target","workspace_root":"/repo"}"#,
    )
    .expect("parse top-level paths");
    assert_eq!(parsed.workspace_root, PathBuf::from("/repo"));
    assert_eq!(parsed.target_directory, PathBuf::from("/repo/target"));
    assert_eq!(parsed.build_directory, PathBuf::from("/repo/target"));
}

#[test]
fn validation_requires_exact_target_and_build_directory() {
    let root = TempDirectory::new("exact");
    let repository = root.path.canonicalize().expect("canonical repo");
    let target = repository.join("target");
    let exact = cargo_paths(&repository, target.clone());
    assert_eq!(
        validate_cargo_paths(&repository, &exact).expect("exact paths"),
        target
    );

    let nested = cargo_paths(&repository, repository.join("target/build/hash"));
    let error = validate_cargo_paths(&repository, &nested).expect_err("nested build refused");
    assert!(error.to_string().contains("exact build_directory"));

    let external = CargoPaths {
        workspace_root: repository.clone(),
        target_directory: repository.join("../shared-target"),
        build_directory: repository.join("../shared-target"),
    };
    let error = validate_cargo_paths(&repository, &external).expect_err("external target refused");
    assert!(error.to_string().contains("owns only"));
}

#[cfg(windows)]
#[test]
fn windows_path_comparison_accepts_verbatim_prefix_and_case() {
    assert!(same_path(
        Path::new(r"\\?\C:\Code\ripr-swarm\target"),
        Path::new(r"c:\code\RIPR-SWARM\target")
    ));
    assert!(same_path(
        Path::new(r"\\?\UNC\server\share\ripr\target"),
        Path::new(r"\\server\share\RIPR\target")
    ));
}

#[test]
fn plan_selects_direct_target_entries_and_preserves_ripr() {
    let root = TempDirectory::new("plan");
    let target = root.path.join("target");
    fs::create_dir_all(target.join("debug/deps")).expect("debug tree");
    fs::write(target.join("debug/deps/a.rlib"), b"1234").expect("artifact");
    fs::write(target.join("scratch.tmp"), b"12").expect("scratch");
    fs::create_dir_all(target.join("RIPR/reports")).expect("ripr tree");
    fs::write(target.join("RIPR/reports/keep.json"), b"retained").expect("receipt");

    let plan = build_plan(&target, 5).expect("plan");
    assert_eq!(
        plan.selected
            .iter()
            .map(|entry| entry.name.as_str())
            .collect::<Vec<_>>(),
        vec!["debug", "scratch.tmp"]
    );
    assert_eq!(plan.selected_measurement.bytes, 6);
    assert_eq!(plan.retained_measurement.bytes, 8);
    assert_eq!(plan.over_budget_bytes, 1);
}

#[cfg(not(windows))]
#[test]
fn plan_retains_and_counts_multiple_evidence_case_variants() {
    let root = TempDirectory::new("evidence-variants");
    let target = root.path.join("target");
    fs::create_dir_all(target.join("ripr")).expect("lowercase evidence");
    fs::create_dir_all(target.join("RIPR")).expect("uppercase evidence");
    fs::write(target.join("ripr/lower"), b"1234").expect("lowercase receipt");
    fs::write(target.join("RIPR/upper"), b"123456").expect("uppercase receipt");

    let plan = build_plan(&target, u64::MAX).expect("plan");
    assert!(plan.selected.is_empty());
    assert_eq!(plan.retained_measurement.bytes, 10);
}

#[test]
fn quarantine_moves_source_to_a_new_direct_child() {
    let root = TempDirectory::new("quarantine");
    let target = root.path.join("target");
    fs::create_dir_all(target.join("debug/deps")).expect("debug tree");
    fs::write(target.join("debug/deps/a.rlib"), b"artifact").expect("artifact");
    let plan = build_plan(&target, u64::MAX).expect("plan");
    let entry = plan.selected.first().expect("selected debug entry");

    let quarantine = quarantine_planned_entry(&target, entry, 0).expect("quarantine entry");
    assert!(!entry.path.exists());
    assert!(quarantine.exists());
    assert_eq!(quarantine.parent(), Some(target.as_path()));
    assert!(
        quarantine
            .file_name()
            .and_then(OsStr::to_str)
            .is_some_and(|name| name.starts_with(".cargo-artifacts-delete-"))
    );
    fs::remove_dir_all(quarantine).expect("remove quarantine");
}

#[test]
fn cleanup_deletes_exact_selection_and_preserves_ripr() {
    let root = TempDirectory::new("delete");
    let target = root.path.join("target");
    fs::create_dir_all(target.join("debug/deps")).expect("debug tree");
    fs::write(target.join("debug/deps/a.rlib"), b"artifact").expect("artifact");
    fs::write(target.join("leak.tmp"), b"scratch").expect("scratch");
    fs::create_dir_all(target.join("ripr/reports")).expect("ripr tree");
    fs::write(target.join("ripr/reports/keep.json"), b"retained").expect("receipt");
    let plan = build_plan(&target, u64::MAX).expect("plan");

    for (sequence, entry) in plan.selected.iter().enumerate() {
        remove_planned_entry(&target, entry, sequence as u64).expect("remove selected entry");
    }

    assert!(!target.join("debug").exists());
    assert!(!target.join("leak.tmp").exists());
    assert!(target.join("ripr/reports/keep.json").exists());
    let remaining = fs::read_dir(&target)
        .expect("read target")
        .map(|entry| entry.expect("target entry").file_name())
        .collect::<Vec<_>>();
    assert_eq!(remaining, vec![OsStr::new("ripr").to_os_string()]);
}

#[test]
fn direct_child_guard_refuses_ancestor_nested_and_ripr_paths() {
    let root = TempDirectory::new("boundary");
    let target = root.path.join("target");
    fs::create_dir_all(target.join("debug/deps")).expect("tree");
    assert!(require_direct_child(&target, &target.join("debug")).is_ok());
    assert!(require_direct_child(&target, &target).is_err());
    assert!(require_direct_child(&target, &target.join("debug/deps")).is_err());
    assert!(require_direct_child(&target, &target.join("ripr")).is_err());
    assert!(require_direct_child(&target, &target.join("RIPR")).is_err());
}

#[test]
fn destructive_mode_requires_receipt() {
    let error = parse_options([
        "clean-current".to_string(),
        "--apply".to_string(),
        "--no-receipt".to_string(),
    ])
    .expect_err("destructive no-receipt refused");
    assert!(error.to_string().contains("refuses --no-receipt"));
}

#[test]
fn pending_failure_state_is_retained_in_both_receipts() {
    let root = TempDirectory::new("failure-receipt");
    let target = root.path.join("target");
    fs::create_dir_all(&target).expect("target");
    let plan = build_plan(&target, 1).expect("plan");
    let options = Options {
        operation: Operation::CleanCurrent,
        apply: true,
        max_size_bytes: 1,
        no_receipt: false,
    };
    let state = ApplyState {
        status: "partial_failure".to_string(),
        deleted: vec!["debug".to_string()],
        pending: Some("release".to_string()),
        failure: Some(Failure {
            path: target.join("release").display().to_string(),
            error: "locked".to_string(),
        }),
    };
    write_receipts(&target, &options, &plan, &state).expect("write receipts");
    let json = fs::read_to_string(target.join("ripr/reports/cargo-artifacts.json"))
        .expect("json receipt");
    let markdown = fs::read_to_string(target.join("ripr/reports/cargo-artifacts.md"))
        .expect("markdown receipt");
    assert!(json.contains("partial_failure"));
    assert!(json.contains("\"pending\": \"release\""));
    assert!(json.contains("locked"));
    assert!(markdown.contains("Pending outcome: `release`"));
    assert!(markdown.contains("Cleanup stopped"));
}

#[cfg(unix)]
#[test]
fn plan_refuses_symlink_without_traversing_it() {
    use std::os::unix::fs::symlink;

    let root = TempDirectory::new("symlink");
    let target = root.path.join("target");
    let outside = root.path.join("outside");
    fs::create_dir_all(target.join("debug")).expect("debug");
    fs::create_dir_all(&outside).expect("outside");
    fs::write(outside.join("keep"), b"outside").expect("outside file");
    symlink(&outside, target.join("debug/link")).expect("symlink");

    let error = build_plan(&target, u64::MAX).expect_err("symlink refused");
    assert!(error.to_string().contains("symlink or reparse"));
    assert!(outside.join("keep").exists());
}
