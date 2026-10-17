//! Shared assertion producer for the literal-JSON runtime control and #8445.

pub(crate) fn assert_runtime_output(
    case: &str,
    broken: bool,
    admitted: bool,
    output: &std::process::Output,
) {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    // These are captured child diagnostics, not owning Cargo/libtest records.
    // Debug escapes line breaks and control bytes in both streams.
    assert!(
        stdout.contains("running 1 test") && stdout.contains("0 ignored"),
        "{case}, broken={broken}: no executed subject: status={}; stdout={stdout:?}; stderr={stderr:?}",
        output.status
    );
    assert_eq!(
        output.status.success(),
        !broken || !admitted,
        "{case}, broken={broken}: status={}; stdout={stdout:?}; stderr={stderr:?}",
        output.status
    );
}
