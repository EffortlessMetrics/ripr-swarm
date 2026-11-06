import * as assert from 'assert';
import * as crypto from 'crypto';
import * as fs from 'fs';
import * as https from 'https';
import * as os from 'os';
import * as path from 'path';
import { rootCertificates } from 'tls';
import {
  ARCHIVE_MAX_BYTES,
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
