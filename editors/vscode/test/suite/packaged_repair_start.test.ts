import * as assert from 'assert';
import { createHash } from 'crypto';
import { promises as fs } from 'fs';
import * as path from 'path';
import { setTimeout as delay } from 'timers/promises';
import * as vscode from 'vscode';

const GAP = 'gap:rust:src/lib.rs:loyalty_price:predicate_boundary:predicate:member_years==5';
const TARGET = 'tests/loyalty_price.rs';
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
    assert.ok(kind === 'called' || kind === 'uncalled');
    assert.ok(vscode.workspace.isTrusted);
    const workspace = vscode.workspace.workspaceFolders?.[0];
    assert.ok(workspace);
    assert.strictEqual(await fs.realpath(workspace.uri.fsPath),
      await fs.realpath(process.env.RIPR_TEST_REPAIR_START_WORKSPACE!));
    const extension = vscode.extensions.getExtension('EffortlessMetrics.ripr');
    assert.ok(extension);
    assert.strictEqual(await fs.realpath(extension.extensionPath),
      await fs.realpath(process.env.RIPR_TEST_EXTENSION_PATH!));

    const uri = vscode.Uri.joinPath(workspace.uri, 'src/lib.rs');
    const config = vscode.workspace.getConfiguration('ripr', uri);
    const server = config.get<string>('server.path');
    assert.ok(server && path.isAbsolute(server));
    assert.strictEqual(server, process.env.RIPR_TEST_PACKAGED_SERVER_PATH);
    assert.strictEqual(config.get('server.autoDownload'), false);
    assert.strictEqual(config.get('baseRef'), 'main');
    assert.strictEqual(config.get('check.mode'), 'deep');
    assert.strictEqual(config.get('diagnosticProfile'), 'full');
    const serverDigest = createHash('sha256').update(await fs.readFile(server)).digest('hex');
    assert.strictEqual(serverDigest, process.env.RIPR_TEST_PACKAGED_SERVER_SHA256);

    await extension.activate();
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
        const commands = (actions ?? []).map(enabledCommand).filter((command) => command !== undefined);
        const inspection = commands.find((command) => command.command === 'ripr.copyContext'
          && command.arguments?.[0]?.label === 'repair_card');
        if (inspection) {
          // A freshly copied current card establishes that the same provider
          // answered for this spot before we test the absence of enabled Start.
          await vscode.env.clipboard.writeText('ripr-repair-card-sentinel');
          await vscode.commands.executeCommand(inspection.command, ...(inspection.arguments ?? []));
          const copied = await vscode.env.clipboard.readText();
          assert.notStrictEqual(copied, 'ripr-repair-card-sentinel', 'Inspect must copy a card');
          const card = JSON.parse(copied);
          assert.strictEqual(card.subject.canonical_gap_id, GAP);
          assert.strictEqual(card.snapshot.currentness, 'current');
          assert.strictEqual(card.selected_target.file, TARGET);
          const seam: unknown = card.subject.seam_id;
          assert.ok(typeof seam === 'string' && seam.length > 0);

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
          } else {
            assert.strictEqual(card.instruction.state, 'static_limitation');
            assert.strictEqual(card.next_action, null);
          }

          // The caller joins this observation with artifact custody and
          // fixture revisions before producing the existing parity receipt.
          const observation = { case: kind, gap: GAP, seam, target: TARGET,
            currentness: card.snapshot.currentness, enabled_start: starts.length, copied_start: copiedStart,
            server, server_sha256: serverDigest, extension_path: extension.extensionPath,
            extension_version: extension.packageJSON.version, vscode_version: vscode.version,
            node_version: process.versions.node, platform: process.platform, arch: process.arch };
          await fs.writeFile(path.join(process.env.RIPR_TEST_ARTIFACT_ROOT!, 'repair-start-observation.json'),
            `${JSON.stringify(observation, null, 2)}\n`, { flag: 'wx' });
          return;
        }
      }
      await delay(250);
    }
    assert.fail(`missing current loyalty_price inspection for ${kind}`);
  });
});
