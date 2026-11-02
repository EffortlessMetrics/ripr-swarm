use super::*;
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Result<Self, String> {
        let root = std::env::temp_dir().join(format!(
            "preflight-binding-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).map_err(|error| error.to_string())?;
        let fixture = Self(root);
        git(&fixture.0, &["init"])?;
        git(
            &fixture.0,
            &["config", "user.email", "proof@example.invalid"],
        )?;
        git(&fixture.0, &["config", "user.name", "Proof fixture"])?;
        fixture.write("source.rs", "fn answer() -> u8 { 1 }\n")?;
        git(&fixture.0, &["add", "."])?;
        fixture.commit()?;
        Ok(fixture)
    }
    fn write(&self, name: &str, contents: &str) -> Result<(), String> {
        std::fs::write(self.0.join(name), contents).map_err(|error| error.to_string())
    }
    fn commit(&self) -> Result<String, String> {
        git(
            &self.0,
            &["commit", "--no-gpg-sign", "--allow-empty", "-m", "fixture"],
        )?;
        git(&self.0, &["rev-parse", "HEAD"])
    }
    fn binding(&self, base: &str, head: &str, env: &str) -> Result<PreflightBinding, String> {
        capture_binding(&self.0, base, head, env, &digest(b"plan"))
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn changed_environment_values_are_hashed_without_rendering_them() {
    let mut before = Vec::new();
    let mut after = Vec::new();
    append_environment(
        &mut before,
        vec![("RUSTFLAGS".into(), "-C opt-level=1".into())],
    );
    append_environment(
        &mut after,
        vec![("RUSTFLAGS".into(), "-C opt-level=2".into())],
    );
    assert_ne!(digest(&before), digest(&after));
    assert!(!digest(&before).contains("RUSTFLAGS"));
    assert!(!digest(&before).contains("opt-level"));
}

#[test]
fn clean_submodule_worktrees_refuse_binding() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let root = fixture.0.to_str().ok_or("fixture root must be UTF-8")?;
    git(&fixture.0, &["clone", "--no-hardlinks", root, "sub"])?;
    git(&fixture.0, &["add", "sub"])?;
    let head = fixture.commit()?;
    assert!(git(&fixture.0, &["status", "--porcelain"])?.is_empty());
    let reason = fixture
        .binding(&head, &head, &digest(b"environment"))
        .err()
        .ok_or("submodule binding must refuse")?;
    assert!(reason.contains("submodule"));
    Ok(())
}

#[test]
fn equivalent_local_and_published_refs_reuse_without_retagging_execution() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let base = git(&fixture.0, &["rev-parse", "HEAD"])?;
    let executed = fixture.commit()?;
    let original = fixture.binding(&base, &executed, &digest(b"environment"))?;
    let published = fixture.commit()?;
    assert_ne!(executed, published);
    // A remote-tracking subject is simulated locally; no network publication.
    git(
        &fixture.0,
        &["update-ref", "refs/remotes/origin/candidate", &published],
    )?;
    let selected = git(&fixture.0, &["rev-parse", "refs/remotes/origin/candidate"])?;
    let current = fixture.binding(&base, &selected, &digest(b"environment"))?;
    assert_eq!(original.source_tree, current.source_tree);
    assert!(original.compatible_with(&current));
    assert_eq!(original.execution_head_sha, executed);
    assert_ne!(original.execution_head_sha, current.execution_head_sha);
    let mut blank = current.clone();
    blank.environment_digest.clear();
    assert!(!blank.compatible_with(&blank));
    assert!(!original.compatible_with(&fixture.binding(
        &base,
        &selected,
        &digest(b"changed environment")
    )?));
    assert!(!original.compatible_with(&fixture.binding(
        &executed,
        &selected,
        &digest(b"environment")
    )?));
    fixture.write("source.rs", "fn answer() -> u8 { 2 }\n")?;
    assert_eq!(
        fixture
            .binding(&base, &selected, &digest(b"environment"))
            .err()
            .as_deref(),
        Some("executing worktree has source changes")
    );
    git(&fixture.0, &["add", "."])?;
    assert_eq!(
        fixture
            .binding(&base, &selected, &digest(b"environment"))
            .err()
            .as_deref(),
        Some("executing worktree has source changes")
    );
    let changed = fixture.commit()?;
    assert!(!original.compatible_with(&fixture.binding(
        &base,
        &changed,
        &digest(b"environment")
    )?));
    assert_eq!(
        fixture
            .binding(&base, &selected, &digest(b"environment"))
            .err()
            .as_deref(),
        Some("selected head is not the executing HEAD")
    );
    Ok(())
}

#[test]
fn untracked_and_hidden_tracked_source_refuse_binding() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let head = git(&fixture.0, &["rev-parse", "HEAD"])?;
    fixture.write("extra.rs", "source input")?;
    assert_eq!(
        fixture
            .binding(&head, &head, &digest(b"environment"))
            .err()
            .as_deref(),
        Some("executing worktree has source changes")
    );
    std::fs::remove_file(fixture.0.join("extra.rs")).map_err(|error| error.to_string())?;
    for flag in ["--assume-unchanged", "--skip-worktree"] {
        git(&fixture.0, &["update-index", flag, "source.rs"])?;
        fixture.write("source.rs", "hidden edit")?;
        assert!(git(&fixture.0, &["status", "--porcelain"])?.is_empty());
        assert_eq!(
            fixture
                .binding(&head, &head, &digest(b"environment"))
                .err()
                .as_deref(),
            Some("preflight reuse cannot bind hidden index entries")
        );
        fixture.write("source.rs", "fn answer() -> u8 { 1 }\n")?;
        git(
            &fixture.0,
            &[
                "update-index",
                "--no-assume-unchanged",
                "--no-skip-worktree",
                "source.rs",
            ],
        )?;
    }
    Ok(())
}

#[test]
fn external_config_bytes_and_missing_state_change_the_digest() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let config = fixture.0.join("external-config.toml");
    let paths = BTreeSet::from([config.clone()]);
    let capture = || -> Result<String, String> {
        let mut bytes = Vec::new();
        append_configs(&mut bytes, &paths)?;
        Ok(digest(&bytes))
    };
    let absent = capture()?;
    std::fs::write(&config, "[build]\n").map_err(|error| error.to_string())?;
    let present = capture()?;
    assert_ne!(absent, present);
    std::fs::write(&config, "[build]\njobs=2\n").map_err(|error| error.to_string())?;
    assert_ne!(present, capture()?);
    Ok(())
}
