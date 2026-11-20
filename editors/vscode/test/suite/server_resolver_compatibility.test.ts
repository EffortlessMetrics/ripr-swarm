import * as assert from 'assert';
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import * as vscode from 'vscode';
import { RiprConfig } from '../../src/config';
import { ManagedServerInstallation } from '../../src/managedServerInstall';
import { currentRiprPlatform } from '../../src/platform';
import {
  expandConfiguredServerPath,
  managedManifestPlacementDetail,
  missingServerRemedy,
  probeServerVersion,
  relativeConfiguredPathFailure,
  resolveServer,
  serverLaunchCommand,
  ServerResolverRuntime,
  serverVersionOlderThan
} from '../../src/serverResolver';
import { compatibleLspEvidence } from './testCompatibility';

suite('Server resolver compatibility fallback', () => {
  test('renders the exact managed manifest placement into user-visible server detail', () => {
    const selectedManifestUrl =
      'https://github.com/EffortlessMetrics/ripr/releases/download/v0.11.0-rc.1/ripr-server-manifest-v0.11.0.json';
    const installation = {
      executablePath: '/tmp/ripr',
      receiptPath: '/tmp/install-receipt.json',
      receipt: {
        schemaVersion: 1,
        installationState: 'complete',
        requestedVersion: '0.11.0',
        manifestVersion: '0.11.0',
        platformTarget: 'x86_64-unknown-linux-gnu',
        executableName: 'ripr',
        archiveSha256: 'a'.repeat(64),
        executableSha256: 'b'.repeat(64),
        binaryVersion: 'ripr 0.11.0',
        selectedManifestUrl,
        manifestSelection: 'fallback_exact_after_preferred_absent',
        preferredManifestObservation: 'direct_not_found',
        fallbackManifestObservation: 'accepted'
      }
    } satisfies ManagedServerInstallation;

    const detail = managedManifestPlacementDetail(installation);
    assert.ok(detail?.includes('exact fallback after preferred absence'));
    assert.ok(detail?.includes(selectedManifestUrl));
    assert.ok(detail?.includes('preferred=direct 404'));
    assert.ok(detail?.includes('fallback=accepted'));
  });

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
            detail: '[missing_required_capability] hoverProvider',
            kind: 'other'
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

  test('a disabled download keeps a PATH probe failure kind instead of missing_binary', async function () {
    if (!currentRiprPlatform()) {
      this.skip();
      return;
    }
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'ripr-resolver-path-kind-'));
    const runtime: ServerResolverRuntime = {
      probeCandidate: async (_command, _source, detail) => ({
        message: `${detail} failed version check.`,
        detail: `${detail} exited with code 1.`,
        kind: 'other'
      })
    };
    try {
      const result = await resolveServer(stubContext(root), config(), stubOutput(), runtime);
      assert.ok(!('command' in result), JSON.stringify(result));
      if ('command' in result) {
        return;
      }
      assert.strictEqual(result.kind, 'other', JSON.stringify(result));
      assert.ok(
        !result.detail.includes('cargo install ripr'),
        `an unusable PATH binary must not recommend cargo install: ${result.detail}`
      );
    } finally {
      fs.rmSync(root, { recursive: true, force: true });
    }
  });

  test('a disabled download with a missing PATH server remains a missing binary', async function () {
    if (!currentRiprPlatform()) {
      this.skip();
      return;
    }
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'ripr-resolver-missing-kind-'));
    const runtime: ServerResolverRuntime = {
      probeCandidate: async (_command, _source, detail) => ({
        message: `${detail} could not start.`,
        detail: `spawn ${detail} ENOENT`,
        kind: 'missing_binary'
      })
    };
    try {
      const result = await resolveServer(stubContext(root), config(), stubOutput(), runtime);
      assert.ok(!('command' in result), JSON.stringify(result));
      if ('command' in result) {
        return;
      }
      assert.strictEqual(result.kind, 'missing_binary', JSON.stringify(result));
      assert.ok(
        result.detail.includes('cargo install ripr'),
        `missing-binary diagnostic still names cargo install: ${result.detail}`
      );
    } finally {
      fs.rmSync(root, { recursive: true, force: true });
    }
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
        assert.strictEqual(result.kind, 'missing_binary', JSON.stringify(result));
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

  test('a leading ~ in ripr.server.path means the home directory, and nothing else is rewritten (#15075)', () => {
    // Synthetic homes, joined from parts so no literal reads as a machine path.
    const unixHome = '/srv/homes/dev';
    const winHome = ['C:', 'Users', 'dev'].join('\\');
    const winExpected = [winHome, '.cargo', 'bin', 'ripr.exe'].join('\\');
    assert.strictEqual(expandConfiguredServerPath('~/.cargo/bin/ripr', unixHome, 'linux'), `${unixHome}/.cargo/bin/ripr`);
    assert.strictEqual(expandConfiguredServerPath('  ~/.cargo/bin/ripr  ', unixHome, 'linux'), `${unixHome}/.cargo/bin/ripr`);
    assert.strictEqual(expandConfiguredServerPath('~', unixHome, 'linux'), unixHome);
    assert.strictEqual(expandConfiguredServerPath('~\\.cargo\\bin\\ripr.exe', winHome, 'win32'), winExpected);
    assert.strictEqual(expandConfiguredServerPath('~/.cargo/bin/ripr.exe', winHome, 'win32'), winExpected);
    // Not a home reference: another user's home, a mid-path ~, a bare name,
    // an already absolute path, and a backslash on a non-Windows host.
    for (const unchanged of ['~other/bin/ripr', '/opt/~/ripr', 'ripr', '/usr/local/bin/ripr', '~\\ripr', '']) {
      assert.strictEqual(expandConfiguredServerPath(unchanged, unixHome, 'linux'), unchanged, unchanged);
    }
    // No known home directory: leave the setting as written rather than guess.
    assert.strictEqual(expandConfiguredServerPath('~/.cargo/bin/ripr', '', 'linux'), '~/.cargo/bin/ripr');
  });

  test('a relative ripr.server.path with a directory part is refused by name; a bare command is not (#15075)', () => {
    for (const relative of ['./target/debug/ripr', 'target/debug/ripr', '../ripr']) {
      const failure = relativeConfiguredPathFailure(relative, 'linux');
      assert.ok(failure, relative);
      assert.ok(failure.message.includes('is not an absolute path'), failure.message);
      assert.ok(failure.detail.includes(JSON.stringify(relative)), failure.detail);
      assert.ok(failure.remedy?.includes('full path'), failure.remedy);
    }
    assert.ok(relativeConfiguredPathFailure('bin\\ripr.exe', 'win32'));
    for (const allowed of ['ripr', 'ripr-nightly', '/usr/local/bin/ripr']) {
      assert.strictEqual(relativeConfiguredPathFailure(allowed, 'linux'), undefined, allowed);
    }
    // Windows: only a drive plus root, or a UNC root, names one stable file.
    // A drive-relative name follows that drive's current directory and a
    // rooted path without a drive follows the current drive.
    for (const unstable of ['C:ripr.exe', 'C:bin\\ripr.exe', '\\tools\\ripr.exe', '/tools/ripr.exe', 'bin/ripr.exe']) {
      const failure = relativeConfiguredPathFailure(unstable, 'win32');
      assert.ok(failure, unstable);
      assert.ok(failure.detail.includes(JSON.stringify(unstable)), failure.detail);
    }
    const driveBackslash = ['C:', 'tools', 'ripr.exe'].join('\\');
    const driveSlash = ['c:', 'tools', 'ripr.exe'].join('/');
    for (const allowed of [driveBackslash, driveSlash, '\\\\server\\share\\ripr.exe', 'ripr.exe', 'ripr.cmd']) {
      assert.strictEqual(relativeConfiguredPathFailure(allowed, 'win32'), undefined, allowed);
    }
  });

  test('resolveServer probes the expanded home path and never probes a relative one (#15075)', async () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'ripr-resolver-home-path-'));
    const probed: string[] = [];
    const runtime: ServerResolverRuntime = {
      probeCandidate: async (command, source, detail, _useShell, installationState = 'unmanaged') => {
        probed.push(command);
        return { command, source, detail, installationState, compatibilityResult: compatibleLspEvidence };
      }
    };
    const output = { appendLine: () => undefined } as unknown as vscode.OutputChannel;
    try {
      const expected = path.join(os.homedir(), '.cargo', 'bin', 'ripr');
      const resolved = await resolveServer(
        stubContext(root),
        { ...config(), serverPath: '~/.cargo/bin/ripr' },
        output,
        runtime
      );
      assert.ok('command' in resolved, JSON.stringify(resolved));
      if ('command' in resolved) {
        assert.strictEqual(resolved.command, expected);
        assert.strictEqual(resolved.source, 'configured');
      }
      assert.deepStrictEqual(probed, [expected]);

      probed.length = 0;
      const refused = await resolveServer(
        stubContext(root),
        { ...config(), serverPath: './target/debug/ripr' },
        output,
        runtime
      );
      assert.ok('message' in refused, JSON.stringify(refused));
      if ('message' in refused) {
        assert.ok(refused.message.includes('is not an absolute path'), refused.message);
        assert.ok(refused.remedy?.includes('full path'), refused.remedy);
      }
      // No fall-through to bundled, cached, download or PATH either.
      assert.deepStrictEqual(probed, []);
    } finally {
      fs.rmSync(root, { recursive: true, force: true });
    }
  });

  test('a failed configured candidate carries a non-circular remedy', async () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'ripr-resolver-remedy-'));
    const configured = path.join(root, 'nope', 'ripr.exe');
    const runtime: ServerResolverRuntime = {
      probeCandidate: async () => ({
        message: `configured ripr.server.path ${configured} could not start.`,
        detail: 'fixture start failure',
        kind: 'other'
      })
    };
    try {
      const result = await resolveServer(
        stubContext(root),
        { ...config(), serverPath: configured },
        stubOutput(),
        runtime
      );
      assert.ok(!('command' in result), JSON.stringify(result));
      if ('command' in result) {
        return;
      }
      assert.strictEqual(result.kind, 'other', JSON.stringify(result));
      const remedy = (result as { remedy?: string }).remedy;
      assert.ok(remedy, 'the configured-path failure carried no remedy');
      assert.match(remedy, /^Fix or clear ripr\.server\.path/, remedy);
      assert.ok(!remedy.includes('set ripr.server.path'), `circular remedy leaked: ${remedy}`);
    } finally {
      fs.rmSync(root, { recursive: true, force: true });
    }
  });

  test('an unpinned configuration uses a compatible ripr on PATH before any download (#5314)', async function () {
    if (!currentRiprPlatform()) {
      this.skip();
      return;
    }
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'ripr-resolver-path-first-'));
    const attempts: string[] = [];
    const outputLines: string[] = [];
    const runtime: ServerResolverRuntime = {
      probeCandidate: async (command, source, detail, _useShell, installationState = 'unmanaged') => {
        attempts.push(source);
        return { command, source, detail, installationState, compatibilityResult: compatibleLspEvidence };
      }
    };
    try {
      const result = await resolveServer(
        stubContext(root),
        { ...config(), autoDownload: true, serverVersion: '', downloadBaseUrl: 'https://sentinel.invalid/ripr' },
        { appendLine: (line: string) => outputLines.push(line) } as unknown as vscode.OutputChannel,
        runtime
      );
      assert.ok('command' in result, JSON.stringify(result));
      assert.strictEqual(result.source, 'path');
      assert.deepStrictEqual(attempts, ['path']);
      assert.ok(
        !outputLines.some((line) => line.includes('download failed')),
        `PATH must win before the download is attempted: ${outputLines.join('\n')}`
      );
    } finally {
      fs.rmSync(root, { recursive: true, force: true });
    }
  });

  test('a pinned version keeps the download first and probes PATH once afterwards (#5314)', async function () {
    if (!currentRiprPlatform()) {
      this.skip();
      return;
    }
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'ripr-resolver-pinned-'));
    const attempts: string[] = [];
    const outputLines: string[] = [];
    const runtime: ServerResolverRuntime = {
      probeCandidate: async (command, source, detail, _useShell, installationState = 'unmanaged') => {
        attempts.push(source);
        return { command, source, detail, installationState, compatibilityResult: compatibleLspEvidence };
      }
    };
    try {
      const result = await resolveServer(
        stubContext(root),
        { ...config(), autoDownload: true, serverVersion: '0.10.0', downloadBaseUrl: 'https://sentinel.invalid/ripr' },
        { appendLine: (line: string) => outputLines.push(line) } as unknown as vscode.OutputChannel,
        runtime
      );
      assert.ok('command' in result, JSON.stringify(result));
      assert.strictEqual(result.source, 'path');
      assert.deepStrictEqual(attempts, ['path']);
      assert.ok(
        outputLines.some((line) => line.includes('download failed')),
        `a pinned version must attempt the download before PATH: ${outputLines.join('\n')}`
      );
    } finally {
      fs.rmSync(root, { recursive: true, force: true });
    }
  });

  test('an incompatible early PATH probe falls through to the download and is not probed twice (#5314)', async function () {
    if (!currentRiprPlatform()) {
      this.skip();
      return;
    }
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'ripr-resolver-path-incompatible-'));
    const attempts: string[] = [];
    const outputLines: string[] = [];
    const runtime: ServerResolverRuntime = {
      probeCandidate: async (_command, source, detail) => {
        attempts.push(source);
        return { message: 'ripr on PATH is not compatible.', detail: `${detail} rejected`, kind: 'other' };
      }
    };
    try {
      const result = await resolveServer(
        stubContext(root),
        { ...config(), autoDownload: true, serverVersion: '', downloadBaseUrl: 'https://sentinel.invalid/ripr' },
        { appendLine: (line: string) => outputLines.push(line) } as unknown as vscode.OutputChannel,
        runtime
      );
      assert.ok('message' in result, JSON.stringify(result));
      if ('message' in result) {
        assert.notStrictEqual(
          result.kind,
          'missing_binary',
          `download fall-through must keep the download failure kind, got ${JSON.stringify(result)}`
        );
        assert.ok(
          result.kind === 'download' ||
            result.kind === 'network' ||
            result.kind === 'http' ||
            result.kind === 'manifest',
          `expected a download-family kind, got ${result.kind}`
        );
        assert.ok(
          !result.detail.includes('cargo install ripr'),
          `download-family diagnostic must not append cargo install: ${result.detail}`
        );
      }
      assert.deepStrictEqual(attempts, ['path']);
      assert.ok(
        outputLines.some((line) => line.includes('download failed')),
        `an unusable PATH server must fall through to the download: ${outputLines.join('\n')}`
      );
    } finally {
      fs.rmSync(root, { recursive: true, force: true });
    }
  });
});

