import * as assert from 'assert';
import * as http from 'http';
import * as net from 'net';
import * as vscode from 'vscode';
import { fetchManifestBytesOverHttps } from '../../src/downloader';
import { NETWORK_FAILURE_REMEDY } from '../../src/serverResolver';

// The managed download uses Node's https module inside the extension host,
// which VS Code patches with its own proxy support (`http.proxySupport`,
// default `override`). These tests pin that the download really goes through
// `http.proxy` and honors `http.noProxy`, instead of reimplementing a proxy
// client the host would bypass (#15317).
const PROBE_HOST = 'ripr-proxy-probe.invalid';
const PROBE_URL = `https://${PROBE_HOST}/manifest.json`;

suite('VS Code download proxy support (#15317)', () => {
  let proxy: http.Server;
  let proxyPort = 0;
  const connects: string[] = [];
  const sockets = new Set<net.Socket>();
  let previousProxy: unknown;
  let previousNoProxy: unknown;

  suiteSetup(async () => {
    proxy = http.createServer((_request, response) => response.end());
    proxy.on('connect', (request, socket: net.Socket) => {
      sockets.add(socket);
      connects.push(request.url ?? '');
      socket.end('HTTP/1.1 407 Proxy Authentication Required\r\n\r\n');
    });
    await new Promise<void>((resolve) => proxy.listen(0, '127.0.0.1', resolve));
    proxyPort = (proxy.address() as net.AddressInfo).port;
    const settings = vscode.workspace.getConfiguration('http');
    previousProxy = settings.inspect('proxy')?.globalValue;
    previousNoProxy = settings.inspect('noProxy')?.globalValue;
  });

  suiteTeardown(async () => {
    const settings = vscode.workspace.getConfiguration('http');
    await settings.update('proxy', previousProxy, vscode.ConfigurationTarget.Global);
    await settings.update('noProxy', previousNoProxy, vscode.ConfigurationTarget.Global);
    for (const socket of sockets) {
      socket.destroy();
    }
    await new Promise((resolve) => proxy.close(resolve));
  });

  setup(() => {
    connects.length = 0;
  });

  // Resolve once the host has seen the change, so the proxy resolver reads the
  // new value instead of racing the configuration event.
  async function update(key: 'proxy' | 'noProxy', value: unknown): Promise<void> {
    const settings = vscode.workspace.getConfiguration('http');
    if (JSON.stringify(settings.inspect(key)?.globalValue) === JSON.stringify(value)) {
      return;
    }
    const seen = new Promise<void>((resolve) => {
      const timer = setTimeout(() => { listener.dispose(); resolve(); }, 5000);
      const listener = vscode.workspace.onDidChangeConfiguration((event) => {
        if (event.affectsConfiguration(`http.${key}`)) {
          clearTimeout(timer);
          listener.dispose();
          resolve();
        }
      });
    });
    await settings.update(key, value, vscode.ConfigurationTarget.Global);
    await seen;
  }

  async function configure(noProxy: string[] | undefined): Promise<void> {
    await update('proxy', `http://127.0.0.1:${proxyPort}`);
    await update('noProxy', noProxy);
  }

  test('the managed download goes through http.proxy and the proxy refusal reaches it as HTTP 407', async () => {
    await configure(undefined);
    const outcome = await fetchManifestBytesOverHttps(PROBE_URL, 1024);
    assert.deepStrictEqual(
      connects.filter((target) => target.startsWith(PROBE_HOST)),
      [`${PROBE_HOST}:443`],
      JSON.stringify(outcome)
    );
    assert.strictEqual(outcome.kind, 'http_failure', JSON.stringify(outcome));
    assert.strictEqual(outcome.kind === 'http_failure' && outcome.statusCode, 407, JSON.stringify(outcome));
  });

  test('a host listed in http.noProxy bypasses the proxy', async () => {
    await configure([PROBE_HOST]);
    const outcome = await fetchManifestBytesOverHttps(PROBE_URL, 1024);
    assert.deepStrictEqual(
      connects.filter((target) => target.startsWith(PROBE_HOST)),
      [],
      JSON.stringify(outcome)
    );
    // Direct, `.invalid` never resolves; an environment proxy (if any) may
    // answer instead. Either way the download must not succeed.
    assert.notStrictEqual(outcome.kind, 'ok', JSON.stringify(outcome));
  });

  test('the transport-failure hint names the VS Code proxy settings', () => {
    assert.ok(/http\.proxy\b/.test(NETWORK_FAILURE_REMEDY), NETWORK_FAILURE_REMEDY);
    assert.ok(/http\.proxySupport/.test(NETWORK_FAILURE_REMEDY), NETWORK_FAILURE_REMEDY);
  });
});
