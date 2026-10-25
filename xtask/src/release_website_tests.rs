//! Execute the page's project test, separately from its development fixture replay.

use crate::run::{self, ByteCaptureBudget, TimedBytesOutput};
use std::cell::Cell;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

const PAGE: &str = include_str!("../../docs/site/index.html");
const FUNCTION: &str = include_str!("../../fixtures/boundary_gap/input/src/lib.rs");
const TESTS: &str = include_str!("../../fixtures/boundary_gap/input/tests/pricing.rs");
const MANIFEST: &str = include_str!("../../fixtures/boundary_gap/input/Cargo.toml");

fn code(source: &str, id: &str) -> Result<String, String> {
    let marker = format!("<code id=\"{id}\">");
    let (_, tail) = source
        .split_once(&marker)
        .ok_or_else(|| format!("missing example step {id}"))?;
    let (body, _) = tail
        .split_once("</code>")
        .ok_or_else(|| format!("unclosed example step {id}"))?;
    let body = body
        .trim()
        .replace("&gt;", ">")
        .replace("&lt;", "<")
        .replace("&amp;", "&");
    if body.is_empty() {
        return Err(format!("empty example step {id}"));
    }
    Ok(body)
}

fn attributes<'a>(source: &'a str, name: &str) -> Vec<&'a str> {
    source
        .split(&format!("{name}=\""))
        .skip(1)
        .filter_map(|tail| tail.split_once('"').map(|(value, _)| value))
        .collect()
}

#[test]
fn local_links_and_assets_resolve() -> Result<(), String> {
    let ids = attributes(PAGE, "id");
    let unique: BTreeSet<_> = ids.iter().copied().collect();
    assert_eq!(ids.len(), unique.len(), "duplicate HTML id");
    let site = Path::new(env!("CARGO_MANIFEST_DIR")).join("../docs/site");
    for value in attributes(PAGE, "href")
        .into_iter()
        .chain(attributes(PAGE, "src"))
    {
        if let Some(id) = value.strip_prefix('#') {
            assert!(unique.contains(id), "broken local link: {value}");
        } else if !value.contains("://") {
            assert!(site.join(value).is_file(), "missing local asset: {value}");
        }
    }
    Ok(())
}

#[test]
fn development_replay_and_excerpt_match_the_fixture() -> Result<(), String> {
    let command = code(PAGE, "fixture-command")?;
    let spec = include_str!("../../fixtures/boundary_gap/SPEC.md");
    assert!(spec.lines().any(|line| line == command));
    let expected = include_str!("../../fixtures/boundary_gap/expected/human.txt");
    let excerpt = code(PAGE, "fixture-output")?;
    assert!(excerpt.lines().count() >= 3);
    for line in excerpt.lines() {
        assert!(
            expected.lines().any(|value| value.trim() == line.trim()),
            "unknown fixture excerpt: {line}"
        );
    }
    for assertion in code(PAGE, "existing-tests")?.lines() {
        assert!(
            TESTS.contains(assertion.trim()),
            "unknown existing assertion"
        );
    }
    Ok(())
}

struct Project {
    root: PathBuf,
    cleanup_allowed: Cell<bool>,
}

impl Project {
    fn new(test: &str) -> Result<Self, String> {
        let root = crate::tests::temp_dir("website-boundary");
        fs::create_dir_all(root.join("src")).map_err(|e| e.to_string())?;
        fs::create_dir_all(root.join("tests")).map_err(|e| e.to_string())?;
        // The page's assertion is for the reader's own crate, not ripr's nested
        // fixture workspace. Explicitly model that standalone project boundary.
        fs::write(
            root.join("Cargo.toml"),
            format!("{MANIFEST}\n[workspace]\n"),
        )
        .map_err(|e| e.to_string())?;
        fs::write(root.join("src/lib.rs"), FUNCTION).map_err(|e| e.to_string())?;
        fs::write(root.join("tests/pricing.rs"), format!("{TESTS}\n{test}\n"))
            .map_err(|e| e.to_string())?;
        Ok(Self {
            root,
            cleanup_allowed: Cell::new(true),
        })
    }

