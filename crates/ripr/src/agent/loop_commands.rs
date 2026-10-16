use std::path::{Component, Path, PathBuf};

/// How a builder renders its shell-redirect target from `(root, out_path)`.
type RedirectTarget = fn(&str, &str) -> String;

pub(crate) const AGENT_LOOP_COMMAND_TEMPLATE_VERSION: &str = "0.1";

pub(crate) const WORKFLOW_BEFORE_SNAPSHOT_ARTIFACT: &str =
    "target/ripr/workflow/before.repo-exposure.json";
pub(crate) const WORKFLOW_AFTER_SNAPSHOT_ARTIFACT: &str =
    "target/ripr/workflow/after.repo-exposure.json";
pub(crate) const WORKFLOW_ANALYSIS_OUTCOME_ARTIFACT: &str =
    "target/ripr/workflow/analysis-outcome.json";
pub(crate) const WORKFLOW_MANIFEST_ARTIFACT: &str = "target/ripr/workflow/workflow.json";
pub(crate) const WORKFLOW_COMMANDS_MARKDOWN_ARTIFACT: &str = "target/ripr/workflow/commands.md";
pub(crate) const WORKFLOW_AGENT_SEAM_PACKETS_ARTIFACT: &str =
    "target/ripr/workflow/agent-seam-packets.json";
pub(crate) const WORKFLOW_AGENT_PACKET_ARTIFACT: &str = "target/ripr/workflow/agent-packet.json";
pub(crate) const WORKFLOW_AGENT_BRIEF_ARTIFACT: &str = "target/ripr/workflow/agent-brief.json";
pub(crate) const WORKFLOW_AGENT_VERIFY_ARTIFACT: &str = "target/ripr/workflow/agent-verify.json";
pub(crate) const WORKFLOW_AGENT_RECEIPT_ARTIFACT: &str = "target/ripr/reports/agent-receipt.json";

pub(crate) const PILOT_BEFORE_SNAPSHOT_ARTIFACT: &str = "target/ripr/pilot/repo-exposure.json";
pub(crate) const PILOT_AFTER_SNAPSHOT_ARTIFACT: &str = "target/ripr/pilot/after.repo-exposure.json";
pub(crate) const EDITOR_AGENT_PACKET_ARTIFACT: &str = "target/ripr/agent/agent-packet.json";
pub(crate) const EDITOR_AGENT_BRIEF_ARTIFACT: &str = "target/ripr/agent/agent-brief.json";
pub(crate) const EDITOR_AGENT_VERIFY_ARTIFACT: &str = "target/ripr/agent/agent-verify.json";
pub(crate) const EDITOR_AGENT_RECEIPT_ARTIFACT: &str = "target/ripr/agent/agent-receipt.json";

pub(crate) const WORKFLOW_AGENT_STATUS_ARTIFACT: &str = "target/ripr/workflow/agent-status.json";
pub(crate) const WORKFLOW_AGENT_STATUS_MARKDOWN_ARTIFACT: &str =
    "target/ripr/workflow/agent-status.md";
pub(crate) const WORKFLOW_AGENT_REVIEW_SUMMARY_ARTIFACT: &str =
    "target/ripr/workflow/agent-review-summary.json";
pub(crate) const WORKFLOW_AGENT_REVIEW_SUMMARY_MARKDOWN_ARTIFACT: &str =
    "target/ripr/workflow/agent-review-summary.md";

pub(crate) fn agent_start_command(root: &str, seam_id: &str, out_dir: &str) -> String {
    format!(
        "ripr agent start --root {} --seam-id {} --out {}",
        shell_arg(&root_display(root)),
        shell_arg(seam_id),
        shell_arg(out_dir)
    )
}

/// Bind a producer's selected repository root for generated commands
/// (#3999).
///
/// Product-generated copy/paste commands must analyze the repository that was
/// selected when they were rendered, not the directory they are later pasted
/// into. A producer that renders copy/paste guidance calls this once, at the
/// boundary that selected the repository, and passes the result to the
/// builders below: an absolute root passes through; a relative root is
/// resolved exactly once against the producing process working directory —
/// the same directory the producer's own `--root` argument resolved against.
/// The result is one absolute, lexically cleaned path rendered with
/// [`root_path_display`] (stable separators on Windows, native characters on
/// Unix), and the redirect anchor ([`anchored_redirect_target`]) uses
/// the same rule, so the analyzed subject and the written artifact cannot
/// name different repositories. User-authored `--root .` keeps its ordinary
/// meaning.
pub(crate) fn bound_root(root: &str) -> String {
    root_path_display(&bound_root_path(Path::new(root)))
}

/// Render a selected root without rewriting its characters (#4287). On
/// Windows `\` is a separator and renders as `/`, like every other command
/// path. On Unix `\` is an ordinary filename character, so rewriting it would
/// name a different directory; the root renders exactly as the filesystem
/// spells it.
pub(crate) fn root_path_display(path: &Path) -> String {
    if cfg!(windows) {
        return display_path(path);
    }
    let text = path.to_string_lossy();
    if text.is_empty() {
        ".".to_string()
    } else {
        text.into_owned()
    }
}

/// [`bound_root`] as a path: the one resolution rule shared by redirect
/// anchoring and typed recovery (`agent::command_specs`).
pub(crate) fn bound_root_path(root_path: &Path) -> PathBuf {
    if root_path.is_absolute() {
        clean_bound_path(root_path)
    } else {
        let base = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        clean_bound_path(&base.join(root_path))
    }
}

/// Render a builder's `--root` value. An absolute root is a selected
/// repository a producer bound with [`bound_root`] and renders cleaned with
/// stable separators. A relative root is rendered verbatim: `.` is the
/// portable route an artifact-internal record carries (its typed
/// `CommandSpec` runs with the working directory at its own repository
/// root), and a builder never re-resolves it against the renderer's working
/// directory (#3999).
pub(crate) fn root_display(root: &str) -> String {
    let root_path = Path::new(root);
    if root_path.is_absolute() {
        // An absolute root binds to itself, lexically cleaned.
        bound_root(root)
    } else {
        root.to_string()
    }
}

/// Render a shell-redirect target rooted at the bound `--root` (issues
/// #3872, #3999). The root and already-absolute output keep native Unix spelling
/// and stable Windows separators; relative remainders use stable separators.
/// A pasted funnel command reproduces the validated write location from any
/// working directory under both shells and both .NET/provider resolution rules. An
/// already absolute target passes through (lexically cleaned); a relative target
/// joins [`bound_root`].
pub(crate) fn anchored_redirect_target(root: &str, out_path: &str) -> String {
    let out = Path::new(out_path);
    if out.is_absolute() {
        return root_path_display(&clean_bound_path(out));
    }
    // The root keeps its native characters (#4287); only the root-relative
    // remainder is rendered with stable separators.
    let root = bound_root_path(Path::new(root));
    let target = clean_bound_path(&root.join(out));
    match target.strip_prefix(&root) {
        Ok(rest) if rest.as_os_str().is_empty() => root_path_display(&root),
        Ok(rest) => format!(
            "{}/{}",
            root_path_display(&root).trim_end_matches('/'),
            display_path(rest)
        ),
        Err(_) => display_path(&target),
    }
}

/// Render a shell-redirect target for a portable command (#4000): one whose
/// relative `--root` deliberately means the reader's own checkout, such as a
/// review card published into a pull request. A relative root keeps the
/// target relative to that same root, so the command analyzes and writes
/// one repository wherever it is pasted. [`anchored_redirect_target`] would
/// instead resolve the target against the renderer's working directory (often
/// a CI runner), splitting analysis and write across two machines. An
/// absolute root or target is anchored exactly as there.
pub(crate) fn portable_redirect_target(root: &str, out_path: &str) -> String {
    let root_path = Path::new(root);
    if root_path.is_absolute() || Path::new(out_path).is_absolute() {
        return anchored_redirect_target(root, out_path);
    }
    root_path_display(&lexically_clean(&root_path.join(out_path)))
}

