//
// Post-start crash recovery (#1744).
//

import * as assert from 'assert';
import * as vscode from 'vscode';
import { CloseAction, LanguageClientOptions, State } from 'vscode-languageclient/node';
import {
  CRASH_RECOVERY_BACKOFF_MS,
  CRASH_RECOVERY_SESSION_BUDGET,
  CrashRecoveryController,
  crashRecoveryDelayMs
} from '../../src/crashRecovery';
import {
  RiprClientController,
  RiprClientRuntime,
  SERVER_CRASH_RESTART_ACTION
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

suite('crashRecoveryDelayMs (#1744)', () => {
  test('returns bounded backoff for failures under the session budget', () => {
    assert.strictEqual(crashRecoveryDelayMs(1), CRASH_RECOVERY_BACKOFF_MS[0]);
    assert.strictEqual(crashRecoveryDelayMs(2), CRASH_RECOVERY_BACKOFF_MS[1]);
    assert.strictEqual(crashRecoveryDelayMs(CRASH_RECOVERY_SESSION_BUDGET), undefined);
  });
});

suite('CrashRecoveryController (#1744)', () => {
  test('opens the circuit after three failed sessions with one pending retry at a time', () => {
    const scheduled: number[] = [];
    const pending: Array<() => void> = [];
    const controller = new CrashRecoveryController((callback, delayMs) => {
      scheduled.push(delayMs);
      pending.push(callback);
      return setTimeout(() => undefined, 86_400_000);
    });
    let generation = 0;
    let retries = 0;
    const run = () =>
      controller.handleUnexpectedSessionEnd({
        startGeneration: generation,
        readStartGeneration: () => generation,
        onRetry: () => {
          retries += 1;
        }
      });

    assert.strictEqual(run(), 'retry_scheduled');
    assert.strictEqual(run(), 'ignored', 'only one pending retry is allowed');
    pending.shift()?.();
    assert.strictEqual(retries, 1);

    assert.strictEqual(run(), 'retry_scheduled');
    assert.strictEqual(run(), 'ignored');
    pending.shift()?.();
    assert.strictEqual(retries, 2);

    assert.strictEqual(run(), 'circuit_open');
    assert.ok(controller.isCircuitOpen());
    assert.strictEqual(run(), 'ignored');
    assert.strictEqual(scheduled.length, 2);
  });

  test('intentional stops and explicit reset do not charge the budget', () => {
    const controller = new CrashRecoveryController(() => setTimeout(() => undefined, 0));
    controller.beginIntentionalStop();
    assert.strictEqual(
      controller.handleUnexpectedSessionEnd({
        startGeneration: 0,
        readStartGeneration: () => 0,
        onRetry: () => undefined
      }),
      'ignored'
    );
    controller.endIntentionalStop();
    controller.resetCircuit();
    assert.strictEqual(
      controller.handleUnexpectedSessionEnd({
        startGeneration: 0,
        readStartGeneration: () => 0,
        onRetry: () => undefined
      }),
      'retry_scheduled'
    );
  });
});

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

function controllerStatus(controller: RiprClientController): { kind: string; nextStep?: string; detail?: string } {
  return (controller as unknown as { status: { kind: string; nextStep?: string; detail?: string } }).status;
}

function simpleRecoveryHarness(): {
  controller: RiprClientController;
  pendingRetries: Array<() => void>;
  closed(): { action: CloseAction; handled?: boolean };
} {
  const pendingRetries: Array<() => void> = [];
  const scheduler = new CrashRecoveryController((callback) => {
    pendingRetries.push(callback);
    return setTimeout(() => undefined, 0);
  });
  let capturedOptions: LanguageClientOptions | undefined;
  const stateListeners: Array<(event: { oldState: State; newState: State }) => void> = [];
  let state = State.Stopped;
  const runtime: RiprClientRuntime = {
    getConfig: () => enabledConfig,
    workspaceRootState: () => ({ kind: 'singleRoot', root: '/workspace', roots: ['/workspace'] }),
    workspaceFolders: () => [],
    restartWorkspaceRoot: async () => undefined,
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
          return { dispose: () => undefined };
        },
        sendRequest: async () => undefined,
        setTrace: () => undefined,
        start: async () => {
          fake.setState(State.Running);
        },
        stop: async () => {
          fake.setState(State.Stopped);
        }
      };
      return fake;
    },
    createFileSystemWatcher: () => ({} as unknown as vscode.FileSystemWatcher),
    readFile: async () => undefined,
    runRipr: async () => '',
    writeClipboard: async () => undefined,
    isWorkspaceTrusted: () => true,
    showInformationMessage: async () => undefined,
    showWarningMessage: async () => undefined,
    showErrorMessage: async () => undefined
  };
  const output = { appendLine: () => undefined, show: () => undefined } as unknown as vscode.LogOutputChannel;
  const controller = new RiprClientController(
    {} as unknown as vscode.ExtensionContext,
    output,
    runtime,
    undefined,
    undefined,
    scheduler
  );
  return {
    controller,
    pendingRetries,
    closed: () => {
      assert.ok(capturedOptions?.errorHandler, 'errorHandler required');
      const result = capturedOptions!.errorHandler!.closed();
      assert.ok(result && typeof result === 'object' && !('then' in result));
      return result as { action: CloseAction; handled?: boolean };
    }
  };
}

