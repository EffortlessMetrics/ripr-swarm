import * as assert from 'assert';
import { describeCodeActionFailure, MAX_CODE_ACTION_FAILURE_FIELD_CHARS,
  observeCodeActionProvider, observeCodeActionRequests, observeCodeActionRequestWindow } from '../multiRootRequestDiagnostics';
import { MAX_REQUEST_TRACE_BYTES, MAX_REQUEST_TRACE_EVENTS, RequestEventTrace } from '../requestEventTrace';

suite('Multi-root request diagnostic observation', () => {
  for (const failing of [false, true]) {
    test(`request window retains ${failing ? 'rejection' : 'resolution'} and closes its listeners`, async () => {
      const trace = new RequestEventTrace(() => 0);
      let cancel: (() => void) | undefined;
      let disposed = 0;
      const token = { isCancellationRequested: false,
        onCancellationRequested: (listener: () => void) => {
          cancel = listener;
          return { dispose: () => { disposed++; cancel = undefined; } };
        } };
      const type = { method: 'textDocument/codeAction' };
      const params = { textDocument: { uri: 'file:///root-a/alpha.rs' },
        range: { start: { line: 1, character: 0 }, end: { line: 1, character: 28 } } };
      const originalError = Object.assign(new Error('Canceled'), { code: -32800 });
      const actions = [{ title: 'unchanged action' }];
      let calls = 0;
      const client = { sendRequest: function (...args: unknown[]) {
        assert.strictEqual(this, client);
        assert.strictEqual(args[0], type);
        assert.strictEqual(args[1], params);
        assert.strictEqual(args[2], token);
        calls++;
        return failing ? Promise.reject(originalError) : Promise.resolve(actions);
      } };
      const original = client.sendRequest;
      const run = observeCodeActionRequestWindow(client, () => {
        const result = client.sendRequest(type, params, token);
        cancel?.();
        return result;
      }, () => ({ uri: params.textDocument.uri, pid: 731, generation: 4,
        document: [{ version: 2, dirty: false, closed: false }] }),
      trace.clientRequestEvents({ client_id: 7 }));
      if (failing) { await assert.rejects(run, error => error === originalError); }
      else { assert.strictEqual(await run, actions); }
      assert.strictEqual(calls, 1, 'the original command must run exactly once');
      const events = trace.snapshot().events as Record<string, unknown>[];
      assert.deepStrictEqual(events.map(event => event.kind),
        ['client.request.start', 'client.request.cancel', failing ? 'client.request.reject' : 'client.request.resolve']);
      assert.ok(events.every(event => event.client_id === 7 && event.client_pid === 731 && event.client_request_id === 1
        && event.request_window_id === 1 && event.document_uri === params.textDocument.uri
        && event.document_version === 2 && event.range_end_character === 28));
      if (!failing) { assert.strictEqual(events.at(-1)!.result_count, actions.length); }
      assert.strictEqual(client.sendRequest, original);
      assert.strictEqual(disposed, 1);
      assert.strictEqual(cancel, undefined);
    });
  }

  for (const originalError of [undefined, null, false, 0, '', new Error('sync failure')]) {
    test(`request window restores after synchronous rejection ${String(originalError)}`, async () => {
      const trace = new RequestEventTrace();
      const client = { sendRequest: (..._args: unknown[]): unknown => { throw originalError; } };
      const original = client.sendRequest;
      let rejected = false;
      try {
        await observeCodeActionRequestWindow(client, () => client.sendRequest('textDocument/codeAction', {}),
          () => ({}), trace.clientRequestEvents({ client_id: 7 }));
      } catch (caught) { rejected = true; assert.strictEqual(caught, originalError); }
      assert.ok(rejected, 'even a falsy rejection must stay a rejection');
      assert.strictEqual(client.sendRequest, original);
      const events = trace.snapshot().events as Record<string, unknown>[];
      assert.deepStrictEqual(events.map(event => event.kind), ['client.request.start', 'client.request.reject']);
      assert.strictEqual((events[1].error as { message: string }).message,
        originalError instanceof Error ? originalError.message : String(originalError));
    });
  }

  test('one broken disposer does not strand later listeners or replace the command failure', async () => {
    const trace = new RequestEventTrace();
    const disposed: number[] = [];
    const tokens = [0, 1].map(index => ({ isCancellationRequested: false,
      onCancellationRequested: () => ({ dispose: () => {
        disposed.push(index);
        if (index === 0) { throw new Error('disposal failed'); }
      } }) }));
    const originalError = new Error('command failed');
    const client = { sendRequest: (..._args: unknown[]) => Promise.resolve([]) };
    const original = client.sendRequest;
    await assert.rejects(observeCodeActionRequestWindow(client, async () => {
      for (const token of tokens) { await client.sendRequest('textDocument/codeAction', {}, token); }
      throw originalError;
    }, () => ({}), trace.clientRequestEvents({ client_id: 7 })), error => error === originalError);
    assert.strictEqual(client.sendRequest, original);
    assert.deepStrictEqual(disposed, [0, 1]);
    const failures = (trace.snapshot().events as Record<string, unknown>[])
      .filter(event => event.kind === 'client.observer.cleanup-failed');
    assert.strictEqual(failures.length, 1);
    assert.strictEqual((failures[0].error as { message: string }).message, 'disposal failed');
  });

  test('closed windows exclude new requests but retain their late response with distinct local IDs', async () => {
    const trace = new RequestEventTrace();
    let finish!: (value: string) => void;
    const pending = new Promise<string>(resolve => { finish = resolve; });
    let observedPending: Promise<string> | undefined;
    const client = { sendRequest: (..._args: unknown[]) => pending };
    const original = client.sendRequest;
    const params = { textDocument: { uri: 'file:///other.rs' },
      range: { start: { line: 3, character: 4 }, end: { line: 5, character: 6 } } };
    const observe = () => ({ uri: 'file:///selected.rs', document: [{ version: 99 }] });
    await observeCodeActionRequestWindow(client, () => {
      observedPending = client.sendRequest('textDocument/codeAction', params);
      return 'command settled first';
    }, observe, trace.clientRequestEvents({ client_id: 7 }));
    assert.strictEqual(client.sendRequest, original);
    const before = (trace.snapshot().events as unknown[]).length;
    assert.strictEqual(client.sendRequest('textDocument/codeAction', params), pending);
    assert.strictEqual((trace.snapshot().events as unknown[]).length, before);
    const second = observeCodeActionRequestWindow(client,
      () => client.sendRequest('textDocument/codeAction', params), observe,
      trace.clientRequestEvents({ client_id: 7 }));
    finish('same result');
    assert.strictEqual(await observedPending, 'same result');
    assert.strictEqual(await second, 'same result');
    const events = trace.snapshot().events as Record<string, unknown>[];
    assert.deepStrictEqual(events.map(event => [event.kind, event.request_window_id, event.client_request_id]), [
      ['client.request.start', 1, 1], ['client.request.start', 2, 1],
      ['client.request.resolve', 1, 1], ['client.request.resolve', 2, 1],
    ]);
    assert.ok(events.every(event => event.document_uri === params.textDocument.uri
      && event.document_version === undefined && event.range_end_character === 6));
  });

  test('retained observations use the existing bounded sanitizer and reject hostile metadata', async () => {
    const trace = new RequestEventTrace(() => 0);
    const sink = trace.clientRequestEvents({ client_id: 7 });
    const secret = 'ghp_' + 'a'.repeat(80);
    const prefix = 'x'.repeat(MAX_CODE_ACTION_FAILURE_FIELD_CHARS - 10);
    const error = Object.assign(new Error(prefix + secret), { code: -32800,
      name: 'OriginalCancellation', stack: 'original-stack ' + secret });
    const client = { sendRequest: (..._args: unknown[]) => Promise.reject(error) };
    await assert.rejects(observeCodeActionRequestWindow(client,
      () => client.sendRequest('textDocument/codeAction', { textDocument: { uri: 'file:///alpha.rs' } }),
      () => ({}), sink), received => received === error);
    let snapshot = trace.snapshot();
    assert.ok(!JSON.stringify(snapshot).includes('ghp_'), 'redact before clipping a secret in half');
    const failure = (snapshot.events as Record<string, unknown>[])[1].error as Record<string, unknown>;
    assert.strictEqual(failure.name, 'OriginalCancellation');
    assert.strictEqual(failure.code, -32800);
    assert.ok(String(failure.stack).startsWith('original-stack '));
    const cyclic: Record<string, unknown> = {}; cyclic.self = cyclic;
    sink.push({ kind: 'client_request_start', request_id: Infinity, pid: NaN, count: -Infinity,
      request_uri: 'file:///actual.rs', uri: 'file:///selected.rs', document: [{ version: 99 }], arbitrary: cyclic });
    const last = (trace.snapshot().events as Record<string, unknown>[]).at(-1)!;
    assert.ok(!('client_request_id' in last) && !('client_pid' in last) && !('result_count' in last)
      && !('document_version' in last) && !('arbitrary' in last));
    sink.push({ get kind() { throw new Error('kind getter'); } });
    sink.push({ kind: 'client_request_start', get request_range() { throw new Error('range getter'); } });
    assert.strictEqual(trace.snapshot().dropped_events, 2);
    for (let i = 0; i < MAX_REQUEST_TRACE_EVENTS + 1; i++) {
      sink.push({ kind: 'client_request_start', request_id: i, request_uri: 'file:///' + 'x'.repeat(4096) });
    }
    snapshot = trace.snapshot();
    assert.ok((snapshot.events as unknown[]).length <= MAX_REQUEST_TRACE_EVENTS);
    assert.ok(Number(snapshot.dropped_events) > 2);
    assert.ok(Buffer.byteLength(JSON.stringify({ request_event_trace: snapshot }, null, 2)) <= MAX_REQUEST_TRACE_BYTES);
  });

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
