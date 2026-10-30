//! Reservation controls for #10993; all calls use the production store path.

use super::super::extract_source_facts;
use super::*;
use std::collections::BTreeSet;
use std::sync::Barrier;

const SOURCE: &str = "def amount(value):\n    return value + 1\n";
static NEXT_ROOT: AtomicUsize = AtomicUsize::new(0);

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new() -> Result<Self, String> {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|err| err.to_string())?
            .as_nanos();
        let id = NEXT_ROOT.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "ripr-python-budget-{}-{stamp}-{id}",
            std::process::id()
        ));
        std::fs::create_dir(&root).map_err(|err| err.to_string())?;
        Ok(Self { root })
    }

    fn store(&self, label: &str) -> PythonFileFactStore {
        // Do not inherit RIPR_CACHE_DIR or share a store with another test.
        PythonFileFactStore::at_base(&self.root.join(label))
    }

    fn entry(
        &self,
        name: &str,
    ) -> Result<(PythonFileFactKey, PythonSourceFacts, Vec<u8>), String> {
        let path = Path::new(name);
        let key = PythonFileFactKey::new(path, SOURCE, &self.root);
        let facts = extract_source_facts(path, SOURCE);
        assert!(!facts.owners.is_empty(), "exercise a real Python owner");
        let envelope = PythonFileFactEnvelope::new(&key, StoredFacts::from_facts(&facts));
        let bytes =
            seam_cache::encode_integrity_body_compact(&envelope, envelope.expected_digest()?)?;
        assert!(!bytes.is_empty());
        Ok((key, facts, bytes))
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn store_reserves_exactly_the_persisted_bytes_including_exact_capacity() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let (key, facts, bytes) = fixture.entry("src/amount.py")?;
    for spare in [0, 7] {
        let store = fixture.store(&format!("enough-{spare}"));
        store
            .remaining_store_bytes
            .store(bytes.len() + spare, Ordering::Relaxed);
        assert_eq!(store.store(&key, &facts), Ok(true));
        assert_eq!(store.remaining_store_bytes.load(Ordering::Relaxed), spare);
        assert_eq!(
            std::fs::read(store.dir.join(key.filename())).map_err(|err| err.to_string())?,
            bytes
        );
        match store.load(&key) {
            CacheLoad::Hit(served) => assert_eq!(served, served_view(facts.clone())),
            _ => return Err("reserved entry was not served".to_string()),
        }
    }
    Ok(())
}

#[test]
fn insufficient_capacity_neither_spends_budget_nor_creates_a_store() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let (key, facts, bytes) = fixture.entry("src/amount.py")?;
    for available in [0, bytes.len() - 1] {
        let store = fixture.store(&format!("short-{available}"));
        store
            .remaining_store_bytes
            .store(available, Ordering::Relaxed);
        for _ in 0..2 {
            assert_eq!(store.store(&key, &facts), Ok(false));
            assert_eq!(
                store.remaining_store_bytes.load(Ordering::Relaxed),
                available
            );
            assert!(!store.dir.exists(), "a refused reservation must not write");
            assert!(matches!(store.load(&key), CacheLoad::Miss));
        }
    }
    Ok(())
}

#[test]
fn concurrent_stores_cannot_overspend_the_shared_budget() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let entries = (0..8)
        .map(|id| fixture.entry(&format!("src/amount_{id}.py")))
        .collect::<Result<Vec<_>, _>>()?;
    let entry_bytes = entries
        .first()
        .map(|(_, _, bytes)| bytes.len())
        .ok_or_else(|| "concurrency fixture has no entries".to_string())?;
    assert!(
        entries.iter().all(|(_, _, bytes)| bytes.len() == entry_bytes),
        "equal-size entries make the admission count independent of scheduling"
    );
    let filenames = entries
        .iter()
        .map(|(key, _, _)| key.filename())
        .collect::<BTreeSet<_>>();
    assert_eq!(filenames.len(), entries.len(), "each writer has its own key");
    let expected_admissions = 3;
    let available = entry_bytes * expected_admissions;
    assert!(entries.len() > expected_admissions);

    // Each round has ONE store and AtomicUsize shared by all eight workers.
    // Repeat with fresh budgets; this exercises contention, not every schedule.
    for round in 0..8 {
        let store = fixture.store(&format!("concurrent-{round}"));
        store
            .remaining_store_bytes
            .store(available, Ordering::Relaxed);
        let barrier = Barrier::new(entries.len());
        let results = std::thread::scope(|scope| {
            let handles = entries
                .iter()
                .map(|(key, facts, _)| {
                    let key = key.clone();
                    let facts = facts.clone();
                    let store = &store;
                    let barrier = &barrier;
                    scope.spawn(move || {
                        barrier.wait();
                        store.store(&key, &facts)
                    })
                })
                .collect::<Vec<_>>();
            handles
                .into_iter()
                .map(|handle| {
                    handle
                        .join()
                        .map_err(|_| "budget worker panicked".to_string())?
                })
                .collect::<Result<Vec<_>, String>>()
        })?;
        assert_eq!(results.len(), entries.len());
        assert_eq!(
            results.iter().filter(|stored| **stored).count(),
            expected_admissions,
            "exactly three equal-size entries fit the shared budget"
        );
        let spent = entries
            .iter()
            .zip(&results)
            .filter(|(_, stored)| **stored)
            .map(|((_, _, bytes), _)| bytes.len())
            .sum::<usize>();
        assert!(spent <= available, "concurrent callers overspent the budget");
        let remaining = store.remaining_store_bytes.load(Ordering::Relaxed);
        assert_eq!(remaining, available - spent);
        assert_eq!(remaining, 0, "three admissions consume the exact budget");
        for ((key, facts, bytes), stored) in entries.iter().zip(results) {
            let path = store.dir.join(key.filename());
            if stored {
                assert_eq!(
                    std::fs::read(path).map_err(|err| err.to_string())?,
                    *bytes
                );
                match store.load(key) {
                    CacheLoad::Hit(served) => assert_eq!(served, served_view(facts.clone())),
                    _ => return Err("a successful concurrent store was not served".to_string()),
                }
            } else {
                assert!(!path.exists(), "a refused entry was persisted");
                assert!(
                    remaining < bytes.len(),
                    "the final remainder must be smaller than each refused entry"
                );
                assert!(matches!(store.load(key), CacheLoad::Miss));
            }
        }
    }
    Ok(())
}

#[test]
fn a_write_error_keeps_the_existing_reservation_without_a_refund() -> Result<(), String> {
    // Characterize the existing per-run accounting, not a recommendation:
    // refunding a failed write would be a separate behavior change.
    let fixture = Fixture::new()?;
    let (key, facts, bytes) = fixture.entry("src/amount.py")?;
    let store = fixture.store("write-error");
    let parent = store
        .dir
        .parent()
        .ok_or_else(|| "store directory has no parent".to_string())?;
    std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    std::fs::write(&store.dir, b"not a directory").map_err(|err| err.to_string())?;
    store
        .remaining_store_bytes
        .store(bytes.len(), Ordering::Relaxed);
    let result = store.store(&key, &facts);
    assert!(
        matches!(&result, Err(reason) if reason.starts_with("create python file fact cache dir failed:")),
        "expected the real persistence failure, got {result:?}"
    );
    assert_eq!(store.remaining_store_bytes.load(Ordering::Relaxed), 0);
    assert_eq!(store.store(&key, &facts), Ok(false));
    assert_eq!(
        std::fs::read(&store.dir).map_err(|err| err.to_string())?,
        b"not a directory"
    );
    Ok(())
}
