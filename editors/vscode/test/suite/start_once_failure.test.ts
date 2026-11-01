import * as assert from 'assert';
import * as vscode from 'vscode';
import { RiprClientController, RiprClientRuntime } from '../../src/client';
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

function trustedRuntime(
  createLanguageClient: RiprClientRuntime['createLanguageClient']
): RiprClientRuntime {
  return {
    getConfig: () => enabledConfig,
    workspaceRootState: () => ({
      kind: 'singleRoot',
      root: '/workspace',
      roots: ['/workspace']
    }),
    workspaceFolders: () => [],
    showQuickPick: async () => undefined,
    resolveServer: async () => ({
      command: '/sentinel/trusted/ripr',
      source: 'configured',
      detail: 'sentinel server',
      installationState: 'unmanaged',
      compatibilityResult: compatibleLspEvidence
    }),
    createLanguageClient,
    createFileSystemWatcher: () => ({} as unknown as vscode.FileSystemWatcher),
    readFile: async () => undefined,
    runRipr: async () => '',
    writeClipboard: async () => undefined,
    isWorkspaceTrusted: () => true,
    showInformationMessage: async () => undefined,
    showWarningMessage: async () => undefined,
    showErrorMessage: async () => undefined
  };
}

suite('startOnce failure notification (#5315)', () => {
  test('a failed handshake shows exactly one actionable warning', async () => {
    const warningCalls: Array<{ message: string; items: string[] }> = [];
    const runtime = trustedRuntime(() => ({
      onNotification: () => ({ dispose: () => undefined }),
      sendRequest: async () => undefined,
      setTrace: () => undefined,
      start: async () => {
        throw new Error('sentinel handshake failure');
      },
      stop: async () => undefined
    }));
    runtime.showWarningMessage = async (message: string, ...items: string[]) => {
      warningCalls.push({ message, items });
      return undefined;
    };

    const controller = new RiprClientController(
      {} as unknown as vscode.ExtensionContext,
      { appendLine: () => undefined, show: () => undefined } as unknown as vscode.LogOutputChannel,
      runtime
    );

    await assert.rejects(controller.start(), /sentinel handshake failure/);
    await controller.settleStartFailureNotification();

    assert.strictEqual(warningCalls.length, 1, 'expected exactly one start-failure warning');
    assert.match(warningCalls[0].message, /ripr server failed to start: sentinel handshake failure/);
    assert.ok(
      warningCalls[0].items.includes('Show Output'),
      `warning must offer Show Output: ${JSON.stringify(warningCalls[0])}`
    );
    assert.strictEqual(controller.isRunning(), false, 'failed start must leave a clean idle session');
    const status = (controller as unknown as { status: { kind: string } }).status;
    assert.strictEqual(status.kind, 'serverFailed');
  });

  test('a successful start shows no start-failure warning', async () => {
    const warningCalls: string[] = [];
    const runtime = trustedRuntime(() => ({
      onNotification: () => ({ dispose: () => undefined }),
      sendRequest: async () => undefined,
      setTrace: () => undefined,
      start: async () => undefined,
      stop: async () => undefined
    }));
    runtime.showWarningMessage = async (message: string) => {
      warningCalls.push(message);
      return undefined;
    };

    const controller = new RiprClientController(
      {} as unknown as vscode.ExtensionContext,
      { appendLine: () => undefined, show: () => undefined } as unknown as vscode.LogOutputChannel,
      runtime
    );

    await controller.start();
    await controller.settleStartFailureNotification();

    assert.strictEqual(warningCalls.length, 0, 'successful start must not toast a failure');
    assert.strictEqual(controller.isRunning(), true);
  });

  test('concurrent start() calls coalesce and emit one failure warning', async () => {
    const warningCalls: string[] = [];
    let clientStartCalls = 0;
    const runtime = trustedRuntime(() => ({
      onNotification: () => ({ dispose: () => undefined }),
      sendRequest: async () => undefined,
      setTrace: () => undefined,
      start: async () => {
        clientStartCalls += 1;
        throw new Error('sentinel concurrent start failure');
      },
      stop: async () => undefined
    }));
    runtime.showWarningMessage = async (message: string) => {
      warningCalls.push(message);
      return undefined;
    };

    const controller = new RiprClientController(
      {} as unknown as vscode.ExtensionContext,
      { appendLine: () => undefined, show: () => undefined } as unknown as vscode.LogOutputChannel,
      runtime
    );

    const first = controller.start();
    const second = controller.start();
    await assert.rejects(Promise.all([first, second]), /sentinel concurrent start failure/);
    await controller.settleStartFailureNotification();

    assert.strictEqual(clientStartCalls, 1, 'concurrent starts must not double-spawn the client');
    assert.strictEqual(warningCalls.length, 1, 'concurrent failure must not duplicate the toast');
    assert.strictEqual(controller.isRunning(), false);
  });
});
