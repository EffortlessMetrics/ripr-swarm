from pathlib import Path


def replace_once(path: str, old: str, new: str, label: str) -> None:
    target = Path(path)
    text = target.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected exactly one match in {path}, found {count}")
    target.write_text(text.replace(old, new, 1), encoding="utf-8")


# Typed VS Code settings and initialization options.
replace_once(
    "editors/vscode/src/config.ts",
    "export type TraceSetting = 'off' | 'messages' | 'verbose';\n",
    "export type TraceSetting = 'off' | 'messages' | 'verbose';\nexport type DiagnosticProfileSetting = 'actionable' | 'full';\n",
    "diagnostic profile type",
)
replace_once(
    "editors/vscode/src/config.ts",
    "  baseRef: string;\n  traceServer: TraceSetting;\n",
    "  baseRef: string;\n  diagnosticProfile: DiagnosticProfileSetting;\n  seamDiagnostics: boolean;\n  traceServer: TraceSetting;\n",
    "config interface fields",
)
replace_once(
    "editors/vscode/src/config.ts",
    "    baseRef: config.get<string>('baseRef', 'origin/main'),\n    traceServer: config.get<TraceSetting>('trace.server', 'off')\n",
    "    baseRef: config.get<string>('baseRef', 'origin/main'),\n    diagnosticProfile: config.get<DiagnosticProfileSetting>('diagnosticProfile', 'actionable'),\n    seamDiagnostics: config.get<boolean>('seamDiagnostics', true),\n    traceServer: config.get<TraceSetting>('trace.server', 'off')\n",
    "config values",
)

replace_once(
    "editors/vscode/src/client.ts",
    "  'ripr.restartServer',\n  'ripr.selectWorkspaceRoot',\n",
    "  'ripr.restartServer',\n  'ripr.refreshDiagnostics',\n  'ripr.selectWorkspaceRoot',\n",
    "client command advertisement",
)
replace_once(
    "editors/vscode/src/client.ts",
    "        checkMode: config.checkMode,\n        includeUnchangedTests: true\n",
    "        checkMode: config.checkMode,\n        includeUnchangedTests: true,\n        diagnosticProfile: config.diagnosticProfile,\n        seamDiagnostics: config.seamDiagnostics\n",
    "initialization options",
)
replace_once(
    "editors/vscode/src/client.ts",
    "  async restart(): Promise<void> {\n    await this.stop();\n    await this.start();\n  }\n",
    "  async restart(): Promise<void> {\n    await this.stop();\n    await this.start();\n  }\n\n  /** Request the explicit non-deferred seam inventory from the running server. */\n  async refreshDiagnostics(): Promise<void> {\n    if (!this.client) {\n      this.output.appendLine('ripr full diagnostic refresh was requested without a running server.');\n      await this.runtime.showWarningMessage(\n        'ripr server is not running. Run ripr: Restart Server before requesting full diagnostics.'\n      );\n      return;\n    }\n    this.output.appendLine('Requesting explicit full ripr diagnostics (including seam inventory).');\n    await this.client.sendRequest('workspace/executeCommand', {\n      command: 'ripr.refresh',\n      arguments: []\n    });\n  }\n",
    "refresh controller method",
)

replace_once(
    "editors/vscode/src/extension.ts",
    "    vscode.commands.registerCommand('ripr.restartServer', async () => controller?.restart()),\n",
    "    vscode.commands.registerCommand('ripr.restartServer', async () => controller?.restart()),\n    vscode.commands.registerCommand('ripr.refreshDiagnostics', async () =>\n      controller?.refreshDiagnostics()\n    ),\n",
    "refresh command registration",
)
replace_once(
    "editors/vscode/src/extension.ts",
    "        event.affectsConfiguration('ripr.check') ||\n        event.affectsConfiguration('ripr.baseRef')\n",
    "        event.affectsConfiguration('ripr.check') ||\n        event.affectsConfiguration('ripr.baseRef') ||\n        event.affectsConfiguration('ripr.diagnosticProfile') ||\n        event.affectsConfiguration('ripr.seamDiagnostics')\n",
    "configuration restart keys",
)

