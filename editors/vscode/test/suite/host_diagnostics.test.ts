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
  retainHostDiagnostics,
  boundedRequestEvidence
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

  for (const matrixFailure of [false, true]) {
    test(`retains only the ${matrixFailure ? 'matrix' : 'first'} failing host and its intended artifacts`, () => {
      const firstProfile = resolveUserDataDir(root);
      const matrixProfile = path.join(firstProfile, 'multiroot');
      writeLog(root, 'host.log', 'FIRST_HOST_SENTINEL');
      fs.mkdirSync(path.join(matrixProfile, 'logs'), { recursive: true });
      fs.writeFileSync(path.join(matrixProfile, 'logs', 'host.log'), 'MATRIX_HOST_SENTINEL');
      fs.mkdirSync(path.join(matrixProfile, 'User'), { recursive: true });
      fs.writeFileSync(path.join(matrixProfile, 'User', 'settings.json'), 'DO_NOT_COLLECT_SETTINGS');
      const receipt = path.join(root, 'multiroot-fixture', 'acceptance.json');
      fs.mkdirSync(path.dirname(receipt));
      fs.writeFileSync(receipt, '{"phase":"select root A","outcome":"failed"}');
      fs.writeFileSync(path.join(path.dirname(receipt), 'unrelated.json'), 'DO_NOT_COLLECT_SIBLINGS');
      const originalError = new Error('original host cancellation');
      const logged: unknown[][] = [];
      const restore = replaceConsoleError((...args) => logged.push(args));
      try {
        const result = finalizeHostRun({ artifactRoot: root, userDataDir: firstProfile,
          matrix: matrixFailure ? { userDataDir: matrixProfile, receiptPath: receipt } : undefined,
          error: originalError });
        assert.strictEqual(result.exitCode, 1);
        assert.ok(logged.some(args => args[1] === originalError), 'retain the original error object');
        const kept = fs.readFileSync(path.join(result.retention.diagnosticsDir, 'logs', 'host.log'), 'utf8');
        assert.strictEqual(kept, matrixFailure ? 'MATRIX_HOST_SENTINEL' : 'FIRST_HOST_SENTINEL');
        assert.strictEqual(fs.existsSync(path.join(result.retention.diagnosticsDir, 'acceptance.json')), matrixFailure);
        if (matrixFailure) {
          assert.strictEqual(fs.readFileSync(path.join(result.retention.diagnosticsDir, 'acceptance.json'), 'utf8'),
            '{"phase":"select root A","outcome":"failed"}');
        }
        assert.deepStrictEqual(fs.readdirSync(result.retention.diagnosticsDir).sort(),
          [...(matrixFailure ? ['acceptance.json'] : []), 'logs', 'retention.json'].sort());
        assert.deepStrictEqual(fs.readdirSync(path.join(result.retention.diagnosticsDir, 'logs')), ['host.log']);
      } finally { restore(); }
    });
  }

  test('retains a sanitized partial matrix receipt without parsing or collecting siblings', () => {
    const receipt = path.join(root, 'acceptance.json');
    fs.writeFileSync(receipt, '{"partial":"ghp_abcdefghijklmnopqrstuvwxyz0123",\n-----BEGIN PRIVATE KEY-----\nPRIVATE_SENTINEL');
    const result = retainHostDiagnostics({ artifactRoot: root, userDataDir: resolveUserDataDir(root),
      matrixReceiptPath: receipt, failed: true });
    const kept = fs.readFileSync(path.join(result.diagnosticsDir, 'acceptance.json'), 'utf8');
    assert.ok(kept.includes('[redacted-token]'), kept);
    assert.ok(kept.includes('[redacted-pem]'), kept);
    assert.ok(!kept.includes('PRIVATE_SENTINEL'), kept);
  });

  test('missing matrix receipt does not prevent retaining the selected logs', () => {
    writeLog(root, 'host.log', 'SELECTED_LOG');
    const result = retainHostDiagnostics({ artifactRoot: root, userDataDir: resolveUserDataDir(root),
      matrixReceiptPath: path.join(root, 'absent', 'acceptance.json'), failed: true });
    assert.strictEqual(fs.readFileSync(path.join(result.diagnosticsDir, 'logs', 'host.log'), 'utf8'), 'SELECTED_LOG');
    assert.strictEqual(fs.existsSync(path.join(result.diagnosticsDir, 'acceptance.json')), false);
    const identity = JSON.parse(fs.readFileSync(path.join(result.diagnosticsDir, 'retention.json'), 'utf8'));
    assert.strictEqual(identity.matrix_receipt.status, 'absent');
  });

  test('matrix receipt shares the total cap and obeys the existing per-file byte bound', () => {
    const receipt = path.join(root, 'acceptance.json');
    fs.writeFileSync(receipt, 'RECEIPT_HEAD' + '€'.repeat(MAX_HOST_DIAGNOSTICS_FILE_BYTES));
    for (let i = 0; i < 24; i++) writeLog(root, `${i}.log`, '#'.repeat(MAX_HOST_DIAGNOSTICS_FILE_BYTES));
    const result = retainHostDiagnostics({ artifactRoot: root, userDataDir: resolveUserDataDir(root),
      matrixReceiptPath: receipt, failed: true });
    const retainedReceipt = fs.readFileSync(path.join(result.diagnosticsDir, 'acceptance.json'));
    assert.ok(retainedReceipt.length <= MAX_HOST_DIAGNOSTICS_FILE_BYTES);
    assert.ok(retainedReceipt.toString('utf8').startsWith('RECEIPT_HEAD'));
    assert.ok(!retainedReceipt.toString('utf8').includes('\uFFFD'));
    assert.ok(result.bytesWritten <= MAX_HOST_DIAGNOSTICS_BYTES);
    const identity = JSON.parse(fs.readFileSync(path.join(result.diagnosticsDir, 'retention.json'), 'utf8'));
    assert.strictEqual(identity.matrix_receipt.truncated, true);
  });

  test('receipt truncation keeps the PEM opening marker for sanitization of JSON-escaped newlines', () => {
    const receipt = path.join(root, 'acceptance.json');
    fs.writeFileSync(receipt, JSON.stringify({ error: 'before\n-----BEGIN PRIVATE KEY-----\n'
      + 'QUJDREVG'.repeat(50_000) + '\n-----END PRIVATE KEY-----\nafter' }));
    const result = retainHostDiagnostics({ artifactRoot: root, userDataDir: resolveUserDataDir(root),
      matrixReceiptPath: receipt, failed: true });
    const kept = fs.readFileSync(path.join(result.diagnosticsDir, 'acceptance.json'), 'utf8');
    assert.ok(kept.includes('[redacted-pem]'));
    assert.ok(!kept.includes('QUJDREVG'), 'do not expose a clipped JSON-string PEM body');
    assert.ok(Buffer.byteLength(kept) <= MAX_HOST_DIAGNOSTICS_FILE_BYTES);
  });

  test('matrix receipt refuses other filenames, outside paths, directories and symlink traversal', () => {
    const outside = fs.mkdtempSync(path.join(os.tmpdir(), 'ripr-outside-receipt-'));
    try {
      const externalReceipt = path.join(outside, 'acceptance.json');
      fs.writeFileSync(externalReceipt, 'DO_NOT_COLLECT_EXTERNAL');
      const otherFile = path.join(root, 'settings.json');
      fs.writeFileSync(otherFile, 'DO_NOT_COLLECT_SETTINGS');
      const directoryReceipt = path.join(root, 'directory', 'acceptance.json');
      fs.mkdirSync(directoryReceipt, { recursive: true });
      const link = path.join(root, 'linked');
      fs.symlinkSync(outside, link, 'junction');
      for (const receipt of [externalReceipt, otherFile, directoryReceipt, path.join(link, 'acceptance.json')]) {
        const result = retainHostDiagnostics({ artifactRoot: root, userDataDir: resolveUserDataDir(root),
          matrixReceiptPath: receipt, failed: true });
        assert.strictEqual(fs.existsSync(path.join(result.diagnosticsDir, 'acceptance.json')), false, receipt);
        const identity = JSON.parse(fs.readFileSync(path.join(result.diagnosticsDir, 'retention.json'), 'utf8'));
        assert.strictEqual(identity.matrix_receipt.status, 'refused', receipt);
      }
    } finally { fs.rmSync(outside, { recursive: true, force: true }); }
  });

  test('request evidence preserves observed identity and original cancellation without extra payloads', () => {
    const error = Object.assign(new Error('Canceled'), { name: 'Canceled', code: -32800 });
    error.stack = 'ORIGINAL_REQUEST_STACK';
    const observed = { method: 'vscode.executeCodeActionProvider', document_uri: 'file:///root-a/src/alpha.rs',
      document_version: 7, client_id: 1, current_client_id: 2, client_pid: 123, current_client_pid: 456,
      same_client: false, sampled_effective_root: '/root-a', sampled_root_state: 'selected_single_root',
      sampled_attempt_id: 'attempt:4', sampled_run_status: 'completed', document_contents: 'DO_NOT_COLLECT_CONTENTS' };
    const result = boundedRequestEvidence(observed, error);
    assert.deepStrictEqual(result.error, { message: 'Canceled', stack: 'ORIGINAL_REQUEST_STACK', name: 'Canceled', code: -32800 });
    assert.strictEqual(result.client_id, 1);
    assert.strictEqual(result.current_client_id, 2);
    assert.strictEqual(result.client_pid, 123);
    assert.strictEqual(result.document_version, 7);
    assert.strictEqual(result.sampled_effective_root, '/root-a');
    assert.strictEqual(result.same_client, false);
    assert.ok(!JSON.stringify(result).includes('DO_NOT_COLLECT_CONTENTS'));
    assert.ok(!('workspace_root' in result), 'do not invent unavailable observations');
    assert.strictEqual(error.stack, 'ORIGINAL_REQUEST_STACK', 'do not mutate the original error');
  });

  test('request evidence redacts before clipping and omits unavailable numeric observations', () => {
    const error = new Error('-----BEGIN PRIVATE KEY-----\n' + 'PRIVATE_SENTINEL'.repeat(300));
    error.stack = 's'.repeat(100_000);
    const result = boundedRequestEvidence({ method: 'vscode.executeCodeActionProvider',
      document_uri: 'ghp_abcdefghijklmnopqrstuvwxyz0123' + 'x'.repeat(10_000),
      document_version: NaN, client_pid: undefined }, error);
    const serialized = JSON.stringify(result);
    assert.ok(serialized.includes('[redacted-token]'));
    assert.ok(serialized.includes('[redacted-pem]'));
    assert.ok(!serialized.includes('PRIVATE_SENTINEL'));
    assert.ok(!serialized.includes('ghp_'));
    assert.ok(serialized.length < 5000, String(serialized.length));
    assert.ok(!('document_version' in result));
    assert.ok(!('client_pid' in result));
  });

  test('empty and binary receipts do not enter the retained artifacts', () => {
    const receipt = path.join(root, 'acceptance.json');
    for (const contents of ['', 'binary\0private']) {
      fs.writeFileSync(receipt, contents);
      const result = retainHostDiagnostics({ artifactRoot: root, userDataDir: resolveUserDataDir(root),
        matrixReceiptPath: receipt, failed: true });
      assert.strictEqual(fs.existsSync(path.join(result.diagnosticsDir, 'acceptance.json')), false);
      const identity = JSON.parse(fs.readFileSync(path.join(result.diagnosticsDir, 'retention.json'), 'utf8'));
      assert.strictEqual(identity.matrix_receipt.status, 'empty_or_binary');
    }
  });

  test('matrix retention failure preserves the original error and failure exit', () => {
    const blocker = path.join(root, 'not-a-directory');
    fs.writeFileSync(blocker, 'block diagnostics output');
    const original = new Error('ORIGINAL_MATRIX_FAILURE');
    const logged: unknown[][] = [];
    const restore = replaceConsoleError((...args) => logged.push(args));
    try {
      const result = finalizeHostRun({ artifactRoot: blocker, userDataDir: resolveUserDataDir(root),
        matrix: { userDataDir: path.join(root, 'matrix'), receiptPath: path.join(root, 'acceptance.json') },
        error: original });
      assert.strictEqual(result.exitCode, 1);
      assert.strictEqual(result.retention.reason, 'retention_failed');
      assert.ok(logged.some(args => args[1] === original));
      assert.ok(logged.some(args => String(args[0]).includes('Failed to retain')));
    } finally { restore(); }
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
