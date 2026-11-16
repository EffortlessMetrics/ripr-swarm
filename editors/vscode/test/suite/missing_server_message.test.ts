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

const NON_INSTALL_KINDS: ResolveFailureKind[] = ['download', 'network', 'http', 'checksum', 'manifest', 'other'];

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
      classifyDownloadFailureKind('Refusing server archive member with unsafe path "../ripr".'),
      'download'
    );
    assert.strictEqual(
      classifyDownloadFailureKind('No prebuilt ripr server target is known for serenity/arm.'),
      'missing_binary'
    );
  });

  test('a download that never reached its host is a network failure (#13638)', () => {
    for (const message of [
      'Timed out while fetching https://sentinel.invalid/ripr.',
      'Could not fetch https://sentinel.invalid/manifest.json: getaddrinfo ENOTFOUND sentinel.invalid',
      'Could not fetch https://sentinel.invalid/manifest.json: getaddrinfo EAI_AGAIN sentinel.invalid',
      'Could not fetch https://sentinel.invalid/ripr.tar.gz: connect ECONNREFUSED 127.0.0.1:443',
      'Could not fetch https://sentinel.invalid/ripr.tar.gz: read ECONNRESET',
      'Could not fetch https://sentinel.invalid/ripr.tar.gz: socket hang up',
      'Manifest response from https://sentinel.invalid/manifest.json closed before completion.'
    ]) {
      assert.strictEqual(classifyDownloadFailureKind(message), 'network', message);
    }
    // An answer from the host, or a local refusal, is not a connection problem.
    for (const [message, kind] of [
      ['GET https://sentinel.invalid/manifest.json failed with HTTP 503.', 'http'],
      ['Server manifest is missing a string version or asset map.', 'manifest'],
      ['Refusing redirect from https://sentinel.invalid/a to disallowed host evil.invalid.', 'download'],
      ['Response body exceeds the 1024 byte limit while fetching https://sentinel.invalid/ripr.', 'download'],
      ['Downloaded server failed its version probe: spawn EACCES', 'download']
    ] as const) {
      assert.strictEqual(classifyDownloadFailureKind(message), kind, message);
    }
  });

  test('network guidance names the connection and the offline route, other kinds do not (#13638)', () => {
    const failure = (kind: ResolveFailureKind): ResolveFailure => ({
      message: 'ripr server is not available.',
      detail: `${kind} sentinel`,
      kind
    });
    for (const autoDownload of [true, false]) {
      const guidance = resolutionFailureGuidance(failure('network'), autoDownload);
      assert.ok(/network connection or proxy/.test(guidance), guidance);
      assert.ok(/offline/.test(guidance), guidance);
      assert.ok(guidance.includes('ripr.server.path'), guidance);
      assert.ok(guidance.includes('ripr.server.downloadBaseUrl'), guidance);
    }
    for (const kind of ['missing_binary', 'download', 'http', 'checksum', 'manifest', 'other'] as const) {
      const guidance = resolutionFailureGuidance(failure(kind), true);
      assert.ok(!/offline|network connection/.test(guidance), `${kind}: ${guidance}`);
    }
    // An explicit remedy (a configured path that failed) still wins.
    assert.strictEqual(
      resolutionFailureGuidance({ ...failure('network'), remedy: 'Fix ripr.server.path.' }, true),
      'Fix ripr.server.path.'
    );
  });

  test('a network resolution failure shows the offline route in the popup (#13638)', async () => {
    const errorCalls: Array<{ message: string; items: string[] }> = [];
    const { controller } = controllerForFailure(
      {
        message: 'ripr server is not available.',
        detail: 'Could not fetch https://sentinel.invalid/manifest.json: getaddrinfo ENOTFOUND sentinel.invalid',
        kind: 'network'
      },
      errorCalls,
      []
    );

    await controller.start();
    await controller.settleServerMessage();

    assert.strictEqual(errorCalls.length, 1, `expected one error popup, got ${JSON.stringify(errorCalls)}`);
    assert.ok(errorCalls[0].message.includes('ENOTFOUND sentinel.invalid'), errorCalls[0].message);
    assert.ok(/To work offline, set ripr\.server\.path/.test(errorCalls[0].message), errorCalls[0].message);
    assert.ok(!errorCalls[0].message.includes('cargo install ripr'), errorCalls[0].message);
    assert.deepStrictEqual(errorCalls[0].items, [OPEN_SETTINGS, COPY_DIAGNOSTIC, SHOW_OUTPUT, RETRY]);
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
    await controller.settleServerMessage();

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
    await controller.settleServerMessage();

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
      await controller.settleServerMessage();
      assert.strictEqual(errorCalls.length, 1, `${kind}: ${JSON.stringify(errorCalls)}`);
      assert.ok(!errorCalls[0].items.includes(COPY_INSTALL), `${kind}: ${JSON.stringify(errorCalls[0])}`);
      assert.ok(errorCalls[0].items.includes(SHOW_OUTPUT), `${kind}: ${JSON.stringify(errorCalls[0])}`);
    }
  });

  test('start() resolves while the missing-server popup is still unanswered (#9526)', async () => {
    const errorCalls: Array<{ message: string; items: string[] }> = [];
    const { controller } = controllerForFailure(
      { message: 'ripr server is not available.', detail: 'ripr.exe --version timed out.', kind: 'other' },
      errorCalls,
      [],
      undefined,
      true
    );

    const outcome = await Promise.race([
      controller.start().then(() => 'returned'),
      new Promise<string>((resolve) => setTimeout(() => resolve('blocked on the popup'), 2000))
    ]);

    assert.strictEqual(outcome, 'returned');
    assert.strictEqual(errorCalls.length, 1, JSON.stringify(errorCalls));
    // A later start() must not be folded into a pending start.
    const second = await Promise.race([
      controller.start().then(() => 'returned'),
      new Promise<string>((resolve) => setTimeout(() => resolve('blocked on the popup'), 2000))
    ]);
    assert.strictEqual(second, 'returned');
  });
});

function controllerForFailure(
  failure: ResolveFailure,
  errorCalls: Array<{ message: string; items: string[] }>,
  clipboardWrites: string[],
  selectedAction?: string,
  neverAnswer = false
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
      if (neverAnswer) {
        return new Promise<string | undefined>(() => undefined);
      }
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
