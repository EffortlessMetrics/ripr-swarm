use crate::cli::commands_options::{InitCi, InitOptions};
use crate::cli::help;
use crate::cli::parse::expect_value;
use crate::cli::suggest::{unknown_argument, unknown_value};
use crate::config::{CONFIG_FILE_NAME, generated_init_config};
use crate::output;
use std::path::{Path, PathBuf};

use super::init_workflow::{generated_github_actions_workflow, workflow_install_version};

pub(in crate::cli) fn init(args: &[String]) -> Result<(), String> {
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        help::print_init_help();
        return Ok(());
    }
    let options = parse_init_options(args)?;
    // #2572: `--dry-run` and the real run resolve the SAME plan, so a preview
    // can never disagree with the run it previews. Previously `--dry-run`
    // returned before every precondition check and printed file bodies
    // unconditionally, so it reported success for two runs that actually fail:
    // an existing `ripr.toml` without `--force`, and a root that is not a
    // directory.
    let plan = init_plan(&options)?;
    // #7465: `init` writes `./ripr.toml` at `--root` (default `.`) with no
    // workspace walk, while `ripr check` from the same path resolves to the
    // enclosing workspace and never loads this file. Say so before the user
    // treats the write as the policy `check` will apply.
    if let Some(warning) = check_ignores_init_config_warning(&options.root) {
        eprintln!("{warning}");
    }
    if let Some(warning) = unanalyzed_root_warning(&options.root) {
        eprintln!("{warning}");
    }
    // #5252 item 7: `init` into a non-repo wrote branch-based configuration
    // with no hint that no branch exists there. Advisory like the root
    // warning, in both dry-run and real runs.
    if let Some(warning) = non_git_root_warning(&options.root) {
        eprintln!("{warning}");
    }
    // #5208: an unreleased generator cannot pin itself — that version does
    // not exist on crates.io, so the install step would fail. The workflow
    // pins the latest release instead; say so loudly, on stderr like the
    // root warning, in both dry-run and real runs.
    if options.ci.is_some() {
        let generator = env!("CARGO_PKG_VERSION");
        let pinned = workflow_install_version(generator);
        if pinned != generator {
            eprintln!(
                "ripr: warning: this ripr ({generator}) is not released, so the generated workflow pins the latest release ({pinned}) instead of the generator. After upgrading ripr, refresh with `ripr init --ci github --force` and review the diff before committing."
            );
        }
    }
    if options.dry_run {
        print_init_dry_run(&plan);
        return Ok(());
    }
    apply_init_plan(&plan, &options.root)
}

/// Warn when the planned `ripr.toml` sits where implicit `ripr check` will
/// not load it.
///
/// `init` keeps `--root` as given (default `.`). Implicit `check` calls
/// `resolve_project_root` from the current directory and rebinds to an
/// enclosing `[workspace]` (or other project root) when that path differs.
/// Config discovery then walks only ancestors of *that* resolved root, so a
/// file written in a workspace member is skipped with no other signal. This
/// warning reuses that resolver read-only; it does not retarget the write
/// (#7468 owns check/doctor/pilot config identity).
fn check_ignores_init_config_warning(root: &Path) -> Option<String> {
    let (resolved_root, reason) = super::check::resolve_project_root(root).ok()??;
    let current = std::fs::canonicalize(root).ok()?;
    if resolved_root == current {
        return None;
    }
    let shown_root = output::path::human_path(root);
    let shown_resolved = output::path::human_path(&resolved_root);
    let shown_config = output::path::human_path(&root.join(CONFIG_FILE_NAME));
    let quoted_root = crate::agent::loop_commands::shell_arg(&shown_root);
    let quoted_resolved = crate::agent::loop_commands::shell_arg(&shown_resolved);
    let resolved_as = match reason {
        super::check::ImplicitRootReason::Workspace => {
            format!("the workspace root `{shown_resolved}`")
        }
        super::check::ImplicitRootReason::Package => {
            format!("the nearest crate root `{shown_resolved}`")
        }
        super::check::ImplicitRootReason::PnpmWorkspace
        | super::check::ImplicitRootReason::PackageJsonWorkspaces
        | super::check::ImplicitRootReason::UvWorkspace => {
            format!("the workspace root `{shown_resolved}`")
        }
        super::check::ImplicitRootReason::GitTopLevel => {
            format!("the git top level `{shown_resolved}`")
        }
    };
    let resolved_config = resolved_root.join(CONFIG_FILE_NAME);
    let init_recovery = match path_is_occupied(&resolved_config) {
        Ok(true) => {
            let shown_resolved_config = output::path::human_path(&resolved_config);
            format!(
                "The file check will load is `{shown_resolved_config}`; review or edit that file (`ripr init --root {quoted_resolved} --force` resets customizations)."
            )
        }
        Ok(false) | Err(_) => format!(
            "Re-run `ripr init --root {quoted_resolved}` to write the file check will load."
        ),
    };
    Some(format!(
        "ripr: warning: `ripr check` from `{shown_root}` resolves to {resolved_as} and will ignore `{shown_config}`. {init_recovery} Or pass `ripr check --root {quoted_root}` to analyze this directory with this file."
    ))
}

/// Warn before configuring ripr for a repository it cannot analyze: a Go or
/// Java repository got a workflow and "run `ripr check`" with no hint that
/// every change would be reported as not analyzed.
fn unanalyzed_root_warning(root: &Path) -> Option<String> {
    if !crate::analysis::workspace_rust_files(root).is_empty()
        || !crate::analysis::workspace_preview_language_files(root).is_empty()
    {
        return None;
    }
    let unanalyzed = crate::analysis::workspace_unanalyzed_source_languages(root);
    if unanalyzed.is_empty() {
        return None;
    }
    let found = unanalyzed
        .iter()
        .map(|(language, count)| format!("{language} ({count} file(s))"))
        .collect::<Vec<_>>()
        .join(", ");
    Some(format!(
        "ripr: warning: the root `{}` has {found} source and no Rust, TypeScript/JavaScript or Python source. ripr does not analyze these languages, so `ripr check` and this configuration will report their changes as not analyzed.",
        output::path::human_path(root)
    ))
}

/// Warn before configuring a directory that is not a Git work tree: the
/// generated workflow and Next steps assume branch-based analysis, which has
/// no branch to compare outside a repository. The verdict is doctor's
/// three-way work-tree probe: only an established outside is named as one.
/// A refused repository keeps its repair instead of being told it lies
/// outside a repository, and a probe that never answered says Git could
/// not confirm the state. Advisory either way: the configuration is
/// written regardless.
fn non_git_root_warning(root: &Path) -> Option<String> {
    non_git_root_warning_for_probe(root, output::doctor::work_tree_probe(root))
}

