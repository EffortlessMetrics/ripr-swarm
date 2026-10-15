import * as assert from 'assert';
import * as crypto from 'crypto';
import * as fs from 'fs';
import * as path from 'path';
import type { ChildProcess, SpawnOptions } from 'child_process';
import * as vscode from 'vscode';
import type { LanguageClient } from 'vscode-languageclient/node';
import { assertActionTargets, assertContextPacket, normalizeUriSpelling, RootExpectation } from '../multiRootAssertions';

// Read observations from the activated controller without supplying a runtime,
// resolver, client, root selector, status producer or diagnostic collection.
// Private-field shape is intentionally checked at setup and fails closed.
interface ControllerObservation {
  client?: LanguageClient;
  workspaceRoot?: string;
  status: { kind: string };
}

interface ProcessObservation {
  child: ChildProcess;
  pid?: number;
  args: readonly string[];
  cwd?: string;
  session: boolean;
  exits: number;
  closes: number;
}

// Register only in the dedicated fresh-profile host. Other suites mutate the
// single-root fixture; their execution cannot count towards this matrix.
if (process.env.RIPR_TEST_MULTIROOT === '1') {
  suite('Real multi-root server acceptance', function () {
    this.timeout(180_000);
    let controller: ControllerObservation;
    let rootA: RootExpectation;
    let rootB: RootExpectation;
    let restoreSpawn: (() => void) | undefined;
    let deactivateActual: (() => Promise<void>) | undefined;
    const processes: ProcessObservation[] = [];
    const stages: Record<string, unknown>[] = [];
    let phase = 'setup';
    let outcome = 'not_established';

    suiteSetup(async () => {
      const fixture = process.env.RIPR_TEST_MULTIROOT_PATH;
      const server = process.env.RIPR_TEST_SERVER_PATH;
      assert.ok(fixture && server, 'matrix needs staged roots and an exact server path');
      assert.ok(path.isAbsolute(server) && fs.statSync(server).isFile());
      rootA = {
        root: vscode.Uri.file(path.join(fixture, 'root-a')).fsPath, source: 'src/alpha.rs',
        sourceContents: 'pub fn alpha_discount(amount: i32, threshold: i32) -> i32 {\n    if amount >= threshold {\n        amount - 10\n    } else {\n        amount\n    }\n}\n',
        testContents: 'use root_alpha::alpha_discount;\n\n#[test]\nfn alpha_below_boundary() {\n    assert_eq!(alpha_discount(50, 100), 50);\n}\n\n#[test]\nfn alpha_far_above_boundary() {\n    assert_eq!(alpha_discount(10_000, 100), 9_990);\n}\n',
        symbol: 'alpha_discount', changedExpression: 'amount >= threshold',
        relatedTest: 'tests/alpha_contract.rs',
        testNames: ['alpha_below_boundary', 'alpha_far_above_boundary'],
      };
      rootB = {
        root: vscode.Uri.file(path.join(fixture, 'root-b')).fsPath, source: 'src/beta.rs',
        sourceContents: 'pub fn beta_shipping(weight: i32, cutoff: i32) -> i32 {\n    if weight >= cutoff {\n        weight * 3\n    } else {\n        7\n    }\n}\n',
        testContents: 'use root_beta::beta_shipping;\n\n#[test]\nfn beta_light_parcel() {\n    assert_eq!(beta_shipping(5, 10), 7);\n}\n\n#[test]\nfn beta_heavy_parcel() {\n    assert_eq!(beta_shipping(100, 10), 300);\n}\n',
        symbol: 'beta_shipping', changedExpression: 'weight >= cutoff',
        relatedTest: 'tests/beta_contract.rs',
        testNames: ['beta_light_parcel', 'beta_heavy_parcel'],
      };
      assert.deepStrictEqual(vscode.workspace.workspaceFolders?.map(f => f.uri.fsPath),
        [rootA.root, rootB.root], 'must be the actual two-root .code-workspace');
      assert.strictEqual(vscode.window.activeTextEditor, undefined,
        'fresh profile must start without a selected file');
      assert.strictEqual(vscode.workspace.isTrusted, true);
      assert.strictEqual(vscode.workspace.getConfiguration('ripr').get('server.path'), server);

      // Transparent spawn observation includes resolver probes as well as LSP
      // sessions, forwarding the original arguments/options/streams untouched.
      const cp = require('child_process') as typeof import('child_process');
      const originalSpawn = cp.spawn;
      cp.spawn = (function (command: string, argsOrOptions?: readonly string[] | SpawnOptions, options?: SpawnOptions) {
        const child: ChildProcess = Reflect.apply(originalSpawn, cp, arguments);
        if (command === server) {
          const args = Array.isArray(argsOrOptions) ? argsOrOptions : [];
          const spawnOptions = (Array.isArray(argsOrOptions) ? options : argsOrOptions) as SpawnOptions | undefined;
          const cwd = spawnOptions?.cwd === undefined ? undefined : String(spawnOptions.cwd);
          const session = [rootA.root, rootB.root].includes(cwd ?? '')
            && (args ?? []).join(' ') === 'lsp --stdio';
          const observed: ProcessObservation = {
            child, pid: child.pid, args: [...(args ?? [])], cwd, session, exits: 0, closes: 0,
          };
          child.on('exit', () => observed.exits++);
          child.on('close', () => observed.closes++);
          processes.push(observed);
        }
        return child;
      }) as typeof cp.spawn;
      restoreSpawn = () => { cp.spawn = originalSpawn; };

      // Capture the real activation-scoped instance via an existing command;
      // the wrapper delegates the complete production implementation unchanged.
      const extension = vscode.extensions.getExtension('effortlessmetrics.ripr');
      assert.ok(extension, 'real extension must be installed in this host');
      // Load from the extension actually under test, including an extracted
      // VSIX when the caller supplies RIPR_TEST_EXTENSION_PATH. The module
      // must be the instance the running controller was constructed from: a
      // Windows host can spell the development path with a different
      // drive-letter case than the activation loader used, so resolve the
      // module cache case-insensitively before falling back to a fresh
      // require that would observe a class the extension never uses.
      const { RiprClientController } = requireExtensionModule(extension.extensionPath, 'out/src/client') as typeof import('../../src/client');
      deactivateActual = (requireExtensionModule(extension.extensionPath, 'out/src/extension') as typeof import('../../src/extension')).deactivate;
      const originalShowStatus = RiprClientController.prototype.showStatus;
      RiprClientController.prototype.showStatus = function () {
        controller = this as unknown as ControllerObservation;
        return originalShowStatus.call(this);
      };
      try {
        await extension.activate();
        await vscode.commands.executeCommand('ripr.showStatus');
        assert.ok(controller?.status && typeof controller.status.kind === 'string',
          'failed to observe the activated controller');
      } finally {
        RiprClientController.prototype.showStatus = originalShowStatus;
      }
      stages.push({ phase, server, sha256: crypto.createHash('sha256')
        .update(fs.readFileSync(server)).digest('hex'), extension: extension.extensionPath,
        extension_mode: process.env.RIPR_TEST_EXTENSION_PATH ? 'supplied extension' : 'development extension',
        platform: process.platform });
    });

    test('ambiguity → A → B → active-root removal contains authority and reaps servers', async () => {
      phase = 'no selected file';
      assert.strictEqual(controller.status.kind, 'workspaceAmbiguous');
      assert.strictEqual(controller.workspaceRoot, undefined);
      assert.strictEqual(controller.client, undefined);
      assert.strictEqual(processes.length, 0, 'ambiguity must not even resolve/spawn a server');
      assertOnlyRootDiagnostics(undefined);
      record();

      phase = 'select root A';
      await openAndSave(rootA);
      const aActions = await assertRootEvidence(rootA, rootB);
      assert.strictEqual(sessions().length, 1, 'A must start exactly one real session');
      assert.strictEqual(sessions()[0].cwd, rootA.root);
      assert.strictEqual(sessions()[0].exits, 0);
      record();

      phase = 'switch to root B';
      // No manual restart/picker/runtime override: the active supported file
      // and its real save event are the acceptance stimulus.
      await openAndSave(rootB);
      const bActions = await assertRootEvidence(rootB, rootA);
      assert.strictEqual(sessions().length, 2, 'switch must start one B session');
      assert.strictEqual(sessions()[1].cwd, rootB.root);
      await assertReaped(sessions()[0]);
      await assertCachedActionsBlocked(aActions);
      record();

      phase = 'remove active root B';
      let subscription: vscode.Disposable | undefined;
      const changed = new Promise<void>(resolve => {
        subscription = vscode.workspace.onDidChangeWorkspaceFolders(event => {
          if (event.removed.some(f => f.uri.fsPath === rootB.root)) {
            subscription?.dispose(); resolve();
          }
        });
      });
      try {
        assert.strictEqual(vscode.workspace.updateWorkspaceFolders(1, 1), true);
        await bounded(changed, 'actual B removal event');
      } finally { subscription?.dispose(); }
      await waitFor(() => controller.client === undefined
        && ['noWorkspace', 'workspaceAmbiguous'].includes(controller.status.kind),
        'explicit no-root status after B removal');
      assert.strictEqual(controller.workspaceRoot, undefined, 'removed root must lose authority');
      assert.deepStrictEqual(vscode.workspace.workspaceFolders?.map(f => f.uri.fsPath), [rootA.root]);
      assert.strictEqual(sessions().length, 2, 'remaining A must not start silently');
      await assertReaped(sessions()[1]);
      assertOnlyRootDiagnostics(undefined);
      await assertNoRootActions(rootA);
      await assertNoRootActions(rootB);
      await assertCachedActionsBlocked(aActions);
      await assertCachedActionsBlocked(bActions);
      record();
      outcome = 'passed';
    });

    teardown(function () {
      if (this.currentTest?.state === 'failed') {
        outcome = 'failed';
        const failure = this.currentTest.err;
        stages.push({ phase, failure: String(failure) });
        // Re-tag the reported failure with the journey phase: mocha reports
        // hook and test failures identically, and the phase is the first
        // question every reader of a CI log asks. Mutating the existing
        // error keeps the assertion's actual/expected fields and original
        // stack that Mocha's reporter reads.
        if (failure instanceof Error && !failure.message.includes(`[phase: ${phase}]`)) {
          failure.message = `[phase: ${phase}] ${failure.message}`;
        }
      }
    });

    suiteTeardown(async () => {
      // Production deactivation owns normal shutdown. Retain leak evidence
      // before emergency cleanup of only the exact ChildProcess handles above.
      let teardownError: unknown;
      let beforeCleanup: unknown;
      const cleanupErrors: string[] = [];
      try {
        if (deactivateActual) { await bounded(deactivateActual(), 'extension deactivation'); }
        for (const observed of processes) { await assertReaped(observed); }
      } catch (error) {
        teardownError = error;
        outcome = 'failed_teardown';
        beforeCleanup = processes.map(({ child, ...observed }) => ({
          ...observed, exit_code: child.exitCode, signal: child.signalCode,
        }));
        for (const observed of processes) {
          try { await cleanupOwnedProcess(observed); }
          catch (cleanupError) { cleanupErrors.push(String(cleanupError)); }
        }
      } finally {
        restoreSpawn?.();
        const receipt = process.env.RIPR_TEST_MULTIROOT_RECEIPT;
        assert.ok(receipt, 'matrix receipt path is required');
        fs.writeFileSync(receipt, JSON.stringify({
          outcome, last_phase: phase, stages,
          processes: processes.map(({ child, ...observed }) => ({
            ...observed, exit_code: child.exitCode, signal: child.signalCode,
          })),
          teardown_error: teardownError === undefined ? undefined : String(teardownError),
          before_emergency_cleanup: beforeCleanup,
          emergency_cleanup_errors: cleanupErrors,
          claim_limit: 'Real extension/LSP session acceptance; no final release-candidate qualification or Windows descendant-containment claim.',
        }, null, 2));
      }
      if (teardownError !== undefined) { throw teardownError; }
    });

    function sessions(): ProcessObservation[] { return processes.filter(p => p.session); }
    function record(): void {
      stages.push({ phase, status: controller.status.kind, root: controller.workspaceRoot,
        session_pids: sessions().map(p => p.pid), diagnostic_uris: riprDiagnostics().map(([uri]) => uri.toString()) });
    }

    async function assertRootEvidence(expected: RootExpectation, other: RootExpectation): Promise<vscode.Command[]> {
      await waitFor(() => controller.workspaceRoot === expected.root && controller.client?.isRunning() === true
        && controller.client.serverProcess?.pid !== undefined,
        `real session bound to ${expected.root}`);
      const client = controller.client!;
      assert.ok(client.serverProcess?.pid, 'selected session must own a real server process');
      // Interactive save refreshes deliberately defer expensive seam evidence,
      // so drive the framed full-refresh wire command until the full-seam
      // snapshot commits and its seam diagnostics publish. On CI, refreshes
      // issued while the session-start save refresh is still settling can
      // complete without committing full evidence (the save's own deferred
      // attempt supersedes them), while the same wire command issued after
      // the settling window commits — so re-issue on a cadence instead of
      // failing on the startup churn.
      const attemptBeforeRefresh = (await bounded(client.sendRequest('workspace/executeCommand', {
        command: 'ripr.collectWorkspaceStatus', arguments: [],
      }), 'framed workspace status') as { analysis_status?: { attempt_id?: string; run_status?: string } })
        .analysis_status;
      const uri = vscode.Uri.file(path.join(expected.root, expected.source));
      const seamPresent = () => vscode.languages.getDiagnostics(uri).some(d => d.source === 'ripr'
        && (typeof d.code === 'object' ? d.code.value : d.code) === 'ripr-seam-weakly-gripped');
      // The copy requires the CURRENT snapshot to retain the seam inventory:
      // seam diagnostics alone can race a deferred did_save refresh that
      // supersedes the full snapshot (run_status flips back to
      // seams_deferred and the cited evidence goes stale). Evidence is ready
      // only while the seam diagnostics are published AND the projected
      // run status is a seam-retaining full-family state.
      const seamRetainingRunStatus = (runStatus: string | undefined) =>
        runStatus !== 'stale' && runStatus !== 'seams_deferred';
      const sampleStatus = async () => (await bounded(client.sendRequest('workspace/executeCommand', {
        command: 'ripr.collectWorkspaceStatus', arguments: [],
      }), 'framed workspace status') as { analysis_status?: { attempt_id?: string; run_status?: string; effective_root?: string } })
        .analysis_status;
      const evidenceReady = async () => {
        if (!seamPresent()) {
          return false;
        }
        const status = await sampleStatus();
        // The evidence must be published AND bound to THIS root under a
        // seam-retaining run status: an unbound/restarting server projects
        // no effective_root, and a stale-or-deferred snapshot does not
        // retain the seam inventory the copy cites.
        return status?.effective_root?.replace(/\\/g, '/') === expected.root.replace(/\\/g, '/')
          && seamRetainingRunStatus(status?.run_status);
      };
      const seamDeadline = Date.now() + 90_000;
      let attemptAfterRefresh = attemptBeforeRefresh?.attempt_id;
      let lastError: unknown;
      while (Date.now() < seamDeadline && !(await evidenceReady())) {
        try {
          await bounded(client.sendRequest('workspace/executeCommand', {
            command: 'ripr.refresh', arguments: [],
          }), 'framed full refresh');
        } catch (error) {
          lastError = error;
        }
        // The framed refresh resolves when its analysis settles; give the
        // publication a moment, then re-issue if the evidence is not current.
        try {
          await waitFor(evidenceReady, 'seam publication', 10_000);
        } catch (error) {
          lastError = error;
        }
        attemptAfterRefresh = (await sampleStatus())?.attempt_id;
      }
      assert.notStrictEqual(attemptAfterRefresh, attemptBeforeRefresh?.attempt_id,
        `the explicit full refresh must commit a new analysis attempt`
        + ` (last error: ${String(lastError)})`);
      await waitFor(evidenceReady, `nonempty real seam diagnostics at ${uri}`, 10_000, async () => {
        const status = await client.sendRequest('workspace/executeCommand', {
          command: 'ripr.collectWorkspaceStatus', arguments: [],
        }) as { analysis_status?: Record<string, unknown> };
        return JSON.stringify({
          attempt_after_first_refresh: attemptAfterRefresh,
          attempt: (status.analysis_status as { attempt_id?: string })?.attempt_id,
          run_status: (status.analysis_status as { run_status?: string })?.run_status,
          profile: (status.analysis_status as { diagnostic_profile?: string })?.diagnostic_profile,
          components: (status.analysis_status as { components?: Array<{ component: string; state: string; kind: string | null }> })
            ?.components?.map(c => `${c.component}:${c.state}${c.kind ? `(${c.kind})` : ''}`),
        });
      });
      // The bound-root status is sampled through a bounded wait: a root
      // reconciliation restart can transiently project a status without the
      // effective root (the restart settles within the window).
      const boundStatus = async () => {
        const sampled = await sampleStatus() as { effective_root?: string; root_state?: string };
        return sampled ?? {};
      };
      await waitFor(async () => {
        const s = await boundStatus();
        return s.effective_root?.replace(/\\/g, '/') === expected.root.replace(/\\/g, '/')
          && s.root_state === 'selected_single_root';
      }, 'framed workspace status bound to the selected root', 20_000, async () => {
        const s = await boundStatus();
        return JSON.stringify(s);
      });
      const status = { analysis_status: await boundStatus() };
      // The server's status projects its canonical root spelling (forward
      // slashes on every host); identity compares separators-insensitively.
      assert.strictEqual(status.analysis_status?.effective_root?.replace(/\\/g, '/'),
        expected.root.replace(/\\/g, '/'));
      assert.strictEqual(status.analysis_status?.root_state, 'selected_single_root');
      assertOnlyRootDiagnostics(expected.root);
      assert.strictEqual(vscode.languages.getDiagnostics(vscode.Uri.file(path.join(other.root, other.source)))
        .filter(d => d.source === 'ripr').length, 0, 'other root must have no retained diagnostics');
      const diagnostic = vscode.languages.getDiagnostics(uri).find(d => d.source === 'ripr'
        && (typeof d.code === 'object' ? d.code.value : d.code) === 'ripr-seam-weakly-gripped')!;
      assert.strictEqual(diagnostic.message,
        `Weakly gripped behavioral seam (predicate_boundary): ${expected.changedExpression}`);
      assert.strictEqual(diagnostic.range.start.line, 1);
      let actions: Array<vscode.CodeAction | vscode.Command>;
      try {
        actions = await vscode.commands.executeCommand<Array<vscode.CodeAction | vscode.Command>>(
          'vscode.executeCodeActionProvider', uri, diagnostic.range);
      } catch (error) {
        // executeCodeActionProvider throws a bare illegal-argument when the
        // URI has no open text model; disclose the open-document set so the
        // failing point is diagnosable from the CI log alone.
        throw new Error(`code actions for ${uri} failed: ${String(error)}`
          + ` open=${vscode.workspace.textDocuments.map(d => d.uri.toString()).join(', ')}`
          + ` phase=${phase}`, { cause: error });
      }
      const commands = (actions ?? []).flatMap(action => {
        const command = typeof action.command === 'string' ? action as vscode.Command : action.command;
        return command?.command.startsWith('ripr.') ? [command] : [];
      });
      assert.ok(commands.length > 0, 'real LSP must return nonempty actions');
      assertActionTargets(commands, expected, uri.toString(),
        vscode.Uri.file(path.join(expected.root, expected.relatedTest)).toString());
      const context = commands.find(c => c.command === 'ripr.copyContext'
        && c.arguments?.[0]?.label === undefined);
      const related = commands.find(c => c.command === 'ripr.openRelatedTest');
      assert.ok(context && related, 'context and related-test actions must be produced by real LSP');
      const relatedTarget = related.arguments?.[0] as { uri?: string; test_name?: string };
      assert.strictEqual(normalizeUriSpelling(relatedTarget.uri ?? ''),
        vscode.Uri.file(path.join(expected.root, expected.relatedTest)).toString());
      assert.ok(expected.testNames.includes(relatedTarget.test_name ?? ''), JSON.stringify(related));
      await vscode.commands.executeCommand(related.command, ...(related.arguments ?? []));
      const editor = vscode.window.activeTextEditor;
      assert.strictEqual(editor?.document.uri.toString(), normalizeUriSpelling(relatedTarget.uri ?? ''));
      assert.strictEqual(editor!.document.getText().replace(/\r\n/g, '\n'), expected.testContents);
      assert.strictEqual(editor!.selection.active.line, 3 + 5 * expected.testNames.indexOf(relatedTarget.test_name!));
      assert.strictEqual(editor!.document.lineAt(editor!.selection.active.line).text,
        `fn ${relatedTarget.test_name}() {`, 'navigation must land on the actual expected declaration');
      await vscode.commands.executeCommand(context.command, ...(context.arguments ?? []));
      const packet = await vscode.env.clipboard.readText();
      let parsedPacket: unknown;
      try {
        parsedPacket = JSON.parse(packet);
        assertContextPacket(parsedPacket, expected);
      } catch (error) {
        const status = await client.sendRequest('workspace/executeCommand', {
          command: 'ripr.collectWorkspaceStatus', arguments: [],
        }) as { analysis_status?: Record<string, unknown> };
        throw new Error(`context copy verification failed with clipboard ${JSON.stringify(packet.slice(0, 300))}: ${String(error)}`
          + ` status=${JSON.stringify(status.analysis_status)}`, { cause: error });
      }
      const rootedCommands = commands.filter(c => c.command.startsWith('ripr.copy')
        && typeof c.arguments?.[0]?.command === 'string');
      for (const command of rootedCommands) {
        await vscode.commands.executeCommand(command.command, ...(command.arguments ?? []));
        assert.strictEqual(await vscode.env.clipboard.readText(), command.arguments![0].command,
          'real command handler must copy the root-contained command unchanged');
      }
      await assertNoRootActions(other);
      stages.push({ phase, server_status: status, actions: commands });
      return [context, related, ...rootedCommands];
    }

    async function assertCachedActionsBlocked(commands: vscode.Command[]): Promise<void> {
      await vscode.env.clipboard.writeText('multi-root stale-action sentinel');
      const before = vscode.window.activeTextEditor?.document.uri.toString();
      for (const command of commands) {
        await vscode.commands.executeCommand(command.command, ...(command.arguments ?? []));
      }
      assert.strictEqual(await vscode.env.clipboard.readText(), 'multi-root stale-action sentinel');
      assert.strictEqual(vscode.window.activeTextEditor?.document.uri.toString(), before,
        'cached navigation must not open a stale root target');
    }
  });
}