/// [`lexically_clean`] for a path a generated command will hand to the
/// operating system (#6960): the bound root and its redirect targets. On Unix
/// the kernel follows a symlink before it applies the next `..`, so
/// `link/../repo` names the sibling of the link's target, not of the link.
/// A `..` that follows an existing symlink is therefore kept, and every later
/// `..` with it, so the printed path resolves to the directory the producer
/// read. Only an existing path is inspected; a component that does not exist
/// yet (a redirect target) cannot be a symlink and cleans lexically. Windows
/// resolves `..` lexically before the filesystem sees the path, so there this
/// is exactly [`lexically_clean`].
pub(crate) fn clean_bound_path(path: &Path) -> PathBuf {
    let mut cleaned = PathBuf::new();
    let is_absolute = path.is_absolute();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => match cleaned.components().next_back() {
                Some(Component::Normal(_)) if !is_symlink(&cleaned) => {
                    cleaned.pop();
                }
                Some(Component::Normal(_) | Component::ParentDir) => {
                    cleaned.push(component.as_os_str());
                }
                _ if !is_absolute => cleaned.push(component.as_os_str()),
                _ => {}
            },
            Component::Normal(_) | Component::RootDir | Component::Prefix(_) => {
                cleaned.push(component.as_os_str());
            }
        }
    }
    if cleaned.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        cleaned
    }
}

/// Whether `path` is an existing symlink the kernel would follow before a
/// later `..` (Unix only; see [`clean_bound_path`]).
fn is_symlink(path: &Path) -> bool {
    cfg!(unix)
        && std::fs::symlink_metadata(path)
            .map(|metadata| metadata.file_type().is_symlink())
            .unwrap_or(false)
}

/// Drop `.` segments and resolve `..` lexically (no filesystem I/O: the
/// target usually does not exist yet when guidance is rendered). Keeps the
/// anchored target free of `/./` noise when `--root` is `.`. A leading
/// `..` on a relative path is preserved (there is nothing to pop); an
/// absolute path never carries one past the root.
pub(crate) fn lexically_clean(path: &Path) -> PathBuf {
    let mut cleaned = PathBuf::new();
    let is_absolute = path.is_absolute();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if matches!(cleaned.components().next_back(), Some(Component::Normal(_))) {
                    cleaned.pop();
                } else if !is_absolute {
                    cleaned.push(component.as_os_str());
                }
            }
            Component::Normal(_) | Component::RootDir | Component::Prefix(_) => {
                cleaned.push(component.as_os_str());
            }
        }
    }
    if cleaned.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        cleaned
    }
}

pub(crate) fn check_repo_exposure_command(root: &str, mode: &str, out_path: &str) -> String {
    check_repo_exposure_command_with_base(root, None, mode, out_path)
}

pub(crate) fn check_repo_exposure_command_with_base(
    root: &str,
    base: Option<&str>,
    mode: &str,
    out_path: &str,
) -> String {
    let base_arg = base
        .map(|base| format!(" --base {}", shell_arg(base)))
        .unwrap_or_default();
    format!(
        "ripr check --root {}{} --mode {} --format repo-exposure-json > {}",
        shell_arg(&root_display(root)),
        base_arg,
        shell_arg(mode),
        shell_arg(&anchored_redirect_target(root, out_path))
    )
}

pub(crate) fn agent_seam_packets_command(root: &str, mode: &str, out_path: &str) -> String {
    format!(
        "ripr check --root {} --mode {} --format agent-seam-packets-json > {}",
        shell_arg(&root_display(root)),
        shell_arg(mode),
        shell_arg(&anchored_redirect_target(root, out_path))
    )
}

pub(crate) fn check_analysis_outcome_command(root: &str, mode: &str, out_path: &str) -> String {
    check_analysis_outcome_command_with_base(root, None, mode, out_path)
}

pub(crate) fn check_analysis_outcome_command_with_base(
    root: &str,
    base: Option<&str>,
    mode: &str,
    out_path: &str,
) -> String {
    analysis_outcome_command(root, base, mode, out_path, anchored_redirect_target)
}

/// [`check_analysis_outcome_command_with_base`] for a portable root; see
/// [`portable_redirect_target`].
pub(crate) fn portable_check_analysis_outcome_command_with_base(
    root: &str,
    base: Option<&str>,
    mode: &str,
    out_path: &str,
) -> String {
    analysis_outcome_command(root, base, mode, out_path, portable_redirect_target)
}

fn analysis_outcome_command(
    root: &str,
    base: Option<&str>,
    mode: &str,
    out_path: &str,
    redirect_target: RedirectTarget,
) -> String {
    let base_arg = base
        .map(|base| format!(" --base {}", shell_arg(base)))
        .unwrap_or_default();
    format!(
        "ripr check --root {}{} --mode {} --format json > {}",
        shell_arg(&root_display(root)),
        base_arg,
        shell_arg(mode),
        shell_arg(&redirect_target(root, out_path))
    )
}

pub(crate) fn agent_packet_command(root: &str, seam_id: &str, out_path: &str) -> String {
    format!(
        "ripr agent packet --root {} --seam-id {} --json > {}",
        shell_arg(&root_display(root)),
        shell_arg(seam_id),
        shell_arg(&anchored_redirect_target(root, out_path))
    )
}

pub(crate) fn agent_brief_command(root: &str, seam_id: &str, out_path: &str) -> String {
    brief_command(root, seam_id, out_path, anchored_redirect_target)
}

/// [`agent_brief_command`] for a portable root; see
/// [`portable_redirect_target`].
pub(crate) fn portable_agent_brief_command(root: &str, seam_id: &str, out_path: &str) -> String {
    brief_command(root, seam_id, out_path, portable_redirect_target)
}

fn brief_command(
    root: &str,
    seam_id: &str,
    out_path: &str,
    redirect_target: RedirectTarget,
) -> String {
    format!(
        "ripr agent brief --root {} --seam-id {} --json > {}",
        shell_arg(&root_display(root)),
        shell_arg(seam_id),
        shell_arg(&redirect_target(root, out_path))
    )
}

pub(crate) fn agent_verify_command(
    root: &str,
    before_path: &str,
    after_path: &str,
    out_path: Option<&str>,
) -> String {
    let command = verify_command_head(root, before_path, after_path);
    append_redirect(root, command, out_path)
}

/// [`agent_verify_command`] for a portable root; see
/// [`portable_redirect_target`].
pub(crate) fn portable_agent_verify_command(
    root: &str,
    before_path: &str,
    after_path: &str,
    out_path: Option<&str>,
) -> String {
    let command = verify_command_head(root, before_path, after_path);
    match out_path {
        Some(path) => format!(
            "{command} > {}",
            shell_arg(&portable_redirect_target(root, path))
        ),
        None => command,
    }
}

fn verify_command_head(root: &str, before_path: &str, after_path: &str) -> String {
    format!(
        "ripr agent verify --root {} --before {} --after {} --json",
        shell_arg(&root_display(root)),
        shell_arg(before_path),
        shell_arg(after_path)
    )
}

pub(crate) fn agent_receipt_command(
    root: &str,
    verify_json: &str,
    seam_id: &str,
    out_path: Option<&str>,
) -> String {
    let command = format!(
        "ripr agent receipt --root {} --verify-json {} --seam-id {} --json",
        shell_arg(&root_display(root)),
        shell_arg(verify_json),
        shell_arg(seam_id)
    );
    match out_path {
        Some(path) => format!("{command} --out {}", shell_arg(path)),
        None => command,
    }
}