# Contributed settings and command.
replace_once(
    "editors/vscode/package.json",
    '''        "ripr.baseRef": {
          "type": "string",
          "default": "origin/main",
          "description": "Git base ref used by ripr editor diagnostics and context commands."
        },
''',
    '''        "ripr.baseRef": {
          "type": "string",
          "default": "origin/main",
          "description": "Git base ref used by ripr editor diagnostics and context commands."
        },
        "ripr.diagnosticProfile": {
          "type": "string",
          "enum": [
            "actionable",
            "full"
          ],
          "default": "actionable",
          "description": "Diagnostic visibility profile. Actionable is the defaults-first interactive profile; full is required before the explicit full seam refresh can publish seam diagnostics.",
          "scope": "resource"
        },
        "ripr.seamDiagnostics": {
          "type": "boolean",
          "default": true,
          "description": "Allow seam diagnostics when the full profile and explicit full refresh are selected. Ordinary open/save still defers the expensive seam inventory.",
          "scope": "resource"
        },
''',
    "package settings",
)
replace_once(
    "editors/vscode/package.json",
    '''      {
        "command": "ripr.restartServer",
        "title": "ripr: Restart Server"
      },
''',
    '''      {
        "command": "ripr.restartServer",
        "title": "ripr: Restart Server"
      },
      {
        "command": "ripr.refreshDiagnostics",
        "title": "ripr: Refresh Full Diagnostics"
      },
''',
    "package refresh command",
)

# The extension-host starts with an actual diff and the defaults-first profile.
for file_name in ["editors/vscode/test/runTest.ts", "editors/vscode/test/suite/extension.test.ts"]:
    replace_once(file_name, "'ripr.baseRef': 'HEAD',", "'ripr.baseRef': 'HEAD~1',", f"{file_name} base") if file_name.endswith("runTest.ts") else None
    replace_once(file_name, "'ripr.check.mode': 'instant',", "'ripr.check.mode': 'fast',", f"{file_name} mode") if file_name.endswith("runTest.ts") else None

replace_once(
    "editors/vscode/test/runTest.ts",
    "          'ripr.check.mode': 'fast',\n          'security.workspace.trust.enabled': false,\n",
    "          'ripr.check.mode': 'fast',\n          'ripr.diagnosticProfile': 'actionable',\n          'ripr.seamDiagnostics': true,\n          'security.workspace.trust.enabled': false,\n",
    "test host diagnostics settings",
)

# Stage one private two-commit workspace outside the repository. TypeScript
# smoke sources are tracked in both commits; ripr.toml and target/ are excluded
# so test-owned policy/artifacts do not dirty the Git input authority.
replace_once(
    "xtask/src/main.rs",
    '''fn vscode_test_workspace_path() -> Result<PathBuf, String> {
    Ok(repo_root()?
        .join("fixtures")
        .join("boundary_gap")
        .join("input"))
}
''',
    r'''fn vscode_test_workspace_path() -> Result<PathBuf, String> {
    let root = repo_root()?;
    let fixture = root.join("fixtures").join("boundary_gap");
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|err| format!("system clock before UNIX_EPOCH: {err}"))?
        .as_nanos();
    let staged = std::env::temp_dir().join(format!(
        "ripr-vscode-live-workspace-{}-{stamp}",
        std::process::id()
    ));
    copy_vscode_test_tree(&fixture.join("input"), &staged)?;
    write_vscode_preview_sources(&staged)?;

    let git = Path::new("git");
    run_in_dir(git, &["init", "--quiet"], &staged)?;
    let exclude = staged.join(".git/info/exclude");
    fs::write(&exclude, "target/\nripr.toml\n")
        .map_err(|err| format!("failed to write {}: {err}", exclude.display()))?;
    let patch = path_to_utf8(&fixture.join("diff.patch"), "boundary-gap patch")?.to_string();
    run_in_dir(git, &["apply", "--reverse", &patch], &staged)?;
    commit_vscode_test_workspace(&staged, "boundary gap base state")?;
    run_in_dir(git, &["apply", &patch], &staged)?;
    commit_vscode_test_workspace(&staged, "boundary gap changed state")?;
    Ok(staged)
}

fn commit_vscode_test_workspace(root: &Path, message: &str) -> Result<(), String> {
    let git = Path::new("git");
    run_in_dir(git, &["add", "--all"], root)?;
    run_in_dir(
        git,
        &[
            "-c",
            "user.name=RIPR live extension harness",
            "-c",
            "user.email=ripr-live-harness@example.invalid",
            "-c",
            "commit.gpgSign=false",
            "commit",
            "--quiet",
            "--message",
            message,
        ],
        root,
    )
    .map(|_| ())
}

fn copy_vscode_test_tree(source: &Path, destination: &Path) -> Result<(), String> {
    fs::create_dir_all(destination)
        .map_err(|err| format!("failed to create {}: {err}", destination.display()))?;
    for entry in fs::read_dir(source)
        .map_err(|err| format!("failed to read {}: {err}", source.display()))?
    {
        let entry = entry.map_err(|err| format!("failed to read {}: {err}", source.display()))?;
        if entry.file_name() == std::ffi::OsStr::new("target") {
            continue;
        }
        let from = entry.path();
        let to = destination.join(entry.file_name());
        let kind = entry
            .file_type()
            .map_err(|err| format!("failed to stat {}: {err}", from.display()))?;
        if kind.is_dir() {
            copy_vscode_test_tree(&from, &to)?;
        } else {
            fs::copy(&from, &to)
                .map_err(|err| format!("failed to copy {}: {err}", from.display()))?;
        }
    }
    Ok(())
}

fn write_vscode_preview_sources(root: &Path) -> Result<(), String> {
    fs::create_dir_all(root.join("tests"))
        .map_err(|err| format!("failed to create TypeScript test directory: {err}"))?;
    fs::write(
        root.join("src/pricing.ts"),
        "export function discountedTotal(amount: number, threshold: number): number {\n  if (amount >= threshold) {\n    return amount - 10;\n  }\n  return amount;\n}\n",
    )
    .map_err(|err| format!("failed to write TypeScript smoke source: {err}"))?;
    fs::write(
        root.join("tests/pricing.test.ts"),
        "import { discountedTotal } from '../src/pricing';\n\ntest('discount threshold boundary', () => {\n  expect(discountedTotal(50, 100)).toBe(50);\n});\n",
    )
    .map_err(|err| format!("failed to write TypeScript smoke test: {err}"))
}
''',
    "private VS Code workspace staging",
)