    fn test(&self, name: &str) -> Result<TimedBytesOutput, String> {
        let target = self.root.join("target").to_string_lossy().into_owned();
        let result = run::capture_bytes_in_dir_with_env_budget(
            Path::new(env!("CARGO")),
            &["test".to_string(), name.to_string()],
            &self.root,
            (
                &[
                    ("CARGO_TARGET_DIR", &target),
                    ("CARGO_NET_OFFLINE", "true"),
                    ("CARGO_BUILD_JOBS", "1"),
                    ("CARGO_INCREMENTAL", "0"),
                    ("CARGO_PROFILE_TEST_DEBUG", "0"),
                ],
                &[
                    "CARGO_BUILD_TARGET",
                    "CARGO_ENCODED_RUSTFLAGS",
                    "RUSTFLAGS",
                    "RUSTC_WRAPPER",
                    "RUSTC_WORKSPACE_WRAPPER",
                ],
            ),
            ByteCaptureBudget {
                timeout: Duration::from_secs(60),
                stdout_bytes: 128 * 1024,
                stderr_bytes: 128 * 1024,
            },
            "release website project test",
        );
        if let Err(error) = &result {
            self.cleanup_allowed.set(false);
            eprintln!(
                "retain website test project {} after capture failure: {error}",
                self.root.display()
            );
        }
        result
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        if self.cleanup_allowed.get()
            && let Err(error) = fs::remove_dir_all(&self.root)
        {
            eprintln!(
                "retain website test project {}: {error}",
                self.root.display()
            );
        }
    }
}

fn named_test_passed(output: &TimedBytesOutput, name: &str) -> bool {
    let stdout = String::from_utf8_lossy(&output.stdout);
    !output.timed_out
        && output.status.is_some_and(|status| status.success())
        && stdout.contains("running 1 test")
        && stdout.contains(&format!("test {name} ... ok"))
}

#[test]
fn equality_assertion_rejects_old_behavior_and_zero_test_success() -> Result<(), String> {
    assert_eq!(
        code(PAGE, "boundary-test-command")?,
        "cargo test equality_gets_discount"
    );
    let project = Project::new(&code(PAGE, "boundary-test")?)?;
    let new = project.test("equality_gets_discount")?;
    assert!(
        named_test_passed(&new, "equality_gets_discount"),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&new.stdout),
        String::from_utf8_lossy(&new.stderr)
    );
    let zero = project.test("nonexistent_boundary_control")?;
    assert!(zero.status.is_some_and(|status| status.success()));
    assert!(String::from_utf8_lossy(&zero.stdout).contains("running 0 tests"));
    assert!(!named_test_passed(&zero, "nonexistent_boundary_control"));

    assert_eq!(FUNCTION.matches("amount >= discount_threshold").count(), 1);
    fs::write(
        project.root.join("src/lib.rs"),
        FUNCTION.replace("amount >=", "amount >"),
    )
    .map_err(|e| e.to_string())?;
    for name in [
        "below_threshold_has_no_discount",
        "far_above_threshold_discounts",
    ] {
        let old_existing = project.test(name)?;
        assert!(
            named_test_passed(&old_existing, name),
            "stdout={} stderr={}",
            String::from_utf8_lossy(&old_existing.stdout),
            String::from_utf8_lossy(&old_existing.stderr)
        );
    }
    let old = project.test("equality_gets_discount")?;
    assert!(old.status.is_some_and(|status| !status.success()));
    let stdout = String::from_utf8_lossy(&old.stdout);
    assert!(stdout.contains("equality_gets_discount ... FAILED"));
    assert!(stdout.contains("left: 100"));
    assert!(stdout.contains("right: 90"));
    Ok(())
}
