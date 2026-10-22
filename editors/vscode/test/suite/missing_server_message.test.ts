import * as assert from 'assert';
import * as vscode from 'vscode';
import {
  missingServerMessageActions,
  resolutionFailureGuidance,
  RiprClientController,
  RiprClientRuntime
} from '../../src/client';
import { RiprConfig } from '../../src/config';
import { classifyDownloadFailureKind, ResolveFailure, ResolveFailureKind } from '../../src/serverResolver';
import { compatibleLspEvidence } from './testCompatibility';

const enabledConfig: RiprConfig = {
  enabled: true,
  serverPath: '',
  serverArgs: ['lsp', '--stdio'],
  autoDownload: true,
  serverVersion: 'sentinel-trusted-version',
  downloadBaseUrl: 'https://sentinel.invalid/ripr',
  checkMode: 'draft',
  baseRef: 'origin/main',
  includeUnchangedTests: true,
  seamDiagnostics: true,
  diagnosticProfile: 'actionable',
  traceServer: 'off'
};

const COPY_INSTALL = 'Copy Install Command';
const COPY_DIAGNOSTIC = 'Copy Diagnostic';
const SHOW_OUTPUT = 'Show Output';
const RETRY = 'Retry';
const OPEN_SETTINGS = 'Open Settings';

const NON_INSTALL_KINDS: ResolveFailureKind[] = ['download', 'http', 'checksum', 'manifest', 'other'];

suite('Missing-server message actions (#8588)', () => {
  test('Copy Install Command is offered only for a missing binary', () => {
    const missing = missingServerMessageActions('missing_binary');
    assert.deepStrictEqual(missing, [OPEN_SETTINGS, COPY_DIAGNOSTIC, COPY_INSTALL, RETRY]);
    for (const kind of NON_INSTALL_KINDS) {
      const actions = missingServerMessageActions(kind);
      assert.ok(
        !actions.includes(COPY_INSTALL),
        `${kind} must not offer ${COPY_INSTALL}: ${JSON.stringify(actions)}`
      );
      assert.ok(actions.includes(COPY_DIAGNOSTIC), `${kind} must offer ${COPY_DIAGNOSTIC}`);
      assert.ok(actions.includes(RETRY), `${kind} must offer ${RETRY}`);
      assert.ok(
        actions.includes(SHOW_OUTPUT),
        `${kind} must offer ${SHOW_OUTPUT} like the server-failed recovery`
      );
    }
  });

  test('non-install failures do not push cargo install in the default guidance', () => {
    const missingGuidance = resolutionFailureGuidance(
      { message: 'ripr server is not available.', detail: 'ripr on PATH was not found.', kind: 'missing_binary' },
      true
    );
    assert.ok(missingGuidance.includes('cargo install ripr'), missingGuidance);
    for (const kind of NON_INSTALL_KINDS) {
      const guidance = resolutionFailureGuidance(
        { message: 'ripr server is not available.', detail: `${kind} sentinel`, kind },
        true
      );
      assert.ok(
        !guidance.includes('cargo install ripr'),
        `${kind} guidance must not recommend cargo install: ${guidance}`
      );
    }
    const configured = resolutionFailureGuidance(
      {
        message: 'configured path could not start.',
        detail: 'missing',
        kind: 'other',
        remedy: 'Fix or clear ripr.server.path so it points at an existing ripr server executable.'
      },
      false
    );
    assert.ok(configured.startsWith('Fix or clear ripr.server.path'), configured);
    assert.ok(!configured.includes('cargo install ripr'), configured);
  });

  test('download error strings map onto the kinds that withhold install', () => {
    assert.strictEqual(
      classifyDownloadFailureKind('Server archive checksum mismatch. Expected abc, got def.'),
      'checksum'
    );
    assert.strictEqual(
      classifyDownloadFailureKind(
        'Server manifest at https://sentinel.invalid/manifest.json does not match the admitted manifest SHA-256 for its distribution generation.'
      ),
      'checksum'
    );
    assert.strictEqual(
      classifyDownloadFailureKind('GET https://sentinel.invalid/manifest.json failed with HTTP 500.'),
      'http'
    );
    assert.strictEqual(classifyDownloadFailureKind('Server manifest is not an object.'), 'manifest');
    assert.strictEqual(
      classifyDownloadFailureKind('Timed out while fetching https://sentinel.invalid/ripr.'),
      'download'
    );
    assert.strictEqual(
      classifyDownloadFailureKind('No prebuilt ripr server target is known for serenity/arm.'),
      'missing_binary'
    );
  });

  test('a checksum resolution failure does not offer Copy Install Command', async () => {
    const errorCalls: Array<{ message: string; items: string[] }> = [];
    const clipboardWrites: string[] = [];
    const { controller } = controllerForFailure(
      {
        message: 'ripr server is not available.',
        detail: 'Server archive checksum mismatch. Expected abc, got def.',
        kind: 'checksum'
      },
      errorCalls,
      clipboardWrites
    );

    await controller.start();

    assert.strictEqual(errorCalls.length, 1, `expected one error popup, got ${JSON.stringify(errorCalls)}`);
    assert.ok(!errorCalls[0].items.includes(COPY_INSTALL), JSON.stringify(errorCalls[0]));
    assert.ok(errorCalls[0].items.includes(COPY_DIAGNOSTIC), JSON.stringify(errorCalls[0]));
    assert.ok(errorCalls[0].items.includes(SHOW_OUTPUT), JSON.stringify(errorCalls[0]));
    assert.ok(errorCalls[0].items.includes(RETRY), JSON.stringify(errorCalls[0]));
    assert.ok(!errorCalls[0].message.includes('cargo install ripr'), errorCalls[0].message);
    assert.deepStrictEqual(clipboardWrites, []);
  });

  test('a missing-binary resolution failure still offers Copy Install Command', async () => {
    const errorCalls: Array<{ message: string; items: string[] }> = [];
    const clipboardWrites: string[] = [];
    const { controller } = controllerForFailure(
      {
        message: 'ripr server is not available.',
        detail: 'ripr on PATH was not found.',
        kind: 'missing_binary'
      },
      errorCalls,
      clipboardWrites,
      COPY_INSTALL
    );

    await controller.start();

    assert.strictEqual(errorCalls.length, 1, `expected one error popup, got ${JSON.stringify(errorCalls)}`);
    assert.ok(errorCalls[0].items.includes(COPY_INSTALL), JSON.stringify(errorCalls[0]));
    assert.ok(!errorCalls[0].items.includes(SHOW_OUTPUT), JSON.stringify(errorCalls[0]));
    assert.deepStrictEqual(clipboardWrites, ['cargo install ripr']);
  });

  test('http, download, and manifest failures keep Show Output instead of install', async () => {
    for (const kind of ['http', 'download', 'manifest'] as const) {
      const errorCalls: Array<{ message: string; items: string[] }> = [];
      const { controller } = controllerForFailure(
        {
          message: 'ripr server is not available.',
          detail: `${kind} sentinel failure`,
          kind
        },
        errorCalls,
        []
      );
      await controller.start();
      assert.strictEqual(errorCalls.length, 1, `${kind}: ${JSON.stringify(errorCalls)}`);
      assert.ok(!errorCalls[0].items.includes(COPY_INSTALL), `${kind}: ${JSON.stringify(errorCalls[0])}`);
      assert.ok(errorCalls[0].items.includes(SHOW_OUTPUT), `${kind}: ${JSON.stringify(errorCalls[0])}`);
    }
  });
});

