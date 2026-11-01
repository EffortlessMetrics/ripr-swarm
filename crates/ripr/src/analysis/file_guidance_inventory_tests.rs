//! #11358: complete file-guidance reuse keeps full evidence and current bytes.
use super::*;
use crate::analysis::facts::index_build_probe::observe;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);
const FILE: &str = "src/lib.rs";
const SOURCE: &str = "pub fn discount(price: u32, qty: u32) -> u32 {\n    if qty > 10 { price * 2 } else { price }\n}\npub mod other;\n";
const WEAK: &str =
    "#[test]\nfn weak_at_boundary() {\n    assert!(guidance_inventory::discount(10, 10) > 0);\n}\n";
const STRONG: &str = "#[test]\nfn exact_at_boundary() {\n    assert_eq!(guidance_inventory::discount(10, 10), 10);\n}\n";

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Result<Self, String> {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "ripr-file-guidance-{}-{stamp}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let fixture = Self(root);
        fixture.write("Cargo.toml", "[package]\nname = \"guidance_inventory\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[workspace]\n")?;
        fixture.write(FILE, SOURCE)?;
        fixture.write(
            "src/other.rs",
            "pub fn other(value: u32) -> u32 { value + 1 }\n",
        )?;
        fixture.write("tests/weak.rs", WEAK)?;
        Ok(fixture)
    }

    fn write(&self, path: &str, content: &str) -> Result<(), String> {
        let path = self.0.join(path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        std::fs::write(path, content).map_err(|error| error.to_string())
    }

    fn run(
        &self,
        file: &str,
        config: &RiprConfig,
    ) -> Result<(ScopedClassifiedSeamInventory, usize, usize), String> {
        evaluation_probe::reset();
        let (result, builds) =
            observe(|| inventory_file_guidance_at_with_config(&self.0, config, Path::new(file)));
        Ok((result?, builds, evaluation_probe::evaluated()))
    }

    fn canonical(
        &self,
        file: &str,
        config: &RiprConfig,
    ) -> Result<ScopedClassifiedSeamInventory, String> {
        inventory_diff_scoped_classified_seams_at_with_config(
            &self.0,
            config,
            &[PathBuf::from(file)],
            &[],
        )
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn projection(inventory: &ScopedClassifiedSeamInventory) -> serde_json::Value {
    serde_json::json!({
        "classified": inventory.classified,
        "considered": inventory.classified_seams_considered,
        "rust_files": inventory.total_rust_files,
        "production_files": inventory.total_production_files,
        "scope": inventory.scoped_production_files,
        "changed": inventory.changed_production_files,
        "declined": inventory.parser_declined_files,
        "callers": inventory.immediate_caller_files,
        "unevaluated": inventory.unevaluated_seams,
        "absent": inventory.absent_changed_files,
        "limit": format!("{:?}", inventory.repo_seam_limit),
    })
}

fn guidance(inventory: &ScopedClassifiedSeamInventory) -> Option<&str> {
    inventory.classified.iter().find_map(|entry| {
        super::super::repair_route::boundary_observed_weak_oracle_test(entry)
            .map(|test| test.test_name.as_str())
    })
}

fn forget(inventory: &ScopedClassifiedSeamInventory, file: &str) {
    bound_inventory_reuse::forget(
        &inventory.workspace_cache_key,
        &file_guidance_reuse_identity(Path::new(file)),
    );
}

fn entry_path(
    root: &Path,
    inventory: &ScopedClassifiedSeamInventory,
    file: &str,
) -> Result<PathBuf, String> {
    let (dir, _) = super::super::seam_cache::RepoScopedInventoryCache::at(root).entry_dir();
    let key =
        serde_json::to_value(&inventory.workspace_cache_key).map_err(|error| error.to_string())?;
    let identity: String = file_guidance_reuse_identity(Path::new(file))
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    for entry in std::fs::read_dir(dir).map_err(|error| error.to_string())? {
        let path = entry.map_err(|error| error.to_string())?.path();
        let bytes = std::fs::read(&path).map_err(|error| error.to_string())?;
        let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
            continue;
        };
        if value["key"] == key && value["reuse_identity_hex"] == identity {
            return Ok(path);
        }
    }
    Err("cold guidance did not retain its exact scoped cache entry".into())
}

#[test]
fn complete_guidance_reuses_memory_and_disk_without_index_or_evidence_work() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let config = RiprConfig::default();
    let canonical = fixture.canonical(FILE, &config)?;
    let (cold, builds, evaluated) = fixture.run(FILE, &config)?;
    assert!(
        builds > 0 && evaluated > 0,
        "first guidance must perform real work"
    );
    assert!(!cold.classified.is_empty());
    assert_eq!(projection(&cold), projection(&canonical));
    assert_eq!(cold.unevaluated_seams, 0);
    assert!(
        cold.repo_seam_limit.is_none(),
        "do not enable the MCP limit walk"
    );
    assert!(guidance(&cold).is_some_and(|text| text.contains("weak_at_boundary")));
    let original_digest = cold.consumed_sources.digest(Path::new("tests/weak.rs"));
    assert!(
        original_digest.is_some(),
        "full test evidence must be consumed"
    );
    let (memory, builds, evaluated) = fixture.run(FILE, &config)?;
    assert_eq!(
        (builds, evaluated),
        (0, 0),
        "memory reuse must skip index and evidence work"
    );
    assert_eq!(projection(&memory), projection(&canonical));
    assert_eq!(
        memory.consumed_sources.digest(Path::new("tests/weak.rs")),
        original_digest
    );
    assert_eq!(
        memory.file_fact_cache.hits + memory.file_fact_cache.misses,
        0
    );
    entry_path(&fixture.0, &cold, FILE)?;
    forget(&cold, FILE);
    let (disk, builds, evaluated) = fixture.run(FILE, &config)?;
    assert_eq!(
        (builds, evaluated),
        (0, 0),
        "disk reuse must skip index and evidence work"
    );
    assert_eq!(projection(&disk), projection(&canonical));
    assert_eq!(
        disk.consumed_sources.digest(Path::new("tests/weak.rs")),
        original_digest
    );
    assert_eq!(disk.file_fact_cache.hits + disk.file_fact_cache.misses, 0);
    Ok(())
}