/// The warning dispatch, split from the probe call so every wording branch
/// pins without arranging a refused Git or a missing binary.
fn non_git_root_warning_for_probe(
    root: &Path,
    probe: Option<output::doctor::WorkTreeProbe>,
) -> Option<String> {
    use output::doctor::WorkTreeProbe;
    let shown = output::path::human_path(root);
    match probe {
        Some(WorkTreeProbe::Inside) => None,
        Some(WorkTreeProbe::Outside) => Some(format!(
            "ripr: warning: the root `{shown}` is not inside a Git work tree. The generated configuration analyzes branches against their default branch, so `ripr check` has no branch to compare until this directory is inside a repository."
        )),
        Some(WorkTreeProbe::Refused(repair)) => Some(format!(
            "ripr: warning: Git could not confirm the root `{shown}` is inside a work tree ({repair}). The generated configuration analyzes branches against their default branch, so confirm the directory is inside a repository or `ripr check` has no branch to compare."
        )),
        None => Some(format!(
            "ripr: warning: Git could not confirm the root `{shown}` is inside a work tree (the probe did not answer; Git may be unavailable or the spawn timed out). The generated configuration analyzes branches against their default branch, so confirm the directory is inside a repository or `ripr check` has no branch to compare."
        )),
    }
}

/// What `ripr init` would do to one file.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum InitAction {
    /// The path does not exist; it would be created.
    Create,
    /// The path exists and `--force` was given; it would be replaced.
    Overwrite,
    /// The path exists and is the user's own file; only ripr's entry is added
    /// or replaced and the rest of its content is kept.
    Update,
    /// The config exists and `--ci` has work to do, so the config is left as
    /// the user wrote it, with or without `--force`.
    LeaveUnchanged,
}

impl InitAction {
    /// Fixed-width so a multi-target plan lines up in a terminal.
    fn label(self) -> &'static str {
        match self {
            Self::Create => "create        ",
            Self::Overwrite => "overwrite     ",
            Self::Update => "update        ",
            Self::LeaveUnchanged => "leave existing",
        }
    }
}

/// One file in the resolved plan, with the body that would be written.
struct InitTarget {
    path: PathBuf,
    action: InitAction,
    body: String,
}

/// Resolve what the run would do, or fail with the reason it cannot run.
///
/// This is the single authority for `ripr init` preconditions: both
/// `--dry-run` and the real run go through it, so the two always agree on
/// whether the run is possible and on which files it touches.
fn init_plan(options: &InitOptions) -> Result<Vec<InitTarget>, String> {
    if !options.root.is_dir() {
        return Err(format!(
            "init root {} is not a directory; pass the directory that contains the workspace (for a Cargo.toml path, its parent directory)",
            options.root.display()
        ));
    }
    let config_path = options.root.join(CONFIG_FILE_NAME);
    let workflow_path = options
        .ci
        .as_ref()
        .map(|ci| init_ci_workflow_path(&options.root, ci));

    // #2576 review: the plan must reject a parent the real run cannot create,
    // not just a target that already exists. If `<root>/.github` is a regular
    // file, nothing exists at `.github/workflows/ripr.yml`, so the target reads
    // as `create` while `create_dir_all` will fail — and because the config is
    // written first, the run would half-initialize the repo before failing.
    // Checking every target up front also means a doomed run writes nothing.
    let mcp_path = options.mcp.then(|| options.root.join(MCP_CONFIG_FILE_NAME));

    for path in std::iter::once(&config_path)
        .chain(workflow_path.as_ref())
        .chain(mcp_path.as_ref())
    {
        ensure_creatable_parent(path)?;
    }

    // `--ci` and `--mcp` add files; they keep an existing ripr.toml rather
    // than refusing because it is already there.
    let config_only = options.ci.is_none() && !options.mcp;
    if path_is_occupied(&config_path)? && !options.force && config_only {
        return Err(existing_target_error(&config_path, &options.root, None));
    }
    if let Some(path) = workflow_path.as_ref().filter(|_| !options.force)
        && path_is_occupied(path)?
    {
        return Err(existing_target_error(
            path,
            &options.root,
            options.ci.as_ref(),
        ));
    }

    // With `--ci`, `--force` only lets the workflow be replaced. Refreshing a
    // workflow after an upgrade (`ripr init --ci github --force`, which
    // `ripr doctor` recommends) must not reset a customized `ripr.toml`;
    // `ripr init --force` without `--ci` still resets the config.
    let config_action = if path_is_occupied(&config_path)? {
        if options.force && config_only {
            InitAction::Overwrite
        } else {
            InitAction::LeaveUnchanged
        }
    } else {
        InitAction::Create
    };
    let mut targets = vec![InitTarget {
        path: config_path,
        action: config_action,
        body: generated_init_config().to_string(),
    }];
    if let Some(path) = workflow_path {
        let action = if path_is_occupied(&path)? {
            InitAction::Overwrite
        } else {
            InitAction::Create
        };
        targets.push(InitTarget {
            path,
            action,
            body: generated_github_actions_workflow(),
        });
    }
    if let Some(path) = mcp_path {
        targets.push(plan_mcp_target(path, options.force)?);
    }
    Ok(targets)
}

/// Project-scoped MCP client config that Claude Code reads from the repository
/// root. Codex keeps its servers in user-global config, which `init` never
/// writes; the follow-up note prints its command instead.
const MCP_CONFIG_FILE_NAME: &str = ".mcp.json";
const MCP_SERVER_NAME: &str = "ripr";

fn ripr_mcp_server_entry() -> serde_json::Value {
    serde_json::json!({ "command": "ripr", "args": ["mcp", "--stdio"] })
}

/// Plan the `.mcp.json` write: create it, add the `ripr` server beside the
/// user's other servers, or leave an existing `ripr` entry alone unless
/// `--force`. A file that is not a JSON object with an object `mcpServers`
/// fails closed rather than being rewritten.
fn plan_mcp_target(path: PathBuf, force: bool) -> Result<InitTarget, String> {
    let shown = output::path::human_path(&path);
    let render = |document: &serde_json::Value| {
        let mut body = serde_json::to_string_pretty(document)
            .map_err(|err| format!("render {shown} failed: {err}"))?;
        body.push('\n');
        Ok::<String, String>(body)
    };
    if !path_is_occupied(&path)? {
        let document = serde_json::json!({
            "mcpServers": { MCP_SERVER_NAME: ripr_mcp_server_entry() }
        });
        return Ok(InitTarget {
            path,
            action: InitAction::Create,
            body: render(&document)?,
        });
    }
    // The atomic write replaces the directory entry, so a symlinked file
    // (a shared or dotfiles-managed config) would be swapped for a plain
    // copy and the real target never updated.
    if std::fs::symlink_metadata(&path)
        .map_err(|err| format!("cannot inspect {shown}: {err}"))?
        .file_type()
        .is_symlink()
    {
        return Err(format!(
            "existing {shown} is a symlink; add the `ripr` server to its target by hand, nothing was written"
        ));
    }
    let text = std::fs::read_to_string(&path)
        .map_err(|err| format!("cannot read existing {shown}: {err}"))?;
    let mut document: serde_json::Value = serde_json::from_str(&text).map_err(|err| {
        format!(
            "existing {shown} is not valid JSON ({err}); fix it or remove it, nothing was written"
        )
    })?;
    let servers = document
        .as_object_mut()
        .ok_or_else(|| format!("existing {shown} is not a JSON object; nothing was written"))?
        .entry("mcpServers")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .ok_or_else(|| {
            format!(
                "existing {shown} has a `mcpServers` that is not an object; nothing was written"
            )
        })?;
    if servers.contains_key(MCP_SERVER_NAME) && !force {
        return Ok(InitTarget {
            path,
            action: InitAction::LeaveUnchanged,
            body: text,
        });
    }
    servers.insert(MCP_SERVER_NAME.to_string(), ripr_mcp_server_entry());
    Ok(InitTarget {
        path,
        action: InitAction::Update,
        body: render(&document)?,
    })
}

