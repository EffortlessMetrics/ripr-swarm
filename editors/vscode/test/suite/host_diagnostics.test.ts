import * as assert from 'assert';
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import {
  HOST_DIAGNOSTICS_DIR_NAME,
  HOST_DIAGNOSTICS_IDENTITY_FILE,
  HOST_LOG_DIR_NAME,
  MAX_HOST_DIAGNOSTICS_BYTES,
  MAX_HOST_DIAGNOSTICS_FILE_BYTES,
  MAX_HOST_DIAGNOSTICS_IDENTITY_BYTES,
  USER_DATA_DIR_NAME,
  finalizeHostRun,
  redactHostDiagnosticsText,
  resolveHostDiagnosticsDir,
  resolveLogDir,
  resolveUserDataDir,
  retainHostDiagnostics
} from '../hostDiagnostics';

suite('VS Code test host diagnostics', () => {
  let root: string;

  setup(() => {
    root = fs.mkdtempSync(path.join(os.tmpdir(), 'ripr-host-diag-'));
  });

  teardown(() => {
    fs.rmSync(root, { force: true, recursive: true });
  });

  test('pins a known user-data, log, and diagnostics folder under the artifact root', () => {
    const userData = resolveUserDataDir(root);
    assert.strictEqual(path.basename(userData), USER_DATA_DIR_NAME);
    assert.ok(!userData.includes(String(process.pid)), userData);
    assert.strictEqual(resolveLogDir(userData), path.join(userData, HOST_LOG_DIR_NAME));
    assert.strictEqual(
      resolveHostDiagnosticsDir(root),
      path.join(root, HOST_DIAGNOSTICS_DIR_NAME)
    );
  });

  test('does not retain diagnostics on a successful host run', () => {
    writeLog(root, 'window.log', 'CodeWindow: detected unresponsive\n');
    const result = retainHostDiagnostics({
      artifactRoot: root,
      userDataDir: resolveUserDataDir(root),
      failed: false,
      error: undefined
    });
    assert.strictEqual(result.retained, false);
    assert.strictEqual(result.reason, 'success_no_retention');
    assert.ok(!fs.existsSync(resolveHostDiagnosticsDir(root)));
  });

  test('a later successful run removes an earlier failure snapshot', () => {
    writeLog(root, 'window.log', 'CodeWindow: detected unresponsive\n');
    retainHostDiagnostics({
      artifactRoot: root,
      userDataDir: resolveUserDataDir(root),
      failed: true,
      error: new Error('host hung')
    });
    assert.ok(fs.existsSync(resolveHostDiagnosticsDir(root)));
    const result = retainHostDiagnostics({
      artifactRoot: root,
      userDataDir: resolveUserDataDir(root),
      failed: false
    });
    assert.strictEqual(result.retained, false);
    assert.ok(!fs.existsSync(resolveHostDiagnosticsDir(root)));
  });

  test('copies host logs into the known diagnostics folder on failure', () => {
    writeLog(root, '20261014T201353/window1.log', 'CodeWindow: detected unresponsive\n');
    const result = retainHostDiagnostics({
      artifactRoot: root,
      userDataDir: resolveUserDataDir(root),
      failed: true,
      error: new Error('TestRunFailedError')
    });
    assert.strictEqual(result.retained, true);
    const copied = path.join(
      result.diagnosticsDir,
      HOST_LOG_DIR_NAME,
      '20261014T201353',
      'window1.log'
    );
    assert.ok(fs.existsSync(copied), copied);
    assert.ok(fs.readFileSync(copied, 'utf8').includes('detected unresponsive'));
    const identity = JSON.parse(
      fs.readFileSync(path.join(result.diagnosticsDir, HOST_DIAGNOSTICS_IDENTITY_FILE), 'utf8')
    ) as { retained_because: string; copied_files: string[] };
    assert.strictEqual(identity.retained_because, 'test_host_failure');
    assert.deepStrictEqual(identity.copied_files, ['20261014T201353/window1.log']);
  });

  test('does not copy non-log user-data such as User settings', () => {
    const userData = resolveUserDataDir(root);
    fs.mkdirSync(path.join(userData, 'User'), { recursive: true });
    fs.writeFileSync(path.join(userData, 'User', 'settings.json'), '{"ripr.server.path":"/secret"}\n');
    writeLog(root, 'main.log', 'started\n');
    retainHostDiagnostics({
      artifactRoot: root,
      userDataDir: userData,
      failed: true,
      error: new Error('host hung')
    });
    const leaked = path.join(
      resolveHostDiagnosticsDir(root),
      'User',
      'settings.json'
    );
    assert.ok(!fs.existsSync(leaked), leaked);
    assert.ok(
      fs.existsSync(path.join(resolveHostDiagnosticsDir(root), HOST_LOG_DIR_NAME, 'main.log'))
    );
  });

  test('redacts a complete PEM block', () => {
    const redacted = redactHostDiagnosticsText(
      [
        'before',
        '-----BEGIN PRIVATE KEY-----',
        'MIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSjAgEAAoIBAQC7fakekeybody',
        '-----END PRIVATE KEY-----',
        'after'
      ].join('\n')
    );
    assert.ok(!redacted.includes('BEGIN PRIVATE KEY'), redacted);
    assert.ok(!redacted.includes('fakekeybody'), redacted);
    assert.ok(redacted.includes('[redacted-pem]'), redacted);
    assert.ok(redacted.includes('before'));
    assert.ok(redacted.includes('after'));
  });

  test('redacts a truncated PEM tail that has no BEGIN marker', () => {
    const redacted = redactHostDiagnosticsText(
      [
        'MIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSjAgEAAoIBAQC7fakekeybodyline',
        'MIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSjAgEAAoIBAQC7fakekeybodyend=',
        '-----END RSA PRIVATE KEY-----'
      ].join('\n')
    );
    assert.ok(!redacted.includes('fakekeybody'), redacted);
    assert.ok(!redacted.includes('BEGIN'), redacted);
    assert.ok(redacted.includes('[redacted-pem-tail]'), redacted);
  });

  test('redacts a one-line PEM tail without unbounded matching', () => {
    const redacted = redactHostDiagnosticsText(
      `${'A'.repeat(400)}==-----END PRIVATE KEY-----`
    );
    assert.ok(!redacted.includes('A'.repeat(64)), redacted);
    assert.ok(redacted.includes('[redacted-pem-tail]'), redacted);
  });

  test('redacts GitHub tokens that may appear in host logs', () => {
    const redacted = redactHostDiagnosticsText(
      'url=https://x-access-token:ghs_abcdefghijklmnopqrstuvwxyz012345@github.com/org/repo.git ghp_abcdefghijklmnopqrstuvwxyz0123 leftover'
    );
    assert.ok(!redacted.includes('ghs_abcdefghijklmnopqrstuvwxyz012345'), redacted);
    assert.ok(!redacted.includes('ghp_abcdefghijklmnopqrstuvwxyz0123'), redacted);
    assert.ok(redacted.includes('[redacted-token]'), redacted);
    assert.ok(redacted.includes('x-access-token:[redacted]'), redacted);
  });

  test('keeps identity plus logs inside the total byte cap', () => {
    const chunk = '#'.repeat(MAX_HOST_DIAGNOSTICS_FILE_BYTES);
    for (let index = 0; index < 24; index += 1) {
      writeLog(root, `chunk-${index}.log`, chunk);
    }
    const error = new Error('host hung');
    error.stack = 's'.repeat(200_000);
    const result = retainHostDiagnostics({
      artifactRoot: root,
      userDataDir: resolveUserDataDir(root),
      failed: true,
      error
    });
    assert.ok(result.bytesWritten <= MAX_HOST_DIAGNOSTICS_BYTES, String(result.bytesWritten));
    const identitySize = fs.statSync(
      path.join(result.diagnosticsDir, HOST_DIAGNOSTICS_IDENTITY_FILE)
    ).size;
    assert.ok(identitySize <= MAX_HOST_DIAGNOSTICS_IDENTITY_BYTES, String(identitySize));
  });

  test('caps an oversized host log to the per-file tail bound', () => {
    const marker = 'UNRESPONSIVE-TAIL';
    const body = `${'#'.repeat(MAX_HOST_DIAGNOSTICS_FILE_BYTES + 2048)}\n${marker}\n`;
    writeLog(root, 'renderer.log', body);
    retainHostDiagnostics({
      artifactRoot: root,
      userDataDir: resolveUserDataDir(root),
      failed: true,
      error: new Error('host hung')
    });
    const copied = fs.readFileSync(
      path.join(resolveHostDiagnosticsDir(root), HOST_LOG_DIR_NAME, 'renderer.log')
    );
    assert.ok(copied.length <= MAX_HOST_DIAGNOSTICS_FILE_BYTES, String(copied.length));
    assert.ok(copied.toString('utf8').includes(marker));
  });

  test('keeps the original failing exit when diagnostic retention cannot write', () => {
    const blocker = path.join(root, 'not-a-directory');
    fs.writeFileSync(blocker, 'cannot-be-a-parent\n');
    const originalError = console.error;
    const logged: unknown[][] = [];
    console.error = (...args: unknown[]) => {
      logged.push(args);
    };
    try {
      const result = finalizeHostRun({
        artifactRoot: blocker,
        userDataDir: path.join(blocker, USER_DATA_DIR_NAME),
        error: new Error('CodeWindow: detected unresponsive')
      });
      assert.strictEqual(result.exitCode, 1);
      assert.strictEqual(result.retention.retained, false);
      assert.strictEqual(result.retention.reason, 'retention_failed');
      assert.ok(
        logged.some((args) => String(args[0]).includes('Failed to retain VS Code test host diagnostics')),
        JSON.stringify(logged)
      );
      assert.ok(
        logged.some((args) => String(args[0]).includes('Failed to run tests:')),
        JSON.stringify(logged)
      );
    } finally {
      console.error = originalError;
    }
  });
});

function writeLog(artifactRoot: string, relative: string, contents: string): void {
  const file = path.join(resolveLogDir(resolveUserDataDir(artifactRoot)), relative);
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, contents);
}