pub(crate) fn agent_status_command(root: &str, out_path: Option<&str>) -> String {
    append_redirect(
        root,
        format!(
            "ripr agent status --root {} --json",
            shell_arg(&root_display(root))
        ),
        out_path,
    )
}

pub(crate) fn agent_status_markdown_command(root: &str, out_path: Option<&str>) -> String {
    append_redirect(
        root,
        format!(
            "ripr agent status --root {}",
            shell_arg(&root_display(root))
        ),
        out_path,
    )
}

pub(crate) fn agent_review_summary_command(root: &str, out_path: Option<&str>) -> String {
    append_redirect(
        root,
        format!(
            "ripr agent review-summary --root {} --json",
            shell_arg(&root_display(root))
        ),
        out_path,
    )
}

pub(crate) fn agent_review_summary_markdown_command(root: &str, out_path: Option<&str>) -> String {
    append_redirect(
        root,
        format!(
            "ripr agent review-summary --root {}",
            shell_arg(&root_display(root))
        ),
        out_path,
    )
}

pub(crate) fn outcome_command(
    before_path: &str,
    after_path: &str,
    out_path: Option<&str>,
) -> String {
    match out_path {
        Some(path) => {
            format!(
                "ripr outcome --before {} --after {} --format json --out {}",
                shell_arg(before_path),
                shell_arg(after_path),
                shell_arg(path)
            )
        }
        None => format!(
            "ripr outcome --before {} --after {}",
            shell_arg(before_path),
            shell_arg(after_path)
        ),
    }
}

pub(crate) fn display_path(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    if text.is_empty() {
        ".".to_string()
    } else {
        text
    }
}

pub(crate) fn workflow_artifact_path(out_dir: &Path, file_name: &str) -> String {
    let out_dir = root_path_display(out_dir);
    if out_dir == "." {
        file_name.to_string()
    } else {
        format!("{}/{}", out_dir.trim_end_matches('/'), file_name)
    }
}

pub(crate) fn shell_path(path: &Path) -> String {
    shell_arg(&display_path(path))
}

/// Render `value` as one argv token for an advisory command string, quoted
/// for the shell family of the host ripr runs on (#7729).
///
/// These strings are published for a human or an agent to copy into a shell, so
/// every value must survive the copy as exactly one argument with exactly its
/// original bytes (#2347).
///
/// Values made only of unambiguous characters pass through unquoted on every
/// host. Everything else follows the host's quoting form:
///
/// - non-Windows: single-quoted, with an embedded `'` rendered as `'\''`
///   (close, escaped quote, reopen). Single quotes are the only bash quoting
///   form with no interior interpretation at all: `$`, backtick, `!`, `\`,
///   newline, and redirect characters are all literal inside them. That makes
///   one rule total over every possible value, which double quotes cannot be:
///   inside double quotes bash still expands `$VAR`, `$(cmd)`, and `` `cmd` ``;
///   `\!` inside double quotes is literally backslash-bang in a non-interactive
///   shell, so escaping `!` that way corrupts the value instead of protecting
///   it; and a bare `\` cannot be left unquoted, because bash consumes it as an
///   escape.
/// - Windows: double-quoted, with an embedded `"` doubled (`""`) and a
///   trailing backslash run doubled before the closing quote
///   (`windows_console_quote`). Double quotes are the only quoting form
///   honored by every Windows console shell: cmd.exe, PowerShell, and Git
///   Bash all keep a double-quoted spaced path as one argument. Values
///   carrying `$` or backtick keep the single-quote form instead: Git Bash
///   expands `$VAR`, `$(cmd)`, and `` `cmd` `` inside double quotes, and so
///   does PowerShell — pasting `"$(...)"` into either executes the embedded
///   expression (#8152 review, security), while single quotes stay literal
///   in both. cmd.exe passes single quotes through as literal characters
///   (the argument arrives quoted and fails closed), so those values paste
///   in Git Bash or PowerShell, not cmd.exe. PowerShell would escape an
///   interior quote with a backtick instead of doubling, and Git Bash drops
///   an interior quote in the `""` form; an interior `"` cannot occur in a
///   Windows filename, so the `""` doubling is the conservative choice for
///   the paths this form exists to quote.
///
/// The empty string must be quoted (`''` or `""`), or the argument disappears
/// from the command line entirely.
///
/// Values carrying control or bidi characters keep the explicit
/// `"$(printf '\ooo')"` escape form on both hosts: terminal safety owns that
/// policy, and the escape is bash source even when emitted on Windows (no
/// Windows console shell can rebuild those bytes), which
/// `output::markdown::powershell_command` already lifts and rebuilds.
pub(crate) fn shell_arg(value: &str) -> String {
    shell_arg_for(value, cfg!(windows))
}

/// Pure host-parameterised core of [`shell_arg`], so both hosts' quoting forms
/// are testable on any machine (the `human_path_text` pattern, #4378).
pub(crate) fn shell_arg_for(value: &str, windows: bool) -> String {
    if !value.is_empty()
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '/' | '_' | '-' | ':'))
    {
        return value.to_string();
    }
    if value.chars().any(needs_terminal_escape) {
        return ansi_c_quote(value);
    }
    if windows && !value.chars().any(|ch| matches!(ch, '$' | '`')) {
        return windows_console_quote(value);
    }
    format!("'{}'", value.replace('\'', r"'\''"))
}

/// Quote one argument in the Windows console form: double quotes, interior
/// `"` doubled (`""`), and a trailing backslash run doubled before the
/// closing quote. Verified against the native argv parser, cmd.exe, Git
/// Bash, and Windows PowerShell (#8152 review):
///
/// - Interior doubling: `"a""b"` parses as `a"b` in cmd (native argv), in
///   PowerShell, and — losing the quote — in Git Bash; `\"` would be the
///   argv/Git Bash form but closes the string early in PowerShell, so `""`
///   is the conservative choice. Interior `"` cannot occur in a Windows
///   filename, so this diverges only for exotic non-path arguments.
/// - Trailing separator: a root ending in a separator corrupts under the
///   native argv rules (`\"` escapes the closing quote, swallowing the rest
///   of the line), in Git Bash, and in PowerShell 5.1; doubling the run
///   before the closing quote round-trips in all four. A path ending in a
///   separator is routine, so this is encoder policy, not a documented
///   limitation.
///
/// Values containing `$` or backtick never reach this form: both PowerShell
/// and Git Bash expand them inside double quotes, so they keep the
/// single-quote form (`shell_arg_for`).
fn windows_console_quote(value: &str) -> String {
    let mut body = value.replace('"', "\"\"");
    let trailing_backslashes = value.chars().rev().take_while(|ch| *ch == '\\').count();
    body.push_str(&"\\".repeat(trailing_backslashes));
    format!("\"{body}\"")
}

/// Which characters must not reach a terminal raw: every control character
/// except `\n` and `\t`, plus the bidi formatting characters. One owner for the
/// policy; `output::human::terminal_safe` escapes the same set in reports. It
/// lives here, dependency-free, because `xtask` includes this file by path.
pub(crate) fn needs_terminal_escape(ch: char) -> bool {
    match ch {
        '\n' | '\t' => false,
        c if c.is_control() => true,
        // Arabic letter mark, LRM/RLM, embeddings/overrides (LRE..RLO), and
        // isolates (LRI..PDI): they reorder text without any visible glyph.
        '\u{61c}' | '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' => {
            true
        }
        _ => false,
    }
}

