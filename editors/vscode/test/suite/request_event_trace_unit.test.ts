import * as assert from 'assert';
import * as fs from 'fs';
import * as path from 'path';
import { stripTypeScriptTypes } from 'node:module';
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

  test('a synchronously rejecting Thenable records one terminal event and preserves its error', () => {
    const trace = new RequestEventTrace();
    const token = new Token();
    const document = { uri: { toString: () => 'file:///fixture/alpha.rs' }, version: 2 } as vscode.TextDocument;
    const original = new Error('original synchronous Thenable rejection');
    let forwards = 0;
    const thenable = { then: (_resolve: unknown, reject: (error: unknown) => unknown) => reject(original) };
    assert.throws(() => trace.middleware()(document, {} as vscode.Range, {} as vscode.CodeActionContext,
      token as vscode.CancellationToken, () => {
        forwards++; return thenable as unknown as Thenable<vscode.CodeAction[]>;
      }), error => error === original);
    const events = trace.snapshot().events as Array<Record<string, unknown>>;
    assert.strictEqual(forwards, 1);
    assert.deepStrictEqual(events.map(event => event.kind), ['provider.begin', 'provider.reject']);
    assert.strictEqual(token.listeners.size, 0);
  });

  test('the actual host control disposes every fixture after an entry and cleanup timeout', async () => {
    // Execute the committed control with API doubles, not a copied cleanup.
    // Native runs read their compiled sibling; the offline syntax replay reads TS.
    const native = path.join(__dirname, 'request_event_trace_host.test.js');
    const sourcePath = fs.existsSync(native) ? native : [
      path.join(__dirname, 'request_event_trace_host.test.ts'),
      path.resolve(__dirname, '../../../test/suite/request_event_trace_host.test.ts'),
    ].find(file => fs.existsSync(file));
    assert.ok(sourcePath, 'the actual host control source must be available');
    const callbacks = new Map<string, () => Promise<void>>();
    let providers = 0;
    let disposedProviders = 0;
    let listeners = 0;
    let disposedListeners = 0;
    let commands = 0;
    const subscribe = () => {
      listeners++;
      return { dispose: () => {
        disposedListeners++;
        if (disposedListeners === 1) { throw new Error('first listener disposal failed'); }
      } };
    };
    const fakeApi = {
      workspace: { openTextDocument: async () => ({ uri: { toString: () => 'untitled:trace' }, version: 1 }),
        onDidOpenTextDocument: subscribe, onDidChangeTextDocument: subscribe,
        onDidSaveTextDocument: subscribe, onDidCloseTextDocument: subscribe },
      languages: { registerCodeActionsProvider: () => {
        providers++; return { dispose: () => { disposedProviders++; } };
      } },
      commands: { executeCommand: () => { commands++; return new Promise(() => {}); } },
      Range: class {},
    };
    const timeouts: Error[] = [];
    class ObservedTimeout extends Error {
      constructor(message: string) { super(message); timeouts.push(this); }
    }
    const defineSuite = (_name: string, callback: () => void) => callback.call({ timeout() {} });
    const defineTest = (name: string, callback: () => Promise<void>) => { callbacks.set(name, callback); };
    const timer = (callback: () => void) => { queueMicrotask(callback); return 1; };
    const source = fs.readFileSync(sourcePath, 'utf8');
    if (sourcePath === native) {
      const requireDouble = (name: string) => {
        if (name === 'assert') { return assert; }
        if (name === 'vscode') { return fakeApi; }
        if (name === '../requestEventTrace') { return { observeDocuments, RequestEventTrace }; }
        throw new Error(`unexpected host-control import: ${name}`);
      };
      new Function('require', 'exports', 'suite', 'test', 'setTimeout', 'clearTimeout', 'Error', source)(
        requireDouble, {}, defineSuite, defineTest, timer, () => {}, ObservedTimeout);
    } else {
      const body = stripTypeScriptTypes(source.replace(/^import .*;\r?\n/gm, ''), { mode: 'transform' });
      new Function('vscode', 'assert', 'observeDocuments', 'RequestEventTrace', 'suite', 'test',
        'setTimeout', 'clearTimeout', 'Error', body)(fakeApi, assert, observeDocuments, RequestEventTrace,
        defineSuite, defineTest, timer, () => {}, ObservedTimeout);
    }
    const control = callbacks.get('model-edit observes the real host token without retrying the command');
    assert.ok(control, 'must execute the real model-edit control body');
    let failure: unknown;
    try { await control(); } catch (error) { failure = error; }
    assert.strictEqual(commands, 1, 'timeout must not retry the command');
    assert.strictEqual(providers, 1);
    assert.strictEqual(disposedProviders, 1, 'the provider must be disposed even when it was never entered');
    assert.strictEqual(listeners, 4);
    assert.strictEqual(disposedListeners, 4, 'all document listeners must be disposed after cleanup timeout');
    assert.strictEqual(timeouts.length, 2, 'exercise both entry and cleanup timeouts');
    assert.strictEqual(failure, timeouts[0], 'cleanup must preserve the original entry failure');
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
