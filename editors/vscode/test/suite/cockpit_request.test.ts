import * as assert from 'assert';
import { AgentHandoffController, type AgentHandoffHost } from '../../src/agentHandoff';
import type { RiprLanguageClient } from '../../src/client';
import {
  NO_RUNNING_SERVER_MESSAGE,
  cockpitUnavailableMessage,
  interpretCockpitObjectResponse,
  messageClaimsHungServer
} from '../../src/cockpitRequest';

const HUNG_SERVER_MESSAGE = 'No receipt status available — server not responding.';
const SESSION_CHANGED_MESSAGE =
  'The ripr workspace session changed or is switching. Run the command again when the current workspace is ready.';

suite('Cockpit request absence split', () => {
  test('no-client message never claims the server hung', () => {
    const message = cockpitUnavailableMessage({ kind: 'no_client' }, HUNG_SERVER_MESSAGE);

    assert.strictEqual(message, NO_RUNNING_SERVER_MESSAGE);
    assert.strictEqual(messageClaimsHungServer(message), false);
    assert.ok(message.includes('No ripr server is running'));
  });

  test('unavailable keeps the caller hung-server wording', () => {
    const message = cockpitUnavailableMessage({ kind: 'unavailable' }, HUNG_SERVER_MESSAGE);

    assert.strictEqual(message, HUNG_SERVER_MESSAGE);
    assert.strictEqual(messageClaimsHungServer(message), true);
  });

  test('a changed session has its own retry guidance instead of a hung-server claim', () => {
    const message = cockpitUnavailableMessage({ kind: 'session_changed' }, HUNG_SERVER_MESSAGE);

    assert.strictEqual(message, SESSION_CHANGED_MESSAGE);
    assert.strictEqual(messageClaimsHungServer(message), false);
    assert.notStrictEqual(message, NO_RUNNING_SERVER_MESSAGE);
  });

  test('null, undefined, and non-object responses are unavailable, not no-client', () => {
    assert.deepStrictEqual(interpretCockpitObjectResponse(null), { kind: 'unavailable' });
    assert.deepStrictEqual(interpretCockpitObjectResponse(undefined), { kind: 'unavailable' });
    assert.deepStrictEqual(interpretCockpitObjectResponse(42), { kind: 'unavailable' });
    assert.deepStrictEqual(interpretCockpitObjectResponse('not-a-packet'), { kind: 'unavailable' });
  });

  test('object responses stay on the ok path', () => {
    const value = { kind: 'receipt_status', receipt_status: 'improved' };
    assert.deepStrictEqual(interpretCockpitObjectResponse(value), { kind: 'ok', value });
  });

  test('arrays count as objects and stay on the ok path for the caller to reject', () => {
    const value = ['not', 'a', 'packet'];
    assert.deepStrictEqual(interpretCockpitObjectResponse(value), { kind: 'ok', value });
  });
});

interface DeferredReply {
  promise: Promise<unknown>;
  resolve(value: unknown): void;
  reject(reason: Error): void;
}

function deferredReply(): DeferredReply {
  let resolve: DeferredReply['resolve'] = () => undefined;
  let reject: DeferredReply['reject'] = () => undefined;
  const promise = new Promise<unknown>((onResolve, onReject) => {
    resolve = onResolve;
    reject = onReject;
  });
  return { promise, resolve, reject };
}

const repairPacket = {
  kind: 'repair_packet',
  canonical_gap_id: 'gap-from-root-A',
  verify_command: 'ripr --root /workspace/A verify',
  receipt_command: 'ripr --root /workspace/A receipt'
};
const receiptStatus = {
  receipt_status: 'improved-in-root-A',
  copy_receipt_command: 'ripr --root /workspace/A receipt'
};

type WorkspaceCommandName =
  | 'copyTopRepairPacket'
  | 'copyTopVerifyCommand'
  | 'copyTopReceiptCommand'
  | 'copyReceiptCommand'
  | 'showReceiptStatus';