suite('Server resolver stale PATH version (#13638)', () => {
  function versionedRuntime(binaryVersion: string | undefined, attempts: string[]): ServerResolverRuntime {
    return {
      probeCandidate: async (command, source, detail, _useShell, installationState = 'unmanaged') => {
        attempts.push(source);
        return {
          command,
          source,
          detail,
          installationState,
          compatibilityResult: compatibleLspEvidence,
          ...(binaryVersion ? { binaryVersion } : {})
        };
      }
    };
  }

  test('version comparison reuses the setup-diagnosis parse and only flags strictly older', () => {
    assert.strictEqual(serverVersionOlderThan('ripr 0.9.3', '0.10.0'), true);
    assert.strictEqual(serverVersionOlderThan('ripr 0.10.0', '0.10.0'), false);
    assert.strictEqual(serverVersionOlderThan('ripr 0.11.0', '0.10.0'), false);
    assert.strictEqual(serverVersionOlderThan('ripr dev-build', '0.10.0'), false);
    assert.strictEqual(serverVersionOlderThan(undefined, '0.10.0'), false);
  });

  test('a release candidate is older than its release, and successive candidates order by SemVer', () => {
    assert.strictEqual(serverVersionOlderThan('ripr 0.11.0-rc.1', '0.11.0'), true);
    assert.strictEqual(serverVersionOlderThan('ripr 0.11.0-rc.1', '0.11.0-rc.2'), true);
    assert.strictEqual(serverVersionOlderThan('ripr 0.11.0-rc.10', '0.11.0-rc.2'), false);
    assert.strictEqual(serverVersionOlderThan('ripr 0.11.0', '0.11.0-rc.2'), false);
    assert.strictEqual(serverVersionOlderThan('ripr 0.11.0-rc.2', '0.11.0-rc.2'), false);
    assert.strictEqual(serverVersionOlderThan('ripr 0.11.0+build.5 (abc123)', '0.11.0'), false);
    assert.strictEqual(serverVersionOlderThan('ripr 0.10.9-rc.1', '0.10.0'), false);
  });

  test('an older PATH ripr does not skip the managed download, and stays the warned fallback', async function () {
    if (!currentRiprPlatform()) {
      this.skip();
      return;
    }
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'ripr-resolver-stale-path-'));
    const attempts: string[] = [];
    const outputLines: string[] = [];
    try {
      const result = await resolveServer(
        stubContext(root),
        { ...config(), autoDownload: true, serverVersion: '', downloadBaseUrl: 'https://sentinel.invalid/ripr' },
        { appendLine: (line: string) => outputLines.push(line) } as unknown as vscode.OutputChannel,
        versionedRuntime('ripr 0.1.0', attempts)
      );
      assert.ok(
        outputLines.some((line) => line.includes('older than the expected 0.10.0; trying the managed download')),
        `an older PATH server must fall through to the download: ${outputLines.join('\n')}`
      );
      assert.ok(
        !outputLines.some((line) => line.includes('skipping the managed server download')),
        `an older PATH server must not skip the download: ${outputLines.join('\n')}`
      );
      assert.ok(
        outputLines.some((line) => line.includes('download failed')),
        `the managed download must be attempted: ${outputLines.join('\n')}`
      );
      assert.ok('command' in result, JSON.stringify(result));
      if (!('command' in result)) {
        return;
      }
      assert.strictEqual(result.source, 'path');
      assert.deepStrictEqual(attempts, ['path'], 'PATH is probed once and reused as the fallback');
      const warning = result.versionWarning ?? '';
      assert.ok(warning.includes('ripr 0.1.0'), warning);
      assert.ok(warning.includes('0.10.0'), warning);
      assert.ok(warning.includes('cargo install ripr --locked'), warning);
      assert.ok(warning.includes('ripr.server.path'), warning);
    } finally {
      fs.rmSync(root, { recursive: true, force: true });
    }
  });

  test('a current PATH ripr still wins before the download with no warning', async function () {
    if (!currentRiprPlatform()) {
      this.skip();
      return;
    }
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'ripr-resolver-current-path-'));
    const attempts: string[] = [];
    const outputLines: string[] = [];
    try {
      const result = await resolveServer(
        stubContext(root),
        { ...config(), autoDownload: true, serverVersion: '', downloadBaseUrl: 'https://sentinel.invalid/ripr' },
        { appendLine: (line: string) => outputLines.push(line) } as unknown as vscode.OutputChannel,
        versionedRuntime('ripr 0.10.0', attempts)
      );
      assert.ok('command' in result, JSON.stringify(result));
      if (!('command' in result)) {
        return;
      }
      assert.strictEqual(result.source, 'path');
      assert.strictEqual(result.versionWarning, undefined);
      assert.ok(
        !outputLines.some((line) => line.includes('download failed')),
        `a current PATH server must win before the download: ${outputLines.join('\n')}`
      );
    } finally {
      fs.rmSync(root, { recursive: true, force: true });
    }
  });

  test('a disabled download uses an older PATH ripr with a warning', async () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'ripr-resolver-stale-nodownload-'));
    const attempts: string[] = [];
    try {
      const result = await resolveServer(
        stubContext(root),
        { ...config(), autoDownload: false, serverVersion: '' },
        stubOutput(),
        versionedRuntime('ripr 0.9.9', attempts)
      );
      assert.ok('command' in result, JSON.stringify(result));
      if (!('command' in result)) {
        return;
      }
      assert.strictEqual(result.source, 'path');
      assert.ok(result.versionWarning?.includes('ripr on PATH reports ripr 0.9.9'), result.versionWarning);
    } finally {
      fs.rmSync(root, { recursive: true, force: true });
    }
  });

  test('an explicit ripr.server.path is honored as-is even when older, and only warns', async () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'ripr-resolver-stale-configured-'));
    const configured = path.join(root, 'old-ripr');
    const attempts: string[] = [];
    try {
      const result = await resolveServer(
        stubContext(root),
        { ...config(), autoDownload: true, serverVersion: '', serverPath: configured },
        stubOutput(),
        versionedRuntime('ripr 0.2.0', attempts)
      );
      assert.ok('command' in result, JSON.stringify(result));
      if (!('command' in result)) {
        return;
      }
      assert.strictEqual(result.source, 'configured');
      assert.strictEqual(result.command, configured);
      assert.deepStrictEqual(attempts, ['configured']);
      assert.ok(result.versionWarning?.includes(`ripr.server.path (${configured})`), result.versionWarning);
      assert.ok(result.versionWarning?.includes('ripr 0.2.0'), result.versionWarning);
    } finally {
      fs.rmSync(root, { recursive: true, force: true });
    }
  });

  test('configured serverPath probes with win32 shell semantics, forwards serverArgs, and preserves needsShell', async () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'ripr-resolver-shell-'));
    const configured = path.join(root, process.platform === 'win32' ? 'ripr.cmd' : 'ripr');
    const customArgs = ['custom-lsp', '--stdio'];
    let observedUseShell: boolean | undefined;
    let observedArgs: readonly string[] | undefined;
    const runtime: ServerResolverRuntime = {
      probeCandidate: async (command, source, detail, useShell, installationState = 'unmanaged', serverArgs) => {
        observedUseShell = useShell;
        observedArgs = serverArgs;
        return {
          command,
          source,
          detail,
          installationState,
          compatibilityResult: compatibleLspEvidence
        };
      }
    };
    try {
      const result = await resolveServer(
        stubContext(root),
        { ...config(), serverPath: configured, serverArgs: customArgs },
        stubOutput(),
        runtime
      );
      assert.ok('command' in result, JSON.stringify(result));
      if (!('command' in result)) {
        return;
      }
      assert.strictEqual(observedUseShell, process.platform === 'win32');
      assert.deepStrictEqual(observedArgs, customArgs);
      assert.strictEqual(result.needsShell, process.platform === 'win32' ? true : undefined);
    } finally {
      fs.rmSync(root, { recursive: true, force: true });
    }
  });

  test('fallback PATH probe forwards config.serverArgs and serverLaunchCommand quotes spaced shell commands', async () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'ripr-resolver-fallback-args-'));
    const customArgs = ['custom-lsp', '--stdio'];
    let observedArgs: readonly string[] | undefined;
    const runtime: ServerResolverRuntime = {
      probeCandidate: async (command, source, detail, _useShell, installationState = 'unmanaged', serverArgs) => {
        if (source === 'path') {
          observedArgs = serverArgs;
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
    try {
      const result = await resolveServer(
        stubContext(root),
        { ...config(), autoDownload: false, serverArgs: customArgs },
        stubOutput(),
        runtime
      );
      assert.ok('command' in result, JSON.stringify(result));
      assert.deepStrictEqual(observedArgs, customArgs);
      const spacedCmd = path.join(root, 'ripr tools', 'ripr.cmd');
      const spacedExe = path.join(root, 'ripr tools', 'ripr.exe');
      assert.strictEqual(serverLaunchCommand(spacedCmd, true), `"${spacedCmd}"`);
      assert.strictEqual(serverLaunchCommand(`"${spacedCmd}"`, true), `"${spacedCmd}"`);
      assert.strictEqual(serverLaunchCommand(spacedExe, false), spacedExe);
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