#[test]
fn complete_guidance_test_edits_additions_and_removals_invalidate_advice() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let config = RiprConfig::default();
    let (weak, _, _) = fixture.run(FILE, &config)?;
    assert!(guidance(&weak).is_some());
    fixture.write("tests/weak.rs", STRONG)?;
    let (strong, builds, evaluated) = fixture.run(FILE, &config)?;
    assert!(builds > 0 && evaluated > 0);
    assert!(guidance(&strong).is_none());
    assert_ne!(strong.workspace_cache_key, weak.workspace_cache_key);
    assert_eq!(
        projection(&strong),
        projection(&fixture.canonical(FILE, &config)?)
    );
    fixture.write("tests/weak.rs", WEAK)?;
    let (restored, _, _) = fixture.run(FILE, &config)?;
    assert_eq!(projection(&restored), projection(&weak));
    fixture.write("tests/strong.rs", STRONG)?;
    let (second_observer, builds, evaluated) = fixture.run(FILE, &config)?;
    assert!(builds > 0 && evaluated > 0);
    assert!(
        guidance(&second_observer).is_none(),
        "do not omit a stronger second observer"
    );
    assert_eq!(
        projection(&second_observer),
        projection(&fixture.canonical(FILE, &config)?)
    );
    std::fs::remove_file(fixture.0.join("tests/strong.rs")).map_err(|error| error.to_string())?;
    let (only_weak, _, _) = fixture.run(FILE, &config)?;
    assert_eq!(projection(&only_weak), projection(&weak));
    std::fs::remove_file(fixture.0.join("tests/weak.rs")).map_err(|error| error.to_string())?;
    let (no_tests, builds, evaluated) = fixture.run(FILE, &config)?;
    assert!(builds > 0 && evaluated > 0);
    assert!(guidance(&no_tests).is_none());
    assert_eq!(
        projection(&no_tests),
        projection(&fixture.canonical(FILE, &config)?)
    );
    assert_eq!(
        std::fs::read_to_string(fixture.0.join(FILE)).map_err(|error| error.to_string())?,
        SOURCE
    );
    Ok(())
}

