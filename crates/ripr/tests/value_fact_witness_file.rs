//! #8389: the slice-cardinality witness fact names the test file that holds
//! the witnessing call. `ValueFact::file` is serde-skipped, so this reads the
//! typed `check_workspace` output rather than JSON.

use ripr::{CheckInput, Mode, OutputFormat, check_workspace};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn slice_table_witness_names_its_test_file() -> Result<(), String> {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "ripr-witness-file-{}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let scratch = Scratch(root);
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/rust_slice_table_boundary");
    std::fs::create_dir_all(scratch.0.join("src")).map_err(|error| error.to_string())?;
    std::fs::copy(
        fixture.join("input/Cargo.toml"),
        scratch.0.join("Cargo.toml"),
    )
    .map_err(|error| error.to_string())?;
    // Move the changed boundary to length 2, which the table does supply, so
    // the boundary is a witnessed input instead of a missing one.
    let source = std::fs::read_to_string(fixture.join("input/src/lib.rs"))
        .map_err(|error| error.to_string())?;
    let source = source.replacen("samples.len() == 0", "samples.len() == 2", 1);
    assert!(
        source.contains("samples.len() == 2"),
        "fixture predicate edited"
    );
    let diff_file = scratch.0.join("change.diff");
    std::fs::write(
        &diff_file,
        "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -18,1 +18,1 @@\n-    if samples.len() == 0 {\n+    if samples.len() == 2 {\n",
    )
    .map_err(|error| error.to_string())?;
    std::fs::write(scratch.0.join("src/lib.rs"), source).map_err(|error| error.to_string())?;
    let output = check_workspace(CheckInput {
        root: scratch.0.clone(),
        diff_file: Some(diff_file),
        mode: Mode::Fast,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })?;
    let witnesses = output
        .findings
        .iter()
        .flat_map(|finding| finding.activation.observed_values.iter())
        .filter(|fact| fact.text.contains("slice table cardinality"))
        .collect::<Vec<_>>();
    assert!(
        !witnesses.is_empty(),
        "the slice witness fact is produced: {:?}",
        output
            .findings
            .iter()
            .map(|f| (
                f.activation
                    .observed_values
                    .iter()
                    .filter(|v| v.value.contains("=="))
                    .collect::<Vec<_>>(),
                &f.activation.missing_discriminators
            ))
            .collect::<Vec<_>>()
    );
    for fact in witnesses {
        assert_eq!(
            fact.file.as_deref(),
            Some(Path::new("src/lib.rs")),
            "{fact:?}"
        );
    }
    Ok(())
}
