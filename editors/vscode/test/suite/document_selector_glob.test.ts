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

  test('Windows drive-letter spellings select the same workspace, but not a sibling', async function () {
    if (process.platform !== 'win32') {
      this.skip();
    }
    const parent = fs.mkdtempSync(path.join(os.tmpdir(), 'ripr-glob-drive-'));
    const root = path.join(parent, 'proj[1]{a,b}');
    const sibling = path.join(parent, 'proj[1]{a,b}-sibling');
    fs.mkdirSync(root, { recursive: true });
    fs.mkdirSync(sibling, { recursive: true });
    const file = path.join(root, 'lib.rs');
    const other = path.join(sibling, 'lib.rs');
    fs.writeFileSync(file, 'pub fn f() {}\n');
    fs.writeFileSync(other, 'pub fn g() {}\n');
    try {
      assert.match(root, /^[a-z]:[\\/]/i, 'test requires a drive-letter temporary directory');
      const doc = await vscode.workspace.openTextDocument(vscode.Uri.file(file));
      const siblingDoc = await vscode.workspace.openTextDocument(vscode.Uri.file(other));
      const scores = ['upper', 'lower'].map((spelling) => {
        const drive = spelling === 'upper' ? root[0].toUpperCase() : root[0].toLowerCase();
        const selector = riprDocumentSelectorsForWorkspace(drive + root.slice(1))
          .find((candidate) => candidate.language === 'rust');
        assert.ok(selector, 'rust selector present');
        return { spelling, own: vscode.languages.match(selector, doc) > 0,
          sibling: vscode.languages.match(selector, siblingDoc) > 0 };
      });
      const expected = [
        { spelling: 'upper', own: true, sibling: false },
        { spelling: 'lower', own: true, sibling: false }
      ];
      const selected = process.env.RIPR_GLOB_PROBE_ROW;
      if (selected) {
        if (process.env.RIPR_GLOB_PROBE_RESULT) {
          fs.writeFileSync(process.env.RIPR_GLOB_PROBE_RESULT, JSON.stringify({ version: vscode.version, scores }));
        }
        const [spelling, field] = selected.split(':');
        assert.ok((spelling === 'upper' || spelling === 'lower') && (field === 'own' || field === 'sibling'));
        const index = spelling === 'upper' ? 0 : 1;
        assert.strictEqual(scores[index][field as 'own' | 'sibling'], expected[index][field as 'own' | 'sibling']);
      } else {
        assert.deepStrictEqual(scores, expected);
      }
    } finally {
      fs.rmSync(parent, { recursive: true, force: true });
    }
  });
});
