//
// The managed server download must stay a plain https `get` request with no custom
// agent: the VS Code extension host's proxy support (`http.proxySupport`)
// replaces or skips an agent the caller passes, and under `on` a caller agent
// disables proxying entirely. Keeping the request agent-free is what makes
// `http.proxy`, `HTTPS_PROXY` and the system proxy apply (#15317).
//
import * as assert from 'assert';
import * as https from 'https';
import { fetchManifestBytesOverHttps } from '../../src/downloader';

suite('Managed download proxy support (#15317)', () => {
  test('the manifest fetch passes no agent, so VS Code proxy support applies', async () => {
    // eslint-disable-next-line @typescript-eslint/no-require-imports
    const httpsModule = require('https') as typeof https;
    const original = httpsModule.get;
    const calls: unknown[][] = [];
    try {
      (httpsModule as { get: unknown }).get = (...args: unknown[]) => {
        calls.push(args);
        throw new Error('stubbed https get');
      };
      await fetchManifestBytesOverHttps('https://github.com/EffortlessMetrics/ripr/releases/download/v1/m.json', 1024);
    } finally {
      (httpsModule as { get: unknown }).get = original;
    }
    assert.strictEqual(calls.length, 1, 'the fetch goes through the https module get');
    for (const arg of calls[0]) {
      if (arg !== null && typeof arg === 'object' && !(arg instanceof URL)) {
        assert.ok(!('agent' in arg), 'a request option agent would bypass VS Code proxy support');
      }
    }
  });
});
