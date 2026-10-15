//! The VSIX must carry its license terms without requiring a repository checkout.
use std::{fs, path::Path};

#[test]
fn extension_license_carries_both_complete_canonical_license_texts() -> Result<(), std::io::Error> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let packaged_license = fs::read_to_string(root.join("editors/vscode/LICENSE"))?;
    for name in ["LICENSE-MIT", "LICENSE-APACHE"] {
        let canonical = fs::read_to_string(root.join(name))?;
        assert!(!canonical.trim().is_empty(), "{name} must contain terms");
        assert!(
            packaged_license.contains(canonical.trim()),
            "editors/vscode/LICENSE must carry the complete {name} text"
        );
    }
    Ok(())
}
