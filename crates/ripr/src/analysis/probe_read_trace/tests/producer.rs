use super::*;
use crate::analysis::{classifier, classify, rust_index};
use std::error::Error;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Result<Self, Box<dyn Error>> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let parent = std::env::temp_dir();
        std::fs::create_dir_all(&parent)?;
        let root = parent.join(format!(
            "ripr-probe-read-unit-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root)?;
        let fixture = Self(root);
        std::fs::create_dir(fixture.0.join("src"))?;
        std::fs::write(
            fixture.0.join("Cargo.toml"),
            "[package]\nname=\"trace_fixture\"\nversion=\"0.0.0\"\nedition=\"2024\"\n[workspace]\n",
        )?;
        std::fs::write(
            fixture.0.join("src/lib.rs"),
            "pub fn discount(total: i32) -> i32 { if total >= 10 { 90 } else { 100 } }\npub fn shipping(total: i32) -> i32 { if total >= 20 { 0 } else { 5 } }\n#[cfg(test)] mod tests { use super::*; #[test] fn boundaries() { assert_eq!(discount(10), 90); assert_eq!(shipping(20), 0); } }\n",
        )?;
        Ok(fixture)
    }
    fn index(&self) -> Result<rust_index::RustIndex, String> {
        rust_index::build_index(&self.0, &[PathBuf::from("src/lib.rs")])
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn assert_read(
    id: &str,
    kind: Kind,
    subject: &str,
    work: impl FnOnce() -> u32,
) -> Result<(), String> {
    let (expected_line, rows) = capture(true, || {
        let scope = Scope::begin(&probe(id));
        let expected_line = work();
        scope.finish();
        expected_line
    })?;
    assert_eq!(rows.len(), 1, "{id}");
    let row = rows.first().ok_or("accessor emitted no observation")?;
    assert_eq!(row.observed_read_events, 1, "{id}");
    assert_eq!(row.dropped_read_events, 0, "{id}");
    assert_eq!(row.reads.len(), 1, "{id}");
    let entry = row.reads.first().ok_or("accessor emitted no read")?;
    assert_eq!(entry.count, 1, "{id}");
    assert_eq!(entry.read.kind, kind, "{id}");
    assert_eq!(entry.read.subject, subject, "{id}");
    assert!(
        Path::new(entry.read.site.file).ends_with("probe_read_trace/tests/producer.rs"),
        "{id}: {:?}",
        entry.read.site
    );
    assert_eq!(entry.read.site.line, expected_line, "{id}");
    Ok(())
}

#[test]
fn actual_index_views_record_present_absent_and_all_conservative_sets() -> Result<(), Box<dyn Error>>
{
    let fixture = Fixture::new()?;
    let index = fixture.index()?;
    assert!(!index.functions().is_empty());
    assert!(!index.tests().is_empty());
    let present = Path::new("src/lib.rs");
    let absent = Path::new("absent.rs");
    assert_read("get-hit", Kind::File, "src/lib.rs", || {
        let line = line!() + 1;
        let found = index.files().get(present);
        assert!(found.is_some());
        line
    })?;
    assert_read("get-miss", Kind::Absent, "absent.rs", || {
        let line = line!() + 1;
        let found = index.files().get(absent);
        assert!(found.is_none());
        line
    })?;
    assert_read("get-key-hit", Kind::File, "src/lib.rs", || {
        let line = line!() + 1;
        let found = index.files().get_key_value(present);
        assert!(found.is_some());
        line
    })?;
    assert_read("get-key-miss", Kind::Absent, "absent.rs", || {
        let line = line!() + 1;
        let found = index.files().get_key_value(absent);
        assert!(found.is_none());
        line
    })?;
    assert_read("iter", Kind::Universe, "files", || {
        let line = line!() + 1;
        let mut files = index.files().iter();
        assert!(files.next().is_some());
        line
    })?;
    assert_read("values", Kind::Universe, "files", || {
        let line = line!() + 1;
        let mut files = index.files().values();
        assert!(files.next().is_some());
        line
    })?;
    assert_read("len", Kind::Universe, "files", || {
        let line = line!() + 1;
        let count = index.files().len();
        assert_eq!(count, 1);
        line
    })?;
    assert_read("functions", Kind::Universe, "functions", || {
        let line = line!() + 1;
        let functions = index.functions();
        assert!(!functions.is_empty());
        line
    })?;
    assert_read("tests", Kind::Universe, "tests", || {
        let line = line!() + 1;
        let tests = index.tests();
        assert!(!tests.is_empty());
        line
    })?;
    // A test read by position names that one fact; it is not a whole-set read.
    let first = index.tests().at(0);
    let subject = format!(
        "{}:{}:{}",
        first.file.display(),
        first.start_line,
        first.name
    );
    assert_read("test-at", Kind::Test, &subject, || {
        let line = line!() + 1;
        let found = index.test_at(0);
        assert_eq!(found.name, first.name);
        line
    })?;
    Ok(())
}

#[test]
fn actual_classification_preserves_findings_and_value_origins_with_shared_memos()
-> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new()?;
    let index = fixture.index()?;
    let probes = crate::analysis::probes::probes_for_repo_file_seeded(
        &fixture.0,
        Path::new("src/lib.rs"),
        &index,
    );
    assert!(
        probes.len() >= 2,
        "actual producer must seed multiple probes"
    );
    let candidates =
        classify::RelatedTestCandidateIndex::new(&index).with_analysis_root(&fixture.0);
    let reach = classify::TransitiveReachIndex::new(&index);
    let classify = || {
        let origins = std::sync::Mutex::new(crate::value_origins::ValueOrigins::default());
        let findings = probes
            .iter()
            .map(|seeded| {
                classifier::classify_probe_with_candidate_index_and_origins(
                    &seeded.probe,
                    &index,
                    true,
                    None,
                    &candidates,
                    &reach,
                    None,
                    Some(&origins),
                )
            })
            .collect::<Vec<_>>();
        let origins = origins.into_inner().map_err(|error| error.to_string())?;
        Ok::<_, String>((findings, origins))
    };
    let (plain, off_rows) = capture(false, classify)?;
    let (plain_findings, plain_origins) = plain?;
    let (traced, rows) = capture(true, classify)?;
    let (traced_findings, traced_origins) = traced?;
    assert!(off_rows.is_empty());
    assert_eq!(rows.len(), probes.len());
    for (row, seeded) in rows.iter().zip(&probes) {
        assert_eq!(row.probe_id.as_deref(), Some(seeded.probe.id.0.as_str()));
    }
    assert_eq!(
        serde_json::to_value(&plain_findings)?,
        serde_json::to_value(&traced_findings)?
    );
    assert_eq!(plain_origins, traced_origins);
    assert!(
        traced_findings
            .iter()
            .any(|finding| traced_origins.files(finding).is_some()),
        "fixture must exercise the real origin side output"
    );
    assert!(rows.iter().all(|row| row.observed_read_events > 0
        && row.classification_returned
        && row.coverage == "unknown"
        && !row.reusable));
    Ok(())
}
