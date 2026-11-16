//! #14731: construction owns cleanup before a caller can arm its guard.
use super::{
    DeclaredHunkScratch, repair_route_workspace, repair_route_workspace_at, run_git,
    unique_temp_workspace,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn setup_error_removes_the_partially_populated_workspace() -> TestResult {
    let root = unique_temp_workspace("repair route setup error é");
    // Retain a test-owned fallback so the negative control cannot leak either.
    let _fallback = DeclaredHunkScratch(root.clone());
    std::fs::create_dir_all(root.join("src"))?;
    std::fs::write(root.join("src/sentinel"), "partial fixture")?;
    // A regular file deterministically refuses create_dir_all on every platform,
    // including when the test process can write read-only directories.
    std::fs::write(root.join("tests"), "directory blocker")?;
    assert!(root.join("src/sentinel").is_file());
    assert!(root.join("tests").is_file());

    let result = repair_route_workspace_at(root.clone());
    assert!(result.is_err(), "the setup blocker must be reached");
    assert!(
        !root.exists(),
        "construction must clean up before returning its setup error: {}",
        root.display()
    );
    Ok(())
}

#[test]
fn successful_setup_survives_until_the_caller_finishes() -> TestResult {
    let root = repair_route_workspace("repair route setup success é")?;
    let cleanup = DeclaredHunkScratch(root.clone());
    assert!(root.join("src/lib.rs").is_file());
    assert!(root.join("tests/pricing.rs").is_file());
    // Success must include the committed repository, not just written files.
    run_git(&root, &["rev-parse", "--verify", "HEAD"])?;
    run_git(&root, &["diff", "--exit-code", "HEAD", "--"])?;
    drop(cleanup);
    assert!(!root.exists(), "caller cleanup must remove a successful fixture");
    Ok(())
}

#[test]
fn caller_assertion_unwind_removes_the_successful_workspace() -> TestResult {
    let root = repair_route_workspace("repair route assertion unwind ü")?;
    let _fallback = DeclaredHunkScratch(root.clone());
    let unwound = std::panic::catch_unwind(|| {
        let _cleanup = DeclaredHunkScratch(root.clone());
        assert!(root.join("src/lib.rs").is_file());
        assert_eq!(1, 2, "deliberate caller assertion failure");
    });
    assert!(unwound.is_err(), "the deliberate assertion must unwind");
    assert!(!root.exists(), "the caller guard must clean up while unwinding");
    Ok(())
}