function requireExtensionModule(extensionPath: string, relative: string): unknown {
  const resolved = require.resolve(path.join(extensionPath, relative));
  const cached = Object.keys(require.cache)
    .find(key => key.toLowerCase() === resolved.toLowerCase());
  const loaded = cached ? require.cache[cached]?.exports : undefined;
  return loaded ?? require(resolved);
}

function riprDiagnostics(): Array<[vscode.Uri, vscode.Diagnostic[]]> {
  return vscode.languages.getDiagnostics().map(([uri, entries]): [vscode.Uri, vscode.Diagnostic[]] =>
    [uri, entries.filter(d => d.source === 'ripr')]).filter(([, entries]) => entries.length > 0);
}

function assertOnlyRootDiagnostics(root: string | undefined): void {
  for (const [uri] of riprDiagnostics()) {
    assert.ok(root && uri.toString().startsWith(vscode.Uri.file(root).toString() + '/'),
      `diagnostics leaked outside the active root: ${uri}`);
  }
}

async function assertNoRootActions(root: RootExpectation): Promise<void> {
  const uri = vscode.Uri.file(path.join(root.root, root.source));
  // The inactive root's source may never have been opened in this journey;
  // executeCodeActionProvider requires an open text model for the URI and
  // throws a bare illegal argument otherwise, so open the persisted document
  // before asserting the inactive root offers no ripr actions.
  await vscode.workspace.openTextDocument(uri);
  const actions = await vscode.commands.executeCommand<Array<vscode.CodeAction | vscode.Command>>(
    'vscode.executeCodeActionProvider', uri, new vscode.Range(0, 0, 6, 0));
  assert.strictEqual((actions ?? []).filter(action => {
    const command = typeof action.command === 'string' ? action.command : action.command?.command;
    return command?.startsWith('ripr.');
  }).length, 0, `inactive root must not offer ripr actions: ${uri}`);
}

