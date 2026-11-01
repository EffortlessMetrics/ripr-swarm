import * as assert from 'assert';
import * as fs from 'fs';
import * as path from 'path';
import { createRequire } from 'module';
import * as vm from 'vm';
import { observeCodeActionProvider, observeCodeActionRequests } from '../multiRootRequestDiagnostics';

interface Provider {
  provideCodeActions(document: unknown, range: unknown, context: unknown, token: unknown): Promise<unknown>;
}

// Execute the installed, lock-pinned CodeActionFeature and its actual base
// classes. Only VS Code registration is replaced: this exercises the retained
// provider/client/conversion boundary, not an Electron host or server transport.
function retainedProvider(client: object): Provider {
  const entry = require.resolve('vscode-languageclient/$test/common/codeAction');
  const common = path.dirname(entry);
  const modules = new Map<string, { exports: Record<string, unknown> }>();
  let registered: Provider | undefined;
  const vscode = {
    CancellationError: class extends Error {},
    languages: { registerCodeActionsProvider: (_selector: unknown, provider: Provider) => {
      registered = provider; return { dispose: () => {} };
    } }
  };
  function load(file: string): Record<string, unknown> {
    const existing = modules.get(file);
    if (existing) { return existing.exports; }
    const module = { exports: {} as Record<string, unknown> };
    modules.set(file, module);
    const local = createRequire(file);
    const imports = (name: string): unknown => {
      if (name === 'vscode') { return vscode; }
      const resolved = local.resolve(name);
      return resolved.startsWith(`${common}${path.sep}`) ? load(resolved) : local(name);
    };
    const execute = vm.runInThisContext(
      `(function(exports, require, module, __filename, __dirname) {\n${fs.readFileSync(file, 'utf8')}\n})`,
      { filename: file }
    ) as (exports: object, imports: (name: string) => unknown, module: object, file: string, dir: string) => void;
    execute(module.exports, imports, module, file, path.dirname(file));
    return module.exports;
  }
  const Feature = load(entry).CodeActionFeature as new (client: object) => {
    registerLanguageProvider(options: object): [unknown, Provider];
  };
  const [, provider] = new Feature(client).registerLanguageProvider({ documentSelector: [{ language: 'rust' }] });
  assert.strictEqual(registered, provider, 'must reach the retained registration authority');
  return provider;
}

function fixture() {
  let cancel: (() => void) | undefined;
  let disposed = 0;
  const token = { isCancellationRequested: false,
    onCancellationRequested: (callback: () => void) => {
      cancel = callback; return { dispose: () => { disposed++; cancel = undefined; } };
    } };
  const document = { uri: 'file:///alpha/src/alpha.rs', version: 2 };
  const range = { start: { line: 1, character: 4 }, end: { line: 1, character: 25 } };
  const context = { diagnostics: [] };
  const values = [{ title: 'root A context' }];
  const events: Record<string, unknown>[] = [];
  let request: (...args: unknown[]) => Promise<unknown> = async () => values;
  let convert: (input: unknown, observedToken: unknown) => Promise<unknown> = async input => input;
  let failed: (...args: unknown[]) => never = () => { throw new Error('unexpected failure delegation'); };
  let calls = 0;
  const client = {
    middleware: {},
    code2ProtocolConverter: {
      asTextDocumentIdentifier: (doc: unknown) => { assert.strictEqual(doc, document); return { uri: document.uri }; },
      asRange: (input: unknown) => { assert.strictEqual(input, range); return range; },
      asCodeActionContextSync: (input: unknown) => { assert.strictEqual(input, context); return context; }
    },
    protocol2CodeConverter: {
      asDocumentSelector: (selector: unknown) => selector,
      asCodeActionResult: (input: unknown, observedToken: unknown) => convert(input, observedToken)
    },
    sendRequest: (...args: unknown[]) => { calls++; return request(...args); },
    handleFailedRequest: (...args: unknown[]) => failed(...args)
  };
  const provider = retainedProvider(client);
  const original = client.sendRequest;
  const restore = observeCodeActionRequests(client, () => ({ version: document.version }), events);
  return { client, provider, document, range, context, token, values, events,
    request: (handler: typeof request) => { request = handler; },
    convert: (handler: typeof convert) => { convert = handler; },
    failed: (handler: typeof failed) => { failed = handler; },
    cancel: () => { token.isCancellationRequested = true; cancel?.(); },
    run: () => provider.provideCodeActions(document, range, context, token),
    close: () => { restore(); assert.strictEqual(client.sendRequest, original); assert.strictEqual(disposed, 1); },
    calls: () => calls };
}

