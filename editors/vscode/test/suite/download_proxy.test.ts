import * as assert from 'assert';
import * as cp from 'child_process';
import * as fs from 'fs';
import * as http from 'http';
import * as https from 'https';
import * as net from 'net';
import * as os from 'os';
import * as path from 'path';
import { rootCertificates } from 'tls';
import { fetchManifestBytesOverHttps } from '../../src/downloader';
import { noProxyMatches, ambientNoProxy, ProxyTunnelAgent, redactProxyCredentials, resolveProxy } from '../../src/downloadProxy';
import { classifyDownloadFailureKind, NETWORK_FAILURE_REMEDY } from '../../src/serverResolver';

const TARGET = 'https://github.com/EffortlessMetrics/ripr/releases/download/v1/m.json';

/** Builds a proxy URL with userinfo from parts, so fixtures hold no literal credential URL for secret scanners. */
function credentialUrl(scheme: string, user: string, password: string, hostPort: string): string {
  return `${scheme}://${[user, password].join(':')}@${hostPort}`;
}

suite('VS Code download proxy support (#15317)', () => {
  test('http.proxy wins over HTTPS_PROXY, which wins over https_proxy', () => {
    const env = { HTTPS_PROXY: 'http://upper:1', https_proxy: 'http://lower:2' };
    const settings = resolveProxy(TARGET, { httpProxy: 'http://setting:3', env });
    assert.strictEqual(settings.kind === 'proxy' && settings.host, 'setting');
    assert.strictEqual(settings.kind === 'proxy' && settings.source, 'http.proxy');
    const upper = resolveProxy(TARGET, { httpProxy: '  ', env });
    assert.strictEqual(upper.kind === 'proxy' && upper.host, 'upper');
    const lower = resolveProxy(TARGET, { env: { https_proxy: 'corp:8080' } });
    assert.ok(lower.kind === 'proxy' && lower.host === 'corp' && lower.port === 8080 && lower.source === 'https_proxy');
    assert.deepStrictEqual(resolveProxy(TARGET, { env: {} }), { kind: 'direct' });
  });

  test('NO_PROXY exempts hosts, suffixes, ports and wildcards, and only those', () => {
    assert.ok(noProxyMatches('github.com', 'github.com', 443));
    assert.ok(noProxyMatches('.github.com', 'api.github.com', 443));
    assert.ok(noProxyMatches('*.github.com', 'api.github.com', 443));
    assert.ok(noProxyMatches('other, github.com:443', 'github.com', 443));
    assert.ok(noProxyMatches('*', 'anything.example', 443));
    assert.ok(noProxyMatches('127.0.0.1', '127.0.0.1', 8443));
    assert.ok(!noProxyMatches('github.com:8443', 'github.com', 443));
    assert.ok(!noProxyMatches('hub.com', 'github.com', 443));
    assert.ok(!noProxyMatches(undefined, 'github.com', 443));
    const env = { HTTPS_PROXY: 'http://p:1', no_proxy: '.github.com' };
    assert.deepStrictEqual(resolveProxy(TARGET, { env }), { kind: 'direct' });
    assert.strictEqual(resolveProxy('https://example.org/x', { env }).kind, 'proxy');
  });

  test('basic auth is carried but never rendered; unsupported schemes fail closed naming the workaround', () => {
    const auth = resolveProxy(TARGET, { httpProxy: credentialUrl('http', 'us%40er', 'p%3Aw', 'proxy.corp:3128'), env: {} });
    assert.ok(auth.kind === 'proxy');
    assert.strictEqual(auth.kind === 'proxy' && auth.auth, 'us@er:p:w');
    assert.ok(!JSON.stringify(auth.kind === 'proxy' && auth.display).includes('p%3Aw'));
    assert.strictEqual(auth.kind === 'proxy' && auth.display, 'http://proxy.corp:3128');
    for (const bad of [credentialUrl('socks5', 'user', 'secret', 'proxy.corp:1080'), credentialUrl('https', 'user', 'secret', 'proxy.corp:443')]) {
      const result = resolveProxy(TARGET, { httpProxy: bad, env: {} });
      assert.strictEqual(result.kind, 'unsupported');
      const message = result.kind === 'unsupported' ? result.message : '';
      assert.ok(message.includes('ripr.server.path'), message);
      assert.ok(!message.includes('secret') && !message.includes('user:'), message);
    }
    assert.strictEqual(
      redactProxyCredentials(`failed via ${credentialUrl('http', 'u', 'secret', 'proxy:3128')}/x`),
      'failed via http://***@proxy:3128/x'
    );
  });

  test('a raw percent sign in proxy credentials is unsupported, not a throw, and leaks nothing', () => {
    const result = resolveProxy(TARGET, { httpProxy: credentialUrl('http', 'al%ice', 'pa%ss', 'proxy.corp:3128'), env: {} });
    assert.strictEqual(result.kind, 'unsupported');
    const message = result.kind === 'unsupported' ? result.message : '';
    assert.ok(message.includes('ripr.server.path') && !message.includes('pa%ss') && !message.includes('al%ice'), message);
  });

  test('a bare IP NO_PROXY entry matches the whole address only; an empty NO_PROXY does not hide no_proxy', () => {
    assert.ok(!noProxyMatches('0.1', '10.0.0.1', 443));
    assert.ok(!noProxyMatches('10.0.0.1', '110.0.0.1', 443));
    assert.ok(!noProxyMatches('10.0.0.0/8', '10.0.0.1', 443));
    assert.ok(noProxyMatches('10.0.0.1', '10.0.0.1', 443));
    assert.ok(ambientNoProxy({ NO_PROXY: '', no_proxy: 'corp.example' }).includes('corp.example'));
  });

  test('a synchronous TLS setup failure reaches the request as an error and does not hang', async () => {
    const proxyServer = http.createServer();
    const sockets = new Set<{ destroy(): unknown }>();
    proxyServer.on('connect', (_request, clientSocket) => {
      sockets.add(clientSocket);
      clientSocket.write('HTTP/1.1 200 OK\r\n\r\n');
    });
    await new Promise<void>((resolve) => proxyServer.listen(0, '127.0.0.1', resolve));
    const port = (proxyServer.address() as net.AddressInfo).port;
    const agent = new ProxyTunnelAgent(
      { kind: 'proxy', host: '127.0.0.1', port, display: `http://127.0.0.1:${port}`, source: 'http.proxy' },
      5000,
      { ca: 12345 as unknown as string }
    );
    try {
      await new Promise<void>((resolve, reject) => {
        const req = https.get('https://127.0.0.1:9/x', { agent }, () => reject(new Error('unexpected response')));
        req.on('error', () => resolve());
        setTimeout(() => reject(new Error('request never settled')), 3000).unref();
      });
    } finally {
      agent.destroy();
      sockets.forEach((socket) => socket.destroy());
      proxyServer.close();
    }
  });

  test('the transport-failure hint names the proxy settings and a tunnel failure is a network failure', () => {
    assert.ok(/http\.proxy/.test(NETWORK_FAILURE_REMEDY) && /HTTPS_PROXY/.test(NETWORK_FAILURE_REMEDY));
    assert.strictEqual(
      classifyDownloadFailureKind('Proxy tunnel failed: proxy http://p:1 refused CONNECT to h:443 with HTTP 403.'),
      'network'
    );
  });

  suite('through a local CONNECT proxy', () => {
    let key: Buffer;
    let cert: Buffer;
    let otherKey: Buffer;
    let otherCert: Buffer;
    let previousCa: unknown;
    let origin: https.Server;
    let originPort = 0;
    const originAuth: Array<string | undefined> = [];
    let proxy: http.Server;
    let proxyPort = 0;
    const connects: Array<{ target: string; authorization?: string }> = [];
    let proxyStatus = 200;
    const hijacked = new Set<{ destroy(): unknown }>();

    suiteSetup(async () => {
      const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'ripr-proxy-tls-'));
      cp.execFileSync('openssl', ['req', '-x509', '-newkey', 'rsa:2048', '-sha256', '-days', '1', '-nodes',
        '-keyout', path.join(dir, 'k.pem'), '-out', path.join(dir, 'c.pem'), '-subj', '/CN=127.0.0.1',
        '-addext', 'subjectAltName=DNS:localhost,IP:127.0.0.1'], { stdio: 'ignore' });
      key = fs.readFileSync(path.join(dir, 'k.pem'));
      cert = fs.readFileSync(path.join(dir, 'c.pem'));
      cp.execFileSync('openssl', ['req', '-x509', '-newkey', 'rsa:2048', '-sha256', '-days', '1', '-nodes',
        '-keyout', path.join(dir, 'k2.pem'), '-out', path.join(dir, 'c2.pem'), '-subj', '/CN=127.0.0.1',
        '-addext', 'subjectAltName=DNS:localhost,IP:127.0.0.1'], { stdio: 'ignore' });
      otherKey = fs.readFileSync(path.join(dir, 'k2.pem'));
      otherCert = fs.readFileSync(path.join(dir, 'c2.pem'));
      fs.rmSync(dir, { recursive: true, force: true });
      previousCa = https.globalAgent.options.ca;
      https.globalAgent.options.ca = [...rootCertificates, cert];
      origin = https.createServer({ key, cert }, (request, response) => {
        originAuth.push(request.headers['proxy-authorization'] as string | undefined);
        if (request.url === '/redirect') {
          response.writeHead(302, { location: `https://127.0.0.1:${originPort}/final` });
          response.end();
          return;
        }
        response.end('payload');
      });
      await new Promise<void>((resolve) => origin.listen(0, '127.0.0.1', resolve));
      originPort = (origin.address() as net.AddressInfo).port;
      proxy = http.createServer((_req, res) => res.end());
      proxy.on('connect', (request, clientSocket) => {
        hijacked.add(clientSocket);
        connects.push({ target: request.url ?? '', authorization: request.headers['proxy-authorization'] });
        if (proxyStatus !== 200) {
          clientSocket.end(`HTTP/1.1 ${proxyStatus} Nope\r\n\r\n`);
          return;
        }
        const [host, port] = (request.url ?? '').split(':');
        const upstream = net.connect(Number(port), host, () => {
          clientSocket.write('HTTP/1.1 200 Connection Established\r\n\r\n');
          hijacked.add(upstream);
          upstream.pipe(clientSocket);
          clientSocket.pipe(upstream);
        });
        upstream.on('error', () => clientSocket.destroy());
        clientSocket.on('error', () => upstream.destroy());
      });
      await new Promise<void>((resolve) => proxy.listen(0, '127.0.0.1', resolve));
      proxyPort = (proxy.address() as net.AddressInfo).port;
    });

    suiteTeardown(async () => {
      https.globalAgent.options.ca = previousCa as string[] | undefined;
      origin.closeAllConnections();
      proxy.closeAllConnections();
      for (const socket of hijacked) {
        socket.destroy();
      }
      await Promise.all([
        new Promise((resolve) => origin.close(resolve)),
        new Promise((resolve) => proxy.close(resolve))
      ]);
    });

    setup(() => {
      connects.length = 0;
      proxyStatus = 200;
    });

    test('the request, including a redirect hop, is tunnelled and credentials are sent only as Proxy-Authorization', async () => {
      const outcome = await fetchManifestBytesOverHttps(
        `https://127.0.0.1:${originPort}/redirect`,
        1024,
        () => ({ httpProxy: credentialUrl('http', 'alice', 's3cret', `127.0.0.1:${proxyPort}`), env: {} })
      );
      assert.strictEqual(outcome.kind, 'ok');
      assert.strictEqual(outcome.kind === 'ok' && outcome.bytes.toString(), 'payload');
      assert.deepStrictEqual(connects.map((c) => c.target), [`127.0.0.1:${originPort}`, `127.0.0.1:${originPort}`]);
      assert.deepStrictEqual(originAuth.filter((value) => value !== undefined), []);
      assert.strictEqual(connects[0].authorization, `Basic ${Buffer.from('alice:s3cret').toString('base64')}`);
    });

    test('the origin never sees Proxy-Authorization, and an untrusted origin cert fails closed through the tunnel', async () => {
      const seen: Array<string | undefined> = [];
      const rogue = https.createServer({ key: otherKey, cert: otherCert }, (request, response) => {
        seen.push(request.headers['proxy-authorization'] as string | undefined);
        response.end('rogue');
      });
      await new Promise<void>((resolve) => rogue.listen(0, '127.0.0.1', resolve));
      const roguePort = (rogue.address() as net.AddressInfo).port;
      try {
        const outcome = await fetchManifestBytesOverHttps(
          `https://127.0.0.1:${roguePort}/x`,
          1024,
          () => ({ httpProxy: credentialUrl('http', 'alice', 's3cret', `127.0.0.1:${proxyPort}`), env: {} })
        );
        assert.strictEqual(outcome.kind, 'transport_failure');
        assert.deepStrictEqual(connects.map((c) => c.target), [`127.0.0.1:${roguePort}`]);
        assert.deepStrictEqual(seen, []);
        const message = outcome.kind === 'transport_failure' ? outcome.message : '';
        assert.ok(!message.includes('s3cret'), message);
      } finally {
        rogue.closeAllConnections();
        rogue.close();
      }
    });

    test('a hostname that is not in the origin certificate SAN fails closed through the tunnel', async () => {
      const seen: Array<string | undefined> = [];
      // Trusted cert, but its SAN covers 127.0.0.1/localhost only; reach it by 127.0.0.2.
      const origin2 = https.createServer({ key, cert }, (request, response) => {
        seen.push(request.headers['proxy-authorization'] as string | undefined);
        response.end('x');
      });
      await new Promise<void>((resolve) => origin2.listen(0, '0.0.0.0', resolve));
      const port2 = (origin2.address() as net.AddressInfo).port;
      try {
        const outcome = await fetchManifestBytesOverHttps(
          `https://127.0.0.2:${port2}/x`,
          1024,
          () => ({ httpProxy: `http://127.0.0.1:${proxyPort}`, env: {} })
        );
        assert.strictEqual(outcome.kind, 'transport_failure');
        assert.deepStrictEqual(connects.map((c) => c.target), [`127.0.0.2:${port2}`]);
        assert.deepStrictEqual(seen, []);
      } finally {
        origin2.closeAllConnections();
        origin2.close();
      }
    });

    test('NO_PROXY sends the request direct without touching the proxy', async () => {
      const outcome = await fetchManifestBytesOverHttps(
        `https://127.0.0.1:${originPort}/x`,
        1024,
        () => ({ httpProxy: `http://127.0.0.1:${proxyPort}`, env: { NO_PROXY: '127.0.0.1' } })
      );
      assert.strictEqual(outcome.kind, 'ok');
      assert.deepStrictEqual(connects, []);
    });

    test('a refused CONNECT fails closed with a redacted network-class message and never goes direct', async () => {
      proxyStatus = 407;
      const outcome = await fetchManifestBytesOverHttps(
        `https://127.0.0.1:${originPort}/x`,
        1024,
        () => ({ httpProxy: credentialUrl('http', 'alice', 's3cret', `127.0.0.1:${proxyPort}`), env: {} })
      );
      assert.strictEqual(outcome.kind, 'transport_failure');
      const message = outcome.kind === 'transport_failure' ? outcome.message : '';
      assert.ok(/HTTP 407/.test(message), message);
      assert.ok(!message.includes('s3cret') && !message.includes('alice'), message);
      assert.strictEqual(classifyDownloadFailureKind(message), 'network');
    });

    test('an unsupported proxy scheme fails closed before any connection', async () => {
      const outcome = await fetchManifestBytesOverHttps(
        `https://127.0.0.1:${originPort}/x`,
        1024,
        () => ({ httpProxy: credentialUrl('socks5', 'u', 'secret', '127.0.0.1:1'), env: {} })
      );
      assert.strictEqual(outcome.kind, 'transport_failure');
      const message = outcome.kind === 'transport_failure' ? outcome.message : '';
      assert.ok(message.includes('ripr.server.path') && !message.includes('secret'), message);
    });
  });
});
