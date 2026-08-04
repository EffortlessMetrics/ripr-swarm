import * as assert from 'assert';
import * as vscode from 'vscode';

suite('Packaged Workspace Trust', () => {
  test('untrusted host keeps ripr authority disabled', async function () {
    if (process.env.RIPR_TEST_WORKSPACE_TRUST !== 'untrusted') {
      this.skip();
      return;
    }

    assert.strictEqual(vscode.workspace.isTrusted, false);
    const extension = vscode.extensions.getExtension('EffortlessMetrics.ripr');
    assert.ok(extension, 'ripr extension should be present');
    await extension.activate();

    const before = await vscode.env.clipboard.readText();
    await vscode.commands.executeCommand('ripr.copyCurrentRepairPacket');
    const after = await vscode.env.clipboard.readText();
    assert.strictEqual(after, before, 'untrusted workspace must not copy repair authority');
  });
});
