//! #14310: git stores symbolic links, so a clone can deliver `ripr.toml` as a
//! link to a file outside the repository. A TOML parse error quotes the
//! offending line, so loading such a link printed a line of that outside file.
//! Every route that loads the configuration must refuse the link before
//! reading it, and print neither the outside path nor its content. This
//! drives the real binary so each command's own output path is exercised.
#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

const SENTINEL: &str = "canary-outside-content-14310";
const REFUSAL: &str = "symbolic link that leads outside the repository";

struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn git(root: &Path, args: &[&str]) -> Result<(), String> {
    let status = Command::new("git")
        .current_dir(root)
        .args(args)
        .status()
        .map_err(|error| format!("spawn git {args:?}: {error}"))?;
    status
        .success()
        .then_some(())
        .ok_or_else(|| format!("git {args:?} failed"))
}

/// `<scratch>/repo` holds a Rust crate with one committed predicate edit on a
/// feature branch; `<scratch>/outside-zq/private-zq.txt` is malformed TOML
/// carrying the sentinel.
fn project(label: &str) -> Result<(Scratch, PathBuf, PathBuf), String> {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let base = std::env::temp_dir().join(format!(
        "ripr-config-link-{label}-{}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let scratch = Scratch(base.clone());
    let root = base.join("repo");
    let outside = base.join("outside-zq");
    std::fs::create_dir_all(root.join("src")).map_err(|error| error.to_string())?;
    std::fs::create_dir_all(&outside).map_err(|error| error.to_string())?;
    let private = outside.join("private-zq.txt");
    std::fs::write(&private, format!("TOKEN {SENTINEL}\nsecond line\n"))
        .map_err(|error| error.to_string())?;

    git(&root, &["init", "-q", "-b", "main"])?;
    git(&root, &["config", "user.email", "ripr@example.invalid"])?;
    git(&root, &["config", "user.name", "ripr tests"])?;
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"p\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .map_err(|error| error.to_string())?;
    std::fs::write(root.join(".gitignore"), "/target/\n").map_err(|error| error.to_string())?;
    std::fs::write(
        root.join("src/lib.rs"),
        "pub fn f(x: i32) -> i32 { x + 1 }\n",
    )
    .map_err(|error| error.to_string())?;
    git(&root, &["add", "-A"])?;
    git(&root, &["commit", "-q", "-m", "base"])?;
    git(&root, &["checkout", "-q", "-b", "feat"])?;
    std::fs::write(
        root.join("src/lib.rs"),
        "pub fn f(x: i32) -> i32 { if x > 3 { x + 2 } else { 0 } }\n",
    )
    .map_err(|error| error.to_string())?;
    git(&root, &["commit", "-q", "-am", "change"])?;
    Ok((scratch, root, private))
}

fn ripr(root: &Path, args: &[&str]) -> Result<Output, String> {
    Command::new(env!("CARGO_BIN_EXE_ripr"))
        .current_dir(root)
        .args(args)
        .output()
        .map_err(|error| format!("spawn ripr {args:?}: {error}"))
}

fn printed(output: &Output) -> String {
    format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

const ROUTES: [&[&str]; 5] = [
    &["check", "--base", "main"],
    &["check", "--base", "main", "--json"],
    &["diff", "--base", "main"],
    &["diff", "--base", "main", "--json"],
    &["pilot", "--root", "."],
];

#[test]
fn a_config_link_to_an_outside_file_is_refused_on_every_route() -> Result<(), String> {
    let (_scratch, root, private) = project("escape")?;

    // Control: the same bytes as a regular `ripr.toml` are quoted back, so
    // the fixture is a real stimulus for the disclosure.
    std::fs::copy(&private, root.join("ripr.toml")).map_err(|error| error.to_string())?;
    let control = ripr(&root, ROUTES[0])?;
    assert!(
        printed(&control).contains(SENTINEL),
        "control did not quote the config line: {}",
        printed(&control)
    );
    std::fs::remove_file(root.join("ripr.toml")).map_err(|error| error.to_string())?;

    std::os::unix::fs::symlink(&private, root.join("ripr.toml"))
        .map_err(|error| format!("symlink: {error}"))?;
    for args in ROUTES {
        let output = ripr(&root, args)?;
        let text = printed(&output);
        assert_eq!(output.status.code(), Some(2), "{args:?}: {text}");
        for leaked in [SENTINEL, "second line", "private-zq", "outside-zq"] {
            assert!(!text.contains(leaked), "{args:?}: `{leaked}` in {text}");
        }
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(REFUSAL),
            "{args:?}: {text}"
        );
    }
    Ok(())
}

#[test]
fn a_config_link_inside_the_repository_still_loads_on_every_route() -> Result<(), String> {
    let (_scratch, root, _private) = project("inside")?;
    std::fs::create_dir_all(root.join("shared")).map_err(|error| error.to_string())?;
    std::fs::write(
        root.join("shared/ripr.shared.toml"),
        "[analysis]\nmode = \"fast\"\n",
    )
    .map_err(|error| error.to_string())?;
    std::os::unix::fs::symlink("shared/ripr.shared.toml", root.join("ripr.toml"))
        .map_err(|error| format!("symlink: {error}"))?;
    for args in ROUTES {
        let output = ripr(&root, args)?;
        let text = printed(&output);
        assert_eq!(output.status.code(), Some(0), "{args:?}: {text}");
        assert!(!text.contains(REFUSAL), "{args:?}: {text}");
    }
    Ok(())
}
