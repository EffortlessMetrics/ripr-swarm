import * as assert from 'assert';
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import * as vscode from 'vscode';
import { LanguageClient } from 'vscode-languageclient/node';
import { escapeGlobLiteral, riprDocumentSelectorsForWorkspace } from '../../src/workspaceHelpers';

suite('Document selector workspace-root glob escaping', () => {
  test('glob metacharacters in the root are wrapped in bracket classes', () => {
    assert.strictEqual(escapeGlobLiteral('/w/a[1]{b,c}*?'), '/w/a[[]1[]][{]b,c[}][*][?]');
    assert.strictEqual(escapeGlobLiteral('/plain/root'), '/plain/root');
  });

  test('the installed language client matches the escaped workspace selector', () => {
    // Use the client’s exported test surface to exercise the matcher used for
    // diagnostic document filters. VS Code's own languages.match uses a different engine.
    const { matchGlobPattern } = require('vscode-languageclient/$test/common/utils/globPattern') as {
      matchGlobPattern(pattern: string, uri: vscode.Uri): boolean;
    };
    const root = vscode.Uri.file(path.join(os.tmpdir(), 'project[1]'));
    const rust = vscode.Uri.joinPath(root, 'src', 'lib.rs');
    const selector = riprDocumentSelectorsForWorkspace(root.fsPath)[0];

    assert.strictEqual(matchGlobPattern('**/*.{rs,ts}', rust), true);
    assert.strictEqual(matchGlobPattern('**/*.json', rust), false);
    assert.strictEqual(matchGlobPattern(selector.pattern, rust), true);
    const sibling = vscode.Uri.file(path.join(os.tmpdir(), 'project1', 'src', 'lib.rs'));
    assert.strictEqual(matchGlobPattern(selector.pattern, sibling), false);
  });

  test('file-operation registration dispatches only matching file URIs', async () => {
    const sent: vscode.Uri[] = [];
    const client = new LanguageClient('glob-probe', 'glob probe',
      async () => { throw new Error('registration must run without a server'); }, {
        middleware: { workspace: { didCreateFiles: async (event) => { sent.push(...event.files); } } }
      });
    const feature = client.getFeature('workspace/didCreateFiles');
    try {
      feature.register({
        id: 'glob-probe',
        registerOptions: { filters: [{ scheme: 'file', pattern: { glob: '**/*.{rs,ts}' } }] }
      });
      const root = vscode.Uri.file(os.tmpdir());
      const rust = vscode.Uri.joinPath(root, 'example.rs');
      const ts = vscode.Uri.joinPath(root, 'example.ts');
      await feature.send({ files: [rust, ts, vscode.Uri.joinPath(root, 'other.js'), rust.with({ scheme: 'other' })] });
      assert.deepStrictEqual(sent, [rust, ts]);
    } finally {
      feature.unregister('glob-probe');
      await client.dispose();
    }
  });

  test('a root containing glob metacharacters still selects its own files', async () => {
    const parent = fs.mkdtempSync(path.join(os.tmpdir(), 'ripr-glob-'));
    const root = path.join(parent, 'proj[1]{a,b}');
    fs.mkdirSync(path.join(root, 'src'), { recursive: true });
    const file = path.join(root, 'src', 'lib.rs');
    fs.writeFileSync(file, 'pub fn f() {}\n');
    try {
      const doc = await vscode.workspace.openTextDocument(vscode.Uri.file(file));
      // The controller receives workspace folder URI fsPaths, not the raw
      // spelling returned by the OS temporary-directory API. On Windows VS
      // Code normalizes the drive letter in that URI before glob matching.
      const workspaceRoot = vscode.Uri.file(root).fsPath;
      const selectors = riprDocumentSelectorsForWorkspace(workspaceRoot);
      const rust = selectors.find((selector) => selector.language === 'rust');
      assert.ok(rust, 'rust selector present');
      assert.ok(vscode.languages.match(rust, doc) > 0, 'escaped pattern selects the file');
      const unescaped = { ...rust, pattern: `${workspaceRoot.replace(/\\/g, '/')}/**/*` };
      assert.strictEqual(vscode.languages.match(unescaped, doc), 0, 'unescaped root does not match (the defect)');
    } finally {
      fs.rmSync(parent, { recursive: true, force: true });
    }
  });
});
