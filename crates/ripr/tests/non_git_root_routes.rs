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

/// Every line of `text` that names a runnable `ripr check --root` command,
/// wherever on the line it appears (a bare command, `Next: ...`, a Markdown
/// `Route:` bullet, or backticked prose).
fn check_commands(text: &str) -> Vec<&str> {
    text.lines()
        .map(str::trim)
        .filter(|line| line.contains("ripr check --root"))
        .collect()
}

/// `routes` holds only commands that cannot run into the refusal: each one
/// reads a saved patch.
fn assert_no_refused_check(routes: &[&str], out: &str) {
    assert!(
        routes.iter().all(|line| line.contains("--diff")),
        "pilot must not print a check command that refuses here: {routes:?}\n{out}"
    );
}

/// The pilot JSON language routes, parsed.
fn pilot_json_routes(root: &Path) -> Result<Vec<serde_json::Value>, String> {
    let output = ripr(root, &["pilot", "--root", ".", "--format", "json"])?;
    let json: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("pilot JSON: {error}\n{}", text(&output)))?;
    json["language_routes"]["routes"]
        .as_array()
        .cloned()
        .ok_or_else(|| format!("no language_routes.routes: {json}"))
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
    assert_no_refused_check(&check_commands(&out), &out);
    assert!(out.contains(" init`"), "{out}");
    assert!(out.contains("--diff PATH"), "{out}");
    // TypeScript is detected but not enabled by default: the screen must not
    // say enabling it is how to check it, since check still refuses here.
    assert!(!out.contains("to check it"), "{out}");
    // A saved patch still reads only enabled languages, so the disabled
    // state stays disclosed beside the blocker.
    assert!(
        out.contains("not enabled in ripr.toml [languages]"),
        "{out}"
    );
    // `git init` is bound to the pilot root, not the caller's directory.
    assert!(out.contains("git -C"), "{out}");

    // The written Markdown summary carries the same route.
    let markdown = std::fs::read_to_string(dir.0.join("target/ripr/pilot/pilot-summary.md"))
        .map_err(|error| format!("pilot-summary.md: {error}"))?;
    assert_no_refused_check(&check_commands(&markdown), &markdown);
    assert!(!markdown.contains("diff-first"), "{markdown}");

    // The JSON contract documented in OUTPUT_SCHEMA.md.
    let routes = pilot_json_routes(&dir.0)?;
    assert!(!routes.is_empty(), "no routes");
    for route in &routes {
        assert_eq!(route["route"], "needs_git_work_tree", "{route}");
        assert!(route["command"].is_null(), "{route}");
        let guidance = route["guidance"].as_str().unwrap_or_default();
        assert!(guidance.contains(" init`"), "{route}");
    }
    Ok(())
}

#[test]
fn pilot_in_a_bare_repository_names_a_work_tree() -> Result<(), String> {
    let dir = ts_project("pilot-bare")?;
    git(&dir.0, &["init", "--bare", "-q", "."])?;
    let out = text(&ripr(&dir.0, &["pilot", "--root", "."])?);
    assert_no_refused_check(&check_commands(&out), &out);
    assert!(out.contains("git worktree add"), "{out}");
    assert!(!out.contains("run `git init`"), "{out}");
    let routes = pilot_json_routes(&dir.0)?;
    assert!(!routes.is_empty(), "no routes");
    for route in &routes {
        assert_eq!(route["route"], "needs_git_work_tree", "{route}");
        assert!(route["command"].is_null(), "{route}");
    }
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
    let routes = pilot_json_routes(&dir.0)?;
    assert!(
        routes
            .iter()
            .any(|route| route["route"] == "check_diff_first" && route["command"].is_string()),
        "{routes:?}"
    );
    Ok(())
}

#[test]
fn bare_repository_is_named_by_doctor_and_refused_by_init() -> Result<(), String> {
    let dir = scratch("bare")?;
    git(&dir.0, &["init", "--bare", "-q", "."])?;

    let doctor = text(&ripr(&dir.0, &["doctor"])?);
    assert!(doctor.contains("bare Git repository"), "{doctor}");
    assert!(doctor.contains("git worktree add"), "{doctor}");
    // A bare repository has no source for pilot to read either, so doctor
    // names the work tree, not a whole-repository pilot.
    assert!(!doctor.contains("ripr pilot --root"), "{doctor}");

    let dry_run = ripr(&dir.0, &["init", "--dry-run"])?;
    let dry = text(&dry_run);
    assert_eq!(dry_run.status.code(), Some(2), "{dry}");
    assert!(dry.contains("bare Git repository"), "{dry}");

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