/// Refusal retries must target the same directory as the refused plan, even
/// when pasted elsewhere. Share successful init's lossless root authority.
fn existing_target_error(path: &Path, root: &Path, ci: Option<&InitCi>) -> String {
    let path_display = output::path::human_path(path);
    let root_display = match init_command_root(root) {
        Ok(root) => root,
        Err(error) => {
            return format!("{path_display} already exists; recovery commands omitted: {error}.");
        }
    };
    let root_arg = crate::agent::loop_commands::shell_arg(&root_display);
    let workflow = format!("ripr init --ci github --force --root {root_arg}");
    let overwrite = format!("ripr init --force --root {root_arg}");
    let preview = format!("ripr init --force --dry-run --root {root_arg}");
    let (mut message, commands) = if ci.is_some() {
        (
            format!("{path_display} already exists; rerun `{workflow}` to overwrite it"),
            vec![&workflow],
        )
    } else {
        (
            format!(
                "{path_display} already exists; rerun `{overwrite}` to overwrite it (resets customizations). Preview with `{preview}`; `{workflow}` refreshes the workflow and keeps the config"
            ),
            vec![&overwrite, &preview, &workflow],
        )
    };
    for command in commands {
        if let Some(powershell) = output::markdown::powershell_text_variant(command) {
            message.push_str(&format!("\n(PowerShell) {powershell}"));
        }
    }
    message
}

/// Preserve lossless aliases and filesystem traversal; only resolve the
/// physical path if the supplied absolute spelling cannot be represented.
fn init_command_root(root: &Path) -> Result<String, String> {
    output::path::absolute_command_root_display(root).or_else(|error| {
        let resolved = root.canonicalize().map_err(|resolve_error| {
            format!("{error}; cannot resolve selected root: {resolve_error}")
        })?;
        output::path::command_root_display(root, &resolved)
    })
}

/// Is anything at all sitting at `path`?
///
/// This deliberately does not use `Path::exists()`, which follows symlinks and
/// so reports `false` for a dangling symlink. The write path opens with
/// `create_new`, which fails when *any* entry occupies the path — including a
/// dangling symlink — so planning has to ask the same question the write asks.
/// `symlink_metadata` also surfaces permission errors instead of silently
/// reading as "absent, will create".
fn path_is_occupied(path: &Path) -> Result<bool, String> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(err) => Err(format!("cannot inspect {}: {err}", path.display())),
    }
}

/// Fail when a target's parent directory cannot be created.
///
/// `create_dir_all` fails if an existing ancestor is not a directory, so the
/// nearest existing ancestor decides whether the write is possible at all.
/// Ancestors are followed through symlinks, matching what `create_dir_all`
/// itself does.
fn ensure_creatable_parent(path: &Path) -> Result<(), String> {
    let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    else {
        return Ok(());
    };
    for ancestor in parent.ancestors() {
        match std::fs::metadata(ancestor) {
            Ok(metadata) if metadata.is_dir() => return Ok(()),
            Ok(_) => {
                return Err(format!(
                    "cannot write {}: {} exists and is not a directory",
                    path.display(),
                    ancestor.display()
                ));
            }
            // `NotFound` means this level would simply be created. `NotADirectory`
            // means a *shallower* ancestor is the real culprit, so keep walking
            // up until the offending entry itself is found and can be named.
            Err(err)
                if matches!(
                    err.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
                ) =>
            {
                continue;
            }
            Err(err) => {
                return Err(format!(
                    "cannot write {}: inspecting {} failed: {err}",
                    path.display(),
                    ancestor.display()
                ));
            }
        }
    }
    Ok(())
}

fn apply_init_plan(plan: &[InitTarget], root: &Path) -> Result<(), String> {
    let mut wrote_any = false;
    for target in plan {
        match target.action {
            InitAction::LeaveUnchanged => {
                println!(
                    "Left existing {} unchanged",
                    output::path::human_path(&target.path)
                );
            }
            InitAction::Create => {
                write_init_target(target)?;
                wrote_any = true;
            }
            InitAction::Update => {
                write_init_target(target)?;
                println!(
                    "Updated existing {} (your other content is kept)",
                    output::path::human_path(&target.path)
                );
                wrote_any = true;
            }
            InitAction::Overwrite => {
                write_init_target(target)?;
                println!(
                    "Overwrote existing {}",
                    output::path::human_path(&target.path)
                );
                wrote_any = true;
            }
        }
    }
    if wrote_any {
        // #7830: a UTF-8 argument can select a non-UTF-8 cwd. Never render
        // a lossy root as an executable command for a different repository.
        let root_display = match init_command_root(root) {
            Ok(root) => root,
            Err(error) => {
                println!();
                println!("Init completed; follow-up commands omitted: {error}.");
                return Ok(());
            }
        };
        let root_arg = crate::agent::loop_commands::shell_arg(&root_display);
        let doctor = format!("ripr doctor --root {root_arg}");
        let check = format!("ripr check --root {root_arg}");
        if plan.iter().any(|target| {
            target
                .path
                .file_name()
                .is_some_and(|name| name == MCP_CONFIG_FILE_NAME)
        }) {
            println!();
            println!(
                "Claude Code reads `{MCP_CONFIG_FILE_NAME}` and asks you to approve the project server before its tools appear. For Codex run `codex mcp add {MCP_SERVER_NAME} -- ripr mcp --stdio`."
            );
        }
        if let Some(workflow) = plan
            .iter()
            .find(|target| target.path == init_ci_workflow_path(root, &InitCi::Github))
        {
            println!();
            println!(
                "CI next: review `{}` (advisory by default; blocking gates are opt-in via the RIPR_GATE_MODE repository variable), commit the generated files, then open or update a pull request and read the workflow's job summary.",
                output::path::human_path(&workflow.path)
            );
        }
        println!();
        println!(
            "Next: run `{doctor}` to verify your setup, then `{check}` to analyze your branch against its default branch."
        );
        for command in [&doctor, &check] {
            if let Some(powershell) = output::markdown::powershell_text_variant(command) {
                println!("(PowerShell) {powershell}");
            }
        }
    }
    Ok(())
}

