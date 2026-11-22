//! #16045: a library root outside `src/`, declared by `[lib] path` or reached
//! through a symlinked `src` or crate root, gets the repair start the default
//! `src/lib.rs` layout gets. `ripr check` offers a start only for a spot the
//! repo spot inventory lists, and `ripr repair <id>` resolves the id there,
//! so the inventory must list the spots of such a root. Real-binary tests in
//! temporary Git repositories, each with a default-layout control and
//! discriminating negatives.

use std::path::{Path, PathBuf};
use std::process::Output;

struct Repo {
    root: PathBuf,
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

const OLD: &str = "pub fn discount(amount: u32) -> u32 {\n    if amount > 100 { amount - 10 } else { amount }\n}\n";
const NEW: &str = "pub fn discount(amount: u32) -> u32 {\n    if amount >= 100 { amount - 10 } else { amount }\n}\n";
const PACKAGE: &str = "[package]\nname = \"my-pkg\"\nversion = \"0.1.0\"\nedition = \"2021\"\n";

/// A test that reaches the owner far from the boundary: `weakly_exposed`.
fn weak_test(call: &str) -> String {
    format!("#[test]\nfn far_above() {{\n    assert_eq!({call}(200), 190);\n}}\n")
}

/// One entry of a generated repository.
enum Entry {
    File(&'static str, String),
    #[cfg(unix)]
    Link(&'static str, &'static str),
}

fn file(path: &'static str, text: impl Into<String>) -> Entry {
    Entry::File(path, text.into())
}

fn git(root: &Path, args: &[&str]) -> Result<String, String> {
    let output = std::process::Command::new("git")
        .current_dir(root)
        .args([
            "-c",
            "user.name=RIPR test",
            "-c",
            "user.email=ripr@example.invalid",
        ])
        .args(args)
        .output()
        .map_err(|error| format!("spawn git {args:?}: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn ripr(root: &Path, args: &[&str]) -> Result<Output, String> {
    std::process::Command::new(env!("CARGO_BIN_EXE_ripr"))
        .current_dir(root)
        .args(args)
        .output()
        .map_err(|error| format!("spawn ripr {args:?}: {error}"))
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// A repository whose `main` holds `entries` with `OLD` in `changed`, and
/// whose `feature` branch changes `changed` to `NEW`.
fn repo(label: &str, entries: &[Entry], changed: &str) -> Result<Repo, String> {
    let root = std::env::temp_dir().join(format!(
        "ripr-lib-root-repair-start-{label}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let repo = Repo { root: root.clone() };
    let parent = |path: &Path| -> Result<(), String> {
        let dir = path
            .parent()
            .ok_or_else(|| format!("no parent: {}", path.display()))?;
        std::fs::create_dir_all(dir).map_err(|e| format!("dir {dir:?}: {e}"))
    };
    let write = |path: &Path, text: &str| -> Result<(), String> {
        parent(path)?;
        std::fs::write(path, text).map_err(|e| format!("write {path:?}: {e}"))
    };
    write(&root.join(".gitignore"), "/target/\n")?;
    write(&root.join(changed), OLD)?;
    for entry in entries {
        match entry {
            Entry::File(path, text) => write(&root.join(path), text)?,
            #[cfg(unix)]
            Entry::Link(link, target) => {
                let link = root.join(link);
                parent(&link)?;
                std::os::unix::fs::symlink(target, &link)
                    .map_err(|e| format!("link {link:?}: {e}"))?;
            }
        }
    }
    git(&root, &["init", "-q"])?;
    git(&root, &["config", "core.autocrlf", "false"])?;
    git(&root, &["config", "core.symlinks", "true"])?;
    git(&root, &["branch", "-M", "main"])?;
    git(&root, &["add", "."])?;
    git(&root, &["commit", "-q", "-m", "base"])?;
    git(&root, &["checkout", "-q", "-b", "feature"])?;
    write(&root.join(changed), NEW)?;
    git(&root, &["add", "."])?;
    git(&root, &["commit", "-q", "-m", "change"])?;
    Ok(repo)
}

/// The id of the `ripr repair <id> --root ..` line check prints, with check's
/// stdout for diagnostics. A check that does not complete is an error, so a
/// negative control cannot pass on a failed run.
fn check_start(root: &Path) -> Result<(Option<String>, String), String> {
    let check = ripr(root, &["check", "--base", "main"])?;
    let out = text(&check.stdout);
    if check.status.code() != Some(0) {
        return Err(format!(
            "`ripr check` must complete (exit {:?}):\n{out}\n{}",
            check.status.code(),
            text(&check.stderr)
        ));
    }
    let id = out.lines().find_map(|line| {
        let mut words = line.split_whitespace();
        (words.next() == Some("ripr") && words.next() == Some("repair"))
            .then(|| words.next().map(str::to_string))
            .flatten()
    });
    Ok((id, out))
}

fn attempts(root: &Path) -> usize {
    std::fs::read_dir(root.join("target/ripr/repair-attempts"))
        .map(|dir| {
            dir.flatten()
                .filter(|entry| entry.path().join("attempt.json").is_file())
                .count()
        })
        .unwrap_or(0)
}

/// Check's weak finding sits on the changed line of `changed`, check offers a
/// start, and `ripr repair <id>` starts exactly one attempt for that spot.
fn assert_repair_starts(label: &str, entries: &[Entry], changed: &str) -> Result<(), String> {
    let repo = repo(label, entries, changed)?;
    let (id, out) = check_start(&repo.root)?;
    if !out.contains(&format!("{changed}:2")) || !out.contains("Static exposure: weak") {
        return Err(format!(
            "{label}: fixture must read weak on {changed}:2:\n{out}"
        ));
    }
    let id = id.ok_or_else(|| format!("{label}: check offered no repair start:\n{out}"))?;
    let repair = ripr(&repo.root, &["repair", &id])?;
    let (stdout, stderr) = (text(&repair.stdout), text(&repair.stderr));
    if repair.status.code() != Some(0) || attempts(&repo.root) != 1 {
        return Err(format!(
            "{label}: `ripr repair {id}` must start one attempt:\n{stdout}\n{stderr}"
        ));
    }
    Ok(())
}

/// Check offers no repair start for the change in `changed`.
fn assert_no_start(label: &str, entries: &[Entry], changed: &str) -> Result<(), String> {
    let repo = repo(label, entries, changed)?;
    let (id, out) = check_start(&repo.root)?;
    match id {
        Some(id) => Err(format!("{label}: unexpected repair start {id}:\n{out}")),
        None => Ok(()),
    }
}

#[test]
fn a_root_outside_src_gets_the_repair_start_its_src_layout_gets() -> Result<(), String> {
    // Control: the default layout.
    assert_repair_starts(
        "control",
        &[
            file("Cargo.toml", PACKAGE),
            file("src/lib.rs", "pub mod pricing;\n"),
            file("tests/d.rs", weak_test("my_pkg::pricing::discount")),
        ],
        "src/pricing.rs",
    )?;
    assert_repair_starts(
        "outside",
        &[
            file(
                "Cargo.toml",
                format!("{PACKAGE}\n[lib]\npath = \"source/entry.rs\"\n"),
            ),
            file("source/entry.rs", "pub mod pricing;\n"),
            file("tests/d.rs", weak_test("my_pkg::pricing::discount")),
        ],
        "source/pricing.rs",
    )
}

#[test]
fn an_undeclared_twin_of_the_library_gets_no_start() -> Result<(), String> {
    // `real/lib.rs` beside a default `src/lib.rs`: no manifest and no link
    // names it, so no target compiles the change and nothing may start.
    assert_no_start(
        "twin",
        &[
            file("Cargo.toml", PACKAGE),
            file("src/lib.rs", OLD),
            file("tests/d.rs", weak_test("my_pkg::discount")),
        ],
        "real/lib.rs",
    )
}

#[cfg(unix)]
#[test]
fn a_symlinked_src_directory_gets_the_repair_start() -> Result<(), String> {
    assert_repair_starts(
        "symdir",
        &[
            file("Cargo.toml", PACKAGE),
            file("real/lib.rs", "pub mod pricing;\n"),
            Entry::Link("src", "real"),
            file("tests/d.rs", weak_test("my_pkg::pricing::discount")),
        ],
        "real/pricing.rs",
    )
}

#[cfg(unix)]
#[test]
fn a_symlinked_crate_root_gets_the_repair_start() -> Result<(), String> {
    assert_repair_starts(
        "symroot",
        &[
            file("Cargo.toml", PACKAGE),
            Entry::Link("src/lib.rs", "../real/lib.rs"),
            file("tests/d.rs", weak_test("my_pkg::discount")),
        ],
        "real/lib.rs",
    )
}

#[cfg(unix)]
#[test]
fn a_link_that_leaves_the_repository_gets_no_start() -> Result<(), String> {
    // The link target sits outside the analyzed repository, so nothing the
    // change touches is declared here.
    let outside = std::env::temp_dir().join(format!(
        "ripr-lib-root-repair-start-elsewhere-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&outside).map_err(|e| e.to_string())?;
    std::fs::write(outside.join("lib.rs"), OLD).map_err(|e| e.to_string())?;
    let target: &'static str = Box::leak(
        outside
            .join("lib.rs")
            .to_string_lossy()
            .into_owned()
            .into_boxed_str(),
    );
    let result = assert_no_start(
        "escape",
        &[
            file("Cargo.toml", PACKAGE),
            Entry::Link("src/lib.rs", target),
            file("tests/d.rs", weak_test("my_pkg::discount")),
        ],
        "real/lib.rs",
    );
    let _ = std::fs::remove_dir_all(&outside);
    result
}

#[cfg(unix)]
#[test]
fn a_linked_binary_cargo_does_not_build_lists_no_spots() -> Result<(), String> {
    // `autobins = false` with no `[[bin]]`: Cargo builds only the library,
    // so `src/main.rs -> ../old/main.rs` is no target and the repo inventory
    // lists no spot in its file (#16084 review). The library is the control.
    let repo = repo(
        "unbuilt-bin",
        &[
            file(
                "Cargo.toml",
                PACKAGE.replace("edition", "autobins = false\nedition"),
            ),
            file("src/lib.rs", OLD.replace("discount", "alpha")),
            Entry::Link("src/main.rs", "../old/main.rs"),
            file("tests/d.rs", weak_test("my_pkg::alpha")),
        ],
        "old/main.rs",
    )?;
    let cache = repo.root.join("target/test-cache");
    let (listed, _) = repo_exposure(&repo.root, &cache, &["src/lib.rs", "old/main.rs"])?;
    if listed != ["src/lib.rs"] {
        return Err(format!(
            "the inventory must list the library and not the unbuilt binary: {listed:?}"
        ));
    }
    Ok(())
}

/// Which of `files` a cached `repo-exposure-json` run lists, and whether the
/// run missed the classified-seam cache.
#[cfg(unix)]
fn repo_exposure(root: &Path, cache: &Path, files: &[&str]) -> Result<(Vec<String>, bool), String> {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_ripr"))
        .current_dir(root)
        .args(["check", "--root", ".", "--format", "repo-exposure-json"])
        .env("RIPR_CACHE_DIR", cache)
        .env("RIPR_PROGRESS", "0")
        .output()
        .map_err(|error| format!("spawn ripr: {error}"))?;
    let (out, err) = (text(&output.stdout), text(&output.stderr));
    if output.status.code() != Some(0) {
        return Err(format!("repo-exposure-json failed:\n{out}\n{err}"));
    }
    let mut files = files
        .iter()
        .filter(|file| out.contains(&format!("\"{file}\"")))
        .map(|file| file.to_string())
        .collect::<Vec<_>>();
    files.sort();
    Ok((
        files,
        err.contains("no usable cache entry for this workspace state"),
    ))
}

#[cfg(unix)]
#[test]
fn retargeting_a_crate_root_link_moves_the_repo_inventory_cache_key() -> Result<(), String> {
    // A link can move with no manifest or Rust file changing, and discovery
    // skips links, so the classified-seam cache must key on where each built
    // root lands (#16045 review).
    let repo = repo(
        "retarget",
        &[
            file("Cargo.toml", PACKAGE),
            file("real/a.rs", OLD.replace("discount", "alpha")),
            Entry::Link("src/lib.rs", "../real/a.rs"),
            file("tests/d.rs", weak_test("my_pkg::alpha")),
        ],
        "real/b.rs",
    )?;
    let cache = repo.root.join("target/test-cache");
    let first = repo_exposure(&repo.root, &cache, &["real/a.rs", "real/b.rs"])?;
    let second = repo_exposure(&repo.root, &cache, &["real/a.rs", "real/b.rs"])?;
    if first.0 != ["real/a.rs"] || !first.1 || second.0 != first.0 || second.1 {
        return Err(format!(
            "fixture: a cold run lists real/a.rs and a warm rerun reuses it: {first:?} {second:?}"
        ));
    }
    std::fs::remove_file(repo.root.join("src/lib.rs")).map_err(|e| e.to_string())?;
    std::os::unix::fs::symlink("../real/b.rs", repo.root.join("src/lib.rs"))
        .map_err(|e| e.to_string())?;
    let moved = repo_exposure(&repo.root, &cache, &["real/a.rs", "real/b.rs"])?;
    if moved != (vec!["real/b.rs".to_string()], true) {
        return Err(format!(
            "a retargeted root link must miss the cache and list real/b.rs: {moved:?}"
        ));
    }
    Ok(())
}

#[test]
fn pilot_ranks_a_root_outside_src_instead_of_calling_it_left_out() -> Result<(), String> {
    // #16084 review: pilot named a changed file below `[lib] path` as "a
    // crate source declared outside src, which pilot's repo-wide ranking
    // leaves out", though the repo inventory now ranks it. An exact boundary
    // test leaves the changed spot nothing to rank, which is when pilot
    // explains the change's files.
    let exact = "#[test]\nfn boundary() {\n    assert_eq!(my_pkg::pricing::discount(99), 99);\n    assert_eq!(my_pkg::pricing::discount(100), 90);\n    assert_eq!(my_pkg::pricing::discount(101), 91);\n}\n";
    let repo = repo(
        "pilot",
        &[
            file(
                "Cargo.toml",
                format!("{PACKAGE}\n[lib]\npath = \"source/entry.rs\"\n"),
            ),
            file("source/entry.rs", "pub mod pricing;\n"),
            file("tests/d.rs", exact),
        ],
        "source/pricing.rs",
    )?;
    let pilot = ripr(&repo.root, &["pilot", "--quiet"])?;
    let out = text(&pilot.stdout);
    // The change's checked spot is gripped, so pilot explains the change.
    if pilot.status.code() != Some(0) || !out.contains("on a line changed since main has no gap") {
        return Err(format!(
            "fixture: pilot must complete and explain the gripped change (exit {:?}):\n{out}\n{}",
            pilot.status.code(),
            text(&pilot.stderr)
        ));
    }
    if out.contains("declared outside src") || out.contains("ranking leaves out") {
        return Err(format!(
            "pilot must not call a ranked root outside src left out:\n{out}"
        ));
    }
    Ok(())
}