async function openAndSave(root: RootExpectation): Promise<void> {
  const uri = vscode.Uri.file(path.join(root.root, root.source));
  const document = await vscode.workspace.openTextDocument(uri);
  assert.strictEqual(document.languageId, 'rust');
  assert.strictEqual(document.getText().replace(/\r\n/g, '\n'), root.sourceContents,
    'source contents must match an independent literal fixture pin');
  const editor = await vscode.window.showTextDocument(document);
  assert.strictEqual(await editor.edit(builder => builder.insert(document.positionAt(document.getText().length), '\n')), true);
  assert.strictEqual(document.isDirty, true);
  assert.strictEqual(await document.save(), true, 'must drive a real saved-workspace event');
}

async function assertReaped(observed: ProcessObservation): Promise<void> {
  await waitFor(() => observed.exits === 1 && observed.closes === 1, `exit and close for owned PID ${observed.pid}`, 15_000);
  assert.strictEqual(observed.child.exitCode, 0, 'server must terminate cleanly');
  assert.strictEqual(observed.child.signalCode, null);
  assert.ok(observed.pid, 'spawn must produce a native PID');
  assert.throws(() => process.kill(observed.pid!, 0), (error: NodeJS.ErrnoException) => error.code === 'ESRCH',
    `owned PID ${observed.pid} must be absent from the OS`);
}