#[test]
fn complete_guidance_keys_bind_root_selection_source_and_configuration() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let other_root = Fixture::new()?;
    let config = RiprConfig::default();
    let (first, _, _) = fixture.run(FILE, &config)?;
    let (other, builds, evaluated) = fixture.run("src/other.rs", &config)?;
    assert!(builds > 0 && evaluated > 0);
    assert_ne!(projection(&other), projection(&first));
    assert_eq!(
        projection(&other),
        projection(&fixture.canonical("src/other.rs", &config)?)
    );
    let (elsewhere, builds, evaluated) = other_root.run(FILE, &config)?;
    assert!(builds > 0 && evaluated > 0);
    assert_ne!(elsewhere.workspace_cache_key, first.workspace_cache_key);
    let modified = std::fs::metadata(fixture.0.join(FILE))
        .and_then(|meta| meta.modified())
        .map_err(|error| error.to_string())?;
    let edited = SOURCE.replace(" > ", ">= ");
    assert_eq!(
        edited.len(),
        SOURCE.len(),
        "control preserves source length"
    );
    fixture.write(FILE, &edited)?;
    std::fs::File::options()
        .write(true)
        .open(fixture.0.join(FILE))
        .and_then(|file| file.set_modified(modified))
        .map_err(|error| error.to_string())?;
    let (changed, builds, evaluated) = fixture.run(FILE, &config)?;
    assert!(
        builds > 0 && evaluated > 0,
        "content must win over restored mtime"
    );
    assert_ne!(changed.workspace_cache_key, first.workspace_cache_key);
    assert_eq!(
        projection(&changed),
        projection(&fixture.canonical(FILE, &config)?)
    );
    let mut configured = config.clone();
    configured.oracles.broad_error_strength = crate::domain::OracleStrength::Medium;
    assert_eq!(
        configured.source_text, config.source_text,
        "only effective policy changes"
    );
    let (policy, builds, evaluated) = fixture.run(FILE, &configured)?;
    assert!(builds > 0 && evaluated > 0);
    assert_ne!(policy.workspace_cache_key, changed.workspace_cache_key);
    assert_eq!(
        projection(&policy),
        projection(&fixture.canonical(FILE, &configured)?)
    );
    fixture.write("Cargo.toml", "[package]\nname = \"guidance_inventory\"\nversion = \"0.2.0\"\nedition = \"2024\"\n[workspace]\n")?;
    let (manifest, builds, evaluated) = fixture.run(FILE, &configured)?;
    assert!(builds > 0 && evaluated > 0);
    assert_ne!(manifest.workspace_cache_key, policy.workspace_cache_key);
    fixture.write("rust-toolchain.toml", "[toolchain]\nchannel = \"1.95.0\"\n")?;
    let (toolchain, builds, evaluated) = fixture.run(FILE, &configured)?;
    assert!(builds > 0 && evaluated > 0);
    assert_ne!(toolchain.workspace_cache_key, manifest.workspace_cache_key);
    Ok(())
}

