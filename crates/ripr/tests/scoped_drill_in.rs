//! #9340: a scoped oversized-diff `ripr check` prints `--path`/`--exclude` on
//! its explain/context/list drill-ins, and those commands accept the flags.

#[path = "common/printed_words.rs"]
mod printed_words;

use printed_words::printed_words;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn unique_temp_workspace(label: &str) -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!(
        "ripr-{label}-{}-{}-{}",
        std::process::id(),
        stamp,
        TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    ))
}

fn run_ripr(args: &[&str], env: &[(&str, &str)]) -> Result<Output, String> {
    run_ripr_in(None, args, env)
}

fn run_ripr_in(cwd: Option<&Path>, args: &[&str], env: &[(&str, &str)]) -> Result<Output, String> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ripr"));
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    command.args(args).env("RIPR_PROGRESS", "1");
    for (name, value) in env {
        command.env(name, value);
    }
    command
        .output()
        .map_err(|error| format!("run ripr {args:?}: {error}"))
}

fn run_printed(command: &str, cwd: &Path, env: &[(&str, &str)]) -> Result<Output, String> {
    let mut words = printed_words(command)?;
    if words.first().map(String::as_str) != Some("ripr") {
        return Err(format!("printed command must start with ripr: {command}"));
    }
    words.remove(0);
    let args: Vec<&str> = words.iter().map(String::as_str).collect();
    run_ripr_in(Some(cwd), &args, env).map_err(|error| format!("{error} (printed `{command}`)"))
}