suite('Retained CodeActionFeature diagnostic boundary', () => {
  test('forwards a rejected request to its retained failure authority without retry or suppression', async () => {
    const f = fixture();
    const error = Object.assign(new Error('server cancellation'), { code: -32800 });
    f.request(async (method, params, token) => {
      assert.strictEqual((method as { method: string }).method, 'textDocument/codeAction');
      assert.deepStrictEqual(params, { textDocument: { uri: f.document.uri }, range: f.range, context: f.context });
      assert.strictEqual(token, f.token); throw error;
    });
    f.failed((method, token, observed, fallback) => {
      assert.strictEqual((method as { method: string }).method, 'textDocument/codeAction');
      assert.strictEqual(token, f.token); assert.strictEqual(observed, error); assert.strictEqual(fallback, null);
      throw observed;
    });
    try {
      await assert.rejects(f.run(), observed => observed === error);
      assert.strictEqual(f.calls(), 1);
      assert.deepStrictEqual(f.events.map(event => event.kind), ['client_request_start', 'client_request_failed']);
      assert.strictEqual(f.events[1].code, -32800);
    } finally { f.close(); }
  });

  test('distinguishes conversion failure after a successful client request from a rejected request', async () => {
    const f = fixture();
    const error = new Error('late conversion cancellation');
    error.stack = 'ORIGINAL-PROVIDER-CONVERSION-STACK';
    let conversionEntered!: () => void;
    let rejectConversion!: (error: unknown) => void;
    const entered = new Promise<void>(resolve => { conversionEntered = resolve; });
    const conversion = new Promise((_resolve, reject) => { rejectConversion = reject; });
    f.convert((values, token) => {
      assert.strictEqual(values, f.values); assert.strictEqual(token, f.token); conversionEntered(); return conversion;
    });
    try {
      const pending = observeCodeActionProvider(() => f.run(), () => ({ phase: 'select root A' }), f.events);
      await entered;
      f.cancel(); rejectConversion(error);
      await assert.rejects(pending, observed => observed === error);
      assert.strictEqual(f.calls(), 1);
      assert.deepStrictEqual(f.events.map(event => event.kind),
        ['provider_start', 'client_request_start', 'client_request_result', 'client_token_canceled',
          'provider_failed', 'provider_end']);
      assert.strictEqual(f.events[2].count, 1);
      assert.strictEqual(f.events[4].error_stack, 'ORIGINAL-PROVIDER-CONVERSION-STACK');
      assert.strictEqual(f.events[4].error_name, 'Error');
      assert.strictEqual(f.events[4].error_message, 'late conversion cancellation');
      assert.strictEqual(f.events[4].phase, 'select root A');
    } finally { f.close(); }
  });

  test('records cancellation before response while retaining the provider\'s existing canceled-result contract', async () => {
    const f = fixture();
    let complete!: (value: unknown) => void;
    f.request(() => new Promise(resolve => { complete = resolve; }));
    f.convert(async () => { assert.fail('canceled provider must not convert stale actions'); });
    try {
      const pending = f.run(); f.cancel(); complete(f.values);
      assert.strictEqual(await pending, null);
      assert.strictEqual(f.calls(), 1);
      assert.deepStrictEqual(f.events.map(event => event.kind),
        ['client_request_start', 'client_token_canceled', 'client_request_result']);
    } finally { f.close(); }
  });
});
