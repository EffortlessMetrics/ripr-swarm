use crate::agent::loop_commands::shell_arg;
use crate::config::{CONFIG_FILE_NAME, detect_python_project};
use crate::output::path::human_path;
use serde_json::{Value, json};
use std::path::Path;

use super::options::FirstPrOptions;
use super::{
    base_fetch_refspec, command_problem, detect_typescript_project, git_args, missing_base_command,
    resolve_path, run_git,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct FirstPrPreflight {
    status: &'static str,
    mode: &'static str,
    root: String,
    resolved_root: String,
    base: String,
    head: String,
    next_command: Option<String>,
    recovery_commands: Vec<String>,
    recovery_guidance: Option<String>,
    checks: Vec<PreflightCheck>,
}

impl FirstPrPreflight {
    pub(super) fn warnings(&self) -> impl Iterator<Item = String> + '_ {
        self.checks
            .iter()
            .filter(|check| {
                check.status != "ok" && check.status != "defaulted" && check.status != "will_create"
            })
            .map(|check| check.message.clone())
    }

    pub(super) fn to_json(&self) -> Value {
        let mut value = json!({
            "status": self.status,
            "mode": self.mode,
            "root": self.root,
            "resolved_root": self.resolved_root,
            "base": self.base,
            "head": self.head,
            "next_command": self.next_command,
            "checks": self.checks.iter().map(PreflightCheck::to_json).collect::<Vec<_>>()
        });
        if !self.recovery_commands.is_empty() {
            value["recovery_commands"] = json!(self.recovery_commands);
        }
        if let Some(guidance) = &self.recovery_guidance {
            value["recovery_guidance"] = json!(guidance);
        }
        value
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PreflightCheck {
    id: &'static str,
    label: &'static str,
    status: &'static str,
    message: String,
    path: Option<String>,
    next_command: Option<String>,
    recovery_commands: Vec<String>,
    recovery_guidance: Option<String>,
}

impl PreflightCheck {
    fn ok(id: &'static str, label: &'static str, message: impl Into<String>) -> Self {
        Self {
            id,
            label,
            status: "ok",
            message: message.into(),
            path: None,
            next_command: None,
            recovery_commands: Vec::new(),
            recovery_guidance: None,
        }
    }

    fn defaulted(id: &'static str, label: &'static str, message: impl Into<String>) -> Self {
        Self {
            id,
            label,
            status: "defaulted",
            message: message.into(),
            path: None,
            next_command: None,
            recovery_commands: Vec::new(),
            recovery_guidance: None,
        }
    }

    fn needs_attention(
        id: &'static str,
        label: &'static str,
        message: impl Into<String>,
        next_command: Option<String>,
    ) -> Self {
        Self {
            id,
            label,
            status: "needs_attention",
            message: message.into(),
            path: None,
            next_command,
            recovery_commands: Vec::new(),
            recovery_guidance: None,
        }
    }

    fn no_action(
        id: &'static str,
        label: &'static str,
        message: impl Into<String>,
        next_command: Option<String>,
    ) -> Self {
        Self {
            id,
            label,
            status: "no_action",
            message: message.into(),
            path: None,
            next_command,
            recovery_commands: Vec::new(),
            recovery_guidance: None,
        }
    }

    fn with_path(mut self, path: impl Into<String>) -> Self {
        self.path = Some(path.into());
        self
    }

    fn with_recovery_commands(mut self, commands: Vec<String>) -> Self {
        self.recovery_commands = commands;
        self
    }

    fn with_recovery_guidance(mut self, guidance: impl Into<String>) -> Self {
        self.recovery_guidance = Some(guidance.into());
        self
    }

    fn to_json(&self) -> Value {
        let mut value = json!({
            "id": self.id,
            "label": self.label,
            "status": self.status,
            "message": self.message,
            "path": self.path,
            "next_command": self.next_command
        });
        if !self.recovery_commands.is_empty() {
            value["recovery_commands"] = json!(self.recovery_commands);
        }
        if let Some(guidance) = &self.recovery_guidance {
            value["recovery_guidance"] = json!(guidance);
        }
        value
    }
}

pub(super) fn first_pr_preflight(root: &Path, options: &FirstPrOptions) -> FirstPrPreflight {
    let mut checks = Vec::new();
    // #5252 item 4: every renderer-owned preflight spelling renders through
    // the shared human-path rule (#4378) so one document never mixes the
    // verbatim `display` form with the slash-spelled `next_command` lines.
    // `options.root` echoes still carry the caller's own spelling.
    let resolved_root = human_path(root);
    checks.push(preflight_root_check(root, options));
    let git_available = matches!(checks.last().map(|check| check.status), Some("ok"))
        && preflight_git_repo_check(root, &mut checks);
    let mut base_ok = false;
    let mut head_ok = false;
    if git_available {
        base_ok = preflight_git_ref_check(
            root,
            &mut checks,
            "git_base",
            "Git base",
            &options.base,
            Some(missing_base_command(options)),
            missing_base_recovery_commands(options),
        );
        head_ok = preflight_git_ref_check(
            root,
            &mut checks,
            "git_head",
            "Git head",
            &options.head,
            Some(format!(
                "Check --head `{}` or fetch the branch, then rerun `ripr first-pr --root {} --base {} --head {}`.",
                options.head,
                shell_arg(&options.command_root()),
                shell_arg(&options.base),
                shell_arg(&options.head)
            )),
            vec![rerun_command(options)],
        );
    }
    if git_available && base_ok && head_ok {
        preflight_diff_check(root, options, &mut checks);
    }
    checks.push(preflight_project_check(root));
    checks.push(preflight_config_check(root));
    checks.push(preflight_output_check(root, options));
    checks.push(PreflightCheck::ok(
        "mode",
        "Mode",
        if options.check {
            "Check mode validates the existing start-here packet without rewriting it."
        } else {
            "Write mode composes start-here.json and start-here.md from explicit artifacts."
        },
    ));
    let next_command = checks.iter().find_map(|check| check.next_command.clone());
    let recovery_commands = checks
        .iter()
        .find(|check| check.next_command.is_some())
        .map(|check| check.recovery_commands.clone())
        .unwrap_or_default();
    let recovery_guidance = checks
        .iter()
        .find(|check| check.next_command.is_some())
        .and_then(|check| check.recovery_guidance.clone());
    let status = if checks
        .iter()
        .any(|check| check.status == "needs_attention" || check.status == "no_action")
    {
        "needs_attention"
    } else {
        "ready"
    };
    FirstPrPreflight {
        status,
        mode: if options.check { "check" } else { "write" },
        root: options.root.clone(),
        resolved_root,
        base: options.base.clone(),
        head: options.head.clone(),
        next_command,
        recovery_commands,
        recovery_guidance,
        checks,
    }
}

/// Executable display commands are carried separately from recovery prose.
/// Renderers can pair their shell forms without extracting code from a sentence.
fn rerun_command(options: &FirstPrOptions) -> String {
    format!(
        "ripr first-pr --root {} --base {} --head {}",
        shell_arg(&options.command_root()),
        shell_arg(&options.base),
        shell_arg(&options.head)
    )
}

fn missing_base_recovery_commands(options: &FirstPrOptions) -> Vec<String> {
    let mut commands = Vec::new();
    if let Some(branch) = options
        .base
        .strip_prefix("origin/")
        .filter(|branch| !branch.trim().is_empty())
    {
        let refspec = base_fetch_refspec(branch);
        commands.push(format!(
            "git -C {} fetch origin -- {}",
            shell_arg(&options.command_root()),
            shell_arg(&refspec)
        ));
    }
    commands.push(rerun_command(options));
    commands
}

fn preflight_root_check(root: &Path, options: &FirstPrOptions) -> PreflightCheck {
    if root.is_dir() {
        PreflightCheck::ok(
            "root",
            "Workspace root",
            format!("Workspace root `{}` exists.", options.root),
        )
        .with_path(human_path(root))
    } else {
        PreflightCheck::needs_attention(
            "root",
            "Workspace root",
            format!(
                "Workspace root `{}` does not exist or is not a directory.",
                options.root
            ),
            Some("Run from a repository root or pass --root <path>.".to_string()),
        )
        .with_path(human_path(root))
    }
}

fn preflight_git_repo_check(root: &Path, checks: &mut Vec<PreflightCheck>) -> bool {
    match run_git(root, &git_args(&["rev-parse", "--is-inside-work-tree"])) {
        Ok(output) if output.success() && output.stdout.trim() == "true" => {
            checks.push(PreflightCheck::ok(
                "git_repo",
                "Git repository",
                "The root is inside a Git worktree.",
            ));
            true
        }
        Ok(output) => {
            checks.push(PreflightCheck::needs_attention(
                "git_repo",
                "Git repository",
                command_problem(
                    "The root is not a Git worktree.",
                    &output,
                    "Run from a Git worktree or pass --root <repo>.",
                ),
                Some("Run from a Git worktree or pass --root <repo>.".to_string()),
            ));
            false
        }
        Err(message) => {
            checks.push(PreflightCheck::needs_attention(
                "git_repo",
                "Git repository",
                format!("Could not run git preflight: {message}."),
                Some(
                    "Install git or run first-pr from an environment where git is available."
                        .to_string(),
                ),
            ));
            false
        }
    }
}

fn preflight_git_ref_check(
    root: &Path,
    checks: &mut Vec<PreflightCheck>,
    id: &'static str,
    label: &'static str,
    rev: &str,
    next_command: Option<String>,
    recovery_commands: Vec<String>,
) -> bool {
    let commit = format!("{rev}^{{commit}}");
    match run_git(
        root,
        &[
            "rev-parse".to_string(),
            "--verify".to_string(),
            "--quiet".to_string(),
            commit,
        ],
    ) {
        Ok(output) if output.success() => {
            checks.push(PreflightCheck::ok(
                id,
                label,
                format!("Resolved `{rev}` to a commit."),
            ));
            true
        }
        Ok(output) => {
            // #5252 item 4: `rev-parse --verify --quiet` emits no detail, so
            // both refs used to fall back to one shared string and warn
            // twice, byte-identical, naming neither ref. The fallback keeps
            // the summary so each warning names its role and rev; the role
            // parallels the check id at the two call sites.
            let role = match id {
                "git_base" => "--base",
                "git_head" => "--head",
                unknown => unknown,
            };
            let summary = format!("Could not resolve {role} `{rev}` to a commit.");
            checks.push(
                PreflightCheck::needs_attention(
                    id,
                    label,
                    command_problem(
                        &summary,
                        &output,
                        &format!(
                            "{summary} Fetch the missing ref or pass a resolvable --base/--head."
                        ),
                    ),
                    next_command,
                )
                .with_recovery_commands(recovery_commands)
                .with_recovery_guidance(
                    "Fetch the missing ref or pass a resolvable --base/--head before rerunning.",
                ),
            );
            false
        }
        Err(message) => {
            checks.push(
                PreflightCheck::needs_attention(
                    id,
                    label,
                    format!("Could not run git ref preflight for `{rev}`: {message}."),
                    next_command,
                )
                .with_recovery_commands(recovery_commands)
                .with_recovery_guidance(
                    "Restore Git availability and resolve --base/--head before rerunning.",
                ),
            );
            false
        }
    }
}

fn preflight_diff_check(root: &Path, options: &FirstPrOptions, checks: &mut Vec<PreflightCheck>) {
    let range = format!("{}..{}", options.base, options.head);
    match run_git(
        root,
        &[
            "diff".to_string(),
            "--quiet".to_string(),
            range.clone(),
            "--".to_string(),
        ],
    ) {
        Ok(output) if matches!(output.code, Some(0)) => {
            checks.push(PreflightCheck::no_action(
                "git_diff",
                "Git diff",
                format!("No file diff was found for `{range}`."),
                Some(format!(
                    "Choose a head with changes or rerun after committing PR work: `ripr first-pr --root {} --base {} --head {}`.",
                    shell_arg(&options.command_root()),
                    shell_arg(&options.base),
                    shell_arg(&options.head)
                )),
            ).with_recovery_commands(vec![rerun_command(options)])
             .with_recovery_guidance("Choose a head with changes or commit PR work before rerunning."));
        }
        Ok(output) if matches!(output.code, Some(1)) => {
            checks.push(PreflightCheck::ok(
                "git_diff",
                "Git diff",
                format!("Found a file diff for `{range}`."),
            ));
        }
        Ok(output) => {
            checks.push(PreflightCheck::needs_attention(
                "git_diff",
                "Git diff",
                command_problem(
                    &format!("Could not inspect diff range `{range}`."),
                    &output,
                    "Check --base and --head, then rerun first-pr.",
                ),
                Some(format!(
                    "Check --base and --head, then rerun `ripr first-pr --root {} --base {} --head {}`.",
                    shell_arg(&options.command_root()),
                    shell_arg(&options.base),
                    shell_arg(&options.head)
                )),
            ).with_recovery_commands(vec![rerun_command(options)])
             .with_recovery_guidance("Check --base and --head and restore the Git objects needed for the diff before rerunning."));
        }
        Err(message) => {
            checks.push(PreflightCheck::needs_attention(
                "git_diff",
                "Git diff",
                format!("Could not run git diff preflight: {message}."),
                Some(
                    "Install git or rerun from an environment where git is available.".to_string(),
                ),
            ));
        }
    }
}

fn preflight_project_check(root: &Path) -> PreflightCheck {
    let manifest = root.join("Cargo.toml");
    if manifest.is_file() {
        PreflightCheck::ok(
            "cargo_workspace",
            "Cargo workspace",
            "Cargo.toml was found at the workspace root.",
        )
        .with_path(human_path(&manifest))
    } else if detect_python_project(root) {
        PreflightCheck::ok(
            "python_project",
            "Python project",
            "Python project markers were found; first-pr can consume Python preview gap-ledger records.",
        )
        .with_path(human_path(root))
    } else if detect_typescript_project(root) {
        PreflightCheck::ok(
            "typescript_project",
            "TypeScript project",
            "TypeScript project markers were found; first-pr can consume TypeScript preview gap-ledger records.",
        )
        .with_path(human_path(root))
    } else {
        PreflightCheck::needs_attention(
            "cargo_workspace",
            "Cargo workspace",
            "No Cargo.toml was found at the workspace root.",
            Some(
                "Run from a Rust/Cargo workspace, a Python or TypeScript project root, or pass --root <repo>."
                    .to_string(),
            ),
        )
        .with_path(human_path(&manifest))
    }
}

fn preflight_config_check(root: &Path) -> PreflightCheck {
    let config = root.join(CONFIG_FILE_NAME);
    match std::fs::metadata(&config) {
        Ok(meta) if meta.is_file() => match crate::bounded_input::read_to_string(&config) {
            Ok(_) => PreflightCheck::ok(
                "ripr_config",
                "RIPR config",
                format!("{CONFIG_FILE_NAME} was found."),
            )
            .with_path(human_path(&config)),
            Err(_) => unreadable_ripr_config(&config),
        },
        Ok(_) => unreadable_ripr_config(&config),
        Err(err)
            if err.kind() == std::io::ErrorKind::NotFound
                && !crate::config::config_present_at_root(root) =>
        {
            PreflightCheck::defaulted(
                "ripr_config",
                "RIPR config",
                format!("No {CONFIG_FILE_NAME} was found; built-in advisory defaults apply."),
            )
            .with_path(human_path(&config))
        }
        Err(_) => unreadable_ripr_config(&config),
    }
}

fn unreadable_ripr_config(config: &Path) -> PreflightCheck {
    PreflightCheck::needs_attention(
        "ripr_config",
        "RIPR config",
        format!("{CONFIG_FILE_NAME} is present but unreadable."),
        Some(format!(
            "Replace the unreadable {CONFIG_FILE_NAME} with a readable file, then rerun first-pr."
        )),
    )
    .with_path(human_path(config))
}

fn preflight_output_check(root: &Path, options: &FirstPrOptions) -> PreflightCheck {
    let out_dir = resolve_path(root, &options.out_dir);
    if out_dir.exists() && !out_dir.is_dir() {
        return PreflightCheck::needs_attention(
            "output_dir",
            "Output directory",
            format!(
                "Output path `{}` exists but is not a directory.",
                options.out_dir
            ),
            Some("Choose a directory for --out-dir, then rerun first-pr.".to_string()),
        )
        .with_path(human_path(&out_dir));
    }
    if out_dir.is_dir() {
        PreflightCheck::ok(
            "output_dir",
            "Output directory",
            format!("Output directory `{}` exists.", options.out_dir),
        )
        .with_path(human_path(&out_dir))
    } else {
        PreflightCheck {
            id: "output_dir",
            label: "Output directory",
            status: "will_create",
            message: format!(
                "Output directory `{}` will be created if needed.",
                options.out_dir
            ),
            path: Some(human_path(&out_dir)),
            next_command: None,
            recovery_commands: Vec::new(),
            recovery_guidance: None,
        }
    }
}

// Every test below is unix-only (symlink/FIFO semantics), so on Windows the
// module compiles empty and the glob import would be unused under
// `-D warnings`. Gate the module itself; Linux compilation is unchanged.
// Inherited Windows-only clippy repair, reproduced on the untouched base.
#[cfg(all(test, unix))]
mod tests {
    use super::*;

    /// A dangling `ripr.toml` is present but unreadable: first-pr preflight
    /// must not claim built-in defaults, matching `load_for_root`.
    #[cfg(unix)]
    #[test]
    fn dangling_ripr_toml_symlink_is_present_not_built_in_defaults() -> Result<(), String> {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| format!("clock: {error}"))?
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "ripr-first-pr-preflight-dangling-{}-{stamp}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).map_err(|error| error.to_string())?;
        struct Cleanup(std::path::PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let _cleanup = Cleanup(root.clone());

        let absent = preflight_config_check(&root);
        if absent.status != "defaulted" || !absent.message.contains("built-in advisory defaults") {
            return Err(format!(
                "an absent ripr.toml must stay defaulted built-in defaults: {} / {}",
                absent.status, absent.message
            ));
        }

        std::os::unix::fs::symlink("no-such-target.toml", root.join("ripr.toml"))
            .map_err(|error| error.to_string())?;
        let load_error = match crate::config::load_for_root(&root) {
            Ok(_) => {
                return Err("load_for_root must refuse a dangling ripr.toml".to_string());
            }
            Err(error) => error,
        };
        let check = preflight_config_check(&root);
        if !load_error.contains("ripr.toml") {
            return Err(format!(
                "load_for_root must name ripr.toml for a dangling link: {load_error}"
            ));
        }
        if check.status != "needs_attention" {
            return Err(format!(
                "a dangling ripr.toml must need attention, not {}: {}",
                check.status, check.message
            ));
        }
        if check.message.contains("built-in advisory defaults") {
            return Err(format!(
                "a dangling ripr.toml must not be described as built-in defaults: {}",
                check.message
            ));
        }
        if !check.message.contains("ripr.toml") || !check.message.contains("unreadable") {
            return Err(format!(
                "preflight must name ripr.toml as present but unreadable: {}",
                check.message
            ));
        }
        let path = check.path.as_deref().unwrap_or("");
        if !path.contains("ripr.toml") {
            return Err(format!(
                "preflight must attach the ripr.toml path, not {path:?}"
            ));
        }
        Ok(())
    }

    /// A chmod-000 regular `ripr.toml` is still a file, so `Path::is_file()`
    /// is not a readability probe. Preflight must not mark it `ok` while
    /// `load_for_root` cannot read it.
    #[cfg(unix)]
    #[test]
    fn unreadable_regular_ripr_toml_is_present_not_ok() -> Result<(), String> {
        use std::os::unix::fs::PermissionsExt as _;

        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| format!("clock: {error}"))?
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "ripr-first-pr-preflight-unreadable-{}-{stamp}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).map_err(|error| error.to_string())?;
        struct Cleanup {
            root: std::path::PathBuf,
            file: std::path::PathBuf,
        }
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ =
                    std::fs::set_permissions(&self.file, std::fs::Permissions::from_mode(0o644));
                let _ = std::fs::remove_dir_all(&self.root);
            }
        }
        let config = root.join("ripr.toml");
        std::fs::write(&config, "mode = \"advisory\"\n").map_err(|error| error.to_string())?;
        let _cleanup = Cleanup {
            root: root.clone(),
            file: config.clone(),
        };

        let readable = preflight_config_check(&root);
        if readable.status != "ok" {
            return Err(format!(
                "a readable ripr.toml must stay ok, not {}: {}",
                readable.status, readable.message
            ));
        }

        std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o000))
            .map_err(|error| format!("chmod ripr.toml 000: {error}"))?;
        let load_error = match crate::config::load_for_root(&root) {
            Ok(_) => {
                // A privileged process can still read mode 000. Skip rather
                // than fail the production behavior that remains correct for
                // unprivileged users.
                return Ok(());
            }
            Err(error) => error,
        };
        if !load_error.contains("ripr.toml") {
            return Err(format!(
                "load_for_root must name ripr.toml for a chmod-000 file: {load_error}"
            ));
        }
        let check = preflight_config_check(&root);
        if check.status != "needs_attention" {
            return Err(format!(
                "an unreadable regular ripr.toml must need attention, not {}: {}",
                check.status, check.message
            ));
        }
        if check.message.contains("built-in advisory defaults") {
            return Err(format!(
                "an unreadable regular ripr.toml must not be described as built-in defaults: {}",
                check.message
            ));
        }
        if !check.message.contains("ripr.toml") || !check.message.contains("unreadable") {
            return Err(format!(
                "preflight must name ripr.toml as present but unreadable: {}",
                check.message
            ));
        }
        Ok(())
    }

    /// A FIFO named `ripr.toml` is present but not a regular file. Preflight
    /// must not open it: `bounded_input::read_to_string` waits for a writer.
    #[cfg(unix)]
    #[test]
    fn fifo_ripr_toml_is_present_not_ok_and_does_not_block() -> Result<(), String> {
        use std::os::unix::fs::FileTypeExt as _;

        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| format!("clock: {error}"))?
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "ripr-first-pr-preflight-fifo-{}-{stamp}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).map_err(|error| error.to_string())?;
        struct Cleanup(std::path::PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let _cleanup = Cleanup(root.clone());
        let config = root.join("ripr.toml");
        let status = std::process::Command::new("mkfifo")
            .arg(&config)
            .status()
            .map_err(|error| format!("mkfifo: {error}"))?;
        if !status.success() {
            return Err(format!("mkfifo failed: {status}"));
        }
        let file_type = std::fs::symlink_metadata(&config)
            .map_err(|error| format!("inspect FIFO fixture: {error}"))?
            .file_type();
        if !file_type.is_fifo() {
            return Err("fixture must be a FIFO named ripr.toml".to_string());
        }
        let check = preflight_config_check(&root);
        if check.status != "needs_attention" {
            return Err(format!(
                "a FIFO ripr.toml must need attention, not {}: {}",
                check.status, check.message
            ));
        }
        if check.message.contains("built-in advisory defaults") {
            return Err(format!(
                "a FIFO ripr.toml must not be described as built-in defaults: {}",
                check.message
            ));
        }
        Ok(())
    }
}
