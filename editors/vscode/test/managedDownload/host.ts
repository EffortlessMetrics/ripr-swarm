import * as assert from 'assert';
import * as fs from 'fs';
import * as path from 'path';
import * as vscode from 'vscode';
import { digest, TARGET } from './contract';

// Dedicated host entry: ordinary suites must not configure a server path,
// populate the cache, patch download operations, or restart this controller.
export async function run(): Promise<void> {
  const input = JSON.parse(fs.readFileSync(process.env.RIPR_MANAGED_DOWNLOAD_INPUT!, 'utf8'));
  const result: Record<string, unknown> = { case: input.kind, selected: 1, executed: 1, outcome: 'failure', stage: 'setup' };
  let controller: any;
  const extension = vscode.extensions.getExtension('EffortlessMetrics.ripr');
  try {
    assert.ok(vscode.workspace.isTrusted);
    assert.strictEqual(process.env.RIPR_TEST_SERVER_PATH, undefined);
    const config = vscode.workspace.getConfiguration('ripr');
    assert.strictEqual(config.get('server.path'), '');
    assert.strictEqual(config.get('server.autoDownload'), true);
    assert.strictEqual(config.get('server.version'), input.version);
    assert.strictEqual(config.get('server.downloadBaseUrl'), input.mirror);
    assert.ok(extension);
    assert.strictEqual(path.resolve(extension.extensionPath), path.resolve(input.installedExtension));
    // Observe the activated production controller through the same pass-through
    // seam as extension.test.ts. No runtime or successful server is supplied.
    const clientModule = require(path.join(extension.extensionPath, 'out/src/client'));
    const prototype = clientModule.RiprClientController.prototype;
    const original = prototype.showStatus;
    prototype.showStatus = function () {
      controller = this;
      return Reflect.apply(original, this, []);
    };
    try {
      result.stage = 'activation';
      await extension.activate();
      await vscode.commands.executeCommand('ripr.showStatus');
    } finally {
      prototype.showStatus = original;
    }
    assert.ok(controller, 'production controller was not observed');
    result.stage = 'assertions';
    result.status = controller.status;
    if (input.kind === 'valid') {
      assert.strictEqual(controller.server?.source, 'managed_download');
      assert.strictEqual(controller.server.installationState, 'complete');
      assert.strictEqual(controller.server.assetDigest, input.archiveSha256);
      assert.ok(controller.client?.isRunning(), 'downloaded server must finish LSP initialize');
      assert.strictEqual(digest(fs.readFileSync(controller.server.command)), input.executableSha256);
      const install = path.dirname(controller.server.command);
      const receipt = JSON.parse(fs.readFileSync(path.join(install, 'install-receipt.json'), 'utf8'));
      assert.strictEqual(receipt.platformTarget, TARGET);
      assert.strictEqual(receipt.manifestSha256, input.manifestSha256);
      assert.strictEqual(receipt.archiveSha256, input.archiveSha256);
      // A real round trip after initialize, using the client's standard LSP
      // connection. An installed file or version string alone cannot pass.
      const hover = await controller.client.sendRequest('textDocument/hover', {
        textDocument: { uri: vscode.Uri.file(path.join(vscode.workspace.workspaceFolders![0].uri.fsPath, 'src/lib.rs')).toString() },
        position: { line: 0, character: 1 }
      });
      assert.ok(hover === null || typeof hover === 'object');
      result.server = controller.server;
      result.installReceipt = receipt;
      result.lspRoundTrip = 'textDocument/hover';
    } else {
      assert.strictEqual(controller.server, undefined, 'PATH/bundled/cache fallback must not satisfy a negative');
      assert.ok(!controller.client?.isRunning());
      const detail = JSON.stringify(controller.status);
      assert.match(detail, input.kind === 'manifest_tampered' ? /admitted manifest SHA-256/ : /checksum mismatch/i);
      const storage = path.join(input.userData, 'User', 'globalStorage', 'effortlessmetrics.ripr', 'servers');
      const receipts = fs.existsSync(storage) ? fs.readdirSync(storage, { recursive: true }) : [];
      assert.ok(!receipts.some((entry) => String(entry).endsWith('install-receipt.json')));
    }
    result.outcome = 'success';
  } catch (error) {
    result.error = String(error);
    throw error;
  } finally {
    // The production stop owns its client/process cleanup; do not remove an
    // install or profile while its server may still hold it open.
    try {
      if (extension?.isActive) {
        await require(path.join(extension.extensionPath, 'out/src/extension')).deactivate();
      }
    } catch (error) {
      result.outcome = 'failure';
      result.cleanupError = String(error);
      throw error;
    } finally {
      fs.writeFileSync(input.receiptPath, `${JSON.stringify(result, null, 2)}\n`);
    }
  }
}
