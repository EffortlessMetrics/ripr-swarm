import * as assert from 'assert';
import * as fs from 'fs';
import * as path from 'path';
import * as vscode from 'vscode';
import { LanguageClient, LanguageClientOptions, ServerOptions } from 'vscode-languageclient/node';
import { RiprClientController, RiprClientRuntime } from '../../src/client';
import { currentWorkspaceRootState } from '../../src/workspaceHelpers';
import { compatibleLspEvidence } from './testCompatibility';

suite('Windows glob controller probe', () => {
  test('controller selects own file through the workspace URI', async () => {
    const checkpoint = (name: string) => { if (process.env.RIPR_GLOB_PROBE_RESULT) fs.writeFileSync(process.env.RIPR_GLOB_PROBE_RESULT + '.' + name, 'reached'); };
    checkpoint('entered');
    const folder = vscode.workspace.workspaceFolders?.[0];
    assert.ok(folder);
    assert.match(folder.uri.fsPath, /proj\[1\]\{a,b\}$/);
    const file = path.join(folder.uri.fsPath, 'lib.rs');
    const sibling = path.join(path.dirname(folder.uri.fsPath), 'sibling', 'lib.rs');
    fs.mkdirSync(path.dirname(sibling), { recursive: true });
    fs.writeFileSync(file, 'pub fn f() {}\n'); fs.writeFileSync(sibling, 'pub fn g() {}\n');
    const ownDoc = await vscode.workspace.openTextDocument(vscode.Uri.file(file));
    const siblingDoc = await vscode.workspace.openTextDocument(vscode.Uri.file(sibling));
    const receipt = path.join(path.dirname(folder.uri.fsPath), 'did-open.jsonl');
    const script = path.join(path.dirname(folder.uri.fsPath), 'server.cjs');
    const rpc = require.resolve('vscode-jsonrpc/node');
    fs.writeFileSync(script, `const fs=require('fs'); const rpc=require(${JSON.stringify(rpc)});
fs.writeFileSync(${JSON.stringify(receipt)}+'.peer','reached');
const c=rpc.createMessageConnection(new rpc.StreamMessageReader(process.stdin),new rpc.StreamMessageWriter(process.stdout));
c.onRequest('initialize',()=>{fs.writeFileSync(${JSON.stringify(receipt)}+'.init','reached'); return {capabilities:{textDocumentSync:1}}}); c.onRequest('shutdown',()=>null);
c.onNotification('textDocument/didOpen',p=>fs.appendFileSync(${JSON.stringify(receipt)},JSON.stringify(p.textDocument.uri)+'\\n'));
c.onNotification('exit',()=>process.exit(0)); c.listen();`);
    assert.ok(process.env.NODE_BINARY);
    checkpoint('prepared');
    const runtime = {
      getConfig: () => ({ enabled: true, serverArgs: [script], traceServer: 'off' }),
      workspaceRootState: currentWorkspaceRootState,
      workspaceFolders: () => vscode.workspace.workspaceFolders ?? [],
      isWorkspaceTrusted: () => true,
      readFile: async () => undefined,
      resolveServer: async () => ({ command: process.env.NODE_BINARY!, source: 'configured', detail: 'fake client', installationState: 'unmanaged', compatibilityResult: compatibleLspEvidence }),
      createLanguageClient: (server: ServerOptions, options: LanguageClientOptions) => {
        checkpoint('factory');
        const client = new LanguageClient('glob-probe', 'glob-probe', server, { ...options, documentSelector: [{ language: 'rust', scheme: 'file' }] });
        const start = client.start.bind(client);
        client.start = async () => { checkpoint('client-start'); await start(); checkpoint('client-started'); };
        return client;
      },
      createFileSystemWatcher: (pattern: vscode.GlobPattern) => vscode.workspace.createFileSystemWatcher(pattern)
    } as unknown as RiprClientRuntime;
    const output = vscode.window.createOutputChannel('glob probe', { log: true });
    const controller = new RiprClientController({} as vscode.ExtensionContext, output, runtime);
    try {
      checkpoint('starting');
      await controller.start();
      checkpoint('started');
      // Documents opened before client startup are synchronized on startup.
      const deadline = Date.now() + 10000;
      while (!fs.existsSync(receipt) && Date.now() < deadline) await new Promise(resolve => setTimeout(resolve, 100));
      await new Promise(resolve => setTimeout(resolve, 1000));
      const notifications = fs.existsSync(receipt) ? fs.readFileSync(receipt, 'utf8').trim().split('\n').map(line => JSON.parse(line) as string) : [];
      const own = notifications.includes(ownDoc.uri.toString());
      const other = notifications.includes(siblingDoc.uri.toString());
      if (process.env.RIPR_GLOB_PROBE_RESULT) fs.writeFileSync(process.env.RIPR_GLOB_PROBE_RESULT, JSON.stringify({ version: vscode.version, own, other }));
      assert.strictEqual(own, true); assert.strictEqual(other, false);
    } finally { await controller.stop(); output.dispose(); }
  });
});
