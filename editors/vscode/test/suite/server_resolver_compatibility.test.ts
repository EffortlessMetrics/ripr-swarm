import * as assert from 'assert';
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import * as vscode from 'vscode';
import { RiprConfig } from '../../src/config';
import { currentRiprPlatform } from '../../src/platform';
import { missingServerRemedy, probeServerVersion, resolveServer, ServerResolverRuntime } from '../../src/serverResolver';
import { compatibleLspEvidence } from './testCompatibility';

suite('Server resolver compatibility fallback', () => {
  test('skips an incompatible bundled candidate and selects the next allowed channel', async function () {
    const platform = currentRiprPlatform();
    if (!platform) {
      this.skip();
      return;
    }
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'ripr-resolver-probe-'));
    const bundled = path.join(root, 'extension', 'server', platform.target, platform.executableName);
    fs.mkdirSync(path.dirname(bundled), { recursive: true });
    fs.writeFileSync(bundled, 'sentinel');
    const attempts: string[] = [];
    const runtime: ServerResolverRuntime = {
      probeCandidate: async (command, source, detail, _useShell, installationState = 'unmanaged') => {
        attempts.push(source);
        if (source === 'bundled') {
          return {
            message: `${detail} is not LSP compatible.`,
            detail: '[missing_required_capability] hoverProvider'
          };
        }
        return {
          command,
          source,
          detail,
          installationState,
          compatibilityResult: compatibleLspEvidence
        };
      }
    };
    const outputLines: string[] = [];
    const context = {
      extensionUri: vscode.Uri.file(path.join(root, 'extension')),
      globalStorageUri: vscode.Uri.file(path.join(root, 'storage')),
      extension: { packageJSON: { version: '0.10.0' } }
    } as unknown as vscode.ExtensionContext;
    try {
      const result = await resolveServer(context, config(), {
        appendLine: (line: string) => outputLines.push(line)
      } as unknown as vscode.OutputChannel, runtime);
      assert.ok('command' in result, JSON.stringify(result));
      assert.deepStrictEqual(attempts, ['bundled', 'path']);
      assert.ok(outputLines.some((line) => line.includes('Skipping bundled server')));
    } finally {
      fs.rmSync(root, { recursive: true, force: true });
    }
  });

  test('missing-server remedy only suggests enabling download when it is off', () => {
    assert.ok(!missingServerRemedy(true).includes('autoDownload'), missingServerRemedy(true));
    assert.ok(missingServerRemedy(true).includes('cargo install ripr'));
    assert.ok(missingServerRemedy(false).includes('Enable ripr.server.autoDownload'));
  });

  test('a configured path that cannot start fails as could-not-start, not LSP-incompatible', async function () {
    // #5891: on win32 the missing file used to false-pass the version probe
    // and surface as "is not LSP compatible" with a nonsensical code 0.
    this.timeout(25_000);
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'ripr-resolver-missing-'));
    const missing = path.join(root, 'no-such-ripr.exe');
    try {
      const result = await resolveServer(
        stubContext(root),
        { ...config(), serverPath: missing },
        stubOutput()
      );
      assert.ok(!('command' in result), `expected a resolve failure, got ${JSON.stringify(result)}`);
      if ('command' in result) {
        return;
      }
      assert.ok(result.message.includes('could not start'), result.message);
      assert.ok(!result.message.includes('is not LSP compatible'), result.message);
      assert.ok(result.detail.length > 0, 'failure carried no cause detail');
      assert.ok(!result.detail.includes('[process_failure]'), result.detail);
    } finally {
      fs.rmSync(root, { recursive: true, force: true });
    }
  });

  test('probeServerVersion on a missing file does not return an empty version success', async function () {
    this.timeout(25_000);
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'ripr-version-missing-'));
    const missing = path.join(root, 'no-such-ripr.exe');
    try {
      const result = await probeServerVersion(missing, 'configured ripr.server.path fixture', false);
      assert.ok('message' in result, `expected a version-check failure, got ${JSON.stringify(result)}`);
      if ('message' in result) {
        assert.ok(result.message.includes('could not start'), result.message);
      }
    } finally {
      fs.rmSync(root, { recursive: true, force: true });
    }
  });

  test('a configured path at a real ripr binary still resolves as configured', async function () {
    const server = process.env.RIPR_TEST_SERVER_PATH;
    if (!server) {
      this.skip();
      return;
    }
    this.timeout(25_000);
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'ripr-resolver-real-'));
    try {
      const result = await resolveServer(
        stubContext(root),
        { ...config(), serverPath: server },
        stubOutput()
      );
      assert.ok('command' in result, JSON.stringify(result));
      if ('command' in result) {
        assert.strictEqual(result.source, 'configured');
        assert.ok(result.binaryVersion, 'the real server produced no version line');
      }
    } finally {
      fs.rmSync(root, { recursive: true, force: true });
    }
  });

  test('a failed configured candidate carries a non-circular remedy', async () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'ripr-resolver-remedy-'));
    const runtime: ServerResolverRuntime = {
      probeCandidate: async () => ({
        message: 'configured ripr.server.path C:/nope/ripr.exe could not start.',
        detail: 'fixture start failure'
      })
    };
    try {
      const result = await resolveServer(
        stubContext(root),
        { ...config(), serverPath: 'C:/nope/ripr.exe' },
        stubOutput(),
        runtime
      );
      assert.ok(!('command' in result), JSON.stringify(result));
      if ('command' in result) {
        return;
      }
      const remedy = (result as { remedy?: string }).remedy;
      assert.ok(remedy, 'the configured-path failure carried no remedy');
      assert.match(remedy, /^Fix or clear ripr\.server\.path/, remedy);
      assert.ok(!remedy.includes('set ripr.server.path'), `circular remedy leaked: ${remedy}`);
    } finally {
      fs.rmSync(root, { recursive: true, force: true });
    }
  });
});

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

function config(): RiprConfig {
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
    traceServer: 'off'
  };
}