/// One real edit plus one file that alone trips the changed-line cap.
fn scoped_check_fixture(label: &str) -> Result<(PathBuf, PathBuf), String> {
    let dir = unique_temp_workspace(label);
    std::fs::create_dir_all(dir.join("src")).map_err(|error| format!("mkdir: {error}"))?;
    std::fs::write(
        dir.join("Cargo.toml"),
        "[package]\nname = \"scoped\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .map_err(|error| format!("write manifest: {error}"))?;
    std::fs::write(
        dir.join("src/lib.rs"),
        "pub fn gate(a: i32) -> bool { a >= 3 }\n",
    )
    .map_err(|error| format!("write lib: {error}"))?;
    let big: String = (0..12)
        .map(|index| format!("pub fn big_{index}() -> u32 {{ {index} }}\n"))
        .collect();
    std::fs::write(dir.join("src/big.rs"), &big).map_err(|error| format!("write big: {error}"))?;
    let mut diff = String::from(
        "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,1 +1,1 @@\n\
         -pub fn gate(a: i32) -> bool { a > 3 }\n+pub fn gate(a: i32) -> bool { a >= 3 }\n",
    );
    diff.push_str(
        "diff --git a/src/big.rs b/src/big.rs\n--- a/src/big.rs\n+++ b/src/big.rs\n@@ -0,0 +1,12 @@\n",
    );
    for line in big.lines() {
        diff.push('+');
        diff.push_str(line);
        diff.push('\n');
    }
    let diff_path = dir.join("change.diff");
    std::fs::write(&diff_path, &diff).map_err(|error| format!("write diff: {error}"))?;
    Ok((dir, diff_path))
}

fn first_indented_command(text: &str, prefix: &str) -> Result<String, String> {
    text.lines()
        .find(|line| line.starts_with(prefix))
        .map(str::trim)
        .map(str::to_string)
        .ok_or_else(|| format!("missing `{prefix}` line in:\n{text}"))
}

fn command_in_backticks(text: &str, prefix: &str) -> Result<String, String> {
    text.split('`')
        .find(|part| part.starts_with(prefix))
        .map(str::to_string)
        .ok_or_else(|| format!("missing `{prefix}` command in:\n{text}"))
}

#[test]
fn scoped_check_prints_path_exclude_drill_ins_that_replay() -> Result<(), String> {
    let (dir, diff) = scoped_check_fixture("scoped-drill-in")?;
    let root = dir.display().to_string();
    let diff = diff.display().to_string();
    let cap = [("RIPR_MAX_DIFF_CHANGED_RUST_LINES", "5")];

    let refused = run_ripr(
        &["check", "--root", &root, "--diff", &diff, "--quiet"],
        &cap,
    )?;
    let refused_err = String::from_utf8_lossy(&refused.stderr);
    assert!(
        !refused.status.success() && refused_err.contains("diff_scope_oversized"),
        "unscoped check must trip the cap: {refused_err}"
    );

    let listed = run_ripr(
        &[
            "check",
            "--root",
            &root,
            "--diff",
            &diff,
            "--exclude",
            "src/big.rs",
            "--quiet",
        ],
        &cap,
    )?;
    assert!(
        listed.status.success(),
        "scoped check must succeed: {}",
        String::from_utf8_lossy(&listed.stderr)
    );
    let human = String::from_utf8_lossy(&listed.stdout).into_owned();
    // Human check with findings prints indented explain/context; the
    // `ripr check --json` listing is the selector-miss recovery, not a
    // per-finding line.
    let explain = first_indented_command(&human, "  ripr explain ")?;
    let context = first_indented_command(&human, "  ripr context ")?;
    for command in [&explain, &context] {
        assert!(
            command.contains("--exclude") && command.contains("src/big.rs"),
            "scoped drill-in must carry --exclude src/big.rs: {command}"
        );
    }

    let decoy = unique_temp_workspace("scoped-drill-in-decoy");
    std::fs::create_dir_all(&decoy).map_err(|error| format!("mkdir decoy: {error}"))?;

    let explained = run_printed(&explain, &decoy, &cap)?;
    let explained_err = String::from_utf8_lossy(&explained.stderr);
    assert!(
        explained.status.success(),
        "printed explain must replay the scoped run, not trip the cap: {explained_err}\ncommand: {explain}"
    );
    assert!(
        !explained_err.contains("diff_scope_oversized")
            && !explained_err.contains("unknown explain argument"),
        "printed explain must not look like an unknown flag or an oversized diff: {explained_err}"
    );

    let contexted = run_printed(&context, &decoy, &cap)?;
    let context_err = String::from_utf8_lossy(&contexted.stderr);
    assert!(
        contexted.status.success(),
        "printed context must replay the scoped run: {context_err}\ncommand: {context}"
    );
    assert!(
        !context_err.contains("unknown context argument"),
        "printed context must accept --exclude: {context_err}"
    );

    // Selector miss is the production print of the listing drill-in.
    let mut miss_words = printed_words(&explain)?;
    let Some(selector) = miss_words.last_mut() else {
        return Err(format!("printed explain had no selector: {explain}"));
    };
    *selector = "probe:does-not-exist:error_path:deadbeef".to_string();
    let miss_args: Vec<&str> = miss_words.iter().skip(1).map(String::as_str).collect();
    let missed = run_ripr_in(Some(&decoy), &miss_args, &cap)?;
    let missed_err = String::from_utf8_lossy(&missed.stderr);
    assert!(
        !missed.status.success() && missed_err.contains("no finding matched"),
        "unknown selector must miss, not trip the cap: {missed_err}"
    );
    let list = command_in_backticks(&missed_err, "ripr check ")?;
    assert!(
        list.contains("--exclude") && list.contains("src/big.rs"),
        "selector-miss listing must carry --exclude src/big.rs: {list}"
    );
    let listed_again = run_printed(&list, &decoy, &cap)?;
    assert!(
        listed_again.status.success(),
        "printed check listing must replay the scope: {}",
        String::from_utf8_lossy(&listed_again.stderr)
    );

    // Discriminating negative: drop `--exclude` from the printed argv and
    // the same explain hits the cap the scoped check avoided.
    let mut words = printed_words(&explain)?;
    let mut stripped = Vec::new();
    let mut index = 0usize;
    while index < words.len() {
        if words[index] == "--exclude" {
            index += 2;
            continue;
        }
        stripped.push(std::mem::take(&mut words[index]));
        index += 1;
    }
    assert!(
        stripped.len() < printed_words(&explain)?.len(),
        "the printed explain must have contained --exclude: {explain}"
    );
    let unscoped_args: Vec<&str> = stripped.iter().skip(1).map(String::as_str).collect();
    let unscoped = run_ripr_in(Some(&decoy), &unscoped_args, &cap)?;
    let unscoped_err = String::from_utf8_lossy(&unscoped.stderr);
    assert!(
        !unscoped.status.success() && unscoped_err.contains("diff_scope_oversized"),
        "explain without --exclude must trip the cap: {unscoped_err}\nargs: {unscoped_args:?}"
    );

    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&decoy);
    Ok(())
}

#[test]
fn explain_and_context_reject_a_dotdot_path_like_check() -> Result<(), String> {
    let explain = run_ripr(
        &[
            "explain",
            "--path",
            "../escape",
            "probe:src_lib.rs:error_path:abcd",
        ],
        &[],
    )?;
    let explain_err = String::from_utf8_lossy(&explain.stderr);
    assert!(
        !explain.status.success()
            && explain_err.contains("--path pattern \"../escape\"")
            && explain_err.contains("`..` segment")
            && !explain_err.contains("unknown explain argument"),
        "explain must reuse check's pattern refusal: {explain_err}"
    );

    let context = run_ripr(
        &[
            "context",
            "--path",
            "../escape",
            "--at",
            "probe:src_lib.rs:error_path:abcd",
        ],
        &[],
    )?;
    let context_err = String::from_utf8_lossy(&context.stderr);
    assert!(
        !context.status.success()
            && context_err.contains("--path pattern \"../escape\"")
            && context_err.contains("`..` segment")
            && !context_err.contains("unknown context argument"),
        "context must reuse check's pattern refusal: {context_err}"
    );
    Ok(())
}
