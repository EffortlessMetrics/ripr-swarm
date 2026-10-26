import * as assert from 'assert';
import * as vscode from 'vscode';
import {
  RiprClientController, RiprClientRuntime, WorkspaceRootPickItem
} from '../../src/client';
import { RiprConfig } from '../../src/config';
import {
  resetLifecycleCoordinatorForTests,
  restartServerOnce,
  startServerOnce,
  stopServerOnce
} from '../../src/extension';
import { compatibleLspEvidence } from './testCompatibility';

// A manual answer pins the interleaving without timeouts, sleeps, or private fields.
function popupAnswer<T = string>(): {
  promise: Promise<T | undefined>;
  resolve: (answer: T | undefined) => void;
} {
  let resolveAnswer: (answer: T | undefined) => void = () => undefined;
  const promise = new Promise<T | undefined>((resolve) => { resolveAnswer = resolve; });
  return { promise, resolve: (answer) => resolveAnswer(answer) };
}

function pendingMissingServer(missingResolutions = 1) {
  const answer = popupAnswer();
  const answers = [answer];
  const events: string[] = [];
  let resolutions = 0;
  let created = 0;
  let started = 0;
  let stopped = 0;
  let disposed = 0;
  let popups = 0;
  let lifecycleRequests = 0;
  const serverRoots: Array<string | undefined> = [];
  const config: RiprConfig = {
    enabled: true, serverPath: '/sentinel/ripr', serverArgs: ['lsp', '--stdio'],
    autoDownload: false, serverVersion: 'sentinel-trusted-version',
    downloadBaseUrl: 'https://sentinel.invalid/ripr', checkMode: 'draft',
    baseRef: 'origin/main', includeUnchangedTests: true, seamDiagnostics: true,
    diagnosticProfile: 'actionable', traceServer: 'off'
  };
  const runtime: RiprClientRuntime = {
    getConfig: () => config,
    workspaceRootState: () => ({ kind: 'singleRoot', root: '/workspace', roots: ['/workspace'] }),
    workspaceFolders: () => [],
    restartWorkspaceRoot: async () => {
      lifecycleRequests += 1;
      await restartServerOnce(controller);
    },
    showQuickPick: async () => undefined,
    resolveServer: async () => {
      resolutions += 1;
      events.push(`resolve:${resolutions}`);
      return resolutions <= missingResolutions
        ? { message: 'ripr server is not available.', detail: 'sentinel missing binary', kind: 'missing_binary' }
        : {
          command: '/sentinel/ripr', source: 'configured', detail: 'sentinel recovered server',
          installationState: 'unmanaged', compatibilityResult: compatibleLspEvidence
        };
    },
    createLanguageClient: (_serverOptions, clientOptions) => {
      created += 1;
      serverRoots.push(clientOptions.workspaceFolder?.uri.fsPath);
      events.push('create');
      return {
        onNotification: () => ({ dispose: () => { disposed += 1; } }),
        sendRequest: async () => undefined,
        setTrace: () => undefined,
        start: async () => { started += 1; events.push('start'); },
        stop: async () => { stopped += 1; events.push('stop'); }
      };
    },
    createFileSystemWatcher: () => ({} as unknown as vscode.FileSystemWatcher),
    readFile: async () => undefined,
    runRipr: async () => '',
    writeClipboard: async () => undefined,
    isWorkspaceTrusted: () => true,
    showInformationMessage: async () => undefined,
    showWarningMessage: async () => undefined,
    showErrorMessage: async (_message, ...items) => {
      assert.ok(items.includes('Retry'), 'fixture must reach the missing-server Retry popup');
      popups += 1;
      events.push('popup');
      if (popups > 1) { answers.push(popupAnswer()); }
      return answers[popups - 1].promise;
    }
  };
  const output = { appendLine: () => undefined } as unknown as vscode.LogOutputChannel;
  const controller = new RiprClientController({} as vscode.ExtensionContext, output, runtime);
  return {
    answer, answers, controller, events, runtime, serverRoots,
    lifecycleRequests: () => lifecycleRequests,
    counts: () => ({ resolutions, created, started, stopped, disposed, popups })
  };
}

