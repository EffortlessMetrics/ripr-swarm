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

impl Fixture {
    fn write_member(&self, member: &str, bytes: &[u8]) -> std::io::Result<()> {
        let directory = self.0.join(member);
        std::fs::create_dir_all(&directory)?;
        std::fs::write(directory.join("Cargo.toml"), bytes)
    }

    /// The uncached prefilter and a fresh memo must agree on every layout.
    fn eligible(&self) -> bool {
        let uncached = eligible_json_root(&self.0);
        assert_eq!(
            JsonRootEligibilityMemo::default().eligible(&self.0),
            uncached
        );
        uncached
    }
}

const MEMBER: &[u8] = b"[package]\nname='demo'\n[dependencies]\nserde_json='1'\n";
const OTHER: &[u8] = b"[package]\nname='demo'\n[dependencies]\nserde_jsom='1'\n";

/// #14116: ripr's own layout. A virtual workspace root names no dependency,
/// and the member that uses `json!` declares serde_json itself.
#[test]
fn a_named_member_that_declares_serde_json_makes_a_virtual_root_eligible()
-> Result<(), Box<dyn std::error::Error>> {
    for (members, member) in [
        ("['crates/demo']", "crates/demo"),
        ("['tools/other', 'crates/demo']", "crates/demo"),
        ("['crates/*']", "crates/demo"),
        ("['demo']", "demo"),
    ] {
        let fixture = Fixture::new()?;
        fixture.write(format!("[workspace]\nmembers={members}\n").as_bytes())?;
        assert!(!fixture.eligible(), "{members}: no member manifest yet");
        fixture.write_member(member, OTHER)?;
        assert!(!fixture.eligible(), "{members}: member names another crate");
        fixture.write_member(member, MEMBER)?;
        assert!(fixture.eligible(), "{members}: member declares serde_json");
        fixture.write_member(
            member,
            b"[package]\nname='demo'\n[dev-dependencies]\nserde_json='1'\n",
        )?;
        assert!(fixture.eligible(), "{members}: dev declaration");
    }
    Ok(())
}

#[test]
fn a_member_declaration_the_root_does_not_reach_stays_ineligible()
-> Result<(), Box<dyn std::error::Error>> {
    // The declaring manifest exists in every row; only the root's reach differs.
    for (root, member) in [
        ("[workspace]\nmembers=['crates/other']\n", "crates/demo"),
        ("[workspace]\n", "crates/demo"),
        ("[workspace]\nexclude=['crates/demo']\n", "crates/demo"),
        ("[package]\nname='root'\n", "crates/demo"),
        ("[workspace]\nmembers='crates/demo'\n", "crates/demo"),
        ("[workspace]\nmembers=['crates/**']\n", "crates/demo"),
        ("[workspace]\nmembers=['crates/d*']\n", "crates/demo"),
        ("[workspace]\nmembers=['*/demo']\n", "crates/demo"),
        ("[workspace]\nmembers=['crates/[d]emo']\n", "crates/demo"),
        ("[workspace]\nmembers=['crates/*']\n", "crates/nested/demo"),
        (
            "[workspace]\nmembers=['crates/../crates/demo']\n",
            "crates/demo",
        ),
        ("[workspace]\nmembers=['./crates/demo']\n", "crates/demo"),
        ("[workspace]\nmembers=['crates/./demo']\n", "crates/demo"),
        ("[workspace]\nmembers=['crates//demo']\n", "crates/demo"),
        ("[workspace]\nmembers=['crates/demo/']\n", "crates/demo"),
        ("[workspace]\nmembers=['crates/./*']\n", "crates/demo"),
        ("[workspace]\nmembers=['']\n", ""),
        ("[workspace]\nmembers=['/*']\n", "demo"),
        ("[workspace\nmembers=['crates/demo']\n", "crates/demo"),
    ] {
        let fixture = Fixture::new()?;
        fixture.write_member(member, MEMBER)?;
        fixture.write(root.as_bytes())?;
        assert!(!fixture.eligible(), "{root}");
    }
    // A member outside the analysis root is never read through `..` or an
    // absolute entry.
    let outside = Fixture::new()?;
    outside.write(MEMBER)?;
    let fixture = Fixture::new()?;
    let name = outside.0.file_name().and_then(|name| name.to_str());
    let name = name.ok_or("fixture name")?;
    for members in [
        format!("['../{name}']"),
        format!("['{}']", outside.0.display()),
    ] {
        fixture.write(format!("[workspace]\nmembers={members}\n").as_bytes())?;
        assert!(!fixture.eligible(), "{members}");
    }
    Ok(())
}

