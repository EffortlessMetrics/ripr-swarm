import * as assert from 'assert';
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import { probeStandardLspCompatibility } from '../../src/lspCompatibility';

suite('Standard LSP compatibility probe', () => {
  const temporaryRoots: string[] = [];

  teardown(() => {
    for (const root of temporaryRoots.splice(0)) {
      fs.rmSync(root, { recursive: true, force: true });
    }
  });

  test('accepts the real packaged server through initialize and shutdown', async function () {
    const server = process.env.RIPR_TEST_SERVER_PATH;
    if (!server) {
      this.skip();
      return;
    }
    const result = await probeStandardLspCompatibility(server, false, 10_000);
    assert.strictEqual(result.status, 'compatible', JSON.stringify(result));
    if (result.status === 'compatible') {
      assert.strictEqual(result.serverName, 'ripr');
      assert.strictEqual(result.positionEncoding, 'utf-16');
      assert.strictEqual(result.processResult, 'clean_exit');
    }
  });

  test('rejects a version-only executable that cannot frame LSP', async () => {
    const fake = fakeServer('version-only');
    const result = await probeStandardLspCompatibility(fake.command, fake.useShell, 1000);
    assert.strictEqual(result.status, 'incompatible');
    assert.ok(result.status === 'incompatible' && ['framing_failure', 'process_failure'].includes(result.kind));
  });

  test('requires the exercised baseline but records optional omissions', async () => {
    const missing = fakeServer('missing-hover');
    const rejected = await probeStandardLspCompatibility(missing.command, missing.useShell, 1000);
    assert.deepStrictEqual(
      rejected.status === 'incompatible' ? rejected.kind : undefined,
      'missing_required_capability'
    );

    const optional = fakeServer('optional-omitted');
    const accepted = await probeStandardLspCompatibility(optional.command, optional.useShell, 1000);
    assert.strictEqual(accepted.status, 'compatible', JSON.stringify(accepted));
    if (accepted.status === 'compatible') {
      assert.deepStrictEqual(accepted.optional, {
        pullDiagnostics: false,
        codeActionResolve: false,
        executeCommand: false,
        workspaceFolders: false,
        workDoneProgress: false
      });
    }
  });

  test('rejects a server that selects a non-UTF-16 position encoding', async () => {
    const fake = fakeServer('utf8');
    const result = await probeStandardLspCompatibility(fake.command, fake.useShell, 1000);
    assert.deepStrictEqual(
      result.status === 'incompatible' ? result.kind : undefined,
      'position_encoding_mismatch'
    );
  });

  test('rejects a standard-capable process that does not identify as ripr', async () => {
    const fake = fakeServer('wrong-identity');
    const result = await probeStandardLspCompatibility(fake.command, fake.useShell, 1000);
    assert.deepStrictEqual(
      result.status === 'incompatible' ? result.kind : undefined,
      'server_identity_mismatch'
    );
  });

  for (const [mode, expected] of [
    ['timeout', 'initialize_timeout'],
    ['malformed', 'framing_failure'],
    ['partial', 'process_failure'],
    ['crash', 'process_failure'],
    ['nonzero', 'process_failure'],
    ['shutdown-failure', 'shutdown_failure']
  ] as const) {
    test(`classifies and cleans up ${mode}`, async () => {
      const fake = fakeServer(mode);
      const result = await probeStandardLspCompatibility(fake.command, fake.useShell, mode === 'timeout' ? 300 : 3000);
      assert.strictEqual(result.status, 'incompatible');
      assert.deepStrictEqual(result.status === 'incompatible' ? result.kind : undefined, expected);
    });
  }

  test('a later compatible channel can be tried after an incompatible candidate', async () => {
    const first = fakeServer('missing-hover');
    const second = fakeServer('valid');
    const attempts = [first, second];
    let selected: number | undefined;
    for (let index = 0; index < attempts.length; index += 1) {
      const candidate = attempts[index];
      const result = await probeStandardLspCompatibility(candidate.command, candidate.useShell, 1000);
      if (result.status === 'compatible') {
        selected = index;
        break;
      }
    }
    assert.strictEqual(selected, 1);
  });

  function fakeServer(mode: string): { command: string; useShell: boolean } {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'ripr-lsp-probe-'));
    temporaryRoots.push(root);
    const script = path.join(root, 'server.js');
    fs.writeFileSync(script, fakeServerSource(mode));
    if (process.platform === 'win32') {
      const command = path.join(root, 'ripr.cmd');
      fs.writeFileSync(command, `@echo off\r\n"${process.execPath}" "${script}" %*\r\n`);
      return { command, useShell: true };
    }
    const command = path.join(root, 'ripr');
    fs.writeFileSync(command, `#!/bin/sh\nexec "${process.execPath}" "${script}" "$@"\n`, { mode: 0o755 });
    return { command, useShell: false };
  }
});

function fakeServerSource(mode: string): string {
  return `
const mode = ${JSON.stringify(mode)};
if (process.argv.includes('--version')) { console.log('ripr 9.9.9'); process.exit(0); }
if (mode === 'version-only') { process.stdout.write('not lsp'); process.exit(0); }
if (mode === 'timeout') { setInterval(() => {}, 1000); return; }
if (mode === 'malformed') { process.stdout.write('Content-Length: nope\\r\\n\\r\\n{}'); return; }
if (mode === 'partial') { process.stdout.write('Content-Length: 100\\r\\n\\r\\n{'); process.exit(0); }
if (mode === 'crash') { process.exit(0); }
if (mode === 'nonzero') { process.exit(17); }
let input = Buffer.alloc(0);
process.stdin.on('data', chunk => { input = Buffer.concat([input, chunk]); consume(); });
function send(value) {
  const body = Buffer.from(JSON.stringify(value));
  process.stdout.write('Content-Length: ' + body.length + '\\r\\n\\r\\n');
  process.stdout.write(body);
}
function consume() {
  while (true) {
    const end = input.indexOf('\\r\\n\\r\\n'); if (end < 0) return;
    const header = input.subarray(0, end).toString();
    const match = /Content-Length:\\s*(\\d+)/i.exec(header); if (!match) process.exit(2);
    const length = Number(match[1]); const start = end + 4; if (input.length < start + length) return;
    const message = JSON.parse(input.subarray(start, start + length).toString()); input = input.subarray(start + length);
    if (message.method === 'initialize') {
      const capabilities = { textDocumentSync: 1, hoverProvider: true, codeActionProvider: true, positionEncoding: mode === 'utf8' ? 'utf-8' : 'utf-16' };
      if (mode === 'missing-hover') delete capabilities.hoverProvider;
      if (mode === 'valid') Object.assign(capabilities, { diagnosticProvider: {}, executeCommandProvider: { commands: [] }, workspace: { workspaceFolders: { supported: true } }, workDoneProgress: true, codeActionProvider: { resolveProvider: true } });
      send({ jsonrpc: '2.0', id: message.id, result: { capabilities, serverInfo: { name: mode === 'wrong-identity' ? 'other' : 'ripr', version: '9.9.9' } } });
    } else if (message.method === 'shutdown') {
      if (mode === 'shutdown-failure') send({ jsonrpc: '2.0', id: message.id, error: { code: -1, message: 'no' } });
      else send({ jsonrpc: '2.0', id: message.id, result: null });
    } else if (message.method === 'exit') { process.exit(0); }
  }
}
`;
}