async function cleanupOwnedProcess(observed: ProcessObservation): Promise<void> {
  const { child } = observed;
  if (child.exitCode === null && child.signalCode === null) {
    child.kill('SIGTERM');
    try {
      await waitFor(() => child.exitCode !== null || child.signalCode !== null, 'owned server graceful cleanup', 5_000);
    } catch {
      // Escalate only this recorded, still-live ChildProcess handle.
      child.kill('SIGKILL');
    }
  }
  await waitFor(() => observed.exits === 1 && observed.closes === 1,
    `owned server cleanup exit/close for ${observed.pid}`, 5_000);
  if (observed.pid) {
    assert.throws(() => process.kill(observed.pid!, 0), (error: NodeJS.ErrnoException) => error.code === 'ESRCH',
      `owned PID ${observed.pid} remained after emergency cleanup`);
  }
}

async function waitFor(predicate: () => boolean | Promise<boolean>, label: string, timeout = 60_000,
  context?: () => Promise<string> | string): Promise<void> {
  const deadline = Date.now() + timeout;
  while (!(await predicate())) {
    if (Date.now() >= deadline) {
      let suffix = '';
      try {
        suffix = context ? ` context=${await context()}` : '';
      } catch {
        suffix = ' context=<unavailable>';
      }
      throw new Error(`timed out: ${label}${suffix}`);
    }
    await new Promise(resolve => setTimeout(resolve, 50));
  }
}

async function bounded<T>(promise: PromiseLike<T>, label: string): Promise<T> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    return await Promise.race([Promise.resolve(promise), new Promise<never>((_, reject) => {
      timer = setTimeout(() => reject(new Error(`timed out: ${label}`)), 30_000);
    })]);
  } finally { if (timer) { clearTimeout(timer); } }
}
