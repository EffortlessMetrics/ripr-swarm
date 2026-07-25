from pathlib import Path


def replace_once(path: str, old: str, new: str, label: str) -> None:
    target = Path(path)
    text = target.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected exactly one match in {path}, found {count}")
    target.write_text(text.replace(old, new, 1), encoding="utf-8")


replace_once(
    "xtask/src/main.rs",
    '''    let staged = std::env::temp_dir().join(format!(
        "ripr-vscode-live-workspace-{}-{stamp}",
        std::process::id()
    ));
''',
    '''    let canonical_repo = fs::canonicalize(&root)
        .map_err(|err| format!("failed to canonicalize {}: {err}", root.display()))?;
    let mut staging_base = std::env::var_os("RUNNER_TEMP")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    fs::create_dir_all(&staging_base).map_err(|err| {
        format!(
            "failed to create VS Code staging base {}: {err}",
            staging_base.display()
        )
    })?;
    let mut canonical_staging_base = fs::canonicalize(&staging_base).map_err(|err| {
        format!(
            "failed to canonicalize VS Code staging base {}: {err}",
            staging_base.display()
        )
    })?;
    if canonical_staging_base.starts_with(&canonical_repo) {
        let parent = root.parent().ok_or_else(|| {
            format!("repository root {} has no parent for external staging", root.display())
        })?;
        staging_base = parent.join(".ripr-vscode-live-workspaces");
        fs::create_dir_all(&staging_base).map_err(|err| {
            format!(
                "failed to create external VS Code staging base {}: {err}",
                staging_base.display()
            )
        })?;
        canonical_staging_base = fs::canonicalize(&staging_base).map_err(|err| {
            format!(
                "failed to canonicalize external VS Code staging base {}: {err}",
                staging_base.display()
            )
        })?;
    }
    if canonical_staging_base.starts_with(&canonical_repo) {
        return Err(format!(
            "VS Code live workspace staging base {} is inside repository {}",
            canonical_staging_base.display(),
            canonical_repo.display()
        ));
    }
    let staged = canonical_staging_base.join(format!(
        "ripr-vscode-live-workspace-{}-{stamp}",
        std::process::id()
    ));
''',
    "external VS Code staging invariant",
)

replace_once(
    "editors/vscode/test/suite/extension.test.ts",
    "    'timed out waiting for ripr seam diagnostic.',",
    "    'timed out waiting for the expected ripr diagnostic.',",
    "generic diagnostic timeout copy",
)

replace_once(
    "docs/EDITOR_EXTENSION.md",
    "4. Let the saved-workspace analysis refresh or run `ripr: Restart Server`.\n",
    "4. Let the saved-workspace analysis refresh or run `ripr: Restart Server`.\n   Ordinary open/save publishes the bounded diff-finding profile and reports\n   seam inventory as deferred. Run `ripr: Refresh Full Diagnostics` only when\n   you need the explicit full repository seam inventory.\n",
    "editor first-use refresh step",
)
replace_once(
    "docs/EDITOR_EXTENSION.md",
    "- `ripr.baseRef`: Git base ref used by LSP diagnostics and context commands.\n  Defaults to `origin/main`.\n- `ripr.trace.server`: language-server trace setting.\n\nThe extension passes `ripr.check.mode` and `ripr.baseRef` to the language server\nas initialization options. Changing enabled, server, check, base-ref, or trace\nsettings restarts the client so the next diagnostic refresh uses the new\nconfiguration.\n",
    "- `ripr.baseRef`: Git base ref used by LSP diagnostics and context commands.\n  Defaults to `origin/main`.\n- `ripr.diagnosticProfile`: `actionable` (default) publishes only findings with\n  producer-backed missing-discriminator and fix-site evidence; `full` permits\n  the complete diagnostic projection.\n- `ripr.seamDiagnostics`: allows seam diagnostics when the `full` profile and\n  explicit full refresh are selected. Defaults to `true`; ordinary open/save\n  still defers the expensive seam inventory.\n- `ripr.trace.server`: language-server trace setting.\n\nThe extension passes check mode, base ref, diagnostic profile, and seam enablement\nto the language server as initialization options. Changing enabled, server,\ncheck, base-ref, diagnostic-profile, seam, or trace settings restarts the client\nso the next diagnostic refresh uses the new configuration.\n",
    "editor settings contract",
)
replace_once(
    "docs/EDITOR_EXTENSION.md",
    "- `ripr: Restart Server`\n- `ripr: Show Status`\n",
    "- `ripr: Restart Server`\n- `ripr: Refresh Full Diagnostics`\n- `ripr: Show Status`\n",
    "editor command list",
)

replace_once(
    "docs/CONFIGURATION.md",
    "| Seam diagnostics | Saved-workspace LSP seam diagnostics are on, with explicit config or initialization options allowed to disable them. |\n",
    "| Editor diagnostics | Saved-workspace open/save uses the `actionable` diff-finding profile. Seam diagnostics are enabled as a capability but the full seam inventory is deferred until `ripr: Refresh Full Diagnostics`; explicit config or initialization options may disable it. |\n",
    "configuration defaults row",
)
replace_once(
    "docs/CONFIGURATION.md",
    "| Ready preflight | `ready` | All Rust files in the workspace before separate mutation confirmation. |\n\n",
    "| Ready preflight | `ready` | All Rust files in the workspace before separate mutation confirmation. |\n\nThe VS Code client also sends `diagnosticProfile` (`actionable` by default, or\n`full`) and `seamDiagnostics` (`true` by default) as LSP initialization options.\nThose values control what may be projected, not when the expensive seam inventory\nruns. Ordinary `didOpen`/`didSave` refreshes remain `seams_deferred`; the explicit\n`ripr: Refresh Full Diagnostics` command invokes the server's non-deferred\n`ripr.refresh` route. A full profile alone does not put the full seam walk back on\nevery editor save.\n\n",
    "configuration editor profile section",
)
