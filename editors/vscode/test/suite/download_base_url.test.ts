import * as assert from 'assert';
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import * as vscode from 'vscode';
import { RiprConfig } from '../../src/config';
import { currentRiprPlatform } from '../../src/platform';
import {
  DOWNLOAD_BASE_URL_REMEDY,
  downloadBaseUrlProblem,
  resolveServer,
  ServerResolverRuntime
} from '../../src/serverResolver';
import { compatibleLspEvidence } from './testCompatibility';

// #15163: a malformed ripr.server.downloadBaseUrl used to surface as a bare
// "Invalid URL" or an https-only refusal whose remedy pointed back at the
// same setting. These tests pin the up-front check and its message.
suite('Download mirror address', () => {
  test('an empty or plain https folder address is accepted (#15163)', () => {
    for (const value of [
      '',
      '   ',
      'https://mirror.example/ripr',
      'https://mirror.example/ripr/',
      '  https://mirror.example/releases/download/v0.10.0  ',
      'https://mirror.example:8443/ripr'
    ]) {
      assert.strictEqual(downloadBaseUrlProblem(value), undefined, JSON.stringify(value));
    }
  });

  test('each malformed mirror address names what is wrong (#15163)', () => {
    const cases: Array<[string, string]> = [
      ['mirror.example/ripr', 'is not a full URL'],
      ['not a url', 'is not a full URL'],
      ['http://mirror.example/ripr', 'uses http; only https:// downloads are allowed'],
      ['ftp://mirror.example/ripr', 'uses ftp; only https:// downloads are allowed'],
      ['file:///srv/ripr', 'uses file; only https:// downloads are allowed'],
      ['https://mirror.example/ripr?token=abc', 'carries a query or fragment'],
      ['https://mirror.example/ripr#v0.10.0', 'carries a query or fragment'],
      ['https://mirror.example/ripr/ripr-manifest.json', 'names a file'],
      ['https://mirror.example/ripr/MANIFEST.JSON', 'names a file']
    ];
    for (const [value, expected] of cases) {
      const problem = downloadBaseUrlProblem(value);
      assert.ok(problem?.includes(expected), `${JSON.stringify(value)}: ${String(problem)}`);
    }
  });

  test('the mirror remedy asks for the https folder or an empty value (#15163)', () => {
    assert.ok(DOWNLOAD_BASE_URL_REMEDY.includes('ripr.server.downloadBaseUrl'));
    assert.ok(DOWNLOAD_BASE_URL_REMEDY.includes('https://'));
    assert.ok(DOWNLOAD_BASE_URL_REMEDY.includes('clear it'));
  });

  test('a malformed mirror skips the download and names the setting in the failure (#15163)', async function () {
    if (!currentRiprPlatform()) {
      this.skip();
      return;
    }
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'ripr-mirror-bad-'));
    const sources: string[] = [];
    const runtime: ServerResolverRuntime = {
      probeCandidate: async (_command, source, detail) => {
        sources.push(source);
        return { message: `${detail} could not start.`, detail: `spawn ${detail} ENOENT`, kind: 'missing_binary' };
      }
    };
    const lines: string[] = [];
    try {
      const result = await resolveServer(
        stubContext(root),
        { ...config(), downloadBaseUrl: ' http://mirror.example/ripr ' },
        recordingOutput(lines),
        runtime
      );
      assert.ok(!('command' in result), JSON.stringify(result));
      if ('command' in result) {
        return;
      }
      assert.strictEqual(result.kind, 'other', JSON.stringify(result));
      assert.strictEqual(result.remedy, DOWNLOAD_BASE_URL_REMEDY);
      assert.ok(
        result.detail.includes('ripr.server.downloadBaseUrl "http://mirror.example/ripr" uses http'),
        result.detail
      );
      assert.ok(!/offline|network/i.test(result.detail), result.detail);
      assert.ok(!sources.includes('managed_download'), JSON.stringify(sources));
      assert.ok(lines.some((line) => line.startsWith('Skipping the managed server download:')), lines.join('\n'));
    } finally {
      fs.rmSync(root, { recursive: true, force: true });
    }
  });

  test('a malformed mirror still lets a usable PATH server start (#15163)', async function () {
    if (!currentRiprPlatform()) {
      this.skip();
      return;
    }
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'ripr-mirror-path-'));
    const runtime: ServerResolverRuntime = {
      probeCandidate: async (command, source, detail, _useShell, installationState = 'unmanaged') => {
        if (source !== 'path') {
          return { message: `${detail} could not start.`, detail: `spawn ${detail} ENOENT`, kind: 'missing_binary' };
        }
        return { command, source, detail, installationState, compatibilityResult: compatibleLspEvidence };
      }
    };
    try {
      const result = await resolveServer(
        stubContext(root),
        { ...config(), downloadBaseUrl: 'https://mirror.example/ripr/ripr-manifest.json' },
        recordingOutput([]),
        runtime
      );
      assert.ok('command' in result, JSON.stringify(result));
      if ('command' in result) {
        assert.strictEqual(result.source, 'path');
      }
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

function recordingOutput(lines: string[]): vscode.OutputChannel {
  return { appendLine: (line: string) => lines.push(line) } as unknown as vscode.OutputChannel;
}

function config(): RiprConfig {
  return {
    enabled: true,
    serverPath: '',
    serverArgs: ['lsp', '--stdio'],
    autoDownload: true,
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
