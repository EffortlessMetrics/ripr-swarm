import * as assert from 'assert';
import { describeCodeActionFailure, MAX_CODE_ACTION_FAILURE_FIELD_CHARS,
  observeCodeActionProvider, observeCodeActionRequests } from '../multiRootRequestDiagnostics';

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

  test('preserves original client/provider failure fields within the per-field cap', async () => {
    const cap = MAX_CODE_ACTION_FAILURE_FIELD_CHARS;
    const error = Object.assign(new Error('m'.repeat(cap + 20)), {
      name: 'n'.repeat(cap + 20), stack: 'ORIGINAL-CLIENT-STACK' + 's'.repeat(cap), code: -32800
    });
    const events: Record<string, unknown>[] = [];
    const client = { sendRequest: (..._args: unknown[]) => Promise.reject(error) };
    const restore = observeCodeActionRequests(client, () => ({}), events);
    try { await assert.rejects(observeCodeActionProvider(() =>
      client.sendRequest('textDocument/codeAction', {}), () => ({}), events), observed => observed === error); }
    finally { restore(); }
    const failures = events.filter(event => ['client_request_failed', 'provider_failed'].includes(event.kind as string));
    assert.strictEqual(failures.length, 2);
    for (const failure of failures) {
      for (const key of ['error', 'error_name', 'error_message', 'error_stack']) {
        assert.strictEqual(typeof failure[key], 'string', `${key} must retain the original failure field`);
        assert.strictEqual((failure[key] as string).length, cap);
      }
      assert.ok((failure.error_stack as string).startsWith('ORIGINAL-CLIENT-STACK'));
      assert.strictEqual(failure.code, -32800);
    }
    assert.ok(error.stack.length > cap, 'observation must not truncate the original error itself');
  });

  test('hostile failure metadata and observation getters cannot replace client/provider errors', async () => {
    const error = new Proxy(new Error('original'), { get: () => { throw new Error('metadata getter failure'); } });
    const observe = () => ({ get unavailable() { throw new Error('observation getter failure'); } });
    const events: Record<string, unknown>[] = [];
    const client = { sendRequest: (..._args: unknown[]) => Promise.reject(error) };
    const restore = observeCodeActionRequests(client, observe, events);
    try {
      // Node assert.rejects itself reads rejection metadata; use a direct
      // catch so this hostile-getter control reaches our observation boundary.
      let observed: unknown;
      try { await observeCodeActionProvider(() =>
        client.sendRequest('textDocument/codeAction', {}), observe, events); }
      catch (caught) { observed = caught; }
      assert.ok(observed === error, 'diagnostic metadata replaced the original failure');
    } finally { restore(); }
    const failures = events.filter(event => ['client_request_failed', 'provider_failed'].includes(event.kind as string));
    assert.strictEqual(failures.length, 2);
    assert.ok(failures.every(event => event.error === '[failure description unavailable]'
      && event.error_stack === undefined && event.observation_unavailable === true));
    assert.deepStrictEqual(describeCodeActionFailure(error), {
      error: '[failure description unavailable]', error_name: undefined, error_message: undefined,
      error_stack: undefined, code: undefined
    });
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