fn write_init_target(target: &InitTarget) -> Result<(), String> {
    if let Some(parent) = target
        .path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)
            .map_err(|err| format!("create {} failed: {err}", parent.display()))?;
    }
    if matches!(target.action, InitAction::Overwrite | InitAction::Update) {
        // --force replaces the config atomically (#4883): the old file stays
        // until a complete, fsynced temporary file is renamed over it, so a
        // failed write (a full disk, a size limit) never leaves the user
        // without their config. The rename replaces the directory entry
        // itself, so a pre-placed symlink is not followed (#2101).
        return crate::atomic_file::write(&target.path, target.body.as_bytes(), "ripr init");
    }
    // A new file is staged and fsynced beside the destination, then
    // hard-linked into place (#4883). The link fails if anything, including
    // a planted or dangling symlink, appeared at the path after planning
    // (#1948, #2101), and a failed write never leaves a partial config at the
    // destination for a rerun to refuse.
    use crate::atomic_file::CreateNewError;
    match crate::atomic_file::create_new(&target.path, target.body.as_bytes()) {
        Ok(()) => {}
        // A filesystem without hard links: write the destination directly.
        Err(CreateNewError::Link(err)) if err.kind() != std::io::ErrorKind::AlreadyExists => {
            write_new_file_in_place(&target.path, target.body.as_bytes())?;
        }
        Err(CreateNewError::Staging(err) | CreateNewError::Link(err)) => {
            return Err(format!("write {} failed: {err}", target.path.display()));
        }
    }
    println!("Wrote {}", output::path::human_path(&target.path));
    Ok(())
}

/// Direct `create_new` write, for filesystems where the staged hard link is
/// unavailable. Writing through the `create_new` handle avoids a swap window
/// where a planted symlink is followed (#2101 review, CWE-367).
fn write_new_file_in_place(path: &std::path::Path, body: &[u8]) -> Result<(), String> {
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|err| format!("write {} failed: {err}", path.display()))?;
    if let Err(err) = file.write_all(body).and_then(|()| file.sync_all()) {
        // Remove the file this call created rather than leave a truncated
        // config (#4883). Unlike the staged path, this unlinks by name.
        drop(file);
        return Err(match std::fs::remove_file(path) {
            Ok(()) => format!("write {} failed: {err}", path.display()),
            Err(cleanup) => format!(
                "write {} failed: {err}; removing the partial file also failed: {cleanup}",
                path.display()
            ),
        });
    }
    Ok(())
}

pub(super) fn parse_init_options(args: &[String]) -> Result<InitOptions, String> {
    let mut options = InitOptions {
        root: PathBuf::from("."),
        dry_run: false,
        force: false,
        ci: None,
        mcp: false,
    };
    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--root" => {
                i += 1;
                options.root = PathBuf::from(expect_value(args, i, "--root")?);
            }
            "--ci" => {
                i += 1;
                options.ci = Some(parse_init_ci(expect_value(args, i, "--ci")?)?);
            }
            "--mcp" => options.mcp = true,
            "--dry-run" => options.dry_run = true,
            "--force" => options.force = true,
            other => return Err(unknown_argument("init", other)),
        }
        i += 1;
    }
    Ok(options)
}

fn parse_init_ci(value: &str) -> Result<InitCi, String> {
    match value {
        "github" => Ok(InitCi::Github),
        _ => Err(unknown_value("init --ci provider", value, &["github"])),
    }
}

/// Render the resolved plan, then the body of each file that would be written.
///
/// The plan block goes first so the reader learns which paths are involved and
/// what would happen to each before scrolling through a multi-hundred-line
/// generated workflow. Previously this printed bodies only, so `--dry-run`
/// never named its targets or said that nothing had been written.
fn print_init_dry_run(plan: &[InitTarget]) {
    print!("{}", render_init_dry_run(plan, cfg!(windows)));
}

/// The dry-run text, pure over the host separator rule so the Windows
/// rendering is testable anywhere (#4378): a forward-slash `--root` argv
/// prefix and the joined `\ripr.toml` suffix render as one slash path.
fn render_init_dry_run(plan: &[InitTarget], windows: bool) -> String {
    let path = |target: &InitTarget| {
        output::path::human_path_text(&target.path.to_string_lossy(), windows)
    };
    let mut text = String::from("ripr init plan (dry run — nothing was written)\n");
    for target in plan {
        text.push_str(&format!("  {} {}\n", target.action.label(), path(target)));
    }
    for target in plan {
        if target.action == InitAction::LeaveUnchanged {
            continue;
        }
        text.push_str(&format!("\n# {}\n", path(target)));
        text.push_str(&target.body);
    }
    text.push_str("\nRerun without --dry-run to apply.\n");
    text
}

