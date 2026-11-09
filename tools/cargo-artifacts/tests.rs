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

    for entry in &plan.selected {
        remove_planned_entry(&target, entry).expect("remove selected entry");
    }

    assert!(!target.join("debug").exists());
    assert!(!target.join("leak.tmp").exists());
    assert!(target.join("ripr/reports/keep.json").exists());
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
fn failure_state_is_retained_in_both_receipts() {
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
    assert!(json.contains("locked"));
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
