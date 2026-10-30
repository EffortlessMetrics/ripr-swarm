import * as assert from 'assert';
import * as fs from 'fs';
import * as path from 'path';
import * as vscode from 'vscode';
import { RiprClientController, RiprClientRuntime } from '../../src/client';
import { currentWorkspaceRootState } from '../../src/workspaceHelpers';
import { compatibleLspEvidence } from './testCompatibility';

suite('Windows glob controller probe', () => {
  test('controller selects own file through the workspace URI', async () => {
    const folder = vscode.workspace.workspaceFolders?.[0];
    assert.ok(folder);
    assert.match(folder.uri.fsPath, /proj\[1\]\{a,b\}$/);
    const file = path.join(folder.uri.fsPath, 'lib.rs');
    const sibling = path.join(path.dirname(folder.uri.fsPath), 'sibling', 'lib.rs');
    fs.mkdirSync(path.dirname(sibling), { recursive: true });
    fs.writeFileSync(file, 'pub fn f() {}\n'); fs.writeFileSync(sibling, 'pub fn g() {}\n');
    const ownDoc = await vscode.workspace.openTextDocument(vscode.Uri.file(file));
    const siblingDoc = await vscode.workspace.openTextDocument(vscode.Uri.file(sibling));
    let selector: vscode.DocumentSelector | undefined;
    const runtime = {
      getConfig: () => ({ enabled: true, serverArgs: [], traceServer: 'off' }),
      workspaceRootState: currentWorkspaceRootState,
      workspaceFolders: () => vscode.workspace.workspaceFolders ?? [],
      isWorkspaceTrusted: () => true,
      readFile: async () => undefined,
      resolveServer: async () => ({ command: 'sentinel', source: 'configured', detail: 'fake client', installationState: 'unmanaged', compatibilityResult: compatibleLspEvidence }),
      createLanguageClient: (_server: unknown, options: { documentSelector: vscode.DocumentSelector }) => {
        selector = options.documentSelector;
        return { setTrace: () => undefined, onNotification: () => ({ dispose: () => undefined }), start: async () => undefined, stop: async () => undefined };
      },
      createFileSystemWatcher: () => ({ dispose: () => undefined })
    } as unknown as RiprClientRuntime;
    const controller = new RiprClientController({} as vscode.ExtensionContext, { appendLine: () => undefined } as unknown as vscode.LogOutputChannel, runtime);
    try {
      await controller.start();
      assert.ok(selector, 'controller created a language client with selectors');
      const own = vscode.languages.match(selector, ownDoc) > 0;
      const other = vscode.languages.match(selector, siblingDoc) > 0;
      if (process.env.RIPR_GLOB_PROBE_RESULT) fs.writeFileSync(process.env.RIPR_GLOB_PROBE_RESULT, JSON.stringify({ version: vscode.version, own, other }));
      assert.strictEqual(own, true); assert.strictEqual(other, false);
    } finally { await controller.stop(); }
  });
});
