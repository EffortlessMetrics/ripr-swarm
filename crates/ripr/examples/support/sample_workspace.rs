//! The bundled sample crate as a scratch workspace, shared by the examples.
//!
//! `examples/sample` ships without a `Cargo.toml`, because Cargo leaves a
//! nested package out of a published crate. `ripr` reads the manifest to
//! resolve the sample's tests, so each example copies the sample into a
//! scratch directory, adds a manifest, and analyzes that copy. The copy is
//! removed when the workspace is dropped.

use ripr::{CheckInput, OutputFormat};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const MANIFEST: &str =
    "[package]\nname = \"ripr_sample\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[workspace]\n";
const SOURCE: &str = include_str!("../sample/src/lib.rs");
const TESTS: &str = include_str!("../sample/tests/pricing.rs");
/// The change under review: `price` and `validate_currency` gain their
/// conditions. Paths are relative to the sample root.
const DIFF: &str = include_str!("../library.diff");

pub struct SampleWorkspace {
    pub root: PathBuf,
}

impl SampleWorkspace {
    /// Copies the sample into a fresh scratch directory. `example` keeps the
    /// directories of different examples apart.
    pub fn create(example: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-example-{example}-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        // `create_dir` refuses a path that already exists, so a leftover or
        // planted directory of the same name is never reused.
        std::fs::create_dir(&root)
            .map_err(|error| format!("create {} failed: {error}", root.display()))?;
        let workspace = Self { root };
        for (path, text) in [
            ("Cargo.toml", MANIFEST),
            ("src/lib.rs", SOURCE),
            ("tests/pricing.rs", TESTS),
            ("library.diff", DIFF),
        ] {
            let path = workspace.root.join(path);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|error| format!("create {} failed: {error}", parent.display()))?;
            }
            std::fs::write(&path, text)
                .map_err(|error| format!("write {} failed: {error}", path.display()))?;
        }
        Ok(workspace)
    }

    /// A check of the sample change against the tests currently in the copy.
    pub fn input(&self) -> CheckInput {
        CheckInput {
            root: self.root.clone(),
            diff_file: Some(self.root.join("library.diff")),
            format: OutputFormat::Json,
            ..CheckInput::default()
        }
    }
}

impl Drop for SampleWorkspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
