import * as assert from 'assert';
import * as vscode from 'vscode';
import { RiprClientController, RiprClientRuntime, RiprWorkspaceRootState } from '../../src/client';
import { RiprConfig } from '../../src/config';
import {
  pauseRemovedWorkspaceRootOnce, resetLifecycleCoordinatorForTests,
  startServerOnce, stopServerOnce, synchronizeWorkspaceRootOnce
} from '../../src/extension';
import { compatibleLspEvidence } from './testCompatibility';

const alpha = { name: 'alpha', index: 0, uri: vscode.Uri.file('/ws/alpha') };
const beta = { name: 'beta', index: 1, uri: vscode.Uri.file('/ws/beta') };
const config: RiprConfig = {
  enabled: true, serverPath: '/sentinel/binding/ripr', serverArgs: ['lsp', '--stdio'],
  autoDownload: false, serverVersion: 'binding-test', downloadBaseUrl: 'https://sentinel.invalid',
  checkMode: 'draft', baseRef: 'HEAD~1', includeUnchangedTests: true,
  seamDiagnostics: true, diagnosticProfile: 'full', traceServer: 'off'
};

function harness() {
  const events: string[] = [];
  const infos: string[] = [];
  let folders = [alpha, beta];
  let root: string | undefined = alpha.uri.fsPath;
  let stopGate = Promise.resolve();
  let onStop: () => void = () => undefined;
  const runtime: RiprClientRuntime = {
    getConfig: () => config,
    workspaceRootState: (): RiprWorkspaceRootState => ({
      kind: root ? (folders.length === 1 ? 'singleRoot' : 'selectedRoot') : 'ambiguousMultiRoot',
      root, roots: folders.map(f => f.uri.fsPath)
    }),
    workspaceFolders: () => folders,
    showQuickPick: async () => undefined,
    resolveServer: async () => ({
      command: '/sentinel/binding/ripr', source: 'configured', detail: 'controller-only test',
      installationState: 'unmanaged', compatibilityResult: compatibleLspEvidence
    }),
    createLanguageClient: (server, options) => {
      const cwd = (server as { options: { cwd: string } }).options.cwd;
      assert.strictEqual(options.workspaceFolder?.uri.fsPath, cwd);
      return {
        onNotification: () => ({ dispose: () => { events.push(`dispose:${cwd}`); } }),
        sendRequest: async () => undefined, setTrace: () => undefined,
        start: async () => { events.push(`start:${cwd}`); },
        stop: async () => {
          events.push(`stop:${cwd}`); onStop(); await stopGate;
        }
      };
    },
    createFileSystemWatcher: () => ({} as vscode.FileSystemWatcher),
    readFile: async () => undefined,
    runRipr: async () => { throw new Error('controller tests must not execute a server'); },
    writeClipboard: async () => { throw new Error('quarantined targets must not copy'); },
    isWorkspaceTrusted: () => true,
    showInformationMessage: async message => { infos.push(message); return undefined; },
    showWarningMessage: async () => undefined, showErrorMessage: async () => undefined
  };
  const controller = new RiprClientController(
    {} as vscode.ExtensionContext, { appendLine: () => undefined } as unknown as vscode.LogOutputChannel, runtime
  );
  return {
    controller, events, infos,
    select: (selected: string | undefined) => { root = selected; },
    removeBeta: () => { folders = [alpha]; root = alpha.uri.fsPath; },
    holdStop: () => {
      let release!: () => void;
      let entered!: () => void;
      stopGate = new Promise<void>(resolve => { release = resolve; });
      const stopped = new Promise<void>(resolve => { entered = resolve; });
      onStop = entered;
      return { stopped, release };
    }
  };
}

suite('Workspace Root Binding Controller', () => {
  setup(() => resetLifecycleCoordinatorForTests());

  test('ambiguity followed by selecting A starts exactly one A client', async () => {
    const h = harness(); h.select(undefined);
    try {
      await startServerOnce(h.controller);
      assert.deepStrictEqual(h.events, []);
      h.select(alpha.uri.fsPath);
      await synchronizeWorkspaceRootOnce(h.controller);
      assert.deepStrictEqual(h.events, [`start:${alpha.uri.fsPath}`]);
    } finally { await stopServerOnce(h.controller); }
  });

  test('running A switches to B once after stopping and disposing A', async () => {
    const h = harness();
    try {
      await startServerOnce(h.controller);
      h.select(beta.uri.fsPath);
      await synchronizeWorkspaceRootOnce(h.controller);
      assert.deepStrictEqual(h.events, [
        `start:${alpha.uri.fsPath}`, `stop:${alpha.uri.fsPath}`,
        `dispose:${alpha.uri.fsPath}`, `dispose:${alpha.uri.fsPath}`, `start:${beta.uri.fsPath}`
      ]);
    } finally { await stopServerOnce(h.controller); }
  });

  test('removing active B leaves no client and never silently starts remaining A', async () => {
    const h = harness(); h.select(beta.uri.fsPath);
    try {
      await startServerOnce(h.controller);
      h.removeBeta();
      await pauseRemovedWorkspaceRootOnce(h.controller, [beta]);
      assert.strictEqual(h.controller.isRunning(), false);
      assert.deepStrictEqual(h.events, [
        `start:${beta.uri.fsPath}`, `stop:${beta.uri.fsPath}`,
        `dispose:${beta.uri.fsPath}`, `dispose:${beta.uri.fsPath}`
      ]);
      await synchronizeWorkspaceRootOnce(h.controller);
      await startServerOnce(h.controller);
      assert.strictEqual(h.controller.isRunning(), false, 'passive events cannot select the remaining root');
    } finally { await stopServerOnce(h.controller); }
  });

  test('cached navigation and context are suppressed after the session stops', async () => {
    const h = harness();
    try {
      await startServerOnce(h.controller);
      await h.controller.stop();
      const activeEditor = vscode.window.activeTextEditor;
      await h.controller.openRelatedTest({ uri: vscode.Uri.file('/ws/alpha/tests/contract.rs').toString(), line: 4 });
      assert.strictEqual(vscode.window.activeTextEditor, activeEditor, 'cached navigation must not open an editor');
      assert.match(h.infos.at(-1) ?? '', /requires a running.*workspace root/);
      await h.controller.copyContext({ uri: vscode.Uri.file('/ws/alpha/src/alpha.rs').toString(), seam_id: 'cached-alpha' });
      assert.match(h.infos.at(-1) ?? '', /requires a running.*workspace root/);
    } finally { await stopServerOnce(h.controller); }
  });

  test('removing another folder leaves the bound client untouched', async () => {
    const h = harness();
    try {
      await startServerOnce(h.controller);
      await pauseRemovedWorkspaceRootOnce(h.controller, [beta]);
      assert.deepStrictEqual(h.events, [`start:${alpha.uri.fsPath}`]);
      assert.strictEqual(h.controller.isRunning(), true);
    } finally { await stopServerOnce(h.controller); }
  });
});