fn init_ci_workflow_path(root: &Path, ci: &InitCi) -> PathBuf {
    match ci {
        InitCi::Github => root.join(".github/workflows/ripr.yml"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::commands_options::InitCi;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// #4378: on Windows `ripr init --dry-run --root <drive>:/Temp/demo`
    /// printed `<drive>:/Temp/demo\ripr.toml`, two separator styles in one
    /// path. The preview uses one convention per host and never rewrites a
    /// Unix filename's backslash.
    #[test]
    fn dry_run_preview_uses_one_separator_convention_per_host() {
        // `check-local-context` forbids drive-letter path literals, so the
        // Windows shape is assembled from parts.
        let root = format!("{}/Temp/demo", "F:");
        let plan = vec![InitTarget {
            path: PathBuf::from(format!(r"{root}\ripr.toml")),
            action: InitAction::Create,
            body: "[analysis]\n".to_string(),
        }];
        let windows = render_init_dry_run(&plan, true);
        assert!(
            windows.contains(&format!("  create         {root}/ripr.toml\n"))
                && windows.contains(&format!("# {root}/ripr.toml\n")),
            "{windows}"
        );
        assert!(!windows.contains('\\'), "{windows}");
        let unix = render_init_dry_run(&plan, false);
        assert!(unix.contains(&format!(r"# {root}\ripr.toml")), "{unix}");
        assert!(
            unix.ends_with("\nRerun without --dry-run to apply.\n"),
            "{unix}"
        );
    }

    /// #4391: the steps use bash-only syntax, so the job pins `shell: bash`
    /// instead of inheriting a runner default (PowerShell on Windows).
    #[test]
    fn generated_workflow_pins_bash_for_every_run_step() {
        let workflow = generated_github_actions_workflow();
        let defaults_at = workflow
            .find("\ndefaults:\n  run:\n    shell: bash\n")
            .unwrap_or(usize::MAX);
        let jobs_at = workflow.find("\njobs:\n").unwrap_or(usize::MAX);
        assert!(
            defaults_at < jobs_at && jobs_at != usize::MAX,
            "the workflow must pin bash for every job:\n{workflow}"
        );
        assert!(
            workflow.contains("IFS=$'\\t'"),
            "bash-only syntax (ANSI-C quoting) the pin protects"
        );
    }

    /// Every render-time placeholder is substituted; the shared proof-path
    /// labels (#3906) now render in `ripr reports ci-summary`, not the
    /// workflow text.
    #[test]
    fn generated_workflow_substitutes_every_placeholder() {
        let workflow = generated_github_actions_workflow();
        assert!(!workflow.contains("@RIPR_"), "unsubstituted placeholder");
    }

    /// #4726: the gate reads PR labels from `$GITHUB_EVENT_PATH`, so a run
    /// only sees a waiver label that exists when its event fires. Without
    /// `labeled`/`unlabeled` triggers, adding `ripr-waive` never re-evaluates
    /// an `acknowledgeable` gate and removing it leaves a stale green.
    #[test]
    fn generated_workflow_reruns_when_pull_request_labels_change() -> Result<(), String> {
        let workflow = generated_github_actions_workflow();
        assert!(
            include_str!("ci_packet.rs").contains("event.pointer(\"/pull_request/labels\")"),
            "labels are no longer read from the event payload; revisit #4726"
        );
        let on_block: Vec<&str> = workflow
            .lines()
            .skip_while(|line| *line != "on:")
            .skip(1)
            .take_while(|line| line.starts_with(' ') || line.is_empty())
            .collect();
        let pull_request = on_block
            .iter()
            .position(|line| *line == "  pull_request:")
            .ok_or_else(|| format!("no pull_request trigger in {on_block:?}"))?;
        let types = on_block
            .iter()
            .skip(pull_request + 1)
            .take_while(|line| line.starts_with("    "))
            .find_map(|line| line.trim().strip_prefix("types:"))
            .ok_or_else(|| format!("pull_request trigger has no types: {on_block:?}"))?;
        let types: Vec<&str> = types
            .trim()
            .trim_start_matches('[')
            .trim_end_matches(']')
            .split(',')
            .map(str::trim)
            .collect();
        assert_eq!(
            types,
            ["opened", "synchronize", "reopened", "labeled", "unlabeled"]
        );
        Ok(())
    }

    /// W5: an unpinned install takes whatever release is latest, so CI
    /// could run an older ripr that lacks the commands this workflow calls,
    /// or change behavior silently on a later release. Both install routes,
    /// the prebuilt release download and the `cargo install` fallback, pin
    /// an exact version; the fallback keeps `--locked`. #5208: released
    /// generators pin themselves; unreleased generators pin the latest
    /// release (their own version would neither download nor install),
    /// with a stderr warning at the `init` call site.
    #[test]
    fn generated_workflow_pins_a_resolvable_ripr_version() {
        let workflow = generated_github_actions_workflow();
        let version = workflow_install_version(env!("CARGO_PKG_VERSION"));
        let download = format!("          version={version}\n");
        assert!(workflow.contains(&download), "missing {download}");
        let pinned = format!("cargo install ripr --version {version} --locked");
        assert!(workflow.contains(&pinned), "missing {pinned}");
        let installs: Vec<&str> = workflow
            .lines()
            .filter(|line| line.contains("cargo install ripr"))
            .filter(|line| !line.trim_start().starts_with('#'))
            .collect();
        assert_eq!(installs, [format!("            {pinned}")]);
    }

    /// The install downloads the release archive for the runner and refuses
    /// a mismatched checksum instead of falling back to a compile; only a
    /// runner with no archive, or a failed download, takes `cargo install`.
    #[test]
    fn generated_workflow_installs_the_checksummed_release_binary() {
        let workflow = generated_github_actions_workflow();
        let install = workflow
            .split("\n\n")
            .find(|block| block.contains("      - name: Install ripr\n"))
            .unwrap_or_default();
        for needle in [
            "Linux-X64) target=x86_64-unknown-linux-gnu ;;",
            "macOS-ARM64) target=aarch64-apple-darwin ;;",
            r#"asset="ripr-server-v$version-$target.tar.gz""#,
            r#"url="https://github.com/EffortlessMetrics/ripr/releases/download/v$version/$asset""#,
            r#"curl -fsSL --retry 3 -O "$url.sha256"; then"#,
            r#"if [ -z "$expected" ] || [ "$expected" != "$actual" ]; then"#,
            r#"echo "$RUNNER_TEMP/ripr-bin" >> "$GITHUB_PATH""#,
        ] {
            assert!(install.contains(needle), "install step missing {needle}");
        }
        // The mismatch branch exits before the `else` that compiles.
        let mismatch = install.find("does not match its published SHA-256");
        let exit = install.find("exit 1");
        let fallback = install.find("cargo install ripr");
        assert!(
            mismatch < exit && exit < fallback && mismatch.is_some(),
            "a checksum mismatch must fail before the cargo fallback"
        );
    }

    /// The `cli_smoke` tests drive `ripr init` as a subprocess, so they prove
    /// end-to-end behavior but leave the planning logic uninstrumented. These
    /// in-process tests exercise `init_plan` and its precondition helpers
    /// directly, which is also where the interesting branches live.
    fn temp_root(name: &str) -> Result<PathBuf, String> {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let root =
            std::env::temp_dir().join(format!("ripr-init-{name}-{}-{stamp}", std::process::id()));
        std::fs::create_dir_all(&root).map_err(|err| format!("create temp root failed: {err}"))?;
        Ok(root)
    }

    fn write_package_manifest(path: &Path, name: &str) -> Result<(), String> {
        write(
            path,
            &format!("[package]\nname = \"{name}\"\nversion = \"0.0.0\"\n"),
        )
    }

    fn git_init_fixture(root: &Path) -> Result<(), String> {
        crate::git::run_git(root, &["init", "-q"])
            .map_err(|err| format!("git init failed: {err}"))?;
        Ok(())
    }

    fn workspace_with_member(label: &str) -> Result<(PathBuf, PathBuf), String> {
        let workspace = temp_root(label)?;
        git_init_fixture(&workspace)?;
        write(
            &workspace.join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/alpha\"]\n",
        )?;
        let member = workspace.join("crates/alpha");
        std::fs::create_dir_all(&member).map_err(|err| format!("mkdir member: {err}"))?;
        write_package_manifest(&member.join("Cargo.toml"), "alpha")?;
        Ok((workspace, member))
    }

    /// #7465: `init` from a workspace member writes `./ripr.toml`, but
    /// implicit `check` rebinds to the workspace and never loads that file.
    #[test]
    fn check_ignores_init_config_warning_names_a_workspace_member() -> Result<(), String> {
        let (workspace, member) = workspace_with_member("ws-member")?;
        let warning = check_ignores_init_config_warning(&member).ok_or_else(|| {
            "init from a workspace member must warn that check will ignore ./ripr.toml".to_string()
        })?;
        let shown_workspace = output::path::human_path(&workspace);
        let shown_config = output::path::human_path(&member.join(CONFIG_FILE_NAME));
        let quoted_workspace = crate::agent::loop_commands::shell_arg(&shown_workspace);
        let quoted_member =
            crate::agent::loop_commands::shell_arg(&output::path::human_path(&member));
        assert!(
            warning.contains("resolves to the workspace root"),
            "{warning}"
        );
        assert!(warning.contains(&shown_workspace), "{warning}");
        assert!(warning.contains("will ignore"), "{warning}");
        assert!(warning.contains(&shown_config), "{warning}");
        assert!(
            warning.contains(&format!("ripr init --root {quoted_workspace}`")),
            "{warning}"
        );
        assert!(
            warning.contains(&format!("ripr check --root {quoted_member}")),
            "{warning}"
        );
        std::fs::remove_dir_all(&workspace).map_err(|err| format!("cleanup: {err}"))?;
        Ok(())
    }

    /// Copy-paste recovery must keep a space in `--root` as one argument
    /// (#5188 convention via `shell_arg`).
    #[test]
    fn check_ignores_init_config_warning_quotes_spaced_recovery_roots() -> Result<(), String> {
        let (workspace, member) = workspace_with_member("spaced ws")?;
        assert!(
            workspace
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.contains(' ')),
            "fixture name must contain a space: {}",
            workspace.display()
        );
        let warning = check_ignores_init_config_warning(&member).ok_or_else(|| {
            "a spaced workspace member must still warn that check will ignore ./ripr.toml"
                .to_string()
        })?;
        let quoted_workspace =
            crate::agent::loop_commands::shell_arg(&output::path::human_path(&workspace));
        let quoted_member =
            crate::agent::loop_commands::shell_arg(&output::path::human_path(&member));
        // Host-aware quote character (#7729): POSIX single quotes off
        // Windows, the Windows console double-quote form on it. The
        // assertion's point is that the spaced path is quoted at all.
        let quote = if cfg!(windows) { '"' } else { '\'' };
        assert!(quoted_workspace.starts_with(quote), "{quoted_workspace}");
        assert!(quoted_member.starts_with(quote), "{quoted_member}");
        assert!(
            warning.contains(&format!("ripr init --root {quoted_workspace}`")),
            "{warning}"
        );
        assert!(
            warning.contains(&format!("ripr check --root {quoted_member}")),
            "{warning}"
        );
        std::fs::remove_dir_all(&workspace).map_err(|err| format!("cleanup: {err}"))?;
        Ok(())
    }

    /// When the workspace already has `ripr.toml`, a bare `ripr init --root`
    /// fails without `--force`. Name the loaded file instead of advertising
    /// a command that cannot run.
    #[test]
    fn check_ignores_init_config_warning_names_an_existing_workspace_config() -> Result<(), String>
    {
        let (workspace, member) = workspace_with_member("ws-existing-config")?;
        write(&workspace.join(CONFIG_FILE_NAME), "[analysis]\n")?;
        let warning = check_ignores_init_config_warning(&member).ok_or_else(|| {
            "a member init must still warn when the workspace already has ripr.toml".to_string()
        })?;
        let shown_workspace_config = output::path::human_path(&workspace.join(CONFIG_FILE_NAME));
        let quoted_workspace =
            crate::agent::loop_commands::shell_arg(&output::path::human_path(&workspace));
        assert!(warning.contains("will ignore"), "{warning}");
        assert!(warning.contains(&shown_workspace_config), "{warning}");
        assert!(warning.contains("review or edit that file"), "{warning}");
        assert!(
            warning.contains(&format!("ripr init --root {quoted_workspace} --force")),
            "{warning}"
        );
        assert!(warning.contains("resets customizations"), "{warning}");
        assert!(
            !warning.contains(&format!("Re-run `ripr init --root {quoted_workspace}`")),
            "a doomed no-force init must not be the recovery: {warning}"
        );
        std::fs::remove_dir_all(&workspace).map_err(|err| format!("cleanup: {err}"))?;
        Ok(())
    }

    /// Init at the workspace root writes the file implicit `check` loads.
    #[test]
    fn check_ignores_init_config_warning_stays_quiet_at_the_workspace_root() -> Result<(), String> {
        let (workspace, _member) = workspace_with_member("ws-root")?;
        assert_eq!(check_ignores_init_config_warning(&workspace), None);
        std::fs::remove_dir_all(&workspace).map_err(|err| format!("cleanup: {err}"))?;
        Ok(())
    }

    /// A standalone crate is both the init root and the implicit check root.
    /// Git-init the fixture so the walk cannot adopt an enclosing workspace
    /// (this checkout's `[workspace]` sits above `temp_dir()`).
    #[test]
    fn check_ignores_init_config_warning_stays_quiet_for_a_standalone_crate() -> Result<(), String>
    {
        let root = temp_root("solo-crate")?;
        git_init_fixture(&root)?;
        write_package_manifest(&root.join("Cargo.toml"), "solo")?;
        assert_eq!(check_ignores_init_config_warning(&root), None);
        std::fs::remove_dir_all(&root).map_err(|err| format!("cleanup: {err}"))?;
        Ok(())
    }

    #[test]
    fn unanalyzed_root_warning_names_go_only_repositories() -> Result<(), String> {
        let root = temp_root("go-only")?;
        std::fs::write(root.join("main.go"), "package main\n")
            .map_err(|err| format!("write main.go: {err}"))?;
        let warning = unanalyzed_root_warning(&root).unwrap_or_default();
        assert!(warning.contains("Go (1 file(s))"), "{warning}");
        assert!(
            warning.contains("report their changes as not analyzed"),
            "{warning}"
        );
        // Negative control: Rust source beside the Go file is analyzable.
        std::fs::create_dir_all(root.join("src")).map_err(|err| format!("mkdir: {err}"))?;
        std::fs::write(root.join("src/lib.rs"), "pub fn f() {}\n")
            .map_err(|err| format!("write lib.rs: {err}"))?;
        assert_eq!(unanalyzed_root_warning(&root), None);
        std::fs::remove_dir_all(&root).map_err(|err| format!("cleanup: {err}"))?;
        Ok(())
    }

    /// #5252 item 7: a real repository stays quiet. (The warns half cannot
    /// be pinned at unit level: the unit-test temp root sits inside the
    /// checkout's work tree under test, so only the smoke test's
    /// outside-the-checkout fixture with its git-pinned precondition can
    /// carry it.)
    #[test]
    fn non_git_root_warning_stays_quiet_inside_a_work_tree() -> Result<(), String> {
        let root = temp_root("git-quiet")?;
        // The shared git runner, not a raw `Command`: the process policy
        // counts spawns per file, test code included.
        crate::git::run_git(&root, &["init", "-q"])
            .map_err(|err| format!("git init failed: {err}"))?;
        assert_eq!(non_git_root_warning(&root), None);
        std::fs::remove_dir_all(&root).map_err(|err| format!("cleanup: {err}"))?;
        Ok(())
    }

    #[test]
    fn non_git_root_warning_names_only_the_established_outside() {
        use output::doctor::WorkTreeProbe;
        let root = Path::new("some-dir");

        let outside =
            non_git_root_warning_for_probe(root, Some(WorkTreeProbe::Outside)).unwrap_or_default();
        assert!(
            outside.contains("is not inside a Git work tree"),
            "{outside}"
        );

        // A refused repository keeps its repair; it is never told it lies
        // outside a repository.
        let refused = non_git_root_warning_for_probe(
            root,
            Some(WorkTreeProbe::Refused(
                "run `git config --global --add safe.directory /srv/repo`".to_string(),
            )),
        )
        .unwrap_or_default();
        assert!(refused.contains("could not confirm"), "{refused}");
        assert!(refused.contains("safe.directory"), "{refused}");
        assert!(
            !refused.contains("is not inside a Git work tree"),
            "{refused}"
        );

        // A probe that never answered says so instead of claiming a state.
        let silent = non_git_root_warning_for_probe(root, None).unwrap_or_default();
        assert!(silent.contains("could not confirm"), "{silent}");
        assert!(silent.contains("did not answer"), "{silent}");
        assert!(
            !silent.contains("is not inside a Git work tree"),
            "{silent}"
        );

        assert_eq!(
            non_git_root_warning_for_probe(root, Some(WorkTreeProbe::Inside)),
            None
        );
    }

    fn options(root: &Path) -> InitOptions {
        InitOptions {
            root: root.to_path_buf(),
            dry_run: false,
            force: false,
            ci: None,
            mcp: false,
        }
    }

    fn write(path: &Path, text: &str) -> Result<(), String> {
        std::fs::write(path, text).map_err(|err| format!("write {} failed: {err}", path.display()))
    }

    fn mcp_options(root: &Path, force: bool) -> InitOptions {
        InitOptions {
            mcp: true,
            force,
            ..options(root)
        }
    }

    fn mcp_target(plan: &[InitTarget]) -> Result<&InitTarget, String> {
        plan.iter()
            .find(|target| target.path.ends_with(".mcp.json"))
            .ok_or_else(|| "plan has no .mcp.json target".to_string())
    }

    fn parsed(body: &str) -> Result<serde_json::Value, String> {
        serde_json::from_str(body).map_err(|err| format!("body is not JSON: {err}\n{body}"))
    }

    const RIPR_ENTRY: &str = r#"{"command":"ripr","args":["mcp","--stdio"]}"#;

    #[test]
    fn mcp_plan_creates_project_config_and_keeps_an_existing_ripr_toml() -> Result<(), String> {
        let root = temp_root("mcp-create")?;
        write(&root.join("ripr.toml"), "[analysis]\n")?;
        let plan = init_plan(&mcp_options(&root, false))?;
        let target = mcp_target(&plan)?;
        assert_eq!(target.action, InitAction::Create);
        assert_eq!(
            parsed(&target.body)?,
            serde_json::json!({ "mcpServers": { "ripr": parsed(RIPR_ENTRY)? } })
        );
        assert!(target.body.ends_with("}\n"), "{}", target.body);
        let config = plan
            .iter()
            .find(|target| target.path.ends_with("ripr.toml"))
            .ok_or("plan has no ripr.toml target")?;
        assert_eq!(config.action, InitAction::LeaveUnchanged);
        std::fs::remove_dir_all(&root).map_err(|err| format!("cleanup: {err}"))
    }

    #[test]
    fn mcp_plan_adds_ripr_beside_other_servers() -> Result<(), String> {
        let root = temp_root("mcp-merge")?;
        write(
            &root.join(".mcp.json"),
            r#"{"mcpServers":{"other":{"command":"x"}},"extra":1}"#,
        )?;
        let plan = init_plan(&mcp_options(&root, false))?;
        let target = mcp_target(&plan)?;
        assert_eq!(target.action, InitAction::Update);
        assert_eq!(
            parsed(&target.body)?,
            serde_json::json!({
                "mcpServers": { "other": { "command": "x" }, "ripr": parsed(RIPR_ENTRY)? },
                "extra": 1
            })
        );
        std::fs::remove_dir_all(&root).map_err(|err| format!("cleanup: {err}"))
    }

    #[test]
    fn mcp_plan_keeps_a_customized_ripr_entry_unless_forced() -> Result<(), String> {
        let root = temp_root("mcp-existing")?;
        let custom = r#"{"mcpServers":{"ripr":{"command":"/opt/ripr","args":["mcp"]}}}"#;
        write(&root.join(".mcp.json"), custom)?;
        let kept = init_plan(&mcp_options(&root, false))?;
        assert_eq!(mcp_target(&kept)?.action, InitAction::LeaveUnchanged);
        let forced = init_plan(&mcp_options(&root, true))?;
        let target = mcp_target(&forced)?;
        assert_eq!(target.action, InitAction::Update);
        assert_eq!(
            parsed(&target.body)?,
            serde_json::json!({ "mcpServers": { "ripr": parsed(RIPR_ENTRY)? } })
        );
        std::fs::remove_dir_all(&root).map_err(|err| format!("cleanup: {err}"))
    }

    #[test]
    fn mcp_plan_fails_closed_on_unusable_existing_config() -> Result<(), String> {
        let root = temp_root("mcp-malformed")?;
        for (text, needle) in [
            ("{not json", "not valid JSON"),
            ("[]", "not a JSON object"),
            (r#"{"mcpServers":[]}"#, "`mcpServers` that is not an object"),
        ] {
            write(&root.join(".mcp.json"), text)?;
            let err = init_plan(&mcp_options(&root, true))
                .err()
                .ok_or_else(|| format!("{text} should be refused"))?;
            assert!(err.contains(needle), "{text}: {err}");
            assert_eq!(
                std::fs::read_to_string(root.join(".mcp.json")).map_err(|e| e.to_string())?,
                text,
                "planning must not touch the file"
            );
        }
        std::fs::remove_dir_all(&root).map_err(|err| format!("cleanup: {err}"))
    }

    #[test]
    fn mcp_apply_merges_on_disk_and_is_idempotent() -> Result<(), String> {
        let root = temp_root("mcp-apply")?;
        let path = root.join(".mcp.json");
        write(&path, r#"{"mcpServers":{"other":{"command":"x"}}}"#)?;
        let plan = init_plan(&mcp_options(&root, false))?;
        apply_init_plan(&plan, &root)?;
        let merged = std::fs::read_to_string(&path).map_err(|err| err.to_string())?;
        assert_eq!(
            parsed(&merged)?,
            serde_json::json!({
                "mcpServers": { "other": { "command": "x" }, "ripr": parsed(RIPR_ENTRY)? }
            })
        );
        let again = init_plan(&mcp_options(&root, false))?;
        assert_eq!(mcp_target(&again)?.action, InitAction::LeaveUnchanged);
        apply_init_plan(&again, &root)?;
        assert_eq!(
            std::fs::read_to_string(&path).map_err(|err| err.to_string())?,
            merged,
            "a rerun must not change the file"
        );
        std::fs::remove_dir_all(&root).map_err(|err| format!("cleanup: {err}"))
    }

    #[cfg(unix)]
    #[test]
    fn mcp_plan_refuses_a_symlinked_config() -> Result<(), String> {
        let root = temp_root("mcp-symlink")?;
        let real = root.join("shared.json");
        write(&real, "{}")?;
        std::os::unix::fs::symlink(&real, root.join(".mcp.json")).map_err(|e| e.to_string())?;
        let err = init_plan(&mcp_options(&root, true))
            .err()
            .ok_or("a symlinked .mcp.json must be refused")?;
        assert!(err.contains("is a symlink"), "{err}");
        assert_eq!(
            std::fs::read_to_string(&real).map_err(|e| e.to_string())?,
            "{}"
        );
        std::fs::remove_dir_all(&root).map_err(|err| format!("cleanup: {err}"))
    }

    #[test]
    fn plan_creates_the_config_in_a_clean_root() -> Result<(), String> {
        let root = temp_root("clean")?;
        let plan = init_plan(&options(&root))?;

        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].action, InitAction::Create);
        assert_eq!(plan[0].path, root.join(CONFIG_FILE_NAME));
        assert!(plan[0].body.contains("[analysis]"));

        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn plan_adds_the_workflow_target_for_ci_github() -> Result<(), String> {
        let root = temp_root("ci")?;
        let mut opts = options(&root);
        opts.ci = Some(InitCi::Github);
        let plan = init_plan(&opts)?;

        assert_eq!(plan.len(), 2);
        assert_eq!(plan[1].action, InitAction::Create);
        assert_eq!(plan[1].path, root.join(".github/workflows/ripr.yml"));
        assert!(plan[1].body.contains("name: RIPR"));

        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn plan_rejects_an_existing_config_without_force() -> Result<(), String> {
        let root = temp_root("exists")?;
        write(&root.join(CONFIG_FILE_NAME), "[analysis]\n")?;

        match init_plan(&options(&root)) {
            Ok(_) => return Err("an existing config without --force must block".to_string()),
            Err(message) => {
                assert!(message.contains("already exists"), "{message}");
                assert!(message.contains("ripr init --force"), "{message}");
                assert!(message.contains("resets customizations"), "{message}");
                assert!(message.contains("--force --dry-run"), "{message}");
                assert!(
                    message.contains("ripr init --ci github --force"),
                    "{message}"
                );
                assert!(message.contains("keeps the config"), "{message}");
            }
        }

        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn plan_overwrites_an_existing_config_with_force() -> Result<(), String> {
        let root = temp_root("force")?;
        write(&root.join(CONFIG_FILE_NAME), "[analysis]\n")?;
        let mut opts = options(&root);
        opts.force = true;

        let plan = init_plan(&opts)?;
        assert_eq!(plan[0].action, InitAction::Overwrite);

        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    /// An existing config only blocks when there is nothing else to do; with
    /// `--ci` the run still has a workflow to write.
    #[test]
    fn plan_leaves_an_existing_config_alone_when_ci_still_has_work() -> Result<(), String> {
        let root = temp_root("leave")?;
        write(&root.join(CONFIG_FILE_NAME), "[analysis]\n")?;
        let mut opts = options(&root);
        opts.ci = Some(InitCi::Github);

        let plan = init_plan(&opts)?;
        assert_eq!(plan[0].action, InitAction::LeaveUnchanged);
        assert_eq!(plan[1].action, InitAction::Create);

        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    /// Upgrade path: refreshing an existing workflow with `--force` replaces
    /// the workflow and keeps the repository's own `ripr.toml`.
    #[test]
    fn plan_ci_force_refreshes_the_workflow_and_keeps_the_config() -> Result<(), String> {
        let root = temp_root("ci-force")?;
        write(
            &root.join(CONFIG_FILE_NAME),
            "[lsp]\nseam_diagnostics = false\n",
        )?;
        let workflow = root.join(".github/workflows/ripr.yml");
        if let Some(parent) = workflow.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|err| format!("create {} failed: {err}", parent.display()))?;
        }
        write(&workflow, "run: cargo install ripr --locked\n")?;
        let mut opts = options(&root);
        opts.ci = Some(InitCi::Github);
        opts.force = true;

        let plan = init_plan(&opts)?;
        assert_eq!(plan[0].action, InitAction::LeaveUnchanged);
        assert_eq!(plan[1].action, InitAction::Overwrite);
        apply_init_plan(&plan, &opts.root)?;
        let config = std::fs::read_to_string(root.join(CONFIG_FILE_NAME))
            .map_err(|err| format!("read config failed: {err}"))?;
        assert_eq!(config, "[lsp]\nseam_diagnostics = false\n");
        let written = std::fs::read_to_string(&workflow)
            .map_err(|err| format!("read workflow failed: {err}"))?;
        assert!(written.contains("--version"), "{written}");

        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn plan_rejects_an_existing_workflow_without_force() -> Result<(), String> {
        let root = temp_root("wf-exists")?;
        let workflow = root.join(".github/workflows/ripr.yml");
        if let Some(parent) = workflow.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|err| format!("create {} failed: {err}", parent.display()))?;
        }
        write(&workflow, "name: existing\n")?;
        let mut opts = options(&root);
        opts.ci = Some(InitCi::Github);

        match init_plan(&opts) {
            Ok(_) => return Err("an existing workflow without --force must block".to_string()),
            Err(message) => {
                assert!(message.contains("already exists"), "{message}");
                assert!(
                    message.contains("ripr init --ci github --force"),
                    "{message}"
                );
                assert!(
                    !message.contains("resets customizations"),
                    "config-overwrite wording must not leak into the workflow hint: {message}"
                );
            }
        }

        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn plan_rejects_a_root_that_is_not_a_directory() -> Result<(), String> {
        let root = temp_root("bad-root")?;
        let file_root = root.join("a-file");
        write(&file_root, "not a directory\n")?;

        match init_plan(&options(&file_root)) {
            Ok(_) => return Err("a non-directory root must block".to_string()),
            Err(message) => assert!(message.contains("is not a directory"), "{message}"),
        }

        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    /// #2576 review: the plan must reject a parent `create_dir_all` cannot
    /// make, and must name the entry that actually blocks it.
    #[test]
    fn plan_rejects_a_workflow_parent_that_is_a_file() -> Result<(), String> {
        let root = temp_root("parent-file")?;
        write(&root.join(".github"), "not a directory\n")?;
        let mut opts = options(&root);
        opts.ci = Some(InitCi::Github);

        match init_plan(&opts) {
            Ok(_) => return Err("an uncreatable parent must block".to_string()),
            Err(message) => {
                assert!(
                    message.contains("exists and is not a directory"),
                    "{message}"
                );
                assert!(
                    message.contains(&root.join(".github").display().to_string()),
                    "message should name the blocking entry: {message}"
                );
            }
        }

        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn creatable_parent_accepts_directories_that_do_not_exist_yet() -> Result<(), String> {
        let root = temp_root("deep")?;
        ensure_creatable_parent(&root.join("a/b/c/file.yml"))?;

        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn occupied_reports_absent_and_present_paths() -> Result<(), String> {
        let root = temp_root("occupied")?;
        assert!(!path_is_occupied(&root.join("missing"))?);

        let file = root.join("present");
        write(&file, "x")?;
        assert!(path_is_occupied(&file)?);

        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    /// `Path::exists()` follows symlinks and reports `false` here, but
    /// `create_new` still refuses the path, so planning must call it occupied.
    #[cfg(unix)]
    #[test]
    fn occupied_reports_a_dangling_symlink_as_present() -> Result<(), String> {
        let root = temp_root("dangling")?;
        let link = root.join("link");
        std::os::unix::fs::symlink(root.join("nowhere"), &link)
            .map_err(|err| format!("symlink failed: {err}"))?;

        assert!(!link.exists(), "precondition: exists() misses this case");
        assert!(path_is_occupied(&link)?);

        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn action_labels_are_padded_to_a_common_width() {
        let width = InitAction::Create.label().len();
        assert_eq!(InitAction::Overwrite.label().len(), width);
        assert_eq!(InitAction::LeaveUnchanged.label().len(), width);
        assert!(InitAction::Create.label().starts_with("create"));
        assert!(InitAction::Overwrite.label().starts_with("overwrite"));
        assert!(
            InitAction::LeaveUnchanged
                .label()
                .starts_with("leave existing")
        );
    }
}
