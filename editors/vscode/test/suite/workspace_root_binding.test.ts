import * as assert from 'assert';
import * as vscode from 'vscode';
import { RiprClientController, RiprClientRuntime, RiprWorkspaceRootState } from '../../src/client';
import { RiprConfig } from '../../src/config';
import {
  pauseRemovedWorkspaceRootOnce, resetLifecycleCoordinatorForTests, restartServerOnce,
  startAfterFirstWorkspaceFolder, startServerOnce, stopServerOnce, synchronizeWorkspaceRootOnce
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
  let enabled = true;
  let startGate = Promise.resolve();
  let gatedRoot: string | undefined;
  let onStart: () => void = () => undefined;
  let request: () => Promise<unknown> = async () => undefined;
  const runtime: RiprClientRuntime = {
    getConfig: () => ({ ...config, enabled }),
    workspaceRootState: (): RiprWorkspaceRootState => ({
      kind: root ? (folders.length === 1 ? 'singleRoot' : 'selectedRoot') : 'ambiguousMultiRoot',
      root, roots: folders.map(f => f.uri.fsPath)
    }),
    workspaceFolders: () => folders,
    workspaceFolderForUri: uri => folders.find(f => uri.fsPath.startsWith(`${f.uri.fsPath}/`)),
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
        sendRequest: () => request(), setTrace: () => undefined,
        start: async () => {
          events.push(`start:${cwd}`);
          if (cwd === gatedRoot) { onStart(); await startGate; }
        },
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
  // Production picker recovery uses the extension's coordinated restart.
  (runtime as RiprClientRuntime & { restartWorkspaceRoot: () => Promise<void> }).restartWorkspaceRoot =
    () => restartServerOnce(controller);
  return {
    controller, events, infos, runtime,
    setRequest: (handler: () => Promise<unknown>) => { request = handler; },
    disable: () => { enabled = false; },
    select: (selected: string | undefined) => { root = selected; },
    removeBeta: () => { folders = [alpha]; root = alpha.uri.fsPath; },
    holdStop: () => {
      let release!: () => void;
      let entered!: () => void;
      stopGate = new Promise<void>(resolve => { release = resolve; });
      const stopped = new Promise<void>(resolve => { entered = resolve; });
      onStop = entered;
      return { stopped, release };
    },
    holdStart: (targetRoot: string) => {
      gatedRoot = targetRoot;
      let release!: () => void;
      let entered!: () => void;
      startGate = new Promise<void>(resolve => { release = resolve; });
      const started = new Promise<void>(resolve => { entered = resolve; });
      onStart = entered;
      return { started, release };
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
      const state = h.controller as unknown as { workspaceRoot?: string; workspaceRootState: { kind: string } };
      assert.strictEqual(state.workspaceRoot, undefined);
      assert.strictEqual(state.workspaceRootState.kind, 'noWorkspace');
      assert.deepStrictEqual(h.events, [
        `start:${beta.uri.fsPath}`, `stop:${beta.uri.fsPath}`,
        `dispose:${beta.uri.fsPath}`, `dispose:${beta.uri.fsPath}`
      ]);
      await synchronizeWorkspaceRootOnce(h.controller);
      await startServerOnce(h.controller);
      assert.strictEqual(h.controller.isRunning(), false, 'passive events cannot select the remaining root');
    } finally { await stopServerOnce(h.controller); }
  });

  test('a passive start overlapping B removal waits for teardown and leaves no client', async () => {
    const h = harness(); h.select(beta.uri.fsPath);
    try {
      await startServerOnce(h.controller);
      h.removeBeta();
      const removal = pauseRemovedWorkspaceRootOnce(h.controller, [beta]);
      const passiveStart = startServerOnce(h.controller);
      await Promise.all([removal, passiveStart]);
      assert.strictEqual(h.controller.isRunning(), false, 'overlapping passive startup must not cancel teardown');
      const state = h.controller as unknown as { workspaceRoot?: string; workspaceRootState: { kind: string } };
      assert.strictEqual(state.workspaceRoot, undefined);
      assert.strictEqual(state.workspaceRootState.kind, 'noWorkspace');
      assert.deepStrictEqual(h.events, [
        `start:${beta.uri.fsPath}`, `stop:${beta.uri.fsPath}`,
        `dispose:${beta.uri.fsPath}`, `dispose:${beta.uri.fsPath}`
      ]);
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

  test('duplicate B selections wait for A stop and create one B replacement', async () => {
    const h = harness(); const gate = h.holdStop();
    try {
      await startServerOnce(h.controller); h.select(beta.uri.fsPath);
      const first = synchronizeWorkspaceRootOnce(h.controller);
      await gate.stopped;
      const second = synchronizeWorkspaceRootOnce(h.controller);
      assert.deepStrictEqual(h.events, [`start:${alpha.uri.fsPath}`, `stop:${alpha.uri.fsPath}`]);
      await h.controller.openRelatedTest({ uri: vscode.Uri.file('/ws/alpha/tests/contract.rs').toString() });
      assert.match(h.infos.at(-1) ?? '', /requires a running.*workspace root/);
      gate.release(); await Promise.all([first, second]);
      assert.strictEqual(h.events.filter(e => e === `start:${beta.uri.fsPath}`).length, 1);
      assert.strictEqual(h.events.filter(e => e === `stop:${alpha.uri.fsPath}`).length, 1);
    } finally { gate.release(); await stopServerOnce(h.controller); }
  });

  test('a newer A selection while stopping A supersedes the queued B root', async () => {
    const h = harness(); const gate = h.holdStop();
    try {
      await startServerOnce(h.controller); h.select(beta.uri.fsPath);
      const first = synchronizeWorkspaceRootOnce(h.controller); await gate.stopped;
      h.select(alpha.uri.fsPath);
      const newer = synchronizeWorkspaceRootOnce(h.controller);
      gate.release(); await Promise.all([first, newer]);
      assert.deepStrictEqual(h.events.filter(e => e.startsWith('start:')), [
        `start:${alpha.uri.fsPath}`, `start:${alpha.uri.fsPath}`
      ]);
    } finally { gate.release(); await stopServerOnce(h.controller); }
  });

  test('removing queued B during A stop cancels the replacement without a second stop', async () => {
    const h = harness(); const gate = h.holdStop();
    try {
      await startServerOnce(h.controller); h.select(beta.uri.fsPath);
      const switching = synchronizeWorkspaceRootOnce(h.controller); await gate.stopped;
      h.removeBeta();
      const removal = pauseRemovedWorkspaceRootOnce(h.controller, [beta]);
      gate.release(); await Promise.all([switching, removal]);
      assert.strictEqual(h.controller.isRunning(), false);
      assert.deepStrictEqual(h.events, [
        `start:${alpha.uri.fsPath}`, `stop:${alpha.uri.fsPath}`,
        `dispose:${alpha.uri.fsPath}`, `dispose:${alpha.uri.fsPath}`
      ]);
    } finally { gate.release(); await stopServerOnce(h.controller); }
  });

  test('explicit root selection resumes the sole remaining A after removing B', async () => {
    const h = harness(); h.select(beta.uri.fsPath);
    try {
      await startServerOnce(h.controller); h.removeBeta();
      await pauseRemovedWorkspaceRootOnce(h.controller, [beta]);
      await h.controller.selectWorkspaceRoot();
      assert.deepStrictEqual(h.events.filter(e => e.startsWith('start:')), [
        `start:${beta.uri.fsPath}`, `start:${alpha.uri.fsPath}`
      ]);
      assert.strictEqual(h.controller.isRunning(), true);
    } finally { await stopServerOnce(h.controller); }
  });

  test('picker recovery overlapping removal cannot restart after terminal shutdown', async () => {
    const h = harness(); h.select(beta.uri.fsPath); const gate = h.holdStop();
    const operations: Promise<unknown>[] = [];
    try {
      await startServerOnce(h.controller); h.removeBeta();
      operations.push(pauseRemovedWorkspaceRootOnce(h.controller, [beta]));
      await gate.stopped;
      operations.push(h.controller.selectWorkspaceRoot());
      operations.push(stopServerOnce(h.controller));
      gate.release(); await Promise.all(operations);
      assert.strictEqual(h.controller.isRunning(), false);
      assert.deepStrictEqual(h.events, [
        `start:${beta.uri.fsPath}`, `stop:${beta.uri.fsPath}`,
        `dispose:${beta.uri.fsPath}`, `dispose:${beta.uri.fsPath}`
      ]);
    } finally {
      gate.release(); await Promise.allSettled(operations);
      await stopServerOnce(h.controller); await h.controller.stop();
    }
  });

  test('terminal shutdown waits for picker recovery resolution and leaves no replacement', async () => {
    const h = harness(); h.select(beta.uri.fsPath); const gate = h.holdStop();
    let releaseResolver!: () => void; let enteredResolver!: () => void;
    const resolverGate = new Promise<void>(resolve => { releaseResolver = resolve; });
    const resolving = new Promise<void>(resolve => { enteredResolver = resolve; });
    const originalResolve = h.runtime.resolveServer; let resolutions = 0;
    h.runtime.resolveServer = async (...args) => {
      if (++resolutions === 2) { enteredResolver(); await resolverGate; }
      return originalResolve(...args);
    };
    const operations: Promise<unknown>[] = [];
    try {
      await startServerOnce(h.controller); h.removeBeta();
      operations.push(pauseRemovedWorkspaceRootOnce(h.controller, [beta])); await gate.stopped;
      operations.push(h.controller.selectWorkspaceRoot()); gate.release(); await resolving;
      let terminalSettled = false;
      operations.push(stopServerOnce(h.controller).then(() => { terminalSettled = true; }));
      await new Promise<void>(resolve => setImmediate(resolve));
      assert.strictEqual(terminalSettled, false, 'shutdown must retain ownership of pending recovery setup');
      releaseResolver(); await Promise.all(operations);
      assert.strictEqual(h.controller.isRunning(), false);
      const state = h.controller as unknown as { workspaceRoot?: string };
      assert.strictEqual(state.workspaceRoot, undefined);
      await stopServerOnce(h.controller);
      assert.strictEqual(h.controller.isRunning(), false, 'repeated shutdown cannot leave a late client');
      assert.deepStrictEqual(h.events.filter(e => e.startsWith('start:')), [
        `start:${beta.uri.fsPath}`, `start:${alpha.uri.fsPath}`
      ]);
      assert.strictEqual(h.events.filter(e => e === `stop:${beta.uri.fsPath}`).length, 1);
      assert.strictEqual(h.events.filter(e => e === `stop:${alpha.uri.fsPath}`).length, 1);
    } finally {
      gate.release(); releaseResolver(); await Promise.allSettled(operations);
      await stopServerOnce(h.controller); await h.controller.stop();
    }
  });

  test('a newer A selection during B startup is reconciled after the in-flight replacement', async () => {
    const h = harness(); const gate = h.holdStart(beta.uri.fsPath);
    try {
      await startServerOnce(h.controller); h.select(beta.uri.fsPath);
      const first = synchronizeWorkspaceRootOnce(h.controller); await gate.started;
      h.select(alpha.uri.fsPath);
      const newer = synchronizeWorkspaceRootOnce(h.controller);
      gate.release(); await Promise.all([first, newer]);
      assert.deepStrictEqual(h.events.filter(e => e.startsWith('start:')), [
        `start:${alpha.uri.fsPath}`, `start:${beta.uri.fsPath}`, `start:${alpha.uri.fsPath}`
      ]);
      assert.strictEqual(h.events.filter(e => e === `stop:${beta.uri.fsPath}`).length, 1);
    } finally { gate.release(); await stopServerOnce(h.controller); }
  });

  test('a disabled replacement settles without repeatedly trying to start B', async () => {
    const h = harness();
    try {
      await startServerOnce(h.controller); h.select(beta.uri.fsPath); h.disable();
      await synchronizeWorkspaceRootOnce(h.controller);
      assert.strictEqual(h.controller.isRunning(), false);
      assert.deepStrictEqual(h.events.filter(e => e.startsWith('start:')), [`start:${alpha.uri.fsPath}`]);
    } finally { await stopServerOnce(h.controller); }
  });

  test('the existing first-folder startup entry can resume after a removed root', async () => {
    const h = harness(); h.select(beta.uri.fsPath);
    try {
      await startServerOnce(h.controller); h.removeBeta();
      await pauseRemovedWorkspaceRootOnce(h.controller, [beta]);
      await startAfterFirstWorkspaceFolder(h.controller);
      assert.deepStrictEqual(h.events.filter(e => e.startsWith('start:')), [
        `start:${beta.uri.fsPath}`, `start:${alpha.uri.fsPath}`
      ]);
    } finally { await stopServerOnce(h.controller); }
  });

  test('navigation already awaiting a document cannot show it after stop', async () => {
    const h = harness();
    let release!: (document: vscode.TextDocument) => void;
    let entered!: () => void;
    const opened = new Promise<void>(resolve => { entered = resolve; });
    const document = new Promise<vscode.TextDocument>(resolve => { release = resolve; });
    h.runtime.openTextDocument = () => { entered(); return document; };
    try {
      await startServerOnce(h.controller);
      const uri = vscode.Uri.file('/ws/alpha/tests/contract.rs');
      const activeEditor = vscode.window.activeTextEditor;
      const navigation = h.controller.openRelatedTest({ uri: uri.toString() });
      await opened; await h.controller.stop(); release({ uri } as vscode.TextDocument);
      await navigation;
      assert.strictEqual(vscode.window.activeTextEditor, activeEditor);
      assert.match(h.infos.at(-1) ?? '', /requires a running.*workspace root/);
    } finally { release({} as vscode.TextDocument); await stopServerOnce(h.controller); }
  });

  test('context already awaiting the old session cannot copy after stop', async () => {
    const h = harness();
    let release!: (value: unknown) => void;
    let entered!: () => void;
    const requested = new Promise<void>(resolve => { entered = resolve; });
    const response = new Promise<unknown>(resolve => { release = resolve; });
    const copied: string[] = [];
    h.setRequest(() => { entered(); return response; });
    h.runtime.writeClipboard = async text => { copied.push(text); };
    try {
      await startServerOnce(h.controller);
      const copying = h.controller.copyContext({
        uri: vscode.Uri.file('/ws/alpha/src/alpha.rs').toString(), seam_id: 'cached-alpha'
      });
      await requested; await h.controller.stop();
      release({ schema_version: '0.5', total_packets: 1, packets: [{ seam_id: 'cached-alpha' }] });
      await copying;
      assert.deepStrictEqual(copied, []);
      assert.match(h.infos.at(-1) ?? '', /requires a running.*workspace root/);
    } finally { release(undefined); await stopServerOnce(h.controller); }
  });
});