function controllerForFailure(
  failure: ResolveFailure,
  errorCalls: Array<{ message: string; items: string[] }>,
  clipboardWrites: string[],
  selectedAction?: string
): { controller: RiprClientController } {
  const runtime: RiprClientRuntime = {
    getConfig: () => enabledConfig,
    workspaceRootState: () => ({ kind: 'singleRoot', root: '/workspace', roots: ['/workspace'] }),
    workspaceFolders: () => [],
    showQuickPick: async () => undefined,
    resolveServer: async () => failure,
    createLanguageClient: () => ({
      onNotification: () => ({ dispose: () => undefined }),
      sendRequest: async () => undefined,
      setTrace: () => undefined,
      start: async () => undefined,
      stop: async () => undefined
    }),
    createFileSystemWatcher: () => ({} as unknown as vscode.FileSystemWatcher),
    readFile: async () => undefined,
    runRipr: async () => '',
    writeClipboard: async (text) => {
      clipboardWrites.push(text);
    },
    isWorkspaceTrusted: () => true,
    showInformationMessage: async () => undefined,
    showWarningMessage: async () => undefined,
    showErrorMessage: async (message: string, ...items: string[]) => {
      errorCalls.push({ message, items });
      return selectedAction;
    }
  };
  const output = {
    appendLine: () => undefined,
    show: () => undefined
  } as unknown as vscode.LogOutputChannel;
  return {
    controller: new RiprClientController(
      {} as unknown as vscode.ExtensionContext,
      output,
      runtime
    )
  };
}
