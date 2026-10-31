//
// Mid-session language-server crash handling (#9370 section 1).
//
// A fake client captures `clientOptions.errorHandler` and exposes lifecycle
// `state` so the suite can drive `closed` without spawning a server. The
// production path is the captured handler plus `onDidChangeState`, not a
// parallel test double of the restart policy.
//

import * as assert from 'assert';
import * as vscode from 'vscode';
import { CloseAction, LanguageClientOptions, State } from 'vscode-languageclient/node';
import {
  RiprClientController,
  RiprClientRuntime,
  SERVER_CRASH_RESTART_ACTION,
  SERVER_CRASH_RESTART_LIMIT
} from '../../src/client';
import { RiprConfig } from '../../src/config';
import { compatibleLspEvidence } from './testCompatibility';

const enabledConfig: RiprConfig = {
  enabled: true,
  serverPath: '/sentinel/trusted/ripr',
  serverArgs: ['lsp', '--stdio'],
  autoDownload: false,
  serverVersion: 'sentinel-trusted-version',
  downloadBaseUrl: 'https://sentinel.invalid/ripr',
  checkMode: 'draft',
  baseRef: 'origin/main',
  includeUnchangedTests: true,
  seamDiagnostics: true,
  diagnosticProfile: 'actionable',
  traceServer: 'off'
};

interface FakeClient {
  state: State;
  setState(next: State): void;
  onNotification(): vscode.Disposable;
  onDidChangeState(listener: (event: { oldState: State; newState: State }) => void): vscode.Disposable;
  sendRequest(): Promise<unknown>;
  setTrace(): void;
  start(): Promise<void>;
  stop(): Promise<void>;
}

interface CrashHarness {
  controller: RiprClientController;
  client: FakeClient;
  clientOptions: LanguageClientOptions;
  errorCalls: Array<{ message: string; items: string[] }>;
  restartCalls: number;
  outputLines: string[];
  stopClosed?: { action: CloseAction; handled?: boolean };
  resolveError: (choice: string | undefined) => void;
  closed(): { action: CloseAction; handled?: boolean };
}

function pendingChoice(): {
  promise: Promise<string | undefined>;
  resolve: (choice: string | undefined) => void;
} {
  let resolveChoice: (choice: string | undefined) => void = () => undefined;
  const promise = new Promise<string | undefined>((resolve) => {
    resolveChoice = resolve;
  });
  return { promise, resolve: (choice) => resolveChoice(choice) };
}

function crashHarness(): CrashHarness {
  let capturedOptions: LanguageClientOptions | undefined;
  let client: FakeClient | undefined;
  const errorCalls: Array<{ message: string; items: string[] }> = [];
  const outputLines: string[] = [];
  const answers: Array<ReturnType<typeof pendingChoice>> = [];
  let restartCalls = 0;
  let stopClosed: { action: CloseAction; handled?: boolean } | undefined;
  const stateListeners: Array<(event: { oldState: State; newState: State }) => void> = [];
  let state = State.Stopped;

  const runtime: RiprClientRuntime = {
    getConfig: () => enabledConfig,
    workspaceRootState: () => ({ kind: 'singleRoot', root: '/workspace', roots: ['/workspace'] }),
    workspaceFolders: () => [],
    restartWorkspaceRoot: async () => {
      restartCalls += 1;
    },
    showQuickPick: async () => undefined,
    resolveServer: async () => ({
      command: '/sentinel/trusted/ripr',
      source: 'configured',
      detail: 'sentinel server',
      installationState: 'unmanaged',
      compatibilityResult: compatibleLspEvidence
    }),
    createLanguageClient: (_serverOptions, clientOptions) => {
      capturedOptions = clientOptions;
      const fake: FakeClient = {
        get state() {
          return state;
        },
        setState(next: State) {
          const oldState = state;
          state = next;
          for (const listener of stateListeners) {
            listener({ oldState, newState: next });
          }
        },
        onNotification: () => ({ dispose: () => undefined }),
        onDidChangeState: (listener) => {
          stateListeners.push(listener);
          return {
            dispose: () => {
              const index = stateListeners.indexOf(listener);
              if (index >= 0) {
                stateListeners.splice(index, 1);
              }
            }
          };
        },
        sendRequest: async () => undefined,
        setTrace: () => undefined,
        start: async () => {
          fake.setState(State.Running);
        },
        stop: async () => {
          const handler = capturedOptions?.errorHandler;
          if (handler) {
            const result = handler.closed();
            if (result && typeof result === 'object' && !('then' in result)) {
              stopClosed = result as { action: CloseAction; handled?: boolean };
            }
          }
          fake.setState(State.Stopped);
        }
      };
      client = fake;
      return fake;
    },
    createFileSystemWatcher: () => ({} as unknown as vscode.FileSystemWatcher),
    readFile: async () => undefined,
    runRipr: async () => '',
    writeClipboard: async () => undefined,
    isWorkspaceTrusted: () => true,
    showInformationMessage: async () => undefined,
    showWarningMessage: async () => undefined,
    showErrorMessage: async (message, ...items) => {
      errorCalls.push({ message, items });
      const answer = pendingChoice();
      answers.push(answer);
      return answer.promise;
    }
  };
  const output = {
    appendLine: (line: string) => outputLines.push(line),
    show: () => undefined
  } as unknown as vscode.LogOutputChannel;
  const controller = new RiprClientController(
    {} as unknown as vscode.ExtensionContext,
    output,
    runtime
  );

  return {
    controller,
    get client() {
      assert.ok(client, 'createLanguageClient must run before the fake client is used');
      return client;
    },
    get clientOptions() {
      assert.ok(capturedOptions, 'start() must capture clientOptions');
      return capturedOptions;
    },
    errorCalls,
    get restartCalls() {
      return restartCalls;
    },
    get stopClosed() {
      return stopClosed;
    },
    outputLines,
    resolveError: (choice) => {
      const answer = answers[answers.length - 1];
      assert.ok(answer, 'expected a pending crash message');
      answer.resolve(choice);
    },
    closed: () => {
      const handler = capturedOptions?.errorHandler;
      assert.ok(handler, 'clientOptions must install errorHandler');
      const result = handler.closed();
      assert.ok(result && typeof result === 'object' && !('then' in result), 'closed() must return a sync result');
      return result as { action: CloseAction; handled?: boolean };
    }
  };
}