function recoveryHarness(): {
  controller: RiprClientController;
  client: FakeClient;
  resolveCalls: number;
  pendingRetries: Array<() => void>;
  closed(): { action: CloseAction; handled?: boolean };
} {
  let capturedOptions: LanguageClientOptions | undefined;
  let client: FakeClient | undefined;
  let resolveCalls = 0;
  let startAttempts = 0;
  const pendingRetries: Array<() => void> = [];
  const stateListeners: Array<(event: { oldState: State; newState: State }) => void> = [];
  let state = State.Stopped;
  const scheduler = new CrashRecoveryController((callback, delayMs) => {
    assert.ok(CRASH_RECOVERY_BACKOFF_MS.includes(delayMs), `unexpected delay ${delayMs}`);
    pendingRetries.push(callback);
    return setTimeout(() => undefined, 86_400_000);
  });

  const runtime: RiprClientRuntime = {
    getConfig: () => enabledConfig,
    workspaceRootState: () => ({ kind: 'singleRoot', root: '/workspace', roots: ['/workspace'] }),
    workspaceFolders: () => [],
    restartWorkspaceRoot: async () => undefined,
    showQuickPick: async () => undefined,
    resolveServer: async () => {
      resolveCalls += 1;
      return {
        command: '/sentinel/trusted/ripr',
        source: 'configured',
        detail: 'sentinel server',
        installationState: 'unmanaged',
        compatibilityResult: compatibleLspEvidence
      };
    },
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
          return { dispose: () => undefined };
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
    showErrorMessage: async () => undefined
  };
  const output = {
    appendLine: () => undefined,
    show: () => undefined
  } as unknown as vscode.LogOutputChannel;
  const controller = new RiprClientController(
    {} as unknown as vscode.ExtensionContext,
    output,
    runtime,
    undefined,
    undefined,
    scheduler
  );
  return {
    controller,
    get client() {
      assert.ok(client, 'client must exist');
      return client;
    },
    get resolveCalls() {
      return resolveCalls;
    },
    pendingRetries,
    closed: () => {
      assert.ok(capturedOptions?.errorHandler, 'errorHandler required');
      const result = capturedOptions.errorHandler.closed();
      assert.ok(result && typeof result === 'object' && !('then' in result));
      return result as { action: CloseAction; handled?: boolean };
    }
  };
}

suite('RiprClientController crash recovery (#1744)', () => {
  test('unexpected closes schedule bounded retries that re-resolve the server', async () => {
    const h = recoveryHarness();
    await h.controller.start();
    assert.strictEqual(h.resolveCalls, 1);

    h.closed();
    assert.strictEqual(h.pendingRetries.length, 1);
    assert.strictEqual(controllerStatus(h.controller).kind, 'serverFailed');
    assert.ok((controllerStatus(h.controller).nextStep ?? '').includes(SERVER_CRASH_RESTART_ACTION));
    assert.ok((controllerStatus(h.controller).detail ?? '').includes('sentinel server'));

    h.pendingRetries.shift()?.();
    await new Promise((resolve) => setTimeout(resolve, 0));
    assert.strictEqual(h.resolveCalls, 2, 'retries must re-enter resolution');

    h.pendingRetries.shift()?.();
    await new Promise((resolve) => setTimeout(resolve, 0));
    assert.strictEqual(h.resolveCalls, 3);
    assert.strictEqual(controllerStatus(h.controller).kind, 'circuitOpen');
    assert.ok((controllerStatus(h.controller).nextStep ?? '').includes(SERVER_CRASH_RESTART_ACTION));
  });

  test('explicit restart resets the circuit and cancels a stale retry timer', async () => {
    const h = simpleRecoveryHarness();
    await h.controller.start();
    h.closed();
    assert.strictEqual(h.pendingRetries.length, 1);
    await h.controller.restart();
    h.pendingRetries[0]?.();
    await new Promise((resolve) => setTimeout(resolve, 0));
    assert.notStrictEqual(controllerStatus(h.controller).kind, 'circuitOpen');
  });
});
