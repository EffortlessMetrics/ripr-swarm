import * as assert from 'assert';
import * as crypto from 'crypto';
import * as fs from 'fs';
import * as https from 'https';
import * as os from 'os';
import * as path from 'path';
import { rootCertificates } from 'tls';
import {
  ARCHIVE_MAX_BYTES,
  ARCHIVE_MAX_MEMBERS,
  assertSafeTarListing,
  assertSafeZipListing,
  inspectTarArchive,
  inspectZipArchive,
  listZipCentralDirectoryEntries,
  MANIFEST_MAX_BYTES,
  fetchManifestBytesOverHttps,
  isAllowedHttpsRedirect
} from '../../src/downloader';
import {
  assertExtractedTreeContained,
  installManagedServer,
  ManagedServerInstallRequest,
  readManagedServerInstallation
} from '../../src/managedServerInstall';

interface FixtureRoute {
  readonly status: number;
  readonly body?: Buffer;
  readonly location?: string;
  readonly contentLength?: string;
}

class HardeningFixtureServer {
  private readonly routes = new Map<string, FixtureRoute>();
  private server: https.Server | undefined;
  private port = 0;

  route(routePath: string, route: FixtureRoute): void {
    this.routes.set(routePath, route);
  }

  get base(): string {
    return `https://127.0.0.1:${this.port}`;
  }

  async start(key: Buffer, cert: Buffer): Promise<void> {
    return new Promise((resolve, reject) => {
      const created = https.createServer({ key, cert }, (request, response) => {
        const requestPath = (request.url ?? '/').split('?')[0];
        const route = this.routes.get(requestPath);
        if (!route) {
          response.writeHead(404);
          response.end();
          return;
        }
        if (route.location) {
          response.writeHead(route.status, { location: route.location });
          response.end();
          return;
        }
        const headers: Record<string, string> = { 'content-type': 'application/octet-stream' };
        if (route.contentLength !== undefined) {
          headers['content-length'] = route.contentLength;
        }
        response.writeHead(route.status, headers);
        response.end(route.body ?? Buffer.alloc(0));
      });
      created.on('error', reject);
      created.listen(0, '127.0.0.1', () => {
        const address = created.address();
        if (!address || typeof address === 'string') {
          reject(new Error('fixture server did not bind a TCP port'));
          return;
        }
        this.port = address.port;
        this.server = created;
        resolve();
      });
    });
  }

  stop(): Promise<void> {
    const server = this.server;
    if (!server) {
      return Promise.resolve();
    }
    return new Promise((resolve, reject) => {
      server.close((error) => (error ? reject(error) : resolve()));
      server.closeAllConnections();
    });
  }
}

let generatedFixtureTls: { readonly key: Buffer; readonly cert: Buffer } | undefined;

function fixtureTlsMaterial(): Promise<{ readonly key: Buffer; readonly cert: Buffer }> {
  if (generatedFixtureTls) {
    return Promise.resolve(generatedFixtureTls);
  }
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'ripr-hardening-tls-'));
  const keyPath = path.join(dir, 'key.pem');
  const certPath = path.join(dir, 'cert.pem');
  return run('openssl', [
    'req',
    '-x509',
    '-newkey',
    'rsa:2048',
    '-sha256',
    '-days',
    '1',
    '-nodes',
    '-keyout',
    keyPath,
    '-out',
    certPath,
    '-subj',
    '/CN=127.0.0.1',
    '-addext',
    'subjectAltName=DNS:localhost,IP:127.0.0.1'
  ])
    .then(() => {
      generatedFixtureTls = {
        key: fs.readFileSync(keyPath),
        cert: fs.readFileSync(certPath)
      };
      return generatedFixtureTls;
    })
    .finally(() => {
      fs.rmSync(dir, { recursive: true, force: true });
    });
}

function run(command: string, args: string[]): Promise<void> {
  return new Promise((resolve, reject) => {
    const child = require('child_process').spawn(command, args, { shell: false, stdio: 'ignore' });
    child.once('error', reject);
    child.once('exit', (code: number | null) => {
      if (code === 0) {
        resolve();
      } else {
        reject(new Error(`${command} exited with code ${code}`));
      }
    });
  });
}