/// Control and bidi characters cannot be printed raw in a report, and the
/// report escape would change them inside `'...'` so the pasted command named a
/// different argument. Each such character becomes an adjacent
/// `"$(printf '\ooo')"` segment, one octal escape per UTF-8 byte; everything
/// else stays in `'...'` runs. Unlike `$'...'` this is POSIX, so `sh`, dash, bash
/// and zsh all rebuild the exact bytes. `output::markdown::powershell_command`
/// lifts each such argument and rebuilds it as a `[char]` expression.
fn ansi_c_quote(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 8);
    let mut in_run = false;
    for ch in value.chars() {
        if needs_terminal_escape(ch) {
            if in_run {
                out.push('\'');
                in_run = false;
            }
            out.push_str("\"$(printf '");
            let mut buf = [0u8; 4];
            for byte in ch.encode_utf8(&mut buf).bytes() {
                out.push_str(&format!("\\{byte:03o}"));
            }
            out.push_str("')\"");
            continue;
        }
        if !in_run {
            out.push('\'');
            in_run = true;
        }
        if ch == '\'' {
            out.push_str(r"'\''");
        } else {
            out.push(ch);
        }
    }
    if in_run {
        out.push('\'');
    }
    out
}

fn append_redirect(root: &str, command: String, out_path: Option<&str>) -> String {
    match out_path {
        Some(path) => format!(
            "{command} > {}",
            shell_arg(&anchored_redirect_target(root, path))
        ),
        None => command,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Hold a stable process CWD across one render-and-compare (#7034).
    ///
    /// `--root .` builders and `anchored_expectation` both call
    /// `std::env::current_dir()`. When this file is compiled into xtask,
    /// the parent supplies `acquire_test_cwd_read_guard()`. When compiled
    /// as ripr, it supplies `testing::cwd_lock::hold_cwd()` so these tests
    /// cannot race `set_current_dir` writers in the same process.
    fn hold_cwd_read_guard() -> impl Drop {
        super::super::loop_commands_cwd_read_guard()
    }

    /// #4000: a portable command's analyzed root and redirect target must
    /// name one repository. A relative root keeps a relative target under
    /// that root; the bound builders keep anchoring at the renderer's
    /// resolved root, and an absolute root anchors either way.
    #[test]
    fn portable_redirect_stays_under_the_typed_root() -> Result<(), String> {
        let _cwd_guard = hold_cwd_read_guard();
        let artifact = WORKFLOW_AGENT_BRIEF_ARTIFACT;
        for (root, expected) in [
            (".", artifact.to_string()),
            ("my repo", format!("my repo/{artifact}")),
            ("./sub/../my repo", format!("my repo/{artifact}")),
            // A leading `..` stays: the redirect resolves from the same
            // directory as `--root ../repo`, one level up.
            ("../repo", format!("../repo/{artifact}")),
        ] {
            let target = portable_redirect_target(root, artifact);
            if target != expected {
                return Err(format!("portable target for {root:?}: {target}"));
            }
            let command = portable_agent_brief_command(root, "seam", artifact);
            if !command.ends_with(&format!("> {}", shell_arg(&expected))) {
                return Err(format!("portable brief split root and write: {command}"));
            }
        }
        let bound = agent_brief_command(".", "seam", artifact);
        if !bound.ends_with(&format!(
            "> {}",
            shell_arg(&anchored_redirect_target(".", artifact))
        )) || !Path::new(&anchored_redirect_target(".", artifact)).is_absolute()
        {
            return Err(format!("bound brief must stay anchored: {bound}"));
        }
        let absolute = bound_root("my repo");
        if portable_redirect_target(&absolute, artifact)
            != anchored_redirect_target(&absolute, artifact)
        {
            return Err("an absolute root must anchor the same way".to_string());
        }
        let verify = portable_agent_verify_command(
            ".",
            WORKFLOW_BEFORE_SNAPSHOT_ARTIFACT,
            WORKFLOW_AFTER_SNAPSHOT_ARTIFACT,
            Some(WORKFLOW_AGENT_VERIFY_ARTIFACT),
        );
        let outcome = portable_check_analysis_outcome_command_with_base(
            ".",
            Some("main"),
            "draft",
            WORKFLOW_ANALYSIS_OUTCOME_ARTIFACT,
        );
        for (command, target) in [
            (verify, WORKFLOW_AGENT_VERIFY_ARTIFACT),
            (outcome, WORKFLOW_ANALYSIS_OUTCOME_ARTIFACT),
        ] {
            if !command.ends_with(&format!("> {target}")) {
                return Err(format!("portable command left the root: {command}"));
            }
        }
        Ok(())
    }

    #[test]
    fn analysis_outcome_preserves_selected_base_and_default_compatibility() -> Result<(), String> {
        let _cwd_guard = hold_cwd_read_guard();
        let out = "target/ripr/workflow/analysis-outcome.json";
        let selected = check_analysis_outcome_command_with_base(
            ".",
            Some("origin/trunk with space"),
            "draft",
            out,
        );
        let quoted_base = shell_arg("origin/trunk with space");
        if !selected.contains(&format!(" --base {quoted_base} --mode draft ")) {
            return Err(format!("selected base was not shell-quoted: {selected}"));
        }
        let expected = format!(
            "ripr check --root . --mode draft --format json > {}",
            anchored_expectation(out)
        );
        if check_analysis_outcome_command_with_base(".", None, "draft", out) != expected
            || check_analysis_outcome_command(".", "draft", out) != expected
        {
            return Err("default outcome command changed".to_owned());
        }
        Ok(())
    }

    /// Render the expected redirect target for a `--root .` tail: the anchor
    /// rule resolves through the renderer working directory, so expectations
    /// build the same absolute path dynamically instead of pinning a machine
    /// directory (issue #3872).
    fn anchored_expectation(tail: &str) -> String {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        shell_arg(&display_path(&cwd.join(tail)))
    }

    #[test]
    fn anchored_redirect_target_roots_relative_outputs_at_root() {
        let _cwd_guard = hold_cwd_read_guard();
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let base = display_path(&cwd);
        assert_eq!(
            anchored_redirect_target(".", "target/ripr/workflow/before.repo-exposure.json"),
            format!("{base}/target/ripr/workflow/before.repo-exposure.json")
        );
        assert_eq!(
            anchored_redirect_target("repo root", "target/ripr/work flow/before.json"),
            format!("{base}/repo root/target/ripr/work flow/before.json")
        );
        let absolute_root = display_path(&cwd.join("workspace-root"));
        assert_eq!(
            anchored_redirect_target(&absolute_root, "target/out.json"),
            format!("{absolute_root}/target/out.json")
        );
        let absolute_out = format!("{base}/elsewhere/out.json");
        assert_eq!(anchored_redirect_target(".", &absolute_out), absolute_out);
        assert!(
            !anchored_redirect_target(".", "target/out.json").contains("/./"),
            "anchored target must not carry a `/./` segment"
        );
        // Review #3938: a leading `..` on a relative path survives
        // lexical cleaning (there is nothing to pop).
        assert_eq!(
            lexically_clean(Path::new("../ws/target/out.json")),
            PathBuf::from("../ws/target/out.json")
        );
        assert_eq!(
            lexically_clean(&cwd.join("a").join("..").join("b")),
            cwd.join("b")
        );
        assert!(
            Path::new(&anchored_redirect_target(".", "target/out.json")).is_absolute(),
            "anchored target must be absolute"
        );
    }

    /// #6960: the kernel follows a symlink before it applies the next `..`.
    /// A bound root and its redirect anchor keep a `..` that follows an
    /// existing symlink, so the printed path resolves to the directory the
    /// producer read. Lexical cleaning would name a different checkout.
    #[cfg(unix)]
    #[test]
    fn bound_root_keeps_a_parent_segment_after_a_symlink() -> Result<(), String> {
        let dir = RoundTripDir::new("symlink-root")?;
        let base = &dir.path;
        for dir in ["outside/child", "outside/repo", "work/repo"] {
            std::fs::create_dir_all(base.join(dir)).map_err(|err| err.to_string())?;
        }
        std::os::unix::fs::symlink(base.join("outside/child"), base.join("work/link"))
            .map_err(|err| err.to_string())?;
        let through_link = base.join("work/link/../repo");
        let canonical = |path: &Path| std::fs::canonicalize(path).map_err(|err| err.to_string());
        // Fixture: the two spellings really name different checkouts.
        assert_eq!(
            canonical(&through_link)?,
            canonical(&base.join("outside/repo"))?
        );
        assert_eq!(lexically_clean(&through_link), base.join("work/repo"));

        let bound = bound_root_path(&through_link);
        let chained = bound_root_path(&base.join("work/link/../../outside/repo"));
        let plain = bound_root_path(&base.join("work/repo/../repo/./"));
        let redirect = anchored_redirect_target(&through_link.to_string_lossy(), "target/out.json");
        let bound_canonical = canonical(&bound)?;
        let chained_canonical = canonical(&chained)?;

        assert_eq!(
            bound, through_link,
            "the symlink's `..` must survive binding"
        );
        assert_eq!(
            bound_canonical.file_name(),
            Some(std::ffi::OsStr::new("repo"))
        );
        assert!(
            bound_canonical.ends_with("outside/repo"),
            "bound root must resolve to the analyzed checkout: {bound_canonical:?}"
        );
        assert_eq!(chained, base.join("work/link/../../outside/repo"));
        assert!(chained_canonical.ends_with("outside/repo"));
        assert_eq!(
            plain,
            base.join("work/repo"),
            "a real directory still cleans"
        );
        assert_eq!(
            redirect,
            format!("{}/target/out.json", through_link.to_string_lossy())
        );
        Ok(())
    }

    /// #3999: a bound root renders as one absolute `--root` that agrees with
    /// its redirect anchor, while a relative root stays the portable route
    /// verbatim — the builder never resolves it against the renderer's
    /// working directory.
    /// #4287: on Unix `\\` is an ordinary filename character. A selected root
    /// whose directory name carries one renders unchanged in `--root` and in
    /// the anchored redirect, so the command names the real directory. This
    /// file is also compiled into xtask, so the typed-recovery half lives in
    /// `command_specs::tests::backslash_root_display_recovers_its_typed_route`.
    #[cfg(unix)]
    #[test]
    fn bound_root_keeps_a_unix_backslash_directory_name() -> Result<(), String> {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|err| err.to_string())?
            .as_nanos();
        let base = std::env::temp_dir().join(format!(
            "ripr-backslash-root-{}-{nonce}",
            std::process::id()
        ));
        let root = base.join("team\\repo");
        std::fs::create_dir_all(&root).map_err(|err| err.to_string())?;
        let spelled = root.to_string_lossy().into_owned();
        assert!(
            spelled.contains('\\'),
            "fixture must carry a literal backslash"
        );
        assert!(!base.join("team").exists(), "fixture must be one directory");

        let bound = bound_root(&spelled);
        assert_eq!(bound, spelled);
        assert!(
            Path::new(&bound).is_dir(),
            "{bound} must name the selected root"
        );
        assert_eq!(root_display(&spelled), spelled);
        assert_eq!(
            anchored_redirect_target(&spelled, "target/ripr/out.json"),
            format!("{spelled}/target/ripr/out.json")
        );
        let command = check_repo_exposure_command(&bound, "instant", "target/ripr/out.json");
        assert_eq!(
            command,
            format!(
                "ripr check --root {} --mode instant --format repo-exposure-json > {}",
                shell_arg(&spelled),
                shell_arg(&format!("{spelled}/target/ripr/out.json"))
            )
        );
        std::fs::remove_dir_all(&base).map_err(|err| err.to_string())
    }

    #[test]
    fn bound_roots_render_absolute_and_relative_roots_stay_portable() {
        let _cwd_guard = hold_cwd_read_guard();
        let bound = bound_root("repo root/./nested/..");
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        assert_eq!(bound, display_path(&cwd.join("repo root")));
        assert!(Path::new(&bound).is_absolute());
        assert_eq!(bound_root(&bound), bound, "binding is idempotent");
        assert_eq!(
            agent_status_command(&bound, None),
            format!("ripr agent status --root {} --json", shell_arg(&bound))
        );
        assert_eq!(
            agent_packet_command(&bound, "seam-a", "target/out.json"),
            format!(
                "ripr agent packet --root {} --seam-id seam-a --json > {}",
                shell_arg(&bound),
                shell_arg(&format!("{bound}/target/out.json"))
            )
        );
        assert_eq!(
            agent_status_command(".", None),
            "ripr agent status --root . --json",
            "a portable root is never resolved against the renderer directory"
        );
    }

    #[test]
    fn workflow_commands_match_existing_status_templates() {
        let _cwd_guard = hold_cwd_read_guard();
        assert_eq!(
            agent_start_command(".", "seam-a", "target/ripr/workflow"),
            "ripr agent start --root . --seam-id seam-a --out target/ripr/workflow"
        );
        assert_eq!(
            check_repo_exposure_command(".", "draft", WORKFLOW_BEFORE_SNAPSHOT_ARTIFACT),
            format!(
                "ripr check --root . --mode draft --format repo-exposure-json > {}",
                anchored_expectation("target/ripr/workflow/before.repo-exposure.json")
            )
        );
        assert_eq!(
            check_repo_exposure_command(".", "draft", WORKFLOW_AFTER_SNAPSHOT_ARTIFACT),
            format!(
                "ripr check --root . --mode draft --format repo-exposure-json > {}",
                anchored_expectation("target/ripr/workflow/after.repo-exposure.json")
            )
        );
        assert_eq!(
            check_analysis_outcome_command(".", "draft", WORKFLOW_ANALYSIS_OUTCOME_ARTIFACT),
            format!(
                "ripr check --root . --mode draft --format json > {}",
                anchored_expectation("target/ripr/workflow/analysis-outcome.json")
            )
        );
        assert_eq!(
            agent_packet_command(".", "seam-a", WORKFLOW_AGENT_PACKET_ARTIFACT),
            format!(
                "ripr agent packet --root . --seam-id seam-a --json > {}",
                anchored_expectation("target/ripr/workflow/agent-packet.json")
            )
        );
        assert_eq!(
            agent_brief_command(".", "seam-a", WORKFLOW_AGENT_BRIEF_ARTIFACT),
            format!(
                "ripr agent brief --root . --seam-id seam-a --json > {}",
                anchored_expectation("target/ripr/workflow/agent-brief.json")
            )
        );
        assert_eq!(
            agent_verify_command(
                ".",
                WORKFLOW_BEFORE_SNAPSHOT_ARTIFACT,
                WORKFLOW_AFTER_SNAPSHOT_ARTIFACT,
                Some(WORKFLOW_AGENT_VERIFY_ARTIFACT),
            ),
            format!(
                "ripr agent verify --root . --before target/ripr/workflow/before.repo-exposure.json --after target/ripr/workflow/after.repo-exposure.json --json > {}",
                anchored_expectation("target/ripr/workflow/agent-verify.json")
            )
        );
        assert_eq!(
            agent_receipt_command(
                ".",
                WORKFLOW_AGENT_VERIFY_ARTIFACT,
                "seam-a",
                Some(WORKFLOW_AGENT_RECEIPT_ARTIFACT),
            ),
            "ripr agent receipt --root . --verify-json target/ripr/workflow/agent-verify.json --seam-id seam-a --json --out target/ripr/reports/agent-receipt.json"
        );
        assert_eq!(
            agent_status_command(".", Some(WORKFLOW_AGENT_STATUS_ARTIFACT)),
            format!(
                "ripr agent status --root . --json > {}",
                anchored_expectation("target/ripr/workflow/agent-status.json")
            )
        );
        assert_eq!(
            agent_status_markdown_command(".", Some(WORKFLOW_AGENT_STATUS_MARKDOWN_ARTIFACT)),
            format!(
                "ripr agent status --root . > {}",
                anchored_expectation("target/ripr/workflow/agent-status.md")
            )
        );
        assert_eq!(
            agent_review_summary_command(".", Some(WORKFLOW_AGENT_REVIEW_SUMMARY_ARTIFACT)),
            format!(
                "ripr agent review-summary --root . --json > {}",
                anchored_expectation("target/ripr/workflow/agent-review-summary.json")
            )
        );
        assert_eq!(
            agent_review_summary_markdown_command(
                ".",
                Some(WORKFLOW_AGENT_REVIEW_SUMMARY_MARKDOWN_ARTIFACT),
            ),
            format!(
                "ripr agent review-summary --root . > {}",
                anchored_expectation("target/ripr/workflow/agent-review-summary.md")
            )
        );
    }

    #[test]
    fn editor_commands_match_existing_lsp_templates() {
        let _cwd_guard = hold_cwd_read_guard();
        assert_eq!(
            agent_packet_command(".", "seam-a", EDITOR_AGENT_PACKET_ARTIFACT),
            format!(
                "ripr agent packet --root . --seam-id seam-a --json > {}",
                anchored_expectation("target/ripr/agent/agent-packet.json")
            )
        );
        assert_eq!(
            check_repo_exposure_command(".", "ready", PILOT_AFTER_SNAPSHOT_ARTIFACT),
            format!(
                "ripr check --root . --mode ready --format repo-exposure-json > {}",
                anchored_expectation("target/ripr/pilot/after.repo-exposure.json")
            )
        );
        assert_eq!(
            agent_verify_command(
                ".",
                PILOT_BEFORE_SNAPSHOT_ARTIFACT,
                PILOT_AFTER_SNAPSHOT_ARTIFACT,
                Some(EDITOR_AGENT_VERIFY_ARTIFACT),
            ),
            format!(
                "ripr agent verify --root . --before target/ripr/pilot/repo-exposure.json --after target/ripr/pilot/after.repo-exposure.json --json > {}",
                anchored_expectation("target/ripr/agent/agent-verify.json")
            )
        );
    }

    #[test]
    fn command_args_quote_spaces_without_touching_plain_tokens() {
        let _cwd_guard = hold_cwd_read_guard();
        // The quoting form is host-dependent (#7729); the builder
        // expectations below interpolate `shell_arg` itself so the same
        // assertion holds on both hosts.
        assert_eq!(shell_arg_for("repo root", false), "'repo root'");
        assert_eq!(shell_arg_for("repo root", true), "\"repo root\"");
        assert_eq!(
            shell_arg("repo root"),
            shell_arg_for("repo root", cfg!(windows))
        );
        assert_eq!(shell_arg("target/ripr/workflow"), "target/ripr/workflow");
        assert_eq!(
            workflow_artifact_path(Path::new("target/ripr/workflow"), "workflow.json"),
            "target/ripr/workflow/workflow.json"
        );
        assert_eq!(
            agent_seam_packets_command(".", "draft mode", "target/ripr/workflow/packets.json"),
            format!(
                "ripr check --root . --mode {} --format agent-seam-packets-json > {}",
                shell_arg("draft mode"),
                anchored_expectation("target/ripr/workflow/packets.json")
            )
        );
        assert_eq!(
            agent_start_command("repo root", "seam a", "target/ripr/work flow"),
            format!(
                "ripr agent start --root {} --seam-id {} --out {}",
                shell_arg("repo root"),
                shell_arg("seam a"),
                shell_arg("target/ripr/work flow")
            )
        );
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let rooted = |tail: &str| shell_arg(&display_path(&cwd.join("repo root").join(tail)));
        assert_eq!(
            check_repo_exposure_command("repo root", "draft", "target/ripr/work flow/before.json"),
            format!(
                "ripr check --root {} --mode draft --format repo-exposure-json > {}",
                shell_arg("repo root"),
                rooted("target/ripr/work flow/before.json")
            )
        );
        assert_eq!(
            check_repo_exposure_command_with_base(
                "repo root",
                Some("origin/main with space"),
                "draft",
                "target/ripr/work flow/before.json",
            ),
            format!(
                "ripr check --root {} --base {} --mode draft --format repo-exposure-json > {}",
                shell_arg("repo root"),
                shell_arg("origin/main with space"),
                rooted("target/ripr/work flow/before.json")
            )
        );
        assert_eq!(
            agent_verify_command(
                "repo root",
                "target/ripr/work flow/before.json",
                "target/ripr/work flow/after.json",
                Some("target/ripr/work flow/verify.json"),
            ),
            format!(
                "ripr agent verify --root {} --before {} --after {} --json > {}",
                shell_arg("repo root"),
                shell_arg("target/ripr/work flow/before.json"),
                shell_arg("target/ripr/work flow/after.json"),
                rooted("target/ripr/work flow/verify.json")
            )
        );
        assert_eq!(
            agent_receipt_command(
                "repo root",
                "target/ripr/work flow/verify.json",
                "seam a",
                Some("target/ripr/work flow/receipt.json"),
            ),
            format!(
                "ripr agent receipt --root {} --verify-json {} --seam-id {} --json --out {}",
                shell_arg("repo root"),
                shell_arg("target/ripr/work flow/verify.json"),
                shell_arg("seam a"),
                shell_arg("target/ripr/work flow/receipt.json")
            )
        );
    }

    /// Values that must round-trip through bash as exactly one argument with
    /// exactly these bytes. Each is a way the previous double-quote encoder
    /// could leak (#2347).
    #[cfg(not(windows))]
    fn hostile_values() -> Vec<(&'static str, &'static str)> {
        vec![
            ("empty", ""),
            ("space", "repo root"),
            ("backslash", "a\\b"),
            ("backslash separated path", "repo\\crates\\ripr\\src"),
            ("double quote", "a\"b"),
            ("single quote", "it's"),
            ("adjacent quotes", "''"),
            ("dollar var", "$HOME"),
            ("command substitution", "$(whoami)"),
            ("backtick", "`whoami`"),
            ("bang", "with!bang"),
            ("newline", "line one\nline two"),
            ("redirect gap id", "gap:pr:amount>=threshold"),
            ("redirect and space", "gap > /tmp/ripr-clobbered"),
            ("semicolon", "a; rm -rf /"),
            ("pipe and glob", "a | b *"),
            ("ampersand", "a && b"),
            ("tilde", "~/notes"),
            ("leading dash", "--not-a-flag"),
            ("escape sequence", "a\u{1b}[2Jb'c\\d"),
            ("bell and carriage return", "x\u{7}y\rz"),
            ("bidi override", "dir\u{202e}gnissim"),
        ]
    }

    #[test]
    fn shell_arg_spells_control_and_bidi_characters_as_escapes() {
        let quoted = shell_arg("a\u{1b}[2Jb'c\\d\u{202e}");
        assert_eq!(
            quoted,
            r#"'a'"$(printf '\033')"'[2Jb'\''c\d'"$(printf '\342\200\256')""#
        );
        assert!(!quoted.chars().any(needs_terminal_escape), "{quoted:?}");
        // Plain hostile text without control characters keeps the host's
        // quoting: bash single quotes off Windows, cmd/PowerShell double
        // quotes on it (#7729).
        assert_eq!(shell_arg_for("it's", false), r"'it'\''s'");
        assert_eq!(shell_arg_for("it's", true), "\"it's\"");
    }

    /// The control-character form must be POSIX, not bash-only: a pasted line
    /// runs under `/bin/sh` (dash on Debian) as often as under bash.
    #[cfg(unix)]
    #[test]
    fn shell_arg_control_characters_round_trip_through_posix_sh() -> Result<(), String> {
        let sh = std::path::Path::new("/bin/sh");
        if !sh.exists() {
            return Ok(());
        }
        for (label, value) in hostile_values() {
            let script = format!(
                "set -- {}\nprintf '%s\\n' \"$#\"\nprintf '%s' \"$1\"\n",
                shell_arg(value)
            );
            let output = std::process::Command::new(sh)
                .arg("-c")
                .arg(&script)
                .output()
                .map_err(|err| format!("{label}: failed to run sh: {err}"))?;
            let stdout = String::from_utf8_lossy(&output.stdout);
            let expected = format!("1\n{value}");
            if !output.status.success() || stdout != expected {
                return Err(format!(
                    "{label}: sh gave {stdout:?} for script {script:?}, expected {expected:?}"
                ));
            }
        }
        Ok(())
    }

    #[test]
    fn shell_arg_passes_unambiguous_tokens_unquoted() {
        assert_eq!(shell_arg("origin/main"), "origin/main");
        assert_eq!(shell_arg("target/ripr/workflow"), "target/ripr/workflow");
        assert_eq!(shell_arg("seam:a-b_c.d:1"), "seam:a-b_c.d:1");
    }

    /// #7729: on Windows the emitted token must paste into cmd.exe — the
    /// shell the issue reproduced the failure in — and PowerShell as one
    /// argument. Double quotes with the interior quote doubled are the only
    /// form both consoles honor. This pins the quoting form per host; the
    /// byte-exact recovery round-trip of the same form lives in
    /// `agent::command_specs::windows_double_quoted_tokens_round_trip_through_recovery`.
    #[test]
    fn shell_arg_windows_form_is_paste_ready_for_cmd_and_powershell() {
        // The drill-in route from the issue: a spaced drive-letter root and a
        // spaced base revision must each arrive as one argument.
        let root = format!("C:{}tmp{}probe win space", '\\', '\\');
        assert_eq!(shell_arg_for(&root, true), format!("\"{root}\""));
        assert_eq!(
            shell_arg_for("HEAD~1 with space", true),
            "\"HEAD~1 with space\""
        );
        // The unambiguous token set stays bare on both hosts, so a plain
        // selector like `probe:src_lib.rs:return_value:f2a5173e` never
        // changes shape.
        assert_eq!(
            shell_arg_for("probe:src_lib.rs:return_value:f2a5173e", true),
            "probe:src_lib.rs:return_value:f2a5173e"
        );
        // The host binding follows the running host.
        assert_eq!(shell_arg(&root), shell_arg_for(&root, cfg!(windows)));
    }

    #[test]
    fn shell_arg_quotes_every_hostile_value() {
        // A bare backslash must never pass through unquoted: bash would consume
        // it as an escape. The empty string must be quoted or the argument
        // vanishes from the command line. Both hosts' forms are pinned through
        // the pure core (#7729: Windows double quotes, interior quote doubled).
        assert_eq!(shell_arg_for("", false), "''");
        assert_eq!(shell_arg_for("", true), "\"\"");
        assert_eq!(shell_arg_for("a\\b", false), "'a\\b'");
        assert_eq!(shell_arg_for("a\\b", true), "\"a\\b\"");
        assert_eq!(shell_arg_for("$(whoami)", false), "'$(whoami)'");
        // #8152 review (security): PowerShell evaluates `$(...)` and `$var`
        // inside double quotes, and Git Bash expands them too, so a value
        // carrying `$` keeps the single-quote form on Windows — literal in
        // both shells (cmd.exe passes the quotes through and fails closed).
        assert_eq!(shell_arg_for("$(whoami)", true), "'$(whoami)'");
        assert_eq!(shell_arg_for("price `$100", false), "'price `$100'");
        assert_eq!(shell_arg_for("price `$100", true), "'price `$100'");
        assert_eq!(shell_arg_for("it's", false), r"'it'\''s'");
        assert_eq!(shell_arg_for("it's", true), "\"it's\"");
        assert_eq!(shell_arg_for("a\"b", false), r#"'a"b'"#);
        assert_eq!(shell_arg_for("a\"b", true), "\"a\"\"b\"");
        assert_eq!(
            shell_arg_for("gap > /tmp/ripr-clobbered", false),
            "'gap > /tmp/ripr-clobbered'"
        );
        assert_eq!(
            shell_arg_for("gap > /tmp/ripr-clobbered", true),
            "\"gap > /tmp/ripr-clobbered\""
        );
    }

    /// #8152 review: a trailing backslash run inside the closing double quote
    /// is read by the native argv parser as escaping that quote — the rest of
    /// the line is swallowed — in Git Bash, and in PowerShell 5.1. Doubling
    /// the run before the closing quote round-trips in all four consoles;
    /// interior separators stay single.
    #[test]
    fn shell_arg_windows_form_doubles_trailing_backslash_before_closing_quote() {
        let drive_root = format!("C:{}repo{}", '\\', '\\');
        assert_eq!(
            shell_arg_for(&drive_root, true),
            format!("\"C:{}repo{}{}\"", '\\', '\\', '\\')
        );
        assert_eq!(
            shell_arg_for("folder space\\", true),
            "\"folder space\\\\\""
        );
        let spaced_root = format!("C:{}tmp{}probe win space", '\\', '\\');
        assert_eq!(
            shell_arg_for(&spaced_root, true),
            format!("\"{spaced_root}\"")
        );
        assert_eq!(shell_arg_for("a\\b\\", true), "\"a\\b\\\\\"");
        // A value ending in a quote is not a backslash case: quote doubling
        // owns the final position and the closing quote still follows.
        assert_eq!(
            shell_arg_for("ends with quote\"", true),
            "\"ends with quote\"\"\""
        );
    }

    /// Prove the encoding against a real bash, not against an assumed shape.
    ///
    /// Each value is encoded, embedded in a command line, and executed; bash
    /// prints one argument per line and the test compares the bytes it received
    /// with the bytes that went in. This is the check that would have caught the
    /// `\!`-inside-double-quotes and bare-backslash defects, both of which look
    /// correct as string assertions.
    ///
    /// Non-Windows only: this proves the bash single-quote contract, which is
    /// the encoding `shell_arg` emits off Windows. On Windows the renderer
    /// emits the cmd.exe/PowerShell double-quote form (#7729), which this test
    /// would mis-measure through Git Bash's `$` expansion.
    #[cfg(not(windows))]
    #[test]
    fn shell_arg_round_trips_through_real_bash() -> Result<(), String> {
        let dir = RoundTripDir::new("argv")?;
        let Some(bash) = bash_for_round_trip(&dir) else {
            // Bash is not guaranteed on every developer machine. CI runs Linux,
            // where this always executes.
            return Ok(());
        };
        for (label, value) in hostile_values() {
            // `printf '%s\n'` prints each argv entry on its own line, so the
            // output is a faithful transcript of what bash actually parsed.
            //
            // The script is written to a file rather than passed with `-c`.
            // On Windows the `-c` route sends the text through Rust's argv
            // joining and then MSYS's re-parsing, which rewrites the quoting
            // before bash ever sees it — that path tests the host, not the
            // encoder. A file reaches bash byte for byte, which is also how
            // these advisory strings are really used.
            // `set --` binds the encoded token to the positional parameters, so
            // `$#` reports how many arguments bash actually parsed. `$1` is then
            // printed with no trailing newline, giving the exact bytes.
            let script = format!(
                "set -- {}\nprintf '%s\\n' \"$#\"\nprintf '%s' \"$1\"\n",
                shell_arg(value)
            );
            let script_path = dir.join("round-trip.sh");
            std::fs::write(&script_path, script.as_bytes())
                .map_err(|err| format!("{label}: write script: {err}"))?;
            let output = std::process::Command::new(&bash)
                .arg(bash_path(&script_path))
                .output()
                .map_err(|err| format!("{label}: failed to run bash: {err}"))?;
            if !output.status.success() {
                return Err(format!(
                    "{label}: bash exited with {}: {}",
                    output.status,
                    String::from_utf8_lossy(&output.stderr)
                ));
            }
            let stdout = String::from_utf8_lossy(&output.stdout);
            // The argument count is printed first and checked separately. It has
            // to be: a value split across two argv entries prints exactly the
            // same bytes as one argv entry containing a newline, so comparing
            // the bytes alone cannot tell a token split from the
            // `line one\nline two` case.
            let (count, received) = stdout
                .split_once('\n')
                .ok_or_else(|| format!("{label}: bash printed no argument count"))?;
            if count.trim_end_matches('\r') != "1" {
                return Err(format!(
                    "{label}: encoded value produced {count} arguments, expected exactly 1 \
                     (script {script:?})"
                ));
            }
            if received != value {
                return Err(format!(
                    "{label}: bash received {received:?} for script {script:?}, expected {value:?}"
                ));
            }
        }
        Ok(())
    }

    /// A redirect-shaped gap id must not be able to truncate a file.
    ///
    /// This is the concrete harm from #2347: the advisory command already ends
    /// in `> out.json`, so an unquoted id containing `>` supplies a second
    /// redirect, writing to a file the operator never named and changing the id
    /// the command actually receives.
    ///
    /// Non-Windows only: the redirect-containing values are quoted with the
    /// bash single-quote form off Windows; the Windows renderer emits the
    /// double-quote form (#7729), which Git Bash would mis-measure through `$`
    /// expansion.
    #[cfg(not(windows))]
    #[test]
    fn redirect_shaped_gap_id_cannot_open_a_second_redirect() -> Result<(), String> {
        let dir = RoundTripDir::new("redirect")?;
        let Some(bash) = bash_for_round_trip(&dir) else {
            return Ok(());
        };
        let victim = dir.join("must-not-be-written.txt");
        std::fs::write(&victim, b"original contents")
            .map_err(|err| format!("seed victim file: {err}"))?;

        // Use a bash-shaped path for the redirect target so the hostile value is
        // a plausible redirect on the shell under test.
        let victim_display = victim.display().to_string().replace('\\', "/");
        let hostile_id = format!("gap:pr:1 > {victim_display}");
        let script = format!(
            "set -- {}\nprintf '%s\\n' \"$#\"\nprintf '%s' \"$1\"\n",
            shell_arg(&hostile_id)
        );
        let script_path = dir.join("redirect.sh");
        std::fs::write(&script_path, script.as_bytes())
            .map_err(|err| format!("write script: {err}"))?;
        let output = std::process::Command::new(&bash)
            .arg(bash_path(&script_path))
            .output()
            .map_err(|err| format!("failed to run bash: {err}"))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let (count, received) = stdout
            .split_once('\n')
            .map(|(count, rest)| (count.trim_end_matches('\r').to_string(), rest.to_string()))
            .unwrap_or_else(|| (String::new(), stdout.to_string()));
        let contents =
            std::fs::read_to_string(&victim).map_err(|err| format!("read victim file: {err}"))?;

        if count != "1" {
            return Err(format!(
                "redirect-shaped id produced {count} arguments, expected exactly 1"
            ));
        }
        if contents != "original contents" {
            return Err(format!(
                "redirect-shaped id truncated an unintended file; contents now {contents:?}"
            ));
        }
        if received != hostile_id {
            return Err(format!(
                "redirect-shaped id was altered in transit: got {received:?}, expected {hostile_id:?}"
            ));
        }
        Ok(())
    }

    /// Render a path the way the shell under test expects to receive it.
    ///
    /// A Windows path handed to Git Bash loses its backslashes to MSYS argument
    /// processing, so the directory separators are eaten and the remaining
    /// segments run together into one meaningless name. Forward slashes survive
    /// on both platforms.
    #[cfg(not(windows))]
    fn bash_path(path: &std::path::Path) -> String {
        path.display().to_string().replace('\\', "/")
    }

    /// Owns the scratch directory for one round-trip test.
    ///
    /// These tests return `Err` from several places and can panic inside the
    /// loop, so cleanup has to run on scope exit rather than at the end of the
    /// happy path — otherwise every run leaves another `ripr-shell-arg-*`
    /// directory behind in the system temp dir.
    #[cfg(not(windows))]
    struct RoundTripDir {
        path: std::path::PathBuf,
    }

    #[cfg(not(windows))]
    impl RoundTripDir {
        fn new(label: &str) -> Result<Self, String> {
            let path =
                std::env::temp_dir().join(format!("ripr-shell-arg-{}-{label}", std::process::id()));
            std::fs::create_dir_all(&path).map_err(|err| format!("create temp dir: {err}"))?;
            Ok(Self { path })
        }

        fn join(&self, name: &str) -> std::path::PathBuf {
            self.path.join(name)
        }
    }

    #[cfg(not(windows))]
    impl Drop for RoundTripDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    /// Find a bash that can actually execute a script at the path this test
    /// writes to.
    ///
    /// Probing with `bash -c "exit 0"` is not enough on Windows: `bash.exe` on
    /// `PATH` is frequently WSL bash, which runs fine but cannot open a
    /// drive-letter host path, so every case would fail for a reason that has
    /// nothing to do with the encoder. Git Bash is preferred and the candidate
    /// is confirmed by running a probe script from the real directory.
    #[cfg(not(windows))]
    fn bash_for_round_trip(dir: &RoundTripDir) -> Option<std::path::PathBuf> {
        // Git Bash is preferred over a bare `bash.exe`, which on Windows is
        // frequently WSL. The Git install prefix is read from the environment
        // rather than hard-coded, so no machine-local path is baked in.
        let mut candidates: Vec<String> = Vec::new();
        if cfg!(windows) {
            for prefix_var in ["ProgramFiles", "ProgramFiles(x86)"] {
                if let Ok(prefix) = std::env::var(prefix_var) {
                    candidates.push(format!("{prefix}/Git/bin/bash.exe"));
                }
            }
            candidates.push("bash.exe".to_string());
        } else {
            candidates.push("/bin/bash".to_string());
            candidates.push("bash".to_string());
        }
        let probe_path = dir.join("probe.sh");
        std::fs::write(&probe_path, b"printf 'ok\\n'\n").ok()?;
        let probe_arg = bash_path(&probe_path);
        candidates
            .iter()
            .map(std::path::PathBuf::from)
            .find(|candidate| {
                std::process::Command::new(candidate)
                    .arg(&probe_arg)
                    .output()
                    .is_ok_and(|output| {
                        output.status.success()
                            && String::from_utf8_lossy(&output.stdout).trim() == "ok"
                    })
            })
    }
}
