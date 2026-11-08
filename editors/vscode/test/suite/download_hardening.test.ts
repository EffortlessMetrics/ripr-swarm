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
  assertSafeZipCentralDirectory,
  inspectTarArchive,
  inspectZipArchive,
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

interface ZipFixtureEntry {
  readonly name: string;
  readonly host?: number;
  readonly external?: number;
  readonly utf8?: boolean;
}

/** Minimal stored-entry zip writer: local headers, central directory, EOCD. */
interface ZipFixtureOptions {
  readonly totalEntries?: number;
  readonly entriesOnDisk?: number;
  readonly diskNumber?: number;
  readonly centralSize?: number;
  readonly centralOffset?: number;
  readonly comment?: Buffer;
}

function buildZip(entries: readonly ZipFixtureEntry[], options: ZipFixtureOptions = {}): Buffer {
  const locals: Buffer[] = [];
  const centrals: Buffer[] = [];
  let offset = 0;
  for (const entry of entries) {
    const name = Buffer.from(entry.name, entry.utf8 ? 'utf8' : 'latin1');
    const flags = entry.utf8 ? 0x800 : 0;
    const local = Buffer.alloc(30 + name.length);
    local.writeUInt32LE(0x04034b50, 0);
    local.writeUInt16LE(20, 4);
    local.writeUInt16LE(flags, 6);
    local.writeUInt16LE(name.length, 26);
    name.copy(local, 30);
    const central = Buffer.alloc(46 + name.length);
    central.writeUInt32LE(0x02014b50, 0);
    central.writeUInt16LE(((entry.host ?? 3) << 8) | 20, 4);
    central.writeUInt16LE(20, 6);
    central.writeUInt16LE(flags, 8);
    central.writeUInt16LE(name.length, 28);
    central.writeUInt32LE((entry.external ?? (0o100644 << 16)) >>> 0, 38);
    central.writeUInt32LE(offset, 42);
    name.copy(central, 46);
    locals.push(local);
    centrals.push(central);
    offset += local.length;
  }
  const directory = Buffer.concat(centrals);
  const comment = options.comment ?? Buffer.alloc(0);
  const eocd = Buffer.alloc(22);
  eocd.writeUInt32LE(0x06054b50, 0);
  eocd.writeUInt16LE(options.diskNumber ?? 0, 4);
  eocd.writeUInt16LE(options.entriesOnDisk ?? options.totalEntries ?? entries.length, 8);
  eocd.writeUInt16LE(options.totalEntries ?? entries.length, 10);
  eocd.writeUInt32LE(options.centralSize ?? directory.length, 12);
  eocd.writeUInt32LE(options.centralOffset ?? offset, 16);
  eocd.writeUInt16LE(comment.length, 20);
  return Buffer.concat([...locals, directory, eocd, comment]);
}

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

  test('assertSafeZipCentralDirectory accepts the server archive shape and rejects unsafe entries', () => {
    const good = [
      { name: 'LICENSE-MIT' },
      { name: 'README-server.txt' },
      { name: 'ripr.exe', external: (0o100755 << 16) },
      { name: 'docs/', external: (0o040755 << 16) | 0x10 },
      { name: 'docs\\notes.txt', host: 0, external: 0x20 },
      { name: 'caf\u00e9.txt', utf8: true },
      // Unix host with no file-type bits: accepted as a plain file.
      { name: 'typeless', external: 0o644 << 16 },
      // DOS host: high attribute bits are not a Unix mode, so a symlink-looking
      // value must not be read as one.
      { name: 'dos-high-bits', host: 0, external: 0xa1ff0020 },
      { name: 'trailing/', external: (0o040755 << 16) | 0x10 }
    ];
    assert.doesNotThrow(() => assertSafeZipCentralDirectory(buildZip(good)));
    const rejects = (entry: ZipFixtureEntry, pattern: RegExp): void => {
      assert.throws(() => assertSafeZipCentralDirectory(buildZip([{ name: 'ok' }, entry])), pattern, entry.name);
    };
    rejects({ name: '../evil' }, /unsafe path/);
    rejects({ name: 'a/../../evil' }, /unsafe path/);
    rejects({ name: 'a\\..\\evil', host: 0, external: 0x20 }, /unsafe path/);
    rejects({ name: '/etc/passwd' }, /unsafe path/);
    rejects({ name: '\\\\host\\c$\\evil', host: 0 }, /unsafe path/);
    rejects({ name: 'D:evil.dll', host: 0 }, /unsafe path/);
    rejects({ name: 'ripr.exe:stream', host: 0 }, /unsafe path/);
    rejects({ name: 'nul\u0000name' }, /unsafe path/);
    rejects({ name: 'escape', external: (0o120777 << 16) }, /unsupported type/);
    rejects({ name: 'fifo', external: (0o010644 << 16) }, /unsupported type/);
    rejects({ name: 'junction', host: 0, external: 0x410 }, /reparse-point/);
    rejects({ name: '.. /evil', host: 0 }, /unsafe path/);
    rejects({ name: 'ripr.exe.', host: 0 }, /unsafe path/);
    rejects({ name: 'ripr.exe ', host: 0 }, /unsafe path/);
    rejects({ name: '.../x', host: 0 }, /unsafe path/);
    rejects({ name: 'CON', host: 0 }, /unsafe path/);
    rejects({ name: 'dir/aux.txt', host: 0 }, /unsafe path/);
    rejects({ name: 'lpt1', host: 0 }, /unsafe path/);
    rejects({ name: '', host: 0 }, /empty name/);
    assert.throws(
      () => assertSafeZipCentralDirectory(buildZip(Array.from({ length: ARCHIVE_MAX_MEMBERS + 1 }, (_, i) => ({ name: `f${i}` })))),
      /member limit/
    );
  });

  test('assertSafeZipCentralDirectory refuses archives it cannot fully parse', () => {
    const good = buildZip([{ name: 'ripr.exe' }]);
    assert.throws(() => assertSafeZipCentralDirectory(Buffer.alloc(0)), /end-of-central-directory/);
    assert.throws(() => assertSafeZipCentralDirectory(Buffer.from('not a zip at all, just bytes')), /end-of-central-directory/);
    // A trailing byte breaks the EOCD comment-length binding.
    assert.throws(() => assertSafeZipCentralDirectory(Buffer.concat([good, Buffer.from([0])])), /end-of-central-directory/);
    assert.throws(() => assertSafeZipCentralDirectory(buildZip([{ name: 'ripr.exe' }], { totalEntries: 0xffff })), /ZIP64/);
    // Count says two, directory holds one: the second header read runs off the directory.
    assert.throws(() => assertSafeZipCentralDirectory(buildZip([{ name: 'ripr.exe' }], { totalEntries: 2 })), /malformed central directory/);
    // Count says zero, directory holds one: an unlisted entry must not slip past.
    assert.throws(() => assertSafeZipCentralDirectory(buildZip([{ name: '../evil' }], { totalEntries: 0 })), /does not match/);
    assert.throws(() => assertSafeZipCentralDirectory(buildZip([{ name: 'ripr.exe' }], { centralSize: 0xffffffff })), /ZIP64/);
    assert.throws(() => assertSafeZipCentralDirectory(buildZip([{ name: 'ripr.exe' }], { centralOffset: 0xffffffff })), /ZIP64/);
    assert.throws(() => assertSafeZipCentralDirectory(buildZip([{ name: 'ripr.exe' }], { diskNumber: 1 })), /multi-disk/);
    assert.throws(() => assertSafeZipCentralDirectory(buildZip([{ name: 'ripr.exe' }, { name: 'b' }], { entriesOnDisk: 1 })), /multi-disk/);
    assert.throws(() => assertSafeZipCentralDirectory(buildZip([{ name: 'ripr.exe' }], { centralSize: 1000 })), /outside the file/);
    // Entry header fits but its name runs past the declared directory end.
    assert.throws(() => assertSafeZipCentralDirectory(buildZip([{ name: 'ripr.exe' }], { centralSize: 50 })), /truncated/);
    // A comment is fine on its own...
    assert.doesNotThrow(() => assertSafeZipCentralDirectory(buildZip([{ name: 'ripr.exe' }], { comment: Buffer.from('release') })));
    // ...but a fake directory plus end record hidden in the comment is what a
    // last-signature reader (.NET, Python) would extract, so it is refused
    // rather than skipped in favour of the earlier, benign record.
    const hiddenDirectory = buildZip([{ name: '../evil' }]);
    const fakeTail = Buffer.from(hiddenDirectory.subarray(hiddenDirectory.length - 22));
    fakeTail.writeUInt16LE(0x1234, 20);
    const hidden = buildZip([{ name: 'ripr.exe' }], { comment: Buffer.concat([hiddenDirectory.subarray(0, hiddenDirectory.length - 22), fakeTail]) });
    assert.throws(() => assertSafeZipCentralDirectory(hidden), /last end-of-central-directory record does not end the file/);
    const corrupt = Buffer.from(good);
    corrupt.writeUInt32LE(0xdeadbeef, corrupt.length - 22 - (46 + 'ripr.exe'.length));
    assert.throws(() => assertSafeZipCentralDirectory(corrupt), /malformed central directory/);
  });

  test('inspectZipArchive reads the archive from disk', async () => {
    const root = await fs.promises.mkdtemp(path.join(os.tmpdir(), 'ripr-zip-inspect-'));
    try {
      await fs.promises.writeFile(path.join(root, 'good.zip'), buildZip([{ name: 'ripr.exe' }]));
      await inspectZipArchive(path.join(root, 'good.zip'));
      await fs.promises.writeFile(path.join(root, 'bad.zip'), buildZip([{ name: '..\\evil.exe', host: 0 }]));
      await assert.rejects(inspectZipArchive(path.join(root, 'bad.zip')), /unsafe path/);
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
