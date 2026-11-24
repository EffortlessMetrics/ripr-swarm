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

/// Git reads these to find a repository other than the directory it runs
/// in. A caller's shell or a hook can set them, and then `git init` in a
/// scratch directory would act on the caller's repository. They are removed
/// from each child, never from the test process's own environment.
const GIT_REPOSITORY_ENV: [&str; 7] = [
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_COMMON_DIR",
    "GIT_PREFIX",
];

/// `command` without the Git variables that name another repository.
fn without_repository_env(mut command: Command) -> Command {
    for name in GIT_REPOSITORY_ENV {
        command.env_remove(name);
    }
    command
}

/// A temp directory outside any Git work tree: a `TMPDIR` under the checkout
/// (some runners set `TMPDIR=<repo>/target`) would make every "non-Git" root
/// here a subdirectory of the repository running the tests.
fn non_repository_temp_dir() -> PathBuf {
    let inside_repository = |dir: &Path| {
        without_repository_env(Command::new("git"))
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
    let status = without_repository_env(Command::new("git"))
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
    without_repository_env(Command::new(env!("CARGO_BIN_EXE_ripr")))
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
    // #15297 review: this repository has no commit, so a work tree cannot be
    // added to it; the route is a clone, bound to the repository's own path.
    assert!(out.contains("has no commits yet"), "{out}");
    assert!(out.contains("git clone"), "{out}");
    assert!(!out.contains("worktree add"), "{out}");
    assert!(out.contains(bound_name(&dir.0).as_str()), "{out}");
    assert!(!out.contains("run `git init`"), "{out}");
    let routes = pilot_json_routes(&dir.0)?;
    assert!(!routes.is_empty(), "no routes");
    for route in &routes {
        assert_eq!(route["route"], "needs_git_work_tree", "{route}");
        assert!(route["command"].is_null(), "{route}");
        let guidance = route["guidance"].as_str().unwrap_or_default();
        assert!(guidance.contains("has no commits yet"), "{route}");
        assert!(guidance.contains("git clone"), "{route}");
    }
    Ok(())
}

/// The scratch directory's file name, which the bound route must name.
fn bound_name(dir: &Path) -> String {
    dir.file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// The directory's resolved path in the spelling a native process takes as
/// an argument: `scratch` spells its roots drive-relative on some hosts, and
/// Windows prefixes a resolved path with `\\?\`.
fn native_absolute(dir: &Path) -> Result<PathBuf, String> {
    let resolved = dir
        .canonicalize()
        .map_err(|error| format!("resolve {}: {error}", dir.display()))?;
    let text = resolved.to_string_lossy();
    let plain = match text.strip_prefix(r"\\?\") {
        Some(rest) => match rest.strip_prefix("UNC\\") {
            Some(share) => format!("\\\\{share}"),
            None => rest.to_owned(),
        },
        None => text.into_owned(),
    };
    Ok(PathBuf::from(plain))
}

/// Split a printed command into words, honouring double quotes, so the
/// suggested route can be run exactly as printed.
fn command_words(command: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    for ch in command.chars() {
        match ch {
            '"' => quoted = !quoted,
            ch if ch.is_whitespace() && !quoted => {
                if !current.is_empty() {
                    words.push(std::mem::take(&mut current));
                }
            }
            ch => current.push(ch),
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

/// #15297 review: the route for a bare repository names the repository's own
/// path, so it works from a directory that is not that repository, and the
/// printed command really creates the work tree where it says. The caller's
/// directory is an unrelated repository, which a bare `git worktree add`
/// would have acted on.
#[test]
fn bare_repository_route_is_bound_to_the_repository_and_runs_from_an_unrelated_directory()
-> Result<(), String> {
    // A populated bare repository: one commit in a seed work tree, cloned bare.
    let seed = ts_project("bare-route-seed")?;
    git(&seed.0, &["init", "-q", "-b", "main"])?;
    git(&seed.0, &["config", "user.email", "ripr@example.invalid"])?;
    git(&seed.0, &["config", "user.name", "ripr tests"])?;
    git(&seed.0, &["add", "."])?;
    git(&seed.0, &["commit", "-q", "-m", "base"])?;
    let bare = scratch("bare-route-bare")?;
    // The scratch roots are siblings. A relative target names the directory
    // `scratch` made, which `git clone` accepts because it is empty.
    let bare_name = bare
        .0
        .file_name()
        .ok_or("the bare scratch directory has no name")?
        .to_string_lossy()
        .into_owned();
    git(
        &seed.0,
        &["clone", "-q", "--bare", ".", &format!("../{bare_name}")],
    )?;
    let bare_root = native_absolute(&bare.0)?;

    // The caller stands in another repository entirely.
    let unrelated = ts_project("bare-route-unrelated")?;
    git(&unrelated.0, &["init", "-q", "-b", "main"])?;
    git(
        &unrelated.0,
        &["config", "user.email", "ripr@example.invalid"],
    )?;
    git(&unrelated.0, &["config", "user.name", "ripr tests"])?;
    git(&unrelated.0, &["add", "."])?;
    git(&unrelated.0, &["commit", "-q", "-m", "unrelated"])?;

    let (work_tree, command) =
        run_printed_work_tree_route(&unrelated.0, &bare_root, "bare-route-work-tree")?;
    assert!(
        command.ends_with(" worktree add <path>"),
        "a repository whose `HEAD` resolves needs no branch named: {command}"
    );
    // The checkout is a work tree for ripr: no bare-repository route any more.
    let doctor = text(&ripr(&work_tree.0, &["doctor", "--root", "."])?);
    assert!(!doctor.contains("bare Git repository"), "{doctor}");

    // Pilot and init, run from the unrelated repository, offer the same route.
    let pilot = text(&ripr(
        &unrelated.0,
        &["pilot", "--root", &bare_root.display().to_string()],
    )?);
    assert!(pilot.contains("worktree add"), "{pilot}");
    assert!(pilot.contains(bound_name(&bare_root).as_str()), "{pilot}");
    let init = text(&ripr(
        &unrelated.0,
        &["init", "--root", &bare_root.display().to_string()],
    )?);
    assert!(init.contains("worktree add"), "{init}");
    assert!(init.contains(bound_name(&bare_root).as_str()), "{init}");
    assert!(
        !bare_root.join("ripr.toml").exists(),
        "init must write nothing into a bare repository"
    );

    // A bare repository created on one branch that received its commit on
    // another: not empty, and a work tree needs that branch named.
    let misdirected = scratch("bare-route-misdirected")?;
    git(&misdirected.0, &["init", "--bare", "-q", "-b", "main", "."])?;
    let misdirected_name = misdirected
        .0
        .file_name()
        .ok_or("the misdirected scratch directory has no name")?
        .to_string_lossy()
        .into_owned();
    git(
        &seed.0,
        &[
            "push",
            "-q",
            &format!("../{misdirected_name}"),
            "main:refs/heads/feature",
        ],
    )?;
    let misdirected_root = native_absolute(&misdirected.0)?;
    let (branch_work_tree, command) =
        run_printed_work_tree_route(&unrelated.0, &misdirected_root, "bare-route-branch")?;
    assert!(
        command.ends_with(" worktree add <path> feature"),
        "the route names the branch that has the commit: {command}"
    );
    // The work tree checks that branch out: its `HEAD` names it.
    let git_file = std::fs::read_to_string(branch_work_tree.0.join(".git"))
        .map_err(|error| format!("read the work tree's gitfile: {error}"))?;
    let git_dir = PathBuf::from(
        git_file
            .trim()
            .strip_prefix("gitdir:")
            .ok_or("not a gitfile")?
            .trim(),
    );
    let head = std::fs::read_to_string(git_dir.join("HEAD"))
        .map_err(|error| format!("read the work tree's HEAD: {error}"))?;
    assert_eq!(head.trim(), "ref: refs/heads/feature", "{command}");

    // A bare repository whose commit sits under a tag alone is not empty
    // either: the route names the tag, and the work tree checks it out
    // detached.
    let tagged = scratch("bare-route-tagged")?;
    git(&tagged.0, &["init", "--bare", "-q", "-b", "main", "."])?;
    let tagged_name = tagged
        .0
        .file_name()
        .ok_or("the tagged scratch directory has no name")?
        .to_string_lossy()
        .into_owned();
    git(
        &seed.0,
        &[
            "push",
            "-q",
            &format!("../{tagged_name}"),
            "main:refs/tags/v1",
        ],
    )?;
    let tagged_root = native_absolute(&tagged.0)?;
    let (_tagged_work_tree, command) =
        run_printed_work_tree_route(&unrelated.0, &tagged_root, "bare-route-tag")?;
    assert!(
        command.ends_with(" worktree add <path> v1"),
        "the route names the tag that holds the commit: {command}"
    );
    Ok(())
}

/// `git worktree list --porcelain` in `root`.
fn worktree_list(root: &Path) -> Result<String, String> {
    let output = without_repository_env(Command::new("git"))
        .current_dir(root)
        .args(["worktree", "list", "--porcelain"])
        .output()
        .map_err(|error| format!("git worktree list: {error}"))?;
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Doctor, run from `caller`, offers a work-tree route bound to the bare
/// repository `root`; run that command from `caller` exactly as printed, with
/// `<path>` replaced by the scratch directory `label`. Returns the new work
/// tree, as a guard that removes it when dropped, and the command. `caller`
/// must not gain a work tree of its own.
fn run_printed_work_tree_route(
    caller: &Path,
    root: &Path,
    label: &str,
) -> Result<(Scratch, String), String> {
    let doctor = text(&ripr(
        caller,
        &["doctor", "--root", &root.display().to_string()],
    )?);
    assert!(doctor.contains("bare Git repository"), "{doctor}");
    let command = doctor
        .split('`')
        .find(|span| span.starts_with("git --git-dir="))
        .ok_or_else(|| format!("no bound work-tree command in the doctor output:\n{doctor}"))?;
    let before = worktree_list(caller)?;
    let words = command_words(command);
    assert_eq!(words.first().map(String::as_str), Some("git"), "{command}");
    let git_dir = words[1]
        .strip_prefix("--git-dir=")
        .ok_or_else(|| format!("not a --git-dir command: {command}"))?;
    assert!(Path::new(git_dir).is_absolute(), "{command}");
    assert_eq!(
        Path::new(git_dir).canonicalize().ok(),
        root.canonicalize().ok(),
        "the command names the selected bare repository: {command}"
    );
    assert_eq!(&words[2..5], ["worktree", "add", "<path>"], "{command}");
    assert!(words.len() <= 6, "{command}");

    let mut work_tree = scratch(label)?;
    let work_tree_root = native_absolute(&work_tree.0)?;
    std::fs::remove_dir(&work_tree.0).map_err(|error| error.to_string())?;
    // The route creates the work tree where the guard will remove it.
    work_tree.0 = work_tree_root.clone();
    let mut words = words;
    words[4] = work_tree_root.display().to_string();
    let output = without_repository_env(Command::new(&words[0]))
        .current_dir(caller)
        .args(&words[1..])
        .output()
        .map_err(|error| format!("run the printed route: {error}"))?;
    assert!(
        output.status.success(),
        "the printed route did not run from an unrelated directory: {command}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        work_tree_root.join(".git").is_file() && work_tree_root.join("pricing.ts").is_file(),
        "the work tree was not created at <path>: {}",
        work_tree_root.display()
    );
    assert_eq!(
        worktree_list(caller)?,
        before,
        "the caller's repository gained a work tree"
    );
    Ok((work_tree, command.to_owned()))
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
    // #15297 review: this repository has no commit, so the route is a clone
    // and a first commit, bound to the repository's own path.
    assert!(doctor.contains("has no commits yet"), "{doctor}");
    assert!(doctor.contains("git clone"), "{doctor}");
    assert!(doctor.contains(bound_name(&dir.0).as_str()), "{doctor}");
    assert!(!doctor.contains("worktree add"), "{doctor}");
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
    assert!(out.contains("has no commits yet"), "{out}");
    assert!(out.contains("git clone"), "{out}");
    assert!(!out.contains("worktree add"), "{out}");
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
