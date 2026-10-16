import * as assert from 'assert';
import { observeCodeActionRequests } from '../multiRootRequestDiagnostics';

suite('Multi-root request diagnostic observation', () => {
  for (const failing of [false, true]) {
    test(`forwards exact request/token and ${failing ? 'error' : 'result'} and disposes listeners`, async () => {
      let cancellations: (() => void) | undefined;
      let disposed = 0;
      const token = { isCancellationRequested: false,
        onCancellationRequested: (listener: () => void) => {
          cancellations = listener;
          return { dispose: () => { disposed++; cancellations = undefined; } };
        } };
      const type = { method: 'textDocument/codeAction' };
      const params = { textDocument: { uri: 'file:///fixture/src/alpha.rs' }, range: { start: 1, end: 2 } };
      const result = [{ title: 'exact action' }];
      const error = Object.assign(new Error('Canceled'), { code: -32801 });
      let calls = 0;
      const client = { sendRequest: function (...args: unknown[]) {
        assert.strictEqual(this, client);
        assert.strictEqual(args[0], type);
        assert.strictEqual(args[1], params);
        assert.strictEqual(args[2], token);
        calls++;
        return failing ? Promise.reject(error) : Promise.resolve(result);
      } };
      const original = client.sendRequest;
      const events: Record<string, unknown>[] = [];
      const restore = observeCodeActionRequests(client, () => ({ pid: 731, generation: 4, version: 2 }), events);
      try {
        const pending = client.sendRequest(type, params, token);
        cancellations?.();
        if (failing) { await assert.rejects(pending, observed => observed === error); }
        else { assert.strictEqual(await pending, result); }
      } finally { restore(); }
      assert.strictEqual(calls, 1, 'observation must never retry a failed request');
      assert.strictEqual(client.sendRequest, original);
      assert.strictEqual(disposed, 1);
      assert.strictEqual(cancellations, undefined);
      assert.deepStrictEqual(events.map(event => event.kind),
        ['client_request_start', 'client_token_canceled', failing ? 'client_request_failed' : 'client_request_result']);
      assert.ok(events.every(event => event.pid === 731 && event.generation === 4 && event.version === 2));
      if (failing) { assert.strictEqual(events[2].code, -32801); }
      assert.ok(events.every(event => event.request_id === 1
        && event.request_uri === params.textDocument.uri && event.request_range === params.range));
      restore();
      assert.strictEqual(disposed, 1, 'cleanup is idempotent');
    });
  }

  test('overlapping document requests retain separate identities through reordered completion', async () => {
    let finishA!: (value: unknown) => void;
    let failB!: (error: unknown) => void;
    const a = new Promise(resolve => { finishA = resolve; });
    const b = new Promise((_resolve, reject) => { failB = reject; });
    const paramsA = { textDocument: { uri: 'file:///alpha/src/alpha.rs' }, range: { start: 1, end: 2 } };
    const paramsB = { textDocument: { uri: 'file:///beta/src/beta.rs' }, range: { start: 3, end: 4 } };
    const calls: unknown[][] = [];
    const client = { sendRequest: (...args: unknown[]) => {
      calls.push(args); return args[1] === paramsA ? a : b;
    } };
    const events: Record<string, unknown>[] = [];
    const restore = observeCodeActionRequests(client, () => ({ uri: 'selected-alpha' }), events);
    const errorB = Object.assign(new Error('Canceled beta'), { code: -32800 });
    try {
      const resultA = client.sendRequest('textDocument/codeAction', paramsA);
      const resultB = client.sendRequest('textDocument/codeAction', paramsB);
      failB(errorB);
      await assert.rejects(resultB, error => error === errorB);
      const actionsA = [{ title: 'alpha action' }];
      finishA(actionsA);
      assert.strictEqual(await resultA, actionsA);
      assert.strictEqual(calls.length, 2);
      assert.strictEqual(calls[0][1], paramsA);
      assert.strictEqual(calls[1][1], paramsB);
      assert.deepStrictEqual(events.map(event => [event.kind, event.request_id, event.request_uri]), [
        ['client_request_start', 1, paramsA.textDocument.uri],
        ['client_request_start', 2, paramsB.textDocument.uri],
        ['client_request_failed', 2, paramsB.textDocument.uri],
        ['client_request_result', 1, paramsA.textDocument.uri],
      ]);
    } finally { restore(); }
  });

  test('unrelated requests retain their original promise without observation', () => {
    const promise = Promise.resolve('status');
    const client = { sendRequest: (..._args: unknown[]) => promise };
    const original = client.sendRequest;
    const events: Record<string, unknown>[] = [];
    const restore = observeCodeActionRequests(client, () => ({}), events);
    try { assert.strictEqual(client.sendRequest('workspace/executeCommand', {}), promise); }
    finally { restore(); }
    assert.deepStrictEqual(events, []);
    assert.strictEqual(client.sendRequest, original);
  });
});