const workspaceCommands: Array<{
  name: WorkspaceCommandName;
  request: string;
  reply: Record<string, unknown>;
  copied?: string;
}> = [
  { name: 'copyTopRepairPacket', request: 'ripr.collectRepairPacket', reply: repairPacket,
    copied: JSON.stringify(repairPacket, null, 2) },
  { name: 'copyTopVerifyCommand', request: 'ripr.collectRepairPacket', reply: repairPacket,
    copied: repairPacket.verify_command },
  { name: 'copyTopReceiptCommand', request: 'ripr.collectRepairPacket', reply: repairPacket,
    copied: repairPacket.receipt_command },
  { name: 'copyReceiptCommand', request: 'ripr.collectReceiptStatus', reply: receiptStatus,
    copied: receiptStatus.copy_receipt_command },
  { name: 'showReceiptStatus', request: 'ripr.collectReceiptStatus', reply: receiptStatus }
];

function workspaceCommandContext(reply: Promise<unknown>) {
  const clipboard: string[] = [];
  const messages: string[] = [];
  const output: string[] = [];
  const requests: Array<{ method: string; params: unknown }> = [];
  const makeClient = (): RiprLanguageClient => ({
    sendRequest: (method: string, params: unknown) => {
      requests.push({ method, params });
      return reply;
    }
  } as RiprLanguageClient);
  const state = {
    client: makeClient() as RiprLanguageClient | undefined,
    root: '/workspace/A',
    generation: 1,
    blocker: undefined as string | undefined
  };
  // Only the workspace-command surface is injected. File-scoped guards throw
  // so these palette commands cannot accidentally require an active editor.
  const host = {
    getClient: () => state.client,
    getWorkspaceRoot: () => state.root,
    getStartGeneration: () => state.generation,
    sessionRootBlocker: () => state.blocker,
    copyActionRootBlocker: () => { throw new Error('Workspace command required an active editor'); },
    actionRootStillCurrent: () => { throw new Error('Workspace command used a file-scoped guard'); },
    runtime: {
      writeClipboard: async (text: string) => { clipboard.push(text); },
      showInformationMessage: (message: string) => { messages.push(message); }
    },
    output: { appendLine: (line: string) => { output.push(line); }, show: () => undefined }
  } as unknown as AgentHandoffHost;
  return {
    controller: new AgentHandoffController(host), state, makeClient,
    clipboard, messages, output, requests
  };
}

type WorkspaceCommandContext = ReturnType<typeof workspaceCommandContext>;

function assertSuppressed(context: WorkspaceCommandContext): void {
  assert.deepStrictEqual(context.clipboard, []);
  assert.strictEqual(context.messages.at(-1), SESSION_CHANGED_MESSAGE);
  assert.ok(!context.output.some((line) => line.includes('improved-in-root-A')));
  assert.ok(!context.messages.some((message) => message.includes('improved-in-root-A')));
}

