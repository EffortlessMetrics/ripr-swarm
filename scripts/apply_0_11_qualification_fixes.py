#!/usr/bin/env python3
"""Apply the reviewed 0.11 qualification fixes to the supplemental candidate branch."""

from __future__ import annotations

from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def replace_once(path: str, old: str, new: str) -> None:
    target = ROOT / path
    text = target.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise RuntimeError(f"expected exactly one replacement in {path}, found {count}")
    target.write_text(text.replace(old, new, 1), encoding="utf-8")


def main() -> None:
    replace_once(
        "editors/vscode/test/runTest.ts",
        """    const launchArgs = [
      workspacePath,
      '--disable-extensions',
""",
        """    // VS Code parses workspace trust switches before the workspace path.
    // This affects only the isolated extension-test host; production keeps
    // defaultRuntime.isWorkspaceTrusted() bound to vscode.workspace.isTrusted.
    const launchArgs = [
      '--disable-workspace-trust',
      workspacePath,
      '--disable-extensions',
""",
    )
    replace_once(
        "editors/vscode/test/runTest.ts",
        """          'ripr.check.mode': 'instant',
""",
        """          'ripr.check.mode': 'instant',
          'security.workspace.trust.enabled': false,
""",
    )

    replace_once(
        "editors/vscode/test/suite/extension.test.ts",
        """      assertReportIncludes(report, [
        'Workspace trust state: workspace_untrusted',
        'ripr server state: ripr_version_ok (ripr 0.8.0-test)'
      ]);
""",
        """      assertReportIncludes(report, [
        'Status: ripr requires a trusted workspace to start the server.',
        'Workspace trust state: workspace_untrusted',
        'ripr server state: ripr_missing',
        'Server: not resolved',
        'Server started: no; server stopped'
      ]);
""",
    )

    replace_once(
        "editors/vscode/test/suite/walkthrough_contract.test.ts",
        """  const packageJsonPath = path.resolve(__dirname, '../../package.json');
  const extensionRoot = path.resolve(__dirname, '../..');
""",
        """  // Compiled tests live under out/test/suite; walk three levels back
  // to the extension root rather than resolving package/media under out/.
  const packageJsonPath = path.resolve(__dirname, '../../../package.json');
  const extensionRoot = path.resolve(__dirname, '../../..');
""",
    )


if __name__ == "__main__":
    main()
