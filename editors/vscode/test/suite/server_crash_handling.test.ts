//
// Mid-session language-server crash handling (#9370 section 1, #1744 recovery).
//

import * as assert from 'assert';
import * as vscode from 'vscode';
import { CloseAction, LanguageClientOptions, State } from 'vscode-languageclient/node';
import {
  CRASH_RECOVERY_SESSION_BUDGET,
  CrashRecoveryController
} from '../../src/crashRecovery';
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
  flushRetry: () => void;
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
  const retryCallbacks: Array<() => void> = [];
  let restartCalls = 0;
  let startAttempts = 0;
  let stopClosed: { action: CloseAction; handled?: boolean } | undefined;
  const stateListeners: Array<(event: { oldState: State; newState: State }) => void> = [];
  let state = State.Stopped;
  const crashRecovery = new CrashRecoveryController((callback) => {
    retryCallbacks.push(callback);
    return setTimeout(() => undefined, 86_400_000);
  });

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
          startAttempts += 1;
          fake.setState(State.Starting);
          if (startAttempts === 1) {
            fake.setState(State.Running);
            return;
          }
          fake.setState(State.StartFailed);
          throw new Error('simulated recovery start failure');
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
    runtime,
    undefined,
    undefined,
    crashRecovery
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
    flushRetry: () => {
      const callback = retryCallbacks.shift();
      assert.ok(callback, 'expected a pending retry callback');
      callback();
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

suite('Server crash handling (#9370 section 1, #1744)', () => {
  test('SERVER_CRASH_RESTART_LIMIT matches the #1744 session budget', () => {
    assert.strictEqual(SERVER_CRASH_RESTART_LIMIT, CRASH_RECOVERY_SESSION_BUDGET);
  });

  test('closed() never asks the library to restart; controller schedules bounded retries then circuit_open', async () => {
    const h = crashHarness();
    await h.controller.start();
    assert.ok(h.controller.isRunning(), 'a started fake client with Running state is running');

    h.closed();
    assert.strictEqual(h.errorCalls.length, 0, 'under-budget closes must not notify until the circuit opens');
    assert.strictEqual(controllerStatus(h.controller).kind, 'serverFailed');
    h.flushRetry();
    await new Promise((resolve) => setTimeout(resolve, 0));
    h.flushRetry();
    await new Promise((resolve) => setTimeout(resolve, 0));

    assert.strictEqual(h.errorCalls.length, 1, 'exactly one crash message after the circuit opens');
    assert.deepStrictEqual(h.errorCalls[0].items, [SERVER_CRASH_RESTART_ACTION]);
    assert.strictEqual(controllerStatus(h.controller).kind, 'circuitOpen');

    h.resolveError(SERVER_CRASH_RESTART_ACTION);
    await new Promise((resolve) => setTimeout(resolve, 0));
    assert.strictEqual(h.restartCalls, 1, 'Restart Server must run ripr.restartServer via restartWorkspaceRoot');
  });

  test('Stopped after a scheduled retry does not paint circuit_open early', async () => {
    const h = crashHarness();
    await h.controller.start();
    h.closed();
    h.client.setState(State.Stopped);
    assert.notStrictEqual(controllerStatus(h.controller).kind, 'circuitOpen');
  });

  test('StartFailed after a live session is charged through recovery restart failure', async () => {
    const h = crashHarness();
    await h.controller.start();
    h.closed();
    h.flushRetry();
    await new Promise((resolve) => setTimeout(resolve, 0));
    assert.strictEqual(h.errorCalls.length, 0);
    assert.notStrictEqual(controllerStatus(h.controller).kind, 'circuitOpen');
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
