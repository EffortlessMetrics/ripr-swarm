//! The bundled sample crate as a scratch workspace, shared by the examples.
//!
//! `examples/sample` ships without a `Cargo.toml`, because Cargo leaves a
//! nested package out of a published crate. `ripr` reads the manifest to
//! resolve the sample's tests, so each example copies the sample into a
//! scratch directory, adds a manifest, and analyzes that copy. The copy is
//! kept after the example exits, so the root and diff that the printed output
//! names (for example the `ripr context` command after an explanation) still
//! exist, and the copy can be opened or tested as an ordinary crate.

use ripr::{CheckInput, OutputFormat};
use std::path::PathBuf;

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
    /// Recreates the scratch copy for `example` at a fixed path under the
    /// temporary directory, so every run of an example names the same root.
    pub fn create(example: &str) -> Result<Self, String> {
        let root = std::env::temp_dir().join(format!("ripr-example-{example}"));
        match std::fs::remove_dir_all(&root) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("remove {} failed: {error}", root.display())),
        }
        // `create_dir` refuses a path that still exists, so a directory this
        // run could not clear is never written into.
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
