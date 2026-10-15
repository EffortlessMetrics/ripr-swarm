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
  resetHostLogDir,
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

  test('resetting the log dir drops previous-run logs from a later failure snapshot', () => {
    writeLog(root, 'old.log', 'PREVIOUS-RUN\n');
    resetHostLogDir(resolveUserDataDir(root));
    writeLog(root, 'new.log', 'CURRENT-RUN\n');
    retainHostDiagnostics({
      artifactRoot: root,
      userDataDir: resolveUserDataDir(root),
      failed: true,
      error: new Error('host hung')
    });
    const snapshot = path.join(resolveHostDiagnosticsDir(root), HOST_LOG_DIR_NAME);
    assert.ok(!fs.existsSync(path.join(snapshot, 'old.log')));
    assert.ok(fs.readFileSync(path.join(snapshot, 'new.log'), 'utf8').includes('CURRENT-RUN'));
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

  test('retains distinct first and nested-host logs plus the whole matrix receipt on failure', () => {
    const userDataDir = resolveUserDataDir(root);
    const matrixUserData = path.join(userDataDir, 'multiroot');
    writeLog(root, 'same.log', 'FIRST-HOST-SENTINEL');
    fs.mkdirSync(resolveLogDir(matrixUserData), { recursive: true });
    fs.writeFileSync(path.join(resolveLogDir(matrixUserData), 'same.log'), 'MATRIX-HOST-SENTINEL');
    const receiptPath = path.join(root, 'matrix', 'acceptance.json');
    fs.mkdirSync(path.dirname(receiptPath));
    const receipt = { outcome: 'failed', last_phase: 'select root A', processes: [{ pid: 731, exits: 1 }],
      emergency_cleanup_errors: [], stages: [{ failure: 'Canceled: Canceled' }] };
    fs.writeFileSync(receiptPath, JSON.stringify(receipt));
    const restore = replaceConsoleError(() => {});
    try {
      const result = finalizeHostRun({ artifactRoot: root, userDataDir,
        multiRoot: { userDataDir: matrixUserData, receiptPath }, error: new Error('Canceled') });
      assert.strictEqual(result.exitCode, 1);
      assert.strictEqual(fs.readFileSync(path.join(result.retention.diagnosticsDir, 'logs', 'same.log'), 'utf8'),
        'FIRST-HOST-SENTINEL');
      assert.strictEqual(fs.readFileSync(path.join(result.retention.diagnosticsDir, 'multiroot', 'logs', 'same.log'), 'utf8'),
        'MATRIX-HOST-SENTINEL');
      assert.deepStrictEqual(JSON.parse(fs.readFileSync(path.join(result.retention.diagnosticsDir,
        'multiroot', 'acceptance.json'), 'utf8')), receipt);
      const identity = JSON.parse(fs.readFileSync(path.join(result.retention.diagnosticsDir, 'retention.json'), 'utf8'));
      assert.strictEqual(identity.user_data_dir, userDataDir);
      assert.strictEqual(identity.multiroot.user_data_dir, matrixUserData);
      assert.deepStrictEqual(identity.multiroot.copied_files, ['same.log']);
      assert.strictEqual(identity.multiroot.receipt.state, 'retained');
    } finally { restore(); }
  });

  test('matrix logs and receipt survive first-host saturation within the unchanged total cap', () => {
    for (let i = 0; i < 24; i++) { writeLog(root, `${i}.log`, '#'.repeat(MAX_HOST_DIAGNOSTICS_FILE_BYTES)); }
    const matrixUserData = path.join(resolveUserDataDir(root), 'multiroot');
    fs.mkdirSync(resolveLogDir(matrixUserData), { recursive: true });
    fs.writeFileSync(path.join(resolveLogDir(matrixUserData), 'matrix.log'), 'MATRIX-PRIORITY');
    for (let i = 0; i < 24; i++) {
      fs.writeFileSync(path.join(resolveLogDir(matrixUserData), `zz-${i}.log`), '#'.repeat(MAX_HOST_DIAGNOSTICS_FILE_BYTES));
    }
    const receiptPath = path.join(root, 'receipt.json');
    fs.writeFileSync(receiptPath, '{"outcome":"failed"}');
    const result = retainHostDiagnostics({ artifactRoot: root, userDataDir: resolveUserDataDir(root),
      multiRoot: { userDataDir: matrixUserData, receiptPath }, failed: true });
    const actualBytes = retainedBytes(result.diagnosticsDir);
    assert.ok(actualBytes <= MAX_HOST_DIAGNOSTICS_BYTES, String(actualBytes));
    assert.strictEqual(result.bytesWritten, actualBytes);
    assert.strictEqual(fs.readFileSync(path.join(result.diagnosticsDir, 'multiroot', 'logs', 'matrix.log'), 'utf8'),
      'MATRIX-PRIORITY');
    assert.deepStrictEqual(JSON.parse(fs.readFileSync(path.join(result.diagnosticsDir, 'multiroot', 'acceptance.json'), 'utf8')),
      { outcome: 'failed' });
  });

  test('matrix receipt refusals are explicit and success removes all retained profiles', () => {
    const matrixUserData = path.join(resolveUserDataDir(root), 'multiroot');
    const receiptPath = path.join(root, 'receipt.json');
    const input = { artifactRoot: root, userDataDir: resolveUserDataDir(root),
      multiRoot: { userDataDir: matrixUserData, receiptPath }, failed: true };
    for (const [contents, expected] of [[undefined, 'missing'], ['{bad', 'invalid_json'],
      ['x'.repeat(MAX_HOST_DIAGNOSTICS_FILE_BYTES + 1), 'oversized']] as const) {
      if (contents !== undefined) { fs.writeFileSync(receiptPath, contents); }
      const result = retainHostDiagnostics(input);
      const identity = JSON.parse(fs.readFileSync(path.join(result.diagnosticsDir, 'retention.json'), 'utf8'));
      assert.strictEqual(identity.multiroot.receipt.state, expected);
      assert.ok(!fs.existsSync(path.join(result.diagnosticsDir, 'multiroot', 'acceptance.json')));
    }
    const result = finalizeHostRun({ ...input, error: undefined });
    assert.strictEqual(result.exitCode, 0);
    assert.ok(!fs.existsSync(result.retention.diagnosticsDir));
  });

  test('refuses matrix receipt/profile symlinks and outside-root receipt sources', function () {
    if (process.platform === 'win32') { this.skip(); }
    const outside = fs.mkdtempSync(path.join(os.tmpdir(), 'ripr-unowned-diag-'));
    try {
      const receiptPath = path.join(outside, 'acceptance.json');
      fs.writeFileSync(receiptPath, '{"secret":"OUTSIDE-SENTINEL"}');
      fs.mkdirSync(path.join(outside, 'logs'));
      fs.writeFileSync(path.join(outside, 'logs', 'secret.log'), 'UNOWNED-LOG');
      const userDataDir = resolveUserDataDir(root);
      fs.mkdirSync(userDataDir, { recursive: true });
      const matrixUserData = path.join(userDataDir, 'multiroot');
      fs.symlinkSync(outside, matrixUserData, 'dir');
      const linkReceipt = path.join(root, 'receipt.json');
      fs.symlinkSync(receiptPath, linkReceipt);
      for (const source of [receiptPath, linkReceipt]) {
        const result = retainHostDiagnostics({ artifactRoot: root, userDataDir,
          multiRoot: { userDataDir: matrixUserData, receiptPath: source }, failed: true });
        const identity = JSON.parse(fs.readFileSync(path.join(result.diagnosticsDir, 'retention.json'), 'utf8'));
        assert.strictEqual(identity.multiroot.receipt.state, 'outside_root_or_symlink');
        assert.deepStrictEqual(identity.multiroot.copied_files, []);
        assert.ok(!fs.existsSync(path.join(result.diagnosticsDir, 'multiroot', 'logs', 'secret.log')));
      }
    } finally { fs.rmSync(outside, { recursive: true, force: true }); }
  });

  test('matrix launch failure retains its log without inventing a teardown receipt and redacts both profiles', () => {
    const userDataDir = resolveUserDataDir(root);
    const matrixUserData = path.join(userDataDir, 'multiroot');
    const token = 'ghp_abcdefghijklmnopqrstuvwxyz0123';
    writeLog(root, 'main.log', token);
    fs.mkdirSync(resolveLogDir(matrixUserData), { recursive: true });
    fs.writeFileSync(path.join(resolveLogDir(matrixUserData), 'main.log'), `MATRIX-LAUNCH-FAILED ${token}`);
    const receiptPath = path.join(root, 'missing-receipt.json');
    const restore = replaceConsoleError(() => {});
    try {
      const result = finalizeHostRun({ artifactRoot: root, userDataDir,
        multiRoot: { userDataDir: matrixUserData, receiptPath }, error: new Error('launch failed') });
      assert.strictEqual(result.exitCode, 1);
      const identity = JSON.parse(fs.readFileSync(path.join(result.retention.diagnosticsDir, 'retention.json'), 'utf8'));
      assert.strictEqual(identity.multiroot.receipt.state, 'missing');
      for (const profile of ['', 'multiroot']) {
        const text = fs.readFileSync(path.join(result.retention.diagnosticsDir, profile, 'logs', 'main.log'), 'utf8');
        assert.ok(!text.includes(token));
        assert.ok(text.includes('[redacted-token]'));
      }
      fs.writeFileSync(receiptPath, JSON.stringify({ error: token, outcome: 'failed' }));
      retainHostDiagnostics({ artifactRoot: root, userDataDir,
        multiRoot: { userDataDir: matrixUserData, receiptPath }, failed: true });
      const receipt = JSON.parse(fs.readFileSync(path.join(result.retention.diagnosticsDir, 'multiroot', 'acceptance.json'), 'utf8'));
      assert.strictEqual(receipt.error, '[redacted-token]');
      assert.strictEqual(receipt.outcome, 'failed');
    } finally { restore(); }
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

  test('redacts an unterminated PEM from BEGIN through the rest of the text', () => {
    const redacted = redactHostDiagnosticsText(
      ['before', '-----BEGIN PRIVATE KEY-----', 'unterminatedfakekeybody', 'after'].join('\n')
    );
    assert.ok(!redacted.includes('unterminatedfakekeybody'), redacted);
    assert.ok(!redacted.includes('BEGIN PRIVATE KEY'), redacted);
    assert.ok(redacted.includes('[redacted-pem]'), redacted);
    assert.ok(redacted.includes('before'));
    assert.ok(!redacted.includes('after'), redacted);
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

  test('redacts a PEM longer than the error clip from retention.json', () => {
    const keyBody = 'MIIEvFAKEKEYMATERIAL'.repeat(160);
    assert.ok(keyBody.length > 2048, String(keyBody.length));
    const error = new Error(
      [
        'host hung',
        '-----BEGIN PRIVATE KEY-----',
        keyBody,
        '-----END PRIVATE KEY-----'
      ].join('\n')
    );
    retainHostDiagnostics({
      artifactRoot: root,
      userDataDir: resolveUserDataDir(root),
      failed: true,
      error
    });
    const identity = fs.readFileSync(
      path.join(resolveHostDiagnosticsDir(root), HOST_DIAGNOSTICS_IDENTITY_FILE),
      'utf8'
    );
    assert.ok(!identity.includes('FAKEKEYMATERIAL'), identity.slice(0, 400));
    assert.ok(identity.includes('[redacted-pem]'), identity.slice(0, 400));
  });

  test('redacts an unterminated PEM from retention.json before clipping', () => {
    const keyBody = 'MIIEvFAKEUNTERMINATED'.repeat(160);
    assert.ok(keyBody.length > 2048, String(keyBody.length));
    const error = new Error(`-----BEGIN RSA PRIVATE KEY-----\n${keyBody}`);
    retainHostDiagnostics({
      artifactRoot: root,
      userDataDir: resolveUserDataDir(root),
      failed: true,
      error
    });
    const identity = fs.readFileSync(
      path.join(resolveHostDiagnosticsDir(root), HOST_DIAGNOSTICS_IDENTITY_FILE),
      'utf8'
    );
    assert.ok(!identity.includes('FAKEUNTERMINATED'), identity.slice(0, 400));
    assert.ok(identity.includes('[redacted-pem]'), identity.slice(0, 400));
  });

  test('keeps retention.json parseable when copied-file metadata would overflow', () => {
    const longName = `${'n'.repeat(240)}.log`;
    for (let index = 0; index < 80; index += 1) {
      writeLog(root, `${index}-${longName}`, 'x\n');
    }
    const result = retainHostDiagnostics({
      artifactRoot: root,
      userDataDir: resolveUserDataDir(root),
      failed: true,
      error: new Error('host hung')
    });
    const identityRaw = fs.readFileSync(
      path.join(result.diagnosticsDir, HOST_DIAGNOSTICS_IDENTITY_FILE),
      'utf8'
    );
    assert.ok(
      identityRaw.trimEnd().endsWith('}'),
      'identity must remain complete JSON, not a sliced prefix'
    );
    const identity = JSON.parse(identityRaw) as {
      schema_version: string;
      copied_files?: unknown;
      copied_file_count?: number;
    };
    assert.strictEqual(identity.schema_version, '0.1');
    assert.ok(
      Array.isArray(identity.copied_files) || typeof identity.copied_file_count === 'number',
      JSON.stringify(identity)
    );
    assert.ok(
      Buffer.byteLength(identityRaw, 'utf8') <= MAX_HOST_DIAGNOSTICS_IDENTITY_BYTES,
      String(Buffer.byteLength(identityRaw, 'utf8'))
    );
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

  test('log tail cap stays inside the byte budget across a multibyte cut', () => {
    const euro = '€';
    writeLog(
      root,
      'unicode.log',
      euro.repeat(Math.floor(MAX_HOST_DIAGNOSTICS_FILE_BYTES / 3) + 2)
    );
    retainHostDiagnostics({
      artifactRoot: root,
      userDataDir: resolveUserDataDir(root),
      failed: true,
      error: new Error('host hung')
    });
    const copied = fs.readFileSync(
      path.join(resolveHostDiagnosticsDir(root), HOST_LOG_DIR_NAME, 'unicode.log')
    );
    const copiedText = copied.toString('utf8');
    assert.ok(copied.length <= MAX_HOST_DIAGNOSTICS_FILE_BYTES, String(copied.length));
    assert.ok(!copiedText.includes('\uFFFD'), copiedText.slice(0, 16));
    assert.ok(copiedText.includes(euro), copiedText.slice(0, 16));
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
    const logged: unknown[][] = [];
    const restoreError = replaceConsoleError((...args: unknown[]) => {
      logged.push(args);
    });
    try {
      const result = finalizeHostRun({
        artifactRoot: blocker,
        userDataDir: path.join(blocker, USER_DATA_DIR_NAME),
        error: new Error('synthetic host failure for retention write control')
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
      restoreError();
    }
  });
});

function writeLog(artifactRoot: string, relative: string, contents: string): void {
  const file = path.join(resolveLogDir(resolveUserDataDir(artifactRoot)), relative);
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, contents);
}

function replaceConsoleError(stub: (...args: unknown[]) => void): () => void {
  const descriptor = Object.getOwnPropertyDescriptor(console, 'error');
  Object.defineProperty(console, 'error', {
    configurable: true,
    enumerable: descriptor?.enumerable ?? true,
    writable: true,
    value: stub
  });
  return () => {
    if (descriptor === undefined) {
      delete (console as { error?: unknown }).error;
      return;
    }
    Object.defineProperty(console, 'error', descriptor);
  };
}

function retainedBytes(root: string): number {
  return fs.readdirSync(root, { withFileTypes: true }).reduce((total, entry) => {
    const file = path.join(root, entry.name);
    return total + (entry.isDirectory() ? retainedBytes(file) : fs.statSync(file).size);
  }, 0);
}
