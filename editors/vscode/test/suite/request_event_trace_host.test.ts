import * as assert from 'assert';
import * as vscode from 'vscode';
import { observeDocuments, RequestEventTrace } from '../requestEventTrace';

// Instrument calibration only; these controlled providers are not RIPR/LSP
// acceptance and do not classify any historical cancellation.
suite('Request event trace real-host controls', function () {
  this.timeout(20_000);
  for (const stimulus of ['model-edit', 'matching-provider-change', 'neither'] as const) {
    test(`${stimulus} observes the real host token without retrying the command`, async () => {
      const trace = new RequestEventTrace();
      const document = await vscode.workspace.openTextDocument({ language: 'plaintext', content: 'trace fixture' });
      const subscriptions = observeDocuments(vscode.workspace, trace, observed => observed === document);
      const selector = { scheme: 'untitled', language: 'plaintext' };
      let entered!: () => void;
      const entry = new Promise<void>(resolve => { entered = resolve; });
      let release!: (value: vscode.CodeAction[]) => void;
      let refuse!: (error: unknown) => void;
      let calls = 0;
      const wrapped = trace.middleware();
      const provider = vscode.languages.registerCodeActionsProvider(selector, {
        provideCodeActions: (doc, range, context, token) => wrapped(doc, range, context, token, () => {
          calls++;
          const pending = new Promise<vscode.CodeAction[]>((resolve, reject) => { release = resolve; refuse = reject; });
          const cancellation = token.onCancellationRequested(() => refuse(new vscode.CancellationError()));
          subscriptions.push(cancellation);
          entered();
          return pending;
        }),
      });
      let second: vscode.Disposable | undefined;
      // Attach the rejection handler immediately; this is the one command.
      const command = Promise.resolve(vscode.commands.executeCommand<vscode.CodeAction[]>(
        'vscode.executeCodeActionProvider', document.uri, new vscode.Range(0, 0, 0, 1)))
        .then(value => ({ value, error: undefined }), error => ({ value: undefined, error }));
      const bounded = async <T>(pending: PromiseLike<T>): Promise<T> => {
        let timer: ReturnType<typeof setTimeout> | undefined;
        try {
          return await Promise.race([pending, new Promise<never>((_, reject) => {
            timer = setTimeout(() => reject(new Error('trace control did not settle')), 5_000);
          })]);
        } finally { if (timer) { clearTimeout(timer); } }
      };
      let primaryFailed = false;
      try {
        await bounded(entry);
        trace.record(`control.${stimulus}`, trace.document(document));
        if (stimulus === 'model-edit') {
          const edit = new vscode.WorkspaceEdit();
          edit.insert(document.uri, new vscode.Position(0, 0), ' ');
          assert.strictEqual(await vscode.workspace.applyEdit(edit), true);
        } else if (stimulus === 'matching-provider-change') {
          second = vscode.languages.registerCodeActionsProvider(selector, { provideCodeActions: () => [] });
        } else { release([]); }
        const settled = await bounded(command);
        if (stimulus === 'neither') { assert.strictEqual(settled.error, undefined); }
        else { assert.ok(settled.error, 'the controlled cancellation must reject the original command'); }
        const events = trace.snapshot().events as Array<Record<string, unknown>>;
        assert.strictEqual(calls, 1, 'the control must exercise one provider invocation');
        assert.strictEqual(events.some(event => event.kind === 'provider.cancel'), stimulus !== 'neither');
        assert.strictEqual(events.some(event => event.kind === 'document.change'
          && Number(event.content_changes) > 0), stimulus === 'model-edit');
        assert.strictEqual(trace.snapshot().dropped_events, 0);
      } catch (error) {
        primaryFailed = true;
        throw error;
      } finally {
        // Settle only this fixture's pending provider before removing it.
        let cleanupFailed = false;
        let cleanupError: unknown;
        try {
          release?.([]);
          await bounded(command);
        } catch (error) {
          cleanupFailed = true;
          cleanupError = error;
        } finally {
          // An unentered provider leaves release unset and command pending.
          // Neither that timeout nor one throwing disposer may skip the rest.
          for (const resource of [second, provider, ...subscriptions]) {
            try { resource?.dispose(); }
            catch (error) {
              if (!cleanupFailed) { cleanupFailed = true; cleanupError = error; }
            }
          }
        }
        if (!primaryFailed && cleanupFailed) { throw cleanupError; }
      }
    });
  }
});
