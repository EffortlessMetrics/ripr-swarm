from pathlib import Path
import re


def replace_once(path: str, old: str, new: str, label: str) -> None:
    target = Path(path)
    text = target.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected exactly one match in {path}, found {count}")
    target.write_text(text.replace(old, new, 1), encoding="utf-8")


extension = "editors/vscode/test/suite/extension.test.ts"
replace_once(
    extension,
    "    const config = vscode.workspace.getConfiguration('ripr');\n"
    "    await config.update('diagnosticProfile', 'actionable', vscode.ConfigurationTarget.Global);\n"
    "    await config.update('seamDiagnostics', true, vscode.ConfigurationTarget.Global);\n"
    "    const config = vscode.workspace.getConfiguration('ripr');\n"
    "    await config.update('diagnosticProfile', 'full', vscode.ConfigurationTarget.Global);\n"
    "    await config.update('seamDiagnostics', true, vscode.ConfigurationTarget.Global);\n",
    "    const config = vscode.workspace.getConfiguration('ripr');\n"
    "    await config.update('diagnosticProfile', 'actionable', vscode.ConfigurationTarget.Global);\n"
    "    await config.update('seamDiagnostics', true, vscode.ConfigurationTarget.Global);\n",
    "remove duplicated default-test config",
)
replace_once(
    extension,
    "  test('real server explicit full refresh surfaces seam diagnostic, hover, and agent actions', async function (this: Mocha.Context) {\n"
    "    this.timeout(75000);\n"
    "    if (!process.env.RIPR_TEST_SERVER_PATH) {\n"
    "      this.skip();\n"
    "    }\n\n"
    "    const uri = workspaceFileUri('src/lib.rs');\n",
    "  test('real server explicit full refresh surfaces seam diagnostic, hover, and agent actions', async function (this: Mocha.Context) {\n"
    "    this.timeout(75000);\n"
    "    if (!process.env.RIPR_TEST_SERVER_PATH) {\n"
    "      this.skip();\n"
    "    }\n\n"
    "    const config = vscode.workspace.getConfiguration('ripr');\n"
    "    await config.update('diagnosticProfile', 'full', vscode.ConfigurationTarget.Global);\n"
    "    await config.update('seamDiagnostics', true, vscode.ConfigurationTarget.Global);\n"
    "    const uri = workspaceFileUri('src/lib.rs');\n",
    "full-journey config",
)

for path in [
    "editors/vscode/test/suite/show_output_action.test.ts",
    "editors/vscode/test/suite/workspace_root_picker.test.ts",
    "editors/vscode/test/suite/workspace_trust_runtime.test.ts",
    "editors/vscode/test/suite/workspace_trust_transition.test.ts",
]:
    target = Path(path)
    text = target.read_text(encoding="utf-8")
    pattern = re.compile(
        r"(?m)^(?P<indent>\s*)baseRef: 'origin/main',\n(?P=indent)traceServer: 'off'"
    )
    text, count = pattern.subn(
        lambda match: (
            f"{match.group('indent')}baseRef: 'origin/main',\n"
            f"{match.group('indent')}diagnosticProfile: 'actionable',\n"
            f"{match.group('indent')}seamDiagnostics: true,\n"
            f"{match.group('indent')}traceServer: 'off'"
        ),
        text,
        count=1,
    )
    if count != 1:
        raise SystemExit(f"RiprConfig fixture fields: expected one match in {path}, found {count}")
    target.write_text(text, encoding="utf-8")