#[test]
fn complete_guidance_corrupt_missing_and_unwritable_cache_recomputes() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let config = RiprConfig::default();
    let (cold, _, _) = fixture.run(FILE, &config)?;
    let entry = entry_path(&fixture.0, &cold, FILE)?;
    forget(&cold, FILE);
    std::fs::write(&entry, b"{ broken json").map_err(|error| error.to_string())?;
    let (corrupt, builds, evaluated) = fixture.run(FILE, &config)?;
    assert!(builds > 0 && evaluated > 0);
    assert_eq!(projection(&corrupt), projection(&cold));
    forget(&corrupt, FILE);
    std::fs::remove_file(&entry).map_err(|error| error.to_string())?;
    let (missing, builds, evaluated) = fixture.run(FILE, &config)?;
    assert!(builds > 0 && evaluated > 0);
    assert_eq!(projection(&missing), projection(&cold));
    forget(&missing, FILE);
    let layer = fixture
        .0
        .join("target/ripr/cache/repo-scoped-classified-seams");
    std::fs::remove_dir_all(&layer).map_err(|error| error.to_string())?;
    std::fs::write(&layer, b"not a directory").map_err(|error| error.to_string())?;
    let (failed_store, builds, evaluated) = fixture.run(FILE, &config)?;
    assert!(builds > 0 && evaluated > 0);
    assert_eq!(projection(&failed_store), projection(&cold));
    forget(&failed_store, FILE);
    let (again, builds, evaluated) = fixture.run(FILE, &config)?;
    assert!(
        builds > 0 && evaluated > 0,
        "failed persistence cannot masquerade as a disk hit"
    );
    assert_eq!(projection(&again), projection(&cold));
    Ok(())
}

#[test]
fn complete_guidance_unverifiable_cargo_identity_never_reuses() -> Result<(), String> {
    let fixture = Fixture::new()?;
    // An oversized Cargo configuration is outside the existing identity bound.
    fixture.write(".cargo/config.toml", &" ".repeat(1024 * 1024 + 1))?;
    let config = RiprConfig::default();
    let (first, builds, evaluated) = fixture.run(FILE, &config)?;
    assert!(builds > 0 && evaluated > 0);
    assert!(!first.workspace_cache_key.is_reusable());
    let (repeat, builds, evaluated) = fixture.run(FILE, &config)?;
    assert!(
        builds > 0 && evaluated > 0,
        "an unknown key must never admit reuse"
    );
    assert_eq!(projection(&repeat), projection(&first));
    Ok(())
}

#[test]
fn complete_guidance_rejects_source_drift_on_memory_and_disk_hits() -> Result<(), String> {
    for persisted in [false, true] {
        let fixture = Fixture::new()?;
        let config = RiprConfig::default();
        let (cold, _, _) = fixture.run(FILE, &config)?;
        if persisted {
            forget(&cold, FILE);
        }
        let path = fixture.0.join("tests/weak.rs");
        between_passes::install(&fixture.0, move || {
            let _ = std::fs::write(&path, STRONG);
        });
        let result = fixture.run(FILE, &config);
        assert!(
            matches!(result, Err(ref error) if error.contains("changed during analysis")),
            "{result:?}"
        );
        let (fresh, _, _) = fixture.run(FILE, &config)?;
        assert!(
            guidance(&fresh).is_none(),
            "fresh strong evidence replaces old advice"
        );
        assert_eq!(
            projection(&fresh),
            projection(&fixture.canonical(FILE, &config)?)
        );
    }
    Ok(())
}

#[test]
fn complete_guidance_cancellation_at_a_reuse_hit_returns_no_result() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let config = RiprConfig::default();
    fixture.run(FILE, &config)?;
    let token = cancellation::AnalysisCancellationToken::new();
    let fired = std::rc::Rc::new(std::cell::Cell::new(false));
    let hook_fired = fired.clone();
    let cancel = token.clone();
    bound_inventory_reuse::on_next_hit(Box::new(move || {
        hook_fired.set(true);
        cancel.cancel(cancellation::AnalysisAbortKind::Cancelled);
    }));
    let result = cancellation::with_token(&token, || fixture.run(FILE, &config));
    assert!(
        fired.get(),
        "the control must cancel after the cache lookup hits"
    );
    assert_eq!(
        result.err().as_deref(),
        Some("analysis cancelled: Cancelled")
    );
    Ok(())
}