function pendingWorkspacePick(h: ReturnType<typeof pendingMissingServer>) {
  const answer = popupAnswer<WorkspaceRootPickItem>();
  const folders = ['alpha', 'beta'].map((name, index) => ({
    name, index, uri: vscode.Uri.file(`/workspace/${name}`)
  }));
  const offered: WorkspaceRootPickItem[][] = [];
  h.runtime.workspaceFolders = () => folders;
  h.runtime.workspaceRootState = () => ({
    kind: 'ambiguousMultiRoot', roots: folders.map(folder => folder.uri.fsPath),
    detail: 'choose a workspace root'
  });
  h.runtime.showQuickPick = async <T extends vscode.QuickPickItem>(items: T[]) => {
    offered.push(items as unknown as WorkspaceRootPickItem[]);
    return await answer.promise as T | undefined;
  };
  return { answer, offered, folders };
}

suite('Missing-server popup and terminal shutdown', () => {
  setup(() => { resetLifecycleCoordinatorForTests(); });

  test('Retry recovers while the extension lifecycle remains active', async () => {
    const h = pendingMissingServer();
    try {
      await startServerOnce(h.controller);
      assert.deepStrictEqual(h.counts(), {
        resolutions: 1, created: 0, started: 0, stopped: 0, disposed: 0, popups: 1
      });
      const handled = h.controller.settleServerMessage();
      h.answer.resolve('Retry');
      await handled;
      assert.strictEqual(h.controller.isRunning(), true);
      assert.deepStrictEqual(h.events, ['resolve:1', 'popup', 'resolve:2', 'create', 'start']);
      await stopServerOnce(h.controller);
      assert.strictEqual(h.controller.isRunning(), false);
      assert.strictEqual(h.counts().stopped, 1);
      assert.strictEqual(h.counts().disposed, 2);
    } finally {
      await h.controller.stop();
    }
  });

  test('dismissing the popup after terminal shutdown leaves the controller stopped', async () => {
    const h = pendingMissingServer();
    try {
      await startServerOnce(h.controller);
      assert.strictEqual(h.counts().popups, 1);
      await stopServerOnce(h.controller);
      h.events.push('terminal-stop-returned');
      const handled = h.controller.settleServerMessage();
      h.answer.resolve(undefined);
      await handled;
      assert.strictEqual(h.controller.isRunning(), false);
      assert.deepStrictEqual(h.events, ['resolve:1', 'popup', 'terminal-stop-returned']);
      assert.deepStrictEqual(h.counts(), {
        resolutions: 1, created: 0, started: 0, stopped: 0, disposed: 0, popups: 1
      });
    } finally {
      await h.controller.stop();
    }
  });

  test('late Retry cannot start a client after terminal shutdown', async () => {
    const h = pendingMissingServer();
    try {
      await startServerOnce(h.controller);
      assert.strictEqual(h.counts().popups, 1);
      assert.strictEqual(h.controller.isRunning(), false);
      await stopServerOnce(h.controller);
      h.events.push('terminal-stop-returned');
      // Prove shutdown was recorded and the official restart route rejects it.
      await assert.rejects(restartServerOnce(h.controller), /terminal shutdown has begun/);
      const handled = h.controller.settleServerMessage();
      h.answer.resolve('Retry');
      await handled;
      assert.deepStrictEqual(h.events, ['resolve:1', 'popup', 'terminal-stop-returned'],
        'late popup action must not resolve/create/start after terminal shutdown');
      assert.strictEqual(h.controller.isRunning(), false);
    } finally {
      // The current defect creates a fake session. Always reap its owned state.
      await h.controller.stop();
    }
  });

  test('dismissing an active popup still permits later explicit recovery', async () => {
    const h = pendingMissingServer();
    try {
      await startServerOnce(h.controller);
      h.answer.resolve(undefined);
      await h.controller.settleServerMessage();
      assert.strictEqual(h.lifecycleRequests(), 0);
      assert.deepStrictEqual(h.events, ['resolve:1', 'popup']);
      await restartServerOnce(h.controller);
      assert.deepStrictEqual(h.events, ['resolve:1', 'popup', 'resolve:2', 'create', 'start']);
    } finally { await stopServerOnce(h.controller); await h.controller.stop(); }
  });

  test('repeated Retry recovers after two missing-server results', async () => {
    const h = pendingMissingServer(2);
    try {
      await startServerOnce(h.controller);
      const first = h.controller.settleServerMessage();
      h.answer.resolve('Retry');
      await first;
      assert.strictEqual(h.answers.length, 2, 'the first Retry must reach a second popup');
      assert.strictEqual(h.controller.isRunning(), false);
      const second = h.controller.settleServerMessage();
      h.answers[1].resolve('Retry');
      await second;
      assert.strictEqual(h.lifecycleRequests(), 2);
      assert.deepStrictEqual(h.events, [
        'resolve:1', 'popup', 'resolve:2', 'popup', 'resolve:3', 'create', 'start'
      ]);
      assert.strictEqual(h.controller.isRunning(), true);
    } finally { await stopServerOnce(h.controller); await h.controller.stop(); }
  });

  test('two outstanding Retry answers both settle after terminal shutdown without recovery', async () => {
    const h = pendingMissingServer(2);
    try {
      await startServerOnce(h.controller);
      const first = h.controller.settleServerMessage();
      await startServerOnce(h.controller);
      const second = h.controller.settleServerMessage();
      assert.strictEqual(h.answers.length, 2, 'both independently pending popups must be reached');
      await stopServerOnce(h.controller);
      h.answer.resolve('Retry');
      h.answers[1].resolve('Retry');
      await Promise.all([first, second]);
      assert.deepStrictEqual(h.events, ['resolve:1', 'popup', 'resolve:2', 'popup']);
      assert.strictEqual(h.controller.isRunning(), false);
      assert.strictEqual(h.lifecycleRequests(), 2, 'both answers must enter the lifecycle owner');
    } finally { await h.controller.stop(); }
  });

  test('a delayed workspace choice uses the lifecycle owner while active', async () => {
    const h = pendingMissingServer(0);
    const picker = pendingWorkspacePick(h);
    try {
      const selection = h.controller.selectWorkspaceRoot();
      assert.strictEqual(picker.offered.length, 1);
      picker.answer.resolve(picker.offered[0][1]);
      await selection;
      assert.strictEqual(h.lifecycleRequests(), 1);
      assert.deepStrictEqual(h.serverRoots, [picker.folders[1].uri.fsPath]);
      assert.deepStrictEqual(h.events, ['resolve:1', 'create', 'start']);
    } finally { await stopServerOnce(h.controller); await h.controller.stop(); }
  });

  test('a workspace choice answered after terminal shutdown cannot recover', async () => {
    const h = pendingMissingServer(0);
    const picker = pendingWorkspacePick(h);
    const selection = h.controller.selectWorkspaceRoot();
    const rejected = assert.rejects(selection, /terminal shutdown has begun/);
    try {
      assert.strictEqual(picker.offered.length, 1);
      await stopServerOnce(h.controller);
      picker.answer.resolve(picker.offered[0][1]);
      await rejected;
      assert.strictEqual(h.lifecycleRequests(), 1);
      assert.deepStrictEqual(h.events, []);
      assert.strictEqual(h.controller.isRunning(), false);
    } finally {
      picker.answer.resolve(undefined);
      await Promise.allSettled([selection, rejected]);
      await h.controller.stop();
    }
  });

  test('cancelling a workspace choice after terminal shutdown settles without restart', async () => {
    const h = pendingMissingServer(0);
    const picker = pendingWorkspacePick(h);
    const selection = h.controller.selectWorkspaceRoot();
    try {
      assert.strictEqual(picker.offered.length, 1);
      await stopServerOnce(h.controller);
      picker.answer.resolve(undefined);
      await selection;
      assert.strictEqual(h.lifecycleRequests(), 0);
      assert.deepStrictEqual(h.events, []);
      assert.strictEqual(h.controller.isRunning(), false);
    } finally {
      picker.answer.resolve(undefined);
      await selection;
      await h.controller.stop();
    }
  });
});
