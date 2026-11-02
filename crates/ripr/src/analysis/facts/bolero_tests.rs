//! #10758: Bolero registration through the real index.

use super::*;
use crate::analysis::syntax::bolero_profiles;
use crate::analysis::workspace::verified_bolero_identity;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const FIXTURE: &str = include_str!("../../../tests/fixtures/bolero_singleton/src/lib.rs");
const TEST: &str = include_str!("../../../tests/fixtures/bolero_singleton/tests/property.rs");
const MANIFEST: &str = include_str!("../../../tests/fixtures/bolero_singleton/Cargo.toml");
const LOCK: &str = include_str!("../../../tests/fixtures/bolero_singleton/Cargo.lock");

fn bolero_identity_available(root: &Path) -> bool {
    if verified_bolero_identity(root).is_some() {
        return true;
    }
    eprintln!(
        "skipping bolero registration assertions: verified bolero identity \
         unavailable for {} (offline cargo metadata needs the fixture \
         lockfile's crates in the local Cargo cache)",
        root.display()
    );
    false
}

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Result<Self, String> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-bolero-index-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root).map_err(|error| error.to_string())?;
        let fixture = Self(root);
        for directory in ["src", "tests"] {
            std::fs::create_dir(fixture.0.join(directory)).map_err(|error| error.to_string())?;
        }
        fixture.write("Cargo.toml", MANIFEST)?;
        fixture.write("Cargo.lock", LOCK)?;
        fixture.write("src/lib.rs", FIXTURE)?;
        fixture.write("tests/property.rs", TEST)?;
        Ok(fixture)
    }

    fn write(&self, path: &str, source: &str) -> Result<(), String> {
        std::fs::write(self.0.join(path), source).map_err(|error| error.to_string())
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!("retained failed bolero fixture {}", self.0.display());
        } else if let Err(error) = std::fs::remove_dir_all(&self.0) {
            eprintln!("bolero fixture cleanup {}: {error}", self.0.display());
        }
    }
}

#[test]
fn bolero_profiles_match_fixture_test() {
    let profiles = bolero_profiles(TEST);
    assert_eq!(profiles.len(), 1);
    assert_eq!(profiles[0].function_name, "increment_matches_model");
    assert_eq!(profiles[0].input_type, "u8");
    assert!(profiles[0].complete);
}

#[test]
fn register_bolero_tests_retains_receipt_for_audited_property() -> Result<(), String> {
    let fixture = Fixture::new()?;
    if !bolero_identity_available(&fixture.0) {
        return Ok(());
    }
    let index = build_index(
        &fixture.0,
        &[
            PathBuf::from("src/lib.rs"),
            PathBuf::from("tests/property.rs"),
        ],
    )?;
    assert_eq!(index.bolero_receipts.len(), 1);
    let receipt = index
        .bolero_receipts
        .values()
        .next()
        .ok_or("missing bolero receipt")?;
    assert_eq!(receipt.input_type, "u8");
    assert_eq!(receipt.test.name, "increment_matches_model");
    Ok(())
}
