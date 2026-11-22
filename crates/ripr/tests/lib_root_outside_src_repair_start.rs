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
/// stdout for diagnostics.
fn check_start(root: &Path) -> Result<(Option<String>, String), String> {
    let check = ripr(root, &["check", "--base", "main"])?;
    let out = text(&check.stdout);
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
