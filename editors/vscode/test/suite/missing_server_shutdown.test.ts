import * as assert from 'assert';
import * as vscode from 'vscode';
import { RiprClientController, RiprClientRuntime } from '../../src/client';
import { RiprConfig } from '../../src/config';
import {
  resetLifecycleCoordinatorForTests,
  restartServerOnce,
  startServerOnce,
  stopServerOnce
} from '../../src/extension';
import { compatibleLspEvidence } from './testCompatibility';

// A manual answer pins the interleaving without timeouts, sleeps, or private fields.
function popupAnswer(): {
  promise: Promise<string | undefined>;
  resolve: (answer: string | undefined) => void;
} {
  let resolveAnswer: (answer: string | undefined) => void = () => undefined;
  const promise = new Promise<string | undefined>((resolve) => { resolveAnswer = resolve; });
  return { promise, resolve: (answer) => resolveAnswer(answer) };
}

function pendingMissingServer() {
  const answer = popupAnswer();
  const events: string[] = [];
  let resolutions = 0;
  let created = 0;
  let started = 0;
  let stopped = 0;
  let disposed = 0;
  let popups = 0;
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
      await restartServerOnce(controller);
    },
    showQuickPick: async () => undefined,
    resolveServer: async () => {
      resolutions += 1;
      events.push(`resolve:${resolutions}`);
      return resolutions === 1
        ? { message: 'ripr server is not available.', detail: 'sentinel missing binary', kind: 'missing_binary' }
        : {
          command: '/sentinel/ripr', source: 'configured', detail: 'sentinel recovered server',
          installationState: 'unmanaged', compatibilityResult: compatibleLspEvidence
        };
    },
    createLanguageClient: () => {
      created += 1;
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
      return answer.promise;
    }
  };
  const output = { appendLine: () => undefined } as unknown as vscode.LogOutputChannel;
  const controller = new RiprClientController({} as vscode.ExtensionContext, output, runtime);
  return {
    answer, controller, events,
    counts: () => ({ resolutions, created, started, stopped, disposed, popups })
  };
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
});