#[test]
fn a_member_table_other_than_normal_or_dev_dependencies_stays_ineligible()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    fixture.write(b"[workspace]\nmembers=['crates/demo']\n")?;
    for member in [
        &b"[build-dependencies]\nserde_json='1'\n"[..],
        b"[target.'cfg(unix)'.dependencies]\nserde_json='1'\n",
        b"[workspace.dependencies]\nserde_json='1'\n",
        b"[features]\nserde_json=[]\n",
        b"# [dependencies]\n# serde_json='1'\n",
        b"[dependencies\nserde_json='1'\n",
        b"\xff",
    ] {
        fixture.write_member("crates/demo", member)?;
        assert!(!fixture.eligible(), "{member:?}");
    }
    fixture.write_member("crates/demo", MEMBER)?;
    assert!(fixture.eligible());
    Ok(())
}

#[test]
fn member_manifest_changes_are_read_on_every_memo_query() -> Result<(), Box<dyn std::error::Error>>
{
    let fixture = Fixture::new()?;
    fixture.write(b"[workspace]\nmembers=['crates/*']\n")?;
    fixture.write_member("crates/demo", MEMBER)?;
    let memo = JsonRootEligibilityMemo::default();
    let before = parses();
    assert!(memo.eligible(&fixture.0));
    assert!(memo.eligible(&fixture.0));
    assert_eq!(parses() - before, 1, "the unchanged root parses once");
    assert_eq!(MEMBER.len(), OTHER.len());
    fixture.write_member("crates/demo", OTHER)?;
    assert!(!memo.eligible(&fixture.0), "same-size member rewrite");
    fixture.write_member("crates/later", MEMBER)?;
    assert!(memo.eligible(&fixture.0), "a member directory added later");
    std::fs::remove_dir_all(fixture.0.join("crates/later"))?;
    assert!(!memo.eligible(&fixture.0), "the declaring member removed");
    fixture.write_member("crates/demo", MEMBER)?;
    assert!(memo.eligible(&fixture.0));
    std::fs::remove_file(fixture.0.join("crates/demo/Cargo.toml"))?;
    std::fs::create_dir(fixture.0.join("crates/demo/Cargo.toml"))?;
    assert!(!memo.eligible(&fixture.0), "member manifest is a directory");
    assert_eq!(
        parses() - before,
        1,
        "member changes do not reparse the root"
    );
    Ok(())
}

#[test]
fn more_member_directories_than_the_budget_stay_ineligible()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    // The literal entries repeat directories the glob already lists; a
    // directory is charged once however many entries name it.
    fixture.write(b"[workspace]\nmembers=['crates/*','crates/m000','crates/m000']\n")?;
    for member in 0..JSON_MEMBER_BUDGET {
        fixture.write_member(&format!("crates/m{member:03}"), OTHER)?;
    }
    fixture.write_member("crates/m255", MEMBER)?;
    // Plain files beside the members are listed but are not members.
    for file in 0..8 {
        std::fs::write(fixture.0.join(format!("crates/note{file}.md")), b"")?;
    }
    assert!(fixture.eligible(), "at the budget");
    std::fs::create_dir(fixture.0.join("crates/over"))?;
    assert!(!fixture.eligible(), "one member directory over the budget");
    Ok(())
}
