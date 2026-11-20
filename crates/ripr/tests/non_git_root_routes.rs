//! #15297: a root that is not a Git work tree must not loop. `ripr check`
//! refuses there and points at `ripr pilot`; pilot must not point back at
//! `ripr check`. `init` and `doctor` name a bare repository as bare, and
//! `init` writes nothing into it. A real work tree keeps the `ripr check`
//! route (negative control).
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A temp directory outside any Git work tree: a `TMPDIR` under the checkout
/// (some runners set `TMPDIR=<repo>/target`) would make every "non-Git" root
/// here a subdirectory of the repository running the tests.
fn non_repository_temp_dir() -> PathBuf {
    let inside_repository = |dir: &Path| {
        Command::new("git")
            .current_dir(dir)
            .args(["rev-parse", "--is-inside-work-tree"])
            .output()
            .is_ok_and(|output| output.status.success())
    };
    let temp = std::env::temp_dir();
    if inside_repository(&temp) && Path::new("/tmp").is_dir() {
        PathBuf::from("/tmp")
    } else {
        temp
    }
}

fn scratch(label: &str) -> Result<Scratch, String> {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let root = non_repository_temp_dir().join(format!(
        "ripr-non-git-routes-{label}-{}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    Ok(Scratch(root))
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

fn ts_project(label: &str) -> Result<Scratch, String> {
    let dir = scratch(label)?;
    std::fs::write(
        dir.0.join("pricing.ts"),
        "export function price(n: number) { return n > 10 ? n * 2 : n; }\n",
    )
    .map_err(|error| error.to_string())?;
    Ok(dir)
}

fn ripr(root: &Path, args: &[&str]) -> Result<Output, String> {
    Command::new(env!("CARGO_BIN_EXE_ripr"))
        .current_dir(root)
        .args(args)
        .output()
        .map_err(|error| format!("spawn ripr {args:?}: {error}"))
}

fn text(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// Every line of `text` that is a runnable `ripr check` command or route.
fn check_commands(text: &str) -> Vec<&str> {
    text.lines()
        .map(str::trim)
        .filter(|line| line.starts_with("ripr check") || line.starts_with("route: ripr check"))
        .collect()
}

#[test]
fn pilot_in_a_non_git_root_never_routes_back_to_a_refused_check() -> Result<(), String> {
    let dir = ts_project("pilot-nogit")?;
    let check = ripr(&dir.0, &["check"])?;
    assert_eq!(check.status.code(), Some(2), "{}", text(&check));
    assert!(text(&check).contains("not inside a Git work tree"));

    let pilot = ripr(&dir.0, &["pilot", "--root", "."])?;
    let out = text(&pilot);
    assert!(out.contains("typescript"), "{out}");
    // The loop: any printed `ripr check --root ...` runs straight into the
    // refusal above.
    let routes = check_commands(&out);
    assert!(
        routes.iter().all(|line| line.contains("--diff")),
        "pilot must not print a check command that refuses here: {routes:?}\n{out}"
    );
    assert!(out.contains("git init"), "{out}");
    assert!(out.contains("--diff PATH"), "{out}");
    Ok(())
}

#[test]
fn pilot_in_a_work_tree_keeps_its_check_route() -> Result<(), String> {
    let dir = ts_project("pilot-git")?;
    std::fs::write(
        dir.0.join("ripr.toml"),
        "[languages]\nenabled = [\"typescript\"]\n",
    )
    .map_err(|error| error.to_string())?;
    git(&dir.0, &["init", "-q", "-b", "main"])?;
    git(&dir.0, &["config", "user.email", "ripr@example.invalid"])?;
    git(&dir.0, &["config", "user.name", "ripr tests"])?;
    git(&dir.0, &["add", "."])?;
    git(&dir.0, &["commit", "-q", "-m", "base"])?;
    let out = text(&ripr(&dir.0, &["pilot", "--root", "."])?);
    assert!(
        check_commands(&out)
            .iter()
            .any(|line| line.contains("ripr check --root")),
        "a real work tree keeps the check route: {out}"
    );
    assert!(!out.contains("needs a Git work tree"), "{out}");
    Ok(())
}

#[test]
fn bare_repository_is_named_by_doctor_and_refused_by_init() -> Result<(), String> {
    let dir = scratch("bare")?;
    git(&dir.0, &["init", "--bare", "-q", "."])?;

    let doctor = text(&ripr(&dir.0, &["doctor"])?);
    assert!(doctor.contains("bare Git repository"), "{doctor}");
    assert!(doctor.contains("git worktree add"), "{doctor}");

    let init = ripr(&dir.0, &["init"])?;
    let out = text(&init);
    assert_eq!(init.status.code(), Some(2), "{out}");
    assert!(out.contains("bare Git repository"), "{out}");
    assert!(out.contains("git worktree add"), "{out}");
    assert!(!out.contains("run `git init`"), "{out}");
    assert!(
        !dir.0.join("ripr.toml").exists(),
        "init must write nothing into a bare repository"
    );
    Ok(())
}

#[test]
fn init_in_a_work_tree_still_writes_its_config() -> Result<(), String> {
    let dir = scratch("init-git")?;
    git(&dir.0, &["init", "-q", "-b", "main"])?;
    let init = ripr(&dir.0, &["init"])?;
    assert!(init.status.success(), "{}", text(&init));
    assert!(dir.0.join("ripr.toml").exists());
    assert!(!text(&init).contains("bare Git repository"));
    Ok(())
}

#[test]
fn doctor_agrees_with_itself_about_a_detected_but_disabled_language() -> Result<(), String> {
    let dir = ts_project("doctor-nogit")?;
    let doctor = text(&ripr(&dir.0, &["doctor"])?);
    assert!(
        doctor.contains("typescript (preview) [not enabled]"),
        "{doctor}"
    );
    assert!(doctor.contains("detected but not enabled"), "{doctor}");
    // The first command is `ripr pilot`, which does not read the language
    // setting, so the enable step is not a prerequisite for it.
    assert!(doctor.contains("ripr pilot"), "{doctor}");
    assert!(!doctor.contains("Before that: enable"), "{doctor}");
    Ok(())
}
