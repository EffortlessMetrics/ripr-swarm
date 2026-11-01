use super::*;
use std::sync::Arc;

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let path = std::env::temp_dir().join(format!(
            "ripr-json-eligibility-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed),
        ));
        std::fs::create_dir(&path)?;
        Ok(Self(path))
    }

    fn write(&self, bytes: &[u8]) -> std::io::Result<()> {
        std::fs::write(self.0.join("Cargo.toml"), bytes)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn parses() -> usize {
    JSON_ROOT_ELIGIBILITY_PARSES.with(std::cell::Cell::get)
}

const ELIGIBLE: &[u8] = b"[dependencies]\nserde_json='1'\n";
const INELIGIBLE: &[u8] = b"[dependencies]\nserde_jsom='1'\n";

#[test]
fn identical_fresh_manifest_bytes_parse_once() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    fixture.write(ELIGIBLE)?;
    let memo = Arc::new(JsonRootEligibilityMemo::default());
    let clone = Arc::clone(&memo);
    let before = parses();
    for _ in 0..12 {
        assert!(memo.eligible(&fixture.0));
        assert!(clone.eligible(&fixture.0));
    }
    assert_eq!(parses() - before, 1, "clones share one actual pure parse");
    let other = JsonRootEligibilityMemo::default();
    assert!(other.eligible(&fixture.0));
    assert_eq!(
        parses() - before,
        2,
        "independent authorities parse independently"
    );
    Ok(())
}

#[test]
fn changed_bytes_invalidate_despite_same_size_and_mtime() -> Result<(), Box<dyn std::error::Error>>
{
    let fixture = Fixture::new()?;
    fixture.write(ELIGIBLE)?;
    let memo = JsonRootEligibilityMemo::default();
    assert!(memo.eligible(&fixture.0));
    let path = fixture.0.join("Cargo.toml");
    let modified = std::fs::metadata(&path)?.modified()?;
    assert_eq!(ELIGIBLE.len(), INELIGIBLE.len());
    fixture.write(INELIGIBLE)?;
    std::fs::File::options()
        .write(true)
        .open(&path)?
        .set_times(std::fs::FileTimes::new().set_modified(modified))?;
    assert!(!memo.eligible(&fixture.0));
    fixture.write(ELIGIBLE)?;
    assert!(memo.eligible(&fixture.0));
    Ok(())
}

#[test]
fn absent_unreadable_and_oversized_inputs_cannot_reuse_true()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    fixture.write(ELIGIBLE)?;
    let memo = JsonRootEligibilityMemo::default();
    assert!(memo.eligible(&fixture.0));
    let path = fixture.0.join("Cargo.toml");
    std::fs::remove_file(&path)?;
    assert!(!memo.eligible(&fixture.0), "absent manifest");
    std::fs::create_dir(&path)?;
    assert!(!memo.eligible(&fixture.0), "manifest is a directory");
    std::fs::remove_dir(&path)?;
    std::fs::File::create(&path)?.set_len(BUDGET + 1)?;
    assert!(
        !memo.eligible(&fixture.0),
        "bounded read refuses oversized file"
    );
    fixture.write(ELIGIBLE)?;
    assert!(
        memo.eligible(&fixture.0),
        "recovery still reads current bytes"
    );
    Ok(())
}

#[test]
fn false_and_malformed_verdicts_are_keyed_by_exact_bytes() -> Result<(), Box<dyn std::error::Error>>
{
    let fixture = Fixture::new()?;
    let memo = JsonRootEligibilityMemo::default();
    for bytes in [INELIGIBLE, b"[dependencies\n", b"\xff"] {
        fixture.write(bytes)?;
        let before = parses();
        assert!(!memo.eligible(&fixture.0));
        assert!(!memo.eligible(&fixture.0));
        assert_eq!(parses() - before, 1, "present false bytes parse once");
        fixture.write(ELIGIBLE)?;
        assert!(memo.eligible(&fixture.0));
    }
    Ok(())
}

#[test]
fn memo_preserves_root_eligibility_boundaries() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let memo = JsonRootEligibilityMemo::default();
    let cases: &[(&[u8], bool)] = &[
        (ELIGIBLE, true),
        (INELIGIBLE, false),
        (b"[dev-dependencies]\nserde_json='1'", true),
        (b"[workspace.dependencies]\nserde_json='1'", true),
        (b"[build-dependencies]\nserde_json='1'", false),
        (b"[dev_dependencies]\nserde_json='1'", false),
        (b"[target.'cfg(unix)'.dependencies]\nserde_json='1'", false),
        (b"[dependencies]\nserde_json={path='local'}", true),
        (b"[dependencies]\nserde_json=false", true),
        (b"# serde_json='1'", false),
        (b"", false),
        (b"[dependencies]\nserde_json='1'\nserde_json='2'", false),
        (b"[dependencies\n", false),
        (b"\xff", false),
    ];
    for (bytes, expected) in cases {
        fixture.write(bytes)?;
        assert_eq!(
            eligible_json_root(&fixture.0),
            *expected,
            "uncached {bytes:?}"
        );
        assert_eq!(memo.eligible(&fixture.0), *expected, "memo miss {bytes:?}");
        assert_eq!(memo.eligible(&fixture.0), *expected, "memo hit {bytes:?}");
    }
    let other = Fixture::new()?;
    fixture.write(ELIGIBLE)?;
    other.write(ELIGIBLE)?;
    assert!(memo.eligible(&fixture.0));
    assert!(memo.eligible(&other.0));
    other.write(INELIGIBLE)?;
    assert!(!memo.eligible(&other.0));
    assert!(memo.eligible(&fixture.0), "the original root is read again");
    // Eligibility only names a dependency table; all source/version/lock and
    // ambient authority still belongs to verified_json_identity and its callers.
    Ok(())
}
