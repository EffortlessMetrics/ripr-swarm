import * as assert from 'assert';
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import * as vscode from 'vscode';
import { escapeGlobLiteral, riprDocumentSelectorsForWorkspace } from '../../src/workspaceHelpers';

suite('Document selector workspace-root glob escaping', () => {
  test('glob metacharacters in the root are wrapped in bracket classes', () => {
    assert.strictEqual(escapeGlobLiteral('/w/a[1]{b,c}*?'), '/w/a[[]1[]][{]b,c[}][*][?]');
    assert.strictEqual(escapeGlobLiteral('/plain/root'), '/plain/root');
  });

  test('a root containing glob metacharacters still selects its own files', async () => {
    const parent = fs.mkdtempSync(path.join(os.tmpdir(), 'ripr-glob-'));
    const root = path.join(parent, 'proj[1]{a,b}');
    fs.mkdirSync(path.join(root, 'src'), { recursive: true });
    const file = path.join(root, 'src', 'lib.rs');
    fs.writeFileSync(file, 'pub fn f() {}\n');
    try {
      const doc = await vscode.workspace.openTextDocument(vscode.Uri.file(file));
      const selectors = riprDocumentSelectorsForWorkspace(root);
      const rust = selectors.find((selector) => selector.language === 'rust');
      assert.ok(rust, 'rust selector present');
      assert.ok(vscode.languages.match(rust, doc) > 0, 'escaped pattern selects the file');
      const unescaped = { ...rust, pattern: `${root.replace(/\\/g, '/')}/**/*` };
      assert.strictEqual(vscode.languages.match(unescaped, doc), 0, 'unescaped root does not match (the defect)');
    } finally {
      fs.rmSync(parent, { recursive: true, force: true });
    }
  });
});