suite('VS Code download hardening (#11477 item 5)', () => {
  let previousCa: typeof https.globalAgent.options.ca;
  let tls: { readonly key: Buffer; readonly cert: Buffer };

  suiteSetup(async () => {
    tls = await fixtureTlsMaterial();
    previousCa = https.globalAgent.options.ca;
    https.globalAgent.options.ca = [...rootCertificates, tls.cert];
  });

  suiteTeardown(() => {
    https.globalAgent.options.ca = previousCa;
  });

  test('isAllowedHttpsRedirect permits same host and GitHub release hosts', () => {
    assert.strictEqual(
      isAllowedHttpsRedirect('https://mirror.example/v1.json', 'https://mirror.example/v2.json'),
      true
    );
    assert.strictEqual(
      isAllowedHttpsRedirect(
        'https://github.com/EffortlessMetrics/ripr/releases/download/v1.0.0/manifest.json',
        'https://release-assets.githubusercontent.com/github-production-release-asset/abc'
      ),
      true
    );
    assert.strictEqual(
      isAllowedHttpsRedirect('https://mirror.example/v1.json', 'https://evil.example/payload'),
      false
    );
  });

  test('fetchManifestBytesOverHttps rejects Content-Length above the manifest cap', async function () {
    const server = new HardeningFixtureServer();
    await server.start(tls.key, tls.cert);
    try {
      server.route('/oversized.json', {
        status: 200,
        body: Buffer.from('tiny'),
        contentLength: String(MANIFEST_MAX_BYTES + 1)
      });
      const outcome = await fetchManifestBytesOverHttps(`${server.base}/oversized.json`, MANIFEST_MAX_BYTES);
      assert.strictEqual(outcome.kind, 'transport_failure');
      if (outcome.kind === 'transport_failure') {
        assert.match(outcome.message, /Content-Length.*byte limit/);
      }
    } finally {
      await server.stop();
    }
  });

  test('fetchManifestBytesOverHttps rejects a streaming body above the cap', async function () {
    const server = new HardeningFixtureServer();
    await server.start(tls.key, tls.cert);
    try {
      const cap = 32;
      server.route('/stream.json', { status: 200, body: Buffer.alloc(cap + 1) });
      const outcome = await fetchManifestBytesOverHttps(`${server.base}/stream.json`, cap);
      assert.strictEqual(outcome.kind, 'transport_failure');
      if (outcome.kind === 'transport_failure') {
        assert.match(outcome.message, /byte limit/);
      }
    } finally {
      await server.stop();
    }
  });

  test('fetchManifestBytesOverHttps rejects redirects to disallowed hosts', async function () {
    const server = new HardeningFixtureServer();
    await server.start(tls.key, tls.cert);
    try {
      server.route('/redirect', {
        status: 302,
        location: 'https://evil.example/ripr-server-manifest-v2.0.0.json'
      });
      const outcome = await fetchManifestBytesOverHttps(`${server.base}/redirect`, MANIFEST_MAX_BYTES);
      assert.strictEqual(outcome.kind, 'transport_failure');
      if (outcome.kind === 'transport_failure') {
        assert.match(outcome.message, /disallowed host evil\.example/);
      }
    } finally {
      await server.stop();
    }
  });

  test('fetchManifestBytesOverHttps follows redirects within the same host', async function () {
    const server = new HardeningFixtureServer();
    await server.start(tls.key, tls.cert);
    try {
      const payload = Buffer.from('{"version":"1.0.0","assets":{}}\n');
      server.route('/redirect', { status: 302, location: `${server.base}/final.json` });
      server.route('/final.json', { status: 200, body: payload });
      const outcome = await fetchManifestBytesOverHttps(`${server.base}/redirect`, MANIFEST_MAX_BYTES);
      assert.strictEqual(outcome.kind, 'ok');
      if (outcome.kind === 'ok') {
        assert.deepStrictEqual(outcome.bytes, payload);
      }
    } finally {
      await server.stop();
    }
  });

  test('fetchManifestBytesOverHttps uses a larger cap for archives', async function () {
    assert.ok(ARCHIVE_MAX_BYTES > MANIFEST_MAX_BYTES);
    const server = new HardeningFixtureServer();
    await server.start(tls.key, tls.cert);
    try {
      const body = Buffer.alloc(MANIFEST_MAX_BYTES + 1024, 0x61);
      server.route('/archive.bin', { status: 200, body });
      const manifestOutcome = await fetchManifestBytesOverHttps(`${server.base}/archive.bin`, MANIFEST_MAX_BYTES);
      assert.strictEqual(manifestOutcome.kind, 'transport_failure');
      const archiveOutcome = await fetchManifestBytesOverHttps(`${server.base}/archive.bin`, ARCHIVE_MAX_BYTES);
      assert.strictEqual(archiveOutcome.kind, 'ok');
    } finally {
      await server.stop();
    }
  });

  test('assertSafeTarListing rejects traversal, links, and special members before extraction', () => {
    assert.doesNotThrow(() => assertSafeTarListing(['ripr', 'bin/', 'bin/ripr'], ['-rwxr-xr-x u/g 5 2026-01-01 00:00 ripr', 'drwxr-xr-x u/g 0 2026-01-01 00:00 bin/']));
    assert.throws(() => assertSafeTarListing(['../evil'], []), /unsafe path/);
    assert.throws(() => assertSafeTarListing(['a/../../evil'], []), /unsafe path/);
    assert.throws(() => assertSafeTarListing(['/etc/passwd'], []), /unsafe path/);
    assert.throws(() => assertSafeTarListing(['\\etc\\passwd'], []), /unsafe path/);
    assert.throws(() => assertSafeTarListing(['\\\\host\\c$\\evil'], []), /unsafe path/);
    assert.throws(() => assertSafeTarListing(['C:evil'], []), /unsafe path/);
    assert.throws(() => assertSafeTarListing(['a\\..\\evil'], []), /unsafe path/);
    assert.throws(() => assertSafeTarListing([], ['lrwxrwxrwx u/g 0 2026-01-01 00:00 x -> /tmp']), /unsupported type/);
    assert.throws(() => assertSafeTarListing([], ['hrwxr-xr-x u/g 0 2026-01-01 00:00 x link to y']), /unsupported type/);
    assert.throws(() => assertSafeTarListing([], ['-rwxr-xr-x u/g 0 2026-01-01 00:00 x link to y']), /hard link/);
    assert.throws(() => assertSafeTarListing(Array.from({ length: ARCHIVE_MAX_MEMBERS + 1 }, (_, i) => `f${i}`), []), /member limit/);
  });

  function buildZipCentralDirectory(members: ReadonlyArray<{ name: string; externalAttributes?: number }>): Buffer {
    const centralChunks: Buffer[] = [];
    for (const member of members) {
      const nameBytes = Buffer.from(member.name, 'utf8');
      const header = Buffer.alloc(46);
      header.writeUInt32LE(0x02014b50, 0);
      header.writeUInt16LE(0x0800, 8); // UTF-8 names
      header.writeUInt16LE(nameBytes.length, 28);
      header.writeUInt32LE(member.externalAttributes ?? 0, 38);
      centralChunks.push(header, nameBytes);
    }
    const central = Buffer.concat(centralChunks);
    const eocd = Buffer.alloc(22);
    eocd.writeUInt32LE(0x06054b50, 0);
    eocd.writeUInt16LE(members.length, 8);
    eocd.writeUInt16LE(members.length, 10);
    eocd.writeUInt32LE(central.length, 12);
    return Buffer.concat([central, eocd]);
  }

  const UNIX_REGULAR_FILE = 0o100644 * 0x10000;
  const UNIX_DIRECTORY = 0o040755 * 0x10000;
  const UNIX_SYMLINK = 0o120777 * 0x10000;
  const DOS_REPARSE_POINT = 0x0400;

  test('assertSafeZipListing rejects traversal, symlinks, reparse points, and member floods', () => {
    assert.doesNotThrow(() => assertSafeZipListing([
      { name: 'ripr', externalAttributes: UNIX_REGULAR_FILE },
      { name: 'bin/', externalAttributes: UNIX_DIRECTORY },
      { name: 'bin/ripr.exe', externalAttributes: 0 }
    ]));
    assert.throws(() => assertSafeZipListing([
      { name: '../evil', externalAttributes: 0 }
    ]), /unsafe path/);
    assert.throws(() => assertSafeZipListing([
      { name: 'a/../../evil', externalAttributes: 0 }
    ]), /unsafe path/);
    assert.throws(() => assertSafeZipListing([
      { name: '/etc/passwd', externalAttributes: 0 }
    ]), /unsafe path/);
    assert.throws(() => assertSafeZipListing([
      { name: '\\etc\\passwd', externalAttributes: 0 }
    ]), /unsafe path/);
    assert.throws(() => assertSafeZipListing([
      { name: '\\\\host\\c$\\evil', externalAttributes: 0 }
    ]), /unsafe path/);
    assert.throws(() => assertSafeZipListing([
      { name: 'C:evil', externalAttributes: 0 }
    ]), /unsafe path/);
    assert.throws(() => assertSafeZipListing([
      { name: 'a\\..\\evil', externalAttributes: 0 }
    ]), /unsafe path/);
    assert.throws(() => assertSafeZipListing([
      { name: 'link', externalAttributes: UNIX_SYMLINK }
    ]), /unsupported type/);
    assert.throws(() => assertSafeZipListing([
      { name: 'mount', externalAttributes: DOS_REPARSE_POINT }
    ]), /reparse-point/);
    assert.throws(
      () => assertSafeZipListing(
        Array.from({ length: ARCHIVE_MAX_MEMBERS + 1 }, (_, i) => ({ name: `f${i}`, externalAttributes: 0 }))
      ),
      /member limit/
    );
  });

  test('assertSafeZipListing accepts permission-only modes Windows tools write', () => {
    // Python zipfile.writestr stores 0o600 << 16: permission bits with no
    // file type. Those entries are regular files and must not be refused.
    assert.doesNotThrow(() => assertSafeZipListing([
      { name: 'ripr.exe', externalAttributes: 0o600 * 0x10000 }
    ]));
  });

  test('listZipCentralDirectoryEntries refuses entries spilling past the directory end', () => {
    const archive = buildZipCentralDirectory([{ name: 'ripr' }]);
    // Inflate the entry's name-length field so the entry no longer fits the
    // declared central directory, without touching the directory size.
    archive.writeUInt16LE(0x4000, 28);
    assert.throws(
      () => listZipCentralDirectoryEntries(archive),
      /extends past the directory end/
    );
  });

  test('listZipCentralDirectoryEntries reads names and attributes from a fabricated zip', () => {
    const archive = buildZipCentralDirectory([
      { name: 'ripr', externalAttributes: UNIX_REGULAR_FILE },
      { name: 'bin/', externalAttributes: UNIX_DIRECTORY }
    ]);
    const entries = listZipCentralDirectoryEntries(archive);
    assert.deepStrictEqual(
      entries.map((entry) => [entry.name, entry.externalAttributes]),
      [['ripr', UNIX_REGULAR_FILE], ['bin/', UNIX_DIRECTORY]]
    );
  });

  test('listZipCentralDirectoryEntries refuses truncated or malformed archives', () => {
    assert.throws(() => listZipCentralDirectoryEntries(Buffer.from('not a zip')), /not a readable zip/);
    const good = buildZipCentralDirectory([{ name: 'ripr' }]);
    assert.throws(() => listZipCentralDirectoryEntries(good.subarray(0, good.length - 10)), /not a readable zip|malformed/);
    const wrongCount = buildZipCentralDirectory([{ name: 'ripr' }]);
    wrongCount.writeUInt16LE(2, wrongCount.length - 22 + 10);
    assert.throws(() => listZipCentralDirectoryEntries(wrongCount), /expected 2/);
  });

  test('inspectZipArchive accepts a safe fabricated zip and refuses a symlink member', async () => {
    const root = await fs.promises.mkdtemp(path.join(os.tmpdir(), 'ripr-zip-inspect-'));
    try {
      const safePath = path.join(root, 'safe.zip');
      await fs.promises.writeFile(safePath, buildZipCentralDirectory([
        { name: 'ripr', externalAttributes: UNIX_REGULAR_FILE },
        { name: 'bin/', externalAttributes: UNIX_DIRECTORY }
      ]));
      await inspectZipArchive(safePath);
      const unsafePath = path.join(root, 'unsafe.zip');
      await fs.promises.writeFile(unsafePath, buildZipCentralDirectory([{ name: 'link', externalAttributes: UNIX_SYMLINK }]));
      await assert.rejects(inspectZipArchive(unsafePath), /unsupported type/);
    } finally {
      await fs.promises.rm(root, { recursive: true, force: true });
    }
  });

  test('inspectTarArchive reads real tar listings and rejects symlink and traversal archives', async function () {
    if (process.platform === 'win32') {
      this.skip();
    }
    const cp = await import('child_process');
    const root = await fs.promises.mkdtemp(path.join(os.tmpdir(), 'ripr-tar-inspect-'));
    const run = (args: string[], cwd: string): void => {
      cp.execFileSync('tar', args, { cwd });
    };
    try {
      const good = path.join(root, 'good');
      await fs.promises.mkdir(good);
      await fs.promises.writeFile(path.join(good, 'ripr'), 'binary');
      run(['-czf', path.join(root, 'good.tgz'), '.'], good);
      await inspectTarArchive(path.join(root, 'good.tgz'));

      const linked = path.join(root, 'linked');
      await fs.promises.mkdir(linked);
      await fs.promises.symlink('/tmp', path.join(linked, 'escape'));
      run(['-czf', path.join(root, 'linked.tgz'), '.'], linked);
      await assert.rejects(inspectTarArchive(path.join(root, 'linked.tgz')), /unsupported type/);

      const nested = path.join(root, 'nested', 'inner');
      await fs.promises.mkdir(nested, { recursive: true });
      await fs.promises.writeFile(path.join(root, 'nested', 'evil'), 'x');
      run(['-czPf', path.join(root, 'trav.tgz'), '../evil'], nested);
      await assert.rejects(inspectTarArchive(path.join(root, 'trav.tgz')), /unsafe path/);
    } finally {
      await fs.promises.rm(root, { recursive: true, force: true });
    }
  });

  test('assertExtractedTreeContained rejects symlinks', async () => {
    const root = await fs.promises.mkdtemp(path.join(os.tmpdir(), 'ripr-extract-symlink-'));
    try {
      await fs.promises.writeFile(path.join(root, 'ripr'), 'binary', { mode: 0o755 });
      await fs.promises.symlink('/etc/passwd', path.join(root, 'escape'));
      await assert.rejects(assertExtractedTreeContained(root), /Refusing extracted symlink/);
    } finally {
      await fs.promises.rm(root, { recursive: true, force: true });
    }
  });

  test('assertExtractedTreeContained rejects hard links', async () => {
    const root = await fs.promises.mkdtemp(path.join(os.tmpdir(), 'ripr-extract-hardlink-'));
    try {
      const original = path.join(root, 'ripr');
      await fs.promises.writeFile(original, 'binary', { mode: 0o755 });
      await fs.promises.link(original, path.join(root, 'ripr-alias'));
      await assert.rejects(assertExtractedTreeContained(root), /Refusing extracted hard link/);
    } finally {
      await fs.promises.rm(root, { recursive: true, force: true });
    }
  });

  test('assertExtractedTreeContained accepts a normal extracted tree', async () => {
    const root = await fs.promises.mkdtemp(path.join(os.tmpdir(), 'ripr-extract-ok-'));
    try {
      await fs.promises.mkdir(path.join(root, 'nested'), { recursive: true });
      await fs.promises.writeFile(path.join(root, 'nested', 'ripr'), 'binary', { mode: 0o755 });
      await assertExtractedTreeContained(root);
    } finally {
      await fs.promises.rm(root, { recursive: true, force: true });
    }
  });

  test('installManagedServer fails closed when extraction leaves a symlink', async () => {
    const serversRoot = await fs.promises.mkdtemp(path.join(os.tmpdir(), 'ripr-install-symlink-'));
    try {
      const request = installRequest(serversRoot, '7.0.0');
      const bytes = Buffer.from('archive-bytes');
      await assert.rejects(
        installManagedServer(request, {
          resolveArchive: async () => ({
            manifestVersion: '7.0.0',
            expectedSha256: crypto.createHash('sha256').update(bytes).digest('hex'),
            bytes
          }),
          extractArchive: async (_archivePath, destination) => {
            const executableName = process.platform === 'win32' ? 'ripr.exe' : 'ripr';
            await fs.promises.writeFile(path.join(destination, executableName), 'binary', { mode: 0o755 });
            await fs.promises.symlink('/tmp/outside', path.join(destination, 'link'));
          },
          probeExecutable: async () => 'ripr 7.0.0'
        }),
        /Refusing extracted symlink/
      );
      assert.strictEqual(await readManagedServerInstallation(request), undefined);
      const versionEntries = await fs.promises.readdir(path.join(request.serversRoot, '7.0.0'));
      assert.deepStrictEqual(versionEntries, []);
    } finally {
      await fs.promises.rm(serversRoot, { recursive: true, force: true });
    }
  });
});

function installRequest(serversRoot: string, version: string): ManagedServerInstallRequest {
  return {
    serversRoot: path.join(serversRoot, 'servers'),
    version,
    platformTarget: 'test-target',
    executableName: process.platform === 'win32' ? 'ripr.exe' : 'ripr',
    archiveExtension: 'test'
  };
}
