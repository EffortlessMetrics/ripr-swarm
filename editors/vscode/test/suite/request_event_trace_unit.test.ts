import * as assert from 'assert';
import type * as vscode from 'vscode';
import { MAX_REQUEST_TRACE_BYTES, MAX_REQUEST_TRACE_EVENTS, observeDocuments, RequestEventTrace } from '../requestEventTrace';

class Token {
  isCancellationRequested = false;
  readonly listeners = new Set<() => void>();
  onCancellationRequested = (listener: () => void) => {
    this.listeners.add(listener);
    return { dispose: () => { this.listeners.delete(listener); } };
  };
  cancel(): void {
    this.isCancellationRequested = true;
    for (const listener of this.listeners) { listener(); }
  }
}

suite('Request event trace unit controls', () => {
  for (const stimulus of ['model-edit', 'matching-provider-change', 'neither'] as const) {
    test(`${stimulus} retains distinct observations and exactly one original invocation`, async () => {
      let tick = 0;
      const trace = new RequestEventTrace(() => ++tick);
      const document = { uri: { toString: () => 'file:///fixture/alpha.rs' }, version: 2 } as vscode.TextDocument;
      let changed: ((event: vscode.TextDocumentChangeEvent) => void) | undefined;
      const noop = () => ({ dispose() {} });
      const subscriptions = observeDocuments({
        onDidOpenTextDocument: noop, onDidSaveTextDocument: noop, onDidCloseTextDocument: noop,
        onDidChangeTextDocument: listener => { changed = listener; return { dispose() {} }; },
      }, trace, candidate => candidate === document);
      const token = new Token();
      const originalError = new Error('original cancellation');
      const actions: vscode.Command[] = [];
      let release!: (value: vscode.Command[]) => void;
      let reject!: (error: unknown) => void;
      const pending = new Promise<vscode.Command[]>((resolve, refuse) => { release = resolve; reject = refuse; });
      let calls = 0;
      const range = {} as vscode.Range;
      const context = { diagnostics: [], triggerKind: 1 } as vscode.CodeActionContext;
      const result = trace.middleware()(document, range, context, token as vscode.CancellationToken,
        (receivedDocument, receivedRange, receivedContext, receivedToken) => {
          calls++;
          assert.strictEqual(receivedDocument, document);
          assert.strictEqual(receivedRange, range);
          assert.strictEqual(receivedContext, context);
          assert.strictEqual(receivedToken, token);
          return pending;
        });
      trace.record(`control.${stimulus}`);
      if (stimulus === 'model-edit') {
        Object.assign(document, { version: 3 });
        changed?.({ document, contentChanges: [{}] } as vscode.TextDocumentChangeEvent);
      }
      if (stimulus === 'neither') { release(actions); assert.strictEqual(await result, actions); }
      else {
        token.cancel(); reject(originalError);
        await assert.rejects(Promise.resolve(result), error => error === originalError);
      }
      const events = trace.snapshot().events as Array<Record<string, unknown>>;
      assert.strictEqual(calls, 1);
      assert.strictEqual(events.filter(event => event.kind === 'provider.begin').length, 1);
      assert.strictEqual(events.some(event => event.kind === 'provider.cancel'), stimulus !== 'neither');
      assert.strictEqual(events.some(event => event.kind === 'document.change'), stimulus === 'model-edit');
      if (stimulus === 'model-edit') {
        const changedEvent = events.find(event => event.kind === 'document.change')!;
        assert.strictEqual(changedEvent.document_version, 3);
        assert.strictEqual(changedEvent.content_changes, 1);
        assert.strictEqual(changedEvent.document_id, events[0].document_id);
      }
      assert.strictEqual(token.listeners.size, 0, 'settlement must detach its cancellation listener');
      assert.ok(events.every((event, index) => index === 0
        || Number(event.sequence) > Number(events[index - 1].sequence)));
      assert.strictEqual(trace.snapshot().dropped_events, 0);
      for (const subscription of subscriptions) { subscription.dispose(); }
    });
  }

  test('preserves existing middleware, synchronous value and original thrown error', () => {
    const trace = new RequestEventTrace();
    const token = new Token();
    const document = { uri: { toString: () => 'file:///fixture/alpha.rs' }, version: 2 } as vscode.TextDocument;
    const actions: vscode.Command[] = [];
    let calls = 0;
    const wrapped = trace.middleware((doc, range, context, cancellation, next) => {
      calls++; return next(doc, range, context, cancellation);
    });
    assert.strictEqual(wrapped(document, {} as vscode.Range, {} as vscode.CodeActionContext,
      token as vscode.CancellationToken, () => actions), actions);
    assert.strictEqual(calls, 1);
    const original = new Error('original synchronous failure');
    assert.throws(() => wrapped(document, {} as vscode.Range, {} as vscode.CodeActionContext,
      token as vscode.CancellationToken, () => { throw original; }), error => error === original);
    assert.strictEqual(token.listeners.size, 0);
  });

  test('bounds and sanitizes metadata; replacement documents get distinct local identities', () => {
    const trace = new RequestEventTrace();
    const token = `ghp_${'a'.repeat(30)}`;
    const first = { uri: { toString: () => `file:///fixture/${token}` }, version: 2 } as vscode.TextDocument;
    const reopened = { ...first } as vscode.TextDocument;
    assert.notStrictEqual(trace.document(first).document_id, trace.document(reopened).document_id);
    for (let index = 0; index < 150; index++) {
      trace.record('command.reject', { ...trace.document(first), secret: token } as never,
        new Error(`failure ${token} ${'x'.repeat(3000)}`));
    }
    const snapshot = trace.snapshot();
    const serialized = JSON.stringify(snapshot);
    assert.ok(Buffer.byteLength(serialized) <= MAX_REQUEST_TRACE_BYTES);
    assert.ok(Buffer.byteLength(JSON.stringify({ request_event_trace: snapshot }, null, 2))
      <= MAX_REQUEST_TRACE_BYTES, 'the pretty-printed receipt must respect the trace budget');
    assert.ok((snapshot.events as unknown[]).length <= MAX_REQUEST_TRACE_EVENTS);
    assert.ok(Number(snapshot.dropped_events) > 0);
    assert.ok(!serialized.includes(token));
    assert.ok(!serialized.includes('"secret"'));
    assert.ok(!serialized.includes('wire_request_id'));
  });

  test('an observation clock failure cannot replace the original result', () => {
    let clockCalls = 0;
    const trace = new RequestEventTrace(() => {
      if (clockCalls++ > 0) { throw new Error('observer unavailable'); }
      return 0;
    });
    const token = new Token();
    const document = { uri: { toString: () => 'file:///fixture/alpha.rs' }, version: 2 } as vscode.TextDocument;
    const actions: vscode.Command[] = [];
    assert.strictEqual(trace.middleware()(document, {} as vscode.Range, {} as vscode.CodeActionContext,
      token as vscode.CancellationToken, () => actions), actions);
    assert.strictEqual(token.listeners.size, 0);
    assert.ok(Number(trace.snapshot().dropped_events) > 0);
  });
});