suite('Workspace cockpit reply currentness (#13133)', () => {
  for (const command of workspaceCommands) {
    test(`${command.name} accepts a current reply without an active editor`, async () => {
      const context = workspaceCommandContext(Promise.resolve(command.reply));

      await context.controller[command.name]();

      assert.deepStrictEqual(context.requests, [{
        method: 'workspace/executeCommand', params: { command: command.request, arguments: [] }
      }]);
      assert.deepStrictEqual(context.clipboard, command.copied === undefined ? [] : [command.copied]);
      if (command.name === 'showReceiptStatus') {
        assert.strictEqual(context.messages.at(-1), 'ripr receipt status: improved-in-root-A');
        assert.ok(context.output.some((line) => line.includes('improved-in-root-A')));
      }
    });

    test(`${command.name} discards a deferred reply after switching from A to B`, async () => {
      const deferred = deferredReply();
      const context = workspaceCommandContext(deferred.promise);
      const work = context.controller[command.name]();
      assert.strictEqual(context.requests.length, 1);

      context.state.client = context.makeClient();
      context.state.root = '/workspace/B';
      context.state.generation += 1;
      deferred.resolve(command.reply);
      await work;

      assertSuppressed(context);
    });

    test(`${command.name} checks the session again in the consuming continuation`, async () => {
      const deferred = deferredReply();
      const context = workspaceCommandContext(deferred.promise);
      const work = context.controller[command.name]();
      assert.strictEqual(context.requests.length, 1);
      // The request await was registered first. This continuation changes the
      // session after its settlement handler and before the caller consumes it.
      const transition = deferred.promise.then(() => {
        context.state.generation += 1;
      });
      deferred.resolve(command.reply);
      await Promise.all([work, transition]);

      assertSuppressed(context);
    });

    test(`${command.name} does not request a packet during a pending root transition`, async () => {
      const context = workspaceCommandContext(Promise.resolve(command.reply));
      context.state.blocker = 'A root transition is pending.';

      await context.controller[command.name]();

      assert.deepStrictEqual(context.requests, []);
      assertSuppressed(context);
    });

    test(`${command.name} retains no-client guidance`, async () => {
      const context = workspaceCommandContext(Promise.resolve(command.reply));
      context.state.client = undefined;

      await context.controller[command.name]();

      assert.deepStrictEqual(context.requests, []);
      assert.deepStrictEqual(context.clipboard, []);
      assert.strictEqual(context.messages.at(-1), NO_RUNNING_SERVER_MESSAGE);
    });
  }

  const transitions: Array<{ name: string; apply(context: WorkspaceCommandContext): void }> = [
    { name: 'client replacement with the same root and generation',
      apply: (context) => { context.state.client = context.makeClient(); } },
    { name: 'same-root restart generation change',
      apply: (context) => { context.state.generation += 1; } },
    { name: 'selected root change with the same client and generation',
      apply: (context) => { context.state.root = '/workspace/B'; } },
    { name: 'root transition pending while the old client is still present',
      apply: (context) => { context.state.blocker = 'A root transition is pending.'; } },
    { name: 'removal quarantine while teardown is still pending',
      apply: (context) => { context.state.blocker = 'The selected root was removed.'; } }
  ];
  for (const transition of transitions) {
    test(`copyTopVerifyCommand suppresses ${transition.name}`, async () => {
      const deferred = deferredReply();
      const context = workspaceCommandContext(deferred.promise);
      const work = context.controller.copyTopVerifyCommand();
      assert.strictEqual(context.requests.length, 1);

      transition.apply(context);
      deferred.resolve(repairPacket);
      await work;

      assertSuppressed(context);
    });
  }

  test('a rejected old-session request gets session recovery guidance', async () => {
    const deferred = deferredReply();
    const context = workspaceCommandContext(deferred.promise);
    const work = context.controller.copyReceiptCommand();
    context.state.generation += 1;
    deferred.reject(new Error('old client stopped'));
    await work;

    assertSuppressed(context);
  });

  test('current-session null and rejected replies retain request-failure guidance', async () => {
    for (const reject of [false, true]) {
      const deferred = deferredReply();
      const context = workspaceCommandContext(deferred.promise);
      const work = context.controller.copyReceiptCommand();
      if (reject) {
        deferred.reject(new Error('live client failed'));
      } else {
        deferred.resolve(null);
      }
      await work;

      assert.deepStrictEqual(context.clipboard, []);
      assert.strictEqual(context.messages.at(-1), 'No receipt command is available — server not responding.');
      assert.notStrictEqual(context.messages.at(-1), NO_RUNNING_SERVER_MESSAGE);
      assert.notStrictEqual(context.messages.at(-1), SESSION_CHANGED_MESSAGE);
    }
  });

  test('a session change before the caller consumes a rejection supersedes request-failure guidance', async () => {
    const deferred = deferredReply();
    const context = workspaceCommandContext(deferred.promise);
    const work = context.controller.copyReceiptCommand();
    const transition = deferred.promise.catch(() => {
      context.state.generation += 1;
    });
    deferred.reject(new Error('previous client failed'));
    await Promise.all([work, transition]);

    assertSuppressed(context);
  });
});
