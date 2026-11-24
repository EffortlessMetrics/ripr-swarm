import * as assert from 'assert';
import { promises as fs } from 'fs';
import * as path from 'path';
import { setTimeout as delay } from 'timers/promises';
import * as vscode from 'vscode';
import type { ResolvedServer } from '../../src/serverResolver';
import { assertResolvedRepairStartServer, inspectRepairStartCard, REPAIR_START_ADMISSION, REPAIR_START_EVIDENCE,
  REPAIR_START_GAP, REPAIR_START_TARGET, type RepairStartRun } from '../repairStartObservation';
const START = 'ripr.copyAgentRepairCommand';

function enabledCommand(action: vscode.CodeAction | vscode.Command): vscode.Command | undefined {
  if (typeof action.command === 'string') {
    return action as vscode.Command;
  }
  return (action as vscode.CodeAction).disabled ? undefined : action.command;
}

suite('Packaged Repair Start', () => {
  test('distinguishes inspection from an enabled Start for the current gap', async function () {
    const kind = process.env.RIPR_TEST_REPAIR_START_CASE;
    if (kind === undefined) {
      this.skip();
      return;
    }
    this.timeout(120_000);
    const rawRun = process.env.RIPR_TEST_REPAIR_START_RUN;
    assert.ok(rawRun, 'repair Start suite requires the launcher run context');
    const run: RepairStartRun = JSON.parse(rawRun);
    assert.ok(run.workspace && run.extension && run.output, 'launcher must provide the workspace, extension and output paths');
    assert.strictEqual(run.case, kind);
    assert.ok(vscode.workspace.isTrusted);
    const workspace = vscode.workspace.workspaceFolders?.[0];
    assert.ok(workspace);
    assert.strictEqual(await fs.realpath(workspace.uri.fsPath), run.workspace);
    const extension = vscode.extensions.getExtension('EffortlessMetrics.ripr');
    assert.ok(extension);
    assert.strictEqual(await fs.realpath(extension.extensionPath), run.extension);

    const uri = vscode.Uri.joinPath(workspace.uri, 'src/lib.rs');
    const config = vscode.workspace.getConfiguration('ripr', uri);
    const server = config.get<string>('server.path');
    assert.ok(server && path.isAbsolute(server));
    assert.strictEqual(server, run.server);
    assert.strictEqual(config.get('server.autoDownload'), false);
    assert.strictEqual(config.get('baseRef'), 'main');
    assert.strictEqual(config.get('check.mode'), 'deep');
    assert.strictEqual(config.get('diagnosticProfile'), 'full');
    const resolver = require(path.join(extension.extensionPath, 'out/src/serverResolver')) as typeof import('../../src/serverResolver');
    assert.strictEqual(resolver.expandConfiguredServerPath(server), server, 'product path normalization must preserve the pinned path');
    assert.strictEqual(resolver.relativeConfiguredPathFailure(server), undefined);

    // Observe the real controller through the pass-through seam used by the
    // existing managed-download host. Leave its runtime and result untouched.
    type Controller = { server?: ResolvedServer; client?: { isRunning(): boolean }; showStatus(): Promise<void> };
    const { RiprClientController } = require(path.join(extension.extensionPath, 'out/src/client'));
    const prototype: Controller = RiprClientController.prototype;
    const original = prototype.showStatus;
    let controller: Controller | undefined;
    prototype.showStatus = function (this: Controller) {
      controller = this;
      return original.call(this);
    };
    try {
      await extension.activate();
      await vscode.commands.executeCommand('ripr.showStatus');
    } finally {
      prototype.showStatus = original;
    }
    assert.ok(controller, 'the production controller must be observed');
    await assertResolvedRepairStartServer(run, controller.server, controller.client?.isRunning() === true);
    await vscode.window.showTextDocument(await vscode.workspace.openTextDocument(uri));
    await vscode.commands.executeCommand('ripr.refreshDiagnostics');
    const expectedLine = kind === 'uncalled' ? 5 : 9;
    const deadline = Date.now() + 60_000;
    while (Date.now() < deadline) {
      const diagnostic = vscode.languages.getDiagnostics(uri).find((entry) =>
        entry.source === 'ripr' && entry.code === 'ripr-seam-ungripped'
        && entry.range.start.line === expectedLine);
      if (diagnostic) {
        const actions = await vscode.commands.executeCommand<Array<vscode.CodeAction | vscode.Command>>(
          'vscode.executeCodeActionProvider', uri, diagnostic.range);
        const commands = (actions ?? []).map(enabledCommand)
          .filter((command): command is vscode.Command => command !== undefined);
        const inspection = commands.find((command) => command.command === 'ripr.copyContext'
          && command.arguments?.[0]?.label === 'repair_card');
        if (inspection) {
          // A freshly copied current card establishes that the same provider
          // answered for this spot before we test the absence of enabled Start.
          const { seam, targetKind } = await inspectRepairStartCard(run, async () => {
            await vscode.env.clipboard.writeText('ripr-repair-card-sentinel');
            await vscode.commands.executeCommand(inspection.command, ...(inspection.arguments ?? []));
            const copied = await vscode.env.clipboard.readText();
            assert.notStrictEqual(copied, 'ripr-repair-card-sentinel', 'Inspect must copy a card');
            return copied;
          });

          const starts = commands.filter((command) => command.command === START);
          assert.strictEqual(starts.length, kind === 'uncalled' ? 1 : 0,
            `unexpected enabled Start actions for ${kind} ${seam}`);
          const start = starts[0];
          let copiedStart: string | null = null;
          if (start) {
            await vscode.env.clipboard.writeText('ripr-repair-start-sentinel');
            await vscode.commands.executeCommand(start.command, ...(start.arguments ?? []));
            copiedStart = await vscode.env.clipboard.readText();
            assert.ok(copiedStart.startsWith('ripr agent repair --root '), copiedStart);
            assert.ok(copiedStart.endsWith(` --seam-id ${seam} --phase before`), copiedStart);
          }

          // The caller joins this observation with artifact custody and
          // fixture revisions before producing the existing parity receipt.
          await assertResolvedRepairStartServer(run, controller.server, controller.client?.isRunning() === true);
          const observation = { evidence: REPAIR_START_EVIDENCE, package_admission: REPAIR_START_ADMISSION,
            nonce: run.nonce, case: kind, gap: REPAIR_START_GAP, seam, target: REPAIR_START_TARGET,
            target_kind: targetKind, fixture_root: run.workspace, repository_head: run.head, repository_base: run.base,
            currentness: 'current', enabled_start: starts.length, copied_start: copiedStart,
            server: run.server, server_sha256: run.serverSha256, server_version: controller.server?.binaryVersion,
            extension_path: run.extension,
            extension_version: extension.packageJSON.version, vscode_version: vscode.version,
            node_version: process.versions.node, platform: process.platform, arch: process.arch };
          await fs.writeFile(run.output,
            `${JSON.stringify(observation, null, 2)}\n`, { flag: 'wx' });
          return;
        }
      }
      await delay(250);
    }
    assert.fail(`missing current loyalty_price inspection for ${kind}`);
  });
});