function controllerStatus(controller: RiprClientController): { kind: string } {
  return (controller as unknown as { status: { kind: string } }).status;
}

suite('Server crash handling (#9370 section 1)', () => {
  test('closed restarts until the limit, then shows a restart message and is not running', async () => {
    const h = crashHarness();
    await h.controller.start();
    assert.ok(h.controller.isRunning(), 'a started fake client with Running state is running');
    assert.ok(h.clientOptions.errorHandler, 'start() must install errorHandler on clientOptions');

    for (let i = 0; i < SERVER_CRASH_RESTART_LIMIT; i += 1) {
      const result = h.closed();
      assert.strictEqual(
        result.action,
        CloseAction.Restart,
        `close ${i + 1} of ${SERVER_CRASH_RESTART_LIMIT} must restart`
      );
      assert.ok(h.controller.isRunning(), `close ${i + 1} must leave the client running while the library restarts`);
      assert.strictEqual(h.errorCalls.length, 0, 'under-limit closes must not notify the user');
    }

    const exhausted = h.closed();
    assert.strictEqual(exhausted.action, CloseAction.DoNotRestart, 'over-limit close must stop restarting');
    assert.strictEqual(exhausted.handled, true, 'the library must not show its own crash toast');
    assert.strictEqual(h.errorCalls.length, 1, 'exactly one crash message');
    assert.ok(
      h.errorCalls[0].message.includes('stopped unexpectedly'),
      `message must be plain words: ${h.errorCalls[0].message}`
    );
    assert.ok(
      !h.errorCalls[0].message.includes('errorHandler'),
      `message must not leak implementation jargon: ${h.errorCalls[0].message}`
    );
    assert.deepStrictEqual(h.errorCalls[0].items, [SERVER_CRASH_RESTART_ACTION]);
    assert.ok(h.controller.isRunning(), 'DoNotRestart alone does not change client state');

    h.client.setState(State.Stopped);
    assert.strictEqual(h.controller.isRunning(), false, 'Stopped after the restart budget is exhausted is not running');
    assert.strictEqual(controllerStatus(h.controller).kind, 'serverFailed');
    assert.strictEqual(h.restartCalls, 0, 'the message must wait for the Restart Server action');

    h.resolveError(SERVER_CRASH_RESTART_ACTION);
    await new Promise((resolve) => setTimeout(resolve, 0));
    assert.strictEqual(h.restartCalls, 1, 'Restart Server must run ripr.restartServer via restartWorkspaceRoot');
  });

  test('Stopped after an under-limit close does not paint a crash and stays restarting', async () => {
    const h = crashHarness();
    await h.controller.start();
    const result = h.closed();
    assert.strictEqual(result.action, CloseAction.Restart);
    h.client.setState(State.Stopped);
    assert.strictEqual(
      h.controller.isRunning(),
      false,
      'Stopped is not running even when the library is about to restart'
    );
    assert.notStrictEqual(
      controllerStatus(h.controller).kind,
      'serverFailed',
      'an automatic restart must not show the exhausted-crash status'
    );
    assert.strictEqual(h.errorCalls.length, 0);
    h.client.setState(State.Starting);
    assert.ok(h.controller.isRunning(), 'Starting after an automatic restart still counts as running');
  });

  test('StartFailed after an automatic restart paints serverFailed and is not running', async () => {
    const h = crashHarness();
    await h.controller.start();
    const result = h.closed();
    assert.strictEqual(result.action, CloseAction.Restart);
    h.client.setState(State.StartFailed);
    assert.strictEqual(h.controller.isRunning(), false);
    assert.strictEqual(controllerStatus(h.controller).kind, 'serverFailed');
    assert.strictEqual(h.errorCalls.length, 0, 'a failed auto-restart uses status, not a second toast');
  });

  test('Stopped without closed() after a live session paints serverFailed', async () => {
    const h = crashHarness();
    await h.controller.start();
    h.client.setState(State.Stopped);
    assert.strictEqual(h.controller.isRunning(), false);
    assert.strictEqual(controllerStatus(h.controller).kind, 'serverFailed');
    assert.strictEqual(h.errorCalls.length, 0);
  });

  test('closed during an intentional stop does not restart or notify', async () => {
    const h = crashHarness();
    await h.controller.start();
    await h.controller.stop();
    assert.ok(h.stopClosed, 'client.stop must observe errorHandler.closed');
    assert.strictEqual(h.stopClosed.action, CloseAction.DoNotRestart);
    assert.strictEqual(h.stopClosed.handled, true);
    assert.strictEqual(h.errorCalls.length, 0);
    assert.strictEqual(h.controller.isRunning(), false);
  });
});