# Extension smoke contract: one default finding journey, one explicit seam
# journey, and the TypeScript gap projection after an explicit full refresh.
ext_path = Path("editors/vscode/test/suite/extension.test.ts")
ext_text = ext_path.read_text(encoding="utf-8")
ext_text = ext_text.replace(
    "    assert.ok(commands.includes('ripr.restartServer'));\n",
    "    assert.ok(commands.includes('ripr.restartServer'));\n    assert.ok(commands.includes('ripr.refreshDiagnostics'));\n",
    1,
)
ext_text = ext_text.replace(
    "      baseRef: 'origin/main',\n      traceServer: 'off'\n",
    "      baseRef: 'origin/main',\n      diagnosticProfile: 'actionable',\n      seamDiagnostics: true,\n      traceServer: 'off'\n",
    1,
)
ext_text = ext_text.replace(
    "  test('defaults-first check mode is draft', () => {\n    const config = vscode.workspace.getConfiguration('ripr');\n    assert.strictEqual(config.inspect('check.mode')?.defaultValue, 'draft');\n  });\n",
    "  test('defaults-first editor profile is draft, actionable, and seam-capable on explicit refresh', () => {\n    const config = vscode.workspace.getConfiguration('ripr');\n    assert.strictEqual(config.inspect('check.mode')?.defaultValue, 'draft');\n    assert.strictEqual(config.inspect('diagnosticProfile')?.defaultValue, 'actionable');\n    assert.strictEqual(config.inspect('seamDiagnostics')?.defaultValue, true);\n  });\n",
    1,
)
original_test = "  test('real server surfaces seam diagnostic, hover provider, and agent actions', async function (this: Mocha.Context) {"
if ext_text.count(original_test) != 1:
    raise SystemExit("original Rust live test anchor not found exactly once")
