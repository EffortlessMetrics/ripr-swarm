import * as assert from 'assert';
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import * as vscode from 'vscode';
import { RiprClientController, RiprClientRuntime } from '../../src/client';
import { RiprConfig } from '../../src/config';
import { currentRiprPlatform } from '../../src/platform';
import {
  missingServerActions,
  ResolveFailure,
  resolveServer,
  ServerResolverRuntime
} from '../../src/serverResolver';

function config(overrides: Partial<RiprConfig> = {}): RiprConfig {
  return {
    enabled: true,
    serverPath: '',
    serverArgs: ['lsp', '--stdio'],
    autoDownload: false,
    serverVersion: '0.10.0',
    downloadBaseUrl: '',
    checkMode: 'draft',
    baseRef: 'origin/main',
    includeUnchangedTests: true,
    seamDiagnostics: true,
    diagnosticProfile: 'actionable',
    traceServer: 'off',
    ...overrides
  };
}

function stubContext(root: string): vscode.ExtensionContext {
  return {
    extensionUri: vscode.Uri.file(path.join(root, 'extension')),
    globalStorageUri: vscode.Uri.file(path.join(root, 'storage')),
    extension: { packageJSON: { version: '0.10.0' } }
  } as unknown as vscode.ExtensionContext;
}

function stubOutput(): vscode.OutputChannel {
  return { appendLine: () => undefined } as unknown as vscode.OutputChannel;
}

const failingRuntime: ServerResolverRuntime = {
  probeCandidate: async (_command, _source, detail) => ({
    message: `${detail} could not start.`,
    detail: 'fixture: no such binary'
  })
};

suite('Missing server message actions (#8588)', () => {
  test('only the missing-binary class offers Copy Install Command', () => {
    assert.deepStrictEqual(missingServerActions({ kind: 'missing_binary' }), [
      'Open Settings',
      'Copy Diagnostic',
      'Copy Install Command',
      'Retry'
    ]);
    for (const kind of ['download_failed', 'configured_path', 'invalid_version', 'server_unusable'] as const) {
      const actions = missingServerActions({ kind });
      assert.ok(!actions.includes('Copy Install Command'), `${kind} must not offer the install command`);
      assert.ok(actions.includes('Copy Diagnostic'), `${kind} must offer Copy Diagnostic`);
      assert.ok(actions.includes('Retry'), `${kind} must offer Retry`);
    }
    // An unclassified failure cannot be shown to be a missing binary.
    assert.ok(!missingServerActions({}).includes('Copy Install Command'));
  });

  test('a clean fall-through with no binary anywhere is missing_binary', async function () {
    if (!currentRiprPlatform()) {
      this.skip();
      return;
    }
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'ripr-8588-missing-'));
    try {
      const result = await resolveServer(stubContext(root), config(), stubOutput(), failingRuntime);
      assert.ok(!('command' in result), JSON.stringify(result));
      if ('command' in result) {
        return;
      }
      assert.strictEqual(result.kind, 'missing_binary', JSON.stringify(result));
      assert.ok(missingServerActions(result).includes('Copy Install Command'));
    } finally {
      fs.rmSync(root, { recursive: true, force: true });
    }
  });

  test('a failed download is download_failed, not missing_binary', async function () {
    if (!currentRiprPlatform()) {
      this.skip();
      return;
    }
    this.timeout(60_000);
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'ripr-8588-download-'));
    try {
      const result = await resolveServer(
        stubContext(root),
        config({ autoDownload: true, serverVersion: '0.10.0', downloadBaseUrl: 'https://sentinel.invalid/ripr' }),
        stubOutput(),
        failingRuntime
      );
      assert.ok(!('command' in result), JSON.stringify(result));
      if ('command' in result) {
        return;
      }
      assert.strictEqual(result.kind, 'download_failed', JSON.stringify(result));
      assert.ok(!missingServerActions(result).includes('Copy Install Command'));
    } finally {
      fs.rmSync(root, { recursive: true, force: true });
    }
  });

  test('configured-path and invalid-version failures carry their own classes', async () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'ripr-8588-classes-'));
    try {
      const configured = await resolveServer(
        stubContext(root),
        config({ serverPath: path.join(root, 'nope', 'ripr.exe') }),
        stubOutput(),
        failingRuntime
      );
      assert.ok(!('command' in configured), JSON.stringify(configured));
      if (!('command' in configured)) {
        assert.strictEqual(configured.kind, 'configured_path');
        assert.ok(!missingServerActions(configured).includes('Copy Install Command'));
      }
      const invalid = await resolveServer(
        stubContext(root),
        config({ serverVersion: 'not-a-version!!' }),
        stubOutput(),
        failingRuntime
      );
      assert.ok(!('command' in invalid), JSON.stringify(invalid));
      if (!('command' in invalid)) {
        assert.strictEqual(invalid.kind, 'invalid_version');
        assert.ok(!missingServerActions(invalid).includes('Copy Install Command'));
      }
    } finally {
      fs.rmSync(root, { recursive: true, force: true });
    }
  });

  test('the popup offers the install command only for missing_binary and copies it verbatim', async () => {
    const missing = await popupFor({ message: 'ripr server is not available.', detail: 'fixture', kind: 'missing_binary' }, 'Copy Install Command');
    assert.ok(missing.items.includes('Copy Install Command'), JSON.stringify(missing.items));
    assert.deepStrictEqual(missing.clipboard, ['cargo install ripr']);

    const download = await popupFor(
      { message: 'ripr server is not available.', detail: 'GET https://example.invalid/x failed with HTTP 500.', kind: 'download_failed' },
      'Copy Diagnostic'
    );
    assert.ok(!download.items.includes('Copy Install Command'), JSON.stringify(download.items));
    assert.ok(download.items.includes('Copy Diagnostic') && download.items.includes('Retry'));
    assert.ok(download.clipboard.some((text) => text.includes('HTTP 500')), JSON.stringify(download.clipboard));
  });
});

async function popupFor(
  failure: ResolveFailure,
  selection: string
): Promise<{ items: string[]; clipboard: string[] }> {
  const errorCalls: Array<{ message: string; items: string[] }> = [];
  const clipboard: string[] = [];
  const runtime: RiprClientRuntime = {
    getConfig: () => config(),
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
    writeClipboard: async (text: string) => {
      clipboard.push(text);
    },
    isWorkspaceTrusted: () => true,
    showInformationMessage: async () => undefined,
    showWarningMessage: async () => undefined,
    showErrorMessage: async (message: string, ...items: string[]) => {
      errorCalls.push({ message, items });
      return selection;
    }
  };
  const output = { appendLine: () => undefined, show: () => undefined } as unknown as vscode.LogOutputChannel;
  const controller = new RiprClientController({} as unknown as vscode.ExtensionContext, output, runtime);
  await controller.start();
  assert.strictEqual(errorCalls.length, 1, `expected one error popup, got ${JSON.stringify(errorCalls)}`);
  return { items: errorCalls[0].items, clipboard };
}