default_test = r'''  test('real server default interactive path surfaces an actionable diff finding', async function (this: Mocha.Context) {
    this.timeout(75000);
    if (!process.env.RIPR_TEST_SERVER_PATH) {
      this.skip();
    }

    const config = vscode.workspace.getConfiguration('ripr');
    await config.update('diagnosticProfile', 'actionable', vscode.ConfigurationTarget.Global);
    await config.update('seamDiagnostics', true, vscode.ConfigurationTarget.Global);
    const uri = workspaceFileUri('src/lib.rs');
    await vscode.commands.executeCommand('workbench.action.closeAllEditors');
    const document = await vscode.workspace.openTextDocument(uri);
    assert.strictEqual(document.languageId, 'rust');
    await vscode.window.showTextDocument(document);
    await vscode.commands.executeCommand('ripr.restartServer');

    const diagnostic = await waitForDiagnostic(
      uri,
      (entry) => entry.source === 'ripr' && diagnosticCode(entry) === 'weakly_exposed',
      60000
    );
    const hoverPosition = new vscode.Position(
      diagnostic.range.start.line,
      diagnostic.range.start.character + 1
    );
    const hoverText = await waitForHoverText(uri, hoverPosition, (text) =>
      text.includes('**ripr** `weakly_exposed`') &&
      text.includes('## Discriminator witness') &&
      text.includes('amount == discount_threshold') &&
      text.includes('tests/pricing.rs')
    );
    assert.ok(hoverText.includes('**Fix instruction:** fix site ready'), hoverText);

    const actions = await vscode.commands.executeCommand<Array<vscode.CodeAction | vscode.Command>>(
      'vscode.executeCodeActionProvider',
      uri,
      diagnostic.range
    );
    assertCommandAction(actions, 'Inspect Test Gap - Copy Context', 'ripr.copyContext');
    assertCommandAction(actions, 'Write targeted test: copy brief', 'ripr.copyTargetedTestBrief');
    assertCommandAction(actions, 'Write targeted test: open best related test', 'ripr.openRelatedTest');
  });

'''
ext_text = ext_text.replace(original_test, default_test + "  test('real server explicit full refresh surfaces seam diagnostic, hover, and agent actions', async function (this: Mocha.Context) {", 1)
ext_text = ext_text.replace(
    "    const uri = workspaceFileUri('src/lib.rs');\n    await vscode.commands.executeCommand('workbench.action.closeAllEditors');",
    "    const config = vscode.workspace.getConfiguration('ripr');\n    await config.update('diagnosticProfile', 'full', vscode.ConfigurationTarget.Global);\n    await config.update('seamDiagnostics', true, vscode.ConfigurationTarget.Global);\n    const uri = workspaceFileUri('src/lib.rs');\n    await vscode.commands.executeCommand('workbench.action.closeAllEditors');",
    1,
)
ext_text = ext_text.replace(
    "    await vscode.commands.executeCommand('ripr.restartServer');\n\n    const diagnostic = await waitForDiagnostic(\n      uri,\n      (entry) => entry.source === 'ripr' && diagnosticCode(entry) === 'ripr-seam-weakly-gripped',",
    "    await vscode.commands.executeCommand('ripr.restartServer');\n    await vscode.commands.executeCommand('ripr.refreshDiagnostics');\n\n    const diagnostic = await waitForDiagnostic(\n      uri,\n      (entry) => entry.source === 'ripr' && diagnosticCode(entry) === 'ripr-seam-weakly-gripped',",
    1,
)
# The TypeScript ledger is a full-snapshot projection and therefore also uses
# the explicit refresh route.
type_anchor = "      await vscode.commands.executeCommand('ripr.restartServer');\n\n      const diagnostic = await waitForDiagnostic(\n        uri,\n        (entry) => entry.source === 'ripr' && diagnosticCode(entry) === 'ripr-gap-MissingBoundaryAssertion',"
if ext_text.count(type_anchor) != 1:
    raise SystemExit("TypeScript refresh anchor not found exactly once")
ext_text = ext_text.replace(
    type_anchor,
    "      const config = vscode.workspace.getConfiguration('ripr');\n      await config.update('diagnosticProfile', 'full', vscode.ConfigurationTarget.Global);\n      await config.update('seamDiagnostics', true, vscode.ConfigurationTarget.Global);\n      await vscode.commands.executeCommand('ripr.restartServer');\n      await vscode.commands.executeCommand('ripr.refreshDiagnostics');\n\n      const diagnostic = await waitForDiagnostic(\n        uri,\n        (entry) => entry.source === 'ripr' && diagnosticCode(entry) === 'ripr-gap-MissingBoundaryAssertion',",
    1,
)
ext_text = ext_text.replace(
    "    } finally {\n      await cleanupEditorGapSmokeFiles();\n      await vscode.commands.executeCommand('ripr.restartServer');\n    }\n  });",
    "    } finally {\n      const config = vscode.workspace.getConfiguration('ripr');\n      await config.update('diagnosticProfile', 'actionable', vscode.ConfigurationTarget.Global);\n      await config.update('seamDiagnostics', true, vscode.ConfigurationTarget.Global);\n      await cleanupEditorGapSmokeFiles();\n      await vscode.commands.executeCommand('ripr.restartServer');\n    }\n  });",
    1,
)
ext_text = ext_text.replace(
    "  await config.update('baseRef', 'HEAD', vscode.ConfigurationTarget.Global);\n  await config.update('check.mode', 'instant', vscode.ConfigurationTarget.Global);\n",
    "  await config.update('baseRef', 'HEAD~1', vscode.ConfigurationTarget.Global);\n  await config.update('check.mode', 'fast', vscode.ConfigurationTarget.Global);\n  await config.update('diagnosticProfile', 'actionable', vscode.ConfigurationTarget.Global);\n  await config.update('seamDiagnostics', true, vscode.ConfigurationTarget.Global);\n",
    1,
)
ext_text = ext_text.replace(
    "    removeWorkspacePath('ripr.toml'),\n    removeWorkspacePath('src/pricing.ts'),\n    removeWorkspacePath('tests/pricing.test.ts'),\n    removeWorkspacePath('target/ripr/reports/gap-decision-ledger.json')",
    "    removeWorkspacePath('ripr.toml'),\n    removeWorkspacePath('target/ripr/reports/gap-decision-ledger.json')",
    1,
)
ext_path.write_text(ext_text, encoding="utf-8")
